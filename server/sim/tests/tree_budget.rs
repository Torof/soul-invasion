//! The trees' budget per screen (D39, E4; the biomes study of 19 November
//! 2026, approach A): sample regions of every world the game makes have
//! their trees placed as the server places them; screens of the client's
//! canvas (1280 × 720, stretched to any monitor) are laid over them at the
//! farthest, the default and the closest zoom; the kinds each screen shows
//! and the kinds its view holds (19 × 19 chunks) are counted, and multiplied
//! by each kind's image bytes as the graphics card holds them, read from the
//! outlines the art tool writes (`client/art/trees/outlines.json`: each
//! image's size, padded to the compression's blocks, and the share of its
//! rectangle its outline covers). It fails over the lines:
//!
//! - the trees' video memory, at any zoom and place, within D39's 64 MB;
//! - what one frame uploads to the card within `UPLOAD_LINE` (about 2 ms
//!   of the main thread at the measured 1.5–1.9 ms a MB);
//! - the trees' overdraw within `OVERDRAW_LINE` screens of outlines on
//!   99 screens in 100 (the worst is printed).
//!
//! A kind of tree with no image yet is judged as one image of its shown
//! height at the shipped sharpness and the mean shape of today's images
//! (marked `~` in the table), so a new species or biome is weighed before
//! its art is made. The table is printed:
//! `cargo test -p sim --test tree_budget -- --nocapture`.
//!
//! The client's rules are mirrored here, each next to its source: change
//! one there, change it here.

// Screen pixels and texel counts are small: every cast is exact or a
// deliberate rounding of a measure.
#![allow(
    clippy::disallowed_methods,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "screen pixels, texel and tile counts bounded by the world size: casts exact or deliberately rounded; the client's float rules mirrored, not generation (D24)"
)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Deserialize;
use sim::OpenGround;
use sim::content::Species;
use sim::map::{CHUNK_SIZE, ChunkPos, Map, Pos};
use sim::plan::{Continent, Region, Window, region};

const SPECIES: &str = include_str!("../../../content/species.ron");
const OPEN_GROUND: &str = include_str!("../../../content/open_ground.ron");
const OUTLINES: &str = include_str!("../../../client/art/trees/outlines.json");

/// The client's canvas, in its pixels (`project.godot`, `canvas_items`
/// stretch): the world shown does not depend on the monitor.
const CANVAS: (f64, f64) = (1280.0, 720.0);
/// The camera's farthest, default and closest zooms (`camera.gd`).
const ZOOMS: [f64; 3] = [0.4, 1.5, 2.5];
/// The stretch of the project lead's screen, 3072 × 1728 (D39, E2): the
/// largest in use, where kinds sharpen soonest.
const LEAD_STRETCH: f64 = 2.4;
/// `Iso.TILE_W`, `Iso.TILE_H`, `Iso.PX_PER_M`.
const TILE_W: f64 = 64.0;
const TILE_H: f64 = 32.0;
const PX_PER_M: f64 = 64.0;
/// `Trees.SIZE_JITTER`.
const SIZE_JITTER: f64 = 0.12;
/// `Trees.COARSE` and `Trees.FINE_MARGIN`.
const COARSE: u32 = 1;
const FINE_MARGIN: f64 = 0.15;
/// `Trees.Stand.CELL_PX`: a tree gets a canvas item (and counts as shown)
/// when its image covers a cell of this size the view touches.
const CELL_PX: f64 = 128.0;
/// The view held around the camera's chunk, each way (`net.rs`
/// `VIEW_RADIUS`: 19 × 19 chunks).
const VIEW_RADIUS: i32 = 9;
/// Pixels a shown metre the images are shipped at
/// (`content/tools/flora_art.py`, D39): for a kind with no image yet.
const SHIPPED_PX_PER_M: f64 = 307.2;
/// Bytes of a block of 4 × 4 texels in BC7 (D39: all art is BC7).
const BLOCK_BYTES: u64 = 16;

