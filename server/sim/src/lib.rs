//! Pure world simulation for Soul Invasion.
//!
//! This crate has no I/O, no clock and no network (architecture decision D24).
//! The server feeds it intentions and asks it to advance a number of fixed
//! ticks; it returns the events that happened. Same inputs, same outputs.

// World generation is integer-only, so that a seed gives the same world on
// every machine and every build (D24): float arithmetic is linted in the
// whole crate (an error under `-D warnings`), and `clippy.toml` forbids the
// float maths functions. New code is covered by default. Code that is not
// generation (moving bodies, the time of day, content units) and the known
// exceptions of today's generator allow floats where they stand.
#![warn(clippy::float_arithmetic)]

pub mod content;
pub mod flora;
pub mod items;
// `gen` is a reserved keyword in the 2024 edition: the module is `r#gen`,
// its file `gen.rs`.
pub(crate) mod r#gen;
pub mod map;
pub mod time;
pub mod trees;
pub mod woods;

use std::collections::{BTreeSet, VecDeque};

pub use content::{
    Content, ContentError, Food, Look, Material, MaterialKind, Movement, Needs, Spawn,
};
pub use map::{CHUNK_SIZE, ChunkPos, FinePos, MM_PER_TILE, Map, MapSpec, Pos, Terrain, line_clear};
pub use time::{Calendar, CalendarError, GameTime, Tick};

/// Identifies a body (an inhabitant). Players and NPCs are the same kind
/// of body; only the pilot differs (D13).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct BodyId(pub u32);

/// Scale of a gauge: points are kept in millionths so that a rate per
/// day divides into whole steps per tick, deterministically.
const GAUGE_SCALE: u64 = 1_000_000;

/// The top of every gauge, in points.
pub const GAUGE_MAX: u32 = 100;

/// A 0–100 gauge, kept finely (D34).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Gauge(u64);

impl Gauge {
    /// A gauge at `points` (0–100).
    pub fn at(points: u32) -> Self {
        Self(u64::from(points.min(GAUGE_MAX)) * GAUGE_SCALE)
    }

    /// Whole points, 0–100.
    pub fn points(self) -> u32 {
        u32::try_from(self.0 / GAUGE_SCALE).expect("at most 100")
    }

    /// Is it at the top?
    pub fn is_full(self) -> bool {
        self.0 >= u64::from(GAUGE_MAX) * GAUGE_SCALE
    }

    /// Is it at the bottom?
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    fn raise(&mut self, by: u64) {
        self.0 = (self.0 + by).min(u64::from(GAUGE_MAX) * GAUGE_SCALE);
    }

    fn lower(&mut self, by: u64) {
        self.0 = self.0.saturating_sub(by);
    }
}

/// A part of a body that can be wounded (D33, D34).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Limb {
    /// The head.
    Head,
    /// The torso.
    Torso,
    /// The left arm.
    LeftArm,
    /// The right arm.
    RightArm,
    /// The left leg.
    LeftLeg,
    /// The right leg.
    RightLeg,
}

/// How bad a wound is.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Hampers a little, heals in days.
    Light,
    /// Hampers a lot, heals in weeks, may bleed.
    Grave,
    /// Kills.
    Mortal,
}

/// A wound on a limb. Carried empty until combat (D33) deals them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Wound {
    /// Where.
    pub limb: Limb,
    /// How bad.
    pub severity: Severity,
    /// Still bleeding?
    pub bleeding: bool,
}

/// What killed a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Death {
    /// Vitality drained by hunger.
    Starved,
    /// Vitality drained by cold.
    Froze,
}

/// The state of a body: what hurts and what kills (D34).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Condition {
    /// Life left; zero is death.
    pub vitality: Gauge,
    /// Need for food.
    pub hunger: Gauge,
    /// Lack of warmth.
    pub cold: Gauge,
    /// Need for sleep; full, the body collapses asleep.
    pub fatigue: Gauge,
    /// Asleep right now?
    pub asleep: bool,
    /// Wounds by limb.
    pub wounds: Vec<Wound>,
}

impl Condition {
    /// A healthy, fed, warm, rested body.
    pub fn healthy() -> Self {
        Self {
            vitality: Gauge::at(GAUGE_MAX),
            hunger: Gauge::at(0),
            cold: Gauge::at(0),
            fatigue: Gauge::at(0),
            asleep: false,
            wounds: Vec::new(),
        }
    }

    /// The whole points of every gauge and the sleep flag: what a client
    /// is told, and what counts as a change.
    pub fn summary(&self) -> (u32, u32, u32, u32, bool) {
        (
            self.vitality.points(),
            self.hunger.points(),
            self.cold.points(),
            self.fatigue.points(),
            self.asleep,
        )
    }
}

/// An inhabitant's body: where it is, where it is going, how it fares.
///
/// It stands at a fine position (millimetres, D32) and walks along a
/// route of straight segments at the speed the content gives its terrain.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Body {
    id: BodyId,
    at: FinePos,
    /// Remaining waypoints of the current walk, nearest first.
    route: VecDeque<FinePos>,
    condition: Condition,
    /// The item in hand, if any: visible to all, the tool or weapon (D35).
    held: Option<ItemId>,
    /// Everything else carried, in the order it was taken.
    carried: Vec<ItemId>,
}

impl Body {
    /// The body's id.
    pub fn id(&self) -> BodyId {
        self.id
    }

    /// How the body fares.
    pub fn condition(&self) -> &Condition {
        &self.condition
    }

    /// The item in hand, if any.
    pub fn held(&self) -> Option<ItemId> {
        self.held
    }

    /// The items carried but not in hand, in the order they were taken.
    pub fn carried(&self) -> &[ItemId] {
        &self.carried
    }

    /// Does the body have this item, in hand or carried?
    pub fn has(&self, item: ItemId) -> bool {
        self.held == Some(item) || self.carried.contains(&item)
    }

    /// Everything the body has, the item in hand first.
    pub fn inventory(&self) -> impl Iterator<Item = ItemId> + '_ {
        self.held.into_iter().chain(self.carried.iter().copied())
    }

    fn let_go(&mut self, item: ItemId) {
        if self.held == Some(item) {
            self.held = None;
        }
        self.carried.retain(|i| *i != item);
    }

    /// The tile the body stands on.
    pub fn pos(&self) -> Pos {
        self.at.tile()
    }

    /// Where the body stands, in millimetres.
    pub fn at(&self) -> FinePos {
        self.at
    }

    /// The waypoints still ahead, nearest first.
    pub fn route(&self) -> impl Iterator<Item = FinePos> + '_ {
        self.route.iter().copied()
    }

    /// Is the body walking somewhere?
    pub fn is_walking(&self) -> bool {
        !self.route.is_empty()
    }
}

/// Identifies a piece of material lying in the world.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ItemId(pub u64);

/// Where an item is: in exactly one place at a time (D35).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Place {
    /// Lying on a tile.
    Ground {
        /// The tile.
        pos: Pos,
    },
    /// Carried by a body, in hand or on it.
    Carried {
        /// The body.
        body: BodyId,
    },
}

/// A piece of material.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Item {
    id: ItemId,
    /// Content id of its material.
    material: String,
    place: Place,
}

impl Item {
    /// The item's id.
    pub fn id(&self) -> ItemId {
        self.id
    }

    /// Content id of the material it is made of.
    pub fn material(&self) -> &str {
        &self.material
    }

    /// Where it is.
    pub fn place(&self) -> Place {
        self.place
    }

    /// The tile it lies on, if it is on the ground.
    pub fn pos(&self) -> Option<Pos> {
        match self.place {
            Place::Ground { pos } => Some(pos),
            Place::Carried { .. } => None,
        }
    }
}

/// What a body can do with an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// No such body or item.
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

/// What a pilot (player or, later, NPC brain) asks a body to do (D20).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Intention {
    /// Walk to a tile.
    MoveTo {
        /// The body that should walk.
        body: BodyId,
        /// Where to.
        target: Pos,
    },
    /// Eat an item within reach on the ground, or carried.
    Eat {
        /// Who.
        body: BodyId,
        /// What.
        item: ItemId,
    },
    /// Take an item off the ground within reach: into the hand if it is
    /// empty, otherwise carried.
    PickUp {
        /// Who.
        body: BodyId,
        /// What.
        item: ItemId,
    },
    /// Lay a carried item on the body's tile.
    Drop {
        /// Who.
        body: BodyId,
        /// What.
        item: ItemId,
    },
    /// Take a carried item in hand; what was in hand is carried instead.
    Hold {
        /// Who.
        body: BodyId,
        /// What.
        item: ItemId,
    },
    /// Put the item in hand away, among what is carried.
    Stow {
        /// Who.
        body: BodyId,
        /// What.
        item: ItemId,
    },
    /// Lie down and sleep where one stands.
    Sleep {
        /// Who.
        body: BodyId,
    },
    /// Wake up.
    Wake {
        /// Who.
        body: BodyId,
    },
    /// Development only (the server accepts it only when started with
    /// `--dev`): stand at once on the walkable tile nearest `target`,
    /// within `TELEPORT_REACH`, to explore the map quickly. Refused when
    /// there is none.
    Teleport {
        /// Who.
        body: BodyId,
        /// Where to.
        target: Pos,
    },
}

