//! Step 0 of the new world (probe): for a few seeds, the planet's plates
//! and continents (the `planet` crate), our continent on its margin at
//! three widths, drawn for the project lead to judge
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
//! settings) at [`OUR_WIDTH_KM`], `largeur-graine-N-W.png` for W = 350, 450
//! and 550, and `graine-N.json` with the numbers. With `PLANET_LAYERS`
//! set, also each map's layers in `couches/` for the interactive page
//! (`carte.html`): the planet's for every seed, the continent's for
//! `PLANET_CONTINENTS` (default every seed), the widths for
//! `PLANET_WIDTH_SEED` (default 13).

#![allow(
    clippy::many_single_char_names,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::needless_range_loop,
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

/// The widths compared, km; the last is the one shown large.
const WIDTHS: [i64; 3] = [350, 450, 550];
/// The width setting for our continent (26 November): the lead asked
/// ~450 km wide and ~200,000 km² of land; the setting 550 gives that land
/// (a mean width of ~350 km, the coasts being frayed and the ends tapered).
const OUR_WIDTH_KM: i64 = 550;
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
    for &seed in &seeds {
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
        let candidates = Site::candidates(&world);
        let site = candidates.first().cloned();
        let mut zooms = Vec::new();
        if let Some(site) = &site {
            for &w in &WIDTHS {
                zooms.push(Zoom::new(&world, site, w));
            }
        }
        let chosen = zooms.iter().find(|z| z.width_km == OUR_WIDTH_KM);
        if let Some(z) = chosen {
            world.shape(z);
        }
        let elapsed = start.elapsed();
        if std::env::var_os("PLANET_LAYERS").is_some() {
            let dir = out.join("couches");
            std::fs::create_dir_all(&dir).expect("the layers' folder");
            layers::planet_layers(&dir, &world, chosen);
            layers::history_layers(&dir, &world, chosen);
            let continents = std::env::var("PLANET_CONTINENTS").ok();
            if let Some(z) = chosen
                && continents
                    .as_deref()
                    .is_none_or(|c| c.split(',').any(|v| v.trim().parse() == Ok(seed)))
            {
                layers::continent_layers(&dir, seed, z);
            }
            if std::env::var("PLANET_WIDTH_SEED")
                .map_or(seed == 13, |v| v.trim().parse() == Ok(seed))
            {
                for z in &zooms {
                    layers::width_layers(&dir, seed, z);
                }
            }
        }
        write(
            &out.join(format!("planete-graine-{seed}.png")),
            MAP_W,
            MAP_H,
            &draw_planet(&world, chosen),
        );
        for z in &zooms {
            write(
                &out.join(format!("largeur-graine-{seed}-{}.png", z.width_km)),
                ZOOM_SIDE,
                ZOOM_SIDE,
                &draw_zoom(z, false),
            );
        }
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
            json!({"lat": s.lat_lon.0, "lon": s.lat_lon.1, "rate_mm": s.rate, "facing": s.facing,
                "ocean_west": s.ocean_west, "land_near_km2": s.land_near_km2,
                "old_sutures": s.old_near.0, "old_rifts": s.old_near.1, "ends_near": s.ends_near,
                "hotspot_near": s.hotspot_near, "score": s.score, "candidates": candidates.len(),
                "sinking": s.sinking, "riding": s.riding})
        });
        let zoom_json: Vec<serde_json::Value> = zooms
            .iter()
            .map(|z| {
                let m = &z.numbers;
                json!({"width_km": z.width_km, "land_km2": m.land_km2, "islands_km2": m.islands_km2,
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
        println!(
            "seed {seed}: born {born:?}, all {elapsed:?}, site {}",
            site.is_some()
        );
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
        "plates_at_end": alive(last), "terranes": h.terranes, "downwellings": h.downwellings.len(),
        "welds": h.welds.iter().map(|w| json!([w.my, w.kept, w.absorbed, w.kept_km2,
            w.absorbed_km2, w.parted])).collect::<Vec<_>>(),
        "rifts": h.rifts.iter().map(|r| json!([r.my, r.plate, r.new, r.area_km2, r.aborted]))
            .collect::<Vec<_>>(),
        "accretions": h.accretions.len(),
        "accreted_km2": h.accretions.iter().map(|a| a.km2).sum::<i64>(),
        "shortened_km2": h.shortened_km2.iter().sum::<i64>(),
        "arcs_made_km2": h.made_km2.iter().sum::<i64>(),
        "continental_km2": [h.continental_km2[0], h.continental_km2[last]],
        "ocean_floor": {"mean_my": floor_age / floor_km2.max(1), "max_my": floor_max,
            "young_per_mille": young_km2 * 1000 / floor_km2.max(1)},
        "plates_today": speeds.iter().map(|&(a, v)| json!([a, v])).collect::<Vec<_>>(),
        "my": last as i64 * world.settings.step_my, "largest_lands": welded})
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
    let mut px: Vec<Rgb> = cell
        .iter()
        .map(|&c| {
            let e = i64::from(world.elevation[c]);
            let mut col = hypsometric(e);
            if e <= 0 && world.continental[c] {
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
            let land = world.elevation[c] > 0;
            if land != (world.elevation[right] > 0) || land != (world.elevation[down] > 0) {
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
