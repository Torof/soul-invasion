//! Step 0 of the new world (probe, 26 November 2026): the planet's plates
//! and the outlines of its continents, on a coarse grid of fixed size,
//! and our continent shaped at a subduction margin. Nothing reads it yet:
//! it is drawn by `examples/planet.rs` for the project lead to judge.
//!
//! The planet has the Earth's radius. Its grid is a cube projected on the
//! sphere with equal angles (six faces of [`FACE`]² cells of about 100
//! km): a fixed size, whatever detail the layers above it get later.
//!
//! What it tells, in order, each step reading only the ones before:
//!
//! 1. **The plates' history** ([`history`], version 4, the lead's 26
//!    November): plates move through the whole history, driven by the
//!    mantle and by slab pull, making ocean floor at their ridges and
//!    sinking it at their trenches; the supercontinent on them breaks,
//!    its pieces collide and weld, terranes and arcs accrete. The
//!    continents of today are what is left, each piece of crust carrying
//!    its record (piece, age, margins, last mountain building). A world's
//!    character is an input: [`Settings`].
//! 2. **Older seams** inside the supercontinent (old sutures, failed
//!    rifts), carried with its crust.
//! 3. **Today's plates**: the plates of the history's last step, turning
//!    as they last turned.
//! 4. **Boundaries.** Each boundary cell is classified by the relative
//!    motion of the two plates: convergent (collision, continental arc,
//!    island arc, or the trench of the plate that sinks), divergent
//!    (ridge in the ocean, rift in a continent), or transform.
//! 5. **Hotspots.** Fixed plumes under moving plates leave chains, older
//!    the farther they run in the plate's direction of motion.
//! 6. **Height and the sea.** A continent's height is carried by its
//!    crust through the history (version 7) and changes only where
//!    something happens to it; the ocean floor's follows its age. The sea
//!    level leaves the settings' share of the surface dry.
//! 7. **Our continent** ([`Site`], [`Zoom`]): a subduction margin at
//!    40–50° north with the ocean to the west is chosen, and a
//!    continent of [`LENGTH_KM`] is shaped on its overriding side, read
//!    across the margin: trench, coast range, forearc basin, volcanic
//!    arc, back-arc, as on the real margins of Cascadia and Chile.
//!
//! Integers only (D24): angles in millionths of a radian, vectors on the
//! unit sphere at length [`ONE`], sines from a Taylor series.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::many_single_char_names,
    clippy::needless_range_loop,
    clippy::cast_possible_wrap,
    reason = "a probe's geometry: values bounded by the grid and the planet; long functions read as one story; short names follow the maths (the lints `sim::plan` allows, inherited when it lived there)"
)]

use std::cmp::Reverse;
use std::collections::BinaryHeap;

mod hash;
pub mod history;

use hash::hash;
pub use history::{History, Settings};

// ---------------------------------------------------------------- numbers

/// The fixed-point unit: unit vectors have this length, angles are in
/// millionths of a radian.
pub const ONE: i64 = 1_000_000;

/// The planet's radius, km: the Earth's (the lead's call, 26 Nov).
pub const RADIUS_KM: i64 = 6_371;

/// Cells along a cube face's side: 6 × 100² = 60,000 cells of ~100 km.
pub const FACE: usize = 100;

/// Cells of the planet's grid.
pub const CELLS: usize = 6 * FACE * FACE;

/// A vector, each component scaled by [`ONE`].
pub type V3 = [i64; 3];

const PI: i64 = 3_141_593;
const HALF_PI: i64 = 1_570_796;
const TAU: i64 = 6_283_185;

/// The sine of an angle in millionths of a radian, scaled by [`ONE`].
pub fn sin(a: i64) -> i64 {
    let mut x = a.rem_euclid(TAU);
    if x > PI {
        x -= TAU;
    }
    if x > HALF_PI {
        x = PI - x;
    } else if x < -HALF_PI {
        x = -PI - x;
    }
    let x = i128::from(x);
    let one = i128::from(ONE);
    let mut term = x;
    let mut sum = x;
    for k in 1..9_i128 {
        term = -term * x / one * x / one / ((2 * k) * (2 * k + 1));
        sum += term;
    }
    sum as i64
}

/// The cosine, as [`sin`].
pub fn cos(a: i64) -> i64 {
    sin(a + HALF_PI)
}