/// How far from the asked tile a teleport looks for walkable ground.
pub const TELEPORT_REACH: i32 = 64;

/// Something that happened in the world during a tick.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// A new day began.
    DayStarted {
        /// The day that just started (days count from 1).
        day: u64,
    },
    /// A body set off along a route of straight segments.
    BodyWalks {
        /// Who.
        body: BodyId,
        /// Where it starts from.
        from: FinePos,
        /// The waypoints, nearest first, the last being the destination.
        route: Vec<FinePos>,
        /// Its nominal speed, on the reference terrain.
        speed_cm_per_s: u32,
    },
    /// A body stepped onto another tile.
    BodyMoved {
        /// Who.
        body: BodyId,
        /// The tile it now stands on.
        to: Pos,
        /// Exactly where.
        at: FinePos,
        /// Waypoints still ahead of it.
        remaining: usize,
    },
    /// A body stopped walking: arrived, or nothing left to walk.
    BodyStopped {
        /// Who.
        body: BodyId,
        /// Where.
        at: FinePos,
    },
    /// A walk could not start (unknown body, impassable or unreachable target).
    MoveRefused {
        /// Who.
        body: BodyId,
        /// Where it wanted to go.
        target: Pos,
    },
    /// Night fell: cold rises until dawn.
    NightFell {
        /// The day it ends.
        day: u64,
    },
    /// The whole points of a body's gauges changed, or it fell asleep or
    /// woke. At most once per body per tick.
    ConditionChanged {
        /// Who.
        body: BodyId,
        /// (vitality, hunger, cold, fatigue, asleep).
        summary: (u32, u32, u32, u32, bool),
    },
    /// A body ate an item, which is gone.
    BodyAte {
        /// Who.
        body: BodyId,
        /// What.
        item: ItemId,
    },
    /// An act on an item was refused.
    ActRefused {
        /// Who.
        body: BodyId,
        /// What it tried.
        act: ItemAct,
        /// On what.
        item: ItemId,
        /// Why.
        reason: Refusal,
    },
    /// What a body carries changed: in hand, carried, the load and the
    /// capacity in grams. Told to the pilot.
    InventoryChanged {
        /// Who.
        body: BodyId,
        /// In hand.
        held: Option<ItemId>,
        /// Carried, in order.
        carried: Vec<ItemId>,
        /// Grams carried in all.
        load_g: u32,
        /// Grams the body can carry.
        capacity_g: u32,
    },
    /// What a body holds in hand changed. Seen by all.
    BodyHolds {
        /// Who.
        body: BodyId,
        /// The item in hand now, if any.
        item: Option<ItemId>,
    },
    /// An item appeared on the ground.
    ItemAdded {
        /// Which.
        item: ItemId,
        /// Content id of its material.
        material: String,
        /// Where.
        at: Pos,
    },
    /// An item is gone from the ground.
    ItemRemoved {
        /// Which.
        item: ItemId,
        /// Where it was.
        at: Pos,
    },
    /// A body died; it is now a corpse item.
    BodyDied {
        /// Who.
        body: BodyId,
        /// Of what.
        cause: Death,
        /// Where.
        at: Pos,
        /// The corpse.
        corpse: ItemId,
    },
}

/// The world state: time, ground, bodies, things on the ground.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct World {
    calendar: Calendar,
    tick: Tick,
    map: Map,
    bodies: Vec<Body>,
    next_body: u32,
    /// The items a body has touched, made or let fall: stored. The ground's
    /// own (dead wood, pebbles) are made per chunk and never stored
    /// ([`items`]).
    items: Vec<Item>,
    next_item: u64,
    /// Whether the ground drops natural items ([`items`]); false for worlds
    /// made before them, whose scattered items are all in `items`.
    #[serde(default)]
    natural: bool,
    /// The natural items taken off the ground: they do not come back.
    #[serde(default)]
    taken: BTreeSet<ItemId>,
    /// The content's spawn rules the natural items follow, set with the
    /// trees ([`World::plant`]). Never persisted.
    #[serde(skip)]
    spawns: Vec<content::Spawn>,
    /// Intentions received since the last tick, applied in order at the
    /// start of the next one. Never persisted: a restart drops them.
    #[serde(skip)]
    pending: Vec<Intention>,
    /// The trees standing, planted from the map and the content
    /// ([`World::plant`]). Never persisted: regenerated on load.
    #[serde(skip)]
    trees: Vec<trees::Tree>,
}

/// Side of the first version's valley, in tiles: about 3 km, thirty-six
/// minutes of walking (D31).
pub const VALLEY_SIZE: u32 = 3072;

/// Seed of the first version's valley.
pub const VALLEY_SEED: u64 = 1;

impl World {
    /// Creates the first version's valley at tick zero, with a few
    /// inhabitants standing on the grass by the main river and the
    /// content's materials scattered over the ground by its spawn rules.
    pub fn new(calendar: Calendar, content: &Content) -> Self {
        Self::with_valley(calendar, content, VALLEY_SIZE)
    }

    /// The same, on a valley `size` tiles on a side: a smaller one starts
    /// far faster, for the tests and development (`--valley-size`).
    pub fn with_valley(calendar: Calendar, content: &Content, size: u32) -> Self {
        let mut world = Self::empty(calendar, Map::valley(size, size, VALLEY_SEED));
        let camp = world.map.camp();
        for (dx, dy) in [(0, 0), (2, 1), (-1, 3)] {
            let wanted = Pos::new(camp.x + dx, camp.y + dy);
            let pos = world
                .map
                .nearest(wanted, Terrain::Grass)
                .expect("the valley has grass");
            world.spawn_body(pos);
        }
        world.natural = true;
        world.plant(content);
        world
    }

    /// Creates a world on a given map with no bodies.
    pub fn empty(calendar: Calendar, map: Map) -> Self {
        Self {
            calendar,
            tick: Tick(0),
            map,
            bodies: Vec::new(),
            next_body: 1,
            items: Vec::new(),
            next_item: 1,
            natural: false,
            taken: BTreeSet::new(),
            spawns: Vec::new(),
            pending: Vec::new(),
            trees: Vec::new(),
        }
    }

    /// Plants the trees of the map with the content's kinds of tree (D37).
    /// Called when the world is created and when it is loaded.
    pub fn plant(&mut self, content: &Content) {
        let seed = match self.map.spec() {
            MapSpec::Valley { seed, .. } => seed,
            MapSpec::Filled { .. } => 0,
        };
        self.trees = trees::plant(&self.map, &content.species, seed);
        self.spawns.clone_from(&content.spawns);
    }

    /// The trees standing.
    pub fn trees(&self) -> &[trees::Tree] {
        &self.trees
    }

    /// The world's calendar.
    pub fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    /// The current tick.
    pub fn tick(&self) -> Tick {
        self.tick
    }

    /// The current game time.
    pub fn time(&self) -> GameTime {
        self.calendar.time_at(self.tick)
    }

    /// The ground.
    pub fn map(&self) -> &Map {
        &self.map
    }

    /// Every body, in id order.
    pub fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    /// A body by id.
    pub fn body(&self, id: BodyId) -> Option<&Body> {
        self.bodies.iter().find(|b| b.id == id)
    }

