//! The storage thread (D28, D29): the simulation hands it a copy of the
//! world when a checkpoint is due and never waits for the disk. What can
//! go wrong there is kept from costing the world:
//!
//! - a failed or panicking save asks the simulation to stop at once
//!   (the clean stop of "persistence failed"), so nothing is played on
//!   unsaved (audit S1);
//! - the queue holds one snapshot beside the one being written: when the
//!   writer is behind, a checkpoint is skipped, counted and logged
//!   rather than piled up in memory (audit S7);
//! - the first checkpoint is taken as soon as the first body is born,
//!   not five minutes later, so a crash in the first minutes does not
//!   lose a new world whole (audit S9).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::time::{Duration, Instant};

use sim::{Event, World};

use crate::storage::{Storage, StorageError};

/// How often a game with players saves the world: a crash loses at most
/// this much play, which the project lead accepts (D28).
pub const CHECKPOINT_EVERY: Duration = Duration::from_secs(5 * 60);

/// Snapshots waiting beside the one being written. One: the next
/// checkpoint is already the newest world, an older one in the queue
/// would be written for nothing.
const QUEUED_SNAPSHOTS: usize = 1;

/// What the simulation hands the storage thread: a copy of the world, to
/// snapshot. Cheap: the map is shared, not copied; bodies, items and
/// trees are.
enum Write {
    /// A checkpoint.
    Snapshot(Box<World>),
    /// The final snapshot of a clean stop, followed by its mark.
    Last(Box<World>),
}

/// Takes snapshots for the simulation loop, on a thread of its own: the
/// simulation only copies what it hands over, and never waits for the
/// disk (D29).
///
/// The first failure stops the writer and asks the loop to stop;
/// [`Persister::finish`] reports it.
pub struct Persister {
    sender: Option<SyncSender<Write>>,
    writer: Option<std::thread::JoinHandle<Result<(), StorageError>>>,
    /// Real time between snapshots; `None`: at each new game day.
    every: Option<Duration>,
    last: Instant,
    /// Checkpoints skipped because the writer was behind.
    skipped: u64,
    /// Whether a checkpoint was handed over since the run began.
    saved_once: bool,
}

/// Asks the simulation to stop when the storage thread unwinds: a panic
/// there must end the run as a failed save does, not silence every
/// later save while the world plays on.
struct StopOnPanic {
    stop: Arc<AtomicBool>,
}

impl Drop for StopOnPanic {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("persistence failed, stopping: the storage thread panicked");
            self.stop.store(true, Ordering::Relaxed);
        }
    }
}

