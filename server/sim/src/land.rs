//! The valley's land made per chunk (D37, lightness plan idea 8): the map
//! keeps only its waters, sparse, and makes the rest of a chunk (terrain,
//! density of trees, habitat) when it is read, holding the chunks read
//! lately. Its memory follows the water and the chunks in use, never the
//! map's size nor the land visited.
//!
//! - `Waters`: what the rivers painted, per chunk holding water (each
//!   water tile's terrain, depth and current) and a bit per chunk that it
//!   holds any. Water is about 1.5 % of the land (D36).
//! - `Land`: a chunk's **tiles** (its terrain, and which tiles are open
//!   ground for the forest's depth) are made from the woods drafted on
//!   the chunk alone (`Woods::draft`), the distances to the waters
//!   swept over the chunk and the rivers' reach around it; its **woods**
//!   (the density of trees and the habitat) also need the forest's depth,
//!   which looks up to `Woods::FULL_DEPTH` tiles away: the tiles of the
//!   chunks two around it. Both are held in bounded caches, the least
//!   recently used out first, and made with the lock released: two readers
//!   missing the same chunk at once make the same chunk, the second only
//!   replaces the first.
//!
//! Made per chunk, the land is the one the whole map used to be made at
//! once, tile for tile (the fingerprint tests pin it).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::lru::Lru;
use crate::map::{CHUNK_SIZE, ChunkPos, PATH_REACH, Rivers, Terrain};
use crate::plan::window::Window;
use crate::woods::{Habitat, Woods};

/// Tiles in a chunk.
const PAGE: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

/// Chunks of the forest's depth around a chunk, each way: enough to hold
/// `Woods::FULL_DEPTH` tiles.
const DEPTH_CHUNKS: i32 = 2;

/// The chunks a view reaches each way ([`PATH_REACH`], nine chunks).
const VIEW_REACH: usize = (PATH_REACH.unsigned_abs() / CHUNK_SIZE) as usize;

/// Chunks whose tiles are held at most: four views' worth with the rim
/// their woods read (19 + 2 × 2 + 2 chunks across: the trees' rim and the
/// forest's depth beyond it), and the far view's 2 km (64 × 64 chunks,
/// counted tile by tile, `Map::count_in`). About 1.2 KB a chunk: some 8 MB
/// when full.
pub const TILE_CACHE_CHUNKS: usize = {
    let with_rim = 2 * VIEW_REACH + 1 + 2 + 2 * DEPTH_CHUNKS as usize;
    4 * with_rim * with_rim + 64 * 64
};

/// Chunks whose woods are held at most: four views' worth with the rim
/// the trees and plants read (21 × 21), as the trees' own cache
/// (`ground::TREE_CACHE_CHUNKS`). 2 KB a chunk: some 3.5 MB when full.
pub const WOODS_CACHE_CHUNKS: usize = {
    let with_rim = 2 * VIEW_REACH + 1 + 2;
    4 * with_rim * with_rim
};

/// A map's water, by chunk: the valley's, kept sparse from what its
/// rivers painted; or a window's, made chunk by chunk from the atlas's
/// window as it is read (19 November 2026: the window's whole water was
/// scanned at birth, 3 to 8 s of the vivarium's), the chunks read lately
/// held.
#[derive(Debug)]
pub(crate) struct Waters {
    width: u32,
    height: u32,
    chunks_wide: u32,
    source: Source,
}

/// Where a map's water comes from.
#[derive(Debug)]
enum Source {
    /// Kept whole, sparse.
    Sparse {
        /// One bit a chunk, row by row: does it hold any water?
        wet: Vec<u64>,
        /// The water of each chunk holding some, by its index row by row.
        chunks: BTreeMap<u32, WetChunk>,
    },
    /// Made per chunk from the window ([`Window::waters_of_chunk`]).
    Window {
        window: Arc<Window>,
        /// This water among the others (`LANDS`), for the threads' recent
        /// chunks.
        id: u64,
        /// The chunks made lately, by index; a dry chunk is an empty one.
        held: Mutex<Lru<u32, Arc<WetChunk>>>,
    },
}

impl Clone for Waters {
    /// The same water, a window's caches empty.
    fn clone(&self) -> Self {
        let source = match &self.source {
            Source::Sparse { wet, chunks } => Source::Sparse {
                wet: wet.clone(),
                chunks: chunks.clone(),
            },
            Source::Window { window, .. } => Source::window(Arc::clone(window)),
        };
        Self { source, ..*self }
    }
}

impl Source {
    fn window(window: Arc<Window>) -> Self {
        Self::Window {
            window,
            id: LANDS.fetch_add(1, Ordering::Relaxed),
            held: Mutex::new(Lru::new(WET_CACHE_CHUNKS)),
        }
    }
}

/// Chunks of a window's water held at most: those of the tiles held
/// ([`TILE_CACHE_CHUNKS`]), each made from its water and the water two
/// chunks around it. A dry chunk costs its entry; a wet one about 6
/// bytes a water tile: some 2 MB when full.
const WET_CACHE_CHUNKS: usize = TILE_CACHE_CHUNKS;

