//! A probe of the relief's steadiness: following each cell of continental
//! crust by its home from step to step, how much of it flips between dry
//! and under the sea, overall and far from the plates' edges, and what
//! flipped it (the sea alone, a pile of crust, or its carried height).
//!
//! ```text
//! cargo run --release -p planet --example probe -- settings.ron 8,15,3
//! ```

#![allow(
    clippy::print_stdout,
    clippy::cast_precision_loss,
    reason = "a measuring tool: it prints its figures, as rounded shares"
)]

use planet::{Planet, Settings};
use std::collections::HashMap;

/// A home's state at a step: dry, far from its plate's edges, its height
/// before the sea, and its crust's thickness.
type Seen = (bool, bool, i64, u16);

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&args[1]).expect("a settings file");
    let settings: Settings = ron::from_str(&text).expect("settings");
    // Followed, flipped; far from edges: followed, flipped; new crust;
    // flips by the sea alone, by a pile, by the carried height.
    let mut tot = [0_u64; 11];
    for seed in args[2]
        .split(',')
        .map(|s| s.parse::<u64>().expect("a seed"))
    {
        let world = Planet::with(seed, &settings);
        let cells = world.elevation.len();
        let mut prev: Option<Planet> = None;
        let (mut line, mut lands, mut step_most) = (String::new(), Vec::new(), 0);
        for t in 0..=world.history.last() {
            let now = world.at_step(t);
            let land = now.elevation.iter().filter(|&&e| e > 0).count() * 1000 / cells;
            let crust = now.continental.iter().filter(|&&k| k).count() * 1000 / cells;
            lands.push(land);
            line.push_str(
                &[
                    land.to_string(),
                    crust.to_string(),
                    now.sea_rise_m.to_string(),
                ]
                .join("/"),
            );
            line.push(' ');
            if let Some(p) = &prev {
                step_most = step_most.max((now.sea_rise_m - p.sea_rise_m).abs());
                let mut was: HashMap<u32, Seen> = HashMap::new();
                for c in (0..cells).filter(|&c| p.continental[c]) {
                    let raw = i64::from(p.elevation[c]) + p.sea_rise_m;
                    let seen = (
                        p.elevation[c] > 0,
                        p.near[c].0 > 1000,
                        raw,
                        p.history.thick_at[t - 1][c],
                    );
                    was.insert(p.history.home_at[t - 1][c], seen);
                }
                for c in (0..cells).filter(|&c| now.continental[c]) {
                    let Some(&(dry, far, raw, thick)) = was.get(&now.history.home_at[t][c]) else {
                        tot[4] += 1;
                        continue;
                    };
                    let flip = u64::from(dry != (now.elevation[c] > 0));
                    tot[0] += 1;
                    tot[1] += flip;
                    if far && now.near[c].0 > 1000 {
                        tot[2] += 1;
                        tot[3] += flip;
                    }
                    let cause = if raw == i64::from(now.elevation[c]) + now.sea_rise_m {
                        5
                    } else if thick != now.history.thick_at[t][c] {
                        6
                    } else {
                        7
                    };
                    tot[cause] += flip;
                    if far && now.near[c].0 > 1000 {
                        tot[cause + 3] += flip;
                    }
                }
            }
            prev = Some(now);
        }
        println!("seed {seed} land/crust per mille/sea rise m, step by step: {line}");
        println!(
            "seed {seed} land per mille: min {} max {} today {}; largest sea change in a step {step_most} m",
            lands.iter().min().expect("steps"),
            lands.iter().max().expect("steps"),
            lands.last().expect("steps"),
        );
    }
    let pc = |a: u64, b: u64| 100.0 * a as f64 / b.max(1) as f64;
    println!(
        "crust cells followed {} flips {:.2}% | far from edges {} flips {:.2}% | new crust cells {}",
        tot[0],
        pc(tot[1], tot[0]),
        tot[2],
        pc(tot[3], tot[2]),
        tot[4],
    );
    println!(
        "flips by cause, % of followed: the sea alone {:.2}, a pile changed {:.2}, the carried height changed {:.2}",
        pc(tot[5], tot[0]),
        pc(tot[6], tot[0]),
        pc(tot[7], tot[0]),
    );
    println!(
        "far from edges, % of followed there: the sea alone {:.2}, a pile changed {:.2}, the carried height changed {:.2}",
        pc(tot[8], tot[2]),
        pc(tot[9], tot[2]),
        pc(tot[10], tot[2]),
    );
}