/// An angle in tenths of a degree, in millionths of a radian.
pub fn tenths(t: i64) -> i64 {
    t * PI / 1800
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `a` times `k / ONE`.
fn scale(a: V3, k: i64) -> V3 {
    let f = |v: i64| (i128::from(v) * i128::from(k) / i128::from(ONE)) as i64;
    [f(a[0]), f(a[1]), f(a[2])]
}

/// The dot product, scaled by [`ONE`].
pub fn dot(a: V3, b: V3) -> i64 {
    let s: i128 = (0..3).map(|i| i128::from(a[i]) * i128::from(b[i])).sum();
    (s / i128::from(ONE)) as i64
}

fn cross(a: V3, b: V3) -> V3 {
    let c = |x: i64, y: i64, z: i64, w: i64| {
        ((i128::from(x) * i128::from(y) - i128::from(z) * i128::from(w)) / i128::from(ONE)) as i64
    };
    [
        c(a[1], b[2], a[2], b[1]),
        c(a[2], b[0], a[0], b[2]),
        c(a[0], b[1], a[1], b[0]),
    ]
}

fn norm(a: V3) -> i64 {
    let s: i128 = a.iter().map(|&v| i128::from(v) * i128::from(v)).sum();
    s.cast_unsigned().isqrt() as i64
}

/// `a` brought to length [`ONE`] (a null vector stays null).
pub fn normalize(a: V3) -> V3 {
    let n = norm(a);
    if n == 0 {
        return a;
    }
    let f = |v: i64| (i128::from(v) * i128::from(ONE) / i128::from(n)) as i64;
    [f(a[0]), f(a[1]), f(a[2])]
}

/// The latitude of a direction, in tenths of a degree (north positive).
pub fn latitude(v: V3) -> i64 {
    let z = i128::from(v[2]) * i128::from(ONE) / i128::from(norm(v).max(1));
    let z = z as i64;
    let (mut lo, mut hi) = (-900_i64, 900_i64);
    while lo < hi {
        let mid = (lo + hi + 1).div_euclid(2);
        if sin(tenths(mid)) <= z {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// The angle of `(x, y)` from the x axis, in tenths of a degree, in
/// −1800..=1800 (an integer `atan2`).
pub fn angle_of(x: i64, y: i64) -> i64 {
    let r = (i128::from(x) * i128::from(x) + i128::from(y) * i128::from(y))
        .cast_unsigned()
        .isqrt() as i128;
    if r == 0 {
        return 0;
    }
    let c = (i128::from(x) * i128::from(ONE) / r) as i64;
    let (mut lo, mut hi) = (0_i64, 1800_i64);
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if cos(tenths(mid)) >= c {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    if y < 0 { -lo } else { lo }
}

/// The longitude of a direction, in tenths of a degree (east positive).
pub fn longitude(v: V3) -> i64 {
    angle_of(v[0], v[1])
}

/// The direction at a latitude and longitude, both in tenths of a degree.
pub fn from_lat_lon(lat: i64, lon: i64) -> V3 {
    let (cl, sl) = (cos(tenths(lat)), sin(tenths(lat)));
    let (co, so) = (cos(tenths(lon)), sin(tenths(lon)));
    [cl * co / ONE, cl * so / ONE, sl]
}

/// The unit vector pointing east at `v` (tangent to the sphere).
pub fn east_at(v: V3) -> V3 {
    let e = normalize([-v[1], v[0], 0]);
    if e == [0, 0, 0] { [0, ONE, 0] } else { e }
}

/// The unit vector pointing north at `v`, given its east.
pub fn north_at(v: V3, east: V3) -> V3 {
    normalize(cross(v, east))
}

/// `v` moved `km` along the tangent direction `dir`.
pub fn step(v: V3, dir: V3, km: i64) -> V3 {
    normalize(add(v, scale(dir, km * ONE / RADIUS_KM)))
}

/// The straight distance between two points of the sphere, km (the
/// chord: within 1 % of the great circle below 1,500 km).
pub fn km_between(a: V3, b: V3) -> i64 {
    norm(sub(a, b)) * RADIUS_KM / ONE
}

/// A bump of half-width `half` around zero, per mille: 1,000 at 0, 0 at
/// `±half` and beyond, smooth at both ends (`(1 − t²)²`).
pub fn bump(x: i64, half: i64) -> i64 {
    if half <= 0 {
        return 0;
    }
    let t = x * 1000 / half;
    if t.abs() >= 1000 {
        return 0;
    }
    let q = 1000 - t * t / 1000;
    q * q / 1000
}

// ---------------------------------------------------------------- the grid

/// Each face's normal, then its two axes, as unit integers.
const AXES: [[[i64; 3]; 3]; 6] = [
    [[1, 0, 0], [0, 1, 0], [0, 0, 1]],
    [[-1, 0, 0], [0, -1, 0], [0, 0, 1]],
    [[0, 1, 0], [-1, 0, 0], [0, 0, 1]],
    [[0, -1, 0], [1, 0, 0], [0, 0, 1]],
    [[0, 0, 1], [0, 1, 0], [-1, 0, 0]],
    [[0, 0, -1], [0, 1, 0], [1, 0, 0]],
];

fn face_point(face: usize, s: i64, t: i64) -> V3 {
    let [n, r, u] = AXES[face];
    normalize([
        n[0] * ONE + r[0] * s + u[0] * t,
        n[1] * ONE + r[1] * s + u[1] * t,
        n[2] * ONE + r[2] * s + u[2] * t,
    ])
}

/// `tan(k · π / (4 · FACE))`, scaled by [`ONE`]: the cube's equal-angle
/// coordinates.
fn tan_step(k: i64) -> i64 {
    let a = k * PI / (4 * FACE as i64);
    sin(a) * ONE / cos(a)
}

/// The planet's grid: cell centres, areas and neighbours.
#[derive(Debug, Clone)]
pub struct Sphere {
    /// Each cell's centre.
    pub centre: Vec<V3>,
    /// Each cell's area, km².
    pub area_km2: Vec<u32>,
    /// Each cell's neighbours (up to eight) and the distance to them, km.
    pub links: Vec<Vec<(u32, u32)>>,
    edges: Vec<i64>,
}

impl Default for Sphere {
    fn default() -> Self {
        Self::new()
    }
}

impl Sphere {
    /// The grid (the same for every planet).
    pub fn new() -> Self {
        let n = FACE as i64;
        let edges: Vec<i64> = (0..=n).map(|i| tan_step(2 * i - n)).collect();
        let centres: Vec<i64> = (-1..=n).map(|i| tan_step(2 * i + 1 - n)).collect();
        let mut centre = Vec::with_capacity(CELLS);
        let mut area_km2 = Vec::with_capacity(CELLS);
        for f in 0..6 {
            for j in 0..FACE {
                for i in 0..FACE {
                    centre.push(face_point(f, centres[i + 1], centres[j + 1]));
                    let c00 = face_point(f, edges[i], edges[j]);
                    let c11 = face_point(f, edges[i + 1], edges[j + 1]);
                    let c10 = face_point(f, edges[i + 1], edges[j]);
                    let c01 = face_point(f, edges[i], edges[j + 1]);
                    let a = norm(cross(sub(c11, c00), sub(c10, c01)));
                    let km2 =
                        i128::from(a) * i128::from(RADIUS_KM * RADIUS_KM) / i128::from(2 * ONE);
                    area_km2.push(km2 as u32);
                }
            }
        }
        let mut sphere = Self {
            centre,
            area_km2,
            links: Vec::new(),
            edges,
        };
        let mut links = Vec::with_capacity(CELLS);
        for f in 0..6 {
            for j in 0..FACE as i64 {
                for i in 0..FACE as i64 {
                    let me = f * FACE * FACE + j as usize * FACE + i as usize;
                    let mut mine: Vec<(u32, u32)> = Vec::with_capacity(8);
                    for (di, dj) in [
                        (0, -1),
                        (-1, 0),
                        (1, 0),
                        (0, 1),
                        (-1, -1),
                        (1, -1),
                        (-1, 1),
                        (1, 1),
                    ] {
                        let s = centres[(i + di + 1) as usize];
                        let t = centres[(j + dj + 1) as usize];
                        let other = sphere.cell_of(face_point(f, s, t));
                        if other != me && !mine.iter().any(|&(o, _)| o as usize == other) {
                            let km = km_between(sphere.centre[me], sphere.centre[other]);
                            mine.push((other as u32, km as u32));
                        }
                    }
                    links.push(mine);
                }
            }
        }
        sphere.links = links;
        sphere
    }

    /// The cell holding a direction.
    pub fn cell_of(&self, v: V3) -> usize {
        let mut axis = 0;
        for a in 1..3 {
            if v[a].abs() > v[axis].abs() {
                axis = a;
            }
        }
        let face = axis * 2 + usize::from(v[axis] < 0);
        let [n, r, u] = AXES[face];
        let raw = |w: [i64; 3]| -> i128 { (0..3).map(|k| i128::from(v[k] * w[k])).sum() };
        let dn = raw(n).max(1);
        let s = (raw(r) * i128::from(ONE) / dn) as i64;
        let t = (raw(u) * i128::from(ONE) / dn) as i64;
        let inner = &self.edges[1..FACE];
        let i = inner.partition_point(|&e| e <= s);
        let j = inner.partition_point(|&e| e <= t);
        face * FACE * FACE + j * FACE + i
    }

    /// A many-source shortest walk over the grid (Dijkstra), up to
    /// `reach` km, stepping only where `pass(from, to)`: each cell's
    /// distance, km, and the label of its nearest source. Ties go to the
    /// cell popped first, a fixed order.
    pub fn spread(
        &self,
        sources: &[(usize, u32)],
        pass: impl Fn(usize, usize) -> bool,
        reach: u32,
    ) -> (Vec<u32>, Vec<u32>) {
        let mut dist = vec![u32::MAX; CELLS];
        let mut label = vec![u32::MAX; CELLS];
        let mut heap = BinaryHeap::new();
        for &(c, l) in sources {
            if dist[c] == u32::MAX {
                dist[c] = 0;
                label[c] = l;
                heap.push(Reverse((0_u32, c as u32)));
            }
        }
        while let Some(Reverse((d, c))) = heap.pop() {
            let c = c as usize;
            if d > dist[c] {
                continue;
            }
            for &(n, km) in &self.links[c] {
                let n = n as usize;
                if !pass(c, n) {
                    continue;
                }
                let nd = d + km;
                if nd <= reach && nd < dist[n] {
                    dist[n] = nd;
                    label[n] = label[c];
                    heap.push(Reverse((nd, n as u32)));
                }
            }
        }
        (dist, label)
    }
}

// ---------------------------------------------------------------- noise

fn lattice3(seed: u64, i: i64, j: i64, k: i64) -> i128 {
    i128::from(hash(&[seed, i as u64, j as u64, k as u64]) % 1000)
}

/// Smooth value noise in 0..1000 over space, on a lattice of `cell`
/// (in [`ONE`] units): the sphere's own noise, with no seam anywhere.
pub fn noise3(seed: u64, p: V3, cell: i64) -> i64 {
    let c = i128::from(cell);
    let mut base = [0_i64; 3];
    let mut e = [0_i128; 3];
    for a in 0..3 {
        base[a] = p[a].div_euclid(cell);
        let f = i128::from(p[a].rem_euclid(cell));
        e[a] = f * f * (3 * c - 2 * f) / (c * c);
    }
    let l = |dx: i64, dy: i64, dz: i64| lattice3(seed, base[0] + dx, base[1] + dy, base[2] + dz);
    let lerp = |a: i128, b: i128, t: i128| a + (b - a) * t / c;
    let x00 = lerp(l(0, 0, 0), l(1, 0, 0), e[0]);
    let x10 = lerp(l(0, 1, 0), l(1, 1, 0), e[0]);
    let x01 = lerp(l(0, 0, 1), l(1, 0, 1), e[0]);
    let x11 = lerp(l(0, 1, 1), l(1, 1, 1), e[0]);
    let y0 = lerp(x00, x10, e[1]);
    let y1 = lerp(x01, x11, e[1]);
    lerp(y0, y1, e[2]) as i64
}

/// Octaves of [`noise3`], `(cell, weight)`, averaged: 0..1000.
pub fn fbm3(seed: u64, p: V3, octaves: &[(i64, i64)]) -> i64 {
    let mut sum = 0;
    let mut weights = 0;
    for (o, &(cell, w)) in octaves.iter().enumerate() {
        sum += noise3(hash(&[seed, o as u64]), p, cell) * w;
        weights += w;
    }
    sum / weights.max(1)
}

/// `p` pushed by noise, `(cell, amplitude)` per octave: borders that
/// wander instead of a Voronoi's straight edges.
fn warp(seed: u64, p: V3, octaves: &[(i64, i64)]) -> V3 {
    let mut q = p;
    for (o, &(cell, amp)) in octaves.iter().enumerate() {
        for a in 0..3 {
            q[a] += (noise3(hash(&[seed, o as u64, a as u64]), p, cell) - 500) * amp / 500;
        }
    }
    normalize(q)
}

fn draw(seed: u64, purpose: u64, k: u64, lo: i64, hi: i64) -> i64 {
    let span = (hi - lo + 1).max(1) as u64;
    lo + (hash(&[seed, purpose, k]) % span) as i64
}

fn random_unit(seed: u64, purpose: u64, k: u64) -> V3 {
    for t in 0..1000_u64 {
        let v = [
            draw(seed, purpose, k * 3000 + t * 3, -ONE, ONE),
            draw(seed, purpose, k * 3000 + t * 3 + 1, -ONE, ONE),
            draw(seed, purpose, k * 3000 + t * 3 + 2, -ONE, ONE),
        ];
        let n = norm(v);
        if (ONE / 10..=ONE).contains(&n) {
            return normalize(v);
        }
    }
    [0, 0, ONE]
}

// ---------------------------------------------------------------- plates

/// Draw purposes.
const P_PLATES: u64 = 0x0c50_0010;
const P_OLD: u64 = 0x0c50_0020;
const P_RELIEF: u64 = 0x0c50_0040;
const P_ZOOM: u64 = 0x0c50_0050;

/// Plates of the older layout: fewer, larger (a supercontinent's time).
const OLD_PLATES: (i64, i64) = (6, 9);
/// The largest weight a plate's seed point gets, in [`ONE`] units, drawn
/// squared so a few plates are large and many small.
const PLATE_WEIGHT: i64 = 280_000;
/// Plate speeds at 90° from their pole, mm a year (the Earth's: 10–100).
const SPEED_MM: (i64, i64) = (25, 95);
/// The plates' borders' warp, `(cell, amplitude)` in [`ONE`] units.
const PLATE_WARP: [(i64, i64); 4] = [
    (600_000, 200_000),
    (200_000, 70_000),
    (70_000, 22_000),
    (25_000, 7_000),
];

/// A plate: a seed point with a weight, turning about a pole.
#[derive(Debug, Clone)]
pub struct Plate {
    /// Its seed point.
    pub centre: V3,
    /// Its weight in the Voronoi (larger, a larger plate).
    pub weight: i64,
    /// The pole it turns about.
    pub pole: V3,
    /// Its speed at 90° from the pole, mm a year.
    pub speed: i64,
    /// Which of two oceanic plates rides over the other (larger rides).
    pub buoyancy: i64,
}

/// A set of plates over the sphere.
#[derive(Debug, Clone)]
pub struct Layout {
    /// The plates.
    pub plates: Vec<Plate>,
    warp_seed: u64,
    /// Plates read from a grid (today's, the history's last step) rather
    /// than from a Voronoi of seed points.
    grid: Option<Box<Grid>>,
}

/// Each cell's plate on the grid.
#[derive(Debug, Clone)]
struct Grid {
    sphere: Sphere,
    plate: Vec<u8>,
}

/// The warp of a grid's plate borders, `(cell, amplitude)` in [`ONE`]
/// units: a few tens of km, so a border is no staircase of cells.
const GRID_WARP: [(i64, i64); 2] = [(80_000, 10_000), (25_000, 3_000)];
/// A grid cell votes for its plate within this, km.
const GRID_VOTE_KM: i64 = 150;

impl Layout {
    fn draw(seed: u64, purpose: u64, (lo, hi): (i64, i64)) -> Self {
        let count = draw(seed, purpose, 0, lo, hi) as u64;
        let plates = (0..count)
            .map(|k| {
                let u = draw(seed, purpose, 100 + k, 0, 1000);
                Plate {
                    centre: random_unit(seed, purpose + 1, k),
                    weight: u * u * PLATE_WEIGHT / 1_000_000,
                    pole: random_unit(seed, purpose + 2, k),
                    speed: draw(seed, purpose, 200 + k, SPEED_MM.0, SPEED_MM.1),
                    buoyancy: draw(seed, purpose, 300 + k, 0, 1000),
                }
            })
            .collect();
        Self {
            plates,
            warp_seed: hash(&[seed, purpose, 0xa5]),
            grid: None,
        }
    }

    /// The plates at a step of the history (today's: the last), each
    /// turning as it turned then. Ids are renumbered from 0, largest plate first; the second
    /// value is each cell's plate.
    fn of_history(
        seed: u64,
        s: &Settings,
        sphere: &Sphere,
        h: &History,
        last: usize,
    ) -> (Self, Vec<u8>) {
        let raw = &h.plate[last];
        let mut area = vec![0_i64; history::MAXP];
        let mut sum = vec![[0_i64; 3]; history::MAXP];
        let mut ocean = vec![(0_i64, 0_i64); history::MAXP];
        for c in 0..CELLS {
            let p = raw[c] as usize;
            area[p] += i64::from(sphere.area_km2[c]);
            sum[p] = add(sum[p], sphere.centre[c]);
            if h.class[last][c] == history::OCEAN {
                ocean[p].0 += i64::from(h.age[last][c]);
                ocean[p].1 += 1;
            }
        }
        let mut ids: Vec<usize> = (0..history::MAXP).filter(|&p| area[p] > 0).collect();
        ids.sort_by_key(|&p| (Reverse(area[p]), p));
        let mut dense = [0_u8; history::MAXP];
        let plates = ids
            .iter()
            .enumerate()
            .map(|(k, &p)| {
                dense[p] = k as u8;
                let m = h.motion[last][p];
                let age = if ocean[p].1 > 0 {
                    ocean[p].0 / ocean[p].1
                } else {
                    0
                };
                Plate {
                    centre: normalize(sum[p]),
                    weight: 0,
                    pole: if m.speed_mm > 0 { m.pole } else { [0, 0, ONE] },
                    speed: m.speed_mm.max(s.speed_mm.0),
                    // Younger floor is lighter: it rides.
                    buoyancy: (1000 - age * 1000 / 200).clamp(0, 1000),
                }
            })
            .collect();
        let plate: Vec<u8> = raw.iter().map(|&p| dense[p as usize]).collect();
        let layout = Self {
            plates,
            warp_seed: hash(&[seed, P_PLATES, 0xa5]),
            grid: Some(Box::new(Grid {
                sphere: sphere.clone(),
                plate: plate.clone(),
            })),
        };
        (layout, plate)
    }

    /// The plate at a point.
    pub fn plate_at(&self, p: V3) -> usize {
        if let Some(g) = &self.grid {
            // The cells around vote, nearer ones more.
            let q = warp(self.warp_seed, p, &GRID_WARP);
            let c = g.sphere.cell_of(q);
            let floor = cos(GRID_VOTE_KM * ONE / RADIUS_KM);
            let mut tally: [(u8, i64); 9] = [(u8::MAX, 0); 9];
            let around =
                std::iter::once(c).chain(g.sphere.links[c].iter().map(|&(n, _)| n as usize));
            for k in around {
                let w = (dot(q, g.sphere.centre[k]) - floor).max(0);
                let plate = g.plate[k];
                if let Some(t) = tally.iter_mut().find(|t| t.0 == plate || t.0 == u8::MAX) {
                    t.0 = plate;
                    t.1 += w;
                }
            }
            return tally
                .iter()
                .filter(|t| t.0 != u8::MAX)
                .max_by_key(|t| (t.1, Reverse(t.0)))
                .map_or(g.plate[c] as usize, |t| t.0 as usize);
        }
        let q = warp(self.warp_seed, p, &PLATE_WARP);
        let mut best = 0;
        let mut best_score = i64::MIN;
        for (k, plate) in self.plates.iter().enumerate() {
            let score = dot(q, plate.centre) + plate.weight;
            if score > best_score {
                best_score = score;
                best = k;
            }
        }
        best
    }

    /// A plate's velocity at a point, mm a year (tangent).
    pub fn velocity(&self, k: usize, p: V3) -> V3 {
        let plate = &self.plates[k];
        let w = cross(plate.pole, p);
        [
            w[0] * plate.speed / ONE,
            w[1] * plate.speed / ONE,
            w[2] * plate.speed / ONE,
        ]
    }

    /// How fast plate `a` closes on plate `b` across a border whose normal
    /// (from `a` toward `b`) is `normal`, and how fast they slide, mm a year.
    pub fn motion(&self, a: usize, b: usize, p: V3, normal: V3) -> (i64, i64) {
        let rel = sub(self.velocity(a, p), self.velocity(b, p));
        let closing = rel.iter().zip(normal).map(|(&r, n)| r * n).sum::<i64>() / ONE;
        let along = sub(
            rel,
            [
                normal[0] * closing / ONE,
                normal[1] * closing / ONE,
                normal[2] * closing / ONE,
            ],
        );
        let slide = along.iter().map(|&v| v * v).sum::<i64>().isqrt();
        (closing, slide)
    }
}

/// What a plate boundary does, seen from one side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Oceanic plates part: a mid-ocean ridge.
    Ridge,
    /// A continent is pulled apart: a rift valley, long lakes, a new sea.
    Rift,
    /// The plates slide past: a transform fault.
    Transform,
    /// This side sinks under the other: a trench.
    Trench,
    /// This continental side rides over a sinking ocean: a volcanic range.
    Arc,
    /// This oceanic side rides over a sinking ocean: an island arc.
    IslandArc,
    /// Two continents meet: a wide high range.
    Collision,
}

/// Convergence or divergence below this, mm a year, is a transform.
const LEAST_MOTION_MM: i64 = 8;

/// What a convergent or divergent motion makes, given each side's crust.
pub fn classify(
    closing: i64,
    slide: i64,
    mine_cont: bool,
    theirs_cont: bool,
    mine_up: bool,
) -> Kind {
    if closing >= LEAST_MOTION_MM && 2 * closing >= slide {
        match (mine_cont, theirs_cont) {
            (true, true) => Kind::Collision,
            (true, false) => Kind::Arc,
            (false, true) => Kind::Trench,
            (false, false) => {
                if mine_up {
                    Kind::IslandArc
                } else {
                    Kind::Trench
                }
            }
        }
    } else if closing <= -LEAST_MOTION_MM && -2 * closing >= slide {
        if mine_cont { Kind::Rift } else { Kind::Ridge }
    } else {
        Kind::Transform
    }
}

/// What an older boundary left inside a continent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OldKind {
    /// Where old plates met: a worn range of old granite and gneiss.
    Suture,
    /// Where old plates began to part and stopped: a trough of sediment.
    FailedRift,
    /// An old fault where they slid.
    OldFault,
}

/// A boundary cell: what it does, against which plate, how fast.
#[derive(Debug, Clone, Copy)]
pub struct Edge {
    /// What it does on this side.
    pub kind: Kind,
    /// The plate across.
    pub other: u8,
    /// Closing (or opening, or sliding) speed, mm a year.
    pub rate: i32,
    /// The normal toward the plate across.
    pub normal: V3,
}

/// A hotspot and its chain.
#[derive(Debug, Clone)]
pub struct Hotspot {
    /// The plume, fixed under the plates.
    pub at: V3,
    /// The chain it left, from the plume on, with each point's age in
    /// millions of years.
    pub track: Vec<(V3, i64)>,
}

/// A piece of land: a connected set of dry cells.
#[derive(Debug, Clone)]
pub struct Landmass {
    /// Its cells.
    pub cells: Vec<u32>,
    /// Its area, km².
    pub area_km2: i64,
    /// Its middle.
    pub centre: V3,
    /// Twice its farthest cell from the middle, km: a rough size.
    pub span_km: i64,
}

/// The planet.
pub struct Planet {
    /// Its seed.
    pub seed: u64,
    /// The grid.
    pub sphere: Sphere,
    /// Today's plates.
    pub plates: Layout,
    /// The older plates.
    pub old: Layout,
    /// Each cell's plate.
    pub plate: Vec<u8>,
    /// Each cell's older plate.
    pub old_plate: Vec<u8>,
    /// Each cell's crust value (the supercontinent's, read at its source).
    pub crust: Vec<i64>,
    /// The threshold of continental crust.
    pub crust_cut: i64,
    /// Whether a cell's crust is continental.
    pub continental: Vec<bool>,
    /// Boundary cells of today.
    pub edge: Vec<Option<Edge>>,
    /// Boundary cells of the older layout, carried with the crust.
    pub old_edge: Vec<Option<OldKind>>,
    /// Each cell's distance to the nearest boundary of its own plate, km,
    /// and that boundary's cell.
    pub near: Vec<(u32, u32)>,
    /// The hotspots.
    pub hotspots: Vec<Hotspot>,
    /// Each cell's height above the sea, m.
    pub elevation: Vec<i32>,
    /// Today's sea level, in the raw heights' metres: the one that leaves
    /// the settings' share of land dry.
    sea: i64,
    /// How much higher than today's the sea stands at this step, m: it
    /// follows the ocean floor's age (a young floor is shallow, and the sea
    /// spills over the continents' edges).
    pub sea_rise_m: i64,
    /// The pieces of land, largest first.
    pub lands: Vec<Landmass>,
    /// Cells reshaped for our continent (none until [`Planet::shape`]).
    pub ours: Vec<bool>,
    /// The world's settings.
    pub settings: Settings,
    /// The plates' history that made the continents (shared with the
    /// planet at each of its steps).
    pub history: std::sync::Arc<History>,
    /// The step this planet shows (the history's last: today).
    pub step: usize,
    /// The older plates at each cell where the supercontinent's crust
    /// stood, and their seams.
    old_at: Vec<u8>,
    old_at_edge: Vec<Option<OldKind>>,
}

/// Ocean floor older than this, millions of years, sinks no more.
const FLAT_FLOOR_MY: i64 = 120;

/// How far the ocean floor has sunk at an age, millions of years, m: it
/// cools, ~350 m per square root of a million years (Parsons and
/// Sclater), flat past [`FLAT_FLOOR_MY`].
fn sinking(age: i64) -> i64 {
    350 * (age.min(FLAT_FLOOR_MY) * 1_000_000).isqrt() / 1000
}

/// How far the ocean floor has sunk on average at step `t`, m.
fn floor_sinking(sphere: &Sphere, h: &History, t: usize) -> i64 {
    let (mut sum, mut area) = (0_i64, 0_i64);
    for c in 0..CELLS {
        if h.class[t][c] == history::OCEAN {
            let a = i64::from(sphere.area_km2[c]);
            sum += sinking(i64::from(h.age[t][c])) * a;
            area += a;
        }
    }
    sum / area.max(1)
}

/// A hotspot's island or seamount sinks as it ages: gone after this,
/// millions of years.
const TRACK_MYR: i64 = 120;

impl Planet {
    /// The planet born from `seed`, with the default settings.
    pub fn new(seed: u64) -> Self {
        Self::with(seed, &Settings::default())
    }

    /// The planet born from `seed` and `settings`.
    pub fn with(seed: u64, settings: &Settings) -> Self {
        let sphere = Sphere::new();
        // 1. Older plates: the seams inside the supercontinent, read where
        // its crust stood before it broke, and carried with it.
        let old = Layout::draw(seed, P_OLD, OLD_PLATES);
        let old_at: Vec<u8> = sphere
            .centre
            .iter()
            .map(|&p| old.plate_at(p) as u8)
            .collect();
        let all = vec![true; CELLS];
        let old_at_edge: Vec<Option<OldKind>> = (0..CELLS)
            .map(|c| {
                across(&sphere, &old, &old_at, &all, c).map(|(other, normal, _)| {
                    let (closing, slide) =
                        old.motion(old_at[c] as usize, other, sphere.centre[c], normal);
                    match classify(closing, slide, true, true, false) {
                        Kind::Collision => OldKind::Suture,
                        Kind::Rift => OldKind::FailedRift,
                        _ => OldKind::OldFault,
                    }
                })
            })
            .collect();
        let old_is = |kind| -> Vec<bool> { old_at_edge.iter().map(|&k| k == Some(kind)).collect() };
        let (old_suture, old_rift) = (old_is(OldKind::Suture), old_is(OldKind::FailedRift));
        // 2. The history: the supercontinent, its break-up, the drift.
        let history = std::sync::Arc::new(history::run(
            seed,
            settings,
            &sphere,
            (&old_suture, &old_rift),
        ));
        let last = history.last();
        Self::build(
            seed,
            settings,
            sphere,
            old,
            (old_at, old_at_edge),
            history,
            (last, None),
        )
    }

    /// The planet as it stood at step `t` of its history, drawn as today's
    /// is: its plates, their boundaries, its relief and its sea.
    #[must_use]
    pub fn at_step(&self, t: usize) -> Self {
        Self::build(
            self.seed,
            &self.settings,
            self.sphere.clone(),
            self.old.clone(),
            (self.old_at.clone(), self.old_at_edge.clone()),
            std::sync::Arc::clone(&self.history),
            (t.min(self.history.last()), Some(self.sea)),
        )
    }

    fn build(
        seed: u64,
        settings: &Settings,
        sphere: Sphere,
        old: Layout,
        (old_at, old_at_edge): (Vec<u8>, Vec<Option<OldKind>>),
        history: std::sync::Arc<History>,
        (t, sea): (usize, Option<i64>),
    ) -> Self {
        let h = &*history;
        let now_my = ((h.last() - t) as i64 * settings.step_my) as u16;
        let continental: Vec<bool> = h.class[t].iter().map(|&k| k != history::OCEAN).collect();
        let home = &h.home_at[t];
        let crust_cut = h.crust_cut;
        let crust: Vec<i64> = (0..CELLS)
            .map(|c| {
                if continental[c] {
                    h.field[home[c] as usize].max(crust_cut)
                } else {
                    crust_cut - 400
                }
            })
            .collect();
        let mut old_plate = vec![0_u8; CELLS];
        let mut old_edge = vec![None; CELLS];
        for c in 0..CELLS {
            if continental[c] {
                let home = home[c] as usize;
                old_plate[c] = old_at[home];
                old_edge[c] = old_at_edge[home];
            } else {
                old_plate[c] = old.plate_at(sphere.centre[c]) as u8;
            }
        }
        // 3. The plates at this step (today's: the history's last).
        let (plates, plate) = Layout::of_history(seed, settings, &sphere, h, t);
        // 4. Boundaries.
        let mut edge = vec![None; CELLS];
        for c in 0..CELLS {
            if let Some((other, normal, theirs_cont)) =
                across(&sphere, &plates, &plate, &continental, c)
            {
                let mine = plate[c] as usize;
                let p = sphere.centre[c];
                let (closing, slide) = plates.motion(mine, other, p, normal);
                let up = plates.plates[mine].buoyancy > plates.plates[other].buoyancy;
                let kind = classify(closing, slide, continental[c], theirs_cont, up);
                let rate = if matches!(kind, Kind::Transform) {
                    slide
                } else {
                    closing.abs()
                };
                edge[c] = Some(Edge {
                    kind,
                    other: other as u8,
                    rate: rate as i32,
                    normal,
                });
            }
        }
        // Distances.
        let sources: Vec<(usize, u32)> = (0..CELLS)
            .filter(|&c| edge[c].is_some())
            .map(|c| (c, c as u32))
            .collect();
        let (near_d, near_l) = sphere.spread(&sources, |a, b| plate[a] == plate[b], 1_500);
        let near: Vec<(u32, u32)> = near_d.into_iter().zip(near_l).collect();
        // 5. Hotspots: their chains are the marks the history carried, the
        // youngest first.
        let hotspots: Vec<Hotspot> = h
            .hotspots
            .iter()
            .enumerate()
            .map(|(k, &at)| {
                let mut track: Vec<(V3, i64)> = h.marks_at[t]
                    .iter()
                    .filter(|m| usize::from(m.1) == k)
                    .map(|m| (m.0, i64::from(m.2.saturating_sub(now_my))))
                    .collect();
                track.sort_by_key(|m| m.1);
                if track.first().is_none_or(|m| m.1 > 0) {
                    track.insert(0, (at, 0));
                }
                Hotspot { at, track }
            })
            .collect();
        // Each cell's nearest hotspot mark within reach, and its age.
        let mut lava: Vec<Option<(i64, i64)>> = vec![None; CELLS];
        for hs in &hotspots {
            for &(x, age) in &hs.track {
                let c = sphere.cell_of(x);
                let mut ring = vec![c];
                for &(n, _) in &sphere.links[c] {
                    ring.push(n as usize);
                    ring.extend(sphere.links[n as usize].iter().map(|&(m, _)| m as usize));
                }
                for k in ring {
                    let d = km_between(sphere.centre[k], x);
                    if d < 250 && lava[k].is_none_or(|(bd, ba)| (d, age) < (bd, ba)) {
                        lava[k] = Some((d, age));
                    }
                }
            }
        }
        // 6. Height. A continent's is carried by its crust (version 7,
        // 27 November 2026): the history changed it only where something
        // happened to the crust, and the grain is its home's. The ocean
        // floor's follows its age, with its trenches, its island arcs and
        // its hotspots' chains.
        let mut elevation: Vec<i64> = (0..CELLS)
            .map(|c| {
                if continental[c] {
                    let from = home[c] as usize;
                    // A pile of crust floats higher (isostasy), flood
                    // basalts stand as plateaus.
                    let pile = (i64::from(h.thick_at[t][c]) - i64::from(history::CONTINENT_THICK))
                        .max(0)
                        * history::ISOSTASY_M
                        / 10;
                    let lip = if h.margins_at[t][c] & history::LIP != 0 {
                        350
                    } else {
                        0
                    };
                    return i64::from(h.high_at[t][c])
                        + i64::from(h.rough[from])
                        + i64::from(h.broad[from]) / 2
                        + pile
                        + lip;
                }
                // The floor sinks as it cools: ~2,500 m at a ridge,
                // ~350 m more per square root of a million years
                // (Parsons and Sclater), flat past [`FLAT_FLOOR_MY`].
                let mut e = -2_500 - sinking(i64::from(h.age[t][c])) + i64::from(h.rough[c]);
                let (d, src) = near[c];
                if d != u32::MAX {
                    let d = i64::from(d);
                    e += match edge[src as usize].expect("a source is an edge").kind {
                        Kind::IslandArc => 3_800 * bump(d - 140, 90) / 1000,
                        Kind::Trench => -2_600 * bump(d, 90) / 1000,
                        _ => 0,
                    };
                }
                if let Some((d, age)) = lava[c] {
                    e += 3_800 * bump(d, 130) / 1000 * (TRACK_MYR - age).max(0) / TRACK_MYR;
                }
                e
            })
            .collect();
        // The sea level is anchored today: it leaves the settings' share
        // of land dry. In the past it follows the ocean floor's age: where
        // the floor was younger than today's on average it stood higher,
        // and the sea with it (27 November 2026, version 6; a range rising
        // somewhere never drowns lowlands everywhere).
        let sea =
            sea.unwrap_or_else(|| quantile(&sphere, &elevation, 1000 - settings.land_per_mille));
        let sea_rise_m = (floor_sinking(&sphere, h, h.last()) - floor_sinking(&sphere, h, t))
            * settings.sea_follows_floor
            / 1000;
        for e in &mut elevation {
            *e -= sea + sea_rise_m;
        }
        let elevation: Vec<i32> = elevation
            .iter()
            .map(|&e| e.clamp(-11_000, 9_000) as i32)
            .collect();
        let mut planet = Self {
            seed,
            sphere,
            plates,
            old,
            plate,
            old_plate,
            crust,
            crust_cut,
            continental,
            edge,
            old_edge,
            near,
            hotspots,
            elevation,
            sea,
            sea_rise_m,
            lands: Vec::new(),
            ours: vec![false; CELLS],
            settings: *settings,
            history,
            step: t,
            old_at,
            old_at_edge,
        };
        planet.lands = planet.landmasses();
        planet
    }

    /// The pieces of land, largest first (ties by their first cell).
    pub fn landmasses(&self) -> Vec<Landmass> {
        let mut seen = vec![false; CELLS];
        let mut lands = Vec::new();
        for start in 0..CELLS {
            if seen[start] || self.elevation[start] <= 0 {
                continue;
            }
            seen[start] = true;
            let mut cells = vec![start as u32];
            let mut k = 0;
            while k < cells.len() {
                let c = cells[k] as usize;
                k += 1;
                for &(n, _) in &self.sphere.links[c] {
                    let n = n as usize;
                    if !seen[n] && self.elevation[n] > 0 {
                        seen[n] = true;
                        cells.push(n as u32);
                    }
                }
            }
            let area_km2 = cells
                .iter()
                .map(|&c| i64::from(self.sphere.area_km2[c as usize]))
                .sum();
            let mut sum = [0_i64; 3];
            for &c in &cells {
                sum = add(sum, self.sphere.centre[c as usize]);
            }
            let centre = normalize(sum);
            let span_km = 2 * cells
                .iter()
                .map(|&c| km_between(centre, self.sphere.centre[c as usize]))
                .max()
                .unwrap_or(0);
            lands.push(Landmass {
                cells,
                area_km2,
                centre,
                span_km,
            });
        }
        lands.sort_by_key(|l| (Reverse(l.area_km2), l.cells[0]));
        lands
    }

    /// Boundary length by kind, km (each boundary counted on both sides).
    pub fn boundary_km(&self) -> Vec<(Kind, i64)> {
        let kinds = [
            Kind::Ridge,
            Kind::Rift,
            Kind::Transform,
            Kind::Trench,
            Kind::Arc,
            Kind::IslandArc,
            Kind::Collision,
        ];
        kinds
            .iter()
            .map(|&k| {
                let cells = self
                    .edge
                    .iter()
                    .filter(|e| e.is_some_and(|e| e.kind == k))
                    .count();
                (k, cells as i64 * 100)
            })
            .collect()
    }

    /// When no piece of land qualifies for our continent (see
    /// [`Site::candidates`]), one is cut from a larger land behind a
    /// trench at [`SITE_LATITUDE`]: the land nearest the trench, grown
    /// outward until it holds [`PIECE_KM2`], is parted from the rest by a
    /// strait flooded along its narrowest neck (the cut that floods the
    /// least land). Returns the land flooded, km² (0: nothing cut).
    pub fn carve(&mut self) -> i64 {
        let s = &self.sphere;
        let reach = cos(TRENCH_REACH_KM * ONE / RADIUS_KM);
        // (flooded km², distance from the goal area, the strait's cells)
        let mut best: Option<(i64, i64, Vec<usize>)> = None;
        for c in 0..CELLS {
            let Some(e) = self.edge[c] else { continue };
            let p = s.centre[c];
            let lat = latitude(p);
            if e.kind != Kind::Trench || lat < SITE_LATITUDE.0 || lat > SITE_LATITUDE.1 {
                continue;
            }
            let east = east_at(p);
            if dot(e.normal, east) * 1000 / ONE < SITE_FACING {
                continue;
            }
            // The land nearest the trench, east of it.
            let Some((_, start)) = (0..CELLS)
                .filter(|&l| {
                    self.elevation[l] > 0
                        && dot(s.centre[l], p) >= reach
                        && dot(sub(s.centre[l], p), east) > 0
                })
                .map(|l| (km_between(p, s.centre[l]), l))
                .min()
            else {
                continue;
            };
            let (dist, _) = s.spread(&[(start, 0)], |_, b| self.elevation[b] > 0, 900);
            let mut order: Vec<(u32, usize)> = (0..CELLS)
                .filter(|&l| dist[l] != u32::MAX)
                .map(|l| (dist[l], l))
                .collect();
            order.sort_unstable();
            let mut inside = vec![false; CELLS];
            let mut area = 0;
            let mut sum = [0_i64; 3];
            for (k, &(_, l)) in order.iter().enumerate() {
                inside[l] = true;
                area += i64::from(s.area_km2[l]);
                sum = add(sum, s.centre[l]);
                if area < *PIECE_KM2.start() {
                    continue;
                }
                if area > *PIECE_KM2.end() {
                    break;
                }
                let mid = latitude(normalize(sum));
                if mid < SITE_LATITUDE.0 || mid > SITE_LATITUDE.1 {
                    continue;
                }
                // The strait: land outside the piece touching it.
                let mut strait: Vec<usize> = order[..=k]
                    .iter()
                    .flat_map(|&(_, a)| s.links[a].iter().map(|&(n, _)| n as usize))
                    .filter(|&n| !inside[n] && self.elevation[n] > 0)
                    .collect();
                strait.sort_unstable();
                strait.dedup();
                let flooded: i64 = strait.iter().map(|&n| i64::from(s.area_km2[n])).sum();
                let key = (flooded, (area - PIECE_GOAL_KM2).abs());
                if best.as_ref().is_none_or(|b| key < (b.0, b.1)) {
                    best = Some((key.0, key.1, strait));
                }
            }
        }
        let Some((flooded, _, strait)) = best else {
            return 0;
        };
        for c in strait {
            self.elevation[c] = STRAIT_M;
        }
        self.lands = self.landmasses();
        flooded
    }

    /// Our continent marked on the grid: the cells of `site`'s piece
    /// (nothing is reshaped, nothing sunk).
    pub fn mark(&mut self, site: &Site) {
        self.ours = vec![false; CELLS];
        for &c in &self.lands[site.land].cells {
            self.ours[c as usize] = true;
        }
    }
}

/// Other land within this of our continent's middle is counted, km.
pub const CLEAR_KM: i64 = 1_100;

/// The plate across a boundary cell (the one most of its foreign
/// neighbours belong to, ties to the lower), the normal toward it, and
/// whether its crust there is continental.
fn across(
    sphere: &Sphere,
    layout: &Layout,
    plate: &[u8],
    continental: &[bool],
    c: usize,
) -> Option<(usize, V3, bool)> {
    let mine = plate[c];
    let mut counts = [0_u8; history::MAXP];
    for &(n, _) in &sphere.links[c] {
        let o = plate[n as usize];
        if o != mine {
            counts[o as usize] += 1;
        }
    }
    let (other, &most) = counts
        .iter()
        .enumerate()
        .max_by_key(|&(k, &n)| (n, Reverse(k)))?;
    if most == 0 {
        return None;
    }
    let p = sphere.centre[c];
    let mut dir = [0_i64; 3];
    let mut cont = 0;
    for &(n, _) in &sphere.links[c] {
        if plate[n as usize] as usize == other {
            dir = add(dir, sub(sphere.centre[n as usize], p));
            cont += i32::from(continental[n as usize]);
        }
    }
    // The normal from the plates themselves, looked at all around the
    // cell: the grid's eight neighbours alone give only a few directions,
    // and those favour some cells over others.
    let east = east_at(p);
    let north = north_at(p, east);
    let mut around = [0_i64; 3];
    for k in 0..NORMAL_LOOKS {
        let a = tenths(k * 3600 / NORMAL_LOOKS);
        let d = add(scale(east, cos(a)), scale(north, sin(a)));
        if layout.plate_at(step(p, d, NORMAL_REACH_KM)) == other {
            around = add(around, d);
        }
    }
    let normal = if around == [0, 0, 0] {
        // Keep the normal tangent: remove its part along `p`.
        let radial = dot(dir, p);
        normalize(sub(dir, scale(p, radial)))
    } else {
        normalize(around)
    };
    Some((other, normal, 2 * cont >= i32::from(most)))
}

/// Directions looked at around a boundary cell for its normal, and how
/// far, km (a little more than a cell).
const NORMAL_LOOKS: i64 = 24;
const NORMAL_REACH_KM: i64 = 130;

/// The value below which `share` per mille of the surface lies (by area).
fn quantile(sphere: &Sphere, values: &[i64], share: i64) -> i64 {
    let mut order: Vec<usize> = (0..CELLS).collect();
    order.sort_by_key(|&c| (values[c], c));
    let total: i64 = sphere.area_km2.iter().map(|&a| i64::from(a)).sum();
    let goal = total * share / 1000;
    let mut acc = 0;
    for c in order {
        acc += i64::from(sphere.area_km2[c]);
        if acc >= goal {
            return values[c];
        }
    }
    values[CELLS - 1]
}

// ---------------------------------------------------------------- our site

/// Our continent's length along its chain, km (the lead's ~550).
pub const LENGTH_KM: i64 = 550;
/// Its middle's latitudes north, tenths of a degree (the lead's 40–50°
/// N, north the cold side since 26 November).
const SITE_LATITUDE: (i64, i64) = (400, 500);
/// Our continent's area, km²: the lead's ~210,000, and what is accepted
/// (a piece the history left, not drawn: 26 November, version 4.1).
const PIECE_GOAL_KM2: i64 = 210_000;
const PIECE_KM2: std::ops::RangeInclusive<i64> = 150_000..=300_000;
/// A strait cut to part our continent from a larger land, m.
const STRAIT_M: i32 = -60;
/// Its trench lies at most this far west of its land, km.
const TRENCH_REACH_KM: i64 = 500;
/// How far the margin may turn from north–south: the cosine between the
/// trench's normal and the east, per mille (45°).
const SITE_FACING: i64 = 707;
/// Land a site may sink around it, km² (more, and it is a last resort).
/// How far west the ocean is looked at, km: the westerlies' fetch.
const FETCH_KM: i64 = 3_000;

/// Where our continent stands: a piece of land the history left at
/// 40–50° north, with a trench to its west (the ocean sinking under it).
#[derive(Debug, Clone)]
pub struct Site {
    /// The piece: its index in [`Planet::lands`].
    pub land: usize,
    /// Its area, km².
    pub area_km2: i64,
    /// From its trench to its nearest land, km.
    pub trench_km: i64,
    /// The trench cell.
    pub cell: usize,
    /// The sinking plate (west).
    pub sinking: usize,
    /// The riding plate (east), ours.
    pub riding: usize,
    /// The trench point.
    pub origin: V3,
    /// Latitude and longitude of the trench point, tenths of a degree.
    pub lat_lon: (i64, i64),
    /// Closing speed, mm a year.
    pub rate: i64,
    /// Cosine between the trench's normal and the east, per mille.
    pub facing: i64,
    /// Share of ocean along the fetch to the west, per mille.
    pub ocean_west: i64,
    /// Land within [`CLEAR_KM`] that has to be sunk, km².
    pub land_near_km2: i64,
    /// Old sutures and failed rifts within 300 km of the middle, cells.
    pub old_near: (u32, u32),
    /// Other boundaries (transforms, ridges) within 500 km, cells.
    pub ends_near: u32,
    /// A hotspot or its chain within 700 km.
    pub hotspot_near: bool,
    /// Its score.
    pub score: i64,
}

impl Site {
    /// Every candidate, best first: each piece of land of
    /// [`PIECE_KM2`] whose middle is at [`SITE_LATITUDE`], with a trench
    /// within [`TRENCH_REACH_KM`] to its west, the ocean sinking east
    /// under it.
    pub fn candidates(planet: &Planet) -> Vec<Self> {
        let s = &planet.sphere;
        let mut out = Vec::new();
        for (k, land) in planet.lands.iter().enumerate() {
            let lat = latitude(land.centre);
            if !PIECE_KM2.contains(&land.area_km2) || lat < SITE_LATITUDE.0 || lat > SITE_LATITUDE.1
            {
                continue;
            }
            let near = cos((land.span_km / 2 + TRENCH_REACH_KM) * ONE / RADIUS_KM);
            // The trench cell nearest the piece, west of it, facing east.
            let mut best: Option<(i64, usize, Edge, i64)> = None;
            for c in 0..CELLS {
                let Some(e) = planet.edge[c] else { continue };
                let p = s.centre[c];
                if e.kind != Kind::Trench || dot(p, land.centre) < near {
                    continue;
                }
                let east = east_at(p);
                let facing = dot(e.normal, east) * 1000 / ONE;
                if facing < SITE_FACING || dot(sub(land.centre, p), east) <= 0 {
                    continue;
                }
                let d = land
                    .cells
                    .iter()
                    .map(|&l| km_between(p, s.centre[l as usize]))
                    .min()
                    .unwrap_or(i64::MAX);
                if d <= TRENCH_REACH_KM && best.is_none_or(|(b, bc, _, _)| (d, c) < (b, bc)) {
                    best = Some((d, c, e, facing));
                }
            }
            let Some((trench_km, c, e, facing)) = best else {
                continue;
            };
            let p = s.centre[c];
            let mut ocean = 0;
            let mut seen = 0;
            let mut x = p;
            for _ in 0..FETCH_KM / 100 {
                x = step(x, east_at(x).map(|v| -v), 100);
                seen += 1;
                ocean += i64::from(planet.elevation[s.cell_of(x)] < 0);
            }
            let ocean_west = ocean * 1000 / seen;
            let middle = land.centre;
            let mut land_near_km2 = 0;
            let mut old_near = (0, 0);
            let mut ends_near = 0;
            for q in 0..CELLS {
                let at = s.centre[q];
                if dot(at, middle) < cos(CLEAR_KM * ONE / RADIUS_KM) {
                    continue;
                }
                let d = km_between(at, middle);
                if planet.elevation[q] > 0 && !land.cells.contains(&(q as u32)) {
                    land_near_km2 += i64::from(s.area_km2[q]);
                }
                if d <= 300 {
                    match planet.old_edge[q] {
                        Some(OldKind::Suture) => old_near.0 += 1,
                        Some(OldKind::FailedRift) => old_near.1 += 1,
                        _ => {}
                    }
                }
                if d <= 500
                    && planet.edge[q].is_some_and(|b| {
                        matches!(b.kind, Kind::Transform | Kind::Ridge | Kind::Rift)
                    })
                {
                    ends_near += 1;
                }
            }
            let hotspot_near = planet
                .hotspots
                .iter()
                .any(|h| h.track.iter().any(|&(x, _)| km_between(x, middle) < 700));
            let rate = i64::from(e.rate);
            let score = ocean_west - (land.area_km2 - PIECE_GOAL_KM2).abs() / 500 - trench_km
                + facing / 4
                - (lat - 450).abs();
            out.push(Self {
                land: k,
                area_km2: land.area_km2,
                trench_km,
                cell: c,
                sinking: planet.plate[c] as usize,
                riding: e.other as usize,
                origin: p,
                lat_lon: (latitude(p), longitude(p)),
                rate,
                facing,
                ocean_west,
                land_near_km2,
                old_near,
                ends_near,
                hotspot_near,
                score,
            });
        }
        out.sort_by_key(|x| (Reverse(x.score), x.land));
        out
    }

    /// The best candidate, if the planet has one.
    pub fn find(planet: &Planet) -> Option<Self> {
        Self::candidates(planet).into_iter().next()
    }
}

// ---------------------------------------------------------------- the zoom

/// Pixels a side of the zoom on our continent.
pub const ZOOM_SIDE: usize = 640;
/// A zoom pixel, km.
pub const ZOOM_PX_KM: i64 = 2;

/// The zoom's frame: a tangent plane at our continent's middle.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    /// The middle.
    pub centre: V3,
    /// East at the middle.
    pub east: V3,
    /// North at the middle.
    pub north: V3,
}

impl Frame {
    /// The point of a pixel's centre.
    pub fn point(&self, i: usize) -> V3 {
        let (x, y) = Self::xy(i);
        normalize(add(
            add(self.centre, scale(self.east, x * ONE / RADIUS_KM)),
            scale(self.north, y * ONE / RADIUS_KM),
        ))
    }

    /// A pixel's position east and north of the middle, km.
    pub fn xy(i: usize) -> (i64, i64) {
        let half = ZOOM_SIDE as i64 / 2;
        let (px, py) = ((i % ZOOM_SIDE) as i64, (i / ZOOM_SIDE) as i64);
        ((px - half) * ZOOM_PX_KM + 1, (half - py) * ZOOM_PX_KM - 1)
    }

    /// The pixel holding a point, if the frame holds it.
    pub fn pixel_of(&self, p: V3) -> Option<usize> {
        let dn = dot(p, self.centre);
        if dn <= 0 {
            return None;
        }
        let x = dot(p, self.east) * RADIUS_KM / dn;
        let y = dot(p, self.north) * RADIUS_KM / dn;
        let half = ZOOM_SIDE as i64 / 2;
        let px = x.div_euclid(ZOOM_PX_KM) + half;
        let py = half - 1 - y.div_euclid(ZOOM_PX_KM);
        let side = ZOOM_SIDE as i64;
        (0..side).contains(&px).then_some(())?;
        (0..side).contains(&py).then_some(())?;
        Some(py as usize * ZOOM_SIDE + px as usize)
    }
}

/// A line drawn on the zoom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    /// Nothing.
    None,
    /// Our trench.
    Trench,
    /// Another convergent boundary.
    Convergent,
    /// A ridge or a rift of today.
    Divergent,
    /// A transform fault of today.
    Transform,
    /// An old suture.
    Suture,
    /// An old failed rift.
    FailedRift,
}

/// The geological setting of a zoom pixel: what the rock layer will read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// Open ocean floor (basalt under mud).
    Ocean,
    /// The trench and the sea floor sinking into it.
    Trench,
    /// The outer-arc high, the coast range: scraped-off sea sediments,
    /// sandstone and mudstone, slices of ocean floor.
    CoastRange,
    /// The forearc basin between the coast range and the volcanoes:
    /// sandstone and silt, lowlands, an inland sea where it is drowned.
    ForearcBasin,
    /// The volcanic arc: andesite, basalt, tuff; granite in its roots.
    Arc,
    /// Behind the arc, the uplands: lavas, folded older rock.
    BackArc,
    /// A basin behind the arc: limestone, chalk with flint, clay.
    BackArcBasin,
    /// The old core of the continent: granite and gneiss.
    OldCore,
    /// An old rift's trough: sandstone, salt, long lakes.
    FailedRift,
    /// An old suture: worn hills of granite and schist.
    Suture,
    /// A low coastal plain on the quiet east coast: deltas and marshes.
    CoastalPlain,
    /// A rift of today crossing the land: basalt, salt, hot springs.
    Rift,
    /// A range raised where two plates of today meet across the land.
    Range,
    /// A hotspot's lavas.
    Hotspot,
}

