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
//! the client is told the window it now holds (`view`). A chunk the
//! client still keeps at its version, in view or in its cache, is not
//! sent again: `chunk_same` goes instead (D4).
//!
//! Limits, from the milestone 1 audit: a client has [`HANDSHAKE_TIMEOUT`]
//! to upgrade to WebSocket and then as long again to say hello; messages
//! larger than [`MAX_MESSAGE_BYTES`] end the connection; at most
//! `max_connections` clients are served at once; and a body keeps only the
//! latest intention per tick, so a flooding client costs one path search
//! per tick and bothers nobody else.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use protocol::{
    Blockage, BodyState, ChunkFrame, ClientMessage, Clock, Condition, DeathCause, FloraView,
    ItemAct, ItemState, ItemView, MaterialView, NaturalItem, PROTOCOL_VERSION, Point, Refusal,
    ServerMessage, Side, SpeciesView, Standing, StoredItem, Terrain, Thought, decode, encode,
    encode_chunk,
};
use sim::ground::{Ground, Version};
use sim::lru::Lru;
use sim::{BodyId, CHUNK_SIZE, ChunkPos, Content, FinePos, Intention, ItemId, Map, Pos, World};

use crate::interest::{self, Inbox, Router};
use crate::pool::ChunkPool;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::{Bytes, Message};

/// How long a client has to finish the WebSocket upgrade, and then how
/// long it has to send its `hello`.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest incoming message, in bytes. Client messages are a few dozen
/// bytes; anything near this limit is not our client.
pub const MAX_MESSAGE_BYTES: usize = 4096;

/// Largest `hello`, in bytes: the one message that may carry the chunks a
/// client holds ([`protocol::HELD_CHUNKS_MAX`] entries, about 25 bytes
/// each in JSON at most).
pub const HELLO_MAX_BYTES: usize = 64 * 1024;

/// Chunks held around the body's own chunk, in every direction: a window
/// of `(2 × VIEW_RADIUS + 1)²` chunks, 608 m across (19 × 19, the project
/// lead's call: it was 13 × 13, then 15 × 15). The client's view reaches
/// 288 tiles from the body (as far as the window always reaches),
/// following it tile by tile; its last tiles fade under a veil into a fog
/// where the camera stops, with nothing drawn under the fog.
pub const VIEW_RADIUS: i32 = 9;

/// Chunks whose plants the server keeps at most, the least recently asked
/// for forgotten first: four views' worth (4 × 19 × 19 = 1,444 chunks).
///
/// The plants of a chunk are placed again from the seed whenever they are
/// asked for after being forgotten (`sim::flora`, a few milliseconds a
/// chunk), identical, so the bound changes nothing on screen; only the
/// memory is bounded, by this number rather than by the land visited
/// (D39). Four views keep, for a lone player, the view around them and
/// the land just left behind, so turning back costs nothing, and the
/// camp's view warmed before the first connection; for a few players
/// apart, a view each. Measured on the game's valley (9 November 2026), a
/// chunk holds about 110 plants of 12 bytes, 500 at most: the cache holds
/// about 2 MB, under 9 MB were every chunk the densest.
pub const PLANT_CACHE_CHUNKS: usize = {
    let side = 2 * VIEW_RADIUS.unsigned_abs() as usize + 1;
    4 * side * side
};

/// Chunk frames the server keeps encoded at most, the least recently
/// sent forgotten first: as many as the chunks whose plants it keeps
/// ([`PLANT_CACHE_CHUNKS`]), about 2 KB each. A chunk is encoded once per
/// version of its ground and of its items, and the same bytes go to every
/// client asking for it (D26).
pub const FRAME_CACHE_CHUNKS: usize = PLANT_CACHE_CHUNKS;

/// Far views held, encoded, by the chunk they centre on: a few players'
/// worth, about 64 KB each. Counting one costs the 2 km of land around it
/// (64 × 64 chunks' tiles, made per chunk since the map holds only the
/// water, D37): done once on the chunk pool, sent to every client centred
/// there.
pub const FAR_VIEWS_HELD: usize = 16;

/// How many published messages may wait for a connection before it
/// misses some and is told its view again ([`Link::resync`]). A
/// connection falls behind only when its task is starved of the network
/// threads: it never waits on the wire nor builds chunks itself since
/// the chunk pool (11 November 2026); it was 256.
const INBOX_CAPACITY: usize = 1024;

/// Messages handed to a connection's writer task ahead of the wire, at
/// most: one, so an event never waits behind more than one chunk frame.
const WRITER_AHEAD: usize = 1;

/// Events a connection may have waiting to leave before it is judged
/// too slow to be shown the world and disconnected: a few hundred
/// kilobytes, minutes of play at today's rates. Events are never dropped
/// (a world with holes in it is worse than a lost connection); chunk
/// frames no longer in view are.
pub const EVENT_QUEUE_LIMIT: usize = 8192;

/// Deaths remembered, the latest kept, to tell a connection that fell
/// behind of its own body's death with the cause (the rest only leave).
const DEATHS_REMEMBERED: usize = 256;

/// How many private messages (refusals) a pilot may have waiting before
/// new ones are dropped.
const PRIVATE_CAPACITY: usize = 16;

type BoxError = Box<dyn Error + Send + Sync>;

/// What the simulation thread publishes, handed to the connections it
/// concerns by the place it happens in ([`Router`], D5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Published {
    /// A new game day began.
    DayStarted {
        /// The day.
        day: u64,
        /// The tick at which it started.
        tick: u64,
    },
    /// A body set off along a route.
    BodyWalks {
        /// Who.
        body: u32,
        /// The tile it starts from.
        from: Pos,
        /// When.
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
    /// A body stopped walking.
    BodyStopped {
        /// Who.
        body: u32,
        /// The tile it stands on.
        at: Pos,
    },
    /// Night fell.
    NightFell {
        /// The day it ends.
        day: u64,
    },
    /// A body lay down or got up.
    BodySleeps {
        /// Who.
        body: u32,
        /// Where.
        at: Pos,
        /// Asleep now?
        asleep: bool,
    },
    /// A body died where it stood.
    BodyDied {
        /// Who.
        body: u32,
        /// Where.
        at: Pos,
        /// Of what.
        cause: DeathCause,
    },
    /// An item appeared on the ground.
    ItemAdded {
        /// The item.
        item: ItemState,
        /// Its chunk's version right after it.
        version: Version,
    },
    /// An item vanished from the ground.
    ItemRemoved {
        /// Which.
        item: u64,
        /// Where it was.
        at: Pos,
        /// Its chunk's version right after it.
        version: Version,
    },
    /// A body changed what it holds in hand.
    BodyHolds {
        /// Who.
        body: u32,
        /// Where.
        at: Pos,
        /// In hand now.
        item: Option<ItemView>,
    },
}

/// FNV-1a, 64 bits: a hash the same on every run and every machine, for
/// the world's identity (the standard library's may change).
struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
        // A separator, so that ("ab", "c") and ("a", "bc") differ.
        self.0 = (self.0 ^ 0xff).wrapping_mul(0x0100_0000_01b3);
    }
}

fn point(at: FinePos) -> Point {
    Point { x: at.x, y: at.y }
}

/// A body as the clients are told it: where it stands, its walk, its
/// sleep and its hand.
fn body_state_of(world: &World, body: &sim::Body, content: &Content) -> BodyState {
    let timeline = world.walk_timeline(body.id(), content);
    BodyState {
        id: body.id().0,
        at: point(body.at()),
        route: timeline.iter().map(|&(p, _)| point(p)).collect(),
        arrivals: timeline.iter().map(|&(_, t)| t).collect(),
        speed_cm_per_s: content.movement.walk_speed_cm_per_s,
        asleep: body.condition().asleep,
        holds: body.held().and_then(|id| world.item(id)).map(|i| ItemView {
            id: i.id().0,
            material: i.material().to_owned(),
            mass_g: i.mass_g(),
        }),
    }
}

/// A connection waiting for a body to be born at the camp for it: no
/// free body was left to pilot. The simulation thread takes it before
/// its next tick ([`Shared::take_births`]).
#[derive(Debug)]
struct Birth {
    /// The channel for messages meant for that pilot alone.
    private: mpsc::Sender<ServerMessage>,
    /// Where the connection learns the id of its body.
    born: mpsc::UnboundedSender<u32>,
}

impl Birth {
    /// Has the connection gone meanwhile? No body is born for it.
    fn is_gone(&self) -> bool {
        self.born.is_closed()
    }
}

/// State shared between the simulation thread and all connections.
#[derive(Debug)]
pub struct Shared {
    /// The world's current tick, updated by the simulation thread.
    pub tick: AtomicU64,
    /// The connections by the chunks they hold and the chunks within
    /// their range: what the simulation thread publishes goes only to the
    /// connections it concerns (D5).
    router: Mutex<Router>,
    /// The ground. Static for now: nothing changes the terrain yet.
    pub map: Arc<Map>,
    /// Every body as last published, by id and by the chunk it stands in,
    /// kept by the simulation thread.
    bodies: Mutex<Bodies>,
    /// What items are made of, as clients need it.
    pub materials: Vec<MaterialView>,
    /// The kinds of tree, as clients need them.
    pub species: Vec<SpeciesView>,
    /// The content's kinds of tree, the trees of a chunk are placed with
    /// the first time it is asked for ([`Ground::trees_in`]). Nothing
    /// fells them yet.
    tree_kinds: Vec<sim::content::Species>,
    /// The placed trees of the chunks asked for lately, the world's own
    /// cache shared by handle (as the map is): a chunk's trees are placed
    /// once for the chunk's message, for the plants of the nine chunks
    /// around it and for the simulation alike, and held while the chunk
    /// is among the [`sim::ground::TREE_CACHE_CHUNKS`] asked for most
    /// recently. Keyed by the chunk and its version, so felling, when it
    /// comes, bumps the version and the chunk is placed again.
    ground: Arc<Ground>,
    /// The kinds of shrub, plant and mushroom, as clients need them.
    pub flora: Vec<FloraView>,
    /// The content's flora (trees still to come included), to place it.
    flora_kinds: Vec<sim::flora::Kind>,
    /// Index in `flora` of each kind of the content's flora (`None` for
    /// the trees still to come).
    flora_wire: Vec<Option<u8>>,
    /// The seed the trees and the flora are placed with (the map's).
    seed: u64,
    /// The plants of the chunks asked for lately, at most
    /// [`PLANT_CACHE_CHUNKS`] chunks (the cost follows the chunks in use,
    /// never the map's size nor the land visited, D39). Shared, so a hit
    /// holds the lock only to find them, never to copy them. Only what the
    /// seed gives may live here: a change to a plant (one picked, later)
    /// must be kept apart and laid over these, or forgetting a chunk would
    /// undo it.
    plants: Mutex<Lru<ChunkPos, Arc<[Standing]>>>,
    /// The chunk frames sent lately, encoded, by chunk, version of its
    /// ground and version of its items (at most [`FRAME_CACHE_CHUNKS`]):
    /// what a frame holds besides the bodies, which travel apart.
    frames: Mutex<Lru<FrameKey, Bytes>>,
    /// The far views counted lately, encoded, by the chunk they centre on
    /// (at most [`FAR_VIEWS_HELD`]): the land seen from afar is the
    /// seed's, the same for every client.
    far_views: Mutex<Lru<ChunkPos, Message>>,
    /// The index of each material in `materials`, by its content id.
    material_index: BTreeMap<String, u16>,
    /// Is it night? Kept by the simulation thread.
    pub night: AtomicBool,
    /// The current game day, kept by the simulation thread.
    pub day: AtomicU64,
    /// The latest deaths and their cause, for a connection that fell
    /// behind the world while its own body died.
    deaths: Mutex<Lru<u32, DeathCause>>,
    /// Simulation ticks per game second.
    ticks_per_second: u32,
    /// Ticks per game day.
    ticks_per_day: u64,
    /// How many times faster than real time the server runs, in thousandths.
    time_scale_permille: u32,
    /// The stored items on the ground, by chunk, the natural ones taken,
    /// and each chunk's version, kept by the simulation thread. The
    /// ground's natural ones are not here: they are made again each time a
    /// chunk is sent ([`sim::items`]), so this holds what men changed,
    /// never the land visited (D39).
    items: Mutex<Items>,
    /// What the ground's natural items are made from: whether this world
    /// has them, the spawn rules, the seed.
    natural: Option<(Vec<sim::content::Spawn>, u64)>,
    /// The world's identity, told in `world_state` (D4): a hash of what
    /// the chunk frames are made from, so a client never shows a chunk
    /// kept from another world, or from this one generated otherwise.
    world: String,
    /// Connections served at once, at most.
    max_connections: usize,
    /// Connections currently served.
    connections: AtomicUsize,
    /// The latest intention of each body, taken by the simulation thread
    /// before each tick.
    intentions: Mutex<BTreeMap<u32, Intention>>,
    /// Bodies currently piloted, with the channel for messages meant for
    /// that pilot alone.
    pilots: Mutex<BTreeMap<u32, mpsc::Sender<ServerMessage>>>,
    /// Connections waiting for a body born at the camp, in the order
    /// they asked.
    births: Mutex<Vec<Birth>>,
    /// The development tools (`--dev`): the map's overview sent to every
    /// client, and teleports accepted. `None` in play.
    dev_overview: Option<ServerMessage>,
}

/// The bodies as last published: by id, and by the chunk each stands
/// in, so that a step's bookkeeping and the bodies of a chunk are
/// lookups, never a scan of every body (D39).
#[derive(Debug, Default)]
struct Bodies {
    by_id: BTreeMap<u32, BodyState>,
    by_chunk: BTreeMap<ChunkPos, std::collections::BTreeSet<u32>>,
}

impl Bodies {
    /// Adds a body where it stands.
    fn insert(&mut self, state: BodyState) {
        self.by_chunk
            .entry(tile_of(state.at).chunk())
            .or_default()
            .insert(state.id);
        self.by_id.insert(state.id, state);
    }

    /// Forgets a body; what was known of it.
    fn remove(&mut self, id: u32) -> Option<BodyState> {
        let state = self.by_id.remove(&id)?;
        let chunk = tile_of(state.at).chunk();
        if let Some(ids) = self.by_chunk.get_mut(&chunk) {
            ids.remove(&id);
            if ids.is_empty() {
                self.by_chunk.remove(&chunk);
            }
        }
        Some(state)
    }

    /// A body as last published.
    fn get(&self, id: u32) -> Option<&BodyState> {
        self.by_id.get(&id)
    }

    /// A body to change, but not where it stands (see [`Bodies::place`]).
    fn get_mut(&mut self, id: u32) -> Option<&mut BodyState> {
        self.by_id.get_mut(&id)
    }

    /// Stands a body at `at`, keeping the chunks right; the tile it came
    /// from, or `None` for a body nobody knew about.
    fn place(&mut self, id: u32, at: Point) -> Option<Pos> {
        let state = self.by_id.get_mut(&id)?;
        let from = tile_of(state.at);
        state.at = at;
        let (before, after) = (from.chunk(), tile_of(at).chunk());
        if before != after {
            if let Some(ids) = self.by_chunk.get_mut(&before) {
                ids.remove(&id);
                if ids.is_empty() {
                    self.by_chunk.remove(&before);
                }
            }
            self.by_chunk.entry(after).or_default().insert(id);
        }
        Some(from)
    }

    /// The bodies standing in a chunk, in id order.
    fn in_chunk(&self, chunk: ChunkPos) -> Vec<BodyState> {
        self.by_chunk.get(&chunk).map_or_else(Vec::new, |ids| {
            ids.iter()
                .filter_map(|id| self.by_id.get(id).cloned())
                .collect()
        })
    }
}

