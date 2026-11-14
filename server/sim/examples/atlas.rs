//! The atlas: for seeds 1 to 10, a continent born by `sim::plan`, drawn
//! for the project lead to judge (`content/ai-reserve/review/monde-genere/`).
//! Nothing here enters the game.
//!
//! ```text
//! cargo run --release -p sim --example atlas
//! ATLAS_OUT=some/folder ATLAS_SEEDS=1,4 cargo run --release -p sim --example atlas
//! ```
//!
//! Writes, per seed, the continent (relief, sea, lakes, rivers by
//! discharge, biomes), its rocks, its passages (where a walker crosses
//! the rivers wading) and a 64 km region around a lowland river; then a
//! contact sheet, `atlas.html` (every seed side by side, with its
//! numbers) and `mesures.md`. The PNG encoder is written here (a small
//! deflate), so the atlas adds no dependency.

// Drawing code reads as arithmetic: `x`, `y`, `i`, `c`, `r`, `h`.
#![allow(clippy::many_single_char_names)]

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use sim::Rock;
use sim::plan::measure::{self, Numbers, Passage};
use sim::plan::region::{self, REGION_SIDE, Water};
use sim::plan::{Biome, CELL_M, Continent, RIVER_ML, Region};

fn main() {
    let out = std::env::var_os("ATLAS_OUT").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../content/ai-reserve/review/monde-genere/atlas")
        },
        PathBuf::from,
    );
    std::fs::create_dir_all(&out).expect("the output folder");
    let seeds: Vec<u64> = std::env::var("ATLAS_SEEDS").ok().map_or_else(
        || (1..=10).collect(),
        |s| s.split(',').filter_map(|v| v.trim().parse().ok()).collect(),
    );
    let mut rows = Vec::new();
    let mut thumbs = Vec::new();
    for &seed in &seeds {
        let _ = std::fs::write("/proc/self/clear_refs", "5");
        let start = Instant::now();
        let continent = Continent::new(seed);
        let born = start.elapsed();
        let site = region::site(&continent);
        let region = Region::around(&continent, site, REGION_SIDE);
        let total = start.elapsed();
        let peak_mb = peak_memory_kb() / 1024;
        let numbers = measure::measure(&continent);
        let fingerprint = continent.fingerprint();
        let name = |what: &str| out.join(format!("{what}-{seed:02}.png"));
        let relief = draw_continent(&continent);
        std::fs::write(name("continent"), png(1024, 1024, &relief)).expect("write");
        std::fs::write(name("roches"), png(1024, 1024, &draw_rocks(&continent))).expect("write");
        std::fs::write(
            name("passages"),
            png(1024, 1024, &draw_passages(&continent)),
        )
        .expect("write");
        std::fs::write(name("climat"), png(1024, 1024, &draw_climate(&continent))).expect("write");
        std::fs::write(name("region"), png(1024, 1024, &draw_region(&region))).expect("write");
        std::fs::write(name("nappe"), png(1024, 1024, &draw_table(&continent))).expect("write");
        std::fs::write(
            name("temperature"),
            png(1024, 1024, &draw_temperature(&continent)),
        )
        .expect("write");
        std::fs::write(name("fleuves"), png(1024, 1024, &draw_network(&continent))).expect("write");
        thumbs.push(shrink(&relief, 1024, 4));
        println!(
            "seed {seed}: born {:.1} s, with region {:.1} s, peak {peak_mb} MB, fingerprint {fingerprint:016x}, water {}+{} bp, pieces {}/{} bp, longest {}/{} km",
            born.as_secs_f64(),
            total.as_secs_f64(),
            numbers.rivers_bp,
            numbers.lakes_bp,
            numbers.dry_piece_bp,
            numbers.wading_piece_bp,
            numbers.longest_dry_km,
            numbers.longest_wading_km,
        );
        rows.push(Row {
            seed,
            numbers,
            born_ms: born.as_millis(),
            total_ms: total.as_millis(),
            peak_mb,
            fingerprint,
            site,
        });
    }
    std::fs::write(
        out.join("planche.png"),
        png(256 * 5, 256 * seeds.len().div_ceil(5), &sheet(&thumbs, 5)),
    )
    .expect("write");
    std::fs::write(out.join("atlas.html"), page(&rows)).expect("write");
    std::fs::write(out.join("mesures.md"), measures(&rows)).expect("write");
    std::fs::write(out.join("mesures.tsv"), table(&rows)).expect("write");
    println!("written to {}", out.display());
}

