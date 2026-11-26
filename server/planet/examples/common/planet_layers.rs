//! The `planet` example's layers: each map split in stackable PNGs of one
//! size (transparent but for the base relief), and a small script of data
//! per continent for the page's cursor readout
//! (`content/ai-reserve/review/monde-nouveau/etape-0/carte.html`).

use std::fmt::Write as _;
use std::path::Path;

use planet::history::{self, Margin, NONE};
use planet::{
    self, Edge, Kind, Line, OldKind, Planet, Setting, ZOOM_SIDE, Zoom, chamfer, open_sea,
};

use super::png::{Rgba, png_rgba};
use super::{hypsometric, mix, setting_colour};

/// The planet view's size.
pub const PW: usize = 2048;
/// See [`PW`].
pub const PH: usize = 1024;
/// The continent view: the zoom's middle 440 pixels (880 km), each drawn
/// as `SCALE` × `SCALE`.
pub const CROP: usize = 100;
/// See [`CROP`].
pub const SIDE: usize = 440;
/// See [`CROP`].
pub const SCALE: usize = 4;
/// The continent view's size, pixels.
pub const CW: usize = SIDE * SCALE;

const CLEAR: Rgba = [0, 0, 0, 0];

/// A layer being drawn.
pub struct Layer {
    w: usize,
    h: usize,
    px: Vec<Rgba>,
}

impl Layer {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![CLEAR; w * h],
        }
    }

    fn put(&mut self, x: i64, y: i64, c: Rgba) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }

    fn dot(&mut self, x: i64, y: i64, r: i64, c: Rgba) {
        for dx in -r..=r {
            for dy in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    self.put(x + dx, y + dy, c);
                }
            }
        }
    }

    fn ring(&mut self, x: i64, y: i64, r: i64, width: i64, c: Rgba) {
        for dx in -r - width..=r + width {
            for dy in -r - width..=r + width {
                let d = dx * dx + dy * dy;
                if d >= r * r && d <= (r + width) * (r + width) {
                    self.put(x + dx, y + dy, c);
                }
            }
        }
    }

    fn segment(&mut self, (x0, y0): (i64, i64), (x1, y1): (i64, i64), thick: i64, c: Rgba) {
        let n = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for k in 0..=n {
            let (x, y) = (x0 + (x1 - x0) * k / n, y0 + (y1 - y0) * k / n);
            for d in 0..thick {
                self.put(x + d, y, c);
                self.put(x, y + d, c);
            }
        }
    }

    fn write(&self, dir: &Path, name: &str) {
        std::fs::write(dir.join(name), png_rgba(self.w, self.h, &self.px)).expect("a layer");
    }
}

fn rgba(c: [u8; 3], a: u8) -> Rgba {
    [c[0], c[1], c[2], a]
}

const PLATE_COLOURS: [[u8; 3]; 16] = [
    [230, 120, 120],
    [120, 180, 230],
    [140, 210, 130],
    [230, 200, 110],
    [190, 140, 220],
    [110, 210, 200],
    [240, 160, 90],
    [160, 160, 230],
    [210, 120, 180],
    [170, 220, 90],
    [100, 160, 160],
    [220, 170, 170],
    [150, 200, 240],
    [200, 200, 140],
    [130, 130, 190],
    [240, 140, 140],
];

/// The colour of a plate boundary between two cells, both sides read:
/// collision, island arc, trench, ridge or rift, transform.
fn boundary_colour(a: Option<Edge>, b: Option<Edge>) -> Rgba {
    let kinds: Vec<Kind> = [a, b].into_iter().flatten().map(|e| e.kind).collect();
    let any = |f: &dyn Fn(Kind) -> bool| kinds.iter().any(|&k| f(k));
    let c = if any(&|k| k == Kind::Collision) {
        [200, 60, 230]
    } else if any(&|k| k == Kind::IslandArc) {
        [255, 150, 30]
    } else if any(&|k| matches!(k, Kind::Trench | Kind::Arc)) {
        [235, 35, 35]
    } else if any(&|k| matches!(k, Kind::Ridge | Kind::Rift)) {
        [255, 230, 70]
    } else {
        [60, 230, 120]
    };
    rgba(c, 255)
}

fn lat_y(lat_tenths: i64) -> i64 {
    (900 - lat_tenths) * PH as i64 / 1800
}

/// The map's x of a longitude, the map centred on `lon0` (tenths).
fn lon_x(lon_tenths: i64, lon0: i64) -> i64 {
    (lon_tenths - lon0 + 1800).rem_euclid(3600) * PW as i64 / 3600
}

