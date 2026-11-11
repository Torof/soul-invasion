//! The chunk pool: a few plain threads beside the network that build what
//! the connections' views ask for (D29, amended 11 November 2026).
//!
//! A connection's view is 361 chunks; building them cold costs about a
//! second of one processor, and used to run inside the connection's own
//! task, on one of the network's two threads: a login or a teleport
//! stalled every other connection, and the connection read no intention
//! until its view was sent. Now a view change hands the pool one job a
//! chunk and the connection goes on reading its client and forwarding
//! the world's events; the built chunks come back through a channel and
//! leave after the events waiting.
//!
//! The pool knows nothing of chunks: it runs jobs, in **lanes**. A lane
//! is a connection; the workers take the lanes in turn, one job each, so
//! four clients logging in at once each see their ground arrive rather
//! than the first one getting all of its before the next gets any. In a
//! lane the jobs run in the order given: nearest the body first.
//!
//! The simulation thread never runs here nor waits for it (D29): a job
//! reads the shared state (the map, the seed, the trees' and plants'
//! caches) and the simulation goes on ticking.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

/// Threads the pool runs at most, whatever the machine: a view is built
/// in a few turns of eight threads, and the rest of a big machine stays
/// for the simulation, the network and whoever else runs on it.
pub const CHUNK_THREADS_MOST: usize = 8;

/// Threads the pool runs on a machine of `cores` hardware threads, when
/// the command line does not say: the cores left once the simulation
/// thread and the network's two have theirs, at least one, at most
/// [`CHUNK_THREADS_MOST`]. The storage thread is not counted: it sleeps
/// between two snapshots.
pub fn chunk_threads(cores: usize) -> usize {
    cores.saturating_sub(3).clamp(1, CHUNK_THREADS_MOST)
}

/// A piece of work for the pool.
pub type Job = Box<dyn FnOnce() + Send + 'static>;

/// A pool of threads running jobs lane by lane. Dropping it lets the
/// jobs started finish, drops the ones waiting and joins the threads.
pub struct ChunkPool {
    inner: Arc<Inner>,
    workers: Vec<JoinHandle<()>>,
}

struct Inner {
    queue: Mutex<Queue>,
    /// Signalled when a job is given or the pool closes.
    ready: Condvar,
}

#[derive(Default)]
struct Queue {
    /// The jobs waiting, by lane, each lane in the order given.
    lanes: BTreeMap<u64, VecDeque<Job>>,
    /// The lanes with jobs waiting, in the order they are served.
    turn: VecDeque<u64>,
    /// The next lane number given out.
    next_lane: u64,
    /// Jobs waiting, in every lane.
    waiting: usize,
    closed: bool,
}

impl std::fmt::Debug for ChunkPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkPool")
            .field("threads", &self.workers.len())
            .field("waiting", &self.waiting())
            .finish_non_exhaustive()
    }
}

impl ChunkPool {
    /// A pool of `threads` threads, at least one.
    pub fn new(threads: usize) -> Self {
        let inner = Arc::new(Inner {
            queue: Mutex::new(Queue::default()),
            ready: Condvar::new(),
        });
        let workers = (0..threads.max(1))
            .map(|i| {
                let inner = Arc::clone(&inner);
                std::thread::Builder::new()
                    .name(format!("chunks-{i}"))
                    .spawn(move || inner.work())
                    .expect("a chunk thread starts")
            })
            .collect();
        Self { inner, workers }
    }

    /// How many threads run.
    pub fn threads(&self) -> usize {
        self.workers.len()
    }

    /// A lane of its own for a connection: its jobs run in order, and in
    /// turn with the other lanes'.
    pub fn lane(&self) -> u64 {
        let mut queue = self.inner.lock();
        let lane = queue.next_lane;
        queue.next_lane += 1;
        lane
    }

    /// Gives `job` to `lane`, to run after the lane's earlier jobs.
    pub fn submit(&self, lane: u64, job: Job) {
        let mut guard = self.inner.lock();
        let queue = &mut *guard;
        if queue.closed {
            return;
        }
        let jobs = queue.lanes.entry(lane).or_default();
        if jobs.is_empty() {
            queue.turn.push_back(lane);
        }
        jobs.push_back(job);
        queue.waiting += 1;
        drop(guard);
        self.inner.ready.notify_one();
    }

