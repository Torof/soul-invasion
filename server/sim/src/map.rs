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

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use serde::{Deserialize, Serialize};

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
    /// The river; not walkable.
    Water,
    /// A shallow crossing of the river.
    Ford,
    /// The cliff; not walkable.
    Rock,
}

impl Terrain {
    /// Every terrain, for content that must cover them all.
    pub const ALL: [Self; 5] = [
        Self::Grass,
        Self::Forest,
        Self::Water,
        Self::Ford,
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

/// The grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "MapSpec", into = "MapSpec")]
pub struct Map {
    spec: MapSpec,
    width: u32,
    height: u32,
    tiles: Vec<Terrain>,
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
        }
    }

    /// The first version's valley: a cliff to the north, rivers crossing
    /// the map, groves of forest, grass elsewhere. Deterministic for a
    /// given seed. The rivers follow the rules of the shore (see
    /// [`Rivers`]): they enter and leave the map, hold a direction for a
    /// run then bend by one step, keep their width within bounds, may
    /// cross and join, and carry islands that keep clear water all round;
    /// each river has one ford, a band of shallow water that can be
    /// walked across.
    pub fn valley(width: u32, height: u32, seed: u64) -> Self {
        let mut map = Self::filled(width, height, Terrain::Grass);
        map.spec = MapSpec::Valley {
            width,
            height,
            seed,
        };
        let groves = Groves::new(width, height, seed);
        let cliff_rows = (height / 8).max(2);
        let rivers = Rivers::generate(width, height, cliff_rows, seed);

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
                } else if groves.is_forest(x, y) {
                    Terrain::Forest
                } else {
                    Terrain::Grass
                };
                map.set(pos, terrain);
            }
        }
        map
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

    /// The terrain at `pos`, or `None` outside the map.
    pub fn get(&self, pos: Pos) -> Option<Terrain> {
        self.index(pos).map(|i| self.tiles[i])
    }

    /// Sets the terrain at `pos`; ignored outside the map. Generation
    /// only: terrain is not stored, so a change here does not survive a
    /// snapshot.
    fn set(&mut self, pos: Pos, terrain: Terrain) {
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
        let mut distance = vec![u32::MAX; self.tiles.len()];
        let mut queue = VecDeque::new();
        for (i, t) in self.tiles.iter().enumerate() {
            if *t == terrain {
                distance[i] = 0;
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
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
    /// `None` when unreachable.
    pub fn path(
        &self,
        from: Pos,
        to: Pos,
        cost: impl Fn(Terrain) -> Option<u32>,
    ) -> Option<Vec<Pos>> {
        if from == to {
            return Some(Vec::new());
        }
        let cost_at = |pos: Pos| self.get(pos).and_then(&cost);
        cost_at(from)?;
        cost_at(to)?;
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
        let start = self.index(from)?;
        let goal = self.index(to)?;
        let mut best = vec![u32::MAX; self.tiles.len()];
        let mut came_from: Vec<Option<usize>> = vec![None; self.tiles.len()];
        let mut open = BinaryHeap::new();
        let mut pushed = 0u64;
        best[start] = 0;
        open.push(Reverse((estimate(from), pushed, start)));

        while let Some(Reverse((_, _, current))) = open.pop() {
            if current == goal {
                let mut path = Vec::new();
                let mut step = goal;
                while step != start {
                    path.push(self.pos_at(step));
                    step = came_from[step].expect("reached tiles have a predecessor");
                }
                path.reverse();
                return Some(path);
            }
            let here = self.pos_at(current);
            let cost_here = best[current];
            for next in here.neighbours() {
                let Some(i) = self.index(next) else {
                    continue;
                };
                let Some(terrain_cost) = cost(self.tiles[i]) else {
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

    /// Pulls a tile path taut into straight segments from `from`: each
    /// waypoint is the farthest tile centre reachable in a straight line
    /// that crosses only walkable tiles no dearer than the tiles the
    /// segment replaces, so a road is not short-cut through the field
    /// beside it (D32). The last waypoint is the path's last tile.
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
            for (j, tile) in path.iter().enumerate().skip(first) {
                dearest = dearest.max(cost_at(*tile).unwrap_or(u32::MAX));
                let allowed = |pos: Pos| cost_at(pos).is_some_and(|c| c <= dearest);
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
#[allow(clippy::cast_possible_truncation)]
pub fn line_clear(a: FinePos, b: FinePos, allowed: impl Fn(Pos) -> bool) -> bool {
    let scale = f64::from(MM_PER_TILE);
    let (x0, y0) = (f64::from(a.x) / scale, f64::from(a.y) / scale);
    let (x1, y1) = (f64::from(b.x) / scale, f64::from(b.y) / scale);
    let (mut tx, mut ty) = (x0.floor() as i32, y0.floor() as i32);
    let (ex, ey) = (x1.floor() as i32, y1.floor() as i32);
    if !allowed(Pos::new(tx, ty)) {
        return false;
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
    while (tx, ty) != (ex, ey) && remaining > 0 {
        remaining -= 1;
        if (t_max_x - t_max_y).abs() < 1e-9 {
            // Through a corner: no slipping between two blocked tiles.
            if !allowed(Pos::new(tx + step_x, ty)) && !allowed(Pos::new(tx, ty + step_y)) {
                return false;
            }
            tx += step_x;
            ty += step_y;
            t_max_x += t_delta_x;
            t_max_y += t_delta_y;
        } else if t_max_x < t_max_y {
            tx += step_x;
            t_max_x += t_delta_x;
        } else {
            ty += step_y;
            t_max_y += t_delta_y;
        }
        if !allowed(Pos::new(tx, ty)) {
            return false;
        }
    }
    (tx, ty) == (ex, ey)
}

/// Lower bound on the walking cost between two tiles with eight-way steps.
fn octile(a: Pos, b: Pos) -> u32 {
    let dx = a.x.abs_diff(b.x);
    let dy = a.y.abs_diff(b.y);
    STRAIGHT_COST * dx.max(dy) + (DIAGONAL_COST - STRAIGHT_COST) * dx.min(dy)
}

/// A tiny deterministic generator, so world generation needs no dependency
/// and gives the same valley on every machine.
pub(crate) struct Lcg(u64);

impl Lcg {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1))
    }

    pub(crate) fn next_below(&mut self, bound: u32) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        u32::try_from((self.0 >> 33) % u64::from(bound)).expect("below bound")
    }
}

/// Where the forest grows: smooth value noise on a coarse lattice, so
/// trees come in groves rather than salt and pepper.
struct Groves {
    cell: u32,
    columns: u32,
    values: Vec<u32>,
}

impl Groves {
    /// Lattice spacing in tiles.
    const CELL: u32 = 12;
    /// Noise is in 0..1000; forest above this.
    const THRESHOLD: u32 = 640;

    fn new(width: u32, height: u32, seed: u64) -> Self {
        let cell = Self::CELL;
        let columns = width / cell + 2;
        let rows = height / cell + 2;
        let mut rng = Lcg::new(seed);
        let values = (0..columns * rows).map(|_| rng.next_below(1000)).collect();
        Self {
            cell,
            columns,
            values,
        }
    }

    fn is_forest(&self, x: u32, y: u32) -> bool {
        let (cx, fx) = (x / self.cell, x % self.cell);
        let (cy, fy) = (y / self.cell, y % self.cell);
        let at = |i: u32, j: u32| self.values[(j * self.columns + i) as usize];
        // Bilinear interpolation, in thousandths.
        let top = at(cx, cy) * (self.cell - fx) + at(cx + 1, cy) * fx;
        let bottom = at(cx, cy + 1) * (self.cell - fx) + at(cx + 1, cy + 1) * fx;
        let value = (top * (self.cell - fy) + bottom * fy) / (self.cell * self.cell);
        value > Self::THRESHOLD
    }
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

/// The rivers of a map, generated by the rules of the shore:
///
/// - a river is a centre path that holds a heading for a run of steps
///   (one tile each) and then bends by one step of 22.5 degrees, never
///   turning back on the way it entered;
/// - its width holds for a run, then drifts by one tile, within
///   `MIN_WIDTH..=MAX_WIDTH`: at the widest the banks stop parting, at the
///   narrowest they stop closing;
/// - it enters the map by an edge and leaves it by another, never closed;
/// - several rivers may cross; their waters join;
/// - islands are land inside the water, from one tile up to a large
///   island, stretched along the current, allowed only where
///   `MIN_WIDTH` tiles of water stay on every side;
/// - each river has one ford: a band of shallow, walkable water.
///
/// All arithmetic is in integers (tile thousandths), so the same seed
/// gives the same rivers everywhere.
pub struct Rivers {
    width: u32,
    height: u32,
    /// Per tile: `None` for land, else the water terrain there.
    water: Vec<Option<Terrain>>,
    /// Per water tile, the heading of the river that painted it.
    heading: Vec<u8>,
}

/// Flat index of a tile in a `width` × `height` grid, if inside.
fn index_in(width: u32, height: u32, x: i64, y: i64) -> Option<usize> {
    (x >= 0 && y >= 0 && x < i64::from(width) && y < i64::from(height))
        .then(|| usize::try_from(y * i64::from(width) + x).expect("fits"))
}

impl Rivers {
    /// Narrowest river, in tiles.
    pub const MIN_WIDTH: u32 = 3;
    /// Widest river, in tiles.
    pub const MAX_WIDTH: u32 = 12;
    /// How many steps a heading or a width is held, at least and at most.
    const RUN: (u32, u32) = (6, 14);
    /// A river never strays more than this many steps of heading from the
    /// heading it entered with (4 steps = 90 degrees).
    const MAX_DRIFT: i32 = 3;
    /// Length of a ford along the river, in steps.
    const FORD_STEPS: usize = 3;

    /// The rivers of a map of `width` by `height`, below `cliff_rows`.
    pub fn generate(width: u32, height: u32, cliff_rows: u32, seed: u64) -> Self {
        let mut rivers = Self {
            width,
            height,
            water: vec![None; (width * height) as usize],
            heading: vec![0; (width * height) as usize],
        };
        let mut rng = Lcg::new(seed.wrapping_add(0x5157));
        let w = i64::from(width);
        let h = i64::from(height);
        let top = i64::from(cliff_rows);
        // Three rivers: one from the cliff's foot going south, one from the
        // west going east, one from the east going south-west.
        let starts = [
            (
                (
                    w / 4 + i64::from(rng.next_below(u32::try_from(w / 2).expect("fits"))),
                    top,
                ),
                0u8,
            ),
            (
                (
                    0,
                    top + h / 6 + i64::from(rng.next_below(u32::try_from(h / 2).expect("fits"))),
                ),
                4u8,
            ),
            (
                (
                    w - 1,
                    top + i64::from(rng.next_below(u32::try_from(h / 3).expect("fits"))),
                ),
                14u8,
            ),
        ];
        let mut fords = Vec::new();
        for (start, heading) in starts {
            let path = rivers.trace(start, heading, &mut rng);
            rivers.rasterize(&path);
            // The ford: a band in the middle third of the river's course.
            let third = path.len() / 3;
            if path.len() > Self::FORD_STEPS + 2 * third {
                let at = third
                    + usize::try_from(rng.next_below(u32::try_from(third).expect("fits")))
                        .expect("fits");
                fords.push(path[at..at + Self::FORD_STEPS].to_vec());
            }
        }
        rivers.round_banks();
        rivers.carve_islands(&mut rng);
        for band in fords {
            rivers.rasterize_as(&band, Terrain::Ford);
        }
        rivers.connect_regions(cliff_rows);
        rivers
    }

    /// Rounds the banks: a land tile with water on three of its four sides
    /// (the tip of a tongue at a junction, a pimple on a bank) becomes
    /// water, a water tile with land on three sides becomes land. Two
    /// passes, so thin tongues erode but nothing drifts further.
    fn round_banks(&mut self) {
        for _ in 0..2 {
            let before = self.water.clone();
            let (width, height) = (self.width, self.height);
            let wet = |x: i64, y: i64| match index_in(width, height, x, y) {
                Some(i) => before[i].is_some(),
                None => true,
            };
            for y in 0..i64::from(height) {
                for x in 0..i64::from(width) {
                    let sides = [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)];
                    let wet_sides = sides.iter().filter(|(sx, sy)| wet(*sx, *sy)).count();
                    let i = self.index(x, y).expect("inside");
                    if before[i].is_none() && wet_sides >= 3 {
                        self.water[i] = Some(Terrain::Water);
                        self.heading[i] = sides
                            .iter()
                            .find_map(|(sx, sy)| {
                                self.index(*sx, *sy).filter(|j| before[*j].is_some())
                            })
                            .map_or(0, |j| self.heading[j]);
                    } else if before[i].is_some() && wet_sides <= 1 {
                        self.water[i] = None;
                    }
                }
            }
        }
    }

    /// Fords wherever the land would otherwise be split: the land regions
    /// (eight-way, fords walkable) are joined one to the next by the
    /// shortest crossing of water between them, painted as a ford. Small
    /// islands (fewer than 20 tiles) stay as they are.
    fn connect_regions(&mut self, cliff_rows: u32) {
        const BIG: usize = 20;
        let (width, height) = (i64::from(self.width), i64::from(self.height));
        let top = i64::from(cliff_rows);
        loop {
            // Label the land regions.
            let mut region = vec![usize::MAX; self.water.len()];
            let mut sizes: Vec<usize> = Vec::new();
            for y in top..height {
                for x in 0..width {
                    let i = self.index(x, y).expect("inside");
                    if region[i] != usize::MAX || self.water[i] == Some(Terrain::Water) {
                        continue;
                    }
                    let id = sizes.len();
                    let mut size = 0;
                    let mut stack = vec![(x, y)];
                    while let Some((cx, cy)) = stack.pop() {
                        let Some(j) = self.index(cx, cy) else {
                            continue;
                        };
                        if cy < top
                            || region[j] != usize::MAX
                            || self.water[j] == Some(Terrain::Water)
                        {
                            continue;
                        }
                        region[j] = id;
                        size += 1;
                        for dy in -1..=1 {
                            for dx in -1..=1 {
                                stack.push((cx + dx, cy + dy));
                            }
                        }
                    }
                    sizes.push(size);
                }
            }
            let Some(main) = (0..sizes.len()).max_by_key(|r| sizes[*r]) else {
                return;
            };
            if !(0..sizes.len()).any(|r| r != main && sizes[r] >= BIG) {
                return;
            }
            // From the largest region, the shortest walk over water to any
            // other big region, by breadth-first search.
            let mut from = vec![usize::MAX; self.water.len()];
            let mut queue = std::collections::VecDeque::new();
            for y in top..height {
                for x in 0..width {
                    let i = self.index(x, y).expect("inside");
                    if region[i] == main {
                        from[i] = i;
                        queue.push_back((x, y));
                    }
                }
            }
            let mut reached = None;
            'search: while let Some((cx, cy)) = queue.pop_front() {
                let i = self.index(cx, cy).expect("inside");
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (cx + dx, cy + dy);
                    let Some(j) = self.index(nx, ny) else {
                        continue;
                    };
                    if ny < top || from[j] != usize::MAX {
                        continue;
                    }
                    from[j] = i;
                    if region[j] != usize::MAX && region[j] != main && sizes[region[j]] >= BIG {
                        reached = Some(j);
                        break 'search;
                    }
                    if self.water[j].is_some() {
                        queue.push_back((nx, ny));
                    }
                }
            }
            let Some(mut at) = reached else { return };
            // Paint the crossing, two tiles wide, as a ford.
            while from[at] != at {
                let flat = i64::try_from(at).expect("fits");
                let (x, y) = (flat % width, flat / width);
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    if let Some(k) = self
                        .index(x + dx, y + dy)
                        .filter(|k| self.water[*k].is_some())
                    {
                        self.water[k] = Some(Terrain::Ford);
                    }
                }
                at = from[at];
            }
        }
    }

    /// The water terrain of a tile, or `None` on land.
    pub fn at(&self, x: u32, y: u32) -> Option<Terrain> {
        self.water[(y * self.width + x) as usize]
    }

    fn index(&self, x: i64, y: i64) -> Option<usize> {
        index_in(self.width, self.height, x, y)
    }

    /// The centre path of one river: steps of one tile, each with the
    /// heading it was taken with and the half-width there, in thousandths.
    fn trace(&self, start: (i64, i64), heading: u8, rng: &mut Lcg) -> Vec<Step> {
        let (run_min, run_max) = Self::RUN;
        let run = |rng: &mut Lcg| run_min + rng.next_below(run_max - run_min + 1);
        let base = i32::from(heading);
        let mut dir = base;
        let mut width =
            i64::from(Self::MIN_WIDTH + rng.next_below(Self::MAX_WIDTH - Self::MIN_WIDTH + 1));
        let (mut x, mut y) = (start.0 * 1000, start.1 * 1000);
        let (mut run_dir, mut run_width) = (run(rng), run(rng));
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
                let turn = i32::try_from(rng.next_below(3)).expect("small") - 1;
                let drift = dir + turn - base;
                dir += if drift.abs() > Self::MAX_DRIFT {
                    -turn
                } else {
                    turn
                };
                run_dir = run(rng);
            }
            if run_width == 0 {
                let drift = i64::from(rng.next_below(3)) - 1;
                width =
                    (width + drift).clamp(i64::from(Self::MIN_WIDTH), i64::from(Self::MAX_WIDTH));
                run_width = run(rng);
            }
            let h = HEADINGS[usize::try_from(dir.rem_euclid(16)).expect("small")];
            path.push(Step {
                x,
                y,
                heading: u8::try_from(dir.rem_euclid(16)).expect("small"),
                half_width: width * 500,
            });
            x += h.0;
            y += h.1;
            run_dir -= 1;
            run_width -= 1;
        }
        path
    }

    /// Paints water under a path: every tile whose centre is within the
    /// local half-width of the path.
    fn rasterize(&mut self, path: &[Step]) {
        self.rasterize_as(path, Terrain::Water);
    }

    fn rasterize_as(&mut self, path: &[Step], terrain: Terrain) {
        for pair in path.windows(2) {
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
                    if d2 <= hw * hw && self.water[i] != Some(Terrain::Ford) {
                        self.water[i] = Some(terrain);
                        self.heading[i] = a.heading;
                    }
                }
            }
        }
    }

    /// Does `MIN_WIDTH` of water stay on every side of tile (x, y)?
    fn clear_around(&self, x: i64, y: i64) -> bool {
        let r = i64::from(Self::MIN_WIDTH);
        for dy in -r..=r {
            for dx in -r..=r {
                match self.index(x + dx, y + dy) {
                    Some(i) if self.water[i].is_some() => {}
                    _ => return false,
                }
            }
        }
        true
    }

    /// Islands: from seeds that keep clear water all round, grown mostly
    /// along the current, each to a size drawn from one tile to large.
    fn carve_islands(&mut self, rng: &mut Lcg) {
        const SIZES: [u32; 11] = [1, 1, 2, 3, 5, 8, 12, 20, 35, 60, 100];
        let wanted = 6 + rng.next_below(7);
        let mut islands: Vec<Vec<(i64, i64)>> = Vec::new();
        let mut tries = 0;
        while islands.len() < wanted as usize && tries < 4000 {
            tries += 1;
            let sx = i64::from(rng.next_below(self.width));
            let sy = i64::from(rng.next_below(self.height));
            if !self.clear_around(sx, sy) {
                continue;
            }
            // Keep islands apart from each other by a river's width.
            let apart = i64::from(Self::MIN_WIDTH) * 2 + 2;
            if islands
                .iter()
                .flatten()
                .any(|(ix, iy)| (ix - sx).abs() <= apart && (iy - sy).abs() <= apart)
            {
                continue;
            }
            let size =
                SIZES[usize::try_from(rng.next_below(u32::try_from(SIZES.len()).expect("small")))
                    .expect("small")];
            let heading = HEADINGS[usize::from(self.heading[self.index(sx, sy).expect("inside")])];
            let along = (heading.0.signum(), heading.1.signum());
            let mut island = vec![(sx, sy)];
            let mut frontier = vec![(sx, sy)];
            // Growth stops when the size is reached, the frontier is spent,
            // or too many steps found no room (a seed boxed in).
            let mut attempts = 0;
            while island.len() < size as usize && !frontier.is_empty() && attempts < 200 {
                attempts += 1;
                let pick =
                    usize::try_from(rng.next_below(u32::try_from(frontier.len()).expect("small")))
                        .expect("small");
                let (px, py) = frontier.swap_remove(pick);
                // Along the current three times out of four, across otherwise.
                let (dx, dy) = if rng.next_below(4) < 3 {
                    if rng.next_below(2) == 0 {
                        along
                    } else {
                        (-along.0, -along.1)
                    }
                } else {
                    let across = (-along.1, along.0);
                    if rng.next_below(2) == 0 {
                        across
                    } else {
                        (-across.0, -across.1)
                    }
                };
                // One axis at a time, so the island stays in one piece.
                let (dx, dy) = match (dx, dy) {
                    (0, 0) => (1, 0),
                    (dx, dy) if dx != 0 && dy != 0 => {
                        if rng.next_below(2) == 0 {
                            (dx, 0)
                        } else {
                            (0, dy)
                        }
                    }
                    other => other,
                };
                let q = (px + dx, py + dy);
                if island.contains(&q) || !self.clear_around(q.0, q.1) {
                    frontier.push((px, py));
                    continue;
                }
                island.push(q);
                frontier.push(q);
                frontier.push((px, py));
            }
            islands.push(island);
        }
        for island in islands {
            for (x, y) in island {
                let i = self.index(x, y).expect("inside");
                self.water[i] = None;
            }
        }
    }
}

