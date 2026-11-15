//! Contact sheets of the continent's windows, for the project lead's eye
//! (the safety net of 15 November 2026, beside the invariants of
//! `plan::net`): for every land biome of a seed, 24 windows of one atlas
//! cell (2 km) drawn by the seed on one PNG sheet of 6 × 4 tiles of
//! 256 px (8 m a pixel): the ground by biome, the forest, the rivers at
//! their width with their fords, the brooks, the lakes, the trees as
//! dots (a pixel darkens with the trees on it) and 20 m contours; each
//! tile labelled with its cell and its numbers (the river's flow, the
//! trees a hectare). Nothing here enters the game.
//!
//! ```text
//! cargo run --release -p sim --example windows_sheet -- DIR [SEED]
//! ```
//!
//! Writes `DIR/<biome>.png` and `DIR/filet.html`, and prints one line a
//! window.

// Drawing code reads as arithmetic: `x`, `y`, `i`, `c`, `h`.
#![allow(
    clippy::many_single_char_names,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use sim::content::Species;
use sim::plan::{Biome, Continent, VIVARIUM_SEED, Window};
use sim::{CHUNK_SIZE, ChunkPos, Map, OpenGround, Pos, Terrain};

use png::{Rgb, png};

#[path = "common/png.rs"]
mod png;

/// Windows a sheet, and their layout.
const PER_SHEET: usize = 24;
const COLUMNS: usize = 6;

/// A tile's side in pixels, and the metres each pixel covers (a cell of
/// 2,048 m over 256 px).
const TILE_PX: usize = 256;
const M_PER_PX: i32 = 8;

/// Contours every 20 m (in decimetres).
const CONTOUR_DM: i32 = 200;

/// Cells kept clear of the continent's edge (the region around a window
/// reaches five cells each way).
const EDGE_CELLS: i32 = 6;

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().unwrap_or_else(|| {
        eprintln!("usage: windows_sheet DIR [SEED]");
        std::process::exit(2);
    }));
    let seed: u64 = args
        .next()
        .map_or(VIVARIUM_SEED, |s| s.parse().expect("a seed"));
    std::fs::create_dir_all(&out).expect("the output folder");
    let species: Vec<Species> =
        ron::from_str(include_str!("../../../content/species.ron")).expect("species.ron");
    let open: OpenGround =
        ron::from_str(include_str!("../../../content/open_ground.ron")).expect("open_ground.ron");
    let start = Instant::now();
    let continent = Continent::new(seed);
    println!(
        "seed {seed}: the continent in {:.1} s",
        start.elapsed().as_secs_f64()
    );
    let mut page = String::new();
    for biome in Biome::LAND {
        let cells = pick(&continent, biome);
        if cells.is_empty() {
            continue;
        }
        let mut thumbs = Vec::new();
        for &cell in &cells {
            let start = Instant::now();
            let (thumb, line) = draw(&continent, cell, &species, &open);
            println!("{biome:?} {line} ({:.1} s)", start.elapsed().as_secs_f64());
            thumbs.push(thumb);
        }
        let name = format!("{}.png", biome_name(biome));
        let rows = thumbs.len().div_ceil(COLUMNS);
        std::fs::write(
            out.join(&name),
            png(COLUMNS * TILE_PX, rows * TILE_PX, &sheet(&thumbs)),
        )
        .expect("the sheet");
        let _ = write!(
            page,
            "<h2>{biome:?} ({} windows)</h2>\n<img src=\"{name}\" alt=\"{biome:?}\">\n",
            thumbs.len()
        );
    }
    std::fs::write(out.join("filet.html"), html(seed, &page)).expect("the page");
    println!("done in {:.0} s", start.elapsed().as_secs_f64());
}

/// A mixing hash (splitmix64) for the example's own draws.
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The sheet's cells of `biome`: the highest-rolled land cells of it,
/// clear of the edge, the same for the same seed.
fn pick(c: &Continent, biome: Biome) -> Vec<(i32, i32)> {
    let side = i32::try_from(c.grid.side).expect("small");
    let mut rolled: Vec<(u64, usize)> = (0..c.grid.len())
        .filter(|&i| {
            let (x, y) = c.grid.xy(i);
            !c.sea[i]
                && !c.lake[i]
                && c.biome[i] == biome
                && (EDGE_CELLS..side - EDGE_CELLS).contains(&x)
                && (EDGE_CELLS..side - EDGE_CELLS).contains(&y)
        })
        .map(|i| (mix(c.seed ^ mix(i as u64)), i))
        .collect();
    rolled.sort_unstable_by(|a, b| b.cmp(a));
    rolled
        .iter()
        .take(PER_SHEET)
        .map(|&(_, i)| c.grid.xy(i))
        .collect()
}

