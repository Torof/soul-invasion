//! The bench: small written scenes, one mechanism each, run by the real
//! model (`history::run_scene`: the model's own start and steps, on
//! continents and plates laid by hand, with turns imposed where a scene
//! says so). Each scene prints a line of numbers a step and writes a
//! sheet of its steps, `banc-<scene>.png`.
//!
//! ```text
//! cargo run --profile essai -p planet --example banc              # every scene
//! cargo run --profile essai -p planet --example banc -- rift      # one
//! PLANET_OUT=dir cargo run --profile essai -p planet --example banc
//! ```
//!
//! The scenes: `rift` (one continent, the forces free: its lid warms the
//! mantle and it tears), `collision` (two continents driven at each
//! other), `trench` (an ocean plate driven under a continent that stands
//! still: an active margin, its arc), `arcs` (two ocean plates driven at
//! each other: island arcs), `carried` (a continent carried 20 steps by
//! its plate: a rigid piece). To add one, write a function like these
//! and name it in `SCENES`.
//!
//! A scene runs on the planet's whole grid (its cell count is a
//! constant): its time is the time of its steps on 60,000 cells.

#![allow(
    clippy::print_stdout,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    clippy::naive_bytecount,
    reason = "a bench: pixels and rounded numbers"
)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use planet::history::{self, CONTINENT, CONTINENT_THICK, ISOSTASY_M, OCEAN, Scene};
use planet::{CELLS, History, Kind, ONE, Settings, Sphere, V3, from_lat_lon, km_between};

use png::{Rgb, png};

#[path = "common/png.rs"]
mod png;

#[path = "common/par.rs"]
mod par;

/// A scene: its settings, what is laid on the grid, and the window drawn.
struct Written {
    settings: Settings,
    scene: Scene,
    /// The centre of the window, tenths of a degree, and its half-width
    /// in degrees of longitude (its height is half its width).
    window: ((i64, i64), i64),
}

type Write = fn(&Sphere) -> Written;

const SCENES: [(&str, &str, Write); 5] = [
    ("rift", "one continent, the forces free", rift),
    (
        "collision",
        "two continents driven at each other",
        collision,
    ),
    ("trench", "an ocean plate driven under a continent", trench),
    ("arcs", "two ocean plates driven at each other", arcs),
    (
        "carried",
        "a continent carried 20 steps by its plate",
        carried,
    ),
];

/// A turn about the north pole, eastward at the equator at `mm` a year
/// (westward if negative).
fn east(mm: i64) -> V3 {
    [0, 0, mm * 1000]
}

/// The cells within `km` of a place (latitude and longitude in degrees).
fn cap(sphere: &Sphere, (lat, lon): (i64, i64), km: i64) -> Vec<bool> {
    let at = from_lat_lon(lat * 10, lon * 10);
    sphere
        .centre
        .iter()
        .map(|&x| km_between(x, at) < km)
        .collect()
}

/// Each cell on the plate whose centre is nearest (centres in degrees).
fn nearest(sphere: &Sphere, centres: &[(i64, i64)]) -> Vec<u8> {
    let at: Vec<V3> = centres
        .iter()
        .map(|&(lat, lon)| from_lat_lon(lat * 10, lon * 10))
        .collect();
    sphere
        .centre
        .iter()
        .map(|&x| {
            (0..at.len())
                .min_by_key(|&k| (km_between(x, at[k]), k))
                .expect("a plate") as u8
        })
        .collect()
}

/// The model's settings with nothing drawn by chance in the way: no
/// terrane, no hotspot.
fn plain(steps: usize) -> Settings {
    Settings {
        steps,
        terranes: (0, 0),
        hotspots: (0, 0),
        ..Settings::default()
    }
}

/// One continent of 3,000 km across on its plate, three ocean plates
/// around it, and the forces free: the lid warms the mantle under it, a
/// trough begins and grows cell by cell, the plate is cut once it has
/// crossed, and the ocean opens from where the trough is oldest (32
/// steps: the pull is weak here, the whole takes 150 My).
fn rift(sphere: &Sphere) -> Written {
    Written {
        settings: plain(32),
        scene: Scene {
            continent: cap(sphere, (0, 0), 3000),
            plate: nearest(sphere, &[(0, 0), (0, 120), (0, -120), (80, 180)]),
            continent_plates: 1,
            plates: 4,
            turns: Vec::new(),
        },
        window: ((0, 0), 60),
    }
}

