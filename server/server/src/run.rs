//! The simulation loop (D29): one thread, fixed ticks.
//!
//! Two ways to run the same loop:
//! - [`run_headless`]: as fast as the machine allows, no pacing, stops after
//!   a number of game days. Used for tests and scenario checks (D09).
//! - [`run_paced`]: ticks are spread over real time according to the
//!   calendar and a speed factor. Used when clients are connected.
//!
//! Both call the same `World::step`, so the pace never changes the result.

use std::time::{Duration, Instant};

use sim::{Event, World};

/// What happened during a run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunSummary {
    /// Ticks simulated during the run.
    pub ticks: u64,
    /// Events the world produced, in order.
    pub events: Vec<Event>,
}

/// Runs `world` for `days` full game days as fast as possible.
pub fn run_headless(world: &mut World, days: u64) -> RunSummary {
    let total = days * world.calendar().ticks_per_day();
    let mut events = Vec::new();
    world.advance(total, &mut events);
    RunSummary {
        ticks: total,
        events,
    }
}

/// Runs `world` in real time, `time_scale` times faster than the calendar
/// says, until `days` full game days have passed (or forever when `None`).
///
/// `on_events` is called after every tick that produced events; this is
/// where the server will later notify clients and the event journal.
///
/// Uses a fixed-step accumulator: if the machine falls behind, several
/// ticks run back to back to catch up, and the rules never see the
/// difference.
pub fn run_paced(
    world: &mut World,
    time_scale: f64,
    days: Option<u64>,
    mut on_events: impl FnMut(&World, &[Event]),
) -> RunSummary {
    let tick_duration = tick_duration(world.calendar().ticks_per_second(), time_scale);
    let stop_at_tick = days.map(|d| world.tick().0 + d * world.calendar().ticks_per_day());

    let mut summary = RunSummary::default();
    let mut events = Vec::new();
    let mut last = Instant::now();
    let mut owed = Duration::ZERO;

    loop {
        if stop_at_tick.is_some_and(|stop| world.tick().0 >= stop) {
            return summary;
        }

        let now = Instant::now();
        owed += now.duration_since(last);
        last = now;

        while owed >= tick_duration && stop_at_tick.is_none_or(|stop| world.tick().0 < stop) {
            owed -= tick_duration;
            events.clear();
            world.step(&mut events);
            summary.ticks += 1;
            if !events.is_empty() {
                on_events(world, &events);
                summary.events.extend(events.iter().cloned());
            }
        }

        if owed < tick_duration {
            std::thread::sleep(tick_duration.saturating_sub(owed));
        }
    }
}

/// Real duration of one tick at the given resolution and speed factor.
fn tick_duration(ticks_per_second: u32, time_scale: f64) -> Duration {
    Duration::from_secs_f64(1.0 / (f64::from(ticks_per_second) * time_scale))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::Calendar;

    fn small_world() -> World {
        World::new(Calendar::new(2, 3).unwrap()) // 6 ticks per day
    }

    #[test]
    fn headless_runs_exactly_the_requested_days() {
        let mut w = small_world();
        let s = run_headless(&mut w, 3);
        assert_eq!(s.ticks, 18);
        assert_eq!(w.time().day, 4);
        assert_eq!(
            s.events,
            vec![
                Event::DayStarted { day: 2 },
                Event::DayStarted { day: 3 },
                Event::DayStarted { day: 4 },
            ]
        );
    }

    #[test]
    fn paced_and_headless_give_the_same_result() {
        let mut headless = small_world();
        let mut paced = small_world();
        let expected = run_headless(&mut headless, 2);
        let mut seen = Vec::new();
        // Very fast pace so the test stays quick; the result must not depend on it.
        let got = run_paced(&mut paced, 10_000.0, Some(2), |_, ev| {
            seen.extend(ev.iter().cloned());
        });
        assert_eq!(got, expected);
        assert_eq!(seen, expected.events);
        assert_eq!(paced.tick(), headless.tick());
    }

    #[test]
    fn paced_stops_at_the_boundary_even_when_behind() {
        // A slow calendar (1 tick/s) with a huge scale: the first loop
        // iteration owes many ticks at once, but must not overshoot.
        let mut w = World::new(Calendar::new(1, 2).unwrap()); // 2 ticks per day
        let s = run_paced(&mut w, 1_000_000.0, Some(1), |_, _| {});
        assert_eq!(s.ticks, 2);
        assert_eq!(w.time().day, 2);
    }

    #[test]
    fn tick_duration_follows_scale() {
        assert_eq!(tick_duration(10, 1.0), Duration::from_millis(100));
        assert_eq!(tick_duration(10, 2.0), Duration::from_millis(50));
    }
}
