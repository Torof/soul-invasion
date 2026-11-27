//! A probe of the pieces of continent (version 10): are they rigid? For
//! each piece of at least 30 cells and each span of steps, the cells it
//! keeps, gains and loses; and for the cells kept, how far they are, turned
//! back, from where the same cells were at the first end (km: 0 is the
//! same outline).
//!
//! ```text
//! cargo run --release -p planet --example rigid -- settings.ron 14,9
//! ```

#![allow(
    clippy::print_stdout,
    clippy::cast_precision_loss,
    reason = "a measuring tool"
)]

use planet::history::Piece;
use planet::{Planet, Settings, V3, dot, km_between, normalize};
use std::collections::HashMap;

fn back(p: &Piece, x: V3) -> V3 {
    let m = p.turn;
    normalize([0, 1, 2].map(|j| dot([m[0][j], m[1][j], m[2][j]], x)))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&args[1]).expect("a settings file");
    let settings: Settings = ron::from_str(&text).expect("settings");
    for span in [1_usize, 5, 20] {
        // Pieces, untouched pieces, cells at the first end, kept, lost,
        // gained, cells moved in their lattice, worst offset km.
        let mut n = [0_u64; 8];
        let mut largest = 0;
        for seed in args[2]
            .split(',')
            .map(|s| s.parse::<u64>().expect("a seed"))
        {
            let world = Planet::with(seed, &settings);
            let h = &world.history;
            let sphere = &world.sphere;
            for t in 0..=h.last().saturating_sub(span) {
                for a in h.pieces_at[t].iter().filter(|a| a.tiles.len() >= 30) {
                    let Some(b) = h.pieces_at[t + span].iter().find(|b| b.frame == a.frame) else {
                        continue;
                    };
                    n[0] += 1;
                    n[2] += a.tiles.len() as u64;
                    let then: HashMap<u32, usize> =
                        a.tiles.iter().enumerate().map(|(i, x)| (x.id, i)).collect();
                    let mut kept = 0;
                    for (j, x) in b.tiles.iter().enumerate() {
                        let Some(&i) = then.get(&x.id) else {
                            n[5] += 1;
                            continue;
                        };
                        kept += 1;
                        n[6] += u64::from(a.tiles[i].at != x.at);
                        // Both ends turned back into the piece's lattice.
                        let off =
                            km_between(back(a, a.place(sphere, i)), back(b, b.place(sphere, j)));
                        n[7] = n[7].max(off.unsigned_abs());
                    }
                    n[3] += kept;
                    n[4] += a.tiles.len() as u64 - kept;
                    if kept == a.tiles.len() as u64 && b.tiles.len() == a.tiles.len() {
                        n[1] += 1;
                        largest = largest.max(a.tiles.len());
                    }
                }
            }
        }
        println!(
            "over {span} steps: {} pieces of 30 cells or more, {} untouched (largest {largest} cells); of their cells {:.2}% kept, {:.2}% gone (a collision, or cut off to another piece), {:.2}% gained (silt, docking); kept cells that changed place in their lattice: {}; worst offset of a kept cell, both ends turned back: {} km",
            n[0],
            n[1],
            100.0 * n[3] as f64 / n[2].max(1) as f64,
            100.0 * n[4] as f64 / n[2].max(1) as f64,
            100.0 * n[5] as f64 / n[2].max(1) as f64,
            n[6],
            n[7]
        );
    }
}
