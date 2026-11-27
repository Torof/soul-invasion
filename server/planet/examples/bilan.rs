//! The planet's balance sheet: one command, one stable table, written as
//! a JSON file per version of the model. Every later round of work on the
//! plates adds a row here, never a new program.
//!
//! ```text
//! cargo run --release -p planet --example bilan -- \
//!     --settings monde-defaut.ron --seeds 1-30 --version v10 --out bilan-v10.json
//! ```
//!
//! The rows: the land/sea flips per step (all, far from the plates'
//! edges) and their share by cause; whether the rigid pieces of
//! continent stay unchanged; the plates by size class and the
//! short-lived ones; the continental crust kept (area, volume); the
//! ground below sea level inside the continents (linked to the ocean or
//! not) and what makes it change; whether a continent moves with its
//! plate; the rifts; the seconds a planet takes.

#![allow(
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::needless_range_loop,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::struct_excessive_bools,
    reason = "a measuring tool: one long count, written as rounded shares"
)]

use planet::history::{self, MAXP, OCEAN, Piece};
use planet::{CELLS, Planet, Settings, V3, dot, km_between, normalize};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

const CAUSES: [&str; 16] = [
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
    "height changed with no event",
    "it came out from under another piece",
    "new crust (silt, an arc's island)",
    "a hollow's link to the ocean changed",
];
const SWITCHES: [&str; 9] = [
    "unnamed",
    "weld after a collision",
    "block escaping a collision",
    "terrane or arc docking",
    "lone cell on the grid's view",
    "large part cut off: its own plate, same motion",
    "small part cut off, taken by the plate beside",
    "crumb or enclosed small plate taken whole",
    "rift",
];
/// Patches of water, km²: small, medium, large, huge.
const SIZES: [(&str, i64); 4] = [
    ("under 50 k km2", 50_000),
    ("50-200 k km2", 200_000),
    ("200 k-1 M km2", 1_000_000),
    ("over 1 M km2", i64::MAX),
];
/// Plates, km²: micro, small, medium, large (the Earth: about 7 large,
/// 8 medium).
const PLATE_KM2: [i64; 3] = [500_000, 2_000_000, 20_000_000];
const PLATE_NAMES: [&str; 4] = [
    "micro under 0.5 M km2",
    "small 0.5-2 M",
    "medium 2-20 M",
    "large over 20 M",
];
/// Water is inland when no nearer than this to the ocean's floor, km.
const INLAND_KM: u32 = 300;
/// Far from the plates' edges, km.
const FAR_KM: u32 = 1000;
/// A cell of crust slides on its plate when it ends a step farther than
/// this from where its plate's turn carries it, km.
const SLIDE_KM: i64 = 2;

/// A cell of crust at a step.
#[derive(Clone, Copy)]
struct Seen {
    /// Dry as the model paints it; below the sea's level.
    dry: bool,
    low: bool,
    far: bool,
    under: bool,
    high: i64,
    pile: i64,
    lip: i64,
    pos: V3,
    own: u8,
    /// 0 dry, 1 hollow with no path to the ocean, 2 inland water linked
    /// to the ocean, 3 water near the ocean.
    water: u8,
    /// Its hollow's size class (when `water` is 1).
    size: u8,
}

/// Whether the model paints this cell of crust dry. Version 10: any
/// ground above the sea's level, and no other.
fn painted_dry(now: &Planet, tile: &history::Tile, _cell: usize) -> bool {
    !tile.under && now.tile_height(tile) > 0
}

fn back(p: &Piece, x: V3) -> V3 {
    let m = p.turn;
    normalize([0, 1, 2].map(|j| dot([m[0][j], m[1][j], m[2][j]], x)))
}

