//! A window of the atlas at one metre: the relief look test (generator
//! version 3, the project lead's decision of 13 November 2026). Never the
//! default: a server reaches it only by `--world plan:<seed>`.
//!
//! A square of a [`Region`] is cut out and refined tile by tile:
//!
//! - **the height** (centimetres here, decimetres on the wire): the
//!   region's heights interpolated (Catmull-Rom, so the slope has no
//!   crease at a cell's border), plus small hills of a few metres
//!   (cubic lattice noise, three octaves), flattened on alluvium;
//! - **the water**: the continent's rivers along the region's own
//!   meandering courses, at their width; the region's brooks from cell
//!   to receiver; its lakes, their shore drawn where the interpolated
//!   share of lake cells passes a half. Each water lies flat at its
//!   level, and the ground around it is drawn down to that level over a
//!   bank, so water always sits in the low ground;
//! - **forest or open ground**, from the region's biome, its border
//!   ragged by noise; the woods themselves are the valley's
//!   ([`crate::woods::Woods`]), allowed only inside the forest.
//!
//! Everything is worked out at the tile from the seed and a few small
//! grids of the window's cells (its own cells and a margin of two): any
//! chunk, in any order, gives the same tiles. Only the water is kept, as
//! the valley keeps it.
//!
//! Rough on purpose: it is made to judge how slopes read, not to last.

use std::collections::BTreeMap;

use super::region::{self, REGION_CELL_M, RIVER_DISCHARGE, Water};
use super::{Biome, Continent, Region, isqrt, measure};
use crate::r#gen::scatter::{key, purpose, roll};
use crate::r#gen::{noise, unit};
use crate::map::{CHUNK_SIZE, Terrain, current_code};

/// The window's side in tiles: 6 km, 192 chunks.
pub const WINDOW_SIDE: u32 = 6144;

/// The seed whose window the look test opens on: an atlas seed with a
/// river, lakes, rolling hills of some 80 m and woods on about a third of
/// the ground (scouted on 13 November 2026 over seeds 1 to 10).
pub const LOOK_SEED: u64 = 7;

/// Cells of margin kept around the window, each way: Catmull-Rom reads
/// one cell past the cell a tile is in, and the brooks' points another.
const MARGIN: i64 = 2;

/// The region's cell side, in tiles (metres).
const CELL: i64 = REGION_CELL_M;

/// The small hills, `(lattice in metres, amplitude in cm either way)`:
/// low swells and knolls, so the ground between the region's 64 m cells
/// is never a smooth sheet. A metre or two at most: the region holds the
/// real relief, and stronger hills drew the same hummocks everywhere
/// over it (tried at 2.5 m, 80 cm and 25 cm on 13 November 2026).
const HILLS: [(i64, i64); 2] = [(128, 120), (40, 40)];

/// The hills' strength on alluvium, per mille: a flood plain is flat.
const HILLS_ON_ALLUVIUM: i64 = 300;

/// How far a brook's point strays from its cell's centre, in metres
/// either way: off the 64 m grid, so a brook does not run in straight
/// 45° steps.
const BROOK_JITTER_M: i64 = 16;

/// A brook's half-width, in millimetres: one or two tiles wide.
const BROOK_HALF_MM: i64 = 1_100;

/// The bank: over how many metres the ground is drawn down to a water's
/// level beside it. A brook's is short; a river's grows with its depth,
/// so a deep river's valley floor meets the region's slopes.
const BROOK_BANK_M: i64 = 20;
const RIVER_BANK_M: i64 = 24;
const RIVER_BANK_PER_DEPTH_M: i64 = 32;

/// A river this shallow or less is a ford, walked through (cm).
const FORD_DEPTH_CM: u64 = 50;

/// Currents, in cm a second: a river, a brook.
const RIVER_SPEED: u16 = 45;
const BROOK_SPEED: u16 = 25;

