//! What the seed places on the ground, worked out once a chunk and held
//! (D37, D39): the trees, and the tiles their trunks block ([`Trunks`],
//! D32); the rock under the ground (`geology`, D42); the ground's fields
//! the flora reads, next. Derived state: never stored,
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
use crate::crops::Bearer;
use crate::geology::{Geology, Rock};
use crate::lru::Lru;
use crate::map::{CHUNK_SIZE, ChunkPos, Map, PATH_REACH, Pos, Terrain};
use crate::open_ground::OpenGround;
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
    /// The walkable pieces of a chunk at a version, under a set of
    /// walkable terrains ([`Links`]), the least recently used out first.
    links: Lru<(ChunkPos, Version, u8), Arc<Links>>,
    /// The chunks changed by men, with how many times: absent means 0.
    /// Grows with what men change, never with the land visited; the
    /// versions of the world's chunk changes, set as they change.
    versions: BTreeMap<ChunkPos, Version>,
    /// The rock under each tile of a chunk (`geology`), on a map that has
    /// it: never changed by men, so held by the chunk alone.
    rocks: Lru<ChunkPos, Arc<[Rock]>>,
    /// The trees and plants of a chunk bearing a crop (D45), placed from
    /// the map, the seed, the content and the trees around: as the rocks,
    /// held by chunk alone, for no man changes a tree yet. Felling, when
    /// it comes, places a chunk's bearers and its neighbours' again.
    bearers: Lru<ChunkPos, Arc<[Bearer]>>,
    /// How many times a chunk's bearers were placed into `bearers`, in
    /// all: what the placing cost, read by the vivarium's timing.
    bearers_placed: u64,
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

/// Which terrains a body may walk under a cost rule: a bit per
/// [`Terrain::rank`], set when `cost` gives the terrain a cost.
#[must_use]
pub fn walkable_mask(cost: impl Fn(Terrain) -> Option<u32>) -> u8 {
    Terrain::ALL
        .iter()
        .filter(|t| cost(**t).is_some())
        .fold(0, |mask, t| mask | 1 << t.rank())
}

/// The walkable pieces of one chunk (D32, `foule/chemins.md` §2): a
/// label per tile, 0 where a body may not stand (terrain the rule does
/// not walk, a trunk, beyond the map's edge), the same label for two
/// tiles joined by walkable ground within the chunk. Four neighbours
/// make a piece: a diagonal step never cuts a corner, so it always has a
/// a straight way round, and the pieces are exactly what a path can join.
/// Derived from the chunk's terrain and trunks, never stored; 2 KB a
/// chunk, held by the [`Ground`] with the trees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Links {
    labels: Box<[u16]>,
    /// How many pieces: the labels run 1 to `pieces`.
    pieces: u16,
}

impl Links {
    /// The pieces of `chunk` on `map`, walking the terrains of `mask`
    /// ([`walkable_mask`]) and never a tile of `trunks`.
    #[must_use]
    pub fn of(map: &Map, chunk: ChunkPos, mask: u8, trunks: &Trunks) -> Self {
        let side = CHUNK_SIZE as usize;
        let origin = chunk.origin();
        let mut open = vec![false; side * side];
        if let Some(tiles) = map.chunk(chunk) {
            for (i, terrain) in tiles.iter().enumerate() {
                let pos = Pos::new(
                    origin.x + i32::try_from(i % side).expect("in its chunk"),
                    origin.y + i32::try_from(i / side).expect("in its chunk"),
                );
                let on_map = u32::try_from(pos.x).is_ok_and(|x| x < map.width())
                    && u32::try_from(pos.y).is_ok_and(|y| y < map.height());
                open[i] = on_map && mask >> terrain.rank() & 1 == 1 && !trunks.blocks(pos);
            }
        }
        let mut labels = vec![0u16; side * side].into_boxed_slice();
        let mut pieces = 0u16;
        let mut stack = Vec::new();
        for start in 0..open.len() {
            if !open[start] || labels[start] != 0 {
                continue;
            }
            pieces += 1;
            labels[start] = pieces;
            stack.push(start);
            while let Some(i) = stack.pop() {
                let (x, y) = (i % side, i / side);
                let around = [
                    (x > 0).then(|| i - 1),
                    (x + 1 < side).then(|| i + 1),
                    (y > 0).then(|| i - side),
                    (y + 1 < side).then(|| i + side),
                ];
                for j in around.into_iter().flatten() {
                    if open[j] && labels[j] == 0 {
                        labels[j] = pieces;
                        stack.push(j);
                    }
                }
            }
        }
        Self { labels, pieces }
    }

