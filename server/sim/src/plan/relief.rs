//! The land, its uplift, the erosion that carves it and the rock it
//! uncovers: passes 1 to 4 of [`super`].
//!
//! Heights are centimetres; the uplift is centimetres a pass. A pass is
//! the erosion's time step: it has no length in years, only the balance
//! between uplift and the rivers' cutting matters, and the numbers below
//! are tuned by looking at the atlas.

use super::{CELL_M, DIAGONAL_PER_MILLE, Grid, isqrt};
use crate::r#gen::noise;
use crate::r#gen::scatter::{purpose, roll};
use crate::geology::Rock;

// ---------------------------------------------------------------- outline

/// Share of the grid that is land, per mille. A continent of about 1,300
/// to 1,600 km across in a square of 2,100, with sea all around: the sea
/// is the world's edge, and every river ends in it or in a lake.
pub const LAND_PER_MILLE: u32 = 340;

/// Cells along each edge that are always sea, so no land touches the
/// grid's border and every river finds the sea.
pub const EDGE_SEA_CELLS: i32 = 4;

/// The outline's noise: (cell as a share of the side per mille, weight).
/// Broad first, so one main landmass; the finer octaves make capes, bays
/// and islands.
const OUTLINE_OCTAVES: [(u32, i64); 6] =
    [(400, 32), (200, 16), (100, 9), (50, 5), (25, 3), (12, 2)];

/// How far the outline's noise is warped, per mille of the side: coasts
/// that wander instead of value noise's round blobs.
const WARP_PER_MILLE: i64 = 60;

/// The outline noise's weight against the falloff: higher, a more
/// ragged continent; lower, a rounder one.
const OUTLINE_GAIN: i64 = 2;

/// How strongly the land falls off toward the grid's edges, in the
/// noise's units at the corner: the main mass sits in the middle.
const FALLOFF: i64 = 1_500;

/// A band along the grid's edges, per mille of the side, over which the
/// outline sinks by up to this much (the noise's units): the land keeps
/// clear of the edges, so the sea surrounds it.
const EDGE_BAND_PER_MILLE: i64 = 80;
const EDGE_DROWN: i64 = 1_200;

/// How far the middle of the land may sit off the grid's centre, per
/// mille of the side.
const CENTRE_JITTER_PER_MILLE: i64 = 50;

/// Sea depth a unit of the outline's noise below the coast, in cm, and
/// the deepest sea drawn: only for the pictures' shading.
const SEA_DEPTH_PER_UNIT_CM: i64 = 2_000;
const SEA_DEEPEST_CM: i64 = 400_000;

// ---------------------------------------------------------------- uplift

/// The uplift of the land between strokes, in cm a pass, as a broad swell
/// between these two: low plains and rolling uplands.
const SWELL_MIN_CM: i64 = 40;
const SWELL_MAX_CM: i64 = 1_000;

/// The swell's noise: (cell per mille of the side, weight).
const SWELL_OCTAVES: [(u32, i64); 3] = [(160, 4), (80, 2), (40, 1)];

/// Ranges drawn when the lead gives none: how many, their vertices,
/// segment length and half-width (per mille of the side), and their
/// uplift at the crest (cm a pass). Two to five chains: a continent has a
/// few, not a crumpled sheet.
const RANGES: (u64, u64) = (2, 5);
const RANGE_VERTICES: (u64, u64) = (3, 6);
const RANGE_SEGMENT_PER_MILLE: (u64, u64) = (50, 110);
const RANGE_HALF_WIDTH_PER_MILLE: (u64, u64) = (40, 75);
const RANGE_UPLIFT_CM: (u64, u64) = (4_000, 8_000);

/// A range's crest is not even: its uplift is multiplied by a noise
/// between these, per mille, with a cell of this per mille of the side,
/// so peaks and cols come out (the cols are where people will cross).
const PEAKS_PER_MILLE: (i64, i64) = (300, 1_500);
const PEAKS_CELL_PER_MILLE: u32 = 30;

/// Plateaus: a broad block lifted evenly, with soft edges.
const PLATEAUS: (u64, u64) = (1, 3);
const PLATEAU_HALF_WIDTH_PER_MILLE: (u64, u64) = (50, 100);
const PLATEAU_UPLIFT_CM: (u64, u64) = (500, 1_300);

/// Basins: a block that sinks, where lakes and sediments gather. It
/// sinks by more than the swell lifts, or it would not sink at all.
const BASINS: (u64, u64) = (1, 4);
const BASIN_HALF_WIDTH_PER_MILLE: (u64, u64) = (15, 40);
const BASIN_SINK_CM: (u64, u64) = (500, 1_100);

/// Where strokes overlap their uplifts add up; past this knee (cm a pass)
/// only a quarter of the excess is kept, so two crossing ranges make a
/// massif of 5 to 6 km, not a 7 km spike, while a single strong range
/// still reaches 4 to 5 km.
const UPLIFT_KNEE_CM: i64 = 10_000;
const UPLIFT_PAST_KNEE_PER_MILLE: i64 = 300;

// ---------------------------------------------------------------- erosion

/// Passes of the erosion. Enough for the ranges to reach their balance
/// between uplift and cutting (a crest settles in about twenty).
pub const PASSES: i32 = 60;

