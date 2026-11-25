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
//!
//! The door, from the security audit of 20 November 2026 (D25, amended):
//! an upgrade carrying an `Origin` header is a browser page, never the
//! Godot client, and is refused; a connection is pinged at an interval
//! and dropped once silent for [`Limits::drop_silent_after`]; an address
//! holds at most [`Limits::connections_per_address`] places; what a
//! client's bad messages write to the log is escaped, bounded and capped
//! ([`Complaints`]).
//!
//! The leftovers of that audit (lot F): an address opens at most
//! [`Limits::sessions_per_address`] connections a minute, and its
//! refusals are logged once a minute (N8); a `chunk_missing` is honoured
//! once, in answer to a `chunk_same` or a `chunk_fruit`, and counted as a
//! bad message otherwise (N7); under `--dev` a connection teleports at
//! most once every [`TELEPORT_EVERY`] (N3); a message's unknown fields
//! are ignored as before (D26), and told once a connection (N10).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use protocol::{
    Blockage, BodyState, ChunkFrame, ClientMessage, Clock, Condition, DeathCause, FarView,
    FloraView, ItemAct, ItemState, ItemView, MaterialView, NaturalItem, PROTOCOL_VERSION, Point,
    Refusal, Season, ServerMessage, Side, SpeciesView, Standing, StandingTree, StoredItem, Terrain,
    Thought, decode, encode, encode_chunk, encode_far_view,
};
use sim::changes::Picked;
use sim::crops::{Crop, Source};
use sim::ground::{Ground, Version};
use sim::lru::Lru;
use sim::{
    BodyId, CHUNK_SIZE, Calendar, ChunkPos, Content, FinePos, Intention, ItemId, Map, Pos, Tick,
    World,
};

use crate::interest::{self, Inbox, Router};
use crate::pool::{Answer, ChunkPool};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::{Bytes, Message};

/// How long a client has to finish the WebSocket upgrade, and then how
/// long it has to send its `hello`.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest incoming message, in bytes. Client messages are a few dozen
/// bytes; anything near this limit is not our client.
pub const MAX_MESSAGE_BYTES: usize = 4096;

/// What bounds a connection's life, from the server's flags (the
/// security audit of 20 November 2026, finding N4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// A WebSocket ping is sent to every connection this often. The
    /// transport answers it on the client's side (Godot's included), so
    /// a player who leaves the game open stays; a peer that vanished
    /// does not. Twenty seconds: a handful of frames an hour, and a lost
    /// peer found within a minute.
    pub ping_every: Duration,
    /// A connection from which nothing came for this long, pings
    /// included, is dropped: sixty seconds, three pings missed, enough
    /// for a laptop waking up.
    pub drop_silent_after: Duration,
    /// Connections served at once from one address, at most. Eight: a
    /// household behind one address, or a few clients on a development
    /// machine; a hundred from one address is a flood.
    pub connections_per_address: usize,
    /// Connections one address may open within [`SESSIONS_WINDOW`], at
    /// most (finding N8): sixty, a connection a second on average, far
    /// above a client reconnecting and far below a connect-and-leave loop
    /// (170 a second measured), which costs a core and three log lines a
    /// session. Past it the address's connections are closed before the
    /// upgrade, at no cost but an accept.
    pub sessions_per_address: u32,
    /// The least time between two teleports one connection may ask for
    /// under `--dev` ([`TELEPORT_EVERY`] unless the command line says;
    /// zero: no limit, for the tests that hop about). Finding N3.
    pub teleport_every: Duration,
}

/// The window [`Limits::sessions_per_address`] is counted in, and how
/// long an address's refusals are logged once.
pub const SESSIONS_WINDOW: Duration = Duration::from_secs(60);

/// The least time between two teleports one connection may ask for under
/// `--dev` (finding N3). A teleport moves the whole view: 361 chunks to
/// build on the pool, about a processor-second cold, and the trunks of
/// the chunks around the target worked out on the simulation thread.
/// The simulation applies at most one a tick a body; one a second a
/// connection is ten times cheaper and more than a hand clicks the
/// overview. A teleport asked sooner is dropped: the client asks again.
pub const TELEPORT_EVERY: Duration = Duration::from_secs(1);

/// What lets an act through at most once every so often: the first at
/// once, the next once the interval has passed since the last let through.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Throttle {
    last: Option<Instant>,
}

impl Throttle {
    /// Whether an act at `now` goes through, `every` after the last one
    /// that did.
    pub fn allow(&mut self, now: Instant, every: Duration) -> bool {
        if self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) < every)
        {
            return false;
        }
        self.last = Some(now);
        true
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            ping_every: Duration::from_secs(20),
            drop_silent_after: Duration::from_secs(60),
            connections_per_address: 8,
            sessions_per_address: 60,
            teleport_every: TELEPORT_EVERY,
        }
    }
}

/// Log lines written for one connection's bad messages (undecodable, or
/// refused tools); the rest are counted silently. Ten says what is wrong;
/// a thousand fills the disk (finding N6).
pub const COMPLAINTS_LOGGED: u32 = 10;

/// Bad messages one connection may send in all; past that it is not our
/// client and is closed (finding N6).
pub const COMPLAINTS_MAX: u32 = 100;
const _: () = assert!(COMPLAINTS_LOGGED < COMPLAINTS_MAX);

/// Bad-message lines one address may put in the log within
/// [`SESSIONS_WINDOW`], over all its connections: three connections'
/// worth. A loop of connect, ten bad messages and close got every
/// connection's own ten lines and filled the log at 692 KiB/s (the
/// second security audit of 20 November 2026, A2); past this the
/// address's bad messages are counted, never written, until the window
/// turns.
pub const COMPLAINTS_LOGGED_PER_ADDRESS: u32 = 3 * COMPLAINTS_LOGGED;

/// Longest client-derived text a log line carries, in bytes (finding
/// N5): enough to show what was wrong, never a page.
pub const LOGGED_TEXT_MAX: usize = 200;

/// Client-derived text as it may go to the log: control characters and
/// quotes escaped, cut at [`LOGGED_TEXT_MAX`] bytes. A client's bytes
/// never reach the terminal as they are (finding N5): no title set, no
/// screen cleared, no fake line.
pub fn loggable(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if out.len() >= LOGGED_TEXT_MAX {
            out.push_str("...");
            break;
        }
        out.extend(c.escape_debug());
    }
    out
}

/// What one connection's bad messages cost the log: the first
/// [`COMPLAINTS_LOGGED`] are written, the rest counted, and past
/// [`COMPLAINTS_MAX`] the connection is closed (findings N5, N6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Complaints {
    count: u32,
    logged: u32,
    max: u32,
}

/// What to do with one more bad message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Complaint {
    /// Write it.
    Logged,
    /// Count it, write nothing.
    Silent,
    /// The cap is passed: close the connection.
    TooMany,
}

impl Default for Complaints {
    fn default() -> Self {
        Self::new(COMPLAINTS_LOGGED, COMPLAINTS_MAX)
    }
}

impl Complaints {
    /// A budget of `logged` lines and `max` bad messages in all.
    pub fn new(logged: u32, max: u32) -> Self {
        Self {
            count: 0,
            logged,
            max,
        }
    }

    /// Notes one more bad message.
    pub fn note(&mut self) -> Complaint {
        self.count = self.count.saturating_add(1);
        if self.count > self.max {
            Complaint::TooMany
        } else if self.count <= self.logged {
            Complaint::Logged
        } else {
            Complaint::Silent
        }
    }

    /// Bad messages noted so far.
    pub fn count(self) -> u32 {
        self.count
    }
}

/// The upgrade's door (D25, amended 20 November 2026, the lead's decision
/// Q2): a request carrying an `Origin` header comes from a browser page,
/// never from the Godot desktop client, and is refused with 403. Any
/// page open in a player's browser could otherwise drive the server on
/// 127.0.0.1 (finding N2). A web client, should one come, would need an
/// allowlist here instead.
#[allow(
    clippy::result_large_err,
    reason = "the shape tungstenite's handshake callback asks for"
)]
pub fn refuse_origin(request: &Request, response: Response) -> Result<Response, ErrorResponse> {
    if request.headers().contains_key("origin") {
        let mut refused = ErrorResponse::new(Some("browser pages are not served".to_owned()));
        *refused.status_mut() = StatusCode::FORBIDDEN;
        return Err(refused);
    }
    Ok(response)
}

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
    /// A new season began (D09).
    SeasonStarted {
        /// The year.
        year: u64,
        /// Which.
        season: Season,
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
        /// Of what; none for a cause the protocol does not carry (D49).
        cause: Option<DeathCause>,
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
///
/// Its locks are taken with the guard recovered should a panic have
/// poisoned them (`PoisonError::into_inner`): every section under one
/// is a few plain inserts, removes or reads of a map, a queue or a
/// cache, whole at any point a panic could unwind through (an allocation
/// that fails aborts, it does not unwind), and a cache entry at worst
/// stale is made again. A poisoned lock would otherwise turn the next
/// tick into a panic of the simulation thread, and the whole world's
/// loss of its last minutes, for a bug in one connection's task.
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
    /// The content's open biomes' trees, placed with them on a world cut
    /// from the atlas.
    open_ground: sim::OpenGround,
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
    /// The content's crops, and the world's calendar: the fruit a tree
    /// shows follows from them, the date and the picks (D48).
    crops: Vec<Crop>,
    calendar: Calendar,
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
    deaths: Mutex<Lru<u32, Option<DeathCause>>>,
    /// Simulation ticks per game second.
    ticks_per_second: u32,
    /// Ticks per game day.
    ticks_per_day: u64,
    /// Game days per year.
    days_per_year: u32,
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
    /// What bounds a connection's life.
    limits: Limits,
    /// Connections currently served, and opened or refused lately, by
    /// address (findings N4, N8).
    per_address: Mutex<BTreeMap<IpAddr, AddressRecord>>,
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
    /// What men picked of the crops of each chunk (D45), as the
    /// simulation told it: the fruit its trees show follows (D48). Grows
    /// with the chunks men picked in, never with the land.
    picked: BTreeMap<ChunkPos, Arc<Picks>>,
}

// The fruit's levels on the wire are the simulation's (D48).
const _: () = assert!(protocol::FRUIT_LEVELS == sim::crops::LEVELS);

/// What men picked of a chunk's crops ([`sim::changes::ChunkChanges`]'s
/// `picked`).
type Picks = BTreeMap<(Source, String), Picked>;

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
    /// What the map and the seed give it; `None` when its frame, at its
    /// versions and the fruit phase, was held when the pool looked
    /// ([`Shared::chunk_ground_cached`]): nothing was made again.
    made: Option<Made>,
}

/// What the map and the seed give a chunk ([`ChunkGround`]).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Made {
    tiles: Vec<Terrain>,
    /// Its trees, as placed: the fruit each shows is worked out as the
    /// frame is made (D48).
    trees: Arc<[sim::trees::Tree]>,
    plants: Vec<Standing>,
    currents: Vec<u16>,
    /// Its tiles' heights in decimetres; empty in a world with no relief.
    heights: Vec<i16>,
    /// The ground's natural items, none taken out yet.
    natural: Vec<ItemState>,
}

/// A chunk frame's key in the cache: the chunk, the version its trees
/// were placed at, its version as the frame carries it, and the fruit
/// phase its trees' fruit was worked out in ([`sim::crops::fruit_phase`]).
type FrameKey = (ChunkPos, Version, Version, u64);

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
    /// The fruit phase its trees' fruit was worked out in (D48).
    pub phase: u64,
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
/// Chunks a side of the far view's grid: the far view is centred on the
/// middle of the grid's square the body stands in ([`far_centre`]), so
/// it is counted once for a region, whoever stands where in it, and
/// every player there is sent the same bytes.
pub const FAR_GRID: i32 = 16;

/// The centre of the far view sent to a body in `chunk`: the middle chunk
/// of its square of the grid ([`FAR_GRID`]). The window is cut on the
/// same cells wherever it is centred, so a cell shows the same land in
/// any far view that holds it.
#[must_use]
pub fn far_centre(chunk: ChunkPos) -> ChunkPos {
    let middle = |c: i32| c.div_euclid(FAR_GRID) * FAR_GRID + FAR_GRID / 2;
    ChunkPos::new(middle(chunk.x), middle(chunk.y))
}

/// The land around chunk `centre`, coarse ([`FarView`], a binary
/// message), for the client to draw past the chunks it holds: what can be
/// seen from afar anyway, never what lies in it (bodies, items).
pub fn far_view(map: &Map, centre: ChunkPos) -> FarView {
    let cell = i32::try_from(FAR_CELL).expect("small");
    let size = i32::try_from(CHUNK_SIZE).expect("small");
    let half = i32::try_from(FAR_CELLS / 2).expect("small");
    let (cx, cy) = (
        (centre.x * size + size / 2).div_euclid(cell),
        (centre.y * size + size / 2).div_euclid(cell),
    );
    let (x, y) = (cx - half, cy - half);
    let small = |n: u32| u16::try_from(n).expect("small");
    FarView {
        cell: small(FAR_CELL),
        x,
        y,
        width: small(FAR_CELLS),
        height: small(FAR_CELLS),
        cells: coarse(map, FAR_CELL, x, y, FAR_CELLS, FAR_CELLS).into_bytes(),
    }
}

/// A served connection's place among the `max_connections` and among
/// its address's, given back when dropped.
#[derive(Debug)]
pub struct Admission<'a> {
    shared: &'a Shared,
    address: IpAddr,
}

impl Drop for Admission<'_> {
    fn drop(&mut self) {
        let mut by_address = self
            .shared
            .per_address
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(mine) = by_address.get_mut(&self.address) {
            mine.places = mine.places.saturating_sub(1);
            // The record stays while its window counts the sessions.
            if mine.is_stale(Instant::now()) {
                by_address.remove(&self.address);
            }
        }
        self.shared.connections.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Why a connection was not admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// All the places are taken (`max_connections`).
    Full(usize),
    /// The address's places are taken ([`Limits::connections_per_address`]).
    Address(usize),
    /// The address opened its connections for the minute
    /// ([`Limits::sessions_per_address`], finding N8).
    Rate(u32),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full(n) => write!(f, "refused, {n} connections already"),
            Self::Address(n) => write!(f, "refused, {n} connections already from this address"),
            Self::Rate(n) => write!(f, "refused, {n} connections from this address this minute"),
        }
    }
}

