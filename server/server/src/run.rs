//! The simulation loop (D29): one thread, fixed ticks.
//!
//! Two ways to run the same loop:
//! - [`run_headless`]: as fast as the machine allows, no pacing, stops after
//!   a number of game days. Used for tests and scenario checks (D09).
//! - [`run_paced`]: ticks are spread over real time according to the
//!   calendar and a speed factor. Used when clients are connected.
//!
//! Both call the same `World::step`, so the pace never changes the result.

use std::sync::atomic::{AtomicBool, Ordering};
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
///
/// `on_tick` is called after every tick with the events of that tick
/// (often none), exactly as in [`run_paced`].
pub fn run_headless(
    world: &mut World,
    days: u64,
    mut on_tick: impl FnMut(&World, &[Event]),
) -> RunSummary {
    let total = days * world.calendar().ticks_per_day();
    let mut summary = RunSummary::default();
    let mut events = Vec::new();
    for _ in 0..total {
        events.clear();
        world.step(&mut events);
        summary.ticks += 1;
        on_tick(world, &events);
        summary.events.extend(events.iter().cloned());
    }
    summary
}

/// Runs `world` in real time, `time_scale` times faster than the calendar
/// says, until `days` full game days have passed (or forever when `None`),
/// or until `stop` becomes true.
///
/// `before_tick` runs just before each tick: this is where the server
/// hands the world the intentions that arrived from clients. `on_tick`
/// is called after every tick with the events of that tick (often none);
/// this is where the server notifies clients and the event journal.
///
/// Uses a fixed-step accumulator: if the machine falls behind, several
/// ticks run back to back to catch up, and the rules never see the
/// difference.
pub fn run_paced(
    world: &mut World,
    time_scale: f64,
    days: Option<u64>,
    stop: &AtomicBool,
    mut before_tick: impl FnMut(&mut World),
    mut on_tick: impl FnMut(&World, &[Event]),
) -> RunSummary {
    let tick_duration = tick_duration(world.calendar().ticks_per_second(), time_scale);
    let stop_at_tick = days.map(|d| world.tick().0 + d * world.calendar().ticks_per_day());
    let reached_limit = |world: &World| stop_at_tick.is_some_and(|stop| world.tick().0 >= stop);

    let mut summary = RunSummary::default();
    let mut events = Vec::new();
    let mut last = Instant::now();
    let mut owed = Duration::ZERO;

    loop {
        if reached_limit(world) || stop.load(Ordering::Relaxed) {
            return summary;
        }

        let now = Instant::now();
        owed += now.duration_since(last);
        last = now;

        while owed >= tick_duration && !reached_limit(world) && !stop.load(Ordering::Relaxed) {
            owed -= tick_duration;
            events.clear();
            before_tick(world);
            world.step(&mut events);
            summary.ticks += 1;
            on_tick(world, &events);
            summary.events.extend(events.iter().cloned());
        }

        if owed < tick_duration {
            // Never sleep long: the stop flag must be noticed promptly.
            std::thread::sleep(tick_duration.saturating_sub(owed).min(MAX_SLEEP));
        }
    }
}

/// Upper bound on one sleep of the paced loop, so a stop request is seen
/// quickly even with a very slow calendar.
const MAX_SLEEP: Duration = Duration::from_millis(50);

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

    fn never_stop() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn headless_runs_exactly_the_requested_days() {
        let mut w = small_world();
        let s = run_headless(&mut w, 3, |_, _| {});
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
        let expected = run_headless(&mut headless, 2, |_, _| {});
        let mut seen = Vec::new();
        let mut ticks_seen = 0;
        // Very fast pace so the test stays quick; the result must not depend on it.
        let got = run_paced(
            &mut paced,
            10_000.0,
            Some(2),
            &never_stop(),
            |_| {},
            |_, ev| {
                ticks_seen += 1;
                seen.extend(ev.iter().cloned());
            },
        );
        assert_eq!(got, expected);
        assert_eq!(seen, expected.events);
        assert_eq!(ticks_seen, expected.ticks);
        assert_eq!(paced.tick(), headless.tick());
    }

    #[test]
    fn paced_stops_at_the_boundary_even_when_behind() {
        // A slow calendar (1 tick/s) with a huge scale: the first loop
        // iteration owes many ticks at once, but must not overshoot.
        let mut w = World::new(Calendar::new(1, 2).unwrap()); // 2 ticks per day
        let s = run_paced(
            &mut w,
            1_000_000.0,
            Some(1),
            &never_stop(),
            |_| {},
            |_, _| {},
        );
        assert_eq!(s.ticks, 2);
        assert_eq!(w.time().day, 2);
    }

    #[test]
    fn paced_honours_the_stop_flag() {
        let mut w = small_world();
        let stop = AtomicBool::new(false);
        let s = run_paced(
            &mut w,
            10_000.0,
            None,
            &stop,
            |_| {},
            |world, _| {
                if world.tick().0 == 5 {
                    stop.store(true, Ordering::Relaxed);
                }
            },
        );
        assert_eq!(s.ticks, 5);
    }

    #[test]
    fn tick_duration_follows_scale() {
        assert_eq!(tick_duration(10, 1.0), Duration::from_millis(100));
        assert_eq!(tick_duration(10, 2.0), Duration::from_millis(50));
    }
}
