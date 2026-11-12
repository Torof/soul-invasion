//! Client <-> server protocol for Soul Invasion.
//!
//! Architecture decisions D25 and D26: WebSocket transport, JSON encoding
//! confined to this crate (and to one module on the client side), every
//! message carries its protocol version. Clients only ever send
//! *intentions*; the server only ever sends what the player's body perceives.
//!
//! The ground travels in chunks (D08, D31): after [`ServerMessage::Welcome`]
//! and [`ServerMessage::WorldState`], the client receives one chunk frame
//! per chunk around its body, then more as the body walks, and
//! [`ServerMessage::View`] for the window it holds: those outside are left
//! behind. A chunk the client still holds at the right version is not
//! sent again ([`ServerMessage::ChunkSame`], D4). A chunk is the
//! one message sent in binary ([`encode_chunk`], the layout in
//! [`ChunkFrame`]'s doc); every other message is JSON. The bodies standing
//! in a chunk follow its frame in [`ServerMessage::ChunkBodies`]. Bodies
//! are only reported while they stand within the creatures' range of the
//! client's body (about 150 m, D5), in a chunk it holds.
//!
//! Any change to this crate is a protected area (see `CODEOWNERS`).

use serde::{Deserialize, Serialize};

/// Protocol version. Bump on any incompatible change.
///
/// - v2: the ground travels in chunks around the body instead of whole.
/// - v3: chunks carry the items lying in them; `world_state` carries the
///   catalogue of materials they are made of.
/// - v4: bodies stand at fine positions and walk along routes of straight
///   segments (`body_walks`, `body_stopped`) instead of tile by tile.
/// - v5: bodies have a condition (needs, vitality, sleep) told to their
///   pilot; they eat and sleep; they die and their pilot is given another;
///   items appear and vanish; night falls.
/// - v6: `world_state` carries the clock (tick, ticks per second and per
///   day, pace), so clients can show the time of day between messages.
/// - v7: materials carry their mass and a description, for tooltips.
/// - v8: bodies pick up, hold, stow and drop items; the pilot is told its
///   inventory, everyone what a body holds in hand; `act_refused` replaces
///   `eat_refused`.
/// - v9: the `brook` terrain, water one or two tiles wide that is walked
///   through.
/// - v10: chunks carry the trees standing in them; `world_state` carries
///   the kinds of tree, with their height.
/// - v11: chunks carry where their water runs (`currents`).
/// - v12: development tools, on only when the server runs with `--dev`:
///   `dev_overview` (the whole map, coarse) and `dev_teleport`.
/// - v13: the flora: `world_state` lists its kinds (`flora`), chunks carry
///   their plants (`plants`, a kind's index and a place each).
/// - v14: `far_view`, the land around the body, coarse, drawn past the
///   chunks held.
/// - v15: `chunk_outline`, a chunk of the ring around the view: its ground,
///   currents, trees and tall plants, drawn fading into the haze.
/// - v16: `chunk_outline` is gone: the view's last tiles fade into a fog
///   inside the chunks held, so no ring is sent around them.
/// - v17: a chunk travels as a binary frame ([`ChunkFrame`]), with its
///   version; the bodies standing in it follow in `chunk_bodies`.
/// - v18: never resend what the client holds (D4): `world_state` names its
///   world (`world`); `chunk_same` instead of a frame the client keeps at
///   that version, `chunk_missing` when it does not; `hello` tells the
///   chunks a reconnecting client holds (`held`); `view`, the window
///   held, replaces `chunk_hidden`.
/// - v19: interest by place (D5, A6): bodies are told within the
///   creatures' range of the client's body (150 m between chunk centres),
///   not across the whole view: `chunk_bodies` follows only the frames
///   within it, and comes too for a chunk held that comes within it as
///   the body walks; `body_left` when a body walks out of it or it moves
///   away. `body_walks` is sent again each time a walking body walks into
///   another chunk, never at each tile: the client walks it along its
///   route in between, by time: `body_walks` and `chunk_bodies`' bodies
///   carry the tick each waypoint is reached (`arrivals`), the route
///   holding the points where the pace changes with the terrain.
/// - v20: `body_blocked`, to the pilot alone: its body stopped for good
///   short of where it was sent, and what it sees barring its way (D32).
pub const PROTOCOL_VERSION: u32 = 20;

/// Chunks whose frames a client keeps after they left its view, the
/// least recently hidden forgotten first (D4): about 3 MB of frames. The
/// server mirrors this cache per connection by the same rule, to know
/// when [`ServerMessage::ChunkSame`] is enough.
pub const CHUNK_CACHE_CHUNKS: usize = 1500;

/// Entries of [`HeldChunks`] read at most: the chunks of a
/// view and of the cache ([`CHUNK_CACHE_CHUNKS`]) fit.
pub const HELD_CHUNKS_MAX: usize = 2048;

/// A tile on the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Terrain {
    /// Open ground.
    Grass,
    /// Trees.
    Forest,
    /// The river; not walkable.
    Water,
    /// A shallow crossing of the river.
    Ford,
    /// A brook, one or two tiles wide; walked through.
    Brook,
    /// The cliff; not walkable.
    Rock,
}

/// A fine position: millimetres from the grid's origin. Tile (x, y)
/// spans `(x × 1000 .. (x + 1) × 1000, y × 1000 .. (y + 1) × 1000)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    /// Millimetres east.
    pub x: i32,
    /// Millimetres south.
    pub y: i32,
}

/// An item as a client refers to it when it is not on the ground.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemView {
    /// Item id.
    pub id: u64,
    /// Content id of its material, found in [`ServerMessage::WorldState`].
    pub material: String,
}