fn map_xy(v: planet::V3, lon0: i64) -> (i64, i64) {
    (
        lon_x(planet::longitude(v), lon0),
        lat_y(planet::latitude(v)),
    )
}

/// The planet view's layers for one seed, as `p{seed}-{name}.png`,
/// centred on our continent's longitude (a whole degree), and
/// `p{seed}-donnees.js` for the cursor readout.
pub fn planet_layers(dir: &Path, world: &Planet, zoom: Option<&Zoom>) {
    let seed = world.seed;
    let lon0 = lon0_of(zoom);
    let map_xy = |v: planet::V3| map_xy(v, lon0);
    let mut cell = vec![0_usize; PW * PH];
    for y in 0..PH {
        let lat = 900 - (2 * y as i64 + 1) * 1800 / (2 * PH as i64);
        for x in 0..PW {
            let lon = lon0 - 1800 + (2 * x as i64 + 1) * 3600 / (2 * PW as i64);
            cell[y * PW + x] = world.sphere.cell_of(planet::from_lat_lon(lat, lon));
        }
    }
    let right = |i: usize| cell[(i / PW) * PW + (i % PW + 1) % PW];
    let down = |i: usize| {
        if i / PW + 1 < PH {
            cell[i + PW]
        } else {
            cell[i]
        }
    };
    // Relief and the sea's depth.
    let mut l = Layer::new(PW, PH);
    for (i, &c) in cell.iter().enumerate() {
        let e = i64::from(world.elevation[c]);
        let mut col = hypsometric(e);
        if e <= 0 && world.continental[c] {
            col = mix(col, [150, 190, 210], 350);
        }
        let land = e > 0;
        if land != (world.elevation[right(i)] > 0) || land != (world.elevation[down(i)] > 0) {
            col = [25, 25, 25];
        }
        l.px[i] = rgba(col, 255);
    }
    l.write(dir, &format!("p{seed}-relief.png"));
    // Plates.
    let mut l = Layer::new(PW, PH);
    for (i, &c) in cell.iter().enumerate() {
        l.px[i] = rgba(PLATE_COLOURS[world.plate[c] as usize % 16], 255);
    }
    l.write(dir, &format!("p{seed}-plaques.png"));
    // Boundaries.
    let mut l = Layer::new(PW, PH);
    for i in 0..PW * PH {
        let c = cell[i];
        for (n, j) in [
            (right(i), (i / PW) * PW + (i % PW + 1) % PW),
            (down(i), (i + PW).min(PW * PH - 1)),
        ] {
            if world.plate[c] != world.plate[n] {
                let col = boundary_colour(world.edge[c], world.edge[n]);
                l.px[i] = col;
                l.px[j] = col;
            }
        }
    }
    l.write(dir, &format!("p{seed}-frontieres.png"));
    // Plate motion.
    let mut l = Layer::new(PW, PH);
    for lat in (-75..=75).step_by(15) {
        for lon in (-172..=172).step_by(15) {
            let p = planet::from_lat_lon(lat * 10, lon * 10);
            let k = world.plates.plate_at(p);
            let v = world.plates.velocity(k, p);
            let rate = v.iter().map(|&a| a * a).sum::<i64>().isqrt();
            if rate < 3 {
                continue;
            }
            let q = planet::step(p, planet::normalize(v), rate * 22);
            let (a, b) = (map_xy(p), map_xy(q));
            if (a.0 - b.0).abs() > PW as i64 / 4 {
                continue;
            }
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = (dx * dx + dy * dy).isqrt().max(1);
            let head = |c: i64, s: i64| {
                let (ux, uy) = (-dx * 10 / len, -dy * 10 / len);
                (
                    b.0 + (ux * c - uy * s) / 1000,
                    b.1 + (ux * s + uy * c) / 1000,
                )
            };
            for (thick, col) in [(4, [10, 10, 10, 230]), (2, [255, 255, 255, 255])] {
                let off = if thick == 4 { -1 } else { 0 };
                l.segment((a.0 + off, a.1 + off), (b.0 + off, b.1 + off), thick, col);
                l.segment((b.0 + off, b.1 + off), head(906, 423), thick, col);
                l.segment((b.0 + off, b.1 + off), head(906, -423), thick, col);
            }
            l.dot(a.0, a.1, 3, [10, 10, 10, 255]);
        }
    }
    l.write(dir, &format!("p{seed}-mouvement.png"));
    // Continental crust.
    let mut l = Layer::new(PW, PH);
    for (i, &c) in cell.iter().enumerate() {
        if world.continental[c] {
            let edge = !world.continental[right(i)] || !world.continental[down(i)];
            l.px[i] = if edge {
                [120, 70, 20, 255]
            } else {
                [240, 200, 110, 150]
            };
        }
    }
    l.write(dir, &format!("p{seed}-croute.png"));
    // Old sutures and failed rifts, inside the continents.
    let mut l = Layer::new(PW, PH);
    for i in 0..PW * PH {
        let c = cell[i];
        if !world.continental[c] {
            continue;
        }
        if world.old_plate[c] != world.old_plate[right(i)]
            || world.old_plate[c] != world.old_plate[down(i)]
        {
            let col = match world.old_edge[c] {
                Some(OldKind::Suture) => [150, 60, 210, 255],
                Some(OldKind::FailedRift) => [245, 120, 20, 255],
                _ => [120, 120, 120, 160],
            };
            for d in 0..2 {
                let j = (i + d).min(PW * PH - 1);
                l.px[j] = col;
            }
        }
    }
    l.write(dir, &format!("p{seed}-anciennes.png"));
    // Hotspots and their chains, paler as they age.
    let mut l = Layer::new(PW, PH);
    for h in &world.hotspots {
        for &(x, age) in &h.track {
            let (a, b) = map_xy(x);
            let t = (age * 1000 / 70).min(1000);
            let c = mix([255, 240, 120], [150, 90, 40], t);
            l.dot(a, b, 2, rgba(c, 255));
        }
        let (a, b) = map_xy(h.at);
        l.dot(a, b, 5, [255, 255, 255, 255]);
        l.ring(a, b, 5, 2, [200, 20, 20, 255]);
    }
    l.write(dir, &format!("p{seed}-points-chauds.png"));
    // The grid, 40° and 50° marked.
    let mut l = Layer::new(PW, PH);
    for deg in (-180..=180).step_by(10) {
        let x = lon_x(deg * 10, lon0);
        for y in 0..PH as i64 {
            l.put(x, y, [255, 255, 255, if deg == 0 { 170 } else { 90 }]);
        }
    }
    for deg in (-80..=80).step_by(10) {
        let y = lat_y(deg * 10);
        let marked = matches!(deg, -50 | -40 | 40 | 50);
        for x in 0..PW as i64 {
            if marked {
                if x % 14 < 9 {
                    l.put(x, y, [60, 230, 255, 255]);
                    l.put(x, y + 1, [60, 230, 255, 255]);
                }
            } else {
                l.put(x, y, [255, 255, 255, if deg == 0 { 170 } else { 90 }]);
            }
        }
    }
    l.write(dir, &format!("p{seed}-grille.png"));
    // Our continent: its land filled and outlined in a strong colour, a
    // wide ring around it, and the continent view's frame.
    let mut l = Layer::new(PW, PH);
    let mut site = None;
    if let Some(z) = zoom {
        let mut ours = vec![false; PW * PH];
        for i in 0..ZOOM_SIDE * ZOOM_SIDE {
            if z.ours[i] && z.elevation[i] > 0 {
                let (x, y) = map_xy(z.frame.point(i));
                ours[(y.clamp(0, PH as i64 - 1) as usize) * PW + x as usize % PW] = true;
            }
        }
        for i in 0..PW * PH {
            if !ours[i] {
                continue;
            }
            let (x, y) = ((i % PW) as i64, (i / PW) as i64);
            let rim = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| {
                let (u, v) = (x + dx, y + dy);
                !(0..PW as i64).contains(&u)
                    || !(0..PH as i64).contains(&v)
                    || !ours[v as usize * PW + u as usize]
            });
            if !rim {
                l.put(x, y, [255, 40, 200, 150]);
            }
        }
        for i in 0..PW * PH {
            let (x, y) = ((i % PW) as i64, (i / PW) as i64);
            if ours[i]
                && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| {
                    let (u, v) = (x + dx, y + dy);
                    (0..PW as i64).contains(&u)
                        && (0..PH as i64).contains(&v)
                        && !ours[v as usize * PW + u as usize]
                })
            {
                l.dot(x, y, 1, [255, 0, 170, 255]);
            }
        }
        for t in (0..SIDE).step_by(2) {
            for (px, py) in [
                (CROP + t, CROP),
                (CROP + t, CROP + SIDE - 1),
                (CROP, CROP + t),
                (CROP + SIDE - 1, CROP + t),
            ] {
                let (x, y) = map_xy(z.frame.point(py * ZOOM_SIDE + px));
                l.put(x, y, [255, 255, 255, 230]);
            }
        }
        let (cx, cy) = map_xy(z.frame.centre);
        l.ring(cx, cy, 70, 4, [10, 10, 10, 200]);
        l.ring(cx, cy, 69, 2, [255, 0, 170, 255]);
        site = Some((cx, cy));
    }
    l.write(dir, &format!("p{seed}-site.png"));
    // The history's record, today: where each piece of crust came from,
    // what its margins are, its last mountain building, its age.
    let h = &world.history;
    let mut l = Layer::new(PW, PH);
    for (i, &c) in cell.iter().enumerate() {
        let p = h.piece[c];
        if p == NONE {
            continue;
        }
        let edge = h.piece[right(i)] != p || h.piece[down(i)] != p;
        l.px[i] = if edge {
            [30, 30, 30, 255]
        } else {
            rgba(piece_colour(p), 255)
        };
    }
    l.write(dir, &format!("p{seed}-fragments.png"));
    let mut l = Layer::new(PW, PH);
    let last = h.margin.len() - 1;
    for (i, &c) in cell.iter().enumerate() {
        if h.piece[c] == NONE {
            continue;
        }
        let flags = h.margins[c];
        let mut col = if flags & history::ACCRETED != 0 {
            [255, 150, 40, 170]
        } else if flags & history::SUTURE != 0 {
            [180, 70, 230, 170]
        } else if flags & history::RIFT != 0 {
            [255, 225, 60, 110]
        } else {
            CLEAR
        };
        match h.margin[last][c] {
            k if k == Margin::Active as u8 => col = [235, 35, 35, 255],
            k if k == Margin::Passive as u8 => col = [90, 220, 120, 255],
            k if k == Margin::Suture as u8 => col = [180, 70, 230, 255],
            _ => {}
        }
        l.px[i] = col;
    }
    l.write(dir, &format!("p{seed}-marges.png"));
    let mut l = Layer::new(PW, PH);
    for (i, &c) in cell.iter().enumerate() {
        let ago = h.orogeny_my[c];
        if h.piece[c] == NONE || ago == history::NEVER {
            continue;
        }
        l.px[i] = rgba(orogeny_colour(i64::from(ago)), 220);
    }
    l.write(dir, &format!("p{seed}-montagnes.png"));
    let mut l = Layer::new(PW, PH);
    for (i, &c) in cell.iter().enumerate() {
        if h.piece[c] == NONE {
            continue;
        }
        let t = (i64::from(h.age_my[c]) * 1000 / 3000).min(1000);
        l.px[i] = rgba(mix([255, 245, 200], [90, 45, 20], t), 230);
    }
    l.write(dir, &format!("p{seed}-age.png"));
    let mut l = Layer::new(PW, PH);
    for (i, &c) in cell.iter().enumerate() {
        if h.piece[c] != NONE {
            continue;
        }
        l.px[i] = rgba(floor_colour(i64::from(h.age_my[c])), 255);
    }
    l.write(dir, &format!("p{seed}-fond.png"));
    // Data for the readout, on a 512 × 256 grid: a letter per cell for its
    // plate (upper case on land, lower case at sea), and its height in
    // hundreds of metres as one character from U+00A1.
    let (gw, gh) = (PW / 4, PH / 4);
    let mut cells = String::with_capacity(gw * gh);
    let mut heights = String::with_capacity(gw * gh * 2);
    for y in 0..gh {
        for x in 0..gw {
            let c = cell[(4 * y + 2) * PW + 4 * x + 2];
            let e = i64::from(world.elevation[c]);
            let k = (world.plate[c] % 26) + b'a';
            cells.push(char::from(if e > 0 { k.to_ascii_uppercase() } else { k }));
            let h = (e.div_euclid(100) + 110).clamp(0, 200) as u32;
            heights.push(char::from_u32(0xa1 + h).expect("a Latin character"));
        }
    }
    let site = site.map_or_else(|| "null".to_string(), |(x, y)| format!("[{x},{y}]"));
    let js = format!(
        "window.DONNEES=window.DONNEES||{{}};window.DONNEES[\"p{seed}\"]={{lon0:{lon0},w:{gw},h:{gh},site:{site},plaques:\"{cells}\",alt:\"{heights}\"}};\n"
    );
    std::fs::write(dir.join(format!("p{seed}-donnees.js")), js).expect("the data");
}