/// Two continents on two plates (the two hemispheres), driven at each
/// other at 60 mm a year each: they meet after a few steps, pile up, and
/// weld once they have stopped.
fn collision(sphere: &Sphere) -> Written {
    let (a, b) = (cap(sphere, (0, -30), 1800), cap(sphere, (0, 30), 1800));
    Written {
        settings: plain(14),
        scene: Scene {
            continent: (0..CELLS).map(|c| a[c] || b[c]).collect(),
            plate: nearest(sphere, &[(0, -90), (0, 90)]),
            continent_plates: 2,
            plates: 2,
            turns: vec![(0, east(60)), (1, east(-60))],
        },
        window: ((0, 0), 60),
    }
}

/// A continent that stands still, and the ocean plate east of it driven
/// under it at 80 mm a year: the floor sinks under the coast (an active
/// margin), the margin rises.
fn trench(sphere: &Sphere) -> Written {
    let land = cap(sphere, (0, -20), 2000);
    Written {
        settings: plain(14),
        scene: Scene {
            plate: (0..CELLS)
                .map(|c| u8::from(!land[c] && sphere.centre[c][1] > -ONE / 3))
                .collect(),
            continent: land,
            continent_plates: 1,
            plates: 2,
            turns: vec![(0, east(0)), (1, east(-80))],
        },
        window: ((0, 0), 60),
    }
}

/// Two ocean plates (the two hemispheres) driven at each other at 50 mm
/// a year each, no continent near: the older floor sinks, the riding
/// edge grows island arcs.
fn arcs(sphere: &Sphere) -> Written {
    Written {
        settings: plain(14),
        scene: Scene {
            continent: cap(sphere, (0, 180), 600),
            plate: nearest(sphere, &[(0, -90), (0, 90)]),
            continent_plates: 1,
            plates: 2,
            turns: vec![(0, east(50)), (1, east(-50))],
        },
        window: ((0, 0), 60),
    }
}

/// A continent of 2,400 km across carried east by its plate at 90 mm a
/// year for 20 steps (about 80°): a rigid piece, its cells unchanged.
fn carried(sphere: &Sphere) -> Written {
    Written {
        settings: plain(20),
        scene: Scene {
            continent: cap(sphere, (10, -40), 1200),
            plate: nearest(sphere, &[(0, 0), (0, 180)]),
            continent_plates: 1,
            plates: 2,
            turns: vec![(0, east(90)), (1, east(0))],
        },
        window: ((0, 0), 80),
    }
}

/// A tile's width, pixels: its height is half of it.
const TILE: usize = 320;

fn main() {
    let out = std::env::var_os("PLANET_OUT").map_or_else(|| PathBuf::from("."), PathBuf::from);
    std::fs::create_dir_all(&out).expect("the output folder");
    let asked: Vec<String> = std::env::args().skip(1).collect();
    if let Some(unknown) = asked
        .iter()
        .find(|a| !SCENES.iter().any(|s| s.0 == a.as_str()))
    {
        let names: Vec<&str> = SCENES.iter().map(|s| s.0).collect();
        println!("no scene `{unknown}`; the scenes: {}", names.join(", "));
        return;
    }
    let chosen: Vec<&(&str, &str, Write)> = SCENES
        .iter()
        .filter(|s| asked.is_empty() || asked.iter().any(|a| a == s.0))
        .collect();
    let clock = Instant::now();
    let sphere = Sphere::new();
    let grid = clock.elapsed();
    let reports = par::par_map(&chosen, par::threads(), |&&(name, what, write)| {
        let clock = Instant::now();
        let mut written = write(&sphere);
        // The sea leaves the lower half of the scene's continents under
        // water (a shelf), whatever their size.
        let crust: i64 = (0..CELLS)
            .filter(|&c| written.scene.continent[c])
            .map(|c| i64::from(sphere.area_km2[c]))
            .sum();
        let whole: i64 = sphere.area_km2.iter().map(|&a| i64::from(a)).sum();
        written.settings.land_per_mille = crust * 550 / whole;
        let h = history::run_scene(1, &written.settings, &sphere, &written.scene);
        let ran = clock.elapsed();
        let path = out.join(format!("banc-{name}.png"));
        sheet(&path, &sphere, &h, written.window);
        let mut text = format!(
            "{name}: {what}. {} steps in {:.2} s, sheet {}\n",
            written.settings.steps,
            ran.as_secs_f64(),
            path.display()
        );
        text += &numbers(&sphere, &h);
        text
    });
    for report in reports {
        println!("{report}");
    }
    println!(
        "the grid {:.2} s, everything {:.2} s",
        grid.as_secs_f64(),
        clock.elapsed().as_secs_f64()
    );
}