/// What a zoom measures, for the lead's width question.
#[derive(Debug, Clone, Default)]
pub struct ZoomNumbers {
    /// Land of our continent, km².
    pub land_km2: i64,
    /// Land outside it (islands of the arc, hotspots), km².
    pub islands_km2: i64,
    /// Its land's extent along the chain, km.
    pub length_km: i64,
    /// Its land's mean width (area over length), km.
    pub mean_width_km: i64,
    /// The farthest a piece of its land lies from the sea, km.
    pub farthest_sea_km: i64,
    /// Shares of its land farther than 50 and 100 km from the sea, per mille.
    pub far_50: i64,
    /// See `far_50`.
    pub far_100: i64,
    /// Mean width of its land east of the volcanic crest (the lee), km.
    pub lee_km: i64,
    /// Its land east of the crest's lee slope (more than 60 km past the
    /// crest) and more than 40 km from any sea: room for a dry interior, km².
    pub interior_km2: i64,
    /// Volcanoes standing on land.
    pub volcanoes: i64,
    /// Sea enclosed by its land (not touching the frame), km².
    pub enclosed_sea_km2: i64,
    /// Land over sedimentary basins (forearc, back-arc, rift, coastal
    /// plain), km².
    pub basin_km2: i64,
    /// Land where the chalk and limestone platform can lie (back-arc
    /// basins and coastal plain), km².
    pub platform_km2: i64,
    /// Land over the old core, km².
    pub old_core_km2: i64,
    /// Low flat land where marshes will come, km².
    pub marsh_km2: i64,
    /// Highest land, m.
    pub highest_m: i64,
}

