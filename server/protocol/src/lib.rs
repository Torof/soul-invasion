//! Client <-> server protocol for Soul Invasion.
//!
//! Architecture decisions D25 and D26: WebSocket transport, JSON encoding
//! confined to this crate (and to one module on the client side), every
//! message carries its protocol version. Clients only ever send
//! *intentions*; the server only ever sends what the player's body perceives.
//!
//! The ground travels in chunks (D08, D31): after [`ServerMessage::Welcome`]
//! and [`ServerMessage::WorldState`], the client receives one
//! [`ServerMessage::Chunk`] per chunk around its body, then more as the
//! body walks and [`ServerMessage::ChunkHidden`] for those left behind.
//! Bodies are only reported while they stand in a chunk the client holds.
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
pub const PROTOCOL_VERSION: u32 = 14;

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
    pub route: Vec<Point>,
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

/// A plant standing on the ground, compact: many come in every chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlantState {
    /// Its kind: an index into [`ServerMessage::WorldState`]'s `flora`.
    pub kind: u16,
    /// Where it stands, millimetres east of the grid's origin.
    pub x: i32,
    /// Millimetres south.
    pub y: i32,
}

/// A tree standing on the ground.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeState {
    /// Where its trunk meets the ground.
    pub at: Point,
    /// Content id of its kind, found in [`ServerMessage::WorldState`].
    pub species: String,
}

/// Messages sent by the client to the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// First message after connecting.
    Hello {
        /// Protocol version the client speaks.
        protocol_version: u32,
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
    /// The ground itself follows in [`ServerMessage::Chunk`] messages.
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
    },
    /// A chunk of ground the client now holds, with the bodies standing
    /// in it and the items lying in it. Chunk (0, 0) holds tiles
    /// `(0..chunk_size, 0..chunk_size)`.
    Chunk {
        /// Column of chunks.
        x: i32,
        /// Row of chunks.
        y: i32,
        /// `chunk_size × chunk_size` tiles, row by row from the north-west
        /// corner; rock where the chunk reaches past the map.
        tiles: Vec<Terrain>,
        /// Every body standing in this chunk.
        bodies: Vec<BodyState>,
        /// Every item lying in this chunk.
        items: Vec<ItemState>,
        /// Every tree standing in this chunk.
        trees: Vec<TreeState>,
        /// Every plant standing in this chunk.
        plants: Vec<PlantState>,
        /// Where the water runs, tile by tile like `tiles`: 0 where it
        /// does not (land, rock); else its heading times 256 plus its speed
        /// in centimetres a second (1 to 255). Heading h, from 0 to 15,
        /// points along (sin(h × 22.5°), cos(h × 22.5°)) in tiles: 0 down
        /// the map (+y), 4 along +x.
        currents: Vec<u16>,
    },
    /// A chunk the client no longer holds: forget its tiles and the
    /// bodies in it.
    ChunkHidden {
        /// Column of chunks.
        x: i32,
        /// Row of chunks.
        y: i32,
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
    /// A body in a chunk the client holds set off (or is seen for the
    /// first time) along a route: move it from `at` through the waypoints
    /// at its speed.
    BodyWalks {
        /// Who.
        body: u32,
        /// Where it is now.
        at: Point,
        /// Waypoints ahead, nearest first; the last is the destination.
        route: Vec<Point>,
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
    /// A body walked out of the chunks the client holds: forget it.
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
}

/// Encodes a message to its JSON wire form.
pub fn encode<T: Serialize>(message: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(message)
}

/// Decodes a message from its JSON wire form.
pub fn decode<'a, T: Deserialize<'a>>(text: &'a str) -> Result<T, serde_json::Error> {
    serde_json::from_str(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_messages_round_trip() {
        let messages = [
            ClientMessage::Hello {
                protocol_version: PROTOCOL_VERSION,
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
            },
            ServerMessage::Chunk {
                x: 5,
                y: -1,
                tiles: vec![Terrain::Grass, Terrain::Water],
                bodies: vec![BodyState {
                    id: 1,
                    at: Point { x: 160_500, y: 500 },
                    route: vec![Point { x: 170_500, y: 500 }],
                    speed_cm_per_s: 143,
                    asleep: false,
                    holds: Some(ItemView {
                        id: 5,
                        material: "flint".into(),
                    }),
                }],
                items: vec![ItemState {
                    id: 9,
                    x: 161,
                    y: 1,
                    material: "flint".into(),
                }],
                trees: vec![TreeState {
                    at: Point { x: 162_300, y: 700 },
                    species: "oak".into(),
                }],
                plants: vec![PlantState {
                    kind: 0,
                    x: 161_200,
                    y: 900,
                }],
                currents: vec![0, 4 * 256 + 35],
            },
            ServerMessage::ChunkHidden { x: 5, y: 4 },
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
        let text = encode(&ServerMessage::Chunk {
            x: 1,
            y: 2,
            tiles: vec![Terrain::Ford],
            bodies: vec![],
            items: vec![ItemState {
                id: 3,
                x: 32,
                y: 64,
                material: "pebble".into(),
            }],
            trees: vec![TreeState {
                at: Point {
                    x: 32_400,
                    y: 64_700,
                },
                species: "birch".into(),
            }],
            plants: vec![PlantState {
                kind: 2,
                x: 32_100,
                y: 64_300,
            }],
            currents: vec![1059],
        })
        .unwrap();
        assert_eq!(
            text,
            r#"{"type":"chunk","x":1,"y":2,"tiles":["ford"],"bodies":[],"items":[{"id":3,"x":32,"y":64,"material":"pebble"}],"trees":[{"at":{"x":32400,"y":64700},"species":"birch"}],"plants":[{"kind":2,"x":32100,"y":64300}],"currents":[1059]}"#
        );
    }

    #[test]
    fn unknown_messages_are_rejected() {
        assert!(decode::<ClientMessage>(r#"{"type":"teleport"}"#).is_err());
        assert!(decode::<ClientMessage>("not json").is_err());
    }
}