/// What men changed on the ground as the connections see it: the items
/// dropped, by chunk, the natural ones taken, and each chunk's version
/// (D4): the simulation's own ([`World::chunk_version`]), as each item
/// event told it, so a frame and its version always agree. A connection
/// that sent a chunk at a version, then each item event, knows its client
/// holds that chunk right; one that fell behind compares and sends again
/// what differs.
#[derive(Debug, Default)]
struct Items {
    stored: BTreeMap<ChunkPos, Vec<ItemState>>,
    /// The natural items taken off the ground: left out when their chunk
    /// is sent. It grows with what bodies pick up, never with the land.
    taken: BTreeSet<ItemId>,
    /// Absent means 0. Grows with the chunks men changed, never with the
    /// land visited.
    versions: BTreeMap<ChunkPos, Version>,
}

impl Items {
    /// Notes `chunk`'s version after a change.
    fn changed(&mut self, chunk: ChunkPos, version: Version) {
        let known = self.versions.entry(chunk).or_insert(0);
        *known = (*known).max(version);
    }
}

/// What the map and the seed give a chunk, built on the chunk pool: the
/// ground, the trees, the plants, the currents, the natural items. What
/// changes on it (the bodies standing there, the items taken or dropped)
/// is added as the frame leaves ([`Shared::chunk_send_of`]), so that
/// it is as fresh as the events around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkGround {
    /// Which chunk.
    pub chunk: ChunkPos,
    /// Its version when its trees were placed.
    version: sim::ground::Version,
    tiles: Vec<Terrain>,
    trees: Vec<Standing>,
    plants: Vec<Standing>,
    currents: Vec<u16>,
    /// The ground's natural items, none taken out yet.
    natural: Vec<ItemState>,
}

/// A chunk frame's key in the cache: the chunk, the version its trees
/// were placed at, its version as the frame carries it.
type FrameKey = (ChunkPos, Version, Version);

/// A chunk as it leaves: its frame, encoded once for every client, and
/// the bodies standing in it now, which follow it in their own message.
#[derive(Debug, Clone)]
pub struct ChunkSend {
    /// The frame ([`encode_chunk`]).
    pub frame: Bytes,
    /// The bodies standing in the chunk.
    pub bodies: Vec<BodyState>,
    /// The chunk's version the frame carries.
    pub version: Version,
}

/// Millimetres from the corner of `chunk` to `at`, as a frame carries
/// them.
fn offset_in(chunk: ChunkPos, at: FinePos) -> Option<(u16, u16)> {
    let origin = chunk.origin();
    let mm = sim::MM_PER_TILE;
    Some((
        u16::try_from(at.x - origin.x * mm).ok()?,
        u16::try_from(at.y - origin.y * mm).ok()?,
    ))
}

/// Tiles each cell of the development overview stands for, either way,
/// at least; more on a map so large the overview would pass
/// `OVERVIEW_MOST` cells across.
pub const OVERVIEW_CELL: u32 = 8;
/// Cells the overview has across, at most, whatever the map's size: the
/// message stays a few hundred kilobytes.
pub const OVERVIEW_MOST: u32 = 512;

/// Tiles a cell of the overview stands for, for a map `side` tiles across.
pub fn overview_cell(side: u32) -> u32 {
    OVERVIEW_CELL.max(side.div_ceil(OVERVIEW_MOST))
}

/// The whole map, coarse (`ServerMessage::DevOverview`): each cell is the
/// water crossing it when there is enough of it to see (a river, a ford,
/// a brook), else its most common ground.
pub fn overview(map: &Map) -> ServerMessage {
    let cell = overview_cell(map.width().max(map.height()));
    let (width, height) = (map.width().div_ceil(cell), map.height().div_ceil(cell));
    ServerMessage::DevOverview {
        cell,
        width,
        height,
        cells: coarse(map, cell, 0, 0, width, height),
    }
}

/// The map coarse, one letter a cell of `cell` tiles, over `width` ×
/// `height` cells from cell (`x0`, `y0`): the water crossing a cell when
/// there is enough of it to see (`w` river, `o` ford, `b` brook), else its
/// most common ground (`g` grass, `f` forest, `r` rock); `.` past the map.
fn coarse(map: &Map, cell: u32, x0: i32, y0: i32, width: u32, height: u32) -> String {
    // Enough water to see: 6 tiles of 64 for a river, 3 for a brook.
    let (river, brook) = (
        (cell * cell * 6).div_ceil(64),
        (cell * cell * 3).div_ceil(64),
    );
    let (cols, rows) = (
        i32::try_from(map.width().div_ceil(cell)).expect("small"),
        i32::try_from(map.height().div_ceil(cell)).expect("small"),
    );
    let mut cells = String::with_capacity((width * height) as usize);
    for j in 0..i32::try_from(height).expect("small") {
        for i in 0..i32::try_from(width).expect("small") {
            let (cx, cy) = (x0 + i, y0 + j);
            if cx < 0 || cy < 0 || cx >= cols || cy >= rows {
                cells.push('.');
                continue;
            }
            let (cx, cy) = (cx.unsigned_abs(), cy.unsigned_abs());
            // Grass, forest, rock, river, ford, brook.
            let count = map.count_in(cx * cell, cy * cell, (cx + 1) * cell, (cy + 1) * cell);
            let letter = if count[3] + count[4] >= river {
                if count[4] > count[3] { 'o' } else { 'w' }
            } else if count[5] >= brook {
                'b'
            } else {
                let most = (0..3).max_by_key(|k| count[*k]).unwrap_or(0);
                ['g', 'f', 'r'][most]
            };
            cells.push(letter);
        }
    }
    cells
}

/// Tiles a cell of the far view stands for, either way.
pub const FAR_CELL: u32 = 8;
/// Cells the far view has across: 2 km of land around the body.
pub const FAR_CELLS: u32 = 256;
/// Chunks the body may go from the far view's centre before it is sent
/// again.
pub const FAR_RESEND: i32 = 16;

