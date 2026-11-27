//! Step 0 of the new world (probe): for a few seeds, the planet's plates
//! and continents (the `planet` crate), our continent (a piece the
//! history left on an active margin), drawn for the project lead to judge
//! (`content/ai-reserve/review/monde-nouveau/etape-0/`). Nothing here
//! enters the game.
//!
//! ```text
//! cargo run --release -p planet --example planet
//! PLANET_OUT=some/folder PLANET_SEEDS=1,4 cargo run --release -p planet --example planet
//! ```
//!
//! Writes per seed `planete-graine-N.png` (equirectangular, 1,024 × 512),
//! `zoom-graine-N.png` (relief) and `cadre-graine-N.png` (geological
//! settings), and `graine-N.json` with the numbers. With `PLANET_LAYERS`
//! set, also each map's layers in `couches/` for the interactive page
//! (`carte.html`): the planet's and its history's for every seed, the
//! continent's for `PLANET_CONTINENTS` (default every seed). The seeds
//! and the steps' layers are made on every core (`PLANET_THREADS` to
//! change it); what is written does not depend on the threads.

#![allow(
    clippy::many_single_char_names,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::needless_range_loop,
    clippy::cast_precision_loss,
    clippy::type_complexity,
    reason = "drawing code reads as arithmetic over bounded pixels"
)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use planet::{
    self, CELLS, Kind, Line, OldKind, Planet, Setting, Site, V3, ZOOM_PX_KM, ZOOM_SIDE, Zoom,
};
use serde_json::json;

use png::{Rgb, png};

#[path = "common/png.rs"]
mod png;

#[path = "common/planet_layers.rs"]
mod layers;

#[path = "common/par.rs"]
mod par;

const MAP_W: usize = 1024;
const MAP_H: usize = 512;

