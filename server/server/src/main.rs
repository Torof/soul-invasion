//! Soul Invasion game server binary.
//!
//! Thin wrapper around the `server` library: parse options, build the
//! world, run the loop, report. Run `server --help` for the options.
//!
//! Paced mode wires the pieces of D29 together: the simulation runs on
//! its own thread; the tokio runtime beside it accepts WebSocket clients
//! and forwards what the simulation publishes.

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use protocol::ServerMessage;
use server::cli::{CliError, USAGE};
use server::net::{Shared, serve};
use server::{Options, run_headless, run_paced};
use sim::{Event, World};
use tokio::net::TcpListener;
use tokio::sync::watch;

type BoxError = Box<dyn std::error::Error>;

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

    match run_paced_server(world, &opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_paced_server(mut world: World, opts: &Options) -> Result<(), BoxError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let listener = runtime.block_on(TcpListener::bind(opts.listen))?;
    // Tests parse this line to learn the port: keep its shape stable.
    println!("listening on {}", listener.local_addr()?);
    match opts.days {
        Some(days) => println!(
            "paced: time scale x{}, stopping after {days} day(s)",
            opts.time_scale
        ),
        None => println!(
            "paced: time scale x{}, running until stopped (Ctrl-C)",
            opts.time_scale
        ),
    }

    let shared = Arc::new(Shared::new());
    let stop = Arc::new(AtomicBool::new(false));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let acceptor = runtime.spawn(serve(listener, Arc::clone(&shared), shutdown_rx));

    let mut simulation = {
        let shared = Arc::clone(&shared);
        let stop = Arc::clone(&stop);
        let time_scale = opts.time_scale;
        let days = opts.days;
        runtime.spawn_blocking(move || {
            let summary = run_paced(&mut world, time_scale, days, &stop, |world, events| {
                shared.tick.store(world.tick().0, Ordering::Relaxed);
                for event in events {
                    println!("tick {}: {event:?}", world.tick().0);
                    // No subscribers is not an error: nobody is connected.
                    let _ = shared.publish.send(to_message(world, event));
                }
            });
            (summary, world)
        })
    };

    // Run until the simulation reaches its day limit or Ctrl-C arrives.
    let finished = runtime.block_on(async {
        tokio::select! {
            finished = &mut simulation => Some(finished),
            _ = tokio::signal::ctrl_c() => {
                println!("stopping (Ctrl-C)");
                None
            }
        }
    });
    let (summary, world) = if let Some(finished) = finished {
        finished?
    } else {
        stop.store(true, Ordering::Relaxed);
        runtime.block_on(simulation)?
    };

    // Close every connection and stop accepting new ones.
    let _ = shutdown_tx.send(true);
    runtime.block_on(async {
        let _ = acceptor.await;
    });

    println!(
        "stopped: {} ticks, {} events, now day {}",
        summary.ticks,
        summary.events.len(),
        world.time().day
    );
    Ok(())
}

fn to_message(world: &World, event: &Event) -> ServerMessage {
    match *event {
        Event::DayStarted { day } => ServerMessage::DayStarted {
            day,
            tick: world.tick().0,
        },
    }
}
