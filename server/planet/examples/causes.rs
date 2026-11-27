//! A probe of what changes the lands' shapes from one step to the next,
//! cause by cause (version 10: read in the pieces of continent
//! themselves). Every cell of continental crust is followed by its name
//! in its piece: those that flip between dry and under the sea, and why;
//! the one-cell lakes and islets that appear or vanish; every patch of
//! sea appearing inside a continent, by size; and, apart, what the
//! planet's grid (a view of the pieces) draws anew.
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
    clippy::similar_names,
    clippy::type_complexity,
    clippy::bool_to_int_with_if,
    clippy::unnecessary_sort_by,
    reason = "a measuring tool: one long count, printed as rounded shares"
)]

use planet::history::OCEAN;
use planet::{CELLS, Planet, Settings};
use std::collections::{HashMap, HashSet};

const NAMES: [&str; 15] = [
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
    "it came out from under another piece",
    "new crust (silt, an arc's island)",
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

/// A cell of crust at a step: dry, far from the plates' edges, under
/// another piece; its carried height, pile and flood basalt; its piece
/// and cell.
#[derive(Clone, Copy)]
struct Seen {
    dry: bool,
    far: bool,
    under: bool,
    high: i64,
    pile: i64,
    lip: i64,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&args[1]).expect("a settings file");
    let settings: Settings = ron::from_str(&text).expect("settings");
    let mut flips = [0_u64; 15];
    let mut far_flips = [0_u64; 15];
    let mut to_sea = [0_u64; 15];
    // Followed, far followed, steps, land cells, dry cells run under
    // another piece or piled, crust cells, grid land cells, grid land
    // cells drawn from crust not drawn before, pieces' cells the grid
    // does not draw, of them drawn before, crust volume worst low and
    // high (per mille of the start).
    let mut n = [0_u64; 10];
    let mut volume = (1000_i64, 1000_i64);
    let mut area_low = 1000_i64;
    // One-cell lakes and islets: present, appeared, vanished.
    let mut lone = [[0_u64; 3]; 2];
    let mut patches = [[0_u64; 4]; 3];
    let mut patch_cause = [[0_u64; 15]; 2];
    let mut largest = (0_i64, 0_u64, 0_usize);
    let mut sea_step = Vec::new();
    for seed in args[2]
        .split(',')
        .map(|s| s.parse::<u64>().expect("a seed"))
    {
        let world = Planet::with(seed, &settings);
        let h = &world.history;
        let links = &world.sphere.links;
        let mut prev: Option<(Planet, HashMap<u32, Seen>, [HashSet<u32>; 2])> = None;
        let mut volume0 = 0_i64;
        for t in 0..=h.last() {
            let now = world.at_step(t);
            let pieces = now.pieces();
            let vol: i64 = pieces
                .iter()
                .flat_map(|p| &p.tiles)
                .map(|x| i64::from(x.thick) * i64::from(x.area_km2))
                .sum();
            if t == 0 {
                volume0 = vol;
            }
            let share = vol * 1000 / volume0;
            volume = (volume.0.min(share), volume.1.max(share));
            area_low = area_low.min(h.continental_km2[t] * 1000 / h.continental_km2[0]);
            let mut seen: HashMap<u32, Seen> = HashMap::new();
            let mut lakes = [HashSet::new(), HashSet::new()];
            // Per piece, per cell: the cause of its flip to sea (0 none,
            // else cause + 1), and whether it was sea already.
            let mut drowned: Vec<Vec<u8>> = Vec::new();
            let mut old_sea: Vec<Vec<bool>> = Vec::new();
            let d_sea = prev.as_ref().map_or(0, |p| now.sea_rise_m - p.0.sea_rise_m);
            if prev.is_some() {
                n[2] += 1;
                sea_step.push(d_sea.abs());
            }
            for piece in pieces {
                let place: HashMap<u32, usize> = piece
                    .tiles
                    .iter()
                    .enumerate()
                    .map(|(i, x)| (x.at, i))
                    .collect();
                let dry: Vec<bool> = piece
                    .tiles
                    .iter()
                    .map(|x| !x.under && now.tile_height(x) > 0)
                    .collect();
                let mut drowned_here = vec![0_u8; piece.tiles.len()];
                let mut old_sea_here = vec![false; piece.tiles.len()];
                for (i, x) in piece.tiles.iter().enumerate() {
                    let cell = world.sphere.cell_of(piece.place(&world.sphere, i));
                    let me = Seen {
                        dry: dry[i],
                        far: now.near[cell].0 > 1000,
                        under: x.under,
                        high: i64::from(x.high),
                        pile: (i64::from(x.thick) - 350).max(0) * 10,
                        lip: i64::from(x.margins & 128 != 0) * 350,
                    };
                    seen.insert(x.id, me);
                    if x.under {
                        continue;
                    }
                    // Its neighbours in its own lattice.
                    let around: Vec<Option<usize>> = links[x.at as usize]
                        .iter()
                        .map(|&(m, _)| place.get(&m).copied())
                        .collect();
                    let dry_around = around.iter().flatten().filter(|&&j| dry[j]).count();
                    if !dry[i] && dry_around == around.len() {
                        lakes[0].insert(x.id);
                    }
                    if dry[i] && dry_around == 0 {
                        lakes[1].insert(x.id);
                    }
                    let Some((_, was, _)) = &prev else {
                        continue;
                    };
                    n[5] += 1;
                    n[3] += u64::from(dry[i]);
                    let (cause, was_dry, far) = match was.get(&x.id) {
                        None => (14, false, me.far),
                        Some(b) if b.under => (13, false, me.far),
                        Some(b) => {
                            let known: i64 = x.why.iter().map(|&w| i64::from(w)).sum();
                            let dp = me.pile - b.pile;
                            let mut parts = vec![
                                (-d_sea, 0),
                                (dp, if dp > 0 { 9 } else { 10 }),
                                (me.lip - b.lip, 11),
                                (me.high - b.high - known, 12),
                            ];
                            parts.extend((0..8).map(|k| (i64::from(x.why[k]), k + 1)));
                            let sign = if dry[i] { 1 } else { -1 };
                            let &(_, cause) = parts
                                .iter()
                                .max_by_key(|p| (p.0 * sign, std::cmp::Reverse(p.1)))
                                .expect("parts");
                            (cause, b.dry, me.far && b.far)
                        }
                    };
                    n[0] += 1;
                    n[1] += u64::from(far);
                    old_sea_here[i] = !was_dry;
                    if dry[i] != was_dry {
                        flips[cause] += 1;
                        far_flips[cause] += u64::from(far);
                        if !dry[i] {
                            to_sea[cause] += 1;
                            drowned_here[i] = cause as u8 + 1;
                        }
                    }
                }
                drowned.push(drowned_here);
                old_sea.push(old_sea_here);
            }
            if let Some((_, was, lone_was)) = &prev {
                for (id, b) in was {
                    if b.dry && seen.get(id).is_none_or(|x| x.under) {
                        n[4] += 1;
                    }
                }
                for k in 0..2 {
                    lone[k][1] += lakes[k].difference(&lone_was[k]).count() as u64;
                    lone[k][2] += lone_was[k].difference(&lakes[k]).count() as u64;
                }
                // The grid's view.
                for c in 0..CELLS {
                    if now.continental[c] && now.elevation[c] > 0 {
                        n[6] += 1;
                        n[7] += u64::from(h.shown_at[t][c] == 1);
                    }
                }
                n[8] += u64::from(h.hidden_at[t][0]);
                n[9] += u64::from(h.hidden_at[t][1]);
                // Patches of new sea, in each piece's own lattice.
                let ocean: Vec<(usize, u32)> = (0..CELLS)
                    .filter(|&c| h.class[t][c] == OCEAN)
                    .map(|c| (c, 0))
                    .collect();
                let (from_ocean, _) = world.sphere.spread(&ocean, |_, _| true, 2_000);
                for (k, piece) in pieces.iter().enumerate() {
                    let place: HashMap<u32, usize> = piece
                        .tiles
                        .iter()
                        .enumerate()
                        .map(|(i, x)| (x.at, i))
                        .collect();
                    let mut done = vec![false; piece.tiles.len()];
                    for i0 in 0..piece.tiles.len() {
                        if drowned[k][i0] == 0 || done[i0] {
                            continue;
                        }
                        let (mut stack, mut cells) = (vec![i0], Vec::new());
                        done[i0] = true;
                        let mut from_coast = false;
                        while let Some(i) = stack.pop() {
                            cells.push(i);
                            for &(m, _) in &links[piece.tiles[i].at as usize] {
                                match place.get(&m) {
                                    None => from_coast = true,
                                    Some(&j) if drowned[k][j] != 0 => {
                                        if !done[j] {
                                            done[j] = true;
                                            stack.push(j);
                                        }
                                    }
                                    Some(&j) => {
                                        let x = &piece.tiles[j];
                                        from_coast |=
                                            !x.under && now.tile_height(x) <= 0 && old_sea[k][j];
                                    }
                                }
                            }
                        }
                        let km2: i64 = cells
                            .iter()
                            .map(|&i| i64::from(piece.tiles[i].area_km2))
                            .sum();
                        let inland = cells.iter().all(|&i| {
                            from_ocean[world.sphere.cell_of(piece.place(&world.sphere, i))]
                                >= INLAND_KM
                        });
                        let mut votes = [0_u32; 16];
                        for &i in &cells {
                            votes[drowned[k][i] as usize] += 1;
                        }
                        let cause = (1..16).max_by_key(|&c| (votes[c], c)).expect("votes") - 1;
                        let size = SIZES.iter().position(|s| km2 < s.1).expect("a size");
                        let kind = if !inland {
                            2
                        } else if from_coast {
                            1
                        } else {
                            0
                        };
                        patches[kind][size] += 1;
                        if kind < 2 {
                            patch_cause[kind][cause] += (km2 / 1000) as u64;
                            if kind == 0 && km2 > largest.0 {
                                largest = (km2, seed, t);
                            }
                        }
                    }
                }
            }
            for k in 0..2 {
                lone[k][0] += lakes[k].len() as u64;
            }
            prev = Some((now, seen, lakes));
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
        "dry crust run under another piece or piled, % of land cells per step: {:.2}",
        pc(n[4], n[3])
    );
    println!(
        "in the pieces, per step: one-cell lakes {:.1} (appeared {:.1}, vanished {:.1}); one-cell islets {:.1} (appeared {:.1}, vanished {:.1})",
        lone[0][0] as f64 / (steps + 1.0),
        lone[0][1] as f64 / steps,
        lone[0][2] as f64 / steps,
        lone[1][0] as f64 / (steps + 1.0),
        lone[1][1] as f64 / steps,
        lone[1][2] as f64 / steps,
    );
    println!(
        "the grid's view (no crust is kept on it): % of its land cells drawn from crust it did not draw before {:.2}; % of the pieces' cells it does not draw {:.2}, newly per step {:.2}",
        pc(n[7], n[6]),
        pc(n[8], n[5]),
        pc(n[9], n[5]),
    );
    println!(
        "continental crust, per mille of the start: volume between {} and {}; area not under another piece, lowest {}",
        volume.0, volume.1, area_low
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
        let mut line: Vec<(u64, String)> = (0..15)
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