/// The lines. Megabytes are a million bytes, as the study's table.
const MB: f64 = 1_000_000.0;
/// D39, E4: the trees' share of the video memory.
const TREES_LINE_MB: f64 = 64.0;
/// What one frame may upload: about 2 ms of the main thread; room for
/// today's largest coarse image (1.33 MB) and for a quarter of the largest
/// whole one.
const UPLOAD_LINE_MB: f64 = 1.4;
/// Screens of tree outlines drawn over one screen, on 99 screens of 100.
/// The study set 7 screens from the audit of 18 November (6.1–6.4 screens
/// of the images' rectangles at the forest heart, 6.7 ms of the GPU); what
/// the card shades is the outlines (D39, B6: about 0.72 of the
/// rectangles, 4.6 screens there), and in rectangles a twentieth of the
/// wooded screens are over 7 today. A single screen of the valley at zoom
/// 2.5 has 7.6: the frame-time line is held by the many, the worst shown.
const OVERDRAW_LINE: f64 = 7.0;

/// The audit's forest heart, a tile of the valley.
const HEART: Pos = Pos::new(368, 848);

/// Tiles between two cameras laid over a region.
const STEP: i32 = 64;

/// An image as the art tool wrote its outline.
#[derive(Deserialize)]
struct Outline {
    /// The texture's size as it reports itself.
    texture: [f64; 2],
    /// The image as the card holds it, padded to the blocks.
    image: [u32; 2],
    /// The share of its rectangle the outline covers.
    covered: f64,
}

/// A kind of tree as the client draws it.
struct Kind {
    id: String,
    /// Its height on screen at zoom 1, in the canvas' pixels (D40).
    shown_px: f64,
    /// Its images: (texture width / height, padded width, padded height,
    /// covered).
    images: Vec<(f64, u32, u32, f64)>,
    /// No image of its own yet: one estimated.
    estimated: bool,
}

/// `Scale.shown`: the height something `metres` tall is drawn at (D40).
fn shown(metres: f64) -> f64 {
    const TRUE_UP_TO: f64 = 3.0;
    const ABOVE: f64 = 0.5;
    const SMALL_UP_TO: f64 = 0.3;
    const SMALL_TIMES: f64 = 2.0;
    const GROWN_FROM: f64 = 1.0;
    if metres <= SMALL_UP_TO {
        return metres.max(0.0) * SMALL_TIMES;
    }
    let small = SMALL_UP_TO * SMALL_TIMES;
    if metres < GROWN_FROM {
        return small + (metres - SMALL_UP_TO) * (GROWN_FROM - small) / (GROWN_FROM - SMALL_UP_TO);
    }
    if metres <= TRUE_UP_TO {
        return metres;
    }
    TRUE_UP_TO * (metres / TRUE_UP_TO).powf(ABOVE)
}

/// `Trees.sharpness`: the shader's mip bias at `zoom`.
fn sharpness(zoom: f64) -> f64 {
    -((zoom - 0.5) / 0.5).clamp(0.0, 1.0)
}

/// Bytes the card holds for an image `w` × `h` (padded) from mip `level`
/// down to 1 × 1 (`Trees.read_image`).
fn chain_bytes(w: u32, h: u32, level: u32) -> u64 {
    let (mut lw, mut lh) = (w, h);
    let mut bytes = 0;
    let mut i = 0;
    loop {
        if i >= level {
            bytes += u64::from(lw.div_ceil(4)) * u64::from(lh.div_ceil(4)) * BLOCK_BYTES;
        }
        if lw == 1 && lh == 1 {
            return bytes;
        }
        lw = (lw >> 1).max(1);
        lh = (lh >> 1).max(1);
        i += 1;
    }
}

impl Kind {
    /// Bytes the card holds for all its images from `level`.
    fn bytes(&self, level: u32) -> u64 {
        self.images
            .iter()
            .map(|&(_, w, h, _)| chain_bytes(w, h, level))
            .sum()
    }

    /// Its largest single image from `level`: what one upload sends.
    fn largest(&self, level: u32) -> u64 {
        self.images
            .iter()
            .map(|&(_, w, h, _)| chain_bytes(w, h, level))
            .max()
            .unwrap_or(0)
    }