    /// Jobs given and not yet started.
    pub fn waiting(&self) -> usize {
        self.inner.lock().waiting
    }
}

impl Drop for ChunkPool {
    fn drop(&mut self) {
        {
            let mut queue = self.inner.lock();
            queue.closed = true;
            queue.lanes.clear();
            queue.turn.clear();
            queue.waiting = 0;
        }
        self.inner.ready.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl Inner {
    fn lock(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.queue.lock().expect("the pool's queue is not poisoned")
    }

    /// A worker's life: the next job of the next lane, until closed.
    fn work(&self) {
        loop {
            let job = {
                let mut queue = self.lock();
                loop {
                    if queue.closed {
                        return;
                    }
                    if let Some(job) = queue.take() {
                        break job;
                    }
                    queue = self
                        .ready
                        .wait(queue)
                        .expect("the pool's queue is not poisoned");
                }
            };
            job();
        }
    }
}

impl Queue {
    /// The next job: the first of the lane whose turn it is, which goes
    /// to the back of the turn if it has more.
    fn take(&mut self) -> Option<Job> {
        let lane = self.turn.pop_front()?;
        let jobs = self.lanes.get_mut(&lane)?;
        let job = jobs.pop_front();
        if jobs.is_empty() {
            self.lanes.remove(&lane);
        } else {
            self.turn.push_back(lane);
        }
        self.waiting -= 1;
        job
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn the_size_follows_the_cores_within_bounds() {
        assert_eq!(chunk_threads(1), 1);
        assert_eq!(chunk_threads(2), 1);
        assert_eq!(chunk_threads(4), 1);
        assert_eq!(chunk_threads(6), 3);
        assert_eq!(chunk_threads(8), 5);
        assert_eq!(chunk_threads(16), 8, "the project lead's machine");
        assert_eq!(chunk_threads(64), CHUNK_THREADS_MOST);
    }

    #[test]
    fn lanes_take_turns_and_keep_their_own_order() {
        // One thread, so the order the jobs run in is the order the pool
        // takes them; the jobs are given while the thread is held up by
        // the first one.
        let pool = ChunkPool::new(1);
        let (tx, rx) = mpsc::channel();
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        let (started_tx, started_rx) = mpsc::channel::<()>();
        let (a, b) = (pool.lane(), pool.lane());
        pool.submit(
            a,
            Box::new(move || {
                started_tx.send(()).unwrap();
                gate_rx.recv().expect("released");
            }),
        );
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the thread holds the first job");
        for i in 0..3 {
            let tx = tx.clone();
            pool.submit(a, Box::new(move || tx.send(format!("a{i}")).unwrap()));
        }
        for i in 0..2 {
            let tx = tx.clone();
            pool.submit(b, Box::new(move || tx.send(format!("b{i}")).unwrap()));
        }
        assert_eq!(pool.waiting(), 5);
        gate_tx.send(()).unwrap();
        let ran: Vec<String> = (0..5)
            .map(|_| rx.recv_timeout(Duration::from_secs(5)).expect("a job ran"))
            .collect();
        assert_eq!(ran, ["a0", "b0", "a1", "b1", "a2"]);
        assert_eq!(pool.waiting(), 0);
    }

    #[test]
    fn every_job_runs_once_on_several_threads_and_drop_joins() {
        let pool = ChunkPool::new(4);
        assert_eq!(pool.threads(), 4);
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (tx, rx) = mpsc::channel();
        for _ in 0..5 {
            let lane = pool.lane();
            for _ in 0..50 {
                let (count, tx) = (Arc::clone(&count), tx.clone());
                pool.submit(
                    lane,
                    Box::new(move || {
                        count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        tx.send(()).unwrap();
                    }),
                );
            }
        }
        for _ in 0..250 {
            rx.recv_timeout(Duration::from_secs(10)).expect("a job ran");
        }
        assert_eq!(count.load(std::sync::atomic::Ordering::Relaxed), 250);
        drop(pool);
        assert_eq!(count.load(std::sync::atomic::Ordering::Relaxed), 250);
    }
}