/// The rivers' cutting a pass, per mille, before the rock's factor: the
/// `K dt / dx` of the stream-power law, with the drained area counted in
/// cells. A cell that drains only itself drops toward its receiver by
/// the uplift over this a pass at balance.
const CUTTING_PER_MILLE: i64 = 350;

/// In a hollow, sediment fills toward the outlet at this fraction of the
/// cutting rate, per mille: slowly, so the deeper basins keep a lake.
const FILLING_PER_MILLE: i64 = 100;

/// The first ground: this high, plus a roughness up to this much, in
/// centimetres, plus a gentle dome toward the land's middle. Without the
/// roughness, flats drain in straight lines.
const START_CM: i64 = 200;
const ROUGH_CM: i64 = 3_000;
const DOME_CM_PER_UNIT: i64 = 40;

// ---------------------------------------------------------------- rock

/// The three covers' thickest, in cm: chalk on top (laid in warm shallow
/// seas), limestone under it, sandstone under that, then the granite
/// basement. Plains keep their cover, ranges lose it (they are cut
/// kilometres deep), and the uplands between show whatever layer the
/// erosion reached.
const CHALK_CM: i64 = 30_000;
const LIMESTONE_CM: i64 = 60_000;
const SANDSTONE_CM: i64 = 70_000;

/// Each cover's noise, at several scales (cell per mille of the side,
/// weight: 190, 60 and 20 km), and where in the noise's range (-500..500)
/// it starts and reaches its thickest: chalk and limestone over half the
/// land each, sandstone nearly everywhere, all of them thinning and
/// thickening from one valley to the next.
const COVER_OCTAVES: [(u32, i64); 3] = [(90, 4), (30, 2), (10, 2)];
const CHALK_SPREAD: (i64, i64) = (-40, 140);
const LIMESTONE_SPREAD: (i64, i64) = (-60, 120);
const SANDSTONE_SPREAD: (i64, i64) = (-300, 0);

/// The layers are folded and faulted: the depth at which a cell meets
/// them is shifted by up to this much either way (cm), by a noise of
/// cells of 16 and 6 km, so the erosion's surface cuts every layer in
/// patches and each region shows a bit of every rock; the big structures
/// (granite ranges, covered plains) still follow the relief's history.
const FOLD_CM: i64 = 30_000;
const FOLD_OCTAVES: [(u32, i64); 2] = [(8, 2), (3, 1)];

/// How fast each rock gives way to a river, per mille of
/// [`CUTTING_PER_MILLE`]: granite resists, limestone holds plateaus up,
/// sandstone and chalk give, fresh sediment gives most.
const GRANITE_K: i64 = 550;
const LIMESTONE_K: i64 = 750;
const SANDSTONE_K: i64 = 1_150;
const CHALK_K: i64 = 1_400;
const SEDIMENT_K: i64 = 1_700;

/// A hollow is a lake when the flood filled it at least this deep, in
/// cm; shallower, it is a wet flat.
pub const LAKE_MIN_DEPTH_CM: i32 = 400;

/// Small lakes the erosion's 2 km cells smooth away, put back where
/// nature makes them: per 100,000 land cells, everywhere (landslide and
/// beaver dams, karst sinks, old river loops), and up to this many more
/// where the land is cold (the scoured rock of former ice sheets and the
/// cirques of high ranges: Finland's and Canada's lake districts).
const SCATTERED_LAKES: u64 = 230;
const GLACIAL_LAKES: u64 = 700;
/// Colder than this at the ground (tenths of a degree, yearly mean), the
/// ice scoured; the scouring is fullest this much colder.
const GLACIAL_TENTHS: i64 = 60;
const GLACIAL_FULL_TENTHS: i64 = 80;
/// A small lake's hollow below its rim, in cm.
const SMALL_LAKE_DEPTH_CM: (u64, u64) = (800, 3_500);

/// A lake keeps a share of its basin drawn for it, per mille, by a table
/// on a draw per mille, at least [`LAKE_LEAST_CELLS`] and at most
/// [`LAKE_MOST_CELLS`] cells: most basins keep a small lake, some a
/// medium one, a big basin now and then a big lake (6,000 km² at most,
/// Lake Ontario's ten-thousand). The rest of the basin is silted up (see
/// `drain_lakes`).
const LAKE_SHARES: [(i64, i64); 4] = [(0, 20), (500, 70), (850, 220), (1000, 500)];
const LAKE_LEAST_CELLS: usize = 3;
const LAKE_MOST_CELLS: usize = 1_500;

/// Alluvium lies where a river of at least this many drained cells runs
/// this gently (cm of drop a cell) or lower: a floodplain.
const ALLUVIUM_AREA: u32 = 400;
const ALLUVIUM_DROP_CM: i32 = 150;

// ---------------------------------------------------------------- strokes

/// What a stroke does to the uplift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeKind {
    /// A chain of mountains along its line: the uplift peaks on the line
    /// and fades to its half-width, uneven along the crest.
    Range,
    /// A block lifted evenly, soft-edged.
    Plateau,
    /// A block that sinks: lakes and sediment.
    Basin,
}

