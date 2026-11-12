//! What the seed places on the ground, worked out once a chunk and held
//! (D37, D39): the trees, and the tiles their trunks block ([`Trunks`],
//! D32); the ground's fields the flora reads, next. Derived state: never stored,
//! placed again from the map and the seed, so forgetting any of it
//! changes nothing but the time the next request takes (a cache only
//! memoises).
//!
//! One [`Ground`] a world, behind an `Arc`, shared by the simulation
//! thread and the network's threads (and the pool that will build chunks
//! beside them): a chunk's trees are placed **once** for every reader,
//! where each reader used to place them again (a chunk's plants read the
//! trees of the nine chunks around it: a view of 361 chunks placed about
//! 3,600 chunks' trees). An entry is keyed by the chunk and its
//! **version**: how many times men changed it (an item taken, a tree
//! felled, one planted: [`crate::changes::ChunkChanges`], which the world
//! keeps and stores). Every chunk is at version 0 until men change it; a
//! change sets the new version here ([`Ground::set_version`]), so the
//! next request places the chunk again and, when felling comes, lays the
//! changes over it. The old entry ages out. Bounded ([`TREE_CACHE_CHUNKS`]), the least
//! recently used out first: the memory follows the chunks in use, never
//! the map's size nor the land visited.
//!
//! A `Ground` belongs to one world and one content: the trees it holds
//! were placed on that world's map with that content's kinds of tree.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::content::Species;
use crate::lru::Lru;
use crate::map::{CHUNK_SIZE, ChunkPos, Map, PATH_REACH, Pos};
use crate::trees::{self, Tree};

/// How many times a chunk was changed by men: 0 while it is as the seed
/// made it.
pub type Version = u32;

/// Chunks whose trees are held at most. A view reaches [`PATH_REACH`]
/// tiles (nine chunks) each way, 19 × 19 chunks, and its plants read the
/// trees one chunk beyond: 21 × 21 = 441 chunks of trees a view; four
/// views' worth, as the plants' own cache holds (the view around a lone
/// player and the land just left, or a view each for a few players
/// apart). A chunk holds about 19 trees of 16 bytes on the game's valley
/// (55 % forest, 10 November 2026), 130 at most, 24 bytes each with
/// their key: about 0.8 MB, under 6 MB were every chunk the densest.
pub const TREE_CACHE_CHUNKS: usize = {
    let reach = (PATH_REACH.unsigned_abs() / CHUNK_SIZE) as usize;
    let with_rim = 2 * reach + 1 + 2;
    4 * with_rim * with_rim
};

/// The placed trees of the chunks asked for lately, and each chunk's
/// version. See the module's doc.
#[derive(Debug)]
pub struct Ground {
    held: Mutex<Held>,
}

/// What the one lock guards: both are looked up together, so that a
/// version bumped between the two is seen as a whole.
#[derive(Debug)]
struct Held {
    /// The trees of a chunk at a version and the tiles their trunks
    /// block, the least recently used out first.
    trees: Lru<(ChunkPos, Version), Placed>,
    /// The chunks changed by men, with how many times: absent means 0.
    /// Grows with what men change, never with the land visited; the
    /// versions of the world's chunk changes, set as they change.
    versions: BTreeMap<ChunkPos, Version>,
}

/// A chunk's trees as placed, with the tiles their trunks block: made
/// together, held together, forgotten together.
#[derive(Debug, Clone)]
struct Placed {
    trees: Arc<[Tree]>,
    trunks: Arc<Trunks>,
}

impl Placed {
    fn new(chunk: ChunkPos, trees: Vec<Tree>) -> Self {
        let trunks = Arc::new(Trunks::of(chunk, &trees));
        Self {
            trees: trees.into(),
            trunks,
        }
    }
}

/// The tiles of a chunk an adult tree's trunk stands in, a bit a tile:
/// 32 rows of 32 bits, 128 bytes (D32, lightness plan F2). A body never
/// walks into such a tile: path searches go round it, a pulled segment
/// never crosses it, a straight walk steps round it. Derived from the
/// chunk's trees, never stored. Every placed tree is adult today; when
/// trees grow in stages, a seedling's or a young tree's does not block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trunks {
    rows: [u32; CHUNK_SIZE as usize],
}

