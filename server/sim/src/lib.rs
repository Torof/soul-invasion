//! Pure world simulation for Soul Invasion.
//!
//! This crate has no I/O, no clock and no network (architecture decision D24).
//! The server feeds it intentions and asks it to advance a number of fixed
//! ticks; it returns the events that happened. Same inputs, same outputs.

pub mod content;
pub mod map;
pub mod time;

use std::collections::{BTreeMap, VecDeque};

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
}

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
    items: Vec<Item>,
    next_item: u64,
    /// Intentions received since the last tick, applied in order at the
    /// start of the next one. Never persisted: a restart drops them.
    #[serde(skip)]
    pending: Vec<Intention>,
}

/// Side of the first version's valley, in tiles: about 256 m, three
/// minutes of walking (D31).
pub const VALLEY_SIZE: u32 = 256;

/// Seed of the first version's valley.
pub const VALLEY_SEED: u64 = 1;

impl World {
    /// Creates the first version's valley at tick zero, with a few
    /// inhabitants standing on the grass of the east bank and the
    /// content's materials scattered over the ground by its spawn rules.
    pub fn new(calendar: Calendar, content: &Content) -> Self {
        let mut world = Self::empty(calendar, Map::valley(VALLEY_SIZE, VALLEY_SIZE, VALLEY_SEED));
        for wanted in [Pos::new(164, 141), Pos::new(166, 142), Pos::new(163, 144)] {
            let pos = world
                .map
                .nearest(wanted, Terrain::Grass)
                .expect("the valley has grass");
            world.spawn_body(pos);
        }
        world.scatter(content, VALLEY_SEED);
        world
    }

    /// Lays the content's materials on the ground following its spawn
    /// rules, deterministically for a given seed: tile by tile, rule by
    /// rule, a piece lands with the rule's probability.
    fn scatter(&mut self, content: &Content, seed: u64) {
        let mut rng = map::Lcg::new(seed ^ 0x5eed_17e5);
        // Distance of every tile to each terrain some rule wants to be near.
        let mut distances: BTreeMap<Terrain, Vec<u32>> = BTreeMap::new();
        for near in content.spawns.iter().filter_map(|s| s.near) {
            distances
                .entry(near.terrain)
                .or_insert_with(|| self.map.distance_to(near.terrain));
        }
        let (width, height) = (self.map.width(), self.map.height());
        for y in 0..height {
            for x in 0..width {
                let pos = Pos::new(
                    i32::try_from(x).expect("map fits in i32"),
                    i32::try_from(y).expect("map fits in i32"),
                );
                let index = (y * width + x) as usize;
                let terrain = self.map.get(pos).expect("inside the map");
                for spawn in &content.spawns {
                    if spawn.terrain != terrain {
                        continue;
                    }
                    let close_enough = spawn
                        .near
                        .is_none_or(|near| distances[&near.terrain][index] <= near.within);
                    if close_enough && rng.next_below(1000) < spawn.per_thousand {
                        self.spawn_item(&spawn.material, pos);
                    }
                }
            }
        }
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
            pending: Vec::new(),
        }
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

    /// Every item on the ground, in id order.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// The items lying on the ground in a chunk, in id order.
    pub fn items_in(&self, chunk: ChunkPos) -> impl Iterator<Item = &Item> {
        self.items
            .iter()
            .filter(move |i| i.pos().is_some_and(|p| p.chunk() == chunk))
    }

    /// An item by id.
    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
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

    /// Items lying on `pos` or on one of its eight neighbours, in id order.
    pub fn items_within_reach(&self, pos: Pos) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(move |i| {
            i.pos()
                .is_some_and(|p| (p.x - pos.x).abs() <= 1 && (p.y - pos.y).abs() <= 1)
        })
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
        match *intention {
            Intention::MoveTo { body: id, target } => {
                let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
                    events.push(Event::MoveRefused { body: id, target });
                    return;
                };
                self.wake(index, events);
                let cost = |t: Terrain| rules.movement.cost(t);
                let from = self.bodies[index].at;
                match self.map.path(from.tile(), target, cost) {
                    Some(path) => {
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
                    }
                    None => events.push(Event::MoveRefused { body: id, target }),
                }
            }
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

#[allow(clippy::cast_possible_truncation)]
fn walk(
    body: &mut Body,
    map: &Map,
    movement: &Movement,
    speed_percent: u32,
    ticks_per_second: f64,
    events: &mut Vec<Event>,
) {
    let terrain = map.get(body.at.tile()).unwrap_or(Terrain::Rock);
    // Millimetres this tick: 143 cm/s at 10 ticks/s is exactly 143 mm.
    let mut budget =
        f64::from(movement.speed_cm_per_s(terrain) * 10 * speed_percent) / 100.0 / ticks_per_second;
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
        // 4 ticks per day: easy to reason about.
        World::new(Calendar::new(2, 2).unwrap(), &rules())
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
    fn advancing_is_deterministic() {
        let mut a = small_world();
        let mut b = small_world();
        let (mut ea, mut eb) = (Vec::new(), Vec::new());
        go(&mut a, Pos::new(30, 200));
        go(&mut b, Pos::new(30, 200));
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
    fn impossible_walks_are_refused() {
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
                Event::MoveRefused {
                    body: BodyId(1),
                    target: Pos::new(50, 50)
                },
                Event::MoveRefused {
                    body: BodyId(9),
                    target: Pos::new(1, 1)
                },
            ]
        );
    }

    #[test]
    fn items_are_scattered_by_the_spawn_rules_deterministically() {
        let content = Content::sample();
        let a = World::new(Calendar::default(), &content);
        let b = World::new(Calendar::default(), &content);
        assert_eq!(a.items(), b.items());
        assert!(a.items().len() > 100, "{} items", a.items().len());
        // Every item obeys a rule: branches in the forest, berries on grass
        // within two tiles of it.
        let to_forest = a.map().distance_to(Terrain::Forest);
        for item in a.items() {
            let terrain = a.map().get(item.pos().unwrap()).unwrap();
            let rule = content
                .spawns
                .iter()
                .find(|s| s.material == item.material())
                .expect("a rule for each material laid down");
            assert_eq!(terrain, rule.terrain, "{item:?}");
            if let Some(near) = rule.near {
                let pos = item.pos().unwrap();
                let index = usize::try_from(pos.y * 256 + pos.x).unwrap();
                assert!(
                    to_forest[index] <= near.within,
                    "{item:?} is far from the forest"
                );
            }
        }
        assert!(
            a.items().iter().any(|i| i.material() == "berries"),
            "the edges do grow berries"
        );
        // Densities land near their rule, over tens of thousands of tiles.
        let forest = a
            .map()
            .tiles()
            .iter()
            .filter(|t| **t == Terrain::Forest)
            .count();
        let branches = a
            .items()
            .iter()
            .filter(|i| i.material() == "branch")
            .count();
        let per_thousand = branches * 1000 / forest;
        assert!(
            (45..=75).contains(&per_thousand),
            "{per_thousand}‰ branches"
        );
        // Items are found by chunk.
        let first = &a.items()[0];
        assert!(
            a.items_in(first.pos().unwrap().chunk())
                .any(|i| i.id() == first.id())
        );
        assert_eq!(a.items_in(ChunkPos::new(-1, -1)).count(), 0);
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
            w.items_within_reach(Pos::new(1, 1)).count(),
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