/// The map's centre longitude, tenths: our continent's, to a degree.
fn lon0_of(zoom: Option<&Zoom>) -> i64 {
    zoom.map_or(0, |z| {
        (planet::longitude(z.frame.centre) + 5).div_euclid(10) * 10
    })
}

/// A piece's colour: the supercontinent's plates from a wheel, terranes
/// brown, island arcs olive.
fn piece_colour(p: u8) -> [u8; 3] {
    if p >= history::ARC_BASE {
        [150, 150, 80]
    } else if p >= history::TERRANE_BASE {
        [150, 115, 85]
    } else {
        PLATE_COLOURS[p as usize % 16]
    }
}

/// A history plate's colour (ids past the wheel, darker).
fn plate_colour(p: u8) -> [u8; 3] {
    let c = PLATE_COLOURS[p as usize % 16];
    if p >= 16 {
        mix(c, [40, 40, 40], 300)
    } else {
        c
    }
}

/// An ocean floor's age colour: red at the ridge, then orange, yellow,
/// green, blue, violet past 200 My (the usual map's scale).
pub fn floor_colour(age: i64) -> [u8; 3] {
    const STOPS: [(i64, [u8; 3]); 6] = [
        (0, [200, 30, 40]),
        (20, [245, 140, 40]),
        (50, [245, 225, 80]),
        (90, [110, 200, 110]),
        (140, [70, 140, 220]),
        (200, [120, 70, 170]),
    ];
    for w in STOPS.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if age <= b {
            return mix(ca, cb, (age - a).max(0) * 1000 / (b - a));
        }
    }
    STOPS[5].1
}

