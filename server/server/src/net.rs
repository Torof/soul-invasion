//! WebSocket listener and per-connection handling (D25, D26).
//!
//! The network side only translates: it decodes client messages, answers
//! the handshake and liveness checks, hands intentions to the simulation
//! thread, and forwards what that thread publishes. It never touches the
//! world itself (D29).
//!
//! Each connection holds a [`View`]: the chunks around its body (D08,
//! D31). It receives those chunks, then only what happens inside them;
//! as the body walks the view follows, chunks entering it are sent and
//! chunks leaving it are hidden.
//!
//! Limits, from the milestone 1 audit: a client has [`HANDSHAKE_TIMEOUT`]
//! to upgrade to WebSocket and then as long again to say hello; messages
//! larger than [`MAX_MESSAGE_BYTES`] end the connection; at most
//! `max_connections` clients are served at once; and a body keeps only the
//! latest intention per tick, so a flooding client costs one path search
//! per tick and bothers nobody else.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use protocol::{
    BodyState, ClientMessage, ItemState, MaterialView, PROTOCOL_VERSION, ServerMessage, Terrain,
    decode, encode,
};
use sim::{BodyId, CHUNK_SIZE, ChunkPos, Content, Intention, Map, Pos, World};
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

/// Chunks held around the body's own chunk, in every direction: a window
/// of `(2 × VIEW_RADIUS + 1)²` chunks, 160 m across.
pub const VIEW_RADIUS: i32 = 2;

/// How many published messages a slow client may fall behind before it
/// starts missing some.
const BROADCAST_CAPACITY: usize = 256;

/// How many private messages (refusals) a pilot may have waiting before
/// new ones are dropped.
const PRIVATE_CAPACITY: usize = 16;

type BoxError = Box<dyn Error + Send + Sync>;

/// What the simulation thread publishes to every connection; each one
/// keeps what falls inside its view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Published {
    /// A new game day began.
    DayStarted {
        /// The day.
        day: u64,
        /// The tick at which it started.
        tick: u64,
    },
    /// A body stepped from one tile to the next.
    BodyMoved {
        /// Who.
        body: u32,
        /// Where it stood.
        from: Pos,
        /// Where it stands now.
        to: Pos,
        /// When.
        tick: u64,
    },
}