impl Trunks {
    /// The tiles blocked by the trunks of `trees`, those of them standing
    /// in `chunk`.
    #[must_use]
    pub fn of(chunk: ChunkPos, trees: &[Tree]) -> Self {
        let mut rows = [0; CHUNK_SIZE as usize];
        let origin = chunk.origin();
        for tree in trees {
            let tile = tree.at.tile();
            if tile.chunk() != chunk {
                continue;
            }
            let (x, y) = (tile.x - origin.x, tile.y - origin.y);
            rows[usize::try_from(y).expect("in its chunk")] |= 1 << x;
        }
        Self { rows }
    }

    /// Does a trunk stand in `pos`, a tile of this bitmap's chunk?
    #[must_use]
    pub fn blocks(&self, pos: Pos) -> bool {
        let side = i32::try_from(CHUNK_SIZE).expect("small chunk side");
        let (x, y) = (pos.x.rem_euclid(side), pos.y.rem_euclid(side));
        self.rows[usize::try_from(y).expect("non-negative")] >> x & 1 == 1
    }

    /// How many tiles are blocked.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.rows.iter().map(|r| r.count_ones()).sum()
    }
}

/// The trunks of the chunks one walk is planned over, looked up tile by
/// tile ([`Ground::trunks_at`]): each chunk's bitmap is taken from the
/// [`Ground`] once and kept for the lookup's life, which is one plan, the
/// world unchanged meanwhile.
pub struct TrunkLookup<'a> {
    ground: &'a Ground,
    map: &'a Map,
    species: &'a [Species],
    seed: u64,
    seen: RefCell<BTreeMap<ChunkPos, Arc<Trunks>>>,
}

impl TrunkLookup<'_> {
    /// Does an adult tree's trunk stand in `pos`? Never outside the map.
    #[must_use]
    pub fn blocks(&self, pos: Pos) -> bool {
        let chunk = pos.chunk();
        if !self.map.has_chunk(chunk) {
            return false;
        }
        if let Some(trunks) = self.seen.borrow().get(&chunk) {
            return trunks.blocks(pos);
        }
        let trunks = self
            .ground
            .trunks_in(self.map, self.species, self.seed, chunk);
        let blocks = trunks.blocks(pos);
        self.seen.borrow_mut().insert(chunk, trunks);
        blocks
    }
}

impl Default for Ground {
    fn default() -> Self {
        Self::new(TREE_CACHE_CHUNKS)
    }
}

