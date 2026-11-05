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
use std::collections::BinaryHeap;

use serde::{Deserialize, Serialize};

/// Side of a chunk, in tiles.
pub const CHUNK_SIZE: u32 = 32;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// Can a body stand here?
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

        for y in 0..height {
            for x in 0..width {
                let pos = Pos::new(
                    i32::try_from(x).expect("map fits in i32"),
                    i32::try_from(y).expect("map fits in i32"),
                );
                let terrain = if y < cliff_rows {
                    Terrain::Rock
                } else if (pos.x - river_x).abs() <= river_half_width {
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

    /// Cheapest walking path from `from` to `to`, not including `from`.
    /// Steps go in eight directions; a diagonal step costs √2 straight
    /// ones and never cuts a corner (both straight neighbours it passes
    /// between must be passable) (D32). Empty when already there; `None`
    /// when unreachable.
    pub fn path(&self, from: Pos, to: Pos) -> Option<Vec<Pos>> {
        if from == to {
            return Some(Vec::new());
        }
        if !self.is_passable(from) || !self.is_passable(to) {
            return None;
        }
        // A* with an octile heuristic. Ties break on insertion order so
        // the path is the same on every machine.
        let start = self.index(from)?;
        let goal = self.index(to)?;
        let mut best = vec![u32::MAX; self.tiles.len()];
        let mut came_from: Vec<Option<usize>> = vec![None; self.tiles.len()];
        let mut open = BinaryHeap::new();
        let mut pushed = 0u64;
        best[start] = 0;
        open.push(Reverse((octile(from, to), pushed, start)));

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
                if !self.tiles[i].is_passable() {
                    continue;
                }
                let step_cost = if here.is_diagonal_to(next) {
                    if !self.is_passable(Pos::new(here.x, next.y))
                        || !self.is_passable(Pos::new(next.x, here.y))
                    {
                        continue; // no cutting corners
                    }
                    DIAGONAL_COST
                } else {
                    STRAIGHT_COST
                };
                let cost = cost_here + step_cost;
                if cost < best[i] {
                    best[i] = cost;
                    came_from[i] = Some(current);
                    pushed += 1;
                    open.push(Reverse((cost + octile(next, to), pushed, i)));
                }
            }
        }
        None
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

/// Lower bound on the walking cost between two tiles with eight-way steps.
fn octile(a: Pos, b: Pos) -> u32 {
    let dx = a.x.abs_diff(b.x);
    let dy = a.y.abs_diff(b.y);
    STRAIGHT_COST * dx.max(dy) + (DIAGONAL_COST - STRAIGHT_COST) * dx.min(dy)
}

/// A tiny deterministic generator, so map generation needs no dependency
/// and gives the same valley on every machine.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1))
    }

    fn next_below(&mut self, bound: u32) -> u32 {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let path = map.path(Pos::new(0, 0), Pos::new(2, 0)).unwrap();
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
            open.path(Pos::new(0, 0), Pos::new(3, 3)).unwrap(),
            vec![Pos::new(1, 1), Pos::new(2, 2), Pos::new(3, 3)]
        );
        // A knight's move: two straight-ish steps, one of them diagonal.
        assert_eq!(open.path(Pos::new(0, 0), Pos::new(2, 1)).unwrap().len(), 2);

        // G R
        // R G   — (0,0) to (1,1) would slip between two rocks: refused.
        let mut pinched = Map::filled(2, 2, Terrain::Grass);
        pinched.set(Pos::new(1, 0), Terrain::Rock);
        pinched.set(Pos::new(0, 1), Terrain::Rock);
        assert_eq!(pinched.path(Pos::new(0, 0), Pos::new(1, 1)), None);

        // G R
        // G G   — one rock only: still no diagonal past its corner.
        let mut corner = Map::filled(2, 2, Terrain::Grass);
        corner.set(Pos::new(1, 0), Terrain::Rock);
        assert_eq!(
            corner.path(Pos::new(0, 0), Pos::new(1, 1)).unwrap(),
            vec![Pos::new(0, 1), Pos::new(1, 1)]
        );
    }

    #[test]
    fn paths_are_cheapest_not_merely_shortest_in_steps() {
        // Going straight 4 tiles costs 40; a 4-diagonal detour would cost 56.
        let open = Map::filled(6, 6, Terrain::Grass);
        let path = open.path(Pos::new(0, 2), Pos::new(4, 2)).unwrap();
        assert_eq!(path.len(), 4);
        assert!(path.iter().all(|p| p.y == 2), "stays on the row: {path:?}");
    }

    #[test]
    fn unreachable_and_trivial_paths() {
        let mut map = Map::filled(3, 1, Terrain::Grass);
        map.set(Pos::new(1, 0), Terrain::Rock);
        assert_eq!(map.path(Pos::new(0, 0), Pos::new(2, 0)), None);
        assert_eq!(map.path(Pos::new(0, 0), Pos::new(1, 0)), None);
        assert_eq!(map.path(Pos::new(0, 0), Pos::new(0, 0)), Some(vec![]));
        assert_eq!(map.path(Pos::new(0, 0), Pos::new(9, 9)), None);
    }

    #[test]
    fn valley_is_deterministic_and_crossable() {
        let a = Map::valley(256, 256, 7);
        let b = Map::valley(256, 256, 7);
        assert_eq!(a, b);
        assert_ne!(a, Map::valley(256, 256, 8));
        // Cliff on top, river with a ford, and the two banks are connected.
        assert_eq!(a.get(Pos::new(5, 0)), Some(Terrain::Rock));
        assert_eq!(a.get(Pos::new(85, 60)), Some(Terrain::Water));
        assert_eq!(a.get(Pos::new(85, 128)), Some(Terrain::Ford));
        let west = a.nearest(Pos::new(20, 200), Terrain::Grass).unwrap();
        let east = a.nearest(Pos::new(240, 200), Terrain::Grass).unwrap();
        let path = a.path(west, east).unwrap();
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