/// The land around chunk `centre`, coarse (`ServerMessage::FarView`), for
/// the client to draw past the chunks it holds: what can be seen from
/// afar anyway, never what lies in it (bodies, items).
pub fn far_view(map: &Map, centre: ChunkPos) -> ServerMessage {
    let cell = i32::try_from(FAR_CELL).expect("small");
    let size = i32::try_from(CHUNK_SIZE).expect("small");
    let half = i32::try_from(FAR_CELLS / 2).expect("small");
    let (cx, cy) = (
        (centre.x * size + size / 2).div_euclid(cell),
        (centre.y * size + size / 2).div_euclid(cell),
    );
    let (x, y) = (cx - half, cy - half);
    ServerMessage::FarView {
        cell: FAR_CELL,
        x,
        y,
        width: FAR_CELLS,
        height: FAR_CELLS,
        cells: coarse(map, FAR_CELL, x, y, FAR_CELLS, FAR_CELLS),
    }
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
    #[allow(
        clippy::too_many_lines,
        reason = "one field a line: every part of the shared state, set up"
    )]
    pub fn new(world: &World, content: &Content, max_connections: usize, time_scale: f64) -> Self {
        let materials: Vec<MaterialView> = content
            .materials
            .iter()
            .map(|m| MaterialView {
                id: m.id.clone(),
                name: m.name.clone(),
                description: m.description.clone(),
                colour: m.look.colour.clone(),
                edible: m.food.is_some(),
                mass_g: grams(m.mass_kg),
            })
            .collect();
        let species: Vec<SpeciesView> = content
            .species
            .iter()
            .map(|s| SpeciesView {
                id: s.id.clone(),
                name: s.name.clone(),
                height_cm: centimetres(s.height_m),
            })
            .collect();
        let (flora, flora_wire) = flora_views(content);
        let mut items = Items {
            taken: world.taken().collect(),
            versions: world
                .chunk_changes()
                .map(|(chunk, changes)| (chunk, changes.version))
                .collect(),
            ..Items::default()
        };
        for item in world.items() {
            let Some(pos) = item.pos() else {
                continue; // carried: not on the ground
            };
            items
                .stored
                .entry(pos.chunk())
                .or_default()
                .push(ItemState {
                    id: item.id().0,
                    x: pos.x,
                    y: pos.y,
                    material: item.material().to_owned(),
                    mass_g: item.mass_g(),
                });
        }
        let mut bodies = Bodies::default();
        for b in world.bodies() {
            bodies.insert(body_state_of(world, b, content));
        }
        let mut shared = Self {
            tick: AtomicU64::new(world.tick().0),
            router: Mutex::new(Router::default()),
            map: world.shared_map(),
            bodies: Mutex::new(bodies),
            material_index: materials
                .iter()
                .enumerate()
                .map(|(i, m)| (m.id.clone(), u16::try_from(i).expect("few materials")))
                .collect(),
            materials,
            species,
            tree_kinds: content.species.clone(),
            ground: world.shared_ground(),
            flora,
            flora_kinds: content.flora.clone(),
            flora_wire,
            seed: world.seed(),
            plants: Mutex::new(Lru::new(PLANT_CACHE_CHUNKS)),
            frames: Mutex::new(Lru::new(FRAME_CACHE_CHUNKS)),
            far_views: Mutex::new(Lru::new(FAR_VIEWS_HELD)),
            night: AtomicBool::new(world.is_night(content)),
            day: AtomicU64::new(world.time().day),
            deaths: Mutex::new(Lru::new(DEATHS_REMEMBERED)),
            ticks_per_second: world.calendar().ticks_per_second(),
            ticks_per_day: world.calendar().ticks_per_day(),
            time_scale_permille: permille(time_scale),
            items: Mutex::new(items),
            natural: world
                .has_natural_items()
                .then(|| (content.spawns.clone(), world.seed())),
            world: String::new(),
            max_connections,
            connections: AtomicUsize::new(0),
            intentions: Mutex::new(BTreeMap::new()),
            pilots: Mutex::new(BTreeMap::new()),
            births: Mutex::new(Vec::new()),
            dev_overview: None,
        };
        shared.world = shared.world_identity(world);
        shared
    }

    /// The far view around `centre`, encoded: held, or else counted now
    /// and held. Slow when counted (the 2 km around, tile by tile): run
    /// on the chunk pool, never on the network's threads. `None` if it
    /// could not be encoded.
    pub fn far_view_message(&self, centre: ChunkPos) -> Option<Message> {
        let lock = || self.far_views.lock().expect("far views not poisoned");
        if let Some(message) = lock().get(&centre).cloned() {
            return Some(message);
        }
        match encode(&far_view(&self.map, centre)) {
            Ok(text) => {
                let message = Message::text(text);
                lock().insert(centre, message.clone());
                Some(message)
            }
            Err(e) => {
                eprintln!("the far view around {centre:?} could not be encoded: {e}");
                None
            }
        }
    }

    /// Chunks sampled into the world's identity: the middle of the map
    /// and four around it, a quarter of the way to each edge.
    const IDENTITY_SAMPLES: [(i64, i64); 5] = [(2, 2), (1, 1), (3, 1), (1, 3), (3, 3)];

    /// The world's identity (D4): a hash of the map's spec, of the kinds
    /// the frames name by index (materials, trees, flora), and of the
    /// frames of a few chunks as the seed made them, so that the same
    /// world generated by other code (a server built since) is another
    /// world to a client. Sixteen hex digits.
    fn world_identity(&mut self, world: &World) -> String {
        // Worked out on caches of its own, so the shared ones hold only
        // what was asked for.
        let (ground, plants) = (
            std::mem::replace(&mut self.ground, Arc::new(Ground::new(64))),
            std::mem::replace(&mut self.plants, Mutex::new(Lru::new(16))),
        );
        let mut hash = Fnv::default();
        hash.write(format!("{:?}", world.map().spec()).as_bytes());
        for m in &self.materials {
            hash.write(m.id.as_bytes());
        }
        for s in &self.species {
            hash.write(s.id.as_bytes());
        }
        for f in &self.flora {
            hash.write(f.id.as_bytes());
        }
        let side = |n: u32| i64::from(n.div_ceil(sim::CHUNK_SIZE));
        let (w, h) = (side(self.map.width()), side(self.map.height()));
        for (qx, qy) in Self::IDENTITY_SAMPLES {
            let chunk = ChunkPos::new(
                i32::try_from(w * qx / 4).unwrap_or(0),
                i32::try_from(h * qy / 4).unwrap_or(0),
            );
            if let Some(ground) = self.chunk_ground(chunk) {
                let natural = ground.natural.clone();
                if let Ok(frame) = self
                    .frame_of(ground, &natural, &[], 0)
                    .and_then(|f| Ok(encode_chunk(&f)?))
                {
                    hash.write(&frame);
                }
            }
        }
        (self.ground, self.plants) = (ground, plants);
        format!("{:016x}", hash.0)
    }

    /// Folds the stored world's birth and epoch into its identity, once,
    /// at startup: a world made anew, or a run after a crash, is another
    /// world to a client, which drops the chunk frames it holds (a chunk's
    /// version may have gone back to a number it holds with other
    /// content). Both 0 (an ephemeral world) leave it as it is.
    pub fn fold_lineage(&mut self, birth: u64, epoch: u64) {
        if (birth, epoch) != (0, 0) {
            let mut hash = Fnv::default();
            hash.write(self.world.as_bytes());
            hash.write(&birth.to_le_bytes());
            hash.write(&epoch.to_le_bytes());
            self.world = format!("{:016x}", hash.0);
        }
    }

    /// The world's identity, as `world_state` tells it.
    pub fn world_id(&self) -> &str {
        &self.world
    }

    /// Turns the development tools on (`--dev`): every client gets the
    /// map's overview, and teleports are accepted.
    pub fn enable_dev(&mut self) {
        self.dev_overview = Some(overview(&self.map));
    }

    /// Are the development tools on?
    pub fn is_dev(&self) -> bool {
        self.dev_overview.is_some()
    }

    /// Hands what the simulation published to the connections it
    /// concerns (D5): the day and the night to all, an item's change to
    /// those holding its chunk, a body's to those within range of it and
    /// to its pilot.
    pub fn publish(&self, event: &Published) {
        self.router().publish(event);
    }

    /// A new connection's end of the router.
    pub fn subscribe(&self) -> Inbox {
        self.router().subscribe(INBOX_CAPACITY)
    }

    /// The router, locked: by the simulation thread at each event, by a
    /// connection as its view moves.
    fn router(&self) -> std::sync::MutexGuard<'_, Router> {
        self.router.lock().expect("router not poisoned")
    }

    /// Records where a walking body is and the rest of its walk, as the
    /// simulation will walk it ([`World::walk_timeline`]): as it sets
    /// off, and again at each chunk it walks into, so that what it is
    /// told agrees with the world whatever changed (its load, the ground).
    pub fn body_walks(
        &self,
        body: BodyId,
        at: FinePos,
        timeline: &[(FinePos, u64)],
        speed_cm_per_s: u32,
    ) {
        let mut bodies = self.bodies();
        if bodies.place(body.0, point(at)).is_some()
            && let Some(state) = bodies.get_mut(body.0)
        {
            state.route = timeline.iter().map(|&(p, _)| point(p)).collect();
            state.arrivals = timeline.iter().map(|&(_, t)| t).collect();
            state.speed_cm_per_s = speed_cm_per_s;
        }
    }

    /// The bodies, locked: taken for a moment by the simulation thread
    /// at each event, and by a connection reading one body or a chunk's.
    fn bodies(&self) -> std::sync::MutexGuard<'_, Bodies> {
        self.bodies.lock().expect("bodies not poisoned")
    }

    /// Records a body's position as it crosses into another tile at
    /// `tick`, leaving out the waypoints it passed by then. Returns the
    /// tile it came from, or `None` for a body nobody knew about.
    pub fn body_moved(&self, body: BodyId, at: FinePos, tick: u64) -> Option<Pos> {
        let mut bodies = self.bodies();
        let from = bodies.place(body.0, point(at))?;
        if let Some(state) = bodies.get_mut(body.0) {
            let passed = state.arrivals.iter().take_while(|&&t| t <= tick).count();
            state.route.drain(..passed);
            state.arrivals.drain(..passed);
        }
        Some(from)
    }

    /// Records what a body holds in hand.
    pub fn body_holds(&self, body: BodyId, item: Option<ItemView>) {
        if let Some(state) = self.bodies().get_mut(body.0) {
            state.holds = item;
        }
    }

    /// Records whether a body is asleep; true when that changed.
    pub fn body_sleeps(&self, body: BodyId, asleep: bool) -> bool {
        let mut changed = false;
        if let Some(state) = self.bodies().get_mut(body.0) {
            changed = state.asleep != asleep;
            state.asleep = asleep;
        }
        changed
    }

    /// Forgets a dead body, remembering of what it died; returns the
    /// tile it died on, if it was known.
    pub fn body_died(&self, body: BodyId, cause: DeathCause) -> Option<Pos> {
        self.deaths
            .lock()
            .expect("deaths not poisoned")
            .insert(body.0, cause);
        self.bodies().remove(body.0).map(|state| tile_of(state.at))
    }

    /// What `body` died of, if it died lately.
    pub fn death_of(&self, body: u32) -> Option<DeathCause> {
        self.deaths
            .lock()
            .expect("deaths not poisoned")
            .get(&body)
            .copied()
    }

    /// Records that a day began.
    pub fn day_started(&self, day: u64) {
        self.day.store(day, Ordering::Relaxed);
        self.night.store(false, Ordering::Relaxed);
    }

    /// Records that night fell.
    pub fn night_fell(&self) {
        self.night.store(true, Ordering::Relaxed);
    }

    /// Records an item lying on the ground, and its chunk's version
    /// right after it ([`World::chunk_version`]).
    pub fn item_added(&self, item: ItemState, version: Version) {
        let chunk = Pos::new(item.x, item.y).chunk();
        let mut items = self.items.lock().expect("items not poisoned");
        items.stored.entry(chunk).or_default().push(item);
        items.changed(chunk, version);
    }

    /// `chunk`'s version as the item events told it: the version of the
    /// frame it would be sent in now.
    pub fn chunk_version(&self, chunk: ChunkPos) -> Version {
        self.items
            .lock()
            .expect("items not poisoned")
            .versions
            .get(&chunk)
            .copied()
            .unwrap_or(0)
    }

    /// Forgets an item that was on the ground, and records its chunk's
    /// version right after it.
    pub fn item_removed(&self, item: ItemId, at: Pos, version: Version) {
        // Under the one lock `ground_items` holds while it gathers a
        // chunk's: a chunk sent at the same moment sees the item either
        // there or gone, never twice, and the version that goes with it.
        let mut items = self.items.lock().expect("items not poisoned");
        if sim::items::is_natural(item) {
            items.taken.insert(item);
        }
        if let Some(stored) = items.stored.get_mut(&at.chunk()) {
            stored.retain(|i| i.id != item.0);
        }
        items.changed(at.chunk(), version);
    }

    /// Records that a body stopped exactly there.
    pub fn body_stopped(&self, body: BodyId, at: FinePos) {
        let mut bodies = self.bodies();
        if bodies.place(body.0, point(at)).is_some()
            && let Some(state) = bodies.get_mut(body.0)
        {
            state.route.clear();
            state.arrivals.clear();
        }
    }

    /// The tile a body stands on, as last published.
    pub fn position_of(&self, body: u32) -> Option<Pos> {
        self.bodies().get(body).map(|b| tile_of(b.at))
    }

    /// A body as last published.
    pub fn body_state(&self, body: u32) -> Option<BodyState> {
        self.bodies().get(body).cloned()
    }

    /// The bodies standing in a chunk, in id order.
    pub fn bodies_in(&self, chunk: ChunkPos) -> Vec<BodyState> {
        self.bodies().in_chunk(chunk)
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
    pub fn intend(&self, intention: Intention) {
        let body = match &intention {
            Intention::MoveTo { body, .. }
            | Intention::Eat { body, .. }
            | Intention::PickUp { body, .. }
            | Intention::Drop { body, .. }
            | Intention::Hold { body, .. }
            | Intention::Stow { body, .. }
            | Intention::Sleep { body }
            | Intention::Wake { body }
            | Intention::Teleport { body, .. } => body.0,
        };
        self.intentions
            .lock()
            .expect("intentions not poisoned")
            .insert(body, intention);
    }

    /// Takes every pending intention, in body order, leaving none.
    pub fn take_intentions(&self) -> Vec<Intention> {
        let taken = std::mem::take(&mut *self.intentions.lock().expect("intentions not poisoned"));
        taken.into_values().collect()
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

    /// The bodies a connection pilots, in id order: told to the
    /// simulation before each tick, their path searches take the
    /// players' lane (D32).
    pub fn piloted(&self) -> Vec<BodyId> {
        self.pilots
            .lock()
            .expect("pilot set not poisoned")
            .keys()
            .map(|id| BodyId(*id))
            .collect()
    }

    /// Gives the connection the free body with the lowest id, if any.
    fn take_body(&self, private: mpsc::Sender<ServerMessage>) -> Option<u32> {
        let mut pilots = self.pilots.lock().expect("pilot set not poisoned");
        let free = self
            .bodies()
            .by_id
            .keys()
            .copied()
            .find(|id| !pilots.contains_key(id))?;
        pilots.insert(free, private);
        Some(free)
    }

    /// Gives the connection a free body if one is left (its pilot gone,
    /// or from an older world); else asks for one to be born at the camp
    /// for it, its id sent on `born` once the simulation thread made it.
    fn body_for(
        &self,
        private: mpsc::Sender<ServerMessage>,
        born: &mpsc::UnboundedSender<u32>,
    ) -> Option<u32> {
        let free = self.take_body(private.clone());
        if free.is_none() {
            self.births
                .lock()
                .expect("births not poisoned")
                .push(Birth {
                    private,
                    born: born.clone(),
                });
        }
        free
    }

    /// A body is born at the camp for each connection waiting for one, in
    /// the order they asked, none for a connection gone meanwhile: the
    /// simulation thread's, before a tick, as it takes the intentions.
    pub fn bear_bodies(&self, world: &mut World, content: &Content) {
        for birth in self.take_births() {
            if birth.is_gone() {
                continue;
            }
            let Some(body) = world.spawn_at_camp() else {
                eprintln!("no free grass on the map: no body born");
                continue;
            };
            let state = body_state_of(world, world.body(body).expect("just born"), content);
            self.body_born(birth, state);
        }
    }

    /// Takes every connection waiting for a body, in the order they
    /// asked, leaving none.
    fn take_births(&self) -> Vec<Birth> {
        std::mem::take(&mut *self.births.lock().expect("births not poisoned"))
    }

    /// Records a body just born at the camp for the connection that
    /// asked: it pilots it, and whoever sees the camp is told it stands
    /// there. A connection gone meanwhile leaves it free for the next.
    fn body_born(&self, birth: Birth, state: BodyState) {
        let (body, at) = (state.id, tile_of(state.at));
        {
            let mut pilots = self.pilots.lock().expect("pilot set not poisoned");
            pilots.insert(body, birth.private);
            self.bodies().insert(state);
        }
        if birth.born.send(body).is_err() {
            self.release_body(body);
        }
        self.publish(&Published::BodyStopped { body, at });
    }

    fn release_body(&self, body: u32) {
        self.pilots
            .lock()
            .expect("pilot set not poisoned")
            .remove(&body);
    }

    /// The clock as of now.
    pub fn clock(&self) -> Clock {
        Clock {
            tick: self.tick.load(Ordering::Relaxed),
            ticks_per_second: self.ticks_per_second,
            ticks_per_day: self.ticks_per_day,
            time_scale_permille: self.time_scale_permille,
        }
    }

    /// The chunk a watcher without a body looks at: the middle of the map.
    fn centre_chunk(&self) -> ChunkPos {
        Pos::new(
            i32::try_from(self.map.width() / 2).expect("map fits in i32"),
            i32::try_from(self.map.height() / 2).expect("map fits in i32"),
        )
        .chunk()
    }

    /// A chunk as it leaves ([`ChunkSend`]): its ground as the pool
    /// builds it, finished now.
    ///
    /// # Panics
    ///
    /// If the chunk cannot be framed (a tree off its chunk, more than
    /// 65,535 plants): a bug of the server.
    pub fn chunk_message(&self, chunk: ChunkPos) -> Option<ChunkSend> {
        self.chunk_ground(chunk).map(|ground| {
            self.chunk_send_of(ground)
                .expect("a chunk the frame carries")
        })
    }

    /// What the map and the seed give `chunk`, as the pool builds it:
    /// no lock the simulation thread takes is held while the plants are
    /// placed (D29). `None` off the map.
    pub fn chunk_ground(&self, chunk: ChunkPos) -> Option<ChunkGround> {
        let tiles = self.map.chunk(chunk)?;
        // Read before the trees are placed: they are at least as new.
        let version = self.ground.version(chunk);
        Some(ChunkGround {
            chunk,
            version,
            tiles: tiles.into_iter().map(terrain_view).collect(),
            trees: self.trees_in(chunk),
            plants: self.plants_in(chunk),
            currents: self.map.chunk_currents(chunk)?,
            natural: self.natural_items(chunk),
        })
    }

    /// A chunk's ground finished with what stands and lies on it now:
    /// its frame, encoded once per version of its ground and of its items
    /// and the same bytes then for every client, and the bodies in it.
    ///
    /// # Errors
    ///
    /// When the chunk cannot be framed ([`encode_chunk`]).
    pub fn chunk_send_of(&self, ground: ChunkGround) -> Result<ChunkSend, BoxError> {
        let chunk = ground.chunk;
        // Gathered together, as the items' events see them, so the frame
        // and its version agree.
        let (natural, stored, version) = self.ground_items(chunk, &ground.natural);
        let bodies = self.bodies_in(chunk);
        let key = (chunk, ground.version, version);
        let held = self
            .frames
            .lock()
            .expect("frames not poisoned")
            .get(&key)
            .cloned();
        let frame = if let Some(frame) = held {
            frame
        } else {
            let frame = Bytes::from(encode_chunk(
                &self.frame_of(ground, &natural, &stored, version)?,
            )?);
            self.frames
                .lock()
                .expect("frames not poisoned")
                .insert(key, frame.clone());
            frame
        };
        Ok(ChunkSend {
            frame,
            bodies,
            version,
        })
    }

    /// The frame of a chunk's ground and items, at `version`.
    fn frame_of(
        &self,
        ground: ChunkGround,
        natural: &[ItemState],
        stored: &[ItemState],
        version: Version,
    ) -> Result<ChunkFrame, BoxError> {
        let chunk = ground.chunk;
        // A material the client was not told of (none in the content's
        // catalogue) goes as an index past the list: unknown to the
        // client, as its id was before.
        let material = |id: &str| self.material_index.get(id).copied();
        let origin = chunk.origin();
        let natural = natural
            .iter()
            .map(|i| {
                Ok(NaturalItem {
                    slot: sim::items::slot_of(ItemId(i.id)),
                    material: material(&i.material)
                        .and_then(|m| u8::try_from(m).ok())
                        .unwrap_or(u8::MAX),
                })
            })
            .collect::<Result<Vec<_>, BoxError>>()?;
        let stored = stored
            .iter()
            .map(|i| {
                Ok(StoredItem {
                    id: i.id,
                    dx: u8::try_from(i.x - origin.x)?,
                    dy: u8::try_from(i.y - origin.y)?,
                    material: material(&i.material).unwrap_or(u16::MAX),
                    mass_g: i.mass_g,
                })
            })
            .collect::<Result<Vec<_>, BoxError>>()?;
        Ok(ChunkFrame {
            x: chunk.x,
            y: chunk.y,
            version,
            tiles: ground.tiles,
            currents: ground.currents,
            trees: ground.trees,
            plants: ground.plants,
            natural,
            stored,
        })
    }

    /// Development (`--dump-chunk`): `chunk` as a client decodes it from
    /// its frame, one line a thing, with the names of the kinds.
    ///
    /// # Errors
    ///
    /// When the chunk is off the map or cannot be framed.
    pub fn dump_chunk(&self, chunk: ChunkPos) -> Result<String, BoxError> {
        use std::fmt::Write as _;
        let ground = self.chunk_ground(chunk).ok_or("off the map")?;
        let sent = self.chunk_send_of(ground)?;
        let f = protocol::decode_chunk(&sent.frame)?;
        let side = f.tiles.len().isqrt().max(1);
        let mut out = format!(
            "chunk ({}, {}) version {}: {} bytes, {} trees, {} plants, {} natural items, {} stored\n",
            f.x,
            f.y,
            f.version,
            sent.frame.len(),
            f.trees.len(),
            f.plants.len(),
            f.natural.len(),
            f.stored.len()
        );
        for row in f.tiles.chunks(side) {
            let line: String = row
                .iter()
                .map(|t| match t {
                    Terrain::Grass => 'g',
                    Terrain::Forest => 'f',
                    Terrain::Water => 'w',
                    Terrain::Ford => 'o',
                    Terrain::Brook => 'b',
                    Terrain::Rock => 'r',
                })
                .collect();
            writeln!(out, "  {line}")?;
        }
        for (i, c) in f.currents.iter().enumerate().filter(|(_, c)| **c != 0) {
            writeln!(
                out,
                "  current tile ({}, {}): heading {}, {} cm/s",
                i % side,
                i / side,
                c >> 8,
                c & 0xff
            )?;
        }
        let name =
            |list: &[String], k: usize| list.get(k).cloned().unwrap_or_else(|| format!("#{k}"));
        let species: Vec<String> = self.species.iter().map(|s| s.id.clone()).collect();
        let flora: Vec<String> = self.flora.iter().map(|s| s.id.clone()).collect();
        let materials: Vec<String> = self.materials.iter().map(|s| s.id.clone()).collect();
        for t in &f.trees {
            writeln!(
                out,
                "  tree {} at +{} mm, +{} mm",
                name(&species, t.kind.into()),
                t.dx,
                t.dy
            )?;
        }
        for p in &f.plants {
            writeln!(
                out,
                "  plant {} at +{} mm, +{} mm",
                name(&flora, p.kind.into()),
                p.dx,
                p.dy
            )?;
        }
        for n in &f.natural {
            writeln!(
                out,
                "  natural item slot {}: {}",
                n.slot,
                name(&materials, n.material.into())
            )?;
        }
        for s in &f.stored {
            writeln!(
                out,
                "  stored item {} on tile +{}, +{}: {}",
                s.id,
                s.dx,
                s.dy,
                name(&materials, s.material.into())
            )?;
        }
        for b in &sent.bodies {
            writeln!(out, "  body {} at {} mm, {} mm", b.id, b.at.x, b.at.y)?;
        }
        Ok(out)
    }

    /// The ground's natural items in `chunk`, made again from the seed
    /// and the spawn rules ([`sim::items`]) on the pool, none taken out:
    /// what the seed gives, whoever took what since.
    fn natural_items(&self, chunk: ChunkPos) -> Vec<ItemState> {
        let Some((spawns, seed)) = &self.natural else {
            return Vec::new();
        };
        sim::items::natural_items(&self.map, spawns, *seed, chunk)
            .into_iter()
            .filter_map(|i| {
                let pos = i.pos()?;
                Some(ItemState {
                    id: i.id().0,
                    x: pos.x,
                    y: pos.y,
                    material: i.material().to_owned(),
                    mass_g: i.mass_g(),
                })
            })
            .collect()
    }

    /// The items lying in `chunk`: its `natural` ones, those taken
    /// excepted, and the stored ones, each in the order of their ids; and
    /// the chunk's version.
    fn ground_items(
        &self,
        chunk: ChunkPos,
        natural: &[ItemState],
    ) -> (Vec<ItemState>, Vec<ItemState>, Version) {
        // Filtered and gathered under the lock `item_removed` holds, so an
        // item taken meanwhile shows either here or nowhere.
        let items = self.items.lock().expect("items not poisoned");
        let mut natural: Vec<ItemState> = natural
            .iter()
            .filter(|i| !items.taken.contains(&ItemId(i.id)))
            .cloned()
            .collect();
        let mut stored = items.stored.get(&chunk).cloned().unwrap_or_default();
        let version = items.versions.get(&chunk).copied().unwrap_or(0);
        drop(items);
        natural.sort_by_key(|i| i.id);
        stored.sort_by_key(|i| i.id);
        (natural, stored, version)
    }

    /// The trees standing in `chunk`, placed from the map and the seed
    /// the first time they are asked for ([`Ground::trees_in`]).
    fn trees_in(&self, chunk: ChunkPos) -> Vec<Standing> {
        self.ground
            .trees_in(&self.map, &self.tree_kinds, self.seed, chunk)
            .iter()
            .map(|tree| {
                let (dx, dy) = offset_in(chunk, tree.at).expect("a tree stands in its chunk");
                Standing {
                    dx,
                    dy,
                    kind: u8::try_from(tree.species).expect("at most 256 kinds of tree"),
                }
            })
            .collect()
    }

    /// The trees of `chunk` and of the eight chunks around it, as the
    /// flora reads them (a plant stands clear of trunks over the chunk's
    /// border too), each chunk's from the shared cache: placed once, read
    /// nine times.
    fn trees_around(&self, chunk: ChunkPos) -> Vec<sim::trees::Tree> {
        (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| ChunkPos::new(chunk.x + dx, chunk.y + dy)))
            .flat_map(|around| {
                self.ground
                    .trees_in(&self.map, &self.tree_kinds, self.seed, around)
                    .to_vec()
            })
            .collect()
    }

    /// Places the plants of the view around the camp (where the first
    /// bodies stand) before any connection, so the first client is not
    /// kept waiting: the cost of a view, never of the map.
    pub fn warm_plants(&self) {
        let camp = self.map.camp().chunk();
        for chunk in View::window(&self.map, camp) {
            self.plants_in(chunk);
        }
    }

    /// The plants of a chunk (`sim::flora`), placed when the chunk is
    /// asked for and kept while it is among the [`PLANT_CACHE_CHUNKS`]
    /// asked for most recently. Placing them again gives the same plants,
    /// so whether they were kept never shows.
    fn plants_in(&self, chunk: ChunkPos) -> Vec<Standing> {
        let held = self
            .plants
            .lock()
            .expect("plants not poisoned")
            .get(&chunk)
            .map(Arc::clone);
        if let Some(plants) = held {
            return plants.to_vec(); // copied with the lock released
        }
        let trees = self.trees_around(chunk);
        let near: Vec<sim::flora::TreeAt<'_>> = trees
            .iter()
            .map(|tree| sim::flora::TreeAt {
                at: tree.at,
                species: self.tree_kinds[tree.species].id.as_str(),
            })
            .collect();
        let plants: Vec<Standing> =
            sim::flora::plants_in(&self.map, &self.flora_kinds, &near, self.seed, chunk)
                .into_iter()
                .filter_map(|p| {
                    let kind = self
                        .flora_wire
                        .get(usize::from(p.kind))
                        .copied()
                        .flatten()?;
                    let (dx, dy) = offset_in(chunk, p.at).expect("a plant stands in its chunk");
                    Some(Standing { dx, dy, kind })
                })
                .collect();
        // Placed with no lock held: two connections missing the same
        // chunk at once place the same plants, and the second insert only
        // replaces the first.
        self.plants
            .lock()
            .expect("plants not poisoned")
            .insert(chunk, Arc::from(plants.as_slice()));
        plants
    }
}