    /// The stored items: those a body touched, made or let fall, carried or
    /// on the ground, in the order they were stored. The ground's natural
    /// items are not among them until taken ([`World::items_in`]).
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Whether the ground drops natural items (worlds made since 10
    /// November 2026).
    pub fn has_natural_items(&self) -> bool {
        self.natural
    }

    /// The natural items taken off the ground.
    pub fn taken(&self) -> &BTreeSet<ItemId> {
        &self.taken
    }

    /// The seed the ground's natural items are made with: the map's.
    pub fn seed(&self) -> u64 {
        match self.map.spec() {
            MapSpec::Valley { seed, .. } => seed,
            MapSpec::Filled { .. } => 0,
        }
    }

    /// The ground's natural items of `chunk` still there, if this world
    /// has them.
    fn natural_in(&self, chunk: ChunkPos) -> Vec<Item> {
        if !self.natural {
            return Vec::new();
        }
        let mut natural = items::natural_items(&self.map, &self.spawns, self.seed(), chunk);
        natural.retain(|i| !self.taken.contains(&i.id));
        natural
    }

    /// The items lying on the ground in a chunk, stored and natural, in id
    /// order.
    pub fn items_in(&self, chunk: ChunkPos) -> Vec<Item> {
        let mut here: Vec<Item> = self
            .items
            .iter()
            .filter(|i| i.pos().is_some_and(|p| p.chunk() == chunk))
            .cloned()
            .collect();
        here.extend(self.natural_in(chunk));
        here.sort_by_key(|i| i.id);
        here
    }

    /// A stored item by id (natural items are stored once taken).
    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    /// Makes the natural item `id` a stored one, lying where it was, and
    /// remembers it was taken: what a body does with it is then done to a
    /// stored item. Nothing when it is not a natural item still there.
    fn materialise(&mut self, id: ItemId) {
        if !items::is_natural(id) || self.taken.contains(&id) {
            return;
        }
        if let Some(item) = self
            .natural_in(items::chunk_of(id))
            .into_iter()
            .find(|i| i.id == id)
        {
            self.items.push(item);
            self.taken.insert(id);
        }
    }

    /// Grams a body carries in all, under the content's masses.
    pub fn load_g(&self, body: &Body, rules: &Content) -> u32 {
        body.inventory()
            .filter_map(|id| self.item(id))
            .filter_map(|i| rules.material(&i.material))
            .map(Material::mass_g)
            .sum()
    }

    /// Lays a piece of `material` on the ground at `pos`. The material is
    /// the caller's responsibility: content is validated before the world
    /// is built.
    pub fn spawn_item(&mut self, material: &str, pos: Pos) -> ItemId {
        let id = ItemId(self.next_item);
        self.next_item += 1;
        self.items.push(Item {
            id,
            material: material.to_owned(),
            place: Place::Ground { pos },
        });
        id
    }

    /// Adds a body standing at `pos`. The tile must be passable.
    ///
    /// # Panics
    /// If `pos` is not a passable tile of the map.
    pub fn spawn_body(&mut self, pos: Pos) -> BodyId {
        assert!(self.map.is_passable(pos), "spawn on a passable tile");
        let id = BodyId(self.next_body);
        self.next_body += 1;
        self.bodies.push(Body {
            id,
            at: pos.centre(),
            route: VecDeque::new(),
            condition: Condition::healthy(),
            held: None,
            carried: Vec::new(),
        });
        id
    }

    /// Is it night? The day starts at dawn; night covers its last part.
    pub fn is_night(&self, rules: &Content) -> bool {
        let now = self.time();
        now.tick_of_day * 100
            >= u64::from(rules.needs.night_from_percent) * self.calendar.ticks_per_day()
    }

    /// Items lying on `pos` or on one of its eight neighbours, stored and
    /// natural, in id order.
    pub fn items_within_reach(&self, pos: Pos) -> Vec<Item> {
        let near = |p: Pos| (p.x - pos.x).abs() <= 1 && (p.y - pos.y).abs() <= 1;
        let mut chunks: Vec<ChunkPos> = [(-1, -1), (1, 1), (-1, 1), (1, -1)]
            .iter()
            .map(|(dx, dy)| Pos::new(pos.x + dx, pos.y + dy).chunk())
            .collect();
        chunks.sort_unstable_by_key(|c| (c.x, c.y));
        chunks.dedup();
        let mut here: Vec<Item> = chunks
            .into_iter()
            .flat_map(|c| self.items_in(c))
            .filter(|i| i.pos().is_some_and(near))
            .collect();
        here.sort_by_key(|i| i.id);
        here
    }

    /// Takes an item off the ground, if it is there.
    fn remove_item(&mut self, id: ItemId) -> Option<Item> {
        let index = self.items.iter().position(|i| i.id == id)?;
        Some(self.items.remove(index))
    }

    /// Queues an intention for the next tick.
    pub fn queue(&mut self, intention: Intention) {
        self.pending.push(intention);
    }

    /// Advances the world by exactly one fixed tick under the content's
    /// rules, appending the events of that tick to `events`.
    pub fn step(&mut self, rules: &Content, events: &mut Vec<Event>) {
        let was_night = self.is_night(rules);
        self.tick.0 += 1;
        let now = self.time();
        if now.is_day_start() {
            events.push(Event::DayStarted { day: now.day });
        }
        let night = self.is_night(rules);
        if night && !was_night {
            events.push(Event::NightFell { day: now.day });
        }

        let pending = std::mem::take(&mut self.pending);
        for intention in pending {
            self.apply(&intention, rules, events);
        }

        let per_second = f64::from(self.calendar.ticks_per_second());
        for index in 0..self.bodies.len() {
            if self.bodies[index].route.is_empty() {
                continue;
            }
            let load = self.load_g(&self.bodies[index], rules);
            let speed = loaded_speed(&rules.movement, load);
            walk(
                &mut self.bodies[index],
                &self.map,
                &rules.movement,
                speed,
                per_second,
                events,
            );
        }

        self.fare(rules, night, events);
    }

    /// Lets every body hunger, freeze, tire, rest, heal — and die.
    fn fare(&mut self, rules: &Content, night: bool, events: &mut Vec<Event>) {
        let per_day = self.calendar.ticks_per_day();
        let step = |rate_per_day: u32| u64::from(rate_per_day) * GAUGE_SCALE / per_day;
        let needs = &rules.needs;
        let mut died = Vec::new();
        for body in &mut self.bodies {
            let before = body.condition.summary();
            let c = &mut body.condition;
            c.hunger.raise(step(needs.hunger_per_day));
            if night {
                c.cold.raise(step(needs.cold_per_day_at_night));
            } else {
                c.cold.lower(step(needs.cold_relief_per_day));
            }
            if c.asleep {
                c.fatigue.lower(step(needs.rest_per_day_asleep));
                // Rested, one sleeps on through the night and wakes with the day.
                if c.fatigue.is_empty() && !night {
                    c.asleep = false;
                }
            } else {
                c.fatigue.raise(step(needs.fatigue_per_day_awake));
                if c.fatigue.is_full() {
                    c.asleep = true;
                    body.route.clear();
                }
            }
            let mut cause = None;
            if c.hunger.is_full() {
                c.vitality.lower(step(needs.starving_per_day));
                cause = Some(Death::Starved);
            }
            if c.cold.is_full() {
                c.vitality.lower(step(needs.freezing_per_day));
                cause = cause.or(Some(Death::Froze));
            }
            if cause.is_none() {
                c.vitality.raise(step(needs.vitality_regain_per_day));
            }
            if c.vitality.is_empty() {
                died.push((body.id, cause.expect("only a full need drains vitality")));
            } else if c.summary() != before {
                events.push(Event::ConditionChanged {
                    body: body.id,
                    summary: c.summary(),
                });
            }
        }
        for (id, cause) in died {
            self.die(id, cause, events);
        }
    }

    /// Turns a body into a corpse on the ground.
    fn die(&mut self, id: BodyId, cause: Death, events: &mut Vec<Event>) {
        let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
            return;
        };
        let body = self.bodies.remove(index);
        let at = body.pos();
        for item in body.inventory().collect::<Vec<_>>() {
            if let Some(dropped) = self.items.iter_mut().find(|i| i.id == item) {
                dropped.place = Place::Ground { pos: at };
                events.push(Event::ItemAdded {
                    item,
                    material: dropped.material.clone(),
                    at,
                });
            }
        }
        let corpse = self.spawn_item("corpse", at);
        events.push(Event::ItemAdded {
            item: corpse,
            material: "corpse".to_owned(),
            at,
        });
        events.push(Event::BodyDied {
            body: id,
            cause,
            at,
            corpse,
        });
    }

    /// Advances the world by `n` fixed ticks, appending events to `events`.
    pub fn advance(&mut self, n: u64, rules: &Content, events: &mut Vec<Event>) {
        for _ in 0..n {
            self.step(rules, events);
        }
    }

