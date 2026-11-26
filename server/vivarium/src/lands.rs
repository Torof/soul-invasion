//! The vivarium's land kept on disk between runs (D51, amended 25
//! November 2026): a cache of this tool, nothing of the game's (the
//! game's storage is untouched, D28). A chunk's tiles, woods and water
//! are a pure function of the world (its zone, its seed, the generator),
//! never of a run (D24), so they are written once ([`sim::map::Map::land_bytes`])
//! and read back into each run's map before its first tick
//! ([`sim::map::Map::take_land_bytes`]), which then reads them as made:
//! the same pages, with and without. The trees and the plants bearing
//! food are kept the same way ([`ground_into`]): a pure function of the
//! world and of the content's kinds, their file also carries the
//! content's fingerprint ([`sim::ahead::Placing::fingerprint`]).
//!
//! **Prepared ahead, shared by every build of the same land-making code**
//! (the project lead, 25 November 2026: "charger à l'avant"; a file
//! written by another build may be read, a check guarding it). The files
//! are named by the world, the generator's version ([`VIVARIUM_GENERATOR`])
//! and a fingerprint of the code that makes the land ([`land_code`]: the
//! sources of `sim` that make tiles, woods, water and trees, hashed) or
//! the trees and plants ([`ground_code`]), no longer of the whole
//! program: a change of the brain, which every pull request makes, keeps
//! the files. A file is checked when read: [`CHECK_CHUNKS`] of its chunks,
//! drawn anew at each run, are made by this build and every chunk held is
//! compared with the file's; one that differs refuses the whole file (a
//! change of the land's code outside the files hashed), which is then made
//! again. When a run's world has no whole file yet (on maps of at most
//! [`PREPARE_MAX_CHUNKS`] chunks), the run prepares it before its first
//! tick: every chunk of the map made on all the machine's cores, then
//! written, under a lock so that the runs of a series started together
//! prepare it once and the others read it. `vivarium --prepare-land`
//! does the same alone, before a series. Under
//! `${XDG_CACHE_HOME:-~/.cache}/soul-sim/lands/`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use sim::ahead::Placing;
use sim::land::{Refused, Taken};
use sim::map::{ChunkPos, Map, Terrain, VIVARIUM_GENERATOR};

/// Days a kept land of another code is left before it is cleared, when
/// a run keeps its own: long enough for the builds of several worktrees
/// run side by side to keep theirs.
pub const STALE_DAYS: u64 = 3;

/// Chunks of a kept file made again by the run and compared, besides
/// those it already holds: well under a second (the land's woods read the
/// tiles of the 25 chunks around, a chunk's plants the trees of the nine).
pub const CHECK_CHUNKS: usize = 30;

/// The largest map a run prepares whole when no whole file is kept: the
/// vivarium's wide world (18,496 chunks, some 57 MB of land and 39 MB of
/// trees and plants). The huge world's, some 800 MB, is kept as the runs
/// make it.
pub const PREPARE_MAX_CHUNKS: usize = 20_000;