/// A boundary kind's colour on the history's maps (`Kind as u8 + 1`).
fn kind_colour(k: u8) -> Option<[u8; 3]> {
    let kinds = [
        Kind::Ridge,
        Kind::Rift,
        Kind::Transform,
        Kind::Trench,
        Kind::Arc,
        Kind::IslandArc,
        Kind::Collision,
    ];
    let kind = *kinds.get(usize::from(k).checked_sub(1)?)?;
    Some(match kind {
        Kind::Ridge | Kind::Rift => [255, 230, 50],
        Kind::Transform => [235, 235, 235],
        Kind::Trench => [230, 20, 30],
        Kind::Arc | Kind::IslandArc => [255, 120, 30],
        Kind::Collision => [200, 60, 255],
    })
}

/// A last mountain building's colour: today red, 100 My orange, 200
/// yellow, older sutures (300–700 My) violet.
fn orogeny_colour(ago: i64) -> [u8; 3] {
    match ago {
        0..=100 => mix([235, 30, 30], [250, 150, 40], ago * 10),
        101..=250 => mix([250, 150, 40], [245, 230, 90], (ago - 100) * 1000 / 150),
        _ => mix(
            [200, 140, 230],
            [110, 60, 160],
            ((ago - 250) * 1000 / 450).min(1000),
        ),
    }
}