/// The water tiles of a chunk, by their place in it (row by row),
/// ascending: about 6 bytes a water tile.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct WetChunk {
    at: Vec<u16>,
    /// What lies there: a brook, a ford, water; `None` for a tile with a
    /// current but no water (none today, kept as the rivers say).
    terrain: Vec<Option<Terrain>>,
    /// Half-metres deep.
    depth: Vec<u8>,
    /// Where it runs (`map::current_code`), 0 for still.
    current: Vec<u16>,
}

impl WetChunk {
    /// The water of chunk `pos` of the atlas's `window`.
    fn of_window(window: &Window, pos: ChunkPos) -> Self {
        let mut wet = Self::default();
        let (cx, cy) = (
            u32::try_from(pos.x).expect("on the map"),
            u32::try_from(pos.y).expect("on the map"),
        );
        for (x, y, tile) in window.waters_of_chunk(cx, cy) {
            let (_, place) = place(1, x % CHUNK_SIZE, y % CHUNK_SIZE);
            wet.at.push(place);
            wet.terrain.push(Some(tile.terrain));
            wet.depth.push(tile.depth);
            wet.current.push(tile.current);
        }
        wet
    }
}

/// The chunk of `(x, y)` on a map `chunks_wide` chunks wide, by its
/// index row by row, and the tile's place in it.
fn place(chunks_wide: u32, x: u32, y: u32) -> (u32, u16) {
    let chunk = (y / CHUNK_SIZE) * chunks_wide + x / CHUNK_SIZE;
    let place = (y % CHUNK_SIZE) * CHUNK_SIZE + x % CHUNK_SIZE;
    (chunk, u16::try_from(place).expect("within a chunk"))
}

impl Waters {
    /// The water the `rivers` painted, kept sparse: their working arrays
    /// go with them.
    pub(crate) fn from_rivers(rivers: &Rivers, width: u32, height: u32) -> Self {
        let chunks_wide = width.div_ceil(CHUNK_SIZE);
        let chunks_high = height.div_ceil(CHUNK_SIZE);
        let mut wet = vec![0_u64; (chunks_wide * chunks_high).div_ceil(64) as usize];
        let mut chunks: BTreeMap<u32, WetChunk> = BTreeMap::new();
        for y in 0..height {
            for x in 0..width {
                let depth = rivers.depth_at(x, y);
                let current = rivers.current_code_at(x, y);
                if depth == 0 && current == 0 {
                    continue;
                }
                let (chunk, place) = place(chunks_wide, x, y);
                wet[chunk as usize / 64] |= 1 << (chunk % 64);
                let tiles = chunks.entry(chunk).or_default();
                tiles.at.push(place);
                tiles.terrain.push(rivers.at(x, y));
                tiles.depth.push(depth);
                tiles.current.push(current);
            }
        }
        Self {
            width,
            height,
            chunks_wide,
            source: Source::Sparse { wet, chunks },
        }
    }

    /// The water of the atlas's `window`, made per chunk as it is read.
    pub(crate) fn on_window(window: Arc<Window>) -> Self {
        let side = window.side();
        Self {
            width: side,
            height: side,
            chunks_wide: side.div_ceil(CHUNK_SIZE),
            source: Source::window(window),
        }
    }

    /// The water of the chunk of index `chunk`, if it holds any, handed
    /// to `f`.
    fn with_chunk<R>(&self, chunk: u32, f: impl FnOnce(Option<&WetChunk>) -> R) -> R {
        match &self.source {
            Source::Sparse { wet, chunks } => {
                if wet[chunk as usize / 64] & (1 << (chunk % 64)) == 0 {
                    return f(None);
                }
                f(chunks.get(&chunk))
            }
            Source::Window { window, id, held } => {
                let pos = ChunkPos::new(
                    i32::try_from(chunk % self.chunks_wide).expect("few chunks"),
                    i32::try_from(chunk / self.chunks_wide).expect("few chunks"),
                );
                let lock = || held.lock().unwrap_or_else(PoisonError::into_inner);
                // `f` is called once, in whichever branch has the chunk.
                let f = RefCell::new(Some(f));
                recent(
                    |r| &mut r.wet,
                    *id,
                    pos,
                    || {
                        let found = lock().get(&chunk).map(Arc::clone);
                        found.unwrap_or_else(|| {
                            let made = Arc::new(WetChunk::of_window(window, pos));
                            lock().insert(chunk, Arc::clone(&made));
                            made
                        })
                    },
                    |wet| {
                        let f = f.borrow_mut().take().expect("read once");
                        f((!wet.at.is_empty()).then_some(wet))
                    },
                )
            }
        }
    }

    /// The water tile at `(x, y)`: `f` of its chunk's lists and its rank
    /// in them, or `None` on land.
    fn find<R>(&self, x: u32, y: u32, f: impl FnOnce(&WetChunk, usize) -> R) -> Option<R> {
        let (chunk, place) = place(self.chunks_wide, x, y);
        self.with_chunk(chunk, |wet| {
            let wet = wet?;
            let k = wet.at.binary_search(&place).ok()?;
            Some(f(wet, k))
        })
    }