/// The zoom on our continent at one width.
pub struct Zoom {
    /// The frame.
    pub frame: Frame,
    /// Height above the sea, m, per pixel.
    pub elevation: Vec<i32>,
    /// Setting per pixel.
    pub setting: Vec<Setting>,
    /// Lines per pixel.
    pub line: Vec<Line>,
    /// Whether a pixel is our continent's crust.
    pub ours: Vec<bool>,
    /// Marshes to come, per pixel.
    pub marsh: Vec<bool>,
    /// Latitude per pixel, tenths of a degree.
    pub latitude: Vec<i16>,
    /// Volcanoes, as pixels.
    pub volcanoes: Vec<usize>,
    /// Hotspot chain points inside the frame, as pixels, with their age.
    pub hotspot_points: Vec<(usize, i64)>,
    /// The trench to volcanic arc distance, km.
    pub arc_km: i64,
    /// The trench to west coast distance, km.
    pub coast_km: i64,
    /// Closing speed at the trench, mm a year.
    pub rate: i64,
    /// The numbers.
    pub numbers: ZoomNumbers,
}

/// The coast's wander at the zoom's scale, `(cell, amplitude)` in
/// [`ONE`] units (~90, 32, 10 and 3 km): the grid's 100-km cells read as
/// a natural coast, not a staircase.
const COAST_WARP: [(i64, i64); 4] = [
    (120_000, 14_000),
    (40_000, 5_000),
    (12_000, 1_500),
    (4_000, 500),
];
/// A grid cell votes for land or sea within this, km.
const COAST_VOTE_KM: i64 = 150;
/// Trench to west coast, km (Cascadia: ~100; Chile: 60–130).
const COAST_KM: (i64, i64) = (55, 90);
/// Trench to volcanic arc, km: 100 km of depth to the sinking slab over
/// the slab's dip (25° → 215 km, 45° → 100 km); Cascadia ~250, Chile ~250.
const ARC_KM: (i64, i64) = (150, 230);
/// Between two volcanoes of the arc, km (Cascades: 40–100).
const VOLCANO_SPACING_KM: (i64, i64) = (40, 85);

