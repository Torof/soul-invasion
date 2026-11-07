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
    /// Per tile, how deep the water is, in tiles; 0 on land.
    depth: Vec<u8>,
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
            depth: vec![0; (width * height) as usize],
        }
    }

    /// The first version's valley: a cliff to the north, a tree of rivers
    /// (see [`Rivers`]: a main river from the cliff's foot to the south
    /// edge, tributaries joining it, water as a volume whose flow sets
    /// width and depth, fords where it is one deep, islands between the
    /// arms of a divergence), groves of forest, grass elsewhere.
    /// Deterministic for a given seed.
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
                if y >= cliff_rows {
                    map.depth[(y * width + x) as usize] = rivers.depth_at(x, y);
                }
            }
        }
        map
    }

    /// How deep the water is at `pos`, in tiles: 0 on land and outside
    /// the map, 1 on a ford, more in a river.
    pub fn depth(&self, pos: Pos) -> u8 {
        self.index(pos).map_or(0, |i| self.depth[i])
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

/// The heading closest to the direction `(dx, dy)`.
fn heading_towards(dx: i64, dy: i64) -> u8 {
    (0..16u8)
        .max_by_key(|h| {
            let v = HEADINGS[usize::from(*h)];
            v.0 * dx + v.1 * dy
        })
        .expect("sixteen headings")
}

/// The rivers of a map, generated by the rules of the shore and of
/// water as a volume:
///
/// - a tile of water one deep holds one cube; a river carries a flow, in
///   cubes, conserved along its course: at a confluence the flows add
///   up, at a divergence the flow is shared between the two arms;
/// - the flow sets the section: a river of flow `f` is about `width ×
///   depth = f` tiles, the width within `MIN_WIDTH..=MAX_WIDTH`, the
///   depth within `1..=MAX_DEPTH`; a bigger flow is both wider and
///   deeper, and past `MAX_DEPTH` it only widens (see [`Rivers::section`]);
/// - a ford is where a river spreads until it is one deep right across,
///   which only a flow of `MAX_WIDTH` cubes or less can do; one tile deep
///   is walkable;
/// - the network is a tree: a main river enters at the cliff's foot and
///   leaves by the south edge; tributaries enter by an edge or rise from
///   a spring inside the map, and end where they meet water;
/// - a divergence splits a river in two arms around an island, from a
///   rock to a large island, and the arms join again downstream; the
///   smaller arm, when shallow enough, shoals into a ford;
/// - a centre path holds a heading for a run of steps (one tile each),
///   then bends by one step of 22.5 degrees, never turning back on the
///   way it entered.
///
/// All arithmetic is in integers (tile thousandths), so the same seed
/// gives the same rivers everywhere.
pub struct Rivers {
    width: u32,
    height: u32,
    /// Per tile: 0 for land, else how many tiles deep the water is.
    depth: Vec<u8>,
    /// Per tile: the river that painted it, as index + 1; 0 for land.
    owner: Vec<u16>,
    /// Per tile painted by a river: the step of its path that did.
    step: Vec<u32>,
    /// One line per river, for the tests' dumps.
    #[cfg(test)]
    log: Vec<String>,
}

/// Flat index of a tile in a `width` × `height` grid, if inside.
fn index_in(width: u32, height: u32, x: i64, y: i64) -> Option<usize> {
    (x >= 0 && y >= 0 && x < i64::from(width) && y < i64::from(height))
        .then(|| usize::try_from(y * i64::from(width) + x).expect("fits"))
}

/// One river of the network, before it is painted.
struct River {
    /// Centre path, one step per tile.
    path: Vec<Step>,
    /// Cubes it carries where it starts.
    entry_flow: i64,
    /// Tributaries joining it: the step of this river where they arrive,
    /// and their index. A tributary is always traced after the river it
    /// joins, so its index is greater.
    joins: Vec<(usize, usize)>,
    /// Cubes it carries where it ends, once the tributaries are counted.
    end_flow: i64,
    /// Divergences along it.
    divergences: Vec<Divergence>,
    /// Steps spread into fords.
    fords: Vec<usize>,
}

impl River {
    /// Cubes carried at `step`: what entered plus every tributary that
    /// has arrived by then.
    fn flow_at(&self, step: usize, rivers: &[River]) -> i64 {
        self.entry_flow
            + self
                .joins
                .iter()
                .filter(|(at, _)| *at <= step)
                .map(|(_, j)| rivers[*j].end_flow)
                .sum::<i64>()
    }
}

/// A river parting in two arms around an island.
struct Divergence {
    /// First step of the split; the arms leave the centre path there.
    from: usize,
    /// Steps until the arms meet again.
    len: usize,
    /// Cubes carried by the arm to the left of the current, then the
    /// right.
    flows: (i64, i64),
    /// Greatest offset of each arm's centre from the path, in thousandths.
    offset: i64,
    /// Steps over which the offset rises to its greatest, and falls back.
    ramp: usize,
}

/// One step of a river's centre path, in tile thousandths.
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
    /// Narrowest river, in tiles.
    pub const MIN_WIDTH: u32 = 3;
    /// Widest river, in tiles.
    pub const MAX_WIDTH: u32 = 12;
    /// Deepest water, in tiles; past that a river only widens.
    pub const MAX_DEPTH: u8 = 4;
    /// Cubes the main river brings in, at least and at most.
    const MAIN_FLOW: (u32, u32) = (8, 12);
    /// Cubes a tributary entering by an edge brings in.
    const TRIBUTARY_FLOW: (u32, u32) = (8, 16);
    /// Cubes a spring inside the map gives.
    const SPRING_FLOW: (u32, u32) = (4, 6);
    /// Below this, an arm of a divergence would be no river: no split.
    const MIN_ARM_FLOW: i64 = 4;
    /// A river parts in two only from this flow up.
    const DIVERGE_FLOW: i64 = 12;
    /// How many steps a heading is held, at least and at most.
    const RUN: (u32, u32) = (6, 14);
    /// A river never strays more than this many steps of heading from the
    /// heading it entered with (4 steps = 90 degrees).
    const MAX_DRIFT: i32 = 3;
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

    /// The rivers of a map of `width` by `height`, below `cliff_rows`.
    pub fn generate(width: u32, height: u32, cliff_rows: u32, seed: u64) -> Self {
        let mut rivers = Self {
            width,
            height,
            depth: vec![0; (width * height) as usize],
            owner: vec![0; (width * height) as usize],
            step: vec![0; (width * height) as usize],
            #[cfg(test)]
            log: Vec::new(),
        };
        let mut rng = Lcg::new(seed.wrapping_add(0x5157));
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
        let (path, _) = rivers.trace(start, 0, &mut rng);
        let flow = between(&mut rng, Self::MAIN_FLOW);
        rivers.paint_provisional(&path, flow, 0);
        network.push(River::new(path, flow));

        // Tributaries: three from the edges, then one or two springs
        // inside the map. Each heads for a point of the main river and
        // ends where it meets water.
        let sides = [0, w - 1, if rng.next_below(2) == 0 { 0 } else { w - 1 }];
        for side in sides {
            let y =
                top + h / 8 + i64::from(rng.next_below(u32::try_from(h * 3 / 4).expect("fits")));
            let flow = between(&mut rng, Self::TRIBUTARY_FLOW);
            rivers.add_tributary(&mut network, (side, y), flow, &mut rng);
        }
        for _ in 0..=rng.next_below(2) {
            let mut tries = 0;
            while tries < 30 {
                tries += 1;
                let x = w / 8 + i64::from(rng.next_below(u32::try_from(w * 3 / 4).expect("fits")));
                let y = top
                    + h / 8
                    + i64::from(rng.next_below(u32::try_from(h * 3 / 4).expect("fits")));
                if rivers.clear_of_water(x, y, 15) {
                    let flow = between(&mut rng, Self::SPRING_FLOW);
                    rivers.add_tributary(&mut network, (x, y), flow, &mut rng);
                    break;
                }
            }
        }

        // Flows: from the last traced river, which nothing joins, back to
        // the main one.
        for i in (0..network.len()).rev() {
            let joined: i64 = network[i]
                .joins
                .iter()
                .map(|(_, j)| network[*j].end_flow)
                .sum();
            network[i].end_flow = network[i].entry_flow + joined;
        }
        for i in 0..network.len() {
            let wanted = if i == 0 {
                1 + rng.next_below(2)
            } else {
                rng.next_below(2)
            };
            rivers.diverge(&mut network, i, wanted, &mut rng);
            Self::ford(&mut network, i, &mut rng);
        }

        #[cfg(test)]
        for (i, r) in network.iter().enumerate() {
            let (a, z) = (r.path[0], r.path[r.path.len() - 1]);
            rivers.log.push(format!(
                "river {i}: ({},{})->({},{}) {} steps, flow {}->{}, joins {:?}, divergences {:?}, fords {:?}",
                a.x / 1000,
                a.y / 1000,
                z.x / 1000,
                z.y / 1000,
                r.path.len(),
                r.entry_flow,
                r.end_flow,
                r.joins,
                r.divergences.iter().map(|d| (d.from, d.len, d.flows, d.offset / 1000, d.ramp)).collect::<Vec<_>>(),
                r.fords
            ));
        }
        // Paint everything afresh with the real sections.
        rivers.depth.fill(0);
        rivers.owner.fill(0);
        rivers.step.fill(0);
        for i in 0..network.len() {
            for band in Self::bands(&network, i) {
                rivers.paint(&band, i, false);
            }
        }
        for i in 0..network.len() {
            for band in Self::ford_bands(&network, i) {
                rivers.paint(&band, i, true);
            }
        }
        rivers.round_banks();
        rivers.connect_regions(cliff_rows);
        rivers
    }

    /// The section of a river carrying `flow` cubes: width then depth,
    /// in tiles. The width is the square root of three times the flow
    /// (a river is wider than deep), the depth takes the rest; past
    /// `MAX_DEPTH` the width takes everything, up to `MAX_WIDTH`.
    pub fn section(flow: i64) -> (i64, i64) {
        let (min, max) = (i64::from(Self::MIN_WIDTH), i64::from(Self::MAX_WIDTH));
        let deepest = i64::from(Self::MAX_DEPTH);
        let flow = flow.max(1);
        // round(sqrt(3 × flow)) = round(sqrt(12 × flow) / 2).
        let mut width = ((12 * flow).isqrt() + 1) / 2;
        width = width.clamp(min, max);
        let ceil_div = |a: i64, b: i64| (a + b - 1) / b;
        let mut depth = ceil_div(flow, width);
        if depth > deepest {
            width = ceil_div(flow, deepest).clamp(min, max);
            depth = ceil_div(flow, width).min(deepest);
        }
        (width, depth.max(1))
    }

    /// What is at `(x, y)`: `None` on land, a ford one tile deep, water
    /// deeper.
    pub fn at(&self, x: u32, y: u32) -> Option<Terrain> {
        match self.depth[(y * self.width + x) as usize] {
            0 => None,
            1 => Some(Terrain::Ford),
            _ => Some(Terrain::Water),
        }
    }

    /// How deep the water is at `(x, y)`, in tiles; 0 on land.
    pub fn depth_at(&self, x: u32, y: u32) -> u8 {
        self.depth[(y * self.width + x) as usize]
    }

    fn index(&self, x: i64, y: i64) -> Option<usize> {
        index_in(self.width, self.height, x, y)
    }

    /// No water within `reach` tiles of `(x, y)`.
    fn clear_of_water(&self, x: i64, y: i64, reach: i64) -> bool {
        (y - reach..=y + reach).all(|ty| {
            (x - reach..=x + reach).all(|tx| self.index(tx, ty).is_none_or(|i| self.depth[i] == 0))
        })
    }

    /// Traces a tributary from `start` towards the main river, and
    /// records where it joins.
    fn add_tributary(
        &mut self,
        network: &mut Vec<River>,
        start: (i64, i64),
        flow: i64,
        rng: &mut Lcg,
    ) {
        if self
            .index(start.0, start.1)
            .is_none_or(|i| self.depth[i] != 0)
        {
            return;
        }
        // Aim at a point of the main river well downstream of the start,
        // so the junction points the way the water goes.
        let main = &network[0].path;
        let downstream: Vec<&Step> = main
            .iter()
            .skip(main.len() / 5)
            .take(main.len() * 3 / 5)
            .filter(|s| s.y >= (start.1 + i64::from(self.height) / 8) * 1000)
            .collect();
        if downstream.is_empty() {
            return;
        }
        let aim = downstream[usize::try_from(
            rng.next_below(u32::try_from(downstream.len()).expect("fits")),
        )
        .expect("fits")];
        let heading = heading_towards(aim.x - start.0 * 1000, aim.y - start.1 * 1000);
        let (path, end) = self.trace(start, heading, rng);
        if path.len() < 2 * Self::CLEARANCE {
            return;
        }
        let index = network.len();
        self.paint_provisional(&path, flow, index);
        if let End::Water(into, at) = end {
            network[into].joins.push((at, index));
        }
        network.push(River::new(path, flow));
    }

    /// The centre path of one river from `start`: steps of one tile, each
    /// with the heading it was taken with, until the map's edge or the
    /// water of another river.
    fn trace(&self, start: (i64, i64), heading: u8, rng: &mut Lcg) -> (Vec<Step>, End) {
        let (run_min, run_max) = Self::RUN;
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
                dir += if (drift + turn).abs() > Self::MAX_DRIFT {
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

    /// Paints a path with the section of its entry flow, so later rivers
    /// know where it runs.
    fn paint_provisional(&mut self, path: &[Step], flow: i64, river: usize) {
        let (width, depth) = Self::section(flow);
        let band: Vec<Band> = path
            .iter()
            .map(|s| Band {
                x: s.x,
                y: s.y,
                half_width: width * 500,
                depth: u8::try_from(depth).expect("shallow"),
            })
            .collect();
        self.paint(&band, river, false);
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
            if !Self::stretch_is_clear(&network[i], from, span)
                || !self.inside(&network[i], from, span)
            {
                continue;
            }
            let flow = network[i].flow_at(from, network);
            if flow < Self::DIVERGE_FLOW {
                continue;
            }
            let share = 55 + i64::from(rng.next_below(31));
            let big = flow * share / 100;
            let small = flow - big;
            if small < Self::MIN_ARM_FLOW {
                continue;
            }
            let flows = if rng.next_below(2) == 0 {
                (big, small)
            } else {
                (small, big)
            };
            let island = Self::ISLAND_WIDTHS[usize::try_from(
                rng.next_below(u32::try_from(Self::ISLAND_WIDTHS.len()).expect("few")),
            )
            .expect("few")];
            let (left, right) = (Self::section(flows.0).0, Self::section(flows.1).0);
            // The arms' inner banks must leave the island's width between
            // them; a band covers one tile more than its width, the banks
            // are rounded afterwards, and the smaller arm, if it shoals
            // into a ford, spreads to its flow in width.
            let small = flows.0.min(flows.1);
            let shoal = if small <= i64::from(Self::MAX_WIDTH) {
                small - Self::section(small).0
            } else {
                0
            };
            let offset = (left + right + shoal) * 250 + (island + 2) * 500;
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

    /// Spreads river `i` into a ford somewhere its flow allows it, clear
    /// of everything else; and shoals the smaller arm of each divergence
    /// when it is shallow enough.
    fn ford(network: &mut [River], i: usize, rng: &mut Lcg) {
        let len = network[i].path.len();
        let widest = i64::from(Self::MAX_WIDTH);
        if len > 4 * Self::CLEARANCE {
            let room = len - 2 * Self::CLEARANCE - Self::FORD_STEPS;
            for _ in 0..20 {
                let from = Self::CLEARANCE
                    + usize::try_from(rng.next_below(u32::try_from(room).expect("fits")))
                        .expect("fits");
                if network[i].flow_at(from + Self::FORD_STEPS, network) <= widest
                    && Self::stretch_is_clear(&network[i], from, Self::FORD_STEPS)
                {
                    network[i].fords.push(from);
                    break;
                }
            }
        }
    }

    /// The steps `from..=from + len` all lie inside the map, a river's
    /// width from its edges.
    fn inside(&self, river: &River, from: usize, len: usize) -> bool {
        let margin = i64::from(Self::MAX_WIDTH) * 1000;
        let (w, h) = (i64::from(self.width) * 1000, i64::from(self.height) * 1000);
        river.path[from..=from + len]
            .iter()
            .all(|s| s.x >= margin && s.y >= margin && s.x < w - margin && s.y < h - margin)
    }

    /// No junction, ford or divergence within `CLEARANCE` of the steps
    /// `from..from + len`.
    fn stretch_is_clear(river: &River, from: usize, len: usize) -> bool {
        let lo = from.saturating_sub(Self::CLEARANCE);
        let hi = from + len + Self::CLEARANCE;
        let busy = |a: usize, b: usize| a < hi && b > lo;
        !river.joins.iter().any(|(at, _)| busy(*at, at + 1))
            && !river.fords.iter().any(|f| busy(*f, f + Self::FORD_STEPS))
            && !river
                .divergences
                .iter()
                .any(|d| busy(d.from, d.from + d.len))
    }

    /// The bands of water of river `i`: the single channel between
    /// divergences, two arms through each.
    fn bands(network: &[River], i: usize) -> Vec<Vec<Band>> {
        let river = &network[i];
        let band_at = |step: usize, flow: i64, offset: i64| {
            let s = river.path[step];
            let (width, depth) = Self::section(flow);
            let h = HEADINGS[usize::from(s.heading)];
            Band {
                x: s.x - h.1 * offset / 1000,
                y: s.y + h.0 * offset / 1000,
                half_width: width * 500,
                depth: u8::try_from(depth).expect("shallow"),
            }
        };
        let mut divergences: Vec<&Divergence> = river.divergences.iter().collect();
        divergences.sort_by_key(|d| d.from);
        let mut out = Vec::new();
        let mut channel: Vec<Band> = Vec::new();
        let mut step = 0;
        for d in divergences {
            while step <= d.from {
                channel.push(band_at(step, river.flow_at(step, network), 0));
                step += 1;
            }
            out.push(std::mem::take(&mut channel));
            let (mut left, mut right) = (Vec::new(), Vec::new());
            for t in 0..=d.len {
                let rise = t.min(d.len - t).min(d.ramp);
                let offset = d.offset * i64::try_from(rise).expect("fits")
                    / i64::try_from(d.ramp).expect("fits");
                left.push(band_at(d.from + t, d.flows.0, -offset));
                right.push(band_at(d.from + t, d.flows.1, offset));
            }
            out.push(left);
            out.push(right);
            step = d.from + d.len;
        }
        while step < river.path.len() {
            channel.push(band_at(step, river.flow_at(step, network), 0));
            step += 1;
        }
        out.push(channel);
        out
    }

    /// The bands of river `i` that are fords: its own fords, spread to
    /// one deep right across, and the shoaling smaller arm of each
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
                half_width: width * 500,
                depth: 1,
            }
        };
        // A ford is a lens: the river's own width at each end, the flow's
        // width (one deep right across) in the middle.
        let lens = |from: usize, flow: i64, offset: i64| -> Vec<Band> {
            let spread = flow.max(i64::from(Self::MIN_WIDTH));
            let own = Self::section(flow).0;
            let ramp = Self::FORD_RAMP;
            (0..Self::FORD_STEPS + 2 * ramp)
                .map(|t| {
                    let width = if t < ramp || t >= ramp + Self::FORD_STEPS {
                        own
                    } else {
                        spread
                    };
                    shoal(from + t - ramp, offset, width)
                })
                .collect()
        };
        let mut out = Vec::new();
        for from in &river.fords {
            let flow = river.flow_at(from + Self::FORD_STEPS, network);
            out.push(lens(*from, flow, 0));
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
            out.push(lens(middle, flow, sign * d.offset));
        }
        out
    }

    /// Paints water under a band: every tile whose centre is within the
    /// local half-width. Deeper water wins over shallower; a tile keeps
    /// the first river that reached it. A ford (`shoal`) sets one deep
    /// across, only on land and on its own river's water.
    fn paint(&mut self, band: &[Band], river: usize, shoal: bool) {
        let mine = u16::try_from(river + 1).expect("few rivers");
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
                    if d2 > hw * hw {
                        continue;
                    }
                    if shoal {
                        if self.owner[i] == 0 || self.owner[i] == mine {
                            self.depth[i] = 1;
                            self.owner[i] = mine;
                        }
                    } else {
                        let depth = if t < 500 { a.depth } else { b.depth };
                        self.depth[i] = self.depth[i].max(depth);
                        if self.owner[i] == 0 {
                            self.owner[i] = mine;
                            self.step[i] = u32::try_from(n).expect("fits");
                        }
                    }
                }
            }
        }
    }

    /// Rounds the banks: a land tile with water on three of its four sides
    /// (the tip of a tongue at a junction, a pimple on a bank) becomes
    /// water, a water tile with land on three sides becomes land. Two
    /// passes, so thin tongues erode but nothing drifts further.
    fn round_banks(&mut self) {
        for _ in 0..2 {
            let before = self.depth.clone();
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
                    if before[i] == 0 && wet >= 3 {
                        self.depth[i] = shallowest;
                    } else if before[i] > 0 && wet <= 1 {
                        self.depth[i] = 0;
                    }
                }
            }
        }
    }

    /// Labels the land regions below `cliff_rows` (eight-way, one deep
    /// being walkable): per tile its region, `usize::MAX` on water, and
    /// the size of every region.
    fn land_regions(&self, cliff_rows: u32) -> (Vec<usize>, Vec<usize>) {
        let (width, height) = (i64::from(self.width), i64::from(self.height));
        let top = i64::from(cliff_rows);
        let mut region = vec![usize::MAX; self.depth.len()];
        let mut sizes: Vec<usize> = Vec::new();
        for start in usize::try_from(top * width).expect("fits")..self.depth.len() {
            if region[start] != usize::MAX || self.depth[start] > 1 {
                continue;
            }
            let id = sizes.len();
            let mut size = 0;
            let mut stack = vec![start];
            while let Some(i) = stack.pop() {
                if region[i] != usize::MAX || self.depth[i] > 1 {
                    continue;
                }
                region[i] = id;
                size += 1;
                let flat = i64::try_from(i).expect("fits");
                let (x, y) = (flat % width, flat / width);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (nx, ny) = (x + dx, y + dy);
                        if ny >= top && ny < height && nx >= 0 && nx < width {
                            stack.push(usize::try_from(ny * width + nx).expect("fits"));
                        }
                    }
                }
            }
            sizes.push(size);
        }
        (region, sizes)
    }

    /// Fords wherever the land would otherwise be split: the land regions
    /// (eight-way, one deep being walkable) are joined to the largest one
    /// by the shallowest crossing of water between them, set one deep,
    /// and merged into it, until every large region is joined. Small
    /// islands (fewer than 20 tiles) stay as they are.
    fn connect_regions(&mut self, cliff_rows: u32) {
        const BIG: usize = 20;
        let (width, height) = (i64::from(self.width), i64::from(self.height));
        let top = i64::from(cliff_rows);
        let neighbours = |i: usize| {
            let flat = i64::try_from(i).expect("fits");
            let (x, y) = (flat % width, flat / width);
            [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .filter_map(move |(dx, dy)| {
                    let (nx, ny) = (x + dx, y + dy);
                    (ny >= top && ny < height && nx >= 0 && nx < width)
                        .then(|| usize::try_from(ny * width + nx).expect("fits"))
                })
        };
        let (mut region, mut sizes) = self.land_regions(cliff_rows);
        let Some(main) = (0..sizes.len()).max_by_key(|r| sizes[*r]) else {
            return;
        };
        while (0..sizes.len()).any(|r| r != main && sizes[r] >= BIG) {
            // From the main region's banks, the shallowest walk over water
            // to any other big region: Dijkstra, a tile costing its depth.
            let mut from = vec![usize::MAX; self.depth.len()];
            let mut cost = vec![i64::MAX; self.depth.len()];
            let mut queue = std::collections::BinaryHeap::new();
            for i in 0..self.depth.len() {
                if region[i] == main && neighbours(i).any(|j| self.depth[j] > 1) {
                    from[i] = i;
                    cost[i] = 0;
                    queue.push(std::cmp::Reverse((0_i64, i)));
                }
            }
            let mut reached = None;
            while let Some(std::cmp::Reverse((c, i))) = queue.pop() {
                if c > cost[i] {
                    continue;
                }
                if region[i] != usize::MAX && region[i] != main {
                    if sizes[region[i]] >= BIG {
                        reached = Some(i);
                        break;
                    }
                    // Land of a small island: not a way through.
                    continue;
                }
                for j in neighbours(i) {
                    if region[j] == main {
                        continue;
                    }
                    let next = c + i64::from(self.depth[j]);
                    if next < cost[j] {
                        cost[j] = next;
                        from[j] = i;
                        queue.push(std::cmp::Reverse((next, j)));
                    }
                }
            }
            let Some(mut at) = reached else { return };
            // Set the crossing one deep, two tiles wide, and merge the
            // region reached into the main one.
            let joined = region[at];
            while from[at] != at {
                let flat = i64::try_from(at).expect("fits");
                let (x, y) = (flat % width, flat / width);
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    if let Some(k) = self.index(x + dx, y + dy).filter(|k| self.depth[*k] > 1) {
                        self.depth[k] = 1;
                        region[k] = main;
                    }
                }
                at = from[at];
            }
            for r in &mut region {
                if *r == joined {
                    *r = main;
                }
            }
            sizes[main] += sizes[joined];
            sizes[joined] = 0;
        }
    }
}