    /// `Trees.fine_read` for its tallest image (`_needs_fine`): whether the
    /// card may read the finest level at `zoom` on a screen `stretch`.
    fn fine_read(&self, zoom: f64, stretch: f64) -> bool {
        let texels = self.images.iter().map(|i| i.2).min().unwrap_or(0);
        let pixels = self.shown_px * (1.0 + SIZE_JITTER) * zoom * stretch;
        let lod = (f64::from(texels) / pixels).log2() + sharpness(zoom);
        lod < 1.0 + FINE_MARGIN
    }

    /// Its widest image's width over height.
    fn widest(&self) -> f64 {
        self.images.iter().map(|i| i.0).fold(0.0, f64::max)
    }

    /// The mean area its outline covers, in the canvas' pixels at zoom 1,
    /// over its images and the size's jitter (E[s²] = 1 + J²/3).
    fn mean_area(&self) -> f64 {
        let h = self.shown_px;
        let mean: f64 = self
            .images
            .iter()
            .map(|&(aspect, _, _, covered)| covered * h * h * aspect)
            .sum::<f64>()
            / self.images.len() as f64;
        mean * (1.0 + SIZE_JITTER * SIZE_JITTER / 3.0)
    }

    /// Its mean image's width over height and covered share.
    fn mean_shape(&self) -> (f64, f64) {
        let n = self.images.len() as f64;
        (
            self.images.iter().map(|i| i.0).sum::<f64>() / n,
            self.images.iter().map(|i| i.3).sum::<f64>() / n,
        )
    }
}

/// The kinds of `species`, their images from the outlines; a kind with
/// none gets one estimated from the others' mean shape.
fn kinds(species: &[Species]) -> Vec<Kind> {
    let outlines: BTreeMap<String, Outline> =
        serde_json::from_str(OUTLINES).expect("outlines.json parses");
    let mut kinds: Vec<Kind> = species
        .iter()
        .map(|s| {
            let images = (1..)
                .map_while(|n| outlines.get(&format!("{}_adult_{n:02}_summer.webp", s.id)))
                .map(|o| {
                    (
                        o.texture[0] / o.texture[1],
                        o.image[0],
                        o.image[1],
                        o.covered,
                    )
                })
                .collect::<Vec<_>>();
            Kind {
                id: s.id.clone(),
                shown_px: shown(f64::from(s.height_m)) * PX_PER_M,
                estimated: images.is_empty(),
                images,
            }
        })
        .collect();
    let drawn: Vec<(f64, f64)> = kinds
        .iter()
        .filter(|k| !k.estimated)
        .flat_map(|k| k.images.iter().map(|i| (i.0, i.3)))
        .collect();
    assert!(!drawn.is_empty(), "no tree image has an outline");
    let n = drawn.len() as f64;
    let aspect = drawn.iter().map(|d| d.0).sum::<f64>() / n;
    let covered = drawn.iter().map(|d| d.1).sum::<f64>() / n;
    for k in kinds.iter_mut().filter(|k| k.estimated) {
        let h = (k.shown_px / PX_PER_M * SHIPPED_PX_PER_M).ceil() as u32;
        let w = (f64::from(h) * aspect).ceil() as u32;
        k.images.push((
            aspect,
            w.next_multiple_of(4),
            h.next_multiple_of(4),
            covered,
        ));
    }
    kinds
}

/// `Iso.point_to_screen` of a tile's centre (x, y).
fn tile_to_screen(x: f64, y: f64) -> (f64, f64) {
    ((x - y) * TILE_W / 2.0, (x + y) * TILE_H / 2.0)
}

/// A rectangle (x0, y0, x1, y1) in the canvas' pixels at zoom 1.
type Rect = (f64, f64, f64, f64);

fn overlap(a: Rect, b: Rect) -> f64 {
    let w = a.2.min(b.2) - a.0.max(b.0);
    let h = a.3.min(b.3) - a.1.max(b.1);
    if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
}

/// `ViewLedger.chunk_rect`: the screen rectangle a chunk's ground spans.
fn chunk_rect(c: ChunkPos) -> Rect {
    let side = f64::from(CHUNK_SIZE);
    let (lx, ly) = (f64::from(c.x) * side, f64::from(c.y) * side);
    let (hx, hy) = (lx + side, ly + side);
    (
        (lx - hy) * TILE_W / 2.0,
        (lx + ly) * TILE_H / 2.0,
        (hx - ly) * TILE_W / 2.0,
        (hx + hy) * TILE_H / 2.0,
    )
}