fn biome_name(b: Biome) -> &'static str {
    match b {
        Biome::Sea => "sea",
        Biome::Lake => "lake",
        Biome::Desert => "desert",
        Biome::Steppe => "steppe",
        Biome::WoodedSteppe => "wooded-steppe",
        Biome::Grassland => "grassland",
        Biome::Broadleaf => "broadleaf",
        Biome::Jungle => "jungle",
        Biome::MixedForest => "mixed-forest",
        Biome::Conifer => "conifer",
        Biome::Marsh => "marsh",
        Biome::Tundra => "tundra",
        Biome::Alpine => "alpine",
    }
}

/// The ground's colour by biome (the `open_ground` example's palette).
fn ground(b: Biome) -> Rgb {
    match b {
        Biome::Grassland => [190, 220, 140],
        Biome::WoodedSteppe => [200, 200, 120],
        Biome::Steppe => [220, 210, 150],
        Biome::Desert => [232, 214, 170],
        Biome::Marsh => [150, 190, 170],
        Biome::Tundra => [196, 200, 186],
        Biome::Alpine => [214, 214, 220],
        b if b.is_forest() => [140, 190, 110],
        _ => [210, 210, 190],
    }
}

const FOREST: Rgb = [60, 110, 50];
const FORD: Rgb = [140, 215, 225];
const BROOK: Rgb = [90, 150, 210];
const LAKE: Rgb = [86, 146, 205];

/// A river's colour by its depth in half-metres.
fn river(depth: u8) -> Rgb {
    let d = u32::from(depth);
    [
        30,
        u8::try_from(110_u32.saturating_sub(d * 8)).expect("small"),
        u8::try_from(200_u32.saturating_sub(d * 7)).expect("small"),
    ]
}

fn mixed(a: Rgb, b: Rgb, t: u32) -> Rgb {
    let mut out = [0; 3];
    for c in 0..3 {
        out[c] = u8::try_from((u32::from(a[c]) * (1000 - t) + u32::from(b[c]) * t) / 1000)
            .expect("a mix");
    }
    out
}