fn main() {
    let out = std::env::var_os("PLANET_OUT").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../content/ai-reserve/review/monde-nouveau/etape-0")
        },
        PathBuf::from,
    );
    std::fs::create_dir_all(&out).expect("the output folder");
    // The world's settings: `--settings file.ron`, else the defaults.
    let args: Vec<String> = std::env::args().collect();
    let settings: planet::Settings = args
        .iter()
        .position(|a| a == "--settings")
        .and_then(|i| args.get(i + 1))
        .map_or_else(planet::Settings::default, |path| {
            let text = std::fs::read_to_string(path).expect("the settings file");
            ron::from_str(&text).expect("settings in RON")
        });
    let seeds: Vec<u64> = std::env::var("PLANET_SEEDS").ok().map_or_else(
        || (1..=6).collect(),
        |s| s.split(',').filter_map(|v| v.trim().parse().ok()).collect(),
    );
    // The seeds are made on several threads, and the steps' layers of
    // each seed too; the lines are printed in the order of the seeds.
    let threads = par::threads();
    let inner = (threads / seeds.len().max(1)).max(1);
    let lines = par::par_map(&seeds, threads, |&seed| {
        let start = Instant::now();
        let mut world = Planet::with(seed, &settings);
        let born = start.elapsed();
        let before: Vec<serde_json::Value> = world
            .lands
            .iter()
            .take(8)
            .map(|l| {
                json!({"area_km2": l.area_km2, "span_km": l.span_km,
                "lat": planet::latitude(l.centre), "lon": planet::longitude(l.centre)})
            })
            .collect();
        let boundary: Vec<serde_json::Value> = world
            .boundary_km()
            .iter()
            .map(|(k, km)| json!({"kind": format!("{k:?}"), "km": km}))
            .collect();
        let mut candidates = Site::candidates(&world);
        // No piece qualifies: one is cut from a larger land.
        let mut cut_km2 = 0;
        if candidates.is_empty() {
            cut_km2 = world.carve();
            candidates = Site::candidates(&world);
        }
        let site = candidates.first().cloned();
        let zoom = site.as_ref().map(|s| Zoom::new(&world, s));
        if let Some(s) = &site {
            world.mark(s);
        }
        let chosen = zoom.as_ref();
        let elapsed = start.elapsed();
        if std::env::var_os("PLANET_LAYERS").is_some() {
            let dir = out.join("couches");
            std::fs::create_dir_all(&dir).expect("the layers' folder");
            layers::planet_layers(&dir, &world, chosen);
            let steps: Vec<usize> = (0..=world.history.last()).collect();
            par::par_map(&steps, inner, |&t| {
                layers::step_layers(&dir, &world.at_step(t), chosen);
            });
            let continents = std::env::var("PLANET_CONTINENTS").ok();
            if let Some(z) = chosen
                && continents
                    .as_deref()
                    .is_none_or(|c| c.split(',').any(|v| v.trim().parse() == Ok(seed)))
            {
                layers::continent_layers(&dir, seed, z);
            }
        }
        write(
            &out.join(format!("planete-graine-{seed}.png")),
            MAP_W,
            MAP_H,
            &draw_planet(&world, chosen),
        );
        if let Some(z) = chosen {
            write(
                &out.join(format!("zoom-graine-{seed}.png")),
                ZOOM_SIDE,
                ZOOM_SIDE,
                &draw_zoom(z, false),
            );
            write(
                &out.join(format!("cadre-graine-{seed}.png")),
                ZOOM_SIDE,
                ZOOM_SIDE,
                &draw_zoom(z, true),
            );
        }
        let plates: Vec<serde_json::Value> = (0..world.plates.plates.len())
            .map(|k| {
                let cells: Vec<usize> = (0..CELLS)
                    .filter(|&c| world.plate[c] as usize == k)
                    .collect();
                let area: i64 = cells
                    .iter()
                    .map(|&c| i64::from(world.sphere.area_km2[c]))
                    .sum();
                let cont: i64 = cells
                    .iter()
                    .filter(|&&c| world.continental[c])
                    .map(|&c| i64::from(world.sphere.area_km2[c]))
                    .sum();
                json!({"area_km2": area, "continental_per_mille": cont * 1000 / area.max(1),
                    "speed_mm": world.plates.plates[k].speed})
            })
            .collect();
        let site_json = site.as_ref().map(|s| {
            json!({"land_km2": s.area_km2, "trench_km": s.trench_km, "cut_km2": cut_km2, "lat": s.lat_lon.0, "lon": s.lat_lon.1, "rate_mm": s.rate, "facing": s.facing,
                "ocean_west": s.ocean_west, "land_near_km2": s.land_near_km2,
                "old_sutures": s.old_near.0, "old_rifts": s.old_near.1, "ends_near": s.ends_near,
                "hotspot_near": s.hotspot_near, "score": s.score, "candidates": candidates.len(),
                "sinking": s.sinking, "riding": s.riding})
        });
        let zoom_json: Vec<serde_json::Value> = zoom
            .iter()
            .map(|z| {
                let m = &z.numbers;
                json!({"land_km2": m.land_km2, "islands_km2": m.islands_km2,
                    "length_km": m.length_km, "mean_width_km": m.mean_width_km,
                    "farthest_sea_km": m.farthest_sea_km, "far_50": m.far_50, "far_100": m.far_100,
                    "lee_km": m.lee_km, "interior_km2": m.interior_km2, "volcanoes": m.volcanoes,
                    "enclosed_sea_km2": m.enclosed_sea_km2, "basin_km2": m.basin_km2,
                    "platform_km2": m.platform_km2, "old_core_km2": m.old_core_km2,
                    "marsh_km2": m.marsh_km2, "highest_m": m.highest_m,
                    "arc_km": z.arc_km, "coast_km": z.coast_km,
                    "lines": line_counts(z), "lat_rows": lat_rows(z)})
            })
            .collect();
        let ours = world
            .lands
            .iter()
            .find(|l| l.cells.iter().any(|&c| world.ours[c as usize]));
        let summary = json!({
            "seed": seed,
            "born_ms": born.as_millis(),
            "total_ms": elapsed.as_millis(),
            "plates": plates,
            "old_plates": world.old.plates.len(),
            "hotspots": world.hotspots.len(),
            "boundary": boundary,
            "lands_before": before,
            "lands_after": world.lands.iter().take(8).map(|l| json!({"area_km2": l.area_km2,
                "span_km": l.span_km, "lat": planet::latitude(l.centre),
                "lon": planet::longitude(l.centre),
                "ours": l.cells.iter().any(|&c| world.ours[c as usize])})).collect::<Vec<_>>(),
            "land_total_km2": world.lands.iter().map(|l| l.area_km2).sum::<i64>(),
            "land_beyond_60_km2": (0..CELLS)
                .filter(|&c| world.elevation[c] > 0
                    && planet::latitude(world.sphere.centre[c]).abs() > 600)
                .map(|c| i64::from(world.sphere.area_km2[c]))
                .sum::<i64>(),
            "settings": settings,
            "history": history_json(&world),
            "metrics": if std::env::var_os("PLANET_METRICS").is_some() {
                metrics(&world)
            } else {
                serde_json::Value::Null
            },
            "land_count_over_1M": world.lands.iter().filter(|l| l.area_km2 > 1_000_000).count(),
            "ours_on_grid_km2": ours.map(|l| l.area_km2),
            "site": site_json,
            "zooms": zoom_json,
        });
        std::fs::write(
            out.join(format!("graine-{seed}.json")),
            serde_json::to_string_pretty(&summary).expect("json"),
        )
        .expect("the summary");
        format!(
            "seed {seed}: born {born:?}, all {elapsed:?}, site {}",
            site.is_some()
        )
    });
    for line in lines {
        println!("{line}");
    }
}