/// The kinds of shrub, plant and mushroom as clients need them, and the
/// index of each kind of the content's flora among them (`None` for the
/// trees still to come).
fn flora_views(content: &Content) -> (Vec<FloraView>, Vec<Option<u8>>) {
    let mut flora = Vec::new();
    let flora_wire = content
        .flora
        .iter()
        .map(|k| {
            let family = match k.family {
                sim::flora::Family::Tree => return None,
                sim::flora::Family::Shrub => "shrub",
                sim::flora::Family::Plant => "plant",
                sim::flora::Family::Mushroom => "mushroom",
            };
            flora.push(FloraView {
                id: k.id.clone(),
                name: k.name.clone(),
                family: family.to_owned(),
                height_cm: centimetres(k.height_m),
            });
            Some(u8::try_from(flora.len() - 1).expect("at most 256 kinds of flora"))
        })
        .collect();
    (flora, flora_wire)
}

/// The chunks a connection holds: a square window around a centre chunk,
/// clipped to the map.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct View {
    chunks: BTreeSet<ChunkPos>,
    /// The centre of the far view last sent, if any.
    far: Option<ChunkPos>,
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

    /// Is this chunk inside the view?
    pub fn contains_chunk(&self, chunk: ChunkPos) -> bool {
        self.chunks.contains(&chunk)
    }

    /// The window held, as `view` tells it: `None` while empty.
    fn window_rect(&self) -> Option<ServerMessage> {
        let first = self.chunks.first()?;
        let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
        for c in &self.chunks {
            (x0, y0, x1, y1) = (x0.min(c.x), y0.min(c.y), x1.max(c.x), y1.max(c.y));
        }
        Some(ServerMessage::View {
            x: x0,
            y: y0,
            width: (x1 - x0 + 1).unsigned_abs(),
            height: (y1 - y0 + 1).unsigned_abs(),
        })
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

/// A mass in grams, for the wire.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn grams(mass_kg: f32) -> u32 {
    (mass_kg * 1000.0).round().clamp(0.0, 1e9) as u32
}

/// A length in metres as whole centimetres, for the wire.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn centimetres(metres: f32) -> u32 {
    (metres * 100.0).round().clamp(0.0, 1e9) as u32
}

/// A pace as thousandths, for the wire; absurd paces saturate.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn permille(time_scale: f64) -> u32 {
    (time_scale * 1000.0)
        .round()
        .clamp(1.0, f64::from(u32::MAX)) as u32
}

/// The tile a published point is in.
fn tile_of(at: Point) -> Pos {
    FinePos::new(at.x, at.y).tile()
}

fn terrain_view(terrain: sim::Terrain) -> Terrain {
    match terrain {
        sim::Terrain::Grass => Terrain::Grass,
        sim::Terrain::Forest => Terrain::Forest,
        sim::Terrain::Water => Terrain::Water,
        sim::Terrain::Ford => Terrain::Ford,
        sim::Terrain::Brook => Terrain::Brook,
        sim::Terrain::Rock => Terrain::Rock,
    }
}

