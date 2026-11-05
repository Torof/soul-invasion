//! Drives the real server binary over WebSocket with a small fake client:
//! handshake, liveness, published events, and the ways a client gets
//! disconnected.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use protocol::{ClientMessage, PROTOCOL_VERSION, ServerMessage, decode, encode};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A running server, killed when dropped.
struct Server {
    child: Child,
    addr: String,
}

impl Server {
    /// Starts a paced server with one game day per real second.
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_server"))
            .args([
                "--ephemeral",
                "--listen",
                "127.0.0.1:0",
                "--ticks-per-second",
                "20",
                "--seconds-per-day",
                "1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("server binary starts");

        let stdout = child.stdout.take().expect("stdout is piped");
        let mut lines = BufReader::new(stdout).lines();
        let addr = loop {
            let line = lines
                .next()
                .expect("server prints its address before exiting")
                .expect("stdout is readable");
            if let Some(addr) = line.strip_prefix("listening on ") {
                break addr.to_owned();
            }
        };
        // Keep draining stdout so the server never blocks on a full pipe.
        std::thread::spawn(move || for _ in lines {});

        Self { child, addr }
    }

    async fn connect(&self) -> Socket {
        let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{}", self.addr))
            .await
            .expect("websocket connects");
        socket
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn send(socket: &mut Socket, message: &ClientMessage) {
    socket
        .send(Message::text(encode(message).unwrap()))
        .await
        .expect("message is sent");
}

/// Next server message, or `None` once the server closed the connection.
async fn receive(socket: &mut Socket) -> Option<ServerMessage> {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("server answers within 5 s");
        match frame {
            None | Some(Ok(Message::Close(_)) | Err(_)) => return None,
            Some(Ok(Message::Text(text))) => {
                return Some(decode(text.as_str()).expect("server speaks the protocol"));
            }
            Some(Ok(_)) => {}
        }
    }
}

/// Waits for the next published day boundary.
async fn next_day(socket: &mut Socket) -> u64 {
    loop {
        if let Some(ServerMessage::DayStarted { day, .. }) = receive(socket).await {
            break day;
        }
    }
}

#[tokio::test]
async fn hello_ping_and_a_published_day() {
    let server = Server::start();
    let mut socket = server.connect().await;

    send(
        &mut socket,
        &ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .await;
    let welcome = receive(&mut socket).await;
    assert!(
        matches!(
            welcome,
            Some(ServerMessage::Welcome {
                protocol_version: PROTOCOL_VERSION,
                ..
            })
        ),
        "got {welcome:?}"
    );

    send(&mut socket, &ClientMessage::Ping { nonce: 7 }).await;
    // A day boundary may slip in before the pong: skip those.
    let pong = loop {
        match receive(&mut socket).await {
            Some(ServerMessage::DayStarted { .. }) => {}
            other => break other,
        }
    };
    assert_eq!(pong, Some(ServerMessage::Pong { nonce: 7 }));

    // One game day lasts one real second: a day boundary must be published.
    let (day, tick) = match receive(&mut socket).await {
        Some(ServerMessage::DayStarted { day, tick }) => (day, tick),
        Some(other) => panic!("unexpected {other:?}"),
        None => panic!("connection closed before a day started"),
    };
    assert!(day >= 2);
    assert_eq!(tick % 20, 0, "a day starts on a day boundary");

    socket.close(None).await.expect("clean close");
}

#[tokio::test]
async fn wrong_protocol_version_is_refused() {
    let server = Server::start();
    let mut socket = server.connect().await;

    send(
        &mut socket,
        &ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION + 1,
        },
    )
    .await;
    assert_eq!(
        receive(&mut socket).await,
        Some(ServerMessage::ProtocolMismatch {
            expected: PROTOCOL_VERSION
        })
    );
    assert_eq!(receive(&mut socket).await, None, "server closes afterwards");
}

#[tokio::test]
async fn anything_but_hello_first_is_disconnected() {
    let server = Server::start();
    let mut socket = server.connect().await;

    send(&mut socket, &ClientMessage::Ping { nonce: 1 }).await;
    assert_eq!(receive(&mut socket).await, None);
}

#[tokio::test]
async fn several_clients_each_get_the_same_days() {
    let server = Server::start();
    let mut a = server.connect().await;
    let mut b = server.connect().await;
    for socket in [&mut a, &mut b] {
        send(
            socket,
            &ClientMessage::Hello {
                protocol_version: PROTOCOL_VERSION,
            },
        )
        .await;
        assert!(matches!(
            receive(socket).await,
            Some(ServerMessage::Welcome { .. })
        ));
    }

    let day_a = next_day(&mut a).await;
    let day_b = next_day(&mut b).await;
    assert_eq!(day_a, day_b);
}