    /// The piece of a tile of this chunk, by its column and row within
    /// the chunk; 0 where a body may not stand.
    #[must_use]
    pub fn label(&self, x: usize, y: usize) -> u16 {
        self.labels[y * CHUNK_SIZE as usize + x]
    }

    /// The piece of `pos`, a tile of this chunk; 0 where a body may not
    /// stand.
    #[must_use]
    pub fn label_at(&self, pos: Pos) -> u16 {
        let side = i32::try_from(CHUNK_SIZE).expect("small chunk side");
        let (x, y) = (pos.x.rem_euclid(side), pos.y.rem_euclid(side));
        self.label(
            usize::try_from(x).expect("non-negative"),
            usize::try_from(y).expect("non-negative"),
        )
    }

    /// How many pieces the chunk has.
    #[must_use]
    pub fn pieces(&self) -> u16 {
        self.pieces
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
    open: &'a OpenGround,
    seed: u64,
    seen: RefCell<BTreeMap<ChunkPos, Arc<Trunks>>>,
    /// The chunk looked up last: a search's tiles mostly follow one
    /// another within a chunk, and need no lookup in `seen`.
    last: RefCell<Option<(ChunkPos, Arc<Trunks>)>>,
}

impl TrunkLookup<'_> {
    /// Does an adult tree's trunk stand in `pos`? Never outside the map.
    #[must_use]
    pub fn blocks(&self, pos: Pos) -> bool {
        let chunk = pos.chunk();
        if let Some((known, trunks)) = &*self.last.borrow()
            && *known == chunk
        {
            return trunks.blocks(pos);
        }
        if !self.map.has_chunk(chunk) {
            return false;
        }
        // The borrow of `seen` ends before it is borrowed to insert.
        let seen = self.seen.borrow().get(&chunk).map(Arc::clone);
        let trunks = seen.unwrap_or_else(|| {
            let trunks = self
                .ground
                .trunks_in(self.map, self.species, self.open, self.seed, chunk);
            self.seen.borrow_mut().insert(chunk, Arc::clone(&trunks));
            trunks
        });
        let blocks = trunks.blocks(pos);
        *self.last.borrow_mut() = Some((chunk, trunks));
        blocks
    }

    /// May a path join `from` to `to` within `reach` tiles of `from` on
    /// each axis, walking the terrains of `mask` ([`walkable_mask`]) and
    /// round these trunks? The pre-test of `foule/chemins.md` §2 (D32):
    /// the walkable pieces of the chunks the square touches
    /// ([`Links`]), joined wherever two pieces meet across a chunk's
    /// edge, whole chunks and all. "No" is sure: a path within the
    /// square is a path over those chunks, so none exists. "Maybe" costs
    /// the search as ever. `from` may stand on a trunk (a search leaves
    /// its first tile whatever it is): it joins through the tiles beside
    /// it. `to` is expected walkable, as the search checks first.
    #[must_use]
    pub fn may_join(&self, from: Pos, to: Pos, reach: i32, mask: u8) -> bool {
        let (Ok(width), Ok(height)) = (
            i32::try_from(self.map.width()),
            i32::try_from(self.map.height()),
        ) else {
            return true;
        };
        let reach = reach.saturating_abs();
        let square = Square {
            x0: from.x.saturating_sub(reach).max(0),
            y0: from.y.saturating_sub(reach).max(0),
            x1: from.x.saturating_add(reach).min(width - 1),
            y1: from.y.saturating_add(reach).min(height - 1),
        };
        if square.x0 > square.x1 || square.y0 > square.y1 {
            return true;
        }
        let links_of = |chunk: ChunkPos| {
            self.ground
                .links_in(self.map, self.species, self.open, self.seed, chunk, mask)
        };
        // Both in one chunk and one piece: joined, nothing to assemble.
        let goal = links_of(to.chunk());
        let goal_label = goal.label_at(to);
        if from.chunk() == to.chunk() && goal.label_at(from) == goal_label && goal_label != 0 {
            return true;
        }
        let mut joined = Joined::over(&square, links_of);
        let Some(goal) = joined.piece(to) else {
            return true;
        };
        // Where the search sets out from: its first tile, or the tiles
        // beside it when it stands on a trunk.
        let starts = [
            from,
            Pos::new(from.x + 1, from.y),
            Pos::new(from.x - 1, from.y),
            Pos::new(from.x, from.y + 1),
            Pos::new(from.x, from.y - 1),
        ];
        starts
            .into_iter()
            .any(|start| joined.piece(start) == Some(goal))
    }
}