/// A stroke on the uplift: a line (or a single point) with a half-width
/// and a strength. The generator draws its own from the seed; the
/// project lead's own strokes are meant to come in here, as content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stroke {
    /// What it does.
    pub kind: StrokeKind,
    /// Its line, in cells.
    pub points: Vec<(i32, i32)>,
    /// How far it reaches from its line, in cells.
    pub half_width: i32,
    /// Uplift on its line, in cm a pass (a basin's is how far it sinks).
    pub strength: i32,
}

/// Sixteen headings, as a cosine and a sine per mille.
pub(crate) const HEADINGS: [(i64, i64); 16] = [
    (1000, 0),
    (924, 383),
    (707, 707),
    (383, 924),
    (0, 1000),
    (-383, 924),
    (-707, 707),
    (-924, 383),
    (-1000, 0),
    (-924, -383),
    (-707, -707),
    (-383, -924),
    (0, -1000),
    (383, -924),
    (707, -707),
    (924, -383),
];

/// The rock layers as the erosion meets them, from the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Sediment,
    Chalk,
    Limestone,
    Sandstone,
    Granite,
}

impl Layer {
    fn cutting(self) -> i64 {
        match self {
            Self::Sediment => SEDIMENT_K,
            Self::Chalk => CHALK_K,
            Self::Limestone => LIMESTONE_K,
            Self::Sandstone => SANDSTONE_K,
            Self::Granite => GRANITE_K,
        }
    }
}

/// A value in -500..500: octaves of [`noise`] for `purpose`, each a cell
/// of a share of `side` (per mille) and a weight.
pub(crate) fn fbm(
    seed: u64,
    purpose: u64,
    side: usize,
    x: i32,
    y: i32,
    octaves: &[(u32, i64)],
) -> i64 {
    let mut sum = 0;
    let mut weights = 0;
    for (k, &(per_mille, weight)) in octaves.iter().enumerate() {
        let cell = u32::try_from(side as u64 * u64::from(per_mille) / 1000)
            .unwrap_or(1)
            .max(2);
        let s = roll(seed, purpose, &[k as u64]);
        sum += (i64::from(noise(s, cell, x, y)) - 500) * weight;
        weights += weight;
    }
    sum / weights
}

/// The land and its relief once eroded: what the climate is laid on.
pub(crate) struct Relief {
    pub(crate) grid: Grid,
    pub(crate) sea: Vec<bool>,
    pub(crate) height: Vec<i32>,
    pub(crate) routing: Routing,
    pub(crate) area: Vec<u32>,
    pub(crate) uplift: Vec<i32>,
    pub(crate) exhumed: Vec<i32>,
    pub(crate) rock: Vec<Rock>,
    pub(crate) alluvium: Vec<bool>,
    pub(crate) lake: Vec<bool>,
    pub(crate) strokes: Vec<Stroke>,
}

impl Relief {
    /// Passes 1 to 4: outline, uplift, erosion, rock and lakes.
    pub(crate) fn birth(seed: u64, grid: Grid, strokes: Option<Vec<Stroke>>) -> Self {
        let (sea, landness, threshold) = outline(seed, grid);
        let strokes = strokes.unwrap_or_else(|| draw_strokes(seed, grid, &sea));
        let uplift = uplift(seed, grid, &sea, &strokes);
        let covers = Covers::new(seed, grid);
        let start: Vec<i32> = (0..grid.len())
            .map(|i| {
                let (x, y) = grid.xy(i);
                let above = i64::from(landness[i]) - threshold;
                if sea[i] {
                    clamp_i32((-above * SEA_DEPTH_PER_UNIT_CM).clamp(-SEA_DEEPEST_CM, -100))
                } else {
                    let rough = (fbm(
                        seed,
                        purpose::PLAN_ROUGH,
                        grid.side,
                        x,
                        y,
                        &[(20, 2), (6, 1)],
                    ) + 500)
                        * ROUGH_CM
                        / 1000;
                    clamp_i32(START_CM + rough + above * DOME_CM_PER_UNIT)
                }
            })
            .collect();
        let mut relief = Self {
            grid,
            sea,
            height: start.clone(),
            routing: Routing::new(grid.len()),
            area: vec![0; grid.len()],
            uplift,
            exhumed: vec![0; grid.len()],
            rock: vec![Rock::Granite; grid.len()],
            alluvium: vec![false; grid.len()],
            lake: vec![false; grid.len()],
            strokes,
        };
        relief.erode(&start, &covers);
        relief.keep_small_lakes(seed);
        relief.drain_lakes(seed);
        relief.settle_rock(&start, &covers);
        relief
    }