struct Row {
    seed: u64,
    numbers: Numbers,
    born_ms: u128,
    total_ms: u128,
    peak_mb: u64,
    fingerprint: u64,
    site: (i32, i32),
}

/// The process's peak resident memory, in kB (Linux), reset before each
/// seed through `/proc/self/clear_refs`.
fn peak_memory_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse().ok()))
        })
        .unwrap_or(0)
}

// ---------------------------------------------------------------- drawing

type Rgb = [u8; 3];

const SEA: Rgb = [38, 74, 122];
const SEA_SHALLOW: Rgb = [70, 118, 168];
const LAKE: Rgb = [86, 146, 205];
const RIVER: Rgb = [40, 96, 190];

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

fn rock_colour(r: Rock, alluvium: bool) -> Rgb {
    if alluvium {
        return [176, 182, 132];
    }
    match r {
        Rock::Granite => [204, 160, 160],
        Rock::Limestone => [232, 220, 178],
        Rock::Chalk => [250, 250, 244],
        Rock::Sandstone => [214, 150, 84],
    }
}

/// A shading factor per mille from the slope toward the north-west light:
/// the drop across the pixel's neighbours (cm) over `divisor`.
fn shade(h: &[i32], side: usize, i: usize, divisor: i64) -> i64 {
    let (x, y) = (i % side, i / side);
    let at = |x: usize, y: usize| i64::from(h[y * side + x]);
    let (l, r) = (at(x.saturating_sub(1), y), at((x + 1).min(side - 1), y));
    let (u, d) = (at(x, y.saturating_sub(1)), at(x, (y + 1).min(side - 1)));
    (1000 + ((l - r) + (u - d)) / divisor).clamp(400, 1500)
}

fn lit(c: Rgb, factor: i64) -> Rgb {
    c.map(|v| u8::try_from((i64::from(v) * factor / 1000).clamp(0, 255)).expect("clamped"))
}

fn mix(a: Rgb, b: Rgb, t: i64) -> Rgb {
    [0, 1, 2].map(|k| {
        u8::try_from((i64::from(a[k]) * (1000 - t) + i64::from(b[k]) * t) / 1000).expect("mixed")
    })
}

fn sea_colour(depth_cm: i32) -> Rgb {
    mix(
        SEA_SHALLOW,
        SEA,
        (i64::from(-depth_cm) * 1000 / 300_000).clamp(0, 1000),
    )
}

/// River pixels: (discharge in m³/s, half-thickness in pixels, opacity).
/// The faintest class is the atlas's threshold, [`RIVER_ML`]: under it a
/// watercourse is a brook of the region, not drawn here.
const RIVER_CLASSES: [(u64, i32, i64); 4] = [
    (5_000, 1, 1000),
    (600, 0, 1000),
    (150, 0, 850),
    (RIVER_ML / 1_000_000, 0, 550),
];

/// Draws the rivers of a continent onto `img`.
fn draw_rivers(c: &Continent, img: &mut [Rgb]) {
    let side = c.grid.side;
    for i in 0..c.grid.len() {
        if c.sea[i] || c.lake[i] {
            continue;
        }
        let q = c.cubic_metres(i);
        let Some(&(_, half, alpha)) = RIVER_CLASSES.iter().find(|k| q >= k.0) else {
            continue;
        };
        let (x, y) = c.grid.xy(i);
        for dy in -half..=half {
            for dx in -half..=half {
                if c.grid.inside(x + dx, y + dy) {
                    let j = c.grid.index(x + dx, y + dy);
                    if !c.sea[j] {
                        img[j] = mix(img[j], RIVER, alpha);
                    }
                }
            }
        }
    }
    let _ = side;
}

fn draw_continent(c: &Continent) -> Vec<Rgb> {
    let side = c.grid.side;
    let mut img: Vec<Rgb> = (0..c.grid.len())
        .map(|i| {
            if c.sea[i] {
                return sea_colour(c.height[i]);
            }
            if c.lake[i] {
                return LAKE;
            }
            let base = biome_colour(c.biome[i]);
            let high = (i64::from(c.height[i]) * 1000 / 500_000).clamp(0, 350);
            lit(
                mix(base, [235, 228, 220], high),
                shade(&c.surface, side, i, 150),
            )
        })
        .collect();
    draw_rivers(c, &mut img);
    img
}

fn draw_rocks(c: &Continent) -> Vec<Rgb> {
    let side = c.grid.side;
    let mut img: Vec<Rgb> = (0..c.grid.len())
        .map(|i| {
            if c.sea[i] {
                sea_colour(c.height[i])
            } else if c.lake[i] {
                LAKE
            } else {
                lit(
                    rock_colour(c.rock[i], c.alluvium[i]),
                    shade(&c.surface, side, i, 150),
                )
            }
        })
        .collect();
    draw_rivers(c, &mut img);
    img
}