/// The history in numbers: plates, welds, rifts, accretions, the ocean
/// floor, and what the largest lands are made of.
fn history_json(world: &Planet) -> serde_json::Value {
    use planet::history::{NONE, OCEAN, TERRANE_BASE};
    let h = &world.history;
    let last = h.last();
    let alive = |t: usize| {
        let mut v = h.plate[t].clone();
        v.sort_unstable();
        v.dedup();
        v.len()
    };
    let welded: Vec<serde_json::Value> = world
        .lands
        .iter()
        .take(6)
        .map(|l| {
            let mut pieces: Vec<u8> = l
                .cells
                .iter()
                .map(|&c| h.piece[c as usize])
                .filter(|&p| p != NONE)
                .collect();
            pieces.sort_unstable();
            pieces.dedup();
            let (f, t): (Vec<u8>, Vec<u8>) = pieces.iter().partition(|&&p| p < TERRANE_BASE);
            json!({"area_km2": l.area_km2, "span_km": l.span_km, "fragments": f.len(),
                "terranes": t.len()})
        })
        .collect();
    let s = &world.sphere;
    let (mut floor_km2, mut floor_age, mut young_km2, mut floor_max) = (0_i64, 0_i64, 0_i64, 0);
    for c in 0..planet::CELLS {
        if h.class[last][c] != OCEAN {
            continue;
        }
        let a = i64::from(s.area_km2[c]);
        let age = i64::from(h.age[last][c]);
        floor_km2 += a;
        floor_age += a * age;
        young_km2 += if age <= 20 { a } else { 0 };
        floor_max = floor_max.max(age);
    }
    let mut speeds: Vec<(i64, i64)> = (0..world.plates.plates.len())
        .map(|k| {
            let area: i64 = (0..planet::CELLS)
                .filter(|&c| world.plate[c] as usize == k)
                .map(|c| i64::from(s.area_km2[c]))
                .sum();
            (area, world.plates.plates[k].speed)
        })
        .collect();
    speeds.sort_unstable_by_key(|&(a, _)| std::cmp::Reverse(a));
    json!({"plates_at_start": h.plates_at_start.1, "continent_plates_at_start": h.plates_at_start.0,
        "plates_at_end": alive(last), "terranes": h.terranes,
        "welds": h.welds.iter().map(|w| json!([w.my, w.kept, w.absorbed, w.kept_km2,
            w.absorbed_km2, w.parted])).collect::<Vec<_>>(),
        "rifts": h.rifts.iter().map(|r| json!([r.my, r.plate, r.new, r.area_km2, r.aborted, r.cut_my,
            r.parted_my, r.failed_my, r.cells]))
            .collect::<Vec<_>>(),
        "collisions": h.collisions.iter().map(|c| json!([c.from_my, c.a, c.b,
            if c.welded_my == planet::history::NEVER { -1 } else { i64::from(c.welded_my) },
            c.closed_km, c.front_cells, c.steps])).collect::<Vec<_>>(),
        "escapes": h.escapes.iter().map(|e| json!([e.my, e.from, e.block, e.km2]))
            .collect::<Vec<_>>(),
        "lips": h.lips.iter().map(|l| json!([l.my, l.km2])).collect::<Vec<_>>(),
        "hotspot_chains": world.hotspots.iter().map(|hs| json!([hs.track.len(),
            hs.track.last().map_or(0, |m| m.1)])).collect::<Vec<_>>(),
        "accretions": h.accretions.len(),
        "accreted_km2": h.accretions.iter().map(|a| a.km2).sum::<i64>(),
        "shortened_km2": h.shortened_km2.iter().sum::<i64>(),
        "arcs_made_km2": h.made_km2.iter().sum::<i64>(),
        "continental_km2": [h.continental_km2[0], h.continental_km2[last]],
        "continental_steps_km2": h.continental_km2,
        "ocean_floor": {"mean_my": floor_age / floor_km2.max(1), "max_my": floor_max,
            "young_per_mille": young_km2 * 1000 / floor_km2.max(1)},
        "plates_today": speeds.iter().map(|&(a, v)| json!([a, v])).collect::<Vec<_>>(),
        "my": last as i64 * world.settings.step_my, "largest_lands": welded})
}