/// The history view: the planet at each step, `h{seed}-{step}.png`
/// (1,024 × 512, centred as the planet view): each plate in its colour,
/// dark at sea and light under continents, the coasts outlined, the
/// boundaries by what they do at that step (ridge or rift yellow, trench
/// red, arc orange, collision violet, transform white), the upwelling
/// (orange ring) and the downwellings (blue rings); and the ocean floor's
/// age at each step, `o{seed}-{step}.png`.
pub fn history_layers(dir: &Path, world: &Planet, zoom: Option<&Zoom>) {
    let (w, hgt) = (PW / 2, PH / 2);
    let lon0 = lon0_of(zoom);
    let h = &world.history;
    let mut cell = vec![0_usize; w * hgt];
    for y in 0..hgt {
        let lat = 900 - (2 * y as i64 + 1) * 1800 / (2 * hgt as i64);
        for x in 0..w {
            let lon = lon0 - 1800 + (2 * x as i64 + 1) * 3600 / (2 * w as i64);
            cell[y * w + x] = world.sphere.cell_of(planet::from_lat_lon(lat, lon));
        }
    }
    let at = |v: planet::V3| {
        (
            (planet::longitude(v) - lon0 + 1800).rem_euclid(3600) * w as i64 / 3600,
            (900 - planet::latitude(v)) * hgt as i64 / 1800,
        )
    };
    let right = |i: usize| {
        if (i + 1).is_multiple_of(w) {
            i + 1 - w
        } else {
            i + 1
        }
    };
    let down = |i: usize| if i + w < w * hgt { i + w } else { i };
    for t in 0..=h.last() {
        let (plate, class, kind) = (&h.plate[t], &h.class[t], &h.kind[t]);
        let mut l = Layer::new(w, hgt);
        let mut o = Layer::new(w, hgt);
        for (i, &c) in cell.iter().enumerate() {
            let land = class[c] != history::OCEAN;
            let base = plate_colour(plate[c]);
            let coast = [right(i), down(i)]
                .iter()
                .any(|&j| (class[cell[j]] != history::OCEAN) != land);
            l.px[i] = if coast {
                [25, 25, 25, 255]
            } else if let Some(k) = kind_colour(kind[c]) {
                rgba(k, 255)
            } else if land {
                rgba(mix(base, [255, 250, 235], 450), 255)
            } else {
                rgba(mix(base, [15, 25, 60], 600), 255)
            };
            o.px[i] = if coast {
                [25, 25, 25, 255]
            } else if land {
                [200, 195, 185, 255]
            } else {
                rgba(floor_colour(i64::from(h.age[t][c])), 255)
            };
        }
        let (x, y) = at(h.upwelling);
        l.ring(x, y, 12, 3, [255, 140, 20, 255]);
        for &d in &h.downwellings {
            let (x, y) = at(d);
            l.ring(x, y, 12, 3, [60, 200, 255, 255]);
        }
        l.write(dir, &format!("h{}-{t:02}.png", world.seed));
        o.write(dir, &format!("o{}-{t:02}.png", world.seed));
    }
}