/// A region's trees, chunk by chunk: each one's foot on screen and kind;
/// and each chunk's kinds as a mask.
struct Placed {
    name: String,
    side: u32,
    trees: HashMap<ChunkPos, Vec<(f64, f64, u8)>>,
    masks: HashMap<ChunkPos, u64>,
}

fn place(name: &str, map: &Map, seed: u64, species: &[Species], open: &OpenGround) -> Placed {
    assert!(species.len() <= 64, "a kind mask holds 64 kinds");
    let side = map.width().min(map.height());
    let chunks = side.div_ceil(CHUNK_SIZE) as i32;
    let mut trees = HashMap::new();
    let mut masks = HashMap::new();
    for cy in 0..chunks {
        for cx in 0..chunks {
            let chunk = ChunkPos::new(cx, cy);
            let placed = sim::trees::trees_in(map, species, open, seed, chunk);
            let mut mask = 0_u64;
            let feet = placed
                .iter()
                .map(|t| {
                    mask |= 1 << t.species;
                    let (sx, sy) = tile_to_screen(
                        f64::from(t.at.x) / 1000.0 - 0.5,
                        f64::from(t.at.y) / 1000.0 - 0.5,
                    );
                    (sx, sy, t.species as u8)
                })
                .collect();
            trees.insert(chunk, feet);
            masks.insert(chunk, mask);
        }
    }
    Placed {
        name: name.to_owned(),
        side,
        trees,
        masks,
    }
}

/// What one screen costs.
#[derive(Clone, Copy, Default)]
struct Screen {
    kinds: u32,
    trees: u32,
    held: u32,
    trees_mb: f64,
    /// Screens of tree outlines drawn over the screen: what the card
    /// shades (the clear corners of the images are not drawn, D39 B6).
    overdraw: f64,
    /// Screens of tree rectangles, the audit's measure.
    rects: f64,
}

/// The screen of a camera on tile (x, y) at `zoom`, the client's rules of
/// today (D39, E1 and E2): every kind the view holds kept from its coarse
/// level; a kind shown whose finest level the card may read on the lead's
/// screen kept whole.
fn screen(placed: &Placed, kinds: &[Kind], tile: Pos, zoom: f64) -> Screen {
    let (cx, cy) = tile_to_screen(f64::from(tile.x), f64::from(tile.y));
    let (hw, hh) = (CANVAS.0 / zoom / 2.0, CANVAS.1 / zoom / 2.0);
    let view: Rect = (cx - hw, cy - hh, cx + hw, cy + hh);
    let cells: Rect = (
        view.0 - CELL_PX,
        view.1 - CELL_PX,
        view.2 + CELL_PX,
        view.3 + CELL_PX,
    );
    let tallest = kinds.iter().map(|k| k.shown_px).fold(0.0, f64::max) * (1.0 + SIZE_JITTER);
    let at = tile.chunk();
    let mut shown = 0_u64;
    let mut held = 0_u64;
    let mut out = Screen::default();
    let mut area = 0.0;
    for dy in -VIEW_RADIUS..=VIEW_RADIUS {
        for dx in -VIEW_RADIUS..=VIEW_RADIUS {
            let chunk = ChunkPos::new(at.x + dx, at.y + dy);
            let Some(&mask) = placed.masks.get(&chunk) else {
                continue;
            };
            held |= mask;
            let r = chunk_rect(chunk);
            // A crown reaches up from its foot; a foot below the screen
            // may still show its crown.
            if overlap((r.0 - tallest, r.1, r.2 + tallest, r.3 + tallest), cells) <= 0.0 {
                continue;
            }
            for &(fx, fy, k) in &placed.trees[&chunk] {
                let kind = &kinds[usize::from(k)];
                let h = kind.shown_px * (1.0 + SIZE_JITTER);
                let w = h * kind.widest();
                if overlap((fx - w / 2.0, fy - h, fx + w / 2.0, fy), cells) <= 0.0 {
                    continue;
                }
                shown |= 1 << k;
                out.trees += 1;
                // The outline's mean area, the share of the rectangle in
                // the view.
                let (aspect, _) = kind.mean_shape();
                let (h1, w1) = (kind.shown_px, kind.shown_px * aspect);
                let rect = (fx - w1 / 2.0, fy - h1, fx + w1 / 2.0, fy);
                area += kind.mean_area() * overlap(rect, view) / (w1 * h1);
                out.rects += overlap(rect, view);
            }
        }
    }
    out.kinds = shown.count_ones();
    out.held = held.count_ones();
    out.overdraw = area / ((view.2 - view.0) * (view.3 - view.1));
    out.rects /= (view.2 - view.0) * (view.3 - view.1);
    out.trees_mb = (0..kinds.len())
        .filter(|k| held & (1 << k) != 0)
        .map(|k| {
            let kind = &kinds[k];
            let fine = shown & (1 << k) != 0 && kind.fine_read(zoom, LEAD_STRETCH);
            kind.bytes(if fine { 0 } else { COARSE }) as f64 / MB
        })
        .sum();
    out
}

