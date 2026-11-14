//! Candidate zones for the vivarium on the atlas (lot 0 of the vivarium
//! plan, D41): on one seed, the river crossings a band could live by,
//! each refined to 64 m and judged by what the plan asks of a zone, the
//! best three written out for the project lead to pick from. Nothing
//! here enters the game.
//!
//! ```text
//! cargo run --release -p sim --example vivarium_zones
//! ZONES_OUT=some/folder ZONES_SEED=14 cargo run --release -p sim --example vivarium_zones
//! ```
//!
//! A zone is a square of 1.5 to 2.2 km a side that holds: a natural
//! crossing of an atlas river (40 to 500 m³/s, never a great river); a
//! forest edge and a hill (40 m of local relief, under 600 m) within 2 km
//! of it; two camp sites on open ground, on opposite banks, 2 to 4 km
//! apart, water near each, the crossing hidden from both by forest or by
//! the ground; a mild climate (mean year 15–25 °C), no desert, no marsh.
//!
//! Writes, per zone, the raw picture of the zone with a kilometre around
//! it and of the 20 km around it (PPM, one pixel a region cell of 64 m),
//! its heights in metres (TSV), and `zones.tsv`: the measures and where
//! every landmark is. The labels
//! and the page are drawn from those by a script beside them.

// Grid code reads as arithmetic: `x`, `y`, `i`, `h`, `q`, `d`; cells are
// indexed as `usize` and measured as `i64`, both well within range on a
// 320-cell region; `camp_a`, `camp_b`, `camps_m` are the names the page
// uses; the judging and the writing are long because they are plain.
#![allow(
    clippy::many_single_char_names,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::similar_names,
    clippy::too_many_lines
)]

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use sim::plan::measure::{self, Passage};
use sim::plan::region::{PER_CELL, REGION_CELL_M, Water};
use sim::plan::{Biome, CELL_M, Continent, Region};

/// The seed the zones are looked for on, unless `ZONES_SEED` says.
const DEFAULT_SEED: u64 = 14;

/// Continent cells a side of the region around a crossing: 10 cells,
/// 20 km, the context picture.
const CONTEXT_CELLS: usize = 10;

/// Region cells a side of that region.
const REGION_CELLS: usize = CONTEXT_CELLS * PER_CELL as usize;

/// A crossing's river, in m³/s: an atlas river, under a great river.
const FLOW_M3S: (u64, u64) = (40, measure::GREAT_RIVER_M3S);

/// The mean year a zone may have, in tenths of a degree.
const WARMTH_TENTHS: (i16, i16) = (150, 250);

/// The ground of a zone stays under this height (cm).
const HIGHEST_CM: i32 = 60_000;

/// Zone sides tried, in region cells: 1.5 to 2.2 km.
const ZONE_SIDES: [usize; 4] = [24, 28, 32, 34];

/// The margin drawn around a zone, in region cells: a kilometre.
const MARGIN: usize = 16;

/// The crossing, the hill and the camps stay this far inside the zone
/// (cells).
const INSIDE: usize = 3;

/// A hill's least local relief (cm), and how far around its summit the
/// low point is looked for (cells, 768 m).
const HILL_CM: i32 = 4_000;
const HILL_RADIUS: i64 = 12;

/// How far apart the two camps are, as the crow flies (m).
const CAMPS_M: (i64, i64) = (2_000, 4_000);

/// A camp has water within this (cells, 320 m) and stands at least this
/// far from the crossing (m).
const CAMP_WATER_CELLS: i64 = 5;
const CAMP_FROM_CROSSING_M: i64 = 800;

/// Forest cells on the line of sight that hide the crossing.
const HIDING_FOREST: usize = 3;

/// A bank is at least this share of the zone, per mille.
const BANK_PER_MILLE: usize = 100;

/// Crossings kept after the coarse pass, and how far apart they are
/// kept (continent cells).
const COARSE_KEEP: usize = 48;
const COARSE_APART: i32 = 6;

/// The chosen zones are at least this far apart (continent cells, 120
/// km) and on different rivers.
const CHOSEN_APART: i32 = 60;

type Rgb = [u8; 3];