/// How far the border between forest and open ground wanders, in metres
/// either way, so it does not draw the 64 m cells; a lake's shore, the
/// same.
const EDGE_SHIFT_M: i64 = 28;
const SHORE_SHIFT_M: i64 = 20;

/// The camp: on dry, gentle ground this many metres from a river (a
/// brook's head may be a few tiles long).
const CAMP_WATER_M: (i64, i64) = (8, 40);
/// The camp's ground rises at most this much (dm) to a neighbour.
const CAMP_FLAT_DM: i64 = 1;

/// The window's choice in its region: tried every this many cells.
const CHOICE_STEP: usize = 16;
/// A window wanted: woods over this share of it, in per cent, so the
/// ground shows between them.
const CHOICE_FOREST: (i64, i64) = (25, 65);
/// A cell is steep when it rises more than this to a neighbour (cm over
/// 64 m: 10 %).
const CHOICE_STEEP_CM: i32 = 640;

/// A smoothstep over 0..1000, per mille.
fn smooth(t: i64) -> i64 {
    let t = t.clamp(0, 1000);
    t * t * (3000 - 2 * t) / 1_000_000
}

/// Catmull-Rom weights at `t` in `0..f`, over `2 f³`.
fn weights(t: i64, f: i64) -> [i64; 4] {
    [
        -t * t * t + 2 * t * t * f - t * f * f,
        3 * t * t * t - 5 * t * t * f + 2 * f * f * f,
        -3 * t * t * t + 4 * t * t * f + t * f * f,
        t * t * t - t * t * f,
    ]
}

/// A lattice of `f` units a side read at `(u, v)` by Catmull-Rom: `at`
/// gives a node's value. Smooth, its slope continuous across the nodes.
fn cubic(at: impl Fn(i64, i64) -> i64, f: i64, u: i64, v: i64) -> i64 {
    let (i, j) = (u.div_euclid(f), v.div_euclid(f));
    let (wx, wy) = (weights(u.rem_euclid(f), f), weights(v.rem_euclid(f), f));
    let whole = 2 * f * f * f;
    let mut sum = 0;
    for (b, wyb) in wy.iter().enumerate() {
        let mut row = 0;
        for (a, wxa) in wx.iter().enumerate() {
            row += wxa * at(i - 1 + a as i64, j - 1 + b as i64);
        }
        sum += wyb * (row / whole);
    }
    sum / whole
}

/// What runs on a stretch of water.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A river from the continent, this deep (cm).
    River(u64),
    /// A brook of the region's own drainage.
    Brook,
}

/// A straight stretch of running water, in millimetres from the
/// window's north-west corner, upstream end first.
#[derive(Debug, Clone, Copy)]
struct Stretch {
    a: (i64, i64),
    b: (i64, i64),
    /// The water's level at each end (cm).
    level: (i64, i64),
    /// Half its width (mm).
    half: i64,
    /// Its bank (mm).
    bank: i64,
    kind: Kind,
    /// Its heading, as a current's (`map::current_code`).
    heading: u8,
}

impl Stretch {
    /// The distance from `p` to the stretch (mm) and the water's level at
    /// the nearest point (cm).
    fn near(&self, p: (i64, i64)) -> (i64, i64) {
        let (dx, dy) = (self.b.0 - self.a.0, self.b.1 - self.a.1);
        let len2 = dx * dx + dy * dy;
        let dot = (p.0 - self.a.0) * dx + (p.1 - self.a.1) * dy;
        let (num, den) = if len2 == 0 {
            (0, 1)
        } else {
            (dot.clamp(0, len2), len2)
        };
        let (cx, cy) = (self.a.0 + dx * num / den, self.a.1 + dy * num / den);
        let d2 = (p.0 - cx).pow(2) + (p.1 - cy).pow(2);
        let d = i64::try_from(isqrt(u64::try_from(d2).expect("a square"))).expect("small");
        (d, self.level.0 + (self.level.1 - self.level.0) * num / den)
    }
}

