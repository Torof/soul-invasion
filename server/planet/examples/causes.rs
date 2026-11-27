//! A probe of what changes the lands' shapes from one step to the next,
//! cause by cause (version 9): every cell of continental crust followed by
//! its home that flips between dry and under the sea; the land the fixed
//! grid draws anew or stops drawing; and every patch of sea appearing
//! inside a continent, by size.
//!
//! ```text
//! cargo run --release -p planet --example causes -- settings.ron 14,9
//! ```

#![allow(
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::needless_range_loop,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::filter_map_next,
    clippy::bool_to_int_with_if,
    clippy::unnecessary_sort_by,
    reason = "a measuring tool: one long count, printed as rounded shares"
)]

use planet::history::OCEAN;
use planet::{CELLS, Planet, Settings, normalize};
use std::collections::HashMap;

const NAMES: [&str; 16] = [
    "the sea level alone",
    "a rift stretched it",
    "a stretched margin cooled",
    "an arc built",
    "a terrane docked",
    "a plume lifted it",
    "it wore down",
    "it levelled with its neighbours",
    "sediment filled it",
    "a collision's pile rose",
    "a pile spread or wore down",
    "flood basalt",
    "height changed with no event (the grid)",
    "the grid drew another crust there (hidden before, or a copy)",
    "new crust (silt, an arc's island)",
    "the grid drew ocean floor in its place",
];
const SIZES: [(&str, i64); 4] = [
    ("under 50 k km2", 50_000),
    ("50-200 k", 200_000),
    ("200 k-1 M", 1_000_000),
    ("over 1 M", i64::MAX),
];
/// A patch is inland when none of its cells is nearer than this to the
/// ocean's floor, km.
const INLAND_KM: u32 = 300;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&args[1]).expect("a settings file");
    let settings: Settings = ron::from_str(&text).expect("settings");
    let mut flips = [0_u64; 16];
    let mut far_flips = [0_u64; 16];
    let mut to_sea = [0_u64; 16];
    // Followed, far followed, steps, land cells, land cells drawn anew
    // (hidden before / a copy / home unseen before), dry homes no longer
    // drawn, crust cells, hidden bits, newly hidden, copies.
    let mut n = [0_u64; 12];
    // Patches of new sea: [inland popped, inland from the coast, at the
    // coast] by size, then by cause for the inland ones.
    let mut patches = [[0_u64; 4]; 3];
    let mut patch_cause = [[0_u64; 16]; 2];
    let mut largest = (0_i64, 0_u64, 0_usize);
    let mut sea_step = Vec::new();
    for seed in args[2]
        .split(',')
        .map(|s| s.parse::<u64>().expect("a seed"))
    {
        let world = Planet::with(seed, &settings);
        let h = &world.history;
        let links = &world.sphere.links;
        let area = &world.sphere.area_km2;
        let mut prev: Option<Planet> = None;
        for t in 0..=h.last() {
            let now = world.at_step(t);
            if let Some(p) = &prev {
                n[2] += 1;
                sea_step.push((now.sea_rise_m - p.sea_rise_m).abs());
                let d_sea = now.sea_rise_m - p.sea_rise_m;
                let mut was: HashMap<u32, usize> = HashMap::new();
                for c in (0..CELLS).filter(|&c| p.continental[c] && h.shown_at[t - 1][c] != 2) {
                    was.insert(h.id_at[t - 1][c], c);
                }
                let mut seen: HashMap<u32, usize> = HashMap::new();
                // Per cell: the cause of its flip to sea (0 none, else
                // cause + 1), and whether it was sea already.
                let mut drowned = vec![0_u8; CELLS];
                let mut old_sea = vec![false; CELLS];
                let mut anew: Vec<usize> = Vec::new();
                for c in (0..CELLS).filter(|&c| now.continental[c]) {
                    let dry = now.elevation[c] > 0;
                    n[3] += u64::from(dry);
                    n[6] += 1;
                    let home = h.id_at[t][c];
                    let shown = h.shown_at[t][c];
                    let before = if shown == 0 { was.get(&home) } else { None };
                    let Some(&b) = before else {
                        // Drawn anew: hidden before, a copy, or new crust.
                        let k = match shown {
                            1 => 4,
                            2 => 5,
                            _ => 11,
                        };
                        n[k] += u64::from(dry);
                        anew.push(c);
                        continue;
                    };
                    seen.insert(home, c);
                    let far = p.near[b].0 > 1000 && now.near[c].0 > 1000;
                    n[0] += 1;
                    n[1] += u64::from(far);
                    let was_dry = p.elevation[b] > 0;
                    old_sea[c] = !was_dry;
                    if dry == was_dry {
                        continue;
                    }
                    let dh = i64::from(h.high_at[t][c]) - i64::from(h.high_at[t - 1][b]);
                    let pile = |t: usize, c: usize| (i64::from(h.thick_at[t][c]) - 350).max(0) * 10;
                    let dp = pile(t, c) - pile(t - 1, b);
                    let lip = |t: usize, c: usize| i64::from(h.margins_at[t][c] & 128 != 0) * 350;
                    let dl = lip(t, c) - lip(t - 1, b);
                    let why = h.cause_at[t][c];
                    let known: i64 = why.iter().map(|&w| i64::from(w)).sum();
                    let mut parts = vec![
                        (-d_sea, 0),
                        (dp, if dp > 0 { 9 } else { 10 }),
                        (dl, 11),
                        (dh - known, 12),
                    ];
                    parts.extend((0..8).map(|k| (i64::from(why[k]), k + 1)));
                    let sign = if dry { 1 } else { -1 };
                    let &(_, cause) = parts
                        .iter()
                        .max_by_key(|x| (x.0 * sign, std::cmp::Reverse(x.1)))
                        .expect("parts");
                    flips[cause] += 1;
                    far_flips[cause] += u64::from(far);
                    if !dry {
                        to_sea[cause] += 1;
                        drowned[c] = cause as u8 + 1;
                    }
                }
                // A cell drawn anew: the same place of its continent at the
                // step before, found from a followed neighbour.
                let prev_of: HashMap<usize, usize> =
                    seen.iter().map(|(h, &c)| (c, was[h])).collect();
                let v = &world.sphere.centre;
                for &c in &anew {
                    let dry = now.elevation[c] > 0;
                    let place = links[c]
                        .iter()
                        .filter_map(|&(m, _)| {
                            let m = m as usize;
                            prev_of.get(&m).map(|&b| {
                                world.sphere.cell_of(normalize(
                                    [0, 1, 2].map(|i| v[b][i] + v[c][i] - v[m][i]),
                                ))
                            })
                        })
                        .next()
                        .unwrap_or(c);
                    let was_dry = p.elevation[place] > 0;
                    old_sea[c] = !was_dry;
                    n[0] += 1;
                    let far = now.near[c].0 > 1000;
                    n[1] += u64::from(far);
                    if dry != was_dry {
                        let cause = if h.shown_at[t][c] == 0 { 14 } else { 13 };
                        flips[cause] += 1;
                        far_flips[cause] += u64::from(far);
                        if !dry {
                            to_sea[cause] += 1;
                            drowned[c] = cause as u8 + 1;
                        }
                    }
                }
                // Dry crust no longer drawn, and ocean floor in its place.
                let next_of: HashMap<usize, usize> =
                    prev_of.iter().map(|(&c, &b)| (b, c)).collect();
                for (&home, &b) in &was {
                    if p.elevation[b] > 0 && !seen.contains_key(&home) {
                        n[7] += 1;
                        let place = links[b]
                            .iter()
                            .filter_map(|&(m, _)| {
                                let m = m as usize;
                                next_of.get(&m).map(|&c| {
                                    world.sphere.cell_of(normalize(
                                        [0, 1, 2].map(|i| v[c][i] + v[b][i] - v[m][i]),
                                    ))
                                })
                            })
                            .next();
                        if let Some(c) = place
                            && h.class[t][c] == OCEAN
                        {
                            flips[15] += 1;
                            to_sea[15] += 1;
                            far_flips[15] += u64::from(p.near[b].0 > 1000);
                            if drowned[c] == 0 {
                                drowned[c] = 16;
                            }
                        }
                    }
                }
                n[8] += u64::from(h.hidden_at[t][0]);
                n[9] += u64::from(h.hidden_at[t][1]);
                n[10] += u64::from(h.hidden_at[t][2]);
                // Patches of new sea.
                let ocean: Vec<(usize, u32)> = (0..CELLS)
                    .filter(|&c| h.class[t][c] == OCEAN)
                    .map(|c| (c, 0))
                    .collect();
                let (from_ocean, _) = world.sphere.spread(&ocean, |_, _| true, 2_000);
                let mut done = vec![false; CELLS];
                for c0 in 0..CELLS {
                    if drowned[c0] == 0 || done[c0] {
                        continue;
                    }
                    let (mut stack, mut cells) = (vec![c0], Vec::new());
                    done[c0] = true;
                    while let Some(c) = stack.pop() {
                        cells.push(c);
                        for &(m, _) in &links[c] {
                            let m = m as usize;
                            if drowned[m] != 0 && !done[m] {
                                done[m] = true;
                                stack.push(m);
                            }
                        }
                    }
                    let km2: i64 = cells.iter().map(|&c| i64::from(area[c])).sum();
                    let inland = cells.iter().all(|&c| from_ocean[c] >= INLAND_KM);
                    let from_coast = cells.iter().any(|&c| {
                        links[c].iter().any(|&(m, _)| {
                            let m = m as usize;
                            drowned[m] == 0
                                && (!now.continental[m] || (now.elevation[m] <= 0 && old_sea[m]))
                        })
                    });
                    let mut votes = [0_u32; 17];
                    for &c in &cells {
                        votes[drowned[c] as usize] += 1;
                    }
                    let cause = (1..17).max_by_key(|&k| (votes[k], k)).expect("votes") - 1;
                    let size = SIZES.iter().position(|s| km2 < s.1).expect("a size");
                    let kind = if !inland {
                        2
                    } else if from_coast {
                        1
                    } else {
                        0
                    };
                    patches[kind][size] += 1;
                    if km2 >= 150_000 && args.len() > 3 {
                        let x = v[cells[0]];
                        let mut shares: Vec<String> = (1..17)
                            .filter(|&k| votes[k] > 0)
                            .map(|k| format!("{} {}", NAMES[k - 1], votes[k]))
                            .collect();
                        shares.sort();
                        println!(
                            "seed {seed} step {t}: {} k km2, {} at lat {} lon {}: {}",
                            km2 / 1000,
                            ["inland popped", "inland spread", "near the ocean"][kind],
                            planet::latitude(x) / 10,
                            planet::longitude(x) / 10,
                            shares.join(", ")
                        );
                    }
                    if kind < 2 {
                        patch_cause[kind][cause] += (km2 / 1000) as u64;
                        if kind == 0 && km2 > largest.0 {
                            largest = (km2, seed, t);
                        }
                    }
                }
            }
            prev = Some(now);
        }
    }
    let pc = |a: u64, b: u64| 100.0 * a as f64 / b.max(1) as f64;
    let steps = n[2] as f64;
    println!(
        "followed {} crust cells over {} steps; flips {:.2}% (far from edges {:.2}%), by cause, % of followed (far):",
        n[0],
        n[2],
        pc(flips.iter().sum(), n[0]),
        pc(far_flips.iter().sum(), n[1]),
    );
    let all: u64 = flips.iter().sum();
    for (k, name) in NAMES.iter().enumerate() {
        println!(
            "  {name}: {:.3} ({:.3}) = {:.0}% of flips, {:.0}% of them to sea",
            pc(flips[k], n[0]),
            pc(far_flips[k], n[1]),
            pc(flips[k], all),
            pc(to_sea[k], flips[k])
        );
    }
    println!(
        "the grid, % of land cells per step: drawn from crust hidden before {:.2}, a copy of a neighbour {:.2}, new crust {:.2}; dry crust no longer drawn {:.2}",
        pc(n[4], n[3]),
        pc(n[5], n[3]),
        pc(n[11], n[3]),
        pc(n[7], n[3]),
    );
    println!(
        "the grid, % of crust cells: bits hidden {:.2}, newly hidden per step {:.2}, cells drawn as copies {:.2}",
        pc(n[8], n[6]),
        pc(n[9], n[6]),
        pc(n[10], n[6]),
    );
    sea_step.sort_unstable();
    println!(
        "sea level change per step, m: median {}, 9th decile {}, most {}",
        sea_step[sea_step.len() / 2],
        sea_step[sea_step.len() * 9 / 10],
        sea_step[sea_step.len() - 1]
    );
    for (k, name) in [
        "inland, popped among land",
        "inland, spread from a sea",
        "near the ocean (under 300 km)",
    ]
    .iter()
    .enumerate()
    {
        let line: Vec<String> = SIZES
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{} {:.2}", s.0, patches[k][i] as f64 / steps))
            .collect();
        println!("new sea patches per step, {name}: {}", line.join("; "));
    }
    for (k, name) in ["popped", "spread from a sea"].iter().enumerate() {
        let total: u64 = patch_cause[k].iter().sum();
        let mut line: Vec<(u64, String)> = (0..16)
            .filter(|&c| patch_cause[k][c] > 0)
            .map(|c| {
                (
                    patch_cause[k][c],
                    format!("{} {:.0}%", NAMES[c], pc(patch_cause[k][c], total)),
                )
            })
            .collect();
        line.sort_by(|a, b| b.0.cmp(&a.0));
        let line: Vec<String> = line.into_iter().map(|x| x.1).collect();
        println!(
            "inland new sea {name}: {:.0} k km2 per step, by area: {}",
            total as f64 / steps,
            line.join("; ")
        );
    }
    println!(
        "largest inland sea popped in one step: {} k km2 (seed {}, step {})",
        largest.0 / 1000,
        largest.1,
        largest.2
    );
}
