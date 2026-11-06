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

    /// The first version's valley: a cliff to the north, a river running
    /// south with one ford, groves of forest, grass elsewhere.
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
        let river_x = i32::try_from(width / 3).expect("map fits in i32");
        let river_half_width = i32::try_from(width / 80).expect("map fits in i32").max(1);
        let ford_y = i32::try_from(height / 2).expect("map fits in i32");
        let ford_half_length = i32::try_from(height / 64).expect("map fits in i32").max(1);
        // The river meanders: wide slow bends, small wobbles on top, and a
        // width that breathes, all from the seed.
        let bends = Meander::new(
            height,
            seed.wrapping_add(1),
            48,
            i32::try_from(width / 10).expect("map fits in i32"),
        );
        let wobble = Meander::new(height, seed.wrapping_add(2), 12, 3);
        let breath = Meander::new(height, seed.wrapping_add(3), 20, 1);

        for y in 0..height {
            for x in 0..width {
                let pos = Pos::new(
                    i32::try_from(x).expect("map fits in i32"),
                    i32::try_from(y).expect("map fits in i32"),
                );
                let centre = river_x + bends.at(y) + wobble.at(y);
                let half_width = (river_half_width + breath.at(y)).max(1);
                let terrain = if y < cliff_rows {
                    Terrain::Rock
                } else if (pos.x - centre).abs() <= half_width {
                    if (pos.y - ford_y).abs() <= ford_half_length {
                        Terrain::Ford
                    } else {
                        Terrain::Water
                    }
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

/// A smooth signed wander along one axis: lattice values every `cell`
/// rows, interpolated with an integer smoothstep. Deterministic from the
/// seed, like the groves.
struct Meander {
    cell: u32,
    values: Vec<i32>,
}

impl Meander {
    fn new(length: u32, seed: u64, cell: u32, amplitude: i32) -> Self {
        let mut rng = Lcg::new(seed);
        let span = u32::try_from(2 * amplitude + 1).expect("small amplitude");
        let values = (0..length / cell + 2)
            .map(|_| i32::try_from(rng.next_below(span)).expect("small") - amplitude)
            .collect();
        Self { cell, values }
    }

    fn at(&self, y: u32) -> i32 {
        let index = (y / self.cell) as usize;
        let within = i64::from(y % self.cell) * 1000 / i64::from(self.cell);
        let smooth = within * within * (3000 - 2 * within) / 1_000_000;
        let before = i64::from(self.values[index]);
        let after = i64::from(self.values[index + 1]);
        i32::try_from((before * (1000 - smooth) + after * smooth) / 1000).expect("small")
    }
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
        // Cliff on top, river with a ford, and the two banks are connected.
        assert_eq!(a.get(Pos::new(5, 0)), Some(Terrain::Rock));
        let first_of = |row: i32, terrain: Terrain| {
            (0..256).find(|x| a.get(Pos::new(*x, row)) == Some(terrain))
        };
        assert!(first_of(60, Terrain::Water).is_some());
        assert!(first_of(128, Terrain::Ford).is_some());
        // The river meanders: its bank is not at the same x on every row.
        let banks: Vec<Option<i32>> = (40..220)
            .step_by(20)
            .map(|row| first_of(row, Terrain::Water))
            .collect();
        assert!(banks.iter().any(|b| *b != banks[0]), "{banks:?}");
        let west = a.nearest(Pos::new(20, 200), Terrain::Grass).unwrap();
        let east = a.nearest(Pos::new(240, 200), Terrain::Grass).unwrap();
        let path = a.path(west, east, walk).unwrap();
        assert!(path.iter().any(|p| a.get(*p) == Some(Terrain::Ford)));
        // Trees come in groves: between a tenth and half of the ground.
        let forest = a.tiles().iter().filter(|t| **t == Terrain::Forest).count();
        assert!((6_000..33_000).contains(&forest), "{forest} forest tiles");
    }

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
