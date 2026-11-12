//! The ground: a grid of tiles, one tile being about one metre — the
//! room one person takes (D31).
//!
//! Terrain is generated from a few parameters and never stored tile by
//! tile: a [`Map`] serializes as its [`MapSpec`]. Everything that changes
//! the world (walls, roads, fires) is an object placed on a tile and goes
//! through the event journal, not through the terrain. Materials,
//! resources and buildings come as content later (D15).
//!
//! The grid is cut into square chunks of [`CHUNK_SIZE`] tiles, the unit
//! of transfer and of interest (D08, D31).

// Generation is integer-only (D24; linted from `lib.rs` and
// `sim/clippy.toml`). The terrain and the waters have no exception. This
// module also holds geometry for moving bodies, which is not generation:
// `FinePos::distance_to` and `line_clear` allow floats where they stand.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::hash::{BuildHasherDefault, Hasher};

use serde::{Deserialize, Serialize};

use crate::r#gen::scatter::purpose;
use crate::r#gen::{Groves, Lcg};
use crate::woods::{Habitat, Woods};

/// Side of a chunk, in tiles.
pub const CHUNK_SIZE: u32 = 32;

/// Millimetres per tile: bodies are placed this finely inside the grid
/// (D32), so that a walking pace is a whole number per tick. Everything
/// else (items, buildings, perception) lives on tiles.
pub const MM_PER_TILE: i32 = 1000;

/// Walking cost of a straight step, in path units.
const STRAIGHT_COST: u32 = 10;
/// Walking cost of a diagonal step: √2 straight steps, rounded (D32).
const DIAGONAL_COST: u32 = 14;

/// How far [`Map::path`] may go from where it starts, in tiles on each
/// axis: the view's 288 tiles (D08), the outer bound of any search. A
/// target farther away, or reachable only by leaving the square, gets no
/// path. So no search explores more than 577 × 577 tiles, about 50 to
/// 100 ms at worst in a release build (0.16 to 0.3 µs a tile, measured
/// on 10 November 2026), whatever the size of the world. The world's own
/// walks search far less: within what the body sees from the terrain it
/// stands on and under a budget of tiles, both from the content
/// ([`Map::path_within`], D32).
pub const PATH_REACH: i32 = 288;

/// A tile position on the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Pos {
    /// Column, growing east.
    pub x: i32,
    /// Row, growing south.
    pub y: i32,
}

impl Pos {
    /// A position.
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// The chunk this tile belongs to.
    pub fn chunk(self) -> ChunkPos {
        let side = i32::try_from(CHUNK_SIZE).expect("small chunk side");
        ChunkPos::new(self.x.div_euclid(side), self.y.div_euclid(side))
    }

    /// Is a step from here to `other` diagonal? Only meaningful for
    /// adjacent tiles.
    pub fn is_diagonal_to(self, other: Self) -> bool {
        self.x != other.x && self.y != other.y
    }

    /// The centre of this tile, in millimetres.
    pub fn centre(self) -> FinePos {
        FinePos::new(
            self.x * MM_PER_TILE + MM_PER_TILE / 2,
            self.y * MM_PER_TILE + MM_PER_TILE / 2,
        )
    }

    /// The eight neighbours, in a fixed order (determinism): the four
    /// straight ones first, then the diagonals.
    fn neighbours(self) -> [Self; 8] {
        [
            Self::new(self.x, self.y - 1),
            Self::new(self.x + 1, self.y),
            Self::new(self.x, self.y + 1),
            Self::new(self.x - 1, self.y),
            Self::new(self.x + 1, self.y - 1),
            Self::new(self.x + 1, self.y + 1),
            Self::new(self.x - 1, self.y + 1),
            Self::new(self.x - 1, self.y - 1),
        ]
    }
}

/// A position inside the grid, in millimetres: where a body actually
/// stands, not snapped to a tile's centre (D32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FinePos {
    /// Millimetres east of the grid's origin.
    pub x: i32,
    /// Millimetres south of the grid's origin.
    pub y: i32,
}

impl FinePos {
    /// A fine position.
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// The tile this position is in.
    pub fn tile(self) -> Pos {
        Pos::new(
            self.x.div_euclid(MM_PER_TILE),
            self.y.div_euclid(MM_PER_TILE),
        )
    }

    /// Straight-line distance to another position, in millimetres.
    // Not generation (it moves bodies): floats are allowed here, and stay.
    #[allow(clippy::disallowed_methods)]
    pub fn distance_to(self, other: Self) -> f64 {
        let dx = f64::from(other.x - self.x);
        let dy = f64::from(other.y - self.y);
        dx.hypot(dy)
    }
}

/// A chunk position: the grid of chunks, chunk (0, 0) holding tiles
/// (0..32, 0..32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChunkPos {
    /// Column of chunks, growing east.
    pub x: i32,
    /// Row of chunks, growing south.
    pub y: i32,
}

impl ChunkPos {
    /// A chunk position.
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// The north-west tile of this chunk.
    pub fn origin(self) -> Pos {
        let side = i32::try_from(CHUNK_SIZE).expect("small chunk side");
        Pos::new(self.x * side, self.y * side)
    }
}

/// What a tile is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Terrain {
    /// Open ground.
    Grass,
    /// Trees; walkable, slower later.
    Forest,
    /// The river, a metre deep or more; not walkable.
    Water,
    /// A shallow crossing of the river, half a metre deep.
    Ford,
    /// A brook, one or two tiles wide, a metre deep at most; walked
    /// through.
    Brook,
    /// The cliff; not walkable.
    Rock,
}

impl Terrain {
    /// Every terrain, for content that must cover them all.
    pub const ALL: [Self; 6] = [
        Self::Grass,
        Self::Forest,
        Self::Water,
        Self::Ford,
        Self::Brook,
        Self::Rock,
    ];

    /// Can a body ever stand here? The content's movement rules refine
    /// this with a cost; they cannot make water or rock walkable.
    pub fn is_passable(self) -> bool {
        !matches!(self, Self::Water | Self::Rock)
    }
}

/// How a map is generated. A map is stored and sent as this, never as
/// its tiles (D31).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MapSpec {
    /// One terrain everywhere.
    Filled {
        /// Columns.
        width: u32,
        /// Rows.
        height: u32,
        /// The terrain.
        terrain: Terrain,
    },
    /// The first version's valley, see [`Map::valley`].
    Valley {
        /// Columns.
        width: u32,
        /// Rows.
        height: u32,
        /// Generation seed.
        seed: u64,
    },
}

/// The grid. Stored as its spec alone (D31): the tiles are generated
/// again on load.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "MapSpec")]
pub struct Map {
    spec: MapSpec,
    width: u32,
    height: u32,
    tiles: Vec<Terrain>,
    /// Per tile, how deep the water is, in half-metres; 0 on land.
    depth: Vec<u8>,
    /// Per tile, the density of trees, 0 to 100 (D37).
    trees: Vec<u8>,
    /// Per tile, the kind of place it is for trees (D37).
    habitat: Vec<Habitat>,
    /// Per tile of water, where it runs (`current_code`); 0 elsewhere (D36).
    currents: Vec<u16>,
}

impl From<MapSpec> for Map {
    fn from(spec: MapSpec) -> Self {
        match spec {
            MapSpec::Filled {
                width,
                height,
                terrain,
            } => Self::filled(width, height, terrain),
            MapSpec::Valley {
                width,
                height,
                seed,
            } => Self::valley(width, height, seed),
        }
    }
}

impl From<Map> for MapSpec {
    fn from(map: Map) -> Self {
        map.spec
    }
}

/// Written as its spec, read from the map in place: a snapshot used to
/// copy every tile (57 MB on the 3072 valley) only to write a few bytes.
impl Serialize for Map {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.spec.serialize(serializer)
    }
}

impl Map {
    /// A map filled with one terrain.
    pub fn filled(width: u32, height: u32, terrain: Terrain) -> Self {
        Self {
            spec: MapSpec::Filled {
                width,
                height,
                terrain,
            },
            width,
            height,
            tiles: vec![terrain; (width * height) as usize],
            depth: vec![0; (width * height) as usize],
            trees: vec![0; (width * height) as usize],
            habitat: vec![Habitat::None; (width * height) as usize],
            currents: vec![0; (width * height) as usize],
        }
    }

    /// The first version's valley: a cliff to the north, a tree of waters
    /// (see [`Rivers`]: a main river from the cliff's foot to the south
    /// edge, tributaries and brooks joining it, water as a volume whose
    /// flow sets width and depth, fords where it is half a metre deep,
    /// islands between the arms of a divergence), woods (see [`Woods`]:
    /// massifs with their edges and hearts, clearings, copses, lone and
    /// riverside trees, a clearing at the camp), grass elsewhere.
    /// Deterministic for a given seed.
    pub fn valley(width: u32, height: u32, seed: u64) -> Self {
        let mut map = Self::filled(width, height, Terrain::Grass);
        map.spec = MapSpec::Valley {
            width,
            height,
            seed,
        };
        let cliff_rows = (height / 8).max(2);
        let rivers = Rivers::generate(width, height, cliff_rows, seed);
        let camp = Self::camp_of(width, height);
        let woods = Woods::generate(width, height, cliff_rows, seed, &rivers, camp);

        for y in 0..height {
            for x in 0..width {
                let pos = Pos::new(
                    i32::try_from(x).expect("map fits in i32"),
                    i32::try_from(y).expect("map fits in i32"),
                );
                let terrain = if y < cliff_rows {
                    Terrain::Rock
                } else if let Some(water) = rivers.at(x, y) {
                    water
                } else if woods.density(x, y) >= Woods::FOREST {
                    Terrain::Forest
                } else {
                    Terrain::Grass
                };
                map.set(pos, terrain);
                let i = (y * width + x) as usize;
                if y >= cliff_rows {
                    map.depth[i] = rivers.depth_at(x, y);
                    if let Some((heading, speed)) = rivers.current_at(x, y) {
                        map.currents[i] = current_code(heading, speed);
                    }
                }
                map.trees[i] = woods.density(x, y);
                map.habitat[i] = woods.habitat(x, y);
            }
        }
        map
    }

    /// How deep the water is at `pos`, in half-metres: 0 on land and
    /// outside the map, 1 on a ford, 1 or 2 in a brook, 2 or more in a
    /// river.
    pub fn depth(&self, pos: Pos) -> u8 {
        self.index(pos).map_or(0, |i| self.depth[i])
    }

    /// The density of trees at `pos`, 0 to 100: 0 on open ground and
    /// outside the map, from `Woods::FOREST` up in the forest (D37).
    #[must_use]
    pub fn trees(&self, pos: Pos) -> u8 {
        self.index(pos).map_or(0, |i| self.trees[i])
    }

    /// The kind of place `pos` is for trees; `Habitat::None` on water,
    /// rock and outside the map (D37).
    #[must_use]
    pub fn habitat(&self, pos: Pos) -> Habitat {
        self.index(pos).map_or(Habitat::None, |i| self.habitat[i])
    }

    /// Where the first bodies start: the camp, left open by the woods.
    #[must_use]
    pub fn camp(&self) -> Pos {
        let (x, y) = Self::camp_of(self.width, self.height);
        Pos::new(
            i32::try_from(x).expect("map fits in i32"),
            i32::try_from(y).expect("map fits in i32"),
        )
    }

    /// The camp of a map of that size: in the first version's valley of
    /// 256, tile (134, 139), by the main river.
    fn camp_of(width: u32, height: u32) -> (u32, u32) {
        (width * 134 / 256, height * 139 / 256)
    }

    /// How this map is generated.
    pub fn spec(&self) -> MapSpec {
        self.spec
    }

    /// Columns.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Rows.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// All tiles, row by row from the north-west corner.
    pub fn tiles(&self) -> &[Terrain] {
        &self.tiles
    }

    /// How many tiles of each terrain lie in the columns `x0..x1` and rows
    /// `y0..y1` (clipped to the map), in the order grass, forest, rock,
    /// water, ford, brook.
    pub fn count_in(&self, x0: u32, y0: u32, x1: u32, y1: u32) -> [u32; 6] {
        let mut count = [0_u32; 6];
        for y in y0..y1.min(self.height) {
            let row = (y * self.width) as usize;
            for &t in
                &self.tiles[row + x0.min(self.width) as usize..row + x1.min(self.width) as usize]
            {
                count[match t {
                    Terrain::Grass => 0,
                    Terrain::Forest => 1,
                    Terrain::Rock => 2,
                    Terrain::Water => 3,
                    Terrain::Ford => 4,
                    Terrain::Brook => 5,
                }] += 1;
            }
        }
        count
    }

    /// Chunks across, the last one possibly reaching past the edge.
    pub fn chunks_wide(&self) -> u32 {
        self.width.div_ceil(CHUNK_SIZE)
    }

    /// Chunks down, the last one possibly reaching past the edge.
    pub fn chunks_high(&self) -> u32 {
        self.height.div_ceil(CHUNK_SIZE)
    }

    /// Does this chunk hold any tile of the map?
    pub fn has_chunk(&self, chunk: ChunkPos) -> bool {
        u32::try_from(chunk.x).is_ok_and(|x| x < self.chunks_wide())
            && u32::try_from(chunk.y).is_ok_and(|y| y < self.chunks_high())
    }

    /// The tiles of a chunk, row by row, `CHUNK_SIZE × CHUNK_SIZE` of
    /// them; beyond the map's edge the ground is rock. `None` for a chunk
    /// entirely outside the map.
    pub fn chunk(&self, chunk: ChunkPos) -> Option<Vec<Terrain>> {
        if !self.has_chunk(chunk) {
            return None;
        }
        let origin = chunk.origin();
        let side = i32::try_from(CHUNK_SIZE).expect("small chunk side");
        let mut tiles = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize);
        for y in 0..side {
            for x in 0..side {
                tiles.push(
                    self.get(Pos::new(origin.x + x, origin.y + y))
                        .unwrap_or(Terrain::Rock),
                );
            }
        }
        Some(tiles)
    }

    /// Where the water runs in a chunk, tile by tile like `chunk`
    /// (`current_code`: 0 where it does not). `None` for a chunk entirely
    /// outside the map.
    pub fn chunk_currents(&self, chunk: ChunkPos) -> Option<Vec<u16>> {
        if !self.has_chunk(chunk) {
            return None;
        }
        let origin = chunk.origin();
        let side = i32::try_from(CHUNK_SIZE).expect("small chunk side");
        let mut out = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize);
        for y in 0..side {
            for x in 0..side {
                let pos = Pos::new(origin.x + x, origin.y + y);
                out.push(self.index(pos).map_or(0, |i| self.currents[i]));
            }
        }
        Some(out)
    }

    /// Where the water runs at `pos`: its heading (0 to 15, as
    /// `current_code`) and speed in centimetres a second; `None` on land
    /// and outside the map.
    pub fn current(&self, pos: Pos) -> Option<(u8, u16)> {
        self.index(pos)
            .and_then(|i| decode_current(self.currents[i]))
    }

    /// The terrain at `pos`, or `None` outside the map.
    pub fn get(&self, pos: Pos) -> Option<Terrain> {
        self.index(pos).map(|i| self.tiles[i])
    }

    /// Sets the terrain at `pos`; ignored outside the map. Generation
    /// only: terrain is not stored, so a change here does not survive a
    /// snapshot.
    pub(crate) fn set(&mut self, pos: Pos, terrain: Terrain) {
        if let Some(i) = self.index(pos) {
            self.tiles[i] = terrain;
        }
    }

    /// Can a body stand at `pos`?
    pub fn is_passable(&self, pos: Pos) -> bool {
        self.get(pos).is_some_and(Terrain::is_passable)
    }

    /// For every tile, its distance in eight-way steps to the nearest tile
    /// of `terrain` (0 on such a tile; `u32::MAX` when there is none),
    /// row by row like [`Map::tiles`].
    pub fn distance_to(&self, terrain: Terrain) -> Vec<u32> {
        self.distance_within(terrain, u32::MAX)
    }

    /// The same, but only out to `limit` steps: tiles farther away keep
    /// `u32::MAX`, and the search does not walk the rest of the map.
    pub fn distance_within(&self, terrain: Terrain, limit: u32) -> Vec<u32> {
        let mut distance = vec![u32::MAX; self.tiles.len()];
        let mut queue = VecDeque::new();
        for (i, t) in self.tiles.iter().enumerate() {
            if *t == terrain {
                distance[i] = 0;
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            if distance[i] >= limit {
                continue;
            }
            let here = self.pos_at(i);
            for next in here.neighbours() {
                if let Some(j) = self.index(next)
                    && distance[j] == u32::MAX
                {
                    distance[j] = distance[i] + 1;
                    queue.push_back(j);
                }
            }
        }
        distance
    }

    /// The nearest tile to `pos` with the given terrain, searching in
    /// growing squares; `pos` itself first. `None` when there is none.
    pub fn nearest(&self, pos: Pos, terrain: Terrain) -> Option<Pos> {
        let reach = i32::try_from(self.width.max(self.height)).expect("map fits in i32");
        for radius in 0..=reach {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs() != radius && dy.abs() != radius {
                        continue;
                    }
                    let candidate = Pos::new(pos.x + dx, pos.y + dy);
                    if self.get(candidate) == Some(terrain) {
                        return Some(candidate);
                    }
                }
            }
        }
        None
    }

    /// Cheapest walking path from `from` to `to`, not including `from`,
    /// weighing each tile by `cost` (10 is the reference, `None` cannot
    /// be walked). Steps go in eight directions; a diagonal step costs √2
    /// straight ones and never cuts a corner (both straight neighbours it
    /// passes between must be walkable) (D32). Empty when already there;
    /// `None` when unreachable within [`PATH_REACH`] tiles of `from`. Its
    /// work and memory follow the tiles it expands, never the map's size
    /// (D31, D39).
    pub fn path(
        &self,
        from: Pos,
        to: Pos,
        cost: impl Fn(Terrain) -> Option<u32>,
    ) -> Option<Vec<Pos>> {
        self.search(from, to, PATH_REACH, u32::MAX, cost).0
    }

    /// [`Map::path`] within what a body sees and the effort it may spend
    /// (D32): the search stays within `reach` tiles of `from` on each
    /// axis and expands at most `budget` tiles. `None` for a target out
    /// of reach, out of the square's ground, or not found within the
    /// budget; the caller then walks straight. Within both, the path is
    /// exactly the one [`Map::path`] finds in the same square. Its cost is
    /// bounded by the budget whatever the ground: a refusal costs under
    /// half a millisecond at 1,500 tiles (measured on 10 November 2026).
    pub fn path_within(
        &self,
        from: Pos,
        to: Pos,
        reach: i32,
        budget: u32,
        cost: impl Fn(Terrain) -> Option<u32>,
    ) -> Option<Vec<Pos>> {
        self.search(from, to, reach, budget, cost).0
    }

    /// The A* search behind [`Map::path`] and [`Map::path_within`], with
    /// the number of tiles it expanded.
    fn search(
        &self,
        from: Pos,
        to: Pos,
        reach: i32,
        budget: u32,
        cost: impl Fn(Terrain) -> Option<u32>,
    ) -> (Option<Vec<Pos>>, u32) {
        if from == to {
            return (Some(Vec::new()), 0);
        }
        let within = |pos: Pos| (pos.x - from.x).abs() <= reach && (pos.y - from.y).abs() <= reach;
        let cost_at = |pos: Pos| self.get(pos).and_then(&cost);
        if !within(to) || cost_at(from).is_none() || cost_at(to).is_none() {
            return (None, 0);
        }
        // The heuristic must not overestimate: scale it by the cheapest
        // terrain there is.
        let cheapest = Terrain::ALL
            .iter()
            .filter_map(|t| cost(*t))
            .min()
            .unwrap_or(STRAIGHT_COST);
        let estimate = |a: Pos| octile(a, to) * cheapest / STRAIGHT_COST;
        // A* with an octile heuristic. Ties break on insertion order so
        // the path is the same on every machine.
        let mut reached = Reached::default();
        let mut open = BinaryHeap::new();
        let mut pushed = 0u64;
        let mut expanded = 0u32;
        reached.improve(from, 0, 0);
        open.push(Reverse((estimate(from), pushed, from)));

        while let Some(Reverse((guess, _, here))) = open.pop() {
            if here == to {
                return (Some(reached.walk_back(from, to)), expanded);
            }
            let cost_here = reached.cost(here);
            if guess > cost_here + estimate(here) {
                // A stale entry: the tile was reached more cheaply since,
                // and expanded from there already; expanding it again
                // would change nothing.
                continue;
            }
            expanded += 1;
            if expanded > budget {
                // The effort is spent: the body walks straight instead.
                return (None, expanded);
            }
            for (step, next) in (0u8..).zip(here.neighbours()) {
                if !within(next) {
                    continue;
                }
                let Some(terrain_cost) = cost_at(next) else {
                    continue;
                };
                let step_cost = if here.is_diagonal_to(next) {
                    if cost_at(Pos::new(here.x, next.y)).is_none()
                        || cost_at(Pos::new(next.x, here.y)).is_none()
                    {
                        continue; // no cutting corners
                    }
                    DIAGONAL_COST
                } else {
                    STRAIGHT_COST
                };
                let cost = cost_here + step_cost * terrain_cost / STRAIGHT_COST;
                if reached.improve(next, cost, step) {
                    pushed += 1;
                    open.push(Reverse((cost + estimate(next), pushed, next)));
                }
            }
        }
        (None, expanded)
    }

    /// Pulls a tile path taut into straight segments from `from`: each
    /// waypoint is the farthest tile centre reachable in a straight line
    /// that crosses only walkable tiles no dearer than the tiles the
    /// segment replaces, so a road is not short-cut through the field
    /// beside it (D32). The tile a segment starts in is left whatever it
    /// costs: a body under a tree, sent onto the grass, walks straight
    /// from where it stands rather than to a tile's centre first. The
    /// last waypoint is the path's last tile.
    pub fn pull(
        &self,
        from: FinePos,
        path: &[Pos],
        cost: impl Fn(Terrain) -> Option<u32>,
    ) -> Vec<FinePos> {
        let cost_at = |pos: Pos| self.get(pos).and_then(&cost);
        let mut route = Vec::new();
        let mut current = from;
        let mut first = 0;
        while first < path.len() {
            let mut dearest = cost_at(path[first]).unwrap_or(u32::MAX);
            let mut chosen = first;
            let start = current.tile();
            for (j, tile) in path.iter().enumerate().skip(first) {
                dearest = dearest.max(cost_at(*tile).unwrap_or(u32::MAX));
                let allowed = |pos: Pos| pos == start || cost_at(pos).is_some_and(|c| c <= dearest);
                if line_clear(current, tile.centre(), allowed) {
                    chosen = j;
                }
            }
            current = path[chosen].centre();
            route.push(current);
            first = chosen + 1;
        }
        route
    }

    fn index(&self, pos: Pos) -> Option<usize> {
        let x = u32::try_from(pos.x).ok()?;
        let y = u32::try_from(pos.y).ok()?;
        (x < self.width && y < self.height).then(|| (y * self.width + x) as usize)
    }

    fn pos_at(&self, index: usize) -> Pos {
        let index = u32::try_from(index).expect("index within the map");
        Pos::new(
            i32::try_from(index % self.width).expect("map fits in i32"),
            i32::try_from(index / self.width).expect("map fits in i32"),
        )
    }
}

