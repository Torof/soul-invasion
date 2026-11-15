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
//! - **the water**: the continent's rivers along their natural courses
//!   (`course`, the region's own stems, whose levels settled on its
//!   ground and whose valleys it cut: the meanders carried down to the tile, their
//!   corners rounded, each bank walking on its own as the valley's do,
//!   tributaries joining at first contact, a ford across the river at
//!   every crossing the atlas finds dry-shod); the region's
//!   brooks from cell to receiver; its lakes, their shore drawn where
//!   the interpolated share of lake cells passes a half. Each water lies
//!   flat at its level, and the ground around it is drawn down to that
//!   level over a bank, so water always sits in the low ground;
//! - **forest or open ground**, from the region's biome, its border
//!   ragged by noise; the woods themselves are the valley's
//!   ([`crate::woods::Woods`]), allowed only inside the forest. The open
//!   ground is a meadow, its biome read tile by tile ([`Window::biome`])
//!   by the trees that stand alone and in groves on it
//!   ([`crate::open_ground`], 15 November 2026).
//!
//! Everything is worked out at the tile from the seed and a few small
//! grids of the window's cells (its own cells and a margin of two): any
//! chunk, in any order, gives the same tiles. Only the water is kept, as
//! the valley keeps it.
//!
//! Rough on purpose: it is made to judge how slopes read, not to last.

use std::collections::BTreeMap;

use super::region::{self, REGION_CELL_M, Water};
use super::vivarium::REGION_CELLS;
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

/// A river whose mean depth is this or less is a ford all along, walked
/// through (cm): the atlas's dry-shod depth, the one its crossings are
/// found at ([`measure::passages`]).
const FORD_DEPTH_CM: i64 = measure::DRY_CM as i64;

/// Currents, in cm a second: a river where its section is the one its
/// flow gives (slower where its banks stand out, quicker where they stand
/// in), a brook; a ford,
/// spread thin over its stones, runs half as fast again (D36).
const RIVER_SPEED: i64 = 45;
const BROOK_SPEED: u16 = 25;
const FORD_SPEED_PER_MILLE: i64 = 1500;

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

/// A cell grid of `cells` a side read at cell `(i, j)` of the window (0
/// is the window's first cell), clamped to the margins.
fn cell_of<T: Copy>(grid: &[T], cells: i64, i: i64, j: i64) -> T {
    let c = |v: i64| (v + MARGIN).clamp(0, cells - 1);
    grid[usize::try_from(c(j) * cells + c(i)).expect("inside")]
}

/// The ground at tile `(x, y)` before the water, from the window's cell
/// grids (`cells` a side): the region's heights and the small hills (cm),
/// the hills keyed on the continent's tile (`origin`, the window's corner
/// in region cells) so two windows draw the same hills on the same ground.
fn ground_of(
    seed: u64,
    origin: (i64, i64),
    cells: i64,
    height: &[i32],
    hills: &[i32],
    x: i64,
    y: i64,
) -> i64 {
    let smooth_at = |grid: &[i32]| {
        cubic(
            |i, j| i64::from(cell_of(grid, cells, i, j)),
            CELL,
            x - CELL / 2,
            y - CELL / 2,
        )
    };
    let base = smooth_at(height);
    let strength = smooth_at(hills);
    let mut sum = 0;
    let (wx, wy) = (x + origin.0 * CELL, y + origin.1 * CELL);
    for (k, &(cell, amplitude)) in HILLS.iter().enumerate() {
        let s = roll(seed, purpose::PLAN_HILLS, &[k as u64]);
        let node = |i: i64, j: i64| i64::from(unit(&[s, key(i), key(j)])) - 500;
        sum += cubic(node, cell, wx, wy) * amplitude / 500;
    }
    base + sum * strength.clamp(0, 1000) / 1000
}

/// What runs on a stretch of water.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A river from the continent.
    River,
    /// A brook of the region's own drainage.
    Brook,
}