/// A grid of the zoom's values read at the continent view's pixels,
/// interpolated between the zoom's pixel centres.
fn sample(values: &[i64], x: usize, y: usize) -> i64 {
    // In eighths of a zoom pixel: the view pixel's centre.
    let at = |v: usize| -> (usize, i64) {
        let e = (CROP as i64 * 8 - 4) + (8 * v as i64 + 4) / SCALE as i64;
        let base = e.div_euclid(8).clamp(0, ZOOM_SIDE as i64 - 2);
        (base as usize, (e - base * 8).clamp(0, 8))
    };
    let ((bx, fx), (by, fy)) = (at(x), at(y));
    let g = |dx: usize, dy: usize| values[(by + dy) * ZOOM_SIDE + bx + dx];
    let top = g(0, 0) * (8 - fx) + g(1, 0) * fx;
    let bottom = g(0, 1) * (8 - fx) + g(1, 1) * fx;
    (top * (8 - fy) + bottom * fy) / 64
}

/// The zoom pixel under a view pixel.
fn under(x: usize, y: usize) -> usize {
    (CROP + y / SCALE) * ZOOM_SIDE + CROP + x / SCALE
}

/// The view pixel at a zoom pixel's centre.
fn view_of(i: usize) -> Option<(i64, i64)> {
    let (px, py) = (i % ZOOM_SIDE, i / ZOOM_SIDE);
    if px < CROP || py < CROP || px >= CROP + SIDE || py >= CROP + SIDE {
        return None;
    }
    let s = SCALE as i64;
    Some((
        (px - CROP) as i64 * s + s / 2,
        (py - CROP) as i64 * s + s / 2,
    ))
}

/// The relief of a zoom at the view's size, with light from the north-west.
fn relief(z: &Zoom) -> (Layer, Vec<i64>) {
    let elev: Vec<i64> = z.elevation.iter().map(|&e| i64::from(e)).collect();
    let fine: Vec<i64> = (0..CW * CW)
        .map(|i| sample(&elev, i % CW, i / CW))
        .collect();
    let mut l = Layer::new(CW, CW);
    let at = |x: i64, y: i64| {
        fine[(y.clamp(0, CW as i64 - 1) as usize) * CW + x.clamp(0, CW as i64 - 1) as usize]
    };
    for i in 0..CW * CW {
        let (x, y) = ((i % CW) as i64, (i / CW) as i64);
        let h = fine[i];
        let slope = (at(x - 2, y) - at(x + 2, y)) + (at(x, y - 2) - at(x, y + 2));
        let shade = if h > 0 {
            (1000 + slope.clamp(-1600, 1600) / 4).clamp(600, 1300)
        } else {
            1000
        };
        let c = hypsometric(h);
        let f = |v: u8| (i64::from(v) * shade / 1000).clamp(0, 255) as u8;
        l.px[i] = [f(c[0]), f(c[1]), f(c[2]), 255];
    }
    for i in 0..CW * CW {
        let (x, y) = (i % CW, i / CW);
        if x + 1 < CW && y + 1 < CW {
            let land = fine[i] > 0;
            if land != (fine[i + 1] > 0) || land != (fine[i + CW] > 0) {
                l.px[i] = [25, 25, 25, 255];
            }
        }
    }
    (l, fine)
}