/// The `q` quantile of `values`.
fn quantile(values: &mut [f64], q: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * q).round() as usize]
}

/// A window of the atlas, as the relief test cuts one (a small continent,
/// a window of 1 km): its biomes vary.
fn atlas(seed: u64) -> Map {
    const SIDE: u32 = 1024;
    let continent = Continent::with(seed, 128, None);
    let site = region::site(&continent);
    let region = Region::around(&continent, site, 256);
    let corner = Window::choose(&region, SIDE);
    Map::on_window(Window::cut(&continent, &region, corner, SIDE), seed)
}

/// The regions sampled: the game's valley (`--valley`, 3 km, seed 1), and
/// two windows of the atlas, whose biomes vary. A biome with kinds of its
/// own adds its regions here.
fn regions(species: &[Species], open: &OpenGround) -> Vec<Placed> {
    let valley = Map::valley(sim::VALLEY_SIZE, sim::VALLEY_SIZE, sim::VALLEY_SEED);
    let mut regions = vec![place("valley", &valley, sim::VALLEY_SEED, species, open)];
    drop(valley);
    for seed in [1, 2] {
        let map = atlas(seed);
        let mut biomes = BTreeSet::new();
        for y in (0..map.height() as i32).step_by(32) {
            for x in (0..map.width() as i32).step_by(32) {
                if let Some(b) = map.biome(Pos::new(x, y)) {
                    biomes.insert(format!("{b:?}"));
                }
            }
        }
        let name = format!(
            "atlas {seed} ({})",
            biomes.into_iter().collect::<Vec<_>>().join(", ")
        );
        regions.push(place(&name, &map, seed, species, open));
    }
    regions
}

fn print_kinds(kinds: &[Kind]) {
    println!("Kinds of tree (MB the card holds: whole, coarse; an upload: whole, coarse):");
    for k in kinds {
        println!(
            "  {}{:<12} {} image(s) {:>5.2} / {:>4.2} MB, upload {:>4.2} / {:>4.2} MB",
            if k.estimated { "~" } else { " " },
            k.id,
            k.images.len(),
            k.bytes(0) as f64 / MB,
            k.bytes(COARSE) as f64 / MB,
            k.largest(0) as f64 / MB,
            k.largest(COARSE) as f64 / MB,
        );
    }
    println!();
}