/// The heading of a current running along `(dx, dy)`: the nearest of the
/// sixteen, `h` pointing along (sin(h × 22.5°), cos(h × 22.5°)).
fn heading(dx: i64, dy: i64) -> u8 {
    // sin(h × 22.5°) × 1000, h from 0 to 15.
    const SIN: [i64; 16] = [
        0, 383, 707, 924, 1000, 924, 707, 383, 0, -383, -707, -924, -1000, -924, -707, -383,
    ];
    let mut best = (i64::MIN, 0);
    for h in 0..16 {
        let along = dx * SIN[h] + dy * SIN[(h + 4) % 16];
        if along > best.0 {
            best = (along, h);
        }
    }
    u8::try_from(best.1).expect("below 16")
}

/// What lies on a wet tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wet {
    /// A river (`Water` or `Ford`), a brook, or a lake (`Water`).
    pub terrain: Terrain,
    /// How deep, in half-metres.
    pub depth: u8,
    /// Where it runs (`map::current_code`), 0 for a lake.
    pub current: u16,
    /// Its level (cm).
    pub level: i64,
}

/// No lake on a cell.
const NO_LAKE: i32 = i32::MIN;

/// A window of the atlas at one metre (see the module).
#[derive(Debug)]
pub struct Window {
    seed: u64,
    side: u32,
    /// The window's north-west corner in absolute region cells.
    origin: (i64, i64),
    /// Cells a side of the grids below: the window's and the margins.
    cells: i64,
    /// Per cell: the ground (cm), the hills' strength (per mille), the
    /// forest, a lake's level ([`NO_LAKE`] where there is none).
    height: Vec<i32>,
    hills: Vec<i32>,
    forest: Vec<bool>,
    lake: Vec<i32>,
    stretches: Vec<Stretch>,
    /// Per chunk of the window, row by row: the stretches whose bank
    /// reaches it.
    by_chunk: Vec<Vec<u32>>,
    camp: (u32, u32),
}

impl Window {
    /// The look test's window for `seed`: the atlas's continent and
    /// region ([`region::site`]), and the window of `side` tiles chosen
    /// in it ([`Window::choose`]).
    pub fn atlas(seed: u64, side: u32) -> Self {
        let continent = Continent::new(seed);
        let centre = region::site(&continent);
        let region = Region::around(&continent, centre, region::REGION_SIDE);
        let corner = Self::choose(&region, side);
        Self::cut(&continent, &region, corner, side)
    }

    /// Where to cut a window of `side` tiles in `region`, in its cells:
    /// no sea, a river, woods over [`CHOICE_FOREST`] of it if any window
    /// has them, and the most steep ground then the steepest on average.
    pub fn choose(region: &Region, side: u32) -> (usize, usize) {
        let n = side as usize / usize::try_from(CELL).expect("small");
        let rs = region.grid.side;
        let m = usize::try_from(MARGIN).expect("small");
        assert!(n + 2 * m <= rs, "a window inside its region");
        let mut best: Option<(bool, i64, (usize, usize))> = None;
        let mut wy = m;
        while wy + n + m <= rs {
            let mut wx = m;
            while wx + n + m <= rs {
                let (mut sea, mut river, mut forest, mut steep, mut rise) = (0, 0, 0, 0_i64, 0_i64);
                for y in wy..wy + n {
                    for x in wx..wx + n {
                        let i = y * rs + x;
                        match region.water[i] {
                            Water::Sea => sea += 1,
                            Water::River => river += 1,
                            _ => {}
                        }
                        if matches!(region.biome[i], Biome::Broadleaf | Biome::Conifer) {
                            forest += 1;
                        }
                        let up = (region.height[i + 1] - region.height[i])
                            .abs()
                            .max((region.height[i + rs] - region.height[i]).abs());
                        rise += i64::from(up);
                        if up > CHOICE_STEEP_CM {
                            steep += 1;
                        }
                    }
                }
                let cells = i64::try_from(n * n).expect("small");
                let share = forest * 100 / cells;
                if sea == 0 && river > 0 {
                    let wooded = (CHOICE_FOREST.0..=CHOICE_FOREST.1).contains(&share);
                    let score = steep + 2 * rise * 1000 / cells / (CELL * 100);
                    if best.is_none_or(|(w, s, _)| (wooded, score) > (w, s)) {
                        best = Some((wooded, score, (wx, wy)));
                    }
                }
                wx += CHOICE_STEP;
            }
            wy += CHOICE_STEP;
        }
        best.map_or(((rs - n) / 2, (rs - n) / 2), |(_, _, at)| at)
    }

