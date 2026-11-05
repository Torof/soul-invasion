//! Soul Invasion game server binary.
//!
//! Thin wrapper around the `server` library: parse options, open storage,
//! load or create the world, run the loop, report. Run `server --help` for
//! the options.
//!
//! Paced mode wires the pieces of D29 together: the simulation runs on
//! its own thread; the tokio runtime beside it accepts WebSocket clients
//! and forwards what the simulation publishes.

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use protocol::ServerMessage;
use server::cli::{CliError, DataLocation, USAGE};
use server::content;
use server::net::{Published, Shared, serve};
use server::{Options, Sqlite, Storage, StorageError, run_headless, run_paced};
use sim::{Content, Event, World};
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

    match run(&opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(opts: &Options) -> Result<(), BoxError> {
    println!(
        "soul-invasion server (protocol v{})",
        protocol::PROTOCOL_VERSION
    );

    let content_dir = content::find_dir(opts.content.as_deref())?;
    let content = Arc::new(content::load(&content_dir)?);
    println!(
        "content: {} ({} materials, {} spawn rules)",
        content_dir.display(),
        content.materials.len(),
        content.spawns.len()
    );

    let stop = Arc::new(AtomicBool::new(false));
    let (mut world, persister) = open_world(opts, &content, Arc::clone(&stop))?;
    let calendar = *world.calendar();
    println!(
        "calendar: {} ticks/s, {} s/day ({} ticks/day)",
        calendar.ticks_per_second(),
        calendar.seconds_per_day(),
        calendar.ticks_per_day(),
    );

    if opts.headless {
        let days = opts.days.expect("validated by Options::parse");
        println!("headless: simulating {days} day(s) as fast as possible");
        let started = std::time::Instant::now();
        let mut persister = persister;
        let summary = run_headless(&mut world, days, |world, events| {
            persister.on_tick(world, events);
        });
        persister.finish(&world)?;
        let elapsed = started.elapsed();
        println!(
            "done: {} ticks, {} events, now day {} — {:.3}s real time",
            summary.ticks,
            summary.events.len(),
            world.time().day,
            elapsed.as_secs_f64(),
        );
        return Ok(());
    }

    run_paced_server(world, opts, &content, &stop, persister)
}

/// Opens storage and loads the latest world from it, or creates a new one.
fn open_world(
    opts: &Options,
    content: &Arc<Content>,
    stop: Arc<AtomicBool>,
) -> Result<(World, Persister), BoxError> {
    let mut storage: Box<dyn Storage> = match &opts.data {
        DataLocation::File(path) => {
            println!("data: {}", path.display());
            Box::new(Sqlite::open(path, Arc::clone(content))?)
        }
        DataLocation::Ephemeral => {
            println!("data: ephemeral (nothing is saved)");
            Box::new(Sqlite::in_memory(Arc::clone(content))?)
        }
    };

    let world = if let Some(world) = storage.latest_world()? {
        println!(
            "world: resumed at tick {} (day {})",
            world.tick().0,
            world.time().day
        );
        if world.calendar() != &opts.calendar {
            println!("note: the world's own calendar wins over the command line");
        }
        world
    } else {
        println!("world: new");
        World::new(opts.calendar, content)
    };

    Ok((world, Persister::new(storage, stop)))
}

/// Feeds the journal and takes snapshots from inside the simulation loop.
///
/// The loop's callback cannot return errors, so the first failure is kept,
/// the loop is asked to stop, and [`Persister::finish`] reports it.
struct Persister {
    storage: Box<dyn Storage>,
    stop: Arc<AtomicBool>,
    failed: Option<StorageError>,
}

impl Persister {
    fn new(storage: Box<dyn Storage>, stop: Arc<AtomicBool>) -> Self {
        Self {
            storage,
            stop,
            failed: None,
        }
    }

    /// Journals the tick's events and snapshots the world on each new day.
    fn on_tick(&mut self, world: &World, events: &[Event]) {
        if self.failed.is_some() || events.is_empty() {
            return;
        }
        let day_started = events.iter().any(|e| matches!(e, Event::DayStarted { .. }));
        let result = self
            .storage
            .append_events(world.tick(), events)
            .and_then(|()| {
                if day_started {
                    self.storage.save_world(world)
                } else {
                    Ok(())
                }
            });
        if let Err(e) = result {
            eprintln!("persistence failed, stopping: {e}");
            self.failed = Some(e);
            self.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Final snapshot, or the failure that interrupted the run.
    fn finish(mut self, world: &World) -> Result<(), StorageError> {
        if let Some(e) = self.failed.take() {
            return Err(e);
        }
        self.storage.save_world(world)
    }
}

fn run_paced_server(
    mut world: World,
    opts: &Options,
    content: &Content,
    stop: &Arc<AtomicBool>,
    mut persister: Persister,
) -> Result<(), BoxError> {
    // The network only translates (D29): two workers are plenty, and the
    // machine's cores stay for the simulation thread.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
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

    let shared = Arc::new(Shared::new(&world, content, opts.max_connections));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let acceptor = runtime.spawn(serve(listener, Arc::clone(&shared), shutdown_rx));

    let mut simulation = {
        let shared = Arc::clone(&shared);
        let stop = Arc::clone(stop);
        let time_scale = opts.time_scale;
        let days = opts.days;
        runtime.spawn_blocking(move || {
            let before_tick = |world: &mut World| {
                for intention in shared.take_intentions() {
                    world.queue(intention);
                }
            };
            let on_tick = |world: &World, events: &[Event]| {
                shared.tick.store(world.tick().0, Ordering::Relaxed);
                persister.on_tick(world, events);
                let tick = world.tick().0;
                for event in events {
                    // Publishing to no subscriber is not an error: nobody is
                    // connected.
                    match *event {
                        Event::BodyMoved { body, to } => {
                            if let Some(from) = shared.body_moved(body, to) {
                                let _ = shared.publish.send(Published::BodyMoved {
                                    body: body.0,
                                    from,
                                    to,
                                    tick,
                                });
                            }
                        }
                        Event::DayStarted { day } => {
                            println!("tick {tick}: {event:?}");
                            let _ = shared.publish.send(Published::DayStarted { day, tick });
                        }
                        // A refusal concerns only whoever asked.
                        Event::MoveRefused { body, target } => shared.tell_pilot(
                            body,
                            ServerMessage::MoveRefused {
                                body: body.0,
                                x: target.x,
                                y: target.y,
                            },
                        ),
                        // Arrivals are implied by the last move.
                        Event::BodyArrived { .. } => {}
                    }
                }
            };
            let summary = run_paced(&mut world, time_scale, days, &stop, before_tick, on_tick);
            (summary, world, persister)
        })
    };

    // Run until the simulation reaches its day limit, stops on a
    // persistence failure, or Ctrl-C arrives.
    let finished = runtime.block_on(async {
        tokio::select! {
            finished = &mut simulation => Some(finished),
            _ = tokio::signal::ctrl_c() => {
                println!("stopping (Ctrl-C)");
                None
            }
        }
    });
    let (summary, world, persister) = if let Some(finished) = finished {
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

    persister.finish(&world)?;
    println!(
        "stopped: {} ticks, {} events, now day {}",
        summary.ticks,
        summary.events.len(),
        world.time().day
    );
    Ok(())
}