/// A body as the client sees it: where it stands and where it is going.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyState {
    /// Body id.
    pub id: u32,
    /// Where it stands.
    pub at: Point,
    /// Waypoints still ahead, nearest first; empty when standing still.
    /// Where it turns and where its pace changes (the terrain): between
    /// two of them it walks straight at one pace.
    pub route: Vec<Point>,
    /// The tick it stands at each waypoint of `route`: move it by time
    /// between them, from `at` now.
    pub arrivals: Vec<u64>,
    /// Nominal walking speed, on open ground; terrain slows it.
    pub speed_cm_per_s: u32,
    /// Lying asleep?
    pub asleep: bool,
    /// What it holds in hand, visible to all.
    pub holds: Option<ItemView>,
}

/// What a body can do with an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemAct {
    /// Take it off the ground.
    PickUp,
    /// Lay it on the ground.
    Drop,
    /// Take it in hand.
    Hold,
    /// Put it away, out of the hand.
    Stow,
    /// Eat it.
    Eat,
}

/// Why an act on an item was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// No such item.
    Unknown,
    /// Not on the body's tile nor next to it.
    OutOfReach,
    /// The body does not have it.
    NotCarried,
    /// It would exceed what the body can carry.
    TooHeavy,
    /// It is not food.
    NotFood,
}

/// What a body that stopped for good sees barring its way (D32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Blockage {
    /// Water too deep to walk through.
    Water,
    /// The cliff.
    Rock,
    /// Nothing it may not enter right ahead, yet no way it can see.
    NoWay,
}

/// What a pilot knows of its own body (D34). Nobody else is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    /// Life left, 0–100; zero is death.
    pub vitality: u32,
    /// Need for food, 0–100.
    pub hunger: u32,
    /// Lack of warmth, 0–100.
    pub cold: u32,
    /// Need for sleep, 0–100.
    pub fatigue: u32,
    /// Asleep?
    pub asleep: bool,
}

/// The world's clock, enough to show the time of day between messages:
/// `tick` was current when sent; ticks advance at `ticks_per_second ×
/// time_scale_permille / 1000` per real second; a day is `ticks_per_day`
/// ticks and starts at dawn (D09).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clock {
    /// The tick current when this was sent.
    pub tick: u64,
    /// Simulation ticks per game second.
    pub ticks_per_second: u32,
    /// Ticks per game day.
    pub ticks_per_day: u64,
    /// How many times faster than real time the server runs, in
    /// thousandths (1000 is real time).
    pub time_scale_permille: u32,
}

/// What killed a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeathCause {
    /// Hunger.
    Starved,
    /// Cold.
    Froze,
}

/// A material as the client needs to know it: a name to show and a
/// provisional look (D27: the client never reads the content files).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialView {
    /// Content id, the key items refer to.
    pub id: String,
    /// Name shown to players.
    pub name: String,
    /// One sentence shown to players.
    pub description: String,
    /// `#rrggbb`.
    pub colour: String,
    /// Can it be eaten?
    pub edible: bool,
    /// Mass of one piece, in grams.
    pub mass_g: u32,
}

/// A piece of material lying on the ground, as the client sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemState {
    /// Item id.
    pub id: u64,
    /// Column.
    pub x: i32,
    /// Row.
    pub y: i32,
    /// Content id of its material, found in [`ServerMessage::WorldState`].
    pub material: String,
}

/// A kind of tree, as clients need it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeciesView {
    /// Content id, the key trees refer to.
    pub id: String,
    /// Name shown to players.
    pub name: String,
    /// Height of a grown tree, in centimetres.
    pub height_cm: u32,
}

/// A kind of shrub, plant or mushroom, as clients need it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FloraView {
    /// Content id; the client finds its images by it.
    pub id: String,
    /// Name shown to players.
    pub name: String,
    /// `shrub`, `plant` or `mushroom`.
    pub family: String,
    /// Grown height, in centimetres.
    pub height_cm: u32,
}

/// The chunks a client still holds of a world (D4), told with
/// [`ClientMessage::Hello`] on a reconnection: the server sends
/// [`ServerMessage::ChunkSame`] for those unchanged. Ignored unless `world`
/// is the server's; at most [`HELD_CHUNKS_MAX`] entries are read. A client
/// that lies only misses what it would have been sent: nothing leaks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldChunks {
    /// The world they belong to (`world_state`'s `world`).
    pub world: String,
    /// Each chunk held, the least recently hidden first and those in view
    /// last: column, row, version of its frame.
    pub chunks: Vec<(i32, i32, u32)>,
}

/// Messages sent by the client to the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// First message after connecting.
    Hello {
        /// Protocol version the client speaks.
        protocol_version: u32,
        /// The chunks a reconnecting client still holds, if any: told
        /// with the hello, so the server knows them before it sends the
        /// first chunk. The one message allowed past the usual size limit.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        held: Option<HeldChunks>,
    },
    /// Liveness check.
    Ping {
        /// Echoed back in [`ServerMessage::Pong`].
        nonce: u64,
    },
    /// Intention: walk the piloted body to a tile.
    MoveTo {
        /// Column.
        x: i32,
        /// Row.
        y: i32,
    },
    /// Intention: eat an item within reach on the ground, or carried.
    Eat {
        /// The item.
        item: u64,
    },
    /// Intention: take an item off the ground within reach.
    PickUp {
        /// The item.
        item: u64,
    },
    /// Intention: lay a carried item on the body's tile.
    Drop {
        /// The item.
        item: u64,
    },
    /// Intention: take a carried item in hand.
    Hold {
        /// The item.
        item: u64,
    },
    /// Intention: put the item in hand away.
    Stow {
        /// The item.
        item: u64,
    },
    /// Intention: lie down and sleep where one stands.
    Sleep,
    /// Intention: wake up.
    Wake,
    /// Development only, ignored unless the server runs with `--dev`:
    /// stand the piloted body at once on the walkable tile nearest this
    /// one, to explore the map quickly.
    DevTeleport {
        /// Column.
        x: i32,
        /// Row.
        y: i32,
    },
    /// Answer to [`ServerMessage::ChunkSame`] when the client does not
    /// hold that chunk at that version after all: the frame follows.
    ChunkMissing {
        /// Column of chunks.
        x: i32,
        /// Row of chunks.
        y: i32,
    },
}

