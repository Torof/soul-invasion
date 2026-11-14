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
//!    between the two cells' levels. Its channel is cut below that level
//!    and its valley shaped around it.
//! 3. The water: a priority flood from the sea and the region's edges
//!    fills the hollows; those deep enough are lakes; brooks run where
//!    enough ground drains.

use super::relief::{Routing, fbm};
use super::{Biome, Continent, Grid, isqrt, measure};
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

/// A river's valley: how far it shapes the ground, in region cells, and
/// how steeply its sides rise, in cm a cell (about 2 %).
const VALLEY_CELLS: i64 = 8;
const VALLEY_RISE_CM: i64 = 130;

/// A river's flood plain, where alluvium lies: within this many cells of
/// a river of at least this discharge.
const PLAIN_CELLS: i64 = 3;
const PLAIN_DISCHARGE: u64 = 5_000_000;

/// A hollow is a lake when filled at least this deep (cm): shallower
/// hollows of the detail are filled (a pond, a bog), else every dip of
/// the noise would hold a lake.
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
        let mut height = vec![0_i32; n];
        let mut sea = vec![false; n];
        for i in 0..n {
            let (gx, gy) = abs(i);
            let base = interpolate(&ground, gx, gy);
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
            let amp = interpolate(&amplitude, gx, gy).max(0);
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

        // The rivers of the continent cells around, carved in.
        let mut river = vec![false; n];
        let mut plain = vec![false; n];
        let margin = 2;
        for y in (origin.1 - margin)..(origin.1 + cells + margin) {
            for x in (origin.0 - margin)..(origin.0 + cells + margin) {
                if !c.grid.inside(x, y) {
                    continue;
                }
                let i = c.grid.index(x, y);
                let q = c.discharge[i];
                if c.sea[i] || q < RIVER_DISCHARGE {
                    continue;
                }
                let r = c.receiver[i] as usize;
                let (rx, ry) = c.grid.xy(r);
                let path = course(c.seed, node(c.seed, x, y), node(c.seed, rx, ry));
                let (top, bottom) = (i64::from(c.surface[i]), i64::from(c.surface[r]));
                let width = i64::try_from(measure::width_cm(q)).expect("small");
                let depth = i64::try_from(measure::depth_cm(q)).expect("small");
                let half_width = width / (REGION_CELL_M * 100) / 2;
                let reach = VALLEY_CELLS + half_width;
                let steps = path.len() - 1;
                for (k, pair) in path.windows(2).enumerate() {
                    let (a, b) = (pair[0], pair[1]);
                    let samples = (b.0 - a.0).abs().max((b.1 - a.1).abs()).max(1);
                    for s in 0..=samples {
                        let (px, py) = (
                            a.0 + (b.0 - a.0) * s / samples,
                            a.1 + (b.1 - a.1) * s / samples,
                        );
                        // The level falls evenly along the course.
                        let along = (k as i64) * samples + s;
                        let level = top - (top - bottom) * along / (steps as i64 * samples);
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
                                let d = i64::try_from(isqrt(
                                    u64::try_from(dx * dx + dy * dy).expect("square"),
                                ))
                                .expect("small");
                                if d > reach {
                                    continue;
                                }
                                let cap = if d <= half_width {
                                    river[j] = true;
                                    level - depth
                                } else {
                                    level + VALLEY_RISE_CM * (d - half_width)
                                };
                                height[j] = height[j].min(i32::try_from(cap).expect("small"));
                                if q >= PLAIN_DISCHARGE && d <= half_width + PLAIN_CELLS {
                                    plain[j] = true;
                                }
                            }
                        }
                    }
                }
            }
        }

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
            .collect();

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
        }
    }
}