/// A line a step: what the mechanisms did.
fn numbers(sphere: &Sphere, h: &History) -> String {
    let mut text = String::from(
        "  step  plates  continent k km2  thickest km  troughs  collision cells  trench cells  arcs made k km2  piled k km2\n",
    );
    for t in 0..=h.last() {
        let mut plates: Vec<u8> = h.plate[t].clone();
        plates.sort_unstable();
        plates.dedup();
        let km2: i64 = (0..CELLS)
            .filter(|&c| h.class[t][c] == CONTINENT)
            .map(|c| i64::from(sphere.area_km2[c]))
            .sum();
        let thickest = (0..CELLS)
            .filter(|&c| h.class[t][c] != OCEAN)
            .map(|c| h.thick_at[t][c])
            .max()
            .unwrap_or(0);
        let of = |k: Kind| h.kind[t].iter().filter(|&&v| v == k as u8 + 1).count();
        let mut troughs: Vec<u16> = h.rifts_at[t].iter().map(|r| r.1).collect();
        troughs.dedup();
        let _ = writeln!(
            text,
            "  {t:>4}  {:>6}  {:>15}  {:>11}  {:>7}  {:>15}  {:>12}  {:>15}  {:>11}",
            plates.len(),
            km2 / 1000,
            f64::from(thickest) / 10.0,
            troughs.len(),
            of(Kind::Collision),
            of(Kind::Trench),
            h.made_km2[t] / 1000,
            h.shortened_km2[t] / 1000,
        );
    }
    let events = format!(
        "  rifts {} (cut {}), collisions {}, welds {}, accretions {}, escapes {}, captures {}\n",
        h.rifts.len(),
        h.rifts
            .iter()
            .filter(|r| r.cut_my != history::NEVER)
            .count(),
        h.collisions.len(),
        h.welds.len(),
        h.accretions.len(),
        h.escapes.len(),
        h.captures.len(),
    );
    text + &events
}

fn mix(a: Rgb, b: Rgb, t: i64) -> Rgb {
    let t = t.clamp(0, 1000);
    [0, 1, 2].map(|k| ((i64::from(a[k]) * (1000 - t) + i64::from(b[k]) * t) / 1000) as u8)
}

/// A cell's colour at a step: the crust's height where there is crust
/// (green low, brown, grey high; pale blue below the datum), the floor's
/// age at sea (dark: old).
fn colour(h: &History, t: usize, c: usize) -> Rgb {
    if h.class[t][c] == OCEAN {
        return mix(
            [70, 120, 190],
            [14, 30, 80],
            i64::from(h.age[t][c]) * 1000 / 150,
        );
    }
    let m = i64::from(h.high_at[t][c])
        + i64::from(h.rough[h.home_at[t][c] as usize])
        + i64::from(h.broad[h.home_at[t][c] as usize]) / 2
        + (i64::from(h.thick_at[t][c]) - i64::from(CONTINENT_THICK)).max(0) * ISOSTASY_M / 10;
    if m <= 0 {
        mix([150, 190, 220], [90, 140, 200], -m * 1000 / 1500)
    } else if m < 800 {
        mix([92, 140, 84], [184, 160, 104], m * 1000 / 800)
    } else {
        mix([184, 160, 104], [240, 240, 240], (m - 800) * 1000 / 4000)
    }
}