    fn apply(&mut self, intention: &Intention, rules: &Content, events: &mut Vec<Event>) {
        // An act on a natural item makes it a stored one first, then goes
        // as for any other.
        if let Intention::Eat { item, .. }
        | Intention::PickUp { item, .. }
        | Intention::Drop { item, .. }
        | Intention::Hold { item, .. }
        | Intention::Stow { item, .. } = *intention
        {
            self.materialise(item);
        }
        match *intention {
            Intention::MoveTo { body: id, target } => {
                let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
                    events.push(Event::MoveRefused { body: id, target });
                    return;
                };
                self.wake(index, events);
                let cost = |t: Terrain| rules.movement.cost(t);
                let from = self.bodies[index].at;
                if let Some(path) = self.map.path(from.tile(), target, cost) {
                    let route = self.map.pull(from, &path, cost);
                    let body = &mut self.bodies[index];
                    body.route = route.iter().copied().collect();
                    if route.is_empty() {
                        events.push(Event::BodyStopped { body: id, at: from });
                    } else {
                        events.push(Event::BodyWalks {
                            body: id,
                            from,
                            route,
                            speed_cm_per_s: rules.movement.walk_speed_cm_per_s,
                        });
                    }
                } else {
                    // No path found within reach: straight toward the
                    // target, stopping before the first tile the body
                    // may not enter (D31, D32). The player walks it
                    // round.
                    let walkable = |p: Pos| self.map.get(p).is_some_and(|t| cost(t).is_some());
                    let end = map::line_reach(from, target.centre(), walkable);
                    let (dx, dy) = (i64::from(end.x - from.x), i64::from(end.y - from.y));
                    let quarter = i64::from(MM_PER_TILE / 4);
                    if dx * dx + dy * dy < quarter * quarter {
                        events.push(Event::MoveRefused { body: id, target });
                    } else {
                        self.bodies[index].route = VecDeque::from([end]);
                        events.push(Event::BodyWalks {
                            body: id,
                            from,
                            route: vec![end],
                            speed_cm_per_s: rules.movement.walk_speed_cm_per_s,
                        });
                    }
                }
            }
            Intention::Teleport { body: id, target } => self.teleport(id, target, rules, events),
            Intention::Eat { body: id, item } => self.eat(id, item, rules, events),
            Intention::PickUp { body: id, item } => self.pick_up(id, item, rules, events),
            Intention::Drop { body: id, item } => self.drop(id, item, rules, events),
            Intention::Hold { body: id, item } => self.hold(id, item, rules, events),
            Intention::Stow { body: id, item } => self.stow(id, item, rules, events),
            Intention::Sleep { body: id } => {
                if let Some(index) = self.bodies.iter().position(|b| b.id == id) {
                    let body = &mut self.bodies[index];
                    if !body.condition.asleep {
                        body.condition.asleep = true;
                        if !body.route.is_empty() {
                            body.route.clear();
                            events.push(Event::BodyStopped {
                                body: id,
                                at: body.at,
                            });
                        }
                        events.push(Event::ConditionChanged {
                            body: id,
                            summary: body.condition.summary(),
                        });
                    }
                }
            }
            Intention::Wake { body: id } => {
                if let Some(index) = self.bodies.iter().position(|b| b.id == id) {
                    self.wake(index, events);
                }
            }
        }
    }

    /// Where an item is relative to a body: within reach on the ground,
    /// carried by it, or neither.
    fn reach(&self, index: usize, item: ItemId) -> Result<Place, Refusal> {
        let body = &self.bodies[index];
        let Some(found) = self.item(item) else {
            return Err(Refusal::Unknown);
        };
        match found.place {
            Place::Carried { body: carrier } if carrier == body.id => Ok(found.place),
            Place::Carried { .. } => Err(Refusal::NotCarried),
            Place::Ground { pos } => {
                let here = body.pos();
                if (pos.x - here.x).abs() <= 1 && (pos.y - here.y).abs() <= 1 {
                    Ok(found.place)
                } else {
                    Err(Refusal::OutOfReach)
                }
            }
        }
    }

    fn refuse(events: &mut Vec<Event>, body: BodyId, act: ItemAct, item: ItemId, reason: Refusal) {
        events.push(Event::ActRefused {
            body,
            act,
            item,
            reason,
        });
    }

    /// Why a body cannot let go of an item it does not have.
    fn not_had(&self, item: ItemId) -> Refusal {
        if self.item(item).is_some() {
            Refusal::NotCarried
        } else {
            Refusal::Unknown
        }
    }

    /// Tells what a body carries now.
    fn inventory_changed(&self, index: usize, rules: &Content, events: &mut Vec<Event>) {
        let body = &self.bodies[index];
        events.push(Event::InventoryChanged {
            body: body.id,
            held: body.held,
            carried: body.carried.clone(),
            load_g: self.load_g(body, rules),
            capacity_g: rules.movement.carry_capacity_g,
        });
    }

    /// A development teleport (`Intention::Teleport`): the body stands at
    /// once on the walkable tile nearest `target`, ring by ring, awake and
    /// with no route; it is seen to move there and stop.
    fn teleport(&mut self, id: BodyId, target: Pos, rules: &Content, events: &mut Vec<Event>) {
        let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
            events.push(Event::MoveRefused { body: id, target });
            return;
        };
        let walkable = |p: Pos| {
            self.map
                .get(p)
                .is_some_and(|t| rules.movement.cost(t).is_some())
        };
        let found = (0..=TELEPORT_REACH).find_map(|r| {
            (-r..=r)
                .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
                .filter(|(dx, dy)| dx.abs() == r || dy.abs() == r)
                .map(|(dx, dy)| Pos::new(target.x + dx, target.y + dy))
                .find(|p| walkable(*p))
        });
        let Some(tile) = found else {
            events.push(Event::MoveRefused { body: id, target });
            return;
        };
        self.wake(index, events);
        let body = &mut self.bodies[index];
        body.route.clear();
        body.at = tile.centre();
        events.push(Event::BodyMoved {
            body: id,
            to: tile,
            at: body.at,
            remaining: 0,
        });
        events.push(Event::BodyStopped {
            body: id,
            at: body.at,
        });
    }

    fn eat(&mut self, id: BodyId, item: ItemId, rules: &Content, events: &mut Vec<Event>) {
        let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
            return Self::refuse(events, id, ItemAct::Eat, item, Refusal::Unknown);
        };
        let place = match self.reach(index, item) {
            Ok(place) => place,
            Err(reason) => return Self::refuse(events, id, ItemAct::Eat, item, reason),
        };
        let food = self
            .item(item)
            .and_then(|i| rules.material(&i.material))
            .and_then(|m| m.food);
        let Some(food) = food else {
            return Self::refuse(events, id, ItemAct::Eat, item, Refusal::NotFood);
        };
        self.wake(index, events);
        self.remove_item(item);
        let relief =
            u64::from(food.energy_kj) * GAUGE_SCALE / u64::from(rules.needs.kj_per_hunger_point);
        self.bodies[index].condition.hunger.lower(relief);
        match place {
            Place::Ground { pos } => events.push(Event::ItemRemoved { item, at: pos }),
            Place::Carried { .. } => {
                let was_held = self.bodies[index].held == Some(item);
                self.bodies[index].let_go(item);
                self.inventory_changed(index, rules, events);
                if was_held {
                    events.push(Event::BodyHolds {
                        body: id,
                        item: None,
                    });
                }
            }
        }
        events.push(Event::BodyAte { body: id, item });
    }

    fn pick_up(&mut self, id: BodyId, item: ItemId, rules: &Content, events: &mut Vec<Event>) {
        let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
            return Self::refuse(events, id, ItemAct::PickUp, item, Refusal::Unknown);
        };
        let pos = match self.reach(index, item) {
            Ok(Place::Ground { pos }) => pos,
            Ok(Place::Carried { .. }) => {
                return Self::refuse(events, id, ItemAct::PickUp, item, Refusal::NotCarried);
            }
            Err(reason) => return Self::refuse(events, id, ItemAct::PickUp, item, reason),
        };
        let mass = self
            .item(item)
            .and_then(|i| rules.material(&i.material))
            .map_or(0, Material::mass_g);
        if self.load_g(&self.bodies[index], rules) + mass > rules.movement.carry_capacity_g {
            return Self::refuse(events, id, ItemAct::PickUp, item, Refusal::TooHeavy);
        }
        self.wake(index, events);
        if let Some(taken) = self.items.iter_mut().find(|i| i.id == item) {
            taken.place = Place::Carried { body: id };
        }
        let body = &mut self.bodies[index];
        let in_hand = body.held.is_none();
        if in_hand {
            body.held = Some(item);
        } else {
            body.carried.push(item);
        }
        events.push(Event::ItemRemoved { item, at: pos });
        self.inventory_changed(index, rules, events);
        if in_hand {
            events.push(Event::BodyHolds {
                body: id,
                item: Some(item),
            });
        }
    }

    fn drop(&mut self, id: BodyId, item: ItemId, rules: &Content, events: &mut Vec<Event>) {
        let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
            return Self::refuse(events, id, ItemAct::Drop, item, Refusal::Unknown);
        };
        if !self.bodies[index].has(item) {
            let reason = self.not_had(item);
            return Self::refuse(events, id, ItemAct::Drop, item, reason);
        }
        self.wake(index, events);
        let at = self.bodies[index].pos();
        let was_held = self.bodies[index].held == Some(item);
        self.bodies[index].let_go(item);
        let Some(dropped) = self.items.iter_mut().find(|i| i.id == item) else {
            return;
        };
        dropped.place = Place::Ground { pos: at };
        let material = dropped.material.clone();
        events.push(Event::ItemAdded { item, material, at });
        self.inventory_changed(index, rules, events);
        if was_held {
            events.push(Event::BodyHolds {
                body: id,
                item: None,
            });
        }
    }

    fn hold(&mut self, id: BodyId, item: ItemId, rules: &Content, events: &mut Vec<Event>) {
        let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
            return Self::refuse(events, id, ItemAct::Hold, item, Refusal::Unknown);
        };
        if self.bodies[index].held == Some(item) {
            return;
        }
        if !self.bodies[index].carried.contains(&item) {
            let reason = self.not_had(item);
            return Self::refuse(events, id, ItemAct::Hold, item, reason);
        }
        let body = &mut self.bodies[index];
        body.carried.retain(|i| *i != item);
        if let Some(previous) = body.held.replace(item) {
            body.carried.push(previous);
        }
        self.inventory_changed(index, rules, events);
        events.push(Event::BodyHolds {
            body: id,
            item: Some(item),
        });
    }

    fn stow(&mut self, id: BodyId, item: ItemId, rules: &Content, events: &mut Vec<Event>) {
        let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
            return Self::refuse(events, id, ItemAct::Stow, item, Refusal::Unknown);
        };
        if self.bodies[index].held != Some(item) {
            let reason = self.not_had(item);
            return Self::refuse(events, id, ItemAct::Stow, item, reason);
        }
        let body = &mut self.bodies[index];
        body.held = None;
        body.carried.push(item);
        self.inventory_changed(index, rules, events);
        events.push(Event::BodyHolds {
            body: id,
            item: None,
        });
    }

    /// Wakes a body up, if asleep; any deliberate act does.
    fn wake(&mut self, index: usize, events: &mut Vec<Event>) {
        let body = &mut self.bodies[index];
        if body.condition.asleep {
            body.condition.asleep = false;
            events.push(Event::ConditionChanged {
                body: body.id,
                summary: body.condition.summary(),
            });
        }
    }
}

