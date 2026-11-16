//! A region: a square of the continent refined to cells of 64 m
//! (`pipeline.md` §2), enough to show hills, valleys, river courses and
//! lakes. Worked out from the continent cells around it, the seed and
//! absolute coordinates only, so two regions that overlap agree where
//! they overlap: a river's course between two continent cells is drawn
//! from those two cells alone.
//!
//! 1. The ground: the continent's heights interpolated (Catmull-Rom,
//!    integers), plus a detail whose strength follows the rock and the
//!    local relief (granite hilly, chalk soft, alluvium flat).
//! 2. The rivers: each continent river runs from a point in its cell to
//!    the point of its receiver's, along a meandering line (midpoint
//!    displacement, keyed by the two points), its level falling evenly
//!    between the two cells' levels, then settled on this ground (never
//!    above it, never rising; `course::stems`, the courses the window
//!    draws). Its channel is cut below that level and its valley shaped
//!    around it: a floor, a foot, a flank of one slope up to the ground.
//!    Then the ground's ceiling: nothing rises faster than 0.75 m a metre
//!    ([`MAX_RISE_PER_MILLE`]).
//! 3. The water: a priority flood from the sea and the region's edges
//!    fills the hollows; those deep enough are lakes; brooks run where
//!    enough ground drains.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::course;
use super::relief::{Routing, fbm};
use super::{Biome, Continent, Grid, isqrt};
use crate::r#gen::noise;
use crate::r#gen::scatter::{key, purpose, roll};
use crate::geology::Rock;

/// Side of a region cell, in metres.
pub const REGION_CELL_M: i64 = 64;

/// Region cells a side of a continent cell (2,048 m over 64 m).
pub const PER_CELL: i32 = 32;

/// Cells a side of the atlas's region: 65.5 km, 32 continent cells.
pub const REGION_SIDE: usize = 1024;

/// The detail's strength on each rock, in cm either way, and the share
/// of the continent's local relief (the spread of the 3 × 3 cells around,
/// per mille) added to it: granite makes rounded hills, sandstone buttes,
/// limestone plateaus, chalk soft downs, alluvium next to nothing.
const DETAIL_GRANITE_CM: i64 = 3_000;
const DETAIL_SANDSTONE_CM: i64 = 2_000;
const DETAIL_LIMESTONE_CM: i64 = 1_500;
const DETAIL_CHALK_CM: i64 = 900;
const DETAIL_ALLUVIUM_PER_MILLE: i64 = 150;
const DETAIL_RELIEF_PER_MILLE: i64 = 200;

/// The detail's octaves: (cell in region cells, weight).
const DETAIL_OCTAVES: [(u32, i64); 4] = [(48, 8), (20, 4), (8, 2), (3, 1)];

/// Rivers drawn from the continent: at least this discharge (mL/s), so
/// one cubic metre a second. Smaller waters come from the region's own
/// drainage.
pub(crate) const RIVER_DISCHARGE: u64 = 1_000_000;

/// How far a river's point strays from its cell's centre, in region cells
/// either way.
const NODE_JITTER: i64 = 9;

/// Midpoint displacement of a course: halvings, and the largest sideways
/// shift of a midpoint, per mille of its segment's length.
const MEANDER_DEPTH: u32 = 5;
const MEANDER_PER_MILLE: i64 = 280;

/// A river's valley, outward from the channel's edge in region cells: a
/// floor rising `VALLEY_RISE_CM` a cell (about 2 %) over `VALLEY_CELLS`;
/// then a concave foot of `VALLEY_FOOT_CELLS` whose slope grows evenly
/// from the floor's to the flank's; then the flank, a plane at
/// `VALLEY_FLANK_PER_MILLE` (0.4 m a metre, a grassy versant) up to
/// wherever it meets the ground, so that higher ground means a longer
/// flank and never a steeper one (the project lead's rules for relief,
/// D44's drafts). The shoulder where the flank meets the ground is the
/// window's: Catmull-Rom between the 64 m cells rounds the crease over a
/// cell either way; a rounding drawn here would bite into the ground on
/// either side of the crease, a trench along every river of a plain.
/// Before 15 November 2026 the valley stopped at its floor's edge and the
/// ground stood as it was beyond: a wall of up to 120 m in one cell (the
/// net's `Steps`, 37 windows of 108).
const VALLEY_CELLS: i64 = 8;
const VALLEY_RISE_CM: i64 = 130;
const VALLEY_FOOT_CELLS: i64 = 4;
const VALLEY_FLANK_PER_MILLE: i64 = 400;