/// Rain from brown (dry) through green to blue (wet), with the
/// isotherms every 5 °C as dark lines.
fn draw_climate(c: &Continent) -> Vec<Rgb> {
    const SCALE: [(i64, Rgb); 6] = [
        (0, [150, 90, 40]),
        (250, [220, 190, 110]),
        (500, [200, 215, 120]),
        (900, [90, 170, 90]),
        (1600, [50, 120, 170]),
        (3000, [30, 50, 140]),
    ];
    let side = c.grid.side;
    (0..c.grid.len())
        .map(|i| {
            if c.sea[i] {
                return sea_colour(c.height[i]);
            }
            let r = i64::from(c.rain[i]);
            let k = SCALE.iter().rposition(|s| s.0 <= r).expect("a band");
            let colour = if k + 1 < SCALE.len() {
                let (a, b) = (SCALE[k], SCALE[k + 1]);
                mix(a.1, b.1, (r - a.0) * 1000 / (b.0 - a.0))
            } else {
                SCALE[k].1
            };
            let band = |t: i16| i64::from(t).div_euclid(50);
            let (x, y) = (i % side, i / side);
            let edge = (x + 1 < side && band(c.temperature[i + 1]) != band(c.temperature[i]))
                || (y + 1 < side && band(c.temperature[i + side]) != band(c.temperature[i]));
            if edge {
                [60, 40, 40]
            } else {
                lit(colour, shade(&c.surface, side, i, 400))
            }
        })
        .collect()
}

/// The water table's depth: blue-green where it meets the surface, green
/// within 10 m (a well dug by hand), yellow to 50 m, orange to 200 m,
/// brown deeper; springs as white dots; rivers faint.
fn draw_table(c: &Continent) -> Vec<Rgb> {
    const SCALE: [(i64, Rgb); 5] = [
        (0, [60, 170, 160]),
        (1_000, [120, 190, 90]),
        (5_000, [230, 214, 110]),
        (20_000, [214, 140, 70]),
        (60_000, [120, 70, 40]),
    ];
    let side = c.grid.side;
    let mut img: Vec<Rgb> = (0..c.grid.len())
        .map(|i| {
            if c.sea[i] {
                return sea_colour(c.height[i]);
            }
            if c.lake[i] {
                return LAKE;
            }
            let d = i64::from(c.water_table[i]);
            let k = SCALE.iter().rposition(|s| s.0 <= d).expect("a band");
            let colour = if k + 1 < SCALE.len() {
                let (a, b) = (SCALE[k], SCALE[k + 1]);
                mix(a.1, b.1, (d - a.0) * 1000 / (b.0 - a.0))
            } else {
                SCALE[k].1
            };
            lit(colour, shade(&c.surface, side, i, 250))
        })
        .collect();
    for (i, pixel) in img.iter_mut().enumerate() {
        if !c.sea[i] && !c.lake[i] && c.cubic_metres(i) >= 15 {
            *pixel = mix(*pixel, RIVER, 500);
        }
    }
    for i in 0..c.grid.len() {
        if c.spring[i] {
            let (x, y) = c.grid.xy(i);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (-1, 0), (0, -1)] {
                if c.grid.inside(x + dx, y + dy) {
                    img[c.grid.index(x + dx, y + dy)] = [250, 250, 255];
                }
            }
        }
    }
    img
}

/// The yearly mean: a hard band of white-violet under 0 °C, then blue
/// (0 °C) to green (15 °C), yellow (20) and orange (25) to red (30 and
/// over); isotherms every 5 °C as thin dark lines.
fn draw_temperature(c: &Continent) -> Vec<Rgb> {
    const SCALE: [(i64, Rgb); 7] = [
        (0, [70, 110, 190]),
        (50, [80, 160, 200]),
        (100, [110, 190, 150]),
        (150, [150, 200, 90]),
        (200, [230, 210, 80]),
        (250, [236, 140, 50]),
        (300, [190, 40, 40]),
    ];
    const FROZEN: Rgb = [232, 226, 246];
    let side = c.grid.side;
    (0..c.grid.len())
        .map(|i| {
            if c.sea[i] {
                return sea_colour(c.height[i]);
            }
            let t = i64::from(c.temperature[i]);
            if t < 0 {
                return FROZEN;
            }
            let k = SCALE.iter().rposition(|s| s.0 <= t).expect("a band");
            let colour = if k + 1 < SCALE.len() {
                let (a, b) = (SCALE[k], SCALE[k + 1]);
                mix(a.1, b.1, (t - a.0) * 1000 / (b.0 - a.0))
            } else {
                SCALE[k].1
            };
            let band = |t: i16| i64::from(t).div_euclid(50);
            let (x, y) = (i % side, i / side);
            let edge = (x + 1 < side && band(c.temperature[i + 1]) != band(c.temperature[i]))
                || (y + 1 < side && band(c.temperature[i + side]) != band(c.temperature[i]));
            if edge {
                [60, 50, 50]
            } else if c.lake[i] {
                mix(colour, LAKE, 500)
            } else {
                lit(colour, shade(&c.surface, side, i, 400))
            }
        })
        .collect()
}