/// The tiles a search may touch: a square clipped to the map.
struct Square {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

/// The pieces of the chunks a square touches, joined across the chunks'
/// edges: a union-find over them, one id a piece.
struct Joined {
    square: Square,
    corner: ChunkPos,
    cols: usize,
    links: Vec<Arc<Links>>,
    /// The first id of each chunk's pieces, and one past the last.
    first: Vec<usize>,
    parent: Vec<usize>,
}

impl Joined {
    /// The pieces of the chunks `square` touches, each chunk's by
    /// `links_of`, joined wherever two walkable tiles meet across an
    /// edge between two of those chunks.
    fn over(square: &Square, links_of: impl Fn(ChunkPos) -> Arc<Links>) -> Self {
        let side = i32::try_from(CHUNK_SIZE).expect("small chunk side");
        let corner = ChunkPos::new(square.x0.div_euclid(side), square.y0.div_euclid(side));
        let cols = usize::try_from(square.x1.div_euclid(side) - corner.x + 1).expect("positive");
        let rows = usize::try_from(square.y1.div_euclid(side) - corner.y + 1).expect("positive");
        let links: Vec<Arc<Links>> = (0..rows)
            .flat_map(|r| (0..cols).map(move |c| (r, c)))
            .map(|(r, c)| {
                links_of(ChunkPos::new(
                    corner.x + i32::try_from(c).expect("few"),
                    corner.y + i32::try_from(r).expect("few"),
                ))
            })
            .collect();
        let mut first = Vec::with_capacity(links.len() + 1);
        first.push(0usize);
        for l in &links {
            first.push(first.last().expect("one at least") + usize::from(l.pieces()));
        }
        let parent = (0..*first.last().expect("one at least")).collect();
        let mut joined = Self {
            square: Square { ..*square },
            corner,
            cols,
            links,
            first,
            parent,
        };
        let edge = CHUNK_SIZE as usize - 1;
        for r in 0..rows {
            for c in 0..cols {
                let slot = r * cols + c;
                if c + 1 < cols {
                    for k in 0..=edge {
                        let (a, b) = (
                            joined.links[slot].label(edge, k),
                            joined.links[slot + 1].label(0, k),
                        );
                        if a != 0 && b != 0 {
                            joined.join((slot, a), (slot + 1, b));
                        }
                    }
                }
                if r + 1 < rows {
                    for k in 0..=edge {
                        let (a, b) = (
                            joined.links[slot].label(k, edge),
                            joined.links[slot + cols].label(k, 0),
                        );
                        if a != 0 && b != 0 {
                            joined.join((slot, a), (slot + cols, b));
                        }
                    }
                }
            }
        }
        joined
    }

    /// The id of a chunk's piece.
    fn id(&self, (slot, label): (usize, u16)) -> usize {
        self.first[slot] + usize::from(label) - 1
    }

    /// The root of a piece's set, the paths halved on the way.
    fn root(&mut self, mut i: usize) -> usize {
        while self.parent[i] != i {
            self.parent[i] = self.parent[self.parent[i]];
            i = self.parent[i];
        }
        i
    }

    /// Joins two pieces' sets.
    fn join(&mut self, a: (usize, u16), b: (usize, u16)) {
        let (a, b) = (self.root(self.id(a)), self.root(self.id(b)));
        if a != b {
            self.parent[a.max(b)] = a.min(b);
        }
    }