/// The table's rows for `placed`, one a zoom; what is over the lines goes
/// in `over`.
fn judge(placed: &Placed, kinds: &[Kind], over: &mut Vec<String>) {
    let cameras: Vec<Pos> = (STEP / 2..placed.side as i32)
        .step_by(STEP as usize)
        .flat_map(|y| {
            (STEP / 2..placed.side as i32)
                .step_by(STEP as usize)
                .map(move |x| Pos::new(x, y))
        })
        .collect();
    for zoom in ZOOMS {
        let screens: Vec<Screen> = cameras
            .iter()
            .map(|&c| screen(placed, kinds, c, zoom))
            .filter(|s| s.trees > 0)
            .collect();
        let column = |f: fn(&Screen) -> f64| screens.iter().map(f).collect::<Vec<_>>();
        let mut shown = column(|s| f64::from(s.kinds));
        let mut held = column(|s| f64::from(s.held));
        let mut mb = column(|s| s.trees_mb);
        let mut draw = column(|s| s.overdraw);
        let mut rects = column(|s| s.rects);
        let trees = screens.iter().map(|s| s.trees).max().unwrap_or(0);
        println!(
            "| {} | {zoom} | {} | {} / {} / {} | {trees} | {}–{} | {:.1} / {:.1} | {:.1} / {:.1} / {:.1} | {:.1} / {:.1} |",
            placed.name,
            screens.len(),
            quantile(&mut shown, 0.5),
            quantile(&mut shown, 0.95),
            quantile(&mut shown, 1.0),
            quantile(&mut held, 0.0),
            quantile(&mut held, 1.0),
            quantile(&mut mb, 0.5),
            quantile(&mut mb, 1.0),
            quantile(&mut draw, 0.5),
            quantile(&mut draw, 0.99),
            quantile(&mut draw, 1.0),
            quantile(&mut rects, 0.5),
            quantile(&mut rects, 1.0),
        );
        let (worst_mb, worst_draw) = (quantile(&mut mb, 1.0), quantile(&mut draw, 0.99));
        if worst_mb > TREES_LINE_MB {
            over.push(format!(
                "{} at zoom {zoom}: trees {worst_mb:.1} MB",
                placed.name
            ));
        }
        if worst_draw > OVERDRAW_LINE {
            over.push(format!(
                "{} at zoom {zoom}: overdraw {worst_draw:.1} screens on 1 in 100",
                placed.name
            ));
        }
    }
}

#[test]
fn the_trees_stay_within_their_budget_on_every_screen() {
    let species: Vec<Species> = ron::from_str(SPECIES).expect("species.ron parses");
    let open: OpenGround = ron::from_str(OPEN_GROUND).expect("open_ground.ron parses");
    let kinds = kinds(&species);
    print_kinds(&kinds);
    let regions = regions(&species, &open);

    // The audit's forest heart (18 November 2026): its 6.1–6.4 screens of
    // tree rectangles, 40 / 439 trees at zooms 2.5 / 0.4, and D39's 25.9 MB
    // there from zoom 0.8 check this model.
    for zoom in ZOOMS {
        let s = screen(&regions[0], &kinds, HEART, zoom);
        println!(
            "The valley's forest heart at zoom {zoom}: {} trees, {} kinds, {:.1} MB, {:.1} screens of rectangles, {:.1} of outlines",
            s.trees, s.kinds, s.trees_mb, s.rects, s.overdraw
        );
    }
    println!();
    println!(
        "| region | zoom | screens | kinds on screen p50/p95/max | trees max | kinds held | trees MB p50/max | outlines p50/p99/max | rectangles p50/max |"
    );
    println!("|---|---|---|---|---|---|---|---|---|");
    let mut over = Vec::new();
    for placed in &regions {
        judge(placed, &kinds, &mut over);
    }

    // One image arrives a frame, from its coarse level (E1): the largest
    // coarse image is the most a frame uploads when a kind comes into view.
    let arrival = kinds
        .iter()
        .max_by_key(|k| k.largest(COARSE))
        .expect("kinds of tree");
    let sharpen = kinds
        .iter()
        .max_by_key(|k| k.largest(0))
        .expect("kinds of tree");
    println!();
    println!(
        "Uploads a frame: a kind arriving, at most {:.2} MB ({}); a kind sharpening, at most {:.2} MB ({}): \
         over the line until the uploads are paced by bytes. Line {UPLOAD_LINE_MB} MB.",
        arrival.largest(COARSE) as f64 / MB,
        arrival.id,
        sharpen.largest(0) as f64 / MB,
        sharpen.id,
    );
    if arrival.largest(COARSE) as f64 / MB > UPLOAD_LINE_MB {
        over.push(format!(
            "{} arrives as {:.2} MB in one frame",
            arrival.id,
            arrival.largest(COARSE) as f64 / MB
        ));
    }
    assert!(over.is_empty(), "over the trees' budget: {over:#?}");
}
