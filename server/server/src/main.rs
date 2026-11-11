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
use server::cli::{CliError, DataLocation, USAGE, WarmUp};
use server::content;
use server::net::{Published, Shared, condition_view, refusal_view, serve};
use server::pool::{ChunkPool, chunk_threads};
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

    if let Some((x, y)) = opts.dump_chunk {
        // Development: a chunk as the client decodes it, then nothing more.
        let shared = Shared::new(&world, &content, 1, 1.0);
        print!(
            "{}",
            shared
                .dump_chunk(sim::ChunkPos::new(x, y))
                .map_err(|e| e.to_string())?
        );
        return Ok(());
    }

    if opts.headless {
        let days = opts.days.expect("validated by Options::parse");
        println!("headless: simulating {days} day(s) as fast as possible");
        let started = std::time::Instant::now();
        let mut persister = persister;
        let summary = run_headless(&mut world, &content, days, |world, events| {
            persister.on_tick(world, events);
        });
        persister.finish(&world)?;
        let elapsed = started.elapsed();
        println!(
            "done: {} ticks, {} events, now day {} — {:.3}s real time",
            summary.ticks,
            summary.events,
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
    let mut storage: Box<dyn Storage + Send> = match &opts.data {
        DataLocation::File(path) => {
            if opts.reset {
                // The database and SQLite's sidecar files, if any.
                for suffix in ["", "-wal", "-shm", "-journal"] {
                    let mut file = path.as_os_str().to_owned();
                    file.push(suffix);
                    match std::fs::remove_file(&file) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(e.into()),
                    }
                }
                println!("data: {} (reset: starting a new world)", path.display());
            } else {
                println!("data: {}", path.display());
            }
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
        World::with_valley(opts.calendar, content, opts.valley_size)
    };

    // A game with players saves every few real minutes; a headless run,
    // which plays days in seconds, at each new game day.
    let every = (!opts.headless).then_some(CHECKPOINT_EVERY);
    Ok((world, Persister::new(storage, stop, every)))
}

/// How often a game with players saves the world: a crash loses at most
/// this much play, which the project lead accepts (D28).
const CHECKPOINT_EVERY: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// What the simulation hands the storage thread: a copy of the world, to
/// snapshot. Cheap: the map is shared, not copied; bodies, items and
/// trees are.
type Write = Box<World>;

/// Takes snapshots for the simulation loop, on a
/// thread of its own: the simulation only copies what it hands over, and
/// never waits for the disk (D29).
///
/// The first failure stops the writer and asks the loop to stop;
/// [`Persister::finish`] reports it.
struct Persister {
    sender: Option<std::sync::mpsc::Sender<Write>>,
    writer: Option<std::thread::JoinHandle<Result<(), StorageError>>>,
    /// Real time between snapshots; `None`: at each new game day.
    every: Option<std::time::Duration>,
    last: std::time::Instant,
}

impl Persister {
    fn new(
        storage: Box<dyn Storage + Send>,
        stop: Arc<AtomicBool>,
        every: Option<std::time::Duration>,
    ) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel::<Write>();
        let writer = std::thread::Builder::new()
            .name("storage".to_owned())
            .spawn(move || {
                let mut storage = storage;
                for world in receiver {
                    if let Err(e) = storage.save_world(&world) {
                        eprintln!("persistence failed, stopping: {e}");
                        stop.store(true, Ordering::Relaxed);
                        return Err(e);
                    }
                }
                Ok(())
            })
            .expect("the storage thread starts");
        Self {
            sender: Some(sender),
            writer: Some(writer),
            every,
            last: std::time::Instant::now(),
        }
    }

    /// Hands over a copy of the world when a snapshot is due.
    fn on_tick(&mut self, world: &World, events: &[Event]) {
        let due = match self.every {
            Some(every) => self.last.elapsed() >= every,
            None => events.iter().any(|e| matches!(e, Event::DayStarted { .. })),
        };
        if due {
            self.last = std::time::Instant::now();
            self.send(Box::new(world.clone()));
        }
    }

    fn send(&mut self, write: Write) {
        // A writer that has stopped has failed: `finish` reports why.
        if self.sender.as_ref().is_some_and(|s| s.send(write).is_err()) {
            self.sender = None;
        }
    }

    /// The final snapshot, written before returning; or the failure that
    /// interrupted the run.
    fn finish(mut self, world: &World) -> Result<(), StorageError> {
        self.send(Box::new(world.clone()));
        drop(self.sender.take());
        self.writer
            .take()
            .expect("finished once")
            .join()
            .expect("the storage thread does not panic")
    }
}