impl Persister {
    /// Starts the storage thread, writing to `storage`; a failure sets
    /// `stop`. `every`: real time between checkpoints, or `None` for one
    /// at each new game day.
    #[must_use]
    pub fn new(
        storage: Box<dyn Storage + Send>,
        stop: Arc<AtomicBool>,
        every: Option<Duration>,
    ) -> Self {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<Write>(QUEUED_SNAPSHOTS);
        let writer = std::thread::Builder::new()
            .name("storage".to_owned())
            .spawn(move || {
                let _guard = StopOnPanic {
                    stop: Arc::clone(&stop),
                };
                let mut storage = storage;
                for write in receiver {
                    let (world, last) = match write {
                        Write::Snapshot(world) => (world, false),
                        Write::Last(world) => (world, true),
                    };
                    let saved = storage.save_world(&world).and_then(|()| {
                        // Only once its final snapshot is written: a run
                        // that ends otherwise (a panic) is a crash.
                        if last {
                            storage.mark_clean_stop()
                        } else {
                            Ok(())
                        }
                    });
                    if let Err(e) = saved {
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
            last: Instant::now(),
            skipped: 0,
            saved_once: false,
        }
    }

    /// Hands over a copy of the world when a snapshot is due: at the
    /// first birth of the run, then every `every`, or at each new day.
    pub fn on_tick(&mut self, world: &World, events: &[Event]) {
        let due = match self.every {
            Some(every) => self.last.elapsed() >= every,
            None => events.iter().any(|e| matches!(e, Event::DayStarted { .. })),
        };
        let first_birth = !self.saved_once && !world.bodies().is_empty();
        if (due || first_birth) && self.send(Write::Snapshot(Box::new(world.clone()))) {
            self.last = Instant::now();
            self.saved_once = true;
        }
    }

    /// Checkpoints skipped so far because the writer was behind.
    #[must_use]
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// Hands a snapshot over without waiting: when the writer is behind,
    /// the checkpoint is skipped and counted (the next tick tries again
    /// in a paced game). Whether it was handed over.
    fn send(&mut self, write: Write) -> bool {
        let Some(sender) = &self.sender else {
            return false;
        };
        match sender.try_send(write) {
            Ok(()) => true,
            Err(TrySendError::Full(Write::Snapshot(world))) => {
                self.skipped += 1;
                // Logged at 1, 2, 4, 8...: a headless run saving a day a
                // tick would otherwise print a line a tick.
                if self.skipped.is_power_of_two() {
                    println!(
                        "note: the storage thread is behind: checkpoint at tick {} skipped \
                         ({} skipped so far)",
                        world.tick().0,
                        self.skipped
                    );
                }
                false
            }
            Err(TrySendError::Full(Write::Last(_))) => {
                unreachable!("the last write waits for the queue")
            }
            // A writer that has stopped has failed: `finish` reports why.
            Err(TrySendError::Disconnected(_)) => {
                self.sender = None;
                false
            }
        }
    }

    /// The final snapshot and the clean stop's mark, written before
    /// returning; or the failure that interrupted the run.
    pub fn finish(mut self, world: &World) -> Result<(), StorageError> {
        if let Some(sender) = self.sender.take() {
            // Waits for the queue: the final snapshot is never skipped.
            let _ = sender.send(Write::Last(Box::new(world.clone())));
        }
        if self.skipped > 0 {
            println!(
                "checkpoints skipped (the storage thread was behind): {}",
                self.skipped
            );
        }
        match self.writer.take().expect("finished once").join() {
            Ok(result) => result,
            Err(panic) => Err(StorageError::WriterPanicked(panic_message(panic.as_ref()))),
        }
    }
}

/// What a panic said, when it said anything.
fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "no message".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Lineage;
    use sim::{Calendar, Content};
    use std::sync::Mutex;
    use std::sync::mpsc::{Receiver, channel};

    fn world() -> World {
        World::with_valley(Calendar::new(2, 3).unwrap(), &Content::sample(), 256)
    }

    /// Saves nothing, counts the saves asked, and waits for a permit
    /// before each when given a gate.
    struct Counting {
        saves: Arc<Mutex<u64>>,
        gate: Option<Receiver<()>>,
        panics: bool,
    }

    impl Storage for Counting {
        fn latest_world(&mut self) -> Result<Option<World>, StorageError> {
            Ok(None)
        }

        fn save_world(&mut self, _world: &World) -> Result<(), StorageError> {
            assert!(!self.panics, "a bug in the save path");
            if let Some(gate) = &self.gate {
                gate.recv().expect("a permit");
            }
            *self.saves.lock().unwrap() += 1;
            Ok(())
        }

        fn begin_run(&mut self) -> Result<Lineage, StorageError> {
            Ok(Lineage::default())
        }

        fn mark_clean_stop(&mut self) -> Result<(), StorageError> {
            Ok(())
        }
    }

    /// Waits up to ten seconds for `done`.
    fn wait_until(mut done: impl FnMut() -> bool) -> bool {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    fn wait_for(stop: &AtomicBool) -> bool {
        wait_until(|| stop.load(Ordering::Relaxed))
    }

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
        fn begin_run(&mut self) -> Result<Lineage, StorageError> {
            Ok(Lineage::default())
        }
        fn mark_clean_stop(&mut self) -> Result<(), StorageError> {
            self.log.lock().unwrap().push("clean stop".to_owned());
            Ok(())
        }
    }

    fn empty_world() -> World {
        use sim::{Map, Terrain};
        World::empty(
            Calendar::new(2, 3).unwrap(),
            Map::filled(8, 8, Terrain::Grass),
        )
    }

    #[test]
    fn the_storage_thread_saves_when_due_and_last_the_final_snapshot() {
        use sim::{BodyId, FinePos};
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
        let w = empty_world();
        // At each new game day (headless): only the day's start saves.
        let mut daily = Persister::new(storage, Arc::clone(&stop), None);
        daily.on_tick(&w, std::slice::from_ref(&moved));
        daily.on_tick(&w, &[Event::DayStarted { day: 2 }]);
        daily.finish(&w).unwrap();
        assert_eq!(
            *log.lock().unwrap(),
            ["snapshot 0", "snapshot 0", "clean stop"]
        );
        // Due at every tick, as if minutes had passed: each tick saves,
        // the writer keeping up (it is waited for: the queue holds one).
        log.lock().unwrap().clear();
        let storage = Box::new(Recorder {
            log: Arc::clone(&log),
            fail: false,
        });
        let mut timed = Persister::new(storage, Arc::clone(&stop), Some(Duration::ZERO));
        timed.on_tick(&w, std::slice::from_ref(&moved));
        assert!(wait_until(|| log.lock().unwrap().len() == 1));
        timed.on_tick(&w, &[moved]);
        assert_eq!(timed.skipped(), 0);
        timed.finish(&w).unwrap();
        assert_eq!(
            *log.lock().unwrap(),
            ["snapshot 0", "snapshot 0", "snapshot 0", "clean stop"]
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
        assert!(persister.finish(&empty_world()).is_err());
        assert!(stop.load(Ordering::Relaxed));
    }

    #[test]
    fn a_panicking_writer_stops_the_run_and_finish_reports_it() {
        let stop = Arc::new(AtomicBool::new(false));
        let storage = Counting {
            saves: Arc::default(),
            gate: None,
            panics: true,
        };
        let mut persister =
            Persister::new(Box::new(storage), Arc::clone(&stop), Some(Duration::ZERO));
        let world = world();
        persister.on_tick(&world, &[]);
        assert!(wait_for(&stop), "the simulation was never asked to stop");
        // The final snapshot has nowhere to go: the panic is reported, not
        // raised again.
        let result = persister.finish(&world);
        assert!(
            matches!(result, Err(StorageError::WriterPanicked(ref m)) if m.contains("a bug in the save path")),
            "{result:?}"
        );
    }

    #[test]
    fn a_slow_writer_never_piles_snapshots_up() {
        let stop = Arc::new(AtomicBool::new(false));
        let saves = Arc::new(Mutex::new(0));
        let (permit, gate) = channel();
        let storage = Counting {
            saves: Arc::clone(&saves),
            gate: Some(gate),
            panics: false,
        };
        let mut persister = Persister::new(Box::new(storage), stop, Some(Duration::ZERO));
        let world = world();
        // Ten checkpoints due while the writer waits on its first.
        for _ in 0..10 {
            persister.on_tick(&world, &[]);
        }
        let skipped = persister.skipped();
        for _ in 0..20 {
            permit.send(()).unwrap();
        }
        persister.finish(&world).unwrap();
        // The one being written, the one queued, the last: never the ten.
        let written = *saves.lock().unwrap();
        assert!(written <= 3, "{written} snapshots written");
        assert_eq!(
            skipped,
            10 - (written - 1),
            "skipped {skipped}, written {written}"
        );
    }

    #[test]
    fn the_first_checkpoint_comes_with_the_first_birth() {
        let stop = Arc::new(AtomicBool::new(false));
        let saves = Arc::new(Mutex::new(0));
        let storage = Counting {
            saves: Arc::clone(&saves),
            gate: None,
            panics: false,
        };
        let mut persister =
            Persister::new(Box::new(storage), stop, Some(Duration::from_secs(3600)));
        let mut world = world();
        persister.on_tick(&world, &[]);
        assert!(!persister.saved_once, "nobody is born yet");
        world.spawn_at_camp().expect("room at the camp");
        persister.on_tick(&world, &[]);
        assert!(persister.saved_once, "the first body is born: a checkpoint");
        persister.on_tick(&world, &[]);
        persister.finish(&world).unwrap();
        // The birth's checkpoint and the last: the hour has not passed.
        assert_eq!(*saves.lock().unwrap(), 2);
    }
}