/// The model's behaviour in numbers (with `PLANET_METRICS`): whether a
/// rift's halves stay apart, how long a range lasts, how much the coasts
/// change inside the plates from a step to the next, how fast the plates
/// go. Crust is followed by its home cell.
fn metrics(world: &Planet) -> serde_json::Value {
    use planet::history::{CONTINENT_THICK, OCEAN};
    let h = &world.history;
    let s = &world.sphere;
    let last = h.last();
    let step_my = world.settings.step_my as usize;
    // The continental masses at a step: each cell's mass (0: ocean).
    let masses = |t: usize| {
        let mut mass = vec![0_u32; CELLS];
        let mut next = 0;
        for start in 0..CELLS {
            if mass[start] != 0 || h.class[t][start] == OCEAN {
                continue;
            }
            next += 1;
            mass[start] = next;
            let mut stack = vec![start];
            while let Some(c) = stack.pop() {
                for &(n, _) in &s.links[c] {
                    let n = n as usize;
                    if mass[n] == 0 && h.class[t][n] != OCEAN {
                        mass[n] = next;
                        stack.push(n);
                    }
                }
            }
        }
        mass
    };
    // 1. Rifts: do the halves part, and are they apart five steps later?
    let (mut never, mut back, mut apart, mut unknown) = (0, 0, 0, 0);
    for r in h
        .rifts
        .iter()
        .filter(|r| r.cut_my != planet::history::NEVER)
    {
        let t0 = last - r.cut_my as usize / step_my;
        // The halves as drawn at the rift's step, or the next (plates
        // moving as one are drawn as one).
        let mut side = vec![0_u8; CELLS];
        let mut n = [0_usize; 3];
        for t in t0..=(t0 + 1).min(last) {
            side.fill(0);
            n = [0; 3];
            for c in 0..CELLS {
                if h.class[t][c] == OCEAN {
                    continue;
                }
                let k = if h.plate[t][c] == r.plate {
                    1
                } else if h.plate[t][c] == r.new {
                    2
                } else {
                    continue;
                };
                side[h.home_at[t][c] as usize] = k;
                n[k as usize] += 1;
            }
            if n[1] > 0 && n[2] > 0 {
                break;
            }
        }
        if t0 + 5 > last {
            unknown += 1;
            continue;
        }
        if n[1] == 0 || n[2] == 0 {
            never += 1;
            continue;
        }
        let (mut parted, mut rejoined) = (false, false);
        for t in t0 + 1..=t0 + 5 {
            let mass = masses(t);
            let mut tally: std::collections::BTreeMap<u32, [usize; 3]> =
                std::collections::BTreeMap::new();
            for c in 0..CELLS {
                let k = side[h.home_at[t][c] as usize];
                if mass[c] != 0 && k != 0 {
                    tally.entry(mass[c]).or_insert([0; 3])[k as usize] += 1;
                }
            }
            let most = |k: usize| tally.iter().max_by_key(|(_, v)| v[k]).map(|(&m, _)| m);
            if most(1) == most(2) {
                rejoined |= parted;
            } else {
                parted = true;
            }
        }
        if !parted {
            never += 1;
        } else if rejoined {
            back += 1;
        } else {
            apart += 1;
        }
    }
    // 2. Ranges: from a pile's peak (at least 1,000 m above a plain
    // continent) to the step it stands under 300 m.
    let mut peak = vec![0_i64; CELLS];
    let mut peak_t = vec![0_usize; CELLS];
    let mut gone_t = vec![usize::MAX; CELLS];
    let mut dropped = vec![false; CELLS];
    for t in 0..=last {
        let mut cur = vec![i64::MIN; CELLS];
        for c in 0..CELLS {
            if h.class[t][c] != OCEAN {
                let e = i64::from(h.thick_at[t][c]) - i64::from(CONTINENT_THICK);
                let home = h.home_at[t][c] as usize;
                cur[home] = cur[home].max(e);
            }
        }
        for home in 0..CELLS {
            if cur[home] == i64::MIN {
                dropped[home] |= peak[home] > 0 && gone_t[home] == usize::MAX;
            } else if cur[home] > peak[home] {
                peak[home] = cur[home];
                peak_t[home] = t;
                gone_t[home] = usize::MAX;
            } else if cur[home] < 30 && gone_t[home] == usize::MAX && peak[home] >= 100 {
                gone_t[home] = t;
            }
        }
    }
    let room = 100 / step_my;
    let mut lives: Vec<usize> = Vec::new();
    let mut shown_100 = 0;
    for home in 0..CELLS {
        if peak[home] < 100 || dropped[home] || peak_t[home] + room > last {
            continue;
        }
        let life = gone_t[home].min(last) - peak_t[home];
        lives.push(life * step_my);
        shown_100 += usize::from(life >= room);
    }
    lives.sort_unstable();
    // 3. Coasts inside the plates (over 300 km from a boundary): the share
    // of coast cells whose crust is on the other side of the shore a step
    // later.
    let inside_km: u32 = std::env::var("PLANET_COAST_KM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let (mut coast, mut changed) = (0_i64, 0_i64);
    let mut land_share = Vec::new();
    let mut sea_rise = Vec::new();
    let mut before = world.at_step(0);
    for t in 0..=last {
        let now = if t == 0 { None } else { Some(world.at_step(t)) };
        if let Some(now) = &now {
            let mut cell_of = vec![u32::MAX; CELLS];
            for c in 0..CELLS {
                if h.class[t][c] != OCEAN {
                    let home = h.home_at[t][c] as usize;
                    if cell_of[home] == u32::MAX {
                        cell_of[home] = c as u32;
                    }
                }
            }
            for c in 0..CELLS {
                if h.class[t - 1][c] == OCEAN || before.near[c].0 <= inside_km {
                    continue;
                }
                let dry = before.elevation[c] > 0;
                if !s.links[c]
                    .iter()
                    .any(|&(n, _)| (before.elevation[n as usize] > 0) != dry)
                {
                    continue;
                }
                let to = cell_of[h.home_at[t - 1][c] as usize];
                if to == u32::MAX {
                    continue;
                }
                coast += 1;
                changed += i64::from((now.elevation[to as usize] > 0) != dry);
            }
        }
        if let Some(now) = now {
            before = now;
        }
        let land: i64 = (0..CELLS)
            .filter(|&c| before.elevation[c] > 0)
            .map(|c| i64::from(s.area_km2[c]))
            .sum();
        land_share.push(land * 1000 / 510_064_472);
        sea_rise.push(before.sea_rise_m);
    }
    // 4. Speeds: each plate's mean speed over its surface, at every step.
    let mut plate_steps: Vec<(f64, f64, f64)> = Vec::new();
    for t in 1..=last {
        let mut sum = vec![0_f64; planet::history::MAXP];
        let mut cells = vec![0_f64; planet::history::MAXP];
        let mut cont = vec![0_f64; planet::history::MAXP];
        for c in 0..CELLS {
            let p = h.plate[t][c] as usize;
            let m = h.motion[t][p];
            let (a, b) = (
                m.pole.map(|v| v as f64 / 1e6),
                s.centre[c].map(|v| v as f64 / 1e6),
            );
            let x = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            sum[p] += m.speed_mm as f64 * (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
            cells[p] += 1.0;
            cont[p] += f64::from(u8::from(h.class[t][c] != OCEAN));
        }
        for p in 0..planet::history::MAXP {
            if cells[p] >= 300.0 {
                plate_steps.push((sum[p] / cells[p], cont[p] / cells[p], cells[p]));
            }
        }
    }
    let mean = |f: &dyn Fn(f64) -> bool| {
        let (mut a, mut w) = (0.0, 0.0);
        for &(v, share, n) in &plate_steps {
            if f(share) {
                a += v * n;
                w += n;
            }
        }
        if w > 0.0 { (a / w).round() } else { -1.0 }
    };
    let mut v: Vec<i64> = plate_steps.iter().map(|p| p.0.round() as i64).collect();
    v.sort_unstable();
    let q = |k: usize| v.get(v.len() * k / 100).copied().unwrap_or(0);
    let area = &h.continental_km2;
    json!({
        "rifts": {"never_parted": never, "rejoined": back, "apart": apart, "unknown": unknown},
        "ranges": {"count": lives.len(), "median_my": lives.get(lives.len() / 2),
            "shown_after_100_my_per_mille": shown_100 * 1000 / lives.len().max(1)},
        "coast": {"cells": coast, "changed": changed},
        "land_share_steps": land_share,
        "sea_rise_m": sea_rise,
        "speed_mm": {"mean": mean(&|_| true), "continental": mean(&|c| c >= 0.4),
            "oceanic": mean(&|c| c < 0.1), "p10": q(10), "p50": q(50), "p90": q(90),
            "max": v.last()},
        "crust_ratio": [*area.iter().min().expect("steps") as f64 / area[0] as f64,
            *area.iter().max().expect("steps") as f64 / area[0] as f64],
    })
}

fn write(path: &Path, w: usize, h: usize, pixels: &[Rgb]) {
    std::fs::write(path, png(w, h, pixels)).expect("a picture");
}

fn line_counts(z: &Zoom) -> serde_json::Value {
    let count = |l: Line| z.line.iter().filter(|&&x| x == l).count();
    json!({"suture_px": count(Line::Suture), "failed_rift_px": count(Line::FailedRift),
        "transform_px": count(Line::Transform), "divergent_px": count(Line::Divergent),
        "convergent_px": count(Line::Convergent), "hotspot_points": z.hotspot_points.len()})
}

/// Rows where a whole degree of latitude crosses the frame's left edge.
fn lat_rows(z: &Zoom) -> Vec<(usize, i64)> {
    let mut rows = Vec::new();
    for y in 1..ZOOM_SIDE {
        let a = i64::from(z.latitude[(y - 1) * ZOOM_SIDE + 40]);
        let b = i64::from(z.latitude[y * ZOOM_SIDE + 40]);
        if a.div_euclid(10) != b.div_euclid(10) {
            rows.push((y, a.max(b).div_euclid(10)));
        }
    }
    rows
}

fn mix(a: Rgb, b: Rgb, t: i64) -> Rgb {
    let f = |x: u8, y: u8| {
        (i64::from(x) + (i64::from(y) - i64::from(x)) * t / 1000).clamp(0, 255) as u8
    };
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2])]
}

