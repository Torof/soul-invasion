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
//! 1. **Crust.** Continental crust is a set of separate continents
//!    (version 2, the lead's 26 November): warped caps of drawn sizes,
//!    kept apart so no two large ones fuse into a supercontinent, large
//!    and middling ones kept below ~60° of latitude, their coasts frayed
//!    by noise. About a fifth of the surface carries crust.
//! 2. **Plates.** A warped, weighted Voronoi on the sphere, each plate
//!    turning about its own pole; plates carrying much continent move
//!    slower, as on the Earth.
//! 3. **Boundaries.** Each boundary cell is classified by the relative
//!    motion of the two plates: convergent (collision, continental arc,
//!    island arc, or the trench of the plate that sinks), divergent
//!    (ridge in the ocean, rift in a continent), or transform.
//! 4. **An older layout.** A second, older set of plates, drawn the same
//!    way, leaves its marks inside the continents: old sutures (worn
//!    ranges) where its plates met, failed rifts where they parted.
//! 5. **Hotspots.** Fixed plumes under moving plates leave chains, older
//!    the farther they run in the plate's direction of motion.
//! 6. **Height and the sea.** A height from these, and a sea level that
//!    leaves [`LAND_PER_MILLE`] of the surface dry (17.5 %).
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

use hash::hash;

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
const P_CRUST: u64 = 0x0c50_0001;
const P_CRUST_WARP: u64 = 0x0c50_0002;
const P_CONTINENTS: u64 = 0x0c50_0003;
const P_PLATES: u64 = 0x0c50_0010;
const P_OLD: u64 = 0x0c50_0020;
const P_HOTSPOTS: u64 = 0x0c50_0030;
const P_RELIEF: u64 = 0x0c50_0040;
const P_ZOOM: u64 = 0x0c50_0050;

/// Plates of today: 10 to 15, as the Earth has 7 large and ~8 middling.
const PLATES: (i64, i64) = (10, 15);
/// Plates of the older layout: fewer, larger (a supercontinent's time).
const OLD_PLATES: (i64, i64) = (6, 9);
/// The largest weight a plate's seed point gets, in [`ONE`] units, drawn
/// squared so a few plates are large and many small.
const PLATE_WEIGHT: i64 = 280_000;
/// Plate speeds at 90° from their pole, mm a year (the Earth's: 10–100).
const SPEED_MM: (i64, i64) = (25, 95);
/// A plate all continent moves this share slower, per mille.
const CONTINENT_DRAG: i64 = 600;
/// The plates' borders' warp, `(cell, amplitude)` in [`ONE`] units.
const PLATE_WARP: [(i64, i64); 4] = [
    (600_000, 200_000),
    (200_000, 70_000),
    (70_000, 22_000),
    (25_000, 7_000),
];
/// A tier of [`CONTINENT_TIERS`].
type Tier = ((i64, i64), (i64, i64), i64, i64);
/// The continents, by tier, largest first: `(count, radius km, gap km,
/// farthest latitude its cap may reach, tenths of a degree)`. Each
/// continent is a few overlapping lobes inside its cap, so it is long,
/// bent or lobed rather than round. Large and
/// middling ones stay below ~62° so the cold north and south are not
/// millions of km² of ice; small ones and islets may go farther. The gap
/// keeps two caps apart, so the largest land stays near one cap's size
/// (the lead's ceiling: ~10–15 M km², 26 November).
const CONTINENT_TIERS: [Tier; 4] = [
    ((4, 6), (1_900, 2_350), 600, 620),
    ((4, 7), (1_300, 1_900), 500, 620),
    ((8, 12), (550, 1_000), 400, 680),
    ((10, 16), (150, 350), 250, 780),
];
/// Tries to place a cap before it is given up.
const CONTINENT_TRIES: u64 = 400;
/// The crust's noise, `(cell, weight)`: frays each cap's coast by about a
/// quarter of its radius, in or out.
const CRUST_OCTAVES: [(i64, i64); 4] = [(900_000, 5), (450_000, 4), (200_000, 2), (80_000, 1)];
const CRUST_WARP: [(i64, i64); 2] = [(500_000, 90_000), (150_000, 30_000)];
/// Share of the surface carried by continental crust, per mille: the
/// lobes swell or shrink a little to it, whatever caps were placed.
const CONTINENTAL_PER_MILLE: i64 = 220;
/// Satellite lobes per continent, and their radius and the main lobe's,
/// per mille of the cap's.
const LOBES: (i64, i64) = (2, 4);
const LOBE_RADIUS: (i64, i64) = (300, 520);
const MAIN_RADIUS: (i64, i64) = (550, 720);
/// Share of the surface left dry by the sea level, per mille (the lead's
/// 15–20 %, 26 November; the Earth: 29 %).
pub const LAND_PER_MILLE: i64 = 175;

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
}

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
        }
    }

    /// The plate at a point.
    pub fn plate_at(&self, p: V3) -> usize {
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
    /// Each cell's crust value (the noise, 0..1000 and a little beyond).
    pub crust: Vec<i64>,
    /// The threshold of continental crust.
    pub crust_cut: i64,
    /// Whether a cell's crust is continental.
    pub continental: Vec<bool>,
    /// Boundary cells of today.
    pub edge: Vec<Option<Edge>>,
    /// Boundary cells of the older layout (all crusts).
    pub old_edge: Vec<Option<OldKind>>,
    /// Each cell's distance to the nearest boundary of its own plate, km,
    /// and that boundary's cell.
    pub near: Vec<(u32, u32)>,
    /// The hotspots.
    pub hotspots: Vec<Hotspot>,
    /// Each cell's height above the sea, m.
    pub elevation: Vec<i32>,
    /// The pieces of land, largest first.
    pub lands: Vec<Landmass>,
    /// Cells reshaped for our continent (none until [`Planet::shape`]).
    pub ours: Vec<bool>,
}

