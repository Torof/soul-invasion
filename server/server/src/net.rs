//! WebSocket listener and per-connection handling (D25, D26).
//!
//! The network side only translates: it decodes client messages, answers
//! the handshake and liveness checks, forwards intentions to the
//! simulation thread, and forwards what that thread publishes. It never
//! touches the world itself (D29).

use std::collections::BTreeSet;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use protocol::{
    BodyState, ClientMessage, PROTOCOL_VERSION, ServerMessage, Terrain, decode, encode,
};
use sim::{BodyId, Intention, Pos, World};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, watch};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

/// How long a client has to send its `hello` after connecting.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// How many published messages a slow client may fall behind before it
/// starts missing some.
const BROADCAST_CAPACITY: usize = 256;

type BoxError = Box<dyn Error + Send + Sync>;

/// The map as clients receive it. Static for now: the ground does not
/// change yet.
#[derive(Debug, Clone)]
pub struct MapView {
    /// Columns.
    pub width: u32,
    /// Rows.
    pub height: u32,
    /// Tiles, row by row.
    pub tiles: Vec<Terrain>,
}

/// State shared between the simulation thread and all connections.
#[derive(Debug)]
pub struct Shared {
    /// The world's current tick, updated by the simulation thread.
    pub tick: AtomicU64,
    /// Messages published by the simulation thread to every client.
    pub publish: broadcast::Sender<ServerMessage>,
    /// Intentions from clients, drained by the simulation thread before
    /// each tick.
    pub intentions: mpsc::UnboundedSender<Intention>,
    /// The ground.
    pub map: MapView,
    /// Latest known position of every body, kept by the simulation thread.
    pub bodies: watch::Sender<Vec<BodyState>>,
    /// Bodies currently piloted by a connection.
    piloted: Mutex<BTreeSet<u32>>,
}

impl Shared {
    /// Creates the shared state from the world as it is at startup.
    pub fn new(world: &World, intentions: mpsc::UnboundedSender<Intention>) -> Self {
        let (publish, _) = broadcast::channel(BROADCAST_CAPACITY);
        let map = MapView {
            width: world.map().width(),
            height: world.map().height(),
            tiles: world
                .map()
                .tiles()
                .iter()
                .map(|t| terrain_view(*t))
                .collect(),
        };
        let bodies = world
            .bodies()
            .iter()
            .map(|b| BodyState {
                id: b.id().0,
                x: b.pos().x,
                y: b.pos().y,
            })
            .collect();
        Self {
            tick: AtomicU64::new(world.tick().0),
            publish,
            intentions,
            map,
            bodies: watch::Sender::new(bodies),
            piloted: Mutex::new(BTreeSet::new()),
        }
    }

    /// Records a body's new position.
    pub fn body_moved(&self, body: BodyId, to: Pos) {
        self.bodies.send_modify(|bodies| {
            if let Some(state) = bodies.iter_mut().find(|b| b.id == body.0) {
                state.x = to.x;
                state.y = to.y;
            }
        });
    }

    /// Gives the connection the free body with the lowest id, if any.
    fn take_body(&self) -> Option<u32> {
        let mut piloted = self.piloted.lock().expect("pilot set not poisoned");
        let free = self
            .bodies
            .borrow()
            .iter()
            .map(|b| b.id)
            .find(|id| !piloted.contains(id))?;
        piloted.insert(free);
        Some(free)
    }

    fn release_body(&self, body: u32) {
        self.piloted
            .lock()
            .expect("pilot set not poisoned")
            .remove(&body);
    }
}

fn terrain_view(terrain: sim::Terrain) -> Terrain {
    match terrain {
        sim::Terrain::Grass => Terrain::Grass,
        sim::Terrain::Forest => Terrain::Forest,
        sim::Terrain::Water => Terrain::Water,
        sim::Terrain::Ford => Terrain::Ford,
        sim::Terrain::Rock => Terrain::Rock,
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

/// The body a connection pilots, given back when the connection ends.
struct Pilot<'a> {
    shared: &'a Shared,
    body: Option<u32>,
}

impl Drop for Pilot<'_> {
    fn drop(&mut self) {
        if let Some(body) = self.body {
            self.shared.release_body(body);
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

    // Subscribe before answering hello so nothing is missed between the
    // welcome tick and the first published message.
    let mut published = shared.publish.subscribe();

    let pilot = Pilot {
        shared: &shared,
        body: None,
    };
    let Some(pilot) = handshake(&mut ws, pilot).await? else {
        ws.close(None).await?;
        return Ok(());
    };
    match pilot.body {
        Some(body) => println!("connection {peer}: pilots body {body}"),
        None => println!("connection {peer}: watching, no free body"),
    }

    let state = ServerMessage::WorldState {
        width: shared.map.width,
        height: shared.map.height,
        tiles: shared.map.tiles.clone(),
        bodies: shared.bodies.borrow().clone(),
    };
    send(&mut ws, &state).await?;

    loop {
        tokio::select! {
            incoming = ws.next() => match incoming {
                None | Some(Ok(Message::Close(_))) => return Ok(()),
                Some(Err(e)) => return Err(e.into()),
                Some(Ok(Message::Text(text))) => match decode::<ClientMessage>(text.as_str()) {
                    Ok(ClientMessage::Ping { nonce }) => {
                        send(&mut ws, &ServerMessage::Pong { nonce }).await?;
                    }
                    Ok(ClientMessage::MoveTo { x, y }) => {
                        if let Some(body) = pilot.body {
                            // The simulation thread is gone only at shutdown.
                            let _ = shared.intentions.send(Intention::MoveTo {
                                body: BodyId(body),
                                target: Pos::new(x, y),
                            });
                        }
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

/// Runs the hello/welcome exchange. Returns `Ok(None)` when the client
/// must be disconnected (wrong version, wrong first message, or silence).
async fn handshake<'a>(
    ws: &mut WebSocketStream<TcpStream>,
    mut pilot: Pilot<'a>,
) -> Result<Option<Pilot<'a>>, BoxError> {
    let first = tokio::time::timeout(HANDSHAKE_TIMEOUT, ws.next()).await;
    let Ok(Some(Ok(Message::Text(text)))) = first else {
        return Ok(None);
    };
    match decode::<ClientMessage>(text.as_str()) {
        Ok(ClientMessage::Hello { protocol_version }) if protocol_version == PROTOCOL_VERSION => {
            pilot.body = pilot.shared.take_body();
            let welcome = ServerMessage::Welcome {
                protocol_version: PROTOCOL_VERSION,
                tick: pilot.shared.tick.load(Ordering::Relaxed),
                body: pilot.body,
            };
            send(ws, &welcome).await?;
            Ok(Some(pilot))
        }
        Ok(ClientMessage::Hello { .. }) => {
            let mismatch = ServerMessage::ProtocolMismatch {
                expected: PROTOCOL_VERSION,
            };
            send(ws, &mismatch).await?;
            Ok(None)
        }
        _ => Ok(None),
    }
}

async fn send(
    ws: &mut WebSocketStream<TcpStream>,
    message: &ServerMessage,
) -> Result<(), BoxError> {
    ws.send(Message::text(encode(message)?)).await?;
    Ok(())
}