/// A connection not admitted: why, and whether it is the address's first
/// refusal of the window, the one to log (the rest are counted: a
/// connect-and-leave loop wrote a line a try, finding N8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotAdmitted {
    /// Why.
    pub why: Refused,
    /// Whether to log it.
    pub first: bool,
}

/// What one address holds and did lately.
#[derive(Debug, Clone, Copy)]
struct AddressRecord {
    /// Connections served now.
    places: usize,
    /// When the current window began.
    window: Instant,
    /// Connections admitted within the window.
    sessions: u32,
    /// Connections refused within the window.
    refused: u32,
    /// Bad-message lines written within the window, over all the
    /// address's connections (A2).
    logged: u32,
}

impl AddressRecord {
    fn new(now: Instant) -> Self {
        Self {
            places: 0,
            window: now,
            sessions: 0,
            refused: 0,
            logged: 0,
        }
    }

    /// Starts a new window if the current one is over.
    fn turn(&mut self, now: Instant) {
        if now.saturating_duration_since(self.window) >= SESSIONS_WINDOW {
            self.window = now;
            self.sessions = 0;
            self.refused = 0;
            self.logged = 0;
        }
    }

    /// Whether nothing holds the record any more: no place taken and the
    /// window over.
    fn is_stale(&self, now: Instant) -> bool {
        self.places == 0 && now.saturating_duration_since(self.window) >= SESSIONS_WINDOW
    }
}

/// A death's cause as the wire carries it: the three the protocol names;
/// the life rule's (D49: childbirth, illness, old age) as none, the way a
/// witness is told of a death, until births come to the game and the
/// protocol learns them. Nothing changes on the wire.
pub fn wire_cause(cause: sim::Death) -> Option<DeathCause> {
    match cause {
        sim::Death::Starved => Some(DeathCause::Starved),
        sim::Death::Froze => Some(DeathCause::Froze),
        sim::Death::Dehydrated => Some(DeathCause::Dehydrated),
        sim::Death::Childbirth | sim::Death::Illness | sim::Death::OldAge => None,
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
            picked: world
                .chunk_changes()
                .filter(|(_, changes)| !changes.picked.is_empty())
                .map(|(chunk, changes)| (chunk, Arc::new(changes.picked.clone())))
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
            open_ground: content.open_ground.clone(),
            ground: world.shared_ground(),
            flora,
            flora_kinds: content.flora.clone(),
            flora_wire,
            seed: world.seed(),
            crops: content.crops.clone(),
            calendar: *world.calendar(),
            plants: Mutex::new(Lru::new(PLANT_CACHE_CHUNKS)),
            frames: Mutex::new(Lru::new(FRAME_CACHE_CHUNKS)),
            far_views: Mutex::new(Lru::new(FAR_VIEWS_HELD)),
            night: AtomicBool::new(world.is_night(content)),
            day: AtomicU64::new(world.time().day),
            deaths: Mutex::new(Lru::new(DEATHS_REMEMBERED)),
            ticks_per_second: world.calendar().ticks_per_second(),
            ticks_per_day: world.calendar().ticks_per_day(),
            days_per_year: world.calendar().days_per_cycle(),
            time_scale_permille: permille(time_scale),
            items: Mutex::new(items),
            natural: world
                .has_natural_items()
                .then(|| (content.spawns.clone(), world.seed())),
            world: String::new(),
            max_connections,
            connections: AtomicUsize::new(0),
            limits: Limits::default(),
            per_address: Mutex::new(BTreeMap::new()),
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
        let lock = || {
            self.far_views
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
        };
        if let Some(message) = lock().get(&centre).cloned() {
            return Some(message);
        }
        match encode_far_view(&far_view(&self.map, centre)) {
            Ok(bytes) => {
                let message = Message::Binary(Bytes::from(bytes));
                lock().insert(centre, message.clone());
                Some(message)
            }
            Err(e) => {
                eprintln!("the far view around {centre:?} could not be encoded: {e}");
                None
            }
        }
    }

    /// The world's identity (D4) before its lineage is folded in
    /// ([`Shared::fold_lineage`]): a hash of the kinds the frames name
    /// by index (materials, trees, flora), so that a client holding
    /// frames of another content drops them. Nothing of the map's spec,
    /// nor of the land it makes, goes in: the seed is the world's secret
    /// and no hash of it goes on the wire (the security audit of 20
    /// November 2026, K1); what tells one world from another is the
    /// random birth folded in next. Sixteen hex digits.
    fn world_identity(&self, _world: &World) -> String {
        let mut hash = Fnv::default();
        for m in &self.materials {
            hash.write(m.id.as_bytes());
        }
        for s in &self.species {
            hash.write(s.id.as_bytes());
        }
        for f in &self.flora {
            hash.write(f.id.as_bytes());
        }
        format!("{:016x}", hash.0)
    }

    /// Folds the world's birth and epoch into its identity, once, at
    /// startup: a world made anew, or a run after a crash, is another
    /// world to a client, which drops the chunk frames it holds (a chunk's
    /// version may have gone back to a number it holds with other
    /// content). A stored world's birth is the random number its database
    /// drew when it was made; an ephemeral world (both 0) gets one drawn
    /// now, so that no two worlds share an identity, whatever their seeds.
    pub fn fold_lineage(&mut self, birth: u64, epoch: u64) {
        let birth = if (birth, epoch) == (0, 0) {
            crate::secret::draw()
        } else {
            birth
        };
        let mut hash = Fnv::default();
        hash.write(self.world.as_bytes());
        hash.write(&birth.to_le_bytes());
        hash.write(&epoch.to_le_bytes());
        self.world = format!("{:016x}", hash.0);
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
        self.router.lock().unwrap_or_else(PoisonError::into_inner)
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
        self.bodies.lock().unwrap_or_else(PoisonError::into_inner)
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

    /// Forgets a dead body, remembering of what it died (none for a cause
    /// the protocol does not carry); returns the tile it died on, if it
    /// was known.
    pub fn body_died(&self, body: BodyId, cause: Option<DeathCause>) -> Option<Pos> {
        self.deaths
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(body.0, cause);
        self.bodies().remove(body.0).map(|state| tile_of(state.at))
    }

    /// What `body` died of, if it died lately: none too when it died of
    /// a cause the protocol does not carry (D49).
    pub fn death_of(&self, body: u32) -> Option<DeathCause> {
        self.deaths
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&body)
            .copied()
            .flatten()
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
        let mut items = self.items.lock().unwrap_or_else(PoisonError::into_inner);
        items.stored.entry(chunk).or_default().push(item);
        items.changed(chunk, version);
    }

    /// `chunk`'s version as the item events told it: the version of the
    /// frame it would be sent in now.
    pub fn chunk_version(&self, chunk: ChunkPos) -> Version {
        self.items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .versions
            .get(&chunk)
            .copied()
            .unwrap_or(0)
    }

    /// Records what men picked of `chunk`'s crops now (`picked`, the
    /// chunk's changes'), and its version right after it (D45, D48): its
    /// frame shows the fruit left on its trees.
    pub fn crop_picked(&self, chunk: ChunkPos, picked: &Picks, version: Version) {
        let mut items = self.items.lock().unwrap_or_else(PoisonError::into_inner);
        items.picked.insert(chunk, Arc::new(picked.clone()));
        items.changed(chunk, version);
    }

    /// The fruit phase now ([`sim::crops::fruit_phase`]): which fruit
    /// windows and crops of the trees and the shrubs are open. A frame
    /// made in another phase may show its trees' and shrubs' fruit wrong
    /// (D48).
    pub fn fruit_phase(&self) -> u64 {
        sim::crops::fruit_phase(
            &self.tree_kinds,
            &self.flora_kinds,
            &self.crops,
            &self.calendar,
            Tick(self.tick.load(Ordering::Relaxed)),
        )
    }

    /// Forgets an item that was on the ground, and records its chunk's
    /// version right after it.
    pub fn item_removed(&self, item: ItemId, at: Pos, version: Version) {
        // Under the one lock `ground_items` holds while it gathers a
        // chunk's: a chunk sent at the same moment sees the item either
        // there or gone, never twice, and the version that goes with it.
        let mut items = self.items.lock().unwrap_or_else(PoisonError::into_inner);
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

    /// Sets what bounds a connection's life (the server's flags).
    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    /// What bounds a connection's life.
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Takes a place for a new connection from `address`, or the refusal:
    /// all places taken, or the address's places taken.
    pub fn admit(&self, address: IpAddr) -> Result<Admission<'_>, NotAdmitted> {
        self.admit_at(address, Instant::now())
    }

    /// [`Shared::admit`] as of `now`: the sessions of an address are
    /// counted by the minute ([`Limits::sessions_per_address`], finding
    /// N8), and a refused connection is the address's first of the minute
    /// or not ([`NotAdmitted::first`]).
    pub fn admit_at(&self, address: IpAddr, now: Instant) -> Result<Admission<'_>, NotAdmitted> {
        let mut by_address = self
            .per_address
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !by_address.contains_key(&address) {
            // A new address: the records nothing holds any more go, so
            // the map holds the addresses of the last minute at most.
            by_address.retain(|_, record| !record.is_stale(now));
        }
        let mine = by_address
            .entry(address)
            .or_insert_with(|| AddressRecord::new(now));
        mine.turn(now);
        let refuse = |mine: &mut AddressRecord, why| {
            mine.refused = mine.refused.saturating_add(1);
            Err(NotAdmitted {
                why,
                first: mine.refused == 1,
            })
        };
        if mine.sessions >= self.limits.sessions_per_address {
            return refuse(mine, Refused::Rate(self.limits.sessions_per_address));
        }
        if mine.places >= self.limits.connections_per_address {
            return refuse(mine, Refused::Address(self.limits.connections_per_address));
        }
        let mut current = self.connections.load(Ordering::Relaxed);
        loop {
            if current >= self.max_connections {
                return refuse(mine, Refused::Full(self.max_connections));
            }
            match self.connections.compare_exchange_weak(
                current,
                current + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    mine.places += 1;
                    mine.sessions = mine.sessions.saturating_add(1);
                    return Ok(Admission {
                        shared: self,
                        address,
                    });
                }
                Err(seen) => current = seen,
            }
        }
    }

    /// May one more bad message from `address` be written in the log? Its
    /// lines are budgeted by the minute over all its connections
    /// ([`COMPLAINTS_LOGGED_PER_ADDRESS`], finding A2); the log says once
    /// when the budget is spent. An address not admitted through
    /// [`Shared::admit`] has no record and every line.
    pub fn complaint_logged(&self, address: IpAddr) -> bool {
        self.complaint_logged_at(address, Instant::now())
    }

    /// [`Shared::complaint_logged`] as of `now`.
    pub fn complaint_logged_at(&self, address: IpAddr, now: Instant) -> bool {
        let mut by_address = self
            .per_address
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(mine) = by_address.get_mut(&address) else {
            return true;
        };
        mine.turn(now);
        mine.logged = mine.logged.saturating_add(1);
        if mine.logged == COMPLAINTS_LOGGED_PER_ADDRESS + 1 {
            eprintln!(
                "address {address}: {COMPLAINTS_LOGGED_PER_ADDRESS} bad messages logged within \
                 a minute, its next ones are counted and not logged"
            );
        }
        mine.logged <= COMPLAINTS_LOGGED_PER_ADDRESS
    }

    /// Connections currently served from `address`.
    pub fn connections_from(&self, address: IpAddr) -> usize {
        self.per_address
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&address)
            .map_or(0, |record| record.places)
    }

    /// Connections currently served.
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::Relaxed)
    }

    /// Sets a body's intention for the coming tick, replacing any earlier
    /// one: the latest wins.
    pub fn intend(&self, intention: Intention) {
        let body = intention.body().0;
        self.intentions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(body, intention);
    }

    /// Takes every pending intention, in body order, leaving none.
    pub fn take_intentions(&self) -> Vec<Intention> {
        let taken = std::mem::take(
            &mut *self
                .intentions
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        taken.into_values().collect()
    }

    /// Sends a message to the connection piloting `body`, if any. Dropped
    /// when that pilot has too many waiting already.
    pub fn tell_pilot(&self, body: BodyId, message: ServerMessage) {
        if let Some(pilot) = self
            .pilots
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
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
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .map(|id| BodyId(*id))
            .collect()
    }

    /// Gives the connection the free body with the lowest id, if any.
    fn take_body(&self, private: mpsc::Sender<ServerMessage>) -> Option<u32> {
        let mut pilots = self.pilots.lock().unwrap_or_else(PoisonError::into_inner);
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
                .unwrap_or_else(PoisonError::into_inner)
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
        std::mem::take(&mut *self.births.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Records a body just born at the camp for the connection that
    /// asked: it pilots it, and whoever sees the camp is told it stands
    /// there. A connection gone meanwhile leaves it free for the next.
    fn body_born(&self, birth: Birth, state: BodyState) {
        let (body, at) = (state.id, tile_of(state.at));
        {
            let mut pilots = self.pilots.lock().unwrap_or_else(PoisonError::into_inner);
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
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&body);
    }

    /// The clock as of now.
    pub fn clock(&self) -> Clock {
        Clock {
            tick: self.tick.load(Ordering::Relaxed),
            ticks_per_second: self.ticks_per_second,
            ticks_per_day: self.ticks_per_day,
            days_per_year: self.days_per_year,
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
                .expect("a ground made whole")
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
            made: Some(Made {
                tiles: tiles.into_iter().map(terrain_view).collect(),
                trees: self.trees_in(chunk),
                plants: self.plants_in(chunk),
                currents: self.map.chunk_currents(chunk)?,
                heights: self.map.chunk_heights(chunk).unwrap_or_default(),
                natural: self.natural_items(chunk),
            }),
        })
    }

    /// `chunk`'s ground as the pool builds it, the cache looked at first:
    /// when the frame of the chunk at its versions now and in the fruit
    /// phase now is held, nothing is made again (its natural items alone
    /// cost a millisecond); else [`Shared::chunk_ground`]. Should the key
    /// move before the frame leaves, [`Shared::chunk_send_of`] says so and
    /// the chunk is built again, whole. `None` off the map.
    pub fn chunk_ground_cached(&self, chunk: ChunkPos) -> Option<ChunkGround> {
        // The key in the order the frame reads it: the ground's version
        // first, as the trees are placed after it.
        let version = self.ground.version(chunk);
        let key = (
            chunk,
            version,
            self.chunk_version(chunk),
            self.fruit_phase(),
        );
        // Only a chunk of the map was ever framed.
        if self
            .frames
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&key)
        {
            return Some(ChunkGround {
                chunk,
                version,
                made: None,
            });
        }
        self.chunk_ground(chunk)
    }

    /// A chunk's ground finished with what stands and lies on it now:
    /// its frame, encoded once per version of its ground and of its items
    /// and the same bytes then for every client, and the bodies in it.
    /// `None` only for a ground the pool found held
    /// ([`Shared::chunk_ground_cached`]) whose frame is not held any more
    /// (its versions or the fruit phase moved since, or the cache let it
    /// go): it is to be built again, whole.
    ///
    /// # Errors
    ///
    /// When the chunk cannot be framed ([`encode_chunk`]).
    pub fn chunk_send_of(&self, ground: ChunkGround) -> Result<Option<ChunkSend>, BoxError> {
        let chunk = ground.chunk;
        let no_items: &[ItemState] = &[];
        // Gathered together, as the items' events see them, so the frame
        // and its version agree.
        let (natural, stored, version, picked) =
            self.ground_items(chunk, ground.made.as_ref().map_or(no_items, |m| &m.natural));
        let bodies = self.bodies_in(chunk);
        // The fruit as of now: the frame is the same bytes for every
        // client until a window or a crop opens or shuts (D48).
        let tick = Tick(self.tick.load(Ordering::Relaxed));
        let phase = self.fruit_phase_at(tick);
        let key = (chunk, ground.version, version, phase);
        let held = self
            .frames
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
            .cloned();
        let frame = if let Some(frame) = held {
            frame
        } else {
            if ground.made.is_none() {
                return Ok(None);
            }
            let frame = Bytes::from(encode_chunk(&self.frame_of(
                ground,
                &natural,
                &stored,
                version,
                tick,
                picked.as_deref(),
            )?)?);
            self.frames
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(key, frame.clone());
            frame
        };
        Ok(Some(ChunkSend {
            frame,
            bodies,
            version,
            phase,
        }))
    }

    /// The fruit phase at `tick` ([`Shared::fruit_phase`]).
    fn fruit_phase_at(&self, tick: Tick) -> u64 {
        sim::crops::fruit_phase(
            &self.tree_kinds,
            &self.flora_kinds,
            &self.crops,
            &self.calendar,
            tick,
        )
    }

    /// The frame of a chunk's ground and items, at `version`, its trees'
    /// and shrubs' fruit as of `tick` with `picked` its picks (D48).
    fn frame_of(
        &self,
        ground: ChunkGround,
        natural: &[ItemState],
        stored: &[ItemState],
        version: Version,
        tick: Tick,
        picked: Option<&Picks>,
    ) -> Result<ChunkFrame, BoxError> {
        let chunk = ground.chunk;
        let ground = ground.made.ok_or("a ground made whole")?;
        let trees = ground
            .trees
            .iter()
            .map(|tree| {
                let (dx, dy) = offset_in(chunk, tree.at).ok_or("a tree off its chunk")?;
                let fruit = sim::crops::fruit_on(
                    &self.tree_kinds[tree.species],
                    &self.crops,
                    &self.calendar,
                    tick,
                    self.seed,
                    self.map.origin(),
                    tree,
                    picked,
                );
                Ok(StandingTree {
                    dx,
                    dy,
                    kind: u16::try_from(tree.species)?,
                    stage: protocol::STAGE_GROWN,
                    fruit,
                })
            })
            .collect::<Result<Vec<_>, BoxError>>()?;
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
            heights: ground.heights,
            trees,
            plants: self.plants_fruit(chunk, ground.plants, tick, picked),
            natural,
            stored,
        })
    }

    /// `plants`, of `chunk`, each with the fruit it shows as of `tick`
    /// with `picked` its chunk's picks (D48): a berry shrub's, as a
    /// tree's; none for a kind without a fruit window.
    fn plants_fruit(
        &self,
        chunk: ChunkPos,
        mut plants: Vec<Standing>,
        tick: Tick,
        picked: Option<&Picks>,
    ) -> Vec<Standing> {
        // The kinds the client knows, by their index on the wire.
        let mut kinds: Vec<Option<&sim::flora::Kind>> = vec![None; self.flora.len()];
        for (kind, wire) in self.flora_kinds.iter().zip(&self.flora_wire) {
            if let Some(slot) = wire.and_then(|w| kinds.get_mut(usize::from(w))) {
                *slot = Some(kind).filter(|k| k.fruit.is_some());
            }
        }
        if kinds.iter().all(Option::is_none) {
            return plants;
        }
        let origin = chunk.origin();
        let mm = sim::MM_PER_TILE;
        for plant in &mut plants {
            let Some(kind) = kinds.get(usize::from(plant.kind)).copied().flatten() else {
                continue;
            };
            let at = FinePos::new(
                origin.x * mm + i32::from(plant.dx),
                origin.y * mm + i32::from(plant.dy),
            );
            let fruit = sim::crops::plant_fruit_on(
                kind,
                &self.crops,
                &self.calendar,
                tick,
                self.seed,
                self.map.origin(),
                chunk,
                at,
                picked,
            );
            plant.fruit = fruit;
        }
        plants
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
        let sent = self.chunk_send_of(ground)?.ok_or("a ground made whole")?;
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
        if let (Some(lo), Some(hi)) = (f.heights.iter().min(), f.heights.iter().max()) {
            writeln!(out, "  heights: {lo} to {hi} dm")?;
        }
        let name =
            |list: &[String], k: usize| list.get(k).cloned().unwrap_or_else(|| format!("#{k}"));
        let species: Vec<String> = self.species.iter().map(|s| s.id.clone()).collect();
        let flora: Vec<String> = self.flora.iter().map(|s| s.id.clone()).collect();
        let materials: Vec<String> = self.materials.iter().map(|s| s.id.clone()).collect();
        for t in &f.trees {
            writeln!(
                out,
                "  tree {} at +{} mm, +{} mm, fruit {}",
                name(&species, t.kind.into()),
                t.dx,
                t.dy,
                t.fruit
            )?;
        }
        for p in &f.plants {
            writeln!(
                out,
                "  plant {} at +{} mm, +{} mm, fruit {}",
                name(&flora, p.kind.into()),
                p.dx,
                p.dy,
                p.fruit
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
    /// excepted, and the stored ones, each in the order of their ids; the
    /// chunk's version, and what men picked of its crops at that version.
    fn ground_items(
        &self,
        chunk: ChunkPos,
        natural: &[ItemState],
    ) -> (Vec<ItemState>, Vec<ItemState>, Version, Option<Arc<Picks>>) {
        // Filtered and gathered under the lock `item_removed` holds, so an
        // item taken meanwhile shows either here or nowhere.
        let items = self.items.lock().unwrap_or_else(PoisonError::into_inner);
        let mut natural: Vec<ItemState> = natural
            .iter()
            .filter(|i| !items.taken.contains(&ItemId(i.id)))
            .cloned()
            .collect();
        let mut stored = items.stored.get(&chunk).cloned().unwrap_or_default();
        let version = items.versions.get(&chunk).copied().unwrap_or(0);
        let picked = items.picked.get(&chunk).cloned();
        drop(items);
        natural.sort_by_key(|i| i.id);
        stored.sort_by_key(|i| i.id);
        (natural, stored, version, picked)
    }

    /// The trees standing in `chunk`, placed from the map and the seed
    /// the first time they are asked for ([`Ground::trees_in`]).
    fn trees_in(&self, chunk: ChunkPos) -> Arc<[sim::trees::Tree]> {
        self.ground.trees_in(
            &self.map,
            &self.tree_kinds,
            &self.open_ground,
            self.seed,
            chunk,
        )
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
                    .trees_in(
                        &self.map,
                        &self.tree_kinds,
                        &self.open_ground,
                        self.seed,
                        around,
                    )
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
            .unwrap_or_else(PoisonError::into_inner)
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
                    // Its fruit is worked out as the frame is made (D48).
                    let fruit = protocol::FRUIT_NONE;
                    Some(Standing {
                        dx,
                        dy,
                        kind,
                        stage: protocol::STAGE_GROWN,
                        fruit,
                    })
                })
                .collect();
        // Placed with no lock held: two connections missing the same
        // chunk at once place the same plants, and the second insert only
        // replaces the first.
        self.plants
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
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
            Some(u16::try_from(flora.len() - 1).expect("at most 65,536 kinds of flora"))
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
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "values bounded by the world size and the content ranges; values checked non-negative by the surrounding rule"
)]
fn grams(mass_kg: f32) -> u32 {
    (mass_kg * 1000.0).round().clamp(0.0, 1e9) as u32
}