/// One window drawn as a tile, and its line of numbers.
fn draw(
    c: &Continent,
    cell: (i32, i32),
    species: &[Species],
    open: &OpenGround,
) -> (Vec<Rgb>, String) {
    let i = c.grid.index(cell.0, cell.1);
    let flow = c.cubic_metres(i);
    let window = Window::of_cell(c, cell);
    let map = Map::on_window(window.clone(), c.seed);
    let side = i32::try_from(map.width()).expect("small");
    let n = TILE_PX;
    // The ground sampled at each pixel's centre tile, and its contour
    // band.
    let mut img = vec![[0_u8; 3]; n * n];
    let mut band = vec![0_i32; n * n];
    let (mut forest_px, mut land_tiles) = (0_usize, 0_u64);
    for py in 0..n {
        for px in 0..n {
            let p = Pos::new(
                i32::try_from(px).expect("small") * M_PER_PX + M_PER_PX / 2,
                i32::try_from(py).expect("small") * M_PER_PX + M_PER_PX / 2,
            );
            let k = py * n + px;
            img[k] = match map.get(p) {
                Some(Terrain::Forest) => {
                    forest_px += 1;
                    FOREST
                }
                Some(Terrain::Rock) | None => [120, 120, 120],
                _ => ground(map.biome(p).unwrap_or(Biome::Grassland)),
            };
            band[k] = i32::from(map.elevation(p).unwrap_or(0)).div_euclid(CONTOUR_DM);
        }
    }
    // The waters, every wet tile painted on its pixel (the deepest wins).
    let mut wet_tiles = 0_u64;
    let mut deepest = vec![0_u8; n * n];
    for (x, y, wet) in window.waters() {
        wet_tiles += 1;
        let k = (y / M_PER_PX as u32) as usize * n + (x / M_PER_PX as u32) as usize;
        let rank = match wet.terrain {
            Terrain::Water if wet.current == 0 => 1,
            Terrain::Brook => 2,
            Terrain::Ford => 3,
            _ => 4 + wet.depth,
        };
        if rank > deepest[k] {
            deepest[k] = rank;
            img[k] = match wet.terrain {
                Terrain::Water if wet.current == 0 => LAKE,
                Terrain::Brook => BROOK,
                Terrain::Ford => FORD,
                _ => river(wet.depth),
            };
        }
    }
    land_tiles += u64::from(map.width()) * u64::from(map.height()) - wet_tiles;
    // The trees: a pixel darkens with the trees standing on it.
    let chunks = i32::try_from(map.width().div_ceil(CHUNK_SIZE)).expect("small");
    let mut count = vec![0_u32; n * n];
    let mut trees = 0_u64;
    for cy in 0..chunks {
        for cx in 0..chunks {
            for t in sim::trees::trees_in(&map, species, open, c.seed, ChunkPos::new(cx, cy)) {
                let p = t.at.tile();
                if p.x >= 0 && p.y >= 0 && p.x < side && p.y < side {
                    let (px, py) = (
                        usize::try_from(p.x / M_PER_PX).expect("inside"),
                        usize::try_from(p.y / M_PER_PX).expect("inside"),
                    );
                    count[py * n + px] += 1;
                    trees += 1;
                }
            }
        }
    }
    for k in 0..n * n {
        if count[k] > 0 && deepest[k] == 0 {
            img[k] = mixed(img[k], [20, 40, 15], (count[k].min(6) * 700 / 6).min(700));
        }
    }
    // The contours: where the band changes against the left or upper
    // pixel.
    let mut lined = img.clone();
    for y in 1..n {
        for x in 1..n {
            let k = y * n + x;
            if deepest[k] == 0 && (band[k] != band[k - 1] || band[k] != band[k - n]) {
                lined[k] = mixed(img[k], [60, 40, 20], 550);
            }
        }
    }
    let per_ha = trees as f64 * 10_000.0 / land_tiles.max(1) as f64;
    let label = format!("{},{} {}m3/s {:.1}t/ha", cell.0, cell.1, flow, per_ha);
    text(&mut lined, n, 3, 3, &label);
    let line = format!(
        "cell ({}, {}) {} m³/s, {} trees ({per_ha:.1}/ha), forest {} % of the tile, {} wet tiles",
        cell.0,
        cell.1,
        flow,
        trees,
        forest_px * 100 / (n * n),
        wet_tiles
    );
    (lined, line)
}

/// Thumbnails laid out [`COLUMNS`] a row, a thin white gutter between.
fn sheet(thumbs: &[Vec<Rgb>]) -> Vec<Rgb> {
    let rows = thumbs.len().div_ceil(COLUMNS);
    let width = COLUMNS * TILE_PX;
    let mut out = vec![[255; 3]; width * TILE_PX * rows];
    for (k, t) in thumbs.iter().enumerate() {
        let (cx, cy) = ((k % COLUMNS) * TILE_PX, (k / COLUMNS) * TILE_PX);
        for y in 0..TILE_PX {
            for x in 0..TILE_PX {
                let edge = x == 0 || y == 0 || x == TILE_PX - 1 || y == TILE_PX - 1;
                out[(cy + y) * width + cx + x] = if edge { [255; 3] } else { t[y * TILE_PX + x] };
            }
        }
    }
    out
}