/// The sources of `sim` that make a chunk's tiles, woods and water, the
/// continent under them, and the trees (which the continent's net reads):
/// what the land's file is named by. The test
/// `every_source_of_sim_is_sorted` asks a new file of `sim` to be sorted
/// here, in [`GROUND_SOURCES`] or in `NOT_LAND_SOURCES`.
const LAND_SOURCES: &[(&str, &[u8])] = &[
    ("map.rs", include_bytes!("../../sim/src/map.rs")),
    ("land.rs", include_bytes!("../../sim/src/land.rs")),
    ("gen.rs", include_bytes!("../../sim/src/gen.rs")),
    (
        "gen/scatter.rs",
        include_bytes!("../../sim/src/gen/scatter.rs"),
    ),
    ("woods.rs", include_bytes!("../../sim/src/woods.rs")),
    ("relief.rs", include_bytes!("../../sim/src/relief.rs")),
    ("geology.rs", include_bytes!("../../sim/src/geology.rs")),
    ("trees.rs", include_bytes!("../../sim/src/trees.rs")),
    (
        "open_ground.rs",
        include_bytes!("../../sim/src/open_ground.rs"),
    ),
    (
        "plan/climate.rs",
        include_bytes!("../../sim/src/plan/climate.rs"),
    ),
    (
        "plan/course.rs",
        include_bytes!("../../sim/src/plan/course.rs"),
    ),
    ("plan/kept.rs", include_bytes!("../../sim/src/plan/kept.rs")),
    (
        "plan/measure.rs",
        include_bytes!("../../sim/src/plan/measure.rs"),
    ),
    ("plan/mod.rs", include_bytes!("../../sim/src/plan/mod.rs")),
    ("plan/net.rs", include_bytes!("../../sim/src/plan/net.rs")),
    (
        "plan/region.rs",
        include_bytes!("../../sim/src/plan/region.rs"),
    ),
    (
        "plan/relief.rs",
        include_bytes!("../../sim/src/plan/relief.rs"),
    ),
    (
        "plan/vivarium.rs",
        include_bytes!("../../sim/src/plan/vivarium.rs"),
    ),
    (
        "plan/window.rs",
        include_bytes!("../../sim/src/plan/window.rs"),
    ),
];

/// The sources that place the trees and the plants bearing food over the
/// land: with [`LAND_SOURCES`], what the trees and plants' file is named
/// by.
const GROUND_SOURCES: &[(&str, &[u8])] = &[
    ("flora.rs", include_bytes!("../../sim/src/flora.rs")),
    ("soil.rs", include_bytes!("../../sim/src/soil.rs")),
    ("ground.rs", include_bytes!("../../sim/src/ground.rs")),
    ("ahead.rs", include_bytes!("../../sim/src/ahead.rs")),
];

/// The sources of `sim` that make neither the land nor its trees and
/// plants: changed by most pull requests, they leave the files kept. What
/// they lend the placing (a kind's name, a bearer's key in `crops.rs`,
/// `MOST_SPREAD_M` in `content.rs`, folded into [`ground_code`]) is
/// guarded by the check of the chunks.
#[cfg(test)]
const NOT_LAND_SOURCES: &[&str] = &[
    "lib.rs",
    "brain.rs",
    "changes.rs",
    "content.rs",
    "crops.rs",
    "hands.rs",
    "invariants.rs",
    "items.rs",
    "life.rs",
    "lru.rs",
    "memory.rs",
    "time.rs",
];

/// The sources of `sim` under these folders make neither: the brain.
#[cfg(test)]
const NOT_LAND_FOLDERS: &[&str] = &["brain"];

/// Where the lands are kept unless told otherwise:
/// `$XDG_CACHE_HOME/soul-sim/lands`, or `~/.cache/soul-sim/lands`.
#[must_use]
pub fn default_dir() -> PathBuf {
    crate::continents::default_dir().with_file_name("lands")
}

/// FNV-1a over `bytes`, eight bytes at a time, from `h`.
fn fnv_from(mut h: u64, bytes: &[u8]) -> u64 {
    let (words, rest) = bytes.as_chunks::<8>();
    for &word in words {
        h = (h ^ u64::from_le_bytes(word)).wrapping_mul(0x0100_0000_01b3);
    }
    for &b in rest {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// FNV-1a over `bytes`.
fn fingerprint(bytes: &[u8]) -> u64 {
    fnv_from(0xcbf2_9ce4_8422_2325, bytes)
}

/// `sources` hashed, each by its name, its length and its bytes.
fn hash_sources(mut h: u64, sources: &[(&str, &[u8])]) -> u64 {
    for (name, bytes) in sources {
        h = fnv_from(h, name.as_bytes());
        h = fnv_from(h, &(bytes.len() as u64).to_le_bytes());
        h = fnv_from(h, bytes);
    }
    h
}

/// The fingerprint of the code that makes the land (`LAND_SOURCES`):
/// what the land's and the picture's files are named by.
#[must_use]
pub fn land_code() -> u64 {
    static CODE: OnceLock<u64> = OnceLock::new();
    *CODE.get_or_init(|| hash_sources(fingerprint(b"land"), LAND_SOURCES))
}

/// The fingerprint of the code that places the trees and the plants
/// bearing food (`LAND_SOURCES`, `GROUND_SOURCES`, the widest spread
/// of a tree the placing reads): what their file is named by.
#[must_use]
pub fn ground_code() -> u64 {
    static CODE: OnceLock<u64> = OnceLock::new();
    *CODE.get_or_init(|| {
        let h = hash_sources(land_code(), GROUND_SOURCES);
        fnv_from(h, &sim::content::MOST_SPREAD_M.to_le_bytes())
    })
}

/// A salt for the chunks a check draws: another at each run, so a series
/// checks other chunks run after run. Which ones changes nothing but the
/// time (D24): every chunk held is compared.
fn salt() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::from(d.subsec_nanos()) ^ d.as_secs());
    nanos ^ (u64::from(std::process::id()) << 32)
}

