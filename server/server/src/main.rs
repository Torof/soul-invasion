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
use server::cli::{CliError, DataLocation, TestWorld, USAGE, WarmUp};
use server::content;
use server::net::{
    Published, Shared, blocked_view, condition_view, drink_refused_view, refusal_view,
    refused_view, season_view, serve, thinks_view,
};
use server::persist::{CHECKPOINT_EVERY, Persister};
use server::pool::{ChunkPool, chunk_threads};
use server::storage::ContinentStore;
use server::{Lineage, Options, Sqlite, Storage, run_headless, run_paced};
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
    // The lock on the world file lives as long as this run (D28, amended
    // 20 November 2026).
    let (mut world, persister, lineage, _world_lock) =
        open_world(opts, &content, Arc::clone(&stop))?;
    let calendar = *world.calendar();
    println!(
        "calendar: {} ticks/s, {} s/day ({} ticks/day), {} days/year",
        calendar.ticks_per_second(),
        calendar.seconds_per_day(),
        calendar.ticks_per_day(),
        calendar.days_per_year(),
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

    run_paced_server(world, opts, &content, &stop, persister, lineage)
}

/// Opens storage and loads the latest world from it, or creates a new one.
/// Also starts the run: the world's lineage, its epoch one more if the
/// last run crashed (all 0 for an ephemeral world, which then gets a birth
/// of its own when its identity is made, and for `--dump-chunk`,
/// which only looks and must not take the clean stop's mark).
fn open_world(
    opts: &Options,
    content: &Arc<Content>,
    stop: Arc<AtomicBool>,
) -> Result<(World, Persister, Lineage, Option<std::fs::File>), BoxError> {
    let mut world_lock = None;
    let mut storage: Box<dyn Storage + Send> = match &opts.data {
        DataLocation::File(path) => {
            world_lock = Some(lock_world_file(path)?);
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

    // A world cut from the atlas: its continent read where it was kept,
    // else born and kept there for the next start (D28, 19 November
    // 2026). Only the test worlds are cut from it today, and they are
    // never saved: no stored world needs one.
    let (continents, known) = match opts.world.and_then(TestWorld::continent_seed) {
        Some(seed) => {
            let store = ContinentStore::open(&opts.continents)?;
            let kept = store.load(seed, sim::plan::CONTINENT_SIDE)?;
            println!(
                "continent of seed {seed}: {} {}",
                if kept.is_some() {
                    "read from"
                } else {
                    "to be born and kept in"
                },
                opts.continents.display()
            );
            (Some(store), kept.into_iter().collect())
        }
        None => (None, Vec::new()),
    };
    let (world, born) = sim::plan::born_within(known, || -> Result<World, BoxError> {
        Ok(if let Some(world) = storage.latest_world()? {
            println!(
                "world: resumed at tick {} (day {})",
                world.tick().0,
                world.time().day
            );
            if world.calendar() != &opts.calendar {
                println!("note: the world's own calendar wins over the command line");
            }
            world
        } else if let Some(test) = opts.world {
            println!("world: new, the test world {test:?} (never for play)");
            server::test_world::make(test, opts.calendar, content)
        } else {
            // Its seed is the world's secret (F1): chosen here, kept in
            // the snapshot, never printed.
            println!("world: new");
            let seed = server::secret::world_seed(opts);
            World::on_valley(
                opts.calendar,
                content,
                sim::Map::valley(opts.valley_size, opts.valley_size, seed),
            )
        })
    });
    let world = world?;
    if let Some(store) = &continents {
        for continent in &born {
            store.keep(continent)?;
        }
    }
    drop(born);

    let lasting = matches!(opts.data, DataLocation::File(_)) && opts.dump_chunk.is_none();
    let lineage = if lasting {
        storage.begin_run()?
    } else {
        Lineage::default()
    };

    // A game with players saves every few real minutes; a headless run,
    // which plays days in seconds, at each new game day.
    let every = (!opts.headless).then_some(CHECKPOINT_EVERY);
    Ok((
        world,
        Persister::new(storage, stop, every),
        lineage,
        world_lock,
    ))
}

/// Holds the world file for this server alone (the security audit of 20
/// November 2026): a sidecar `<path>.lock`, locked for the process's
/// life. A second server on the same file is refused at once, instead of
/// two servers braiding their snapshots into one database. The kernel
/// releases the lock when the process ends, however it ends.
fn lock_world_file(path: &std::path::Path) -> Result<std::fs::File, BoxError> {
    let mut name = path.as_os_str().to_owned();
    name.push(".lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&name)?;
    match lock.try_lock() {
        Ok(()) => Ok(lock),
        Err(std::fs::TryLockError::WouldBlock) => Err(format!(
            "the world file {} is held by another server ({} is locked)",
            path.display(),
            std::path::Path::new(&name).display()
        )
        .into()),
        Err(std::fs::TryLockError::Error(e)) => Err(e.into()),
    }
}

fn run_paced_server(
    mut world: World,
    opts: &Options,
    content: &Arc<Content>,
    stop: &Arc<AtomicBool>,
    mut persister: Persister,
    lineage: Lineage,
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
            "paced: time scale x{}, running until stopped (Ctrl-C, SIGTERM)",
            opts.time_scale
        ),
    }

    let shared = Arc::new(shared_state(&world, opts, content, lineage));
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
                shared.bear_bodies(world, &rules);
                world.set_piloted(shared.piloted());
                for intention in shared.take_intentions() {
                    world.queue(intention);
                }
            };
            let on_tick = |world: &World, events: &[Event]| {
                shared.tick.store(world.tick().0, Ordering::Relaxed);
                persister.on_tick(world, events);
                for event in events {
                    publish(&shared, world, &rules, event);
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
            signal = stop_signal() => {
                println!("stopping ({signal})");
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

/// Waits for a request to stop: Ctrl-C, or `SIGTERM` (how scripts and
/// service managers stop a server). Either stops cleanly: the final
/// snapshot, then the clean stop's mark (D28). Its name.
async fn stop_signal() -> &'static str {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => "Ctrl-C",
                _ = term.recv() => "SIGTERM",
            }
        } else {
            let _ = tokio::signal::ctrl_c().await;
            "Ctrl-C"
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        "Ctrl-C"
    }
}

/// The state the network shares with the simulation, as the options
/// ask, the world's lineage folded into its identity.
fn shared_state(world: &World, opts: &Options, content: &Content, lineage: Lineage) -> Shared {
    let mut shared = Shared::new(world, content, opts.max_connections, opts.time_scale);
    shared.set_limits(opts.limits);
    shared.fold_lineage(lineage.birth, lineage.epoch);
    // Tests read this line: keep its shape stable.
    println!("world id: {} (epoch {})", shared.world_id(), lineage.epoch);
    if opts.dev {
        shared.enable_dev();
        println!("development tools on: overview and teleports (--dev)");
    }
    if opts.warm_up == WarmUp::BeforeListening {
        shared.warm_plants();
    }
    shared
}

/// Tells the connections and the pilots what one event means to them
/// (D29: the simulation thread publishes; the router hands each event to
/// the connections it concerns, D5).
#[allow(
    clippy::too_many_lines,
    reason = "one straight reading, top to bottom; splitting it would scatter the rule"
)]
fn publish(shared: &Shared, world: &World, rules: &Content, event: &Event) {
    let tick = world.tick().0;
    // Publishing to no subscriber is not an error: nobody is
    // connected.
    match event {
        Event::BodyWalks {
            body,
            from,
            speed_cm_per_s,
            ..
        } => {
            // Where it is at the tick's end and the rest of its walk, by
            // time: the clients walk it there whatever the ground.
            if let Some(now) = world.body(*body) {
                let timeline = world.walk_timeline(*body, rules);
                shared.body_walks(*body, now.at(), &timeline, *speed_cm_per_s);
            }
            shared.publish(&Published::BodyWalks {
                body: body.0,
                from: from.tile(),
                tick,
            });
        }
        Event::BodyMoved { body, to, at, .. } => {
            // Every step is recorded, so a body told is told where it
            // is; only a step into another chunk is published (A6), its
            // walk worked out again: the clients walk it along its route
            // by time in between.
            if let Some(from) = shared.body_moved(*body, *at, tick)
                && from.chunk() != to.chunk()
            {
                let timeline = world.walk_timeline(*body, rules);
                shared.body_walks(*body, *at, &timeline, rules.movement.walk_speed_cm_per_s);
                shared.publish(&Published::BodyMoved {
                    body: body.0,
                    from,
                    to: *to,
                    tick,
                });
            }
        }
        Event::BodyStopped { body, at } => {
            shared.body_stopped(*body, *at);
            shared.publish(&Published::BodyStopped {
                body: body.0,
                at: at.tile(),
            });
        }
        Event::DayStarted { day } => {
            println!("tick {tick}: {event:?}");
            shared.day_started(*day);
            shared.publish(&Published::DayStarted { day: *day, tick });
        }
        Event::NightFell { day } => {
            shared.night_fell();
            shared.publish(&Published::NightFell { day: *day });
        }
        Event::SeasonStarted { year, season } => {
            println!("tick {tick}: {event:?}");
            shared.publish(&Published::SeasonStarted {
                year: *year,
                season: season_view(*season),
            });
        }
        Event::ConditionChanged { body, summary } => {
            shared.tell_pilot(
                *body,
                ServerMessage::BodyCondition {
                    body: body.0,
                    condition: condition_view(*summary),
                },
            );
            let asleep = summary.5;
            if shared.body_sleeps(*body, asleep)
                && let Some(at) = shared.position_of(body.0)
            {
                shared.publish(&Published::BodySleeps {
                    body: body.0,
                    at,
                    asleep,
                });
            }
        }
        Event::CropPicked { chunk, version } => {
            // The fruit its trees and shrubs show follows what is left (D48).
            if let Some(changes) = world.changes_in(*chunk) {
                shared.crop_picked(*chunk, &changes.picked, *version);
            }
        }
        // Gathering is the brains' alone, and spoiled food goes as an item
        // removed and an inventory changed, both told already: the
        // protocol says nothing more yet (D45).
        Event::BodyAte { .. }
        | Event::BodyDrank { .. }
        | Event::WorkStarted { .. }
        | Event::WorkStopped { .. }
        | Event::GatherRefused { .. }
        | Event::Spoiled { .. } => {}
        Event::DrinkRefused { body, at, reason } => {
            shared.tell_pilot(*body, drink_refused_view(*body, *at, *reason));
        }
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
            let view = |id: &sim::ItemId| {
                let item = world.item(*id);
                protocol::ItemView {
                    id: id.0,
                    material: item.map(|i| i.material().to_owned()).unwrap_or_default(),
                    mass_g: item.map_or(0, sim::Item::mass_g),
                }
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
                    mass_g: i.mass_g(),
                });
            shared.body_holds(*body, item.clone());
            if let Some(at) = shared.position_of(body.0) {
                shared.publish(&Published::BodyHolds {
                    body: body.0,
                    at,
                    item,
                });
            }
        }
        Event::ItemAdded {
            item,
            material,
            mass_g,
            at,
            version,
        } => {
            let state = protocol::ItemState {
                id: item.0,
                x: at.x,
                y: at.y,
                material: material.clone(),
                mass_g: *mass_g,
            };
            shared.item_added(state.clone(), *version);
            shared.publish(&Published::ItemAdded {
                item: state,
                version: *version,
            });
        }
        Event::ItemRemoved { item, at, version } => {
            shared.item_removed(*item, *at, *version);
            shared.publish(&Published::ItemRemoved {
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
                sim::Death::Dehydrated => protocol::DeathCause::Dehydrated,
            };
            shared.body_died(*body, cause);
            shared.publish(&Published::BodyDied {
                body: body.0,
                at: *at,
                cause,
            });
        }
        // A refusal concerns only whoever asked.
        Event::MoveRefused {
            body,
            target,
            reason,
        } => shared.tell_pilot(*body, refused_view(*body, *target, *reason)),
        // So does a body that can get no nearer: what it sees is its own.
        Event::BodyBlocked {
            body,
            at,
            target,
            reason,
        } => shared.tell_pilot(*body, blocked_view(*body, *at, *target, *reason)),
        // And so does what it thinks as it decides how to go on.
        Event::BodyThinks { body, thought, at } => {
            shared.tell_pilot(*body, thinks_view(*body, *thought, *at));
        }
    }
}