fn main() {
    let seed = std::env::var("ZONES_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SEED);
    let out = std::env::var_os("ZONES_OUT").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../content/ai-reserve/review/vivarium/zones")
        },
        PathBuf::from,
    );
    std::fs::create_dir_all(&out).expect("the output folder");

    let start = Instant::now();
    let c = Continent::new(seed);
    eprintln!(
        "continent {seed} born in {} ms",
        start.elapsed().as_millis()
    );
    let coarse = coarse(&c);
    eprintln!("{} crossings kept after the coarse pass", coarse.len());

    let mut found: Vec<Found> = Vec::new();
    for (k, &i) in coarse.iter().enumerate() {
        let centre = c.grid.xy(i);
        let region = Region::around(&c, centre, REGION_CELLS);
        if let Some(f) = best_zone(&c, &region, i) {
            eprintln!(
                "{k:2} cell ({}, {}) score {} relief {} m camps {} m",
                centre.0,
                centre.1,
                f.score,
                f.relief_cm / 100,
                f.camps_m
            );
            found.push(f);
        }
    }
    found.sort_by_key(|f| std::cmp::Reverse(f.score));
    let mut chosen: Vec<Found> = Vec::new();
    for f in found {
        let apart = chosen.iter().all(|g| {
            let (ax, ay) = c.grid.xy(f.cell);
            let (bx, by) = c.grid.xy(g.cell);
            (ax - bx).abs().max((ay - by).abs()) >= CHOSEN_APART && f.mouth != g.mouth
        });
        if apart {
            chosen.push(f);
        }
        if chosen.len() == 3 {
            break;
        }
    }

    let mut tsv = String::from(
        "zone\tcell_x\tcell_y\tregion_origin_x\tregion_origin_y\tzone_x\tzone_y\tzone_side\t\
         zone_km2\tflow_m3s\tflow_low_m3s\tcrossing_x\tcrossing_y\tedge_x\tedge_y\thill_x\thill_y\t\
         camp_a_x\tcamp_a_y\tcamp_b_x\tcamp_b_y\tcrossing_edge_m\tcrossing_hill_m\tcamps_m\t\
         hill_relief_m\thill_top_m\tcrossing_height_m\ttemperature_tenths\tlatitude_hundredths\t\
         rain_mm\tbiomes\tcamps_apart_ok\tscore\n",
    );
    for (n, f) in chosen.iter().enumerate() {
        let n = n + 1;
        let (cx, cy) = c.grid.xy(f.cell);
        let region = Region::around(&c, (cx, cy), REGION_CELLS);
        let crop = (f.zone.0 - MARGIN, f.zone.1 - MARGIN, f.zone.2 + 2 * MARGIN);
        let (main_channel, _) = channel(&c, &region, f.cell).expect("found once already");
        std::fs::write(
            out.join(format!("zone-{n}-brut.ppm")),
            ppm(crop.2, crop.2, &draw(&region, &main_channel, crop)),
        )
        .expect("write");
        std::fs::write(
            out.join(format!("zone-{n}-contexte-brut.ppm")),
            ppm(
                REGION_CELLS,
                REGION_CELLS,
                &draw(&region, &main_channel, (0, 0, REGION_CELLS)),
            ),
        )
        .expect("write");
        let mut heights = String::new();
        for y in crop.1..crop.1 + crop.2 {
            let row: Vec<String> = (crop.0..crop.0 + crop.2)
                .map(|x| (region.height[y * REGION_CELLS + x] / 100).to_string())
                .collect();
            let _ = writeln!(heights, "{}", row.join("\t"));
        }
        std::fs::write(out.join(format!("zone-{n}-hauteurs.tsv")), heights).expect("write");
        let biomes = f
            .biomes
            .iter()
            .map(|(b, share)| format!("{}:{share}", biome_name(*b)))
            .collect::<Vec<_>>()
            .join(";");
        let at = |p: (usize, usize)| format!("{}\t{}", p.0, p.1);
        let _ = writeln!(
            tsv,
            "{n}\t{cx}\t{cy}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            region.origin.0,
            region.origin.1,
            f.zone.0,
            f.zone.1,
            f.zone.2,
            km2(f.zone.2),
            c.cubic_metres(f.cell),
            c.low_discharge[f.cell] / 1_000_000,
            at(f.crossing),
            at(f.edge),
            at(f.hill),
            at(f.camp_a),
            at(f.camp_b),
            f.edge_m,
            f.hill_m,
            f.camps_m,
            f.relief_cm / 100,
            f.hill_top_cm / 100,
            f.crossing_cm / 100,
            c.temperature[f.cell],
            latitude(&c, cy),
            c.rain[f.cell],
            biomes,
            u8::from(f.camps_ok),
            f.score,
        );
    }
    std::fs::write(out.join("zones.tsv"), tsv).expect("write");
    eprintln!(
        "{} zones written to {} in {} s",
        chosen.len(),
        out.display(),
        start.elapsed().as_secs()
    );
}