/// State shared between the simulation thread and all connections.
#[derive(Debug)]
pub struct Shared {
    /// The world's current tick, updated by the simulation thread.
    pub tick: AtomicU64,
    /// What the simulation thread publishes to every connection.
    pub publish: broadcast::Sender<Published>,
    /// The ground. Static for now: nothing changes the terrain yet.
    pub map: Map,
    /// Latest known position of every body, kept by the simulation thread.
    pub bodies: watch::Sender<Vec<BodyState>>,
    /// What items are made of, as clients need it.
    pub materials: Vec<MaterialView>,
    /// Items on the ground, by chunk. Static until something picks them up.
    items: BTreeMap<ChunkPos, Vec<ItemState>>,
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
    pub fn new(world: &World, content: &Content, max_connections: usize) -> Self {
        let (publish, _) = broadcast::channel(BROADCAST_CAPACITY);
        let materials = content
            .materials
            .iter()
            .map(|m| MaterialView {
                id: m.id.clone(),
                name: m.name.clone(),
                colour: m.look.colour.clone(),
            })
            .collect();
        let mut items: BTreeMap<ChunkPos, Vec<ItemState>> = BTreeMap::new();
        for item in world.items() {
            items
                .entry(item.pos().chunk())
                .or_default()
                .push(ItemState {
                    id: item.id().0,
                    x: item.pos().x,
                    y: item.pos().y,
                    material: item.material().to_owned(),
                });
        }
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
            map: world.map().clone(),
            bodies: watch::Sender::new(bodies),
            materials,
            items,
            max_connections,
            connections: AtomicUsize::new(0),
            intentions: Mutex::new(BTreeMap::new()),
            pilots: Mutex::new(BTreeMap::new()),
        }
    }

    /// Records a body's new position and returns where it stood, or
    /// `None` for a body nobody knew about.
    pub fn body_moved(&self, body: BodyId, to: Pos) -> Option<Pos> {
        let mut from = None;
        self.bodies.send_modify(|bodies| {
            if let Some(state) = bodies.iter_mut().find(|b| b.id == body.0) {
                from = Some(Pos::new(state.x, state.y));
                state.x = to.x;
                state.y = to.y;
            }
        });
        from
    }

    /// Where a body stands, as last published.
    pub fn position_of(&self, body: u32) -> Option<Pos> {
        self.bodies
            .borrow()
            .iter()
            .find(|b| b.id == body)
            .map(|b| Pos::new(b.x, b.y))
    }

    /// The bodies standing in a chunk, in id order.
    pub fn bodies_in(&self, chunk: ChunkPos) -> Vec<BodyState> {
        self.bodies
            .borrow()
            .iter()
            .filter(|b| Pos::new(b.x, b.y).chunk() == chunk)
            .copied()
            .collect()
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

    /// The chunk a watcher without a body looks at: the middle of the map.
    fn centre_chunk(&self) -> ChunkPos {
        Pos::new(
            i32::try_from(self.map.width() / 2).expect("map fits in i32"),
            i32::try_from(self.map.height() / 2).expect("map fits in i32"),
        )
        .chunk()
    }

    /// The message carrying a chunk and the bodies in it.
    fn chunk_message(&self, chunk: ChunkPos) -> Option<ServerMessage> {
        let tiles = self.map.chunk(chunk)?;
        Some(ServerMessage::Chunk {
            x: chunk.x,
            y: chunk.y,
            tiles: tiles.into_iter().map(terrain_view).collect(),
            bodies: self.bodies_in(chunk),
            items: self.items.get(&chunk).cloned().unwrap_or_default(),
        })
    }
}

/// The chunks a connection holds: a square window around a centre chunk,
/// clipped to the map.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct View {
    chunks: BTreeSet<ChunkPos>,
}

impl View {
    /// The window around `centre`, clipped to the map.
    fn window(map: &Map, centre: ChunkPos) -> BTreeSet<ChunkPos> {
        let mut chunks = BTreeSet::new();
        for dy in -VIEW_RADIUS..=VIEW_RADIUS {
            for dx in -VIEW_RADIUS..=VIEW_RADIUS {
                let chunk = ChunkPos::new(centre.x + dx, centre.y + dy);
                if map.has_chunk(chunk) {
                    chunks.insert(chunk);
                }
            }
        }
        chunks
    }

    /// Moves the window to `centre`. Returns the chunks now shown and
    /// those now hidden, each in a fixed order.
    pub fn recentre(&mut self, map: &Map, centre: ChunkPos) -> (Vec<ChunkPos>, Vec<ChunkPos>) {
        let next = Self::window(map, centre);
        let shown = next.difference(&self.chunks).copied().collect();
        let hidden = self.chunks.difference(&next).copied().collect();
        self.chunks = next;
        (shown, hidden)
    }

    /// Is this tile inside the view?
    pub fn contains(&self, pos: Pos) -> bool {
        self.chunks.contains(&pos.chunk())
    }

    /// Chunks held.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Holds nothing?
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
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

    send(
        &mut ws,
        &ServerMessage::WorldState {
            width: shared.map.width(),
            height: shared.map.height(),
            chunk_size: CHUNK_SIZE,
            materials: shared.materials.clone(),
        },
    )
    .await?;