/// The file of the land of `world` (`vivarium`, `vivarium-wide`, ...)
/// made by the land-making code of fingerprint `code`, under `dir`.
#[must_use]
pub fn path_of(dir: &Path, world: &str, code: u64) -> PathBuf {
    dir.join(format!(
        "land-{world}-g{VIVARIUM_GENERATOR}-{code:016x}.bin"
    ))
}

/// The file of the trees and bearers of `world` placed with the content
/// of fingerprint `placing` ([`sim::ahead::Placing::fingerprint`]) by the
/// code of fingerprint `code` ([`ground_code`]), under `dir`: the trees
/// and plants read the content, so each content has its own.
#[must_use]
pub fn ground_path_of(dir: &Path, world: &str, code: u64, placing: u64) -> PathBuf {
    dir.join(format!(
        "ground-{world}-g{VIVARIUM_GENERATOR}-{placing:016x}-{code:016x}.bin"
    ))
}

/// The file of the terrain under the points of `world`'s picture, of
/// the land-making code of fingerprint `code`, under `dir`.
#[must_use]
pub fn picture_path_of(dir: &Path, world: &str, code: u64) -> PathBuf {
    dir.join(format!(
        "picture-{world}-g{VIVARIUM_GENERATOR}-{code:016x}.bin"
    ))
}

/// What a run's map or ground got from the files kept: timing, never
/// simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Got<const N: usize> {
    /// Chunks taken from the file, of each kind (tiles, woods, water; or
    /// trees, bearers).
    pub taken: [usize; N],
    /// Chunks held before the file was taken, compared with it and found
    /// the same (those the check made, and their neighbours).
    pub compared: usize,
    /// The file was refused: damaged, or of another code (a chunk made
    /// here differed). It is made again.
    pub refused: bool,
    /// The run prepared the whole map itself (no whole file was kept):
    /// how long it took on the wall, in seconds.
    pub prepared_s: Option<f64>,
    /// The whole map is held from a file or prepared: the run keeps
    /// nothing more at its end.
    pub whole: bool,
}

impl<const N: usize> Default for Got<N> {
    fn default() -> Self {
        Self {
            taken: [0; N],
            compared: 0,
            refused: false,
            prepared_s: None,
            whole: false,
        }
    }
}

impl<const N: usize> Got<N> {
    /// Adds what `taken` gave.
    fn add(&mut self, taken: &Taken<N>) {
        for (sum, n) in self.taken.iter_mut().zip(taken.taken) {
            *sum += n;
        }
        self.compared += taken.compared;
    }
}

/// The chunks of `map`, row by row.
fn chunks_of(map: &Map) -> Vec<ChunkPos> {
    let (wide, high) = (
        i32::try_from(map.chunks_wide()).expect("few chunks"),
        i32::try_from(map.chunks_high()).expect("few chunks"),
    );
    (0..high)
        .flat_map(|y| (0..wide).map(move |x| ChunkPos::new(x, y)))
        .collect()
}

/// Does `map` hold every chunk of its land (tiles and woods)?
fn land_whole(map: &Map) -> bool {
    let n = chunks_of(map).len();
    let held = map.land_held();
    held[0] >= n && held[1] >= n
}