    /// Pass 4, first half: the small lakes the erosion's cells smooth
    /// away, dug back where nature makes them (see [`SCATTERED_LAKES`]).
    /// Each is a hollow below the lowest ground around it, so the flood
    /// fills it to that rim and its outlet spills there.
    fn keep_small_lakes(&mut self, seed: u64) {
        let grid = self.grid;
        let north = super::climate::north_latitude(seed);
        for i in 0..grid.len() {
            if self.sea[i] {
                continue;
            }
            let (x, y) = grid.xy(i);
            let ground = super::climate::ground_tenths(north, y, self.height[i]);
            let cold = (GLACIAL_TENTHS - ground).clamp(0, GLACIAL_FULL_TENTHS);
            let density = SCATTERED_LAKES
                + GLACIAL_LAKES * cold.unsigned_abs() / GLACIAL_FULL_TENTHS.unsigned_abs();
            if roll(seed, purpose::PLAN_LAKES, &[i as u64]) % 100_000 >= density {
                continue;
            }
            // A single cell, a cross of five, or a square of nine.
            let shape = roll(seed, purpose::PLAN_LAKES, &[i as u64, 1]) % 3;
            let depth = i64::try_from(
                SMALL_LAKE_DEPTH_CM.0
                    + roll(seed, purpose::PLAN_LAKES, &[i as u64, 2])
                        % (SMALL_LAKE_DEPTH_CM.1 - SMALL_LAKE_DEPTH_CM.0 + 1),
            )
            .expect("small");
            let inside = |dx: i32, dy: i32| match shape {
                0 => dx == 0 && dy == 0,
                1 => dx.abs() + dy.abs() <= 1,
                _ => true,
            };
            let mut rim = i64::MAX;
            let mut near_sea = false;
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let (nx, ny) = (x + dx, y + dy);
                    if !grid.inside(nx, ny) {
                        near_sea = true;
                        continue;
                    }
                    let j = grid.index(nx, ny);
                    near_sea |= self.sea[j];
                    if !(dx.abs() <= 1 && dy.abs() <= 1 && inside(dx, dy)) {
                        rim = rim.min(i64::from(self.height[j]));
                    }
                }
            }
            if near_sea || rim == i64::MAX {
                continue;
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if inside(dx, dy) {
                        let j = grid.index(x + dx, y + dy);
                        let floor = rim - if (dx, dy) == (0, 0) { depth } else { depth / 2 };
                        self.height[j] = self.height[j].min(clamp_i32(floor));
                    }
                }
            }
        }
        self.routing.route(grid, &self.height, &self.sea);
    }

    /// Pass 4, second half: each lake keeps a size drawn for it
    /// ([`LAKE_SHARES`]): its deepest cells, as many as that, stay under
    /// water; the rest of its basin is filled by the rivers' silt up to
    /// the lake's level, a flat plain around a smaller lake (the Po's, the
    /// Hungarian plain). A lake whose basin sank below the sea becomes a
    /// plain at sea level the same way.
    fn drain_lakes(&mut self, seed: u64) {
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;
        let grid = self.grid;
        let n = grid.len();
        let lake = |r: &Self, i: usize| {
            !r.sea[i] && r.routing.filled[i] - r.height[i] >= LAKE_MIN_DEPTH_CM
        };
        let mut done = vec![false; n];
        let mut kept = vec![false; n];
        let mut changed = false;
        for start in 0..n {
            if done[start] || !lake(self, start) {
                continue;
            }
            // One lake: its cells, all at one level.
            let level = self.routing.filled[start];
            let mut cells = vec![start];
            done[start] = true;
            let mut k = 0;
            while k < cells.len() {
                let c = cells[k];
                k += 1;
                for (m, _) in grid.around(c) {
                    if !done[m] && lake(self, m) && self.routing.filled[m] == level {
                        done[m] = true;
                        cells.push(m);
                    }
                }
            }
            let deepest = *cells
                .iter()
                .min_by_key(|&&c| (self.height[c], c))
                .expect("a cell");
            let draw =
                i64::try_from(roll(seed, purpose::PLAN_LAKES, &[0xd4, deepest as u64]) % 1001)
                    .expect("small");
            let share = usize::try_from(super::table(&LAKE_SHARES, draw)).expect("positive");
            let size = (cells.len() * share / 1000).clamp(LAKE_LEAST_CELLS, LAKE_MOST_CELLS);
            if cells.len() <= size {
                continue;
            }
            // The deepest `size` cells joined to the deepest one, grown
            // lowest bed first (ties by index: a fixed order).
            let mut heap = BinaryHeap::new();
            heap.push(Reverse((self.height[deepest], deepest)));
            kept[deepest] = true;
            let mut count = 0;
            while let Some(Reverse((_, c))) = heap.pop() {
                count += 1;
                if count >= size {
                    break;
                }
                for (m, _) in grid.around(c) {
                    if !kept[m] && lake(self, m) && self.routing.filled[m] == level {
                        kept[m] = true;
                        heap.push(Reverse((self.height[m], m)));
                    }
                }
            }
            // Whatever was pushed but not reached stays dry too.
            for Reverse((_, c)) in heap {
                kept[c] = false;
            }
            for &c in &cells {
                if !kept[c] {
                    self.height[c] = level;
                }
            }
            changed = true;
        }
        if changed {
            self.routing.route(grid, &self.height, &self.sea);
        }
        self.accumulate();
    }

    /// Pass 2: the stream-power law, implicit (Braun and Willett 2013).
    ///
    /// Each pass routes the water (a priority flood: every cell then
    /// drains to the sea, through the hollows it filled), adds up the
    /// area each cell drains, and walks the cells from the sea upward,
    /// so each cell's receiver is already settled:
    /// `h = (h + u + F h_r) / (1 + F)`, `F = K √A / distance`.
    fn erode(&mut self, start: &[i32], covers: &Covers) {
        let n = self.grid.len();
        let roots: Vec<i64> = (0..65_536_u64)
            .map(|a| i64::try_from(isqrt(a * 1_000_000)).expect("small"))
            .collect();
        let root = |a: u32| -> i64 {
            roots
                .get(a as usize)
                .copied()
                .unwrap_or_else(|| i64::try_from(isqrt(u64::from(a) * 1_000_000)).expect("small"))
        };
        for pass in 0..PASSES {
            self.routing.route(self.grid, &self.height, &self.sea);
            self.accumulate();
            for k in 0..n {
                let c = self.routing.order[k] as usize;
                if self.sea[c] {
                    continue;
                }
                let r = self.routing.receiver[c] as usize;
                let hr = if self.sea[r] {
                    0
                } else {
                    i64::from(self.height[r])
                };
                let top = i64::from(start[c]) + i64::from(pass) * i64::from(self.uplift[c]);
                let layer = covers.layer(c, top - i64::from(self.height[c]));
                let mut f = CUTTING_PER_MILLE * layer.cutting() / 1000 * root(self.area[c]) / 1000;
                if is_diagonal(self.grid, c, r) {
                    f = f * 1000 / DIAGONAL_PER_MILLE;
                }
                let h = i64::from(self.height[c]) + i64::from(self.uplift[c]);
                if h <= hr {
                    f = f * FILLING_PER_MILLE / 1000;
                }
                self.height[c] = clamp_i32((1000 * h + f * hr).div_euclid(1000 + f));
            }
        }
        self.routing.route(self.grid, &self.height, &self.sea);
        self.accumulate();
    }

    /// The area each cell drains, in cells, down the routing.
    fn accumulate(&mut self) {
        for (i, a) in self.area.iter_mut().enumerate() {
            *a = u32::from(!self.sea[i]);
        }
        for &c in self.routing.order.iter().rev() {
            let c = c as usize;
            let r = self.routing.receiver[c] as usize;
            if r != c && !self.sea[c] {
                self.area[r] += self.area[c];
            }
        }
    }

    /// Passes 3 and 4: the rock the erosion uncovered, the alluvium of the
    /// floodplains, and the lakes.
    fn settle_rock(&mut self, start: &[i32], covers: &Covers) {
        for c in 0..self.grid.len() {
            if self.sea[c] {
                continue;
            }
            let top = i64::from(start[c]) + i64::from(PASSES) * i64::from(self.uplift[c]);
            let cut = top - i64::from(self.height[c]);
            self.exhumed[c] = clamp_i32(cut);
            let layer = covers.layer(c, cut.max(0));
            self.rock[c] = match layer {
                Layer::Chalk => Rock::Chalk,
                Layer::Limestone => Rock::Limestone,
                Layer::Sandstone => Rock::Sandstone,
                Layer::Granite | Layer::Sediment => Rock::Granite,
            };
            let r = self.routing.receiver[c] as usize;
            let drop = self.routing.filled[c] - self.routing.filled[r];
            self.alluvium[c] =
                cut < 0 || (self.area[c] >= ALLUVIUM_AREA && drop <= ALLUVIUM_DROP_CM);
            self.lake[c] = self.routing.filled[c] - self.height[c] >= LAKE_MIN_DEPTH_CM;
        }
    }
}