/// The sheet of a scene's steps: a tile a step, the window on an
/// equirectangular map. Black: plate borders; magenta: a collision;
/// red: a trench or an arc's side; yellow: a ridge or a rift; a red
/// dot: a trough's cell (brown once abandoned).
fn sheet(path: &Path, sphere: &Sphere, h: &History, ((lat0, lon0), half): ((i64, i64), i64)) {
    let (w, hh) = (TILE, TILE / 2);
    let steps = h.last() + 1;
    let cols = 4.min(steps);
    let rows = steps.div_ceil(cols);
    let (sw, sh) = (cols * (w + 4) + 4, rows * (hh + 4) + 4);
    let mut px = vec![[255_u8; 3]; sw * sh];
    // Each pixel's cell, the same for every tile.
    let cell: Vec<usize> = (0..w * hh)
        .map(|i| {
            let (x, y) = ((i % w) as i64, (i / w) as i64);
            let lon = lon0 + (2 * x + 1 - w as i64) * half * 10 / w as i64;
            let lat = lat0 - (2 * y + 1 - hh as i64) * half * 10 / w as i64;
            sphere.cell_of(from_lat_lon(lat.clamp(-899, 899), lon))
        })
        .collect();
    for t in 0..steps {
        let (ox, oy) = (4 + (t % cols) * (w + 4), 4 + (t / cols) * (hh + 4));
        let mut trough = vec![0_u8; CELLS];
        for &(at, _, failed) in &h.rifts_at[t] {
            trough[sphere.cell_of(at)] = 1 + u8::from(failed);
        }
        for i in 0..w * hh {
            let c = cell[i];
            let (x, y) = (i % w, i / w);
            let edge = (x + 1 < w && h.plate[t][cell[i + 1]] != h.plate[t][c])
                || (y + 1 < hh && h.plate[t][cell[i + w]] != h.plate[t][c]);
            let kind = h.kind[t][c];
            let is = |k: Kind| kind == k as u8 + 1;
            px[(oy + y) * sw + ox + x] = if trough[c] == 1 {
                [255, 40, 0]
            } else if trough[c] == 2 {
                [120, 70, 30]
            } else if edge {
                [0, 0, 0]
            } else if is(Kind::Collision) {
                [200, 60, 230]
            } else if is(Kind::Trench) || is(Kind::Arc) || is(Kind::IslandArc) {
                [235, 35, 35]
            } else if is(Kind::Ridge) || is(Kind::Rift) {
                [255, 230, 70]
            } else {
                colour(h, t, c)
            };
        }
        digits(&mut px, sw, (ox + 3, oy + 3), t);
    }
    std::fs::write(path, png(sw, sh, &px)).expect("the sheet");
}

/// The step's number in a corner of its tile, 3 × 5 digits doubled.
fn digits(px: &mut [Rgb], sw: usize, (x0, y0): (usize, usize), n: usize) {
    const FONT: [u16; 10] = [
        0b111_101_101_101_111,
        0b010_110_010_010_111,
        0b111_001_111_100_111,
        0b111_001_111_001_111,
        0b101_101_111_001_001,
        0b111_100_111_001_111,
        0b111_100_111_101_111,
        0b111_001_001_001_001,
        0b111_101_111_101_111,
        0b111_101_111_001_111,
    ];
    let text = n.to_string();
    for y in 0..14 {
        for x in 0..text.len() * 8 + 4 {
            px[(y0 + y) * sw + x0 + x] = [255, 255, 255];
        }
    }
    for (k, d) in text.bytes().enumerate() {
        let glyph = FONT[usize::from(d - b'0')];
        for i in 0..15 {
            if glyph >> (14 - i) & 1 == 1 {
                let (gx, gy) = (i % 3, i / 3);
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    px[(y0 + 2 + gy * 2 + dy) * sw + x0 + 2 + k * 8 + gx * 2 + dx] = [0, 0, 0];
                }
            }
        }
    }
}