    /// The set of the piece a tile of the square stands in, if a body may
    /// stand there.
    fn piece(&mut self, pos: Pos) -> Option<usize> {
        let s = &self.square;
        if !(s.x0..=s.x1).contains(&pos.x) || !(s.y0..=s.y1).contains(&pos.y) {
            return None;
        }
        let chunk = pos.chunk();
        let slot = usize::try_from(chunk.y - self.corner.y).expect("in the square") * self.cols
            + usize::try_from(chunk.x - self.corner.x).expect("in the square");
        let label = self.links[slot].label_at(pos);
        (label != 0)
            .then(|| self.id((slot, label)))
            .map(|id| self.root(id))
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
                links: Lru::new(capacity),
                versions: BTreeMap::new(),
                rocks: Lru::new(capacity),
                bearers: Lru::new(capacity),
                bearers_placed: 0,
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
        open: &OpenGround,
        seed: u64,
        chunk: ChunkPos,
    ) -> Arc<[Tree]> {
        self.placed(map, species, open, seed, chunk).trees
    }

    /// The tiles blocked by the trunks of `chunk`'s trees at its current
    /// version ([`Trunks`]): held with the trees, made when they are
    /// placed, and placed again with them.
    #[must_use]
    pub fn trunks_in(
        &self,
        map: &Map,
        species: &[Species],
        open: &OpenGround,
        seed: u64,
        chunk: ChunkPos,
    ) -> Arc<Trunks> {
        self.placed(map, species, open, seed, chunk).trunks
    }

    /// The walkable pieces of `chunk` at its current version under the
    /// terrains of `mask` ([`Links`], [`walkable_mask`]): held, or else
    /// worked out now from the chunk's terrain and trunks, and held.
    #[must_use]
    pub fn links_in(
        &self,
        map: &Map,
        species: &[Species],
        open: &OpenGround,
        seed: u64,
        chunk: ChunkPos,
        mask: u8,
    ) -> Arc<Links> {
        let key = {
            let mut held = self.lock();
            let key = (chunk, held.versions.get(&chunk).copied().unwrap_or(0), mask);
            if let Some(links) = held.links.get(&key) {
                return Arc::clone(links);
            }
            key
        };
        let trunks = self.trunks_in(map, species, open, seed, chunk);
        let links = Arc::new(Links::of(map, chunk, mask, &trunks));
        self.lock().links.insert(key, Arc::clone(&links));
        links
    }

