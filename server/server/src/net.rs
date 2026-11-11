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

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use protocol::{
    BodyState, ClientMessage, Clock, Condition, DeathCause, FloraView, ItemAct, ItemState,
    ItemView, MaterialView, PROTOCOL_VERSION, PlantState, Point, Refusal, ServerMessage,
    SpeciesView, Terrain, TreeState, decode, encode,
};
use sim::ground::Ground;
use sim::lru::Lru;
use sim::{BodyId, CHUNK_SIZE, ChunkPos, Content, FinePos, Intention, ItemId, Map, Pos, World};

use crate::pool::ChunkPool;
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

/// How many published messages a connection may fall behind before it
/// misses some and is told its view again ([`Link::resync`]). A
/// connection falls behind only when its task is starved of the network
/// threads: it never waits on the wire nor builds chunks itself since
/// the chunk pool (11 November 2026); it was 256.
const BROADCAST_CAPACITY: usize = 1024;

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
        /// The version of its chunk's items after it.
        version: u64,
    },
    /// An item vanished from the ground.
    ItemRemoved {
        /// Which.
        item: u64,
        /// Where it was.
        at: Pos,
        /// The version of its chunk's items after it.
        version: u64,
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

fn point(at: FinePos) -> Point {
    Point { x: at.x, y: at.y }
}

/// State shared between the simulation thread and all connections.
#[derive(Debug)]
pub struct Shared {
    /// The world's current tick, updated by the simulation thread.
    pub tick: AtomicU64,
    /// What the simulation thread publishes to every connection.
    pub publish: broadcast::Sender<Published>,
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
    flora_wire: Vec<Option<u16>>,
    /// The seed the trees and the flora are placed with (the map's).
    seed: u64,
    /// The plants of the chunks asked for lately, at most
    /// [`PLANT_CACHE_CHUNKS`] chunks (the cost follows the chunks in use,
    /// never the map's size nor the land visited, D39). Shared, so a hit
    /// holds the lock only to find them, never to copy them. Only what the
    /// seed gives may live here: a change to a plant (one picked, later)
    /// must be kept apart and laid over these, or forgetting a chunk would
    /// undo it.
    plants: Mutex<Lru<ChunkPos, Arc<[PlantState]>>>,
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
    /// The stored items on the ground, by chunk, and each chunk's items
    /// version, kept by the simulation thread. The ground's natural ones
    /// are not here: they are made again each time a chunk is sent
    /// ([`sim::items`]), so this holds what was dropped, never the land
    /// visited (D39).
    items: Mutex<Items>,
    /// What the ground's natural items are made from: whether this world
    /// has them, the spawn rules, the seed.
    natural: Option<(Vec<sim::content::Spawn>, u64)>,
    /// The natural items taken off the ground: left out when their chunk
    /// is sent. It grows with what bodies pick up, never with the land.
    natural_taken: Mutex<std::collections::BTreeSet<ItemId>>,
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

/// The items dropped on the ground, by chunk, and how many times each
/// chunk's items changed: a connection sent a chunk at a version, then
/// each item event, knows its client holds that chunk's items right; one
/// that fell behind compares and sends again what differs.
#[derive(Debug, Default)]
struct Items {
    stored: BTreeMap<ChunkPos, Vec<ItemState>>,
    /// Absent means 0. Grows with the chunks where something was dropped
    /// or taken, never with the land visited.
    versions: BTreeMap<ChunkPos, u64>,
}

impl Items {
    /// Counts a change to `chunk`'s items; the new version.
    fn changed(&mut self, chunk: ChunkPos) -> u64 {
        let version = self.versions.entry(chunk).or_insert(0);
        *version += 1;
        *version
    }
}

/// What the map and the seed give a chunk, built on the chunk pool: the
/// ground, the trees, the plants, the currents, the natural items. What
/// changes on it (the bodies standing there, the items taken or dropped)
/// is added as the frame leaves ([`Shared::chunk_message_of`]), so that
/// it is as fresh as the events around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkGround {
    /// Which chunk.
    pub chunk: ChunkPos,
    tiles: Vec<Terrain>,
    trees: Vec<TreeState>,
    plants: Vec<PlantState>,
    currents: Vec<u16>,
    /// The ground's natural items, none taken out yet.
    natural: Vec<ItemState>,
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
    pub fn new(world: &World, content: &Content, max_connections: usize, time_scale: f64) -> Self {
        let (publish, _) = broadcast::channel(BROADCAST_CAPACITY);
        let materials = content
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
        let mut items = Items::default();
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
                });
        }
        let mut bodies = Bodies::default();
        for b in world.bodies() {
            bodies.insert(BodyState {
                id: b.id().0,
                at: point(b.at()),
                route: b.route().map(point).collect(),
                speed_cm_per_s: content.movement.walk_speed_cm_per_s,
                asleep: b.condition().asleep,
                holds: b.held().and_then(|id| world.item(id)).map(|i| ItemView {
                    id: i.id().0,
                    material: i.material().to_owned(),
                }),
            });
        }
        Self {
            tick: AtomicU64::new(world.tick().0),
            publish,
            map: world.shared_map(),
            bodies: Mutex::new(bodies),
            materials,
            species,
            tree_kinds: content.species.clone(),
            ground: world.shared_ground(),
            flora,
            flora_kinds: content.flora.clone(),
            flora_wire,
            seed: world.seed(),
            plants: Mutex::new(Lru::new(PLANT_CACHE_CHUNKS)),
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
            natural_taken: Mutex::new(world.taken().clone()),
            max_connections,
            connections: AtomicUsize::new(0),
            intentions: Mutex::new(BTreeMap::new()),
            pilots: Mutex::new(BTreeMap::new()),
            dev_overview: None,
        }
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

    /// Records that a body set off: where it is and the route ahead.
    pub fn body_walks(&self, body: BodyId, from: FinePos, route: &[FinePos], speed_cm_per_s: u32) {
        let mut bodies = self.bodies();
        if bodies.place(body.0, point(from)).is_some()
            && let Some(state) = bodies.get_mut(body.0)
        {
            state.route = route.iter().copied().map(point).collect();
            state.speed_cm_per_s = speed_cm_per_s;
        }
    }

    /// The bodies, locked: taken for a moment by the simulation thread
    /// at each event, and by a connection reading one body or a chunk's.
    fn bodies(&self) -> std::sync::MutexGuard<'_, Bodies> {
        self.bodies.lock().expect("bodies not poisoned")
    }

    /// Records a body's position as it crosses into another tile, keeping
    /// the last `remaining` waypoints of its route. Returns the tile it
    /// came from, or `None` for a body nobody knew about.
    pub fn body_moved(&self, body: BodyId, at: FinePos, remaining: usize) -> Option<Pos> {
        let mut bodies = self.bodies();
        let from = bodies.place(body.0, point(at))?;
        if let Some(state) = bodies.get_mut(body.0) {
            let passed = state.route.len().saturating_sub(remaining);
            state.route.drain(..passed);
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

    /// Records an item lying on the ground; the new version of its
    /// chunk's items.
    pub fn item_added(&self, item: ItemState) -> u64 {
        let chunk = Pos::new(item.x, item.y).chunk();
        let mut items = self.items.lock().expect("items not poisoned");
        items.stored.entry(chunk).or_default().push(item);
        items.changed(chunk)
    }

    /// How many times `chunk`'s items changed.
    pub fn items_version(&self, chunk: ChunkPos) -> u64 {
        self.items
            .lock()
            .expect("items not poisoned")
            .versions
            .get(&chunk)
            .copied()
            .unwrap_or(0)
    }

    /// Forgets an item that was on the ground; the new version of its
    /// chunk's items.
    pub fn item_removed(&self, item: ItemId, at: Pos) -> u64 {
        // Held while the item goes, as `ground_items` holds it while it
        // gathers a chunk's: a chunk sent at the same moment sees the item
        // either there or gone, never twice.
        let mut taken = self
            .natural_taken
            .lock()
            .expect("natural items not poisoned");
        if sim::items::is_natural(item) {
            taken.insert(item);
        }
        let mut items = self.items.lock().expect("items not poisoned");
        if let Some(stored) = items.stored.get_mut(&at.chunk()) {
            stored.retain(|i| i.id != item.0);
        }
        items.changed(at.chunk())
    }

    /// Records that a body stopped exactly there.
    pub fn body_stopped(&self, body: BodyId, at: FinePos) {
        let mut bodies = self.bodies();
        if bodies.place(body.0, point(at)).is_some()
            && let Some(state) = bodies.get_mut(body.0)
        {
            state.route.clear();
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

    /// The message carrying a chunk and the bodies in it: its ground as
    /// the pool builds it, finished now.
    pub fn chunk_message(&self, chunk: ChunkPos) -> Option<ServerMessage> {
        self.chunk_ground(chunk)
            .map(|ground| self.chunk_message_of(ground).0)
    }

    /// What the map and the seed give `chunk`, as the pool builds it:
    /// no lock the simulation thread takes is held while the plants are
    /// placed (D29). `None` off the map.
    pub fn chunk_ground(&self, chunk: ChunkPos) -> Option<ChunkGround> {
        let tiles = self.map.chunk(chunk)?;
        Some(ChunkGround {
            chunk,
            tiles: tiles.into_iter().map(terrain_view).collect(),
            trees: self.trees_in(chunk),
            plants: self.plants_in(chunk),
            currents: self.map.chunk_currents(chunk)?,
            natural: self.natural_items(chunk),
        })
    }

    /// The message carrying a chunk's ground and what stands and lies on
    /// it now, with the version of its items it carries.
    pub fn chunk_message_of(&self, ground: ChunkGround) -> (ServerMessage, u64) {
        let ChunkGround {
            chunk,
            tiles,
            trees,
            plants,
            currents,
            natural,
        } = ground;
        // Gathered first, in a statement of its own: a lock taken inside
        // the message's literal would stay held to the literal's end.
        let (items, version) = self.ground_items(chunk, natural);
        let message = ServerMessage::Chunk {
            x: chunk.x,
            y: chunk.y,
            tiles,
            bodies: self.bodies_in(chunk),
            items,
            trees,
            plants,
            currents,
        };
        (message, version)
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
                })
            })
            .collect()
    }

    /// The items lying in `chunk`: its `natural` ones, those taken
    /// excepted, then the stored ones; and the version of the chunk's
    /// items.
    fn ground_items(&self, chunk: ChunkPos, natural: Vec<ItemState>) -> (Vec<ItemState>, u64) {
        // Filtered and gathered under the lock `item_removed` holds, so an
        // item taken meanwhile shows either here or nowhere.
        let taken = self
            .natural_taken
            .lock()
            .expect("natural items not poisoned");
        let mut items: Vec<ItemState> = natural
            .into_iter()
            .filter(|i| !taken.contains(&ItemId(i.id)))
            .collect();
        let version = {
            let stored = self.items.lock().expect("items not poisoned");
            if let Some(stored) = stored.stored.get(&chunk) {
                items.extend(stored.iter().cloned());
            }
            stored.versions.get(&chunk).copied().unwrap_or(0)
        };
        drop(taken);
        // In the order of their ids, as the simulation lists them.
        items.sort_by_key(|i| i.id);
        (items, version)
    }

    /// The trees standing in `chunk`, placed from the map and the seed
    /// the first time they are asked for ([`Ground::trees_in`]).
    fn trees_in(&self, chunk: ChunkPos) -> Vec<TreeState> {
        self.ground
            .trees_in(&self.map, &self.tree_kinds, self.seed, chunk)
            .iter()
            .map(|tree| TreeState {
                at: point(tree.at),
                species: self.species[tree.species].id.clone(),
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
    fn plants_in(&self, chunk: ChunkPos) -> Vec<PlantState> {
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
        let plants: Vec<PlantState> =
            sim::flora::plants_in(&self.map, &self.flora_kinds, &near, self.seed, chunk)
                .into_iter()
                .filter_map(|p| {
                    let kind = self
                        .flora_wire
                        .get(usize::from(p.kind))
                        .copied()
                        .flatten()?;
                    Some(PlantState {
                        kind,
                        x: p.at.x,
                        y: p.at.y,
                    })
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
fn flora_views(content: &Content) -> (Vec<FloraView>, Vec<Option<u16>>) {
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
            Some(u16::try_from(flora.len() - 1).expect("few kinds"))
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
}

impl Drop for Pilot {
    fn drop(&mut self) {
        if let Some(body) = self.body {
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

    let pilot = Pilot {
        shared: Arc::clone(shared),
        body: None,
    };
    let Some(pilot) = handshake(&mut ws, pilot, private_tx.clone()).await? else {
        ws.close(None).await?;
        return Ok(());
    };
    match pilot.body {
        Some(body) => println!("connection {peer}: pilots body {body}"),
        None => println!("connection {peer}: watching, no free body"),
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
    });
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
                    Some(Ok(Message::Text(text))) => match decode::<ClientMessage>(text.as_str()) {
                        Ok(ClientMessage::Ping { nonce }) => {
                            link.event(&ServerMessage::Pong { nonce });
                        }
                        Ok(ClientMessage::Hello { .. }) => {
                            // A second hello is a client bug; drop the connection.
                            return Ok(());
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
                message = published.recv() => match message {
                    Ok(published) => {
                        if link.forward(published) {
                            link.reassign();
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        // What the channel still holds is forwarded first,
                        // late but whole; then the view is told again, newer
                        // than everything sent before it.
                        let mut skipped = skipped;
                        loop {
                            use broadcast::error::TryRecvError;
                            match published.try_recv() {
                                Ok(published) => {
                                    if link.forward(published) {
                                        link.reassign();
                                    }
                                }
                                Err(TryRecvError::Lagged(more)) => skipped += more,
                                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
                            }
                        }
                        link.resync(skipped);
                    }
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                },
                // The sender lives in the pilot set as long as this connection
                // pilots a body, so the channel never closes before the loop ends.
                Some(message) = private.recv() => link.event(&message),
                Some(ground) = results.recv() => link.ground_built(ground),
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
    /// A message as is (the far view).
    Frame(Message),
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
    pilot: Pilot,
    /// The channel for messages meant for this pilot alone.
    private: mpsc::Sender<ServerMessage>,
    /// The chunks held (asked for or sent), and where the far view was
    /// last centred.
    view: View,
    /// The chunks held, as the pool's jobs read them: a job for a chunk
    /// no longer here does nothing.
    wanted: Arc<Mutex<BTreeSet<ChunkPos>>>,
    /// The chunks in view whose frame has not left yet.
    awaiting: BTreeSet<ChunkPos>,
    /// The chunks whose frame left: what the client holds.
    sent: BTreeSet<ChunkPos>,
    /// For each chunk sent, the version of its items the client knows
    /// (that of the frame, then of each item event forwarded).
    item_versions: BTreeMap<ChunkPos, u64>,
    /// The bodies the client was told of and not told to forget.
    known: BTreeSet<u32>,
    /// The events lane, encoded.
    events: VecDeque<Message>,
    /// The ground lane.
    ground: VecDeque<Outgoing>,
    /// Where the pool's jobs send what they built.
    results: mpsc::UnboundedSender<ChunkGround>,
}

impl Drop for Link {
    fn drop(&mut self) {
        // The jobs still waiting in the lane find nothing wanted and
        // build nothing for a connection that is gone.
        self.wanted.lock().expect("wanted not poisoned").clear();
    }
}

impl Link {
    fn new(
        shared: Arc<Shared>,
        pool: Arc<ChunkPool>,
        peer: SocketAddr,
        pilot: Pilot,
        private: mpsc::Sender<ServerMessage>,
        results: mpsc::UnboundedSender<ChunkGround>,
    ) -> Self {
        let lane = pool.lane();
        Self {
            shared,
            pool,
            peer,
            lane,
            pilot,
            private,
            view: View::default(),
            wanted: Arc::default(),
            awaiting: BTreeSet::new(),
            sent: BTreeSet::new(),
            item_versions: BTreeMap::new(),
            known: BTreeSet::new(),
            events: VecDeque::new(),
            ground: VecDeque::new(),
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
            Some(Outgoing::Chunk(ground)) => {
                let chunk = ground.chunk;
                let (message, version) = self.shared.chunk_message_of(ground);
                if let ServerMessage::Chunk { bodies, .. } = &message {
                    self.known.extend(bodies.iter().map(|b| b.id));
                }
                self.sent.insert(chunk);
                self.item_versions.insert(chunk, version);
                Ok(Some(Message::text(encode(&message)?)))
            }
        }
    }

    /// A chunk the pool built: into the ground lane if still in view and
    /// not sent meanwhile, else dropped.
    fn ground_built(&mut self, ground: ChunkGround) {
        if self.view.chunks.contains(&ground.chunk) && self.awaiting.remove(&ground.chunk) {
            self.ground.push_back(Outgoing::Chunk(ground));
        }
    }

    /// Asks the pool for `chunk`'s ground, nearest the body first in the
    /// order called.
    fn ask(&mut self, chunk: ChunkPos) {
        if !self.awaiting.insert(chunk) {
            return; // already asked for, not back yet
        }
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
                    let _ = results.send(ground);
                }
            }),
        );
    }

    /// Moves the view to `centre`, telling the client: chunks no longer
    /// held hidden (those it has), new ones asked of the pool, nearest
    /// the centre first.
    fn move_view(&mut self, centre: ChunkPos) {
        let (mut shown, hidden) = self.view.recentre(&self.shared.map, centre);
        self.wanted
            .lock()
            .expect("wanted not poisoned")
            .clone_from(&self.view.chunks);
        shown.sort_by_key(|c| ((c.x - centre.x).pow(2) + (c.y - centre.y).pow(2), c.y, c.x));
        for chunk in hidden {
            self.awaiting.remove(&chunk);
            self.ground
                .retain(|out| !matches!(out, Outgoing::Chunk(g) if g.chunk == chunk));
            // Only what the client holds is hidden; a chunk never sent is
            // only forgotten here.
            if self.sent.remove(&chunk) {
                self.item_versions.remove(&chunk);
                self.event(&ServerMessage::ChunkHidden {
                    x: chunk.x,
                    y: chunk.y,
                });
            }
        }
        for chunk in shown {
            self.ask(chunk);
        }
    }

    /// Sends the far view around `centre`, in the ground lane.
    fn far_view(&mut self, centre: ChunkPos) {
        match encode(&far_view(&self.shared.map, centre)) {
            Ok(text) => self.ground.push_back(Outgoing::Frame(Message::text(text))),
            Err(e) => eprintln!(
                "connection {}: the far view could not be encoded: {e}",
                self.peer
            ),
        }
        self.view.far = Some(centre);
    }

    /// Provisional (D34): a pilot whose body died takes the first free
    /// body, or watches, and the view moves there.
    fn reassign(&mut self) {
        if let Some(dead) = self.pilot.body.take() {
            self.shared.release_body(dead);
        }
        self.pilot.body = self.shared.take_body(self.private.clone());
        match self.pilot.body {
            Some(body) => println!("connection {}: now pilots body {body}", self.peer),
            None => println!("connection {}: no free body, watching", self.peer),
        }
        self.event(&ServerMessage::BodyAssigned {
            body: self.pilot.body,
        });
        let centre = self.home();
        self.move_view(centre);
    }

    /// Forwards what the simulation published, as far as this
    /// connection's view is concerned, moving the view along with the
    /// piloted body. Returns true when the piloted body just died.
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
                if self.view.contains(at) {
                    self.event(&ServerMessage::BodySleeps { body, asleep });
                }
            }
            Published::BodyDied { body, at, cause } => {
                if self.view.contains(at) || Some(body) == piloted {
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
                if self.view.contains(at) {
                    self.event(&ServerMessage::BodyHolds { body, item });
                }
            }
            Published::BodyWalks { body, from, tick } => {
                if self.view.contains(from) {
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
                let (was_seen, is_seen) = (self.view.contains(from), self.view.contains(to));
                if is_seen && !was_seen {
                    // Walked into view: the client learns of it with its route.
                    self.tell_walk(body, tick);
                } else if was_seen && !is_seen {
                    self.known.remove(&body);
                    self.event(&ServerMessage::BodyLeft { body });
                } else if is_seen
                    && self.sent.contains(&to.chunk())
                    && !self.sent.contains(&from.chunk())
                {
                    // Stepped from a chunk whose frame is still to leave
                    // into one the client already holds: neither frame
                    // carries it (the first will be taken after the step,
                    // the second was taken before), so it is told here.
                    self.tell_walk(body, tick);
                }
            }
            Published::BodyStopped { body, at } => {
                if self.view.contains(at)
                    && let Some(state) = self.shared.body_state(body)
                {
                    self.known.insert(body);
                    self.event(&ServerMessage::BodyStopped { body, at: state.at });
                }
            }
        }
        false
    }

    /// Tells the client where `body` is and where it is going.
    fn tell_walk(&mut self, body: u32, tick: u64) {
        if let Some(state) = self.shared.body_state(body) {
            self.known.insert(body);
            self.event(&walks_message(&state, tick));
        }
    }

    /// Notes that the client is told `chunk`'s items at `version`.
    fn item_changed(&mut self, chunk: ChunkPos, version: u64) {
        if let Some(known) = self.item_versions.get_mut(&chunk) {
            *known = (*known).max(version);
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

        // The bodies in the chunks the client holds; those in chunks
        // still to leave come with their frame.
        let standing: BTreeMap<u32, BodyState> = self
            .sent
            .iter()
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
            .copied()
            .filter(|chunk| {
                shared.items_version(*chunk) != self.item_versions.get(chunk).copied().unwrap_or(0)
            })
            .collect();
        for chunk in stale {
            self.ask(chunk);
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
        ClientMessage::Hello { .. } | ClientMessage::Ping { .. } => {
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
        speed_cm_per_s: state.speed_cm_per_s,
        tick,
    }
}

/// Runs the hello/welcome exchange. Returns `Ok(None)` when the client
/// must be disconnected (wrong version, wrong first message, or silence).
async fn handshake(
    ws: &mut WebSocketStream<TcpStream>,
    mut pilot: Pilot,
    private: mpsc::Sender<ServerMessage>,
) -> Result<Option<Pilot>, BoxError> {
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

    fn shared(max_connections: usize) -> Shared {
        // A small valley: the rules are the same, and it is made in a
        // fraction of the time.
        let world = World::with_valley(Calendar::default(), &Content::sample(), 768);
        Shared::new(&world, &Content::sample(), max_connections, 1.0)
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
        let first: Vec<Vec<PlantState>> = chunks
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
            let fresh: Vec<TreeState> =
                sim::trees::trees_in(&shared.map, &shared.tree_kinds, shared.seed, chunk)
                    .into_iter()
                    .map(|tree| TreeState {
                        at: point(tree.at),
                        species: shared.species[tree.species].id.clone(),
                    })
                    .collect();
            if misses.is_empty() {
                assert!(!shared.ground.holds_trees(chunk), "nothing asked for yet");
            }
            let miss = shared.chunk_message(chunk).expect("a chunk");
            assert!(shared.ground.holds_trees(chunk), "held once asked for");
            let hit = shared.chunk_message(chunk).expect("a chunk");
            let ServerMessage::Chunk { trees, .. } = &miss else {
                panic!("a chunk");
            };
            assert_eq!(trees, &fresh, "{chunk:?}: a miss places the trees");
            assert_eq!(
                encode(&miss).unwrap(),
                encode(&hit).unwrap(),
                "{chunk:?}: a hit sends the same bytes"
            );
            misses.push(miss);
        }
        assert!(
            misses
                .iter()
                .any(|m| matches!(m, ServerMessage::Chunk { trees, .. } if !trees.is_empty())),
            "trees grow around the camp"
        );
        // Each chunk's trees were placed once, for its own message and
        // for the plants of its neighbours alike: the twelve chunks and
        // their rim are held, nothing beyond.
        let rim: BTreeSet<ChunkPos> = chunks
            .iter()
            .flat_map(|c| {
                (-1..=1)
                    .flat_map(move |dy| (-1..=1).map(move |dx| ChunkPos::new(c.x + dx, c.y + dy)))
            })
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
            assert_eq!(&forgets.chunk_message(chunk).unwrap(), miss, "{chunk:?}");
            assert_eq!(forgets.ground.trees_held(), 1, "bounded");
        }
        // A change to a chunk (felling, later) bumps its version: placed
        // again, the same today, nothing laying changes over them yet.
        assert_eq!(shared.ground.changed(chunks[0]), 1);
        assert!(
            !shared.ground.holds_trees(chunks[0]),
            "the old version is stale"
        );
        assert_eq!(&shared.chunk_message(chunks[0]).unwrap(), &misses[0]);
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
        };
        shared.item_added(corpse.clone());
        let ServerMessage::Chunk { items, .. } = shared.chunk_message(here.chunk()).unwrap() else {
            panic!("expected a chunk");
        };
        assert!(items.contains(&corpse));
        shared.item_removed(ItemId(999), here);
        let ServerMessage::Chunk { items, .. } = shared.chunk_message(here.chunk()).unwrap() else {
            panic!("expected a chunk");
        };
        assert!(!items.contains(&corpse));
    }

    #[test]
    fn natural_items_are_made_per_chunk_and_never_held() {
        let world = World::with_valley(Calendar::default(), &Content::sample(), 768);
        let shared = Shared::new(&world, &Content::sample(), 1, 1.0);
        let camp = world.map().camp().chunk();
        let mut natural = None;
        for chunk in View::window(&shared.map, camp) {
            let Some(ServerMessage::Chunk { items, trees, .. }) = shared.chunk_message(chunk)
            else {
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
        shared.item_removed(ItemId(item.id), Pos::new(item.x, item.y));
        for _ in 0..2 {
            let Some(ServerMessage::Chunk { items, .. }) = shared.chunk_message(chunk) else {
                panic!("expected a chunk");
            };
            assert!(items.iter().all(|i| i.id != item.id));
        }
    }

    #[test]
    fn a_chunk_message_carries_the_bodies_standing_in_it() {
        let shared = shared(1);
        let body = shared.position_of(1).expect("body 1 exists");
        let message = shared
            .chunk_message(body.chunk())
            .expect("chunk on the map");
        let ServerMessage::Chunk { tiles, bodies, .. } = message else {
            panic!("expected a chunk");
        };
        assert_eq!(tiles.len(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
        assert!(bodies.iter().any(|b| b.id == 1));
        // The items of a chunk are those lying in it; the sample content
        // scatters berries on the grass at the forest's edge, somewhere in
        // the body's view.
        let mut seen_items = false;
        for chunk in View::window(&shared.map, body.chunk()) {
            let Some(ServerMessage::Chunk { items, .. }) = shared.chunk_message(chunk) else {
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
        let route = [Pos::new(12, 240).centre(), far.centre()];
        shared.body_walks(BodyId(1), body.centre(), &route, 143);
        assert_eq!(shared.body_state(1).unwrap().route.len(), 2);
        assert_eq!(shared.body_moved(BodyId(1), far.centre(), 0), Some(body));
        assert_eq!(shared.body_moved(BodyId(99), far.centre(), 0), None);
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
    ) -> (Link, mpsc::UnboundedReceiver<ChunkGround>) {
        let (private, _) = mpsc::channel(PRIVATE_CAPACITY);
        let (results, results_rx) = mpsc::unbounded_channel();
        let pilot = Pilot {
            shared: Arc::clone(shared),
            body,
        };
        let peer = "127.0.0.1:1".parse().expect("an address");
        let link = Link::new(
            Arc::clone(shared),
            Arc::clone(pool),
            peer,
            pilot,
            private,
            results,
        );
        (link, results_rx)
    }

    /// Hands the link every chunk the pool built for it until nothing
    /// is awaited any more.
    fn settle(link: &mut Link, results: &mut mpsc::UnboundedReceiver<ChunkGround>) {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while !link.awaiting.is_empty() {
            if let Ok(ground) = results.try_recv() {
                link.ground_built(ground);
            } else {
                assert!(std::time::Instant::now() < deadline, "the pool answers");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    /// Everything the link has to send, decoded, in the order it leaves.
    fn drain(link: &mut Link) -> Vec<ServerMessage> {
        let mut out = Vec::new();
        while link.has_output() {
            if let Some(Message::Text(text)) = link.next_output().expect("encodable") {
                out.push(decode(text.as_str()).expect("the protocol"));
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
            let (pooled, _) = shared.chunk_message_of(ground);
            let inline = shared.chunk_message(chunk).expect("on the map");
            assert_eq!(
                encode(&pooled).unwrap(),
                encode(&inline).unwrap(),
                "{chunk:?}"
            );
            if let ServerMessage::Chunk { bodies, .. } = &pooled {
                with_bodies += usize::from(!bodies.is_empty());
            }
        }
        assert!(with_bodies >= 1, "the bodies are added as the frame leaves");
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

        // The event first, then the far view (queued before any chunk
        // came back), then the chunks, the body's own first.
        let out = drain(&mut link);
        assert_eq!(out.len(), total + 2);
        assert!(matches!(out[0], ServerMessage::Pong { nonce: 7 }));
        assert!(matches!(out[1], ServerMessage::FarView { .. }));
        assert!(matches!(
            &out[2],
            ServerMessage::Chunk { x, y, bodies, .. }
                if ChunkPos::new(*x, *y) == home && bodies.iter().any(|b| b.id == 1)
        ));
        assert_eq!(link.sent.len(), total);
        assert!(link.known.contains(&1));

        // The view moves: what the client holds and is out now is hidden,
        // nothing out of view is built or sent, each chunk comes once.
        let away = ChunkPos::new(home.x + VIEW_RADIUS + 3, home.y);
        link.move_view(away);
        let out = drain(&mut link);
        let hidden = out.len();
        assert!(hidden > 0);
        assert!(out.iter().all(|m| matches!(
            m,
            ServerMessage::ChunkHidden { x, .. } if *x < away.x - VIEW_RADIUS
        )));
        assert_eq!(link.sent.len() + hidden, total);
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
            ServerMessage::Chunk { x, .. } if *x > home.x + VIEW_RADIUS
        )));
        assert_eq!(link.sent, link.view.chunks);
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
        shared.item_added(ItemState {
            id: 999,
            x: at.x,
            y: at.y,
            material: "corpse".into(),
        });
        link.resync(5);
        let out = drain(&mut link);
        assert!(matches!(out[0], ServerMessage::DayStarted { .. }));
        assert!(
            out.iter()
                .any(|m| matches!(m, ServerMessage::BodyLeft { body } if *body == other))
        );
        for body in known_before.iter().filter(|&&b| b != other) {
            assert!(out.iter().any(|m| matches!(
                m,
                ServerMessage::BodyStopped { body: b, .. } | ServerMessage::BodyWalks { body: b, .. }
                    if b == body
            )));
            assert!(
                out.iter()
                    .any(|m| matches!(m, ServerMessage::BodySleeps { body: b, .. } if b == body))
            );
            assert!(
                out.iter()
                    .any(|m| matches!(m, ServerMessage::BodyHolds { body: b, .. } if b == body))
            );
        }
        assert!(
            !out.iter().any(|m| matches!(m, ServerMessage::Chunk { .. })),
            "the ground waits for the pool"
        );
        assert!(!link.known.contains(&other));
        // Only the chunk whose items changed is sent again, with them.
        settle(&mut link, &mut results);
        let out = drain(&mut link);
        assert_eq!(out.len(), 1);
        assert!(matches!(
            &out[0],
            ServerMessage::Chunk { x, y, items, .. }
                if ChunkPos::new(*x, *y) == home && items.iter().any(|i| i.id == 999)
        ));
        assert_eq!(link.item_versions[&home], shared.items_version(home));

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
            ServerMessage::BodyDied {
                body: 1,
                cause: DeathCause::Froze
            }
        )));
        assert!(
            out.iter()
                .any(|m| matches!(m, ServerMessage::BodyAssigned { body: Some(b) } if *b != 1))
        );
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
