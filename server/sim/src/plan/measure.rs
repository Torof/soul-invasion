//! What the project lead judges a seed by (`premier-pas.md` §2): the
//! water on the land, the land one can walk across, the longest river
//! with no natural crossing and the walk across a big river, the
//! biomes' and the rocks' shares, the lakes, springs and water table.
//!
//! A river's width and depth come from its discharge by the average
//! hydraulic geometry of rivers (Leopold and Maddock 1953): width about
//! `4 √Q` metres, depth about `0.3 Q^0.4` metres, `Q` in cubic metres a
//! second. This is not yet D36's section (its depth grows faster); the
//! two are to be reconciled when D36 is amended for a generated world.
//!
//! A river can be crossed in a 2 km cell where its shallowest point is
//! shallow enough. Its shallowest point is a riffle: a natural river
//! alternates pools and riffles every five to seven widths (Leopold,
//! Wolman and Miller 1964), several to a cell, and is shallower on them;
//! shallower still where it spreads wider (the continent's spread: a
//! bar, a braided reach, a lake's sill, a cell's widest riffle). It is
//! measured at the mean discharge (all year) and at low water, late in a
//! dry season; and so is the walk from one bank of a big river to the
//! facing one (the detour).

use std::collections::VecDeque;

use super::relief::step_m;
use super::{Biome, CELL_M, Continent, Grid, RIVER_ML, iroot, isqrt};
use crate::geology::Rock;

/// Depth on a riffle, per mille of the mean depth.
pub const RIFFLE_PER_MILLE: u64 = 500;

/// A reach that spreads `k` times wider than its usual width keeps its
/// discharge at a depth of `k^-0.6` (Manning's law: `Q ∝ w d^(5/3)`):
/// the continent's [`Continent::spread`] (bars, braids, a lake's sill,
/// the widest of a cell's riffles). The water's width over such a reach
/// grows by half the spread: islands and bars stand out of it.
fn spread_depth_per_mille(spread: u16) -> u64 {
    let s = u128::from(spread.max(1000));
    1_000_000 / iroot(s * s * s * 1_000_000, 5).max(1)
}

/// Rivers wind: a river's length over its cells' steps, per mille (a
/// plain's river runs about one and a half times the straight line).
pub const SINUOSITY_PER_MILLE: u64 = 1_300;

/// Depths a walker crosses: dry-shod enough (half a metre) and wading (a
/// metre), D36's two thresholds, in cm.
pub const DRY_CM: u64 = 50;
/// Wading depth, in cm.
pub const WADE_CM: u64 = 100;

/// A river's width, in cm, for a discharge in millilitres a second.
pub fn width_cm(discharge: u64) -> u64 {
    isqrt(discharge.saturating_mul(16)) / 10
}

/// A river's mean depth, in cm, for a discharge in millilitres a second.
pub fn depth_cm(discharge: u64) -> u64 {
    let q = u128::from(discharge);
    iroot(q * q, 5) * 11_943 / 100_000
}