/// A colour for a height, m: sea blues, then greens, ochres, greys, snow.
fn hypsometric(e: i64) -> Rgb {
    const SEA: [(i64, Rgb); 5] = [
        (-7000, [10, 22, 60]),
        (-4500, [22, 48, 104]),
        (-2500, [36, 78, 140]),
        (-200, [76, 132, 188]),
        (0, [128, 178, 214]),
    ];
    const LAND: [(i64, Rgb); 7] = [
        (0, [92, 140, 84]),
        (150, [120, 162, 96]),
        (500, [170, 178, 112]),
        (1000, [184, 160, 104]),
        (1800, [156, 122, 88]),
        (2800, [150, 140, 136]),
        (4200, [245, 245, 245]),
    ];
    let table: &[(i64, Rgb)] = if e <= 0 { &SEA } else { &LAND };
    if e <= table[0].0 {
        return table[0].1;
    }
    for w in table.windows(2) {
        if e <= w[1].0 {
            let t = (e - w[0].0) * 1000 / (w[1].0 - w[0].0).max(1);
            return mix(w[0].1, w[1].1, t);
        }
    }
    table[table.len() - 1].1
}

fn kind_colour(k: Kind) -> Rgb {
    match k {
        Kind::Ridge | Kind::Rift => [255, 200, 40],
        Kind::Transform => [60, 230, 120],
        Kind::Trench | Kind::Arc | Kind::IslandArc => [235, 40, 40],
        Kind::Collision => [200, 40, 200],
    }
}

