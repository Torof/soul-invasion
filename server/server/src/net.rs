//! WebSocket listener and per-connection handling (D25, D26).
//!
//! The network side only translates: it decodes client messages, answers
//! the handshake and liveness checks, hands intentions to the simulation
//! thread, and forwards what that thread publishes. It never touches the
//! world itself (D29).
//!
//! Limits, from the milestone 1 audit: a client has [`HANDSHAKE_TIMEOUT`]
//! to upgrade to WebSocket and then as long again to say hello; messages
//! larger than [`MAX_MESSAGE_BYTES`] end the connection; at most
//! `max_connections` clients are served at once; and a body keeps only the
//! latest intention per tick, so a flooding client costs one path search
//! per tick and bothers nobody else.

use std::collections::BTreeMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
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
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

/// How long a client has to finish the WebSocket upgrade, and then how
/// long it has to send its `hello`.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest incoming message, in bytes. Client messages are a few dozen
/// bytes; anything near this limit is not our client.
pub const MAX_MESSAGE_BYTES: usize = 4096;

/// How many published messages a slow client may fall behind before it
/// starts missing some.
const BROADCAST_CAPACITY: usize = 256;

/// How many private messages (refusals) a pilot may have waiting before
/// new ones are dropped.
const PRIVATE_CAPACITY: usize = 16;

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
    /// The ground.
    pub map: MapView,
    /// Latest known position of every body, kept by the simulation thread.
    pub bodies: watch::Sender<Vec<BodyState>>,
    /// Connections served at once, at most.
    max_connections: usize,
    /// Connections currently served.
    connections: AtomicUsize,
    /// The latest intention of each body, taken by the simulation thread
    /// before each tick.
    intentions: Mutex<BTreeMap<u32, Pos>>,
    /// Bodies currently piloted, with the channel for messages meant for
    /// that pilot alone.
    pilots: Mutex<BTreeMap<u32, mpsc::Sender<ServerMessage>>>,
}

/// A served connection's place among the `max_connections`, given back
/// when dropped.
#[derive(Debug)]
pub struct Admission<'a> {
    shared: &'a Shared,
}