/// Does the straight segment from `a` to `b` cross only allowed tiles?
/// Walks the tiles the segment passes through; where it goes exactly
/// through a corner it may graze one disallowed tile but never slip
/// between two.
pub fn line_clear(a: FinePos, b: FinePos, allowed: impl Fn(Pos) -> bool) -> bool {
    first_block(a, b, allowed).is_none()
}

/// How far a body gets walking straight from `a` toward `b`: `b` when
/// the way is clear, else a point on the segment a little before the
/// first tile it may not enter (`a` itself when it is that close). For a
/// player's walk when no path is found within reach (D31, D32).
// Not generation: floats, as in `first_block`.
#[allow(clippy::cast_possible_truncation, clippy::float_arithmetic)]
pub fn line_reach(a: FinePos, b: FinePos, allowed: impl Fn(Pos) -> bool) -> FinePos {
    /// How far before the obstacle the body stops, in millimetres.
    const SHORT_OF: f64 = 300.0;
    let Some(t) = first_block(a, b, allowed) else {
        return b;
    };
    let length = a.distance_to(b);
    if length <= 0.0 {
        return a;
    }
    let t = (t - SHORT_OF / length).max(0.0);
    FinePos::new(
        a.x + (f64::from(b.x - a.x) * t).round() as i32,
        a.y + (f64::from(b.y - a.y) * t).round() as i32,
    )
}

/// Where along the segment from `a` to `b` (0 at `a`, 1 at `b`) it first
/// enters a disallowed tile, slipping between two at a corner counting
/// as entering; `None` when it never does.
// Not generation (it pulls a walked path taut): floats are allowed here,
// and stay.
#[allow(clippy::cast_possible_truncation, clippy::float_arithmetic)]
fn first_block(a: FinePos, b: FinePos, allowed: impl Fn(Pos) -> bool) -> Option<f64> {
    let scale = f64::from(MM_PER_TILE);
    let (x0, y0) = (f64::from(a.x) / scale, f64::from(a.y) / scale);
    let (x1, y1) = (f64::from(b.x) / scale, f64::from(b.y) / scale);
    let (mut tx, mut ty) = (x0.floor() as i32, y0.floor() as i32);
    let (ex, ey) = (x1.floor() as i32, y1.floor() as i32);
    if !allowed(Pos::new(tx, ty)) {
        return Some(0.0);
    }
    let (dx, dy) = (x1 - x0, y1 - y0);
    let step_x = if dx > 0.0 { 1 } else { -1 };
    let step_y = if dy > 0.0 { 1 } else { -1 };
    let edge = |t: i32, step: i32| f64::from(if step > 0 { t + 1 } else { t });
    let mut t_max_x = if dx == 0.0 {
        f64::INFINITY
    } else {
        (edge(tx, step_x) - x0) / dx
    };
    let mut t_max_y = if dy == 0.0 {
        f64::INFINITY
    } else {
        (edge(ty, step_y) - y0) / dy
    };
    let t_delta_x = if dx == 0.0 {
        f64::INFINITY
    } else {
        (1.0 / dx).abs()
    };
    let t_delta_y = if dy == 0.0 {
        f64::INFINITY
    } else {
        (1.0 / dy).abs()
    };
    let mut remaining = (ex - tx).abs() + (ey - ty).abs() + 1;
    let mut entered = 0.0;
    while (tx, ty) != (ex, ey) && remaining > 0 {
        remaining -= 1;
        if (t_max_x - t_max_y).abs() < 1e-9 {
            // Through a corner: no slipping between two blocked tiles.
            if !allowed(Pos::new(tx + step_x, ty)) && !allowed(Pos::new(tx, ty + step_y)) {
                return Some(t_max_x);
            }
            entered = t_max_x;
            tx += step_x;
            ty += step_y;
            t_max_x += t_delta_x;
            t_max_y += t_delta_y;
        } else if t_max_x < t_max_y {
            entered = t_max_x;
            tx += step_x;
            t_max_x += t_delta_x;
        } else {
            entered = t_max_y;
            ty += step_y;
            t_max_y += t_delta_y;
        }
        if !allowed(Pos::new(tx, ty)) {
            return Some(entered);
        }
    }
    ((tx, ty) != (ex, ey)).then_some(entered)
}

/// Tiles of a chunk: the size of a page of [`Reached`].
const PAGE: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

/// The tiles one path search has reached, kept in pages of a chunk: for
/// each tile, its cheapest cost so far and the step that reached it. Its
/// memory follows the chunks the search touches, never the map's size
/// (D31, D39).
#[derive(Default)]
struct Reached {
    /// The first slot of each chunk's page.
    pages: HashMap<ChunkPos, usize, BuildHasherDefault<ChunkHasher>>,
    /// Per slot, the cheapest cost so far; `u32::MAX` when not reached.
    cost: Vec<u32>,
    /// Per slot, which of [`Pos::neighbours`] of the previous tile this
    /// one is.
    step: Vec<u8>,
    /// The page looked up last: most tiles a search looks at are in the
    /// chunk of the tile before.
    last: Option<(ChunkPos, usize)>,
}

impl Reached {
    /// The slot of a tile, making its chunk's page when it has none.
    fn slot(&mut self, pos: Pos) -> usize {
        let chunk = pos.chunk();
        let first = match self.last {
            Some((known, first)) if known == chunk => first,
            _ => {
                let fresh = self.cost.len();
                let first = *self.pages.entry(chunk).or_insert(fresh);
                if first == fresh {
                    self.cost.resize(fresh + PAGE, u32::MAX);
                    self.step.resize(fresh + PAGE, 0);
                }
                self.last = Some((chunk, first));
                first
            }
        };
        let origin = chunk.origin();
        let side = i32::try_from(CHUNK_SIZE).expect("small chunk side");
        first
            + usize::try_from((pos.y - origin.y) * side + (pos.x - origin.x)).expect("in its chunk")
    }

    /// The cheapest cost found so far to `pos`.
    fn cost(&mut self, pos: Pos) -> u32 {
        let slot = self.slot(pos);
        self.cost[slot]
    }

    /// Records reaching `pos` at `cost` by `step` if that is cheaper than
    /// before; says whether it was.
    fn improve(&mut self, pos: Pos, cost: u32, step: u8) -> bool {
        let slot = self.slot(pos);
        let better = cost < self.cost[slot];
        if better {
            self.cost[slot] = cost;
            self.step[slot] = step;
        }
        better
    }

    /// The path from `from` to `to`, not including `from`, following the
    /// recorded steps back.
    fn walk_back(&mut self, from: Pos, to: Pos) -> Vec<Pos> {
        let offsets = Pos::new(0, 0).neighbours();
        let mut path = Vec::new();
        let mut at = to;
        while at != from {
            path.push(at);
            let slot = self.slot(at);
            let back = offsets[usize::from(self.step[slot])];
            at = Pos::new(at.x - back.x, at.y - back.y);
        }
        path.reverse();
        path
    }
}

/// Hashes a chunk for [`Reached`]: a multiply and a fold, no random keys,
/// so a search does the same work on every run (D24). Its table is only
/// looked up, never iterated, so the hash cannot change a path.
#[derive(Default)]
struct ChunkHasher(u64);

impl ChunkHasher {
    fn mix(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

impl Hasher for ChunkHasher {
    fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 32)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.mix(u64::from(byte));
        }
    }

    fn write_i32(&mut self, n: i32) {
        self.mix(u64::from(n.cast_unsigned()));
    }
}

/// Lower bound on the walking cost between two tiles with eight-way steps.
fn octile(a: Pos, b: Pos) -> u32 {
    let dx = a.x.abs_diff(b.x);
    let dy = a.y.abs_diff(b.y);
    STRAIGHT_COST * dx.max(dy) + (DIAGONAL_COST - STRAIGHT_COST) * dx.min(dy)
}

/// Sixteen headings, 22.5 degrees apart, as unit vectors in thousandths
/// (x then y); heading 0 points down the map (+y), heading 4 to +x.
const HEADINGS: [(i64, i64); 16] = [
    (0, 1000),
    (383, 924),
    (707, 707),
    (924, 383),
    (1000, 0),
    (924, -383),
    (707, -707),
    (383, -924),
    (0, -1000),
    (-383, -924),
    (-707, -707),
    (-924, -383),
    (-1000, 0),
    (-924, 383),
    (-707, 707),
    (-383, 924),
];

/// The heading closest to the direction `(dx, dy)`.
fn heading_towards(dx: i64, dy: i64) -> u8 {
    (0..16u8)
        .max_by_key(|h| {
            let v = HEADINGS[usize::from(*h)];
            v.0 * dx + v.1 * dy
        })
        .expect("sixteen headings")
}

/// The waters of a map, generated by the rules of the shore and of
/// water as a volume (D36):
///
/// - depth is counted in **half-metres**; a tile half a metre deep holds
///   one *half-cube*; a river carries a **flow** in half-cubes, conserved
///   along its course: at a confluence the flows add up, at a divergence
///   the flow is shared between the two arms;
/// - the flow sets the **section**: `width × depth` covers the flow, the
///   width within `1..=MAX_WIDTH`, the depth within `1..=MAX_DEPTH`; a
///   stretch is either *wide* or *deep* (see [`Rivers::section`]);
/// - water one or two tiles wide is a **brook** ([`Terrain::Brook`]):
///   walkable, at most a metre deep; from three wide it is a river;
/// - a **ford** is where a river spreads until it is half a metre deep
///   right across, which only a flow of `MAX_WIDTH` half-cubes or less
///   can do; a river that could have one has one by chance, not by
///   right: a bank without a ford is a reason for a bridge or a boat;
/// - the network is a **tree**: a main river enters at the cliff's foot
///   and leaves by the south edge; one or two tributaries enter by an
///   edge; a few brooks
///   rise where the ground is wet and meander to the nearest water; each
///   ends where it meets water, so waters join and never cross;
/// - a **divergence** splits a river in two arms around an island, from
///   a rock to a large island, and the arms join again downstream; the
///   smaller arm, when shallow enough, shoals into a ford;
/// - a centre path holds a heading for a run of steps (one tile each),
///   then bends by one step of 22.5 degrees, never turning back on the
///   way it entered;
/// - the **banks breathe**: each bank of a river, on its own, holds its
///   line for a while, then turns a quarter of a step of heading out or in for a
///   few steps and comes back, so a river widens, narrows, slides
///   sideways or bends on one side only, up to a pool of
///   `MAX_POOL_WIDTH`; the depth takes what the width leaves, so the flow
///   is kept, but never less than a metre: a wide pool is slower water,
///   never a ford. Calm near junctions, fords, divergences, the map's
///   edges, and on brooks.
///
/// All arithmetic is in integers (tile thousandths), so the same seed
/// gives the same waters everywhere.
pub struct Rivers {
    width: u32,
    height: u32,
    /// Per tile: 0 for land, else how many half-metres deep the water is.
    depth: Vec<u8>,
    /// Per tile: whether the water there is a brook (one or two wide).
    brook: Vec<bool>,
    /// Per tile of water: where it runs, `current_code` of its heading and
    /// speed; 0 on land.
    current: Vec<u16>,
    /// Per tile: the water that painted it, as index + 1; 0 for land.
    owner: Vec<u16>,
    /// Per tile painted by a water: the step of its path that did.
    step: Vec<u32>,
    /// While the brooks are traced: per block of `WATER_BLOCK` tiles,
    /// whether any water lies in it, so a search for the nearest water
    /// skips at once a square holding none (empty the rest of the time).
    water_blocks: Vec<bool>,
    /// One line per water, for the tests' dumps.
    #[cfg(test)]
    log: Vec<String>,
    /// The network the waters were painted from, for the tests.
    #[cfg(test)]
    network: Vec<River>,
}

/// Side of a block of `Rivers::water_blocks`, in tiles.
const WATER_BLOCK: u32 = 16;

/// A current as one number: its heading (0 to 15) times 256 plus its
/// speed in centimetres a second (1 to 255); 0 for none.
pub fn current_code(heading: u8, speed: u16) -> u16 {
    u16::from(heading % 16) * 256 + speed.clamp(1, 255)
}

/// The heading and speed of a current coded by `current_code`.
pub fn decode_current(code: u16) -> Option<(u8, u16)> {
    (code != 0).then(|| (u8::try_from(code / 256).expect("heading"), code % 256))
}

/// Flat index of a tile in a `width` × `height` grid, if inside.
fn index_in(width: u32, height: u32, x: i64, y: i64) -> Option<usize> {
    (x >= 0 && y >= 0 && x < i64::from(width) && y < i64::from(height))
        .then(|| usize::try_from(y * i64::from(width) + x).expect("fits"))
}

/// How a stretch of water spends its flow: spread out, or dug in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    /// Wider than deep: width about the square root of twice the flow.
    Wide,
    /// Narrow and deep: width about the square root of the flow.
    Deep,
}

/// One water of the network, before it is painted.
struct River {
    /// Centre path, one step per tile.
    path: Vec<Step>,
    /// Half-cubes it carries where it starts.
    entry_flow: i64,
    /// Waters joining it: the step of this water where they arrive, and
    /// their index. A tributary is always traced after the water it
    /// joins, so its index is greater.
    joins: Vec<(usize, usize)>,
    /// Half-cubes it carries where it ends, once the tributaries are
    /// counted.
    end_flow: i64,
    /// The shape of each stretch between two junctions, from the start.
    shapes: Vec<Shape>,
    /// Divergences along it.
    divergences: Vec<Divergence>,
    /// Steps spread into fords.
    fords: Vec<usize>,
    /// Per step, how far each bank stands outside its line, the left one
    /// then the right one, in steps of `BANK_SLOPE` (negative inside).
    /// Empty until the banks breathe.
    banks: Vec<(i64, i64)>,
}

impl River {
    fn new(path: Vec<Step>, entry_flow: i64) -> Self {
        Self {
            path,
            entry_flow,
            joins: Vec::new(),
            end_flow: entry_flow,
            shapes: Vec::new(),
            divergences: Vec::new(),
            fords: Vec::new(),
            banks: Vec::new(),
        }
    }

    /// Half-cubes carried at `step`: what entered plus every tributary
    /// that has arrived by then.
    fn flow_at(&self, step: usize, rivers: &[River]) -> i64 {
        self.entry_flow
            + self
                .joins
                .iter()
                .filter(|(at, _)| *at <= step)
                .map(|(_, j)| rivers[*j].end_flow)
                .sum::<i64>()
    }

    /// The shape of the stretch `step` is in.
    fn shape_at(&self, step: usize) -> Shape {
        let arrived = self.joins.iter().filter(|(at, _)| *at <= step).count();
        self.shapes.get(arrived).copied().unwrap_or(Shape::Wide)
    }
}

/// A river parting in two arms around an island.
struct Divergence {
    /// First step of the split; the arms leave the centre path there.
    from: usize,
    /// Steps until the arms meet again.
    len: usize,
    /// Half-cubes carried by the arm to the left of the current, then
    /// the right.
    flows: (i64, i64),
    /// Greatest offset of each arm's centre from the path, in thousandths.
    offset: i64,
    /// Steps over which the offset rises to its greatest, and falls back.
    ramp: usize,
}

/// One step of a water's centre path, in tile thousandths.
#[derive(Clone, Copy)]
struct Step {
    x: i64,
    y: i64,
    heading: u8,
}

/// A point of a band of water to paint: a centre and a section.
#[derive(Clone, Copy)]
struct Band {
    x: i64,
    y: i64,
    half_width: i64,
    depth: u8,
    /// One or two tiles wide: a brook.
    brook: bool,
    /// The heading the water runs along here.
    heading: u8,
    /// How fast it runs, in centimetres a second.
    speed: u16,
}

/// Where a traced path ended.
enum End {
    /// Out of the map.
    Edge,
    /// In the water of another river: its index and the step it was
    /// painted by.
    Water(usize, usize),
}