    /// The window of `side` tiles (a multiple of 64) whose north-west
    /// corner is cell `corner` of `region`, cut from `continent`.
    #[allow(clippy::too_many_lines)]
    pub fn cut(continent: &Continent, region: &Region, corner: (usize, usize), side: u32) -> Self {
        assert!(
            side.is_multiple_of(CHUNK_SIZE) && i64::from(side) % CELL == 0,
            "a window of whole cells"
        );
        let seed = continent.seed;
        let n = i64::from(side) / CELL;
        let cells = n + 2 * MARGIN;
        let rs = i64::try_from(region.grid.side).expect("small");
        let (cx0, cy0) = (
            i64::try_from(corner.0).expect("small") - MARGIN,
            i64::try_from(corner.1).expect("small") - MARGIN,
        );
        assert!(
            cx0 >= 0 && cy0 >= 0 && cx0 + cells <= rs && cy0 + cells <= rs,
            "a window and its margin inside the region"
        );
        let per = i64::from(region::PER_CELL);
        let origin = (
            i64::from(region.origin.0) * per + cx0 + MARGIN,
            i64::from(region.origin.1) * per + cy0 + MARGIN,
        );
        let index = |x: i64, y: i64| usize::try_from((cy0 + y) * rs + cx0 + x).expect("inside");
        let total = usize::try_from(cells * cells).expect("small");
        let (mut height, mut hills, mut forest, mut lake) = (
            Vec::with_capacity(total),
            Vec::with_capacity(total),
            Vec::with_capacity(total),
            Vec::with_capacity(total),
        );
        for y in 0..cells {
            for x in 0..cells {
                let i = index(x, y);
                height.push(region.height[i]);
                hills.push(if region.alluvium[i] {
                    i32::try_from(HILLS_ON_ALLUVIUM).expect("small")
                } else {
                    1000
                });
                forest.push(matches!(region.biome[i], Biome::Broadleaf | Biome::Conifer));
                lake.push(if region.water[i] == Water::Lake {
                    region.surface[i]
                } else {
                    NO_LAKE
                });
            }
        }
        // Absolute region cell → millimetres from the window's corner, at
        // the cell's centre.
        let mm = |g: (i64, i64)| {
            (
                (g.0 - origin.0) * CELL * 1000 + CELL * 500,
                (g.1 - origin.1) * CELL * 1000 + CELL * 500,
            )
        };

        // The rivers, along the region's courses.
        let mut stretches = Vec::new();
        let span = i32::try_from(n / per + 3).expect("small");
        let (kx, ky) = (
            i32::try_from(origin.0.div_euclid(per)).expect("small"),
            i32::try_from(origin.1.div_euclid(per)).expect("small"),
        );
        for y in ky - 2..ky + span {
            for x in kx - 2..kx + span {
                if !continent.grid.inside(x, y) {
                    continue;
                }
                let i = continent.grid.index(x, y);
                let q = continent.discharge[i];
                if continent.sea[i] || q < RIVER_DISCHARGE {
                    continue;
                }
                let r = continent.receiver[i] as usize;
                let (rx, ry) = continent.grid.xy(r);
                let path =
                    region::course(seed, region::node(seed, x, y), region::node(seed, rx, ry));
                let (top, bottom) = (
                    i64::from(continent.surface[i]),
                    i64::from(continent.surface[r]),
                );
                let depth = measure::depth_cm(q);
                let half = i64::try_from(measure::width_cm(q)).expect("small") * 5;
                let bank = (RIVER_BANK_M
                    + RIVER_BANK_PER_DEPTH_M * i64::try_from(depth).expect("small") / 100)
                    * 1000;
                let steps = i64::try_from(path.len() - 1).expect("small").max(1);
                for (k, pair) in path.windows(2).enumerate() {
                    let k = i64::try_from(k).expect("small");
                    let (a, b) = (mm(pair[0]), mm(pair[1]));
                    stretches.push(Stretch {
                        a,
                        b,
                        level: (
                            top - (top - bottom) * k / steps,
                            top - (top - bottom) * (k + 1) / steps,
                        ),
                        half: half.max(1000),
                        bank,
                        kind: Kind::River(depth),
                        heading: heading(b.0 - a.0, b.1 - a.1),
                    });
                }
            }
        }
        let rivers = stretches.len();

        // The brooks, from cell to receiver.
        let brook_point = |gx: i64, gy: i64| {
            let jitter = |k: u64| {
                i64::try_from(roll(seed, purpose::PLAN_BROOK, &[key(gx), key(gy), k]) % 2001)
                    .expect("small")
                    * BROOK_JITTER_M
                    - 1000 * BROOK_JITTER_M
            };
            let c = mm((gx, gy));
            (c.0 + jitter(0), c.1 + jitter(1))
        };
        for y in 0..cells {
            for x in 0..cells {
                let i = index(x, y);
                if region.water[i] != Water::Brook {
                    continue;
                }
                let r = region.receiver[i] as usize;
                if r == i {
                    continue;
                }
                let (gx, gy) = (origin.0 - MARGIN + x, origin.1 - MARGIN + y);
                let rs_u = region.grid.side;
                let (rx, ry) = (
                    i64::try_from(r % rs_u).expect("small") - cx0 + origin.0 - MARGIN,
                    i64::try_from(r / rs_u).expect("small") - cy0 + origin.1 - MARGIN,
                );
                let a = brook_point(gx, gy);
                let from = i64::from(region.surface[i]);
                let (b, to) = match region.water[r] {
                    Water::Brook => (brook_point(rx, ry), i64::from(region.surface[r])),
                    Water::River => {
                        // Onto the nearest river stretch, at its level.
                        let c = mm((rx, ry));
                        let mut best: Option<(i64, (i64, i64), i64)> = None;
                        for s in &stretches[..rivers] {
                            let (d, level) = s.near(c);
                            if best.is_none_or(|(bd, _, _)| d < bd) {
                                // The nearest point itself.
                                let (dx, dy) = (s.b.0 - s.a.0, s.b.1 - s.a.1);
                                let len2 = (dx * dx + dy * dy).max(1);
                                let t = ((c.0 - s.a.0) * dx + (c.1 - s.a.1) * dy).clamp(0, len2);
                                best = Some((
                                    d,
                                    (s.a.0 + dx * t / len2, s.a.1 + dy * t / len2),
                                    level,
                                ));
                            }
                        }
                        match best {
                            Some((_, p, level)) => (p, level.min(from)),
                            None => (c, i64::from(region.surface[r])),
                        }
                    }
                    Water::Lake => (mm((rx, ry)), i64::from(region.surface[r])),
                    Water::Land | Water::Sea => continue,
                };
                stretches.push(Stretch {
                    a,
                    b,
                    level: (from, to.min(from)),
                    half: BROOK_HALF_MM,
                    bank: BROOK_BANK_M * 1000,
                    kind: Kind::Brook,
                    heading: heading(b.0 - a.0, b.1 - a.1),
                });
            }
        }

        // Each stretch, filed under the chunks its bank reaches.
        let chunks = i64::from(side / CHUNK_SIZE);
        let chunk_mm = i64::from(CHUNK_SIZE) * 1000;
        let mut by_chunk = vec![Vec::new(); usize::try_from(chunks * chunks).expect("small")];
        for (k, s) in stretches.iter().enumerate() {
            let reach = s.half + s.bank;
            let lo = |a: i64, b: i64| ((a.min(b) - reach).div_euclid(chunk_mm)).max(0);
            let hi = |a: i64, b: i64| ((a.max(b) + reach).div_euclid(chunk_mm)).min(chunks - 1);
            for cy in lo(s.a.1, s.b.1)..=hi(s.a.1, s.b.1) {
                for cx in lo(s.a.0, s.b.0)..=hi(s.a.0, s.b.0) {
                    by_chunk[usize::try_from(cy * chunks + cx).expect("inside")]
                        .push(u32::try_from(k).expect("few stretches"));
                }
            }
        }
        let mut window = Self {
            seed,
            side,
            origin,
            cells,
            height,
            hills,
            forest,
            lake,
            stretches,
            by_chunk,
            camp: (side / 2, side / 2),
        };
        window.camp = window.find_camp();
        window
    }