/// The crossings worth refining: a wadeable atlas river under a great
/// river, a mild year, low ground, no desert or marsh, forest and open
/// ground both within a cell; the best `COARSE_KEEP`, kept apart.
fn coarse(c: &Continent) -> Vec<usize> {
    let passages = measure::passages(c, measure::WADE_CM);
    let grid = c.grid;
    let mut scored: Vec<(i64, usize)> = (0..grid.len())
        .filter(|&i| {
            passages[i] == Passage::Crossing
                && (FLOW_M3S.0..FLOW_M3S.1).contains(&c.cubic_metres(i))
                && (WARMTH_TENTHS.0..=WARMTH_TENTHS.1).contains(&c.temperature[i])
                && c.surface[i] < HIGHEST_CM
                && !matches!(c.biome[i], Biome::Desert | Biome::Marsh)
        })
        .filter_map(|i| {
            let (x, y) = grid.xy(i);
            let (mut forest, mut open, mut bad) = (0_i64, 0_i64, 0_i64);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if grid.inside(x + dx, y + dy) {
                        let b = c.biome[grid.index(x + dx, y + dy)];
                        forest += i64::from(is_forest(b));
                        open += i64::from(is_open(b));
                        bad += i64::from(matches!(b, Biome::Desert | Biome::Marsh | Biome::Sea));
                    }
                }
            }
            (forest > 0 && open > 0 && bad == 0).then(|| {
                let warmth = 50 - (i64::from(c.temperature[i]) - 200).abs();
                let balance = 9 - (forest - open).abs();
                (warmth + 10 * balance, i)
            })
        })
        .collect();
    eprintln!("{} crossings fit the coarse criteria", scored.len());
    scored.sort_by_key(|&(s, i)| (std::cmp::Reverse(s), i));
    let mut kept: Vec<usize> = Vec::new();
    for (_, i) in scored {
        let (x, y) = grid.xy(i);
        let apart = kept.iter().all(|&j| {
            let (jx, jy) = grid.xy(j);
            (x - jx).abs().max((y - jy).abs()) >= COARSE_APART
        });
        if apart {
            kept.push(i);
        }
        if kept.len() == COARSE_KEEP {
            break;
        }
    }
    kept
}

/// A pair of camps: its worth, whether it is far enough apart, camp A,
/// camp B and the distance between them (m).
type Camps = (i64, bool, (usize, usize), (usize, usize), i64);

/// A zone found around a crossing: positions are region cells.
struct Found {
    cell: usize,
    mouth: usize,
    score: i64,
    /// The zone's north-west corner and side, in region cells.
    zone: (usize, usize, usize),
    crossing: (usize, usize),
    edge: (usize, usize),
    hill: (usize, usize),
    camp_a: (usize, usize),
    camp_b: (usize, usize),
    edge_m: i64,
    hill_m: i64,
    camps_m: i64,
    camps_ok: bool,
    relief_cm: i32,
    hill_top_cm: i32,
    crossing_cm: i32,
    biomes: Vec<(Biome, u64)>,
}

/// The best zone around the crossing of continent cell `cell`, whose
/// region `r` is centred on it.
fn best_zone(c: &Continent, r: &Region, cell: usize) -> Option<Found> {
    let (channel, crossing) = channel(c, r, cell)?;
    let mouth = mouth(c, cell);
    let mut best: Option<Found> = None;
    for &side in &ZONE_SIDES {
        let (kx, ky) = crossing;
        let lo = |k: usize| (k + INSIDE + 1).saturating_sub(side).max(MARGIN);
        let hi = |k: usize| (k - INSIDE).min(REGION_CELLS - MARGIN - side);
        let mut oy = lo(ky);
        while oy <= hi(ky) {
            let mut ox = lo(kx);
            while ox <= hi(kx) {
                if let Some(f) = judge(r, &channel, cell, mouth, (ox, oy, side), crossing)
                    && best.as_ref().is_none_or(|b| f.score > b.score)
                {
                    best = Some(f);
                }
                ox += 2;
            }
            oy += 2;
        }
    }
    best
}