/// The numbers of one continent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Numbers {
    /// Land (all but the sea), in km².
    pub land_km2: u64,
    /// Rivers' water over the land, in hundredths of a percent.
    pub rivers_bp: u64,
    /// Lakes over the land, in hundredths of a percent.
    pub lakes_bp: u64,
    /// The largest piece of land one walks across dry-shod (water no
    /// deeper than half a metre), over the walkable land, in hundredths
    /// of a percent.
    pub dry_piece_bp: u64,
    /// The same, wading up to a metre.
    pub wading_piece_bp: u64,
    /// The longest stretch of river with no crossing dry-shod, in km.
    pub longest_dry_km: u64,
    /// The longest stretch of river with no crossing wading, in km.
    pub longest_wading_km: u64,
    /// Each land biome's share of the land outside lakes, in hundredths
    /// of a percent, in [`Biome::LAND`] order.
    pub biomes_bp: Vec<(Biome, u64)>,
    /// The highest ground, in metres.
    pub highest_m: i64,
    /// The largest river's discharge at its mouth, in m³/s.
    pub largest_river_m3s: u64,
    /// Each bedrock's share of the land outside lakes, in hundredths of a
    /// percent, in [`Rock::ALL`] order (under the alluvium too).
    pub rocks_bp: Vec<(Rock, u64)>,
    /// The alluvium's share of the land outside lakes, in hundredths of a
    /// percent.
    pub alluvium_bp: u64,
    /// How many bedrocks a block of [`BLOCK_CELLS`] cells a side shows on
    /// average (each on at least [`BLOCK_ROCK_PER_MILLE`] of its land),
    /// times 100: "a bit of everything everywhere" is near 400.
    pub rocks_a_block_centi: u64,
    /// The walk from one bank of a big river to the facing bank, wading,
    /// in km: the median over the big rivers' cells.
    pub detour_median_km: u64,
    /// The same, its ninth decile.
    pub detour_p90_km: u64,
    /// The same, its longest (at most [`DETOUR_CAP_KM`]).
    pub detour_max_km: u64,
    /// The longest stretch of river with no crossing wading at low water,
    /// late in a dry season, in km.
    pub longest_low_km: u64,
    /// The walk between facing banks of the big rivers wading at low
    /// water: median, ninth decile and longest, in km.
    pub detour_low_median_km: u64,
    /// Its ninth decile.
    pub detour_low_p90_km: u64,
    /// Its longest.
    pub detour_low_max_km: u64,
    /// How many lakes (one or more cells together), and the largest's
    /// area in km².
    pub lakes: u64,
    /// The largest lake, in km².
    pub largest_lake_km2: u64,
    /// Springs, per 10,000 km² of land.
    pub springs_10k_km2: u64,
    /// The water table's median depth under the land outside water, in
    /// metres.
    pub table_median_m: u64,
    /// The share of the land whose water table is within 10 m (a well
    /// dug by hand reaches it), in hundredths of a percent.
    pub table_near_bp: u64,
    /// The land outside lakes by its yearly mean, in hundredths of a
    /// percent: under 0 °C, 0 to 15, 15 to 25, 25 to 30, 30 and over
    /// ([`WARMTH_BANDS_TENTHS`]).
    pub warmth_bp: [u64; 5],
    /// The coldest yearly mean on the land, in tenths of a degree.
    pub coldest_tenths: i64,
    /// How many watercourses the atlas shows: the stems of at least
    /// [`RIVER_ML`], a stem starting where its largest inflow is under it.
    pub rivers: u64,
    /// How many of them reach the sea with at least [`GREAT_RIVER_M3S`].
    pub great_rivers: u64,
    /// The three biggest rivers by their flow at the sea: `(m³/s, km)`,
    /// the length being the stem's, with its windings, from the sea up
    /// to where it falls under [`RIVER_ML`].
    pub biggest: Vec<(u64, u64)>,
}

/// The bands of the land's yearly mean, in tenths of a degree: the
/// project lead's targets of 14 November 2026 (most of the land between
/// 15 and 25 °C, some at 25–30, little at 0, nothing far below).
pub const WARMTH_BANDS_TENTHS: [i64; 4] = [0, 150, 250, 300];

/// A great river reaches the sea with at least this (m³/s): the project
/// lead wants two to four a continent.
pub const GREAT_RIVER_M3S: u64 = 500;

/// A block of the continent for the rocks' variety: 32 cells, 65.5 km a
/// side, about what a tribe roams in a generation.
pub const BLOCK_CELLS: usize = 32;
/// A rock counts in a block when it shows on this much of its land, per
/// mille.
pub const BLOCK_ROCK_PER_MILLE: u64 = 20;

/// Rivers carrying at least this (m³/s) are the big rivers the detour is
/// measured on: about 40 m wide and 2 m deep on average.
pub const DETOUR_RIVER_M3S: u64 = 100;
/// The detour's search stops this far (km): longer counts as this.
pub const DETOUR_CAP_KM: u64 = 300;
/// One big-river cell in this many is measured (a fixed choice by the
/// cell's index), so the search stays quick on rivers with no crossing.
pub const DETOUR_SAMPLE: usize = 3;

/// What a cell is for a walker who crosses water up to some depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passage {
    /// The sea.
    Sea,
    /// A lake: walked around.
    Lake,
    /// A river too deep to cross here.
    Barrier,
    /// A river of the atlas (at least [`RIVER_ML`]), crossed here.
    Crossing,
    /// Land in the largest piece.
    Main,
    /// Land cut off from the largest piece.
    Cut,
}

