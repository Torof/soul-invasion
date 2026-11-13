//! The rock under the ground (D42): granite, limestone, chalk or
//! sandstone, a region each, from the generator's version 2 on (D24).
//! It decides what comes next on the rocks' page (cliffs, shelters,
//! caves, boulders, flint, clay); today it only tints the ground.
//!
//! The regions are the cells of a coarse grid (`region_m`, 2 km: a real
//! region is 10 to 100 km, so the game compresses them five to fifty
//! times, a named content factor). Each cell has a **site**, anywhere in
//! it, and a rock drawn by the content's weights; a tile
//! belongs to the nearest site, so regions are irregular polygons and two
//! cells of the same rock make one region. Before the nearest site is
//! sought the tile is **shifted** by a smooth noise of `border_cell_m`
//! (100 m), up to `border_shift_m` each way, so the borders wander
//! instead of running straight.
//!
//! Integer only, from the seed and absolute coordinates alone (D24's
//! scheme): any chunk, alone and in any order, gets the same rocks, and a
//! chunk is worked out by itself, never the whole map ([`Geology::rocks_in`],
//! held by `ground::Ground`). Nothing of it is stored.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::r#gen::scatter::{purpose, roll, weighted};
use crate::r#gen::{lattice, noise_by};
use crate::map::{CHUNK_SIZE, ChunkPos};

/// The rock a place stands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rock {
    /// Hard and crystalline: rounded pinkish-grey boulders, few shelters.
    Granite,
    /// Pale, layered: shelters, and the only caves; some flint.
    Limestone,
    /// Soft and white, low cliffs: the flint's rock.
    Chalk,
    /// Ochre to red: shelters, ochre, polishing stones.
    Sandstone,
}

impl Rock {
    /// Every rock, in the order of their weights and codes.
    pub const ALL: [Self; 4] = [Self::Granite, Self::Limestone, Self::Chalk, Self::Sandstone];
}

/// How the rock regions are drawn: `content/geology.ron` (D27).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Geology {
    /// Side of the coarse grid's cells, in metres (tiles): a region's
    /// usual breadth.
    pub region_m: u32,
    /// Cell of the noise that shifts the borders, in metres.
    pub border_cell_m: u32,
    /// How far the borders are shifted at most, each way, in metres.
    pub border_shift_m: u32,
    /// How often each rock is a region's, against the others. Every rock
    /// is listed.
    pub weights: BTreeMap<Rock, u32>,
}

/// The placeholder values of `content/geology.ron`, for content made
/// without the file (the tests' sample).
impl Default for Geology {
    fn default() -> Self {
        Self {
            region_m: 2000,
            border_cell_m: 100,
            border_shift_m: 150,
            weights: BTreeMap::from([
                (Rock::Granite, 30),
                (Rock::Limestone, 30),
                (Rock::Chalk, 20),
                (Rock::Sandstone, 20),
            ]),
        }
    }
}

/// A coarse cell's site, where its region is centred, and its rock.
#[derive(Debug, Clone, Copy)]
struct Site {
    x: i64,
    y: i64,
    rock: Rock,
}

/// A small box of values on a lattice, drawn once for a chunk.
struct Window<T> {
    i0: i64,
    j0: i64,
    columns: i64,
    values: Vec<T>,
}

impl<T: Copy> Window<T> {
    /// The values of the lattice points `i0..=i1` × `j0..=j1`.
    fn new(i0: i64, j0: i64, i1: i64, j1: i64, value: impl Fn(i64, i64) -> T) -> Self {
        let columns = i1 - i0 + 1;
        let values = (j0..=j1)
            .flat_map(|j| (i0..=i1).map(move |i| (i, j)))
            .map(|(i, j)| value(i, j))
            .collect();
        Self {
            i0,
            j0,
            columns,
            values,
        }
    }

    fn at(&self, i: i64, j: i64) -> T {
        let k = (j - self.j0) * self.columns + (i - self.i0);
        self.values[usize::try_from(k).expect("within the window")]
    }
}

