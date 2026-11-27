//! Today's plates: the plates of the history's last step turning as they
//! last turned, the older plates under the supercontinent, and the kind of
//! each boundary (convergent, divergent, transform).

use crate::*;

/// The largest weight a plate's seed point gets, in [`ONE`] units, drawn
/// squared so a few plates are large and many small.
pub(crate) const PLATE_WEIGHT: i64 = 280_000;
/// Plate speeds at 90° from their pole, mm a year (the Earth's: 10–100).
pub(crate) const SPEED_MM: (i64, i64) = (25, 95);
/// The plates' borders' warp, `(cell, amplitude)` in [`ONE`] units.
pub(crate) const PLATE_WARP: [(i64, i64); 4] = [
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
    /// The warp of its borders (a Voronoi of seed points only).
    warp: &'static [(i64, i64)],
    /// Plates read from a grid (today's, the history's last step) rather
    /// than from a Voronoi of seed points.
    grid: Option<Box<Grid>>,
}

/// Each cell's plate on the grid.
#[derive(Debug, Clone)]
pub(crate) struct Grid {
    sphere: Sphere,
    plate: Vec<u8>,
}

/// The warp of a grid's plate borders, `(cell, amplitude)` in [`ONE`]
/// units: a few tens of km, so a border is no staircase of cells.
pub(crate) const GRID_WARP: [(i64, i64); 2] = [(80_000, 10_000), (25_000, 3_000)];
/// A grid cell votes for its plate within this, km.
pub(crate) const GRID_VOTE_KM: i64 = 150;

impl Layout {
    pub(crate) fn draw(
        seed: u64,
        purpose: u64,
        (lo, hi): (i64, i64),
        warp: &'static [(i64, i64)],
    ) -> Self {
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
            warp,
            grid: None,
        }
    }

    /// The plates at a step of the history (today's: the last), each
    /// turning as it turned then. Ids are renumbered from 0, largest plate first; the second
    /// value is each cell's plate.
    pub(crate) fn of_history(
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
            warp: &PLATE_WARP,
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
        let q = warp(self.warp_seed, p, self.warp);
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
pub(crate) const LEAST_MOTION_MM: i64 = 8;

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
