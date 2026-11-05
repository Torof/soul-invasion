//! WebSocket listener and per-connection handling (D25, D26).
//!
//! The network side only translates: it decodes client messages, answers
//! the handshake and liveness checks, and forwards what the simulation
//! thread publishes. It never touches the world itself (D29).

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use protocol::{ClientMessage, PROTOCOL_VERSION, ServerMessage, decode, encode};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, watch};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

/// How long a client has to send its `hello` after connecting.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// How many published messages a slow client may fall behind before it
/// starts missing some.
const BROADCAST_CAPACITY: usize = 256;

type BoxError = Box<dyn Error + Send + Sync>;

/// State shared between the simulation thread and all connections.
#[derive(Debug)]
pub struct Shared {
    /// The world's current tick, updated by the simulation thread.
    pub tick: AtomicU64,
    /// Messages published by the simulation thread to every client.
    pub publish: broadcast::Sender<ServerMessage>,
}

impl Shared {
    /// Creates the shared state at tick zero with no subscribers.
    pub fn new() -> Self {
        let (publish, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            tick: AtomicU64::new(0),
            publish,
        }
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

/// Accepts connections until `shutdown` becomes true.
pub async fn serve(
    listener: TcpListener,
    shared: Arc<Shared>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, peer)) => {
                    let shared = Arc::clone(&shared);
                    let shutdown = shutdown.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, peer, shared, shutdown).await {
                            eprintln!("connection {peer}: {e}");
                        }
                        println!("connection {peer}: closed");
                    });
                }
                Err(e) => eprintln!("accept failed: {e}"),
            },
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return;
                }
            }
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    shared: Arc<Shared>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), BoxError> {
    let mut ws = tokio_tungstenite::accept_async(stream).await?;
    println!("connection {peer}: opened");

    // Subscribe before answering hello so no day boundary is missed between
    // the welcome tick and the first published message.
    let mut published = shared.publish.subscribe();

    if !handshake(&mut ws, &shared).await? {
        ws.close(None).await?;
        return Ok(());
    }

    loop {
        tokio::select! {
            incoming = ws.next() => match incoming {
                None | Some(Ok(Message::Close(_))) => return Ok(()),
                Some(Err(e)) => return Err(e.into()),
                Some(Ok(Message::Text(text))) => match decode::<ClientMessage>(text.as_str()) {
                    Ok(ClientMessage::Ping { nonce }) => {
                        send(&mut ws, &ServerMessage::Pong { nonce }).await?;
                    }
                    Ok(ClientMessage::Hello { .. }) => {
                        // A second hello is a client bug; drop the connection.
                        ws.close(None).await?;
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("connection {peer}: undecodable message ignored: {e}");
                    }
                },
                // Binary frames are not part of the protocol; ping/pong frames
                // are handled by the WebSocket layer itself.
                Some(Ok(_)) => {}
            },
            message = published.recv() => match message {
                Ok(message) => send(&mut ws, &message).await?,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    eprintln!("connection {peer}: too slow, {n} message(s) skipped");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    ws.close(None).await?;
                    return Ok(());
                }
            },
            _ = shutdown.changed() => {
                ws.close(None).await?;
                return Ok(());
            }
        }
    }
}

/// Runs the hello/welcome exchange. Returns `Ok(false)` when the client
/// must be disconnected (wrong version, wrong first message, or silence).
async fn handshake(ws: &mut WebSocketStream<TcpStream>, shared: &Shared) -> Result<bool, BoxError> {
    let first = tokio::time::timeout(HANDSHAKE_TIMEOUT, ws.next()).await;
    let Ok(Some(Ok(Message::Text(text)))) = first else {
        return Ok(false);
    };
    match decode::<ClientMessage>(text.as_str()) {
        Ok(ClientMessage::Hello { protocol_version }) if protocol_version == PROTOCOL_VERSION => {
            let welcome = ServerMessage::Welcome {
                protocol_version: PROTOCOL_VERSION,
                tick: shared.tick.load(Ordering::Relaxed),
            };
            send(ws, &welcome).await?;
            Ok(true)
        }
        Ok(ClientMessage::Hello { .. }) => {
            let mismatch = ServerMessage::ProtocolMismatch {
                expected: PROTOCOL_VERSION,
            };
            send(ws, &mismatch).await?;
            Ok(false)
        }
        _ => Ok(false),
    }
}

async fn send(
    ws: &mut WebSocketStream<TcpStream>,
    message: &ServerMessage,
) -> Result<(), BoxError> {
    ws.send(Message::text(encode(message)?)).await?;
    Ok(())
}