    /// Tiles a side.
    pub fn side(&self) -> u32 {
        self.side
    }

    /// The window's north-west corner, in absolute region cells.
    pub fn origin(&self) -> (i64, i64) {
        self.origin
    }

    /// The camp: dry, gentle ground near a river, the nearest such to the
    /// window's centre.
    pub fn camp(&self) -> (u32, u32) {
        self.camp
    }

    /// The value of a cell grid at cell `(i, j)` of the window (0 is the
    /// window's first cell), clamped to the margins.
    fn cell<T: Copy>(&self, grid: &[T], i: i64, j: i64) -> T {
        let c = |v: i64| (v + MARGIN).clamp(0, self.cells - 1);
        grid[usize::try_from(c(j) * self.cells + c(i)).expect("inside")]
    }

    /// A cell grid read at tile `(x, y)` by Catmull-Rom between the cells'
    /// centres.
    fn smooth_at(value: impl Fn(i64, i64) -> i64, x: i64, y: i64) -> i64 {
        cubic(value, CELL, x - CELL / 2, y - CELL / 2)
    }

    /// The ground at tile `(x, y)` before the water: the region's and the
    /// small hills (cm).
    fn ground(&self, x: i64, y: i64) -> i64 {
        let base = Self::smooth_at(|i, j| i64::from(self.cell(&self.height, i, j)), x, y);
        let strength = Self::smooth_at(|i, j| i64::from(self.cell(&self.hills, i, j)), x, y);
        let mut hills = 0;
        for (k, &(cell, amplitude)) in HILLS.iter().enumerate() {
            let s = roll(self.seed, purpose::PLAN_HILLS, &[k as u64]);
            let node = |i: i64, j: i64| i64::from(unit(&[s, key(i), key(j)])) - 500;
            hills += cubic(node, cell, x, y) * amplitude / 500;
        }
        base + hills * strength.clamp(0, 1000) / 1000
    }