/// A straight stretch of running water, in millimetres from the
/// window's north-west corner, upstream end first. A river's is a few
/// metres long, one segment of its natural course ([`course`]); a
/// brook's runs from its cell to its receiver's.
#[derive(Debug, Clone, Copy)]
struct Stretch {
    a: (i64, i64),
    b: (i64, i64),
    /// The water's level at each end (cm).
    level: (i64, i64),
    /// The half-width at `a` then at `b`, each on the left bank then the
    /// right bank of the current (mm): a river's banks walk each on its
    /// own, a brook's hold their lines.
    half: ((i64, i64), (i64, i64)),
    /// The width the flow gives the water at each end, before its banks
    /// walk (mm).
    width: (i64, i64),
    /// The mean depth the flow gives it at each end (cm; 0 on a brook).
    depth: (i64, i64),
    /// Its bank (mm).
    bank: i64,
    kind: Kind,
    /// On a ford's bar: half a metre deep right across.
    ford: bool,
    /// Its heading, as a current's (`map::current_code`).
    heading: u8,
}

/// What a stretch is at the point of it nearest to a tile.
#[derive(Debug, Clone, Copy)]
struct Probe {
    /// How far the tile is from the stretch's line (mm).
    d: i64,
    /// The half-width on the tile's side of the line (mm).
    half: i64,
    /// The water's level (cm).
    level: i64,
    /// Its depth, what the width leaves of the flow (cm): deeper where
    /// the banks stand in, shallower where they stand out, never under a
    /// metre on a river.
    depth: i64,
    /// Its current's speed (cm/s).
    speed: i64,
}

impl Stretch {
    /// The stretch probed from `p`: its distance, and what the water is
    /// at the nearest point.
    fn probe(&self, p: (i64, i64)) -> Probe {
        let (dx, dy) = (self.b.0 - self.a.0, self.b.1 - self.a.1);
        let len2 = dx * dx + dy * dy;
        let dot = (p.0 - self.a.0) * dx + (p.1 - self.a.1) * dy;
        let (num, den) = if len2 == 0 {
            (0, 1)
        } else {
            (dot.clamp(0, len2), len2)
        };
        let mix = |x: i64, y: i64| x + (y - x) * num / den;
        let (cx, cy) = (mix(self.a.0, self.b.0), mix(self.a.1, self.b.1));
        let d2 = (p.0 - cx).pow(2) + (p.1 - cy).pow(2);
        let d = i64::try_from(isqrt(u64::try_from(d2).expect("small"))).expect("small");
        // Which bank: the sign of the cross product of the stretch and
        // the way to the tile (seen from above with y downwards, the
        // current's left bank is where it is positive).
        let left = dx * (p.1 - self.a.1) - dy * (p.0 - self.a.0) > 0;
        let (ha, hb) = if left {
            (self.half.0.0, self.half.1.0)
        } else {
            (self.half.0.1, self.half.1.1)
        };
        let both = mix(self.half.0.0 + self.half.0.1, self.half.1.0 + self.half.1.1).max(1);
        let width = mix(self.width.0, self.width.1);
        let nominal = mix(self.depth.0, self.depth.1);
        let depth = (nominal * width / both).max(nominal.min(100));
        Probe {
            d,
            half: mix(ha, hb),
            level: mix(self.level.0, self.level.1),
            depth,
            speed: RIVER_SPEED * depth / nominal.max(1),
        }
    }