    /// What is at `(x, y)`: `None` on land, a brook, a ford or water.
    pub(crate) fn at(&self, x: u32, y: u32) -> Option<Terrain> {
        self.find(x, y, |wet, k| wet.terrain[k]).flatten()
    }

    /// How deep the water is at `(x, y)`, in half-metres; 0 on land.
    pub(crate) fn depth(&self, x: u32, y: u32) -> u8 {
        self.find(x, y, |wet, k| wet.depth[k]).unwrap_or(0)
    }

    /// Where the water runs at `(x, y)` (`map::current_code`); 0 on land.
    pub(crate) fn current(&self, x: u32, y: u32) -> u16 {
        self.find(x, y, |wet, k| wet.current[k]).unwrap_or(0)
    }

    /// The water tiles of the chunks crossing `x0..x1` × `y0..y1` (clipped
    /// to the map), with where they lie: `f(x, y, terrain)`.
    fn each_in(
        &self,
        (x0, y0, x1, y1): (i64, i64, i64, i64),
        mut f: impl FnMut(u32, u32, Terrain),
    ) {
        let side = i64::from(CHUNK_SIZE);
        let last = |end: i64, size: u32| (end.min(i64::from(size)) - 1).max(0) / side;
        for cy in y0.max(0) / side..=last(y1, self.height) {
            for cx in x0.max(0) / side..=last(x1, self.width) {
                let chunk =
                    u32::try_from(cy * i64::from(self.chunks_wide) + cx).expect("on the map");
                let (ox, oy) = (
                    u32::try_from(cx * side).expect("on the map"),
                    u32::try_from(cy * side).expect("on the map"),
                );
                self.with_chunk(chunk, |wet| {
                    let Some(wet) = wet else {
                        return;
                    };
                    for (k, place) in wet.at.iter().enumerate() {
                        if let Some(terrain) = wet.terrain[k] {
                            let place = u32::from(*place);
                            f(ox + place % CHUNK_SIZE, oy + place / CHUNK_SIZE, terrain);
                        }
                    }
                });
            }
        }
    }

    /// Tiles (eight-way) from `(x, y)` to the nearest river (water or
    /// ford) and to the nearest brook, each up to its reach + 1, as the
    /// woods read them.
    pub(crate) fn to_water(&self, x: u32, y: u32) -> (u8, u8) {
        let (far_river, far_brook) = Self::far();
        let (mut river, mut brook) = (far_river, far_brook);
        let (px, py) = (i64::from(x), i64::from(y));
        let reach = i64::from(far_river);
        self.each_in(
            (px - reach, py - reach, px + reach + 1, py + reach + 1),
            |wx, wy, terrain| {
                let d = (i64::from(wx) - px).abs().max((i64::from(wy) - py).abs());
                let d = u8::try_from(d.min(reach)).expect("within reach");
                if terrain == Terrain::Brook {
                    brook = brook.min(d);
                } else {
                    river = river.min(d);
                }
            },
        );
        (river, brook.min(far_brook))
    }

    /// The same for every tile of `x0..x1` × `y0..y1`, row by row: two
    /// sweeps over the rectangle and the waters' reach around it.
    pub(crate) fn to_water_in(&self, (x0, y0, x1, y1): (u32, u32, u32, u32)) -> Vec<(u8, u8)> {
        let (far_river, far_brook) = Self::far();
        let field = |far: u8, brook: bool| -> Vec<u8> {
            let reach = i64::from(far);
            // The window: the rectangle and the reach around it, on the map.
            let (wx0, wy0) = (
                (i64::from(x0) - reach).max(0),
                (i64::from(y0) - reach).max(0),
            );
            let (wx1, wy1) = (
                (i64::from(x1) + reach).min(i64::from(self.width)),
                (i64::from(y1) + reach).min(i64::from(self.height)),
            );
            let (wide, high) = (
                usize::try_from(wx1 - wx0).expect("small"),
                usize::try_from(wy1 - wy0).expect("small"),
            );
            let mut to = vec![far; wide * high];
            let mut any = false;
            self.each_in((wx0, wy0, wx1, wy1), |tx, ty, terrain| {
                let (tx, ty) = (i64::from(tx), i64::from(ty));
                if (terrain == Terrain::Brook) == brook
                    && (wx0..wx1).contains(&tx)
                    && (wy0..wy1).contains(&ty)
                {
                    let i = usize::try_from((ty - wy0) * (wx1 - wx0) + tx - wx0).expect("in");
                    to[i] = 0;
                    any = true;
                }
            });
            if any {
                Woods::sweep(&mut to, wide, high, far);
            }
            let mut out = Vec::with_capacity(((x1 - x0) * (y1 - y0)) as usize);
            for y in y0..y1 {
                let row = usize::try_from(i64::from(y) - wy0).expect("in") * wide;
                let from = row + usize::try_from(i64::from(x0) - wx0).expect("in");
                out.extend_from_slice(&to[from..from + (x1 - x0) as usize]);
            }
            out
        };
        let river = field(far_river, false);
        let brook = field(far_brook, true);
        river.into_iter().zip(brook).collect()
    }