/// The shallowest water across each cell's river, in cm (0 at sea and in
/// lakes, which are not crossed but walked around).
fn shallowest(c: &Continent, low: bool) -> Vec<u64> {
    (0..c.grid.len())
        .map(|i| {
            if c.sea[i] || c.lake[i] {
                return 0;
            }
            let q = if low {
                c.low_discharge[i]
            } else {
                c.discharge[i]
            };
            depth_cm(q) * RIFFLE_PER_MILLE / 1000 * spread_depth_per_mille(c.spread[i]) / 1000
        })
        .collect()
}

/// Each cell's passage for a walker crossing up to `limit_cm`, at the
/// mean discharge (all year).
pub fn passages(c: &Continent, limit_cm: u64) -> Vec<Passage> {
    passages_at(c, limit_cm, false)
}

/// The same at low water, late in a dry season.
pub fn passages_low(c: &Continent, limit_cm: u64) -> Vec<Passage> {
    passages_at(c, limit_cm, true)
}

fn passages_at(c: &Continent, limit_cm: u64, low_water: bool) -> Vec<Passage> {
    let grid = c.grid;
    let n = grid.len();
    let low = shallowest(c, low_water);
    let mut out: Vec<Passage> = (0..n)
        .map(|i| {
            if c.sea[i] {
                Passage::Sea
            } else if c.lake[i] {
                Passage::Lake
            } else if low[i] > limit_cm {
                Passage::Barrier
            } else if c.discharge[i] >= RIVER_ML {
                Passage::Crossing
            } else {
                Passage::Cut
            }
        })
        .collect();
    // The largest piece, four ways: a diagonal river blocks a walker.
    let walkable = |p: Passage| matches!(p, Passage::Crossing | Passage::Cut);
    let mut piece = vec![u32::MAX; n];
    let mut sizes = Vec::new();
    let mut queue = VecDeque::new();
    for start in 0..n {
        if piece[start] != u32::MAX || !walkable(out[start]) {
            continue;
        }
        let id = u32::try_from(sizes.len()).expect("pieces");
        let mut size = 0_u64;
        piece[start] = id;
        queue.push_back(start);
        while let Some(i) = queue.pop_front() {
            size += 1;
            let (x, y) = grid.xy(i);
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if grid.inside(nx, ny) {
                    let j = grid.index(nx, ny);
                    if piece[j] == u32::MAX && walkable(out[j]) {
                        piece[j] = id;
                        queue.push_back(j);
                    }
                }
            }
        }
        sizes.push(size);
    }
    let main = sizes
        .iter()
        .enumerate()
        .max_by_key(|&(k, &s)| (s, std::cmp::Reverse(k)))
        .map(|(k, _)| k);
    for i in 0..n {
        if out[i] == Passage::Cut && main.is_some_and(|m| piece[i] as usize == m) {
            out[i] = Passage::Main;
        }
    }
    out
}

/// The share of the walkable land in the largest piece, and the longest
/// run of river with no crossing (in metres), for `limit_cm`.
fn walking(c: &Continent, limit_cm: u64, low_water: bool) -> (u64, u64) {
    let p = passages_at(c, limit_cm, low_water);
    let main = p.iter().filter(|&&q| q == Passage::Main).count() as u64;
    let walkable = p
        .iter()
        .filter(|&&q| matches!(q, Passage::Main | Passage::Cut | Passage::Crossing))
        .count() as u64;
    let mut run = vec![0_u64; c.grid.len()];
    let mut longest = 0;
    for &i in &c.order {
        let i = i as usize;
        if p[i] != Passage::Barrier {
            continue;
        }
        let r = c.receiver[i] as usize;
        let step = u64::try_from(step_m(c.grid, i, r)).expect("positive");
        run[i] = step + if p[r] == Passage::Barrier { run[r] } else { 0 };
        longest = longest.max(run[i]);
    }
    (main * 10_000 / walkable.max(1), longest)
}