impl Zoom {
    /// Our continent as the history left it: the land of `site`'s piece
    /// (and any other land in the frame), read across its margin.
    pub fn new(planet: &Planet, site: &Site) -> Self {
        let seed = hash(&[planet.seed, P_ZOOM]);
        let centre = planet.lands[site.land].centre;
        let east = east_at(centre);
        let frame = Frame {
            centre,
            east,
            north: north_at(centre, east),
        };
        let n = ZOOM_SIDE * ZOOM_SIDE;
        let points: Vec<V3> = (0..n).map(|i| frame.point(i)).collect();
        let cur: Vec<u8> = points
            .iter()
            .map(|&p| planet.plates.plate_at(p) as u8)
            .collect();
        let old: Vec<u8> = points
            .iter()
            .map(|&p| planet.old.plate_at(p) as u8)
            .collect();
        let latitude: Vec<i16> = points.iter().map(|&p| latitude(p) as i16).collect();
        // The land the history left, cell by cell, read at each pixel: the
        // cells around vote, nearer ones more, at a gently wandering point.
        let mut piece = vec![false; CELLS];
        for &c in &planet.lands[site.land].cells {
            piece[c as usize] = true;
        }
        let floor = cos(COAST_VOTE_KM * ONE / RADIUS_KM);
        let coast_seed = hash(&[seed, 0xc0a5]);
        let (land_px, ours_px): (Vec<bool>, Vec<bool>) = points
            .iter()
            .map(|&p| {
                let q = warp(coast_seed, p, &COAST_WARP);
                let c = planet.sphere.cell_of(q);
                let (mut mine, mut other, mut sea) = (0, 0, 0);
                let around = std::iter::once(c)
                    .chain(planet.sphere.links[c].iter().map(|&(n, _)| n as usize));
                for k in around {
                    let w = (dot(q, planet.sphere.centre[k]) - floor).max(0);
                    if piece[k] {
                        mine += w;
                    } else if planet.elevation[k] > 0 {
                        other += w;
                    } else {
                        sea += w;
                    }
                }
                let land = mine + other > sea;
                (land, land && mine >= other)
            })
            .unzip();
        let sea_px: Vec<bool> = land_px.iter().map(|&l| !l).collect();
        let d_in = chamfer(&sea_px);
        let d_out = chamfer(&land_px);
        let (sink, ride) = (site.sinking as u8, site.riding as u8);
        // Boundaries of today and of old, per pixel.
        let mut line = vec![Line::None; n];
        let mut trench = vec![false; n];
        let mut trench_sink = vec![false; n];
        let mut sutures = vec![false; n];
        let mut rifts = vec![false; n];
        for i in 0..n {
            let (px, py) = (i % ZOOM_SIDE, i / ZOOM_SIDE);
            for (j, dir) in [
                (i + 1, frame.east),
                (i + ZOOM_SIDE, frame.north.map(|v| -v)),
            ] {
                if (j == i + 1 && px + 1 >= ZOOM_SIDE)
                    || (j == i + ZOOM_SIDE && py + 1 >= ZOOM_SIDE)
                {
                    continue;
                }
                let (a, b) = (cur[i], cur[j]);
                if a != b {
                    if (a, b) == (ride, sink) || (a, b) == (sink, ride) {
                        let (r, s) = if a == ride { (i, j) } else { (j, i) };
                        trench[r] = true;
                        trench_sink[s] = true;
                        line[i] = Line::Trench;
                    } else {
                        let (closing, slide) =
                            planet.plates.motion(a as usize, b as usize, points[i], dir);
                        line[i] = match classify(closing, slide, false, false, true) {
                            Kind::Ridge | Kind::Rift => Line::Divergent,
                            Kind::Transform => Line::Transform,
                            _ => Line::Convergent,
                        };
                    }
                }
                let (a, b) = (old[i], old[j]);
                if a != b && line[i] == Line::None {
                    let (closing, slide) =
                        planet.old.motion(a as usize, b as usize, points[i], dir);
                    match classify(closing, slide, true, true, false) {
                        Kind::Collision => {
                            sutures[i] = true;
                            line[i] = Line::Suture;
                        }
                        Kind::Rift => {
                            rifts[i] = true;
                            line[i] = Line::FailedRift;
                        }
                        _ => {}
                    }
                }
            }
        }
        let d_ride = chamfer(&trench);
        let d_sink = chamfer(&trench_sink);
        let d_suture = chamfer(&sutures);
        let d_rift = chamfer(&rifts);
        let of_line = |l: Line| chamfer(&line.iter().map(|&x| x == l).collect::<Vec<_>>());
        let (d_parting, d_meeting, d_sliding) = (
            of_line(Line::Divergent),
            of_line(Line::Convergent),
            of_line(Line::Transform),
        );
        // The chain's direction: the trench's main axis near the middle.
        let (mut sx, mut sy, mut sxx, mut syy, mut sxy, mut count) =
            (0_i64, 0_i64, 0_i64, 0_i64, 0_i64, 0_i64);
        for i in 0..n {
            if trench[i] {
                let (x, y) = Frame::xy(i);
                if y.abs() <= 400 {
                    sx += x;
                    sy += y;
                    sxx += x * x;
                    syy += y * y;
                    sxy += x * y;
                    count += 1;
                }
            }
        }
        let (ax, ay) = if count > 10 {
            let (mx, my) = (sx / count, sy / count);
            let cxx = sxx / count - mx * mx;
            let cyy = syy / count - my * my;
            let cxy = sxy / count - mx * my;
            let theta = angle_of(cxx - cyy, 2 * cxy) / 2;
            let (c, s) = (cos(tenths(theta)) / 1000, sin(tenths(theta)) / 1000);
            if s < 0 { (-c, -s) } else { (c, s) }
        } else {
            (0, 1000)
        };
        // The cross-section's numbers for this margin.
        let coast_km = draw(seed, 1, 0, COAST_KM.0, COAST_KM.1);
        let arc_km = draw(seed, 2, 0, ARC_KM.0, ARC_KM.1);
        let rate = site.rate;
        let squeezed = rate >= 45;
        let half_len = LENGTH_KM / 2;
        // Volcanoes along the arc, in (along, across) km.
        let mut cones: Vec<(i64, i64)> = Vec::new();
        let mut s = -half_len - 400 + draw(seed, 3, 0, 0, 40);
        let mut k = 0;
        while s < half_len + 400 {
            cones.push((s, arc_km + draw(seed, 4, k, -14, 14)));
            s += draw(seed, 5, k, VOLCANO_SPACING_KM.0, VOLCANO_SPACING_KM.1);
            k += 1;
        }
        let hotspot_points: Vec<(usize, i64)> = planet
            .hotspots
            .iter()
            .flat_map(|h| {
                h.track
                    .iter()
                    .filter_map(|&(x, age)| frame.pixel_of(x).map(|i| (i, age)))
            })
            .collect();
        let sig = |v: i64| v - 500;
        let mut elevation = vec![0_i32; n];
        let mut setting = vec![Setting::Ocean; n];
        let mut ours = vec![false; n];
        let mut along_of = vec![0_i64; n];
        let mut across_of = vec![0_i64; n];
        for i in 0..n {
            let (x, y) = Frame::xy(i);
            let along = (x * ax + y * ay) / 1000;
            let p = points[i];
            let pl = cur[i];
            // Across the margin, km: positive on the riding side.
            // A third plate's ground counts as the riding side: our
            // continent may straddle its border, which then crosses it.
            let d = if pl == sink {
                -(i64::from(d_sink[i]) / 1000)
            } else {
                i64::from(d_ride[i]) / 1000
            };
            along_of[i] = along;
            across_of[i] = d;
            let n1 = |purpose: u64, cell: i64| {
                sig(noise3(
                    hash(&[seed, purpose]),
                    [along * 1000, 0, 0],
                    cell * 1000,
                ))
            };
            let n2 = |purpose: u64, cell: i64| {
                sig(noise3(hash(&[seed, purpose]), p, cell * ONE / RADIUS_KM))
            };
            let inner = (n2(26, 50) * 60 + n2(27, 15) * 25) / 1000;
            let db = d + inner;
            let inside = land_px[i];
            // Into the land from its coast, and out to sea from it, km.
            let (inland, offshore) = (i64::from(d_in[i]) / 1000, i64::from(d_out[i]) / 1000);
            // Ocean floor, the trench and the arc's ridge, everywhere.
            let mut e: i64 = if d <= -1000 {
                -4_200
            } else if d < 0 {
                -4_600 - 2_800 * bump(d, 70) / 1000
            } else {
                -3_400 - 3_400 * bump(d, 70) / 1000
            };
            let rough = (n2(20, 18) * 2 + n2(21, 6)) / 6;
            if d > -1000 {
                e += 2_300 * bump(d - arc_km, 50) / 1000;
                e += 1_900 * bump(d - (coast_km + 25), 38) / 1000;
                for &(cs, cd) in &cones {
                    if (along - cs).abs() < 30 {
                        let r = ((along - cs).pow(2) + (d - cd).pow(2)).isqrt();
                        e += 1_900 * bump(r, 16) / 1000;
                    }
                }
            }
            let mut set = if d <= -1000 {
                Setting::Ocean
            } else if d < 80 {
                Setting::Trench
            } else {
                Setting::Ocean
            };
            if inside {
                let edge = inland;
                let gap = n1(16, 70);
                let mut base: i64 = 260 + rough;
                set = Setting::BackArc;
                let fore_lo = coast_km + 55;
                let fore_hi = arc_km - 38;
                // Each band fades into the next over ~15–40 km: no step.
                let ramp = |v: i64, over: i64| (v * 1000 / over).clamp(0, 1000);
                if db < fore_lo {
                    set = Setting::CoastRange;
                    // Where the coast range sags, the sea comes through.
                    if gap > 330 {
                        base -= 900 + 2_000 * bump(d - (coast_km + 25), 38) / 1000;
                    }
                }
                let fore = ramp(db - fore_lo, 15) * ramp(fore_hi - db, 15) / 1000;
                if fore > 0 {
                    let floor = -110 + n1(17, 90) * 800 / 1000 + n2(18, 26) * 3 / 4 + n2(31, 9) / 4;
                    base = (base * (1000 - fore) + floor * fore) / 1000;
                    if fore > 500 {
                        set = Setting::ForearcBasin;
                    }
                }
                if db >= fore_hi && db < arc_km + 45 {
                    set = Setting::Arc;
                }
                // Behind the arc: an upland rising off the volcanoes' back
                // and sinking toward the east coast.
                let behind = db - arc_km - 45;
                let plateau = if squeezed { 950 } else { 450 };
                base += plateau * ramp(db - arc_km, 45) / 1000 * (1000 - ramp(behind, 200)) / 1000;
                if behind >= 0 {
                    let basin = n2(19, 160) + n2(32, 50) / 3;
                    let sag = ramp(-60 - basin, 160);
                    if sag > 0 {
                        // A flat floor; only the deepest sag is drowned.
                        let floor = 40 + (basin + 330) * 2 / 3 + rough / 8;
                        base = (base * (1000 - sag) + floor * sag) / 1000;
                        if sag > 300 {
                            set = Setting::BackArcBasin;
                        }
                    }
                    let core = n2(22, 130) + n2(33, 45) / 3;
                    let rise = ramp(core - 80, 200) * ramp(behind - 40, 60) / 1000;
                    if rise > 0 {
                        base += 420 * rise / 1000;
                        if rise > 200 {
                            set = Setting::OldCore;
                        }
                    }
                    if inland < 45 && base < 350 {
                        set = Setting::CoastalPlain;
                        base = base.min(4 + inland * 2 + rough / 12);
                    }
                }
                let sut = i64::from(d_suture[i]) / 1000;
                if sut < 40 {
                    base += 320 * bump(sut, 40) / 1000;
                    if !matches!(set, Setting::Arc | Setting::CoastRange) {
                        set = Setting::Suture;
                    }
                }
                let rift = i64::from(d_rift[i]) / 1000;
                if rift < 45 {
                    base += -900 * bump(rift, 24) / 1000 + 260 * bump(rift - 32, 14) / 1000;
                    if rift < 24 && set != Setting::Arc {
                        set = Setting::FailedRift;
                    }
                }
                // Today's other borders, where they cross our land: a rift
                // opens a trough the sea may enter, a meeting raises a
                // range, a transform cuts a straight valley.
                let parting = i64::from(d_parting[i]) / 1000;
                if parting < 70 {
                    base += -1_400 * bump(parting, 34) / 1000 + 380 * bump(parting - 48, 20) / 1000;
                    if parting < 34 {
                        set = Setting::Rift;
                    }
                }
                let meeting = i64::from(d_meeting[i]) / 1000;
                if meeting < 60 {
                    base += 1_500 * bump(meeting, 60) / 1000;
                    set = Setting::Range;
                }
                let sliding = i64::from(d_sliding[i]) / 1000;
                if sliding < 12 {
                    base -= 260 * bump(sliding, 12) / 1000;
                }
                // The shore: a shelf just off the coast, low ground along it.
                let shore = (edge * 1000 / 12).clamp(0, 1000);
                let land = base + (e + 3_400).max(0) * 1000 / 1000;
                e = (land * shore + (-80) * (1000 - shore)) / 1000;
                ours[i] = ours_px[i];
            } else if offshore < 200 {
                // Just off the coasts, the shelf falls to the deep.
                e = e.max(-80 - offshore * 70);
            }
            // Hotspots: islands and seamounts in the ocean, lavas on land.
            for &(hp, age) in &hotspot_points {
                let (hx, hy) = Frame::xy(hp);
                let r = ((x - hx).pow(2) + (y - hy).pow(2)).isqrt();
                if inside {
                    if r < 70 {
                        e += 700 * bump(r, 70) / 1000 / (1 + age / 15);
                        set = Setting::Hotspot;
                    }
                } else if r < 25 {
                    e += 4_600 * bump(r, 25) / 1000 * (TRACK_MYR - age).max(0) / TRACK_MYR;
                }
            }
            elevation[i] = e.clamp(-9_000, 7_000) as i32;
            setting[i] = set;
        }
        // Volcanoes standing above the sea, at their summit pixel.
        let mut volcanoes = Vec::new();
        for &(cs, cd) in &cones {
            let mut best: Option<(i32, usize)> = None;
            for i in 0..n {
                if (along_of[i] - cs).abs() <= 6
                    && (across_of[i] - cd).abs() <= 6
                    && elevation[i] > 0
                {
                    let e = elevation[i];
                    if best.is_none_or(|(b, _)| e > b) {
                        best = Some((e, i));
                    }
                }
            }
            if let Some((_, i)) = best {
                volcanoes.push(i);
            }
        }
        // Marshes to come: low flat land near water or in a basin.
        let sea: Vec<bool> = elevation.iter().map(|&e| e <= 0).collect();
        let d_water = chamfer(&sea);
        let d_sea = chamfer(&open_sea(&elevation));
        let mut marsh = vec![false; n];
        for i in 0..n {
            if elevation[i] <= 0 || elevation[i] > 60 {
                continue;
            }
            let near_sea = d_water[i] <= 30_000;
            let basin = matches!(
                setting[i],
                Setting::ForearcBasin
                    | Setting::BackArcBasin
                    | Setting::CoastalPlain
                    | Setting::FailedRift
            );
            if near_sea || basin {
                let (px, py) = (i % ZOOM_SIDE, i / ZOOM_SIDE);
                let mut flat = true;
                for (dx, dy) in [(-2_i64, 0_i64), (2, 0), (0, -2), (0, 2)] {
                    let qx = px as i64 + dx;
                    let qy = py as i64 + dy;
                    if (0..ZOOM_SIDE as i64).contains(&qx) && (0..ZOOM_SIDE as i64).contains(&qy) {
                        let j = qy as usize * ZOOM_SIDE + qx as usize;
                        if (elevation[j] - elevation[i]).abs() > 30 {
                            flat = false;
                        }
                    }
                }
                marsh[i] = flat;
            }
        }
        let mut zoom = Self {
            frame,
            elevation,
            setting,
            line,
            ours,
            marsh,
            latitude,
            volcanoes,
            hotspot_points,
            arc_km,
            coast_km,
            rate,
            numbers: ZoomNumbers::default(),
        };
        zoom.numbers = zoom.measure(&along_of, &across_of, &d_sea);
        zoom
    }