    /// The share of lake around tile `(x, y)`, per mille (above 500 is a
    /// lake), and the lowest level of the lakes it reads.
    fn lake_at(&self, x: i64, y: i64) -> (i64, i64) {
        let (x, y) = self.ragged(x, y, 2, SHORE_SHIFT_M);
        let share = Self::smooth_at(
            |i, j| {
                if self.cell(&self.lake, i, j) == NO_LAKE {
                    0
                } else {
                    1000
                }
            },
            x,
            y,
        );
        if share <= 0 {
            return (0, 0);
        }
        let (i, j) = (
            (x - CELL / 2).div_euclid(CELL),
            (y - CELL / 2).div_euclid(CELL),
        );
        let mut level = i64::MAX;
        for b in -1..=2 {
            for a in -1..=2 {
                let l = self.cell(&self.lake, i + a, j + b);
                if l != NO_LAKE {
                    level = level.min(i64::from(l));
                }
            }
        }
        (share, level)
    }

    /// The stretches whose bank reaches tile `(x, y)`.
    fn stretches_near(&self, x: i64, y: i64) -> impl Iterator<Item = &Stretch> {
        let side = i64::from(CHUNK_SIZE);
        let chunks = i64::from(self.side / CHUNK_SIZE);
        let (cx, cy) = (x.div_euclid(side), y.div_euclid(side));
        let list: &[u32] = if (0..chunks).contains(&cx) && (0..chunks).contains(&cy) {
            &self.by_chunk[usize::try_from(cy * chunks + cx).expect("inside")]
        } else {
            &[]
        };
        list.iter().map(|&k| &self.stretches[k as usize])
    }