/// The main channel through the crossing's cell and the crossing on
/// it: the atlas river enters the cell from its main upstream neighbour
/// and leaves it for its receiver, so the channel is the shortest run of
/// river cells from the one to the other (tributaries are side branches
/// and fall off it); the crossing is the channel cell nearest the
/// cell's middle. The channel is marked over the whole region.
fn channel(c: &Continent, r: &Region, cell: usize) -> Option<(Vec<bool>, (usize, usize))> {
    let rs = r.grid.side;
    let (cx, cy) = c.grid.xy(cell);
    let local = |k: usize| -> (i64, i64) {
        let (x, y) = c.grid.xy(k);
        (
            (i64::from(x - r.origin.0) * 2 + 1) * i64::from(PER_CELL) / 2,
            (i64::from(y - r.origin.1) * 2 + 1) * i64::from(PER_CELL) / 2,
        )
    };
    let upstream = (0..c.grid.len())
        .filter(|&j| j != cell && c.receiver[j] as usize == cell)
        .max_by_key(|&j| c.discharge[j])?;
    let nearest = |p: (i64, i64)| -> Option<usize> {
        (0..rs * rs)
            .filter(|&i| r.water[i] == Water::River)
            .min_by_key(|&i| distance_sq(((i % rs) as i64, (i / rs) as i64), p))
    };
    let (from, to) = (
        nearest(local(upstream))?,
        nearest(local(c.receiver[cell] as usize))?,
    );
    // The shortest run of river cells, eight ways.
    let mut back = vec![usize::MAX; rs * rs];
    back[from] = from;
    let mut queue = VecDeque::from([from]);
    while let Some(i) = queue.pop_front() {
        if i == to {
            break;
        }
        let (x, y) = ((i % rs) as i64, (i / rs) as i64);
        for dy in -1..=1_i64 {
            for dx in -1..=1_i64 {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= rs as i64 || ny >= rs as i64 {
                    continue;
                }
                let j = ny as usize * rs + nx as usize;
                if back[j] == usize::MAX && r.water[j] == Water::River {
                    back[j] = i;
                    queue.push_back(j);
                }
            }
        }
    }
    if back[to] == usize::MAX {
        return None;
    }
    let mut on = vec![false; rs * rs];
    let mut i = to;
    loop {
        on[i] = true;
        if i == from {
            break;
        }
        i = back[i];
    }
    let (x0, y0) = (
        usize::try_from(cx - r.origin.0).ok()? * PER_CELL as usize,
        usize::try_from(cy - r.origin.1).ok()? * PER_CELL as usize,
    );
    let middle = local(cell);
    let crossing = (0..rs * rs)
        .filter(|&i| {
            on[i]
                && (x0..x0 + PER_CELL as usize).contains(&(i % rs))
                && (y0..y0 + PER_CELL as usize).contains(&(i / rs))
        })
        .min_by_key(|&i| distance_sq(((i % rs) as i64, (i / rs) as i64), middle))?;
    Some((on, (crossing % rs, crossing / rs)))
}

