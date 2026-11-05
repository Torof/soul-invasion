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
                body: Some(1),
                ..
            })
        ),
        "got {welcome:?}"
    );

    // The whole valley follows the welcome, before anything else.
    match receive(&mut socket).await {
        Some(ServerMessage::WorldState {
            width,
            height,
            tiles,
            bodies,
        }) => {
            assert_eq!((width, height), (32, 32));
            assert_eq!(tiles.len(), 32 * 32);
            assert_eq!(bodies.len(), 3);
            assert!(bodies.iter().any(|b| b.id == 1));
        }
        other => panic!("expected world_state, got {other:?}"),
    }

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

/// Opens a piloting connection: hello, welcome, `world_state` consumed.
async fn join(server: &Server) -> (Socket, Option<u32>) {
    let mut socket = server.connect().await;
    send(
        &mut socket,
        &ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .await;
    let body = match receive(&mut socket).await {
        Some(ServerMessage::Welcome { body, .. }) => body,
        other => panic!("expected welcome, got {other:?}"),
    };
    assert!(matches!(
        receive(&mut socket).await,
        Some(ServerMessage::WorldState { .. })
    ));
    (socket, body)
}

#[tokio::test]
async fn a_move_intention_makes_the_piloted_body_walk() {
    let server = Server::start();
    let (mut pilot, body) = join(&server).await;
    let (mut watcher, other) = join(&server).await;
    let body = body.expect("first connection pilots a body");
    assert_ne!(Some(body), other, "two connections never share a body");

    // Body 1 starts at (20, 20) on grass; walk two tiles east.
    send(&mut pilot, &ClientMessage::MoveTo { x: 22, y: 20 }).await;

    let mut seen_by_pilot = Vec::new();
    let mut seen_by_watcher = Vec::new();
    for (socket, seen) in [
        (&mut pilot, &mut seen_by_pilot),
        (&mut watcher, &mut seen_by_watcher),
    ] {
        while seen.len() < 2 {
            match receive(socket).await {
                Some(ServerMessage::BodyMoved {
                    body: who, x, y, ..
                }) if who == body => seen.push((x, y)),
                Some(ServerMessage::DayStarted { .. } | ServerMessage::BodyMoved { .. }) => {}
                other => panic!("unexpected {other:?}"),
            }
        }
    }
    assert_eq!(seen_by_pilot, vec![(21, 20), (22, 20)]);
    assert_eq!(
        seen_by_watcher, seen_by_pilot,
        "everyone sees the same walk"
    );
}

#[tokio::test]
async fn walking_into_the_cliff_is_refused() {
    let server = Server::start();
    let (mut pilot, body) = join(&server).await;
    let body = body.expect("pilots a body");

    // Row 0 is the cliff: not walkable.
    send(&mut pilot, &ClientMessage::MoveTo { x: 20, y: 0 }).await;
    loop {
        match receive(&mut pilot).await {
            Some(ServerMessage::MoveRefused { body: who, x, y }) => {
                assert_eq!((who, x, y), (body, 20, 0));
                break;
            }
            Some(ServerMessage::DayStarted { .. }) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[tokio::test]
async fn a_released_body_can_be_piloted_again() {
    let server = Server::start();
    let (first, body) = join(&server).await;
    assert_eq!(body, Some(1));
    drop(first);
    // The server releases the body when the connection ends.
    for _ in 0..50 {
        let (_, body) = join(&server).await;
        if body == Some(1) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("body 1 was never released");
}