    fn measure(&self, along: &[i64], across: &[i64], d_sea: &[u32]) -> ZoomNumbers {
        let n = ZOOM_SIDE * ZOOM_SIDE;
        let px = ZOOM_PX_KM * ZOOM_PX_KM;
        let mut m = ZoomNumbers::default();
        let (mut lo, mut hi) = (i64::MAX, i64::MIN);
        let mut lee = 0;
        let (mut far50, mut far100) = (0, 0);
        let mut land_px = 0;
        for i in 0..n {
            let e = i64::from(self.elevation[i]);
            if e <= 0 {
                continue;
            }
            if !self.ours[i] {
                m.islands_km2 += px;
                continue;
            }
            land_px += 1;
            m.land_km2 += px;
            m.highest_m = m.highest_m.max(e);
            lo = lo.min(along[i]);
            hi = hi.max(along[i]);
            let sea_km = i64::from(d_sea[i]) / 1000;
            m.farthest_sea_km = m.farthest_sea_km.max(sea_km);
            far50 += i64::from(sea_km > 50);
            far100 += i64::from(sea_km > 100);
            if across[i] > self.arc_km {
                lee += 1;
            }
            if across[i] > self.arc_km + 60 && sea_km > 40 {
                m.interior_km2 += px;
            }
            match self.setting[i] {
                Setting::ForearcBasin | Setting::FailedRift | Setting::Rift => m.basin_km2 += px,
                Setting::BackArcBasin | Setting::CoastalPlain => {
                    m.basin_km2 += px;
                    m.platform_km2 += px;
                }
                Setting::OldCore => m.old_core_km2 += px,
                _ => {}
            }
            if self.marsh[i] {
                m.marsh_km2 += px;
            }
        }
        if land_px > 0 {
            m.length_km = hi - lo;
            m.mean_width_km = m.land_km2 / m.length_km.max(1);
            m.lee_km = lee * px / m.length_km.max(1);
            m.far_50 = far50 * 1000 / land_px;
            m.far_100 = far100 * 1000 / land_px;
        }
        m.volcanoes = self.volcanoes.iter().filter(|&&i| self.ours[i]).count() as i64;
        let seen = open_sea(&self.elevation);
        m.enclosed_sea_km2 = (0..n)
            .filter(|&i| self.elevation[i] <= 0 && !seen[i])
            .count() as i64
            * px;
        m
    }
}

