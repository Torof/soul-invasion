//! Pure world simulation for Soul Invasion.
//!
//! This crate has no I/O, no clock and no network (architecture decision D24).
//! The server feeds it intentions and asks it to advance a number of fixed
//! ticks; it returns the events that happened. Same inputs, same outputs.

pub mod map;
pub mod time;

use std::collections::VecDeque;

pub use map::{CHUNK_SIZE, ChunkPos, Map, MapSpec, Pos, Terrain};
pub use time::{Calendar, CalendarError, GameTime, Tick};

/// Identifies a body (an inhabitant). Players and NPCs are the same kind
/// of body; only the pilot differs (D13).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct BodyId(pub u32);

/// Ticks a body needs for a straight step: one metre in 0.7 s at the
/// default calendar, a walking pace. Content later (D15).
pub const STEP_TICKS: u64 = 7;

/// Ticks a body needs for a diagonal step: √2 metres (D32).
pub const DIAGONAL_STEP_TICKS: u64 = 10;

/// An inhabitant's body: where it is and where it is going.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Body {
    id: BodyId,
    pos: Pos,
    /// Remaining steps of the current walk, nearest first.
    path: VecDeque<Pos>,
    /// Tick at which the next step may happen.
    next_step: Tick,
}

impl Body {
    /// The body's id.
    pub fn id(&self) -> BodyId {
        self.id
    }

    /// Where the body stands.
    pub fn pos(&self) -> Pos {
        self.pos
    }

