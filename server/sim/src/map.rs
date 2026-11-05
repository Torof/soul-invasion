//! The valley: a grid of tiles.
//!
//! Milestone 1 only needs ground to walk on. Terrain is deliberately
//! minimal; materials, resources and buildings come as content later (D15).

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

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

    /// The four orthogonal neighbours, in a fixed order (determinism).
    fn neighbours(self) -> [Self; 4] {
        [
            Self::new(self.x, self.y - 1),
            Self::new(self.x + 1, self.y),
            Self::new(self.x, self.y + 1),
            Self::new(self.x - 1, self.y),
        ]
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

/// The grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Map {
    width: u32,
    height: u32,
    tiles: Vec<Terrain>,
}

impl Map {
    /// A map filled with one terrain.
    pub fn filled(width: u32, height: u32, terrain: Terrain) -> Self {
        Self {
            width,
            height,
            tiles: vec![terrain; (width * height) as usize],
        }
    }

    /// The first version's valley: a cliff to the north, a river running
    /// south with one ford, forest patches, grass elsewhere. Deterministic
    /// for a given seed.
    pub fn valley(width: u32, height: u32, seed: u64) -> Self {
        let mut map = Self::filled(width, height, Terrain::Grass);
        let mut rng = Lcg(seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1));

        let cliff_rows = (height / 8).max(2);
        let river_x = i32::try_from(width / 3).expect("small map");
        let ford_y = i32::try_from(height / 2).expect("small map");

        for y in 0..height {
            for x in 0..width {
                let pos = Pos::new(
                    i32::try_from(x).expect("small map"),
                    i32::try_from(y).expect("small map"),
                );
                let terrain = if y < cliff_rows {
                    Terrain::Rock
                } else if (pos.x - river_x).abs() <= 1 {
                    if (pos.y - ford_y).abs() <= 1 {
                        Terrain::Ford
                    } else {
                        Terrain::Water
                    }
                } else if rng.next_below(100) < 18 {
                    Terrain::Forest
                } else {
                    Terrain::Grass
                };
                map.set(pos, terrain);
            }
        }
        map
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

    /// The terrain at `pos`, or `None` outside the map.
    pub fn get(&self, pos: Pos) -> Option<Terrain> {
        self.index(pos).map(|i| self.tiles[i])
    }

    /// Sets the terrain at `pos`; ignored outside the map.
    pub fn set(&mut self, pos: Pos, terrain: Terrain) {
        if let Some(i) = self.index(pos) {
            self.tiles[i] = terrain;
        }
    }

    /// Can a body stand at `pos`?
    pub fn is_passable(&self, pos: Pos) -> bool {
        self.get(pos).is_some_and(Terrain::is_passable)
    }

    /// Shortest walking path from `from` to `to` (orthogonal steps), not
    /// including `from`. Empty when already there; `None` when unreachable.
    pub fn path(&self, from: Pos, to: Pos) -> Option<Vec<Pos>> {
        if from == to {
            return Some(Vec::new());
        }
        if !self.is_passable(from) || !self.is_passable(to) {
            return None;
        }
        // Breadth-first search; the map is small and the order is fixed.
        let mut came_from: Vec<Option<Pos>> = vec![None; self.tiles.len()];
        let mut queue = VecDeque::from([from]);
        let start = self.index(from)?;
        came_from[start] = Some(from);
        while let Some(current) = queue.pop_front() {
            if current == to {
                let mut path = vec![to];
                let mut step = to;
                while let Some(previous) = came_from[self.index(step)?] {
                    if previous == from {
                        break;
                    }
                    path.push(previous);
                    step = previous;
                }
                path.reverse();
                return Some(path);
            }
            for next in current.neighbours() {
                if let Some(i) = self.index(next)
                    && came_from[i].is_none()
                    && self.tiles[i].is_passable()
                {
                    came_from[i] = Some(current);
                    queue.push_back(next);
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
}

/// A tiny deterministic generator, so map generation needs no dependency
/// and gives the same valley on every machine.
struct Lcg(u64);

impl Lcg {
    fn next_below(&mut self, bound: u32) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        u32::try_from((self.0 >> 33) % u64::from(bound)).expect("below bound")
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
        let a = Map::valley(32, 32, 7);
        let b = Map::valley(32, 32, 7);
        assert_eq!(a, b);
        assert_ne!(a, Map::valley(32, 32, 8));
        // Cliff on top, river with a ford, and the two banks are connected.
        assert_eq!(a.get(Pos::new(5, 0)), Some(Terrain::Rock));
        assert_eq!(a.get(Pos::new(10, 5)), Some(Terrain::Water));
        assert_eq!(a.get(Pos::new(10, 16)), Some(Terrain::Ford));
        assert!(a.path(Pos::new(2, 20), Pos::new(28, 20)).is_some());
    }
}