impl Ground {
    /// A ground holding the trees of at most `capacity` chunks.
    ///
    /// # Panics
    ///
    /// If `capacity` is 0.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            held: Mutex::new(Held {
                trees: Lru::new(capacity),
                versions: BTreeMap::new(),
            }),
        }
    }

    /// A chunk's version: 0 while it is as the seed made it.
    #[must_use]
    pub fn version(&self, chunk: ChunkPos) -> Version {
        self.lock().versions.get(&chunk).copied().unwrap_or(0)
    }

    /// Notes that men changed `chunk` (an item taken, a tree felled, one
    /// planted), now at `version`: its trees are placed again, with the
    /// changes laid over them, the next time they are asked for.
    pub fn set_version(&self, chunk: ChunkPos, version: Version) {
        self.lock().versions.insert(chunk, version);
    }

    /// The trees standing in `chunk` at its current version: those held,
    /// or else placed now ([`trees::trees_in`]) and held. The same trees
    /// either way; a hit copies nothing. Placed with the lock released:
    /// two readers missing the same chunk at once place the same trees,
    /// and the second only replaces the first.
    #[must_use]
    pub fn trees_in(
        &self,
        map: &Map,
        species: &[Species],
        seed: u64,
        chunk: ChunkPos,
    ) -> Arc<[Tree]> {
        self.placed(map, species, seed, chunk).trees
    }

    /// The tiles blocked by the trunks of `chunk`'s trees at its current
    /// version ([`Trunks`]): held with the trees, made when they are
    /// placed, and placed again with them.
    #[must_use]
    pub fn trunks_in(
        &self,
        map: &Map,
        species: &[Species],
        seed: u64,
        chunk: ChunkPos,
    ) -> Arc<Trunks> {
        self.placed(map, species, seed, chunk).trunks
    }

    /// A lookup of the trunks tile by tile, for one walk's plan.
    #[must_use]
    pub fn trunks_at<'a>(
        &'a self,
        map: &'a Map,
        species: &'a [Species],
        seed: u64,
    ) -> TrunkLookup<'a> {
        TrunkLookup {
            ground: self,
            map,
            species,
            seed,
            seen: RefCell::new(BTreeMap::new()),
        }
    }

    /// `chunk`'s trees and trunks at its current version: held, or else
    /// placed now and held.
    fn placed(&self, map: &Map, species: &[Species], seed: u64, chunk: ChunkPos) -> Placed {
        let (key, held) = {
            let mut held = self.lock();
            let key = (chunk, held.versions.get(&chunk).copied().unwrap_or(0));
            (key, held.trees.get(&key).cloned())
        };
        if let Some(placed) = held {
            return placed;
        }
        let placed = Placed::new(chunk, trees::trees_in(map, species, seed, chunk));
        self.lock().trees.insert(key, placed.clone());
        placed
    }

    /// Are `chunk`'s trees held, at its current version? Does not count
    /// as a use.
    #[must_use]
    pub fn holds_trees(&self, chunk: ChunkPos) -> bool {
        let held = self.lock();
        let version = held.versions.get(&chunk).copied().unwrap_or(0);
        held.trees.contains(&(chunk, version))
    }

    /// How many chunks' trees are held, stale versions included.
    #[must_use]
    pub fn trees_held(&self) -> usize {
        self.lock().trees.len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Held> {
        // A reader that panicked while holding the lock left nothing half
        // written: every change here is one insert.
        self.held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Terrain;

    /// The kinds of tree of the repository's `content/species.ron`.
    const SPECIES: &str = include_str!("../../../content/species.ron");

    /// A small valley with the repository's kinds of tree.
    fn valley() -> (Map, Vec<Species>) {
        let species: Vec<Species> = ron::from_str(SPECIES).expect("species.ron parses");
        assert!(!species.is_empty(), "the content has trees");
        (Map::valley(256, 256, 3), species)
    }

    /// The chunks of a small valley that hold trees, a dozen of them.
    fn wooded(map: &Map, species: &[Species]) -> Vec<ChunkPos> {
        let side = i32::try_from(map.width() / CHUNK_SIZE).expect("small");
        let chunks: Vec<ChunkPos> = (0..side)
            .flat_map(|y| (0..side).map(move |x| ChunkPos::new(x, y)))
            .filter(|&chunk| !trees::trees_in(map, species, SEED, chunk).is_empty())
            .take(12)
            .collect();
        assert_eq!(chunks.len(), 12, "a wooded valley");
        chunks
    }

    /// The valley's seed.
    const SEED: u64 = 3;

    #[test]
    fn a_hit_gives_the_same_trees_as_a_fresh_placing_and_copies_nothing() {
        let (map, species) = valley();
        let ground = Ground::default();
        let chunks = wooded(&map, &species);
        for &chunk in &chunks {
            let fresh = trees::trees_in(&map, &species, SEED, chunk);
            assert!(!ground.holds_trees(chunk));
            let placed = ground.trees_in(&map, &species, SEED, chunk);
            assert_eq!(&*placed, &fresh[..], "a miss places the trees");
            assert!(ground.holds_trees(chunk));
            let again = ground.trees_in(&map, &species, SEED, chunk);
            assert!(Arc::ptr_eq(&placed, &again), "a hit shares the same trees");
        }
        assert_eq!(ground.trees_held(), chunks.len());
    }

    #[test]
    fn the_bound_forgets_the_least_recently_asked_and_forgetting_changes_nothing() {
        let (map, species) = valley();
        let ground = Ground::new(4);
        let chunks = wooded(&map, &species);
        let first: Vec<Arc<[Tree]>> = chunks
            .iter()
            .map(|&chunk| {
                let trees = ground.trees_in(&map, &species, SEED, chunk);
                assert!(ground.trees_held() <= 4, "bounded");
                trees
            })
            .collect();
        assert!(!ground.holds_trees(chunks[0]), "the first was forgotten");
        assert!(ground.holds_trees(chunks[11]), "the last is held");
        // Asked again in the same order, each chunk has been forgotten
        // (each placing drops the next to be asked): placed again, the
        // same trees in another allocation. Asked once more at once, held.
        for (&chunk, trees) in chunks.iter().zip(&first) {
            assert!(!ground.holds_trees(chunk), "forgotten");
            let again = ground.trees_in(&map, &species, SEED, chunk);
            assert_eq!(again, *trees, "{chunk:?} placed again, the same");
            assert!(!Arc::ptr_eq(&again, trees), "placed anew");
            assert!(Arc::ptr_eq(
                &ground.trees_in(&map, &species, SEED, chunk),
                &again
            ));
        }
    }

    #[test]
    fn a_change_bumps_the_version_and_places_the_chunk_again() {
        let (map, species) = valley();
        let ground = Ground::default();
        let chunk = wooded(&map, &species)[0];
        assert_eq!(ground.version(chunk), 0);
        let before = ground.trees_in(&map, &species, SEED, chunk);
        ground.set_version(chunk, 1);
        assert_eq!(ground.version(chunk), 1);
        assert!(
            !ground.holds_trees(chunk),
            "the old version is not the chunk's"
        );
        let after = ground.trees_in(&map, &species, SEED, chunk);
        assert!(!Arc::ptr_eq(&before, &after), "placed again");
        assert_eq!(before, after, "nothing lays changes over them yet");
        assert_eq!(
            ground.trees_held(),
            2,
            "the old entry ages out, it is not dropped"
        );
        ground.set_version(chunk, 2);
        assert_eq!(ground.version(chunk), 2);
        // Another chunk keeps its own version.
        let other = ChunkPos::new(chunk.x + 1, chunk.y);
        assert_eq!(ground.version(other), 0);
    }

    #[test]
    fn the_bound_is_four_views_with_their_rim() {
        assert_eq!(TREE_CACHE_CHUNKS, 4 * 21 * 21);
        let empty = Map::filled(64, 64, Terrain::Grass);
        let ground = Ground::default();
        let (_, species) = valley();
        let none = ground.trees_in(&empty, &species, SEED, ChunkPos::new(0, 0));
        assert!(none.is_empty(), "no woods, no trees");
        assert!(
            ground.holds_trees(ChunkPos::new(0, 0)),
            "an empty chunk is held too"
        );
    }
    #[test]
    fn the_trunks_are_the_tiles_of_the_trees_and_survive_eviction() {
        let (map, species) = valley();
        let ground = Ground::new(4);
        let chunks = wooded(&map, &species);
        let first: Vec<Arc<Trunks>> = chunks
            .iter()
            .map(|&chunk| {
                let trunks = ground.trunks_in(&map, &species, SEED, chunk);
                let trees = trees::trees_in(&map, &species, SEED, chunk);
                let tiles: std::collections::BTreeSet<Pos> =
                    trees.iter().map(|t| t.at.tile()).collect();
                let origin = chunk.origin();
                let side = i32::try_from(CHUNK_SIZE).expect("small");
                for y in origin.y..origin.y + side {
                    for x in origin.x..origin.x + side {
                        let p = Pos::new(x, y);
                        assert_eq!(trunks.blocks(p), tiles.contains(&p), "{p:?}");
                    }
                }
                assert_eq!(trunks.count() as usize, tiles.len());
                assert!(trunks.count() > 0, "a wooded chunk");
                trunks
            })
            .collect();
        assert_eq!(std::mem::size_of::<Trunks>(), 128, "a bit a tile");
        // The first chunks were forgotten with their trees: made again,
        // the same.
        assert!(!ground.holds_trees(chunks[0]));
        for (&chunk, trunks) in chunks.iter().zip(&first) {
            let again = ground.trunks_in(&map, &species, SEED, chunk);
            assert_eq!(*again, **trunks);
            assert!(ground.holds_trees(chunk), "held with the trees");
        }
        // A changed chunk makes its trunks again with its trees.
        let chunk = chunks[11];
        let before = ground.trunks_in(&map, &species, SEED, chunk);
        ground.set_version(chunk, 1);
        let after = ground.trunks_in(&map, &species, SEED, chunk);
        assert!(!Arc::ptr_eq(&before, &after), "made again");
        assert_eq!(before, after, "nothing felled yet");
        // A lookup reads the same bits, and nothing off the map.
        let lookup = ground.trunks_at(&map, &species, SEED);
        let origin = chunk.origin();
        for y in origin.y..origin.y + 32 {
            for x in origin.x..origin.x + 32 {
                let p = Pos::new(x, y);
                assert_eq!(lookup.blocks(p), after.blocks(p));
            }
        }
        assert!(!lookup.blocks(Pos::new(-1, 5)));
    }
}