/// One step of a river's centre path, in tile thousandths.
#[derive(Clone)]
struct Step {
    x: i64,
    y: i64,
    heading: u8,
    half_width: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn valley_is_deterministic_and_crossable() {
        let a = Map::valley(256, 256, 7);
        let b = Map::valley(256, 256, 7);
        assert_eq!(a, b);
        assert_ne!(a, Map::valley(256, 256, 8));
        // Cliff on top, rivers with fords, and the land is connected
        // across them.
        assert_eq!(a.get(Pos::new(5, 0)), Some(Terrain::Rock));
        let count = |t: Terrain| a.tiles().iter().filter(|x| **x == t).count();
        assert!(
            count(Terrain::Water) > 1500,
            "{} water tiles",
            count(Terrain::Water)
        );
        assert!(
            count(Terrain::Ford) >= 6,
            "{} ford tiles",
            count(Terrain::Ford)
        );
        let west = a.nearest(Pos::new(20, 200), Terrain::Grass).unwrap();
        let east = a.nearest(Pos::new(240, 200), Terrain::Grass).unwrap();
        let path = a.path(west, east, walk).unwrap();
        assert!(path.iter().any(|p| a.get(*p) == Some(Terrain::Ford)));
        // Trees come in groves: between a tenth and half of the ground.
        let forest = count(Terrain::Forest);
        assert!((6_000..33_000).contains(&forest), "{forest} forest tiles");
    }