/// The ground's ceiling off declared rock, per mille (0.75 m a metre, the
/// project lead's rule; no cliff layer exists yet, so nothing is
/// excepted). After the valleys, a pass (`limit_rise`) lowers every cell that
/// rises faster than this from a lower one, so the window's tiles,
/// interpolated between cells, never step more than the net's 1.7 m
/// (Catmull-Rom overshoots a cell's chord by half at most: 1.125 m a
/// tile, plus the small hills). A placeholder, like every value of the
/// relief (D30).
pub const MAX_RISE_PER_MILLE: i64 = 750;

/// A river's flood plain, where alluvium lies: within this many cells of
/// a river of at least this discharge.
const PLAIN_CELLS: i64 = 3;
const PLAIN_DISCHARGE: u64 = 5_000_000;

/// A hollow is a lake when filled at least this deep (cm): shallower
/// hollows of the detail are filled flat (a pond, a bog, the ground
/// raised to the spill level), else every dip of the noise would hold a
/// lake.
const LAKE_MIN_CM: i32 = 800;

/// A brook runs where at least this many cells drain: half a square
/// kilometre (122 cells of 64 m).
const BROOK_CELLS: u32 = 122;

/// How far the continent's biome and rock borders wander, in region
/// cells either way, so they do not draw its 2 km squares.
const BORDER_SHIFT: i64 = 20;

/// A lowland river for the atlas's region: discharge (mL/s) between
/// these, ground under this (cm), and this many continent cells from the
/// grid's edge.
const SITE_DISCHARGE: (u64, u64) = (20_000_000, 600_000_000);
const SITE_HIGHEST_CM: i32 = 30_000;
const SITE_MARGIN: i32 = 24;

/// What lies on a region cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Water {
    /// Dry land.
    Land,
    /// The sea.
    Sea,
    /// A lake.
    Lake,
    /// A river from the continent.
    River,
    /// A brook of the region's own drainage.
    Brook,
}

/// A region of the continent at 64 m.
#[derive(Debug, Clone)]
pub struct Region {
    /// Its grid.
    pub grid: Grid,
    /// The continent cell at its north-west corner.
    pub origin: (i32, i32),
    /// The ground, in cm.
    pub height: Vec<i32>,
    /// What lies on each cell.
    pub water: Vec<Water>,
    /// The water's surface: the ground, or a lake's level where the
    /// flood filled a hollow (cm).
    pub surface: Vec<i32>,
    /// Where each cell's water goes: a neighbour, or itself at an outlet.
    pub receiver: Vec<u32>,
    /// The biome, from the continent.
    pub biome: Vec<Biome>,
    /// The bedrock.
    pub rock: Vec<Rock>,
    /// Whether alluvium covers it.
    pub alluvium: Vec<bool>,
    /// The rivers' courses at the tile scale over the region and a margin
    /// of two continent cells, their levels settled on this ground and
    /// their valleys cut in it: what the window draws.
    pub(crate) stems: Vec<course::Stem>,
}