    /// How far the woods look for a river and for a brook: their reach
    /// and a tile.
    fn far() -> (u8, u8) {
        (Woods::RIVER_REACH + 1, Woods::BROOK_REACH + 1)
    }
}

/// A chunk's tiles: its terrain, and which are open ground for the
/// forest's depth (neither forest before the depth nor water). Tiles past
/// the map's edge are rock.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ChunkTiles {
    pub(crate) terrain: Box<[Terrain]>,
    open: [u64; PAGE / 64],
}

/// A chunk's woods: per tile, the density of trees and the habitat.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ChunkWoods {
    pub(crate) density: Box<[u8]>,
    pub(crate) habitat: Box<[Habitat]>,
}

/// The valley's land, made per chunk from its waters and its woods' plan.
#[derive(Debug)]
pub(crate) struct Land {
    /// This land among the others (`LANDS`), for the threads' recent
    /// chunks.
    id: u64,
    width: u32,
    height: u32,
    cliff_rows: u32,
    waters: Waters,
    woods: Woods,
    /// The atlas's window the land is cut from (generator version 3, the
    /// relief look test): its heights, and where the forest may grow.
    relief: Option<Arc<Window>>,
    held: Mutex<Held>,
}

/// What the lock guards: the chunks made lately.
#[derive(Debug)]
struct Held {
    tiles: Lru<ChunkPos, Arc<ChunkTiles>>,
    woods: Lru<ChunkPos, Arc<ChunkWoods>>,
}

/// The lands made so far, numbered: the threads' recent chunks name their
/// land by its number.
static LANDS: AtomicU64 = AtomicU64::new(0);

/// Slots of chunks of each kind a thread keeps at hand, a square of
/// [`RECENT_SIDE`] chunks a side laid over the map: a chunk's slot is its
/// place in that square, so chunks apart by less than the side never
/// share one, and a search window (19 × 19 chunks, [`PATH_REACH`]) or a
/// chunk's placing with its rim (3 × 3) finds its chunks at hand. Memory
/// a thread, every slot full: `RECENT_SLOTS` × (1.2 KB of tiles + 2 KB of
/// woods + a wet chunk) ≈ 1 MB at most, pinned past the land's caches.
const RECENT_SLOTS: usize = RECENT_SIDE * RECENT_SIDE;
const RECENT_SIDE: usize = 16;

/// The chunks a thread read last, by land, a slot a chunk: most reads
/// follow one another in a chunk or two (a path search, a chunk's trees
/// and plants), and need neither the lock nor the cache's order. A chunk
/// never changes, so one held here a while after the cache let it go is
/// still right. Before 22 November 2026 this was a list of four, searched
/// and reordered at each read, the chunk cloned out (`Arc`): the tiles
/// read by the path searches were a quarter of a vivarium run (D51).
#[derive(Default)]
struct Recent {
    tiles: RecentTable<ChunkTiles>,
    woods: RecentTable<ChunkWoods>,
    wet: RecentTable<WetChunk>,
}

/// A thread's recent chunks of one kind, [`RECENT_SLOTS`] slots once
/// used: land, chunk, what was made.
type RecentTable<T> = Vec<Option<(u64, ChunkPos, Arc<T>)>>;

thread_local! {
    static RECENT: RefCell<Recent> = RefCell::new(Recent::default());
}

/// The slot of `chunk` among a thread's recent chunks.
#[allow(
    clippy::cast_sign_loss,
    reason = "the chunk's place in a square tiling: only the low bits matter"
)]
fn slot_of(chunk: ChunkPos) -> usize {
    let side = i32::try_from(RECENT_SIDE).expect("small");
    ((chunk.x & (side - 1)) + (chunk.y & (side - 1)) * side) as usize
}

/// `read` of the entry of `land` and `chunk` among a thread's recent
/// chunks (`pick` chooses tiles, woods or wet), or else of the chunk
/// `fetch`ed (the thread's table not borrowed meanwhile) and put in its
/// slot. The chunk is read in place, never cloned out.
fn recent<T, R>(
    pick: fn(&mut Recent) -> &mut RecentTable<T>,
    land: u64,
    chunk: ChunkPos,
    fetch: impl FnOnce() -> Arc<T>,
    read: impl Fn(&T) -> R,
) -> R {
    let slot = slot_of(chunk);
    let held = RECENT.with_borrow_mut(|r| {
        let table = pick(r);
        match table.get(slot) {
            Some(Some((l, c, value))) if *l == land && *c == chunk => Some(read(value)),
            _ => None,
        }
    });
    if let Some(value) = held {
        return value;
    }
    let value = fetch();
    let out = read(&value);
    RECENT.with_borrow_mut(|r| {
        let table = pick(r);
        if table.is_empty() {
            table.resize_with(RECENT_SLOTS, || None);
        }
        table[slot] = Some((land, chunk, value));
    });
    out
}

/// How many chunks of tiles this thread holds at hand (tests).
#[cfg(test)]
fn recent_tiles_held() -> usize {
    RECENT.with_borrow(|r| r.tiles.iter().flatten().count())
}