impl Rivers {
    /// Narrowest river, in tiles; narrower water is a brook.
    pub const MIN_WIDTH: u32 = 3;
    /// Widest a ford may spread, in tiles: a ford is half a metre deep
    /// right across, so only a river carrying this many half-cubes or
    /// fewer can have one (and a divergence's arm shoal into one).
    pub const MAX_WIDTH: u32 = 12;
    /// Widest pool of a small river, in tiles: where its banks breathe
    /// out, a river may widen to this, or to half again its own width,
    /// whichever is more.
    pub const MAX_POOL_WIDTH: u32 = 18;
    /// Deepest water, in half-metres (6 m, a great river); past that a
    /// river only widens (8 November 2026; it was 4 m).
    pub const MAX_DEPTH: u8 = 12;
    /// Widest river, in tiles: a safeguard only, far past what any valley
    /// gives (a river widens with its flow and no other limit; 8 November
    /// 2026, it was 12, and a river took no more water past that).
    pub const MAX_RIVER_WIDTH: u32 = 64;
    /// Deepest brook, in half-metres: one can still stand in it.
    pub const MAX_BROOK_DEPTH: u8 = 2;
    /// Half-cubes the main river brings in, at least and at most.
    const MAIN_FLOW: (u32, u32) = (6, 10);
    /// Half-cubes a tributary entering by an edge brings in: never more
    /// than `MAX_WIDTH`, so that a ford is possible on it.
    const TRIBUTARY_FLOW: (u32, u32) = (5, 9);
    /// Half-cubes a brook rises with.
    const BROOK_FLOW: (u32, u32) = (1, 2);
    /// How many brooks a map has, in tenths, for every 256 tiles of its
    /// side (its mean side), at least and at most: 3 to 5 on 256 × 256,
    /// 10 to 16 on 768 × 768. By the side, not the ground: a brook is
    /// long and crosses much of a map, so more ground calls for longer
    /// brooks rather than proportionally more of them.
    const BROOKS_TENTHS: (u32, u32) = (34, 54);
    /// Share of brooks, in percent, that enter by the map's edges (west,
    /// east or south) or rise at the cliff's foot; the others rise on wet
    /// ground inside.
    const EDGE_BROOKS: u32 = 67;
    /// A brook rising inside does so at least this many tiles from any
    /// water, so it runs a while before it joins one.
    const SPRING_CLEARANCE: i64 = 50;
    /// Shortest brook, in steps.
    const BROOK_MIN_STEPS: usize = 60;
    /// A brook entering by an edge (or rising at the cliff's foot) goes at
    /// least this many tiles away from it, rather than along it.
    const BROOK_INWARD: i64 = 30;
    /// Past its first `BROOK_LEAVE` steps, a brook keeps at least this
    /// many tiles from the map's edges and the cliff.
    const BROOK_EDGE_CLEAR: i64 = 5;
    const BROOK_LEAVE: usize = 8;
    /// Two brooks never run closer than this many tiles, except over the
    /// last stretch of one, where it comes to join the other.
    const BROOK_SPACING: i64 = 25;
    /// How many tributaries enter by the edges, in tenths for every 256
    /// tiles of the map's side, at least and at most: 1 on 256 × 256, 7 to
    /// 12 on 3072 × 3072, so the valley is about 1.5 % water. By the
    /// side, like the brooks: they enter along the edges, and counted by
    /// the ground they would make a large map mostly water.
    const TRIBUTARIES_TENTHS: (u32, u32) = (6, 10);
    /// Longest brook, in steps: one that wanders further is no brook.
    const BROOK_STEPS: usize = 250;
    /// How fast water runs where its section just holds its flow, in
    /// centimetres a second, and the slowest and fastest it runs.
    const BASE_SPEED: u16 = 35;
    const SPEED: (u16, u16) = (4, 200);
    /// Farthest a brook goes on past its traced end to meet the water.
    const BROOK_REACH: usize = 8;
    /// How many steps a brook holds a heading, at least and at most: it
    /// meanders more than a river.
    const BROOK_RUN: (u32, u32) = (2, 5);
    /// A brook never strays more than this many steps of heading from the
    /// heading it rose with (4 steps = 90 degrees).
    const BROOK_DRIFT: i32 = 4;
    /// Wetness (0..1000) a tile needs for a brook to rise there.
    const WET: u32 = 560;
    /// A river parts in two only from this flow up.
    const DIVERGE_FLOW: i64 = 16;
    /// How many steps a heading is held, at least and at most.
    const RUN: (u32, u32) = (4, 10);
    /// How fast a bank leaves its line, in thousandths of a tile per step:
    /// the tangent of 5.6 degrees, a quarter of a step of heading, so a
    /// river widens or narrows slowly.
    const BANK_SLOPE: i64 = 98;
    /// How far a bank may stand inside its line and outside it, in steps
    /// of `BANK_SLOPE`: about a tile in, six out.
    const BANK_REACH: (i64, i64) = (-10, 61);
    /// Steps a bank holds its line between two moves, at least and at most.
    const BANK_HOLD: (u32, u32) = (4, 12);
    /// Steps a move of a bank lasts, at least and at most.
    const BANK_SHIFT: (u32, u32) = (12, 40);
    /// Tries at the banks of one free stretch before leaving it straight.
    const BANK_TRIES: u32 = 30;
    /// A river never strays more than this many steps of heading from the
    /// heading it entered with (4 steps = 90 degrees).
    const MAX_DRIFT: i32 = 3;
    /// Chance, in percent, that a river which could have a ford has one:
    /// a ford is a place the terrain gives or not, never a promise.
    const FORD_CHANCE: u32 = 50;
    /// Length of a ford along the river, in steps.
    const FORD_STEPS: usize = 6;
    /// Steps over which a ford widens from the river's width, each end.
    const FORD_RAMP: usize = 2;
    /// Steps kept clear around a junction, a ford or a divergence.
    const CLEARANCE: usize = 6;
    /// Lengths a divergence may have, in steps.
    const DIVERGENCE_LENGTHS: [usize; 10] = [4, 6, 8, 12, 16, 24, 32, 48, 64, 90];
    /// Widths an island may have, in tiles.
    const ISLAND_WIDTHS: [i64; 7] = [1, 2, 3, 5, 8, 12, 20];

    /// The waters of a map of `width` by `height`, below `cliff_rows`.
    pub fn generate(width: u32, height: u32, cliff_rows: u32, seed: u64) -> Self {
        let tiles = (width * height) as usize;
        let mut rivers = Self {
            width,
            height,
            depth: vec![0; tiles],
            brook: vec![false; tiles],
            current: vec![0; tiles],
            owner: vec![0; tiles],
            step: vec![0; tiles],
            water_blocks: Vec::new(),
            #[cfg(test)]
            log: Vec::new(),
            #[cfg(test)]
            network: Vec::new(),
        };
        let mut rng = Lcg::new(seed.wrapping_add(purpose::RIVERS));
        let w = i64::from(width);
        let h = i64::from(height);
        let top = i64::from(cliff_rows);
        let between =
            |rng: &mut Lcg, (lo, hi): (u32, u32)| i64::from(lo + rng.next_below(hi - lo + 1));
        let mut network: Vec<River> = Vec::new();

        // The main river: from the cliff's foot, south to the bottom edge.
        let start = (
            w / 4 + i64::from(rng.next_below(u32::try_from(w / 2).expect("fits"))),
            top,
        );
        let (path, _) = rivers.trace(start, 0, false, &mut rng);
        let flow = between(&mut rng, Self::MAIN_FLOW);
        rivers.paint_provisional(&path, flow, 0);
        network.push(River::new(path, flow));
        rivers.ford(&mut network, 0, Self::FORD_CHANCE, &mut rng);

        // Tributaries from the edges, one or two, the first from a side
        // drawn at random and the next from the other: each heads for a
        // point of the main river downstream and ends where it meets water.
        let first = rng.next_below(2);
        for n in 0..between(
            &mut rng,
            Self::by_side(width, height, Self::TRIBUTARIES_TENTHS),
        ) {
            let side = if (u32::try_from(n).expect("few") + first).is_multiple_of(2) {
                0
            } else {
                w - 1
            };
            let y =
                top + h / 8 + i64::from(rng.next_below(u32::try_from(h * 3 / 4).expect("fits")));
            let flow = between(&mut rng, Self::TRIBUTARY_FLOW);
            if rivers.add_tributary(&mut network, (side, y), flow, &mut rng) {
                let last = network.len() - 1;
                rivers.ford(&mut network, last, Self::FORD_CHANCE, &mut rng);
            }
        }
        // While the brooks look for water, a coarse map of where it lies
        // (kept up to date as they are painted: water is only added then).
        rivers.mark_water_blocks();
        rivers.add_brooks(&mut network, seed, cliff_rows, &mut rng);
        rivers.water_blocks = Vec::new();

        // Flows: from the last traced water, which nothing joins, back to
        // the main river. Then the shape of every stretch, and what each
        // river does with its course.
        for i in (0..network.len()).rev() {
            let joined: i64 = network[i]
                .joins
                .iter()
                .map(|(_, j)| network[*j].end_flow)
                .sum();
            network[i].end_flow = network[i].entry_flow + joined;
            network[i].joins.sort_unstable();
            network[i].shapes = (0..=network[i].joins.len())
                .map(|_| {
                    if rng.next_below(3) == 0 {
                        Shape::Deep
                    } else {
                        Shape::Wide
                    }
                })
                .collect();
        }
        for i in 0..network.len() {
            // A ford reserved before the brooks came may be too deep now:
            // find it another place, if any.
            let widest = i64::from(Self::MAX_WIDTH);
            if network[i]
                .fords
                .iter()
                .any(|f| network[i].flow_at(f + Self::FORD_STEPS, &network) > widest)
            {
                network[i].fords.clear();
                rivers.ford(&mut network, i, 100, &mut rng);
            }
            let wanted = if i == 0 {
                1 + rng.next_below(2)
            } else {
                rng.next_below(2)
            };
            rivers.diverge(&mut network, i, wanted, &mut rng);
            rivers.breathe(&mut network, i, &mut rng);
        }

        #[cfg(test)]
        rivers.log_network(&network);

        rivers.paint_all(&network);
        rivers.reach_the_water(&network);
        rivers.close_brook_beds();
        rivers.round_banks();
        rivers.fill_currents();
        #[cfg(test)]
        {
            rivers.network = network;
        }
        rivers
    }

    /// Paints every water afresh with its real sections: rivers first,
    /// then brooks on the land left, then the fords.
    fn paint_all(&mut self, network: &[River]) {
        self.depth.fill(0);
        self.current.fill(0);
        self.brook.fill(false);
        self.owner.fill(0);
        self.step.fill(0);
        let bands: Vec<(usize, Vec<Band>)> = (0..network.len())
            .flat_map(|i| Self::bands(network, i).into_iter().map(move |b| (i, b)))
            .collect();
        for narrow in [false, true] {
            for (i, band) in bands.iter().filter(|(_, b)| b[0].brook == narrow) {
                self.paint(band, *i, false);
            }
        }
        for i in 0..network.len() {
            for band in Self::ford_bands(network, i) {
                self.paint(&band, i, true);
            }
        }
    }

    /// A brook stops where it met the water as first traced; the water
    /// painted in the end can be narrower there (its banks breathe), so
    /// a brook may stop short of it: each brook that does goes on along
    /// its last heading until it meets other water, a few steps at most.
    fn reach_the_water(&mut self, network: &[River]) {
        for (i, water) in network.iter().enumerate() {
            let Some(&last) = water.path.last() else {
                continue;
            };
            let flow = water.end_flow;
            let shape = water.shape_at(water.path.len() - 1);
            if Self::section(flow, shape).0 >= i64::from(Self::MIN_WIDTH) {
                continue;
            }
            let mine = u16::try_from(i + 1).expect("few rivers");
            let other_water = |rivers: &Self, x: i64, y: i64| {
                rivers
                    .index(x / 1000, y / 1000)
                    .is_some_and(|t| rivers.depth[t] > 0 && rivers.owner[t] != mine)
            };
            let h = HEADINGS[usize::from(last.heading)];
            let mut band = vec![Self::band_at(last, flow, shape, 0)];
            let mut at = last;
            for _ in 0..Self::BROOK_REACH {
                if other_water(self, at.x, at.y) {
                    break;
                }
                at = Step {
                    x: at.x + h.0,
                    y: at.y + h.1,
                    heading: last.heading,
                };
                band.push(Self::band_at(at, flow, shape, 0));
            }
            if band.len() > 1 {
                self.paint(&band, i, false);
            }
        }
    }

    /// A count given in tenths for every 256 tiles of a map's mean side,
    /// at least and at most, as whole counts for a map of `width ×
    /// height`, rounded, never zero.
    fn by_side(width: u32, height: u32, (lo, hi): (u32, u32)) -> (u32, u32) {
        let side = u64::midpoint(u64::from(width), u64::from(height));
        let count = |tenths: u32| {
            u32::try_from((u64::from(tenths) * side / 256 + 5) / 10)
                .unwrap_or(u32::MAX)
                .max(1)
        };
        (count(lo), count(hi))
    }

