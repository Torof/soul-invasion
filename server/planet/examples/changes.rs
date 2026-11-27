//! A probe of what changes the lands from one step to the next: following
//! each cell of continental crust by its home, what made it flip between
//! dry and under the sea; how much crust the grid hides or shows again;
//! the sea's pinholes in the continents; and how fast the margins build
//! out.
//!
//! ```text
//! cargo run --release -p planet --example changes -- settings.ron 14,15
//! ```

#![allow(
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::needless_range_loop,
    reason = "a measuring tool: one long count, printed as rounded shares"
)]

use planet::history::{ACCRETED, ACTIVE, Margin, OCEAN, RIFT};
use planet::{CELLS, Planet, Settings};
use std::collections::HashMap;

const CAUSES: [&str; 9] = [
    "the sea alone",
    "a pile rose",
    "a pile wore down",
    "a terrane docked",
    "an arc built (active margin)",
    "rose otherwise (plume, infill, levelling)",
    "a rift stretched it or it cooled",
    "sank a little (levelling, wear)",
    "its grain changed (a redrawn cell)",
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&args[1]).expect("a settings file");
    let settings: Settings = ron::from_str(&text).expect("settings");
    let mut flips = [0_u64; 9];
    // Followed; dry cells; dry homes hidden; dry homes new; pinholes;
    // crust cells born; coast cells (sea beside crust); copies; steps.
    let mut n = [0_u64; 9];
    let mut built = 0_i64;
    for seed in args[2]
        .split(',')
        .map(|s| s.parse::<u64>().expect("a seed"))
    {
        let world = Planet::with(seed, &settings);
        let h = &world.history;
        let links = &world.sphere.links;
        built += h.built_km2.iter().sum::<i64>();
        let mut prev: Option<Planet> = None;
        for t in 0..=h.last() {
            let now = world.at_step(t);
            if let Some(p) = &prev {
                n[8] += 1;
                let mut was: HashMap<u32, usize> = HashMap::new();
                for c in (0..CELLS).filter(|&c| p.continental[c]) {
                    was.insert(h.home_at[t - 1][c], c);
                }
                let mut seen: HashMap<u32, usize> = HashMap::new();
                for c in (0..CELLS).filter(|&c| now.continental[c]) {
                    let home = h.home_at[t][c];
                    if seen.insert(home, c).is_some() {
                        n[7] += 1;
                    }
                    let dry = now.elevation[c] > 0;
                    n[1] += u64::from(dry);
                    let Some(&b) = was.get(&home) else {
                        n[5] += 1;
                        n[3] += u64::from(dry);
                        continue;
                    };
                    n[0] += 1;
                    if dry == (p.elevation[b] > 0) {
                        continue;
                    }
                    let dh = i64::from(h.high_at[t][c]) - i64::from(h.high_at[t - 1][b]);
                    let dt = i64::from(h.thick_at[t][c]) - i64::from(h.thick_at[t - 1][b]);
                    let raw = i64::from(now.elevation[c]) + now.sea_rise_m
                        - i64::from(p.elevation[b])
                        - p.sea_rise_m;
                    let (m0, m1) = (h.margins_at[t - 1][b], h.margins_at[t][c]);
                    let arc = h.margin[t][c] == Margin::Active as u8
                        || m1 & ACTIVE != 0
                            && links[c]
                                .iter()
                                .any(|&(k, _)| h.margin[t][k as usize] == Margin::Active as u8);
                    let cause = if raw == 0 {
                        0
                    } else if dt > 0 {
                        1
                    } else if dt < 0 {
                        2
                    } else if dh == 0 {
                        8
                    } else if m1 & ACCRETED != 0 && m0 & ACCRETED == 0 {
                        3
                    } else if dh > 0 && arc {
                        4
                    } else if dh > 0 {
                        5
                    } else if dh <= -30 || m1 & RIFT != 0 && m0 & RIFT == 0 {
                        6
                    } else {
                        7
                    };
                    flips[cause] += 1;
                }
                for (&home, &b) in &was {
                    if p.elevation[b] > 0 && !seen.contains_key(&home) {
                        n[2] += 1;
                    }
                }
                for c in 0..CELLS {
                    if h.class[t][c] != OCEAN {
                        continue;
                    }
                    let land = links[c]
                        .iter()
                        .filter(|&&(k, _)| h.class[t][k as usize] != OCEAN)
                        .count();
                    n[6] += u64::from(land > 0);
                    n[4] += u64::from(land + 1 >= links[c].len());
                }
            }
            prev = Some(now);
        }
    }
    let pc = |a: u64, b: u64| 100.0 * a as f64 / b.max(1) as f64;
    println!(
        "followed {} crust cells; flips {:.2}%, by cause, % of followed:",
        n[0],
        pc(flips.iter().sum(), n[0])
    );
    for (k, name) in CAUSES.iter().enumerate() {
        println!("  {name}: {:.2}", pc(flips[k], n[0]));
    }
    println!(
        "dry crust per step, % of dry cells: hidden by the grid or gone {:.2}, shown new {:.2}; crust cells drawn twice {:.2}% of crust",
        pc(n[2], n[1]),
        pc(n[3], n[1]),
        pc(n[7], n[0]),
    );
    println!(
        "margins built of silt: {:.0} k km2 per step, {:.1} cells, {:.2} km of advance per step along the coast",
        built as f64 / n[8] as f64 / 1000.0,
        built as f64 / n[8] as f64 / 8500.0,
        92.0 * (built as f64 / 8500.0) / n[6].max(1) as f64,
    );
    println!(
        "per step: sea pinholes in the crust {:.1}; crust cells born {:.1} for {:.0} sea cells beside crust ({:.1}% of the coast gains a cell of ~92 km: {:.1} km per step)",
        n[4] as f64 / n[8] as f64,
        n[5] as f64 / n[8] as f64,
        n[6] as f64 / n[8] as f64,
        pc(n[5], n[6]),
        92.0 * n[5] as f64 / n[6].max(1) as f64,
    );
}