/// The rivers the atlas shows, alone on a pale shaded ground: every
/// watercourse of at least [`RIVER_ML`], thicker as it grows, the lakes,
/// the sea.
fn draw_network(c: &Continent) -> Vec<Rgb> {
    let side = c.grid.side;
    let mut img: Vec<Rgb> = (0..c.grid.len())
        .map(|i| {
            if c.sea[i] {
                sea_colour(c.height[i])
            } else if c.lake[i] {
                LAKE
            } else {
                let high = (i64::from(c.height[i]) * 1000 / 400_000).clamp(0, 500);
                lit(
                    mix([238, 234, 222], [196, 186, 176], high),
                    shade(&c.surface, side, i, 200),
                )
            }
        })
        .collect();
    draw_rivers(c, &mut img);
    img
}

fn draw_passages(c: &Continent) -> Vec<Rgb> {
    let side = c.grid.side;
    let p = measure::passages(c, measure::WADE_CM);
    let low = measure::passages_low(c, measure::WADE_CM);
    (0..c.grid.len())
        .map(|i| {
            let colour = match p[i] {
                Passage::Sea => return sea_colour(c.height[i]),
                Passage::Lake => return LAKE,
                Passage::Barrier if low[i] == Passage::Crossing => return [236, 196, 30],
                Passage::Barrier => return [214, 36, 36],
                Passage::Crossing => return [30, 170, 70],
                Passage::Main => [226, 222, 206],
                Passage::Cut => [236, 160, 80],
            };
            lit(colour, shade(&c.surface, side, i, 250))
        })
        .collect()
}

fn draw_region(r: &Region) -> Vec<Rgb> {
    let side = r.grid.side;
    (0..r.grid.len())
        .map(|i| match r.water[i] {
            Water::Sea => sea_colour(r.height[i]),
            Water::Lake => LAKE,
            Water::River => RIVER,
            Water::Brook => [72, 136, 206],
            Water::Land => {
                let (x, y) = (i % side, i / side);
                let beside = [
                    (x.wrapping_sub(1), y),
                    (x + 1, y),
                    (x, y.wrapping_sub(1)),
                    (x, y + 1),
                ]
                .iter()
                .any(|&(a, b)| a < side && b < side && r.water[b * side + a] == Water::River);
                let base = biome_colour(r.biome[i]);
                let base = if beside { mix(base, RIVER, 600) } else { base };
                let base = if r.alluvium[i] {
                    mix(base, [196, 196, 150], 250)
                } else {
                    base
                };
                lit(base, shade(&r.height, side, i, 6))
            }
        })
        .collect()
}

/// Every `k`-th pixel's block averaged: a thumbnail.
fn shrink(img: &[Rgb], side: usize, k: usize) -> Vec<Rgb> {
    let s = side / k;
    let mut out = vec![[0; 3]; s * s];
    for y in 0..s {
        for x in 0..s {
            let mut sum = [0_u32; 3];
            for dy in 0..k {
                for dx in 0..k {
                    let p = img[(y * k + dy) * side + x * k + dx];
                    for c in 0..3 {
                        sum[c] += u32::from(p[c]);
                    }
                }
            }
            let n = u32::try_from(k * k).expect("small");
            out[y * s + x] = sum.map(|v| u8::try_from(v / n).expect("an average"));
        }
    }
    out
}

/// Thumbnails of 256 px laid out `columns` a row.
fn sheet(thumbs: &[Vec<Rgb>], columns: usize) -> Vec<Rgb> {
    let rows = thumbs.len().div_ceil(columns);
    let width = 256 * columns;
    let mut out = vec![[255; 3]; width * 256 * rows];
    for (k, t) in thumbs.iter().enumerate() {
        let (cx, cy) = ((k % columns) * 256, (k / columns) * 256);
        for y in 0..256 {
            for x in 0..256 {
                out[(cy + y) * width + cx + x] = t[y * 256 + x];
            }
        }
    }
    out
}