/// Messages sent by the server to the client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Reply to [`ClientMessage::Hello`].
    Welcome {
        /// Protocol version the server speaks.
        protocol_version: u32,
        /// Current world tick.
        tick: u64,
        /// The body this connection pilots, or `None` to only watch.
        body: Option<u32>,
    },
    /// The shape of the world, sent once after [`ServerMessage::Welcome`].
    /// The ground itself follows in chunk frames ([`encode_chunk`]).
    WorldState {
        /// Map columns.
        width: u32,
        /// Map rows.
        height: u32,
        /// Side of a chunk, in tiles.
        chunk_size: u32,
        /// Every material items may be made of.
        materials: Vec<MaterialView>,
        /// Every kind of tree chunks may hold.
        species: Vec<SpeciesView>,
        /// Every kind of shrub, plant and mushroom chunks may hold.
        flora: Vec<FloraView>,
        /// Is it night right now?
        night: bool,
        /// The clock.
        clock: Clock,
        /// The world's identity: chunks kept from another world are never
        /// shown in this one (D4).
        world: String,
    },
    /// The bodies standing in a chunk within the creatures' range, sent
    /// right after its frame ([`encode_chunk`]) when there are any, or
    /// when a chunk held comes within range.
    ChunkBodies {
        /// Column of chunks.
        x: i32,
        /// Row of chunks.
        y: i32,
        /// Every body standing in this chunk.
        bodies: Vec<BodyState>,
    },
    /// In place of a chunk's frame: the client holds it at `version`
    /// already (in view or in its cache, [`CHUNK_CACHE_CHUNKS`]) and shows
    /// it again as it is. If it does not, it answers
    /// [`ClientMessage::ChunkMissing`]. The bodies standing in it follow in
    /// [`ServerMessage::ChunkBodies`], as after a frame.
    ChunkSame {
        /// Column of chunks.
        x: i32,
        /// Row of chunks.
        y: i32,
        /// The version of its frame.
        version: u32,
    },
    /// The window of chunks the client holds, in chunks: every chunk held
    /// outside it is left (with the bodies in it), kept in the client's
    /// cache in the order of their rows, then columns. Sent each time the
    /// view moves, one message however far.
    View {
        /// First column.
        x: i32,
        /// First row.
        y: i32,
        /// Columns.
        width: u32,
        /// Rows.
        height: u32,
    },
    /// Reply to [`ClientMessage::Ping`].
    Pong {
        /// The nonce from the ping.
        nonce: u64,
    },
    /// The land around the body, coarse, for the client to draw past the
    /// chunks it holds (in a haze): sent after the first chunks, and again
    /// when the body has gone far from its centre. What can be seen from
    /// afar anyway, nothing that lies on it.
    FarView {
        /// Tiles each cell stands for, either way.
        cell: u32,
        /// Column of the first cell, in cells (cell x spans tiles
        /// `x × cell .. (x + 1) × cell`).
        x: i32,
        /// Row of the first cell, in cells.
        y: i32,
        /// Cells across.
        width: u32,
        /// Cells down.
        height: u32,
        /// One letter a cell, row by row: `g` grass, `f` forest, `w` river,
        /// `o` ford, `b` brook, `r` rock, `.` past the map.
        cells: String,
    },
    /// Development only, sent after [`ServerMessage::WorldState`] when the
    /// server runs with `--dev`: the whole map, coarse, for an overview
    /// to teleport from. Its presence tells the client the development
    /// tools are on.
    DevOverview {
        /// Tiles each cell of the overview stands for, either way.
        cell: u32,
        /// Cells across.
        width: u32,
        /// Cells down.
        height: u32,
        /// One letter a cell, row by row from the north-west corner: `g`
        /// grass, `f` forest, `w` river, `o` ford, `b` brook, `r` rock.
        cells: String,
    },
    /// The client spoke a protocol version the server does not support.
    /// The server closes the connection after sending it.
    ProtocolMismatch {
        /// Version the server expected.
        expected: u32,
    },
    /// A new game day began: dawn.
    DayStarted {
        /// The day that just started (days count from 1).
        day: u64,
        /// The tick at which it started.
        tick: u64,
    },
    /// Night fell: dark and cold until dawn.
    NightFell {
        /// The day it ends.
        day: u64,
    },
    /// How the pilot's own body fares. Sent to the pilot alone, whenever
    /// a whole point changes.
    BodyCondition {
        /// Which body.
        body: u32,
        /// Its condition.
        condition: Condition,
    },
    /// A body in a chunk the client holds lay down or got up.
    BodySleeps {
        /// Who.
        body: u32,
        /// Asleep now?
        asleep: bool,
    },
    /// A body in a chunk the client holds died; its corpse is an item
    /// (an `item_added` precedes this message).
    BodyDied {
        /// Who.
        body: u32,
        /// Of what.
        cause: DeathCause,
    },
    /// The pilot's body changed: after a death, the first free body, or
    /// none to watch. The chunks around it follow.
    BodyAssigned {
        /// The body now piloted, or `None`.
        body: Option<u32>,
    },
    /// An item appeared in a chunk the client holds.
    ItemAdded {
        /// The item.
        item: ItemState,
    },
    /// An item vanished from a chunk the client holds.
    ItemRemoved {
        /// Which.
        item: u64,
        /// Column it was on.
        x: i32,
        /// Row it was on.
        y: i32,
    },
    /// An act on an item was refused. Sent to the pilot who asked.
    ActRefused {
        /// Who.
        body: u32,
        /// What it tried.
        act: ItemAct,
        /// On what.
        item: u64,
        /// Why.
        reason: Refusal,
    },
    /// What the pilot's body carries. Sent to the pilot alone, whenever
    /// it changes.
    Inventory {
        /// Which body.
        body: u32,
        /// In hand.
        held: Option<ItemView>,
        /// Carried, in the order taken.
        carried: Vec<ItemView>,
        /// Grams carried in all.
        load_g: u32,
        /// Grams the body can carry.
        capacity_g: u32,
    },
    /// A body in a chunk the client holds changed what it holds in hand.
    BodyHolds {
        /// Who.
        body: u32,
        /// In hand now, if anything.
        item: Option<ItemView>,
    },
    /// A body within range set off along a route, walked into another
    /// chunk, or is seen for the first time: move it from `at` through the
    /// waypoints at its speed. Told again at each chunk it walks into
    /// (A6), never at each tile.
    BodyWalks {
        /// Who.
        body: u32,
        /// Where it is now.
        at: Point,
        /// Waypoints ahead, nearest first; the last is the destination.
        /// Where it turns and where its pace changes (the terrain).
        route: Vec<Point>,
        /// The tick it stands at each waypoint of `route`: move it by
        /// time between them, from `at` at `tick`, and it is where the
        /// world has it whatever the ground (A6).
        arrivals: Vec<u64>,
        /// Nominal walking speed, on open ground; terrain slows it.
        speed_cm_per_s: u32,
        /// When.
        tick: u64,
    },
    /// A body in a chunk the client holds stopped walking.
    BodyStopped {
        /// Who.
        body: u32,
        /// Exactly where.
        at: Point,
    },
    /// A body walked out of the creatures' range, or the range moved away
    /// from it: forget it.
    BodyLeft {
        /// Who.
        body: u32,
    },
    /// A walk could not start (impassable or unreachable target). Sent to
    /// the pilot who asked, and to nobody else.
    MoveRefused {
        /// Who.
        body: u32,
        /// Column asked.
        x: i32,
        /// Row asked.
        y: i32,
    },
    /// The pilot's body stopped for good short of where it was sent: it
    /// walked straight for want of a way, looked again and could get no
    /// nearer (D32). Sent to the pilot alone, after its `body_stopped`.
    BodyBlocked {
        /// Who.
        body: u32,
        /// Exactly where it stands.
        at: Point,
        /// Column it was sent to.
        x: i32,
        /// Row it was sent to.
        y: i32,
        /// What it sees barring its way.
        reason: Blockage,
    },
}