    /// The brooks: most enter by the map's edges (west, east, south) or
    /// rise at the cliff's foot; the others rise on wet ground well away
    /// from any water. Each runs to the nearest water, river or brook, at
    /// least `BROOK_MIN_STEPS` long, and keeps `BROOK_SPACING` from the
    /// other brooks but where it joins one.
    fn add_brooks(&mut self, network: &mut Vec<River>, seed: u64, cliff_rows: u32, rng: &mut Lcg) {
        let (width, height) = (self.width, self.height);
        let (wide, tall, top) = (i64::from(width), i64::from(height), i64::from(cliff_rows));
        let wetness = Groves::with_cell(width, height, seed ^ purpose::WETNESS, 32);
        let (lo, hi) = Self::by_side(width, height, Self::BROOKS_TENTHS);
        let count = lo + rng.next_below(hi - lo + 1);
        // Tiles within `BROOK_SPACING` of a brook already laid.
        let mut near_brook = vec![false; (width * height) as usize];
        let inside = |rng: &mut Lcg, lo: i64, hi: i64| {
            lo + i64::from(rng.next_below(u32::try_from((hi - lo).max(1)).expect("fits")))
        };
        for _ in 0..count {
            for _ in 0..300 {
                let start = if rng.next_below(100) < Self::EDGE_BROOKS {
                    match rng.next_below(4) {
                        0 => (0, inside(rng, top + 8, tall - 8)),
                        1 => (wide - 1, inside(rng, top + 8, tall - 8)),
                        2 => (inside(rng, 8, wide - 8), tall - 1),
                        _ => (inside(rng, 8, wide - 8), top + 1),
                    }
                } else {
                    let (x, y) = (inside(rng, 4, wide - 4), inside(rng, top + 4, tall - 4));
                    let wet = wetness.value(
                        u32::try_from(x).expect("inside"),
                        u32::try_from(y).expect("inside"),
                    );
                    if wet < Self::WET || !self.clear_of_water(x, y, Self::SPRING_CLEARANCE) {
                        continue;
                    }
                    (x, y)
                };
                let Some(at) = self.index(start.0, start.1) else {
                    continue;
                };
                if near_brook[at] || !self.clear_of_water(start.0, start.1, Self::BROOK_SPACING) {
                    continue;
                }
                let flow = i64::from(
                    Self::BROOK_FLOW.0
                        + rng.next_below(Self::BROOK_FLOW.1 - Self::BROOK_FLOW.0 + 1),
                );
                if self.add_brook(network, start, flow, &near_brook, top, rng) {
                    let path = &network[network.len() - 1].path;
                    for step in path.iter().step_by(2) {
                        let (x, y) = (step.x / 1000, step.y / 1000);
                        for ty in y - Self::BROOK_SPACING..=y + Self::BROOK_SPACING {
                            for tx in x - Self::BROOK_SPACING..=x + Self::BROOK_SPACING {
                                if let Some(t) = self.index(tx, ty) {
                                    near_brook[t] = true;
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    /// Traces a brook from `start` to the nearest water and adds it when
    /// it is long enough, short enough, ends in water where no ford is
    /// reserved, stays on the map below the cliff (`top`, its first row
    /// of ground), and keeps away from the brooks already
    /// laid (`near`) but over its last `BROOK_SPACING` steps. Tells whether it was added.
    fn add_brook(
        &mut self,
        network: &mut Vec<River>,
        start: (i64, i64),
        flow: i64,
        near: &[bool],
        top: i64,
        rng: &mut Lcg,
    ) -> bool {
        let reach = i64::try_from(Self::BROOK_STEPS).expect("small");
        let Some(aim) = self.nearest_water(start.0, start.1, reach) else {
            return false;
        };
        let heading = heading_towards(aim.0 - start.0 * 1000, aim.1 - start.1 * 1000);
        let (path, end) = self.trace(start, heading, true, rng);
        if !(Self::BROOK_MIN_STEPS..=Self::BROOK_STEPS).contains(&path.len()) {
            return false;
        }
        let End::Water(into, at) = end else {
            return false;
        };
        // Never out of the map or into the cliff and back: the stretch
        // outside (or under the rock) would leave a piece of brook alone.
        if path
            .iter()
            .any(|s| s.y / 1000 < top || self.index(s.x / 1000, s.y / 1000).is_none())
        {
            return false;
        }
        // One that enters by an edge (or rises at the cliff's foot) heads
        // into the valley, not along the edge.
        let (wide, tall) = (i64::from(self.width), i64::from(self.height));
        let off_edge = |s: &Step| {
            let (x, y) = (s.x / 1000, s.y / 1000);
            if start.0 == 0 {
                x
            } else if start.0 == wide - 1 {
                wide - 1 - x
            } else if start.1 == tall - 1 {
                tall - 1 - y
            } else if start.1 == top + 1 {
                y - top - 1
            } else {
                i64::MAX
            }
        };
        if path.iter().map(off_edge).max().unwrap_or(0) < Self::BROOK_INWARD {
            return false;
        }
        // Nor does it run along any edge: past its first steps, it keeps
        // clear of the map's edges and of the cliff (along an edge, half
        // of it would lie past the ground drawn).
        let edge_clear = |s: &Step| {
            let (x, y) = (s.x / 1000, s.y / 1000);
            x.min(wide - 1 - x).min(tall - 1 - y).min(y - top - 1) >= Self::BROOK_EDGE_CLEAR
        };
        if !path.iter().skip(Self::BROOK_LEAVE).all(edge_clear) {
            return false;
        }
        let spaced = usize::try_from(Self::BROOK_SPACING).expect("small");
        let crowded = path[..path.len().saturating_sub(spaced)]
            .iter()
            .any(|s| self.index(s.x / 1000, s.y / 1000).is_some_and(|i| near[i]));
        let reserved = network[into].fords.iter().any(|f| {
            (f.saturating_sub(Self::CLEARANCE)..f + Self::FORD_STEPS + Self::CLEARANCE)
                .contains(&at)
        });
        if crowded || reserved {
            return false;
        }
        let index = network.len();
        self.paint_provisional(&path, flow, index);
        network[into].joins.push((at, index));
        network.push(River::new(path, flow));
        true
    }

    #[cfg(test)]
    fn log_network(&mut self, network: &[River]) {
        for (i, r) in network.iter().enumerate() {
            let (a, z) = (r.path[0], r.path[r.path.len() - 1]);
            self.log.push(format!(
                "water {i}: ({},{})->({},{}) {} steps, flow {}->{}, shapes {:?}, joins {:?}, divergences {:?}, fords {:?}",
                a.x / 1000,
                a.y / 1000,
                z.x / 1000,
                z.y / 1000,
                r.path.len(),
                r.entry_flow,
                r.end_flow,
                r.shapes,
                r.joins,
                r.divergences
                    .iter()
                    .map(|d| (d.from, d.len, d.flows, d.offset / 1000, d.ramp))
                    .collect::<Vec<_>>(),
                r.fords
            ));
        }
    }

    /// The section of water carrying `flow` half-cubes in a stretch of
    /// that `shape`: width then depth, in tiles and half-metres. The
    /// width is about the square root of twice the flow when wide, of
    /// the flow when deep; the depth takes the rest. A brook (one or two
    /// wide) is at most `MAX_BROOK_DEPTH` deep, a river at most
    /// `MAX_DEPTH`: past that, the width takes everything. The section
    /// always holds the flow (up to the safeguard `MAX_RIVER_WIDTH`).
    pub fn section(flow: i64, shape: Shape) -> (i64, i64) {
        let widest = i64::from(Self::MAX_RIVER_WIDTH);
        let flow = flow.max(1);
        let ceil_div = |a: i64, b: i64| (a + b - 1) / b;
        // round(sqrt(k × flow)) = round(sqrt(4k × flow) / 2).
        let mut width = match shape {
            Shape::Wide => ((14 * flow).isqrt() + 1) / 2,
            Shape::Deep => ((4 * flow).isqrt() + 1) / 2,
        }
        .clamp(1, widest);
        // A river (three wide and more) is a metre deep at least: wider
        // only as far as its flow keeps that depth, never a ford all along
        // (fords are their own places, D36).
        if width >= i64::from(Self::MIN_WIDTH) {
            width = if flow < 2 * i64::from(Self::MIN_WIDTH) {
                // Too little water to be a river a metre deep: a brook.
                2
            } else {
                width.min(flow / 2)
            };
        }
        let mut depth = ceil_div(flow, width);
        if width < i64::from(Self::MIN_WIDTH) && depth > i64::from(Self::MAX_BROOK_DEPTH) {
            width = i64::from(Self::MIN_WIDTH);
            depth = ceil_div(flow, width);
        }
        let deepest = i64::from(Self::MAX_DEPTH);
        if depth > deepest {
            width = ceil_div(flow, deepest).clamp(1, widest);
            depth = ceil_div(flow, width).min(deepest);
        }
        (width, depth.max(1))
    }

    /// The widths a river carrying `flow` in a stretch of that `shape` may
    /// take when its banks breathe, in tiles, narrowest then widest: never
    /// a brook, never deeper than `MAX_DEPTH`, and up to `MAX_POOL_WIDTH`
    /// or half again its own width. Its own width always lies within.
    fn breathing_room(flow: i64, shape: Shape) -> (i64, i64) {
        let width = Self::section(flow, shape).0;
        let deepest = i64::from(Self::MAX_DEPTH);
        let narrowest = i64::from(Self::MIN_WIDTH)
            .max((flow + deepest - 1) / deepest)
            .min(width);
        let widest = i64::from(Self::MAX_POOL_WIDTH).max(width * 3 / 2);
        (narrowest, widest)
    }

    /// The section of a stretch whose two banks together stand `out`
    /// thousandths of a tile outside their lines (negative inside): the
    /// width, in thousandths, grows or shrinks by that much, and the depth,
    /// in half-metres, takes what the flow leaves (D36), but never less
    /// than a metre: a pool wider than its flow is slower water holding
    /// more than its flow, never a ford.
    fn breathing_section(flow: i64, shape: Shape, out: i64) -> (i64, i64) {
        let (width, depth) = Self::section(flow, shape);
        if out == 0 {
            return (width * 1000, depth);
        }
        let wide = width * 1000 + out;
        let tiles = ((wide + 500) / 1000).max(1);
        let depth = ((flow + tiles - 1) / tiles).clamp(2, i64::from(Self::MAX_DEPTH));
        (wide, depth)
    }

    /// What is at `(x, y)`: `None` on land, a brook, a ford half a metre
    /// deep, or water deeper.
    pub fn at(&self, x: u32, y: u32) -> Option<Terrain> {
        let i = (y * self.width + x) as usize;
        match self.depth[i] {
            0 => None,
            _ if self.brook[i] => Some(Terrain::Brook),
            1 => Some(Terrain::Ford),
            _ => Some(Terrain::Water),
        }
    }

    /// How deep the water is at `(x, y)`, in half-metres; 0 on land.
    pub fn depth_at(&self, x: u32, y: u32) -> u8 {
        self.depth[(y * self.width + x) as usize]
    }

    fn index(&self, x: i64, y: i64) -> Option<usize> {
        index_in(self.width, self.height, x, y)
    }

    /// Builds `water_blocks` from the water painted so far.
    fn mark_water_blocks(&mut self) {
        let wide = self.width.div_ceil(WATER_BLOCK) as usize;
        self.water_blocks = vec![false; wide * self.height.div_ceil(WATER_BLOCK) as usize];
        for i in 0..self.depth.len() {
            if self.depth[i] > 0 {
                self.mark_water(i);
            }
        }
    }

    /// Notes that tile `i` holds water, while `water_blocks` is kept.
    fn mark_water(&mut self, i: usize) {
        if self.water_blocks.is_empty() {
            return;
        }
        let width = self.width as usize;
        let block = WATER_BLOCK as usize;
        let wide = self.width.div_ceil(WATER_BLOCK) as usize;
        self.water_blocks[(i / width / block) * wide + (i % width) / block] = true;
    }

    /// Whether the square of `reach` tiles around `(x, y)` may hold water:
    /// false only when `water_blocks` is kept and says none of its blocks
    /// does.
    fn may_hold_water(&self, x: i64, y: i64, reach: i64) -> bool {
        if self.water_blocks.is_empty() {
            return true;
        }
        let block = i64::from(WATER_BLOCK);
        let wide = i64::from(self.width.div_ceil(WATER_BLOCK));
        let high = i64::from(self.height.div_ceil(WATER_BLOCK));
        let (x0, x1) = (
            (x - reach).max(0) / block,
            ((x + reach) / block).min(wide - 1),
        );
        let (y0, y1) = (
            (y - reach).max(0) / block,
            ((y + reach) / block).min(high - 1),
        );
        (y0..=y1).any(|by| {
            (x0..=x1).any(|bx| self.water_blocks[usize::try_from(by * wide + bx).expect("inside")])
        })
    }

    /// No water within `reach` tiles of `(x, y)`.
    fn clear_of_water(&self, x: i64, y: i64, reach: i64) -> bool {
        (y - reach..=y + reach).all(|ty| {
            (x - reach..=x + reach).all(|tx| self.index(tx, ty).is_none_or(|i| self.depth[i] == 0))
        })
    }

    /// The water tile closest to `(x, y)` within `reach` tiles, in
    /// thousandths: searched ring by ring outwards, so it costs what lies
    /// around the point, not the whole map. Ties go to the first found,
    /// row by row.
    fn nearest_water(&self, x: i64, y: i64, reach: i64) -> Option<(i64, i64)> {
        // Most rises lie far from any water: say so without walking the
        // whole square ring by ring.
        if !self.may_hold_water(x, y, reach) {
            return None;
        }
        let mut best: Option<(i64, (i64, i64))> = None;
        for r in 0..=reach {
            // Every tile of ring r is at least r away: past the best found,
            // nothing nearer remains.
            if best.is_some_and(|(d, _)| d < r * r) {
                break;
            }
            for ty in y - r..=y + r {
                let edge_row = ty == y - r || ty == y + r;
                let step = if edge_row {
                    1
                } else {
                    usize::try_from(2 * r).expect("small").max(1)
                };
                for tx in (x - r..=x + r).step_by(step) {
                    let Some(i) = self.index(tx, ty) else {
                        continue;
                    };
                    if self.depth[i] == 0 {
                        continue;
                    }
                    let d = (tx - x) * (tx - x) + (ty - y) * (ty - y);
                    if best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, (tx, ty)));
                    }
                }
            }
        }
        best.map(|(_, (tx, ty))| (tx * 1000, ty * 1000))
    }

    /// Traces a tributary from `start` towards a point of the main river
    /// downstream, and records where it joins, never where a ford is
    /// reserved. Tells whether the water was added. (Brooks: `add_brook`.)
    fn add_tributary(
        &mut self,
        network: &mut Vec<River>,
        start: (i64, i64),
        flow: i64,
        rng: &mut Lcg,
    ) -> bool {
        if self
            .index(start.0, start.1)
            .is_none_or(|i| self.depth[i] != 0)
        {
            return false;
        }
        let Some(aim) = self.downstream_aim(&network[0].path, start, rng) else {
            return false;
        };
        let heading = heading_towards(aim.0 - start.0 * 1000, aim.1 - start.1 * 1000);
        let (path, end) = self.trace(start, heading, false, rng);
        if path.len() < Self::CLEARANCE {
            return false;
        }
        let join = match end {
            End::Water(into, at) => {
                let reserved = network[into].fords.iter().any(|f| {
                    (f.saturating_sub(Self::CLEARANCE)..f + Self::FORD_STEPS + Self::CLEARANCE)
                        .contains(&at)
                });
                if reserved {
                    return false;
                }
                Some((into, at))
            }
            End::Edge => None,
        };
        let index = network.len();
        self.paint_provisional(&path, flow, index);
        if let Some((into, at)) = join {
            network[into].joins.push((at, index));
        }
        network.push(River::new(path, flow));
        true
    }

    /// A point of the main river well downstream of `start`, so that the
    /// junction points the way the water goes; none when the start is
    /// too far down already.
    fn downstream_aim(
        &self,
        main: &[Step],
        start: (i64, i64),
        rng: &mut Lcg,
    ) -> Option<(i64, i64)> {
        let downstream: Vec<&Step> = main
            .iter()
            .skip(main.len() / 5)
            .take(main.len() * 3 / 5)
            .filter(|s| s.y >= (start.1 + i64::from(self.height) / 8) * 1000)
            .collect();
        if downstream.is_empty() {
            return None;
        }
        let pick = usize::try_from(rng.next_below(u32::try_from(downstream.len()).expect("fits")))
            .expect("fits");
        Some((downstream[pick].x, downstream[pick].y))
    }

    /// The centre path of one water from `start`: steps of one tile, each
    /// with the heading it was taken with, until the map's edge or the
    /// water of another. A `brook` meanders: it holds its headings for
    /// fewer steps and may stray further.
    fn trace(
        &self,
        start: (i64, i64),
        heading: u8,
        brook: bool,
        rng: &mut Lcg,
    ) -> (Vec<Step>, End) {
        let ((run_min, run_max), max_drift) = if brook {
            (Self::BROOK_RUN, Self::BROOK_DRIFT)
        } else {
            (Self::RUN, Self::MAX_DRIFT)
        };
        let run = |rng: &mut Lcg| run_min + rng.next_below(run_max - run_min + 1);
        let base = i32::from(heading);
        let mut dir = base;
        let (mut x, mut y) = (start.0 * 1000, start.1 * 1000);
        let mut run_dir = run(rng);
        let mut path = Vec::new();
        let margin = i64::from(Self::MAX_WIDTH) * 1000;
        let limit = (
            i64::from(self.width) * 1000 + margin,
            i64::from(self.height) * 1000 + margin,
        );
        while x >= -margin
            && y >= -margin
            && x <= limit.0
            && y <= limit.1
            && path.len() < 4 * self.width as usize
        {
            if run_dir == 0 {
                // A bend of one step either way; once strayed, half the
                // bends come back towards the heading it entered with.
                let drift = dir - base;
                let turn = if drift != 0 && rng.next_below(2) == 0 {
                    -drift.signum()
                } else {
                    i32::try_from(rng.next_below(3)).expect("small") - 1
                };
                dir += if (drift + turn).abs() > max_drift {
                    -turn
                } else {
                    turn
                };
                run_dir = run(rng);
            }
            let heading = u8::try_from(dir.rem_euclid(16)).expect("small");
            path.push(Step { x, y, heading });
            if let Some(i) = self
                .index(x / 1000, y / 1000)
                .filter(|i| self.owner[*i] != 0)
            {
                let into = usize::from(self.owner[i]) - 1;
                let at = usize::try_from(self.step[i]).expect("fits");
                return (path, End::Water(into, at));
            }
            let h = HEADINGS[usize::from(heading)];
            x += h.0;
            y += h.1;
            run_dir -= 1;
        }
        (path, End::Edge)
    }

    /// Paints a path with the wide section of its entry flow, so later
    /// waters know where it runs.
    fn paint_provisional(&mut self, path: &[Step], flow: i64, river: usize) {
        let band: Vec<Band> = path
            .iter()
            .map(|s| Self::band_at(*s, flow, Shape::Wide, 0))
            .collect();
        self.paint(&band, river, false);
    }

    /// The band point of step `s` carrying `flow`, shifted `offset`
    /// thousandths to the right of the current.
    fn band_at(s: Step, flow: i64, shape: Shape, offset: i64) -> Band {
        let (width, depth) = Self::section(flow, shape);
        let h = HEADINGS[usize::from(s.heading)];
        Band {
            x: s.x - h.1 * offset / 1000,
            y: s.y + h.0 * offset / 1000,
            // Just under the half-width, so a band covers exactly its
            // width in tiles when the path runs along tile centres.
            half_width: width * 500 - 1,
            depth: u8::try_from(depth).expect("shallow"),
            brook: width < i64::from(Self::MIN_WIDTH),
            heading: s.heading,
            speed: Self::speed(flow, width * 1000, depth),
        }
    }

    /// How fast water carrying `flow` half-cubes runs through a section
    /// `width` thousandths of a tile wide and `depth` half-metres deep, in
    /// centimetres a second: `BASE_SPEED` where the section just holds the
    /// flow, slower where it holds more (a pool), faster where it holds
    /// less.
    fn speed(flow: i64, width: i64, depth: i64) -> u16 {
        let held = (width * depth.max(1)).max(1);
        let speed = i64::from(Self::BASE_SPEED) * flow * 1000 / held;
        u16::try_from(speed.clamp(i64::from(Self::SPEED.0), i64::from(Self::SPEED.1)))
            .expect("bounded")
    }

    /// Lets the banks of water `i` breathe where it is free: each bank, on
    /// its own, holds its line, then turns a quarter of a step of heading out or in
    /// for a few steps, within its reach and the widths its flow allows.
    /// Calm within `CLEARANCE` of its ends, of the map's edges, of a
    /// junction, a ford or a divergence, and on brooks; a bank never bends
    /// within a step of a bend of the centre path, so it never turns by
    /// two steps at once.
    fn breathe(&self, network: &mut [River], i: usize, rng: &mut Lcg) {
        let river = &network[i];
        let len = river.path.len();
        let calm = self.calm_steps(river, network);
        let bend = |step: usize| {
            step > 0 && step < len && river.path[step].heading != river.path[step - 1].heading
        };
        let mut banks = vec![(0, 0); len];
        let mut step = 0;
        while step < len {
            if calm[step] {
                step += 1;
                continue;
            }
            let first = step;
            while step < len && !calm[step] {
                step += 1;
            }
            // Nodes first - 1 (calm) to step (calm), as j = 0..=nodes.
            let nodes = step - first + 1;
            let flow = river.flow_at(first, network);
            let shape = river.shape_at(first);
            let width = Self::section(flow, shape).0 * 1000;
            let (narrowest, widest) = Self::breathing_room(flow, shape);
            let fits = |total: i64| {
                let tiles = (width + total * Self::BANK_SLOPE + 500) / 1000;
                (narrowest..=widest).contains(&tiles)
            };
            let near_bend = |j: usize| {
                let node = first - 1 + j;
                bend(node.saturating_sub(1)) || bend(node) || bend(node + 1)
            };
            let (inside, outside) = Self::BANK_REACH;
            for attempt in 0..2 * Self::BANK_TRIES {
                // First each bank on its own, the second fitting around
                // the first; then, if that keeps failing, the first bank
                // leaves the second its whole reach.
                let first_fits =
                    |k: i64| attempt < Self::BANK_TRIES || (fits(k + inside) && fits(k + outside));
                let Some(one) = Self::bank_profile(nodes, &near_bend, |_, k| first_fits(k), rng)
                else {
                    continue;
                };
                let Some(other) =
                    Self::bank_profile(nodes, &near_bend, |j, k| fits(one[j] + k), rng)
                else {
                    continue;
                };
                if (0..=nodes).all(|j| fits(one[j] + other[j])) {
                    let swap = rng.next_below(2) == 0;
                    for j in 1..nodes {
                        banks[first - 1 + j] = if swap {
                            (other[j], one[j])
                        } else {
                            (one[j], other[j])
                        };
                    }
                    break;
                }
            }
            if banks[first..step].iter().all(|b| *b == (0, 0)) {
                // No room to breathe on its own (a narrow river whose
                // flow allows no other width): the river slides sideways
                // instead, one bank out as the other comes in, its width
                // kept.
                for _ in 0..Self::BANK_TRIES {
                    if let Some(left) =
                        Self::bank_profile(nodes, &near_bend, |_, k| k.abs() <= -inside, rng)
                    {
                        for j in 1..nodes {
                            banks[first - 1 + j] = (left[j], -left[j]);
                        }
                        break;
                    }
                }
            }
        }
        network[i].banks = banks;
    }

    /// The steps of `river` where its banks hold their lines: within
    /// `CLEARANCE` of its ends, of the map's edges, of a junction, a ford
    /// or a divergence, and where it is a brook.
    fn calm_steps(&self, river: &River, network: &[River]) -> Vec<bool> {
        let len = river.path.len();
        let clear = Self::CLEARANCE;
        let margin = i64::try_from(clear).expect("small") * 1000;
        let (right_edge, bottom_edge) =
            (i64::from(self.width) * 1000, i64::from(self.height) * 1000);
        let mut calm: Vec<bool> = (0..len)
            .map(|step| {
                let at = river.path[step];
                step < clear
                    || step + clear >= len
                    || at.x < margin
                    || at.y < margin
                    || at.x >= right_edge - margin
                    || at.y >= bottom_edge - margin
                    || Self::section(river.flow_at(step, network), river.shape_at(step)).0
                        < i64::from(Self::MIN_WIDTH)
            })
            .collect();
        let mut quiet = |from: usize, to: usize| {
            let end = (to + clear).min(len);
            calm[from.saturating_sub(clear)..end].fill(true);
        };
        for (at, _) in &river.joins {
            quiet(*at, *at + 1);
        }
        for f in &river.fords {
            quiet(
                f.saturating_sub(Self::FORD_RAMP),
                f + Self::FORD_STEPS + Self::FORD_RAMP,
            );
        }
        for d in &river.divergences {
            quiet(d.from, d.from + d.len + 1);
        }
        calm
    }

    /// One bank over the nodes `0..=n` of a free stretch, standing on its
    /// line at both ends: how far out it stands at each, in steps of
    /// `BANK_SLOPE`, or none if this draw breaks a rule. `fits` tells
    /// whether the bank may stand `k` out at node `j`.
    fn bank_profile(
        n: usize,
        near_bend: &impl Fn(usize) -> bool,
        fits: impl Fn(usize, i64) -> bool,
        rng: &mut Lcg,
    ) -> Option<Vec<i64>> {
        let between = |rng: &mut Lcg, (lo, hi): (u32, u32)| lo + rng.next_below(hi - lo + 1);
        let (inside, outside) = Self::BANK_REACH;
        let (shortest, longest) = Self::BANK_SHIFT;
        // Steps of hold kept in hand before the way back, so that a move
        // back can wait for the centre to finish a bend.
        let spare = 6;
        let mut k = vec![0; n + 1];
        let (mut j, mut out) = (0, 0_i64);
        let mut hold = between(rng, Self::BANK_HOLD);
        while j < n {
            let back = usize::try_from(out.unsigned_abs()).expect("small");
            if n - j < back {
                return None;
            }
            let must_return = out != 0 && n - j <= back + spare;
            if (hold == 0 || must_return) && !near_bend(j) {
                // A move from node j: back towards the line half the time
                // once off it, else out or in; of a length that fits the
                // reach and the width at every step, ends clear of the
                // centre's bends, and leaves room to come back.
                let dir = if must_return || (out != 0 && rng.next_below(2) == 0) {
                    -out.signum()
                } else if rng.next_below(2) == 0 {
                    1
                } else {
                    -1
                };
                let span = (longest - shortest + 1) as usize;
                let first = rng.next_below(longest - shortest + 1) as usize;
                let lengths: Vec<usize> = if must_return {
                    vec![back]
                } else {
                    (0..span)
                        .map(|i| shortest as usize + (first + i) % span)
                        .collect()
                };
                let chosen = lengths.into_iter().find(|&steps| {
                    let end = j + steps;
                    let last = out + dir * i64::try_from(steps).expect("small");
                    end <= n
                        && (inside..=outside).contains(&last)
                        && !near_bend(end)
                        && (must_return
                            || n - end
                                >= usize::try_from(last.unsigned_abs()).expect("small") + spare)
                        && (1..=steps)
                            .all(|i| fits(j + i, out + dir * i64::try_from(i).expect("small")))
                });
                if let Some(steps) = chosen {
                    for _ in 0..steps {
                        out += dir;
                        j += 1;
                        k[j] = out;
                    }
                    hold = between(rng, Self::BANK_HOLD);
                    continue;
                }
            }
            hold = hold.saturating_sub(1);
            j += 1;
            k[j] = out;
        }
        if k[n] != 0 {
            return None;
        }
        // No bend of the bank within a step of a bend of the centre.
        let slope = |j: usize| if j < n { k[j + 1] - k[j] } else { 0 };
        let kinked = (0..=n).any(|j| {
            let before = if j > 0 { slope(j - 1) } else { 0 };
            before != slope(j) && near_bend(j)
        });
        (!kinked).then_some(k)
    }

    /// Chooses up to `wanted` divergences along river `i`, where it is
    /// strong enough and clear of junctions, fords and other divergences.
    fn diverge(&self, network: &mut [River], i: usize, wanted: u32, rng: &mut Lcg) {
        let len = network[i].path.len();
        let mut tries = 0;
        while u32::try_from(network[i].divergences.len()).expect("few") < wanted && tries < 80 {
            tries += 1;
            if len <= 4 * Self::CLEARANCE {
                return;
            }
            let span = Self::DIVERGENCE_LENGTHS[usize::try_from(
                rng.next_below(u32::try_from(Self::DIVERGENCE_LENGTHS.len()).expect("few")),
            )
            .expect("few")];
            let room = len - 2 * Self::CLEARANCE;
            if span + Self::CLEARANCE > room {
                continue;
            }
            let from = Self::CLEARANCE
                + usize::try_from(rng.next_below(u32::try_from(room - span).expect("fits")))
                    .expect("fits");
            if !Self::stretch_is_clear(&network[i], network, from, span)
                || !self.inside(&network[i], from, span, Self::MAX_WIDTH)
            {
                continue;
            }
            let flow = network[i].flow_at(from, network);
            if flow < Self::DIVERGE_FLOW {
                continue;
            }
            let shape = network[i].shape_at(from);
            let percent = 55 + i64::from(rng.next_below(31));
            let big = flow * percent / 100;
            let small = flow - big;
            let flows = if rng.next_below(2) == 0 {
                (big, small)
            } else {
                (small, big)
            };
            let (left, right) = (
                Self::section(flows.0, shape).0,
                Self::section(flows.1, shape).0,
            );
            if left.min(right) < i64::from(Self::MIN_WIDTH) {
                // An arm is a river, not a brook.
                continue;
            }
            let island = Self::ISLAND_WIDTHS[usize::try_from(
                rng.next_below(u32::try_from(Self::ISLAND_WIDTHS.len()).expect("few")),
            )
            .expect("few")];
            // The arms' inner banks must leave the island's width between
            // them; the banks are rounded afterwards, and the smaller arm,
            // if it shoals into a ford, spreads to its flow in width.
            let shoal = if small <= i64::from(Self::MAX_WIDTH) {
                small - Self::section(small, shape).0
            } else {
                0
            };
            let offset = (left + right + shoal) * 250 + (island + 1) * 500;
            // The arms part at 35 degrees or less.
            let ramp = usize::try_from(offset / 700).expect("fits").max(1);
            if span < 3 * ramp {
                // Too short to part so far and come back gently.
                continue;
            }
            network[i].divergences.push(Divergence {
                from,
                len: span,
                flows,
                offset,
                ramp,
            });
        }
    }

    /// With `chance` percent, spreads river `i` into a ford somewhere its
    /// flow allows it, clear of everything else: at most one per river,
    /// chosen among every place that can take one. A brook needs none:
    /// it is walked through.
    fn ford(&self, network: &mut [River], i: usize, chance: u32, rng: &mut Lcg) {
        if rng.next_below(100) >= chance {
            return;
        }
        let len = network[i].path.len();
        let widest = i64::from(Self::MAX_WIDTH);
        let span = Self::FORD_STEPS + 2 * Self::FORD_RAMP;
        if len <= 2 * Self::CLEARANCE + span {
            return;
        }
        let river = &network[i];
        let places: Vec<usize> = (Self::CLEARANCE..len - Self::CLEARANCE - span)
            .filter(|from| {
                let flow = river.flow_at(from + span, network);
                flow <= widest
                    && Self::section(flow, river.shape_at(from + span)).0
                        >= i64::from(Self::MIN_WIDTH)
                    && Self::stretch_is_clear(river, network, *from, span)
                    && self.inside(river, *from, span, Self::MIN_WIDTH)
            })
            .collect();
        if places.is_empty() {
            return;
        }
        let pick = usize::try_from(rng.next_below(u32::try_from(places.len()).expect("fits")))
            .expect("fits");
        network[i].fords.push(places[pick] + Self::FORD_RAMP);
    }

    /// The steps `from..=from + len` all lie inside the map, `margin`
    /// tiles from its edges.
    fn inside(&self, river: &River, from: usize, len: usize, margin: u32) -> bool {
        let margin = i64::from(margin) * 1000;
        let (w, h) = (i64::from(self.width) * 1000, i64::from(self.height) * 1000);
        river.path[from..=(from + len).min(river.path.len() - 1)]
            .iter()
            .all(|s| s.x >= margin && s.y >= margin && s.x < w - margin && s.y < h - margin)
    }

    /// No junction, ford or divergence within `CLEARANCE` of the steps
    /// `from..from + len`; a brook's junction only needs a tile.
    fn stretch_is_clear(river: &River, network: &[River], from: usize, len: usize) -> bool {
        let lo = from.saturating_sub(Self::CLEARANCE);
        let hi = from + len + Self::CLEARANCE;
        let busy = |a: usize, b: usize| a < hi && b > lo;
        let narrow = |j: usize| {
            Self::section(network[j].end_flow, Shape::Wide).0 < i64::from(Self::MIN_WIDTH)
        };
        !river.joins.iter().any(|(at, j)| {
            if narrow(*j) {
                (from.saturating_sub(1)..from + len + 1).contains(at)
            } else {
                busy(*at, at + 1)
            }
        }) && !river.fords.iter().any(|f| {
            busy(
                f.saturating_sub(Self::FORD_RAMP),
                f + Self::FORD_STEPS + Self::FORD_RAMP,
            )
        }) && !river
            .divergences
            .iter()
            .any(|d| busy(d.from, d.from + d.len))
    }

    /// The bands of water `i`: the single channel between divergences,
    /// two arms through each.
    fn bands(network: &[River], i: usize) -> Vec<Vec<Band>> {
        let river = &network[i];
        let at = |step: usize, flow: i64, offset: i64| {
            let shape = river.shape_at(step);
            let (left, right) = river.banks.get(step).copied().unwrap_or((0, 0));
            let shift = (right - left) * Self::BANK_SLOPE / 2;
            let mut band = Self::band_at(river.path[step], flow, shape, offset + shift);
            if left + right != 0 {
                let (wide, depth) =
                    Self::breathing_section(flow, shape, (left + right) * Self::BANK_SLOPE);
                band.half_width = wide / 2 - 1;
                band.depth = u8::try_from(depth).expect("shallow");
                band.speed = Self::speed(flow, wide, depth);
            }
            band
        };
        let mut divergences: Vec<&Divergence> = river.divergences.iter().collect();
        divergences.sort_by_key(|d| d.from);
        let mut out = Vec::new();
        let mut channel: Vec<Band> = Vec::new();
        let mut step = 0;
        for d in divergences {
            while step <= d.from {
                channel.push(at(step, river.flow_at(step, network), 0));
                step += 1;
            }
            out.push(std::mem::take(&mut channel));
            let (mut left, mut right) = (Vec::new(), Vec::new());
            for t in 0..=d.len {
                let rise = t.min(d.len - t).min(d.ramp);
                let offset = d.offset * i64::try_from(rise).expect("fits")
                    / i64::try_from(d.ramp).expect("fits");
                left.push(at(d.from + t, d.flows.0, -offset));
                right.push(at(d.from + t, d.flows.1, offset));
            }
            out.push(left);
            out.push(right);
            step = d.from + d.len;
        }
        while step < river.path.len() {
            channel.push(at(step, river.flow_at(step, network), 0));
            step += 1;
        }
        out.push(channel);
        out.into_iter().filter(|b| b.len() >= 2).collect()
    }

    /// The bands of river `i` that are fords: its own ford, spread to
    /// half a metre right across, and the shoaling smaller arm of each
    /// divergence.
    fn ford_bands(network: &[River], i: usize) -> Vec<Vec<Band>> {
        let river = &network[i];
        let widest = i64::from(Self::MAX_WIDTH);
        let shoal = |step: usize, offset: i64, width: i64| {
            let s = river.path[step];
            let h = HEADINGS[usize::from(s.heading)];
            Band {
                x: s.x - h.1 * offset / 1000,
                y: s.y + h.0 * offset / 1000,
                half_width: width * 500 - 1,
                depth: 1,
                brook: false,
                heading: s.heading,
                speed: 0,
            }
        };
        // A ford is a lens: the river's own width at each end, the flow's
        // width (half a metre deep right across) in the middle.
        let lens = |from: usize, flow: i64, shape: Shape, offset: i64| -> Vec<Band> {
            let spread = flow.max(i64::from(Self::MIN_WIDTH));
            let own = Self::section(flow, shape).0;
            let ramp = Self::FORD_RAMP;
            (0..Self::FORD_STEPS + 2 * ramp)
                .map(|t| {
                    let width = if t < ramp || t >= ramp + Self::FORD_STEPS {
                        own
                    } else {
                        spread
                    };
                    let mut band = shoal(from + t - ramp, offset, width);
                    // Spread thin over the stones, the water runs faster.
                    band.speed = Self::speed(flow, width * 1000, 1) * 3 / 2;
                    band
                })
                .collect()
        };
        let mut out = Vec::new();
        for from in &river.fords {
            let flow = river.flow_at(from + Self::FORD_STEPS, network);
            out.push(lens(*from, flow, river.shape_at(*from), 0));
        }
        for d in &river.divergences {
            let (flow, sign) = if d.flows.0 <= d.flows.1 {
                (d.flows.0, -1)
            } else {
                (d.flows.1, 1)
            };
            if flow > widest || d.len < 2 * d.ramp + Self::FORD_STEPS + 2 * Self::FORD_RAMP {
                continue;
            }
            let middle = d.from + d.len / 2 - Self::FORD_STEPS / 2;
            out.push(lens(middle, flow, river.shape_at(d.from), sign * d.offset));
        }
        out
    }

    /// Paints water under a band: every tile whose centre is within the
    /// local half-width, and for a brook the tile each point of the path
    /// is in. A river's deeper water wins over shallower and
    /// over a brook; a brook only takes land. A tile keeps the first
    /// water that reached it. A ford (`shoal`) sets half a metre right
    /// across, only on land and on its own river's water.
    fn paint(&mut self, band: &[Band], river: usize, shoal: bool) {
        let mine = u16::try_from(river + 1).expect("few rivers");
        let own = |p: &Band| ((p.x + 500).div_euclid(1000), (p.y + 500).div_euclid(1000));
        for (n, pair) in band.windows(2).enumerate() {
            let (a, b) = (&pair[0], &pair[1]);
            let reach = a.half_width.max(b.half_width) / 1000 + 1;
            let (x0, x1) = ((a.x.min(b.x)) / 1000 - reach, (a.x.max(b.x)) / 1000 + reach);
            let (y0, y1) = ((a.y.min(b.y)) / 1000 - reach, (a.y.max(b.y)) / 1000 + reach);
            for ty in y0..=y1 {
                for tx in x0..=x1 {
                    let Some(i) = self.index(tx, ty) else {
                        continue;
                    };
                    let (px, py) = (tx * 1000, ty * 1000);
                    let (vx, vy) = (b.x - a.x, b.y - a.y);
                    let len2 = (vx * vx + vy * vy).max(1);
                    let t = (((px - a.x) * vx + (py - a.y) * vy) * 1000 / len2).clamp(0, 1000);
                    let (cx, cy) = (a.x + vx * t / 1000, a.y + vy * t / 1000);
                    let d2 = (px - cx) * (px - cx) + (py - cy) * (py - cy);
                    let hw = a.half_width + (b.half_width - a.half_width) * t / 1000;
                    // A brook also takes the tile each point of its path
                    // is in (the nearest centre): a brook one tile wide
                    // on a slant would otherwise leave the path's own
                    // tile dry now and then, its water only touching by
                    // the corners.
                    let on_path = a.brook && ((tx, ty) == own(a) || (tx, ty) == own(b));
                    if d2 > hw * hw && !on_path {
                        continue;
                    }
                    let point = if t < 500 { a } else { b };
                    let current = current_code(point.heading, point.speed);
                    if shoal {
                        if self.owner[i] == 0 || self.owner[i] == mine {
                            self.depth[i] = 1;
                            self.brook[i] = false;
                            self.owner[i] = mine;
                            self.current[i] = current;
                        }
                    } else if point.brook {
                        if self.depth[i] == 0 {
                            self.depth[i] = point.depth;
                            self.brook[i] = true;
                            self.owner[i] = mine;
                            self.step[i] = u32::try_from(n).expect("fits");
                            self.current[i] = current;
                        }
                    } else {
                        if self.brook[i] || self.depth[i] < point.depth {
                            self.depth[i] = point.depth;
                        }
                        if self.owner[i] == 0 || self.brook[i] {
                            self.current[i] = current;
                        }
                        self.brook[i] = false;
                        if self.owner[i] == 0 {
                            self.owner[i] = mine;
                            self.step[i] = u32::try_from(n).expect("fits");
                        }
                    }
                    if self.depth[i] > 0 {
                        self.mark_water(i);
                    }
                }
            }
        }
    }

    /// Every tile of water runs: a tile the rounding of the banks made
    /// water takes the current of a neighbour; land has none.
    fn fill_currents(&mut self) {
        let (w, h) = (i64::from(self.width), i64::from(self.height));
        for _ in 0..3 {
            let before = self.current.clone();
            for y in 0..h {
                for x in 0..w {
                    let i = self.index(x, y).expect("inside");
                    if self.depth[i] == 0 {
                        self.current[i] = 0;
                        continue;
                    }
                    if before[i] != 0 {
                        continue;
                    }
                    let near = [
                        (0, -1),
                        (1, 0),
                        (0, 1),
                        (-1, 0),
                        (1, 1),
                        (-1, -1),
                        (1, -1),
                        (-1, 1),
                    ]
                    .iter()
                    .filter_map(|(dx, dy)| self.index(x + dx, y + dy))
                    .map(|j| before[j])
                    .find(|c| *c != 0);
                    if let Some(c) = near {
                        self.current[i] = c;
                    }
                }
            }
        }
    }

    /// Where the water runs at `(x, y)`: its heading (as the centre paths',
    /// 0 to 15) and speed in centimetres a second; `None` on land.
    pub fn current_at(&self, x: u32, y: u32) -> Option<(u8, u16)> {
        decode_current(self.current[(y * self.width + x) as usize])
    }

    /// Closes the notches in the brooks' beds: a brook is painted on the
    /// tiles whose centre lies within its half-width of its centre line,
    /// and where a straight run sits just past a row's centres between
    /// two slanting runs that reach it, that row keeps one land tile
    /// with brook on both sides, grass and plants in the middle of the
    /// water (11 November 2026). Such a tile, brook on its left and
    /// right or above and below, becomes brook, as deep as its
    /// neighbour; a few passes, since closing one may expose another.
    /// Its current comes with `fill_currents`.
    fn close_brook_beds(&mut self) {
        let (width, height) = (i64::from(self.width), i64::from(self.height));
        for _ in 0..4 {
            let mut closed = false;
            for y in 0..height {
                for x in 0..width {
                    let i = self.index(x, y).expect("inside");
                    if self.depth[i] != 0 {
                        continue;
                    }
                    let brook = |rivers: &Self, x: i64, y: i64| {
                        rivers.index(x, y).filter(|j| rivers.brook[*j])
                    };
                    let from = match (brook(self, x - 1, y), brook(self, x + 1, y)) {
                        (Some(j), Some(_)) => Some(j),
                        _ => match (brook(self, x, y - 1), brook(self, x, y + 1)) {
                            (Some(j), Some(_)) => Some(j),
                            _ => None,
                        },
                    };
                    if let Some(j) = from {
                        self.depth[i] = self.depth[j];
                        self.brook[i] = true;
                        self.owner[i] = self.owner[j];
                        self.step[i] = self.step[j];
                        closed = true;
                    }
                }
            }
            if !closed {
                break;
            }
        }
    }

    /// Rounds the banks of the rivers: a land tile with river water on
    /// three of its four sides (the tip of a tongue at a junction, a
    /// pimple on a bank) becomes water, a river tile with land on three
    /// sides becomes land. Brooks are left alone. Two passes, so thin
    /// tongues erode but nothing drifts further.
    fn round_banks(&mut self) {
        for _ in 0..2 {
            let before: Vec<u8> = self
                .depth
                .iter()
                .zip(&self.brook)
                .map(|(d, brook)| if *brook { 0 } else { *d })
                .collect();
            let (width, height) = (self.width, self.height);
            let depth_at = |x: i64, y: i64| index_in(width, height, x, y).map_or(1, |i| before[i]);
            for y in 0..i64::from(height) {
                for x in 0..i64::from(width) {
                    let sides = [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)];
                    let (mut wet, mut shallowest) = (0, u8::MAX);
                    for (sx, sy) in sides {
                        let d = depth_at(sx, sy);
                        if d > 0 {
                            wet += 1;
                            shallowest = shallowest.min(d);
                        }
                    }
                    let i = self.index(x, y).expect("inside");
                    if self.brook[i] {
                        continue;
                    }
                    if before[i] == 0 && self.depth[i] == 0 && wet >= 3 {
                        self.depth[i] = shallowest;
                    } else if before[i] > 0 && wet <= 1 {
                        self.depth[i] = 0;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn a_map_is_written_as_its_spec_and_read_back_whole() {
        let map = Map::valley(256, 256, 3);
        let written = serde_json::to_value(&map).unwrap();
        assert_eq!(written, serde_json::to_value(map.spec()).unwrap());
        let read: Map = serde_json::from_value(written).unwrap();
        assert_eq!(read, map);
    }

    /// Every passable terrain at the reference cost.
    fn walk(terrain: Terrain) -> Option<u32> {
        terrain.is_passable().then_some(STRAIGHT_COST)
    }

    #[test]
    fn positions_outside_the_map_are_none() {
        let map = Map::filled(4, 3, Terrain::Grass);
        assert_eq!(map.get(Pos::new(0, 0)), Some(Terrain::Grass));
        assert_eq!(map.get(Pos::new(3, 2)), Some(Terrain::Grass));
        assert_eq!(map.get(Pos::new(4, 0)), None);
        assert_eq!(map.get(Pos::new(0, 3)), None);
        assert_eq!(map.get(Pos::new(-1, 0)), None);
    }

    #[test]
    fn path_goes_around_water() {
        // G W G
        // G W G
        // G G G
        let mut map = Map::filled(3, 3, Terrain::Grass);
        map.set(Pos::new(1, 0), Terrain::Water);
        map.set(Pos::new(1, 1), Terrain::Water);
        let path = map.path(Pos::new(0, 0), Pos::new(2, 0), walk).unwrap();
        // Down the west bank, along the bottom, up the east bank. No
        // diagonal can shorten it: each would brush the water's corner.
        assert_eq!(
            path,
            vec![
                Pos::new(0, 1),
                Pos::new(0, 2),
                Pos::new(1, 2),
                Pos::new(2, 2),
                Pos::new(2, 1),
                Pos::new(2, 0),
            ]
        );
    }

    #[test]
    fn diagonals_are_taken_but_corners_are_not_cut() {
        // Open field: a pure diagonal walk.
        let open = Map::filled(5, 5, Terrain::Grass);
        assert_eq!(
            open.path(Pos::new(0, 0), Pos::new(3, 3), walk).unwrap(),
            vec![Pos::new(1, 1), Pos::new(2, 2), Pos::new(3, 3)]
        );
        // A knight's move: two straight-ish steps, one of them diagonal.
        assert_eq!(
            open.path(Pos::new(0, 0), Pos::new(2, 1), walk)
                .unwrap()
                .len(),
            2
        );

        // G R
        // R G   — (0,0) to (1,1) would slip between two rocks: refused.
        let mut pinched = Map::filled(2, 2, Terrain::Grass);
        pinched.set(Pos::new(1, 0), Terrain::Rock);
        pinched.set(Pos::new(0, 1), Terrain::Rock);
        assert_eq!(pinched.path(Pos::new(0, 0), Pos::new(1, 1), walk), None);

        // G R
        // G G   — one rock only: still no diagonal past its corner.
        let mut corner = Map::filled(2, 2, Terrain::Grass);
        corner.set(Pos::new(1, 0), Terrain::Rock);
        assert_eq!(
            corner.path(Pos::new(0, 0), Pos::new(1, 1), walk).unwrap(),
            vec![Pos::new(0, 1), Pos::new(1, 1)]
        );
    }

    #[test]
    fn paths_are_cheapest_not_merely_shortest_in_steps() {
        // Going straight 4 tiles costs 40; a 4-diagonal detour would cost 56.
        let open = Map::filled(6, 6, Terrain::Grass);
        let path = open.path(Pos::new(0, 2), Pos::new(4, 2), walk).unwrap();
        assert_eq!(path.len(), 4);
        assert!(path.iter().all(|p| p.y == 2), "stays on the row: {path:?}");
    }

    #[test]
    fn unreachable_and_trivial_paths() {
        let mut map = Map::filled(3, 1, Terrain::Grass);
        map.set(Pos::new(1, 0), Terrain::Rock);
        assert_eq!(map.path(Pos::new(0, 0), Pos::new(2, 0), walk), None);
        assert_eq!(map.path(Pos::new(0, 0), Pos::new(1, 0), walk), None);
        assert_eq!(map.path(Pos::new(0, 0), Pos::new(0, 0), walk), Some(vec![]));
        assert_eq!(map.path(Pos::new(0, 0), Pos::new(9, 9), walk), None);
    }

    /// The content's walking costs (`content/movement.ron`), which the
    /// game's paths use: the heuristic is weaker than with `walk`, so a
    /// search expands more.
    fn valley_walk(terrain: Terrain) -> Option<u32> {
        match terrain {
            Terrain::Grass => Some(10),
            Terrain::Forest | Terrain::Brook => Some(15),
            Terrain::Ford => Some(20),
            Terrain::Water | Terrain::Rock => None,
        }
    }

    /// The path search as it was before it had a reach (5 November
    /// 2026), kept as the reference the bounded one must equal.
    fn unbounded_path(
        map: &Map,
        from: Pos,
        to: Pos,
        cost: impl Fn(Terrain) -> Option<u32>,
    ) -> Option<Vec<Pos>> {
        if from == to {
            return Some(Vec::new());
        }
        let cost_at = |pos: Pos| map.get(pos).and_then(&cost);
        cost_at(from)?;
        cost_at(to)?;
        let cheapest = Terrain::ALL
            .iter()
            .filter_map(|t| cost(*t))
            .min()
            .unwrap_or(STRAIGHT_COST);
        let estimate = |a: Pos| octile(a, to) * cheapest / STRAIGHT_COST;
        let start = map.index(from)?;
        let goal = map.index(to)?;
        let mut best = vec![u32::MAX; map.tiles.len()];
        let mut came_from: Vec<Option<usize>> = vec![None; map.tiles.len()];
        let mut open = BinaryHeap::new();
        let mut pushed = 0u64;
        best[start] = 0;
        open.push(Reverse((estimate(from), pushed, start)));
        while let Some(Reverse((_, _, current))) = open.pop() {
            if current == goal {
                let mut path = Vec::new();
                let mut step = goal;
                while step != start {
                    path.push(map.pos_at(step));
                    step = came_from[step].expect("reached tiles have a predecessor");
                }
                path.reverse();
                return Some(path);
            }
            let here = map.pos_at(current);
            let cost_here = best[current];
            for next in here.neighbours() {
                let Some(i) = map.index(next) else {
                    continue;
                };
                let Some(terrain_cost) = cost(map.tiles[i]) else {
                    continue;
                };
                let step_cost = if here.is_diagonal_to(next) {
                    if cost_at(Pos::new(here.x, next.y)).is_none()
                        || cost_at(Pos::new(next.x, here.y)).is_none()
                    {
                        continue;
                    }
                    DIAGONAL_COST
                } else {
                    STRAIGHT_COST
                };
                let cost = cost_here + step_cost * terrain_cost / STRAIGHT_COST;
                if cost < best[i] {
                    best[i] = cost;
                    came_from[i] = Some(current);
                    pushed += 1;
                    open.push(Reverse((cost + estimate(next), pushed, i)));
                }
            }
        }
        None
    }

    /// `count` random pairs of tiles on `map`, the second within `reach`
    /// tiles of the first on each axis, both on the map.
    fn random_pairs(map: &Map, seed: u64, count: usize, reach: u32) -> Vec<(Pos, Pos)> {
        let mut rng = Lcg::new(seed);
        let side = |n: u32| i32::try_from(n).expect("small map");
        let mut pairs = Vec::new();
        while pairs.len() < count {
            let a = Pos::new(
                side(rng.next_below(map.width())),
                side(rng.next_below(map.height())),
            );
            let b = Pos::new(
                a.x + side(rng.next_below(2 * reach + 1)) - side(reach),
                a.y + side(rng.next_below(2 * reach + 1)) - side(reach),
            );
            if map.get(b).is_some() {
                pairs.push((a, b));
            }
        }
        pairs
    }

    #[test]
    fn bounded_paths_are_the_unbounded_ones() {
        // Pairs anywhere, walkable or not, reachable or not: the bounded
        // search answers exactly as the search it replaced, path for path,
        // but for the rare way that must leave the square around the start.
        let mut found = 0;
        let mut detours = 0;
        for (size, seed, count, reach) in
            [(256, 1, 300, 256), (256, 7, 300, 256), (768, 1, 120, 250)]
        {
            let map = Map::valley(size, size, seed);
            for (a, b) in random_pairs(&map, seed ^ u64::from(size), count, reach) {
                for cost in [walk, valley_walk] {
                    let path = map.path(a, b, cost);
                    let unbounded = unbounded_path(&map, a, b, cost);
                    if path.is_none() && unbounded.is_some() {
                        // The only way leaves the square around `a`.
                        detours += 1;
                        continue;
                    }
                    assert_eq!(path, unbounded, "{a:?} -> {b:?}");
                    found += usize::from(path.is_some_and(|p| !p.is_empty()));
                }
            }
        }
        assert!(found > 600, "many pairs are walkable and joined: {found}");
        assert!(
            detours * 100 < found,
            "few ways leave the square: {detours}"
        );
    }

    #[test]
    fn a_straight_walk_stops_before_the_first_obstacle() {
        let walkable = |p: Pos| p.x != 5;
        let a = Pos::new(1, 1).centre();
        // Clear: all the way.
        assert_eq!(
            line_reach(a, Pos::new(4, 3).centre(), walkable),
            Pos::new(4, 3).centre()
        );
        // A wall at x = 5: stops 30 cm short of it, on the segment.
        let end = line_reach(a, Pos::new(9, 1).centre(), walkable);
        assert_eq!(end, FinePos::new(4700, 1500));
        // Already against it, or on it: no step.
        let near = FinePos::new(4900, 1500);
        assert_eq!(line_reach(near, Pos::new(9, 1).centre(), walkable), near);
        let on = Pos::new(5, 1).centre();
        assert_eq!(line_reach(on, a, walkable), on);
    }

    #[test]
    fn a_search_stays_within_its_reach() {
        // An open field larger than the square, the target walled in by
        // rock: the search explores the square at most, then refuses.
        let mut map = Map::filled(1500, 1500, Terrain::Grass);
        for x in 0..4 {
            map.set(Pos::new(x, 3), Terrain::Rock);
            map.set(Pos::new(3, x), Terrain::Rock);
        }
        let (from, to) = (Pos::new(300, 300), Pos::new(1, 1));
        let (path, expanded) = map.search(from, to, PATH_REACH, u32::MAX, walk);
        assert_eq!(path, None);
        let side = u32::try_from(2 * PATH_REACH + 1).expect("small");
        assert!(expanded <= side * side, "{expanded}");
        // A target beyond the reach is refused at once, even in the open.
        let far = Pos::new(1, 1 + PATH_REACH + 1);
        assert_eq!(
            map.search(Pos::new(1, 1), far, PATH_REACH, u32::MAX, walk),
            (None, 0)
        );
        // At the edge of the reach it is found, straight.
        let edge = Pos::new(10 + PATH_REACH, 10);
        let path = map.path(Pos::new(10, 10), edge, walk).expect("in reach");
        assert_eq!(path.len(), usize::try_from(PATH_REACH).expect("small"));
        // A way that must leave the square is refused: a wall across the
        // field with its only gap out of reach.
        let mut walled = Map::filled(800, 800, Terrain::Grass);
        for y in 0..799 {
            walled.set(Pos::new(400, y), Terrain::Rock);
        }
        assert_eq!(
            walled.path(Pos::new(390, 10), Pos::new(410, 10), walk),
            None
        );
        assert!(
            walled
                .path(Pos::new(390, 780), Pos::new(410, 780), walk)
                .is_some()
        );
    }

    #[test]
    fn a_search_within_a_sight_and_a_budget_finds_the_same_paths_or_none() {
        // Random pairs within a small sight on a valley: under the budget
        // the path is the one found with no budget; over it, none, and
        // the search stops the tile after the budget. Beyond the sight,
        // nothing is searched at all.
        let map = Map::valley(256, 256, 3);
        let (reach, budget) = (60, 300);
        let (mut under, mut over) = (0, 0);
        for (a, b) in random_pairs(&map, 11, 400, 60) {
            let (free, expanded) = map.search(a, b, reach, u32::MAX, valley_walk);
            let (capped, spent) = map.search(a, b, reach, budget, valley_walk);
            if expanded <= budget {
                assert_eq!(capped, free, "{a:?} -> {b:?}");
                assert_eq!(spent, expanded);
                under += usize::from(free.is_some());
            } else {
                assert_eq!(capped, None, "{a:?} -> {b:?}");
                assert_eq!(spent, budget + 1);
                over += 1;
            }
            assert_eq!(map.path_within(a, b, reach, budget, valley_walk), capped);
        }
        assert!(under > 100 && over > 10, "both cases seen: {under}, {over}");
        let open = Map::filled(300, 300, Terrain::Grass);
        let from = Pos::new(150, 150);
        assert_eq!(
            open.search(
                from,
                Pos::new(150, 150 + reach + 1),
                reach,
                budget,
                valley_walk
            ),
            (None, 0),
            "beyond the sight: nothing searched"
        );
        let (edge, _) = open.search(from, Pos::new(150 + reach, 150), reach, budget, valley_walk);
        assert_eq!(
            edge.map(|p| p.len()),
            Some(60),
            "at the sight's edge, straight"
        );
    }

    /// Ground a body can walk to from each tile: a label per tile (`None`
    /// on water and rock), the same for two tiles joined by walkable
    /// ground. Four neighbours suffice: a diagonal step never cuts a
    /// corner, so it always has a straight way round.
    fn walkable_pieces(map: &Map) -> Vec<Option<u32>> {
        let mut label = vec![None; map.tiles.len()];
        let mut next = 0;
        for start in 0..map.tiles.len() {
            if label[start].is_some() || valley_walk(map.tiles[start]).is_none() {
                continue;
            }
            label[start] = Some(next);
            let mut queue = VecDeque::from([start]);
            while let Some(i) = queue.pop_front() {
                for n in map.pos_at(i).neighbours().into_iter().take(4) {
                    if let Some(j) = map.index(n)
                        && label[j].is_none()
                        && valley_walk(map.tiles[j]).is_some()
                    {
                        label[j] = Some(next);
                        queue.push_back(j);
                    }
                }
            }
            next += 1;
        }
        label
    }

    /// How many tiles real paths expand on the game's valleys, and what
    /// the worst refusal costs: the measure behind [`PATH_REACH`]. Slow;
    /// run by hand: `cargo test --release -p sim path_budget_survey --
    /// --ignored --nocapture`.
    #[test]
    #[ignore = "a measure, run by hand in release"]
    fn path_budget_survey() {
        // The quickest of three runs, against the noise of a busy machine.
        let quickest = |run: &mut dyn FnMut()| {
            (0..3)
                .map(|_| {
                    let started = std::time::Instant::now();
                    run();
                    started.elapsed()
                })
                .min()
                .expect("three runs")
        };
        for size in [768, 1536, 3072] {
            let map = Map::valley(size, size, 1);
            let pieces = walkable_pieces(&map);
            let piece = |p: Pos| map.index(p).and_then(|i| pieces[i]);
            let mut sizes = BTreeMap::new();
            for label in pieces.iter().flatten() {
                *sizes.entry(*label).or_insert(0u32) += 1;
            }
            let largest = sizes.values().copied().max().expect("walkable ground");
            println!(
                "{size}: {} pieces of ground, the largest {largest} tiles",
                sizes.len()
            );
            for reach in [100, 300] {
                let pairs: Vec<_> = random_pairs(&map, u64::from(size + reach), 50_000, reach)
                    .into_iter()
                    .filter(|&(a, b)| piece(a).is_some() && piece(a) == piece(b))
                    .take(2000)
                    .collect();
                let mut expansions: Vec<u32> = pairs
                    .iter()
                    .map(|&(a, b)| map.search(a, b, PATH_REACH, u32::MAX, valley_walk).1)
                    .collect();
                let bounded = quickest(&mut || {
                    for &(a, b) in &pairs {
                        assert!(map.path(a, b, valley_walk).is_some());
                    }
                });
                let unbounded = quickest(&mut || {
                    for &(a, b) in &pairs {
                        assert!(unbounded_path(&map, a, b, valley_walk).is_some());
                    }
                });
                expansions.sort_unstable();
                let n = expansions.len();
                println!(
                    "{size}, within {reach}: {n} joined pairs, expanded median {} p90 {} p99 {} max {}; \
                     {:?} a path bounded, {:?} unbounded",
                    expansions[n / 2],
                    expansions[n * 9 / 10],
                    expansions[n * 99 / 100],
                    expansions[n - 1],
                    bounded / u32::try_from(n).expect("few pairs"),
                    unbounded / u32::try_from(n).expect("few pairs"),
                );
            }
            // The worst refusal: from the camp to the nearest walkable
            // tile it is not joined to (an island, a bank with no ford).
            let camp = map.camp();
            let Some(target) = (1..i32::try_from(size).expect("small map"))
                .flat_map(|r| {
                    (-r..=r).flat_map(move |d| {
                        [(d, -r), (d, r), (-r, d), (r, d)]
                            .map(|(dx, dy)| Pos::new(camp.x + dx, camp.y + dy))
                    })
                })
                .find(|&p| piece(p).is_some() && piece(p) != piece(camp))
            else {
                println!("{size}: one piece of ground, nothing to refuse");
                continue;
            };
            let (_, expanded) = map.search(camp, target, PATH_REACH, u32::MAX, valley_walk);
            let bounded = quickest(&mut || assert!(map.path(camp, target, valley_walk).is_none()));
            let unbounded = quickest(&mut || {
                assert!(unbounded_path(&map, camp, target, valley_walk).is_none());
            });
            println!(
                "{size}, refused {camp:?} -> {target:?}: expanded {expanded}; {bounded:?} bounded, {unbounded:?} unbounded"
            );
        }
        // A walled-in target on ground larger than the square: the search
        // stops at the square.
        let field = Map::filled(1500, 1500, Terrain::Grass);
        let (from, to) = (Pos::new(PATH_REACH, PATH_REACH), Pos::new(0, 0));
        let mut walled = field.clone();
        walled.set(Pos::new(0, 1), Terrain::Rock);
        walled.set(Pos::new(1, 0), Terrain::Rock);
        walled.set(Pos::new(1, 1), Terrain::Rock);
        let bounded = quickest(&mut || assert!(walled.path(from, to, valley_walk).is_none()));
        let unbounded = quickest(&mut || {
            assert!(unbounded_path(&walled, from, to, valley_walk).is_none());
        });
        println!("1500 field, walled-in target: {bounded:?} bounded, {unbounded:?} unbounded");
        let found = quickest(&mut || assert!(field.path(from, to, valley_walk).is_some()));
        println!("1500 field, to the reach's corner: {found:?}");
    }

    /// What a search costs under the content's sight and budget (D32):
    /// the worst refusal on the game's valley, in the open and under the
    /// trees, and how many real walks fit the budget. Run by hand:
    /// `cargo test --release -p sim sight_and_budget_survey -- --ignored
    /// --nocapture`.
    #[test]
    #[ignore = "a measure, run by hand in release"]
    fn sight_and_budget_survey() {
        let budget = 1500;
        let quickest = |run: &mut dyn FnMut()| {
            (0..5)
                .map(|_| {
                    let started = std::time::Instant::now();
                    run();
                    started.elapsed()
                })
                .min()
                .expect("five runs")
        };
        let map = Map::valley(3072, 3072, 1);
        let pieces = walkable_pieces(&map);
        let piece = |p: Pos| map.index(p).and_then(|i| pieces[i]);
        let camp = map.camp();
        let mut worst = std::time::Duration::ZERO;
        for (name, reach) in [("open", 60_u32), ("forest", 12)] {
            // The nearest walkable tile not joined to the camp within the
            // sight, if any: a refusal after the search of a whole piece.
            let unjoined = (1..=i32::try_from(reach).expect("small"))
                .flat_map(|r| {
                    (-r..=r).flat_map(move |d| {
                        [(d, -r), (d, r), (-r, d), (r, d)]
                            .map(|(dx, dy)| Pos::new(camp.x + dx, camp.y + dy))
                    })
                })
                .find(|&p| piece(p).is_some() && piece(p) != piece(camp));
            // Joined pairs within the sight: how many fit the budget.
            let pairs: Vec<_> = random_pairs(&map, 3072 + u64::from(reach), 50_000, reach)
                .into_iter()
                .filter(|&(a, b)| piece(a).is_some() && piece(a) == piece(b))
                .take(2000)
                .collect();
            let reach = i32::try_from(reach).expect("small");
            let mut fit = 0;
            let mut longest = std::time::Duration::ZERO;
            for &(a, b) in &pairs {
                let (path, expanded) = map.search(a, b, reach, budget, valley_walk);
                fit += usize::from(path.is_some());
                let took = quickest(&mut || {
                    map.search(a, b, reach, budget, valley_walk);
                });
                longest = longest.max(took);
                assert!(path.is_some() || expanded > budget);
            }
            println!(
                "{name} (sight {reach}): {fit} of {} joined pairs fit the budget of {budget}; the longest search {longest:?}",
                pairs.len()
            );
            worst = worst.max(longest);
            if let Some(target) = unjoined {
                let (path, expanded) = map.search(camp, target, reach, budget, valley_walk);
                assert_eq!(path, None);
                let took = quickest(&mut || {
                    map.search(camp, target, reach, budget, valley_walk);
                });
                println!(
                    "{name}: refused {camp:?} -> {target:?} after {expanded} tiles in {took:?}"
                );
                worst = worst.max(took);
            } else {
                println!("{name}: every walkable tile within sight is joined to the camp");
            }
        }
        // The worst there is: open ground larger than the sight, the
        // target walled in, so the whole budget is spent.
        let mut walled = Map::filled(300, 300, Terrain::Grass);
        walled.set(Pos::new(0, 1), Terrain::Rock);
        walled.set(Pos::new(1, 0), Terrain::Rock);
        walled.set(Pos::new(1, 1), Terrain::Rock);
        let from = Pos::new(50, 50);
        let (path, expanded) = walled.search(from, Pos::new(0, 0), 60, budget, valley_walk);
        assert_eq!((path, expanded), (None, budget + 1));
        let took = quickest(&mut || {
            walled.search(from, Pos::new(0, 0), 60, budget, valley_walk);
        });
        println!("300 field, walled-in target, the whole budget spent: {took:?}");
        worst = worst.max(took);
        assert!(
            worst < std::time::Duration::from_millis(1),
            "every search under a millisecond: {worst:?}"
        );
    }

    #[test]
    fn valley_is_deterministic_and_leaves_room_to_start() {
        let a = Map::valley(256, 256, 7);
        let b = Map::valley(256, 256, 7);
        assert_eq!(a, b);
        assert_ne!(a, Map::valley(256, 256, 8));
        // Cliff on top, rivers and brooks.
        assert_eq!(a.get(Pos::new(5, 0)), Some(Terrain::Rock));
        let count = |t: Terrain| a.tiles().iter().filter(|x| **x == t).count();
        assert!(
            count(Terrain::Water) > 1500,
            "{} water tiles",
            count(Terrain::Water)
        );
        assert!(
            count(Terrain::Brook) >= 20,
            "{} brook tiles",
            count(Terrain::Brook)
        );
        // Trees come in groves: between a tenth and three quarters of
        // the ground (a small map is mostly riverside, where the water
        // draws the woods: D37, 11 November 2026).
        let forest = count(Terrain::Forest);
        assert!((6_000..43_000).contains(&forest), "{forest} forest tiles");
        // Rivers may cut the map in parts: fords are a chance, bridges and
        // boats come later. But the standard world's bodies (seed 1, near
        // tile 134, 139) start on a piece of land at least a quarter of
        // the map, brooks being walked through.
        let standard = Map::valley(256, 256, 1);
        let start = standard
            .nearest(Pos::new(134, 139), Terrain::Grass)
            .unwrap();
        let mut seen = vec![false; 256 * 256];
        let mut stack = vec![start];
        let mut room = 0;
        while let Some(p) = stack.pop() {
            let Some(i) = standard.index(p) else { continue };
            if seen[i] || !standard.is_passable(p) {
                continue;
            }
            seen[i] = true;
            room += 1;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    stack.push(Pos::new(p.x + dx, p.y + dy));
                }
            }
        }
        assert!(room >= 256 * 256 / 4, "{room} tiles to start on");
    }

    #[test]
    fn rivers_reach_the_edges_and_islands_keep_clear_water() {
        let (mut islands_seen, mut ford_tiles) = (0, 0);
        for seed in 1..=6 {
            let (islands, fords) = check_rivers(seed);
            islands_seen += usize::from(islands > 0);
            ford_tiles += fords;
        }
        // Islands need a river of 16 half-cubes and more: rarer on small
        // maps since there are fewer tributaries (9 November 2026).
        assert!(islands_seen >= 1, "islands on {islands_seen} of six maps");
        assert!(ford_tiles >= 60, "{ford_tiles} ford tiles on six maps");
    }

    /// Checks one map's waters; how many island tiles and ford tiles it
    /// has.
    fn check_rivers(seed: u64) -> (usize, usize) {
        let r = Rivers::generate(256, 256, 32, seed);
        let ford_tiles = (0..256)
            .flat_map(|y| (0..256).map(move |x| (x, y)))
            .filter(|(x, y)| r.at(*x, *y) == Some(Terrain::Ford))
            .count();
        // Wet: water or ford, no bank there. Water: deeper than one tile,
        // not walkable.
        let depth = |x: i64, y: i64| match (u32::try_from(x), u32::try_from(y)) {
            (Ok(x), Ok(y)) if x < 256 && y < 256 => r.depth_at(x, y),
            _ => 0,
        };
        let wet = |x: i64, y: i64| depth(x, y) > 0;
        let water = |x: i64, y: i64| depth(x, y) > 1;
        // Depth and terrain agree, and nothing is deeper than allowed.
        for y in 0..256 {
            for x in 0..256 {
                let d = r.depth_at(x, y);
                assert!(d <= Rivers::MAX_DEPTH);
                match r.at(x, y) {
                    None => assert_eq!(d, 0),
                    Some(Terrain::Ford) => assert_eq!(d, 1),
                    Some(Terrain::Brook) => assert!((1..=Rivers::MAX_BROOK_DEPTH).contains(&d)),
                    Some(Terrain::Water) => assert!(d >= 2),
                    other => panic!("{other:?} from the rivers"),
                }
            }
        }
        // Water touches at least two edges of the map: rivers are open.
        let edges = [
            (0..256).any(|x| water(x, 32)),
            (0..256).any(|x| water(x, 255)),
            (32..256).any(|y| water(0, y)),
            (32..256).any(|y| water(255, y)),
        ];
        assert!(edges.iter().filter(|e| **e).count() >= 2, "{edges:?}");
        // An island is land not connected over dry land to the map's
        // border; every island tile keeps MIN_WIDTH tiles of water or ford
        // on every side (a ford reaching an island has no bank either).
        let mut mainland = vec![false; 256 * 256];
        let mut stack: Vec<(i64, i64)> = Vec::new();
        for i in 0..256 {
            for (x, y) in [(i, 32), (i, 255), (0, i.max(32)), (255, i.max(32))] {
                if !wet(x, y) {
                    stack.push((x, y));
                }
            }
        }
        while let Some((x, y)) = stack.pop() {
            if !(0..256).contains(&x) || !(32..256).contains(&y) || wet(x, y) {
                continue;
            }
            let i = usize::try_from(y * 256 + x).expect("inside");
            if mainland[i] {
                continue;
            }
            mainland[i] = true;
            for dy in -1..=1_i64 {
                for dx in -1..=1_i64 {
                    stack.push((x + dx, y + dy));
                }
            }
        }
        let m = i64::from(Rivers::MIN_WIDTH);
        let mut island_tiles = 0;
        for y in 32..256_i64 {
            for x in 0..256_i64 {
                if wet(x, y) || mainland[usize::try_from(y * 256 + x).expect("inside")] {
                    continue;
                }
                island_tiles += 1;
                for dy in -m..=m {
                    for dx in -m..=m {
                        if dx.abs() + dy.abs() > m {
                            // Three tiles of water on every side: as the
                            // crow flies, not across a corner.
                            continue;
                        }
                        let (nx, ny) = (x + dx, y + dy);
                        let ok = wet(nx, ny)
                            || !mainland[usize::try_from(
                                ny.clamp(0, 255) * 256 + nx.clamp(0, 255),
                            )
                            .expect("inside")];
                        assert!(
                            ok,
                            "seed {seed}: island tile ({x}, {y}) too close to the mainland"
                        );
                    }
                }
            }
        }
        (island_tiles, ford_tiles)
    }

    #[test]
    fn banks_breathe_within_their_rules() {
        let c = Rivers::CLEARANCE;
        let (mut free, mut moved, mut longest) = (0_usize, 0_usize, 0_usize);
        let (mut widest_pool, mut past_old_bounds) = (0_i64, 0_usize);
        // Twenty maps: a pool wider than the widest river is rare.
        for seed in 1..=20 {
            let r = Rivers::generate(256, 256, 32, seed);
            for river in &r.network {
                let len = river.path.len();
                assert_eq!(river.banks.len(), len);
                let bend = |t: usize| {
                    t > 0 && t < len && river.path[t].heading != river.path[t - 1].heading
                };
                let side = |t: usize, s: usize| {
                    river
                        .banks
                        .get(t)
                        .map_or(0, |b| if s == 0 { b.0 } else { b.1 })
                };
                let mut same_width = 0;
                for t in 0..len {
                    let (left, right) = river.banks[t];
                    for out in [left, right] {
                        assert!((Rivers::BANK_REACH.0..=Rivers::BANK_REACH.1).contains(&out));
                    }
                    // Calm near the ends, junctions, fords and divergences.
                    let near = |a: usize, b: usize| t + c >= a && t < b + c;
                    let calm = t < c
                        || t + c >= len
                        || river.joins.iter().any(|(at, _)| near(*at, at + 1))
                        || river.fords.iter().any(|f| {
                            near(
                                f.saturating_sub(Rivers::FORD_RAMP),
                                f + Rivers::FORD_STEPS + Rivers::FORD_RAMP,
                            )
                        })
                        || river
                            .divergences
                            .iter()
                            .any(|d| near(d.from, d.from + d.len + 1));
                    if calm {
                        assert_eq!((left, right), (0, 0), "seed {seed}: step {t} is calm");
                    }
                    for s in 0..2 {
                        // A bank moves at one slope at most, and never
                        // bends within a step of a bend of the centre.
                        let before = if t > 0 {
                            side(t, s) - side(t - 1, s)
                        } else {
                            0
                        };
                        let after = side(t + 1, s) - side(t, s);
                        assert!(after.abs() <= 1);
                        if before != after {
                            assert!(
                                !bend(t.saturating_sub(1)) && !bend(t) && !bend(t + 1),
                                "seed {seed}: bank bends at step {t} by a bend of the centre"
                            );
                        }
                    }
                    // The width stays a river's and the flow is kept.
                    let flow = river.flow_at(t, &r.network);
                    let shape = river.shape_at(t);
                    if Rivers::section(flow, shape).0 < i64::from(Rivers::MIN_WIDTH) {
                        assert_eq!((left, right), (0, 0), "a brook does not breathe");
                        continue;
                    }
                    let (wide, depth) =
                        Rivers::breathing_section(flow, shape, (left + right) * Rivers::BANK_SLOPE);
                    let tiles = (wide + 500) / 1000;
                    let (narrowest, widest) = Rivers::breathing_room(flow, shape);
                    assert!((narrowest..=widest).contains(&tiles), "width {tiles}");
                    assert!(
                        (3..=i64::from(Rivers::MAX_POOL_WIDTH)).contains(&tiles),
                        "width {tiles}"
                    );
                    assert!((2..=i64::from(Rivers::MAX_DEPTH)).contains(&depth));
                    widest_pool = widest_pool.max(tiles);
                    past_old_bounds += usize::from(tiles >= flow.min(13));
                    // The section covers the flow; past a metre deep, no more.
                    assert!(tiles * depth >= flow);
                    if (3..i64::from(Rivers::MAX_DEPTH)).contains(&depth) {
                        assert!(tiles * (depth - 1) < flow);
                    }
                    if calm {
                        same_width = 0;
                    } else {
                        free += 1;
                        moved += usize::from((left, right) != (0, 0));
                        // Straight and parallel: the same heading, the
                        // same banks.
                        let width_held = t > 0 && river.banks[t] == river.banks[t - 1] && !bend(t);
                        same_width = if width_held { same_width + 1 } else { 0 };
                        longest = longest.max(same_width);
                    }
                }
            }
        }
        eprintln!(
            "banks off their line on {moved} of {free} free steps; longest straight: {longest}; \
             widest pool {widest_pool}; {past_old_bounds} steps wider than their flow or 12"
        );
        // Pools go past what the flow alone would fill.
        assert!(
            widest_pool >= i64::from(Rivers::MAX_WIDTH),
            "widest pool {widest_pool}"
        );
        assert!(past_old_bounds > 0);
        // Most of the free course breathes, and no stretch runs straight
        // between parallel banks for long.
        assert!(free > 0 && moved * 100 >= free * 50, "{moved} of {free}");
        assert!(longest <= 30, "straight and parallel for {longest} steps");
    }

    #[test]
    fn brooks_meander_more_than_rivers() {
        // Mean steps a heading is held, brooks then rivers, over six maps.
        let (mut brook, mut river) = ((0, 0), (0, 0));
        for seed in 1..=6 {
            let r = Rivers::generate(256, 256, 32, seed);
            for water in &r.network {
                let is_brook = water.entry_flow <= i64::from(Rivers::BROOK_FLOW.1);
                let start = i32::from(water.path[0].heading);
                let mut bends = 0;
                for pair in water.path.windows(2) {
                    let (a, b) = (i32::from(pair[0].heading), i32::from(pair[1].heading));
                    let turn = (b - a + 8).rem_euclid(16) - 8;
                    // One step of heading at a time, never past the drift.
                    assert!(turn.abs() <= 1, "seed {seed}: a turn of {turn} steps");
                    let drift = (b - start + 8).rem_euclid(16) - 8;
                    let most = if is_brook {
                        Rivers::BROOK_DRIFT
                    } else {
                        Rivers::MAX_DRIFT
                    };
                    assert!(drift.abs() <= most, "seed {seed}: strayed {drift} steps");
                    bends += usize::from(turn != 0);
                }
                let tally = if is_brook { &mut brook } else { &mut river };
                tally.0 += water.path.len();
                tally.1 += bends + 1;
            }
        }
        let (brook_run, river_run) = (brook.0 / brook.1, river.0 / river.1);
        eprintln!("a heading held {brook_run} steps by brooks, {river_run} by rivers");
        assert!(brook.1 > 0 && brook_run <= 4, "brooks hold {brook_run}");
        assert!(river_run > brook_run);
    }

    #[test]
    fn brooks_are_long_few_spaced_and_mostly_from_the_edges() {
        // The first valley: 768 × 768, the cliff 96 rows deep.
        let (size, top) = (768, 96);
        let r = Rivers::generate(size, size, top, 1);
        let brooks: Vec<&River> = r
            .network
            .iter()
            .filter(|w| w.entry_flow <= i64::from(Rivers::BROOK_FLOW.1))
            .collect();
        // Fewer waters to join since 9 November 2026 (1.5 % water): on this
        // small map some brooks find none within reach (the real valley,
        // 3072, has 49 to 61).
        assert!((6..=16).contains(&brooks.len()), "{} brooks", brooks.len());
        let mut from_edges = 0;
        for b in &brooks {
            let len = b.path.len();
            assert!(
                (Rivers::BROOK_MIN_STEPS..=Rivers::BROOK_STEPS).contains(&len),
                "a brook of {len} steps"
            );
            // On the valley floor all along.
            assert!(b.path.iter().all(|s| {
                let (x, y) = (s.x / 1000, s.y / 1000);
                (0..i64::from(size)).contains(&x) && (i64::from(top)..i64::from(size)).contains(&y)
            }));
            // Never along an edge, past its first steps.
            assert!(b.path.iter().skip(Rivers::BROOK_LEAVE).all(|s| {
                let (x, y) = (s.x / 1000, s.y / 1000);
                let last = i64::from(size) - 1;
                x.min(last - x).min(last - y).min(y - i64::from(top) - 1)
                    >= Rivers::BROOK_EDGE_CLEAR
            }));
            let (x, y) = (b.path[0].x / 1000, b.path[0].y / 1000);
            let edge = i64::from(size) - 1;
            from_edges += usize::from(x == 0 || x == edge || y == edge || y == i64::from(top) + 1);
        }
        // Many from the edges: with fewer rivers (9 November 2026), an
        // edge is farther from water, so fewer of them find a way.
        assert!(
            from_edges * 3 >= brooks.len(),
            "{from_edges} of {} from the edges",
            brooks.len()
        );
        // Spaced: no step of a brook, short of its last stretch, within
        // BROOK_SPACING of another brook laid before it.
        let spaced = usize::try_from(Rivers::BROOK_SPACING).expect("small");
        for (i, b) in brooks.iter().enumerate() {
            for earlier in &brooks[..i] {
                for s in &b.path[..b.path.len().saturating_sub(spaced)] {
                    for e in earlier.path.iter().step_by(2) {
                        let (dx, dy) = ((s.x - e.x).abs() / 1000, (s.y - e.y).abs() / 1000);
                        // Within two tiles: the generator marks every
                        // other step, on whole tiles.
                        assert!(
                            dx.max(dy) > Rivers::BROOK_SPACING - 2,
                            "two brooks {} tiles apart",
                            dx.max(dy)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_brook_reaches_the_water() {
        // Every brook, with the brooks it joins, touches a river or a ford.
        for seed in 1..=8 {
            let map = Map::valley(256, 256, seed);
            let brook = |p: Pos| map.get(p) == Some(Terrain::Brook);
            let river = |p: Pos| matches!(map.get(p), Some(Terrain::Water | Terrain::Ford));
            let mut seen = std::collections::BTreeSet::new();
            for y in 0..256 {
                for x in 0..256 {
                    let start = Pos::new(x, y);
                    if !brook(start) || seen.contains(&start) {
                        continue;
                    }
                    let (mut stack, mut touches) = (vec![start], false);
                    seen.insert(start);
                    while let Some(p) = stack.pop() {
                        for dy in -1..=1 {
                            for dx in -1..=1 {
                                let q = Pos::new(p.x + dx, p.y + dy);
                                touches |= river(q);
                                if brook(q) && seen.insert(q) {
                                    stack.push(q);
                                }
                            }
                        }
                    }
                    assert!(
                        touches,
                        "seed {seed}: the brooks at {start:?} reach no river"
                    );
                }
            }
        }
    }

    #[test]
    fn a_brook_is_a_band_without_gaps_along_its_whole_path() {
        // Every tile a brook's centre path crosses is water (its own or
        // the water it joins), and no land tile lies in a bed: brook on
        // both its sides, or river water, leaves no grass in the water
        // (11 November 2026: a grass tile with plants in a brook).
        for seed in 1..=8 {
            let r = Rivers::generate(256, 256, 32, seed);
            let wet = |x: i64, y: i64| r.index(x, y).is_some_and(|i| r.depth[i] > 0);
            let brook = |x: i64, y: i64| r.index(x, y).is_some_and(|i| r.brook[i]);
            let river = |x: i64, y: i64| wet(x, y) && !brook(x, y);
            for water in &r.network {
                if water.entry_flow > i64::from(Rivers::BROOK_FLOW.1) {
                    continue;
                }
                for (n, s) in water.path.iter().enumerate() {
                    // Tile centres lie at whole thousandths: the tile a
                    // point is in is the nearest centre.
                    let (x, y) = ((s.x + 500).div_euclid(1000), (s.y + 500).div_euclid(1000));
                    assert!(
                        wet(x, y),
                        "seed {seed}: step {n} of a brook dry at ({x}, {y})"
                    );
                }
            }
            for y in 32..256 {
                for x in 0..256 {
                    if wet(x, y) {
                        continue;
                    }
                    assert!(
                        !((brook(x - 1, y) && brook(x + 1, y))
                            || (brook(x, y - 1) && brook(x, y + 1))),
                        "seed {seed}: land at ({x}, {y}) between two sides of a brook"
                    );
                    assert!(
                        !((river(x - 1, y) && river(x + 1, y))
                            || (river(x, y - 1) && river(x, y + 1))),
                        "seed {seed}: land at ({x}, {y}) between two sides of a river"
                    );
                }
            }
        }
    }

    #[test]
    fn every_brook_runs_from_its_source_to_its_mouth() {
        // Walking a brook's path from source to mouth, the current stored
        // on each of its tiles points along the next step, and the last
        // tile of the path touches the water it joins (11 November 2026:
        // brooks seen running away from the river).
        let valleys: Vec<(u32, u64)> = (1..=8).map(|seed| (256, seed)).chain([(3072, 1)]).collect();
        for (size, seed) in valleys {
            let r = Rivers::generate(size, size, size / 8, seed);
            let tile = |s: &Step| ((s.x + 500).div_euclid(1000), (s.y + 500).div_euclid(1000));
            let (mut checked, mut brooks) = (0, 0);
            for water in &r.network {
                if water.entry_flow > i64::from(Rivers::BROOK_FLOW.1) {
                    continue;
                }
                brooks += 1;
                for (n, pair) in water.path.windows(2).enumerate() {
                    let (x, y) = tile(&pair[0]);
                    let i = r.index(x, y).expect("on the map");
                    if !r.brook[i] {
                        continue;
                    }
                    let (heading, _) = decode_current(r.current[i]).expect("water runs");
                    let along = HEADINGS[usize::from(heading)];
                    let (dx, dy) = (pair[1].x - pair[0].x, pair[1].y - pair[0].y);
                    assert!(
                        along.0 * dx + along.1 * dy > 0,
                        "valley {size} seed {seed}: step {n} of a brook at ({x}, {y}) runs {heading} against its path"
                    );
                    checked += 1;
                }
                let (x, y) = tile(water.path.last().expect("a path"));
                let touches = (-1..=1).any(|dy| {
                    (-1..=1).any(|dx| r.index(x + dx, y + dy).is_some_and(|j| r.depth[j] > 0))
                });
                assert!(
                    touches,
                    "valley {size} seed {seed}: a brook ends dry at ({x}, {y})"
                );
            }
            eprintln!("valley {size} seed {seed}: {checked} tiles of {brooks} brooks");
            assert!(
                brooks > 0 && checked > 10 * brooks,
                "{checked} tiles of {brooks} brooks"
            );
        }
    }

    #[test]
    fn every_water_runs_down_its_course() {
        for seed in 1..=4 {
            let map = Map::valley(256, 256, seed);
            let r = Rivers::generate(256, 256, 32, seed);
            let (mut ford_speed, mut fords, mut river_speed, mut rivers) =
                (0_u64, 0_u64, 0_u64, 0_u64);
            for y in 0..256 {
                for x in 0..256 {
                    let pos = Pos::new(x, y);
                    let terrain = map.get(pos);
                    let wet = matches!(
                        terrain,
                        Some(Terrain::Water | Terrain::Ford | Terrain::Brook)
                    );
                    let current = map.current(pos);
                    // Water runs, land does not.
                    assert_eq!(wet, current.is_some(), "seed {seed}: {pos:?} {terrain:?}");
                    if let Some((heading, speed)) = current {
                        assert!(
                            heading < 16 && (Rivers::SPEED.0..=Rivers::SPEED.1).contains(&speed)
                        );
                        match terrain {
                            Some(Terrain::Ford) => {
                                ford_speed += u64::from(speed);
                                fords += 1;
                            }
                            Some(Terrain::Water) => {
                                river_speed += u64::from(speed);
                                rivers += 1;
                            }
                            _ => {}
                        }
                    }
                }
            }
            // Along the main river, the water runs its way, a step of
            // heading off at most (where it bends).
            let main = &r.network[0];
            let (mut checked, mut astray) = (0, 0);
            for step in &main.path {
                let (x, y) = (step.x / 1000, step.y / 1000);
                let Some(pos) = (x >= 0 && y >= 32 && x < 256 && y < 256)
                    .then(|| Pos::new(i32::try_from(x).unwrap(), i32::try_from(y).unwrap()))
                else {
                    continue;
                };
                if map.get(pos) != Some(Terrain::Water) {
                    continue;
                }
                let (heading, _) = map.current(pos).unwrap();
                let off = (i32::from(heading) - i32::from(step.heading) + 8).rem_euclid(16) - 8;
                // A step of heading off at most, but where a wide water
                // joining paints over the main river's course.
                astray += usize::from(off.abs() > 1);
                checked += 1;
            }
            assert!(checked > 100);
            assert!(
                astray * 50 <= checked,
                "seed {seed}: {astray} of {checked} astray"
            );
            // Spread thin over its stones, a ford runs faster.
            if let Some(ford_mean) = ford_speed.checked_div(fords) {
                assert!(ford_mean > river_speed / rivers, "seed {seed}");
            }
        }
        assert_eq!(decode_current(current_code(13, 42)), Some((13, 42)));
        assert_eq!(decode_current(0), None);
    }

    #[test]
    fn the_valley_is_about_one_and_a_half_percent_water() {
        // Rivers and brooks are landmarks, not the landscape. On the first
        // valley's size, 3072 × 3072 (8 November 2026), water lies on about
        // 2 % of the ground below the cliff, on average over a few maps (one
        // map alone ranges from 1.5 % to 2.5 %). Measured here, not on small
        // maps, where the one main river weighs far more.
        let (size, top) = (3072_u32, 384_u32);
        let (mut wet, mut ground) = (0_u64, 0_u64);
        for seed in 1..=4 {
            let r = Rivers::generate(size, size, top, seed);
            for y in top..size {
                for x in 0..size {
                    ground += 1;
                    wet += u64::from(r.depth_at(x, y) > 0);
                }
            }
        }
        let share = wet * 1000 / ground;
        eprintln!("water on {share} per mille of the valley");
        assert!((11..=20).contains(&share), "{share} per mille");
    }

    #[test]
    #[ignore = "timing, by hand"]
    fn time_the_valley() {
        let t = std::time::Instant::now();
        let _ = Rivers::generate(256, 256, 32, 1);
        eprintln!("rivers: {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let _ = Map::valley(256, 256, 1);
        eprintln!("valley: {:?}", t.elapsed());
    }

    #[test]
    fn the_section_follows_the_flow() {
        // A tile half a metre deep holds one half-cube: width × depth
        // covers the flow, in either shape.
        for shape in [Shape::Wide, Shape::Deep] {
            // Up to the safeguard: 64 tiles × 6 m = 768 half-cubes.
            let most = i64::from(Rivers::MAX_RIVER_WIDTH) * i64::from(Rivers::MAX_DEPTH);
            for flow in 1..most {
                let (width, depth) = Rivers::section(flow, shape);
                assert!(
                    (1..=i64::from(Rivers::MAX_RIVER_WIDTH)).contains(&width),
                    "flow {flow}: width {width}"
                );
                assert!(
                    (1..=i64::from(Rivers::MAX_DEPTH)).contains(&depth),
                    "flow {flow}: depth {depth}"
                );
                // The section always carries the flow: no water is lost.
                assert!(width * depth >= flow, "flow {flow}: {width} × {depth}");
                // A brook is one or two wide and a metre deep at most.
                assert!(width >= 3 || depth <= 2, "flow {flow}: {width} × {depth}");
                // The width never shrinks as the flow grows.
                assert!(Rivers::section(flow + 1, shape).0 >= width, "flow {flow}");
            }
        }
        // Two half-cubes: two wide and half a metre, or one wide and a metre.
        assert_eq!(Rivers::section(1, Shape::Wide), (2, 1));
        assert_eq!(Rivers::section(2, Shape::Wide), (2, 1));
        assert_eq!(Rivers::section(2, Shape::Deep), (1, 2));
        assert_eq!(Rivers::section(8, Shape::Wide), (4, 2));
        assert_eq!(Rivers::section(8, Shape::Deep), (3, 3));
        // Rivers are wide for their flow (9 November 2026), never under a
        // metre deep.
        assert_eq!(Rivers::section(96, Shape::Wide), (18, 6));
        assert_eq!(Rivers::section(224, Shape::Wide), (28, 8));
        // Past the deepest (6 m), the river only widens.
        assert_eq!(Rivers::section(200, Shape::Deep), (17, 12));
        for flow in 6..300 {
            for shape in [Shape::Wide, Shape::Deep] {
                let (width, depth) = Rivers::section(flow, shape);
                assert!(
                    width < 3 || depth >= 2,
                    "flow {flow}: a river {width} wide, {depth} deep"
                );
            }
        }
    }

    #[test]
    fn every_river_of_the_valley_carries_its_flow() {
        // On the first valley's size, all along every water: the section
        // of each step holds what flows there, however many junctions came
        // before, and the widest river stays far from the safeguard.
        let r = Rivers::generate(3072, 3072, 384, 1);
        let mut widest = 0;
        for water in &r.network {
            for step in 0..water.path.len() {
                let flow = water.flow_at(step, &r.network);
                let (width, depth) = Rivers::section(flow, water.shape_at(step));
                assert!(width * depth >= flow, "flow {flow}: {width} × {depth}");
                widest = widest.max(width);
            }
        }
        assert!(widest > 12, "the main river widens past 12: {widest}");
        assert!(
            widest < i64::from(Rivers::MAX_RIVER_WIDTH) / 2,
            "widest {widest}"
        );
    }

    #[test]
    fn the_valley_stores_the_depth_of_its_water() {
        let map = Map::valley(256, 256, 7);
        let mut deep = 0;
        for y in 0..256 {
            for x in 0..256 {
                let pos = Pos::new(x, y);
                let depth = map.depth(pos);
                match map.get(pos).unwrap() {
                    Terrain::Water => {
                        assert!(depth >= 2, "{pos:?}");
                        deep = deep.max(depth);
                    }
                    Terrain::Ford => assert_eq!(depth, 1, "{pos:?}"),
                    Terrain::Brook => assert!((1..=2).contains(&depth), "{pos:?}"),
                    _ => assert_eq!(depth, 0, "{pos:?}"),
                }
            }
        }
        // Wide for its flow (9 November 2026): on this small map the main
        // river is a metre and a half deep at most.
        assert!(
            deep >= 3,
            "the main river gets a metre and a half deep: {deep}"
        );
        assert_eq!(map.depth(Pos::new(-1, 5)), 0);
    }

    #[test]
    #[ignore = "writes the valley as an image for a look; run by hand"]
    fn dump_valley() {
        let path = std::env::var("SOUL_INVASION_DUMP").unwrap_or_else(|_| "valley.pgm".into());
        let seed = std::env::var("SOUL_INVASION_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(VALLEY_SEED_FOR_DUMP);
        let map = Map::valley(256, 256, seed);
        for line in Rivers::generate(256, 256, 256 / 8, seed).log {
            eprintln!("{line}");
        }
        let mut out = String::from("P2\n256 256\n255\n");
        for t in map.tiles() {
            let v = match t {
                Terrain::Grass => 200,
                Terrain::Forest => 120,
                Terrain::Water => 40,
                Terrain::Ford => 90,
                Terrain::Brook => 60,
                Terrain::Rock => 230,
            };
            out.push_str(&v.to_string());
            out.push(' ');
        }
        std::fs::write(path, out).unwrap();
        // The woods, a colour per habitat, when asked for.
        if let Ok(woods) = std::env::var("SOUL_INVASION_DUMP_WOODS") {
            let mut out = String::from("P3\n256 256\n255\n");
            for y in 0..256 {
                for x in 0..256 {
                    let rgb = match map.habitat(Pos::new(x, y)) {
                        Habitat::Heart => "20 60 25",
                        Habitat::Wood => "45 100 45",
                        Habitat::Edge => "110 150 80",
                        Habitat::Meadow => "200 215 150",
                        Habitat::Clearing => "225 215 140",
                        Habitat::Camp => "240 200 120",
                        Habitat::Copse => "70 120 60",
                        Habitat::Lone => "90 140 70",
                        Habitat::Riverside => "60 150 120",
                        Habitat::CliffFoot => "30 80 70",
                        Habitat::None if y < 32 => "150 145 140",
                        Habitat::None => "60 110 170",
                    };
                    out.push_str(rgb);
                    out.push(' ');
                }
            }
            std::fs::write(woods, out).unwrap();
        }
    }

    const VALLEY_SEED_FOR_DUMP: u64 = 1;

    #[test]
    fn a_map_serializes_as_its_spec_and_comes_back_identical() {
        let map = Map::valley(64, 96, 3);
        let text = serde_json::to_string(&map).unwrap();
        assert!(text.len() < 100, "no tiles in the snapshot: {text}");
        let back: Map = serde_json::from_str(&text).unwrap();
        assert_eq!(back, map);
        assert_eq!(
            back.spec(),
            MapSpec::Valley {
                width: 64,
                height: 96,
                seed: 3
            }
        );
    }

    #[test]
    fn chunks_tile_the_map_and_reach_past_its_edge() {
        let map = Map::filled(40, 32, Terrain::Grass);
        assert_eq!((map.chunks_wide(), map.chunks_high()), (2, 1));
        assert_eq!(Pos::new(33, 5).chunk(), ChunkPos::new(1, 0));
        assert_eq!(Pos::new(-1, -1).chunk(), ChunkPos::new(-1, -1));
        assert_eq!(ChunkPos::new(1, 0).origin(), Pos::new(32, 0));
        let east = map.chunk(ChunkPos::new(1, 0)).unwrap();
        assert_eq!(east.len(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
        assert_eq!(east[0], Terrain::Grass, "tile (32, 0) is on the map");
        assert_eq!(east[8], Terrain::Rock, "tile (40, 0) is past the edge");
        assert!(map.chunk(ChunkPos::new(2, 0)).is_none());
        assert!(map.chunk(ChunkPos::new(-1, 0)).is_none());
    }

    #[test]
    fn dear_terrain_is_avoided_when_a_detour_is_worth_it() {
        // Three rows: forest in the middle row, grass above and below. A
        // walk along the middle row goes round through the grass when the
        // forest is dear enough, and straight through otherwise.
        let mut map = Map::filled(9, 3, Terrain::Grass);
        for x in 0..9 {
            map.set(Pos::new(x, 1), Terrain::Forest);
        }
        let slow_forest = |t: Terrain| match t {
            Terrain::Forest => Some(30),
            other => walk(other),
        };
        let path = map
            .path(Pos::new(0, 1), Pos::new(8, 1), slow_forest)
            .unwrap();
        assert!(path.iter().filter(|p| p.y == 1).count() <= 2, "{path:?}");
        let path = map.path(Pos::new(0, 1), Pos::new(8, 1), walk).unwrap();
        assert!(path.iter().all(|p| p.y == 1), "{path:?}");
    }

    #[test]
    fn a_pulled_path_is_a_few_straight_segments_that_avoid_water() {
        // A diagonal walk across an open field is one segment.
        let open = Map::filled(32, 32, Terrain::Grass);
        let path = open.path(Pos::new(2, 3), Pos::new(22, 23), walk).unwrap();
        assert_eq!(path.len(), 20);
        let route = open.pull(Pos::new(2, 3).centre(), &path, walk);
        assert_eq!(route, vec![Pos::new(22, 23).centre()]);

        // Round a pond: a handful of segments, none crossing water.
        let mut pond = Map::filled(32, 32, Terrain::Grass);
        for y in 10..20 {
            for x in 10..20 {
                pond.set(Pos::new(x, y), Terrain::Water);
            }
        }
        let path = pond.path(Pos::new(5, 15), Pos::new(25, 15), walk).unwrap();
        let route = pond.pull(Pos::new(5, 15).centre(), &path, walk);
        assert!(route.len() >= 2 && route.len() <= 5, "{route:?}");
        assert_eq!(*route.last().unwrap(), Pos::new(25, 15).centre());
        let mut from = Pos::new(5, 15).centre();
        for to in &route {
            assert!(
                line_clear(from, *to, |p| pond.is_passable(p)),
                "{from:?} -> {to:?}"
            );
            from = *to;
        }
    }

    #[test]
    fn pulling_keeps_to_the_road_instead_of_the_field() {
        // A road bends round a field; pulling must not cut across the
        // dearer field.
        let mut map = Map::filled(10, 10, Terrain::Forest);
        for x in 0..10 {
            map.set(Pos::new(x, 0), Terrain::Grass);
            map.set(Pos::new(x, 9), Terrain::Grass);
        }
        for y in 0..10 {
            map.set(Pos::new(0, y), Terrain::Grass);
        }
        let road = |t: Terrain| match t {
            Terrain::Grass => Some(10),
            Terrain::Forest => Some(40),
            other => walk(other),
        };
        let path = map.path(Pos::new(9, 0), Pos::new(9, 9), road).unwrap();
        let route = map.pull(Pos::new(9, 0).centre(), &path, road);
        // Along the edges, the corners cut diagonally as A* does: five
        // waypoints, none in the forest, no segment across it.
        assert_eq!(
            route,
            vec![
                Pos::new(1, 0).centre(),
                Pos::new(0, 1).centre(),
                Pos::new(0, 8).centre(),
                Pos::new(1, 9).centre(),
                Pos::new(9, 9).centre()
            ]
        );
        let mut from = Pos::new(9, 0).centre();
        for to in &route {
            assert!(line_clear(from, *to, |p| map.get(p) == Some(Terrain::Grass)));
            from = *to;
        }
    }

    #[test]
    fn a_line_cannot_slip_between_two_blocked_tiles() {
        // G R
        // R G   — from (0,0) to (1,1) exactly through the shared corner.
        let mut pinched = Map::filled(2, 2, Terrain::Grass);
        pinched.set(Pos::new(1, 0), Terrain::Rock);
        pinched.set(Pos::new(0, 1), Terrain::Rock);
        let a = Pos::new(0, 0).centre();
        let b = Pos::new(1, 1).centre();
        assert!(!line_clear(a, b, |p| pinched.is_passable(p)));
        // Grazing one blocked corner is allowed: a body is a point.
        let mut grazed = Map::filled(2, 2, Terrain::Grass);
        grazed.set(Pos::new(1, 0), Terrain::Rock);
        assert!(line_clear(a, b, |p| grazed.is_passable(p)));
        let open = Map::filled(2, 2, Terrain::Grass);
        assert!(line_clear(a, b, |p| open.is_passable(p)));
        assert!(line_clear(a, a, |p| open.is_passable(p)));
        assert!(!line_clear(a, Pos::new(5, 5).centre(), |p| open.is_passable(p)));
    }

    #[test]
    fn fine_positions_map_to_tiles() {
        assert_eq!(FinePos::new(0, 0).tile(), Pos::new(0, 0));
        assert_eq!(FinePos::new(999, 1999).tile(), Pos::new(0, 1));
        assert_eq!(FinePos::new(-1, 1000).tile(), Pos::new(-1, 1));
        assert_eq!(Pos::new(3, 4).centre(), FinePos::new(3500, 4500));
        assert!((FinePos::new(0, 0).distance_to(FinePos::new(3000, 4000)) - 5000.0).abs() < 1e-9);
    }

    #[test]
    fn distance_to_a_terrain_counts_eight_way_steps() {
        let mut map = Map::filled(5, 5, Terrain::Grass);
        map.set(Pos::new(0, 0), Terrain::Rock);
        let distance = map.distance_to(Terrain::Rock);
        assert_eq!(distance[0], 0);
        assert_eq!(
            distance[map.index(Pos::new(1, 1)).unwrap()],
            1,
            "diagonal is one step"
        );
        assert_eq!(distance[map.index(Pos::new(4, 4)).unwrap()], 4);
        assert_eq!(distance[map.index(Pos::new(4, 0)).unwrap()], 4);
        assert!(
            map.distance_to(Terrain::Water)
                .iter()
                .all(|d| *d == u32::MAX),
            "no water anywhere"
        );
    }

    #[test]
    fn nearest_finds_the_closest_tile_of_a_terrain() {
        let mut map = Map::filled(5, 5, Terrain::Rock);
        map.set(Pos::new(4, 4), Terrain::Grass);
        map.set(Pos::new(1, 2), Terrain::Grass);
        assert_eq!(
            map.nearest(Pos::new(2, 2), Terrain::Grass),
            Some(Pos::new(1, 2))
        );
        assert_eq!(
            map.nearest(Pos::new(1, 2), Terrain::Grass),
            Some(Pos::new(1, 2))
        );
        assert_eq!(map.nearest(Pos::new(2, 2), Terrain::Water), None);
    }
}