/// Whether the step from `a` to `b` is a diagonal.
fn is_diagonal(grid: Grid, a: usize, b: usize) -> bool {
    let ((ax, ay), (bx, by)) = (grid.xy(a), grid.xy(b));
    ax != bx && ay != by
}

fn clamp_i32(v: i64) -> i32 {
    i32::try_from(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).expect("clamped")
}

/// Pass 1: which cells are sea, the outline's noise, and the noise's value
/// at the coast. The coast is cut where [`LAND_PER_MILLE`] of the grid
/// lies above it, by a histogram, so every seed has the same share of
/// land.
fn outline(seed: u64, grid: Grid) -> (Vec<bool>, Vec<i32>, i64) {
    let side = grid.side;
    let s = i64::try_from(side).expect("small");
    let jitter = |k: u64| {
        let r =
            i64::try_from(roll(seed, purpose::PLAN_LAND, &[0xce, k]) % 2001).expect("small") - 1000;
        s / 2 + r * s * CENTRE_JITTER_PER_MILLE / 1_000_000
    };
    let (cx, cy) = (jitter(0), jitter(1));
    let mut landness = vec![0_i32; grid.len()];
    for (i, value) in landness.iter_mut().enumerate() {
        let (x, y) = grid.xy(i);
        let warp =
            |p: u64| fbm(seed, p, side, x, y, &[(150, 2), (60, 1)]) * s * WARP_PER_MILLE / 500_000;
        let (wx, wy) = (warp(purpose::PLAN_WARP_X), warp(purpose::PLAN_WARP_Y));
        let (px, py) = (i64::from(x) + wx, i64::from(y) + wy);
        let n = fbm(
            seed,
            purpose::PLAN_LAND,
            side,
            i32::try_from(px).expect("small"),
            i32::try_from(py).expect("small"),
            &OUTLINE_OCTAVES,
        );
        // Square distance from the middle, per mille of the half-side, squared.
        let (dx, dy) = ((px - cx) * 2000 / s, (py - cy) * 2000 / s);
        let falloff = FALLOFF * (dx * dx + dy * dy) / 2_000_000;
        // Past the edge band, the land drowns whatever the noise says.
        let edge = (i64::from(x)
            .min(i64::from(y))
            .min(s - 1 - i64::from(x.max(y))))
            * 1000
            / s;
        let drown = (EDGE_BAND_PER_MILLE - edge).max(0) * EDGE_DROWN / EDGE_BAND_PER_MILLE;
        *value = clamp_i32(n * OUTLINE_GAIN - falloff - drown);
    }
    let (lo, hi) = (
        i64::from(*landness.iter().min().expect("cells")),
        i64::from(*landness.iter().max().expect("cells")),
    );
    let mut counts = vec![0_u64; usize::try_from(hi - lo + 1).expect("small")];
    for &v in &landness {
        counts[usize::try_from(i64::from(v) - lo).expect("in range")] += 1;
    }
    let wanted = grid.len() as u64 * u64::from(LAND_PER_MILLE) / 1000;
    let mut above = 0;
    let mut threshold = hi;
    for (k, &count) in counts.iter().enumerate().rev() {
        above += count;
        threshold = lo + i64::try_from(k).expect("small");
        if above >= wanted {
            break;
        }
    }
    let sea = (0..grid.len())
        .map(|i| {
            let (x, y) = grid.xy(i);
            let edge = x
                .min(y)
                .min(i32::try_from(side).expect("small") - 1 - x.max(y));
            edge < EDGE_SEA_CELLS || i64::from(landness[i]) < threshold
        })
        .collect();
    (sea, landness, threshold)
}