/// The continent cell the atlas's region is centred on: a lowland river,
/// drawn among the candidates by the seed.
pub fn site(c: &Continent) -> (i32, i32) {
    let side = i32::try_from(c.grid.side).expect("small");
    let mut best: Option<(u64, usize)> = None;
    for strict in [true, false] {
        for i in 0..c.grid.len() {
            let (x, y) = c.grid.xy(i);
            if c.sea[i]
                || c.lake[i]
                || x < SITE_MARGIN
                || y < SITE_MARGIN
                || x >= side - SITE_MARGIN
                || y >= side - SITE_MARGIN
            {
                continue;
            }
            let q = c.discharge[i];
            let fits = if strict {
                (SITE_DISCHARGE.0..=SITE_DISCHARGE.1).contains(&q) && c.surface[i] < SITE_HIGHEST_CM
            } else {
                q >= RIVER_DISCHARGE
            };
            if fits {
                let r = roll(c.seed, purpose::PLAN_SITE, &[i as u64]);
                if best.is_none_or(|(b, _)| r > b) {
                    best = Some((r, i));
                }
            }
        }
        if best.is_some() {
            break;
        }
    }
    best.map_or((side / 2, side / 2), |(_, i)| c.grid.xy(i))
}

/// The valley flank's rise, in cm a region cell.
const fn flank_cm_per_cell() -> i64 {
    VALLEY_FLANK_PER_MILLE * REGION_CELL_M / 10
}

/// The valley's ground over the river's level, `t` cells out from the
/// channel's edge (cm): the floor, the foot, the flank (see
/// [`VALLEY_CELLS`]). Never decreasing in `t`, so a cell cut to it always
/// has a lower neighbour toward the river: no hollow is dug.
fn valley_rise(t: i64) -> i64 {
    let flank = flank_cm_per_cell();
    if t <= VALLEY_CELLS {
        return VALLEY_RISE_CM * t;
    }
    let floor = VALLEY_RISE_CM * VALLEY_CELLS;
    let u = t - VALLEY_CELLS;
    if u <= VALLEY_FOOT_CELLS {
        // The slope grows evenly from the floor's to the flank's.
        return floor
            + VALLEY_RISE_CM * u
            + (flank - VALLEY_RISE_CM) * u * u / (2 * VALLEY_FOOT_CELLS);
    }
    let foot = floor
        + VALLEY_RISE_CM * VALLEY_FOOT_CELLS
        + (flank - VALLEY_RISE_CM) * VALLEY_FOOT_CELLS / 2;
    foot + flank * (u - VALLEY_FOOT_CELLS)
}

/// Lowers every cell that rises faster than [`MAX_RISE_PER_MILLE`] from a
/// lower cell: the ground's lower envelope under planes of that slope,
/// walked up from the low ground (Dijkstra over the eight neighbours, the
/// diagonal √2 longer). It cuts and never fills, so the waters' levels
/// and the valleys' floors stay; and it digs no hollow: a lowered cell
/// sits exactly one step above the cell it was lowered from. A cell is
/// changed only by cells lower than it within their difference over the
/// rise, so the pass is local: a window `relief / rise` cells from the
/// region's edge (3.3 km of relief over the 128 cells of
/// [`Window::of_cell`](super::Window::of_cell)) gets the same ground as
/// from a region of any size (D39).
fn limit_rise(grid: Grid, height: &mut [i32]) {
    let rise = i32::try_from(MAX_RISE_PER_MILLE * REGION_CELL_M / 10).expect("small");
    let diagonal = rise * 1414 / 1000;
    let mut heap: BinaryHeap<Reverse<(i32, u32)>> = height
        .iter()
        .enumerate()
        .map(|(i, &h)| Reverse((h, u32::try_from(i).expect("a region's cells fit"))))
        .collect();
    while let Some(Reverse((h, i))) = heap.pop() {
        let i = i as usize;
        if h > height[i] {
            continue;
        }
        let (x, y) = grid.xy(i);
        for dy in -1..=1 {
            for dx in -1..=1 {
                if (dx, dy) == (0, 0) || !grid.inside(x + dx, y + dy) {
                    continue;
                }
                let j = grid.index(x + dx, y + dy);
                let cap = h + if dx != 0 && dy != 0 { diagonal } else { rise };
                if cap < height[j] {
                    height[j] = cap;
                    heap.push(Reverse((
                        cap,
                        u32::try_from(j).expect("a region's cells fit"),
                    )));
                }
            }
        }
    }
}