/// The water joined to the frame's edge: the open ocean, not the lakes
/// and closed seas.
pub fn open_sea(elevation: &[i32]) -> Vec<bool> {
    let n = ZOOM_SIDE * ZOOM_SIDE;
    let mut seen = vec![false; n];
    let mut queue: Vec<usize> = (0..n)
        .filter(|&i| {
            let (x, y) = (i % ZOOM_SIDE, i / ZOOM_SIDE);
            (x == 0 || y == 0 || x + 1 == ZOOM_SIDE || y + 1 == ZOOM_SIDE) && elevation[i] <= 0
        })
        .collect();
    for &i in &queue {
        seen[i] = true;
    }
    let mut k = 0;
    while k < queue.len() {
        let i = queue[k];
        k += 1;
        let (x, y) = (i % ZOOM_SIDE, i / ZOOM_SIDE);
        let mut next = Vec::with_capacity(4);
        if x > 0 {
            next.push(i - 1);
        }
        if x + 1 < ZOOM_SIDE {
            next.push(i + 1);
        }
        if y > 0 {
            next.push(i - ZOOM_SIDE);
        }
        if y + 1 < ZOOM_SIDE {
            next.push(i + ZOOM_SIDE);
        }
        for j in next {
            if !seen[j] && elevation[j] <= 0 {
                seen[j] = true;
                queue.push(j);
            }
        }
    }
    seen
}