    #[test]
    fn rivers_reach_the_edges_and_islands_keep_clear_water() {
        let r = Rivers::generate(256, 256, 32, 7);
        // Wet: water or ford, no bank there. Deep: water only, not walkable.
        let wet = |x: i64, y: i64| match (u32::try_from(x), u32::try_from(y)) {
            (Ok(x), Ok(y)) if x < 256 && y < 256 => r.at(x, y).is_some(),
            _ => false,
        };
        let water = |x: i64, y: i64| match (u32::try_from(x), u32::try_from(y)) {
            (Ok(x), Ok(y)) if x < 256 && y < 256 => r.at(x, y) == Some(Terrain::Water),
            _ => false,
        };
        // Water touches at least two edges of the map: rivers are open.
        let edges = [
            (0..256).any(|x| water(x, 32)),
            (0..256).any(|x| water(x, 255)),
            (32..256).any(|y| water(0, y)),
            (32..256).any(|y| water(255, y)),
        ];
        assert!(edges.iter().filter(|e| **e).count() >= 2, "{edges:?}");
        // An island is land not connected to the map's border land, fords
        // being walkable; every island tile keeps MIN_WIDTH tiles of water
        // or ford on every side.
        let mut mainland = vec![false; 256 * 256];
        let mut stack: Vec<(i64, i64)> = Vec::new();
        for i in 0..256 {
            for (x, y) in [(i, 32), (i, 255), (0, i.max(32)), (255, i.max(32))] {
                if !water(x, y) {
                    stack.push((x, y));
                }
            }
        }
        while let Some((x, y)) = stack.pop() {
            if !(0..256).contains(&x) || !(32..256).contains(&y) || water(x, y) {
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
                        let (nx, ny) = (x + dx, y + dy);
                        let ok = wet(nx, ny)
                            || !mainland[usize::try_from(
                                ny.clamp(0, 255) * 256 + nx.clamp(0, 255),
                            )
                            .expect("inside")];
                        assert!(ok, "island tile ({x}, {y}) too close to the mainland");
                    }
                }
            }
        }
        assert!(island_tiles > 0, "no island");
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
        let mut out = String::from("P2\n256 256\n255\n");
        for t in map.tiles() {
            let v = match t {
                Terrain::Grass => 200,
                Terrain::Forest => 120,
                Terrain::Water => 40,
                Terrain::Ford => 90,
                Terrain::Rock => 230,
            };
            out.push_str(&v.to_string());
            out.push(' ');
        }
        std::fs::write(path, out).unwrap();
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