    /// Is the body walking somewhere?
    pub fn is_walking(&self) -> bool {
        !self.path.is_empty()
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
    /// A body stepped onto a tile.
    BodyMoved {
        /// Who.
        body: BodyId,
        /// Where it now stands.
        to: Pos,
    },
    /// A body finished its walk.
    BodyArrived {
        /// Who.
        body: BodyId,
        /// Where.
        at: Pos,
    },
    /// A walk could not start (unknown body, impassable or unreachable target).
    MoveRefused {
        /// Who.
        body: BodyId,
        /// Where it wanted to go.
        target: Pos,
    },
}

/// The world state: time, ground, bodies.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct World {
    calendar: Calendar,
    tick: Tick,
    map: Map,
    bodies: Vec<Body>,
    next_body: u32,
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
    /// inhabitants standing on the grass of the east bank.
    pub fn new(calendar: Calendar) -> Self {
        let mut world = Self::empty(calendar, Map::valley(VALLEY_SIZE, VALLEY_SIZE, VALLEY_SEED));
        for wanted in [Pos::new(160, 128), Pos::new(162, 129), Pos::new(159, 131)] {
            let pos = world
                .map
                .nearest(wanted, Terrain::Grass)
                .expect("the valley has grass");
            world.spawn_body(pos);
        }
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
            pos,
            path: VecDeque::new(),
            next_step: self.tick,
        });
        id
    }

    /// Queues an intention for the next tick.
    pub fn queue(&mut self, intention: Intention) {
        self.pending.push(intention);
    }

    /// Advances the world by exactly one fixed tick, appending the events
    /// of that tick to `events`.
    pub fn step(&mut self, events: &mut Vec<Event>) {
        self.tick.0 += 1;
        let now = self.time();
        if now.is_day_start() {
            events.push(Event::DayStarted { day: now.day });
        }

        let pending = std::mem::take(&mut self.pending);
        for intention in pending {
            self.apply(&intention, events);
        }

        for body in &mut self.bodies {
            if body.path.is_empty() || body.next_step > self.tick {
                continue;
            }
            if let Some(next) = body.path.pop_front() {
                let pace = if body.pos.is_diagonal_to(next) {
                    DIAGONAL_STEP_TICKS
                } else {
                    STEP_TICKS
                };
                body.pos = next;
                body.next_step = Tick(self.tick.0 + pace);
                events.push(Event::BodyMoved {
                    body: body.id,
                    to: next,
                });
                if body.path.is_empty() {
                    events.push(Event::BodyArrived {
                        body: body.id,
                        at: next,
                    });
                }
            }
        }
    }

    /// Advances the world by `n` fixed ticks, appending events to `events`.
    pub fn advance(&mut self, n: u64, events: &mut Vec<Event>) {
        for _ in 0..n {
            self.step(events);
        }
    }

    fn apply(&mut self, intention: &Intention, events: &mut Vec<Event>) {
        match *intention {
            Intention::MoveTo { body: id, target } => {
                let Some(index) = self.bodies.iter().position(|b| b.id == id) else {
                    events.push(Event::MoveRefused { body: id, target });
                    return;
                };
                let from = self.bodies[index].pos;
                match self.map.path(from, target) {
                    Some(path) => {
                        let body = &mut self.bodies[index];
                        body.path = path.into();
                        // A new walk starts at once; the pace applies between steps.
                        body.next_step = self.tick;
                    }
                    None => events.push(Event::MoveRefused { body: id, target }),
                }
            }
        }
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new(Calendar::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_world() -> World {
        // 4 ticks per day: easy to reason about.
        World::new(Calendar::new(2, 2).unwrap())
    }

    fn open_field() -> World {
        let mut world = World::empty(
            Calendar::new(10, 100).unwrap(),
            Map::filled(8, 8, Terrain::Grass),
        );
        world.spawn_body(Pos::new(0, 0));
        world
    }

    #[test]
    fn new_world_starts_at_tick_zero_on_day_one_with_bodies() {
        let w = World::default();
        assert_eq!(w.tick(), Tick(0));
        assert_eq!(w.time().day, 1);
        assert_eq!(w.bodies().len(), 3);
        assert!(w.bodies().iter().all(|b| w.map().is_passable(b.pos())));
        assert_eq!(
            (w.map().width(), w.map().height()),
            (VALLEY_SIZE, VALLEY_SIZE)
        );
        let text = serde_json::to_string(&w).unwrap();
        assert!(
            text.len() < 1_000,
            "terrain is not in the snapshot: {} bytes",
            text.len()
        );
    }

    #[test]
    fn a_day_started_event_fires_on_each_day_boundary() {
        let mut w = small_world();
        let mut events = Vec::new();
        w.advance(12, &mut events); // three full days
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
        w.advance(3, &mut events);
        assert_eq!(events, Vec::new());
        assert_eq!(w.time().day, 1);
    }

    #[test]
    fn advancing_is_deterministic() {
        let mut a = small_world();
        let mut b = small_world();
        let (mut ea, mut eb) = (Vec::new(), Vec::new());
        a.queue(Intention::MoveTo {
            body: BodyId(1),
            target: Pos::new(30, 200),
        });
        b.queue(Intention::MoveTo {
            body: BodyId(1),
            target: Pos::new(30, 200),
        });
        a.advance(1_000, &mut ea);
        b.advance(1_000, &mut eb);
        assert_eq!(a.tick(), b.tick());
        assert_eq!(ea, eb);
        assert_eq!(a.bodies(), b.bodies());
    }

    #[test]
    fn a_body_walks_one_tile_every_few_ticks_and_arrives() {
        let mut w = open_field();
        let mut events = Vec::new();
        w.queue(Intention::MoveTo {
            body: BodyId(1),
            target: Pos::new(2, 0),
        });
        w.step(&mut events); // tick 1: intention applied, first step at once
        assert_eq!(
            events,
            vec![Event::BodyMoved {
                body: BodyId(1),
                to: Pos::new(1, 0)
            }]
        );
        events.clear();
        w.advance(STEP_TICKS - 1, &mut events);
        assert!(events.is_empty(), "no step before the pace allows");
        w.step(&mut events);
        assert_eq!(
            events,
            vec![
                Event::BodyMoved {
                    body: BodyId(1),
                    to: Pos::new(2, 0)
                },
                Event::BodyArrived {
                    body: BodyId(1),
                    at: Pos::new(2, 0)
                },
            ]
        );
        assert_eq!(w.body(BodyId(1)).unwrap().pos(), Pos::new(2, 0));
        assert!(!w.body(BodyId(1)).unwrap().is_walking());
    }

    #[test]
    fn a_diagonal_step_takes_longer_than_a_straight_one() {
        let mut w = open_field();
        let mut events = Vec::new();
        w.queue(Intention::MoveTo {
            body: BodyId(1),
            target: Pos::new(2, 2),
        });
        w.step(&mut events); // first diagonal step at once
        assert_eq!(w.body(BodyId(1)).unwrap().pos(), Pos::new(1, 1));
        events.clear();
        w.advance(DIAGONAL_STEP_TICKS - 1, &mut events);
        assert!(events.is_empty(), "a diagonal step waits longer");
        w.step(&mut events);
        assert_eq!(w.body(BodyId(1)).unwrap().pos(), Pos::new(2, 2));
    }

    #[test]
    fn a_new_intention_replaces_the_current_walk() {
        let mut w = open_field();
        let mut events = Vec::new();
        w.queue(Intention::MoveTo {
            body: BodyId(1),
            target: Pos::new(5, 0),
        });
        w.step(&mut events);
        w.queue(Intention::MoveTo {
            body: BodyId(1),
            target: Pos::new(1, 3),
        });
        w.advance(60, &mut events);
        assert_eq!(w.body(BodyId(1)).unwrap().pos(), Pos::new(1, 3));
    }

    #[test]
    fn impossible_walks_are_refused() {
        let mut w = open_field();
        let mut events = Vec::new();
        w.queue(Intention::MoveTo {
            body: BodyId(1),
            target: Pos::new(50, 50),
        });
        w.queue(Intention::MoveTo {
            body: BodyId(9),
            target: Pos::new(1, 1),
        });
        w.step(&mut events);
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
    fn pending_intentions_are_not_serialized() {
        let mut w = open_field();
        w.queue(Intention::MoveTo {
            body: BodyId(1),
            target: Pos::new(3, 3),
        });
        let text = serde_json::to_string(&w).unwrap();
        let back: World = serde_json::from_str(&text).unwrap();
        assert_eq!(back.pending, Vec::new());
        assert_eq!(back.bodies(), w.bodies());
        assert_eq!(back.map(), w.map());
    }
}
