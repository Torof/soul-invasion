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
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use protocol::{
    BodyState, ClientMessage, Clock, Condition, DeathCause, FloraView, ItemAct, ItemState,
    ItemView, MaterialView, PROTOCOL_VERSION, PlantState, Point, Refusal, ServerMessage,
    SpeciesView, Terrain, TreeState, decode, encode,
};
use sim::{BodyId, CHUNK_SIZE, ChunkPos, Content, FinePos, Intention, ItemId, Map, Pos, World};
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
/// of `(2 × VIEW_RADIUS + 1)²` chunks, 416 m across; around it a ring of
/// outline chunks (`OUTLINE_RADIUS`) of which the client shows only the
/// first `OUTLINE_BAND` tiles, fading fast into a fog where the camera
/// stops.
pub const VIEW_RADIUS: i32 = 6;
/// Chunks held in outline around the view, out to this radius: their
/// ground and currents, and the trees and tall plants of their band
/// (`ChunkOutline`); nothing that lies or walks there.
pub const OUTLINE_RADIUS: i32 = 7;
/// Tiles of an outline chunk the client shows past the view's edge: its
/// trees and plants are sent only within this many tiles of the chunk's
/// borders (whichever faces the view, as the view moves).
pub const OUTLINE_BAND: i32 = 3;
/// Plants this tall and taller are in an outline chunk, in centimetres.
pub const OUTLINE_PLANT_CM: u32 = 50;

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
    },
    /// An item vanished from the ground.
    ItemRemoved {
        /// Which.
        item: u64,
        /// Where it was.
        at: Pos,
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
    pub map: Map,
    /// Latest known position of every body, kept by the simulation thread.
    pub bodies: watch::Sender<Vec<BodyState>>,
    /// What items are made of, as clients need it.
    pub materials: Vec<MaterialView>,
    /// The kinds of tree, as clients need them.
    pub species: Vec<SpeciesView>,
    /// The trees standing, by chunk. Static for now: nothing fells them.
    trees: BTreeMap<ChunkPos, Vec<TreeState>>,
    /// The kinds of shrub, plant and mushroom, as clients need them.
    pub flora: Vec<FloraView>,
    /// The content's flora (trees still to come included), to place it.
    flora_kinds: Vec<sim::flora::Kind>,
    /// Index in `flora` of each kind of the content's flora (`None` for
    /// the trees still to come).
    flora_wire: Vec<Option<u16>>,
    /// The seed the flora is placed with (the map's).
    flora_seed: u64,
    /// The plants of the chunks asked for so far, placed once each (the
    /// cost follows the chunks in use, never the map's size, D39).
    plants: Mutex<BTreeMap<ChunkPos, Vec<PlantState>>>,
    /// Is it night? Kept by the simulation thread.
    pub night: AtomicBool,
    /// Simulation ticks per game second.
    ticks_per_second: u32,
    /// Ticks per game day.
    ticks_per_day: u64,
    /// How many times faster than real time the server runs, in thousandths.
    time_scale_permille: u32,
    /// Items on the ground, by chunk, kept by the simulation thread.
    items: Mutex<BTreeMap<ChunkPos, Vec<ItemState>>>,
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
        let mut trees: BTreeMap<ChunkPos, Vec<TreeState>> = BTreeMap::new();
        for tree in world.trees() {
            trees
                .entry(tree.at.tile().chunk())
                .or_default()
                .push(TreeState {
                    at: point(tree.at),
                    species: species[tree.species].id.clone(),
                });
        }
        let (flora, flora_wire) = flora_views(content);
        let flora_seed = match world.map().spec() {
            sim::MapSpec::Valley { seed, .. } => seed,
            sim::MapSpec::Filled { .. } => 0,
        };
        let mut items: BTreeMap<ChunkPos, Vec<ItemState>> = BTreeMap::new();
        for item in world.items() {
            let Some(pos) = item.pos() else {
                continue; // carried: not on the ground
            };
            items.entry(pos.chunk()).or_default().push(ItemState {
                id: item.id().0,
                x: pos.x,
                y: pos.y,
                material: item.material().to_owned(),
            });
        }
        let bodies = world
            .bodies()
            .iter()
            .map(|b| BodyState {
                id: b.id().0,
                at: point(b.at()),
                route: b.route().map(point).collect(),
                speed_cm_per_s: content.movement.walk_speed_cm_per_s,
                asleep: b.condition().asleep,
                holds: b.held().and_then(|id| world.item(id)).map(|i| ItemView {
                    id: i.id().0,
                    material: i.material().to_owned(),
                }),
            })
            .collect();
        Self {
            tick: AtomicU64::new(world.tick().0),
            publish,
            map: world.map().clone(),
            bodies: watch::Sender::new(bodies),
            materials,
            species,
            trees,
            flora,
            flora_kinds: content.flora.clone(),
            flora_wire,
            flora_seed,
            plants: Mutex::new(BTreeMap::new()),
            night: AtomicBool::new(world.is_night(content)),
            ticks_per_second: world.calendar().ticks_per_second(),
            ticks_per_day: world.calendar().ticks_per_day(),
            time_scale_permille: permille(time_scale),
            items: Mutex::new(items),
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
        self.bodies.send_modify(|bodies| {
            if let Some(state) = bodies.iter_mut().find(|b| b.id == body.0) {
                state.at = point(from);
                state.route = route.iter().copied().map(point).collect();
                state.speed_cm_per_s = speed_cm_per_s;
            }
        });
    }

    /// Records a body's position as it crosses into another tile, keeping
    /// the last `remaining` waypoints of its route. Returns the tile it
    /// came from, or `None` for a body nobody knew about.
    pub fn body_moved(&self, body: BodyId, at: FinePos, remaining: usize) -> Option<Pos> {
        let mut from = None;
        self.bodies.send_modify(|bodies| {
            if let Some(state) = bodies.iter_mut().find(|b| b.id == body.0) {
                from = Some(tile_of(state.at));
                state.at = point(at);
                let passed = state.route.len().saturating_sub(remaining);
                state.route.drain(..passed);
            }
        });
        from
    }

    /// Records what a body holds in hand.
    pub fn body_holds(&self, body: BodyId, item: Option<ItemView>) {
        self.bodies.send_modify(|bodies| {
            if let Some(state) = bodies.iter_mut().find(|b| b.id == body.0) {
                state.holds = item;
            }
        });
    }

    /// Records whether a body is asleep; true when that changed.
    pub fn body_sleeps(&self, body: BodyId, asleep: bool) -> bool {
        let mut changed = false;
        self.bodies.send_modify(|bodies| {
            if let Some(state) = bodies.iter_mut().find(|b| b.id == body.0) {
                changed = state.asleep != asleep;
                state.asleep = asleep;
            }
        });
        changed
    }

    /// Forgets a dead body; returns the tile it died on, if it was known.
    pub fn body_died(&self, body: BodyId) -> Option<Pos> {
        let mut at = None;
        self.bodies.send_modify(|bodies| {
            if let Some(index) = bodies.iter().position(|b| b.id == body.0) {
                at = Some(tile_of(bodies.remove(index).at));
            }
        });
        at
    }

    /// Records an item lying on the ground.
    pub fn item_added(&self, item: ItemState) {
        self.items
            .lock()
            .expect("items not poisoned")
            .entry(Pos::new(item.x, item.y).chunk())
            .or_default()
            .push(item);
    }

    /// Forgets an item that was on the ground.
    pub fn item_removed(&self, item: ItemId, at: Pos) {
        if let Some(items) = self
            .items
            .lock()
            .expect("items not poisoned")
            .get_mut(&at.chunk())
        {
            items.retain(|i| i.id != item.0);
        }
    }

    /// Records that a body stopped exactly there.
    pub fn body_stopped(&self, body: BodyId, at: FinePos) {
        self.bodies.send_modify(|bodies| {
            if let Some(state) = bodies.iter_mut().find(|b| b.id == body.0) {
                state.at = point(at);
                state.route.clear();
            }
        });
    }

    /// The tile a body stands on, as last published.
    pub fn position_of(&self, body: u32) -> Option<Pos> {
        self.bodies
            .borrow()
            .iter()
            .find(|b| b.id == body)
            .map(|b| tile_of(b.at))
    }

    /// A body as last published.
    pub fn body_state(&self, body: u32) -> Option<BodyState> {
        self.bodies.borrow().iter().find(|b| b.id == body).cloned()
    }

    /// The bodies standing in a chunk, in id order.
    pub fn bodies_in(&self, chunk: ChunkPos) -> Vec<BodyState> {
        self.bodies
            .borrow()
            .iter()
            .filter(|b| tile_of(b.at).chunk() == chunk)
            .cloned()
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

    /// The message carrying a chunk and the bodies in it.
    fn chunk_message(&self, chunk: ChunkPos) -> Option<ServerMessage> {
        let tiles = self.map.chunk(chunk)?;
        Some(ServerMessage::Chunk {
            x: chunk.x,
            y: chunk.y,
            tiles: tiles.into_iter().map(terrain_view).collect(),
            bodies: self.bodies_in(chunk),
            items: self
                .items
                .lock()
                .expect("items not poisoned")
                .get(&chunk)
                .cloned()
                .unwrap_or_default(),
            trees: self.trees.get(&chunk).cloned().unwrap_or_default(),
            plants: self.plants_in(chunk),
            currents: self.map.chunk_currents(chunk)?,
        })
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

    /// The message carrying a chunk in outline: its ground and currents,
    /// the trees and tall plants of its band (`OUTLINE_BAND`); nothing
    /// that lies or walks there.
    fn outline_message(&self, chunk: ChunkPos) -> Option<ServerMessage> {
        let tiles = self.map.chunk(chunk)?;
        let tall = |p: &PlantState| {
            in_band(chunk, p.x, p.y)
                && self
                    .flora
                    .get(usize::from(p.kind))
                    .is_some_and(|k| k.height_cm >= OUTLINE_PLANT_CM)
        };
        Some(ServerMessage::ChunkOutline {
            x: chunk.x,
            y: chunk.y,
            tiles: tiles.into_iter().map(terrain_view).collect(),
            trees: self
                .trees
                .get(&chunk)
                .map(|t| {
                    t.iter()
                        .filter(|t| in_band(chunk, t.at.x, t.at.y))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
            plants: self.plants_in(chunk).into_iter().filter(tall).collect(),
            currents: self.map.chunk_currents(chunk)?,
        })
    }

    /// The plants of a chunk (`sim::flora`), placed the first time the
    /// chunk is asked for and kept.
    fn plants_in(&self, chunk: ChunkPos) -> Vec<PlantState> {
        if let Some(plants) = self.plants.lock().expect("plants not poisoned").get(&chunk) {
            return plants.clone();
        }
        let mut near = Vec::new();
        for dy in -1..=1 {
            for dx in -1..=1 {
                for tree in self
                    .trees
                    .get(&ChunkPos::new(chunk.x + dx, chunk.y + dy))
                    .into_iter()
                    .flatten()
                {
                    near.push(sim::flora::TreeAt {
                        at: FinePos::new(tree.at.x, tree.at.y),
                        species: tree.species.as_str(),
                    });
                }
            }
        }
        let plants: Vec<PlantState> =
            sim::flora::plants_in(&self.map, &self.flora_kinds, &near, self.flora_seed, chunk)
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
        self.plants
            .lock()
            .expect("plants not poisoned")
            .insert(chunk, plants.clone());
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

/// Whether a point (millimetres) of `chunk` lies within `OUTLINE_BAND`
/// tiles of one of the chunk's borders: the part of an outline chunk the
/// client may show, whichever side faces the view.
fn in_band(chunk: ChunkPos, x_mm: i32, y_mm: i32) -> bool {
    let side = i32::try_from(CHUNK_SIZE).expect("small chunk side");
    let local = |mm: i32, origin: i32| mm.div_euclid(1000) - origin * side;
    let (x, y) = (local(x_mm, chunk.x), local(y_mm, chunk.y));
    x < OUTLINE_BAND || y < OUTLINE_BAND || x >= side - OUTLINE_BAND || y >= side - OUTLINE_BAND
}

/// The chunks a connection holds: a square window around a centre chunk,
/// clipped to the map.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct View {
    chunks: BTreeSet<ChunkPos>,
    /// The chunks held in outline: the ring around `chunks`.
    outline: BTreeSet<ChunkPos>,
    /// The centre of the far view last sent, if any.
    far: Option<ChunkPos>,
}

impl View {
    /// The window around `centre`, clipped to the map.
    fn window(map: &Map, centre: ChunkPos) -> BTreeSet<ChunkPos> {
        Self::square(map, centre, VIEW_RADIUS)
    }

    /// The chunks within `radius` of `centre`, clipped to the map.
    fn square(map: &Map, centre: ChunkPos, radius: i32) -> BTreeSet<ChunkPos> {
        let mut chunks = BTreeSet::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
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

    /// Moves the outline ring to `centre` (call after `recentre`). Returns
    /// the chunks now in outline and those no longer, in a fixed order.
    pub fn recentre_outline(
        &mut self,
        map: &Map,
        centre: ChunkPos,
    ) -> (Vec<ChunkPos>, Vec<ChunkPos>) {
        let next: BTreeSet<ChunkPos> = Self::square(map, centre, OUTLINE_RADIUS)
            .difference(&self.chunks)
            .copied()
            .collect();
        let shown = next.difference(&self.outline).copied().collect();
        let hidden = self.outline.difference(&next).copied().collect();
        self.outline = next;
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

#[allow(clippy::too_many_lines)]
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
    let Some(mut pilot) = handshake(&mut ws, pilot, private_tx.clone()).await? else {
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
            species: shared.species.clone(),
            flora: shared.flora.clone(),
            night: shared.night.load(Ordering::Relaxed),
            clock: shared.clock(),
        },
    )
    .await?;
    if let Some(overview) = &shared.dev_overview {
        send(&mut ws, overview).await?;
    }

    // The view starts around the piloted body, or the middle of the map.
    let centre = pilot
        .body
        .and_then(|body| shared.position_of(body))
        .map_or_else(|| shared.centre_chunk(), Pos::chunk);
    let mut view = View::default();
    move_view(&mut ws, shared, &mut view, centre).await?;
    send(&mut ws, &far_view(&shared.map, centre)).await?;
    view.far = Some(centre);

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
                    Ok(ClientMessage::DevTeleport { .. }) if !shared.is_dev() => {
                        eprintln!("connection {peer}: teleport refused, the server is not in --dev");
                    }
                    Ok(message) => {
                        if let Some(body) = pilot.body {
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
                    let own_death = forward(&mut ws, shared, pilot.body, &mut view, published).await?;
                    if own_death {
                        reassign(&mut ws, shared, &mut pilot, &mut view, &private_tx, peer).await?;
                    }
                }
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

/// Provisional (D34): a pilot whose body died takes the first free body,
/// or watches, and the view moves there.
async fn reassign(
    ws: &mut WebSocketStream<TcpStream>,
    shared: &Shared,
    pilot: &mut Pilot<'_>,
    view: &mut View,
    private: &mpsc::Sender<ServerMessage>,
    peer: SocketAddr,
) -> Result<(), BoxError> {
    if let Some(dead) = pilot.body.take() {
        shared.release_body(dead);
    }
    pilot.body = shared.take_body(private.clone());
    match pilot.body {
        Some(body) => println!("connection {peer}: now pilots body {body}"),
        None => println!("connection {peer}: no free body, watching"),
    }
    send(ws, &ServerMessage::BodyAssigned { body: pilot.body }).await?;
    let centre = pilot
        .body
        .and_then(|body| shared.position_of(body))
        .map_or_else(|| shared.centre_chunk(), Pos::chunk);
    move_view(ws, shared, view, centre).await
}

/// Moves the view and its outline ring to `centre`, telling the client:
/// chunks no longer held hidden, new chunks of the view in full, new
/// chunks of the ring in outline (a chunk going from one to the other is
/// sent again, never hidden).
async fn move_view(
    ws: &mut WebSocketStream<TcpStream>,
    shared: &Shared,
    view: &mut View,
    centre: ChunkPos,
) -> Result<(), BoxError> {
    let (shown, hidden) = view.recentre(&shared.map, centre);
    let (outline_shown, outline_hidden) = view.recentre_outline(&shared.map, centre);
    for chunk in hidden
        .into_iter()
        .filter(|c| !view.outline.contains(c))
        .chain(
            outline_hidden
                .into_iter()
                .filter(|c| !view.chunks.contains(c)),
        )
    {
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
    for chunk in outline_shown {
        if let Some(message) = shared.outline_message(chunk) {
            send(ws, &message).await?;
        }
    }
    Ok(())
}

/// Forwards what the simulation published, as far as this connection's
/// view is concerned, moving the view along with the piloted body.
/// Returns true when the piloted body just died.
async fn forward(
    ws: &mut WebSocketStream<TcpStream>,
    shared: &Shared,
    piloted: Option<u32>,
    view: &mut View,
    published: Published,
) -> Result<bool, BoxError> {
    match published {
        Published::DayStarted { day, tick } => {
            send(ws, &ServerMessage::DayStarted { day, tick }).await?;
        }
        Published::NightFell { day } => {
            send(ws, &ServerMessage::NightFell { day }).await?;
        }
        Published::BodySleeps { body, at, asleep } => {
            if view.contains(at) {
                send(ws, &ServerMessage::BodySleeps { body, asleep }).await?;
            }
        }
        Published::BodyDied { body, at, cause } => {
            if view.contains(at) || Some(body) == piloted {
                send(ws, &ServerMessage::BodyDied { body, cause }).await?;
            }
            return Ok(Some(body) == piloted);
        }
        Published::ItemAdded { item } => {
            if view.contains(Pos::new(item.x, item.y)) {
                send(ws, &ServerMessage::ItemAdded { item }).await?;
            }
        }
        Published::ItemRemoved { item, at } => {
            if view.contains(at) {
                send(
                    ws,
                    &ServerMessage::ItemRemoved {
                        item,
                        x: at.x,
                        y: at.y,
                    },
                )
                .await?;
            }
        }
        Published::BodyHolds { body, at, item } => {
            if view.contains(at) {
                send(ws, &ServerMessage::BodyHolds { body, item }).await?;
            }
        }
        Published::BodyWalks { body, from, tick } => {
            if view.contains(from)
                && let Some(state) = shared.body_state(body)
            {
                send(ws, &walks_message(&state, tick)).await?;
            }
        }
        Published::BodyMoved {
            body,
            from,
            to,
            tick,
        } => {
            if Some(body) == piloted && from.chunk() != to.chunk() {
                move_view(ws, shared, view, to.chunk()).await?;
                // The far view again once the body has gone far enough
                // from where it was centred (a teleport, a long walk).
                let moved = view.far.is_none_or(|c| {
                    (c.x - to.chunk().x).abs().max((c.y - to.chunk().y).abs()) > FAR_RESEND
                });
                if moved {
                    send(ws, &far_view(&shared.map, to.chunk())).await?;
                    view.far = Some(to.chunk());
                }
            }
            let (was_seen, is_seen) = (view.contains(from), view.contains(to));
            if is_seen && !was_seen {
                // Walked into view: the client learns of it with its route.
                if let Some(state) = shared.body_state(body) {
                    send(ws, &walks_message(&state, tick)).await?;
                }
            } else if was_seen && !is_seen {
                send(ws, &ServerMessage::BodyLeft { body }).await?;
            }
        }
        Published::BodyStopped { body, at } => {
            if view.contains(at)
                && let Some(state) = shared.body_state(body)
            {
                send(ws, &ServerMessage::BodyStopped { body, at: state.at }).await?;
            }
        }
    }
    Ok(false)
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
        Shared::new(&World::default(), &Content::sample(), max_connections, 1.0)
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

        // The outline ring: around the window, never inside it; moving one
        // chunk east, the column leaving the window turns to outline.
        let mut ringed = View::default();
        ringed.recentre(&map, ChunkPos::new(12, 12));
        let (outline, _) = ringed.recentre_outline(&map, ChunkPos::new(12, 12));
        let wide = usize::try_from(2 * OUTLINE_RADIUS + 1).expect("small");
        assert_eq!(outline.len(), wide * wide - side * side);
        assert!(outline.iter().all(|c| !ringed.chunks.contains(c)));
        ringed.recentre(&map, ChunkPos::new(13, 12));
        let (now_outline, no_longer) = ringed.recentre_outline(&map, ChunkPos::new(13, 12));
        assert!(
            now_outline.contains(&ChunkPos::new(12 - r, 12)),
            "left the window: in outline"
        );
        assert!(
            no_longer.contains(&ChunkPos::new(13 + r, 12)),
            "entered the window: out of outline"
        );

        // Of an outline chunk, only the band along its borders is sent.
        let c = ChunkPos::new(1, 1);
        let at = |x: i32, y: i32| in_band(c, (32 + x) * 1000 + 500, (32 + y) * 1000 + 500);
        assert!(at(0, 16) && at(2, 16) && at(31, 16) && at(16, 29) && at(16, 0));
        assert!(!at(3, 16) && !at(16, 16) && !at(28, 28));

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
        assert_eq!(shared.body_died(BodyId(1)), Some(here));
        assert_eq!(shared.body_died(BodyId(1)), None);
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
}