/// A draw in `lo..=hi` for `seed`, the strokes' purpose and `keys`.
fn draw(seed: u64, keys: &[u64], (lo, hi): (u64, u64)) -> u64 {
    lo + roll(seed, purpose::PLAN_STROKES, keys) % (hi - lo + 1)
}

/// The strokes the generator draws when the lead gives none: ranges,
/// plateaus and basins, each started on land.
fn draw_strokes(seed: u64, grid: Grid, sea: &[bool]) -> Vec<Stroke> {
    let side = grid.side as u64;
    let land_cell = |keys: &[u64]| -> (i64, i64) {
        for attempt in 0..256 {
            let mut k = keys.to_vec();
            k.push(attempt);
            let x = roll(seed, purpose::PLAN_STROKES, &k) % side;
            k.push(0x79);
            let y = roll(seed, purpose::PLAN_STROKES, &k) % side;
            let (x, y) = (
                i64::try_from(x).expect("small"),
                i64::try_from(y).expect("small"),
            );
            if !sea[grid.index(
                i32::try_from(x).expect("small"),
                i32::try_from(y).expect("small"),
            )] {
                return (x, y);
            }
        }
        let half = i64::try_from(side / 2).expect("small");
        (half, half)
    };
    let to_cells = |per_mille: u64| {
        i32::try_from(side * per_mille / 1000)
            .expect("small")
            .max(1)
    };
    let point = |x: i64, y: i64| {
        (
            i32::try_from(x).expect("small"),
            i32::try_from(y).expect("small"),
        )
    };
    let mut strokes = Vec::new();
    for r in 0..draw(seed, &[1], RANGES) {
        let (mut x, mut y) = land_cell(&[1, r]);
        let mut heading = draw(seed, &[1, r, 1], (0, 15));
        let mut points = vec![point(x, y)];
        for v in 0..draw(seed, &[1, r, 2], RANGE_VERTICES) {
            let length =
                i64::try_from(side * draw(seed, &[1, r, 3, v], RANGE_SEGMENT_PER_MILLE) / 1000)
                    .expect("small");
            let (cos, sin) = HEADINGS[usize::try_from(heading).expect("small")];
            x += cos * length / 1000;
            y += sin * length / 1000;
            points.push(point(x, y));
            heading = (heading + 15 + draw(seed, &[1, r, 4, v], (0, 2))) % 16;
        }
        strokes.push(Stroke {
            kind: StrokeKind::Range,
            points,
            half_width: to_cells(draw(seed, &[1, r, 5], RANGE_HALF_WIDTH_PER_MILLE)),
            strength: i32::try_from(draw(seed, &[1, r, 6], RANGE_UPLIFT_CM)).expect("small"),
        });
    }
    for p in 0..draw(seed, &[2], PLATEAUS) {
        let (x, y) = land_cell(&[2, p]);
        strokes.push(Stroke {
            kind: StrokeKind::Plateau,
            points: vec![point(x, y)],
            half_width: to_cells(draw(seed, &[2, p, 1], PLATEAU_HALF_WIDTH_PER_MILLE)),
            strength: i32::try_from(draw(seed, &[2, p, 2], PLATEAU_UPLIFT_CM)).expect("small"),
        });
    }
    for b in 0..draw(seed, &[3], BASINS) {
        let (x, y) = land_cell(&[3, b]);
        strokes.push(Stroke {
            kind: StrokeKind::Basin,
            points: vec![point(x, y)],
            half_width: to_cells(draw(seed, &[3, b, 1], BASIN_HALF_WIDTH_PER_MILLE)),
            strength: -i32::try_from(draw(seed, &[3, b, 2], BASIN_SINK_CM)).expect("small"),
        });
    }
    strokes
}