/// `work` of each chunk of `chunks`, on `threads` threads taking them in
/// turn: what preparing does, a pass at a time.
fn on_threads(chunks: &[ChunkPos], threads: usize, work: &(dyn Fn(ChunkPos) + Sync)) {
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads.max(1) {
            scope.spawn(|| {
                while let Some(&chunk) = chunks.get(next.fetch_add(1, Ordering::Relaxed)) {
                    work(chunk);
                }
            });
        }
    });
}

/// The threads a preparing uses: all the machine's.
#[must_use]
pub fn prepare_threads() -> usize {
    std::thread::available_parallelism().map_or(4, std::num::NonZero::get)
}

/// Makes every chunk of `map`'s land not held yet, on `threads` threads:
/// the tiles first, then the woods and the water (which read the tiles of
/// the chunks around), so no two threads make the same chunk.
pub fn prepare_land(map: &Map, threads: usize) {
    let chunks = chunks_of(map);
    on_threads(&chunks, threads, &|chunk| {
        let _ = map.chunk(chunk);
    });
    on_threads(&chunks, threads, &|chunk| map.make_land(chunk));
}

/// Places the trees then the bearers of every chunk of `placing`'s map
/// not held yet, on `threads` threads.
pub fn prepare_ground(placing: &Placing, threads: usize) {
    let chunks = chunks_of(placing.map());
    on_threads(&chunks, threads, &|chunk| placing.place_trees(chunk));
    on_threads(&chunks, threads, &|chunk| {
        let _ = placing.place_bearers(chunk);
    });
}

/// Holds the lock of `name` under `dir` until dropped: the runs of a
/// series started together wait for the one preparing their land.
fn lock(dir: &Path, name: &str) -> std::io::Result<std::fs::File> {
    std::fs::create_dir_all(dir)?;
    let file = std::fs::File::create(dir.join(format!("{name}.lock")))?;
    file.lock()?;
    Ok(file)
}

/// Writes `bytes` to `path`: whole beside it, then moved over, so a run
/// stopped midway, or one reading meanwhile, never sees half a file.
fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let part = path.with_extension(format!("part{}", std::process::id()));
    std::fs::write(&part, bytes)?;
    std::fs::rename(&part, path)
}

/// Says on stderr why a kept file was refused.
fn note_refused(what: &str, refused: Refused) {
    match refused {
        Refused::Unreadable => {
            eprintln!("note: the kept {what} is damaged; it is made again");
        }
        Refused::Differs(chunk) => eprintln!(
            "note: the kept {what} differs from this build's at chunk ({}, {}): made by another code, it is made again (is a source that makes it missing from vivarium/src/lands.rs?)",
            chunk.x, chunk.y
        ),
    }
}

/// Gives `map` the land kept under `dir` for `world`, checked (see the
/// module's doc), into `got`.
fn take_land(map: &Map, dir: &Path, world: &str, got: &mut Got<3>) {
    let Ok(bytes) = std::fs::read(path_of(dir, world, land_code())) else {
        return;
    };
    match map.take_land_bytes(&bytes, CHECK_CHUNKS, salt()) {
        Ok(taken) => {
            got.add(&taken);
            got.whole = land_whole(map);
        }
        Err(refused) => {
            got.refused = true;
            note_refused("land", refused);
        }
    }
}