/// A 5 × 7 glyph of the label's alphabet, row by row.
fn glyph(ch: char) -> [&'static str; 7] {
    match ch {
        '0' => [
            " ### ", "#   #", "#  ##", "# # #", "##  #", "#   #", " ### ",
        ],
        '1' => [
            "  #  ", " ##  ", "  #  ", "  #  ", "  #  ", "  #  ", " ### ",
        ],
        '2' => [
            " ### ", "#   #", "    #", "   # ", "  #  ", " #   ", "#####",
        ],
        '3' => [
            "#####", "   # ", "  #  ", "   # ", "    #", "#   #", " ### ",
        ],
        '4' => [
            "   # ", "  ## ", " # # ", "#  # ", "#####", "   # ", "   # ",
        ],
        '5' => [
            "#####", "#    ", "#### ", "    #", "    #", "#   #", " ### ",
        ],
        '6' => [
            "  ## ", " #   ", "#    ", "#### ", "#   #", "#   #", " ### ",
        ],
        '7' => [
            "#####", "    #", "   # ", "  #  ", " #   ", " #   ", " #   ",
        ],
        '8' => [
            " ### ", "#   #", "#   #", " ### ", "#   #", "#   #", " ### ",
        ],
        '9' => [
            " ### ", "#   #", "#   #", " ####", "    #", "   # ", " ##  ",
        ],
        ',' => [
            "     ", "     ", "     ", "     ", "  ## ", "  ## ", " #   ",
        ],
        '.' => [
            "     ", "     ", "     ", "     ", "     ", "  ## ", "  ## ",
        ],
        '/' => [
            "    #", "    #", "   # ", "  #  ", " #   ", "#    ", "#    ",
        ],
        '-' => [
            "     ", "     ", "     ", "#####", "     ", "     ", "     ",
        ],
        't' => [
            " #   ", " #   ", "#### ", " #   ", " #   ", " #  #", "  ## ",
        ],
        'h' => [
            "#    ", "#    ", "#    ", "#### ", "#   #", "#   #", "#   #",
        ],
        'a' => [
            "     ", "     ", " ####", "    #", " ####", "#   #", " ####",
        ],
        'm' => [
            "     ", "     ", "## # ", "# # #", "# # #", "# # #", "# # #",
        ],
        's' => [
            "     ", "     ", " ####", "#    ", " ### ", "    #", "#### ",
        ],
        _ => [
            "     ", "     ", "     ", "     ", "     ", "     ", "     ",
        ],
    }
}

/// `label` written in black on a white box at `(x0, y0)` of `img`.
fn text(img: &mut [Rgb], n: usize, x0: usize, y0: usize, label: &str) {
    let width = label.chars().count() * 6 + 3;
    for y in y0..(y0 + 11).min(n) {
        for x in x0..(x0 + width).min(n) {
            img[y * n + x] = [255, 255, 255];
        }
    }
    for (k, ch) in label.chars().enumerate() {
        let g = glyph(ch);
        for (row, line) in g.iter().enumerate() {
            for (col, c) in line.chars().enumerate() {
                let (x, y) = (x0 + 2 + k * 6 + col, y0 + 2 + row);
                if c == '#' && x < n && y < n {
                    img[y * n + x] = [0, 0, 0];
                }
            }
        }
    }
}

/// The page that shows every sheet, with its legend.
fn html(seed: u64, sheets: &str) -> String {
    let mut legend = String::new();
    let items: BTreeMap<&str, Rgb> = [
        ("forest", FOREST),
        ("grassland", ground(Biome::Grassland)),
        ("wooded steppe", ground(Biome::WoodedSteppe)),
        ("steppe", ground(Biome::Steppe)),
        ("desert", ground(Biome::Desert)),
        ("marsh", ground(Biome::Marsh)),
        ("river", river(4)),
        ("ford", FORD),
        ("brook", BROOK),
        ("lake", LAKE),
    ]
    .into_iter()
    .collect();
    for (name, c) in items {
        let _ = write!(
            legend,
            "<span style=\"display:inline-block;width:1em;height:1em;background:rgb({},{},{});vertical-align:middle\"></span> {name} &nbsp; ",
            c[0], c[1], c[2]
        );
    }
    format!(
        "<!doctype html>\n<meta charset=\"utf-8\">\n<title>Filet du continent, graine {seed}</title>\n\
         <style>body{{font-family:sans-serif;margin:1em}}img{{max-width:100%;image-rendering:pixelated}}</style>\n\
         <h1>Fenêtres de 2 km de la graine {seed}, 24 par biome</h1>\n\
         <p>Une tuile = une cellule de l'atlas (2 048 m, 8 m par pixel). Étiquette : cellule, débit du fleuve, arbres par hectare. \
         Les pixels foncent avec les arbres qu'ils portent ; lignes brunes : courbes de niveau de 20 m.</p>\n<p>{legend}</p>\n{sheets}"
    )
}
