//! The planet's grid: a cube projected on the sphere, every cell the same
//! area; a cell's neighbours, the cell of a direction, distances spread
//! over the grid, and a quantile weighted by area.

use crate::*;

/// Cells along a cube face's side: 6 × 100² = 60,000 cells of ~100 km.
pub const FACE: usize = 100;

/// Cells of the planet's grid.
pub const CELLS: usize = 6 * FACE * FACE;

/// Each face's normal, then its two axes, as unit integers.
pub(crate) const AXES: [[[i64; 3]; 3]; 6] = [
    [[1, 0, 0], [0, 1, 0], [0, 0, 1]],
    [[-1, 0, 0], [0, -1, 0], [0, 0, 1]],
    [[0, 1, 0], [-1, 0, 0], [0, 0, 1]],
    [[0, -1, 0], [1, 0, 0], [0, 0, 1]],
    [[0, 0, 1], [0, 1, 0], [-1, 0, 0]],
    [[0, 0, -1], [0, 1, 0], [1, 0, 0]],
];

pub(crate) fn face_point(face: usize, s: i64, t: i64) -> V3 {
    let [n, r, u] = AXES[face];
    normalize([
        n[0] * ONE + r[0] * s + u[0] * t,
        n[1] * ONE + r[1] * s + u[1] * t,
        n[2] * ONE + r[2] * s + u[2] * t,
    ])
}

/// `tan(k · π / (4 · FACE))`, scaled by [`ONE`]: the cube's equal-angle
/// coordinates (a first guess of a direction's cell).
pub(crate) fn tan_step(k: i64) -> i64 {
    let a = k * PI / (4 * FACE as i64);
    sin(a) * ONE / cos(a)
}

/// The direction of the point `(u, v)` of a face's square (each from
/// −[`ONE`] to [`ONE`]), every part of the square covering the same area
/// of the sphere (Roşca and Plonka's equal-area cube, 2011): the square
/// is bent into the face's shape on Lambert's equal-area plane, then
/// laid on the sphere.
pub(crate) fn equal_area_point(face: usize, u: i64, v: i64) -> V3 {
    const ROOT2: i128 = 1_414_214;
    const ROOT4_2: i128 = 1_189_207;
    let one = i128::from(ONE);
    let (x, y) = if u == 0 && v == 0 {
        (0, 0)
    } else {
        let swap = v.abs() > u.abs();
        let (a, b) = if swap { (v, u) } else { (u, v) };
        let angle = (i128::from(b) * i128::from(PI / 12) / i128::from(a)) as i64;
        let (c, sn) = (i128::from(cos(angle)), i128::from(sin(angle)));
        let den = ((ROOT2 - c) * one).cast_unsigned().isqrt() as i128;
        let k = ROOT4_2 * i128::from(a) / den;
        let (p, q) = (
            k * (ROOT2 * c / one - one) / one,
            k * (ROOT2 * sn / one) / one,
        );
        if swap { (q, p) } else { (p, q) }
    };
    let t = (x * x + y * y) / one;
    let f = ((one - t / 4) * one).cast_unsigned().isqrt() as i128;
    let (lx, ly, lz) = (f * x / one, f * y / one, one - t / 2);
    let [n, r, w] = AXES[face];
    let at =
        |k: usize| (i128::from(n[k]) * lz + i128::from(r[k]) * lx + i128::from(w[k]) * ly) as i64;
    normalize([at(0), at(1), at(2)])
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
    /// For each cell of the equal-angle cube, a cell near it.
    guess: Vec<u32>,
}

impl Default for Sphere {
    fn default() -> Self {
        Self::new()
    }
}