    /// The water on tile `(x, y)`, if any: a lake first, else the lowest
    /// of the stretches covering it.
    pub fn wet(&self, x: i64, y: i64) -> Option<Wet> {
        let (share, level) = self.lake_at(x, y);
        if share > 500 {
            return Some(Wet {
                terrain: Terrain::Water,
                depth: 2,
                current: 0,
                level,
            });
        }
        let p = (x * 1000 + 500, y * 1000 + 500);
        let mut best: Option<Wet> = None;
        for s in self.stretches_near(x, y) {
            let (d, level) = s.near(p);
            if d > s.half || best.is_some_and(|b| b.level <= level) {
                continue;
            }
            let (terrain, depth, speed) = match s.kind {
                Kind::River(depth) if depth <= FORD_DEPTH_CM => (Terrain::Ford, 1, RIVER_SPEED),
                Kind::River(depth) => (
                    Terrain::Water,
                    u8::try_from((depth / 50).clamp(2, 12)).expect("small"),
                    RIVER_SPEED,
                ),
                Kind::Brook => (Terrain::Brook, 1, BROOK_SPEED),
            };
            best = Some(Wet {
                terrain,
                depth,
                current: current_code(s.heading, speed),
                level,
            });
        }
        best
    }

    /// The height of tile `(x, y)`, in centimetres: a water's level on
    /// water; elsewhere the ground, drawn down to the waters beside it
    /// over their banks.
    pub fn height_cm(&self, x: i64, y: i64) -> i64 {
        if let Some(wet) = self.wet(x, y) {
            return wet.level;
        }
        let mut h = self.ground(x, y);
        let (share, level) = self.lake_at(x, y);
        if share > 0 {
            let w = smooth((500 - share) * 2);
            h = level + (h - level) * w / 1000;
        }
        let p = (x * 1000 + 500, y * 1000 + 500);
        for s in self.stretches_near(x, y) {
            let (d, level) = s.near(p);
            if d < s.half + s.bank {
                let w = smooth((d - s.half) * 1000 / s.bank);
                h = level + (h - level) * w / 1000;
            }
        }
        h
    }

    /// The height of tile `(x, y)` in decimetres, as the wire carries it.
    pub fn height_dm(&self, x: i64, y: i64) -> i16 {
        let dm = (self.height_cm(x, y) + 5).div_euclid(10);
        i16::try_from(dm.clamp(i64::from(i16::MIN), i64::from(i16::MAX))).expect("clamped")
    }

    /// Is tile `(x, y)` in the forest's biome? Its border with the open
    /// ground wanders [`EDGE_SHIFT_M`] either way.
    pub fn wooded(&self, x: i64, y: i64) -> bool {
        let (sx, sy) = self.ragged(x, y, 0, EDGE_SHIFT_M);
        self.cell(&self.forest, sx.div_euclid(CELL), sy.div_euclid(CELL))
    }