/// The continents' lobes, `(centre, radius km)`: caps placed tier by
/// tier, each clear of those before it by both radii and the larger gap,
/// each filled with a main lobe and a few satellites touching its rim.
fn continents(seed: u64) -> Vec<(V3, i64)> {
    let mut caps: Vec<(V3, i64, i64)> = Vec::new();
    for (t, &((lo, hi), (r_lo, r_hi), gap, reach)) in CONTINENT_TIERS.iter().enumerate() {
        let t = t as u64;
        let count = draw(seed, P_CONTINENTS, t, lo, hi) as u64;
        for k in 0..count {
            let r = draw(seed, P_CONTINENTS, 100 + t * 100 + k, r_lo, r_hi);
            let r_tenths = r * 18_000 / (PI * RADIUS_KM / 100_000);
            for attempt in 0..CONTINENT_TRIES {
                let c = random_unit(
                    seed,
                    P_CONTINENTS + 1,
                    (t * 100 + k) * CONTINENT_TRIES + attempt,
                );
                if latitude(c).abs() + r_tenths > reach {
                    continue;
                }
                if caps
                    .iter()
                    .all(|&(o, ro, go)| km_between(c, o) >= r + ro + gap.max(go))
                {
                    caps.push((c, r, gap));
                    break;
                }
            }
        }
    }
    let mut lobes = Vec::new();
    for (k, &(c, r, _)) in caps.iter().enumerate() {
        let k = 1000 * k as u64;
        let (east, north) = (east_at(c), north_at(c, east_at(c)));
        let toward = |j: u64| {
            let a = draw(seed, P_CONTINENTS + 2, k + j, 0, TAU);
            normalize(add(scale(east, cos(a)), scale(north, sin(a))))
        };
        let main = draw(seed, P_CONTINENTS + 3, k, MAIN_RADIUS.0, MAIN_RADIUS.1);
        let off = draw(seed, P_CONTINENTS + 4, k, 0, 1000 - main);
        lobes.push((step(c, toward(0), r * off / 1000), r * main / 1000));
        for j in 1..=draw(seed, P_CONTINENTS + 5, k, LOBES.0, LOBES.1) as u64 {
            let lr = r * draw(seed, P_CONTINENTS + 6, k + j, LOBE_RADIUS.0, LOBE_RADIUS.1) / 1000;
            lobes.push((step(c, toward(j), r - lr), lr));
        }
    }
    lobes
}