/// Judge the zone `(ox, oy, side)` of region `r`: its score and its
/// landmarks, or nothing when a hard criterion fails.
fn judge(
    r: &Region,
    channel: &[bool],
    cell: usize,
    mouth: usize,
    zone: (usize, usize, usize),
    crossing: (usize, usize),
) -> Option<Found> {
    let (ox, oy, side) = zone;
    let rs = r.grid.side;
    let at = |x: usize, y: usize| y * rs + x;
    let cells: Vec<(usize, usize)> = (oy..oy + side)
        .flat_map(|y| (ox..ox + side).map(move |x| (x, y)))
        .collect();
    let metres = |a: (usize, usize), b: (usize, usize)| -> i64 {
        let d = distance_sq((a.0 as i64, a.1 as i64), (b.0 as i64, b.1 as i64));
        ((d * REGION_CELL_M * REGION_CELL_M) as u64).isqrt() as i64
    };

    // Ground, no sea or lake, nothing too high, a mild year.
    for &(x, y) in &cells {
        let i = at(x, y);
        if matches!(r.water[i], Water::Sea | Water::Lake)
            || r.height[i] >= HIGHEST_CM
            || matches!(r.biome[i], Biome::Desert | Biome::Marsh)
        {
            return None;
        }
    }

    // The banks: the land the main channel cuts, four ways; brooks and
    // side rivers are walked over.
    let mut bank = vec![usize::MAX; rs * rs];
    let mut sizes: Vec<usize> = Vec::new();
    for &(x, y) in &cells {
        let i = at(x, y);
        if bank[i] != usize::MAX || channel[i] {
            continue;
        }
        let id = sizes.len();
        let mut size = 0;
        let mut queue = VecDeque::from([(x, y)]);
        bank[i] = id;
        while let Some((x, y)) = queue.pop_front() {
            size += 1;
            for (nx, ny) in [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ] {
                if nx >= ox && nx < ox + side && ny >= oy && ny < oy + side {
                    let j = at(nx, ny);
                    if bank[j] == usize::MAX && !channel[j] {
                        bank[j] = id;
                        queue.push_back((nx, ny));
                    }
                }
            }
        }
        sizes.push(size);
    }
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&k| std::cmp::Reverse(sizes[k]));
    let (bank_a, bank_b) = (*order.first()?, *order.get(1)?);
    if sizes[bank_b] * 1000 < side * side * BANK_PER_MILLE {
        return None;
    }
    // Both banks touch the crossing's surroundings.
    let near = |id: usize| {
        (-2_i64..=2).any(|dy| {
            (-2_i64..=2).any(|dx| {
                let (x, y) = (crossing.0 as i64 + dx, crossing.1 as i64 + dy);
                x >= 0 && y >= 0 && bank[at(x as usize, y as usize)] == id
            })
        })
    };
    if !near(bank_a) || !near(bank_b) {
        return None;
    }

    // The forest edge nearest the crossing.
    let is_edge = |x: usize, y: usize| {
        is_forest(r.biome[at(x, y)])
            && [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ]
            .iter()
            .any(|&(nx, ny)| nx < rs && ny < rs && is_open(r.biome[at(nx, ny)]))
    };
    let edge = cells
        .iter()
        .copied()
        .filter(|&(x, y)| is_edge(x, y))
        .min_by_key(|&p| metres(p, crossing))?;
    let edge_m = metres(edge, crossing);

    // The hill: the land cell with the most relief over the low point
    // within `HILL_RADIUS`.
    let lowest_near = |x: usize, y: usize| -> i32 {
        let mut low = i32::MAX;
        for dy in -HILL_RADIUS..=HILL_RADIUS {
            for dx in -HILL_RADIUS..=HILL_RADIUS {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx >= ox as i64
                    && nx < (ox + side) as i64
                    && ny >= oy as i64
                    && ny < (oy + side) as i64
                {
                    low = low.min(r.surface[at(nx as usize, ny as usize)]);
                }
            }
        }
        low
    };
    let (hill, relief_cm) = cells
        .iter()
        .copied()
        .filter(|&(x, y)| {
            r.water[at(x, y)] == Water::Land
                && x >= ox + INSIDE
                && x < ox + side - INSIDE
                && y >= oy + INSIDE
                && y < oy + side - INSIDE
        })
        .map(|(x, y)| ((x, y), r.height[at(x, y)] - lowest_near(x, y)))
        .max_by_key(|&(p, relief)| (relief, std::cmp::Reverse(metres(p, crossing))))?;
    if relief_cm < HILL_CM {
        return None;
    }
    let hill_m = metres(hill, crossing);

    // Sight of the crossing from each cell, and water near it.
    let eye_cm = i64::from(r.surface[at(crossing.0, crossing.1)]) + 100;
    let hidden = |x: usize, y: usize| -> (bool, usize) {
        let line = bresenham((x as i64, y as i64), (crossing.0 as i64, crossing.1 as i64));
        let n = line.len();
        if n < 3 {
            return (false, 0);
        }
        let from_cm = i64::from(r.height[at(x, y)]) + 170;
        let mut forest = 0;
        let mut ground = false;
        for (k, &(lx, ly)) in line.iter().enumerate().take(n - 1).skip(1) {
            let j = at(lx as usize, ly as usize);
            forest += usize::from(is_forest(r.biome[j]));
            let sight = from_cm + (eye_cm - from_cm) * k as i64 / (n as i64 - 1);
            ground |= i64::from(r.height[j]) > sight;
        }
        (forest >= HIDING_FOREST || ground, forest)
    };
    let water_near = |x: usize, y: usize| {
        (-CAMP_WATER_CELLS..=CAMP_WATER_CELLS).any(|dy| {
            (-CAMP_WATER_CELLS..=CAMP_WATER_CELLS).any(|dx| {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                nx >= 0
                    && ny >= 0
                    && (nx as usize) < rs
                    && (ny as usize) < rs
                    && matches!(
                        r.water[at(nx as usize, ny as usize)],
                        Water::River | Water::Brook | Water::Lake
                    )
            })
        })
    };
    let camps_on = |id: usize| -> Vec<((usize, usize), usize)> {
        cells
            .iter()
            .copied()
            .filter(|&(x, y)| {
                bank[at(x, y)] == id
                    && x >= ox + INSIDE
                    && x < ox + side - INSIDE
                    && y >= oy + INSIDE
                    && y < oy + side - INSIDE
                    && is_open(r.biome[at(x, y)])
                    && metres((x, y), crossing) >= CAMP_FROM_CROSSING_M
                    && water_near(x, y)
            })
            .filter_map(|(x, y)| {
                let (hid, forest) = hidden(x, y);
                hid.then_some(((x, y), forest))
            })
            .collect()
    };
    let (on_a, on_b) = (camps_on(bank_a), camps_on(bank_b));
    let mut pair: Option<Camps> = None;
    for &(a, fa) in &on_a {
        for &(b, fb) in &on_b {
            let d = metres(a, b);
            let ok = (CAMPS_M.0..=CAMPS_M.1).contains(&d);
            let worth = (if ok { 10_000 } else { 0 })
                + d.min(CAMPS_M.1)
                + 40 * (fa.min(10) + fb.min(10)) as i64;
            if pair.is_none_or(|p| worth > p.0) {
                pair = Some((worth, ok, a, b, d));
            }
        }
    }
    let (camps_worth, camps_ok, camp_a, camp_b, camps_m) = pair?;

    // Biome shares, per mille of the zone.
    let mut shares: Vec<(Biome, u64)> = Vec::new();
    for &(x, y) in &cells {
        let b = r.biome[at(x, y)];
        match shares.iter_mut().find(|(k, _)| *k == b) {
            Some((_, n)) => *n += 1,
            None => shares.push((b, 1)),
        }
    }
    let total = (side * side) as u64;
    for (_, n) in &mut shares {
        *n = *n * 1000 / total;
    }
    shares.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    let forest_per_mille = shares
        .iter()
        .filter(|(b, _)| is_forest(*b))
        .map(|(_, n)| *n)
        .sum::<u64>() as i64;

    // The score: the camps first, then the hill, then a wood that is
    // neither a wall nor a hedge, then the edge and the hill not far.
    let score = camps_worth + 3 * i64::from(relief_cm.min(12_000)) / 100
        - 4 * (forest_per_mille - 450).abs() / 10
        - edge_m / 50
        - hill_m / 50;
    Some(Found {
        cell,
        mouth,
        score,
        zone,
        crossing,
        edge,
        hill,
        camp_a,
        camp_b,
        edge_m,
        hill_m,
        camps_m,
        camps_ok,
        relief_cm,
        hill_top_cm: r.height[at(hill.0, hill.1)],
        crossing_cm: r.surface[at(crossing.0, crossing.1)],
        biomes: shares,
    })
}