fn put(px: &mut [Rgb], w: usize, h: usize, x: i64, y: i64, c: Rgb) {
    if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
        px[y as usize * w + x as usize] = c;
    }
}

fn segment(px: &mut [Rgb], w: usize, h: usize, (x0, y0): (i64, i64), (x1, y1): (i64, i64), c: Rgb) {
    let n = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
    for k in 0..=n {
        put(px, w, h, x0 + (x1 - x0) * k / n, y0 + (y1 - y0) * k / n, c);
    }
}

fn map_xy(v: V3) -> (i64, i64) {
    let lat = planet::latitude(v);
    let lon = planet::longitude(v);
    let x = (lon + 1800) * MAP_W as i64 / 3600;
    let y = (900 - lat) * MAP_H as i64 / 1800;
    (x, y)
}

fn draw_planet(world: &Planet, zoom: Option<&Zoom>) -> Vec<Rgb> {
    let mut cell = vec![0_usize; MAP_W * MAP_H];
    for y in 0..MAP_H {
        let lat = 900 - (2 * y as i64 + 1) * 1800 / (2 * MAP_H as i64);
        for x in 0..MAP_W {
            let lon = -1800 + (2 * x as i64 + 1) * 3600 / (2 * MAP_W as i64);
            cell[y * MAP_W + x] = world.sphere.cell_of(planet::from_lat_lon(lat, lon));
        }
    }
    // The relief is read in the pieces of continent themselves (version
    // 10), finer than the grid.
    let mut surface = Vec::with_capacity(MAP_W * MAP_H);
    for y in 0..MAP_H {
        let lat = 900 - (2 * y as i64 + 1) * 1800 / (2 * MAP_H as i64);
        for x in 0..MAP_W {
            let lon = -1800 + (2 * x as i64 + 1) * 3600 / (2 * MAP_W as i64);
            surface.push(world.surface(planet::from_lat_lon(lat, lon)));
        }
    }
    let mut px: Vec<Rgb> = surface
        .iter()
        .map(|&(e, continental)| {
            let mut col = hypsometric(i64::from(e));
            if e <= 0 && continental {
                // Continental shelf: drowned continent, paler.
                col = mix(col, [150, 190, 210], 350);
            }
            col
        })
        .collect();
    for y in 0..MAP_H {
        for x in 0..MAP_W {
            let i = y * MAP_W + x;
            let c = cell[i];
            let right = cell[y * MAP_W + (x + 1) % MAP_W];
            let down = if y + 1 < MAP_H { cell[i + MAP_W] } else { c };
            let land = surface[i].0 > 0;
            let beside = (
                y * MAP_W + (x + 1) % MAP_W,
                (i + MAP_W).min(MAP_W * MAP_H - 1),
            );
            if land != (surface[beside.0].0 > 0) || land != (surface[beside.1].0 > 0) {
                px[i] = [20, 20, 20];
            }
            if let Some(e) = world.edge[c]
                && (world.plate[c] != world.plate[right] || world.plate[c] != world.plate[down])
            {
                px[i] = kind_colour(e.kind);
            }
            if world.continental[c]
                && world.elevation[c] > 0
                && matches!(
                    world.old_edge[c],
                    Some(OldKind::Suture | OldKind::FailedRift)
                )
                && (x + y) % 3 == 0
            {
                px[i] = if world.old_edge[c] == Some(OldKind::Suture) {
                    [120, 60, 150]
                } else {
                    [230, 130, 30]
                };
            }
        }
    }
    // Latitudes 40 and 50, both hemispheres: dashed.
    for lat in [-500_i64, -400, 400, 500] {
        let y = (900 - lat) * MAP_H as i64 / 1800;
        for x in (0..MAP_W as i64).step_by(6) {
            for d in 0..3 {
                put(&mut px, MAP_W, MAP_H, x + d, y, [255, 255, 255]);
            }
        }
    }
    // Plate motion: a short line from each point of a 20° lattice.
    for lat in (-60..=60).step_by(20) {
        for lon in (-170..=170).step_by(20) {
            let p = planet::from_lat_lon(lat * 10, lon * 10);
            let k = world.plates.plate_at(p);
            let v = world.plates.velocity(k, p);
            let speed = v.iter().map(|&a| a * a).sum::<i64>().isqrt();
            if speed < 3 {
                continue;
            }
            let q = planet::step(p, planet::normalize(v), speed * 22);
            let (a, b) = (map_xy(p), map_xy(q));
            if (a.0 - b.0).abs() < 80 {
                segment(&mut px, MAP_W, MAP_H, a, b, [0, 0, 0]);
                for d in [(-1, 0), (1, 0), (0, -1), (0, 1), (0, 0)] {
                    put(&mut px, MAP_W, MAP_H, a.0 + d.0, a.1 + d.1, [255, 255, 255]);
                }
            }
        }
    }
    // Hotspots and their chains.
    for h in &world.hotspots {
        for &(x, _) in &h.track {
            let (a, b) = map_xy(x);
            put(&mut px, MAP_W, MAP_H, a, b, [255, 150, 30]);
        }
        let (a, b) = map_xy(h.at);
        for d in -2..=2 {
            put(&mut px, MAP_W, MAP_H, a + d, b, [255, 90, 0]);
            put(&mut px, MAP_W, MAP_H, a, b + d, [255, 90, 0]);
        }
    }
    // Our continent: a ring around it.
    if let Some(z) = zoom {
        let (cx, cy) = map_xy(z.frame.centre);
        let lat = planet::latitude(z.frame.centre).abs();
        let stretch = 1_000_000_000 / planet::cos(planet::tenths(lat)).max(1);
        for t in 0..360 {
            let a = planet::tenths(t * 10);
            let r = 18;
            let x = cx + r * planet::cos(a) * stretch / 1_000 / 1_000_000;
            let y = cy + r * planet::sin(a) / 1_000_000;
            put(&mut px, MAP_W, MAP_H, x, y, [255, 0, 255]);
            put(&mut px, MAP_W, MAP_H, x + 1, y, [255, 0, 255]);
        }
    }
    px
}