    /// A lookup of the trunks tile by tile, for one walk's plan.
    #[must_use]
    pub fn trunks_at<'a>(
        &'a self,
        map: &'a Map,
        species: &'a [Species],
        open: &'a OpenGround,
        seed: u64,
    ) -> TrunkLookup<'a> {
        TrunkLookup {
            ground: self,
            map,
            species,
            open,
            seed,
            seen: RefCell::new(BTreeMap::new()),
            last: RefCell::new(None),
        }
    }

    /// `chunk`'s trees and trunks at its current version: held, or else
    /// placed now and held.
    fn placed(
        &self,
        map: &Map,
        species: &[Species],
        open: &OpenGround,
        seed: u64,
        chunk: ChunkPos,
    ) -> Placed {
        let (key, held) = {
            let mut held = self.lock();
            let key = (chunk, held.versions.get(&chunk).copied().unwrap_or(0));
            (key, held.trees.get(&key).cloned())
        };
        if let Some(placed) = held {
            return placed;
        }
        let placed = Placed::new(chunk, trees::trees_in(map, species, open, seed, chunk));
        self.lock().trees.insert(key, placed.clone());
        placed
    }

    /// The trees and plants of `chunk` bearing a crop: held, or else
    /// placed now by `place` and held. A gathering asks which plant it is
    /// at each start, and a meadow holds thousands.
    pub fn bearers_in(
        &self,
        chunk: ChunkPos,
        place: impl FnOnce() -> Vec<Bearer>,
    ) -> Arc<[Bearer]> {
        if let Some(bearers) = self.lock().bearers.get(&chunk) {
            return Arc::clone(bearers);
        }
        let bearers: Arc<[Bearer]> = place().into();
        let mut held = self.lock();
        held.bearers.insert(chunk, Arc::clone(&bearers));
        held.bearers_placed += 1;
        bearers
    }

    /// How many times a chunk's bearers were placed, in all, by every
    /// thread: the chunks placed, plus the few two threads placed at once.
    #[must_use]
    pub fn bearers_placed(&self) -> u64 {
        self.lock().bearers_placed
    }

    /// Holds the trees, rocks and bearers of up to `chunks` chunks from
    /// now on, if that is more than it does ([`Lru::widen`]): the helpers
    /// placing ahead ([`crate::ahead`]) fill the caches with the land
    /// around the bodies, which on the vivarium's world exceeds the four
    /// views they were sized for, and a chunk's bearers read the trees of
    /// the nine chunks around it: forgotten, they would only be placed
    /// again, by the helper and by the simulation thread alike.
    pub fn widen(&self, chunks: usize) {
        let mut held = self.lock();
        held.trees.widen(chunks);
        held.links.widen(chunks);
        held.rocks.widen(chunks);
        held.bearers.widen(chunks);
    }

    /// Are `chunk`'s bearers held? Does not count as a use: a helper
    /// placing ahead asks before it places ([`crate::ahead`]).
    #[must_use]
    pub fn holds_bearers(&self, chunk: ChunkPos) -> bool {
        self.lock().bearers.contains(&chunk)
    }

    /// The rock under each tile of `chunk`, row by row
    /// ([`Geology::rocks_in`]): held, or else worked out now and held.
    /// `None` on a map without rock under its ground (made by the first
    /// generator, or filled) and for a chunk off the map.
    #[must_use]
    pub fn rocks_in(
        &self,
        map: &Map,
        geology: &Geology,
        seed: u64,
        chunk: ChunkPos,
    ) -> Option<Arc<[Rock]>> {
        if !map.has_geology() || !map.has_chunk(chunk) {
            return None;
        }
        if let Some(rocks) = self.lock().rocks.get(&chunk) {
            return Some(Arc::clone(rocks));
        }
        let rocks: Arc<[Rock]> = geology.rocks_in(seed, chunk, map.origin()).into();
        self.lock().rocks.insert(chunk, Arc::clone(&rocks));
        Some(rocks)
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

    /// The open biomes' trees: the valley has no biome, so the
    /// placeholders will do.
    fn open() -> OpenGround {
        OpenGround::default()
    }

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
            .filter(|&chunk| !trees::trees_in(map, species, &open(), SEED, chunk).is_empty())
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
            let fresh = trees::trees_in(&map, &species, &open(), SEED, chunk);
            assert!(!ground.holds_trees(chunk));
            let placed = ground.trees_in(&map, &species, &open(), SEED, chunk);
            assert_eq!(&*placed, &fresh[..], "a miss places the trees");
            assert!(ground.holds_trees(chunk));
            let again = ground.trees_in(&map, &species, &open(), SEED, chunk);
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
                let trees = ground.trees_in(&map, &species, &open(), SEED, chunk);
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
            let again = ground.trees_in(&map, &species, &open(), SEED, chunk);
            assert_eq!(again, *trees, "{chunk:?} placed again, the same");
            assert!(!Arc::ptr_eq(&again, trees), "placed anew");
            assert!(Arc::ptr_eq(
                &ground.trees_in(&map, &species, &open(), SEED, chunk),
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
        let before = ground.trees_in(&map, &species, &open(), SEED, chunk);
        ground.set_version(chunk, 1);
        assert_eq!(ground.version(chunk), 1);
        assert!(
            !ground.holds_trees(chunk),
            "the old version is not the chunk's"
        );
        let after = ground.trees_in(&map, &species, &open(), SEED, chunk);
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
        let none = ground.trees_in(&empty, &species, &open(), SEED, ChunkPos::new(0, 0));
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
                let trunks = ground.trunks_in(&map, &species, &open(), SEED, chunk);
                let trees = trees::trees_in(&map, &species, &open(), SEED, chunk);
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
            let again = ground.trunks_in(&map, &species, &open(), SEED, chunk);
            assert_eq!(*again, **trunks);
            assert!(ground.holds_trees(chunk), "held with the trees");
        }
        // A changed chunk makes its trunks again with its trees.
        let chunk = chunks[11];
        let before = ground.trunks_in(&map, &species, &open(), SEED, chunk);
        ground.set_version(chunk, 1);
        let after = ground.trunks_in(&map, &species, &open(), SEED, chunk);
        assert!(!Arc::ptr_eq(&before, &after), "made again");
        assert_eq!(before, after, "nothing felled yet");
        // A lookup reads the same bits, and nothing off the map.
        let open = open();
        let lookup = ground.trunks_at(&map, &species, &open, SEED);
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
