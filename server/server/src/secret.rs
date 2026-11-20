//! What a real world keeps to itself: its seed, drawn at its birth and
//! never told, and the random numbers the server needs for it (the
//! security audit of 20 November 2026, F1 and K1; the project lead's
//! call Q3: a secret random seed per real world, seed 1 the public
//! development seed).
//!
//! No crate for randomness is added: the numbers come from the standard
//! library's hasher, keyed per process from the operating system's
//! entropy ([`RandomState`]: 128 bits of key, SipHash-1-3), fed the
//! clock and the process id so that two draws of one process differ
//! too. Enough for a seed nobody can guess from outside; not a stream
//! for anything that needs more than one number a world.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::Options;

/// How many numbers were drawn by this process: folded in, so that two
/// draws of the same instant differ.
static DRAWN: AtomicU64 = AtomicU64::new(0);

/// A 64-bit number nobody outside the process can guess.
#[must_use]
pub fn draw() -> u64 {
    let mut hasher = RandomState::new().build_hasher();
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
        .hash(&mut hasher);
    Instant::now().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    DRAWN.fetch_add(1, Ordering::Relaxed).hash(&mut hasher);
    hasher.finish()
}

/// The seed a new world is cut from: the one asked for (`--seed`), else
/// the public development seed under `--dev` (a server that hands every
/// client the map's overview keeps no secret), else one drawn here,
/// never printed and never sent. A world keeps its seed for life in its
/// snapshot (`MapSpec`), so this is read once, at its birth.
#[must_use]
pub fn world_seed(opts: &Options) -> u64 {
    opts.seed
        .unwrap_or_else(|| if opts.dev { sim::VALLEY_SEED } else { draw() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_world_is_cut_from_a_seed_of_its_own() {
        let real = Options::parse(Vec::<&str>::new()).unwrap();
        let seeds: Vec<u64> = (0..4).map(|_| world_seed(&real)).collect();
        assert!(
            seeds.iter().all(|&s| s != sim::VALLEY_SEED),
            "never the public valley: {seeds:?}"
        );
        assert!(
            seeds.windows(2).all(|w| w[0] != w[1]),
            "drawn, not derived: {seeds:?}"
        );
    }

    #[test]
    fn development_and_an_explicit_seed_keep_the_valley_asked_for() {
        assert_eq!(
            world_seed(&Options::parse(["--dev"]).unwrap()),
            sim::VALLEY_SEED
        );
        assert_eq!(world_seed(&Options::parse(["--seed", "42"]).unwrap()), 42);
        assert_eq!(
            world_seed(&Options::parse(["--dev", "--seed", "42"]).unwrap()),
            42
        );
    }

    #[test]
    fn two_draws_differ() {
        assert_ne!(draw(), draw());
    }
}