fn setting_colour(s: Setting) -> Rgb {
    match s {
        Setting::Ocean => [40, 70, 120],
        Setting::Trench => [25, 40, 85],
        Setting::CoastRange => [150, 120, 90],
        Setting::ForearcBasin => [215, 200, 120],
        Setting::Arc => [190, 60, 50],
        Setting::BackArc => [205, 150, 110],
        Setting::BackArcBasin => [170, 210, 225],
        Setting::OldCore => [235, 150, 175],
        Setting::FailedRift => [240, 150, 40],
        Setting::Suture => [150, 110, 190],
        Setting::CoastalPlain => [120, 190, 120],
        Setting::Hotspot => [110, 40, 40],
        Setting::Rift => [250, 110, 20],
        Setting::Range => [120, 70, 110],
    }
}

fn draw_zoom(z: &Zoom, settings: bool) -> Vec<Rgb> {
    let n = ZOOM_SIDE * ZOOM_SIDE;
    let side = ZOOM_SIDE as i64;
    let e = |x: i64, y: i64| {
        i64::from(z.elevation[(y.clamp(0, side - 1) * side + x.clamp(0, side - 1)) as usize])
    };
    let mut px = vec![[0_u8; 3]; n];
    for i in 0..n {
        let (x, y) = ((i % ZOOM_SIDE) as i64, (i / ZOOM_SIDE) as i64);
        let h = e(x, y);
        // Light from the north-west.
        let slope = (e(x - 1, y) - e(x + 1, y)) + (e(x, y - 1) - e(x, y + 1));
        let shade = (1000 + slope.clamp(-2000, 2000) * i64::from(h > 0) / 5).clamp(550, 1300);
        let mut c = if settings {
            if h > 0 {
                let base = setting_colour(z.setting[i]);
                mix(base, hypsometric(h), 250)
            } else {
                mix(hypsometric(h), [255, 255, 255], 250)
            }
        } else {
            hypsometric(h)
        };
        c = [
            (i64::from(c[0]) * shade / 1000).clamp(0, 255) as u8,
            (i64::from(c[1]) * shade / 1000).clamp(0, 255) as u8,
            (i64::from(c[2]) * shade / 1000).clamp(0, 255) as u8,
        ];
        if settings && z.marsh[i] && (x + y) % 3 == 0 {
            c = [30, 110, 60];
        }
        px[i] = c;
    }
    // The coast.
    for i in 0..n {
        let (x, y) = ((i % ZOOM_SIDE) as i64, (i / ZOOM_SIDE) as i64);
        if x + 1 < side && y + 1 < side {
            let land = e(x, y) > 0;
            if land != (e(x + 1, y) > 0) || land != (e(x, y + 1) > 0) {
                px[i] = [25, 25, 25];
            }
        }
    }
    // Degrees of latitude: dotted.
    for i in 0..n {
        let (x, y) = (i % ZOOM_SIDE, i / ZOOM_SIDE);
        if y > 0 && x % 4 == 0 {
            let a = i64::from(z.latitude[i - ZOOM_SIDE]).div_euclid(10);
            let b = i64::from(z.latitude[i]).div_euclid(10);
            if a != b {
                px[i] = [235, 235, 235];
            }
        }
    }
    // Plate lines.
    for i in 0..n {
        let (x, y) = ((i % ZOOM_SIDE) as i64, (i / ZOOM_SIDE) as i64);
        let (c, thick, dashed) = match z.line[i] {
            Line::None => continue,
            Line::Trench => ([230, 30, 30], 2, false),
            Line::Convergent => ([230, 30, 30], 1, false),
            Line::Divergent => ([255, 200, 40], 2, false),
            Line::Transform => ([40, 220, 110], 2, false),
            Line::Suture => ([120, 50, 170], 1, true),
            Line::FailedRift => ([240, 120, 20], 1, true),
        };
        if dashed && (x + y) % 8 < 3 {
            continue;
        }
        for d in 0..thick {
            put(&mut px, ZOOM_SIDE, ZOOM_SIDE, x + d, y, c);
            put(&mut px, ZOOM_SIDE, ZOOM_SIDE, x, y + d, c);
        }
    }
    // Hotspot chain points.
    for &(i, _) in &z.hotspot_points {
        let (x, y) = ((i % ZOOM_SIDE) as i64, (i / ZOOM_SIDE) as i64);
        for dx in -3..=3_i64 {
            for dy in -3..=3_i64 {
                if dx * dx + dy * dy <= 9 && dx * dx + dy * dy >= 4 {
                    put(&mut px, ZOOM_SIDE, ZOOM_SIDE, x + dx, y + dy, [255, 140, 0]);
                }
            }
        }
    }
    // Volcanoes: red triangles.
    for &i in &z.volcanoes {
        let (x, y) = ((i % ZOOM_SIDE) as i64, (i / ZOOM_SIDE) as i64);
        for row in 0..7_i64 {
            for col in -row / 2 - 1..=row / 2 + 1 {
                let edge = row == 6 || col == -row / 2 - 1 || col == row / 2 + 1;
                put(
                    &mut px,
                    ZOOM_SIDE,
                    ZOOM_SIDE,
                    x + col,
                    y - 4 + row,
                    if edge { [40, 0, 0] } else { [240, 30, 20] },
                );
            }
        }
    }
    // A 100 km scale bar, bottom left.
    let bar = 100 / ZOOM_PX_KM;
    for x in 20..20 + bar {
        for y in side - 22..side - 18 {
            put(
                &mut px,
                ZOOM_SIDE,
                ZOOM_SIDE,
                x,
                y,
                if (x - 20) / (bar / 2) == 0 {
                    [0, 0, 0]
                } else {
                    [255, 255, 255]
                },
            );
        }
    }
    px
}
