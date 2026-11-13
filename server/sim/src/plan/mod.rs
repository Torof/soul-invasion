//! The atlas: a continent born from its seed, outside the game
//! (`content/ai-reserve/review/monde-genere/`, the project lead's yes of
//! 13 November 2026). Nothing in the game reads it yet: the valley keeps
//! generators 1 and 2 (D24); this is the first step toward a third.
//!
//! A continent is told as a history, in this order, each pass reading
//! only the ones before it:
//!
//! 1. **The land and its uplift** ([`relief`]): an outline from a warped
//!    noise, cut so a fixed share of the grid is land and the rest sea;
//!    an uplift made of a broad swell and of [`Stroke`]s (ranges,
//!    plateaus, basins), drawn from the seed when none is given. The
//!    strokes are the entry point for the lead's own lines later.
//! 2. **Erosion by the rivers** ([`relief`]): the stream-power law,
//!    solved implicitly in O(n) a pass (Braun and Willett 2013), so the
//!    relief and the river network come out together. Each pass routes
//!    the water with a priority flood (Barnes, Lehman and Mulla 2014),
//!    which fills the hollows and gives the order the solver needs.
//! 3. **The rock, from the relief's history** ([`relief`]): three covers
//!    (chalk, limestone, sandstone) over a granite basement; what the
//!    erosion cut through decides what shows, and the rock in turn sets
//!    how fast the river cuts.
//! 4. **The lakes**: the hollows the last flood filled, deep enough.
//! 5. **The climate** ([`climate`]): temperature by latitude and height,
//!    rain carried by the prevailing winds, wrung out on the way up a
//!    range and withheld on the way down (rain shadows).
//! 6. **The discharge and the biomes** ([`climate`]): what the rain
//!    leaves after evaporation, gathered down the network; a biome from
//!    warmth, wetness and the lie of the land.
//!
//! The [`region`] refines a 64 km square of it at 64 m; [`measure`]
//! counts what the project lead judges a seed by.
//!
//! Integers only, from the first day (D24): a map the lead approves must
//! come out the same once it enters the game. Every draw is a hash of the
//! seed, a purpose and coordinates (D24, 10 November); ties are broken by
//! a fixed order, never by a heap's whims. Every tunable number is a named
//! constant with its reason, in the module that uses it.

// Grid code reads as arithmetic: `i`, `j`, `x`, `y`, `c`, `r`, `h`, `q`
// are the usual names; loops walk several layers by the same index; a
// grid is at most 8,192 cells a side (asserted in `Continent::with`), so
// its indices and sides fit an `i32` and an `i64`.
#![allow(
    clippy::many_single_char_names,
    clippy::needless_range_loop,
    clippy::cast_possible_wrap
)]

pub mod climate;
pub mod measure;
pub mod region;
pub mod relief;
pub mod window;

use crate::r#gen::hash;
use crate::geology::Rock;

pub use climate::Biome;
pub use measure::Numbers;
pub use region::Region;
pub use relief::{Stroke, StrokeKind};
pub use window::Window;

/// Side of a continent cell, in metres: 64 chunks of 32 m, so a chunk
/// never straddles two cells (`pipeline.md` §1.1).
pub const CELL_M: i64 = 2048;

/// Cells a side of the atlas's continent: 1,024 cells of 2 km, a square
/// of 2,097 km with the land inside and the sea around it.
pub const CONTINENT_SIDE: usize = 1024;

/// A grid of square cells, `side` a side, row by row from the north-west.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    /// Cells a side.
    pub side: usize,
}

/// The eight neighbours, as `(dx, dy, diagonal)`, in a fixed order (ties
/// are broken by it).
pub(crate) const NEIGHBOURS: [(i32, i32, bool); 8] = [
    (0, -1, false),
    (-1, 0, false),
    (1, 0, false),
    (0, 1, false),
    (-1, -1, true),
    (1, -1, true),
    (-1, 1, true),
    (1, 1, true),
];

/// A diagonal step's length over a straight one, per mille (√2).
pub(crate) const DIAGONAL_PER_MILLE: i64 = 1414;

impl Grid {
    /// Number of cells.
    pub fn len(self) -> usize {
        self.side * self.side
    }

    /// Whether the grid has no cell.
    pub fn is_empty(self) -> bool {
        self.side == 0
    }

    /// The index of cell `(x, y)`, which must be inside.
    #[allow(clippy::cast_sign_loss)]
    pub fn index(self, x: i32, y: i32) -> usize {
        debug_assert!(self.inside(x, y));
        y as usize * self.side + x as usize
    }

    /// The coordinates of cell `i`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub fn xy(self, i: usize) -> (i32, i32) {
        ((i % self.side) as i32, (i / self.side) as i32)
    }

    /// Whether `(x, y)` is a cell of the grid.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub fn inside(self, x: i32, y: i32) -> bool {
        let s = self.side as i32;
        x >= 0 && y >= 0 && x < s && y < s
    }

    /// The neighbours of cell `i` inside the grid, with whether each is a
    /// diagonal step, in [`NEIGHBOURS`] order.
    pub(crate) fn around(self, i: usize) -> impl Iterator<Item = (usize, bool)> {
        let (x, y) = self.xy(i);
        NEIGHBOURS.iter().filter_map(move |&(dx, dy, d)| {
            let (nx, ny) = (x + dx, y + dy);
            self.inside(nx, ny).then(|| (self.index(nx, ny), d))
        })
    }
}