/// Encodes a message to its JSON wire form.
pub fn encode<T: Serialize>(message: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(message)
}

/// Decodes a message from its JSON wire form.
pub fn decode<'a, T: Deserialize<'a>>(text: &'a str) -> Result<T, serde_json::Error> {
    serde_json::from_str(text)
}

/// A tree or a plant in a [`ChunkFrame`]: where it stands in its chunk
/// and its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    /// Millimetres east of the chunk's north-west corner.
    pub dx: u16,
    /// Millimetres south of it.
    pub dy: u16,
    /// A tree's species, an index into [`ServerMessage::WorldState`]'s
    /// `species`; a plant's kind, into its `flora`.
    pub kind: u8,
}

/// A natural item in a [`ChunkFrame`]: what the ground itself drops. Its
/// id and its tile follow from the chunk and the slot (`sim::items`): id
/// `2^52 | chunk x << 34 | chunk y << 16 | slot`, tile `slot >> 6` in the
/// chunk, row by row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NaturalItem {
    /// Its slot in the chunk.
    pub slot: u16,
    /// Its material, an index into [`ServerMessage::WorldState`]'s
    /// `materials`.
    pub material: u8,
}

/// An item men laid on the ground, in a [`ChunkFrame`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredItem {
    /// Item id.
    pub id: u64,
    /// Its tile's column in the chunk.
    pub dx: u8,
    /// Its tile's row in the chunk.
    pub dy: u8,
    /// Its material, an index into [`ServerMessage::WorldState`]'s
    /// `materials`.
    pub material: u16,
}

/// A chunk of ground the client now holds, sent as a binary frame
/// ([`encode_chunk`], [`decode_chunk`]): what lies and stands in it, the
/// bodies apart ([`ServerMessage::ChunkBodies`]). Chunk (0, 0) holds tiles
/// `(0..side, 0..side)`.
///
/// The frame, little-endian:
///
/// - header, 16 bytes: `x i32`, `y i32`, `version u32`, `flags u8` (bit 0:
///   the chunk has water, so currents follow the tiles), `side u8`, two
///   bytes 0;
/// - tiles: `side × side` bytes, row by row from the north-west corner,
///   each a [`Terrain`] code ([`terrain_code`]);
/// - currents, when flag 0 is set: one `u16` per water tile (river, ford,
///   brook) in tile order;
/// - trees: `n u16`, then `n × (dx u16, dy u16, species u8)`;
/// - plants: `n u16`, then `n × (dx u16, dy u16, kind u8)`;
/// - natural items: `n u16`, then `n × (slot u16, material u8)`;
/// - stored items: `n u16`, then `n × (id u64, dx u8, dy u8, material u16)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkFrame {
    /// Column of chunks.
    pub x: i32,
    /// Row of chunks.
    pub y: i32,
    /// The chunk's version: how many times men changed it, 0 while it is
    /// as the seed made it.
    pub version: u32,
    /// `side × side` tiles, row by row from the north-west corner; rock
    /// where the chunk reaches past the map.
    pub tiles: Vec<Terrain>,
    /// Where the water runs, tile by tile like `tiles`: 0 where it does
    /// not, always on dry tiles (land, rock); else its heading times 256
    /// plus its speed in centimetres a second (1 to 255). Heading h, from
    /// 0 to 15, points along (sin(h × 22.5°), cos(h × 22.5°)) in tiles: 0
    /// down the map (+y), 4 along +x.
    pub currents: Vec<u16>,
    /// Every tree standing in this chunk.
    pub trees: Vec<Standing>,
    /// Every plant standing in this chunk.
    pub plants: Vec<Standing>,
    /// The ground's own items still lying in this chunk.
    pub natural: Vec<NaturalItem>,
    /// The items men laid in this chunk.
    pub stored: Vec<StoredItem>,
}