    /// The widest the stretch is, either side (mm).
    fn widest(&self) -> i64 {
        self.half
            .0
            .0
            .max(self.half.0.1)
            .max(self.half.1.0)
            .max(self.half.1.1)
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
#[derive(Debug, Clone)]
pub struct Window {
    seed: u64,
    side: u32,
    /// The window's north-west corner in absolute region cells.
    origin: (i64, i64),
    /// Cells a side of the grids below: the window's and the margins.
    cells: i64,
    /// Per cell: the ground (cm), the hills' strength (per mille), the
    /// biome, a lake's level ([`NO_LAKE`] where there is none).
    height: Vec<i32>,
    hills: Vec<i32>,
    biome: Vec<Biome>,
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

    /// The window of one continent cell of `continent`, 2 km a side
    /// ([`super::CELL_M`]): cut from the region of [`REGION_CELLS`]
    /// continent cells around `cell`, as the vivarium's zone is. The
    /// unit the continent is sampled by (`plan::net`, `windows_sheet`).
    ///
    /// # Panics
    ///
    /// When `cell` is not a cell of the continent.
    pub fn of_cell(continent: &Continent, cell: (i32, i32)) -> Self {
        assert!(
            continent.grid.inside(cell.0, cell.1),
            "a cell of the continent"
        );
        let per = usize::try_from(region::PER_CELL).expect("positive");
        let region = Region::around(continent, cell, REGION_CELLS * per);
        let corner = REGION_CELLS / 2 * per;
        let side = u32::try_from(super::CELL_M).expect("small");
        Self::cut(continent, &region, (corner, corner), side)
    }

    /// Where to cut a window of `side` tiles in `region`, in its cells:
    /// no sea, a river, woods over `CHOICE_FOREST` of it if any window
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
                        if region.biome[i].is_forest() {
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
        let (mut height, mut hills, mut biome, mut lake) = (
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
                biome.push(region.biome[i]);
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

        // The rivers, along their natural courses, segment by segment.
        let mut stretches = Vec::new();
        let shift = (origin.0 * CELL * 1000, origin.1 * CELL * 1000);
        for stem in &region.stems {
            for pair in stem.nodes.windows(2) {
                let (u, v) = (pair[0], pair[1]);
                let (a, b) = (
                    (u.p.0 - shift.0, u.p.1 - shift.1),
                    (v.p.0 - shift.0, v.p.1 - shift.1),
                );
                let bank = (RIVER_BANK_M + RIVER_BANK_PER_DEPTH_M * u.depth / 100) * 1000;
                stretches.push(Stretch {
                    a,
                    b,
                    level: (u.level, v.level),
                    half: (u.half, v.half),
                    width: (u.width, v.width),
                    depth: (u.depth, v.depth),
                    bank,
                    kind: Kind::River,
                    ford: u.ford,
                    heading: heading(b.0 - a.0, b.1 - a.1),
                });
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
        // A brook runs on the ground the window draws, read at its own
        // point, not at its cell's centre (on a flank the ground falls
        // metres within a cell), and never rises: a point's level is the
        // lowest of its ground and of every brook point upstream of it,
        // so two brooks meeting at a point meet at one level (a window of
        // the mountains stepped 3 m where they did not, 15 November 2026).
        let rs_u = region.grid.side;
        let region_cell =
            |x: i64, y: i64| usize::try_from((cy0 + y) * rs + cx0 + x).expect("inside");
        let local = |i: usize| {
            (
                i64::try_from(i % rs_u).expect("small") - cx0,
                i64::try_from(i / rs_u).expect("small") - cy0,
            )
        };
        let is_brook = |x: i64, y: i64| {
            (0..cells).contains(&x)
                && (0..cells).contains(&y)
                && region.water[region_cell(x, y)] == Water::Brook
        };
        let mut brook_level = vec![i64::MAX; total];
        for y in 0..cells {
            for x in 0..cells {
                if is_brook(x, y) {
                    let p = brook_point(origin.0 - MARGIN + x, origin.1 - MARGIN + y);
                    brook_level[usize::try_from(y * cells + x).expect("small")] = ground_of(
                        seed,
                        origin,
                        cells,
                        &height,
                        &hills,
                        p.0.div_euclid(1000),
                        p.1.div_euclid(1000),
                    );
                }
            }
        }
        loop {
            let mut changed = false;
            for y in 0..cells {
                for x in 0..cells {
                    if !is_brook(x, y) {
                        continue;
                    }
                    let (rx, ry) = local(region.receiver[region_cell(x, y)] as usize);
                    if (rx, ry) != (x, y) && is_brook(rx, ry) {
                        let (i, r) = (
                            usize::try_from(y * cells + x).expect("small"),
                            usize::try_from(ry * cells + rx).expect("small"),
                        );
                        if brook_level[i] < brook_level[r] {
                            brook_level[r] = brook_level[i];
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
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
                let (lx, ly) = local(r);
                let (rx, ry) = (lx + origin.0 - MARGIN, ly + origin.1 - MARGIN);
                let a = brook_point(gx, gy);
                let from = brook_level[usize::try_from(y * cells + x).expect("small")];
                let (b, to) = match region.water[r] {
                    Water::Brook => {
                        let b = brook_point(rx, ry);
                        // A receiver past the margin has no settled level:
                        // its ground, clamped to the margin, serves.
                        let level = if is_brook(lx, ly) {
                            brook_level[usize::try_from(ly * cells + lx).expect("small")]
                        } else {
                            ground_of(
                                seed,
                                origin,
                                cells,
                                &height,
                                &hills,
                                b.0.div_euclid(1000),
                                b.1.div_euclid(1000),
                            )
                        };
                        (b, level)
                    }
                    Water::River => {
                        // Onto the nearest river stretch, at its level.
                        let c = mm((rx, ry));
                        let mut best: Option<(i64, (i64, i64), i64)> = None;
                        for s in &stretches[..rivers] {
                            let Probe { d, level, .. } = s.probe(c);
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
                    half: (
                        (BROOK_HALF_MM, BROOK_HALF_MM),
                        (BROOK_HALF_MM, BROOK_HALF_MM),
                    ),
                    width: (2 * BROOK_HALF_MM, 2 * BROOK_HALF_MM),
                    depth: (0, 0),
                    bank: BROOK_BANK_M * 1000,
                    kind: Kind::Brook,
                    ford: false,
                    heading: heading(b.0 - a.0, b.1 - a.1),
                });
            }
        }

        // Each stretch, filed under the chunks its bank reaches.
        let chunks = i64::from(side / CHUNK_SIZE);
        let chunk_mm = i64::from(CHUNK_SIZE) * 1000;
        let mut by_chunk = vec![Vec::new(); usize::try_from(chunks * chunks).expect("small")];
        for (k, s) in stretches.iter().enumerate() {
            let reach = s.widest() + s.bank;
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
            biome,
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

    /// The window's north-west corner in the continent's tiles: where
    /// the map cut from it lies on the world ([`crate::Map::origin`]).
    #[must_use]
    pub fn tile_origin(&self) -> (i32, i32) {
        (
            i32::try_from(self.origin.0 * CELL).expect("the continent fits in i32 tiles"),
            i32::try_from(self.origin.1 * CELL).expect("the continent fits in i32 tiles"),
        )
    }

    /// The continent's tile under the window's tile `(x, y)`: what the
    /// noises are keyed on, so two windows draw the same hills and
    /// borders on the same ground and different ones elsewhere.
    fn world(&self, x: i64, y: i64) -> (i64, i64) {
        (x + self.origin.0 * CELL, y + self.origin.1 * CELL)
    }

    /// The camp: dry, gentle ground near a river, the nearest such to the
    /// window's centre, unless set ([`Window::with_camp`]).
    pub fn camp(&self) -> (u32, u32) {
        self.camp
    }

    /// The same window with its camp at `at`, or at the nearest dry tile
    /// (no water, no lake's shore) when `at` is wet: the vivarium's camps
    /// are chosen on the atlas, by the cell, not found from the centre.
    ///
    /// # Panics
    ///
    /// When `at` is off the window.
    #[must_use]
    pub fn with_camp(mut self, at: (u32, u32)) -> Self {
        assert!(at.0 < self.side && at.1 < self.side, "a camp on the window");
        let side = i64::from(self.side);
        let (cx, cy) = (i64::from(at.0), i64::from(at.1));
        let dry = |x: i64, y: i64| {
            (1..side - 1).contains(&x)
                && (1..side - 1).contains(&y)
                && self.wet(x, y).is_none()
                && self.lake_at(x, y).0 == 0
        };
        let mut r = 0;
        while r < side {
            for dy in -r..=r {
                for dx in -r..=r {
                    if (dx.abs() == r || dy.abs() == r) && dry(cx + dx, cy + dy) {
                        self.camp = (
                            u32::try_from(cx + dx).expect("inside"),
                            u32::try_from(cy + dy).expect("inside"),
                        );
                        return self;
                    }
                }
            }
            r += 1;
        }
        self.camp = at;
        self
    }

    /// The value of a cell grid at cell `(i, j)` of the window (0 is the
    /// window's first cell), clamped to the margins.
    fn cell<T: Copy>(&self, grid: &[T], i: i64, j: i64) -> T {
        cell_of(grid, self.cells, i, j)
    }

    /// A cell grid read at tile `(x, y)` by Catmull-Rom between the cells'
    /// centres.
    fn smooth_at(value: impl Fn(i64, i64) -> i64, x: i64, y: i64) -> i64 {
        cubic(value, CELL, x - CELL / 2, y - CELL / 2)
    }

    /// The ground at tile `(x, y)` before the water: the region's and the
    /// small hills (cm).
    fn ground(&self, x: i64, y: i64) -> i64 {
        ground_of(
            self.seed,
            self.origin,
            self.cells,
            &self.height,
            &self.hills,
            x,
            y,
        )
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
            let probe = s.probe(p);
            if probe.d > probe.half || best.is_some_and(|b| b.level <= probe.level) {
                continue;
            }
            let (terrain, depth, speed) = match s.kind {
                Kind::River if s.ford || probe.depth <= FORD_DEPTH_CM => {
                    (Terrain::Ford, 1, probe.speed * FORD_SPEED_PER_MILLE / 1000)
                }
                Kind::River => (
                    Terrain::Water,
                    u8::try_from((probe.depth / 50).clamp(2, 12)).expect("small"),
                    probe.speed,
                ),
                Kind::Brook => (Terrain::Brook, 1, i64::from(BROOK_SPEED)),
            };
            best = Some(Wet {
                terrain,
                depth,
                current: current_code(
                    s.heading,
                    u16::try_from(speed.clamp(1, 1000)).expect("small"),
                ),
                level: probe.level,
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
        // Every water within its bank draws the ground down to its own
        // level, the nearest the most: the pull is the strongest one's,
        // the level a mean of the waters' weighted by the cube of each
        // pull, a water lagging behind the strongest left out the more
        // the strongest nears its edge, so the ground at a water's edge
        // is that water's level alone. One water gives the bank's plain blend; two reaches of a
        // mountain river a cell apart and 20 m apart in level, or two
        // brooks converging on a flank, give a saddle between them with
        // no step (15 November 2026; drawing each bank in turn stepped at
        // the higher edge).
        let p = (x * 1000 + 500, y * 1000 + 500);
        let mut pulls: Vec<(i64, i64)> = Vec::new();
        let mut strongest = 0;
        for s in self.stretches_near(x, y) {
            let Probe { d, half, level, .. } = s.probe(p);
            if d < half + s.bank {
                let t = 1000 - smooth((d - half) * 1000 / s.bank);
                strongest = strongest.max(t);
                pulls.push((t, level));
            }
        }
        if strongest > 0 {
            let (mut weighted, mut weights) = (0_i64, 0_i64);
            for (t, level) in pulls {
                // Its share shrinks with its lag behind the strongest,
                // and the room for a lag closes as the strongest nears
                // a water's edge: there, only that water counts.
                let share = (1000 - (strongest - t) * 1000 / (1001 - strongest)).max(0);
                let w = t * t * t * share / 1000;
                weighted += w * level;
                weights += w;
            }
            if weights > 0 {
                let level = weighted / weights;
                h = level + (h - level) * (1000 - strongest) / 1000;
            }
        }
        h
    }

    /// The height of tile `(x, y)` in decimetres, as the wire carries it.
    pub fn height_dm(&self, x: i64, y: i64) -> i16 {
        let dm = (self.height_cm(x, y) + 5).div_euclid(10);
        i16::try_from(dm.clamp(i64::from(i16::MIN), i64::from(i16::MAX))).expect("clamped")
    }

    /// Is tile `(x, y)` in a forest's biome? Its border with the open
    /// ground wanders `EDGE_SHIFT_M` either way.
    pub fn wooded(&self, x: i64, y: i64) -> bool {
        self.biome(x, y).is_forest()
    }

    /// The biome of tile `(x, y)`: its region cell's, the borders between
    /// cells wandering `EDGE_SHIFT_M` either way (the same wandering as
    /// the forest's border, so a forest's edge and its open ground's
    /// trees agree on where the forest ends).
    pub fn biome(&self, x: i64, y: i64) -> Biome {
        let (sx, sy) = self.ragged(x, y, 0, EDGE_SHIFT_M);
        self.cell(&self.biome, sx.div_euclid(CELL), sy.div_euclid(CELL))
    }

    /// The river's widths over the window, for a look at how the banks
    /// walk: the narrowest and the widest width (mm) of a river stretch
    /// wholly inside the window and off any ford, and the widest over the
    /// narrowest within 500 m of each other along a river (per mille).
    #[cfg(test)]
    pub(crate) fn river_breadth(&self) -> (i64, i64, i64) {
        let inside = |p: (i64, i64)| {
            let side = i64::from(self.side) * 1000;
            (0..side).contains(&p.0) && (0..side).contains(&p.1)
        };
        let (mut least, mut most) = (i64::MAX, 0);
        // The widths along the rivers, each river's run of stretches in
        // order through the window: `(how far along, width)`; a ford's
        // stretches count for the way along, not for the widths.
        let mut runs: Vec<(i64, Vec<(i64, i64)>)> = Vec::new();
        let mut last_end = None;
        for s in &self.stretches {
            if s.kind != Kind::River {
                continue;
            }
            if inside(s.a) && inside(s.b) {
                let step = isqrt(
                    u64::try_from((s.b.0 - s.a.0).pow(2) + (s.b.1 - s.a.1).pow(2))
                        .expect("a square"),
                );
                if last_end != Some(s.a) {
                    runs.push((0, Vec::new()));
                }
                let (along, run) = runs.last_mut().expect("one");
                *along += i64::try_from(step).expect("small");
                if !s.ford {
                    let w = s.half.0.0 + s.half.0.1;
                    least = least.min(w);
                    most = most.max(w);
                    run.push((*along, w));
                }
                last_end = Some(s.b);
            }
        }
        let mut spread = 0;
        for (_, run) in &runs {
            for (i, &(from, _)) in run.iter().enumerate() {
                let (mut lo, mut hi) = (i64::MAX, 0);
                for &(at, w) in &run[i..] {
                    if at - from > 500_000 {
                        break;
                    }
                    lo = lo.min(w);
                    hi = hi.max(w);
                }
                if hi > 0 {
                    spread = spread.max(hi * 1000 / lo);
                }
            }
        }
        (least, most, spread)
    }

    /// Where the window's brooks end, in tiles: the downstream end of
    /// every brook stretch that no other brook stretch starts from (the
    /// mouth on a river or a lake, or where the chain stops), for the
    /// safety net to check that each lies in water or off the window.
    #[cfg(test)]
    pub(crate) fn brook_ends(&self) -> Vec<(i64, i64)> {
        let starts: std::collections::BTreeSet<(i64, i64)> = self
            .stretches
            .iter()
            .filter(|s| s.kind == Kind::Brook)
            .map(|s| s.a)
            .collect();
        self.stretches
            .iter()
            .filter(|s| s.kind == Kind::Brook && !starts.contains(&s.b))
            .map(|s| (s.b.0.div_euclid(1000), s.b.1.div_euclid(1000)))
            .collect()
    }

    /// Tile `(x, y)` shifted up to `reach` metres either way by a noise
    /// of its own (`k`), so a border read there wanders.
    fn ragged(&self, x: i64, y: i64, k: u64, reach: i64) -> (i64, i64) {
        let (wx, wy) = self.world(x, y);
        let shift = |k: u64| {
            let s = roll(self.seed, purpose::PLAN_EDGE, &[k]);
            let (xi, yi) = (
                i32::try_from(wx).expect("the continent fits in i32 tiles"),
                i32::try_from(wy).expect("the continent fits in i32 tiles"),
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
            let widest = s.widest();
            let lo = |a: i64, b: i64| (a.min(b) - widest).div_euclid(1000);
            let hi = |a: i64, b: i64| (a.max(b) + widest).div_euclid(1000);
            for y in lo(s.a.1, s.b.1)..=hi(s.a.1, s.b.1) {
                for x in lo(s.a.0, s.b.0)..=hi(s.a.0, s.b.0) {
                    let probe = s.probe((x * 1000 + 500, y * 1000 + 500));
                    if probe.d <= probe.half {
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
                .filter(|s| s.kind == Kind::River)
                .map(|s| {
                    let probe = s.probe(p);
                    probe.d - probe.half
                })
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

#[cfg(test)]
mod tests {
    //! The windows are keyed on the continent (D39): two cells differ, and
    //! the same ground drawn through two windows holds the same things.
    //! Before 15 November 2026 every window held the same woods.

    use std::collections::BTreeSet;

    use super::*;
    use crate::content::Species;
    use crate::flora;
    use crate::geology::Geology;
    use crate::map::{ChunkPos, Map, Pos};
    use crate::open_ground::OpenGround;
    use crate::plan::net::{content, continent, sample};
    use crate::plan::{VIVARIUM_SEED, ZONE_2};
    use crate::trees;

    /// The trees of the chunks `chunks` of `map`: foot (mm) and kind.
    fn trees_of(
        map: &Map,
        species: &[Species],
        open: &OpenGround,
        chunks: impl Iterator<Item = (i32, i32)>,
    ) -> BTreeSet<(i32, i32, usize)> {
        let seed = VIVARIUM_SEED;
        chunks
            .flat_map(|(cx, cy)| trees::trees_in(map, species, open, seed, ChunkPos::new(cx, cy)))
            .map(|t| (t.at.x, t.at.y, t.species))
            .collect()
    }

    /// The chunks of the square from chunk `lo` to chunk `hi` excluded.
    fn block(lo: i32, hi: i32) -> impl Iterator<Item = (i32, i32)> {
        (lo..hi).flat_map(move |cy| (lo..hi).map(move |cx| (cx, cy)))
    }

    #[test]
    fn two_cells_of_the_continent_hold_different_woods() {
        let c = continent();
        let (species, open) = content();
        // The vivarium's cell and the next sampled cell in a forest biome.
        let cells = sample(c);
        let a = cells[0];
        let b = *cells
            .iter()
            .find(|&&cell| cell != a && c.biome[c.grid.index(cell.0, cell.1)].is_forest())
            .expect("a second forest cell");
        let maps = [a, b].map(|cell| Map::on_window(Window::of_cell(c, cell), c.seed));
        // The density of trees, tile by tile over the whole window: the
        // same drawing would agree everywhere.
        let side = i32::try_from(maps[0].width()).expect("small");
        let mut same = 0_u64;
        for y in 0..side {
            for x in 0..side {
                let p = Pos::new(x, y);
                same += u64::from(maps[0].trees(p) == maps[1].trees(p));
            }
        }
        let share = same * 1000 / u64::try_from(side * side).expect("positive");
        assert!(
            share < 900,
            "{share} per mille of the tiles hold the same density"
        );
        // The trees of the heart of each window, foot by foot.
        let [ta, tb] = maps
            .iter()
            .map(|m| trees_of(m, &species, &open, block(24, 40)))
            .collect::<Vec<_>>()
            .try_into()
            .expect("two");
        assert!(
            ta.len() > 200 && tb.len() > 200,
            "{} and {} trees",
            ta.len(),
            tb.len()
        );
        let common = ta.intersection(&tb).count();
        assert!(
            common * 10 < ta.len().min(tb.len()),
            "{common} trees of {} and {} stand at the same foot in cells {a:?} and {b:?}",
            ta.len(),
            tb.len()
        );
    }

    #[test]
    fn the_same_ground_through_two_windows_holds_the_same_things() {
        let c = continent();
        let (species, open) = content();
        let kinds = flora::parse(include_str!("../../../../content/flora.ron")).expect("flora.ron");
        let geology: Geology =
            ron::from_str(include_str!("../../../../content/geology.ron")).expect("geology.ron");
        let per = usize::try_from(region::PER_CELL).expect("positive");
        let region = Region::around(c, ZONE_2.cell, REGION_CELLS * per);
        let corner = REGION_CELLS / 2 * per;
        let side = u32::try_from(super::super::CELL_M).expect("small");
        // Window B is window A moved by `shift` region cells south-east.
        let shift = 4_usize;
        let a = Window::cut(c, &region, (corner, corner), side);
        let b = Window::cut(c, &region, (corner + shift, corner + shift), side);
        let cells = i64::try_from(shift).expect("small");
        assert_eq!(b.origin(), (a.origin().0 + cells, a.origin().1 + cells));
        // The same move in tiles.
        let step = i32::try_from(cells * CELL).expect("small");
        let (ma, mb) = (Map::on_window(a, c.seed), Map::on_window(b, c.seed));
        // A block of 8 × 8 chunks in A, far from both windows' edges (the
        // forest's depth looks 55 tiles) and from both camps (the camp
        // clearing is each window's own).
        let (lo, hi) = (16, 24);
        let chunks = i32::try_from(CHUNK_SIZE).expect("small");
        let tiles = (
            lo * chunks..hi * chunks,
            (lo * chunks - step)..(hi * chunks - step),
        );
        for (map, range) in [(&ma, &tiles.0), (&mb, &tiles.1)] {
            let camp = map.camp();
            assert!(
                !(range.start - 32..range.end + 32).contains(&camp.x)
                    || !(range.start - 32..range.end + 32).contains(&camp.y),
                "the camp {camp:?} lies by the compared block"
            );
        }
        let mut forest = 0;
        for y in tiles.0.clone() {
            for x in tiles.0.clone() {
                let (pa, pb) = (Pos::new(x, y), Pos::new(x - step, y - step));
                let read = |m: &Map, p: Pos| {
                    (
                        m.get(p),
                        m.trees(p),
                        m.habitat(p),
                        m.depth(p),
                        m.elevation(p),
                        m.biome(p),
                    )
                };
                assert_eq!(
                    read(&ma, pa),
                    read(&mb, pb),
                    "tile {pa:?} of A, {pb:?} of B"
                );
                forest += i32::from(ma.trees(pa) > 0);
            }
        }
        assert!(forest > 1000, "{forest} wooded tiles compared");
        let ta = trees_of(&ma, &species, &open, block(lo, hi));
        let tb: BTreeSet<_> = trees_of(
            &mb,
            &species,
            &open,
            block(lo - step / chunks, hi - step / chunks),
        )
        .into_iter()
        .map(|(x, y, k)| (x + step * 1000, y + step * 1000, k))
        .collect();
        assert!(ta.len() > 200, "{} trees", ta.len());
        assert_eq!(ta, tb, "the trees of the block");
        // One chunk's plants and rocks too.
        let (ca, cb) = (
            ChunkPos::new(lo, lo),
            ChunkPos::new(lo - step / chunks, lo - step / chunks),
        );
        let at = |m: &Map, chunk: ChunkPos| trees::trees_in(m, &species, &open, c.seed, chunk);
        let (fa, fb) = (at(&ma, ca), at(&mb, cb));
        let feet = |list: &[trees::Tree]| -> Vec<flora::TreeAt<'_>> {
            list.iter()
                .map(|t| flora::TreeAt {
                    at: t.at,
                    species: &species[t.species].id,
                })
                .collect()
        };
        let plants = |m: &Map,
                      chunk: ChunkPos,
                      list: &[trees::Tree],
                      dx: i32|
         -> BTreeSet<(i32, i32, u16)> {
            flora::plants_in(m, &kinds, &feet(list), c.seed, chunk)
                .into_iter()
                .map(|p| (p.at.x + dx, p.at.y + dx, p.kind))
                .collect()
        };
        let (pa, pb) = (plants(&ma, ca, &fa, 0), plants(&mb, cb, &fb, step * 1000));
        assert!(pa.len() > 20, "{} plants", pa.len());
        assert_eq!(pa, pb, "the plants of the chunk");
        assert_eq!(
            geology.rocks_in(c.seed, ca, ma.origin()),
            geology.rocks_in(c.seed, cb, mb.origin()),
            "the rocks of the chunk"
        );
    }
}