// ---------------------------------------------------------------- page

fn pct(bp: u64) -> String {
    format!("{}.{:02} %", bp / 100, bp % 100)
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

fn biomes_line(n: &Numbers) -> String {
    n.biomes_bp
        .iter()
        .filter(|(_, bp)| *bp > 0)
        .map(|(b, bp)| format!("{} {}", biome_name(*b), pct(*bp)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn largest_biome(n: &Numbers) -> u64 {
    n.biomes_bp.iter().map(|b| b.1).max().unwrap_or(0)
}

fn measures(rows: &[Row]) -> String {
    let mut s = String::from(
        "# Atlas — numbers per seed\n\n*Written by `cargo run --release -p sim --example atlas`. Continent of 1,024 × 1,024 cells of 2,048 m; region of 1,024 × 1,024 cells of 64 m.*\n\n\
         Targets: water 1–1.5 % of the land; largest piece ≥ 70 % dry-shod (≤ 0.5 m), ≥ 90 % wading (≤ 1 m); longest river with no crossing ≤ 30 km; no biome over 35 %.\n\n\
         | seed | land km² | rivers | lakes | water | piece ≤ 0.5 m | piece ≤ 1 m | longest no crossing ≤ 0.5 m | ≤ 1 m | largest biome | highest | largest river | born | with region | peak memory | fingerprint |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for r in rows {
        let n = &r.numbers;
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} | {} | {} km | {} km | {} | {} m | {} m³/s | {:.1} s | {:.1} s | {} MB | `{:016x}` |",
            r.seed,
            n.land_km2,
            pct(n.rivers_bp),
            pct(n.lakes_bp),
            pct(n.rivers_bp + n.lakes_bp),
            pct(n.dry_piece_bp),
            pct(n.wading_piece_bp),
            n.longest_dry_km,
            n.longest_wading_km,
            pct(largest_biome(n)),
            n.highest_m,
            n.largest_river_m3s,
            secs(r.born_ms),
            secs(r.total_ms),
            r.peak_mb,
            r.fingerprint
        );
    }
    s.push_str("\n## Biomes per seed\n\n");
    for r in rows {
        let _ = writeln!(s, "- **{}**: {}", r.seed, biomes_line(&r.numbers));
    }
    s
}

fn rock_name(r: Rock) -> &'static str {
    match r {
        Rock::Granite => "granite",
        Rock::Limestone => "limestone",
        Rock::Chalk => "chalk",
        Rock::Sandstone => "sandstone",
    }
}

/// Every number of every seed, a row each, tab-separated: what the
/// before-and-after pages are built from.
fn table(rows: &[Row]) -> String {
    let mut s = String::from(
        "seed\tland_km2\trivers_bp\tlakes_bp\tdry_piece_bp\twading_piece_bp\tlongest_dry_km\tlongest_wading_km\tdetour_median_km\tdetour_p90_km\tdetour_max_km\thighest_m\tlargest_river_m3s\tborn_ms\trocks_a_block_centi\talluvium_bp\tlongest_low_km\tdetour_low_median_km\tdetour_low_p90_km\tdetour_low_max_km\tlakes\tlargest_lake_km2\tsprings_10k_km2\ttable_median_m\ttable_near_bp\tbiomes\trocks\twarmth_bp\tcoldest_tenths\trivers\tgreat_rivers\tbiggest\n",
    );
    for r in rows {
        let n = &r.numbers;
        let biomes = n
            .biomes_bp
            .iter()
            .map(|(b, bp)| format!("{}:{bp}", biome_name(*b)))
            .collect::<Vec<_>>()
            .join(";");
        let rocks = n
            .rocks_bp
            .iter()
            .map(|(k, bp)| format!("{}:{bp}", rock_name(*k)))
            .collect::<Vec<_>>()
            .join(";");
        let warmth = n
            .warmth_bp
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(";");
        let biggest = n
            .biggest
            .iter()
            .map(|(q, km)| format!("{q}:{km}"))
            .collect::<Vec<_>>()
            .join(";");
        let _ = writeln!(
            s,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{biomes}\t{rocks}\t{warmth}\t{}\t{}\t{}\t{biggest}",
            r.seed,
            n.land_km2,
            n.rivers_bp,
            n.lakes_bp,
            n.dry_piece_bp,
            n.wading_piece_bp,
            n.longest_dry_km,
            n.longest_wading_km,
            n.detour_median_km,
            n.detour_p90_km,
            n.detour_max_km,
            n.highest_m,
            n.largest_river_m3s,
            r.born_ms,
            n.rocks_a_block_centi,
            n.alluvium_bp,
            n.longest_low_km,
            n.detour_low_median_km,
            n.detour_low_p90_km,
            n.detour_low_max_km,
            n.lakes,
            n.largest_lake_km2,
            n.springs_10k_km2,
            n.table_median_m,
            n.table_near_bp,
            n.coldest_tenths,
            n.rivers,
            n.great_rivers,
        );
    }
    s
}

#[allow(clippy::cast_precision_loss)]
fn secs(ms: u128) -> f64 {
    let ms = u32::try_from(ms).unwrap_or(u32::MAX);
    f64::from(ms) / 1000.0
}

fn mark(ok: bool) -> &'static str {
    if ok { "ok" } else { "off" }
}