impl ChunkFrame {
    /// The id of the natural item in `slot` of this chunk, as `sim::items`
    /// makes it: 18 bits a chunk coordinate (natural items lie only in
    /// chunks 0 to 262,143 either way).
    #[must_use]
    pub fn natural_id(&self, slot: u16) -> u64 {
        const MASK: u32 = (1 << 18) - 1;
        let cx = u64::from(self.x.cast_unsigned() & MASK);
        let cy = u64::from(self.y.cast_unsigned() & MASK);
        1 << 52 | cx << 34 | cy << 16 | u64::from(slot)
    }

    /// Every item lying in the chunk, in the order of their ids, as a
    /// client rebuilds them: their materials named from `materials`
    /// (`WorldState`'s), empty for an index past it.
    #[must_use]
    pub fn items(&self, materials: &[MaterialView]) -> Vec<ItemState> {
        let side = self.tiles.len().isqrt().max(1);
        let (x0, y0) = (self.x * side_i32(side), self.y * side_i32(side));
        let name = |i: usize| materials.get(i).map_or_else(String::new, |m| m.id.clone());
        let mut items: Vec<ItemState> = self
            .stored
            .iter()
            .map(|i| ItemState {
                id: i.id,
                x: x0 + i32::from(i.dx),
                y: y0 + i32::from(i.dy),
                material: name(usize::from(i.material)),
            })
            .chain(self.natural.iter().map(|i| {
                let tile = usize::from(i.slot >> 6);
                ItemState {
                    id: self.natural_id(i.slot),
                    x: x0 + side_i32(tile % side),
                    y: y0 + side_i32(tile / side),
                    material: name(usize::from(i.material)),
                }
            }))
            .collect();
        items.sort_by_key(|i| i.id);
        items
    }
}

/// A small count as an `i32`.
fn side_i32(n: usize) -> i32 {
    i32::try_from(n).expect("a chunk's side")
}

/// Bytes of a chunk frame's header.
pub const CHUNK_HEADER: usize = 16;
/// Flag of a chunk frame: the chunk has water, its currents follow.
pub const CHUNK_HAS_WATER: u8 = 1;

/// A chunk frame that cannot be written or read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// The tiles are not a square of at most 255 on a side, or the
    /// currents are not one a tile.
    Shape,
    /// A dry tile has a current: it would be lost on the wire.
    DryCurrent,
    /// More than 65,535 of something.
    TooMany,
    /// The bytes end too early, or go on past the frame.
    Length,
    /// A tile code no terrain has.
    Terrain(u8),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shape => write!(f, "tiles not a square, or currents not one a tile"),
            Self::DryCurrent => write!(f, "a current on a dry tile"),
            Self::TooMany => write!(f, "more than 65535 of a kind of thing"),
            Self::Length => write!(f, "the frame's length does not match its content"),
            Self::Terrain(code) => write!(f, "unknown terrain code {code}"),
        }
    }
}

impl std::error::Error for FrameError {}

/// A terrain's code in a chunk frame: the client's `TerrainGrid` codes (0
/// is no terrain).
#[must_use]
pub fn terrain_code(terrain: Terrain) -> u8 {
    match terrain {
        Terrain::Grass => 1,
        Terrain::Forest => 2,
        Terrain::Water => 3,
        Terrain::Ford => 4,
        Terrain::Brook => 5,
        Terrain::Rock => 6,
    }
}

/// The terrain of a code ([`terrain_code`]), if any.
#[must_use]
pub fn terrain_of_code(code: u8) -> Option<Terrain> {
    Some(match code {
        1 => Terrain::Grass,
        2 => Terrain::Forest,
        3 => Terrain::Water,
        4 => Terrain::Ford,
        5 => Terrain::Brook,
        6 => Terrain::Rock,
        _ => return None,
    })
}

/// Does water run on this terrain (its current is on the wire)?
#[must_use]
pub fn is_water(terrain: Terrain) -> bool {
    matches!(terrain, Terrain::Water | Terrain::Ford | Terrain::Brook)
}

/// A count of things, as the frame writes it.
fn count(n: usize) -> Result<[u8; 2], FrameError> {
    u16::try_from(n)
        .map(u16::to_le_bytes)
        .map_err(|_| FrameError::TooMany)
}

/// Writes a chunk frame (the layout in [`ChunkFrame`]'s doc).
///
/// # Errors
///
/// When the tiles are not a square of at most 255 on a side or the
/// currents not one a tile, when a dry tile has a current, or when there
/// are more than 65,535 of some thing.
pub fn encode_chunk(chunk: &ChunkFrame) -> Result<Vec<u8>, FrameError> {
    let side = chunk.tiles.len().isqrt();
    if side * side != chunk.tiles.len() || chunk.currents.len() != chunk.tiles.len() {
        return Err(FrameError::Shape);
    }
    let side = u8::try_from(side).map_err(|_| FrameError::Shape)?;
    let wet = chunk.tiles.iter().any(|&t| is_water(t));
    let mut out = Vec::with_capacity(
        CHUNK_HEADER
            + chunk.tiles.len()
            + 8
            + 5 * (chunk.trees.len() + chunk.plants.len())
            + 3 * chunk.natural.len()
            + 12 * chunk.stored.len(),
    );
    out.extend_from_slice(&chunk.x.to_le_bytes());
    out.extend_from_slice(&chunk.y.to_le_bytes());
    out.extend_from_slice(&chunk.version.to_le_bytes());
    out.extend_from_slice(&[if wet { CHUNK_HAS_WATER } else { 0 }, side, 0, 0]);
    out.extend(chunk.tiles.iter().map(|&t| terrain_code(t)));
    for (&tile, &current) in chunk.tiles.iter().zip(&chunk.currents) {
        if is_water(tile) {
            out.extend_from_slice(&current.to_le_bytes());
        } else if current != 0 {
            return Err(FrameError::DryCurrent);
        }
    }
    for standing in [&chunk.trees, &chunk.plants] {
        out.extend_from_slice(&count(standing.len())?);
        for s in standing {
            out.extend_from_slice(&s.dx.to_le_bytes());
            out.extend_from_slice(&s.dy.to_le_bytes());
            out.push(s.kind);
        }
    }
    out.extend_from_slice(&count(chunk.natural.len())?);
    for item in &chunk.natural {
        out.extend_from_slice(&item.slot.to_le_bytes());
        out.push(item.material);
    }
    out.extend_from_slice(&count(chunk.stored.len())?);
    for item in &chunk.stored {
        out.extend_from_slice(&item.id.to_le_bytes());
        out.extend_from_slice(&[item.dx, item.dy]);
        out.extend_from_slice(&item.material.to_le_bytes());
    }
    Ok(out)
}

