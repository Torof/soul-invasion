//! Pure world simulation for Soul Invasion.
//!
//! This crate has no I/O, no clock and no network (architecture decision D24).
//! The server feeds it intentions and asks it to advance a number of fixed
//! ticks; it returns the events that happened. Same inputs, same outputs.

pub mod time;

pub use time::{Calendar, CalendarError, GameTime, Tick};

/// Something that happened in the world during a tick.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// A new day began.
    DayStarted {
        /// The day that just started (days count from 1).
        day: u64,
    },
}

/// The world state. Milestone 1 ("the skeleton") only tracks time.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct World {
    calendar: Calendar,
    tick: Tick,
}

impl World {
    /// Creates an empty world at tick zero with the given calendar.
    pub fn new(calendar: Calendar) -> Self {
        Self {
            calendar,
            tick: Tick(0),
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

    /// Advances the world by exactly one fixed tick, appending the events
    /// of that tick to `events`.
    pub fn step(&mut self, events: &mut Vec<Event>) {
        self.tick.0 += 1;
        let now = self.time();
        if now.is_day_start() {
            events.push(Event::DayStarted { day: now.day });
        }
    }

    /// Advances the world by `n` fixed ticks, appending events to `events`.
    pub fn advance(&mut self, n: u64, events: &mut Vec<Event>) {
        for _ in 0..n {
            self.step(events);
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

    #[test]
    fn new_world_starts_at_tick_zero_on_day_one() {
        let w = World::default();
        assert_eq!(w.tick(), Tick(0));
        assert_eq!(w.time().day, 1);
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
        a.advance(1_000, &mut ea);
        b.advance(1_000, &mut eb);
        assert_eq!(a.tick(), b.tick());
        assert_eq!(ea, eb);
    }

    #[test]
    fn stepping_one_by_one_equals_advancing_at_once() {
        let mut a = small_world();
        let mut b = small_world();
        let (mut ea, mut eb) = (Vec::new(), Vec::new());
        a.advance(50, &mut ea);
        for _ in 0..50 {
            b.step(&mut eb);
        }
        assert_eq!(a.tick(), b.tick());
        assert_eq!(ea, eb);
    }
}