fn page(rows: &[Row]) -> String {
    let mut s = String::from(
        r#"<!doctype html><meta charset="utf-8"><title>Atlas</title>
<style>
:root{--bg:#f6f4ef;--fg:#222;--muted:#666;--card:#fff;--line:#ddd;--ok:#2a7a3a;--off:#b03a2e}
@media (prefers-color-scheme:dark){:root{--bg:#1c1d1f;--fg:#e8e6e1;--muted:#9a9a9a;--card:#26272a;--line:#3a3b3e;--ok:#6fc27f;--off:#e07b6f}}
body{background:var(--bg);color:var(--fg);font:14px/1.45 system-ui,sans-serif;margin:0;padding:16px 20px}
h1{font-size:20px;margin:0 0 4px} p.lead{color:var(--muted);margin:0 0 16px;max-width:70em}
.seed{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:12px;margin:0 0 16px}
.seed h2{font-size:16px;margin:0 0 8px}
.imgs{display:flex;gap:8px;flex-wrap:wrap}
.imgs figure{margin:0;flex:1 1 240px;max-width:360px}
.imgs img{width:100%;height:auto;display:block;border-radius:4px;image-rendering:auto}
.imgs figcaption{color:var(--muted);font-size:12px}
table{border-collapse:collapse;margin-top:8px;font-variant-numeric:tabular-nums}
td,th{border-bottom:1px solid var(--line);padding:3px 10px 3px 0;text-align:left;vertical-align:top}
.ok{color:var(--ok)} .off{color:var(--off)}
.wrap{overflow-x:auto}
</style>
<h1>Atlas — ten continents</h1>
<p class="lead">Each continent is 1,024 × 1,024 cells of 2 km (2,097 km a side), born from its seed: uplift (ranges, plateaus, basins drawn from the seed), erosion by the rivers, lakes, rock from the erosion's depth, climate (latitude, winds, rain shadows), discharge, biomes. Rivers drawn by discharge (faint from 15 m³/s, full from 80, thick from 5,000). Passages: red where a river is deeper than a metre even on its riffles, green where a river over 10 m wide can be waded; orange land is cut off from the largest piece. Region: 65.5 km around a lowland river, cells of 64 m. Click an image for full size. Targets: water 1–1.5 %; largest piece ≥ 70 % (≤ 0.5 m) and ≥ 90 % (≤ 1 m); longest river with no crossing ≤ 30 km; no biome over 35 %.</p>
<p><a href="planche.png"><img src="planche.png" alt="contact sheet" style="max-width:100%"></a></p>
"#,
    );
    let km = |cells: i32| i64::from(cells) * CELL_M / 1000;
    for r in rows {
        let n = &r.numbers;
        let water = n.rivers_bp + n.lakes_bp;
        let _ = write!(
            s,
            r#"<div class="seed"><h2>Seed {seed}</h2><div class="imgs">
<figure><a href="continent-{seed:02}.png"><img src="continent-{seed:02}.png" alt="continent {seed}"></a><figcaption>relief, water, biomes</figcaption></figure>
<figure><a href="roches-{seed:02}.png"><img src="roches-{seed:02}.png" alt="rocks {seed}"></a><figcaption>rocks: granite pink-grey, limestone cream, chalk white, sandstone ochre, alluvium olive</figcaption></figure>
<figure><a href="passages-{seed:02}.png"><img src="passages-{seed:02}.png" alt="passages {seed}"></a><figcaption>passages, wading up to 1 m</figcaption></figure>
<figure><a href="region-{seed:02}.png"><img src="region-{seed:02}.png" alt="region {seed}"></a><figcaption>region of 65.5 km at 64 m, centred {cx} km east, {cy} km south of the north-west corner</figcaption></figure>
<figure><a href="temperature-{seed:02}.png"><img src="temperature-{seed:02}.png" alt="temperature {seed}"></a><figcaption>yearly mean: white-violet under 0 °C, blue 0, green 15, yellow 20, orange 25, red 30</figcaption></figure>
<figure><a href="fleuves-{seed:02}.png"><img src="fleuves-{seed:02}.png" alt="rivers {seed}"></a><figcaption>the rivers the atlas shows (from {threshold} m³/s)</figcaption></figure>
</div><div class="wrap"><table>
<tr><th>water on the land</th><td class="{w}">{water} (rivers {rivers}, lakes {lakes})</td></tr>
<tr><th>largest piece, dry-shod ≤ 0.5 m</th><td class="{d}">{dry}</td></tr>
<tr><th>largest piece, wading ≤ 1 m</th><td class="{wd}">{wade}</td></tr>
<tr><th>longest river with no crossing</th><td class="{l}">{ld} km dry-shod, {lw} km wading</td></tr>
<tr><th>biomes</th><td class="{b}">{biomes}</td></tr>
<tr><th>land, highest, largest river</th><td>{land} km², {high} m, {big} m³/s</td></tr>
<tr><th>born, with region, peak memory</th><td>{born:.1} s, {total:.1} s, {peak} MB</td></tr>
<tr><th>fingerprint</th><td><code>{fp:016x}</code></td></tr>
</table></div></div>
"#,
            seed = r.seed,
            threshold = RIVER_ML / 1_000_000,
            cx = km(r.site.0),
            cy = km(r.site.1),
            w = mark((100..=150).contains(&water)),
            water = pct(water),
            rivers = pct(n.rivers_bp),
            lakes = pct(n.lakes_bp),
            d = mark(n.dry_piece_bp >= 7_000),
            dry = pct(n.dry_piece_bp),
            wd = mark(n.wading_piece_bp >= 9_000),
            wade = pct(n.wading_piece_bp),
            l = mark(n.longest_wading_km <= 30),
            ld = n.longest_dry_km,
            lw = n.longest_wading_km,
            b = mark(largest_biome(n) <= 3_500),
            biomes = biomes_line(n),
            land = n.land_km2,
            high = n.highest_m,
            big = n.largest_river_m3s,
            born = secs(r.born_ms),
            total = secs(r.total_ms),
            peak = r.peak_mb,
            fp = r.fingerprint,
        );
    }
    s
}

// ---------------------------------------------------------------- PNG

/// An RGB PNG, rows filtered by whichever of PNG's five filters makes the
/// smallest sum, compressed by [`deflate`].
fn png(width: usize, height: usize, pixels: &[Rgb]) -> Vec<u8> {
    let stride = width * 3;
    let flat: Vec<u8> = pixels.iter().flatten().copied().collect();
    let mut raw = Vec::with_capacity((stride + 1) * height);
    let zero = vec![0_u8; stride];
    for y in 0..height {
        let row = &flat[y * stride..(y + 1) * stride];
        let up = if y == 0 {
            &zero[..]
        } else {
            &flat[(y - 1) * stride..y * stride]
        };
        let mut best: Option<(u64, u8, Vec<u8>)> = None;
        for filter in 0..5_u8 {
            let line: Vec<u8> = (0..stride)
                .map(|k| {
                    let a = if k >= 3 { row[k - 3] } else { 0 };
                    let b = up[k];
                    let c = if k >= 3 { up[k - 3] } else { 0 };
                    let p = match filter {
                        0 => 0,
                        1 => a,
                        2 => b,
                        3 => u8::midpoint(a, b),
                        _ => paeth(a, b, c),
                    };
                    row[k].wrapping_sub(p)
                })
                .collect();
            let cost: u64 = line
                .iter()
                .map(|&v| u64::from(v.min(0_u8.wrapping_sub(v))))
                .sum();
            if best.as_ref().is_none_or(|b| cost < b.0) {
                best = Some((cost, filter, line));
            }
        }
        let (_, filter, line) = best.expect("a filter");
        raw.push(filter);
        raw.extend_from_slice(&line);
    }
    let mut zlib = vec![0x78, 0x01];
    zlib.extend_from_slice(&deflate(&raw));
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    zlib.extend_from_slice(&(b << 16 | a).to_be_bytes());
    let mut header = Vec::new();
    header.extend_from_slice(&u32::try_from(width).expect("small").to_be_bytes());
    header.extend_from_slice(&u32::try_from(height).expect("small").to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    chunk(&mut out, *b"IHDR", &header);
    chunk(&mut out, *b"IDAT", &zlib);
    chunk(&mut out, *b"IEND", &[]);
    out
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = (
        (p - i16::from(a)).abs(),
        (p - i16::from(b)).abs(),
        (p - i16::from(c)).abs(),
    );
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&u32::try_from(data.len()).expect("a chunk").to_be_bytes());
    let mut crc = !0_u32;
    for byte in kind.iter().chain(data) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&(!crc).to_be_bytes());
}

/// Bits written least significant first, as deflate wants.
struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn put(&mut self, value: u32, count: u32) {
        self.acc |= u64::from(value) << self.n;
        self.n += count;
        while self.n >= 8 {
            self.out
                .push(u8::try_from(self.acc & 0xff).expect("a byte"));
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// A Huffman code, written most significant bit first.
    fn code(&mut self, code: u32, len: u32) {
        let reversed = code.reverse_bits() >> (32 - len);
        self.put(reversed, len);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out
                .push(u8::try_from(self.acc & 0xff).expect("a byte"));
        }
        self.out
    }
}

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// A literal or length symbol in deflate's fixed Huffman code.
fn fixed(bits: &mut Bits, symbol: u32) {
    match symbol {
        0..=143 => bits.code(0x30 + symbol, 8),
        144..=255 => bits.code(0x190 + symbol - 144, 9),
        256..=279 => bits.code(symbol - 256, 7),
        _ => bits.code(0xc0 + symbol - 280, 8),
    }
}

/// Raw deflate: one block of the fixed Huffman code, with LZ77 matches
/// found through a hash of three bytes and a chain of earlier places.
fn deflate(data: &[u8]) -> Vec<u8> {
    const WINDOW: usize = 32_768;
    const CHAIN: usize = 24;
    let mut bits = Bits {
        out: Vec::with_capacity(data.len() / 3),
        acc: 0,
        n: 0,
    };
    bits.put(1, 1); // the last block
    bits.put(1, 2); // fixed Huffman
    let mut head = vec![usize::MAX; 1 << 15];
    let mut prev = vec![usize::MAX; WINDOW];
    let hash3 = |i: usize| -> usize {
        ((usize::from(data[i]) << 10) ^ (usize::from(data[i + 1]) << 5) ^ usize::from(data[i + 2]))
            & 0x7fff
    };
    let mut i = 0;
    let insert = |i: usize, head: &mut Vec<usize>, prev: &mut Vec<usize>| {
        if i + 2 < data.len() {
            let h = hash3(i);
            prev[i % WINDOW] = head[h];
            head[h] = i;
        }
    };
    while i < data.len() {
        let (mut best_len, mut best_dist) = (0, 0);
        if i + 2 < data.len() {
            let mut candidate = head[hash3(i)];
            let mut tries = 0;
            while candidate != usize::MAX && i - candidate <= WINDOW && tries < CHAIN {
                let max = (data.len() - i).min(258);
                let mut len = 0;
                while len < max && data[candidate + len] == data[i + len] {
                    len += 1;
                }
                if len > best_len {
                    best_len = len;
                    best_dist = i - candidate;
                    if len == max {
                        break;
                    }
                }
                let next = prev[candidate % WINDOW];
                if next == usize::MAX || next >= candidate {
                    break;
                }
                candidate = next;
                tries += 1;
            }
        }
        if best_len >= 3 {
            let len = u16::try_from(best_len).expect("at most 258");
            let k = LENGTH_BASE
                .iter()
                .rposition(|&b| b <= len)
                .expect("a length code");
            fixed(&mut bits, 257 + u32::try_from(k).expect("small"));
            bits.put(u32::from(len - LENGTH_BASE[k]), u32::from(LENGTH_EXTRA[k]));
            let dist = u16::try_from(best_dist).expect("within the window");
            let d = DIST_BASE
                .iter()
                .rposition(|&b| b <= dist)
                .expect("a distance code");
            bits.code(u32::try_from(d).expect("small"), 5);
            bits.put(u32::from(dist - DIST_BASE[d]), u32::from(DIST_EXTRA[d]));
            for k in 0..best_len {
                insert(i + k, &mut head, &mut prev);
            }
            i += best_len;
        } else {
            fixed(&mut bits, u32::from(data[i]));
            insert(i, &mut head, &mut prev);
            i += 1;
        }
    }
    fixed(&mut bits, 256);
    bits.finish()
}