impl Clone for Land {
    /// The same land, its caches empty.
    fn clone(&self) -> Self {
        let mut land = Self::new(
            self.width,
            self.height,
            self.cliff_rows,
            self.waters.clone(),
            self.woods.clone(),
        );
        land.relief.clone_from(&self.relief);
        land
    }
}

impl Land {
    /// The land of a map of `width` × `height` with the cliff over its
    /// first `cliff_rows`, these `waters` and these `woods`.
    pub(crate) fn new(
        width: u32,
        height: u32,
        cliff_rows: u32,
        waters: Waters,
        woods: Woods,
    ) -> Self {
        Self::with_capacity(
            (width, height, cliff_rows),
            waters,
            woods,
            (TILE_CACHE_CHUNKS, WOODS_CACHE_CHUNKS),
        )
    }

    /// The same, holding at most `capacity` chunks of tiles and of woods.
    fn with_capacity(
        (width, height, cliff_rows): (u32, u32, u32),
        waters: Waters,
        woods: Woods,
        (tiles, woods_held): (usize, usize),
    ) -> Self {
        Self {
            id: LANDS.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            cliff_rows,
            waters,
            woods,
            relief: None,
            held: Mutex::new(Held {
                tiles: Lru::new(tiles),
                woods: Lru::new(woods_held),
            }),
        }
    }

    /// The same land on the atlas's `window` (the relief look test): the
    /// woods grow only in its forest.
    pub(crate) fn on_window(mut self, window: Arc<Window>) -> Self {
        self.relief = Some(window);
        self
    }

    /// The atlas's window under this land, if any.
    pub(crate) fn relief(&self) -> Option<&Window> {
        self.relief.as_deref()
    }

    /// How deep the water is at `(x, y)`, in half-metres: 0 on land and
    /// on the cliff.
    pub(crate) fn depth(&self, x: u32, y: u32) -> u8 {
        if y < self.cliff_rows {
            return 0;
        }
        self.waters.depth(x, y)
    }

    /// Where the water runs at `(x, y)` (`map::current_code`): 0 on land
    /// and on the cliff.
    pub(crate) fn current(&self, x: u32, y: u32) -> u16 {
        if y < self.cliff_rows {
            return 0;
        }
        self.waters.current(x, y)
    }

    fn lock(&self) -> MutexGuard<'_, Held> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The terrain of the tile `(x, y)` of the map.
    pub(crate) fn terrain(&self, x: u32, y: u32) -> Terrain {
        let chunk = ChunkPos::new(
            i32::try_from(x / CHUNK_SIZE).expect("on the map"),
            i32::try_from(y / CHUNK_SIZE).expect("on the map"),
        );
        let place = ((y % CHUNK_SIZE) * CHUNK_SIZE + x % CHUNK_SIZE) as usize;
        recent(
            |r| &mut r.tiles,
            self.id,
            chunk,
            || self.tiles(chunk),
            |tiles| tiles.terrain[place],
        )
    }

    /// The density of trees and the habitat of the tile `(x, y)` of the
    /// map.
    pub(crate) fn wood(&self, x: u32, y: u32) -> (u8, Habitat) {
        let chunk = ChunkPos::new(
            i32::try_from(x / CHUNK_SIZE).expect("on the map"),
            i32::try_from(y / CHUNK_SIZE).expect("on the map"),
        );
        let place = ((y % CHUNK_SIZE) * CHUNK_SIZE + x % CHUNK_SIZE) as usize;
        recent(
            |r| &mut r.woods,
            self.id,
            chunk,
            || self.woods(chunk),
            |woods| (woods.density[place], woods.habitat[place]),
        )
    }

    /// A chunk's tiles: held, or else made now and held.
    pub(crate) fn tiles(&self, chunk: ChunkPos) -> Arc<ChunkTiles> {
        if let Some(tiles) = self.lock().tiles.get(&chunk).map(Arc::clone) {
            return tiles;
        }
        let tiles = Arc::new(self.make_tiles(chunk));
        self.lock().tiles.insert(chunk, Arc::clone(&tiles));
        tiles
    }

    /// A chunk's woods: held, or else made now and held.
    pub(crate) fn woods(&self, chunk: ChunkPos) -> Arc<ChunkWoods> {
        if let Some(woods) = self.lock().woods.get(&chunk).map(Arc::clone) {
            return woods;
        }
        let woods = Arc::new(self.make_woods(chunk));
        self.lock().woods.insert(chunk, Arc::clone(&woods));
        woods
    }

    /// The tiles of `chunk` on the map: `x0, y0, x1, y1`.
    fn rect(&self, chunk: ChunkPos) -> (u32, u32, u32, u32) {
        let x0 = u32::try_from(chunk.x).expect("on the map") * CHUNK_SIZE;
        let y0 = u32::try_from(chunk.y).expect("on the map") * CHUNK_SIZE;
        (
            x0,
            y0,
            (x0 + CHUNK_SIZE).min(self.width),
            (y0 + CHUNK_SIZE).min(self.height),
        )
    }