/// Gives `map` the land kept under `dir` for `world`, checked. With
/// `prepare` threads (on a map of at most [`PREPARE_MAX_CHUNKS`] chunks),
/// when no whole file is kept, prepares the whole land and keeps it,
/// under a lock: another run preparing it meanwhile is waited for, and
/// its file read.
pub fn land_into(map: &Map, dir: &Path, world: &str, prepare: Option<usize>) -> Got<3> {
    let mut got = Got::default();
    take_land(map, dir, world, &mut got);
    let Some(threads) = prepare.filter(|_| chunks_of(map).len() <= PREPARE_MAX_CHUNKS) else {
        return got;
    };
    if got.whole {
        return got;
    }
    let _lock = match lock(dir, &format!("land-{world}")) {
        Ok(lock) => lock,
        Err(err) => {
            eprintln!(
                "note: the land could not be locked under {}: {err}",
                dir.display()
            );
            return got;
        }
    };
    // Another run may have kept it while this one waited.
    if !got.refused {
        take_land(map, dir, world, &mut got);
        if got.whole {
            return got;
        }
    }
    let started = Instant::now();
    prepare_land(map, threads);
    got.prepared_s = Some(started.elapsed().as_secs_f64());
    if let Some(bytes) = map.land_bytes()
        && let Err(err) = write_whole(&path_of(dir, world, land_code()), &bytes)
    {
        eprintln!(
            "note: the land could not be kept under {}: {err}",
            dir.display()
        );
    }
    got.whole = true;
    got
}

/// Does `ground` hold the bearers of every chunk of `placing`'s map?
fn ground_whole(placing: &Placing, ground: &sim::ground::Ground) -> bool {
    ground.ground_held()[1] >= chunks_of(placing.map()).len()
}

/// Gives the world of `placing` the trees and bearers kept under `dir`
/// for `world`, checked, into `got`.
fn take_ground(
    placing: &Placing,
    ground: &sim::ground::Ground,
    dir: &Path,
    world: &str,
    got: &mut Got<2>,
) {
    let path = ground_path_of(dir, world, ground_code(), placing.fingerprint());
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    match placing.take_ground_bytes(&bytes, CHECK_CHUNKS, salt()) {
        Ok(taken) => {
            got.add(&taken);
            got.whole = ground_whole(placing, ground);
        }
        Err(refused) => {
            got.refused = true;
            note_refused("trees and plants", refused);
        }
    }
}

/// [`land_into`] for the trees and the plants bearing food of the world
/// of `placing`, whose ground is `ground` (widened to the whole map by
/// the caller).
pub fn ground_into(
    placing: &Placing,
    ground: &sim::ground::Ground,
    dir: &Path,
    world: &str,
    prepare: Option<usize>,
) -> Got<2> {
    let mut got = Got::default();
    take_ground(placing, ground, dir, world, &mut got);
    let n = chunks_of(placing.map()).len();
    let Some(threads) = prepare.filter(|_| n <= PREPARE_MAX_CHUNKS) else {
        return got;
    };
    if got.whole {
        return got;
    }
    let name = format!("ground-{world}-{:016x}", placing.fingerprint());
    let _lock = match lock(dir, &name) {
        Ok(lock) => lock,
        Err(err) => {
            eprintln!(
                "note: the trees and plants could not be locked under {}: {err}",
                dir.display()
            );
            return got;
        }
    };
    if !got.refused {
        take_ground(placing, ground, dir, world, &mut got);
        if got.whole {
            return got;
        }
    }
    let started = Instant::now();
    prepare_ground(placing, threads);
    got.prepared_s = Some(started.elapsed().as_secs_f64());
    let path = ground_path_of(dir, world, ground_code(), placing.fingerprint());
    if let Err(err) = write_whole(&path, &ground.to_bytes()) {
        eprintln!(
            "note: the trees and plants could not be kept under {}: {err}",
            dir.display()
        );
    }
    got.whole = true;
    got
}

/// Keeps `map`'s land under `dir` for `world`, with whatever another run
/// kept there meanwhile (taken when found the same; written over when
/// not): written whole beside it, then moved over. Files of other codes
/// of the same world untouched for [`STALE_DAYS`] are cleared.
///
/// # Errors
///
/// The file system's, when the directory cannot be made or the file
/// written or moved.
pub fn keep(dir: &Path, world: &str, map: &Map) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = path_of(dir, world, land_code());
    if let Ok(bytes) = std::fs::read(&path) {
        let _ = map.take_land_bytes(&bytes, 0, 0);
    }
    let Some(bytes) = map.land_bytes() else {
        return Ok(());
    };
    write_whole(&path, &bytes)?;
    clear_stale(dir, world);
    Ok(())
}