/// Hotspots: 14 to 20 (the Earth has ~40 listed, ~15 strong).
const HOTSPOTS: (i64, i64) = (14, 20);
/// How far back a hotspot's chain is followed, millions of years.
const TRACK_MYR: i64 = 70;
/// A chain's step, km.
const TRACK_STEP_KM: i64 = 100;

impl Planet {
    /// The planet born from `seed`.
    pub fn new(seed: u64) -> Self {
        let sphere = Sphere::new();
        // 1. Crust.
        let caps = continents(seed);
        let crust: Vec<i64> = sphere
            .centre
            .iter()
            .map(|&p| {
                let q = warp(hash(&[seed, P_CRUST_WARP]), p, &CRUST_WARP);
                // How far inside the nearest cap, per mille of its radius.
                let mut inside = -1000;
                for &(c, r) in &caps {
                    if dot(q, c) < cos((r + 200) * ONE / RADIUS_KM) {
                        continue;
                    }
                    inside = inside.max((r - km_between(q, c)) * 1000 / r);
                }
                let fray = fbm3(hash(&[seed, P_CRUST]), q, &CRUST_OCTAVES) - 500;
                500 + (inside.max(-1000) + fray * 3 / 2) / 4
            })
            .collect();
        let crust_cut = quantile(&sphere, &crust, 1000 - CONTINENTAL_PER_MILLE);
        let continental: Vec<bool> = crust.iter().map(|&c| c >= crust_cut).collect();
        // 2. Plates.
        let mut plates = Layout::draw(seed, P_PLATES, PLATES);
        let plate: Vec<u8> = sphere
            .centre
            .iter()
            .map(|&p| plates.plate_at(p) as u8)
            .collect();
        let mut area = vec![0_i64; plates.plates.len()];
        let mut cont_area = vec![0_i64; plates.plates.len()];
        for c in 0..CELLS {
            area[plate[c] as usize] += i64::from(sphere.area_km2[c]);
            if continental[c] {
                cont_area[plate[c] as usize] += i64::from(sphere.area_km2[c]);
            }
        }
        for (k, p) in plates.plates.iter_mut().enumerate() {
            let share = cont_area[k] * 1000 / area[k].max(1);
            p.speed = p.speed * (1000 - CONTINENT_DRAG * share / 1000) / 1000;
        }
        let old = Layout::draw(seed, P_OLD, OLD_PLATES);
        let old_plate: Vec<u8> = sphere
            .centre
            .iter()
            .map(|&p| old.plate_at(p) as u8)
            .collect();
        // 3. Boundaries.
        let mut edge = vec![None; CELLS];
        let mut old_edge = vec![None; CELLS];
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
            if let Some((other, normal, _)) = across(&sphere, &old, &old_plate, &continental, c) {
                let (closing, slide) =
                    old.motion(old_plate[c] as usize, other, sphere.centre[c], normal);
                old_edge[c] = Some(match classify(closing, slide, true, true, false) {
                    Kind::Collision => OldKind::Suture,
                    Kind::Rift => OldKind::FailedRift,
                    _ => OldKind::OldFault,
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
        let ridges: Vec<(usize, u32)> = (0..CELLS)
            .filter(|&c| {
                matches!(
                    edge[c],
                    Some(Edge {
                        kind: Kind::Ridge,
                        ..
                    })
                )
            })
            .map(|c| (c, 0))
            .collect();
        let (ridge_d, _) = sphere.spread(&ridges, |_, b| !continental[b], 5_000);
        let shore: Vec<(usize, u32)> = (0..CELLS)
            .filter(|&c| !continental[c])
            .map(|c| (c, 0))
            .collect();
        let (inland_d, _) = sphere.spread(&shore, |_, b| continental[b], 2_000);
        let sutures: Vec<(usize, u32)> = (0..CELLS)
            .filter(|&c| continental[c] && old_edge[c].is_some_and(|k| k != OldKind::OldFault))
            .map(|c| (c, c as u32))
            .collect();
        let (old_d, old_l) = sphere.spread(&sutures, |_, b| continental[b], 400);
        // 5. Hotspots.
        let count = draw(seed, P_HOTSPOTS, 0, HOTSPOTS.0, HOTSPOTS.1) as u64;
        let hotspots: Vec<Hotspot> = (0..count)
            .map(|k| {
                let at = random_unit(seed, P_HOTSPOTS + 1, k);
                let home = plates.plate_at(at);
                let mut track = vec![(at, 0)];
                let mut x = at;
                let mut age_kyr = 0;
                loop {
                    let v = plates.velocity(home, x);
                    let speed = norm(v);
                    if speed < 5 {
                        break;
                    }
                    x = step(x, normalize(v), TRACK_STEP_KM);
                    age_kyr += TRACK_STEP_KM * 1000 / speed;
                    if age_kyr > TRACK_MYR * 1000 || plates.plate_at(x) != home {
                        break;
                    }
                    track.push((x, age_kyr / 1000));
                }
                Hotspot { at, track }
            })
            .collect();
        // 6. Height.
        let relief_seed = hash(&[seed, P_RELIEF]);
        let near_track = cos(250 * ONE / RADIUS_KM);
        let mut elevation: Vec<i64> = (0..CELLS)
            .map(|c| {
                let p = sphere.centre[c];
                let rough = fbm3(relief_seed, p, &[(120_000, 2), (40_000, 1)]) - 500;
                let mut e = if continental[c] {
                    let inland = i64::from(inland_d[c].min(600));
                    -300 + inland * 900 / 600 + (crust[c] - crust_cut) * 6 + rough * 2
                } else {
                    let r = i64::from(ridge_d[c].min(3_500));
                    -2_600 - 3_000 * ((r * 1_000_000 / 3_500).isqrt()) / 1000 + rough
                };
                let (d, src) = near[c];
                if d != u32::MAX {
                    let d = i64::from(d);
                    let b = edge[src as usize].expect("a source is an edge");
                    let r = i64::from(b.rate).min(80);
                    e += match b.kind {
                        Kind::Collision => (1_500 + 30 * r) * bump(d, 500) / 1000,
                        Kind::Arc => {
                            (1_600 + 15 * r) * bump(d - 180, 140) / 1000
                                + 700 * bump(d - 40, 60) / 1000
                        }
                        Kind::IslandArc => 3_800 * bump(d - 140, 90) / 1000,
                        Kind::Trench => -2_600 * bump(d, 90) / 1000,
                        Kind::Rift => {
                            -(800 + 20 * r) * bump(d, 110) / 1000 + 300 * bump(d - 150, 70) / 1000
                        }
                        Kind::Ridge => 0,
                        Kind::Transform => {
                            if continental[c] {
                                -200 * bump(d, 60) / 1000
                            } else {
                                0
                            }
                        }
                    };
                }
                if continental[c] && old_d[c] != u32::MAX {
                    let d = i64::from(old_d[c]);
                    e += match old_edge[old_l[c] as usize] {
                        Some(OldKind::Suture) => 450 * bump(d, 220) / 1000,
                        Some(OldKind::FailedRift) => -650 * bump(d, 110) / 1000,
                        _ => 0,
                    };
                }
                for h in &hotspots {
                    for &(x, age) in &h.track {
                        if dot(p, x) < near_track {
                            continue;
                        }
                        let d = km_between(p, x);
                        e += if continental[c] {
                            500 * bump(d, 250) / 1000 / (1 + age / 20)
                        } else {
                            3_800 * bump(d, 130) / 1000 * (TRACK_MYR - age) / TRACK_MYR
                        };
                    }
                }
                e
            })
            .collect();
        let sea = quantile(&sphere, &elevation, 1000 - LAND_PER_MILLE);
        for e in &mut elevation {
            *e -= sea;
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
            lands: Vec::new(),
            ours: vec![false; CELLS],
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

    /// Our continent written into the grid: cells inside the zoom take its
    /// height; other land within [`CLEAR_KM`] of it sinks, so it stands
    /// alone across the ocean (the lead's option A).
    pub fn shape(&mut self, zoom: &Zoom) {
        for c in 0..CELLS {
            let p = self.sphere.centre[c];
            let d = km_between(p, zoom.frame.centre);
            if d > CLEAR_KM {
                continue;
            }
            if let Some(i) = zoom.frame.pixel_of(p) {
                self.elevation[c] = zoom.elevation[i];
                self.ours[c] = zoom.ours[i] && zoom.elevation[i] > 0;
                self.continental[c] = zoom.ours[i];
            } else if self.elevation[c] > 0 || self.continental[c] {
                self.elevation[c] = -3_200;
                self.continental[c] = false;
            }
        }
        self.lands = self.landmasses();
    }
}

/// How far from our continent's middle other land is sunk, km.
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
    let mut counts = [0_u8; 32];
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
/// Its latitudes north, tenths of a degree (the lead's 40–50° N, north
/// the cold side since 26 November: its middle is
/// kept within 42.5–47.5° so its ends, 2.5° away, stay inside).
const SITE_LATITUDE: (i64, i64) = (425, 475);
/// How far the margin may turn from north–south: the cosine between the
/// trench's normal and the east, per mille (45°).
const SITE_FACING: i64 = 707;
/// Land a site may sink around it, km² (more, and it is a last resort).
const MOST_SUNK_KM2: i64 = 500_000;
/// How far west the ocean is looked at, km: the westerlies' fetch.
const FETCH_KM: i64 = 3_000;

/// Where our continent stands: a trench at 40–50° with the ocean to the
/// west, the plate across riding east of it.
#[derive(Debug, Clone)]
pub struct Site {
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
    /// Every candidate, best first.
    pub fn candidates(planet: &Planet) -> Vec<Self> {
        let s = &planet.sphere;
        let mut out = Vec::new();
        for c in 0..CELLS {
            let Some(e) = planet.edge[c] else { continue };
            if e.kind != Kind::Trench {
                continue;
            }
            let p = s.centre[c];
            let lat = latitude(p);
            if lat < SITE_LATITUDE.0 || lat > SITE_LATITUDE.1 {
                continue;
            }
            let east = east_at(p);
            let facing = dot(e.normal, east) * 1000 / ONE;
            if facing < SITE_FACING {
                continue;
            }
            let mut ocean = 0;
            let mut seen = 0;
            let mut x = p;
            for _ in 0..FETCH_KM / 100 {
                x = step(x, east_at(x).map(|v| -v), 100);
                let cell = s.cell_of(x);
                seen += 1;
                ocean += i64::from(planet.elevation[cell] < 0);
            }
            let ocean_west = ocean * 1000 / seen;
            let middle = step(p, east, 250);
            let mut land_near_km2 = 0;
            let mut old_near = (0, 0);
            let mut ends_near = 0;
            for k in 0..CELLS {
                let q = s.centre[k];
                if dot(q, middle) < cos(CLEAR_KM * ONE / RADIUS_KM) {
                    continue;
                }
                let d = km_between(q, middle);
                if d <= CLEAR_KM && planet.elevation[k] > 0 && d > 350 {
                    land_near_km2 += i64::from(s.area_km2[k]);
                }
                if d <= 300 {
                    match planet.old_edge[k] {
                        Some(OldKind::Suture) => old_near.0 += 1,
                        Some(OldKind::FailedRift) => old_near.1 += 1,
                        _ => {}
                    }
                }
                if d <= 500
                    && planet.edge[k].is_some_and(|b| {
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
            let score = ocean_west - land_near_km2 / 2_000
                + 120 * i64::from((old_near.0 + old_near.1).min(3))
                + 80 * i64::from(ends_near.min(2))
                + if hotspot_near { 60 } else { 0 }
                - 4 * (rate - 50).abs()
                - (lat - 450).abs()
                + facing / 4;
            out.push(Self {
                cell: c,
                sinking: planet.plate[c] as usize,
                riding: e.other as usize,
                origin: p,
                lat_lon: (lat, longitude(p)),
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
        // A site that would sink half a continent is a last resort.
        for x in &mut out {
            if x.land_near_km2 > MOST_SUNK_KM2 {
                x.score -= 5_000;
            }
        }
        out.sort_by_key(|x| (Reverse(x.score), x.cell));
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
    /// Its width across the chain, km.
    pub width_km: i64,
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

/// The middle of our continent from the trench, km (fixed for every
/// width, so the widths compare in the same frame).
const MIDDLE_FROM_TRENCH_KM: i64 = 260;
/// Trench to west coast, km (Cascadia: ~100; Chile: 60–130).
const COAST_KM: (i64, i64) = (55, 90);
/// Trench to volcanic arc, km: 100 km of depth to the sinking slab over
/// the slab's dip (25° → 215 km, 45° → 100 km); Cascadia ~250, Chile ~250.
const ARC_KM: (i64, i64) = (150, 230);
/// Between two volcanoes of the arc, km (Cascades: 40–100).
const VOLCANO_SPACING_KM: (i64, i64) = (40, 85);

impl Zoom {
    /// Our continent at a width, on the riding plate of `site`.
    pub fn new(planet: &Planet, site: &Site, width_km: i64) -> Self {
        let seed = hash(&[planet.seed, P_ZOOM]);
        let east0 = east_at(site.origin);
        let centre = step(site.origin, east0, MIDDLE_FROM_TRENCH_KM);
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
            let segment = n1(10, 120);
            let wobble_w = (n2(11, 45) * 90 + n2(12, 16) * 30 + n2(13, 7) * 12) / 1000;
            let wobble_e =
                (n2(23, 70) * 140 + n2(24, 28) * 90 + n2(25, 11) * 36 + n2(28, 4) * 12) / 1000;
            let inner = (n2(26, 50) * 60 + n2(27, 15) * 25) / 1000;
            let end_wobble = (n2(14, 60) * 160 + n2(29, 20) * 70 + n2(30, 7) * 24) / 1000;
            let reach = half_len + end_wobble - along.abs();
            let taper = (reach * 1000 / 140).clamp(0, 1000);
            let west = coast_km + segment * 60 / 1000;
            let east_coast =
                west + width_km * taper / 1000 * (1000 + n1(15, 150) * 300 / 1000) / 1000;
            let (dw, de, db) = (d + wobble_w, d + wobble_e, d + inner);
            let inside =
                d >= 0 && reach > 0 && dw >= west && de <= east_coast && east_coast > west + 8;
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
                let edge = (dw - west).min(east_coast - de);
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
                    if east_coast - de < 45 && base < 350 {
                        set = Setting::CoastalPlain;
                        base = base.min(4 + (east_coast - de) * 2 + rough / 12);
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
                ours[i] = true;
            } else if d > -1000 && reach > -200 {
                // Just off our coasts, the shelf falls to the deep.
                let off = if dw < west {
                    west - dw
                } else {
                    de - east_coast
                }
                .max(0);
                let off = if reach > 0 { off } else { off.max(-reach) };
                e = e.max(-80 - off * 70);
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
            width_km,
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
        let land: i64 = a.lands.iter().map(|l| l.area_km2).sum();
        assert!(
            (land * 1000 / 510_000_000 - LAND_PER_MILLE).abs() < 20,
            "{land}"
        );
    }

    #[test]
    fn lands_are_several_continents_and_the_cold_stays_open() {
        for seed in [5, 13] {
            let world = Planet::new(seed);
            let biggest = world.lands[0].area_km2;
            assert!(biggest <= 15_000_000, "seed {seed}: {biggest} km²");
            let large = world
                .lands
                .iter()
                .filter(|l| l.area_km2 > 1_000_000)
                .count();
            assert!(large >= 8, "seed {seed}: {large} lands over 1 M km²");
            let cold: i64 = world
                .lands
                .iter()
                .filter(|l| l.area_km2 > 1_000_000 && latitude(l.centre).abs() > 600)
                .map(|l| l.area_km2)
                .sum();
            assert_eq!(cold, 0, "seed {seed}");
        }
    }
}