/// Counts the numbers of a continent.
pub fn measure(c: &Continent) -> Numbers {
    let n = c.grid.len();
    let land = (0..n).filter(|&i| !c.sea[i]).count() as u64;
    let cell_m2 = u64::try_from(CELL_M * CELL_M).expect("positive");
    let mut river_m2 = 0_u64;
    let mut lakes = 0_u64;
    let mut counts = vec![0_u64; Biome::LAND.len()];
    let mut largest = 0;
    for i in 0..n {
        if c.sea[i] {
            continue;
        }
        if c.lake[i] {
            lakes += 1;
            continue;
        }
        let r = c.receiver[i] as usize;
        let step = u64::try_from(step_m(c.grid, i, r)).expect("positive");
        let wet = (1000 + u64::from(c.spread[i]).saturating_sub(1000) / 2).min(4_000);
        river_m2 += width_cm(c.discharge[i]) * wet / 1000 * step * SINUOSITY_PER_MILLE / 1000 / 100;
        if c.sea[r] {
            largest = largest.max(c.discharge[i]);
        }
        if let Some(k) = Biome::LAND.iter().position(|&b| b == c.biome[i]) {
            counts[k] += 1;
        }
    }
    let dry_land = land - lakes;
    let (dry_piece_bp, longest_dry) = walking(c, DRY_CM, false);
    let (wading_piece_bp, longest_wading) = walking(c, WADE_CM, false);
    let (_, longest_low) = walking(c, WADE_CM, true);
    let detour = detours(c, &passages(c, WADE_CM));
    let detour_low = detours(c, &passages_low(c, WADE_CM));
    let (warmth, coldest) = warmth(c);
    let (rivers, great_rivers, biggest) = river_stems(c);
    Numbers {
        land_km2: land * cell_m2 / 1_000_000,
        rivers_bp: river_m2 * 10_000 / (land * cell_m2).max(1),
        lakes_bp: lakes * 10_000 / land.max(1),
        dry_piece_bp,
        wading_piece_bp,
        longest_dry_km: longest_dry / 1000,
        longest_wading_km: longest_wading / 1000,
        biomes_bp: Biome::LAND
            .iter()
            .zip(&counts)
            .map(|(&b, &k)| (b, k * 10_000 / dry_land.max(1)))
            .collect(),
        highest_m: i64::from(c.height.iter().copied().max().unwrap_or(0)) / 100,
        largest_river_m3s: largest / 1_000_000,
        rocks_bp: Rock::ALL
            .iter()
            .map(|&r| {
                let k = (0..n)
                    .filter(|&i| !c.sea[i] && !c.lake[i] && c.rock[i] == r)
                    .count() as u64;
                (r, k * 10_000 / dry_land.max(1))
            })
            .collect(),
        alluvium_bp: (0..n)
            .filter(|&i| !c.sea[i] && !c.lake[i] && c.alluvium[i])
            .count() as u64
            * 10_000
            / dry_land.max(1),
        rocks_a_block_centi: rocks_a_block(c),
        detour_median_km: detour[0],
        detour_p90_km: detour[1],
        detour_max_km: detour[2],
        longest_low_km: longest_low / 1000,
        detour_low_median_km: detour_low[0],
        detour_low_p90_km: detour_low[1],
        detour_low_max_km: detour_low[2],
        lakes: lake_sizes(c).len() as u64,
        largest_lake_km2: lake_sizes(c).first().copied().unwrap_or(0) * cell_m2 / 1_000_000,
        springs_10k_km2: (0..n).filter(|&i| c.spring[i]).count() as u64 * 10_000_000_000
            / (land * cell_m2).max(1),
        table_median_m: {
            let mut t: Vec<i32> = (0..n)
                .filter(|&i| !c.sea[i] && !c.lake[i])
                .map(|i| c.water_table[i])
                .collect();
            t.sort_unstable();
            t.get(t.len() / 2)
                .map_or(0, |&v| u64::try_from(v).unwrap_or(0) / 100)
        },
        table_near_bp: (0..n)
            .filter(|&i| !c.sea[i] && !c.lake[i] && c.water_table[i] <= 1_000)
            .count() as u64
            * 10_000
            / dry_land.max(1),
        warmth_bp: warmth.map(|k| k * 10_000 / dry_land.max(1)),
        coldest_tenths: coldest,
        rivers,
        great_rivers,
        biggest,
    }
}