/// The distance from `(qx, qy)` to the segment `a`–`b`, all in
/// sixteenths of a cell.
fn segment_distance(qx: i64, qy: i64, a: (i64, i64), b: (i64, i64)) -> i64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let (cx, cy) = if len2 == 0 {
        a
    } else {
        let t = ((qx - a.0) * dx + (qy - a.1) * dy).clamp(0, len2);
        (a.0 + dx * t / len2, a.1 + dy * t / len2)
    };
    let (ex, ey) = (qx - cx, qy - cy);
    i64::try_from(isqrt(u64::try_from(ex * ex + ey * ey).expect("square"))).expect("small")
}

/// Pass 1 (continued): the uplift, a broad swell plus the strokes.
fn uplift(seed: u64, grid: Grid, sea: &[bool], strokes: &[Stroke]) -> Vec<i32> {
    let side = grid.side;
    let mut uplift: Vec<i64> = (0..grid.len())
        .map(|i| {
            let (x, y) = grid.xy(i);
            let n = fbm(seed, purpose::PLAN_SWELL, side, x, y, &SWELL_OCTAVES) + 500;
            SWELL_MIN_CM + n * (SWELL_MAX_CM - SWELL_MIN_CM) / 1000
        })
        .collect();
    let peaks_cell = u32::try_from(side as u64 * u64::from(PEAKS_CELL_PER_MILLE) / 1000)
        .expect("small")
        .max(2);
    for (k, stroke) in strokes.iter().enumerate() {
        let w = i64::from(stroke.half_width.max(1));
        let pts: Vec<(i64, i64)> = stroke
            .points
            .iter()
            .map(|&(x, y)| (i64::from(x) * 16 + 8, i64::from(y) * 16 + 8))
            .collect();
        let (min_x, max_x) = stroke
            .points
            .iter()
            .fold((i32::MAX, i32::MIN), |(lo, hi), p| {
                (lo.min(p.0), hi.max(p.0))
            });
        let (min_y, max_y) = stroke
            .points
            .iter()
            .fold((i32::MAX, i32::MIN), |(lo, hi), p| {
                (lo.min(p.1), hi.max(p.1))
            });
        let reach = stroke.half_width + 1;
        let peaks_seed = roll(seed, purpose::PLAN_PEAKS, &[k as u64]);
        for y in (min_y - reach)..=(max_y + reach) {
            for x in (min_x - reach)..=(max_x + reach) {
                if !grid.inside(x, y) {
                    continue;
                }
                let (qx, qy) = (i64::from(x) * 16 + 8, i64::from(y) * 16 + 8);
                let d = if pts.len() == 1 {
                    segment_distance(qx, qy, pts[0], pts[0])
                } else {
                    pts.windows(2)
                        .map(|s| segment_distance(qx, qy, s[0], s[1]))
                        .min()
                        .expect("a segment")
                };
                if d >= w * 16 {
                    continue;
                }
                // 1000 on the line, 0 at the half-width.
                let t = (w * 16 - d) * 1000 / (w * 16);
                let value = match stroke.kind {
                    StrokeKind::Range => {
                        let n = i64::from(noise(peaks_seed, peaks_cell, x, y));
                        let crest =
                            PEAKS_PER_MILLE.0 + n * (PEAKS_PER_MILLE.1 - PEAKS_PER_MILLE.0) / 1000;
                        i64::from(stroke.strength) * (t * t * (3000 - 2 * t) / 1_000_000) / 1000
                            * crest
                            / 1000
                    }
                    StrokeKind::Plateau | StrokeKind::Basin => {
                        let s = (2 * t).min(1000);
                        i64::from(stroke.strength) * (s * s * (3000 - 2 * s) / 1_000_000) / 1000
                    }
                };
                uplift[grid.index(x, y)] += value;
            }
        }
    }
    uplift
        .iter()
        .zip(sea)
        .map(|(&u, &s)| {
            if s {
                0
            } else if u > UPLIFT_KNEE_CM {
                clamp_i32(UPLIFT_KNEE_CM + (u - UPLIFT_KNEE_CM) * UPLIFT_PAST_KNEE_PER_MILLE / 1000)
            } else {
                clamp_i32(u)
            }
        })
        .collect()
}

/// The three covers over the granite, thickness per cell, in cm.
struct Covers {
    chalk: Vec<i32>,
    limestone: Vec<i32>,
    sandstone: Vec<i32>,
    fold: Vec<i32>,
}

impl Covers {
    fn new(seed: u64, grid: Grid) -> Self {
        let cover = |p: u64, thickest: i64, (from, full): (i64, i64)| -> Vec<i32> {
            (0..grid.len())
                .map(|i| {
                    let (x, y) = grid.xy(i);
                    let n = fbm(seed, p, grid.side, x, y, &COVER_OCTAVES);
                    clamp_i32(thickest * ((n - from) * 1000 / (full - from)).clamp(0, 1000) / 1000)
                })
                .collect()
        };
        Self {
            chalk: cover(purpose::PLAN_CHALK, CHALK_CM, CHALK_SPREAD),
            limestone: cover(purpose::PLAN_LIMESTONE, LIMESTONE_CM, LIMESTONE_SPREAD),
            sandstone: cover(purpose::PLAN_SANDSTONE, SANDSTONE_CM, SANDSTONE_SPREAD),
            fold: (0..grid.len())
                .map(|i| {
                    let (x, y) = grid.xy(i);
                    let n = fbm(seed, purpose::PLAN_FOLDS, grid.side, x, y, &FOLD_OCTAVES);
                    clamp_i32(n * FOLD_CM / 250)
                })
                .collect(),
        }
    }