impl Drop for Admission<'_> {
    fn drop(&mut self) {
        self.shared.connections.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Shared {
    /// Creates the shared state from the world as it is at startup.
    pub fn new(world: &World, max_connections: usize) -> Self {
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
            map,
            bodies: watch::Sender::new(bodies),
            max_connections,
            connections: AtomicUsize::new(0),
            intentions: Mutex::new(BTreeMap::new()),
            pilots: Mutex::new(BTreeMap::new()),
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

    /// Takes a place for a new connection, or `None` when all are taken.
    pub fn admit(&self) -> Option<Admission<'_>> {
        let mut current = self.connections.load(Ordering::Relaxed);
        loop {
            if current >= self.max_connections {
                return None;
            }
            match self.connections.compare_exchange_weak(
                current,
                current + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Some(Admission { shared: self }),
                Err(seen) => current = seen,
            }
        }
    }

    /// Connections currently served.
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::Relaxed)
    }

    /// Sets a body's intention for the coming tick, replacing any earlier
    /// one: the latest wins.
    pub fn intend(&self, body: BodyId, target: Pos) {
        self.intentions
            .lock()
            .expect("intentions not poisoned")
            .insert(body.0, target);
    }

    /// Takes every pending intention, in body order, leaving none.
    pub fn take_intentions(&self) -> Vec<Intention> {
        let taken = std::mem::take(&mut *self.intentions.lock().expect("intentions not poisoned"));
        taken
            .into_iter()
            .map(|(body, target)| Intention::MoveTo {
                body: BodyId(body),
                target,
            })
            .collect()
    }

    /// Sends a message to the connection piloting `body`, if any. Dropped
    /// when that pilot has too many waiting already.
    pub fn tell_pilot(&self, body: BodyId, message: ServerMessage) {
        if let Some(pilot) = self
            .pilots
            .lock()
            .expect("pilot set not poisoned")
            .get(&body.0)
        {
            let _ = pilot.try_send(message);
        }
    }

    /// Gives the connection the free body with the lowest id, if any.
    fn take_body(&self, private: mpsc::Sender<ServerMessage>) -> Option<u32> {
        let mut pilots = self.pilots.lock().expect("pilot set not poisoned");
        let free = self
            .bodies
            .borrow()
            .iter()
            .map(|b| b.id)
            .find(|id| !pilots.contains_key(id))?;
        pilots.insert(free, private);
        Some(free)
    }

    fn release_body(&self, body: u32) {
        self.pilots
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
                        let Some(_admission) = shared.admit() else {
                            // Dropping the stream closes it; the client sees
                            // the connection end before any upgrade.
                            eprintln!("connection {peer}: refused, {} connections already", shared.max_connections);
                            return;
                        };
                        if let Err(e) = handle_connection(stream, peer, &shared, shutdown).await {
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
    shared: &Shared,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), BoxError> {
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES));
    let upgrade = tokio_tungstenite::accept_async_with_config(stream, Some(config));
    let Ok(upgraded) = tokio::time::timeout(HANDSHAKE_TIMEOUT, upgrade).await else {
        // Silent TCP connections are dropped without ceremony.
        return Ok(());
    };
    let mut ws = upgraded?;
    println!("connection {peer}: opened");

    // Subscribe before answering hello so nothing is missed between the
    // welcome tick and the first published message.
    let mut published = shared.publish.subscribe();
    let (private_tx, mut private) = mpsc::channel(PRIVATE_CAPACITY);

    let pilot = Pilot { shared, body: None };
    let Some(pilot) = handshake(&mut ws, pilot, private_tx).await? else {
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
                            shared.intend(BodyId(body), Pos::new(x, y));
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
            // The sender lives in the pilot set as long as this connection
            // pilots a body, so the channel never closes before the loop ends.
            Some(message) = private.recv() => send(&mut ws, &message).await?,
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
    private: mpsc::Sender<ServerMessage>,
) -> Result<Option<Pilot<'a>>, BoxError> {
    let first = tokio::time::timeout(HANDSHAKE_TIMEOUT, ws.next()).await;
    let Ok(Some(Ok(Message::Text(text)))) = first else {
        return Ok(None);
    };
    match decode::<ClientMessage>(text.as_str()) {
        Ok(ClientMessage::Hello { protocol_version }) if protocol_version == PROTOCOL_VERSION => {
            pilot.body = pilot.shared.take_body(private);
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

#[cfg(test)]
mod tests {
    use super::*;
    use sim::Calendar;

    fn shared(max_connections: usize) -> Shared {
        Shared::new(&World::new(Calendar::default()), max_connections)
    }

    #[test]
    fn admissions_are_counted_and_given_back() {
        let shared = shared(2);
        let first = shared.admit().expect("first place");
        let second = shared.admit().expect("second place");
        assert!(shared.admit().is_none(), "no third place");
        assert_eq!(shared.connections(), 2);
        drop(first);
        assert_eq!(shared.connections(), 1);
        let third = shared.admit().expect("the place is free again");
        drop((second, third));
        assert_eq!(shared.connections(), 0);
    }

    #[test]
    fn a_body_keeps_only_its_latest_intention() {
        let shared = shared(1);
        for _ in 0..1000 {
            shared.intend(BodyId(1), Pos::new(0, 0));
        }
        shared.intend(BodyId(2), Pos::new(5, 5));
        shared.intend(BodyId(1), Pos::new(3, 4));
        assert_eq!(
            shared.take_intentions(),
            vec![
                Intention::MoveTo {
                    body: BodyId(1),
                    target: Pos::new(3, 4)
                },
                Intention::MoveTo {
                    body: BodyId(2),
                    target: Pos::new(5, 5)
                },
            ]
        );
        assert_eq!(
            shared.take_intentions(),
            Vec::new(),
            "taking empties the slots"
        );
    }

    #[test]
    fn private_messages_reach_the_pilot_and_nobody_else() {
        let shared = shared(1);
        let (tx, mut rx) = mpsc::channel(PRIVATE_CAPACITY);
        assert_eq!(shared.take_body(tx), Some(1));
        let refusal = ServerMessage::MoveRefused {
            body: 1,
            x: 0,
            y: 0,
        };
        shared.tell_pilot(BodyId(1), refusal.clone());
        shared.tell_pilot(BodyId(2), refusal.clone());
        assert_eq!(rx.try_recv(), Ok(refusal));
        assert!(rx.try_recv().is_err(), "body 2 has no pilot: dropped");
        shared.release_body(1);
        shared.tell_pilot(BodyId(1), ServerMessage::Pong { nonce: 0 });
        assert!(rx.try_recv().is_err(), "released: nothing more arrives");
    }
}