    /// Tile `(x, y)` shifted up to `reach` metres either way by a noise
    /// of its own (`k`), so a border read there wanders.
    fn ragged(&self, x: i64, y: i64, k: u64, reach: i64) -> (i64, i64) {
        let shift = |k: u64| {
            let s = roll(self.seed, purpose::PLAN_EDGE, &[k]);
            let (xi, yi) = (
                i32::try_from(x).expect("small"),
                i32::try_from(y).expect("small"),
            );
            let n = (i64::from(noise(s, 40, xi, yi)) - 500) * 2
                + i64::from(noise(s ^ 1, 11, xi, yi))
                - 500;
            n * reach / 1500
        };
        (x + shift(k), y + shift(k + 1))
    }

    /// Every wet tile, row by row: `(x, y, wet)`.
    pub fn waters(&self) -> Vec<(u32, u32, Wet)> {
        let side = i64::from(self.side);
        let mut found: BTreeMap<(i64, i64), Wet> = BTreeMap::new();
        let mut look = |x: i64, y: i64| {
            if (0..side).contains(&x)
                && (0..side).contains(&y)
                && !found.contains_key(&(y, x))
                && let Some(wet) = self.wet(x, y)
            {
                found.insert((y, x), wet);
            }
        };
        for s in &self.stretches {
            let lo = |a: i64, b: i64| (a.min(b) - s.half).div_euclid(1000);
            let hi = |a: i64, b: i64| (a.max(b) + s.half).div_euclid(1000);
            for y in lo(s.a.1, s.b.1)..=hi(s.a.1, s.b.1) {
                for x in lo(s.a.0, s.b.0)..=hi(s.a.0, s.b.0) {
                    if s.near((x * 1000 + 500, y * 1000 + 500)).0 <= s.half {
                        look(x, y);
                    }
                }
            }
        }
        // The lakes: the tiles of their cells and the cells around.
        let n = self.cells - 2 * MARGIN;
        for j in -1..=n {
            for i in -1..=n {
                let near_lake = (-1..=1)
                    .any(|b| (-1..=1).any(|a| self.cell(&self.lake, i + a, j + b) != NO_LAKE));
                if !near_lake {
                    continue;
                }
                for y in j * CELL..(j + 1) * CELL {
                    for x in i * CELL..(i + 1) * CELL {
                        look(x, y);
                    }
                }
            }
        }
        found
            .into_iter()
            .map(|((y, x), wet)| {
                (
                    u32::try_from(x).expect("inside"),
                    u32::try_from(y).expect("inside"),
                    wet,
                )
            })
            .collect()
    }

    /// The camp: searched in growing squares from the window's centre,
    /// every 4 m, the first dry tile [`CAMP_WATER_M`] from a river whose
    /// ground is gentle around.
    fn find_camp(&self) -> (u32, u32) {
        let side = i64::from(self.side);
        let centre = side / 2;
        let fits = |x: i64, y: i64| {
            if x < 1 || y < 1 || x >= side - 1 || y >= side - 1 || self.wet(x, y).is_some() {
                return false;
            }
            if self.lake_at(x, y).0 > 0 {
                return false;
            }
            let p = (x * 1000 + 500, y * 1000 + 500);
            let near = self
                .stretches_near(x, y)
                .filter(|s| matches!(s.kind, Kind::River(_)))
                .map(|s| s.near(p).0 - s.half)
                .min()
                .unwrap_or(i64::MAX);
            if !(CAMP_WATER_M.0 * 1000..=CAMP_WATER_M.1 * 1000).contains(&near) {
                return false;
            }
            let h = i64::from(self.height_dm(x, y));
            [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().all(|&(dx, dy)| {
                (i64::from(self.height_dm(x + dx, y + dy)) - h).abs() <= CAMP_FLAT_DM
            })
        };
        let mut r = 0;
        while r < centre {
            for dy in (-r..=r).step_by(4) {
                for dx in (-r..=r).step_by(4) {
                    if (dx.abs() == r || dy.abs() == r) && fits(centre + dx, centre + dy) {
                        return (
                            u32::try_from(centre + dx).expect("inside"),
                            u32::try_from(centre + dy).expect("inside"),
                        );
                    }
                }
            }
            r += 4;
        }
        (self.side / 2, self.side / 2)
    }
}