/// Keeps the trees and bearers of the world of `placing` under `dir` for
/// `world`, with what another run kept there meanwhile; written whole,
/// then moved over.
///
/// # Errors
///
/// The file system's, when the directory cannot be made or the file
/// written or moved.
pub fn keep_ground(
    dir: &Path,
    world: &str,
    placing: &Placing,
    ground: &sim::ground::Ground,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = ground_path_of(dir, world, ground_code(), placing.fingerprint());
    if let Ok(bytes) = std::fs::read(&path) {
        let _ = placing.take_ground_bytes(&bytes, 0, 0);
    }
    write_whole(&path, &ground.to_bytes())?;
    clear_stale(dir, world);
    Ok(())
}

/// The first bytes of a kept picture's terrain, with its layout's version.
const PICTURE_MAGIC: &[u8; 8] = b"SOULPIC1";

/// A terrain's code in a kept picture: its place in [`Terrain::ALL`],
/// `u8::MAX` off the map.
fn code_of(terrain: Option<Terrain>) -> u8 {
    terrain
        .and_then(|t| Terrain::ALL.iter().position(|a| *a == t))
        .map_or(u8::MAX, |i| u8::try_from(i).expect("six terrains"))
}

/// The terrain under the points of `world`'s picture kept under `dir`,
/// or `None` when nothing is kept or the file is damaged.
#[must_use]
pub fn read_picture(dir: &Path, world: &str) -> Option<Vec<Option<Terrain>>> {
    let bytes = std::fs::read(picture_path_of(dir, world, land_code())).ok()?;
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

/// Keeps the terrain under the points of `world`'s picture under `dir`,
/// written whole then moved over.
///
/// # Errors
///
/// The file system's, when the directory cannot be made or the file
/// written or moved.
pub fn keep_picture(dir: &Path, world: &str, terrain: &[Option<Terrain>]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut bytes = PICTURE_MAGIC.to_vec();
    bytes.extend(terrain.iter().map(|&t| code_of(t)));
    let sum = fingerprint(&bytes);
    bytes.extend_from_slice(&sum.to_le_bytes());
    write_whole(&picture_path_of(dir, world, land_code()), &bytes)?;
    clear_stale(dir, world);
    Ok(())
}

/// Removes the files of `world` (lands, pictures, grounds, parts left by a
/// run stopped midway) untouched for [`STALE_DAYS`], but those of this
/// build's codes.
fn clear_stale(dir: &Path, world: &str) {
    let ours = [
        format!("-{:016x}.bin", land_code()),
        format!("-{:016x}.bin", ground_code()),
    ];
    let prefixes = [
        format!("land-{world}-g"),
        format!("picture-{world}-g"),
        format!("ground-{world}-g"),
    ];
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let stale = std::time::Duration::from_secs(STALE_DAYS * 24 * 3600);
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let other = name.to_str().is_some_and(|n| {
            prefixes.iter().any(|p| n.starts_with(p)) && !ours.iter().any(|o| n.ends_with(o))
        });
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

    type Tile = (Option<Terrain>, u8, sim::woods::Habitat, u8);

    fn terrain(m: &Map) -> Vec<Tile> {
        (0..128)
            .flat_map(|y| (0..160).map(move |x| sim::map::Pos::new(x, y)))
            .map(|p| (m.get(p), m.trees(p), m.habitat(p), m.depth(p)))
            .collect()
    }

    #[test]
    fn a_kept_land_is_read_back_as_made_and_a_damaged_one_is_refused() {
        let dir = scratch("round-trip");
        let map = Map::valley(160, 128, 5);
        let made = terrain(&map);
        let none = land_into(&map, &dir, "test", None);
        assert_eq!(none.taken, [0; 3], "nothing kept yet");
        assert!(!none.refused);
        keep(&dir, "test", &map).unwrap();
        // Another map of the same spec takes every chunk; it made only
        // those the check drew, with the tiles their woods read around.
        let again = Map::valley(160, 128, 5);
        let got = land_into(&again, &dir, "test", None);
        assert!(got.whole, "the 5 × 4 chunks: {got:?}");
        assert_eq!(
            got.taken[0] + usize::try_from(again.land_made()[0]).unwrap(),
            20
        );
        assert!(got.compared >= 20, "{got:?}");
        assert_eq!(terrain(&again), made);
        // Another world's file is not this one.
        let other = Map::valley(160, 128, 5);
        assert_eq!(land_into(&other, &dir, "other", None).taken, [0; 3]);
        // A damaged file: refused, nothing taken, and made again whole
        // when the run prepares.
        let path = path_of(&dir, "test", land_code());
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[100] ^= 0xff;
        std::fs::write(&path, bytes).unwrap();
        let fresh = Map::valley(160, 128, 5);
        let got = land_into(&fresh, &dir, "test", Some(2));
        assert!(
            got.refused && got.whole && got.prepared_s.is_some(),
            "{got:?}"
        );
        assert_eq!(got.taken, [0; 3]);
        assert_eq!(terrain(&fresh), made);
        let next = Map::valley(160, 128, 5);
        let got = land_into(&next, &dir, "test", Some(2));
        assert!(
            !got.refused && got.whole && got.prepared_s.is_none(),
            "{got:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_prepared_land_is_the_land_made_on_one_thread() {
        let dir = scratch("prepared");
        let alone = Map::valley(160, 128, 9);
        let made = terrain(&alone);
        let map = Map::valley(160, 128, 9);
        let got = land_into(&map, &dir, "test", Some(4));
        assert!(got.whole && got.prepared_s.is_some());
        assert_eq!(map.land_made()[..2], [20, 20], "each chunk made once");
        assert_eq!(terrain(&map), made);
        assert!(path_of(&dir, "test", land_code()).is_file(), "kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_kept_picture_is_read_back_and_a_damaged_one_refused() {
        let dir = scratch("picture");
        let points = vec![
            Some(Terrain::Grass),
            None,
            Some(Terrain::Rock),
            Some(Terrain::Ford),
        ];
        assert!(read_picture(&dir, "test").is_none());
        keep_picture(&dir, "test", &points).unwrap();
        assert_eq!(read_picture(&dir, "test"), Some(points));
        assert!(read_picture(&dir, "other").is_none());
        let path = picture_path_of(&dir, "test", land_code());
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[9] ^= 0x01;
        std::fs::write(&path, bytes).unwrap();
        assert!(read_picture(&dir, "test").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_codes_name_the_land_and_the_ground_apart() {
        assert_ne!(land_code(), ground_code());
        assert_eq!(land_code(), land_code(), "the same every call");
        let sources: Vec<&str> = LAND_SOURCES
            .iter()
            .chain(GROUND_SOURCES)
            .map(|(name, _)| *name)
            .collect();
        let mut sorted = sources.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), sources.len(), "each source once");
    }

    /// A new source of `sim` is sorted: does it make the land, the trees
    /// and plants, or neither? Forgotten, a change in it could leave a
    /// kept file of another land read (the check of the chunks would
    /// likely, not surely, catch it).
    #[test]
    fn every_source_of_sim_is_sorted() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sim/src");
        let mut found = Vec::new();
        let mut folders = vec![root.clone()];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(&folder).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    folders.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let name = path.strip_prefix(&root).unwrap();
                    found.push(name.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        let sorted: Vec<&str> = LAND_SOURCES
            .iter()
            .chain(GROUND_SOURCES)
            .map(|(name, _)| *name)
            .chain(NOT_LAND_SOURCES.iter().copied())
            .collect();
        for name in &found {
            let in_folder = NOT_LAND_FOLDERS
                .iter()
                .any(|f| name.starts_with(&format!("{f}/")));
            assert!(
                in_folder || sorted.contains(&name.as_str()),
                "sim/src/{name}: sort it in vivarium/src/lands.rs (land, ground or neither)"
            );
        }
        for name in &sorted {
            assert!(found.iter().any(|f| f == name), "sim/src/{name} is gone");
        }
    }

    #[test]
    fn the_default_dir_sits_beside_the_continents() {
        assert!(default_dir().ends_with("soul-sim/lands"));
    }
}
