//! The vivarium's continent kept on disk between runs (D51, lightening
//! 4): a cache of this tool, nothing of the game's (the game's server
//! keeps its continents in `continents.sqlite`, D28; this tool stays on
//! `sim` and `std::fs`, D41). The continent of (seed, side) is written as
//! [`Continent::to_bytes`] gives it (the simulation's own layout: magic
//! and layout version, little-endian fields, a checksum) under
//! `${XDG_CACHE_HOME:-~/.cache}/soul-sim/continents/`, named by its seed,
//! its side and the generator's version ([`VIVARIUM_GENERATOR`]), so a
//! file of another generator is never read. A window cut from the kept
//! continent is the born one (D28's amendment of 19 November 2026): the
//! run's pages are the same with and without, and the birth loses the 5
//! to 17 s the continent cost (92 to 96 % of it).

use std::path::{Path, PathBuf};

use sim::map::VIVARIUM_GENERATOR;
use sim::plan::Continent;

/// Where the continents are kept unless told otherwise:
/// `$XDG_CACHE_HOME/soul-sim/continents`, or `~/.cache/soul-sim/continents`.
#[must_use]
pub fn default_dir() -> PathBuf {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(|| PathBuf::from(".cache"));
    cache.join("soul-sim").join("continents")
}

/// The file of the continent of `seed`, `side` cells a side, under
/// `dir`: stamped with the generator's version.
#[must_use]
pub fn path_of(dir: &Path, seed: u64, side: usize) -> PathBuf {
    dir.join(format!("continent-{seed}-{side}-g{VIVARIUM_GENERATOR}.bin"))
}

/// The continent of `seed` and `side` kept under `dir`, if a whole one
/// is there: a missing, damaged or foreign file gives `None`, and the
/// continent is born again (and kept over it).
#[must_use]
pub fn read(dir: &Path, seed: u64, side: usize) -> Option<Continent> {
    let bytes = std::fs::read(path_of(dir, seed, side)).ok()?;
    Continent::from_bytes(&bytes).filter(|c| c.seed == seed && c.grid.side == side)
}

/// Keeps `continent` under `dir`, in place of any kept of its seed and
/// side: written whole beside it, then moved over, so a run stopped
/// midway leaves no half file.
///
/// # Errors
///
/// The file system's, when the directory cannot be made or the file
/// written or moved.
pub fn keep(dir: &Path, continent: &Continent) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = path_of(dir, continent.seed, continent.grid.side);
    let part = path.with_extension(format!("part{}", std::process::id()));
    std::fs::write(&part, continent.to_bytes())?;
    std::fs::rename(&part, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "soul-vivarium-continents-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_kept_continent_reads_back_whole_and_a_foreign_or_damaged_one_is_refused() {
        let dir = scratch("round-trip");
        let small = Continent::with(7, 64, None);
        assert!(read(&dir, 7, 64).is_none(), "nothing kept yet");
        keep(&dir, &small).unwrap();
        let kept = read(&dir, 7, 64).expect("kept");
        assert_eq!(kept.to_bytes(), small.to_bytes());
        assert_eq!(kept.fingerprint(), small.fingerprint());
        assert!(read(&dir, 8, 64).is_none(), "another seed is not there");
        // A file of another seed under this one's name: refused.
        std::fs::write(
            path_of(&dir, 9, 64),
            Continent::with(7, 64, None).to_bytes(),
        )
        .unwrap();
        assert!(read(&dir, 9, 64).is_none());
        // A damaged file: refused.
        let mut bytes = small.to_bytes();
        bytes[100] ^= 0xff;
        std::fs::write(path_of(&dir, 7, 64), bytes).unwrap();
        assert!(read(&dir, 7, 64).is_none());
        // Kept again over it: whole again.
        keep(&dir, &small).unwrap();
        assert_eq!(
            read(&dir, 7, 64).map(|c| c.to_bytes()),
            Some(small.to_bytes())
        );
        assert!(
            path_of(&dir, 7, 64)
                .to_string_lossy()
                .ends_with(&format!("continent-7-64-g{VIVARIUM_GENERATOR}.bin"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_default_dir_follows_the_cache_home() {
        let dir = default_dir();
        assert!(dir.ends_with("soul-sim/continents"), "{}", dir.display());
    }
}
