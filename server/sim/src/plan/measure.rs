//! What the project lead judges a seed by (`premier-pas.md` §2): the
//! water on the land, the land one can walk across, the longest river
//! with no natural crossing, the biomes' shares.
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
//! shallower still where it braids down a steep reach, or spills over a
//! lake's sill.

use std::collections::VecDeque;

use super::relief::step_m;
use super::{Biome, CELL_M, Continent, iroot, isqrt};

/// Depth on a riffle, per mille of the mean depth.
pub const RIFFLE_PER_MILLE: u64 = 500;

/// A reach dropping at least this many cm a cell (0.2 %) braids into
/// shallow arms; its shallowest arm is this deep, per mille of the mean.
pub const BRAID_DROP_CM: i32 = 400;
/// The shallowest arm of a braided reach, per mille of the mean depth.
pub const BRAID_PER_MILLE: u64 = 300;

/// A lake's outlet spills over a broad sill: this deep, per mille.
pub const SILL_PER_MILLE: u64 = 300;

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
}

/// What a cell is for a walker who crosses water up to some depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passage {
    /// The sea.
    Sea,
    /// A lake: walked around.
    Lake,
    /// A river too deep to cross here.
    Barrier,
    /// A river large enough to matter (wider than 10 m), crossed here.
    Crossing,
    /// Land in the largest piece.
    Main,
    /// Land cut off from the largest piece.
    Cut,
}

/// The shallowest water across each cell's river, in cm (0 at sea and in
/// lakes, which are not crossed but walked around).
fn shallowest(c: &Continent) -> Vec<u64> {
    let n = c.grid.len();
    let mut sill = vec![false; n];
    for i in 0..n {
        let r = c.receiver[i] as usize;
        if c.lake[i] && !c.lake[r] {
            sill[r] = true;
        }
    }
    (0..n)
        .map(|i| {
            if c.sea[i] || c.lake[i] {
                return 0;
            }
            let r = c.receiver[i] as usize;
            let d = depth_cm(c.discharge[i]);
            let mut f = RIFFLE_PER_MILLE;
            if c.surface[i] - c.surface[r] >= BRAID_DROP_CM {
                f = f.min(BRAID_PER_MILLE);
            }
            if sill[i] {
                f = f.min(SILL_PER_MILLE);
            }
            d * f / 1000
        })
        .collect()
}

/// Each cell's passage for a walker crossing up to `limit_cm`.
pub fn passages(c: &Continent, limit_cm: u64) -> Vec<Passage> {
    let grid = c.grid;
    let n = grid.len();
    let low = shallowest(c);
    let mut out: Vec<Passage> = (0..n)
        .map(|i| {
            if c.sea[i] {
                Passage::Sea
            } else if c.lake[i] {
                Passage::Lake
            } else if low[i] > limit_cm {
                Passage::Barrier
            } else if width_cm(c.discharge[i]) >= 1_000 {
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
fn walking(c: &Continent, limit_cm: u64) -> (u64, u64) {
    let p = passages(c, limit_cm);
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
        river_m2 += width_cm(c.discharge[i]) * step * SINUOSITY_PER_MILLE / 1000 / 100;
        if c.sea[r] {
            largest = largest.max(c.discharge[i]);
        }
        if let Some(k) = Biome::LAND.iter().position(|&b| b == c.biome[i]) {
            counts[k] += 1;
        }
    }
    let dry_land = land - lakes;
    let (dry_piece_bp, longest_dry) = walking(c, DRY_CM);
    let (wading_piece_bp, longest_wading) = walking(c, WADE_CM);
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
    }
}