    /// The layer at `depth` cm below the first ground of cell `c`.
    fn layer(&self, c: usize, depth: i64) -> Layer {
        if depth < 0 {
            return Layer::Sediment;
        }
        let depth = (depth + i64::from(self.fold[c])).max(0);
        let chalk = i64::from(self.chalk[c]);
        let limestone = chalk + i64::from(self.limestone[c]);
        let sandstone = limestone + i64::from(self.sandstone[c]);
        if depth < chalk {
            Layer::Chalk
        } else if depth < limestone {
            Layer::Limestone
        } else if depth < sandstone {
            Layer::Sandstone
        } else {
            Layer::Granite
        }
    }
}

// ---------------------------------------------------------------- routing

const NONE: u32 = u32::MAX;

/// The water's way over a grid: a priority flood from the sea (Barnes,
/// Lehman and Mulla 2014) that fills every hollow to its outlet, then
/// each cell sent down its steepest slope on the filled surface.
///
/// The flood's queue is a bucket per centimetre of height, each a
/// first-in first-out list: the flood only ever pops a height at least
/// the last one, so a bucket queue is exact, linear, and its ties come
/// out in a fixed order (D24: never a heap's).
pub(crate) struct Routing {
    /// The water surface: the ground, or the level a hollow is filled to;
    /// 0 at sea.
    pub(crate) filled: Vec<i32>,
    /// Each cell's receiver; a sea cell is its own.
    pub(crate) receiver: Vec<u32>,
    /// The cells in the order the flood reached them: each after its
    /// receiver.
    pub(crate) order: Vec<u32>,
    seen: Vec<bool>,
    next: Vec<u32>,
    head: Vec<u32>,
    tail: Vec<u32>,
}

impl Routing {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            filled: vec![0; n],
            receiver: vec![0; n],
            order: Vec::with_capacity(n),
            seen: vec![false; n],
            next: vec![NONE; n],
            head: Vec::new(),
            tail: Vec::new(),
        }
    }

    fn push(&mut self, cell: u32, level: usize) {
        self.next[cell as usize] = NONE;
        if self.tail[level] == NONE {
            self.head[level] = cell;
        } else {
            self.next[self.tail[level] as usize] = cell;
        }
        self.tail[level] = cell;
    }

    /// Routes the water over `height`, the cells in `sea` being the
    /// outlets at level 0. Every other cell must be reachable from them.
    pub(crate) fn route(&mut self, grid: Grid, height: &[i32], sea: &[bool]) {
        self.route_from(grid, height, sea, sea);
    }

    /// Routes the water over `height` to the cells in `outlet`, each at
    /// its own height (at least 0), or at 0 if it is in `sea`.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    pub(crate) fn route_from(&mut self, grid: Grid, height: &[i32], outlet: &[bool], sea: &[bool]) {
        let n = grid.len();
        let top = height.iter().copied().max().unwrap_or(0).max(0) as usize;
        self.head.clear();
        self.head.resize(top + 1, NONE);
        self.tail.clear();
        self.tail.resize(top + 1, NONE);
        self.order.clear();
        for i in 0..n {
            self.seen[i] = outlet[i];
            if outlet[i] {
                let level = if sea[i] { 0 } else { height[i].max(0) };
                self.filled[i] = level;
                self.receiver[i] = i as u32;
                self.push(i as u32, level as usize);
            }
        }
        let mut level = 0;
        loop {
            while level <= top && self.head[level] == NONE {
                level += 1;
            }
            if level > top {
                break;
            }
            let c = self.head[level];
            self.head[level] = self.next[c as usize];
            if self.head[level] == NONE {
                self.tail[level] = NONE;
            }
            self.order.push(c);
            for (m, _) in grid.around(c as usize) {
                if !self.seen[m] {
                    self.seen[m] = true;
                    let f = height[m].max(level as i32);
                    self.filled[m] = f;
                    self.receiver[m] = c;
                    self.push(m as u32, f as usize);
                }
            }
        }
        debug_assert_eq!(self.order.len(), n, "every cell reached from the sea");
        // Down the steepest slope where the filled surface falls; across a
        // filled flat, toward the outlet the flood came from.
        for c in 0..n {
            if outlet[c] {
                continue;
            }
            let fc = i64::from(self.filled[c]);
            let mut best: Option<(usize, i64, i64)> = None;
            for (m, diagonal) in grid.around(c) {
                let drop = fc - i64::from(self.filled[m]);
                if drop <= 0 {
                    continue;
                }
                let distance = if diagonal { DIAGONAL_PER_MILLE } else { 1000 };
                if best.is_none_or(|(_, d, l)| drop * l > d * distance) {
                    best = Some((m, drop, distance));
                }
            }
            if let Some((m, _, _)) = best {
                self.receiver[c] = m as u32;
            }
        }
    }
}

/// Metres of a step from cell `a` to its neighbour `b`, in metres.
pub(crate) fn step_m(grid: Grid, a: usize, b: usize) -> i64 {
    if is_diagonal(grid, a, b) {
        CELL_M * DIAGONAL_PER_MILLE / 1000
    } else {
        CELL_M
    }
}