    /// The woods of `chunk` drafted, before the forest's depth.
    fn draft(&self, chunk: ChunkPos) -> crate::woods::Draft {
        let rect = self.rect(chunk);
        let (x0, y0, x1, _) = rect;
        let to = self.waters.to_water_in(rect);
        let w = x1 - x0;
        let dry = |x: u32, y: u32| y >= self.cliff_rows && self.waters.at(x, y).is_none();
        self.woods.draft(
            rect,
            |x, y| {
                dry(x, y)
                    && self
                        .relief
                        .as_ref()
                        .is_none_or(|w| w.wooded(i64::from(x), i64::from(y)))
            },
            // The atlas's open biomes: a meadow outside the forest.
            |x, y| {
                dry(x, y)
                    && self
                        .relief
                        .as_ref()
                        .is_some_and(|w| !w.wooded(i64::from(x), i64::from(y)))
            },
            |x, y| to[((y - y0) * w + (x - x0)) as usize],
            |x, y| self.waters.to_water(x, y),
        )
    }

    fn make_tiles(&self, chunk: ChunkPos) -> ChunkTiles {
        let rect = self.rect(chunk);
        let (x0, y0, x1, y1) = rect;
        let draft = self.draft(chunk);
        let mut tiles = ChunkTiles {
            terrain: vec![Terrain::Rock; PAGE].into_boxed_slice(),
            open: [0; PAGE / 64],
        };
        let mut i = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                let place = ((y - y0) * CHUNK_SIZE + (x - x0)) as usize;
                let water = self.waters.at(x, y);
                tiles.terrain[place] = if y < self.cliff_rows {
                    Terrain::Rock
                } else if let Some(water) = water {
                    water
                } else if Woods::is_forest(draft.density[i], draft.ramped[i]) {
                    Terrain::Forest
                } else {
                    Terrain::Grass
                };
                if Woods::is_open(draft.density[i], water.is_some()) {
                    tiles.open[place / 64] |= 1 << (place % 64);
                }
                i += 1;
            }
        }
        tiles
    }

    fn make_woods(&self, chunk: ChunkPos) -> ChunkWoods {
        let (x0, y0, x1, y1) = self.rect(chunk);
        let draft = self.draft(chunk);
        // The forest's depth over the chunks around, from their open
        // ground and the map's edges.
        let chunks_wide = i32::try_from(self.width.div_ceil(CHUNK_SIZE)).expect("few chunks");
        let chunks_high = i32::try_from(self.height.div_ceil(CHUNK_SIZE)).expect("few chunks");
        let (cx0, cx1) = (
            (chunk.x - DEPTH_CHUNKS).max(0),
            (chunk.x + DEPTH_CHUNKS).min(chunks_wide - 1),
        );
        let (cy0, cy1) = (
            (chunk.y - DEPTH_CHUNKS).max(0),
            (chunk.y + DEPTH_CHUNKS).min(chunks_high - 1),
        );
        let (wx0, _, _, _) = self.rect(ChunkPos::new(cx0, cy0));
        let (_, wy0, _, _) = self.rect(ChunkPos::new(cx0, cy0));
        let (_, _, wx1, wy1) = self.rect(ChunkPos::new(cx1, cy1));
        let (w, h) = ((wx1 - wx0) as usize, (wy1 - wy0) as usize);
        let mut depth = vec![Woods::FULL_DEPTH; w * h];
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                let near = ChunkPos::new(cx, cy);
                let tiles = self.tiles(near);
                let (nx0, ny0, nx1, ny1) = self.rect(near);
                for y in ny0..ny1 {
                    for x in nx0..nx1 {
                        let place = ((y - ny0) * CHUNK_SIZE + (x - nx0)) as usize;
                        let open = tiles.open[place / 64] & (1 << (place % 64)) != 0;
                        let edge = x == 0 || y == 0 || x + 1 == self.width || y + 1 == self.height;
                        if open || edge {
                            depth[(y - wy0) as usize * w + (x - wx0) as usize] = 0;
                        }
                    }
                }
            }
        }
        Woods::sweep(&mut depth, w, h, Woods::FULL_DEPTH);
        let mut woods = ChunkWoods {
            density: vec![0; PAGE].into_boxed_slice(),
            habitat: vec![Habitat::None; PAGE].into_boxed_slice(),
        };
        let mut i = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                let place = ((y - y0) * CHUNK_SIZE + (x - x0)) as usize;
                let d = depth[(y - wy0) as usize * w + (x - wx0) as usize];
                (woods.density[place], woods.habitat[place]) =
                    Woods::finish(draft.density[i], draft.habitat[i], draft.ramped[i], d);
                i += 1;
            }
        }
        woods
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Map;
    use crate::woods::WholeWoods;

    /// A valley's waters and woods, as `Map::valley` draws them.
    fn parts(width: u32, height: u32, seed: u64) -> (Rivers, Waters, Woods, u32) {
        let cliff_rows = (height / 8).max(2);
        let rivers = Rivers::generate(width, height, cliff_rows, seed);
        let waters = Waters::from_rivers(&rivers, width, height);
        let camp = (width * 134 / 256, height * 139 / 256);
        let woods = Woods::new(width, height, cliff_rows, seed, camp);
        (rivers, waters, woods, cliff_rows)
    }

    #[test]
    fn the_sparse_waters_are_the_rivers_tile_for_tile() {
        let (width, height) = (300, 260);
        let (rivers, waters, _, _) = parts(width, height, 3);
        let mut wet = 0;
        for y in 0..height {
            for x in 0..width {
                assert_eq!(waters.at(x, y), rivers.at(x, y), "({x}, {y})");
                assert_eq!(waters.depth(x, y), rivers.depth_at(x, y));
                assert_eq!(waters.current(x, y), rivers.current_code_at(x, y));
                wet += usize::from(rivers.depth_at(x, y) > 0);
            }
        }
        assert!(wet > 1000, "{wet} water tiles");
        // A few chunks hold water; the others are a bit each.
        let Source::Sparse { chunks, .. } = &waters.source else {
            unreachable!("a valley's water is sparse");
        };
        assert!(chunks.len() < (width.div_ceil(32) * height.div_ceil(32)) as usize);
        // The distances to the waters, point by point and swept over a
        // chunk, agree with the whole map swept at once.
        let to_river = Woods::to_water(width, height, &rivers, false, Woods::RIVER_REACH);
        let to_brook = Woods::to_water(width, height, &rivers, true, Woods::BROOK_REACH);
        for (x0, y0) in [(0, 0), (96, 64), (288, 256), (160, 224)] {
            let rect = (x0, y0, (x0 + 32).min(width), (y0 + 32).min(height));
            let field = waters.to_water_in(rect);
            let mut k = 0;
            for y in rect.1..rect.3 {
                for x in rect.0..rect.2 {
                    let i = (y * width + x) as usize;
                    let whole = (to_river[i], to_brook[i]);
                    assert_eq!(field[k], whole, "({x}, {y})");
                    assert_eq!(waters.to_water(x, y), whole, "({x}, {y})");
                    k += 1;
                }
            }
        }
    }

    #[test]
    fn the_chunks_are_the_whole_map_made_at_once() {
        // Not a multiple of the chunk: the last chunks reach past the edge.
        for (width, height, seed) in [(512, 512, 1), (330, 290, 2)] {
            let (rivers, _, _, cliff_rows) = parts(width, height, seed);
            let camp = (width * 134 / 256, height * 139 / 256);
            let whole = WholeWoods::generate(width, height, cliff_rows, seed, &rivers, camp);
            let map = Map::valley(width, height, seed);
            for y in 0..height {
                for x in 0..width {
                    let pos =
                        crate::map::Pos::new(i32::try_from(x).unwrap(), i32::try_from(y).unwrap());
                    let terrain = if y < cliff_rows {
                        Terrain::Rock
                    } else if let Some(water) = rivers.at(x, y) {
                        water
                    } else if whole.density(x, y) >= Woods::FOREST {
                        Terrain::Forest
                    } else {
                        Terrain::Grass
                    };
                    assert_eq!(map.get(pos), Some(terrain), "seed {seed} ({x}, {y})");
                    assert_eq!(
                        map.trees(pos),
                        whole.density(x, y),
                        "seed {seed} ({x}, {y})"
                    );
                    assert_eq!(
                        map.habitat(pos),
                        whole.habitat(x, y),
                        "seed {seed} ({x}, {y})"
                    );
                }
            }
        }
    }

    /// A window's water made chunk by chunk, read in any order and
    /// after eviction, is the whole window's scan tile for tile: what
    /// the map kept whole until 19 November 2026.
    #[test]
    fn a_windows_water_made_per_chunk_is_its_whole_scan() {
        use crate::plan::{Continent, Region, region};
        let mut wet_tiles = 0;
        for seed in [1, 2, 3, 5] {
            let continent = Continent::with(seed, 128, None);
            let site = region::site(&continent);
            let region = Region::around(&continent, site, 256);
            let corner = Window::choose(&region, 1024);
            let window = Arc::new(Window::cut(&continent, &region, corner, 1024));
            let whole: BTreeMap<(u32, u32), crate::plan::window::Wet> = window
                .waters()
                .into_iter()
                .map(|(x, y, wet)| ((x, y), wet))
                .collect();
            wet_tiles += whole.len();
            let mut waters = Waters::on_window(Arc::clone(&window));
            // Room for few chunks: read last chunk first, then again
            // first chunk first, each made again after its eviction.
            if let Source::Window { held, .. } = &mut waters.source {
                *held = Mutex::new(Lru::new(3));
            }
            let chunks = window.side() / CHUNK_SIZE;
            let order: Vec<u32> = (0..chunks * chunks)
                .rev()
                .chain(0..chunks * chunks)
                .collect();
            for chunk in order {
                let (x0, y0) = (chunk % chunks * CHUNK_SIZE, chunk / chunks * CHUNK_SIZE);
                for y in y0..y0 + CHUNK_SIZE {
                    for x in x0..x0 + CHUNK_SIZE {
                        let wet = whole.get(&(x, y));
                        assert_eq!(waters.at(x, y), wet.map(|w| w.terrain), "{seed} ({x}, {y})");
                        assert_eq!(waters.depth(x, y), wet.map_or(0, |w| w.depth));
                        assert_eq!(waters.current(x, y), wet.map_or(0, |w| w.current));
                    }
                }
            }
        }
        assert!(wet_tiles > 10_000, "{wet_tiles} water tiles");
    }

    #[test]
    fn a_chunk_made_twice_or_after_eviction_is_the_same() {
        let (width, height) = (384, 384);
        let (_, waters, woods, cliff_rows) = parts(width, height, 5);
        // Room for very few chunks: reading a few around evicts the first.
        let land = Land::with_capacity((width, height, cliff_rows), waters, woods, (2, 2));
        let chunk = ChunkPos::new(5, 6);
        let (tiles, wood) = (land.tiles(chunk), land.woods(chunk));
        assert_eq!(*tiles, land.make_tiles(chunk), "made twice");
        assert_eq!(*wood, land.make_woods(chunk), "made twice");
        for x in 0..4 {
            let _ = land.woods(ChunkPos::new(x, 1));
        }
        {
            let held = land.lock();
            assert!(!held.tiles.contains(&chunk) && !held.woods.contains(&chunk));
            assert!(held.tiles.len() <= 2 && held.woods.len() <= 2);
        }
        assert_eq!(*land.tiles(chunk), *tiles, "made again after eviction");
        assert_eq!(*land.woods(chunk), *wood, "made again after eviction");
        // A tile read one by one is the chunk's.
        let (x0, y0) = (5 * CHUNK_SIZE, 6 * CHUNK_SIZE);
        for k in 0..CHUNK_SIZE {
            let (x, y) = (x0 + k, y0 + (k * 7) % CHUNK_SIZE);
            let place = ((y - y0) * CHUNK_SIZE + (x - x0)) as usize;
            assert_eq!(land.terrain(x, y), tiles.terrain[place]);
            assert_eq!(land.wood(x, y), (wood.density[place], wood.habitat[place]));
        }
    }

    /// A thread's recent chunks fill at most their slots, and a tile read
    /// through them is the generator's, whichever chunk held the slot
    /// before: 20 × 20 chunks read over a 16 × 16 table, chunks sixteen
    /// apart taking turns in one slot.
    #[test]
    fn a_threads_recent_chunks_are_bounded_and_read_the_generators_tiles() {
        let (width, height) = (640, 640);
        let (_, waters, woods, cliff_rows) = parts(width, height, 7);
        let land = Land::new(width, height, cliff_rows, waters, woods);
        let chunks = i32::try_from(width / CHUNK_SIZE).expect("few chunks");
        assert!(chunks * chunks > i32::try_from(RECENT_SLOTS).expect("small"));
        let tile_of = |chunk: ChunkPos, k: u32| {
            let (x0, y0) = (
                u32::try_from(chunk.x).expect("on the map") * CHUNK_SIZE,
                u32::try_from(chunk.y).expect("on the map") * CHUNK_SIZE,
            );
            (x0 + k % CHUNK_SIZE, y0 + (k * 7) % CHUNK_SIZE)
        };
        for cy in 0..chunks {
            for cx in 0..chunks {
                let (x, y) = tile_of(ChunkPos::new(cx, cy), 3);
                let _ = land.terrain(x, y);
            }
        }
        assert!(
            recent_tiles_held() <= RECENT_SLOTS,
            "{}",
            recent_tiles_held()
        );
        // Two chunks of one slot, read in turn: each tile is its own chunk's.
        let (a, b) = (ChunkPos::new(2, 3), ChunkPos::new(18, 3));
        assert_eq!(slot_of(a), slot_of(b));
        let (made_a, made_b) = (land.make_tiles(a), land.make_tiles(b));
        for k in 0..CHUNK_SIZE {
            for (chunk, made) in [(a, &made_a), (b, &made_b)] {
                let (x, y) = tile_of(chunk, k);
                let place = ((y % CHUNK_SIZE) * CHUNK_SIZE + x % CHUNK_SIZE) as usize;
                assert_eq!(
                    land.terrain(x, y),
                    made.terrain[place],
                    "{chunk:?} ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn a_path_crosses_chunks_never_made() {
        // A fresh map: no chunk made yet. A path across many chunks makes
        // the tiles it reads as it goes, and is the path the whole map
        // gives (its tiles read all at once afterwards).
        let map = Map::valley(512, 512, 4);
        let camp = map.camp();
        let walk = |t: Terrain| match t {
            Terrain::Grass | Terrain::Ford | Terrain::Brook => Some(10),
            Terrain::Forest => Some(14),
            Terrain::Water => Some(40),
            Terrain::Rock => None,
        };
        let goal = map
            .nearest(
                crate::map::Pos::new(camp.x + 150, camp.y + 120),
                Terrain::Grass,
            )
            .expect("grass");
        let fresh = Map::valley(512, 512, 4);
        let path = fresh.path(camp, goal, walk).expect("a way");
        assert_eq!(path.last(), Some(&goal));
        let again = map.path(camp, goal, walk).expect("a way");
        assert_eq!(path, again);
        for p in &path {
            assert!(fresh.get(*p).and_then(walk).is_some(), "{p:?}");
        }
    }
}