/// The cell at the sea where `cell`'s river ends: two cells with the
/// same mouth are on the same river.
fn mouth(c: &Continent, cell: usize) -> usize {
    let mut i = cell;
    for _ in 0..c.grid.len() {
        let r = c.receiver[i] as usize;
        if r == i {
            break;
        }
        i = r;
    }
    i
}

/// Latitude of continent row `y`, in hundredths of a degree.
fn latitude(c: &Continent, y: i32) -> i64 {
    i64::from(c.north_latitude) - i64::from(y) * CELL_M * 100 / 111_200
}

fn is_forest(b: Biome) -> bool {
    matches!(
        b,
        Biome::Broadleaf | Biome::Jungle | Biome::MixedForest | Biome::Conifer
    )
}

fn is_open(b: Biome) -> bool {
    matches!(b, Biome::Grassland | Biome::WoodedSteppe | Biome::Steppe)
}

/// The area of a square of `side_cells`, in hundredths of a km².
fn km2(side_cells: usize) -> String {
    let m = side_cells as i64 * REGION_CELL_M;
    let hundredths = m * m / 10_000;
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
}

fn distance_sq(a: (i64, i64), b: (i64, i64)) -> i64 {
    (a.0 - b.0).pow(2) + (a.1 - b.1).pow(2)
}