/// A continent at its birth: every layer, cell by cell.
///
/// Heights are centimetres above the sea; the sea floor is below 0. The
/// water surface is the ground, or a lake's level where the last flood
/// filled a hollow, or 0 at sea: water only ever runs down it.
#[derive(Debug, Clone)]
pub struct Continent {
    /// The seed it was born from.
    pub seed: u64,
    /// Its grid of [`CELL_M`] cells.
    pub grid: Grid,
    /// Latitude of the northern edge, in hundredths of a degree.
    pub north_latitude: i32,
    /// Whether each cell is sea.
    pub sea: Vec<bool>,
    /// The ground, in centimetres (lake beds and sea floor included).
    pub height: Vec<i32>,
    /// The water's surface where water lies, else the ground (cm).
    pub surface: Vec<i32>,
    /// Whether each cell is a lake: a hollow filled at least
    /// [`relief::LAKE_MIN_DEPTH_CM`] deep.
    pub lake: Vec<bool>,
    /// Where each cell's water goes: a neighbour, or itself at sea.
    pub receiver: Vec<u32>,
    /// The cells from the sea upward, each after its receiver.
    pub order: Vec<u32>,
    /// The uplift a pass of the erosion, in centimetres.
    pub uplift: Vec<i32>,
    /// How much rock the erosion took off, in centimetres (negative where
    /// it laid sediment down).
    pub exhumed: Vec<i32>,
    /// The bedrock at the surface.
    pub rock: Vec<Rock>,
    /// Whether river sediment covers the bedrock.
    pub alluvium: Vec<bool>,
    /// Mean yearly temperature, in tenths of a degree Celsius.
    pub temperature: Vec<i16>,
    /// Yearly rain, in millimetres.
    pub rain: Vec<u16>,
    /// What leaves each cell by its river, in millilitres a second.
    pub discharge: Vec<u64>,
    /// The biome.
    pub biome: Vec<Biome>,
    /// The strokes the uplift was made of.
    pub strokes: Vec<Stroke>,
}

impl Continent {
    /// The atlas's continent for `seed`: [`CONTINENT_SIDE`] cells a side,
    /// its strokes drawn from the seed.
    pub fn new(seed: u64) -> Self {
        Self::with(seed, CONTINENT_SIDE, None)
    }

    /// A continent `side` cells a side; its strokes are `strokes` when
    /// given (the lead's own lines), else drawn from the seed. The
    /// outline and the strokes scale with `side`; the physics (erosion,
    /// climate, rivers) does not.
    pub fn with(seed: u64, side: usize, strokes: Option<Vec<Stroke>>) -> Self {
        assert!(
            (64..=8192).contains(&side),
            "a continent of 64 to 8192 cells a side"
        );
        let grid = Grid { side };
        let relief = relief::Relief::birth(seed, grid, strokes);
        climate::settle(seed, grid, relief)
    }

    /// Discharge of cell `i` in cubic metres a second, rounded down.
    pub fn cubic_metres(&self, i: usize) -> u64 {
        self.discharge[i] / 1_000_000
    }

    /// A fingerprint of everything the continent holds: two births that
    /// give the same fingerprint gave the same continent.
    pub fn fingerprint(&self) -> u64 {
        let mut h = hash(&[self.seed, self.grid.side as u64]);
        for i in 0..self.grid.len() {
            #[allow(clippy::cast_sign_loss)]
            let v = [
                h,
                u64::from(self.height[i] as u32),
                u64::from(self.surface[i] as u32),
                u64::from(self.receiver[i]),
                self.discharge[i],
                u64::from(self.rain[i]) << 16 | u64::from(self.temperature[i] as u16),
                (self.biome[i] as u64) << 8
                    | (self.rock[i] as u64) << 4
                    | u64::from(self.alluvium[i]) << 1
                    | u64::from(self.lake[i]),
            ];
            h = hash(&v);
        }
        h
    }
}

/// The square root of `n`, rounded down.
pub(crate) fn isqrt(n: u64) -> u64 {
    n.isqrt()
}

/// The `k`-th root of `n`, rounded down (a binary search).
pub(crate) fn iroot(n: u128, k: u32) -> u64 {
    let (mut lo, mut hi) = (0_u64, 1_u64 << (128 / k).min(63));
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        match u128::from(mid).checked_pow(k) {
            Some(p) if p <= n => lo = mid,
            _ => hi = mid - 1,
        }
    }
    lo
}

/// A piecewise-linear table of `(x, y)` points, sorted by `x`, read at
/// `x` (clamped at both ends).
pub(crate) fn table(points: &[(i64, i64)], x: i64) -> i64 {
    let first = points[0];
    if x <= first.0 {
        return first.1;
    }
    for pair in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        if x <= x1 {
            return y0 + (y1 - y0) * (x - x0) / (x1 - x0);
        }
    }
    points[points.len() - 1].1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_and_tables() {
        assert_eq!(iroot(32, 5), 2);
        assert_eq!(iroot(31, 5), 1);
        assert_eq!(iroot(10_u128.pow(26), 5), 158_489);
        assert_eq!(table(&[(0, 0), (10, 100)], 5), 50);
        assert_eq!(table(&[(0, 0), (10, 100)], 50), 100);
        assert_eq!(table(&[(0, 7), (10, 100)], -5), 7);
    }
}