/// Moves a walking body for one tick: as far along its route as the
/// speed of the terrain it stands on allows, waypoint after waypoint.
/// Walking speed under a load, in percent of the unloaded speed: linear
/// from 100 % empty to the content's percent at full capacity.
fn loaded_speed(movement: &Movement, load_g: u32) -> u32 {
    let full = u64::from(movement.carry_capacity_g);
    let load = u64::from(load_g).min(full);
    let slowest = u64::from(movement.speed_percent_at_full_load);
    u32::try_from(100 - (100 - slowest) * load / full).expect("percent")
}

// Not generation: a body's walk is worked out in floats.
#[allow(clippy::cast_possible_truncation, clippy::float_arithmetic)]
fn walk(
    body: &mut Body,
    map: &Map,
    movement: &Movement,
    speed_percent: u32,
    ticks_per_second: f64,
    events: &mut Vec<Event>,
) {
    let terrain = map.get(body.at.tile()).unwrap_or(Terrain::Rock);
    // A straight segment may graze the corner of a tile it may not enter
    // (`line_clear`), and a body placed to the millimetre can stand a few
    // millimetres into it: it goes on at the slowest pace, never stuck.
    let speed = match movement.speed_cm_per_s(terrain) {
        0 => movement.slowest_speed_cm_per_s(),
        speed => speed,
    };
    // Millimetres this tick: 143 cm/s at 10 ticks/s is exactly 143 mm.
    let mut budget = f64::from(speed * 10 * speed_percent) / 100.0 / ticks_per_second;
    let tile_before = body.at.tile();
    while budget > 0.0 {
        let Some(&next) = body.route.front() else {
            break;
        };
        let distance = body.at.distance_to(next);
        if distance <= budget {
            body.at = next;
            body.route.pop_front();
            budget -= distance;
        } else {
            let ratio = budget / distance;
            body.at = FinePos::new(
                body.at.x + (f64::from(next.x - body.at.x) * ratio).round() as i32,
                body.at.y + (f64::from(next.y - body.at.y) * ratio).round() as i32,
            );
            budget = 0.0;
        }
    }
    let tile = body.at.tile();
    if tile != tile_before {
        events.push(Event::BodyMoved {
            body: body.id,
            to: tile,
            at: body.at,
            remaining: body.route.len(),
        });
    }
    if body.route.is_empty() {
        events.push(Event::BodyStopped {
            body: body.id,
            at: body.at,
        });
    }
}