/// The cells of the straight line from `a` to `b`, both included.
fn bresenham(a: (i64, i64), b: (i64, i64)) -> Vec<(i64, i64)> {
    let (dx, dy) = ((b.0 - a.0).abs(), -(b.1 - a.1).abs());
    let (sx, sy) = ((b.0 - a.0).signum(), (b.1 - a.1).signum());
    let (mut x, mut y, mut err) = (a.0, a.1, dx + dy);
    let mut out = vec![(x, y)];
    while (x, y) != b {
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
        out.push((x, y));
    }
    out
}

// ---------------------------------------------------------------- drawing

const SEA: Rgb = [38, 74, 122];
const LAKE: Rgb = [86, 146, 205];
const RIVER: Rgb = [40, 96, 190];
const MAIN: Rgb = [10, 40, 140];
const BROOK: Rgb = [96, 150, 214];

/// The atlas's biome colours (`atlas.rs`).
fn biome_colour(b: Biome) -> Rgb {
    match b {
        Biome::Sea => SEA,
        Biome::Lake => LAKE,
        Biome::Desert => [226, 204, 150],
        Biome::Steppe => [204, 196, 128],
        Biome::WoodedSteppe => [176, 186, 108],
        Biome::Grassland => [164, 190, 102],
        Biome::Broadleaf => [86, 140, 66],
        Biome::Jungle => [30, 104, 52],
        Biome::MixedForest => [72, 122, 70],
        Biome::Conifer => [56, 100, 74],
        Biome::Marsh => [104, 150, 128],
        Biome::Tundra => [176, 180, 160],
        Biome::Alpine => [196, 190, 184],
    }
}

fn biome_name(b: Biome) -> &'static str {
    match b {
        Biome::Sea => "sea",
        Biome::Lake => "lake",
        Biome::Desert => "desert",
        Biome::Steppe => "steppe",
        Biome::WoodedSteppe => "wooded steppe",
        Biome::Grassland => "grassland",
        Biome::Broadleaf => "broadleaf forest",
        Biome::Jungle => "jungle",
        Biome::MixedForest => "mixed forest",
        Biome::Conifer => "conifer forest",
        Biome::Marsh => "marsh",
        Biome::Tundra => "tundra",
        Biome::Alpine => "alpine",
    }
}

/// The square `(x, y, side)` of region `r`, biomes shaded by the slope,
/// water over them, the crossing's river (`channel`) in its own blue.
fn draw(r: &Region, channel: &[bool], crop: (usize, usize, usize)) -> Vec<Rgb> {
    let (x0, y0, side) = crop;
    let rs = r.grid.side;
    let mut img = vec![[0; 3]; side * side];
    for y in 0..side {
        for x in 0..side {
            let i = (y0 + y) * rs + x0 + x;
            img[y * side + x] = match r.water[i] {
                Water::Sea => SEA,
                Water::Lake => LAKE,
                Water::River if channel[i] => MAIN,
                Water::River => RIVER,
                Water::Brook => BROOK,
                Water::Land => {
                    let at = |x: usize, y: usize| {
                        i64::from(r.height[y.min(rs - 1) * rs + x.min(rs - 1)])
                    };
                    let (ax, ay) = (x0 + x, y0 + y);
                    let (l, rr) = (at(ax.saturating_sub(1), ay), at(ax + 1, ay));
                    let (u, d) = (at(ax, ay.saturating_sub(1)), at(ax, ay + 1));
                    let factor = (1000 + ((l - rr) + (u - d)) / 24).clamp(500, 1400);
                    biome_colour(r.biome[i]).map(|v| {
                        u8::try_from((i64::from(v) * factor / 1000).clamp(0, 255)).expect("clamped")
                    })
                }
            };
        }
    }
    img
}

/// A binary PPM (P6).
fn ppm(width: usize, height: usize, pixels: &[Rgb]) -> Vec<u8> {
    let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
    out.extend(pixels.iter().flatten());
    out
}