/// Zone codes for the page's readout, one letter per setting.
const ZONE_CODES: [(Setting, char); 14] = [
    (Setting::Ocean, 'o'),
    (Setting::Trench, 't'),
    (Setting::CoastRange, 'c'),
    (Setting::ForearcBasin, 'f'),
    (Setting::Arc, 'a'),
    (Setting::BackArc, 'b'),
    (Setting::BackArcBasin, 'k'),
    (Setting::OldCore, 's'),
    (Setting::FailedRift, 'r'),
    (Setting::Suture, 'u'),
    (Setting::CoastalPlain, 'p'),
    (Setting::Hotspot, 'h'),
    (Setting::Rift, 'n'),
    (Setting::Range, 'g'),
];

/// The continent view's layers for one seed, as `c{seed}-{name}.png`,
/// and its data, `c{seed}-donnees.js`.
pub fn continent_layers(dir: &Path, seed: u64, z: &Zoom) {
    let (l, fine) = relief(z);
    l.write(dir, &format!("c{seed}-relief.png"));
    // Zones, on land only.
    let mut l = Layer::new(CW, CW);
    for i in 0..CW * CW {
        if fine[i] > 0 {
            l.px[i] = rgba(setting_colour(z.setting[under(i % CW, i / CW)]), 255);
        }
    }
    l.write(dir, &format!("c{seed}-zones.png"));
    // Low flat land, hatched.
    let mut l = Layer::new(CW, CW);
    for i in 0..CW * CW {
        let (x, y) = (i % CW, i / CW);
        if fine[i] > 0 && z.marsh[under(x, y)] && (x + y) % 6 < 2 {
            l.px[i] = [20, 120, 60, 255];
        }
    }
    l.write(dir, &format!("c{seed}-marais.png"));
    // Volcanoes.
    let mut l = Layer::new(CW, CW);
    for &i in &z.volcanoes {
        if let Some((x, y)) = view_of(i) {
            for row in 0..15_i64 {
                for col in -row / 2 - 1..=row / 2 + 1 {
                    let edge = row >= 13 || col <= -row / 2 || col >= row / 2;
                    l.put(
                        x + col,
                        y - 8 + row,
                        if edge {
                            [50, 0, 0, 255]
                        } else {
                            [240, 30, 20, 255]
                        },
                    );
                }
            }
        }
    }
    l.write(dir, &format!("c{seed}-volcans.png"));
    // Today's boundaries, and the old ones apart.
    let mut now = Layer::new(CW, CW);
    let mut old = Layer::new(CW, CW);
    for (i, &kind) in z.line.iter().enumerate() {
        let Some((x, y)) = view_of(i) else { continue };
        let (layer, col, r) = match kind {
            Line::None => continue,
            Line::Trench => (&mut now, [235, 30, 30, 255], 3),
            Line::Convergent => (&mut now, [235, 30, 30, 255], 2),
            Line::Divergent => (&mut now, [255, 220, 50, 255], 3),
            Line::Transform => (&mut now, [50, 230, 110, 255], 3),
            Line::Suture => (&mut old, [150, 60, 210, 255], 2),
            Line::FailedRift => (&mut old, [245, 120, 20, 255], 2),
        };
        if matches!(kind, Line::Suture | Line::FailedRift)
            && (i % ZOOM_SIDE + i / ZOOM_SIDE) % 6 < 2
        {
            continue;
        }
        layer.dot(x, y, r, col);
    }
    now.write(dir, &format!("c{seed}-frontieres.png"));
    old.write(dir, &format!("c{seed}-anciennes.png"));
    // Seamounts and hotspot lavas.
    let mut l = Layer::new(CW, CW);
    for &(i, age) in &z.hotspot_points {
        if let Some((x, y)) = view_of(i) {
            let c = mix([255, 240, 120], [170, 100, 40], (age * 1000 / 70).min(1000));
            l.ring(x, y, 9, 3, rgba(c, 255));
        }
    }
    l.write(dir, &format!("c{seed}-monts-sous-marins.png"));
    // Distance to the open sea, a line every 25 km (bolder every 100).
    let sea = open_sea(&z.elevation);
    let d: Vec<i64> = chamfer(&sea)
        .iter()
        .map(|&v| i64::from(v.min(2_000_000)))
        .collect();
    let dist: Vec<i64> = (0..CW * CW).map(|i| sample(&d, i % CW, i / CW)).collect();
    let mut l = Layer::new(CW, CW);
    for i in 0..CW * CW {
        let (x, y) = (i % CW, i / CW);
        if fine[i] <= 0 || x + 1 >= CW || y + 1 >= CW {
            continue;
        }
        let band = dist[i] / 25_000;
        let other = (dist[i + 1] / 25_000).max(dist[i + CW] / 25_000);
        if band != other && band.max(other) > 0 {
            let top = band.max(other);
            let (col, r) = if top % 4 == 0 {
                ([255, 255, 255, 255], 2)
            } else {
                ([220, 245, 255, 200], 1)
            };
            l.dot(x as i64, y as i64, r, col);
        }
    }
    l.write(dir, &format!("c{seed}-distance-mer.png"));
    // Whole degrees of latitude and longitude.
    let lat: Vec<i64> = z.latitude.iter().map(|&v| i64::from(v)).collect();
    let lon0 = planet::longitude(z.frame.centre);
    let lon: Vec<i64> = (0..ZOOM_SIDE * ZOOM_SIDE)
        .map(|i| {
            let v = planet::longitude(z.frame.point(i));
            let d = (v - lon0 + 1800).rem_euclid(3600) - 1800;
            lon0 + d
        })
        .collect();
    let flat: Vec<i64> = (0..CW * CW).map(|i| sample(&lat, i % CW, i / CW)).collect();
    let flon: Vec<i64> = (0..CW * CW).map(|i| sample(&lon, i % CW, i / CW)).collect();
    let mut l = Layer::new(CW, CW);
    for i in 0..CW * CW {
        let (x, y) = (i % CW, i / CW);
        if x + 1 >= CW || y + 1 >= CW {
            continue;
        }
        let deg = |v: i64| v.div_euclid(10);
        if (deg(flat[i]) != deg(flat[i + CW]) || deg(flon[i]) != deg(flon[i + 1]))
            && (x + y) % 16 < 11
        {
            l.dot(x as i64, y as i64, 1, [255, 255, 255, 210]);
        }
    }
    l.write(dir, &format!("c{seed}-degres.png"));
    // A 100 km scale bar, top left, in four parts.
    let mut l = Layer::new(CW, CW);
    let bar = 100 / planet::ZOOM_PX_KM * SCALE as i64;
    let (x0, y0) = (40_i64, 40_i64);
    for x in x0 - 3..x0 + bar + 3 {
        for y in y0 - 3..y0 + 19 {
            l.put(x, y, [0, 0, 0, 255]);
        }
    }
    for x in x0..x0 + bar {
        for y in y0..y0 + 16 {
            let white = (x - x0) * 4 / bar % 2 == 1;
            l.put(
                x,
                y,
                if white {
                    [255, 255, 255, 255]
                } else {
                    [30, 30, 30, 255]
                },
            );
        }
    }
    l.write(dir, &format!("c{seed}-echelle.png"));
    // Data for the readout: zone letters and heights (m) of the zoom's
    // middle 440 × 440 pixels.
    let mut zones = String::with_capacity(SIDE * SIDE);
    let mut heights = String::with_capacity(SIDE * SIDE * 4);
    for py in CROP..CROP + SIDE {
        for px in CROP..CROP + SIDE {
            let i = py * ZOOM_SIDE + px;
            let land = z.elevation[i] > 0;
            let code = if z.marsh[i] {
                'm'
            } else {
                ZONE_CODES
                    .iter()
                    .find(|(s, _)| *s == z.setting[i])
                    .map_or('?', |&(_, c)| c)
            };
            zones.push(if land || matches!(code, 't') {
                code
            } else {
                'o'
            });
            let _ = write!(heights, "{},", z.elevation[i]);
        }
    }
    heights.pop();
    let js = format!(
        "window.DONNEES=window.DONNEES||{{}};window.DONNEES[\"c{seed}\"]={{side:{SIDE},km:{},zones:\"{zones}\",alt:[{heights}]}};\n",
        planet::ZOOM_PX_KM
    );
    std::fs::write(dir.join(format!("c{seed}-donnees.js")), js).expect("the data");
}

/// The width view: the relief and the outline of our land at one width,
/// as `l{seed}-relief-{w}.png` and `l{seed}-contour-{w}.png`.
pub fn width_layers(dir: &Path, seed: u64, z: &Zoom) {
    let (l, fine) = relief(z);
    let w = z.width_km;
    l.write(dir, &format!("l{seed}-relief-{w}.png"));
    let col = match w {
        350 => [90, 200, 255, 255],
        450 => [255, 215, 60, 255],
        _ => [255, 70, 70, 255],
    };
    let mut l = Layer::new(CW, CW);
    let ours = |i: usize| fine[i] > 0 && z.ours[under(i % CW, i / CW)];
    for i in 0..CW * CW {
        let (x, y) = (i % CW, i / CW);
        if x + 1 < CW && y + 1 < CW && (ours(i) != ours(i + 1) || ours(i) != ours(i + CW)) {
            l.dot(x as i64, y as i64, 1, col);
        }
    }
    l.write(dir, &format!("l{seed}-contour-{w}.png"));
}