/// Reads a chunk frame byte by byte.
struct Reader<'a> {
    bytes: &'a [u8],
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], FrameError> {
        let (head, rest) = self
            .bytes
            .split_first_chunk::<N>()
            .ok_or(FrameError::Length)?;
        self.bytes = rest;
        Ok(*head)
    }

    fn u8(&mut self) -> Result<u8, FrameError> {
        Ok(self.take::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, FrameError> {
        self.take().map(u16::from_le_bytes)
    }

    fn standing(&mut self) -> Result<Vec<Standing>, FrameError> {
        let n = self.u16()?;
        (0..n)
            .map(|_| {
                Ok(Standing {
                    dx: self.u16()?,
                    dy: self.u16()?,
                    kind: self.u8()?,
                })
            })
            .collect()
    }
}

/// Reads a chunk frame ([`encode_chunk`]).
///
/// # Errors
///
/// When the bytes are not a whole frame and nothing more, or a tile code
/// is no terrain's.
pub fn decode_chunk(bytes: &[u8]) -> Result<ChunkFrame, FrameError> {
    let mut r = Reader { bytes };
    let x = i32::from_le_bytes(r.take()?);
    let y = i32::from_le_bytes(r.take()?);
    let version = u32::from_le_bytes(r.take()?);
    let [flags, side, _, _] = r.take::<4>()?;
    let side = usize::from(side);
    let tiles = (0..side * side)
        .map(|_| {
            let code = r.u8()?;
            terrain_of_code(code).ok_or(FrameError::Terrain(code))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut currents = vec![0; tiles.len()];
    if flags & CHUNK_HAS_WATER != 0 {
        for (current, &tile) in currents.iter_mut().zip(&tiles) {
            if is_water(tile) {
                *current = r.u16()?;
            }
        }
    }
    let trees = r.standing()?;
    let plants = r.standing()?;
    let n = r.u16()?;
    let natural = (0..n)
        .map(|_| {
            Ok(NaturalItem {
                slot: r.u16()?,
                material: r.u8()?,
            })
        })
        .collect::<Result<Vec<_>, FrameError>>()?;
    let n = r.u16()?;
    let stored = (0..n)
        .map(|_| {
            Ok(StoredItem {
                id: u64::from_le_bytes(r.take()?),
                dx: r.u8()?,
                dy: r.u8()?,
                material: r.u16()?,
            })
        })
        .collect::<Result<Vec<_>, FrameError>>()?;
    if !r.bytes.is_empty() {
        return Err(FrameError::Length);
    }
    Ok(ChunkFrame {
        x,
        y,
        version,
        tiles,
        currents,
        trees,
        plants,
        natural,
        stored,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_messages_round_trip() {
        let messages = [
            ClientMessage::Hello {
                protocol_version: PROTOCOL_VERSION,
                held: None,
            },
            ClientMessage::Hello {
                protocol_version: PROTOCOL_VERSION,
                held: Some(HeldChunks {
                    world: "0123456789abcdef".to_owned(),
                    chunks: vec![(3, -1, 0), (4, 7, 12)],
                }),
            },
            ClientMessage::Ping { nonce: 42 },
            ClientMessage::MoveTo { x: 3, y: -1 },
            ClientMessage::Eat { item: 12 },
            ClientMessage::PickUp { item: 12 },
            ClientMessage::Drop { item: 12 },
            ClientMessage::Hold { item: 12 },
            ClientMessage::Stow { item: 12 },
            ClientMessage::Sleep,
            ClientMessage::Wake,
            ClientMessage::DevTeleport { x: 1500, y: -2 },
            ClientMessage::ChunkMissing { x: 3, y: -1 },
        ];
        for message in messages {
            let text = encode(&message).unwrap();
            let back: ClientMessage = decode(&text).unwrap();
            assert_eq!(message, back);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn server_messages_round_trip() {
        let messages = [
            ServerMessage::Welcome {
                protocol_version: PROTOCOL_VERSION,
                tick: 7,
                body: Some(1),
            },
            ServerMessage::Welcome {
                protocol_version: PROTOCOL_VERSION,
                tick: 7,
                body: None,
            },
            ServerMessage::WorldState {
                width: 256,
                height: 256,
                chunk_size: 32,
                materials: vec![MaterialView {
                    id: "flint".into(),
                    name: "Flint".into(),
                    description: "A hard stone.".into(),
                    colour: "#5e5a52".into(),
                    edible: false,
                    mass_g: 400,
                }],
                species: vec![SpeciesView {
                    id: "oak".into(),
                    name: "Oak".into(),
                    height_cm: 400,
                }],
                flora: vec![FloraView {
                    id: "bramble".into(),
                    name: "Bramble".into(),
                    family: "shrub".into(),
                    height_cm: 90,
                }],
                night: true,
                clock: Clock {
                    tick: 3_600,
                    ticks_per_second: 10,
                    ticks_per_day: 72_000,
                    time_scale_permille: 1000,
                },
                world: "0123456789abcdef".to_owned(),
            },
            ServerMessage::ChunkBodies {
                x: 5,
                y: -1,
                bodies: vec![BodyState {
                    id: 1,
                    at: Point { x: 160_500, y: 500 },
                    route: vec![Point { x: 170_500, y: 500 }],
                    arrivals: vec![80],
                    speed_cm_per_s: 143,
                    asleep: false,
                    holds: Some(ItemView {
                        id: 5,
                        material: "flint".into(),
                    }),
                }],
            },
            ServerMessage::ChunkSame {
                x: 5,
                y: -4,
                version: 3,
            },
            ServerMessage::View {
                x: -2,
                y: 3,
                width: 19,
                height: 17,
            },
            ServerMessage::Pong { nonce: 42 },
            ServerMessage::DevOverview {
                cell: 8,
                width: 2,
                height: 2,
                cells: "gfwr".to_owned(),
            },
            ServerMessage::FarView {
                cell: 8,
                x: -3,
                y: 40,
                width: 2,
                height: 1,
                cells: ".g".to_owned(),
            },
            ServerMessage::ProtocolMismatch { expected: 1 },
            ServerMessage::DayStarted {
                day: 2,
                tick: 72_000,
            },
            ServerMessage::BodyWalks {
                body: 1,
                at: Point { x: 4_500, y: 5_500 },
                route: vec![Point { x: 6_500, y: 5_500 }, Point { x: 6_500, y: 9_500 }],
                arrivals: vec![23, 51],
                speed_cm_per_s: 143,
                tick: 9,
            },
            ServerMessage::BodyStopped {
                body: 1,
                at: Point { x: 6_500, y: 9_500 },
            },
            ServerMessage::NightFell { day: 3 },
            ServerMessage::BodyCondition {
                body: 1,
                condition: Condition {
                    vitality: 88,
                    hunger: 40,
                    cold: 0,
                    fatigue: 65,
                    asleep: false,
                },
            },
            ServerMessage::BodySleeps {
                body: 2,
                asleep: true,
            },
            ServerMessage::BodyDied {
                body: 1,
                cause: DeathCause::Froze,
            },
            ServerMessage::BodyAssigned { body: Some(2) },
            ServerMessage::BodyAssigned { body: None },
            ServerMessage::ItemAdded {
                item: ItemState {
                    id: 77,
                    x: 1,
                    y: 2,
                    material: "corpse".into(),
                },
            },
            ServerMessage::ItemRemoved {
                item: 12,
                x: 1,
                y: 2,
            },
            ServerMessage::ActRefused {
                body: 1,
                act: ItemAct::PickUp,
                item: 12,
                reason: Refusal::TooHeavy,
            },
            ServerMessage::Inventory {
                body: 1,
                held: Some(ItemView {
                    id: 5,
                    material: "flint".into(),
                }),
                carried: vec![ItemView {
                    id: 6,
                    material: "berries".into(),
                }],
                load_g: 500,
                capacity_g: 20_000,
            },
            ServerMessage::BodyHolds {
                body: 1,
                item: None,
            },
            ServerMessage::BodyLeft { body: 1 },
            ServerMessage::MoveRefused {
                body: 1,
                x: 4,
                y: 5,
            },
            ServerMessage::BodyBlocked {
                body: 1,
                at: Point {
                    x: 34_700,
                    y: 40_500,
                },
                x: 40,
                y: 40,
                reason: Blockage::Water,
            },
            ServerMessage::BodyBlocked {
                body: 1,
                at: Point { x: 0, y: 0 },
                x: 4,
                y: 5,
                reason: Blockage::NoWay,
            },
        ];
        for message in messages {
            let text = encode(&message).unwrap();
            let back: ServerMessage = decode(&text).unwrap();
            assert_eq!(message, back);
        }
    }

    #[test]
    fn wire_format_is_tagged_snake_case() {
        let text = encode(&ClientMessage::Ping { nonce: 1 }).unwrap();
        assert_eq!(text, r#"{"type":"ping","nonce":1}"#);
        let text = encode(&ClientMessage::MoveTo { x: 2, y: 3 }).unwrap();
        assert_eq!(text, r#"{"type":"move_to","x":2,"y":3}"#);
        assert_eq!(
            encode(&ClientMessage::Sleep).unwrap(),
            r#"{"type":"sleep"}"#
        );
        assert_eq!(
            encode(&ClientMessage::Eat { item: 5 }).unwrap(),
            r#"{"type":"eat","item":5}"#
        );
        assert_eq!(
            encode(&ClientMessage::DevTeleport { x: 7, y: 9 }).unwrap(),
            r#"{"type":"dev_teleport","x":7,"y":9}"#
        );
        assert_eq!(
            encode(&ServerMessage::DevOverview {
                cell: 8,
                width: 1,
                height: 1,
                cells: "g".to_owned(),
            })
            .unwrap(),
            r#"{"type":"dev_overview","cell":8,"width":1,"height":1,"cells":"g"}"#
        );
        assert_eq!(
            encode(&ServerMessage::ActRefused {
                body: 1,
                act: ItemAct::PickUp,
                item: 5,
                reason: Refusal::OutOfReach,
            })
            .unwrap(),
            r#"{"type":"act_refused","body":1,"act":"pick_up","item":5,"reason":"out_of_reach"}"#
        );
        assert_eq!(
            encode(&ServerMessage::BodyBlocked {
                body: 1,
                at: Point { x: 1500, y: 2500 },
                x: 9,
                y: 2,
                reason: Blockage::NoWay,
            })
            .unwrap(),
            r#"{"type":"body_blocked","body":1,"at":{"x":1500,"y":2500},"x":9,"y":2,"reason":"no_way"}"#
        );
        assert_eq!(
            encode(&ServerMessage::ChunkBodies {
                x: 1,
                y: 2,
                bodies: vec![],
            })
            .unwrap(),
            r#"{"type":"chunk_bodies","x":1,"y":2,"bodies":[]}"#
        );
    }

    #[test]
    fn unknown_messages_are_rejected() {
        assert!(decode::<ClientMessage>(r#"{"type":"teleport"}"#).is_err());
        assert!(decode::<ClientMessage>("not json").is_err());
    }
    /// The reference chunk of the golden bytes: water and dry land, trees,
    /// plants, natural and stored items, on a chunk of 4 × 4 tiles.
    fn reference_chunk() -> ChunkFrame {
        use Terrain::{Brook, Ford, Forest, Grass, Rock, Water};
        ChunkFrame {
            x: 3,
            y: -2,
            version: 7,
            tiles: vec![
                Grass, Forest, Water, Ford, //
                Brook, Rock, Grass, Water, //
                Forest, Forest, Grass, Grass, //
                Rock, Rock, Rock, Brook,
            ],
            currents: vec![
                0,
                0,
                4 * 256 + 35,
                50, //
                12 * 256 + 1,
                0,
                0,
                0, //
                0,
                0,
                0,
                0, //
                0,
                0,
                0,
                15 * 256 + 255,
            ],
            trees: vec![
                Standing {
                    dx: 31_999,
                    dy: 0,
                    kind: 2,
                },
                Standing {
                    dx: 1_500,
                    dy: 2_250,
                    kind: 0,
                },
            ],
            plants: vec![Standing {
                dx: 700,
                dy: 3_900,
                kind: 11,
            }],
            natural: vec![
                NaturalItem {
                    slot: 1 << 6 | 3,
                    material: 4,
                },
                NaturalItem {
                    slot: 15 << 6,
                    material: 0,
                },
            ],
            stored: vec![StoredItem {
                id: 0x0001_0000_0000_002a,
                dx: 3,
                dy: 1,
                material: 513,
            }],
        }
    }

    #[test]
    fn chunk_frames_round_trip() {
        let mut dense = reference_chunk();
        dense.tiles = vec![Terrain::Forest; 32 * 32];
        dense.currents = vec![0; 32 * 32];
        dense.trees = (0..400_u16)
            .map(|i| Standing {
                dx: i * 80,
                dy: 32_000 - i * 80,
                kind: u8::try_from(i % 7).unwrap(),
            })
            .collect();
        dense.plants = (0..2_000_u16)
            .map(|i| Standing {
                dx: i * 16,
                dy: i,
                kind: u8::try_from(i % 40).unwrap(),
            })
            .collect();
        let empty = ChunkFrame {
            x: 0,
            y: 0,
            version: 0,
            tiles: vec![Terrain::Grass; 32 * 32],
            currents: vec![0; 32 * 32],
            trees: vec![],
            plants: vec![],
            natural: vec![],
            stored: vec![],
        };
        for chunk in [reference_chunk(), dense, empty] {
            let bytes = encode_chunk(&chunk).unwrap();
            assert_eq!(decode_chunk(&bytes).unwrap(), chunk);
        }
        let empty_bytes = encode_chunk(&ChunkFrame {
            x: 0,
            y: 0,
            version: 0,
            tiles: vec![Terrain::Grass; 32 * 32],
            currents: vec![0; 32 * 32],
            trees: vec![],
            plants: vec![],
            natural: vec![],
            stored: vec![],
        })
        .unwrap();
        assert_eq!(
            empty_bytes.len(),
            CHUNK_HEADER + 32 * 32 + 4 * 2,
            "a dry chunk carries no currents"
        );
    }

    #[test]
    fn chunk_frames_refuse_what_they_cannot_carry() {
        let mut dry_current = reference_chunk();
        dry_current.currents[0] = 3;
        assert_eq!(encode_chunk(&dry_current), Err(FrameError::DryCurrent));
        let mut ragged = reference_chunk();
        ragged.tiles.pop();
        assert_eq!(encode_chunk(&ragged), Err(FrameError::Shape));
        let bytes = encode_chunk(&reference_chunk()).unwrap();
        assert_eq!(
            decode_chunk(&bytes[..bytes.len() - 1]),
            Err(FrameError::Length)
        );
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(decode_chunk(&longer), Err(FrameError::Length));
        let mut unknown = bytes;
        unknown[CHUNK_HEADER] = 9;
        assert_eq!(decode_chunk(&unknown), Err(FrameError::Terrain(9)));
    }

    /// The golden bytes the client's tests decode
    /// (`client/tests/fixtures/chunk_v17.bin`): a change of the layout on
    /// this side breaks this test until the file is written again
    /// (`SOUL_WRITE_GOLDEN=1`), and then the client's test until the
    /// client reads the new layout.
    #[test]
    fn the_golden_chunk_is_the_clients() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../client/tests/fixtures/chunk_v17.bin");
        let bytes = encode_chunk(&reference_chunk()).unwrap();
        if std::env::var_os("SOUL_WRITE_GOLDEN").is_some() {
            std::fs::write(&path, &bytes).unwrap();
        }
        let golden = std::fs::read(&path).expect("the golden chunk, written");
        assert_eq!(
            golden, bytes,
            "the layout changed: write the golden chunk again"
        );
        assert_eq!(decode_chunk(&golden).unwrap(), reference_chunk());
        // The items as the client rebuilds them, ids and tiles: the same
        // numbers its test checks.
        let items: Vec<(u64, i32, i32)> = reference_chunk()
            .items(&[])
            .iter()
            .map(|i| (i.id, i.x, i.y))
            .collect();
        assert_eq!(
            items,
            [
                (281_474_976_710_698, 15, -7),
                (4_503_668_346_716_227, 13, -8),
                (4_503_668_346_717_120, 15, -5),
            ]
        );
    }
}