impl Default for World {
    /// The standard valley with the sample content.
    fn default() -> Self {
        Self::new(Calendar::default(), &Content::sample())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Content {
        Content::sample()
    }

    fn small_world() -> World {
        // 4 ticks per day: easy to reason about. A valley of 768: the same
        // rules as the game's, made in a fraction of the time.
        World::with_valley(Calendar::new(2, 2).unwrap(), &rules(), 768)
    }

    fn open_field() -> World {
        let mut world = World::empty(
            Calendar::new(10, 100).unwrap(),
            Map::filled(8, 8, Terrain::Grass),
        );
        world.spawn_body(Pos::new(0, 0));
        world
    }

    fn go(world: &mut World, target: Pos) {
        world.queue(Intention::MoveTo {
            body: BodyId(1),
            target,
        });
    }

    /// Runs until body 1 stops, returning the ticks it took.
    fn until_stopped(world: &mut World, events: &mut Vec<Event>) -> u64 {
        let rules = rules();
        for n in 1..10_000 {
            world.step(&rules, events);
            if events.iter().any(|e| {
                matches!(
                    e,
                    Event::BodyStopped {
                        body: BodyId(1),
                        ..
                    }
                )
            }) {
                return n;
            }
        }
        panic!("body 1 never stopped");
    }

    #[test]
    fn new_world_starts_at_tick_zero_on_day_one_with_bodies() {
        let w = World::default();
        assert_eq!(w.tick(), Tick(0));
        assert_eq!(w.time().day, 1);
        assert_eq!(w.bodies().len(), 3);
        assert!(w.bodies().iter().all(|b| w.map().is_passable(b.pos())));
        assert!(w.bodies().iter().all(|b| b.at() == b.pos().centre()));
        assert_eq!(
            (w.map().width(), w.map().height()),
            (VALLEY_SIZE, VALLEY_SIZE)
        );
        let text = serde_json::to_string(&w).unwrap();
        assert!(
            !text.contains("\"tiles\""),
            "terrain is not in the snapshot"
        );
        assert!(text.contains("\"items\""), "items are");
    }

    #[test]
    fn a_day_started_event_fires_on_each_day_boundary() {
        let mut w = small_world();
        let mut events = Vec::new();
        w.advance(12, &rules(), &mut events); // three full days
        events.retain(|e| matches!(e, Event::DayStarted { .. }));
        assert_eq!(
            events,
            vec![
                Event::DayStarted { day: 2 },
                Event::DayStarted { day: 3 },
                Event::DayStarted { day: 4 },
            ]
        );
        assert_eq!(w.time().day, 4);
        assert!(w.time().is_day_start());
    }

    #[test]
    fn no_event_before_the_first_boundary() {
        let mut w = small_world();
        let mut events = Vec::new();
        w.advance(3, &rules(), &mut events);
        assert!(!events.iter().any(|e| matches!(e, Event::DayStarted { .. })));
        assert_eq!(w.time().day, 1);
    }

    #[test]
    fn a_teleport_stands_the_body_on_walkable_ground_nearby() {
        let mut w = small_world();
        let rules = rules();
        let mut events = Vec::new();
        // Into the cliff (rock), 20 rows above its foot: the nearest
        // walkable tile below it.
        let foot = i32::try_from(w.map().height() / 8).expect("small");
        let target = Pos::new(100, foot - 20);
        w.queue(Intention::Teleport {
            body: BodyId(1),
            target,
        });
        w.advance(1, &rules, &mut events);
        let at = w.body(BodyId(1)).expect("body 1").pos();
        assert!(
            rules
                .movement
                .cost(w.map().get(at).expect("on the map"))
                .is_some(),
            "{at:?}"
        );
        assert!(
            (at.x - target.x).abs() <= TELEPORT_REACH && (at.y - target.y).abs() <= TELEPORT_REACH
        );
        assert!(events.iter().any(
            |e| matches!(e, Event::BodyMoved { body: BodyId(1), to, remaining: 0, .. } if *to == at)
        ));
        assert!(events.iter().any(|e| matches!(
            e,
            Event::BodyStopped {
                body: BodyId(1),
                ..
            }
        )));
        // Far past the map: nothing walkable within reach, refused.
        let far = Pos::new(-1000, -1000);
        events.clear();
        w.queue(Intention::Teleport {
            body: BodyId(1),
            target: far,
        });
        w.advance(1, &rules, &mut events);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::MoveRefused { target, .. } if *target == far))
        );
        assert_eq!(w.body(BodyId(1)).expect("body 1").pos(), at);
    }

    #[test]
    fn advancing_is_deterministic() {
        let mut a = small_world();
        let mut b = small_world();
        let (mut ea, mut eb) = (Vec::new(), Vec::new());
        // Some grass near body 1, whatever the valley's size.
        let start = a.body(BodyId(1)).expect("body 1").pos();
        let target = a
            .map()
            .nearest(Pos::new(start.x + 20, start.y), Terrain::Grass)
            .expect("grass near the camp");
        go(&mut a, target);
        go(&mut b, target);
        a.advance(1_000, &rules(), &mut ea);
        b.advance(1_000, &rules(), &mut eb);
        assert_eq!(a.tick(), b.tick());
        assert_eq!(ea, eb);
        assert_eq!(a.bodies(), b.bodies());
        assert!(ea.iter().any(|e| matches!(e, Event::BodyMoved { .. })));
    }

    #[test]
    fn a_body_walks_at_the_content_speed_and_arrives() {
        // 143 cm/s at 10 ticks/s: 143 mm per tick; two tiles east is
        // 2 000 mm from centre to centre, so 14 ticks.
        let mut w = open_field();
        let mut events = Vec::new();
        go(&mut w, Pos::new(2, 0));
        w.step(&rules(), &mut events); // tick 1: the walk starts and the body moves at once
        assert_eq!(
            events[0],
            Event::BodyWalks {
                body: BodyId(1),
                from: Pos::new(0, 0).centre(),
                route: vec![Pos::new(2, 0).centre()],
                speed_cm_per_s: 143,
            },
            "one straight segment, no step through tile (1, 0)"
        );
        assert_eq!(w.body(BodyId(1)).unwrap().at(), FinePos::new(643, 500));
        events.clear();
        let took = 1 + until_stopped(&mut w, &mut events);
        assert_eq!(took, 14);
        let body = w.body(BodyId(1)).unwrap();
        assert_eq!(body.at(), Pos::new(2, 0).centre());
        assert!(!body.is_walking());
        let moved: Vec<Pos> = events
            .iter()
            .filter_map(|e| match e {
                Event::BodyMoved { to, .. } => Some(*to),
                _ => None,
            })
            .collect();
        assert_eq!(
            moved,
            vec![Pos::new(1, 0), Pos::new(2, 0)],
            "one event per tile crossed"
        );
        assert_eq!(
            events.last(),
            Some(&Event::BodyStopped {
                body: BodyId(1),
                at: Pos::new(2, 0).centre()
            })
        );
    }

    #[test]
    fn a_diagonal_walk_is_one_straight_segment() {
        let mut w = open_field();
        let mut events = Vec::new();
        go(&mut w, Pos::new(3, 3));
        // 3 000√2 = 4 243 mm at 143 mm per tick: 30 ticks.
        let took = until_stopped(&mut w, &mut events);
        assert_eq!(took, 30);
        assert!(matches!(&events[0], Event::BodyWalks { route, .. } if route.len() == 1));
        assert_eq!(w.body(BodyId(1)).unwrap().at(), Pos::new(3, 3).centre());
    }

    #[test]
    fn forest_slows_the_walk_down() {
        let mut w = World::empty(
            Calendar::new(10, 100).unwrap(),
            Map::filled(8, 1, Terrain::Forest),
        );
        w.spawn_body(Pos::new(0, 0));
        let mut events = Vec::new();
        go(&mut w, Pos::new(2, 0));
        // 95 cm/s in the forest: 95 mm per tick, 2 000 mm: 22 ticks.
        assert_eq!(until_stopped(&mut w, &mut events), 22);
    }

    #[test]
    fn a_new_intention_replaces_the_current_walk() {
        let mut w = open_field();
        let mut events = Vec::new();
        go(&mut w, Pos::new(5, 0));
        w.step(&rules(), &mut events);
        go(&mut w, Pos::new(1, 3));
        w.advance(60, &rules(), &mut events);
        assert_eq!(w.body(BodyId(1)).unwrap().pos(), Pos::new(1, 3));
        assert_eq!(w.body(BodyId(1)).unwrap().at(), Pos::new(1, 3).centre());
    }

    #[test]
    fn walking_to_where_one_stands_stops_at_once() {
        let mut w = open_field();
        let mut events = Vec::new();
        go(&mut w, Pos::new(0, 0));
        w.step(&rules(), &mut events);
        assert_eq!(
            events,
            vec![Event::BodyStopped {
                body: BodyId(1),
                at: Pos::new(0, 0).centre()
            }]
        );
    }

    #[test]
    fn walks_without_a_body_are_refused_and_off_the_map_stop_at_its_edge() {
        let mut w = open_field();
        let mut events = Vec::new();
        go(&mut w, Pos::new(50, 50));
        w.queue(Intention::MoveTo {
            body: BodyId(9),
            target: Pos::new(1, 1),
        });
        w.step(&rules(), &mut events);
        assert_eq!(
            events,
            vec![
                // No path off the map: straight to its edge, 30 cm short.
                Event::BodyWalks {
                    body: BodyId(1),
                    from: Pos::new(0, 0).centre(),
                    route: vec![FinePos::new(7788, 7788)],
                    speed_cm_per_s: rules().movement.walk_speed_cm_per_s,
                },
                Event::MoveRefused {
                    body: BodyId(9),
                    target: Pos::new(1, 1)
                },
            ]
        );
    }

    /// Runs `world` until body 1 stops, at most `ticks` ticks.
    fn walk_until_stopped(world: &mut World, ticks: u32) -> Vec<Event> {
        let rules = rules();
        let mut events = Vec::new();
        for _ in 0..ticks {
            world.step(&rules, &mut events);
            if events.iter().any(|e| {
                matches!(
                    e,
                    Event::BodyStopped {
                        body: BodyId(1),
                        ..
                    } | Event::MoveRefused { .. }
                )
            }) {
                return events;
            }
        }
        panic!("body 1 never stopped");
    }

    #[test]
    fn with_no_path_within_reach_the_body_walks_straight_to_the_obstacle() {
        // A target farther than a path may go, walled in by rock: the
        // body walks straight toward it and stops before the rock. One
        // tick a second, days long enough not to tire.
        let mut map = Map::filled(1500, 1500, Terrain::Grass);
        for (x, y) in [(0, 1), (1, 0), (1, 1)] {
            map.set(Pos::new(x, y), Terrain::Rock);
        }
        const { assert!(1499 > map::PATH_REACH) };
        let mut w = World::empty(Calendar::new(1, 1_000_000).unwrap(), map);
        w.spawn_body(Pos::new(1499, 1499));
        go(&mut w, Pos::new(0, 0));
        let events = walk_until_stopped(&mut w, 5000);
        assert!(matches!(events[0], Event::BodyWalks { ref route, .. } if route.len() == 1));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::MoveRefused { .. }))
        );
        assert_eq!(w.body(BodyId(1)).expect("body 1").pos(), Pos::new(2, 2));
    }

    #[test]
    fn a_body_grazing_a_water_corner_walks_on() {
        // Found on the 768 valley: a straight walk passing 3 cm from the
        // corner of a water tile left the body a few millimetres into it,
        // its speed zero, stuck for good. It must walk on.
        let mut map = Map::filled(20, 20, Terrain::Grass);
        map.set(Pos::new(5, 5), Terrain::Water);
        let mut w = World::empty(Calendar::new(1, 1_000_000).unwrap(), map);
        w.spawn_body(Pos::new(2, 2));
        let body = &mut w.bodies[0];
        body.at = FinePos::new(5020, 5030);
        body.route = VecDeque::from([Pos::new(9, 9).centre()]);
        walk_until_stopped(&mut w, 100);
        assert_eq!(w.body(BodyId(1)).expect("body 1").pos(), Pos::new(9, 9));
    }

    #[test]
    fn across_a_river_with_its_ford_out_of_reach_the_body_walks_to_the_bank() {
        // A river with one ford far upstream: within reach the body takes
        // the ford; beyond it, it walks straight to the bank and stops.
        let mut map = Map::filled(900, 200, Terrain::Grass);
        for x in 0..900 {
            map.set(Pos::new(x, 100), Terrain::Water);
        }
        map.set(Pos::new(850, 100), Terrain::Ford);
        let mut w = World::empty(Calendar::new(1, 1_000_000).unwrap(), map.clone());
        w.spawn_body(Pos::new(10, 90));
        go(&mut w, Pos::new(10, 110));
        walk_until_stopped(&mut w, 5000);
        assert_eq!(w.body(BodyId(1)).expect("body 1").pos(), Pos::new(10, 99));

        let mut w = World::empty(Calendar::new(1, 1_000_000).unwrap(), map);
        w.spawn_body(Pos::new(800, 90));
        go(&mut w, Pos::new(800, 110));
        walk_until_stopped(&mut w, 5000);
        assert_eq!(
            w.body(BodyId(1)).expect("body 1").pos(),
            Pos::new(800, 110),
            "by the ford"
        );
    }

    #[test]
    fn the_ground_drops_natural_items_by_the_spawn_rules() {
        let content = Content::sample();
        let a = World::with_valley(Calendar::default(), &content, 768);
        // None stored: the ground makes them per chunk.
        assert_eq!(a.items(), []);
        assert!(a.has_natural_items());
        let chunks = || (0..24).flat_map(|y| (0..24).map(move |x| ChunkPos::new(x, y)));
        let all: Vec<Item> = chunks().flat_map(|c| a.items_in(c)).collect();
        let b = World::with_valley(Calendar::default(), &content, 768);
        assert_eq!(
            all,
            chunks().flat_map(|c| b.items_in(c)).collect::<Vec<_>>()
        );
        // Every item obeys a rule: on its terrain, near what it must be.
        for item in &all {
            let pos = item.pos().unwrap();
            let rule = content
                .spawns
                .iter()
                .find(|s| s.material == item.material())
                .expect("a rule for each material laid down");
            assert_eq!(a.map().get(pos), Some(rule.terrain), "{item:?}");
            if let Some(near) = rule.near {
                let r = i32::try_from(near.within).unwrap();
                assert!(
                    (-r..=r).any(|dy| (-r..=r)
                        .any(|dx| a.map().get(Pos::new(pos.x + dx, pos.y + dy))
                            == Some(near.terrain))),
                    "{item:?} is far from its {:?}",
                    near.terrain
                );
            }
        }
        // Densities land near their rule, over tens of thousands of tiles.
        let forest = a
            .map()
            .tiles()
            .iter()
            .filter(|t| **t == Terrain::Forest)
            .count();
        let branches = all.iter().filter(|i| i.material() == "branch").count();
        let per_thousand = branches * 1000 / forest;
        assert!(
            (45..=75).contains(&per_thousand),
            "{per_thousand}‰ branches"
        );
        assert_eq!(a.items_in(ChunkPos::new(-1, -1)), [] as [Item; 0]);
    }

    #[test]
    fn a_natural_item_taken_is_stored_and_never_comes_back() {
        let content = Content::sample();
        let mut w = World::with_valley(Calendar::new(10, 100_000).unwrap(), &content, 768);
        // A natural item, and a body set beside it.
        let item = (0..24)
            .flat_map(|y| (0..24).map(move |x| ChunkPos::new(x, y)))
            .flat_map(|c| w.items_in(c))
            .find(|i| {
                let p = i.pos().unwrap();
                w.map().is_passable(p)
            })
            .expect("a natural item on walkable ground");
        let at = item.pos().unwrap();
        assert!(items::is_natural(item.id()));
        let body = w.spawn_body(at);
        w.queue(Intention::PickUp {
            body,
            item: item.id(),
        });
        let mut events = Vec::new();
        w.step(&content, &mut events);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ItemRemoved { item: i, .. } if *i == item.id())),
            "{events:?}"
        );
        // Carried now, stored, and gone from its chunk for good.
        assert!(w.body(body).unwrap().has(item.id()));
        assert!(w.item(item.id()).is_some());
        assert!(w.taken().contains(&item.id()));
        assert!(w.items_in(at.chunk()).iter().all(|i| i.id() != item.id()));
        // Through a save and a load: the same.
        let mut back: World = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
        back.plant(&content);
        assert!(
            back.items_in(at.chunk())
                .iter()
                .all(|i| i.id() != item.id())
        );
        assert!(back.item(item.id()).is_some());
    }

    fn fed_world() -> World {
        let mut w = World::empty(
            Calendar::new(10, 100).unwrap(), // 1000 ticks per day
            Map::filled(4, 4, Terrain::Grass),
        );
        w.spawn_body(Pos::new(1, 1));
        w
    }

    fn condition(w: &World) -> &Condition {
        w.body(BodyId(1)).unwrap().condition()
    }

    #[test]
    fn needs_rise_with_time_at_the_content_rates() {
        let mut w = fed_world();
        let mut events = Vec::new();
        // Half a day awake: hunger 16, fatigue 60; by day cold stays 0.
        w.advance(500, &rules(), &mut events);
        let (vitality, hunger, cold, fatigue, asleep) = condition(&w).summary();
        assert_eq!(
            (vitality, hunger, cold, fatigue, asleep),
            (100, 16, 0, 60, false)
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::ConditionChanged {
                    body: BodyId(1),
                    ..
                }
            )),
            "clients are told"
        );
    }

    #[test]
    fn night_brings_cold_and_dawn_relieves_it() {
        let mut w = fed_world();
        let mut events = Vec::new();
        w.advance(580, &rules(), &mut events); // dusk at 58 %
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::NightFell { day: 1 }))
        );
        assert!(w.is_night(&rules()));
        w.advance(419, &rules(), &mut events); // the rest of the night: 42 % of a day
        assert_eq!(condition(&w).cold.points(), 100, "{:?}", condition(&w));
        w.step(&rules(), &mut events);
        assert!(!w.is_night(&rules()), "dawn");
        w.advance(200, &rules(), &mut events);
        assert_eq!(condition(&w).cold.points(), 0, "daylight undoes it");
    }

    #[test]
    fn full_fatigue_forces_sleep_and_sleep_rests() {
        let mut w = fed_world();
        let mut events = Vec::new();
        go(&mut w, Pos::new(3, 3));
        // 120 per day awake: full after 5/6 of a day.
        w.advance(840, &rules(), &mut events);
        assert!(condition(&w).asleep, "{:?}", condition(&w));
        assert!(!w.body(BodyId(1)).unwrap().is_walking(), "collapsed");
        // 300 per day asleep: empty after a third of a day, then awake
        // (it is day), tiring again a little.
        w.advance(340, &rules(), &mut events);
        assert!(!condition(&w).asleep);
        assert!(condition(&w).fatigue.points() <= 1);
    }

    #[test]
    fn eating_within_reach_relieves_hunger_and_removes_the_item() {
        let mut w = fed_world();
        let mut events = Vec::new();
        w.advance(500, &rules(), &mut events); // hunger 16
        let berries = w.spawn_item("berries", Pos::new(2, 2)); // diagonal neighbour
        let far = w.spawn_item("berries", Pos::new(3, 3));
        let stone = w.spawn_item("flint", Pos::new(1, 1));
        events.clear();
        w.queue(Intention::Eat {
            body: BodyId(1),
            item: far,
        });
        w.queue(Intention::Eat {
            body: BodyId(1),
            item: stone,
        });
        w.queue(Intention::Eat {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        assert!(events.contains(&Event::ActRefused {
            body: BodyId(1),
            act: ItemAct::Eat,
            item: far,
            reason: Refusal::OutOfReach,
        }));
        assert!(events.contains(&Event::ActRefused {
            body: BodyId(1),
            act: ItemAct::Eat,
            item: stone,
            reason: Refusal::NotFood,
        }));
        assert!(events.contains(&Event::ItemRemoved {
            item: berries,
            at: Pos::new(2, 2)
        }));
        assert!(events.contains(&Event::BodyAte {
            body: BodyId(1),
            item: berries
        }));
        // 250 kJ at 25 kJ per point: 10 points off 16.
        assert_eq!(condition(&w).hunger.points(), 6);
        assert!(w.items().iter().all(|i| i.id() != berries));
        assert_eq!(w.items().len(), 2);
    }

    #[test]
    fn starving_drains_vitality_and_kills_leaving_a_corpse() {
        let mut w = fed_world();
        let mut events = Vec::new();
        // Three days to full hunger, then a day at full hunger to die.
        w.advance(3_100, &rules(), &mut events);
        assert_eq!(condition(&w).hunger.points(), 100);
        // Three nights outside have cost vitality, but not life.
        assert!(condition(&w).vitality.points() > 0);
        events.clear();
        w.advance(1_000, &rules(), &mut events);
        assert!(w.body(BodyId(1)).is_none(), "dead and gone");
        let died = events.iter().find_map(|e| match e {
            Event::BodyDied {
                body,
                cause,
                at,
                corpse,
            } => Some((*body, *cause, *at, *corpse)),
            _ => None,
        });
        let (body, cause, at, corpse) = died.expect("a death event");
        assert_eq!(
            (body, cause, at),
            (BodyId(1), Death::Starved, Pos::new(1, 1))
        );
        assert!(
            w.items()
                .iter()
                .any(|i| i.id() == corpse && i.material() == "corpse")
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ItemAdded { item, .. } if *item == corpse))
        );
        // Nothing more happens to a dead body.
        events.clear();
        w.advance(10, &rules(), &mut events);
        assert_eq!(events, Vec::new());
    }

    #[test]
    fn sleeping_stops_a_walk_and_any_act_wakes() {
        let mut w = fed_world();
        let mut events = Vec::new();
        w.advance(100, &rules(), &mut events); // a little tired: sleep holds
        go(&mut w, Pos::new(3, 3));
        w.step(&rules(), &mut events);
        w.queue(Intention::Sleep { body: BodyId(1) });
        w.step(&rules(), &mut events);
        assert!(condition(&w).asleep);
        assert!(!w.body(BodyId(1)).unwrap().is_walking());
        assert!(events.iter().any(|e| matches!(
            e,
            Event::BodyStopped {
                body: BodyId(1),
                ..
            }
        )));
        go(&mut w, Pos::new(0, 0));
        w.step(&rules(), &mut events);
        assert!(!condition(&w).asleep, "walking wakes");
        assert!(w.body(BodyId(1)).unwrap().is_walking());
    }

    fn pick(w: &mut World, item: ItemId, events: &mut Vec<Event>) {
        w.queue(Intention::PickUp {
            body: BodyId(1),
            item,
        });
        w.step(&rules(), events);
    }

    #[test]
    fn picking_up_carries_an_item_and_puts_the_first_in_hand() {
        let mut w = fed_world();
        let mut events = Vec::new();
        let flint = w.spawn_item("flint", Pos::new(2, 2)); // within reach
        let branch = w.spawn_item("branch", Pos::new(1, 2));
        let far = w.spawn_item("branch", Pos::new(3, 3));
        pick(&mut w, flint, &mut events);
        let body = w.body(BodyId(1)).unwrap();
        assert_eq!(body.held(), Some(flint), "the first item goes in hand");
        assert_eq!(
            w.item(flint).unwrap().place(),
            Place::Carried { body: BodyId(1) }
        );
        assert!(events.contains(&Event::ItemRemoved {
            item: flint,
            at: Pos::new(2, 2)
        }));
        assert!(events.contains(&Event::BodyHolds {
            body: BodyId(1),
            item: Some(flint)
        }));
        assert!(events.iter().any(|e| matches!(
            e,
            Event::InventoryChanged { body: BodyId(1), held: Some(f), load_g: 400, capacity_g: 20000, .. } if *f == flint
        )));
        assert_eq!(
            w.items_within_reach(Pos::new(1, 1)).len(),
            1,
            "only the branch is left on the ground nearby"
        );

        events.clear();
        pick(&mut w, branch, &mut events);
        pick(&mut w, far, &mut events);
        let body = w.body(BodyId(1)).unwrap();
        assert_eq!(body.carried(), &[branch], "the second item is carried");
        assert!(events.contains(&Event::ActRefused {
            body: BodyId(1),
            act: ItemAct::PickUp,
            item: far,
            reason: Refusal::OutOfReach,
        }));
        assert_eq!(w.load_g(body, &rules()), 1900);
    }

    #[test]
    fn too_heavy_a_load_is_refused_and_a_load_slows_the_walk() {
        let mut w = fed_world();
        let mut events = Vec::new();
        // Thirteen branches of 1.5 kg fit in 20 kg; the fourteenth does not.
        let branches: Vec<ItemId> = (0..15)
            .map(|_| w.spawn_item("branch", Pos::new(1, 1)))
            .collect();
        for item in &branches {
            pick(&mut w, *item, &mut events);
        }
        let body = w.body(BodyId(1)).unwrap();
        assert_eq!(
            body.inventory().count(),
            13,
            "19.5 kg: one more would make 21"
        );
        assert!(events.iter().any(|e| matches!(
            e,
            Event::ActRefused {
                act: ItemAct::PickUp,
                reason: Refusal::TooHeavy,
                ..
            }
        )));
        // At 19.5 of 20 kg the walk is nearly at 60 %: two tiles take 23 ticks, not 14.
        go(&mut w, Pos::new(3, 1));
        events.clear();
        let took = until_stopped(&mut w, &mut events);
        assert!((22..=24).contains(&took), "{took} ticks");
    }

    #[test]
    fn holding_stowing_dropping_and_eating_what_is_carried() {
        let mut w = fed_world();
        let mut events = Vec::new();
        w.advance(500, &rules(), &mut events); // hungry enough to notice eating
        let flint = w.spawn_item("flint", Pos::new(1, 1));
        let berries = w.spawn_item("berries", Pos::new(1, 1));
        pick(&mut w, flint, &mut events);
        pick(&mut w, berries, &mut events);
        let body = w.body(BodyId(1)).unwrap();
        assert_eq!((body.held(), body.carried()), (Some(flint), &[berries][..]));

        // Hold the berries: the flint goes among the carried.
        events.clear();
        w.queue(Intention::Hold {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        let body = w.body(BodyId(1)).unwrap();
        assert_eq!((body.held(), body.carried()), (Some(berries), &[flint][..]));
        assert!(events.contains(&Event::BodyHolds {
            body: BodyId(1),
            item: Some(berries)
        }));

        // Stow them: nothing in hand.
        w.queue(Intention::Stow {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        let body = w.body(BodyId(1)).unwrap();
        assert_eq!((body.held(), body.carried()), (None, &[flint, berries][..]));

        // Eat the carried berries: gone, hunger relieved, nothing on the ground.
        let hunger_before = condition(&w).hunger.points();
        events.clear();
        w.queue(Intention::Eat {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        assert!(w.item(berries).is_none());
        assert_eq!(condition(&w).hunger.points(), hunger_before - 10);
        assert!(events.contains(&Event::BodyAte {
            body: BodyId(1),
            item: berries
        }));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::ItemRemoved { .. })),
            "it was not on the ground"
        );

        // Drop the flint: on the body's tile, out of the inventory.
        events.clear();
        w.queue(Intention::Drop {
            body: BodyId(1),
            item: flint,
        });
        w.step(&rules(), &mut events);
        assert_eq!(
            w.item(flint).unwrap().place(),
            Place::Ground {
                pos: Pos::new(1, 1)
            }
        );
        assert_eq!(w.body(BodyId(1)).unwrap().inventory().count(), 0);
        assert!(events.iter().any(
            |e| matches!(e, Event::ItemAdded { item, at, .. } if *item == flint && *at == Pos::new(1, 1))
        ));
        // Dropping it again, or stowing what is not in hand, is refused.
        events.clear();
        w.queue(Intention::Drop {
            body: BodyId(1),
            item: flint,
        });
        w.queue(Intention::Stow {
            body: BodyId(1),
            item: flint,
        });
        w.queue(Intention::Hold {
            body: BodyId(1),
            item: ItemId(999),
        });
        w.step(&rules(), &mut events);
        for (act, item, reason) in [
            (ItemAct::Drop, flint, Refusal::NotCarried),
            (ItemAct::Stow, flint, Refusal::NotCarried),
            (ItemAct::Hold, ItemId(999), Refusal::Unknown),
        ] {
            assert!(events.contains(&Event::ActRefused {
                body: BodyId(1),
                act,
                item,
                reason
            }));
        }
    }

    #[test]
    fn a_dead_body_drops_what_it_carried() {
        let mut w = fed_world();
        let mut events = Vec::new();
        let flint = w.spawn_item("flint", Pos::new(1, 1));
        pick(&mut w, flint, &mut events);
        w.advance(4_200, &rules(), &mut events); // starves
        assert!(w.body(BodyId(1)).is_none());
        assert_eq!(
            w.item(flint).unwrap().place(),
            Place::Ground {
                pos: Pos::new(1, 1)
            }
        );
    }

    #[test]
    fn pending_intentions_are_not_serialized_but_routes_are() {
        let mut w = open_field();
        go(&mut w, Pos::new(3, 3));
        w.step(&rules(), &mut Vec::new());
        w.queue(Intention::MoveTo {
            body: BodyId(1),
            target: Pos::new(5, 5),
        });
        let text = serde_json::to_string(&w).unwrap();
        let back: World = serde_json::from_str(&text).unwrap();
        assert_eq!(back.pending, Vec::new());
        assert_eq!(back.bodies(), w.bodies());
        assert!(back.body(BodyId(1)).unwrap().is_walking());
        assert_eq!(back.map(), w.map());
        assert_eq!(back.items(), w.items());
    }
}
