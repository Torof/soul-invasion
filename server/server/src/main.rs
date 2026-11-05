//! Soul Invasion game server binary.
//!
//! Thin wrapper around the `server` library: parse options, build the
//! world, run the loop, report. Run `server --help` for the options.

use std::process::ExitCode;

use server::cli::{CliError, USAGE};
use server::{Options, run_headless, run_paced};
use sim::World;

fn main() -> ExitCode {
    let opts = match Options::parse(std::env::args().skip(1)) {
        Ok(opts) => opts,
        Err(CliError::HelpRequested) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let mut world = World::new(opts.calendar);
    let calendar = *world.calendar();
    println!(
        "soul-invasion server (protocol v{}) — calendar: {} ticks/s, {} s/day ({} ticks/day)",
        protocol::PROTOCOL_VERSION,
        calendar.ticks_per_second(),
        calendar.seconds_per_day(),
        calendar.ticks_per_day(),
    );

    if opts.headless {
        let days = opts.days.expect("validated by Options::parse");
        println!("headless: simulating {days} day(s) as fast as possible");
        let started = std::time::Instant::now();
        let summary = run_headless(&mut world, days);
        let elapsed = started.elapsed();
        println!(
            "done: {} ticks, {} events, now day {} — {:.3}s real time",
            summary.ticks,
            summary.events.len(),
            world.time().day,
            elapsed.as_secs_f64(),
        );
        return ExitCode::SUCCESS;
    }

    match opts.days {
        Some(days) => println!(
            "paced: time scale x{}, stopping after {days} day(s)",
            opts.time_scale
        ),
        None => println!(
            "paced: time scale x{}, running until stopped",
            opts.time_scale
        ),
    }
    let summary = run_paced(&mut world, opts.time_scale, opts.days, |world, events| {
        for event in events {
            println!("tick {}: {event:?}", world.tick().0);
        }
    });
    println!(
        "stopped: {} ticks, {} events, now day {}",
        summary.ticks,
        summary.events.len(),
        world.time().day
    );
    ExitCode::SUCCESS
}
