//! Work shared between threads (`std::thread` only), the results in the
//! order of the items whatever thread made them.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The threads to use: `PLANET_THREADS`, else every core.
pub fn threads() -> usize {
    std::env::var("PLANET_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from))
        .max(1)
}

/// `work` applied to every item on up to `threads` threads; the results
/// in the items' order.
pub fn par_map<T: Sync, R: Send>(
    items: &[T],
    threads: usize,
    work: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<Option<R>>> = Mutex::new(items.iter().map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..threads.clamp(1, items.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else {
                        break;
                    };
                    let result = work(item);
                    done.lock().expect("the results")[i] = Some(result);
                }
            });
        }
    });
    done.into_inner()
        .expect("the results")
        .into_iter()
        .map(|r| r.expect("every item worked"))
        .collect()
}