impl Geology {
    /// What is wrong with these values, if anything.
    ///
    /// # Errors
    ///
    /// A region under 64 m, a border noise cell of 0, a shift as wide as a
    /// region, a rock not listed, or every weight 0.
    pub fn check(&self) -> Result<(), &'static str> {
        if !(64..=1_000_000).contains(&self.region_m) {
            return Err("region_m must be 64 to 1,000,000 metres");
        }
        if !(1..=1_000_000).contains(&self.border_cell_m) {
            return Err("border_cell_m must be 1 to 1,000,000 metres");
        }
        if self.border_shift_m >= self.region_m {
            return Err("border_shift_m must be under region_m");
        }
        if Rock::ALL.iter().any(|r| !self.weights.contains_key(r)) {
            return Err("every rock must have a weight");
        }
        if self.weights.values().all(|w| *w == 0) {
            return Err("some rock must weigh more than 0");
        }
        Ok(())
    }

    /// The rock under tile `(x, y)` of a world of `seed`.
    #[must_use]
    pub fn rock_at(&self, seed: u64, x: i32, y: i32) -> Rock {
        let borders = Self::border_seeds(seed);
        self.rock_by(
            x,
            y,
            |k, i, j| lattice(borders[k], i, j),
            |i, j| self.site(seed, i, j),
        )
    }

    /// The rock under each tile of `chunk`, row by row from its north-west
    /// corner, as [`Geology::rock_at`] gives them: the few lattice points
    /// and sites the chunk needs are drawn once.
    #[must_use]
    pub fn rocks_in(&self, seed: u64, chunk: ChunkPos) -> Vec<Rock> {
        let side = i32::try_from(CHUNK_SIZE).expect("small chunk side");
        let origin = chunk.origin();
        let (x0, y0) = (i64::from(origin.x), i64::from(origin.y));
        let (x1, y1) = (x0 + i64::from(side) - 1, y0 + i64::from(side) - 1);
        let borders = Self::border_seeds(seed);
        let cell = i64::from(self.border_cell_m);
        let noise: [Window<i64>; 2] = borders.map(|b| {
            Window::new(
                x0.div_euclid(cell),
                y0.div_euclid(cell),
                x1.div_euclid(cell) + 1,
                y1.div_euclid(cell) + 1,
                |i, j| lattice(b, i, j),
            )
        });
        let region = i64::from(self.region_m);
        let shift = i64::from(self.border_shift_m);
        let sites = Window::new(
            (x0 - shift).div_euclid(region) - 2,
            (y0 - shift).div_euclid(region) - 2,
            (x1 + shift).div_euclid(region) + 2,
            (y1 + shift).div_euclid(region) + 2,
            |i, j| self.site(seed, i, j),
        );
        (0..side)
            .flat_map(|dy| (0..side).map(move |dx| (dx, dy)))
            .map(|(dx, dy)| {
                self.rock_by(
                    origin.x + dx,
                    origin.y + dy,
                    |k, i, j| noise[k].at(i, j),
                    |i, j| sites.at(i, j),
                )
            })
            .collect()
    }

    /// The seeds of the two border noises, east and south.
    fn border_seeds(seed: u64) -> [u64; 2] {
        [
            roll(seed, purpose::ROCK_BORDER_X, &[]),
            roll(seed, purpose::ROCK_BORDER_Y, &[]),
        ]
    }

    /// The rock under `(x, y)`, the border noises' lattice points read
    /// from `border(k, i, j)` (k: 0 east, 1 south) and the coarse cells'
    /// sites from `site(i, j)`.
    fn rock_by(
        &self,
        x: i32,
        y: i32,
        border: impl Fn(usize, i64, i64) -> i64,
        site: impl Fn(i64, i64) -> Site,
    ) -> Rock {
        let shift = |k: usize| {
            let n = i64::from(noise_by(self.border_cell_m, x, y, |i, j| border(k, i, j)));
            (n - 500) * i64::from(self.border_shift_m) / 500
        };
        let (px, py) = (i64::from(x) + shift(0), i64::from(y) + shift(1));
        let region = i64::from(self.region_m);
        let (ci, cj) = (px.div_euclid(region), py.div_euclid(region));
        // A site lies in its cell, so the nearest one is always among the
        // 25 cells around: one three cells away is at least two cells
        // off, the own cell's at most 1.42.
        let mut best: Option<(i64, Rock)> = None;
        for j in cj - 2..=cj + 2 {
            for i in ci - 2..=ci + 2 {
                let s = site(i, j);
                let d = (s.x - px).pow(2) + (s.y - py).pow(2);
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, s.rock));
                }
            }
        }
        best.expect("25 sites").1
    }

    /// Coarse cell `(i, j)`'s site, anywhere in it, and its rock.
    #[allow(clippy::cast_sign_loss, reason = "cell coordinates as keys")]
    fn site(&self, seed: u64, i: i64, j: i64) -> Site {
        let region = i64::from(self.region_m);
        let span = u64::from(self.region_m);
        let draw = |k: u64| roll(seed, purpose::ROCK_REGION, &[i as u64, j as u64, k]);
        let within = |k: u64| i64::try_from(draw(k) % span).expect("within a cell");
        let weights = Rock::ALL.map(|r| u64::from(self.weights.get(&r).copied().unwrap_or(0)));
        Site {
            x: i * region + within(0),
            y: j * region + within(1),
            rock: weighted(&weights, draw(2)).map_or(Rock::Granite, |k| Rock::ALL[k]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunk_has_the_rocks_of_its_tiles_across_its_borders() {
        // Small regions, so a few chunks hold borders.
        let geology = Geology {
            region_m: 96,
            border_cell_m: 20,
            border_shift_m: 20,
            ..Geology::default()
        };
        let mut borders = 0;
        for (cx, cy) in [(0, 0), (1, 0), (0, 1), (5, 3), (-2, -1), (7, 7)] {
            let chunk = ChunkPos::new(cx, cy);
            let rocks = geology.rocks_in(9, chunk);
            let origin = chunk.origin();
            for (k, rock) in rocks.iter().enumerate() {
                let k = i32::try_from(k).unwrap();
                let (dx, dy) = (k % 32, k / 32);
                assert_eq!(
                    *rock,
                    geology.rock_at(9, origin.x + dx, origin.y + dy),
                    "{chunk:?} tile {k}"
                );
            }
            if rocks.iter().any(|r| *r != rocks[0]) {
                borders += 1;
            }
            assert_eq!(rocks, geology.rocks_in(9, chunk), "the same every time");
        }
        assert!(borders >= 1, "some chunk holds a border");
    }

    #[test]
    fn the_rocks_follow_the_seed() {
        let geology = Geology::default();
        let a: Vec<Rock> = (0..40)
            .map(|k| geology.rock_at(1, k * 400, k * 150))
            .collect();
        assert_eq!(
            a,
            (0..40)
                .map(|k| geology.rock_at(1, k * 400, k * 150))
                .collect::<Vec<_>>()
        );
        let b: Vec<Rock> = (0..40)
            .map(|k| geology.rock_at(2, k * 400, k * 150))
            .collect();
        assert_ne!(a, b, "another seed, other regions");
    }

    #[test]
    fn a_six_km_world_shows_every_rock() {
        // Sampled every 32 m, as a 6 km world of the game's seed.
        let geology = Geology::default();
        let mut count = BTreeMap::<Rock, u32>::new();
        for y in (0..6144).step_by(32) {
            for x in (0..6144).step_by(32) {
                *count
                    .entry(geology.rock_at(crate::VALLEY_SEED, x, y))
                    .or_default() += 1;
            }
        }
        for rock in Rock::ALL {
            assert!(
                count.get(&rock).copied().unwrap_or(0) >= 192 * 192 / 50,
                "{rock:?} holds 2 % of the world at least: {count:?}"
            );
        }
    }

    #[test]
    fn the_default_is_valid_and_bad_values_are_refused() {
        assert_eq!(Geology::default().check(), Ok(()));
        let mut missing = Geology::default();
        missing.weights.remove(&Rock::Chalk);
        assert!(missing.check().is_err());
        let mut none = Geology::default();
        none.weights.values_mut().for_each(|w| *w = 0);
        assert!(none.check().is_err());
        let wide = Geology {
            border_shift_m: 2000,
            ..Geology::default()
        };
        assert!(wide.check().is_err());
    }
}