/// Catmull-Rom weights at `t` in 0..32, over 2 × 32³.
fn weights(t: i64) -> [i64; 4] {
    let f = i64::from(PER_CELL);
    [
        -t * t * t + 2 * t * t * f - t * f * f,
        3 * t * t * t - 5 * t * t * f + 2 * f * f * f,
        -3 * t * t * t + 4 * t * t * f + t * f * f,
        t * t * t - t * t * f,
    ]
}

/// The point of continent cell `(x, y)`'s river, in absolute region
/// cells.
pub(crate) fn node(seed: u64, x: i32, y: i32) -> (i64, i64) {
    let jitter = |k: u64| {
        i64::try_from(
            roll(
                seed,
                purpose::PLAN_NODE,
                &[key(i64::from(x)), key(i64::from(y)), k],
            ) % 1001,
        )
        .expect("small")
            * (2 * NODE_JITTER)
            / 1000
            - NODE_JITTER
    };
    let half = i64::from(PER_CELL / 2);
    (
        i64::from(x) * i64::from(PER_CELL) + half + jitter(0),
        i64::from(y) * i64::from(PER_CELL) + half + jitter(1),
    )
}

/// The meandering course from `a` to `b`, in absolute region cells, both
/// ends included; the same for the same two ends, whichever region asks.
pub(crate) fn course(seed: u64, a: (i64, i64), b: (i64, i64)) -> Vec<(i64, i64)> {
    fn split(seed: u64, a: (i64, i64), b: (i64, i64), depth: u32, out: &mut Vec<(i64, i64)>) {
        if depth == 0 {
            out.push(b);
            return;
        }
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let r = roll(
            seed,
            purpose::PLAN_MEANDER,
            &[key(a.0), key(a.1), key(b.0), key(b.1)],
        );
        let shift = (i64::try_from(r % 2001).expect("small") - 1000) * MEANDER_PER_MILLE / 1000;
        let mid = (
            i64::midpoint(a.0, b.0) - dy * shift / 1000,
            i64::midpoint(a.1, b.1) + dx * shift / 1000,
        );
        split(seed, a, mid, depth - 1, out);
        split(seed, mid, b, depth - 1, out);
    }
    let mut out = vec![a];
    split(seed, a, b, MEANDER_DEPTH, &mut out);
    out
}