fn run_paced_server(
    mut world: World,
    opts: &Options,
    content: &Arc<Content>,
    stop: &Arc<AtomicBool>,
    mut persister: Persister,
) -> Result<(), BoxError> {
    // The network only translates (D29): two workers are plenty, and the
    // machine's cores stay for the simulation thread and the chunk pool.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let cores = std::thread::available_parallelism().map_or(1, usize::from);
    let pool = Arc::new(ChunkPool::new(
        opts.chunk_threads.unwrap_or_else(|| chunk_threads(cores)),
    ));
    println!(
        "chunk pool: {} thread(s) beside the network ({cores} hardware threads)",
        pool.threads()
    );

    let listener = runtime.block_on(TcpListener::bind(opts.listen))?;
    let address = listener.local_addr()?;
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

    let mut shared = Shared::new(&world, content, opts.max_connections, opts.time_scale);
    if opts.dev {
        shared.enable_dev();
        println!("development tools on: overview and teleports (--dev)");
    }
    if opts.warm_up == WarmUp::BeforeListening {
        shared.warm_plants();
    }
    let shared = Arc::new(shared);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let acceptor = runtime.spawn(serve(
        listener,
        Arc::clone(&shared),
        Arc::clone(&pool),
        shutdown_rx,
    ));
    // Ready once the shared state is built and connections are accepted
    // (a large valley takes a moment to copy). Tests parse this line to
    // learn the port: keep its shape stable.
    println!("listening on {address}");

    let mut simulation = {
        let shared = Arc::clone(&shared);
        let stop = Arc::clone(stop);
        let time_scale = opts.time_scale;
        let days = opts.days;
        let rules = Arc::clone(content);
        runtime.spawn_blocking(move || {
            let before_tick = |world: &mut World| {
                for intention in shared.take_intentions() {
                    world.queue(intention);
                }
            };
            let on_tick = |world: &World, events: &[Event]| {
                shared.tick.store(world.tick().0, Ordering::Relaxed);
                persister.on_tick(world, events);
                for event in events {
                    publish(&shared, world, event);
                }
            };
            let summary = run_paced(
                &mut world,
                &rules,
                time_scale,
                days,
                &stop,
                before_tick,
                on_tick,
            );
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
        summary.events,
        world.time().day
    );
    Ok(())
}

/// Tells the connections and the pilots what one event means to them
/// (D29: the simulation thread publishes, the connections filter).
#[allow(clippy::too_many_lines)]
fn publish(shared: &Shared, world: &World, event: &Event) {
    let tick = world.tick().0;
    // Publishing to no subscriber is not an error: nobody is
    // connected.
    match event {
        Event::BodyWalks {
            body,
            from,
            route,
            speed_cm_per_s,
        } => {
            shared.body_walks(*body, *from, route, *speed_cm_per_s);
            let _ = shared.publish.send(Published::BodyWalks {
                body: body.0,
                from: from.tile(),
                tick,
            });
        }
        Event::BodyMoved {
            body,
            to,
            at,
            remaining,
        } => {
            if let Some(from) = shared.body_moved(*body, *at, *remaining) {
                let _ = shared.publish.send(Published::BodyMoved {
                    body: body.0,
                    from,
                    to: *to,
                    tick,
                });
            }
        }
        Event::BodyStopped { body, at } => {
            shared.body_stopped(*body, *at);
            let _ = shared.publish.send(Published::BodyStopped {
                body: body.0,
                at: at.tile(),
            });
        }
        Event::DayStarted { day } => {
            println!("tick {tick}: {event:?}");
            shared.day_started(*day);
            let _ = shared
                .publish
                .send(Published::DayStarted { day: *day, tick });
        }
        Event::NightFell { day } => {
            shared.night_fell();
            let _ = shared.publish.send(Published::NightFell { day: *day });
        }
        Event::ConditionChanged { body, summary } => {
            shared.tell_pilot(
                *body,
                ServerMessage::BodyCondition {
                    body: body.0,
                    condition: condition_view(*summary),
                },
            );
            let asleep = summary.4;
            if shared.body_sleeps(*body, asleep)
                && let Some(at) = shared.position_of(body.0)
            {
                let _ = shared.publish.send(Published::BodySleeps {
                    body: body.0,
                    at,
                    asleep,
                });
            }
        }
        Event::BodyAte { .. } => {}
        Event::ActRefused {
            body,
            act,
            item,
            reason,
        } => {
            let (act, reason) = refusal_view(*act, *reason);
            shared.tell_pilot(
                *body,
                ServerMessage::ActRefused {
                    body: body.0,
                    act,
                    item: item.0,
                    reason,
                },
            );
        }
        Event::InventoryChanged {
            body,
            held,
            carried,
            load_g,
            capacity_g,
        } => {
            let view = |id: &sim::ItemId| protocol::ItemView {
                id: id.0,
                material: world
                    .item(*id)
                    .map(|i| i.material().to_owned())
                    .unwrap_or_default(),
            };
            shared.tell_pilot(
                *body,
                ServerMessage::Inventory {
                    body: body.0,
                    held: held.as_ref().map(view),
                    carried: carried.iter().map(view).collect(),
                    load_g: *load_g,
                    capacity_g: *capacity_g,
                },
            );
        }
        Event::BodyHolds { body, item } => {
            let item = item
                .and_then(|id| world.item(id))
                .map(|i| protocol::ItemView {
                    id: i.id().0,
                    material: i.material().to_owned(),
                });
            shared.body_holds(*body, item.clone());
            if let Some(at) = shared.position_of(body.0) {
                let _ = shared.publish.send(Published::BodyHolds {
                    body: body.0,
                    at,
                    item,
                });
            }
        }
        Event::ItemAdded {
            item,
            material,
            at,
            version,
        } => {
            let state = protocol::ItemState {
                id: item.0,
                x: at.x,
                y: at.y,
                material: material.clone(),
            };
            shared.item_added(state.clone(), *version);
            let _ = shared.publish.send(Published::ItemAdded {
                item: state,
                version: *version,
            });
        }
        Event::ItemRemoved { item, at, version } => {
            shared.item_removed(*item, *at, *version);
            let _ = shared.publish.send(Published::ItemRemoved {
                item: item.0,
                at: *at,
                version: *version,
            });
        }
        Event::BodyDied {
            body, cause, at, ..
        } => {
            println!("tick {tick}: body {} died ({cause:?}) at {at:?}", body.0);
            let cause = match cause {
                sim::Death::Starved => protocol::DeathCause::Starved,
                sim::Death::Froze => protocol::DeathCause::Froze,
            };
            shared.body_died(*body, cause);
            let _ = shared.publish.send(Published::BodyDied {
                body: body.0,
                at: *at,
                cause,
            });
        }
        // A refusal concerns only whoever asked.
        Event::MoveRefused { body, target } => shared.tell_pilot(
            *body,
            ServerMessage::MoveRefused {
                body: body.0,
                x: target.x,
                y: target.y,
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::Persister;
    use server::{Storage, StorageError};
    use sim::{BodyId, Calendar, Event, FinePos, Map, Terrain, World};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    /// A storage that writes down what it is asked, and fails on demand.
    struct Recorder {
        log: Arc<Mutex<Vec<String>>>,
        fail: bool,
    }

    impl Storage for Recorder {
        fn latest_world(&mut self) -> Result<Option<World>, StorageError> {
            Ok(None)
        }
        fn save_world(&mut self, world: &World) -> Result<(), StorageError> {
            if self.fail {
                return Err(StorageError::UnsupportedSnapshot {
                    found: 0,
                    supported: 0,
                });
            }
            self.log
                .lock()
                .unwrap()
                .push(format!("snapshot {}", world.tick().0));
            Ok(())
        }
    }

    fn world() -> World {
        World::empty(
            Calendar::new(2, 3).unwrap(),
            Map::filled(8, 8, Terrain::Grass),
        )
    }

    #[test]
    fn the_storage_thread_saves_when_due_and_last_the_final_snapshot() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let storage = Box::new(Recorder {
            log: Arc::clone(&log),
            fail: false,
        });
        let moved = Event::BodyStopped {
            body: BodyId(1),
            at: FinePos::new(500, 500),
        };
        let w = world();
        // At each new game day (headless): only the day's start saves.
        let mut daily = Persister::new(storage, Arc::clone(&stop), None);
        daily.on_tick(&w, std::slice::from_ref(&moved));
        daily.on_tick(&w, &[Event::DayStarted { day: 2 }]);
        daily.finish(&w).unwrap();
        assert_eq!(*log.lock().unwrap(), ["snapshot 0", "snapshot 0"]);
        // Due at every tick, as if minutes had passed: each tick saves.
        log.lock().unwrap().clear();
        let storage = Box::new(Recorder {
            log: Arc::clone(&log),
            fail: false,
        });
        let mut timed = Persister::new(storage, Arc::clone(&stop), Some(std::time::Duration::ZERO));
        timed.on_tick(&w, std::slice::from_ref(&moved));
        timed.on_tick(&w, &[moved]);
        timed.finish(&w).unwrap();
        assert_eq!(
            *log.lock().unwrap(),
            ["snapshot 0", "snapshot 0", "snapshot 0"]
        );
        assert!(!stop.load(Ordering::Relaxed));
    }

    #[test]
    fn a_failing_storage_stops_the_loop_and_finish_reports_it() {
        let stop = Arc::new(AtomicBool::new(false));
        let storage = Box::new(Recorder {
            log: Arc::new(Mutex::new(Vec::new())),
            fail: true,
        });
        let persister = Persister::new(storage, Arc::clone(&stop), None);
        assert!(persister.finish(&world()).is_err());
        assert!(stop.load(Ordering::Relaxed));
    }
}