/// The land outside lakes in each band of [`WARMTH_BANDS_TENTHS`], in
/// cells, and the coldest yearly mean on it (tenths of a degree).
fn warmth(c: &Continent) -> ([u64; 5], i64) {
    let mut bands = [0_u64; 5];
    let mut coldest = i64::MAX;
    for i in 0..c.grid.len() {
        if c.sea[i] || c.lake[i] {
            continue;
        }
        let t = i64::from(c.temperature[i]);
        coldest = coldest.min(t);
        bands[WARMTH_BANDS_TENTHS.iter().filter(|&&b| t >= b).count()] += 1;
    }
    (bands, if coldest == i64::MAX { 0 } else { coldest })
}

/// The watercourses of the atlas: how many stems of at least
/// [`RIVER_ML`] there are, how many reach the sea as great rivers, and
/// the three biggest with their flow at the sea and their length.
fn river_stems(c: &Continent) -> (u64, u64, Vec<(u64, u64)>) {
    let n = c.grid.len();
    // Each cell's largest inflow.
    let mut main_in = vec![u32::MAX; n];
    for i in 0..n {
        if c.sea[i] {
            continue;
        }
        let r = c.receiver[i] as usize;
        if r == i || c.sea[r] {
            continue;
        }
        let m = main_in[r];
        if m == u32::MAX || c.discharge[i] > c.discharge[m as usize] {
            main_in[r] = u32::try_from(i).expect("small");
        }
    }
    let river = |i: usize| !c.sea[i] && !c.lake[i] && c.discharge[i] >= RIVER_ML;
    let mut stems = 0_u64;
    let mut mouths: Vec<(u64, usize)> = Vec::new();
    for i in 0..n {
        if !river(i) {
            continue;
        }
        let m = main_in[i];
        if m == u32::MAX || c.discharge[m as usize] < RIVER_ML {
            stems += 1;
        }
        if c.sea[c.receiver[i] as usize] {
            mouths.push((c.discharge[i], i));
        }
    }
    mouths.sort_unstable_by(|a, b| b.cmp(a));
    let great = mouths
        .iter()
        .filter(|&&(q, _)| q / 1_000_000 >= GREAT_RIVER_M3S)
        .count() as u64;
    let biggest = mouths
        .iter()
        .take(3)
        .map(|&(q, mouth)| {
            let mut length_m = 0_u64;
            let mut i = mouth;
            loop {
                let m = main_in[i];
                if m == u32::MAX || c.discharge[m as usize] < RIVER_ML {
                    break;
                }
                let m = m as usize;
                length_m += u64::try_from(step_m(c.grid, m, i)).expect("positive");
                i = m;
            }
            (q / 1_000_000, length_m * SINUOSITY_PER_MILLE / 1000 / 1000)
        })
        .collect();
    (stems, great, biggest)
}

/// The lakes' sizes in cells, largest first (eight ways together).
pub fn lake_sizes(c: &Continent) -> Vec<u64> {
    let n = c.grid.len();
    let mut seen = vec![false; n];
    let mut sizes = Vec::new();
    for start in 0..n {
        if seen[start] || !c.lake[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start];
        let mut size = 0;
        while let Some(i) = stack.pop() {
            size += 1;
            for (m, _) in c.grid.around(i) {
                if !seen[m] && c.lake[m] {
                    seen[m] = true;
                    stack.push(m);
                }
            }
        }
        sizes.push(size);
    }
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes
}

/// The mean number of bedrocks a block shows, times 100, over the blocks
/// that are at least a quarter land.
fn rocks_a_block(c: &Continent) -> u64 {
    let side = c.grid.side;
    let (mut blocks, mut kinds) = (0_u64, 0_u64);
    for by in (0..side).step_by(BLOCK_CELLS) {
        for bx in (0..side).step_by(BLOCK_CELLS) {
            let mut counts = [0_u64; 4];
            let mut land = 0_u64;
            for y in by..(by + BLOCK_CELLS).min(side) {
                for x in bx..(bx + BLOCK_CELLS).min(side) {
                    let i = y * side + x;
                    if c.sea[i] || c.lake[i] {
                        continue;
                    }
                    land += 1;
                    let k = Rock::ALL
                        .iter()
                        .position(|&r| r == c.rock[i])
                        .expect("a rock");
                    counts[k] += 1;
                }
            }
            if land * 4 < (BLOCK_CELLS * BLOCK_CELLS) as u64 {
                continue;
            }
            blocks += 1;
            kinds += counts
                .iter()
                .filter(|&&k| k * 1000 >= land * BLOCK_ROCK_PER_MILLE)
                .count() as u64;
        }
    }
    kinds * 100 / blocks.max(1)
}