/// Distance to the nearest set pixel, m, by a two-pass chamfer (2 km
/// straight, 2.83 km diagonal): within 5 % of the true distance.
pub fn chamfer(sources: &[bool]) -> Vec<u32> {
    let side = ZOOM_SIDE;
    let straight = (ZOOM_PX_KM * 1000) as u32;
    let diagonal = straight * 1414 / 1000;
    let far = u32::MAX / 2;
    let mut d: Vec<u32> = sources.iter().map(|&s| if s { 0 } else { far }).collect();
    for y in 0..side {
        for x in 0..side {
            let i = y * side + x;
            let mut v = d[i];
            if x > 0 {
                v = v.min(d[i - 1] + straight);
            }
            if y > 0 {
                v = v.min(d[i - side] + straight);
                if x > 0 {
                    v = v.min(d[i - side - 1] + diagonal);
                }
                if x + 1 < side {
                    v = v.min(d[i - side + 1] + diagonal);
                }
            }
            d[i] = v;
        }
    }
    for y in (0..side).rev() {
        for x in (0..side).rev() {
            let i = y * side + x;
            let mut v = d[i];
            if x + 1 < side {
                v = v.min(d[i + 1] + straight);
            }
            if y + 1 < side {
                v = v.min(d[i + side] + straight);
                if x + 1 < side {
                    v = v.min(d[i + side + 1] + diagonal);
                }
                if x > 0 {
                    v = v.min(d[i + side - 1] + diagonal);
                }
            }
            d[i] = v;
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_finds_its_own_cells_and_covers_the_sphere() {
        let s = Sphere::new();
        for c in 0..CELLS {
            assert_eq!(s.cell_of(s.centre[c]), c);
            assert!(
                s.links[c].len() >= 6,
                "cell {c} has {} neighbours",
                s.links[c].len()
            );
        }
        let total: i64 = s.area_km2.iter().map(|&a| i64::from(a)).sum();
        let earth = 510_064_472_i64;
        assert!((total - earth).abs() < earth / 100, "{total}");
    }

    #[test]
    fn integer_trigonometry_holds() {
        assert_eq!(sin(0), 0);
        assert!((sin(HALF_PI) - ONE).abs() < 3);
        assert!((cos(PI) + ONE).abs() < 3);
        assert!(latitude([0, 0, ONE]) >= 899);
        assert!((latitude(from_lat_lon(450, 300)) - 450).abs() <= 1);
        assert!((longitude(from_lat_lon(-300, -1200)) + 1200).abs() <= 1);
    }

    #[test]
    fn a_seed_gives_the_same_planet() {
        let a = Planet::new(3);
        let b = Planet::new(3);
        assert_eq!(a.elevation, b.elevation);
        assert_eq!(a.plate, b.plate);
        assert_eq!(a.history.plate, b.history.plate);
        assert_eq!(a.history.age, b.history.age);
        assert_eq!(a.history.orogeny_my, b.history.orogeny_my);
        let land: i64 = a.lands.iter().map(|l| l.area_km2).sum();
        assert!(
            (land * 1000 / 510_000_000 - Settings::default().land_per_mille).abs() < 20,
            "{land}"
        );
    }

    #[test]
    fn lands_come_from_the_history_large_but_not_gigantic() {
        let seed = 14;
        {
            let world = Planet::new(seed);
            let biggest = &world.lands[0];
            assert!(
                (14_000_000..=30_000_000).contains(&biggest.area_km2),
                "seed {seed}: {}",
                biggest.area_km2
            );
            let large = world
                .lands
                .iter()
                .filter(|l| l.area_km2 > 1_000_000)
                .count();
            assert!(large >= 4, "seed {seed}: {large} lands over 1 M km²");
            let mut pieces: Vec<u8> = biggest
                .cells
                .iter()
                .map(|&c| world.history.piece[c as usize])
                .collect();
            pieces.sort_unstable();
            pieces.dedup();
            assert!(
                pieces.len() >= 3,
                "seed {seed}: the largest land is one piece"
            );
            assert!(
                world.history.collisions.iter().any(|c| c.front_cells >= 40),
                "seed {seed}: no collision"
            );
        }
    }

    #[test]
    fn continental_crust_is_kept_through_the_history() {
        // Collisions shorten and thicken it, wear builds the margins out:
        // its area holds within a tenth of the start at every step.
        for seed in [1, 9] {
            let world = Planet::new(seed);
            let area = &world.history.continental_km2;
            for (t, &k) in area.iter().enumerate() {
                assert!(
                    (k - area[0]).abs() * 10 <= area[0],
                    "seed {seed} step {t}: {k} of {}",
                    area[0]
                );
            }
        }
    }

    #[test]
    fn a_continent_carries_its_relief() {
        // Followed by its home from step to step, a crust keeps its height
        // unless something happens to it: little of it crosses the level
        // of the start's sea in a step (a height recomputed at each step
        // sent a tenth of it across).
        let world = Planet::new(9);
        let h = &world.history;
        let height = |t: usize, c: usize| {
            let home = h.home_at[t][c] as usize;
            i64::from(h.high_at[t][c])
                + i64::from(h.rough[home])
                + i64::from(h.broad[home]) / 2
                + (i64::from(h.thick_at[t][c]) - i64::from(history::CONTINENT_THICK)).max(0)
                    * history::ISOSTASY_M
                    / 10
        };
        let (mut followed, mut crossed) = (0_i64, 0_i64);
        for t in 1..=h.last() {
            let mut was = vec![None; CELLS];
            for c in 0..CELLS {
                if h.class[t - 1][c] != history::OCEAN {
                    was[h.home_at[t - 1][c] as usize] = Some(height(t - 1, c) > 0);
                }
            }
            for c in 0..CELLS {
                if h.class[t][c] == history::OCEAN {
                    continue;
                }
                if let Some(dry) = was[h.home_at[t][c] as usize] {
                    followed += 1;
                    crossed += i64::from(dry != (height(t, c) > 0));
                }
            }
        }
        assert!(crossed * 100 <= followed * 4, "{crossed} of {followed}");
    }

    #[test]
    fn the_history_breaks_the_supercontinent_and_keeps_its_records() {
        let world = Planet::new(11);
        let h = &world.history;
        // At the start, one supercontinent of the settings' crust share.
        let first = &h.class[0];
        let land = (0..CELLS)
            .filter(|&c| first[c] == history::CONTINENT)
            .count();
        let share = land as i64 * 1000 / CELLS as i64;
        assert!(
            (share - Settings::default().crust_per_mille).abs() < 60,
            "{share}"
        );
        // Today, every piece of crust carries a record.
        for c in 0..CELLS {
            if world.continental[c] {
                assert!((h.home[c] as usize) < CELLS);
            }
            // The supercontinent's crust is old (an arc may be born today).
            if h.piece[c] < history::TERRANE_BASE {
                assert!(h.age_my[c] >= 700, "{}", h.age_my[c]);
            }
        }
        // Leading edges today are young mountains.
        let last = h.last();
        assert!((0..CELLS).any(|c| h.margin[last][c] == history::Margin::Active as u8
            && h.orogeny_my[c] == 0));
        // The ocean floor is young near the ridges and old far from them:
        // new floor was made, old floor sank.
        let ocean: Vec<u16> = (0..CELLS)
            .filter(|&c| h.class[last][c] == history::OCEAN)
            .map(|c| h.age[last][c])
            .collect();
        let young = ocean.iter().filter(|&&a| a <= 20).count();
        assert!(young * 20 > ocean.len(), "{young} of {}", ocean.len());
    }

    #[test]
    fn the_mantle_is_warm_under_the_supercontinent_and_the_sea_is_anchored_today() {
        let world = Planet::new(3);
        let h = &world.history;
        // At the start the supercontinent is a lid: the mantle under it is
        // warmer than under the ocean, and most plumes rise under it.
        let mean = |land: bool| {
            let cells: Vec<i64> = (0..CELLS)
                .filter(|&c| (h.class[0][c] != history::OCEAN) == land)
                .map(|c| i64::from(h.heat_at[0][c]))
                .collect();
            cells.iter().sum::<i64>() / cells.len() as i64
        };
        assert!(
            mean(true) > mean(false) + 300,
            "{} {}",
            mean(true),
            mean(false)
        );
        assert!(h.hotspots.len() >= 14);
        // Slabs have cooled it somewhere since.
        assert!(h.heat_at[h.last()].iter().any(|&v| v < -200));
        // The sea is today's at the last step, and moved in the past.
        assert_eq!(world.sea_rise_m, 0);
        assert!((0..h.last()).any(|t| world.at_step(t).sea_rise_m != 0));
    }

    #[test]
    fn a_plate_is_one_piece_and_no_small_plate_sits_inside_another() {
        // Parts of a plate cut off from the rest and small plates enclosed
        // in one other plate, counted over the whole history (a rift of
        // the step may leave one until the next step).
        let mut strays = 0;
        for seed in [14, 17] {
            let world = Planet::new(seed);
            let h = &world.history;
            for t in 1..=h.last() {
                let plate = &h.plate[t];
                let mut seen = vec![false; CELLS];
                let mut parts: Vec<(u8, i64, usize)> = Vec::new();
                for start in 0..CELLS {
                    if seen[start] {
                        continue;
                    }
                    seen[start] = true;
                    let (mut km2, mut around, mut stack) = (0, Vec::new(), vec![start]);
                    while let Some(c) = stack.pop() {
                        km2 += i64::from(world.sphere.area_km2[c]);
                        for &(n, _) in &world.sphere.links[c] {
                            let n = n as usize;
                            if plate[n] != plate[start] {
                                if !around.contains(&plate[n]) {
                                    around.push(plate[n]);
                                }
                            } else if !seen[n] {
                                seen[n] = true;
                                stack.push(n);
                            }
                        }
                    }
                    parts.push((plate[start], km2, around.len()));
                }
                for &(id, km2, around) in &parts {
                    let largest = parts.iter().filter(|p| p.0 == id).map(|p| p.1).max();
                    if km2 < 2_000_000 && (around <= 1 || Some(km2) < largest) {
                        strays += 1;
                    }
                }
            }
        }
        assert!(strays <= 8, "{strays} stray parts of plates");
    }

    #[test]
    fn every_cell_is_on_exactly_one_plate_at_every_step() {
        for seed in [11, 17] {
            let world = Planet::new(seed);
            let h = &world.history;
            for t in 0..=h.last() {
                // One plate per cell, a plate id (never "none"), whose
                // motion at that step is on record.
                let mut cells = vec![0_usize; history::MAXP];
                for &p in &h.plate[t] {
                    assert!(
                        (p as usize) < h.motion[t].len(),
                        "seed {seed} step {t}: plate {p}"
                    );
                    cells[p as usize] += 1;
                }
                assert_eq!(cells.iter().sum::<usize>(), CELLS);
            }
        }
    }
}