impl River {
    fn new(path: Vec<Step>, entry_flow: i64) -> Self {
        Self {
            path,
            entry_flow,
            joins: Vec::new(),
            end_flow: entry_flow,
            divergences: Vec::new(),
            fords: Vec::new(),
        }
    }
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
                let expected = match d {
                    0 => None,
                    1 => Some(Terrain::Ford),
                    _ => Some(Terrain::Water),
                };
                assert_eq!(r.at(x, y), expected);
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
        // A tile one deep holds one cube: width × depth covers the flow.
        for flow in 1..200 {
            let (width, depth) = Rivers::section(flow);
            assert!((3..=12).contains(&width), "flow {flow}: width {width}");
            assert!((1..=4).contains(&depth), "flow {flow}: depth {depth}");
            if flow <= 48 {
                assert!(width * depth >= flow, "flow {flow}: {width} × {depth}");
            }
            // The width never shrinks as the flow grows.
            assert!(Rivers::section(flow + 1).0 >= width, "flow {flow}");
        }
        assert_eq!(Rivers::section(6), (4, 2));
        assert_eq!(Rivers::section(12), (6, 2));
        assert_eq!(Rivers::section(24), (8, 3));
        assert_eq!(Rivers::section(48), (12, 4));
        // The whole river, one deep: only a small flow can spread so.
        assert_eq!(Rivers::section(3), (3, 1));
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
                    _ => assert_eq!(depth, 0, "{pos:?}"),
                }
            }
        }
        assert!(deep >= 3, "the main river gets deep: {deep}");
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