impl Sphere {
    /// The grid (the same for every planet): six faces of [`FACE`]² cells
    /// of the same area (version 9: a continent drifting over cells of
    /// unequal size was redrawn with more or fewer cells), a direction
    /// belonging to the cell whose centre is nearest.
    pub fn new() -> Self {
        let n = FACE as i64;
        let edges: Vec<i64> = (0..=n).map(|i| tan_step(2 * i - n)).collect();
        let mut centre = Vec::with_capacity(CELLS);
        for f in 0..6 {
            for j in 0..n {
                for i in 0..n {
                    centre.push(equal_area_point(
                        f,
                        (2 * i + 1 - n) * ONE / n,
                        (2 * j + 1 - n) * ONE / n,
                    ));
                }
            }
        }
        let cell_km2 = (4 * i128::from(PI) * i128::from(RADIUS_KM * RADIUS_KM)
            / i128::from(ONE)
            / CELLS as i128) as u32;
        // Neighbours: inside a face by rows and columns; across an edge
        // of the cube, the nearest cells of the other faces' rims.
        let rim: Vec<usize> = (0..CELLS)
            .filter(|&c| {
                let (i, j) = (c % FACE, c / FACE % FACE);
                i == 0 || j == 0 || i == FACE - 1 || j == FACE - 1
            })
            .collect();
        let mut links: Vec<Vec<(u32, u32)>> = Vec::with_capacity(CELLS);
        for me in 0..CELLS {
            let (f, i, j) = (
                me / (FACE * FACE),
                (me % FACE) as i64,
                (me / FACE % FACE) as i64,
            );
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
                let (a, b) = (i + di, j + dj);
                if (0..n).contains(&a) && (0..n).contains(&b) {
                    let other = f * FACE * FACE + b as usize * FACE + a as usize;
                    mine.push((other as u32, km_between(centre[me], centre[other]) as u32));
                }
            }
            if mine.len() < 8 {
                // Within about 300 km (the cosine of the angle).
                let mut across: Vec<(i64, usize)> = rim
                    .iter()
                    .filter(|&&o| o / (FACE * FACE) != f && dot(centre[me], centre[o]) > 998_890)
                    .map(|&o| (km_between(centre[me], centre[o]), o))
                    .collect();
                across.sort_unstable();
                let reach = across.first().map_or(0, |a| a.0 * 17 / 10);
                for &(km, o) in across.iter().take(8 - mine.len()) {
                    if km <= reach {
                        mine.push((o as u32, km as u32));
                    }
                }
            }
            links.push(mine);
        }
        // A neighbour is a neighbour both ways.
        for me in 0..CELLS {
            for k in 0..links[me].len() {
                let (o, km) = links[me][k];
                if !links[o as usize].iter().any(|&(b, _)| b as usize == me) {
                    links[o as usize].push((me as u32, km));
                }
            }
        }
        let mut sphere = Self {
            centre,
            area_km2: vec![cell_km2; CELLS],
            links,
            edges,
            guess: (0..CELLS as u32).collect(),
        };
        let centres: Vec<i64> = (0..n).map(|i| tan_step(2 * i + 1 - n)).collect();
        sphere.guess = (0..CELLS)
            .map(|c| {
                let at = face_point(
                    c / (FACE * FACE),
                    centres[c % FACE],
                    centres[c / FACE % FACE],
                );
                sphere.nearest(c, at) as u32
            })
            .collect();
        sphere
    }

    /// The cell of the equal-angle cube holding a direction.
    fn tan_cell(&self, v: V3) -> usize {
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

    /// The cell whose centre is nearest to a direction, walking from
    /// `from` (ties go to the lower cell).
    fn nearest(&self, from: usize, v: V3) -> usize {
        let mut c = from;
        let mut best = dot(self.centre[c], v);
        loop {
            let mut next = c;
            for &(m, _) in &self.links[c] {
                let d = dot(self.centre[m as usize], v);
                if d > best || (d == best && (m as usize) < next) {
                    best = d;
                    next = m as usize;
                }
            }
            if next == c {
                return c;
            }
            c = next;
        }
    }

    /// The cell holding a direction.
    pub fn cell_of(&self, v: V3) -> usize {
        self.nearest(self.guess[self.tan_cell(v)] as usize, v)
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

/// The value below which `share` per mille of the surface lies (by area).
pub(crate) fn quantile(sphere: &Sphere, values: &[i64], share: i64) -> i64 {
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