/// Accepts connections until `shutdown` becomes true. Every connection
/// builds its chunks on `pool`.
pub async fn serve(
    listener: TcpListener,
    shared: Arc<Shared>,
    pool: Arc<ChunkPool>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, peer)) => {
                    let shared = Arc::clone(&shared);
                    let pool = Arc::clone(&pool);
                    let shutdown = shutdown.clone();
                    tokio::spawn(async move {
                        let Some(_admission) = shared.admit() else {
                            // Dropping the stream closes it; the client sees
                            // the connection end before any upgrade.
                            eprintln!("connection {peer}: refused, {} connections already", shared.max_connections);
                            return;
                        };
                        if let Err(e) = handle_connection(stream, peer, &shared, &pool, shutdown).await {
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
struct Pilot {
    shared: Arc<Shared>,
    body: Option<u32>,
    /// Where a body born at the camp for this connection is told
    /// ([`Shared::body_for`]).
    born: mpsc::UnboundedSender<u32>,
}

impl Drop for Pilot {
    fn drop(&mut self) {
        if let Some(body) = self.body {
            self.shared.release_body(body);
        }
    }
}

/// The end a connection learns of the bodies born for it on. When the
/// connection ends, it is closed, and a body born for it that it never
/// took is given back: the next connection pilots it.
struct Cradle {
    shared: Arc<Shared>,
    born: mpsc::UnboundedReceiver<u32>,
}

impl Drop for Cradle {
    fn drop(&mut self) {
        self.born.close();
        while let Ok(body) = self.born.try_recv() {
            self.shared.release_body(body);
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    shared: &Arc<Shared>,
    pool: &Arc<ChunkPool>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), BoxError> {
    // The hello may carry the chunks a client holds (D4): the socket lets
    // that much through, and every message after it is held to
    // MAX_MESSAGE_BYTES below.
    let config = WebSocketConfig::default()
        .max_message_size(Some(HELLO_MAX_BYTES))
        .max_frame_size(Some(HELLO_MAX_BYTES));
    let upgrade = tokio_tungstenite::accept_async_with_config(stream, Some(config));
    let Ok(upgraded) = tokio::time::timeout(HANDSHAKE_TIMEOUT, upgrade).await else {
        // Silent TCP connections are dropped without ceremony.
        return Ok(());
    };
    let mut ws = upgraded?;
    println!("connection {peer}: opened");

    // Subscribe before answering hello so nothing is missed between the
    // welcome tick and the first published message.
    let mut inbox = shared.subscribe();
    let (private_tx, mut private) = mpsc::channel(PRIVATE_CAPACITY);
    let (born_tx, born) = mpsc::unbounded_channel();
    let mut cradle = Cradle {
        shared: Arc::clone(shared),
        born,
    };

    let pilot = Pilot {
        shared: Arc::clone(shared),
        body: None,
        born: born_tx,
    };
    let Some((pilot, held)) =
        handshake(&mut ws, pilot, private_tx.clone(), &mut cradle.born).await?
    else {
        ws.close(None).await?;
        return Ok(());
    };
    match pilot.body {
        Some(body) => println!("connection {peer}: pilots body {body}"),
        None => println!("connection {peer}: watching until a body is born for it"),
    }

    // From here on the socket is written by a task of its own, one
    // message at a time from the link's queue, so this task never waits
    // on the wire: it keeps reading the client and the world.
    let (mut sink, mut reader) = ws.split();
    let (writer_tx, mut writer_rx) = mpsc::channel::<Message>(WRITER_AHEAD);
    let writer = tokio::spawn(async move {
        while let Some(message) = writer_rx.recv().await {
            if sink.send(message).await.is_err() {
                return;
            }
        }
        let _ = sink.close().await;
    });
    let (results_tx, mut results) = mpsc::unbounded_channel();
    let mut link = Link::new(
        Arc::clone(shared),
        Arc::clone(pool),
        peer,
        inbox.id,
        pilot,
        private_tx,
        results_tx,
    );

    link.event(&ServerMessage::WorldState {
        width: shared.map.width(),
        height: shared.map.height(),
        chunk_size: CHUNK_SIZE,
        materials: shared.materials.clone(),
        species: shared.species.clone(),
        flora: shared.flora.clone(),
        night: shared.night.load(Ordering::Relaxed),
        clock: shared.clock(),
        world: shared.world_id().to_owned(),
    });
    if let Some(held) = held {
        link.held(&held);
    }
    if let Some(overview) = &shared.dev_overview {
        link.event(overview);
    }

    // The view starts around the piloted body, or the middle of the map.
    let centre = link.home();
    link.move_view(centre);
    link.far_view(centre);

    let outcome: Result<(), BoxError> = async {
        loop {
            // Checked here, not as the queue leaves: a client whose wire
            // is stuck takes nothing off it.
            link.check_pace()?;
            tokio::select! {
                incoming = reader.next() => match incoming {
                    None | Some(Ok(Message::Close(_))) => return Ok(()),
                    Some(Err(e)) => return Err(e.into()),
                    Some(Ok(message)) if message.len() > MAX_MESSAGE_BYTES => {
                        return Err(format!("a message of {} bytes, past the limit", message.len()).into());
                    }
                    Some(Ok(Message::Text(text))) => match decode::<ClientMessage>(text.as_str()) {
                        Ok(ClientMessage::Ping { nonce }) => {
                            link.event(&ServerMessage::Pong { nonce });
                        }
                        Ok(ClientMessage::Hello { .. }) => {
                            // A second hello is a client bug; drop the connection.
                            return Ok(());
                        }
                        Ok(ClientMessage::ChunkMissing { x, y }) => {
                            link.missing(ChunkPos::new(x, y));
                        }
                        Ok(ClientMessage::DevTeleport { .. }) if !shared.is_dev() => {
                            eprintln!("connection {peer}: teleport refused, the server is not in --dev");
                        }
                        Ok(message) => {
                            if let Some(body) = link.pilot.body {
                                shared.intend(intention_of(BodyId(body), &message));
                            }
                        }
                        Err(e) => {
                            eprintln!("connection {peer}: undecodable message ignored: {e}");
                        }
                    },
                    // Binary frames are not part of the protocol; ping/pong frames
                    // are handled by the WebSocket layer itself.
                    Some(Ok(_)) => {}
                },
                message = inbox.events.recv() => match message {
                    Some(published) => {
                        if link.forward(published) {
                            link.reassign();
                        }
                        if inbox.lagged.load(Ordering::Relaxed) > 0 {
                            // What the inbox still holds is forwarded first,
                            // late but whole; then the view is told again,
                            // newer than everything sent before it.
                            while let Ok(published) = inbox.events.try_recv() {
                                if link.forward(published) {
                                    link.reassign();
                                }
                            }
                            link.resync(inbox.lagged.swap(0, Ordering::Relaxed));
                        }
                    }
                    None => return Ok(()),
                },
                // The sender lives in the pilot set as long as this connection
                // pilots a body, so the channel never closes before the loop ends.
                Some(message) = private.recv() => link.event(&message),
                Some(body) = cradle.born.recv() => link.born(body),
                Some(built) = results.recv() => link.built(built),
                permit = writer_tx.reserve(), if link.has_output() => match permit {
                    Ok(permit) => {
                        let Some(message) = link.next_output()? else {
                            continue;
                        };
                        permit.send(message);
                    }
                    // The writer stopped: the socket is gone.
                    Err(_) => return Ok(()),
                },
                _ = shutdown.changed() => return Ok(()),
            }
        }
    }
    .await;
    // Let the writer send what it holds and the close frame.
    drop(link);
    drop(writer_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), writer).await;
    outcome
}

/// What a connection has to send: the lane a message goes in.
enum Outgoing {
    /// A chunk built on the pool, finished with what stands and lies in
    /// it when it leaves.
    Chunk(ChunkGround),
    /// A chunk the client keeps at this version: `chunk_same` instead of
    /// its frame, if the version has not moved when it leaves.
    Same(ChunkPos, Version),
    /// A message as is (the far view).
    Frame(Message),
}

/// What the pool built for a connection.
enum Built {
    /// A chunk's ground.
    Ground(ChunkGround),
    /// The far view around a chunk, encoded.
    Far(ChunkPos, Message),
}

/// A chunk in view whose frame (or `chunk_same`) left: the version of the
/// frame the client was given, what it keeps once the chunk leaves its
/// view, and the version it knows, the frame's then each item event's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Held {
    frame: Version,
    known: Version,
}

/// A connection past its handshake: what its client holds, what waits to
/// go out, and how the chunks it asks for come back.
///
/// Two lanes feed the writer task (its inbox holds [`WRITER_AHEAD`]
/// message), the first always before the second:
/// **events** (what the world published, pongs, refusals, hidden chunks),
/// then the **ground** (chunk frames, the far view). A burst of 361
/// chunks never holds back a body walking or a day starting. The ground
/// lane is bounded by the view itself: a chunk is asked for once per
/// showing, and a chunk no longer in view is dropped from the lane,
/// never built nor sent. The events lane is not dropped from: a client
/// that lets [`EVENT_QUEUE_LIMIT`] events pile up is disconnected rather
/// than shown a world with holes in it.
struct Link {
    shared: Arc<Shared>,
    pool: Arc<ChunkPool>,
    peer: SocketAddr,
    /// This connection's lane in the pool.
    lane: u64,
    /// This connection, as the router knows it.
    conn: u64,
    pilot: Pilot,
    /// The channel for messages meant for this pilot alone.
    private: mpsc::Sender<ServerMessage>,
    /// The chunks held (asked for or sent), and where the far view was
    /// last centred.
    view: View,
    /// The chunks held within the creatures' range of the view's centre
    /// ([`interest::BODY_RANGE_M`]): the bodies standing there are told,
    /// those farther are not (D5).
    range: BTreeSet<ChunkPos>,
    /// The chunks held, as the pool's jobs read them: a job for a chunk
    /// no longer here does nothing.
    wanted: Arc<Mutex<BTreeSet<ChunkPos>>>,
    /// The chunks in view whose frame has not left yet.
    awaiting: BTreeSet<ChunkPos>,
    /// The chunks in view whose frame left: what the client holds.
    sent: BTreeMap<ChunkPos, Held>,
    /// The frames the client keeps of chunks no longer in view, as it
    /// keeps them (D4): the chunk and its frame's version, the least
    /// recently hidden forgotten first, at most
    /// [`protocol::CHUNK_CACHE_CHUNKS`]. When one comes back into view at
    /// that version, `chunk_same` goes instead of the frame. A guess:
    /// should the client not hold it, it says so and gets the frame.
    cache: Lru<ChunkPos, Version>,
    /// The bodies the client was told of and not told to forget: all
    /// within range, as last told.
    known: BTreeSet<u32>,
    /// The events lane, encoded.
    events: VecDeque<Message>,
    /// The ground lane.
    ground: VecDeque<Outgoing>,
    /// The far view asked of the pool and not back yet: the chunk it
    /// centres on.
    far_awaited: Option<ChunkPos>,
    /// Where the pool's jobs send what they built.
    results: mpsc::UnboundedSender<Built>,
}

impl Drop for Link {
    fn drop(&mut self) {
        // The jobs still waiting in the lane find nothing wanted and
        // build nothing for a connection that is gone.
        self.wanted.lock().expect("wanted not poisoned").clear();
        self.shared.router().unsubscribe(self.conn);
    }
}

impl Link {
    fn new(
        shared: Arc<Shared>,
        pool: Arc<ChunkPool>,
        peer: SocketAddr,
        conn: u64,
        pilot: Pilot,
        private: mpsc::Sender<ServerMessage>,
        results: mpsc::UnboundedSender<Built>,
    ) -> Self {
        let lane = pool.lane();
        shared.router().set_pilot(conn, pilot.body);
        Self {
            shared,
            pool,
            peer,
            lane,
            conn,
            pilot,
            private,
            view: View::default(),
            range: BTreeSet::new(),
            wanted: Arc::default(),
            awaiting: BTreeSet::new(),
            sent: BTreeMap::new(),
            cache: Lru::new(protocol::CHUNK_CACHE_CHUNKS),
            known: BTreeSet::new(),
            events: VecDeque::new(),
            ground: VecDeque::new(),
            far_awaited: None,
            results,
        }
    }

    /// The chunk the view centres on: the piloted body's, or the middle
    /// of the map for a watcher.
    fn home(&self) -> ChunkPos {
        self.pilot
            .body
            .and_then(|body| self.shared.position_of(body))
            .map_or_else(|| self.shared.centre_chunk(), Pos::chunk)
    }

    /// Queues `message` in the events lane.
    fn event(&mut self, message: &ServerMessage) {
        match encode(message) {
            Ok(text) => self.events.push_back(Message::text(text)),
            Err(e) => eprintln!(
                "connection {}: a message could not be encoded: {e}",
                self.peer
            ),
        }
    }

    /// An error once the client let more than [`EVENT_QUEUE_LIMIT`]
    /// events pile up: too slow to be shown the world.
    fn check_pace(&self) -> Result<(), BoxError> {
        if self.events.len() > EVENT_QUEUE_LIMIT {
            return Err(format!(
                "too slow: {} events waiting, more than {EVENT_QUEUE_LIMIT}",
                self.events.len()
            )
            .into());
        }
        Ok(())
    }

    /// Is there anything to write?
    fn has_output(&self) -> bool {
        !self.events.is_empty() || !self.ground.is_empty()
    }

    /// The next message for the writer: an event before any ground. A
    /// chunk is finished here, as it leaves, with the bodies and items in
    /// it at this moment: every event forwarded before it is older than
    /// its snapshot and every one after it newer, as when the chunk was
    /// built in line. `None` when nothing waits (a chunk was dropped from
    /// the lane meanwhile); an error when a message cannot be encoded.
    fn next_output(&mut self) -> Result<Option<Message>, BoxError> {
        if let Some(message) = self.events.pop_front() {
            return Ok(Some(message));
        }
        match self.ground.pop_front() {
            None => Ok(None),
            Some(Outgoing::Frame(message)) => Ok(Some(message)),
            Some(Outgoing::Same(chunk, version)) => {
                if self.shared.chunk_version(chunk) != version {
                    // Changed since it was asked for: the frame, built.
                    self.build(chunk);
                    return Ok(None);
                }
                self.awaiting.remove(&chunk);
                let bodies = self.bodies_seen_in(chunk);
                self.same(chunk, version, bodies).map(Some)
            }
            Some(Outgoing::Chunk(ground)) => {
                let chunk = ground.chunk;
                let sent = self.shared.chunk_send_of(ground)?;
                // Kept by the client since the connection began (`hello`'s
                // `held`) and unchanged: no frame.
                if self.cache.remove(&chunk) == Some(sent.version) {
                    return self.same(chunk, sent.version, sent.bodies).map(Some);
                }
                self.tell_bodies(chunk, sent.bodies)?;
                self.sent.insert(
                    chunk,
                    Held {
                        frame: sent.version,
                        known: sent.version,
                    },
                );
                Ok(Some(Message::Binary(sent.frame)))
            }
        }
    }

    /// `chunk_same` for `chunk`, held at `version`, with the bodies in it.
    fn same(
        &mut self,
        chunk: ChunkPos,
        version: Version,
        bodies: Vec<BodyState>,
    ) -> Result<Message, BoxError> {
        self.tell_bodies(chunk, bodies)?;
        self.sent.insert(
            chunk,
            Held {
                frame: version,
                known: version,
            },
        );
        let same = ServerMessage::ChunkSame {
            x: chunk.x,
            y: chunk.y,
            version,
        };
        Ok(Message::text(encode(&same)?))
    }

    /// The bodies standing in `chunk` if it is within range, else none.
    fn bodies_seen_in(&self, chunk: ChunkPos) -> Vec<BodyState> {
        if self.range.contains(&chunk) {
            self.shared.bodies_in(chunk)
        } else {
            Vec::new()
        }
    }

    /// Queues the bodies standing in `chunk` right after its frame, if
    /// any and if it is within range.
    fn tell_bodies(&mut self, chunk: ChunkPos, bodies: Vec<BodyState>) -> Result<(), BoxError> {
        if !bodies.is_empty() && self.range.contains(&chunk) {
            self.known.extend(bodies.iter().map(|b| b.id));
            // Next out, before any other event: the bodies of the frame's
            // own moment.
            let bodies = ServerMessage::ChunkBodies {
                x: chunk.x,
                y: chunk.y,
                bodies,
            };
            self.events.push_front(Message::text(encode(&bodies)?));
        }
        Ok(())
    }

    /// The chunks a reconnecting client holds (`hello`'s `held`), the
    /// least recently hidden first: what it keeps, if of this world.
    fn held(&mut self, held: &protocol::HeldChunks) {
        if held.world != self.shared.world_id() {
            return; // another world's: everything is sent
        }
        // The newest, should there be too many.
        let skip = held.chunks.len().saturating_sub(protocol::HELD_CHUNKS_MAX);
        for &(x, y, version) in held.chunks.iter().skip(skip) {
            self.cache.insert(ChunkPos::new(x, y), version);
        }
    }

    /// The client does not hold `chunk` as `chunk_same` said: its frame,
    /// if still in view.
    fn missing(&mut self, chunk: ChunkPos) {
        self.cache.remove(&chunk);
        if self.sent.remove(&chunk).is_some() && self.awaiting.insert(chunk) {
            self.build(chunk);
        }
    }

    /// What the pool built: a chunk's ground or the far view.
    fn built(&mut self, built: Built) {
        match built {
            Built::Ground(ground) => self.ground_built(ground),
            Built::Far(centre, message) => self.far_built(centre, message),
        }
    }

    /// The far view the pool counted: into the ground lane if it is the
    /// one awaited, else dropped (the body went on, a newer one is asked).
    fn far_built(&mut self, centre: ChunkPos, message: Message) {
        if self.far_awaited == Some(centre) {
            self.far_awaited = None;
            self.ground.push_back(Outgoing::Frame(message));
        }
    }

    /// A chunk the pool built: into the ground lane if still in view and
    /// not sent meanwhile, else dropped.
    fn ground_built(&mut self, ground: ChunkGround) {
        if self.view.chunks.contains(&ground.chunk) && self.awaiting.remove(&ground.chunk) {
            self.ground.push_back(Outgoing::Chunk(ground));
        }
    }

    /// Asks for `chunk`, nearest the body first in the order called:
    /// `chunk_same` if the client keeps it at its version, else its ground
    /// of the pool.
    fn ask(&mut self, chunk: ChunkPos) {
        if !self.awaiting.insert(chunk) {
            return; // already asked for, not back yet
        }
        if let Some(version) = self.cache.remove(&chunk)
            && version == self.shared.chunk_version(chunk)
        {
            self.ground.push_back(Outgoing::Same(chunk, version));
            return;
        }
        self.build(chunk);
    }

    /// Asks the pool for `chunk`'s ground.
    fn build(&mut self, chunk: ChunkPos) {
        let (shared, wanted, results) = (
            Arc::clone(&self.shared),
            Arc::clone(&self.wanted),
            self.results.clone(),
        );
        self.pool.submit(
            self.lane,
            Box::new(move || {
                if !wanted.lock().expect("wanted not poisoned").contains(&chunk) {
                    return; // left the view before its turn came
                }
                if let Some(ground) = shared.chunk_ground(chunk) {
                    let _ = results.send(Built::Ground(ground));
                }
            }),
        );
    }

    /// Moves the view to `centre`, telling the client the window it now
    /// holds (`view`: it lets go of what lies outside, keeping the frames
    /// in its cache, rows then columns, as this mirror does) and asking
    /// for the new chunks, nearest the centre first.
    fn move_view(&mut self, centre: ChunkPos) {
        let (mut shown, mut hidden) = self.view.recentre(&self.shared.map, centre);
        self.wanted
            .lock()
            .expect("wanted not poisoned")
            .clone_from(&self.view.chunks);
        shown.sort_by_key(|c| ((c.x - centre.x).pow(2) + (c.y - centre.y).pow(2), c.y, c.x));
        hidden.sort_by_key(|c| (c.y, c.x));
        let ground = std::mem::take(&mut self.ground);
        for out in ground {
            match out {
                Outgoing::Chunk(g) if !self.view.chunks.contains(&g.chunk) => {}
                // Still kept by the client, never shown: kept here too.
                Outgoing::Same(chunk, version) if !self.view.chunks.contains(&chunk) => {
                    self.cache.insert(chunk, version);
                }
                out => self.ground.push_back(out),
            }
        }
        for &chunk in &hidden {
            self.awaiting.remove(&chunk);
            // What the client held goes to its cache; a chunk never sent
            // is only forgotten here.
            if let Some(held) = self.sent.remove(&chunk) {
                self.cache.insert(chunk, held.frame);
            }
        }
        if let Some(window) = self.view.window_rect()
            && (!shown.is_empty() || !hidden.is_empty())
        {
            self.event(&window);
        }
        self.move_range(centre);
        for chunk in shown {
            self.ask(chunk);
        }
    }

    /// Moves the creatures' range to `centre` (D5), and tells the router
    /// what this connection now holds and sees. The bodies of a chunk
    /// held that comes within range are told (`chunk_bodies`), those
    /// known that now stand out of it are forgotten (`body_left`); a chunk
    /// whose frame is still to leave brings its bodies along.
    fn move_range(&mut self, centre: ChunkPos) {
        let range: BTreeSet<ChunkPos> = interest::body_range(&self.shared.map, centre)
            .intersection(&self.view.chunks)
            .copied()
            .collect();
        // The router first: whatever is published from now on reaches
        // this connection, and what was before is in the bodies read next.
        self.shared
            .router()
            .set_interest(self.conn, &self.view.chunks, &range);
        let entered: Vec<ChunkPos> = range.difference(&self.range).copied().collect();
        let left = self.range.difference(&range).next().is_some();
        self.range = range;
        if left {
            let gone: Vec<u32> = self
                .known
                .iter()
                .copied()
                .filter(|&body| {
                    self.shared
                        .position_of(body)
                        .is_some_and(|at| !self.range.contains(&at.chunk()))
                })
                .collect();
            for body in gone {
                self.known.remove(&body);
                self.event(&ServerMessage::BodyLeft { body });
            }
        }
        for chunk in entered {
            if !self.sent.contains_key(&chunk) {
                continue; // its frame brings them
            }
            let bodies = self.shared.bodies_in(chunk);
            if bodies.is_empty() {
                continue;
            }
            self.known.extend(bodies.iter().map(|b| b.id));
            self.event(&ServerMessage::ChunkBodies {
                x: chunk.x,
                y: chunk.y,
                bodies,
            });
        }
    }

    /// Asks the pool for the far view around `centre`, after the chunks
    /// asked before it in the lane; it goes in the ground lane when back
    /// (`far_built`). Counting it reads 2 km of land, made per chunk: never
    /// on the network's threads.
    fn far_view(&mut self, centre: ChunkPos) {
        self.view.far = Some(centre);
        self.far_awaited = Some(centre);
        let (shared, results) = (Arc::clone(&self.shared), self.results.clone());
        self.pool.submit(
            self.lane,
            Box::new(move || {
                if results.is_closed() {
                    return; // the connection is gone
                }
                if let Some(message) = shared.far_view_message(centre) {
                    let _ = results.send(Built::Far(centre, message));
                }
            }),
        );
    }

    /// Provisional (D34, amended on 13 November 2026): a pilot whose body
    /// died takes a free body if one is left, else a new one is born for
    /// it at the camp ([`Link::born`]); the view moves there.
    fn reassign(&mut self) {
        if let Some(dead) = self.pilot.body.take() {
            self.shared.release_body(dead);
        }
        if let Some(body) = self.shared.body_for(self.private.clone(), &self.pilot.born) {
            self.pilots(body);
        } else {
            self.shared.router().set_pilot(self.conn, None);
            println!("connection {}: a body is born for it", self.peer);
        }
    }

    /// A body was born at the camp for this connection: it pilots it.
    /// One born after it found another is given back, free.
    fn born(&mut self, body: u32) {
        if self.pilot.body.is_some() {
            self.shared.release_body(body);
        } else {
            self.pilots(body);
        }
    }

    /// Pilots `body` from now on, and the view moves there.
    fn pilots(&mut self, body: u32) {
        self.pilot.body = Some(body);
        self.shared.router().set_pilot(self.conn, Some(body));
        println!("connection {}: now pilots body {body}", self.peer);
        self.event(&ServerMessage::BodyAssigned { body: Some(body) });
        let centre = self.home();
        self.move_view(centre);
    }

    /// Forwards what the simulation published, as far as this
    /// connection's view and range are concerned (the router hands it a
    /// little more: what was published before the view last moved),
    /// moving the view along with the piloted body. Returns true when the
    /// piloted body just died.
    fn forward(&mut self, published: Published) -> bool {
        let piloted = self.pilot.body;
        match published {
            Published::DayStarted { day, tick } => {
                self.event(&ServerMessage::DayStarted { day, tick });
            }
            Published::NightFell { day } => {
                self.event(&ServerMessage::NightFell { day });
            }
            Published::BodySleeps { body, at, asleep } => {
                if self.sees(at) {
                    self.event(&ServerMessage::BodySleeps { body, asleep });
                }
            }
            Published::BodyDied { body, at, cause } => {
                if self.sees(at) || Some(body) == piloted {
                    self.known.remove(&body);
                    self.event(&ServerMessage::BodyDied { body, cause });
                }
                return Some(body) == piloted;
            }
            Published::ItemAdded { item, version } => {
                let chunk = Pos::new(item.x, item.y).chunk();
                if self.view.contains_chunk(chunk) {
                    self.item_changed(chunk, version);
                    self.event(&ServerMessage::ItemAdded { item });
                }
            }
            Published::ItemRemoved { item, at, version } => {
                if self.view.contains(at) {
                    self.item_changed(at.chunk(), version);
                    self.event(&ServerMessage::ItemRemoved {
                        item,
                        x: at.x,
                        y: at.y,
                    });
                }
            }
            Published::BodyHolds { body, at, item } => {
                if self.sees(at) {
                    self.event(&ServerMessage::BodyHolds { body, item });
                }
            }
            Published::BodyWalks { body, from, tick } => {
                if self.sees(from) {
                    self.tell_walk(body, tick);
                }
            }
            Published::BodyMoved {
                body,
                from,
                to,
                tick,
            } => {
                if Some(body) == piloted && from.chunk() != to.chunk() {
                    self.move_view(to.chunk());
                    // The far view again once the body has gone far enough
                    // from where it was centred (a teleport, a long walk).
                    let moved = self.view.far.is_none_or(|c| {
                        (c.x - to.chunk().x).abs().max((c.y - to.chunk().y).abs()) > FAR_RESEND
                    });
                    if moved {
                        self.far_view(to.chunk());
                    }
                }
                // Published as a body walks into another chunk only (A6):
                // within range, the client is told where it is and its
                // route, whether it walked into range (it learns of it),
                // came from a chunk whose frame is still to leave (neither
                // frame carries it) or walked on in sight (the client's
                // walk, on open ground's speed, set right). Out of range,
                // it is forgotten.
                if self.sees(to) {
                    self.tell_walk(body, tick);
                } else if self.known.remove(&body) {
                    self.event(&ServerMessage::BodyLeft { body });
                }
            }
            Published::BodyStopped { body, at } => {
                if self.sees(at)
                    && let Some(state) = self.shared.body_state(body)
                {
                    self.known.insert(body);
                    self.event(&ServerMessage::BodyStopped { body, at: state.at });
                }
            }
        }
        false
    }

    /// Is this tile within the creatures' range, its bodies told?
    fn sees(&self, at: Pos) -> bool {
        self.range.contains(&at.chunk())
    }

    /// Tells the client where `body` is and where it is going.
    fn tell_walk(&mut self, body: u32, tick: u64) {
        if let Some(state) = self.shared.body_state(body) {
            self.known.insert(body);
            self.event(&walks_message(&state, tick));
        }
    }

    /// Notes that the client is told `chunk`'s items at `version`.
    fn item_changed(&mut self, chunk: ChunkPos, version: Version) {
        if let Some(held) = self.sent.get_mut(&chunk) {
            held.known = held.known.max(version);
        }
    }

    /// The client fell behind the world by `skipped` published messages
    /// it will never see: rather than a world with holes in it, it is
    /// told again, with today's messages, the state of what it holds. The
    /// day and whether it is night; each body standing in the chunks it
    /// holds, with its route, sleep and hand, and the bodies it knew that
    /// stand there no more; the chunks whose items changed, sent again
    /// (the only message that carries a chunk's items); its own body's
    /// death, and another body, if it died meanwhile.
    fn resync(&mut self, skipped: u64) {
        eprintln!(
            "connection {}: {skipped} message(s) behind the world: its view told again",
            self.peer
        );
        let shared = Arc::clone(&self.shared);
        let clock = shared.clock();
        // The tick is now, not the day's first: the client sets its clock
        // by it.
        let day = shared.day.load(Ordering::Relaxed);
        self.event(&ServerMessage::DayStarted {
            day,
            tick: clock.tick,
        });
        if shared.night.load(Ordering::Relaxed) {
            self.event(&ServerMessage::NightFell { day });
        }

        // The bodies in the chunks the client holds within range; those
        // in chunks still to leave come with their frame.
        let standing: BTreeMap<u32, BodyState> = self
            .sent
            .keys()
            .filter(|chunk| self.range.contains(chunk))
            .flat_map(|&chunk| shared.bodies_in(chunk))
            .map(|state| (state.id, state))
            .collect();
        let gone: Vec<u32> = self
            .known
            .iter()
            .copied()
            .filter(|id| !standing.contains_key(id))
            .collect();
        let own_death = self
            .pilot
            .body
            .filter(|&body| shared.body_state(body).is_none());
        for body in gone {
            self.known.remove(&body);
            if Some(body) == own_death {
                continue; // told below, with its cause
            }
            self.event(&ServerMessage::BodyLeft { body });
        }
        for (id, state) in standing {
            self.known.insert(id);
            if state.route.is_empty() {
                self.event(&ServerMessage::BodyStopped {
                    body: id,
                    at: state.at,
                });
            } else {
                self.event(&walks_message(&state, clock.tick));
            }
            self.event(&ServerMessage::BodySleeps {
                body: id,
                asleep: state.asleep,
            });
            self.event(&ServerMessage::BodyHolds {
                body: id,
                item: state.holds.clone(),
            });
        }

        // The chunks whose items the client may hold wrong: sent again.
        let stale: Vec<ChunkPos> = self
            .sent
            .iter()
            .filter(|(chunk, held)| shared.chunk_version(**chunk) != held.known)
            .map(|(chunk, _)| *chunk)
            .collect();
        for chunk in stale {
            if self.awaiting.insert(chunk) {
                self.build(chunk);
            }
        }

        if let Some(body) = own_death {
            match shared.death_of(body) {
                Some(cause) => self.event(&ServerMessage::BodyDied { body, cause }),
                // Too long ago to be remembered: at least it leaves.
                None => self.event(&ServerMessage::BodyLeft { body }),
            }
            self.reassign();
        }
    }
}

/// The intention a client message asks of the piloted body.
fn intention_of(body: BodyId, message: &ClientMessage) -> Intention {
    match *message {
        ClientMessage::MoveTo { x, y } => Intention::MoveTo {
            body,
            target: Pos::new(x, y),
        },
        ClientMessage::DevTeleport { x, y } => Intention::Teleport {
            body,
            target: Pos::new(x, y),
        },
        ClientMessage::Eat { item } => Intention::Eat {
            body,
            item: ItemId(item),
        },
        ClientMessage::PickUp { item } => Intention::PickUp {
            body,
            item: ItemId(item),
        },
        ClientMessage::Drop { item } => Intention::Drop {
            body,
            item: ItemId(item),
        },
        ClientMessage::Hold { item } => Intention::Hold {
            body,
            item: ItemId(item),
        },
        ClientMessage::Stow { item } => Intention::Stow {
            body,
            item: ItemId(item),
        },
        ClientMessage::Sleep => Intention::Sleep { body },
        ClientMessage::Wake => Intention::Wake { body },
        ClientMessage::Hello { .. }
        | ClientMessage::Ping { .. }
        | ClientMessage::ChunkMissing { .. } => {
            unreachable!("handled before intentions")
        }
    }
}

/// An act and a refusal as the wire says them.
pub fn refusal_view(act: sim::ItemAct, reason: sim::Refusal) -> (ItemAct, Refusal) {
    let act = match act {
        sim::ItemAct::PickUp => ItemAct::PickUp,
        sim::ItemAct::Drop => ItemAct::Drop,
        sim::ItemAct::Hold => ItemAct::Hold,
        sim::ItemAct::Stow => ItemAct::Stow,
        sim::ItemAct::Eat => ItemAct::Eat,
    };
    let reason = match reason {
        sim::Refusal::Unknown => Refusal::Unknown,
        sim::Refusal::OutOfReach => Refusal::OutOfReach,
        sim::Refusal::NotCarried => Refusal::NotCarried,
        sim::Refusal::TooHeavy => Refusal::TooHeavy,
        sim::Refusal::NotFood => Refusal::NotFood,
    };
    (act, reason)
}

/// What a pilot is told when its body stopped for good short of
/// `target` (D32).
pub fn blocked_view(
    body: BodyId,
    at: FinePos,
    target: Pos,
    reason: sim::Blockage,
) -> ServerMessage {
    ServerMessage::BodyBlocked {
        body: body.0,
        at: point(at),
        x: target.x,
        y: target.y,
        reason: blockage_view(reason),
    }
}

/// A refused order, as its pilot is told: what the body sees barring its
/// way too (protocol v21).
pub fn refused_view(body: BodyId, target: Pos, reason: sim::Blockage) -> ServerMessage {
    ServerMessage::MoveRefused {
        body: body.0,
        x: target.x,
        y: target.y,
        reason: blockage_view(reason),
    }
}

/// What a pilot is told its body thinks as it decides how to go on (D32).
pub fn thinks_view(body: BodyId, thought: sim::Thought, at: FinePos) -> ServerMessage {
    let side = |side: sim::Side| match side {
        sim::Side::Left => Side::Left,
        sim::Side::Right => Side::Right,
    };
    ServerMessage::BodyThinks {
        body: body.0,
        thought: match thought {
            sim::Thought::Skirting { side: s } => Thought::Skirting { side: side(s) },
            sim::Thought::WaySeen => Thought::WaySeen,
            sim::Thought::NoWay => Thought::NoWay,
        },
        at: point(at),
    }
}

/// What a body sees barring its way, on the wire.
fn blockage_view(reason: sim::Blockage) -> Blockage {
    match reason {
        sim::Blockage::Water => Blockage::Water,
        sim::Blockage::Rock => Blockage::Rock,
        sim::Blockage::NoWay => Blockage::NoWay,
    }
}

/// What a pilot is told of its body's condition.
pub fn condition_view(summary: (u32, u32, u32, u32, bool)) -> Condition {
    let (vitality, hunger, cold, fatigue, asleep) = summary;
    Condition {
        vitality,
        hunger,
        cold,
        fatigue,
        asleep,
    }
}

/// The message telling a client where a body is and where it is going.
fn walks_message(state: &BodyState, tick: u64) -> ServerMessage {
    ServerMessage::BodyWalks {
        body: state.id,
        at: state.at,
        route: state.route.clone(),
        arrivals: state.arrivals.clone(),
        speed_cm_per_s: state.speed_cm_per_s,
        tick,
    }
}

/// Runs the hello/welcome exchange: the pilot, and the chunks the client
/// says it holds. The pilot takes a free body if one is left, else waits
/// (a tick, normally) for one born at the camp: welcomed without one if
/// none came in time, it pilots it once it comes ([`Link::born`]).
/// Returns `Ok(None)` when the client must be disconnected (wrong
/// version, wrong first message, or silence).
async fn handshake(
    ws: &mut WebSocketStream<TcpStream>,
    mut pilot: Pilot,
    private: mpsc::Sender<ServerMessage>,
    born: &mut mpsc::UnboundedReceiver<u32>,
) -> Result<Option<(Pilot, Option<protocol::HeldChunks>)>, BoxError> {
    let first = tokio::time::timeout(HANDSHAKE_TIMEOUT, ws.next()).await;
    let Ok(Some(Ok(Message::Text(text)))) = first else {
        return Ok(None);
    };
    match decode::<ClientMessage>(text.as_str()) {
        Ok(ClientMessage::Hello {
            protocol_version,
            held,
        }) if protocol_version == PROTOCOL_VERSION => {
            pilot.body = pilot.shared.body_for(private, &pilot.born);
            if pilot.body.is_none() {
                pilot.body = tokio::time::timeout(HANDSHAKE_TIMEOUT, born.recv())
                    .await
                    .ok()
                    .flatten();
            }
            let welcome = ServerMessage::Welcome {
                protocol_version: PROTOCOL_VERSION,
                tick: pilot.shared.tick.load(Ordering::Relaxed),
                body: pilot.body,
            };
            send(ws, &welcome).await?;
            Ok(Some((pilot, held)))
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

/// Sends `message` on a socket not yet split: the handshake.
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

    /// A chunk as a client reads it: its frame decoded, its items rebuilt,
    /// and the bodies sent after it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Sent {
        x: i32,
        y: i32,
        frame: Bytes,
        tiles: Vec<Terrain>,
        trees: Vec<Standing>,
        items: Vec<ItemState>,
        bodies: Vec<BodyState>,
    }

    impl Sent {
        fn of(shared: &Shared, send: ChunkSend) -> Self {
            let f = protocol::decode_chunk(&send.frame).expect("a frame");
            Self {
                x: f.x,
                y: f.y,
                items: f.items(&shared.materials),
                tiles: f.tiles,
                trees: f.trees,
                frame: send.frame,
                bodies: send.bodies,
            }
        }
    }

    /// `chunk` as a client reads it, `None` off the map.
    fn sent(shared: &Shared, chunk: ChunkPos) -> Option<Sent> {
        shared
            .chunk_message(chunk)
            .map(|send| Sent::of(shared, send))
    }

    /// What leaves a link: a message, or a chunk with its bodies.
    #[derive(Debug)]
    enum Out {
        Message(ServerMessage),
        Chunk(Sent),
    }

    fn shared(max_connections: usize) -> Shared {
        Shared::new(&players(), &Content::sample(), max_connections, 1.0)
    }

    /// A small valley (the rules are the same, and it is made in a
    /// fraction of the time) where three bodies were born at the camp,
    /// as for three players.
    fn players() -> World {
        let mut world = World::with_valley(Calendar::default(), &Content::sample(), 768);
        for _ in 0..3 {
            world.spawn_at_camp().expect("grass by the camp");
        }
        world
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
            shared.intend(Intention::MoveTo {
                body: BodyId(1),
                target: Pos::new(0, 0),
            });
        }
        shared.intend(Intention::MoveTo {
            body: BodyId(2),
            target: Pos::new(5, 5),
        });
        shared.intend(Intention::Sleep { body: BodyId(1) });
        shared.intend(Intention::MoveTo {
            body: BodyId(1),
            target: Pos::new(3, 4),
        });
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
            reason: Blockage::NoWay,
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
    fn the_overview_stays_small_whatever_the_map() {
        assert_eq!(overview_cell(256), 8);
        assert_eq!(overview_cell(3072), 8);
        for side in [3072, 4096, 4097, 30_720, 307_200] {
            let cell = overview_cell(side);
            assert!(side.div_ceil(cell) <= OVERVIEW_MOST, "{side}: {cell}");
        }
        let ServerMessage::DevOverview {
            cell,
            width,
            height,
            cells,
        } = overview(&Map::valley(256, 256, 1))
        else {
            panic!("an overview");
        };
        assert_eq!((cell, width, height, cells.len()), (8, 32, 32, 32 * 32));
        assert!(cells.starts_with('r'), "the cliff along the north");
    }

    #[test]
    fn the_plants_kept_are_bounded_and_forgetting_them_changes_nothing() {
        assert_eq!(PLANT_CACHE_CHUNKS, 4 * 19 * 19, "four views' worth");
        // The repository's flora: the sample content has none.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = crate::content::load(&dir).expect("repository content");
        let world = World::with_valley(Calendar::default(), &content, 768);
        let never_forgets = Shared::new(&world, &content, 1, 1.0);
        let mut forgets = Shared::new(&world, &content, 1, 1.0);
        forgets.plants = Mutex::new(Lru::new(4));
        let chunks: Vec<ChunkPos> = View::window(&forgets.map, forgets.map.camp().chunk())
            .into_iter()
            .take(12)
            .collect();
        let first: Vec<Vec<Standing>> = chunks
            .iter()
            .map(|&chunk| {
                let plants = forgets.plants_in(chunk);
                assert!(forgets.plants.lock().unwrap().len() <= 4, "bounded");
                plants
            })
            .collect();
        assert!(
            first.iter().filter(|p| !p.is_empty()).count() >= 4,
            "the flora grows around the camp"
        );
        {
            let kept = forgets.plants.lock().unwrap();
            assert!(!kept.contains(&chunks[0]), "the first chunk was forgotten");
            assert!(kept.contains(&chunks[11]), "the last is kept");
        }
        // Asked again in the same order, every chunk has been forgotten
        // (each placing drops the next one to be asked for): placed again,
        // the same plants. Asked once more at once, it is kept: the same
        // plants again. And the same as a server that never forgot.
        for (&chunk, plants) in chunks.iter().zip(&first) {
            assert!(
                !forgets.plants.lock().unwrap().contains(&chunk),
                "forgotten"
            );
            assert_eq!(&forgets.plants_in(chunk), plants, "{chunk:?} placed again");
            assert!(forgets.plants.lock().unwrap().contains(&chunk), "kept");
            assert_eq!(&forgets.plants_in(chunk), plants, "{chunk:?} kept");
            assert_eq!(&never_forgets.plants_in(chunk), plants, "{chunk:?}");
        }
    }

    #[test]
    fn a_world_of_the_first_generator_sends_the_frames_of_before() {
        // The frames of every chunk of a 768 valley, as the code of 13
        // November 2026 sent them before the generator had versions
        // (FNV-1a of their bytes, end to end): a world made by the first
        // generator sends them byte for byte.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = crate::content::load(&dir).expect("repository content");
        let map = Map::valley_by(sim::map::FIRST_GENERATOR, 768, 768, sim::VALLEY_SEED);
        let world = World::on_valley(Calendar::default(), &content, map);
        let shared = Shared::new(&world, &content, 1, 1.0);
        let (mut print, mut bytes) = (0xcbf2_9ce4_8422_2325_u64, 0);
        for y in 0..24 {
            for x in 0..24 {
                let frame = shared.chunk_message(ChunkPos::new(x, y)).unwrap().frame;
                bytes += frame.len();
                for b in &frame {
                    print = (print ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
                }
            }
        }
        assert_eq!((print, bytes), (0xff43_cd45_98af_971b, 1_015_322));
    }

    #[test]
    fn the_trees_are_placed_once_and_a_hit_sends_the_bytes_of_a_miss() {
        // The repository's kinds of tree: the sample content has none.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = crate::content::load(&dir).expect("repository content");
        let world = World::with_valley(Calendar::default(), &content, 768);
        let shared = Shared::new(&world, &content, 1, 1.0);
        let chunks: Vec<ChunkPos> = View::window(&shared.map, shared.map.camp().chunk())
            .into_iter()
            .take(12)
            .collect();
        let mut misses = Vec::new();
        for &chunk in &chunks {
            // Straight from the generator, as the message tells them.
            let fresh: Vec<Standing> =
                sim::trees::trees_in(&shared.map, &shared.tree_kinds, shared.seed, chunk)
                    .into_iter()
                    .map(|tree| {
                        let (dx, dy) = offset_in(chunk, tree.at).unwrap();
                        Standing {
                            dx,
                            dy,
                            kind: u8::try_from(tree.species).unwrap(),
                        }
                    })
                    .collect();
            if misses.is_empty() {
                assert!(!shared.ground.holds_trees(chunk), "nothing asked for yet");
            }
            let miss = sent(&shared, chunk).expect("a chunk");
            assert!(shared.ground.holds_trees(chunk), "held once asked for");
            let hit = shared.chunk_message(chunk).expect("a chunk");
            assert_eq!(miss.trees, fresh, "{chunk:?}: a miss places the trees");
            assert_eq!(
                miss.frame, hit.frame,
                "{chunk:?}: a hit sends the same bytes"
            );
            assert_eq!(
                miss.frame.as_ptr(),
                hit.frame.as_ptr(),
                "{chunk:?}: encoded once, the very same bytes"
            );
            misses.push(miss);
        }
        assert!(
            misses.iter().any(|m: &Sent| !m.trees.is_empty()),
            "trees grow around the camp"
        );
        // Each chunk's trees were placed once, for its own message and
        // for the plants of its neighbours alike: the twelve chunks and
        // their rim are held, nothing beyond but the chunks the bodies
        // stand in (the world made sure none stands in a trunk).
        let rim: BTreeSet<ChunkPos> = chunks
            .iter()
            .flat_map(|c| {
                (-1..=1)
                    .flat_map(move |dy| (-1..=1).map(move |dx| ChunkPos::new(c.x + dx, c.y + dy)))
            })
            .chain(world.bodies().iter().map(|b| b.pos().chunk()))
            .collect();
        assert_eq!(shared.ground.trees_held(), rim.len());
        // The world reads the same cache: the very same trees.
        assert!(Arc::ptr_eq(
            &world.trees_in(chunks[0]),
            &shared
                .ground
                .trees_in(&shared.map, &shared.tree_kinds, shared.seed, chunks[0])
        ));
        // A server that forgets its trees at once sends the same chunks.
        let mut forgets = Shared::new(&world, &content, 1, 1.0);
        forgets.ground = Arc::new(Ground::new(1));
        forgets.plants = Mutex::new(Lru::new(1));
        for (&chunk, miss) in chunks.iter().zip(&misses) {
            assert_eq!(&sent(&forgets, chunk).unwrap(), miss, "{chunk:?}");
            assert_eq!(forgets.ground.trees_held(), 1, "bounded");
        }
        // A change to a chunk (felling, later) bumps its version: placed
        // again, the same today, nothing laying changes over them yet.
        shared.ground.set_version(chunks[0], 1);
        assert!(
            !shared.ground.holds_trees(chunks[0]),
            "the old version is stale"
        );
        // Encoded again. The version its frame carries is the one the
        // events told the network (D4): a felling will come with its own
        // event; none came here.
        let again = sent(&shared, chunks[0]).unwrap();
        assert_eq!(
            (&again.tiles, &again.trees, &again.items),
            (&misses[0].tiles, &misses[0].trees, &misses[0].items)
        );
        assert_eq!(protocol::decode_chunk(&again.frame).unwrap().version, 0);
        assert!(
            shared.ground.holds_trees(chunks[0]),
            "held at the new version"
        );
    }

    #[test]
    fn the_view_is_a_window_clipped_to_the_map() {
        let map = Map::valley(768, 768, 1); // 24×24 chunks
        let r = VIEW_RADIUS;
        let side = usize::try_from(2 * r + 1).expect("small");
        let chunk_tiles = i32::try_from(CHUNK_SIZE).expect("small chunk");
        let mut view = View::default();
        let (shown, hidden) = view.recentre(&map, ChunkPos::new(12, 12));
        assert_eq!(shown.len(), side * side);
        assert_eq!(hidden, Vec::new());
        let corner = (12 - r) * chunk_tiles;
        assert!(
            view.contains(Pos::new(corner, corner)),
            "the window's corner chunk is in"
        );
        assert!(
            !view.contains(Pos::new(corner - 1, corner)),
            "the chunk west of it is out"
        );

        // Sliding one chunk east: a column enters, a column leaves.
        let (shown, hidden) = view.recentre(&map, ChunkPos::new(13, 12));
        let rows = || 12 - r..=12 + r;
        assert_eq!(
            shown,
            rows().map(|y| ChunkPos::new(13 + r, y)).collect::<Vec<_>>()
        );
        assert_eq!(
            hidden,
            rows().map(|y| ChunkPos::new(12 - r, y)).collect::<Vec<_>>()
        );

        // In the corner of the map the window is clipped.
        let (shown, _) = View::default().recentre(&map, ChunkPos::new(0, 0));
        let quarter = usize::try_from(r + 1).expect("small");
        assert_eq!(shown.len(), quarter * quarter);
        assert!(View::default().is_empty());
    }

    #[test]
    fn sleep_death_and_items_are_recorded() {
        let shared = shared(1);
        let here = shared.position_of(1).expect("body 1");
        assert!(shared.body_sleeps(BodyId(1), true));
        assert!(
            !shared.body_sleeps(BodyId(1), true),
            "no change the second time"
        );
        assert!(shared.body_state(1).unwrap().asleep);
        assert_eq!(shared.body_died(BodyId(1), DeathCause::Starved), Some(here));
        assert_eq!(shared.body_died(BodyId(1), DeathCause::Froze), None);
        assert_eq!(shared.death_of(1), Some(DeathCause::Froze));
        assert!(shared.body_state(1).is_none());
        let corpse = ItemState {
            id: 999,
            x: here.x,
            y: here.y,
            material: "corpse".into(),
            mass_g: 60_000,
        };
        shared.item_added(corpse.clone(), 1);
        let Some(Sent { items, .. }) = sent(&shared, here.chunk()) else {
            panic!("expected a chunk");
        };
        // The sample content has no corpses: an index past the catalogue.
        let lying = |items: &[ItemState]| {
            items
                .iter()
                .any(|i| (i.id, i.x, i.y) == (corpse.id, corpse.x, corpse.y))
        };
        assert!(lying(&items));
        shared.item_removed(ItemId(999), here, 2);
        let Some(Sent { items, .. }) = sent(&shared, here.chunk()) else {
            panic!("expected a chunk");
        };
        assert!(!lying(&items));
    }

    #[test]
    fn natural_items_are_made_per_chunk_and_never_held() {
        let world = World::with_valley(Calendar::default(), &Content::sample(), 768);
        let shared = Shared::new(&world, &Content::sample(), 1, 1.0);
        let camp = world.map().camp().chunk();
        let mut natural = None;
        for chunk in View::window(&shared.map, camp) {
            let Some(Sent { items, trees, .. }) = sent(&shared, chunk) else {
                panic!("expected a chunk");
            };
            // The same items and trees as the world's.
            let ids: Vec<u64> = world.items_in(chunk).iter().map(|i| i.id().0).collect();
            assert_eq!(items.iter().map(|i| i.id).collect::<Vec<_>>(), ids);
            assert_eq!(trees.len(), world.trees_in(chunk).len());
            natural = natural.or_else(|| items.first().map(|i| (chunk, i.clone())));
        }
        // Sending the land held nothing: only the stored items are kept.
        assert!(shared.items.lock().unwrap().stored.is_empty());
        // One taken stays gone, sent again or not.
        let (chunk, item) = natural.expect("some natural item around the camp");
        assert!(sim::items::is_natural(ItemId(item.id)));
        shared.item_removed(ItemId(item.id), Pos::new(item.x, item.y), 1);
        for _ in 0..2 {
            let Some(Sent { items, .. }) = sent(&shared, chunk) else {
                panic!("expected a chunk");
            };
            assert!(items.iter().all(|i| i.id != item.id));
        }
    }

    #[test]
    fn a_chunk_message_carries_the_bodies_standing_in_it() {
        let shared = shared(1);
        let body = shared.position_of(1).expect("body 1 exists");
        let Sent { tiles, bodies, .. } = sent(&shared, body.chunk()).expect("chunk on the map");
        assert_eq!(tiles.len(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
        assert!(bodies.iter().any(|b| b.id == 1));
        // The items of a chunk are those lying in it; the sample content
        // scatters berries on the grass at the forest's edge, somewhere in
        // the body's view.
        let mut seen_items = false;
        for chunk in View::window(&shared.map, body.chunk()) {
            let Some(Sent { items, .. }) = sent(&shared, chunk) else {
                panic!("expected a chunk");
            };
            seen_items |= !items.is_empty();
            assert!(items.iter().all(|i| Pos::new(i.x, i.y).chunk() == chunk));
        }
        assert!(seen_items, "the sample content scatters berries on grass");
        assert!(
            shared
                .materials
                .iter()
                .any(|m| m.id == "berries" && m.name == "Berries" && m.mass_g == 100)
        );
        assert!(shared.chunk_message(ChunkPos::new(-1, 0)).is_none());

        // A walk records the route; crossing tiles trims it; stopping clears it.
        let far = Pos::new(10, 250);
        let route = [(Pos::new(12, 240).centre(), 50), (far.centre(), 90)];
        shared.body_walks(BodyId(1), body.centre(), &route, 143);
        assert_eq!(shared.body_state(1).unwrap().route.len(), 2);
        assert_eq!(shared.body_state(1).unwrap().arrivals, vec![50, 90]);
        assert_eq!(shared.body_moved(BodyId(1), far.centre(), 60), Some(body));
        assert_eq!(
            shared.body_state(1).unwrap().arrivals,
            vec![90],
            "the waypoint reached by then passed"
        );
        assert_eq!(shared.body_moved(BodyId(1), far.centre(), 90), Some(far));
        assert_eq!(shared.body_moved(BodyId(99), far.centre(), 90), None);
        assert_eq!(
            shared.body_state(1).unwrap().route,
            Vec::new(),
            "both waypoints passed"
        );
        assert!(shared.bodies_in(far.chunk()).iter().any(|b| b.id == 1));
        assert!(!shared.bodies_in(body.chunk()).iter().any(|b| b.id == 1));
        shared.body_stopped(BodyId(1), far.centre());
        assert_eq!(
            shared.body_state(1).unwrap().at,
            Point {
                x: far.centre().x,
                y: far.centre().y
            }
        );
    }

    /// A link as a connection past its handshake holds it, piloting
    /// `body`, with the end its chunks come back on and the pool they
    /// are built on; nothing on the wire.
    fn link(
        shared: &Arc<Shared>,
        pool: &Arc<ChunkPool>,
        body: Option<u32>,
    ) -> (Link, mpsc::UnboundedReceiver<Built>) {
        let (link, results_rx, _) = link_born(shared, pool, body);
        (link, results_rx)
    }

    /// The same, with the end the bodies born for it are told on.
    fn link_born(
        shared: &Arc<Shared>,
        pool: &Arc<ChunkPool>,
        body: Option<u32>,
    ) -> (
        Link,
        mpsc::UnboundedReceiver<Built>,
        mpsc::UnboundedReceiver<u32>,
    ) {
        let (private, _) = mpsc::channel(PRIVATE_CAPACITY);
        let (results, results_rx) = mpsc::unbounded_channel();
        let (born, born_rx) = mpsc::unbounded_channel();
        let pilot = Pilot {
            shared: Arc::clone(shared),
            body,
            born,
        };
        let peer = "127.0.0.1:1".parse().expect("an address");
        let conn = shared.subscribe().id;
        let link = Link::new(
            Arc::clone(shared),
            Arc::clone(pool),
            peer,
            conn,
            pilot,
            private,
            results,
        );
        (link, results_rx, born_rx)
    }

    /// Hands the link every chunk (and far view) the pool built for it
    /// until nothing is awaited any more.
    fn settle(link: &mut Link, results: &mut mpsc::UnboundedReceiver<Built>) {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while !link.awaiting.is_empty() || link.far_awaited.is_some() {
            if let Ok(built) = results.try_recv() {
                link.built(built);
            } else {
                assert!(std::time::Instant::now() < deadline, "the pool answers");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    /// Everything the link has to send, decoded, in the order it leaves:
    /// a chunk's bodies, right after its frame, folded into it.
    fn drain(link: &mut Link) -> Vec<Out> {
        let mut out = Vec::new();
        while link.has_output() {
            match link.next_output().expect("encodable") {
                Some(Message::Text(text)) => match decode(text.as_str()).expect("the protocol") {
                    ServerMessage::ChunkBodies { x, y, bodies } => {
                        let Some(Out::Chunk(chunk)) = out.last_mut() else {
                            panic!("bodies after anything but their chunk");
                        };
                        assert_eq!((chunk.x, chunk.y), (x, y), "the bodies of their chunk");
                        assert!(chunk.bodies.is_empty() && !bodies.is_empty());
                        chunk.bodies = bodies;
                    }
                    message => out.push(Out::Message(message)),
                },
                Some(Message::Binary(frame)) => out.push(Out::Chunk(Sent::of(
                    &link.shared,
                    ChunkSend {
                        frame,
                        bodies: vec![],
                        version: 0,
                    },
                ))),
                _ => {}
            }
        }
        out
    }

    #[test]
    fn a_chunk_built_on_the_pool_sends_the_bytes_of_one_built_in_line() {
        let shared = Arc::new(shared(1));
        let pool = ChunkPool::new(3);
        let home = shared.position_of(1).expect("body 1").chunk();
        // The body's own chunk among them: a frame carries the bodies.
        let chunks: Vec<ChunkPos> = std::iter::once(home)
            .chain(View::window(&shared.map, home).into_iter().take(29))
            .collect();
        let (tx, rx) = std::sync::mpsc::channel();
        let lane = pool.lane();
        for &chunk in &chunks {
            let (shared, tx) = (Arc::clone(&shared), tx.clone());
            pool.submit(
                lane,
                Box::new(move || {
                    tx.send(shared.chunk_ground(chunk).expect("on the map"))
                        .unwrap();
                }),
            );
        }
        let mut with_bodies = 0;
        for _ in &chunks {
            let ground = rx.recv_timeout(Duration::from_secs(60)).expect("built");
            let chunk = ground.chunk;
            let pooled = Sent::of(&shared, shared.chunk_send_of(ground).unwrap());
            let inline = sent(&shared, chunk).expect("on the map");
            assert_eq!(pooled, inline, "{chunk:?}");
            with_bodies += usize::from(!pooled.bodies.is_empty());
        }
        assert!(with_bodies >= 1, "the bodies are added as the frame leaves");
    }

    #[test]
    fn the_far_view_is_counted_on_the_pool_held_and_only_the_latest_sent() {
        let shared = Arc::new(shared(1));
        // One thread: the jobs run in the order given.
        let pool = Arc::new(ChunkPool::new(1));
        let (mut link, mut results) = link(&shared, &pool, Some(1));
        let home = link.home();
        let away = ChunkPos::new(home.x + FAR_RESEND + 1, home.y);
        // Asked for, nothing waits in the ground lane: the network's
        // thread counted nothing.
        link.far_view(home);
        assert!(link.ground.is_empty() && link.far_awaited == Some(home));
        // A newer one asked before the first is back: the first is
        // dropped when it comes.
        link.far_view(away);
        settle(&mut link, &mut results);
        let far: Vec<ServerMessage> = drain(&mut link)
            .into_iter()
            .filter_map(|out| match out {
                Out::Message(m @ ServerMessage::FarView { .. }) => Some(m),
                _ => None,
            })
            .collect();
        assert_eq!(
            far,
            vec![far_view(&shared.map, away)],
            "the latest only, as counted in line"
        );
        // Both are held: a client centred there is sent the same bytes,
        // counted once.
        let held = |c: ChunkPos| {
            shared
                .far_views
                .lock()
                .expect("far views not poisoned")
                .contains(&c)
        };
        assert!(held(home) && held(away));
        let first = shared.far_view_message(away).expect("encodable");
        let again = shared.far_view_message(away).expect("encodable");
        assert_eq!(first, again);
        assert_eq!(
            decode::<ServerMessage>(first.to_text().expect("text")).expect("the protocol"),
            far_view(&shared.map, away)
        );
    }

    #[test]
    fn a_pilot_whose_body_died_pilots_a_body_born_for_it_at_the_camp() {
        // Body 1 alone in the valley, piloted: when it dies, no free body
        // is left. (Its death is the network's here: the world keeps it,
        // so the new body is born on the next free grass.)
        let content = Content::sample();
        let mut world = World::with_valley(Calendar::default(), &content, 768);
        world.spawn_at_camp().expect("grass by the camp");
        let shared = Arc::new(Shared::new(&world, &content, 1, 1.0));
        let pool = Arc::new(ChunkPool::new(1));
        let (tx, _private) = mpsc::channel(PRIVATE_CAPACITY);
        assert_eq!(shared.take_body(tx), Some(1));
        let (mut link, mut results, mut born) = link_born(&shared, &pool, Some(1));
        shared.body_died(BodyId(1), DeathCause::Starved);

        link.reassign();
        assert_eq!(link.pilot.body, None, "it waits for its birth");
        assert!(shared.piloted().is_empty(), "the dead body is let go");
        assert!(born.try_recv().is_err(), "nothing born before the tick");

        // Before the next tick, the simulation thread bears it at the
        // camp: piloted at once, standing where the world has it.
        shared.bear_bodies(&mut world, &content);
        assert_eq!(born.try_recv(), Ok(2));
        assert_eq!(shared.piloted(), vec![BodyId(2)]);
        let at = world.body(BodyId(2)).expect("born").pos();
        assert_eq!(shared.position_of(2), Some(at));
        let camp = world.map().camp();
        assert!((at.x - camp.x).abs().max((at.y - camp.y).abs()) <= 3);
        assert!(shared.take_births().is_empty(), "born once");

        link.born(2);
        assert_eq!(link.pilot.body, Some(2));
        assert!(matches!(
            drain(&mut link).first(),
            Some(Out::Message(ServerMessage::BodyAssigned { body: Some(2) }))
        ));
        assert_eq!(link.home(), at.chunk(), "the view moves to it");
        settle(&mut link, &mut results);
    }

    #[test]
    fn events_leave_before_the_ground_and_a_hidden_chunk_never_leaves() {
        let shared = Arc::new(shared(1));
        // One thread: the chunks come back in the order asked for.
        let pool = Arc::new(ChunkPool::new(1));
        let (mut link, mut results) = link(&shared, &pool, Some(1));
        let home = link.home();
        link.move_view(home);
        link.far_view(home);
        link.event(&ServerMessage::Pong { nonce: 7 });
        let total = link.view.len();
        assert_eq!(link.awaiting.len(), total);
        settle(&mut link, &mut results);

        // The event first, then the chunks, the body's own first, then
        // the far view, counted on the pool after the chunks asked before
        // it.
        let out = drain(&mut link);
        assert_eq!(out.len(), total + 3);
        assert!(matches!(out[0], Out::Message(ServerMessage::View { .. })));
        assert!(matches!(
            out[1],
            Out::Message(ServerMessage::Pong { nonce: 7 })
        ));
        assert!(matches!(
            &out[2],
            Out::Chunk(Sent { x, y, bodies, .. })
                if ChunkPos::new(*x, *y) == home && bodies.iter().any(|b| b.id == 1)
        ));
        assert!(matches!(
            out.last(),
            Some(Out::Message(ServerMessage::FarView { .. }))
        ));
        assert_eq!(link.sent.len(), total);
        assert!(link.known.contains(&1));

        // The view moves: one message tells the window, what the client
        // holds and is out now goes to its cache (and to the mirror of
        // it), nothing out of view is built or sent, each chunk comes once.
        let away = ChunkPos::new(home.x + VIEW_RADIUS + 3, home.y);
        let known = link.known.len();
        link.move_view(away);
        // The bodies known, out of range now, are forgotten.
        let out = drain(&mut link);
        assert!(matches!(
            out.as_slice(),
            [Out::Message(ServerMessage::View { x, .. }), rest @ ..]
                if *x > home.x - VIEW_RADIUS
                    && rest.len() == known
                    && rest.iter().all(|m| matches!(m, Out::Message(ServerMessage::BodyLeft { .. })))
        ));
        assert!(link.known.is_empty());
        let hidden = total - link.sent.len();
        assert!(hidden > 0);
        assert_eq!(link.cache.len(), hidden);
        // A chunk the pool finishes after it left the view is dropped.
        let gone = shared.chunk_ground(home).expect("on the map");
        link.ground_built(gone);
        assert!(!link.has_output());
        settle(&mut link, &mut results);
        let out = drain(&mut link);
        assert_eq!(
            out.len(),
            link.view.len() - (total - hidden),
            "the new columns only"
        );
        assert!(out.iter().all(|m| matches!(
            m,
            Out::Chunk(Sent { x, .. }) if *x > home.x + VIEW_RADIUS
        )));
        assert!(link.sent.keys().eq(link.view.chunks.iter()));
    }

    #[test]
    fn a_link_that_fell_behind_tells_its_view_again() {
        let shared = Arc::new(shared(1));
        let pool = Arc::new(ChunkPool::new(2));
        let (mut link, mut results) = link(&shared, &pool, Some(1));
        let home = link.home();
        link.move_view(home);
        settle(&mut link, &mut results);
        drain(&mut link);
        let known_before = link.known.clone();
        assert!(known_before.len() >= 2, "the camp's bodies stand in view");

        // Meanwhile, unseen: another body died and an item was dropped
        // where body 1 stands.
        let other = *known_before
            .iter()
            .find(|&&b| b != 1)
            .expect("another body");
        shared.body_died(BodyId(other), DeathCause::Starved);
        let at = shared.position_of(1).expect("body 1");
        shared.item_added(
            ItemState {
                id: 999,
                x: at.x,
                y: at.y,
                material: "corpse".into(),
                mass_g: 60_000,
            },
            shared.chunk_version(at.chunk()) + 1,
        );
        link.resync(5);
        let out = drain(&mut link);
        assert!(matches!(
            out[0],
            Out::Message(ServerMessage::DayStarted { .. })
        ));
        assert!(out.iter().any(
            |m| matches!(m, Out::Message(ServerMessage::BodyLeft { body }) if *body == other)
        ));
        for body in known_before.iter().filter(|&&b| b != other) {
            assert!(out.iter().any(|m| matches!(
                m,
                Out::Message(ServerMessage::BodyStopped { body: b, .. } | ServerMessage::BodyWalks { body: b, .. })
                    if b == body
            )));
            assert!(
                out.iter()
                    .any(|m| matches!(m, Out::Message(ServerMessage::BodySleeps { body: b, .. }) if b == body))
            );
            assert!(out.iter().any(
                |m| matches!(m, Out::Message(ServerMessage::BodyHolds { body: b, .. }) if b == body)
            ));
        }
        assert!(
            !out.iter().any(|m| matches!(m, Out::Chunk(_))),
            "the ground waits for the pool"
        );
        assert!(!link.known.contains(&other));
        // Only the chunk whose items changed is sent again, with them.
        settle(&mut link, &mut results);
        let out = drain(&mut link);
        assert_eq!(out.len(), 1);
        assert!(matches!(
            &out[0],
            Out::Chunk(Sent { x, y, items, .. })
                if ChunkPos::new(*x, *y) == home && items.iter().any(|i| i.id == 999)
        ));
        assert_eq!(link.sent[&home].known, shared.chunk_version(home));

        // Nothing changed: a second resync sends no ground again.
        link.resync(1);
        drain(&mut link);
        assert!(link.awaiting.is_empty());

        // Its own body died meanwhile: told with the cause, then another
        // body is taken and the view moves there.
        shared.body_died(BodyId(1), DeathCause::Froze);
        link.resync(2);
        let out = drain(&mut link);
        assert!(out.iter().any(|m| matches!(
            m,
            Out::Message(ServerMessage::BodyDied {
                body: 1,
                cause: DeathCause::Froze
            })
        )));
        assert!(out.iter().any(
            |m| matches!(m, Out::Message(ServerMessage::BodyAssigned { body: Some(b) }) if *b != 1)
        ));
        assert_ne!(link.pilot.body, Some(1));
        settle(&mut link, &mut results);
    }

    /// What the server spends building the chunk messages of a view on
    /// the game's valley (D39): a measure, not a rule. Run by hand:
    /// `cargo test --release -p server profile_chunk_messages -- --ignored --nocapture`.
    ///
    /// A login's view around the camp built cold (a fresh server) and
    /// again warm (the same server, every cache full), then a teleport's
    /// view cold and warm; and, for the login view, the trees of a chunk
    /// alone and the trees of the nine chunks around it, as the plants
    /// read them.
    #[test]
    #[ignore = "a measure of the game's 3072 valley, run by hand in release"]
    fn profile_chunk_messages() {
        use std::time::Instant;

        fn report(what: &str, mut each: Vec<Duration>) {
            each.sort_unstable();
            let total: Duration = each.iter().sum();
            let count = u32::try_from(each.len()).expect("a view");
            println!(
                "{what}: {count} chunks in {total:?}; a chunk mean {:?}, median {:?}, max {:?}",
                total / count,
                each[each.len() / 2],
                each[each.len() - 1]
            );
        }

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = crate::content::load(&dir).expect("repository content");
        let started = Instant::now();
        let world = World::new(Calendar::default(), &content);
        println!("the valley made in {:?}", started.elapsed());
        let shared = Shared::new(&world, &content, 1, 1.0);
        let camp = shared.map.camp().chunk();
        let login = View::window(&shared.map, camp);
        let away = View::window(&shared.map, ChunkPos::new(30, 40));
        let time_each = |chunks: &BTreeSet<ChunkPos>| -> Vec<Duration> {
            chunks
                .iter()
                .map(|&chunk| {
                    let started = Instant::now();
                    let message = shared.chunk_message(chunk).expect("a chunk of the map");
                    let took = started.elapsed();
                    std::hint::black_box(message);
                    took
                })
                .collect()
        };
        report("login view, cold", time_each(&login));
        report("login view, warm", time_each(&login));
        report("teleport view, cold", time_each(&away));
        report("teleport view, warm", time_each(&away));

        let alone: Vec<Duration> = login
            .iter()
            .map(|&chunk| {
                let started = Instant::now();
                std::hint::black_box(shared.trees_in(chunk));
                started.elapsed()
            })
            .collect();
        report(
            "trees of the chunk itself (as the message holds them)",
            alone,
        );
        let around: Vec<Duration> = login
            .iter()
            .map(|&chunk| {
                let started = Instant::now();
                let trees = shared.trees_around(chunk);
                let took = started.elapsed();
                std::hint::black_box(trees);
                took
            })
            .collect();
        report(
            "trees of the nine chunks around (as the plants read them)",
            around,
        );
    }
}