impl Region {
    /// The region `side` cells a side centred on continent cell `centre`.
    #[allow(clippy::too_many_lines)]
    pub fn around(c: &Continent, centre: (i32, i32), side: usize) -> Self {
        let grid = Grid { side };
        let n = grid.len();
        let cells = i32::try_from(side).expect("small") / PER_CELL;
        let origin = (centre.0 - cells / 2, centre.1 - cells / 2);
        let (ox, oy) = (
            i64::from(origin.0) * i64::from(PER_CELL),
            i64::from(origin.1) * i64::from(PER_CELL),
        );
        let cs = i32::try_from(c.grid.side).expect("small");
        let at = |x: i32, y: i32| c.grid.index(x.clamp(0, cs - 1), y.clamp(0, cs - 1));
        let half = i64::from(PER_CELL / 2);
        let per = i64::from(PER_CELL);
        let abs = |i: usize| {
            let (x, y) = grid.xy(i);
            (ox + i64::from(x), oy + i64::from(y))
        };
        // The continent cell under an absolute region point, and the
        // Catmull-Rom frame around it.
        let frame = |gx: i64, gy: i64| {
            let (u, v) = (gx - half, gy - half);
            let (cx, cy) = (u.div_euclid(per), v.div_euclid(per));
            (cx, cy, u.rem_euclid(per), v.rem_euclid(per))
        };
        let interpolate = |values: &dyn Fn(i32, i32) -> i64, gx: i64, gy: i64| -> i64 {
            let (cx, cy, tx, ty) = frame(gx, gy);
            let (wx, wy) = (weights(tx), weights(ty));
            let mut sum = 0_i64;
            for (j, wyj) in wy.iter().enumerate() {
                let mut row = 0_i64;
                for (k, wxk) in wx.iter().enumerate() {
                    let (x, y) = (cx - 1 + k as i64, cy - 1 + j as i64);
                    row += wxk
                        * values(
                            i32::try_from(x).expect("small"),
                            i32::try_from(y).expect("small"),
                        );
                }
                sum += wyj * (row / 65_536);
            }
            sum / 65_536
        };
        let ground = |x: i32, y: i32| i64::from(c.height[at(x, y)]);
        let amplitude = |x: i32, y: i32| -> i64 {
            let i = at(x, y);
            let mut lo = i64::MAX;
            let mut hi = i64::MIN;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let h = i64::from(c.surface[at(x + dx, y + dy)]);
                    lo = lo.min(h);
                    hi = hi.max(h);
                }
            }
            let base = match c.rock[i] {
                Rock::Granite => DETAIL_GRANITE_CM,
                Rock::Sandstone => DETAIL_SANDSTONE_CM,
                Rock::Limestone => DETAIL_LIMESTONE_CM,
                Rock::Chalk => DETAIL_CHALK_CM,
            };
            let base = if c.alluvium[i] {
                base * DETAIL_ALLUVIUM_PER_MILLE / 1000
            } else {
                base
            };
            if c.sea[i] {
                0
            } else {
                base + (hi - lo) * DETAIL_RELIEF_PER_MILLE / 1000
            }
        };
        let total: i64 = DETAIL_OCTAVES.iter().map(|o| o.1).sum();
        // How far the continent's lakes and filled hollows stand over its
        // ground (0 off them and on the sea): near a lake, the lake's
        // surface is what a river settles on (`ground_around`), never its
        // bed.
        let lake_depth = |x: i32, y: i32| {
            let i = at(x, y);
            if c.sea[i] {
                0
            } else {
                i64::from(c.surface[i] - c.height[i])
            }
        };
        let mut height = vec![0_i32; n];
        let mut lake_surface = vec![i32::MIN; n];
        let mut sea = vec![false; n];
        for i in 0..n {
            let (gx, gy) = abs(i);
            let base = interpolate(&ground, gx, gy);
            let depth = interpolate(&lake_depth, gx, gy);
            if depth > 0 {
                lake_surface[i] = i32::try_from(base + depth).expect("small");
            }
            let (cx, cy, _, _) = frame(gx, gy);
            let near_sea = (0..2).any(|dy| {
                (0..2).any(|dx| {
                    c.sea[at(
                        i32::try_from(cx + dx).expect("small"),
                        i32::try_from(cy + dy).expect("small"),
                    )]
                })
            });
            sea[i] = base < 0 && near_sea;
            if sea[i] {
                height[i] = i32::try_from(base).expect("small");
                continue;
            }
            // The detail never swings deeper than the ground stands above
            // the sea: a lowland's dips stay over the sea's level. Before
            // 15 November 2026 a steppe 33 m up swung over 33 m, its river
            // settled on a dip at the sea's level and kept that level for
            // the rest of its course, 33 m under the plain ((391, 441) of
            // seed 14), and coastal land fell under the sea's level
            // ((787, 719), (288, 569); the net's `SeaAsLand`).
            let amp = interpolate(&amplitude, gx, gy).clamp(0, base.max(0));
            let mut detail = 0_i64;
            for (k, &(cell, w)) in DETAIL_OCTAVES.iter().enumerate() {
                let s = roll(c.seed, purpose::PLAN_DETAIL, &[k as u64]);
                let (x, y) = (
                    i32::try_from(gx).expect("small"),
                    i32::try_from(gy).expect("small"),
                );
                detail += (i64::from(noise(s, cell, x, y)) - 500) * w;
            }
            height[i] = i32::try_from(base + amp * detail / (total * 500)).expect("small");
        }

        // The rivers of the continent cells around, carved in along the
        // courses the window draws (`course::stems`: rounded, refined,
        // the tributaries cut at their junctions), so that the valley
        // lies where the water will run. Each cell takes the valley of
        // the nearest point of any course (the first in order on a tie),
        // not the lowest: a mountain river falls metres a cell and
        // meanders, so a lower reach downstream would otherwise cut the
        // ground from under its own upper reach, which the window then
        // drew as a dyke, the water metres above the ground beside it
        // ((631, 624) of seed 14, 15 November 2026). Where two valleys
        // meet, the ceiling below mends the step.
        let mut river = vec![false; n];
        let mut plain = vec![false; n];
        let mut nearest = vec![i64::MAX; n];
        let mut cap = vec![i32::MAX; n];
        let mut floor = vec![i32::MIN; n];
        let margin = 2;
        let cell_mm = REGION_CELL_M * 1000;
        // The ground around a course's point, before the valleys: the
        // lowest of the nine cells around it, none past the region; on a
        // lake of the continent its surface, not its bed. A river crossing
        // a lake cell of the atlas (its bed 24 m under the lake's surface)
        // dived to the bed, clamped to the sea's level, and kept that
        // level for the rest of its course: 7 km at the sea's level
        // through land 14 m up ((438, 376) of seed 14, 15 November 2026;
        // the net's `SeaAsLand`), and a tributary fell 3.4 m in a few
        // metres to meet its stem so lowered ((149, 564), `Steps`).
        let ground_around = |p: (i64, i64)| -> Option<i64> {
            let (px, py) = (p.0.div_euclid(cell_mm) - ox, p.1.div_euclid(cell_mm) - oy);
            let s = i64::try_from(side).expect("small");
            if px < 1 || py < 1 || px >= s - 1 || py >= s - 1 {
                return None;
            }
            let mut low = i64::MAX;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let j = usize::try_from((py + dy) * s + px + dx).expect("inside");
                    low = low.min(i64::from(height[j].max(lake_surface[j])));
                }
            }
            Some(low)
        };
        let stems = course::stems(
            c,
            (origin.0 - margin)..(origin.0 + cells + margin),
            (origin.1 - margin)..(origin.1 + cells + margin),
            Some(&ground_around),
        );
        // The highest ground a course's cell can meet: the continent's
        // surface around it plus the detail's full swing.
        let highest = |i: usize| -> i64 {
            let (x, y) = c.grid.xy(i);
            let mut top = i64::MIN;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (x, y) = (x + dx, y + dy);
                    top = top.max(i64::from(c.surface[at(x, y)]) + amplitude(x, y));
                }
            }
            top
        };
        for stem in &stems {
            let mut last: Option<(i64, i64)> = None;
            for (k, node) in stem.nodes.iter().enumerate() {
                // A sample every half cell along the course.
                if last.is_some_and(|l| {
                    (node.p.0 - l.0).abs().max((node.p.1 - l.1).abs()) < cell_mm / 2
                }) && k + 1 < stem.nodes.len()
                {
                    continue;
                }
                last = Some(node.p);
                let (px, py) = (node.p.0.div_euclid(cell_mm), node.p.1.div_euclid(cell_mm));
                let q = c.discharge[node.cell];
                let level = node.level;
                let depth = node.depth;
                let half_width = node.width / 2 / cell_mm;
                let climb = (highest(node.cell) - level).max(0);
                let reach = half_width
                    + VALLEY_CELLS
                    + VALLEY_FOOT_CELLS
                    + (climb + flank_cm_per_cell() - 1) / flank_cm_per_cell();
                for dy in -reach..=reach {
                    for dx in -reach..=reach {
                        let (lx, ly) = (px + dx - ox, py + dy - oy);
                        if lx < 0 || ly < 0 || lx >= side as i64 || ly >= side as i64 {
                            continue;
                        }
                        let j = usize::try_from(ly).expect("inside") * side
                            + usize::try_from(lx).expect("inside");
                        if sea[j] {
                            continue;
                        }
                        let d2 = dx * dx + dy * dy;
                        let d = i64::try_from(isqrt(u64::try_from(d2).expect("square")))
                            .expect("small");
                        if d > reach {
                            continue;
                        }
                        if q >= PLAIN_DISCHARGE && d <= half_width + PLAIN_CELLS {
                            plain[j] = true;
                        }
                        if d2 >= nearest[j] {
                            continue;
                        }
                        nearest[j] = d2;
                        river[j] = d <= half_width;
                        cap[j] = i32::try_from(if d <= half_width {
                            level - depth
                        } else {
                            level + valley_rise(d - half_width)
                        })
                        .expect("small");
                        // The floor never lies under the water: a dip of
                        // the detail beside the river is filled to its
                        // level, else it was a lake lower than the river
                        // a cell away, with a step between.
                        floor[j] = if d <= half_width {
                            cap[j]
                        } else if d - half_width <= VALLEY_CELLS {
                            i32::try_from(level).expect("small")
                        } else {
                            i32::MIN
                        };
                    }
                }
            }
        }
        for (i, h) in height.iter_mut().enumerate() {
            *h = (*h).clamp(floor[i], cap[i]);
        }

        // The ground's ceiling: no rise over `MAX_RISE_PER_MILLE`.
        limit_rise(grid, &mut height);

        // The water: a flood from the sea and the edges.
        let outlet: Vec<bool> = (0..n)
            .map(|i| {
                let (x, y) = grid.xy(i);
                let s = i32::try_from(side).expect("small");
                sea[i] || x == 0 || y == 0 || x == s - 1 || y == s - 1
            })
            .collect();
        let mut routing = Routing::new(n);
        routing.route_from(grid, &height, &outlet, &sea);
        let mut area: Vec<u32> = vec![1; n];
        for &k in routing.order.iter().rev() {
            let k = k as usize;
            let r = routing.receiver[k] as usize;
            if r != k {
                area[r] += area[k];
            }
        }
        let water = (0..n)
            .map(|i| {
                if sea[i] {
                    Water::Sea
                } else if routing.filled[i] - height[i] >= LAKE_MIN_CM {
                    Water::Lake
                } else if river[i] {
                    Water::River
                } else if area[i] >= BROOK_CELLS {
                    Water::Brook
                } else {
                    Water::Land
                }
            })
            .collect::<Vec<Water>>();
        // A hollow too shallow for a lake is a pond or a bog: filled
        // flat to its spill level, so the ground holds no basin and a
        // lake's shore, where the flood stood under `LAKE_MIN_CM`, lies
        // at the lake's level and not metres under it (the net's `Steps`
        // and `Pits` on lake shores, 15 November 2026).
        for i in 0..n {
            if !sea[i] && water[i] != Water::Lake {
                height[i] = height[i].max(routing.filled[i]);
            }
        }

        // Biome and rock from the continent, their borders shifted.
        let mut biome = vec![Biome::Sea; n];
        let mut rock = vec![Rock::Granite; n];
        let mut alluvium = vec![false; n];
        for i in 0..n {
            let (gx, gy) = abs(i);
            let (x, y) = (
                i32::try_from(gx).expect("small"),
                i32::try_from(gy).expect("small"),
            );
            let shift = |p: u64| {
                fbm(
                    c.seed,
                    purpose::PLAN_BORDER ^ p,
                    1000,
                    x,
                    y,
                    &[(40, 2), (12, 1)],
                ) * BORDER_SHIFT
                    / 250
            };
            let (sx, sy) = (
                (gx + shift(1)).div_euclid(per),
                (gy + shift(2)).div_euclid(per),
            );
            let k = at(
                i32::try_from(sx).expect("small"),
                i32::try_from(sy).expect("small"),
            );
            biome[i] = if sea[i] {
                Biome::Sea
            } else if c.sea[k] || c.lake[k] {
                // A cell the continent calls water but the region does not.
                Biome::Grassland
            } else {
                c.biome[k]
            };
            rock[i] = c.rock[k];
            alluvium[i] = plain[i] || (c.alluvium[k] && !c.sea[k]);
        }
        Self {
            grid,
            origin,
            height,
            water,
            surface: routing.filled,
            receiver: routing.receiver,
            biome,
            rock,
            alluvium,
            stems,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cell lower than its eight neighbours, strictly.
    fn hollows(grid: Grid, height: &[i32]) -> Vec<usize> {
        let side = i32::try_from(grid.side).expect("small");
        (0..grid.len())
            .filter(|&i| {
                let (x, y) = grid.xy(i);
                x > 0
                    && y > 0
                    && x < side - 1
                    && y < side - 1
                    && (-1..=1).all(|dy| {
                        (-1..=1).all(|dx| {
                            (dx, dy) == (0, 0) || height[grid.index(x + dx, y + dy)] > height[i]
                        })
                    })
            })
            .collect()
    }

    /// The largest rise between two neighbours, orthogonal and diagonal.
    fn steepest(grid: Grid, height: &[i32]) -> (i32, i32) {
        let (mut orth, mut diag) = (0, 0);
        for i in 0..grid.len() {
            let (x, y) = grid.xy(i);
            for (dx, dy) in [(1, 0), (0, 1), (1, 1), (1, -1)] {
                if grid.inside(x + dx, y + dy) {
                    let d = (height[grid.index(x + dx, y + dy)] - height[i]).abs();
                    if dx != 0 && dy != 0 {
                        diag = diag.max(d);
                    } else {
                        orth = orth.max(d);
                    }
                }
            }
        }
        (orth, diag)
    }

    #[test]
    fn the_valley_never_falls_outward_and_holds_its_three_slopes() {
        let flank = flank_cm_per_cell();
        assert_eq!(flank, 2560, "0.4 m a metre over a 64 m cell");
        assert_eq!(valley_rise(0), 0);
        for t in 0..60 {
            let step = valley_rise(t + 1) - valley_rise(t);
            assert!(step >= 0, "falls at {t}");
            if t < VALLEY_CELLS {
                assert_eq!(step, VALLEY_RISE_CM, "the floor at {t}");
            } else if t < VALLEY_CELLS + VALLEY_FOOT_CELLS {
                assert!(
                    (VALLEY_RISE_CM..=flank).contains(&step),
                    "the foot at {t}: {step}"
                );
            } else {
                assert_eq!(step, flank, "the flank at {t}");
            }
        }
        // The foot's slope grows: each step larger than the last.
        for t in VALLEY_CELLS..VALLEY_CELLS + VALLEY_FOOT_CELLS {
            assert!(valley_rise(t + 2) - valley_rise(t + 1) > valley_rise(t + 1) - valley_rise(t));
        }
    }

    #[test]
    fn the_ceiling_cuts_the_walls_fills_nothing_and_digs_no_hollow() {
        let grid = Grid { side: 24 };
        // A plateau 200 m over a plain, a hollow in the plateau, a wall
        // along the plain's edge, and a dip in the plain: hollows exist
        // before, and none may be added.
        let before: Vec<i32> = (0..grid.len())
            .map(|i| {
                let (x, y) = grid.xy(i);
                match (x, y) {
                    (10, 6) => 15_000,
                    (4, 18) => -300,
                    _ if y < 12 => 20_000 + (x * 37 % 5) * 100,
                    _ => (x * 53 % 7) * 100,
                }
            })
            .collect();
        let mut after = before.clone();
        limit_rise(grid, &mut after);
        let rise = i32::try_from(MAX_RISE_PER_MILLE * REGION_CELL_M / 10).expect("small");
        let (orth, diag) = steepest(grid, &after);
        assert!(orth <= rise, "{orth} cm a cell over {rise}");
        assert!(diag <= rise * 1414 / 1000, "{diag} cm over a diagonal");
        assert!(
            before.iter().zip(&after).all(|(b, a)| a <= b),
            "the pass only cuts"
        );
        assert!(
            after.iter().zip(&before).any(|(a, b)| a < b),
            "the wall was cut"
        );
        let plain = (0..grid.len()).filter(|&i| grid.xy(i).1 >= 12);
        assert!(
            plain.clone().all(|i| after[i] == before[i]),
            "the plain, the low ground, is untouched"
        );
        let (h0, h1) = (hollows(grid, &before), hollows(grid, &after));
        assert!(
            h1.iter().all(|h| h0.contains(h)),
            "new hollows: {h1:?} not in {h0:?}"
        );
        assert!(h1.contains(&grid.index(4, 18)), "the plain's dip stays");
        let mut again = after.clone();
        limit_rise(grid, &mut again);
        assert_eq!(again, after, "a second pass changes nothing");
    }
}