#[derive(Default)]
struct Count {
    steps: u64,
    planets: u64,
    followed: u64,
    far_followed: u64,
    flips: [u64; 16],
    far_flips: [u64; 16],
    // Rigid pieces of 30 cells or more: followed and untouched over 1, 5
    // and 20 steps; kept cells moved in their lattice, worst offset km.
    rigid: [[u64; 2]; 3],
    rigid_moved: u64,
    rigid_off: u64,
    plates_today: [u64; 4],
    plates_through: [u64; 4],
    lives: [u64; 4],
    plates_start: u64,
    // Hollows (no path to the ocean) and inland water linked to it.
    hollows: [u64; 4],
    hollow_km2: u64,
    hollow_painted_sea_km2: u64,
    linked_km2: u64,
    // Ground gone below the sea's level, and back above it, km², by
    // where (hollow under 200 k km², larger hollow, linked inland water)
    // and by cause.
    sunk: [[u64; 16]; 3],
    risen: [[u64; 16]; 3],
    // A hollow reached by the sea, and inland water cut from it, km².
    reached: u64,
    cut: u64,
    // Cells of crust followed for their motion, those that slid, those
    // that changed plate by cause, of them those then moving otherwise.
    moved: u64,
    slid: u64,
    slid_worst_km: i64,
    switched: [u64; 9],
    rifts: [u64; 3],
    rift_my: Vec<i64>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let settings: Settings = arg("--settings").map_or_else(Settings::default, |path| {
        let text = std::fs::read_to_string(&path).expect("a settings file");
        ron::from_str(&text).expect("settings")
    });
    let seeds_arg = arg("--seeds").unwrap_or_else(|| "1-30".to_owned());
    let seeds: Vec<u64> = seeds_arg
        .split(',')
        .flat_map(|part| {
            let ends: Vec<u64> = part
                .split('-')
                .map(|s| s.parse().expect("a seed"))
                .collect();
            ends[0]..=ends[ends.len() - 1]
        })
        .collect();
    let version = arg("--version").unwrap_or_else(|| "work".to_owned());
    let mut n = Count::default();
    let mut volume = (1000_i64, 1000_i64);
    let mut area = (1000_i64, 1000_i64);
    let mut seconds = Vec::new();
    for &seed in &seeds {
        let clock = std::time::Instant::now();
        let world = Planet::with(seed, &settings);
        seconds.push(clock.elapsed().as_secs_f64());
        let h = &world.history;
        let sphere = &world.sphere;
        let links = &sphere.links;
        let last = h.last();
        n.planets += 1;
        n.plates_start += h.plates_at_start.1 as u64;
        let mut prev: Option<HashMap<u32, Seen>> = None;
        let mut volume0 = 0_i64;
        let mut sea_before = world.at_step(0).sea_rise_m;
        let mut first_t = [usize::MAX; MAXP];
        let mut last_t = [0_usize; MAXP];
        for t in 0..=last {
            let now = world.at_step(t);
            let pieces = now.pieces();
            // The crust kept.
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
            let share = h.continental_km2[t] * 1000 / h.continental_km2[0];
            area = (area.0.min(share), area.1.max(share));
            // The plates.
            let mut km2 = [0_i64; MAXP];
            for c in 0..CELLS {
                km2[h.plate[t][c] as usize] += i64::from(sphere.area_km2[c]);
            }
            for p in 0..MAXP {
                if km2[p] == 0 {
                    continue;
                }
                first_t[p] = first_t[p].min(t);
                last_t[p] = t;
                let class = PLATE_KM2.iter().filter(|&&k| km2[p] >= k).count();
                n.plates_through[class] += 1;
                if t == last {
                    n.plates_today[class] += 1;
                }
            }
            // The water on the continents: ground below the sea's level,
            // linked to the ocean (a path of such ground to the ocean's
            // floor) or not (a hollow).
            let low_cell: Vec<bool> = now.elevation.iter().map(|&e| e <= 0).collect();
            let floor: Vec<(usize, u32)> = (0..CELLS)
                .filter(|&c| h.class[t][c] == OCEAN)
                .map(|c| (c, 0))
                .collect();
            let (linked, _) = sphere.spread(&floor, |_, b| low_cell[b], u32::MAX - 1);
            let (from_ocean, _) = sphere.spread(&floor, |_, _| true, 2_000);
            let mut water = vec![0_u8; CELLS];
            for c in 0..CELLS {
                if h.class[t][c] != OCEAN && low_cell[c] {
                    water[c] = if linked[c] == u32::MAX {
                        1
                    } else if from_ocean[c] >= INLAND_KM {
                        2
                    } else {
                        3
                    };
                }
            }
            let mut size = vec![0_u8; CELLS];
            let mut done = vec![false; CELLS];
            for c0 in 0..CELLS {
                if water[c0] != 1 || done[c0] {
                    continue;
                }
                let (mut stack, mut cells) = (vec![c0], Vec::new());
                done[c0] = true;
                while let Some(c) = stack.pop() {
                    cells.push(c);
                    for &(m, _) in &links[c] {
                        let m = m as usize;
                        if water[m] == 1 && !done[m] {
                            done[m] = true;
                            stack.push(m);
                        }
                    }
                }
                let km2: i64 = cells.iter().map(|&c| i64::from(sphere.area_km2[c])).sum();
                let class = SIZES.iter().position(|s| km2 < s.1).expect("a size");
                n.hollows[class] += 1;
                n.hollow_km2 += km2 as u64;
                for c in cells {
                    size[c] = class as u8;
                }
            }
            for c in 0..CELLS {
                if water[c] == 2 {
                    n.linked_km2 += u64::from(sphere.area_km2[c]);
                }
            }
            // Every cell of crust, followed by its name in its piece.
            let d_sea = now.sea_rise_m - sea_before;
            sea_before = now.sea_rise_m;
            let mut drawn: HashMap<u32, usize> = HashMap::new();
            for c in 0..CELLS {
                if h.class[t][c] != OCEAN {
                    drawn.insert(h.id_at[t][c], c);
                }
            }
            let mut seen: HashMap<u32, Seen> = HashMap::new();
            if prev.is_some() {
                n.steps += 1;
            }
            for piece in pieces {
                for (i, x) in piece.tiles.iter().enumerate() {
                    let pos = piece.place(sphere, i);
                    let cell = sphere.cell_of(pos);
                    let shown = drawn.get(&x.id).copied();
                    let me = Seen {
                        dry: painted_dry(&now, x, cell),
                        low: !x.under && now.tile_height(x) <= 0,
                        far: now.near[cell].0 > FAR_KM,
                        under: x.under,
                        high: i64::from(x.high),
                        pile: (i64::from(x.thick) - 350).max(0) * 10,
                        lip: i64::from(x.margins & history::LIP != 0) * 350,
                        pos,
                        own: x.own,
                        water: shown.map_or(0, |c| water[c]),
                        size: shown.map_or(0, |c| size[c]),
                    };
                    seen.insert(x.id, me);
                    let Some(was) = &prev else {
                        continue;
                    };
                    // Does it move with its plate?
                    if let Some(b) = was.get(&x.id) {
                        n.moved += 1;
                        let with = |p: u8| {
                            let w = h.turned_at[t][p as usize];
                            km_between(history::carried(w, settings.step_my, b.pos), pos)
                        };
                        let off = with(b.own);
                        if x.switch != 0 || x.own != b.own {
                            n.switched[x.switch as usize] += 1;
                        } else if off > SLIDE_KM {
                            n.slid += 1;
                            n.slid_worst_km = n.slid_worst_km.max(off);
                        }
                    }
                    if x.under {
                        continue;
                    }
                    let (cause, b) = match was.get(&x.id) {
                        None => (14, None),
                        Some(b) if b.under => (13, None),
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
                            let sign = if me.low { -1 } else { 1 };
                            let &(_, cause) = parts
                                .iter()
                                .max_by_key(|p| (p.0 * sign, std::cmp::Reverse(p.1)))
                                .expect("parts");
                            (cause, Some(*b))
                        }
                    };
                    let (was_dry, was_low, far) =
                        b.map_or((false, true, me.far), |b| (b.dry, b.low, me.far && b.far));
                    n.followed += 1;
                    n.far_followed += u64::from(far);
                    if me.dry != was_dry {
                        // Painted otherwise with no change of level: the
                        // hollow's link to the ocean.
                        let cause = if me.low == was_low && b.is_some() {
                            15
                        } else {
                            cause
                        };
                        n.flips[cause] += 1;
                        n.far_flips[cause] += u64::from(far);
                    }
                    let Some(b) = b else {
                        continue;
                    };
                    let km2 = u64::from(x.area_km2);
                    let place = |w: u8, s: u8| match w {
                        1 => Some(usize::from(s >= 2)),
                        2 => Some(2),
                        _ => None,
                    };
                    if me.low && !b.low {
                        if let Some(k) = place(me.water, me.size) {
                            n.sunk[k][cause] += km2;
                        }
                    } else if !me.low && b.low {
                        if let Some(k) = place(b.water, b.size) {
                            n.risen[k][cause] += km2;
                        }
                    } else if me.low {
                        n.reached += km2 * u64::from(b.water == 1 && me.water >= 2);
                        n.cut += km2 * u64::from(b.water >= 2 && me.water == 1);
                    }
                }
            }
            for c in 0..CELLS {
                if water[c] == 1 && seen.get(&h.id_at[t][c]).is_some_and(|x| !x.dry) {
                    n.hollow_painted_sea_km2 += u64::from(sphere.area_km2[c]);
                }
            }
            prev = Some(seen);
        }
        for p in 0..MAXP {
            if first_t[p] != usize::MAX && last_t[p] < last {
                let life = last_t[p] - first_t[p] + 1;
                n.lives[[1, 3, 9].iter().filter(|&&k| life > k).count()] += 1;
            }
        }
        // The rigid pieces.
        for (k, span) in [1_usize, 5, 20].into_iter().enumerate() {
            for t in 0..=last.saturating_sub(span) {
                for a in h.pieces_at[t].iter().filter(|a| a.tiles.len() >= 30) {
                    let Some(b) = h.pieces_at[t + span].iter().find(|b| b.frame == a.frame) else {
                        continue;
                    };
                    n.rigid[k][0] += 1;
                    let then: HashMap<u32, usize> =
                        a.tiles.iter().enumerate().map(|(i, x)| (x.id, i)).collect();
                    let mut kept = 0;
                    for (j, x) in b.tiles.iter().enumerate() {
                        let Some(&i) = then.get(&x.id) else {
                            continue;
                        };
                        kept += 1;
                        n.rigid_moved += u64::from(a.tiles[i].at != x.at);
                        let off =
                            km_between(back(a, a.place(sphere, i)), back(b, b.place(sphere, j)));
                        n.rigid_off = n.rigid_off.max(off.unsigned_abs());
                    }
                    n.rigid[k][1] += u64::from(kept == a.tiles.len() && b.tiles.len() == kept);
                }
            }
        }
        rifts(&world, &settings, &mut n);
    }
    let round = |x: f64| (x * 1000.0).round() / 1000.0;
    let pc = |a: u64, b: u64| round(100.0 * a as f64 / b.max(1) as f64);
    let per = |a: u64, b: u64| round(a as f64 / b.max(1) as f64);
    let named = |names: &[&str], values: Vec<Value>| -> Value {
        Value::Object(
            names
                .iter()
                .zip(values)
                .map(|(k, v)| ((*k).to_owned(), v))
                .collect::<Map<String, Value>>(),
        )
    };
    let all: u64 = n.flips.iter().sum();
    let far: u64 = n.far_flips.iter().sum();
    let by_cause = |v: &[u64; 16]| {
        let total: u64 = v.iter().sum();
        let mut out = Map::new();
        out.insert(
            "k km2 per step and planet".to_owned(),
            json!(per(total / 1000, n.steps)),
        );
        for k in 0..16 {
            if v[k] > 0 {
                out.insert(format!("% {}", CAUSES[k]), json!(pc(v[k], total)));
            }
        }
        Value::Object(out)
    };
    seconds.sort_by(f64::total_cmp);
    n.rift_my.sort_unstable();
    let switched: u64 = n.switched.iter().sum();
    let table = json!({
        "version": version,
        "seeds": seeds_arg,
        "planets": n.planets,
        "steps per planet": settings.steps,
        "1 land/sea flips, % of crust cells per step": {
            "all": pc(all, n.followed),
            "far from the plates' edges (over 1000 km)": pc(far, n.far_followed),
            "share by cause, % of flips": named(&CAUSES, n.flips.iter().map(|&v| json!(pc(v, all))).collect()),
            "share by cause far from edges, % of flips there": named(&CAUSES, n.far_flips.iter().map(|&v| json!(pc(v, far))).collect()),
        },
        "2 rigid pieces (30 cells or more)": {
            "followed, and with no cell gained or lost, over 1 step": n.rigid[0],
            "over 5 steps": n.rigid[1],
            "over 20 steps": n.rigid[2],
            "kept cells that changed place in their piece (must be 0)": n.rigid_moved,
            "worst offset of a kept cell, km (must be 0)": n.rigid_off,
        },
        "3 plates": {
            "at the start, per planet": per(n.plates_start, n.planets),
            "today, per planet": named(&PLATE_NAMES, n.plates_today.iter().map(|&v| json!(per(v, n.planets))).collect()),
            "through the history, per step": named(&PLATE_NAMES, n.plates_through.iter().map(|&v| json!(per(v, n.steps + n.planets))).collect()),
            "ended, per planet, by life": named(
                &["1 step", "2-3 steps", "4-9 steps", "10 steps or more"],
                n.lives.iter().map(|&v| json!(per(v, n.planets))).collect(),
            ),
        },
        "4 continental crust kept, per mille of the start": {
            "area not under another piece, lowest and highest": [area.0, area.1],
            "volume, lowest and highest": [volume.0, volume.1],
        },
        "5 ground below the sea's level inside the continents": {
            "hollows with no path to the ocean, per step and planet": named(
                &SIZES.map(|s| s.0),
                n.hollows.iter().map(|&v| json!(per(v, n.steps + n.planets))).collect(),
            ),
            "hollows, k km2 per planet": per(n.hollow_km2 / 1000, n.steps + n.planets),
            "of which the model paints as sea, %": pc(n.hollow_painted_sea_km2, n.hollow_km2),
            "inland water linked to the ocean (over 300 km from its floor), k km2 per planet": per(n.linked_km2 / 1000, n.steps + n.planets),
            "ground gone below the sea's level, in a hollow under 200 k km2": by_cause(&n.sunk[0]),
            "ground gone below the sea's level, in a larger hollow": by_cause(&n.sunk[1]),
            "ground gone below the sea's level, in linked inland water": by_cause(&n.sunk[2]),
            "ground back above the sea's level, in a hollow under 200 k km2": by_cause(&n.risen[0]),
            "ground back above the sea's level, in a larger hollow": by_cause(&n.risen[1]),
            "ground back above the sea's level, in linked inland water": by_cause(&n.risen[2]),
            "hollow reached by the sea, k km2 per step and planet": per(n.reached / 1000, n.steps),
            "inland water cut from the sea, k km2 per step and planet": per(n.cut / 1000, n.steps),
        },
        "6 does a continent move with its plate": {
            "cells of crust that slid on their plate (over 2 km off its turn), % per step": pc(n.slid, n.moved),
            "worst slide, km": n.slid_worst_km,
            "cells of crust that changed plate, % per step": pc(switched, n.moved),
            "share by cause, % of changes": named(&SWITCHES, n.switched.iter().map(|&v| json!(pc(v, switched))).collect()),
        },
        "7 rifts, per planet": {
            "started": per(n.rifts[0], n.planets),
            "became an ocean": per(n.rifts[1], n.planets),
            "failed": per(n.rifts[2], n.planets),
            "from first trough to ocean, My: least, median, most": [
                n.rift_my.first().copied().unwrap_or(0),
                n.rift_my.get(n.rift_my.len() / 2).copied().unwrap_or(0),
                n.rift_my.last().copied().unwrap_or(0),
            ],
        },
        "8 seconds per planet": {
            "median": round(seconds[seconds.len() / 2]),
            "most": round(seconds[seconds.len() - 1]),
        },
    });
    let text = serde_json::to_string_pretty(&table).expect("a table");
    println!("{text}");
    if let Some(path) = arg("--out") {
        std::fs::write(&path, text + "\n").expect("the table written");
    }
}

/// The rifts of a planet: started, become an ocean, failed, and how long
/// from the first trough to the ocean. Version 10: a rift is one cut in
/// one step (no trough before it).
fn rifts(world: &Planet, _settings: &Settings, n: &mut Count) {
    for r in &world.history.rifts {
        n.rifts[0] += 1;
        if r.aborted {
            n.rifts[2] += 1;
        } else {
            n.rifts[1] += 1;
            n.rift_my.push(0);
        }
    }
}
