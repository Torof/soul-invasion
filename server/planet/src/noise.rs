//! Seeded draws and noise on the sphere: value noise, its octaves, a warp
//! of a point, a random direction.

use crate::*;

pub(crate) fn lattice3(seed: u64, i: i64, j: i64, k: i64) -> i128 {
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
pub(crate) fn warp(seed: u64, p: V3, octaves: &[(i64, i64)]) -> V3 {
    let mut q = p;
    for (o, &(cell, amp)) in octaves.iter().enumerate() {
        for a in 0..3 {
            q[a] += (noise3(hash(&[seed, o as u64, a as u64]), p, cell) - 500) * amp / 500;
        }
    }
    normalize(q)
}

pub(crate) fn draw(seed: u64, purpose: u64, k: u64, lo: i64, hi: i64) -> i64 {
    let span = (hi - lo + 1).max(1) as u64;
    lo + (hash(&[seed, purpose, k]) % span) as i64
}

pub(crate) fn random_unit(seed: u64, purpose: u64, k: u64) -> V3 {
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
