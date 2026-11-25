//! The vivarium's land kept on disk between runs (D51, amended 25
//! November 2026): a cache of this tool, nothing of the game's (the
//! game's storage is untouched, D28). A chunk's tiles, woods and water
//! are a pure function of the world (its zone, its seed, the generator),
//! never of a run (D24), so the chunks a run made are written out at its
//! end ([`sim::map::Map::land_bytes`]) and read back into the next run's
//! map before its first tick ([`sim::map::Map::take_land_bytes`]), which
//! then reads them as made: the same pages, with and without.
//!
//! The file is named by the world, the generator's version
//! ([`VIVARIUM_GENERATOR`]) and a fingerprint of the running program
//! ([`program_fingerprint`]): the land's code changes far more often than
//! the generator's version is raised, and a chunk kept by another build
//! could differ from the one this build makes, so a new build starts a
//! new file (its first run makes its chunks again, as without a cache) and
//! a file is never read by a build that did not write it. Under
//! `${XDG_CACHE_HOME:-~/.cache}/soul-sim/lands/`.

use std::path::{Path, PathBuf};

use sim::map::{Map, Terrain, VIVARIUM_GENERATOR};

/// Days a kept land of another build is left before it is cleared, when
/// a run keeps its own: long enough for the builds of several worktrees
/// run side by side to keep theirs.
pub const STALE_DAYS: u64 = 3;

/// Where the lands are kept unless told otherwise:
/// `$XDG_CACHE_HOME/soul-sim/lands`, or `~/.cache/soul-sim/lands`.
#[must_use]
pub fn default_dir() -> PathBuf {
    crate::continents::default_dir().with_file_name("lands")
}