/// A length in metres as whole centimetres, for the wire.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "values bounded by the world size and the content ranges; values checked non-negative by the surrounding rule"
)]
fn centimetres(metres: f32) -> u32 {
    (metres * 100.0).round().clamp(0.0, 1e9) as u32
}

/// A pace as thousandths, for the wire; absurd paces saturate.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "values bounded by the world size and the content ranges; values checked non-negative by the surrounding rule"
)]
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
                        let _admission = match shared.admit(peer.ip()) {
                            Ok(admission) => admission,
                            Err(refusal) => {
                                // Dropping the stream closes it; the client sees
                                // the connection end before any upgrade. The
                                // address's first refusal of the minute is
                                // logged, the rest are not (finding N8).
                                if refusal.first {
                                    eprintln!(
                                        "connection {peer}: {} (this address's next refusals are not logged for a minute)",
                                        refusal.why
                                    );
                                }
                                return;
                            }
                        };
                        // One line a session's end, its error folded in (N8).
                        match handle_connection(stream, peer, &shared, &pool, shutdown).await {
                            Ok(()) => println!("connection {peer}: closed"),
                            Err(e) => eprintln!("connection {peer}: closed: {e}"),
                        }
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

#[allow(
    clippy::too_many_lines,
    reason = "one straight reading, top to bottom; splitting it would scatter the rule"
)]
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
    let upgrade =
        tokio_tungstenite::accept_hdr_async_with_config(stream, refuse_origin, Some(config));
    let Ok(upgraded) = tokio::time::timeout(HANDSHAKE_TIMEOUT, upgrade).await else {
        // Silent TCP connections are dropped without ceremony.
        return Ok(());
    };
    let mut ws = upgraded?;
    let limits = shared.limits();
    let mut complaints = Complaints::default();
    // Under `--dev`, when this connection last teleported (N3).
    let mut teleports = Throttle::default();
    // Whether this connection was told once of a message's unknown fields
    // (N10): the check costs a second reading of the message, paid until
    // the first is found and no further.
    let mut unknown_told = false;

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

    // The last moment anything came from the client, pongs included: a
    // ping goes every `ping_every`, and a connection silent for
    // `drop_silent_after` is dropped (finding N4).
    let mut last_heard = Instant::now();
    let mut ping = tokio::time::interval_at(
        tokio::time::Instant::now() + limits.ping_every,
        limits.ping_every,
    );
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let outcome: Result<(), BoxError> = async {
        loop {
            // Checked here, not as the queue leaves: a client whose wire
            // is stuck takes nothing off it.
            link.check_pace()?;
            tokio::select! {
                incoming = reader.next() => match incoming {
                    Some(Ok(message)) if message.len() > MAX_MESSAGE_BYTES => {
                        return Err(format!("a message of {} bytes, past the limit", message.len()).into());
                    }
                    Some(Ok(message)) => {
                        last_heard = Instant::now();
                        match message {
                            Message::Close(_) => return Ok(()),
                            Message::Text(text) => {
                              let decoded = decode::<ClientMessage>(text.as_str());
                              // Unknown fields are ignored (D26: a newer client
                              // may add some) and told once a connection (N10).
                              if let Ok(message) = &decoded
                                  && !unknown_told
                              {
                                  let unknown = protocol::unknown_fields(text.as_str(), message);
                                  if !unknown.is_empty() {
                                      unknown_told = true;
                                      match noted(&mut complaints, shared, peer.ip()) {
                                          Complaint::Logged => eprintln!(
                                              "connection {peer}: unknown fields ignored (told once): {}",
                                              loggable(&unknown.join(", "))
                                          ),
                                          Complaint::Silent => {}
                                          Complaint::TooMany => return Err(too_many(complaints)),
                                      }
                                  }
                              }
                              match decoded {
                                Ok(ClientMessage::Ping { nonce }) => {
                                    link.event(&ServerMessage::Pong { nonce });
                                }
                                Ok(ClientMessage::Hello { .. }) => {
                                    // A second hello is a client bug; drop the connection.
                                    return Ok(());
                                }
                                Ok(ClientMessage::ChunkMissing { x, y }) => {
                                    // Honoured once, in answer to a `chunk_same` or
                                    // a `chunk_fruit`; a repeat is a bad message (N7).
                                    if !link.missing(ChunkPos::new(x, y)) {
                                        match noted(&mut complaints, shared, peer.ip()) {
                                            Complaint::Logged => eprintln!("connection {peer}: chunk_missing for a chunk whose frame was sent, ignored"),
                                            Complaint::Silent => {}
                                            Complaint::TooMany => return Err(too_many(complaints)),
                                        }
                                    }
                                }
                                Ok(ClientMessage::DevTeleport { .. }) if !shared.is_dev() => {
                                    match noted(&mut complaints, shared, peer.ip()) {
                                        Complaint::Logged => eprintln!("connection {peer}: teleport refused, the server is not in --dev"),
                                        Complaint::Silent => {}
                                        Complaint::TooMany => return Err(too_many(complaints)),
                                    }
                                }
                                // At most one teleport every `teleport_every` a
                                // connection (N3): a view a tick is a flood.
                                Ok(ClientMessage::DevTeleport { .. })
                                    if !teleports.allow(Instant::now(), limits.teleport_every) => {}
                                Ok(message) => {
                                    if let Some(body) = link.pilot.body {
                                        shared.intend(intention_of(BodyId(body), &message));
                                    }
                                }
                                Err(e) => match noted(&mut complaints, shared, peer.ip()) {
                                    Complaint::Logged => eprintln!(
                                        "connection {peer}: undecodable message ignored: {}",
                                        loggable(&e.to_string())
                                    ),
                                    Complaint::Silent => {}
                                    Complaint::TooMany => return Err(too_many(complaints)),
                                },
                              }
                            }
                            // Binary frames are not part of the protocol; ping/pong
                            // frames are handled by the WebSocket layer itself.
                            _ => {}
                        }
                    }
                    None => return Ok(()),
                    Some(Err(e)) => return Err(e.into()),
                },
                _ = ping.tick() => {
                    let silent = last_heard.elapsed();
                    if silent >= limits.drop_silent_after {
                        return Err(format!("silent for {} s, dropped", silent.as_secs()).into());
                    }
                    link.ping();
                }
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

/// One more bad message on a connection: counted for it
/// ([`Complaints::note`]) and, when its line would be written, for its
/// address too (A2): past the address's budget of the minute the line is
/// not written.
fn noted(complaints: &mut Complaints, shared: &Shared, address: IpAddr) -> Complaint {
    match complaints.note() {
        Complaint::Logged if !shared.complaint_logged(address) => Complaint::Silent,
        verdict => verdict,
    }
}

/// The error that closes a connection past [`COMPLAINTS_MAX`] bad messages.
fn too_many(complaints: Complaints) -> BoxError {
    format!(
        "{} bad messages, more than {COMPLAINTS_MAX}: not our client",
        complaints.count()
    )
    .into()
}

/// What a connection has to send: the lane a message goes in.
enum Outgoing {
    /// A chunk built on the pool, finished with what stands and lies in
    /// it when it leaves.
    Chunk(ChunkGround),
    /// A chunk the client keeps, as it keeps it: `chunk_same` instead of
    /// its frame, if neither its version nor its fruit phase has moved
    /// when it leaves.
    Same(ChunkPos, Kept),
    /// A message as is (the far view).
    Frame(Message),
}

/// What the pool built for a connection.
enum Built {
    /// A chunk's ground.
    Ground(ChunkGround),
    /// The far view around a chunk, encoded.
    Far(ChunkPos, Message),
    /// Nothing: the job found its chunk no longer wanted. Told all the
    /// same, so every job given yields a result and a chunk the pool
    /// refused ([`Link::deferred`]) is asked again as the lane drains.
    Skipped,
}

/// A chunk in view whose frame (or `chunk_same`) left: the version of the
/// frame the client was given, what it keeps once the chunk leaves its
/// view, and the version it knows, the frame's then each item event's;
/// the fruit phase of its frame (D48), and the fruit it shows
/// ([`fruit_signature`]), each `None` when not known (a frame the client
/// kept from an earlier connection). `told`: a `chunk_same` or a
/// `chunk_fruit` left for it, which one `chunk_missing` may answer; a
/// frame left otherwise, and a `chunk_missing` is a repeat (N7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Held {
    frame: Version,
    known: Version,
    phase: Option<u64>,
    fruit: Option<u64>,
    told: bool,
}

impl Held {
    /// The frame as the client keeps it once the chunk leaves its view.
    fn kept(self) -> Kept {
        Kept {
            version: self.frame,
            phase: self.phase,
            fruit: self.fruit,
        }
    }
}

/// A frame the client keeps out of view (D4): its version, the fruit
/// phase it was made in and the fruit it shows (D48), these two `None`
/// when not known (kept from an earlier connection).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Kept {
    version: Version,
    phase: Option<u64>,
    fruit: Option<u64>,
}

/// The fruit a frame shows, as a number: two frames of a chunk show the
/// same fruit if their signatures are equal (D48). The levels of its
/// trees then of its plants ([`protocol::frame_fruit`]), hashed.
fn fruit_signature(trees: &[u8], plants: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    trees.hash(&mut hasher);
    plants.hash(&mut hasher);
    hasher.finish()
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
    /// keeps them (D4): the chunk and its frame's version and fruit phase
    /// (D48; `None` for those kept from an earlier connection), the least
    /// recently hidden forgotten first, at most
    /// [`protocol::CHUNK_CACHE_CHUNKS`]. When one comes back into view at
    /// that version and phase, `chunk_same` goes instead of the frame. A
    /// guess: should the client not hold it, it says so and gets the
    /// frame.
    cache: Lru<ChunkPos, Kept>,
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
    /// The chunks the pool refused, its lane full
    /// ([`crate::pool::LANE_JOBS_MOST`], finding N9), and whether whole:
    /// asked again, one a result, as the lane's jobs are done.
    deferred: BTreeMap<ChunkPos, bool>,
    /// The far view the pool refused, asked again likewise.
    deferred_far: Option<ChunkPos>,
    /// Where the pool's jobs send what they built.
    results: mpsc::UnboundedSender<Built>,
}

impl Drop for Link {
    fn drop(&mut self) {
        // The jobs still waiting in the lane find nothing wanted and
        // build nothing for a connection that is gone.
        self.wanted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
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
            deferred: BTreeMap::new(),
            deferred_far: None,
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

    /// A WebSocket ping, in the events lane: the transport on the other
    /// side answers it, and the answer is heard here.
    fn ping(&mut self) {
        self.events.push_back(Message::Ping(Bytes::new()));
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
            Some(Outgoing::Same(chunk, kept)) => {
                if !self.still_held(chunk, kept) {
                    // Changed in sight since it was asked for: the frame,
                    // built.
                    self.build(chunk);
                    return Ok(None);
                }
                self.awaiting.remove(&chunk);
                let bodies = self.bodies_seen_in(chunk);
                self.same(chunk, kept, bodies).map(Some)
            }
            Some(Outgoing::Chunk(ground)) => {
                let chunk = ground.chunk;
                let Some(sent) = self.shared.chunk_send_of(ground)? else {
                    // Its frame held when the pool looked, not any more:
                    // built again, whole this time.
                    self.awaiting.insert(chunk);
                    self.build_whole(chunk, true);
                    return Ok(None);
                };
                // Kept by the client and unchanged, its fruit too: no
                // frame.
                if let Some(kept) = self.cache.remove(&chunk)
                    && kept.version == sent.version
                    && kept.phase == Some(sent.phase)
                {
                    return self.same(chunk, kept, sent.bodies).map(Some);
                }
                let (trees, plants) = protocol::frame_fruit(&sent.frame)?;
                let fruit = fruit_signature(&trees, &plants);
                // Held at this version already (sent again at a dawn): all
                // but its fruit is as the client holds it (D48).
                if let Some(held) = self.sent.get(&chunk).copied()
                    && held.known == sent.version
                {
                    self.sent.insert(
                        chunk,
                        Held {
                            phase: Some(sent.phase),
                            fruit: Some(fruit),
                            // A `chunk_same` still waiting for its answer
                            // keeps waiting (lot F's F1: a dawn between a
                            // `chunk_same` and its `chunk_missing` forgot it).
                            told: held.told || held.fruit != Some(fruit),
                            ..held
                        },
                    );
                    if held.fruit == Some(fruit) {
                        return Ok(None);
                    }
                    let message = ServerMessage::ChunkFruit {
                        x: chunk.x,
                        y: chunk.y,
                        version: held.frame,
                        trees: protocol::fruit_digits(&trees),
                        plants: protocol::fruit_digits(&plants),
                    };
                    return Ok(Some(Message::text(encode(&message)?)));
                }
                self.tell_bodies(chunk, sent.bodies)?;
                self.sent.insert(
                    chunk,
                    Held {
                        frame: sent.version,
                        known: sent.version,
                        phase: Some(sent.phase),
                        fruit: Some(fruit),
                        told: false,
                    },
                );
                Ok(Some(Message::Binary(sent.frame)))
            }
        }
    }

    /// `chunk_same` for `chunk`, held as `kept`, with the bodies in it.
    fn same(
        &mut self,
        chunk: ChunkPos,
        kept: Kept,
        bodies: Vec<BodyState>,
    ) -> Result<Message, BoxError> {
        self.tell_bodies(chunk, bodies)?;
        self.sent.insert(
            chunk,
            Held {
                frame: kept.version,
                known: kept.version,
                phase: kept.phase,
                fruit: kept.fruit,
                told: true,
            },
        );
        let same = ServerMessage::ChunkSame {
            x: chunk.x,
            y: chunk.y,
            version: kept.version,
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
        // Their fruit phase is not known: kept, and sent again at the next
        // dawn should the fruit have changed (`refresh`, D48).
        for &(x, y, version) in held.chunks.iter().skip(skip) {
            self.cache.insert(
                ChunkPos::new(x, y),
                Kept {
                    version,
                    phase: None,
                    fruit: None,
                },
            );
        }
    }

    /// The client does not hold `chunk` as `chunk_same` (or `chunk_fruit`)
    /// said: its frame, if still in view. Honoured once an answer: a
    /// `chunk_missing` for a chunk whose frame itself left is a repeat,
    /// which made the server send seventeen times the bytes it received
    /// (N7); `false` then, and the frame is not built again, as for a
    /// chunk in view whose frame is on its way. A chunk no longer in view
    /// is a late answer, and nothing.
    fn missing(&mut self, chunk: ChunkPos) -> bool {
        self.cache.remove(&chunk);
        match self.sent.get(&chunk) {
            Some(held) if held.told => {
                self.sent.remove(&chunk);
                if self.awaiting.insert(chunk) {
                    self.build(chunk);
                }
                true
            }
            Some(_) => false,
            None => !self.view.chunks.contains(&chunk),
        }
    }

    /// What the pool built: a chunk's ground or the far view, or nothing
    /// for a chunk gone from the view. One job done, one refused chunk
    /// asked again (N9).
    fn built(&mut self, built: Built) {
        match built {
            Built::Ground(ground) => self.ground_built(ground),
            Built::Far(centre, message) => self.far_built(centre, message),
            Built::Skipped => {}
        }
        self.resume_deferred();
    }

    /// Asks the pool again for one of what it refused: the far view
    /// first, then a chunk still awaited (whatever was answered or left
    /// the view meanwhile is forgotten).
    fn resume_deferred(&mut self) {
        if let Some(chunk) = self.deferred_far.take() {
            self.far_view(chunk);
            return;
        }
        while let Some((chunk, whole)) = self.deferred.pop_first() {
            if self.awaiting.contains(&chunk) {
                self.build_whole(chunk, whole);
                return;
            }
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
        if let Some(kept) = self.cache.remove(&chunk)
            && self.still_held(chunk, kept)
        {
            self.ground.push_back(Outgoing::Same(chunk, kept));
            return;
        }
        self.build(chunk);
    }

    /// May the frame the client keeps as `kept` stand for `chunk`
    /// (`chunk_same`)? When nothing of the chunk moved and its fruit is of
    /// this phase; and, past the eyes, whatever moved in it: what men did
    /// to a chunk out of the creatures' range is not told, neither by an
    /// event (F6) nor by a version a reconnecting or returning client
    /// could compare with its own (the second security audit of 20
    /// November 2026, C1: 360 `chunk_same` and one fresh frame told where
    /// a hidden body had picked something up, 200 m away). The client's
    /// version is kept as what it knows, so the frame goes when the chunk
    /// comes within range (`move_range`) or at dawn (`refresh`: the daily,
    /// chunk-coarse tell the lead accepted on 20 November 2026).
    fn still_held(&self, chunk: ChunkPos, kept: Kept) -> bool {
        kept.phase.is_none_or(|p| p == self.shared.fruit_phase())
            && (kept.version == self.shared.chunk_version(chunk) || !self.range.contains(&chunk))
    }

    /// Asks the pool for `chunk`'s ground, its frame looked for in the
    /// cache first ([`Shared::chunk_ground_cached`]).
    fn build(&mut self, chunk: ChunkPos) {
        self.build_whole(chunk, false);
    }

    /// Asks the pool for `chunk`'s ground: made `whole` whatever the cache
    /// holds ([`Shared::chunk_ground`]), else the cache looked at first.
    fn build_whole(&mut self, chunk: ChunkPos, whole: bool) {
        let (shared, wanted, results) = (
            Arc::clone(&self.shared),
            Arc::clone(&self.wanted),
            self.results.clone(),
        );
        // Answered whatever becomes of the job, a panic included (F2).
        let answer = Answer::new(results, Built::Skipped);
        let given = self.pool.submit(
            self.lane,
            Box::new(move || {
                if !wanted
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .contains(&chunk)
                {
                    // Left the view before its turn came.
                    return;
                }
                let ground = if whole {
                    shared.chunk_ground(chunk)
                } else {
                    shared.chunk_ground_cached(chunk)
                };
                answer.send(ground.map_or(Built::Skipped, Built::Ground));
            }),
        );
        if !given {
            // The lane is full (N9): asked again as its jobs are done.
            self.deferred.insert(chunk, whole);
        }
    }

    /// Moves the view to `centre`, telling the client the window it now
    /// holds (`view`: it lets go of what lies outside, keeping the frames
    /// in its cache, rows then columns, as this mirror does) and asking
    /// for the new chunks, nearest the centre first.
    fn move_view(&mut self, centre: ChunkPos) {
        let (mut shown, mut hidden) = self.view.recentre(&self.shared.map, centre);
        self.wanted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone_from(&self.view.chunks);
        shown.sort_by_key(|c| ((c.x - centre.x).pow(2) + (c.y - centre.y).pow(2), c.y, c.x));
        hidden.sort_by_key(|c| (c.y, c.x));
        let ground = std::mem::take(&mut self.ground);
        for out in ground {
            match out {
                Outgoing::Chunk(g) if !self.view.chunks.contains(&g.chunk) => {}
                // Still kept by the client, never shown: kept here too.
                Outgoing::Same(chunk, kept) if !self.view.chunks.contains(&chunk) => {
                    self.cache.insert(chunk, kept);
                }
                out => self.ground.push_back(out),
            }
        }
        for &chunk in &hidden {
            self.awaiting.remove(&chunk);
            self.deferred.remove(&chunk);
            // What the client held goes to its cache; a chunk never sent
            // is only forgotten here.
            if let Some(held) = self.sent.remove(&chunk) {
                self.cache.insert(chunk, held.kept());
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
            let Some(held) = self.sent.get(&chunk) else {
                continue; // its frame brings them
            };
            // Its items may have changed while it stood past the eyes
            // (F6): sent again, as at dawn.
            if self.shared.chunk_version(chunk) != held.known && self.awaiting.insert(chunk) {
                self.build(chunk);
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

    /// Asks the pool for the far view for a body in `chunk`, centred on
    /// its square of the grid ([`far_centre`]), after the chunks asked
    /// before it in the lane; it goes in the ground lane when back
    /// (`far_built`). Counting it reads 2 km of land, made per chunk: never
    /// on the network's threads, and once for every body of the square
    /// while the far views held keep it.
    fn far_view(&mut self, chunk: ChunkPos) {
        let centre = far_centre(chunk);
        self.view.far = Some(centre);
        self.far_awaited = Some(centre);
        let shared = Arc::clone(&self.shared);
        let answer = Answer::new(self.results.clone(), Built::Skipped);
        let given = self.pool.submit(
            self.lane,
            Box::new(move || {
                if answer.is_closed() {
                    return; // the connection is gone
                }
                answer.send(
                    shared
                        .far_view_message(centre)
                        .map_or(Built::Skipped, |message| Built::Far(centre, message)),
                );
            }),
        );
        if !given {
            self.deferred_far = Some(chunk);
        }
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
                self.refresh();
            }
            Published::NightFell { day } => {
                self.event(&ServerMessage::NightFell { day });
            }
            Published::SeasonStarted { year, season } => {
                self.event(&ServerMessage::SeasonStarted { year, season });
            }
            Published::BodySleeps { body, at, asleep } => {
                if self.sees(at) {
                    self.event(&ServerMessage::BodySleeps { body, asleep });
                }
            }
            Published::BodyDied { body, at, cause } => {
                let own = Some(body) == piloted;
                if self.sees(at) || own {
                    self.known.remove(&body);
                    // The cause is the pilot's alone: a witness sees a
                    // body fall, not why (the audit's M2).
                    self.event(&ServerMessage::BodyDied {
                        body,
                        cause: own.then_some(cause).flatten(),
                    });
                }
                return own;
            }
            // What men do to the ground is told within the eyes' range,
            // as the bodies are (F6); the chunk goes again when it comes
            // within range or at dawn (`move_range`, `refresh`).
            Published::ItemAdded { item, version } => {
                let at = Pos::new(item.x, item.y);
                if self.sees_ground(at) {
                    self.item_changed(at.chunk(), version);
                    self.event(&ServerMessage::ItemAdded { item });
                }
            }
            Published::ItemRemoved { item, at, version } => {
                if self.sees_ground(at) {
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

    /// Is this tile within the eyes, what men do to its ground told? In
    /// a chunk of the creatures' range and within [`sim::EYES_RANGE_M`]
    /// of the eye, in metres, as `World::sight` judges an act on it: an
    /// item change 147 m away is told whatever its chunk, one 151 m away
    /// is not (the second security audit of 20 November 2026, A3: by
    /// chunk, a change within 150 m went untold at the edge of a chunk
    /// while the refusal of an act on it said the truth). The bodies stay
    /// told by chunk, as since D18's stand-in.
    fn sees_ground(&self, at: Pos) -> bool {
        self.sees(at) && sim::within_eyes(self.eye(), at)
    }

    /// Where this connection looks from: the piloted body's tile, or the
    /// middle of the view's home chunk for a watcher.
    fn eye(&self) -> Pos {
        self.pilot
            .body
            .and_then(|body| self.shared.position_of(body))
            .unwrap_or_else(|| {
                let half = i32::try_from(CHUNK_SIZE / 2).expect("a small chunk");
                let origin = self.home().origin();
                Pos::new(origin.x + half, origin.y + half)
            })
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

    /// Sends again the chunks in view the client may hold wrong (at each
    /// dawn, and when it fell behind): those whose version moved past what
    /// it was told (a crop picked: no event tells it, D48) and those whose
    /// frame shows the fruit of another phase, or of a phase not known
    /// (D48). A fruit window or a crop that opens or shuts reaches the
    /// client within a day; on any other day nothing goes. A chunk whose
    /// version did not move goes as its fruit alone, and only if its
    /// levels moved (`chunk_fruit`, [`Link::next_output`]).
    fn refresh(&mut self) {
        let phase = self.shared.fruit_phase();
        let stale: Vec<ChunkPos> = self
            .sent
            .iter()
            .filter(|(chunk, held)| {
                held.phase != Some(phase) || self.shared.chunk_version(**chunk) != held.known
            })
            .map(|(chunk, _)| *chunk)
            .collect();
        for chunk in stale {
            if self.awaiting.insert(chunk) {
                self.build(chunk);
            }
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

        // The chunks whose items or fruit the client may hold wrong: sent
        // again.
        self.refresh();

        if let Some(body) = own_death {
            match shared.death_of(body) {
                Some(cause) => self.event(&ServerMessage::BodyDied {
                    body,
                    cause: Some(cause),
                }),
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
        ClientMessage::Drink { x, y } => Intention::Drink {
            body,
            at: Pos::new(x, y),
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
        // Only the brains gather (D45): no player is told "nothing ripe"
        // yet.
        // Nor "sated" (lot 3): the protocol does not tell it yet.
        sim::Refusal::Unknown | sim::Refusal::NothingRipe | sim::Refusal::Sated => Refusal::Unknown,
        sim::Refusal::OutOfReach => Refusal::OutOfReach,
        sim::Refusal::NotCarried => Refusal::NotCarried,
        sim::Refusal::TooHeavy => Refusal::TooHeavy,
        // Spoiled food is no food to whoever would eat it; the protocol
        // does not tell them apart yet.
        sim::Refusal::NotFood | sim::Refusal::Spoiled => Refusal::NotFood,
        sim::Refusal::NotWater => Refusal::NotWater,
    };
    (act, reason)
}

/// A refused drink, as its pilot is told (protocol v24).
pub fn drink_refused_view(body: BodyId, at: Pos, reason: sim::Refusal) -> ServerMessage {
    let (_, reason) = refusal_view(sim::ItemAct::Eat, reason);
    ServerMessage::DrinkRefused {
        body: body.0,
        x: at.x,
        y: at.y,
        reason,
    }
}

/// A season, on the wire.
pub fn season_view(season: sim::Season) -> Season {
    match season {
        sim::Season::Spring => Season::Spring,
        sim::Season::Summer => Season::Summer,
        sim::Season::Autumn => Season::Autumn,
        sim::Season::Winter => Season::Winter,
    }
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
pub fn condition_view(summary: (u32, u32, u32, u32, u32, bool)) -> Condition {
    let (vitality, hunger, thirst, cold, fatigue, asleep) = summary;
    Condition {
        vitality,
        hunger,
        thirst,
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
        trees: Vec<StandingTree>,
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
        Far(FarView),
    }

    fn shared(max_connections: usize) -> Shared {
        Shared::new(&players(), &Content::sample(), max_connections, 1.0)
    }

    /// A small valley (the rules are the same, and it is made in a
    /// fraction of the time) where three bodies were born at the camp,
    /// as for three players.
    #[test]
    fn a_poisoned_lock_is_recovered_not_a_second_panic() {
        let shared = Arc::new(Shared::new(&players(), &Content::sample(), 1, 1.0));
        let poisoner = Arc::clone(&shared);
        let _ = std::thread::spawn(move || {
            let _held = poisoner.intentions.lock().unwrap();
            panic!("a bug in a connection's task while it queues an intention");
        })
        .join();
        assert!(shared.intentions.is_poisoned());
        // The simulation thread's next tick goes on with what is there.
        assert_eq!(shared.take_intentions().len(), 0);
    }

    fn players() -> World {
        let mut world = World::with_valley(Calendar::default(), &Content::sample(), 768);
        for _ in 0..3 {
            world.spawn_at_camp().expect("grass by the camp");
        }
        world
    }

    const HOME: IpAddr = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
    const AWAY: IpAddr = IpAddr::V4(std::net::Ipv4Addr::new(203, 0, 113, 7));

    #[test]
    fn admissions_are_counted_and_given_back() {
        let shared = shared(2);
        let first = shared.admit(HOME).expect("first place");
        let second = shared.admit(AWAY).expect("second place");
        assert_eq!(
            shared.admit(HOME).err().map(|r| r.why),
            Some(Refused::Full(2)),
            "no third place"
        );
        assert_eq!(shared.connections(), 2);
        drop(first);
        assert_eq!(shared.connections(), 1);
        let third = shared.admit(HOME).expect("the place is free again");
        drop((second, third));
        assert_eq!(shared.connections(), 0);
        assert_eq!(shared.connections_from(HOME), 0);
    }

    /// Finding N4: an address holds at most `connections_per_address`
    /// places, whatever is left for the others.
    #[test]
    fn an_address_holds_at_most_its_places() {
        let mut shared = shared(10);
        shared.set_limits(Limits {
            connections_per_address: 2,
            ..Limits::default()
        });
        let first = shared.admit(HOME).expect("first place");
        let second = shared.admit(HOME).expect("second place");
        assert_eq!(
            shared.admit(HOME).err().map(|r| r.why),
            Some(Refused::Address(2))
        );
        assert_eq!(shared.connections_from(HOME), 2);
        let other = shared.admit(AWAY).expect("another address is served");
        drop(first);
        assert_eq!(shared.connections_from(HOME), 1);
        let third = shared
            .admit(HOME)
            .expect("the address's place is free again");
        drop((second, third, other));
        assert_eq!(shared.connections(), 0);
        assert_eq!(shared.connections_from(HOME), 0);
        assert_eq!(shared.connections_from(AWAY), 0);
    }

    /// Finding N8: an address opens at most `sessions_per_address`
    /// connections a minute, places given back or not; the minute over,
    /// it opens them again. Its first refusal of the minute is the one
    /// to log, the next are not.
    #[test]
    fn an_address_opens_at_most_its_sessions_a_minute() {
        let mut shared = shared(10);
        shared.set_limits(Limits {
            sessions_per_address: 2,
            ..Limits::default()
        });
        let now = Instant::now();
        drop(shared.admit_at(HOME, now).expect("first session"));
        drop(shared.admit_at(HOME, now).expect("second session"));
        assert_eq!(shared.connections_from(HOME), 0, "both given back");
        let Err(refused) = shared.admit_at(HOME, now + Duration::from_secs(30)) else {
            panic!("a third within the minute is refused");
        };
        assert_eq!(refused.why, Refused::Rate(2));
        assert!(refused.first, "the first refusal is logged");
        let Err(again) = shared.admit_at(HOME, now + Duration::from_secs(31)) else {
            panic!("still refused");
        };
        assert!(!again.first, "the next refusals are not logged");
        drop(
            shared
                .admit_at(AWAY, now + Duration::from_secs(31))
                .expect("another address is served"),
        );
        let later = shared
            .admit_at(HOME, now + SESSIONS_WINDOW + Duration::from_secs(1))
            .expect("the minute over, the address is served again");
        drop(later);
        assert_eq!(shared.connections(), 0);
    }

    /// Finding N3: a connection teleports at most once every
    /// `TELEPORT_EVERY`; a teleport asked sooner is dropped.
    #[test]
    fn a_throttle_lets_one_act_through_an_interval() {
        let mut throttle = Throttle::default();
        let now = Instant::now();
        assert!(throttle.allow(now, TELEPORT_EVERY), "the first at once");
        assert!(!throttle.allow(now + Duration::from_millis(100), TELEPORT_EVERY));
        assert!(!throttle.allow(now + Duration::from_millis(999), TELEPORT_EVERY));
        assert!(throttle.allow(now + TELEPORT_EVERY, TELEPORT_EVERY));
        assert!(
            !throttle.allow(
                now + TELEPORT_EVERY + Duration::from_millis(500),
                TELEPORT_EVERY
            ),
            "counted from the last let through"
        );
    }

    /// Finding N5: a client's bytes never reach the terminal as they
    /// are: control characters escaped, the text bounded.
    #[test]
    fn client_text_in_the_log_is_escaped_and_bounded() {
        let hostile = "\u{1b}]0;PWNED\u{7}\u{1b}[2J\nserver stopped\r\"quoted\"";
        let shown = loggable(hostile);
        assert!(!shown.chars().any(char::is_control), "{shown}");
        assert_eq!(
            shown,
            "\\u{1b}]0;PWNED\\u{7}\\u{1b}[2J\\nserver stopped\\r\\\"quoted\\\""
        );
        assert_eq!(loggable("plain text, accents é"), "plain text, accents é");
        let long = "x".repeat(LOGGED_TEXT_MAX * 3);
        let shown = loggable(&long);
        assert!(shown.len() <= LOGGED_TEXT_MAX + 3, "{}", shown.len());
        assert!(shown.ends_with("..."));
    }

    /// Finding N6: a connection's bad messages write a few lines, then
    /// nothing, then close it.
    #[test]
    fn complaints_are_logged_then_counted_then_too_many() {
        let mut complaints = Complaints::new(2, 5);
        assert_eq!(complaints.note(), Complaint::Logged);
        assert_eq!(complaints.note(), Complaint::Logged);
        assert_eq!(complaints.note(), Complaint::Silent);
        assert_eq!(complaints.note(), Complaint::Silent);
        assert_eq!(complaints.note(), Complaint::Silent);
        assert_eq!(complaints.note(), Complaint::TooMany);
        assert_eq!(complaints.count(), 6);
    }

    /// Finding N2, the lead's decision Q2: an upgrade carrying `Origin`
    /// is refused, one without is accepted.
    #[test]
    fn an_upgrade_with_an_origin_header_is_refused() {
        let plain = Request::builder()
            .uri("ws://127.0.0.1:7777/")
            .body(())
            .expect("a request");
        assert!(refuse_origin(&plain, Response::new(())).is_ok());
        for origin in ["https://evil.example", "null", "http://localhost:7777"] {
            let page = Request::builder()
                .uri("ws://127.0.0.1:7777/")
                .header("Origin", origin)
                .body(())
                .expect("a request");
            let refused = refuse_origin(&page, Response::new(())).expect_err(origin);
            assert_eq!(refused.status(), StatusCode::FORBIDDEN);
        }
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
        // generator sends them byte for byte. Re-pinned on 16 November
        // 2026 for 19 more shrubs and plants (1,015,322 bytes before),
        // then for six fruit and nut trees (1,225,292), then for the
        // fruit each tree shows, a byte a tree (1,226,087, D48), then for
        // the berries each shrub shows, a byte a plant (1,234,476), then
        // for protocol v30's compact frames (1,338,344 bytes before; their
        // content, decoded and written again in v29's layout, gave v29's
        // print and bytes exactly, 19 November 2026), then for the flint
        // lying on the banks and by the rock (787,681 bytes before, D57), and
        // for more of it in the gravels and by the rock (787,921 bytes before,
        // 25 November 2026), then for the birch on the banks (788,959 bytes
        // before, D37 amended 25 November 2026: trees have no generator
        // version, so the banks of every world change).
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
        assert_eq!((print, bytes), (0x0eea_d9e7_fcc4_b4e0, 789_231));
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
            let fresh: Vec<StandingTree> = sim::trees::trees_in(
                &shared.map,
                &shared.tree_kinds,
                &shared.open_ground,
                shared.seed,
                chunk,
            )
            .into_iter()
            .map(|tree| {
                let (dx, dy) = offset_in(chunk, tree.at).unwrap();
                StandingTree {
                    dx,
                    dy,
                    kind: u16::try_from(tree.species).unwrap(),
                    stage: protocol::STAGE_GROWN,
                    // The world's first day, in spring: no fruit (D48).
                    fruit: protocol::FRUIT_NONE,
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
            &shared.ground.trees_in(
                &shared.map,
                &shared.tree_kinds,
                &shared.open_ground,
                shared.seed,
                chunks[0]
            )
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
        assert_eq!(
            shared.body_died(BodyId(1), Some(DeathCause::Starved)),
            Some(here)
        );
        assert_eq!(shared.body_died(BodyId(1), Some(DeathCause::Froze)), None);
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
                    ServerMessage::ChunkBodies { x, y, bodies } => match out.last_mut() {
                        Some(Out::Chunk(chunk)) => {
                            assert_eq!((chunk.x, chunk.y), (x, y), "the bodies of their chunk");
                            assert!(chunk.bodies.is_empty() && !bodies.is_empty());
                            chunk.bodies = bodies;
                        }
                        // A chunk_same brings its bodies along as a frame does.
                        Some(Out::Message(ServerMessage::ChunkSame { x: sx, y: sy, .. })) => {
                            assert_eq!((*sx, *sy), (x, y), "the bodies of their chunk");
                        }
                        _ => panic!("bodies after anything but their chunk"),
                    },
                    message => out.push(Out::Message(message)),
                },
                Some(Message::Binary(far))
                    if protocol::binary_kind(&far) == Some(protocol::BINARY_FAR_VIEW) =>
                {
                    out.push(Out::Far(
                        protocol::decode_far_view(&far).expect("a far view"),
                    ));
                }
                Some(Message::Binary(frame)) => out.push(Out::Chunk(Sent::of(
                    &link.shared,
                    ChunkSend {
                        frame,
                        bodies: vec![],
                        version: 0,
                        phase: 0,
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
            assert!(pool.submit(
                lane,
                Box::new(move || {
                    tx.send(shared.chunk_ground(chunk).expect("on the map"))
                        .unwrap();
                }),
            ));
        }
        let mut with_bodies = 0;
        for _ in &chunks {
            let ground = rx.recv_timeout(Duration::from_secs(60)).expect("built");
            let chunk = ground.chunk;
            let pooled = Sent::of(&shared, shared.chunk_send_of(ground).unwrap().unwrap());
            let inline = sent(&shared, chunk).expect("on the map");
            assert_eq!(pooled, inline, "{chunk:?}");
            with_bodies += usize::from(!pooled.bodies.is_empty());
        }
        assert!(with_bodies >= 1, "the bodies are added as the frame leaves");
    }

    /// Finding N7: `chunk_missing` is honoured once, in answer to a
    /// `chunk_same`; asked for a chunk whose frame itself left, it is a
    /// repeat and builds nothing; asked again after the frame it brought,
    /// a repeat as well.
    /// Moves the view to `centre` and reads everything that leaves for
    /// it: what the client holds answers at once, the rest once the pool
    /// built it.
    fn arrive(
        link: &mut Link,
        results: &mut mpsc::UnboundedReceiver<Built>,
        centre: ChunkPos,
    ) -> Vec<Out> {
        link.move_view(centre);
        let mut out = drain(link);
        settle(link, results);
        out.extend(drain(link));
        out
    }

    /// The second security audit of 20 November 2026, C1: a chunk past
    /// the eyes keeps the version the client holds. Reconnecting with
    /// every frame (`hello`'s `held`) or walking back into it, the client
    /// is answered `chunk_same` though a hidden body changed the chunk's
    /// items; the frame goes when the chunk comes within range, and at
    /// dawn (the daily tell the lead accepted).
    #[test]
    fn a_chunk_past_the_eyes_keeps_the_version_the_client_holds() {
        let content = Content::sample();
        let mut world = World::with_valley(Calendar::default(), &content, 768);
        let body = world.spawn_at_camp().expect("grass by the camp");
        let home = world.body(body).expect("born").pos().chunk();
        let shared = Arc::new(Shared::new(&world, &content, 2, 1.0));
        let pool = Arc::new(ChunkPool::new(1));
        // Seven chunks east (or west), in view and past the eyes; two,
        // within them.
        let dir = if shared.map.has_chunk(ChunkPos::new(home.x + 7, home.y)) {
            1
        } else {
            -1
        };
        let far = ChunkPos::new(home.x + 7 * dir, home.y);
        let near = ChunkPos::new(home.x + 2 * dir, home.y);
        let (mut first, mut results) = link(&shared, &pool, Some(body.0));
        arrive(&mut first, &mut results, home);
        assert!(first.view.chunks.contains(&far) && !first.range.contains(&far));
        assert!(first.range.contains(&near));
        let held = protocol::HeldChunks {
            world: shared.world_id().to_owned(),
            chunks: first
                .sent
                .iter()
                .map(|(c, h)| (c.x, c.y, h.frame))
                .collect(),
        };
        let frames_of = |out: &[Out]| -> Vec<(i32, i32)> {
            out.iter()
                .filter_map(|o| match o {
                    Out::Chunk(c) => Some((c.x, c.y)),
                    _ => None,
                })
                .collect()
        };
        let same_at = |out: &[Out], chunk: ChunkPos, at: Version| {
            out.iter().any(|o| {
                matches!(o, Out::Message(ServerMessage::ChunkSame { x, y, version })
                    if (*x, *y, *version) == (chunk.x, chunk.y, at))
            })
        };

        // A hidden body changes the items of both chunks. Reconnecting
        // with every frame: the chunk in sight comes fresh, the one past
        // the eyes as the client holds it.
        shared.crop_picked(far, &BTreeMap::new(), 1);
        shared.crop_picked(near, &BTreeMap::new(), 1);
        let (mut again, mut results) = link(&shared, &pool, Some(body.0));
        again.held(&held);
        let out = arrive(&mut again, &mut results, home);
        assert_eq!(
            frames_of(&out),
            [(near.x, near.y)],
            "the chunk in sight alone"
        );
        assert!(
            same_at(&out, far, 0),
            "past the eyes: as the client holds it"
        );
        assert_eq!(again.sent[&far].known, 0, "what the client knows");

        // Walking away (the chunk leaves the view) and back: the same.
        shared.crop_picked(far, &BTreeMap::new(), 2);
        let away = ChunkPos::new(home.x - 3 * dir, home.y);
        arrive(&mut again, &mut results, away);
        assert!(!again.sent.contains_key(&far), "out of view");
        let out = arrive(&mut again, &mut results, home);
        assert_eq!(frames_of(&out), [], "nothing fresh on the way back");
        assert!(same_at(&out, far, 0));

        // Coming within range: the frame, as it is now.
        let close = ChunkPos::new(home.x + 4 * dir, home.y);
        let before: BTreeSet<ChunkPos> = again.sent.keys().copied().collect();
        let out = arrive(&mut again, &mut results, close);
        assert!(again.range.contains(&far));
        let fresh: Vec<(i32, i32)> = frames_of(&out)
            .into_iter()
            .filter(|&(x, y)| before.contains(&ChunkPos::new(x, y)))
            .collect();
        assert_eq!(
            fresh,
            [(far.x, far.y)],
            "within the eyes now, the rest held"
        );
        assert_eq!(again.sent[&far].known, 2);

        // Out of range again and changed since: the dawn refreshes it.
        arrive(&mut again, &mut results, home);
        shared.crop_picked(far, &BTreeMap::new(), 3);
        again.refresh();
        settle(&mut again, &mut results);
        assert_eq!(frames_of(&drain(&mut again)), [(far.x, far.y)], "at dawn");
    }

    /// The second security audit of 20 November 2026, A3: an item change
    /// is told within 150 m of the body, in metres, as `World::sight`
    /// judges an act on it, whatever the chunk.
    #[test]
    fn an_item_change_is_told_within_150_m_in_metres_whatever_the_chunk() {
        let content = Content::sample();
        let mut world = World::with_valley(Calendar::default(), &content, 768);
        let body = world.spawn_at_camp().expect("grass by the camp");
        let at = world.body(body).expect("born").pos();
        let shared = Arc::new(Shared::new(&world, &content, 2, 1.0));
        let pool = Arc::new(ChunkPool::new(1));
        let (mut pilot, mut results) = link(&shared, &pool, Some(body.0));
        arrive(&mut pilot, &mut results, at.chunk());
        assert_eq!(pilot.eye(), at);
        // East or west, so that 147 m away lies in the fifth chunk: past a
        // disc of chunk centres (four chunks), within the eyes.
        let side = i32::try_from(CHUNK_SIZE).expect("small");
        let dir = if at.x.rem_euclid(side) >= 13 { 1 } else { -1 };
        let near = Pos::new(at.x + 147 * dir, at.y);
        let far = Pos::new(at.x + 151 * dir, at.y);
        assert_eq!((near.chunk().x - at.chunk().x).abs(), 5);
        assert_eq!(
            near.chunk(),
            far.chunk(),
            "one chunk, either side of the line"
        );
        assert!(pilot.view.chunks.contains(&near.chunk()), "in view");
        let removed = |out: &[Out]| {
            out.iter()
                .any(|m| matches!(m, Out::Message(ServerMessage::ItemRemoved { .. })))
        };
        let told = |pilot: &mut Link, item: u64, at: Pos| {
            pilot.forward(Published::ItemRemoved {
                item,
                at,
                version: 1,
            });
            removed(&drain(pilot))
        };
        assert!(told(&mut pilot, 7, near), "147 m: told");
        assert!(!told(&mut pilot, 8, far), "151 m: not told");
        // On the diagonal, 155 m away in the fourth chunk: its centre
        // was within the disc of chunk centres; the tile is not.
        let dy = if shared
            .map
            .has_chunk(ChunkPos::new(at.chunk().x, at.chunk().y + 4))
        {
            1
        } else {
            -1
        };
        let diagonal = Pos::new(at.x + 110 * dir, at.y + 110 * dy);
        assert!((diagonal.chunk().x - at.chunk().x).abs() <= 4);
        assert!(
            !told(&mut pilot, 9, diagonal),
            "155 m on the diagonal: not told"
        );
        assert!(
            told(&mut pilot, 10, Pos::new(at.x + 100 * dir, at.y + 100 * dy)),
            "141 m"
        );
    }

    /// Lot F's F1 (the second security audit of 20 November 2026): a dawn
    /// that sends a chunk again while its `chunk_same` waits for the
    /// client's answer must not forget that a `chunk_missing` is owed.
    #[test]
    fn a_dawn_does_not_forget_a_chunk_same_still_waiting_for_its_answer() {
        let world = players();
        let shared = Arc::new(Shared::new(&world, &Content::sample(), 4, 1.0));
        let pool = Arc::new(ChunkPool::new(1));
        let (mut link, mut results) = link(&shared, &pool, None);
        let home = link.home();
        arrive(&mut link, &mut results, home);
        // Away and back: the frames the client kept answer as chunk_same.
        arrive(
            &mut link,
            &mut results,
            ChunkPos::new(home.x + VIEW_RADIUS + 3, home.y),
        );
        let out = arrive(&mut link, &mut results, home);
        assert!(out.iter().any(|o| matches!(o, Out::Message(ServerMessage::ChunkSame { x, y, .. }) if (*x, *y) == (home.x, home.y))));
        assert!(link.sent[&home].told, "a chunk_missing is owed");
        // A dawn before the answer comes, the fruit phase not the one the
        // chunk was sent in: the chunk is looked at again, nothing moved.
        link.sent.get_mut(&home).expect("held").phase = Some(u64::MAX);
        link.refresh();
        settle(&mut link, &mut results);
        let out = drain(&mut link);
        assert!(out.is_empty(), "nothing moved: no frame, no fruit: {out:?}");
        assert!(
            link.sent[&home].told,
            "the chunk_same still waits for its answer"
        );
        assert!(link.missing(home), "honoured");
    }

    /// The second security audit of 20 November 2026, A2: an address's
    /// bad-message lines are budgeted by the minute over all its
    /// connections, so a loop of connect, complain and leave stops filling
    /// the log after three connections' worth.
    #[test]
    fn an_address_s_bad_messages_are_logged_within_its_budget_of_the_minute() {
        let shared = shared(10);
        let start = Instant::now();
        let mut written = 0;
        for session in 0..5_u32 {
            let admission = shared.admit_at(HOME, start).expect("admitted");
            for _ in 0..COMPLAINTS_LOGGED {
                if shared.complaint_logged_at(HOME, start) {
                    written += 1;
                }
            }
            drop(admission);
            if session < 3 {
                assert_eq!(written, (session + 1) * COMPLAINTS_LOGGED, "its own lines");
            }
        }
        assert_eq!(
            written, COMPLAINTS_LOGGED_PER_ADDRESS,
            "three connections' worth, then nothing"
        );
        // Another address has its own budget.
        let other = shared.admit_at(AWAY, start).expect("admitted");
        assert!(shared.complaint_logged_at(AWAY, start));
        drop(other);
        // The window over: the budget is whole again.
        let later = start + SESSIONS_WINDOW;
        let _admission = shared.admit_at(HOME, later).expect("admitted");
        assert!(shared.complaint_logged_at(HOME, later));
    }

    #[test]
    fn a_chunk_missing_is_honoured_once_an_answer_and_never_for_a_frame_sent() {
        let world = players();
        let shared = Arc::new(Shared::new(&world, &Content::sample(), 4, 1.0));
        let pool = Arc::new(ChunkPool::new(1));
        let (mut first, mut results) = link(&shared, &pool, None);
        let centre = shared.centre_chunk();
        first.move_view(centre);
        settle(&mut first, &mut results);
        let out = drain(&mut first);
        let frames = out.iter().filter(|o| matches!(o, Out::Chunk(_))).count();
        assert!(frames > 0, "the view's frames left");
        assert!(first.sent.values().all(|held| !held.told));

        // A chunk_missing for a chunk whose frame left: a repeat.
        assert!(!first.missing(centre), "a frame was sent: not honoured");
        assert_eq!(first.awaiting, BTreeSet::new(), "nothing is built again");
        assert!(first.sent.contains_key(&centre));
        // For a chunk out of view: a late answer, nothing wrong.
        let far = ChunkPos::new(centre.x + 100, centre.y + 100);
        assert!(first.missing(far));

        // The same view again on a new link that holds every frame: each
        // chunk goes as chunk_same, which one chunk_missing may answer.
        let (mut again, mut results) = link(&shared, &pool, None);
        let held = protocol::HeldChunks {
            world: shared.world_id().to_owned(),
            chunks: first
                .sent
                .iter()
                .map(|(c, h)| (c.x, c.y, h.frame))
                .collect(),
        };
        again.held(&held);
        again.move_view(centre);
        // Nothing of the pool: every chunk goes as chunk_same, as it leaves.
        let out = drain(&mut again);
        assert!(
            out.iter().all(|o| !matches!(o, Out::Chunk(_))),
            "no frame: the client holds them all"
        );
        assert!(again.sent.values().all(|held| held.told));
        assert!(again.missing(centre), "an answer to chunk_same: honoured");
        assert!(again.awaiting.contains(&centre), "the frame is built");
        assert!(!again.missing(centre), "a second chunk_missing is a repeat");
        settle(&mut again, &mut results);
        let out = drain(&mut again);
        assert_eq!(
            out.iter()
                .filter(|o| matches!(o, Out::Chunk(c) if (c.x, c.y) == (centre.x, centre.y)))
                .count(),
            1,
            "the frame, once"
        );
        assert!(!again.sent[&centre].told);
        assert!(
            !again.missing(centre),
            "and after the frame, a repeat again"
        );
    }

    #[test]
    fn the_far_view_is_counted_on_the_pool_held_and_only_the_latest_sent() {
        let shared = Arc::new(shared(1));
        // One thread: the jobs run in the order given.
        let pool = Arc::new(ChunkPool::new(1));
        let (mut link, mut results) = link(&shared, &pool, Some(1));
        let home = link.home();
        let away = ChunkPos::new(home.x + FAR_RESEND + 1, home.y);
        let (home, away) = (far_centre(home), far_centre(away));
        assert_ne!(home, away, "two squares of the grid");
        // Asked for, nothing waits in the ground lane: the network's
        // thread counted nothing.
        link.far_view(home);
        assert!(link.ground.is_empty() && link.far_awaited == Some(home));
        // A newer one asked before the first is back: the first is
        // dropped when it comes.
        link.far_view(away);
        settle(&mut link, &mut results);
        let far: Vec<FarView> = drain(&mut link)
            .into_iter()
            .filter_map(|out| match out {
                Out::Far(far) => Some(far),
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
                .unwrap_or_else(PoisonError::into_inner)
                .contains(&c)
        };
        assert!(held(home) && held(away));
        let first = shared.far_view_message(away).expect("encodable");
        let again = shared.far_view_message(away).expect("encodable");
        assert_eq!(first, again);
        let Message::Binary(first) = first else {
            panic!("a binary message");
        };
        assert_eq!(
            protocol::decode_far_view(&first).expect("the protocol"),
            far_view(&shared.map, away)
        );
    }

    #[test]
    fn the_pool_looks_in_the_frame_cache_before_making_a_chunk_again() {
        let shared = shared(1);
        let chunk = shared.position_of(1).expect("body 1").chunk();
        let send = |ground: ChunkGround| shared.chunk_send_of(ground).expect("framed");
        // Cold: made whole, framed, held.
        let cold = shared.chunk_ground_cached(chunk).expect("on the map");
        assert!(cold.made.is_some(), "nothing held yet");
        let first = send(cold).expect("made whole");
        // Warm: nothing made again, the very same bytes.
        let warm = shared.chunk_ground_cached(chunk).expect("on the map");
        assert!(warm.made.is_none(), "its frame is held: nothing made again");
        let again = send(warm.clone()).expect("held");
        assert_eq!(
            again.frame.as_ptr(),
            first.frame.as_ptr(),
            "the very same bytes"
        );
        assert_eq!(
            Sent::of(&shared, again),
            Sent::of(&shared, shared.chunk_message(chunk).expect("a chunk"))
        );
        // The chunk changes before the warm ground leaves: it is to be
        // built again, whole, and then framed at its new version.
        shared.crop_picked(chunk, &BTreeMap::new(), 1);
        assert!(send(warm).is_none(), "its frame is not held any more");
        let changed = shared.chunk_ground_cached(chunk).expect("on the map");
        assert!(changed.made.is_some(), "a new version: made whole");
        let changed = send(changed).expect("made whole");
        assert_eq!(changed.version, 1);
        assert_ne!(changed.frame, first.frame);
    }

    #[test]
    fn a_chunk_whose_held_frame_went_meanwhile_is_built_again_whole() {
        let shared = Arc::new(shared(1));
        let pool = Arc::new(ChunkPool::new(1));
        let (mut link, mut results) = link(&shared, &pool, Some(1));
        let home = link.home();
        link.move_view(home);
        settle(&mut link, &mut results);
        drain(&mut link);
        // The pool found its frame held; the chunk changed before it left.
        let held = shared.chunk_ground_cached(home).expect("on the map");
        assert!(held.made.is_none());
        shared.crop_picked(home, &BTreeMap::new(), 1);
        link.ground.push_back(Outgoing::Chunk(held));
        assert!(
            link.next_output().expect("encodable").is_none(),
            "nothing sent"
        );
        assert!(link.awaiting.contains(&home), "asked for again");
        settle(&mut link, &mut results);
        let out = drain(&mut link);
        assert!(
            matches!(out.as_slice(), [Out::Chunk(c)] if (c.x, c.y) == (home.x, home.y)),
            "its frame, at its new version: {out:?}"
        );
        assert_eq!(link.sent.get(&home).map(|h| h.frame), Some(1));
    }

    #[test]
    fn the_far_view_is_one_for_a_square_of_the_grid_and_its_cells_never_move() {
        let shared = shared(1);
        let map = &shared.map;
        let home = far_centre(shared.position_of(1).expect("body 1").chunk());
        let cells = |m: &FarView| {
            (
                u32::from(m.cell),
                m.x,
                m.y,
                u32::from(m.width),
                m.cells.clone(),
            )
        };
        let size = i32::try_from(CHUNK_SIZE / FAR_CELL).expect("small");
        for (dx, dy) in [
            (-8, -8),
            (-1, 3),
            (0, 0),
            (7, 7),
            (8, 8),
            (-9, 0),
            (23, -17),
        ] {
            let chunk = ChunkPos::new(home.x + dx, home.y + dy);
            let centre = far_centre(chunk);
            // Every chunk of a square: one centre, so one message, held
            // once and the very same bytes for every body there.
            let square = |c: i32| c.div_euclid(FAR_GRID);
            assert_eq!(
                (square(centre.x), square(centre.y)),
                (square(chunk.x), square(chunk.y))
            );
            assert_eq!(centre, far_centre(centre), "{chunk:?}");
            let shared_bytes = shared
                .far_view_message(far_centre(chunk))
                .expect("encodable");
            assert_eq!(
                shared_bytes,
                shared.far_view_message(centre).expect("encodable"),
                "{chunk:?}"
            );
            // A far view centred on the body's own chunk, as before the
            // grid, and the grid's: each cell they both hold shows the
            // same land (the picture does not change).
            let (cell, ox, oy, width, own) = cells(&far_view(map, chunk));
            let (_, gx, gy, _, grid) = cells(&far_view(map, centre));
            assert_eq!(cell, FAR_CELL);
            assert_eq!(
                ((gx - ox) % size, (gy - oy) % size),
                (0, 0),
                "on whole chunks"
            );
            let w = i32::try_from(width).expect("small");
            let (own, grid) = (own.as_slice(), grid.as_slice());
            let mut shared_cells = 0;
            for j in 0..w {
                for i in 0..w {
                    let (u, v) = (ox + i - gx, oy + j - gy);
                    if (0..w).contains(&u) && (0..w).contains(&v) {
                        let at = |x: i32, y: i32| usize::try_from(y * w + x).expect("in");
                        assert_eq!(own[at(i, j)], grid[at(u, v)], "{chunk:?} cell {i},{j}");
                        shared_cells += 1;
                    }
                }
            }
            assert!(shared_cells >= (w - 2 * 9 * size).pow(2), "{chunk:?}");
        }
        assert_eq!(
            shared
                .far_views
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .len(),
            [(-8, -8), (8, 8), (-9, 0), (23, -17)]
                .iter()
                .map(|&(dx, dy)| far_centre(ChunkPos::new(home.x + dx, home.y + dy)))
                .collect::<BTreeSet<_>>()
                .len(),
            "counted once a square"
        );
    }

    #[test]
    fn the_far_view_always_reaches_well_past_where_the_camera_stops() {
        // The body walks; the far view is sent again once it is more than
        // FAR_RESEND chunks from its centre. Wherever it stands, the far
        // view reaches past the camera's stop (the view's 288 tiles and
        // fog A's 5) by at least as much as before the grid, when it was
        // centred on the body's own chunk.
        let half = i32::try_from(FAR_CELLS * FAR_CELL / 2).expect("small");
        let size = i32::try_from(CHUNK_SIZE).expect("small");
        let mut sent = far_centre(ChunkPos::new(0, 0));
        let mut least = i32::MAX;
        for x in 0..2000 {
            let chunk = ChunkPos::new(x / 3, x / 7);
            if (sent.x - chunk.x).abs().max((sent.y - chunk.y).abs()) > FAR_RESEND {
                sent = far_centre(chunk);
            }
            // From the farthest tile of the body's chunk to the edge.
            let reach = |c: i32, body: i32| half - (c - body).abs() * size - size / 2;
            least = least
                .min(reach(sent.x, chunk.x))
                .min(reach(sent.y, chunk.y));
        }
        // Before the grid: FAR_RESEND chunks off at worst.
        let before = half - FAR_RESEND * size - size;
        assert!(least >= before, "{least} tiles at worst, {before} before");
        assert!(least > 288 + 5, "past the camera's stop");
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
        shared.body_died(BodyId(1), Some(DeathCause::Starved));

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
        assert!(matches!(out.last(), Some(Out::Far(_))));
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

    /// The fruit a world of the repository's content shows (D48): its
    /// shared state, and the tick of the middle of the first autumn's
    /// second day.
    fn fruiting() -> (World, Arc<Shared>, Tick) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = crate::content::load(&dir).expect("repository content");
        let world = World::with_valley(Calendar::default(), &content, 768);
        let shared = Arc::new(Shared::new(&world, &content, 1, 1.0));
        let calendar = *world.calendar();
        let day = calendar.ticks_per_day();
        let autumn = Tick(day * (u64::from(calendar.days_per_cycle()) / 2 + 1) + day / 2);
        (world, shared, autumn)
    }

    #[test]
    fn a_frame_shows_the_fruit_of_its_trees_as_of_now_and_of_the_picks() {
        let (world, shared, autumn) = fruiting();
        let oak = shared
            .tree_kinds
            .iter()
            .position(|s| s.id == "oak")
            .unwrap();
        let hazel = shared
            .tree_kinds
            .iter()
            .position(|s| s.id == "hazel")
            .unwrap();
        let chunk = View::window(&shared.map, shared.map.camp().chunk())
            .into_iter()
            .find(|&c| {
                let trees = world.trees_in(c);
                trees.iter().any(|t| t.species == oak) && trees.iter().any(|t| t.species == hazel)
            })
            .expect("oaks and hazels around the camp");
        let fruit = |shared: &Shared, kind: usize| -> Vec<u8> {
            let sent = shared.chunk_message(chunk).unwrap();
            protocol::decode_chunk(&sent.frame)
                .unwrap()
                .trees
                .iter()
                .filter(|t| usize::from(t.kind) == kind)
                .map(|t| t.fruit)
                .collect()
        };
        // Spring: no fruit anywhere.
        let spring = shared.chunk_message(chunk).unwrap();
        assert!(
            fruit(&shared, oak)
                .iter()
                .all(|&f| f == protocol::FRUIT_NONE)
        );
        // Autumn: the acorns ripening, all of a level, and the hazelnuts
        // as much as each bears; a frame of another phase, encoded anew.
        shared.tick.store(autumn.0, Ordering::Relaxed);
        let laden = shared.chunk_message(chunk).unwrap();
        assert_ne!(laden.phase, spring.phase);
        assert_ne!(laden.frame, spring.frame);
        let acorns = fruit(&shared, oak);
        assert!(acorns[0] > protocol::FRUIT_NONE && acorns.iter().all(|&f| f == acorns[0]));
        let hazels = fruit(&shared, hazel);
        assert!(
            hazels.iter().all(|&f| f != protocol::FRUIT_NONE),
            "{hazels:?}"
        );
        // A hazel picked bare shows none, in a frame of the chunk's new
        // version.
        let picked_tree = world
            .trees_in(chunk)
            .iter()
            .find(|t| t.species == hazel)
            .unwrap()
            .key;
        let picks = BTreeMap::from([(
            (Source::Tree(picked_tree), "hazelnuts".to_owned()),
            Picked {
                year: 1,
                grams: u32::MAX,
            },
        )]);
        shared.crop_picked(chunk, &picks, 1);
        let after = shared.chunk_message(chunk).unwrap();
        assert_eq!(after.version, 1);
        let now = fruit(&shared, hazel);
        assert_eq!(now[0], protocol::FRUIT_NONE, "picked bare");
        assert_eq!(now[1..], hazels[1..], "the others as they were");
    }

    #[test]
    fn a_frame_shows_the_berries_of_its_shrubs_as_of_now_and_of_the_picks() {
        let (world, shared, _) = fruiting();
        let calendar = *world.calendar();
        let day = calendar.ticks_per_day();
        // Late summer: the blackberries are ripe, and the elderberries no
        // one gathers here (D30) on their plateau.
        let late_summer = Tick(day * (u64::from(calendar.days_per_cycle()) * 45 / 100) + day / 2);
        let wire = |id: &str| {
            let index = shared.flora.iter().position(|k| k.id == id).unwrap();
            u16::try_from(index).unwrap()
        };
        let (bramble, elder) = (wire("bramble"), wire("elder"));
        let plants = |chunk: ChunkPos, kind: u16| -> Vec<Standing> {
            let sent = shared.chunk_message(chunk).unwrap();
            protocol::decode_chunk(&sent.frame)
                .unwrap()
                .plants
                .into_iter()
                .filter(|p| p.kind == kind)
                .collect()
        };
        let fruit =
            |chunk, kind| -> Vec<u8> { plants(chunk, kind).iter().map(|p| p.fruit).collect() };
        let window = View::window(&shared.map, shared.map.camp().chunk());
        let with = |kind| {
            window
                .iter()
                .copied()
                .find(|&c| !plants(c, kind).is_empty())
                .expect("around the camp")
        };
        let (brambles, elders) = (with(bramble), with(elder));
        // Spring: no berries.
        assert!(
            fruit(brambles, bramble)
                .iter()
                .all(|&f| f == protocol::FRUIT_NONE)
        );
        assert!(
            fruit(elders, elder)
                .iter()
                .all(|&f| f == protocol::FRUIT_NONE)
        );
        shared.tick.store(late_summer.0, Ordering::Relaxed);
        let on = fruit(brambles, bramble);
        assert!(on.iter().all(|&f| f > protocol::FRUIT_NONE), "{on:?}");
        assert!(
            fruit(elders, elder)
                .iter()
                .all(|&f| f == protocol::FRUIT_LEVELS)
        );
        // A bramble picked bare shows none, in a frame of the chunk's new
        // version; the others as they were.
        let first = plants(brambles, bramble)[0];
        let key = sim::crops::PlantKey {
            x_mm: first.dx,
            y_mm: first.dy,
        };
        let picks = BTreeMap::from([(
            (Source::Plant(key), "blackberries".to_owned()),
            Picked {
                year: 1,
                grams: u32::MAX,
            },
        )]);
        shared.crop_picked(brambles, &picks, 1);
        let now = fruit(brambles, bramble);
        assert_eq!(now[0], protocol::FRUIT_NONE, "picked bare: lower");
        assert_eq!(now[1..], on[1..]);
    }

    #[test]
    fn the_world_identity_tells_nothing_of_the_seed() {
        // The security audit of 20 November 2026, K1: no hash of the
        // seed on the wire. Two valleys of different seeds, the same
        // content, the same lineage: the same identity; what tells
        // worlds apart is their birth, random.
        let content = Content::sample();
        let of_seed = |seed: u64| {
            let map = Map::valley(256, 256, seed);
            let world = World::on_valley(Calendar::default(), &content, map);
            Shared::new(&world, &content, 1, 1.0)
        };
        let (mut a, mut b) = (of_seed(1), of_seed(0x5eed_5eed_5eed_5eed));
        assert_eq!(a.world_id(), b.world_id());
        a.fold_lineage(5, 0);
        b.fold_lineage(5, 0);
        assert_eq!(a.world_id(), b.world_id());
        b.fold_lineage(5, 1);
        assert_ne!(a.world_id(), b.world_id(), "a crash is another world");
        // Ephemeral worlds (no lineage) are each a world of their own.
        let (mut c, mut d) = (of_seed(1), of_seed(1));
        c.fold_lineage(0, 0);
        d.fold_lineage(0, 0);
        assert_ne!(c.world_id(), d.world_id());
        assert_eq!(c.world_id().len(), 16);
    }

    #[test]
    fn a_death_of_a_cause_the_protocol_lacks_is_told_without_it_even_to_its_pilot() {
        // The life rule's deaths (D49) stay off the wire until births come
        // to the game: the protocol does not change, the pilot is told as
        // a witness would be.
        assert_eq!(wire_cause(sim::Death::Starved), Some(DeathCause::Starved));
        for cause in [
            sim::Death::Childbirth,
            sim::Death::Illness,
            sim::Death::OldAge,
        ] {
            assert_eq!(wire_cause(cause), None, "{cause:?}");
        }
        let content = Content::sample();
        let mut world = World::with_valley(Calendar::default(), &content, 768);
        let body = world.spawn_at_camp().expect("grass by the camp");
        let at = world.body(body).expect("born").pos();
        let shared = Arc::new(Shared::new(&world, &content, 2, 1.0));
        let pool = Arc::new(ChunkPool::new(1));
        let (mut pilot, mut results) = link(&shared, &pool, Some(body.0));
        pilot.move_view(at.chunk());
        settle(&mut pilot, &mut results);
        drain(&mut pilot);
        assert_eq!(
            shared.body_died(body, wire_cause(sim::Death::OldAge)),
            Some(at)
        );
        assert_eq!(shared.death_of(body.0), None, "nothing to tell later");
        pilot.forward(Published::BodyDied {
            body: body.0,
            at,
            cause: None,
        });
        let out = drain(&mut pilot);
        assert!(
            out.iter().any(|m| matches!(
                m,
                Out::Message(ServerMessage::BodyDied { body: b, cause: None }) if *b == body.0
            )),
            "{out:?}"
        );
    }

    #[test]
    fn a_witness_is_told_a_death_without_its_cause_and_the_ground_within_the_eyes_only() {
        // The security audit of 20 November 2026: M2, the cause of a
        // death is the pilot's alone; F6, what men do to the ground is
        // told as far as the bodies are, not the whole view.
        let content = Content::sample();
        let mut world = World::with_valley(Calendar::default(), &content, 768);
        let body = world.spawn_at_camp().expect("grass by the camp");
        let at = world.body(body).expect("born").pos();
        let shared = Arc::new(Shared::new(&world, &content, 2, 1.0));
        let pool = Arc::new(ChunkPool::new(1));
        let (mut witness, mut results) = link(&shared, &pool, None);
        witness.move_view(at.chunk());
        settle(&mut witness, &mut results);
        drain(&mut witness);
        let (mut pilot, mut results) = link(&shared, &pool, Some(body.0));
        pilot.move_view(at.chunk());
        settle(&mut pilot, &mut results);
        drain(&mut pilot);

        let death = Published::BodyDied {
            body: body.0,
            at,
            cause: Some(DeathCause::Starved),
        };
        witness.forward(death.clone());
        pilot.forward(death);
        let died = |out: &[Out]| {
            out.iter().find_map(|m| match m {
                Out::Message(ServerMessage::BodyDied { body: b, cause }) if *b == body.0 => {
                    Some(*cause)
                }
                _ => None,
            })
        };
        assert_eq!(died(&drain(&mut witness)), Some(None), "told, not why");
        assert_eq!(died(&drain(&mut pilot)), Some(Some(DeathCause::Starved)));

        // A tile in view but past the eyes: seven chunks east.
        let far = Pos::new(at.x + 7 * i32::try_from(CHUNK_SIZE).unwrap(), at.y);
        assert!(witness.view.chunks.contains(&far.chunk()), "in view");
        assert!(!witness.sees(far), "past the eyes");
        let removed = |out: &[Out]| {
            out.iter()
                .any(|m| matches!(m, Out::Message(ServerMessage::ItemRemoved { .. })))
        };
        witness.forward(Published::ItemRemoved {
            item: 7,
            at,
            version: 1,
        });
        assert!(removed(&drain(&mut witness)), "within the eyes: told");
        witness.forward(Published::ItemRemoved {
            item: 8,
            at: far,
            version: 1,
        });
        assert!(!removed(&drain(&mut witness)), "past the eyes: nothing");
    }

    #[test]
    fn a_link_sends_again_at_dawn_the_chunks_whose_fruit_changed() {
        let (_world, shared, autumn) = fruiting();
        let pool = Arc::new(ChunkPool::new(3));
        let (mut link, mut results) = link(&shared, &pool, None);
        let home = link.home();
        link.move_view(home);
        settle(&mut link, &mut results);
        drain(&mut link);
        let total = link.sent.len();
        let dawn = |link: &mut Link, day: u64| {
            let tick = shared.tick.load(Ordering::Relaxed);
            link.forward(Published::DayStarted { day, tick });
        };
        // A dawn with the same fruit sends nothing again.
        dawn(&mut link, 1);
        assert!(link.awaiting.is_empty());
        // The first dawn of autumn looks at the whole view again, once,
        // and sends no frame: the fruit alone of the chunks whose levels
        // moved (v29), as a frame made now shows them.
        shared.tick.store(autumn.0, Ordering::Relaxed);
        dawn(&mut link, 26);
        assert_eq!(link.awaiting.len(), total);
        settle(&mut link, &mut results);
        let out = drain(&mut link);
        let frames = out.iter().filter(|o| matches!(o, Out::Chunk(_))).count();
        assert_eq!(frames, 0, "no frame: nothing but the fruit moved");
        let fruit = fruit_sent(&shared, &out);
        assert_ne!(fruit.len(), 0, "the levels of some chunks moved");
        let dawns = |out: &[Out]| {
            out.iter()
                .filter(|o| matches!(o, Out::Message(ServerMessage::DayStarted { .. })))
                .count()
        };
        assert_eq!(fruit.len() + dawns(&out), out.len(), "nothing else");
        dawn(&mut link, 27);
        assert!(link.awaiting.is_empty(), "nothing changed since");
        // A dawn whose phase moved but none of the levels in view (most
        // chunks on many dawns): looked at again, nothing sent.
        for held in link.sent.values_mut() {
            held.phase = Some(0);
        }
        dawn(&mut link, 27);
        assert_eq!(link.awaiting.len(), total);
        settle(&mut link, &mut results);
        let out = drain(&mut link);
        assert_eq!(out.len(), dawns(&out), "the dawn alone");
        // A crop picked moves the chunk's version, which no event told the
        // client: its chunk alone is sent again at the next dawn.
        shared.crop_picked(home, &BTreeMap::new(), 1);
        dawn(&mut link, 28);
        assert_eq!(link.awaiting.iter().copied().collect::<Vec<_>>(), [home]);
        settle(&mut link, &mut results);
        drain(&mut link);

        // A client back with the chunks it kept: `chunk_same`, their fruit
        // unknown, so they are sent again at the next dawn.
        let (mut back, mut results) = link_with_held(&shared, &pool);
        let home = back.home();
        back.move_view(home);
        // The kept ones leave as they are drained, the picked one once
        // built.
        let mut out = drain(&mut back);
        settle(&mut back, &mut results);
        out.extend(drain(&mut back));
        let same = out
            .iter()
            .filter(|o| matches!(o, Out::Message(ServerMessage::ChunkSame { .. })))
            .count();
        assert_eq!(same, back.view.len() - 1, "all kept but the picked one");
        dawn(&mut back, 29);
        assert_eq!(back.awaiting.len(), same);
        settle(&mut back, &mut results);
        // Their fruit not known: told, each, but no frame.
        let out = drain(&mut back);
        assert_eq!(fruit_sent(&shared, &out).len(), same);
        assert_eq!(out.len(), same + dawns(&out), "no frame");
        dawn(&mut back, 30);
        assert!(back.awaiting.is_empty());
    }

    /// The chunks whose fruit alone was sent (`chunk_fruit`) in `out`,
    /// each checked against a frame of it made now: the same levels, and
    /// the version the client holds (0 here: no item moved).
    fn fruit_sent(shared: &Shared, out: &[Out]) -> Vec<ChunkPos> {
        out.iter()
            .filter_map(|o| match o {
                Out::Message(ServerMessage::ChunkFruit {
                    x,
                    y,
                    version,
                    trees,
                    plants,
                }) => {
                    let chunk = ChunkPos::new(*x, *y);
                    let frame = shared.chunk_message(chunk).unwrap().frame;
                    let (t, p) = protocol::frame_fruit(&frame).unwrap();
                    assert_eq!(protocol::fruit_levels(trees), Some(t), "{chunk:?}");
                    assert_eq!(protocol::fruit_levels(plants), Some(p), "{chunk:?}");
                    assert_eq!(*version, 0);
                    Some(chunk)
                }
                _ => None,
            })
            .collect()
    }

    /// A watcher's link whose client kept every chunk of the view of the
    /// map's middle at version 0 (`hello`'s `held`).
    fn link_with_held(
        shared: &Arc<Shared>,
        pool: &Arc<ChunkPool>,
    ) -> (Link, mpsc::UnboundedReceiver<Built>) {
        let (mut link, results) = link(shared, pool, None);
        let chunks = View::window(&shared.map, link.home())
            .into_iter()
            .map(|c| (c.x, c.y, 0))
            .collect();
        link.held(&protocol::HeldChunks {
            world: shared.world_id().to_owned(),
            chunks,
        });
        (link, results)
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
        shared.body_died(BodyId(other), Some(DeathCause::Starved));
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
        shared.body_died(BodyId(1), Some(DeathCause::Froze));
        link.resync(2);
        let out = drain(&mut link);
        assert!(out.iter().any(|m| matches!(
            m,
            Out::Message(ServerMessage::BodyDied {
                body: 1,
                cause: Some(DeathCause::Froze)
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
        // The pool's path: the frame cache looked at first.
        let time_cached = |chunks: &BTreeSet<ChunkPos>| -> Vec<Duration> {
            chunks
                .iter()
                .map(|&chunk| {
                    let started = Instant::now();
                    let ground = shared.chunk_ground_cached(chunk).expect("on the map");
                    let message = shared.chunk_send_of(ground).expect("framed");
                    let took = started.elapsed();
                    std::hint::black_box(message);
                    took
                })
                .collect()
        };
        report("login view, cold", time_each(&login));
        // Alternated: made whole (as before the cache was looked at
        // first), then the pool's path.
        for _ in 0..3 {
            report("login view, warm, made whole", time_each(&login));
            report("login view, warm, cache first", time_cached(&login));
        }
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