/// The walk between facing banks of the big rivers, wading: its median,
/// ninth decile and longest, in km.
fn detours(c: &Continent, p: &[Passage]) -> [u64; 3] {
    let grid = c.grid;
    let walkable = |i: usize| matches!(p[i], Passage::Main | Passage::Cut | Passage::Crossing);
    let mut search = Search::new(grid);
    let mut walks = Vec::new();
    for i in (0..grid.len()).filter(|i| i % DETOUR_SAMPLE == 0) {
        if !matches!(p[i], Passage::Barrier | Passage::Crossing)
            || c.cubic_metres(i) < DETOUR_RIVER_M3S
        {
            continue;
        }
        let r = c.receiver[i] as usize;
        let ((x, y), (rx, ry)) = (grid.xy(i), grid.xy(r));
        let (dx, dy) = (rx - x, ry - y);
        if (dx, dy) == (0, 0) {
            continue;
        }
        let bank = |sx: i32, sy: i32| {
            (1..=4).find_map(|k| {
                let (bx, by) = (x + sx * k, y + sy * k);
                (grid.inside(bx, by) && walkable(grid.index(bx, by))).then(|| grid.index(bx, by))
            })
        };
        if let (Some(a), Some(b)) = (bank(-dy, dx), bank(dy, -dx)) {
            walks.push(search.walk(a, b, &walkable));
        }
    }
    if walks.is_empty() {
        return [0; 3];
    }
    walks.sort_unstable();
    let at = |q: usize| walks[(walks.len() - 1) * q / 100] / 1000;
    [at(50), at(90), at(100)]
}

/// A shortest walk over the walkable cells, eight ways (never between two
/// cells that only touch by a corner past water), with a bucket queue:
/// a straight step costs 5, a diagonal 7 (√2 within 1 %).
struct Search {
    grid: Grid,
    cost: Vec<u32>,
    stamp: Vec<u32>,
    now: u32,
    buckets: Vec<Vec<u32>>,
}

impl Search {
    fn new(grid: Grid) -> Self {
        Self {
            grid,
            cost: vec![0; grid.len()],
            stamp: vec![0; grid.len()],
            now: 0,
            buckets: vec![Vec::new(); 8],
        }
    }

    /// The walk from `a` to `b`, in metres, at most [`DETOUR_CAP_KM`].
    fn walk(&mut self, a: usize, b: usize, walkable: &dyn Fn(usize) -> bool) -> u64 {
        let cap = u32::try_from(DETOUR_CAP_KM * 1000 * 5 / CELL_M.unsigned_abs()).expect("small");
        self.now += 1;
        for bucket in &mut self.buckets {
            bucket.clear();
        }
        self.stamp[a] = self.now;
        self.cost[a] = 0;
        self.buckets[0].push(u32::try_from(a).expect("small"));
        let mut d = 0_u32;
        let mut pending = 1_usize;
        while pending > 0 && d <= cap {
            let slot = (d % 8) as usize;
            while let Some(c) = self.buckets[slot].pop() {
                pending -= 1;
                let c = c as usize;
                if self.cost[c] != d {
                    continue;
                }
                if c == b {
                    return u64::from(d) * CELL_M.unsigned_abs() / 5;
                }
                let (x, y) = self.grid.xy(c);
                for &(dx, dy, diagonal) in &super::NEIGHBOURS {
                    let (nx, ny) = (x + dx, y + dy);
                    if !self.grid.inside(nx, ny) {
                        continue;
                    }
                    let m = self.grid.index(nx, ny);
                    if !walkable(m)
                        || (diagonal
                            && !(walkable(self.grid.index(nx, y))
                                && walkable(self.grid.index(x, ny))))
                    {
                        continue;
                    }
                    let nd = d + if diagonal { 7 } else { 5 };
                    if self.stamp[m] != self.now || nd < self.cost[m] {
                        self.stamp[m] = self.now;
                        self.cost[m] = nd;
                        self.buckets[(nd % 8) as usize].push(u32::try_from(m).expect("small"));
                        pending += 1;
                    }
                }
            }
            d += 1;
        }
        DETOUR_CAP_KM * 1000
    }
}
