//! Pure world simulation for Soul Invasion.
//!
//! This crate has no I/O, no clock and no network (architecture decision D24).
//! The server feeds it intentions and asks it to advance a number of fixed
//! ticks; it returns the events that happened. Same inputs, same outputs.

pub mod content;
pub mod map;
pub mod time;

use std::collections::VecDeque;

pub use content::{Content, ContentError, Food, Look, Material, MaterialKind, Movement, Spawn};
pub use map::{CHUNK_SIZE, ChunkPos, FinePos, MM_PER_TILE, Map, MapSpec, Pos, Terrain, line_clear};
pub use time::{Calendar, CalendarError, GameTime, Tick};

/// Identifies a body (an inhabitant). Players and NPCs are the same kind
/// of body; only the pilot differs (D13).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct BodyId(pub u32);

/// An inhabitant's body: where it is and where it is going.
///
/// It stands at a fine position (millimetres, D32) and walks along a
/// route of straight segments at the speed the content gives its terrain.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Body {
    id: BodyId,
    at: FinePos,
    /// Remaining waypoints of the current walk, nearest first.
    route: VecDeque<FinePos>,
}

impl Body {
    /// The body's id.
    pub fn id(&self) -> BodyId {
        self.id
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

/// A piece of material lying on the ground.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Item {
    id: ItemId,
    /// Content id of its material.
    material: String,
    pos: Pos,
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

    /// Where it lies.
    pub fn pos(&self) -> Pos {
        self.pos
    }
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
        for wanted in [Pos::new(160, 128), Pos::new(162, 129), Pos::new(159, 131)] {
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
        let (width, height) = (self.map.width(), self.map.height());
        for y in 0..height {
            for x in 0..width {
                let pos = Pos::new(
                    i32::try_from(x).expect("map fits in i32"),
                    i32::try_from(y).expect("map fits in i32"),
                );
                let terrain = self.map.get(pos).expect("inside the map");
                for spawn in &content.spawns {
                    if spawn.terrain == terrain && rng.next_below(1000) < spawn.per_thousand {
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

    /// The items lying in a chunk, in id order.
    pub fn items_in(&self, chunk: ChunkPos) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(move |i| i.pos.chunk() == chunk)
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
            pos,
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
        });
        id
    }

    /// Queues an intention for the next tick.
    pub fn queue(&mut self, intention: Intention) {
        self.pending.push(intention);
    }

    /// Advances the world by exactly one fixed tick under the content's
    /// rules, appending the events of that tick to `events`.
    pub fn step(&mut self, rules: &Content, events: &mut Vec<Event>) {
        self.tick.0 += 1;
        let now = self.time();
        if now.is_day_start() {
            events.push(Event::DayStarted { day: now.day });
        }

        let pending = std::mem::take(&mut self.pending);
        for intention in pending {
            self.apply(&intention, rules, events);
        }

        let per_second = f64::from(self.calendar.ticks_per_second());
        for body in &mut self.bodies {
            if body.route.is_empty() {
                continue;
            }
            walk(body, &self.map, &rules.movement, per_second, events);
        }
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
        }
    }
}

/// Moves a walking body for one tick: as far along its route as the
/// speed of the terrain it stands on allows, waypoint after waypoint.
#[allow(clippy::cast_possible_truncation)]
fn walk(
    body: &mut Body,
    map: &Map,
    movement: &Movement,
    ticks_per_second: f64,
    events: &mut Vec<Event>,
) {
    let terrain = map.get(body.at.tile()).unwrap_or(Terrain::Rock);
    // Millimetres this tick: 143 cm/s at 10 ticks/s is exactly 143 mm.
    let mut budget = f64::from(movement.speed_cm_per_s(terrain) * 10) / ticks_per_second;
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
        assert_eq!(events, Vec::new());
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
        // Every item obeys a rule: branches in the forest, berries on grass.
        for item in a.items() {
            let terrain = a.map().get(item.pos()).unwrap();
            let rule = content
                .spawns
                .iter()
                .find(|s| s.material == item.material())
                .expect("a rule for each material laid down");
            assert_eq!(terrain, rule.terrain, "{item:?}");
        }
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
            a.items_in(first.pos().chunk())
                .any(|i| i.id() == first.id())
        );
        assert_eq!(a.items_in(ChunkPos::new(-1, -1)).count(), 0);
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