    // The view starts around the piloted body, or the middle of the map.
    let centre = pilot
        .body
        .and_then(|body| shared.position_of(body))
        .map_or_else(|| shared.centre_chunk(), Pos::chunk);
    let mut view = View::default();
    let (shown, _) = view.recentre(&shared.map, centre);
    for chunk in shown {
        if let Some(message) = shared.chunk_message(chunk) {
            send(&mut ws, &message).await?;
        }
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
                Ok(published) => forward(&mut ws, shared, pilot.body, &mut view, published).await?,
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

/// Forwards what the simulation published, as far as this connection's
/// view is concerned, moving the view along with the piloted body.
async fn forward(
    ws: &mut WebSocketStream<TcpStream>,
    shared: &Shared,
    piloted: Option<u32>,
    view: &mut View,
    published: Published,
) -> Result<(), BoxError> {
    match published {
        Published::DayStarted { day, tick } => {
            send(ws, &ServerMessage::DayStarted { day, tick }).await?;
        }
        Published::BodyMoved {
            body,
            from,
            to,
            tick,
        } => {
            if Some(body) == piloted && from.chunk() != to.chunk() {
                let (shown, hidden) = view.recentre(&shared.map, to.chunk());
                for chunk in hidden {
                    send(
                        ws,
                        &ServerMessage::ChunkHidden {
                            x: chunk.x,
                            y: chunk.y,
                        },
                    )
                    .await?;
                }
                for chunk in shown {
                    if let Some(message) = shared.chunk_message(chunk) {
                        send(ws, &message).await?;
                    }
                }
            }
            if view.contains(to) {
                send(
                    ws,
                    &ServerMessage::BodyMoved {
                        body,
                        x: to.x,
                        y: to.y,
                        tick,
                    },
                )
                .await?;
            } else if view.contains(from) {
                send(ws, &ServerMessage::BodyLeft { body }).await?;
            }
        }
    }
    Ok(())
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

    fn shared(max_connections: usize) -> Shared {
        Shared::new(&World::default(), &Content::sample(), max_connections)
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

    #[test]
    fn the_view_is_a_window_clipped_to_the_map() {
        let map = Map::valley(256, 256, 1); // 8×8 chunks
        let mut view = View::default();
        let (shown, hidden) = view.recentre(&map, ChunkPos::new(4, 4));
        assert_eq!(shown.len(), 25);
        assert_eq!(hidden, Vec::new());
        assert!(
            view.contains(Pos::new(64, 64)),
            "chunk (2, 2) is the corner"
        );
        assert!(!view.contains(Pos::new(63, 64)), "chunk (1, 2) is outside");

        // Sliding one chunk east: a column enters, a column leaves.
        let (shown, hidden) = view.recentre(&map, ChunkPos::new(5, 4));
        assert_eq!(
            shown,
            (2..7).map(|y| ChunkPos::new(7, y)).collect::<Vec<_>>()
        );
        assert_eq!(
            hidden,
            (2..7).map(|y| ChunkPos::new(2, y)).collect::<Vec<_>>()
        );

        // In the corner of the map the window is clipped.
        let (shown, _) = View::default().recentre(&map, ChunkPos::new(0, 0));
        assert_eq!(shown.len(), 9);
        assert!(View::default().is_empty());
    }

    #[test]
    fn a_chunk_message_carries_the_bodies_standing_in_it() {
        let shared = shared(1);
        let body = shared.position_of(1).expect("body 1 exists");
        let message = shared
            .chunk_message(body.chunk())
            .expect("chunk on the map");
        let ServerMessage::Chunk {
            tiles,
            bodies,
            items,
            ..
        } = message
        else {
            panic!("expected a chunk");
        };
        assert_eq!(tiles.len(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
        assert!(bodies.iter().any(|b| b.id == 1));
        assert!(
            !items.is_empty(),
            "the sample content scatters berries on grass"
        );
        assert!(
            items
                .iter()
                .all(|i| Pos::new(i.x, i.y).chunk() == body.chunk())
        );
        assert!(
            shared
                .materials
                .iter()
                .any(|m| m.id == "berries" && m.name == "Berries")
        );
        assert!(shared.chunk_message(ChunkPos::new(-1, 0)).is_none());

        // Moving a body reports where it stood and updates the chunk.
        let far = Pos::new(10, 250);
        assert_eq!(shared.body_moved(BodyId(1), far), Some(body));
        assert_eq!(shared.body_moved(BodyId(99), far), None);
        assert!(shared.bodies_in(far.chunk()).iter().any(|b| b.id == 1));
        assert!(!shared.bodies_in(body.chunk()).iter().any(|b| b.id == 1));
    }
}