/// FNV-1a over `bytes`, eight bytes at a time.
fn fingerprint(bytes: &[u8]) -> u64 {
    let (words, rest) = bytes.as_chunks::<8>();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &word in words {
        h = (h ^ u64::from_le_bytes(word)).wrapping_mul(0x0100_0000_01b3);
    }
    for &b in rest {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// The running program's fingerprint: its file's bytes hashed. `None`
/// when the program cannot read itself; the land is then not kept.
#[must_use]
pub fn program_fingerprint() -> Option<u64> {
    let exe = std::env::current_exe().ok()?;
    Some(fingerprint(&std::fs::read(exe).ok()?))
}

/// The file of the land of `world` (`vivarium`, `vivarium-wide`, ...)
/// kept by the program of fingerprint `program`, under `dir`.
#[must_use]
pub fn path_of(dir: &Path, world: &str, program: u64) -> PathBuf {
    dir.join(format!(
        "land-{world}-g{VIVARIUM_GENERATOR}-{program:016x}.bin"
    ))
}

/// Gives `map` the chunks kept under `dir` for `world` by this `program`:
/// how many chunks of tiles, woods and water it took; `None` when nothing
/// is kept or the file is damaged or foreign (nothing taken, the chunks
/// are made as without a cache).
#[must_use]
pub fn read_into(map: &Map, dir: &Path, world: &str, program: u64) -> Option<[usize; 3]> {
    let bytes = std::fs::read(path_of(dir, world, program)).ok()?;
    map.take_land_bytes(&bytes)
}

/// Keeps `map`'s land under `dir` for `world` and this `program`, with
/// whatever another run kept there meanwhile (the same chunks, so taken
/// as made): written whole beside it, then moved over, so a run stopped
/// midway leaves no half file. Lands of other builds of the same world
/// untouched for [`STALE_DAYS`] are cleared.
///
/// # Errors
///
/// The file system's, when the directory cannot be made or the file
/// written or moved.
pub fn keep(dir: &Path, world: &str, program: u64, map: &Map) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let _ = read_into(map, dir, world, program);
    let Some(bytes) = map.land_bytes() else {
        return Ok(());
    };
    let path = path_of(dir, world, program);
    let part = path.with_extension(format!("part{}", std::process::id()));
    std::fs::write(&part, bytes)?;
    std::fs::rename(&part, &path)?;
    clear_stale(dir, world, program);
    Ok(())
}

/// The first bytes of a kept picture's terrain, with its layout's version.
const PICTURE_MAGIC: &[u8; 8] = b"SOULPIC1";

/// The file of the terrain under the points of `world`'s picture, kept
/// by the program of fingerprint `program`, under `dir`.
#[must_use]
pub fn picture_path_of(dir: &Path, world: &str, program: u64) -> PathBuf {
    dir.join(format!(
        "picture-{world}-g{VIVARIUM_GENERATOR}-{program:016x}.bin"
    ))
}

/// A terrain's code in a kept picture: its place in [`Terrain::ALL`],
/// `u8::MAX` off the map.
fn code_of(terrain: Option<Terrain>) -> u8 {
    terrain
        .and_then(|t| Terrain::ALL.iter().position(|a| *a == t))
        .map_or(u8::MAX, |i| u8::try_from(i).expect("six terrains"))
}

/// The terrain under the points of `world`'s picture kept under `dir` by
/// this `program`, or `None` when nothing is kept or the file is damaged.
#[must_use]
pub fn read_picture(dir: &Path, world: &str, program: u64) -> Option<Vec<Option<Terrain>>> {
    let bytes = std::fs::read(picture_path_of(dir, world, program)).ok()?;
    let (body, sum) = bytes.split_at_checked(bytes.len().checked_sub(8)?)?;
    if fingerprint(body) != u64::from_le_bytes(sum.try_into().ok()?) {
        return None;
    }
    let codes = body.strip_prefix(PICTURE_MAGIC)?;
    codes
        .iter()
        .map(|&c| match c {
            u8::MAX => Some(None),
            c => Terrain::ALL.get(usize::from(c)).copied().map(Some),
        })
        .collect()
}

/// Keeps the terrain under the points of `world`'s picture under `dir`
/// for this `program`, written whole then moved over.
///
/// # Errors
///
/// The file system's, when the directory cannot be made or the file
/// written or moved.
pub fn keep_picture(
    dir: &Path,
    world: &str,
    program: u64,
    terrain: &[Option<Terrain>],
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut bytes = PICTURE_MAGIC.to_vec();
    bytes.extend(terrain.iter().map(|&t| code_of(t)));
    let sum = fingerprint(&bytes);
    bytes.extend_from_slice(&sum.to_le_bytes());
    let path = picture_path_of(dir, world, program);
    let part = path.with_extension(format!("part{}", std::process::id()));
    std::fs::write(&part, bytes)?;
    std::fs::rename(&part, &path)?;
    clear_stale(dir, world, program);
    Ok(())
}

/// Removes the files of `world` (lands, pictures, parts left by a run
/// stopped midway) untouched for [`STALE_DAYS`], but the kept files of
/// this `program`.
fn clear_stale(dir: &Path, world: &str, program: u64) {
    let ours = format!("-{program:016x}.bin");
    let prefixes = [format!("land-{world}-g"), format!("picture-{world}-g")];
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let stale = std::time::Duration::from_secs(STALE_DAYS * 24 * 3600);
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let other = name
            .to_str()
            .is_some_and(|n| prefixes.iter().any(|p| n.starts_with(p)) && !n.ends_with(&ours));
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > stale);
        if other && old {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("soul-vivarium-lands-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_kept_land_is_read_back_as_made_and_a_damaged_one_is_refused() {
        let dir = scratch("round-trip");
        let map = Map::valley(160, 128, 5);
        let terrain = |m: &Map| {
            (0..128)
                .flat_map(|y| (0..160).map(move |x| sim::map::Pos::new(x, y)))
                .map(|p| (m.get(p), m.trees(p), m.habitat(p), m.depth(p)))
                .collect::<Vec<_>>()
        };
        let made = terrain(&map);
        assert!(
            read_into(&map, &dir, "test", 1).is_none(),
            "nothing kept yet"
        );
        keep(&dir, "test", 1, &map).unwrap();
        // Another map of the same spec takes every chunk and makes none.
        let again = Map::valley(160, 128, 5);
        let taken = read_into(&again, &dir, "test", 1).expect("kept");
        assert_eq!(taken[0], 20, "the 5 × 4 chunks of tiles");
        assert_eq!(terrain(&again), made);
        assert_eq!(again.land_made(), [0, 0, 0], "nothing made again");
        // Another program's or world's file is not this one.
        assert!(read_into(&again, &dir, "test", 2).is_none());
        assert!(read_into(&again, &dir, "other", 1).is_none());
        // A damaged file: refused, nothing taken.
        let mut bytes = std::fs::read(path_of(&dir, "test", 1)).unwrap();
        bytes[100] ^= 0xff;
        std::fs::write(path_of(&dir, "test", 1), bytes).unwrap();
        let fresh = Map::valley(160, 128, 5);
        assert!(read_into(&fresh, &dir, "test", 1).is_none());
        assert_eq!(terrain(&fresh), made);
        // The picture's terrain: read back as kept, refused when damaged.
        let points = vec![
            Some(Terrain::Grass),
            None,
            Some(Terrain::Rock),
            Some(Terrain::Ford),
        ];
        assert!(read_picture(&dir, "test", 1).is_none());
        keep_picture(&dir, "test", 1, &points).unwrap();
        assert_eq!(read_picture(&dir, "test", 1), Some(points));
        assert!(read_picture(&dir, "test", 2).is_none());
        let mut bytes = std::fs::read(picture_path_of(&dir, "test", 1)).unwrap();
        bytes[9] ^= 0x01;
        std::fs::write(picture_path_of(&dir, "test", 1), bytes).unwrap();
        assert!(read_picture(&dir, "test", 1).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_default_dir_sits_beside_the_continents() {
        assert!(default_dir().ends_with("soul-sim/lands"));
    }
}
