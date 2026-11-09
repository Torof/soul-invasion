//! The deterministic building blocks of world generation, in one place:
//! the hash every random choice is drawn from, the noises built on it,
//! and the older stream generator and lattice noise the valley still
//! uses.
//!
//! A world must be the same on every machine and every build (D24), so
//! everything here is integer arithmetic with wrapping operations: no
//! float, no platform-dependent rounding, no dependency. The golden-value
//! tests at the bottom pin each helper's outputs; a change that moves one
//! of them changes every world, and must be made on purpose.
//!
//! The module is named `gen` but declared as `r#gen`: `gen` is a keyword
//! reserved by the 2024 edition.
//!
//! - [`hash`]: a splitmix64-style mix of a few numbers. A random choice
//!   is `hash(seed, purpose, coordinates or ids)`, so it depends on
//!   nothing but its own inputs.
//! - [`unit`] and [`noise`]: a number in 0..1000 from a hash, and smooth
//!   value noise worked out at a point from the lattice around it,
//!   nothing stored, for any size of map.
//! - [`Lcg`] and [`Groves`]: a linear congruential stream and a lattice
//!   noise drawn from it. Kept as they are so that today's worlds do not
//!   change; a lattice drawn from one stream depends on the map's size,
//!   so these are to give way to [`hash`] and [`noise`] when the generator
//!   is rewritten chunk by chunk.

// No float in generation: see `clippy.toml` and the generator modules.
#![warn(clippy::float_arithmetic)]

/// The odd constant of splitmix64 (2^64 divided by the golden ratio).
const GOLDEN: u64 = 0x9e37_79b9_7f4a_7c15;

/// A hash of a few numbers, well mixed (splitmix64 finaliser, chained).
///
/// The same numbers always give the same hash, on every machine. Order
/// matters: `hash(&[a, b])` and `hash(&[b, a])` differ. Give each use its
/// own constant "purpose" among the numbers, so that two layers drawing
/// at the same place do not draw the same thing.
pub(crate) fn hash(values: &[u64]) -> u64 {
    let mut h: u64 = GOLDEN;
    for v in values {
        h ^= v
            .wrapping_add(GOLDEN)
            .wrapping_add(h << 6)
            .wrapping_add(h >> 2);
        h = h.wrapping_add(GOLDEN);
        let mut z = h;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        h = z ^ (z >> 31);
    }
    h
}

/// A number in 0..1000 for some numbers, fixed: [`hash`] modulo 1000.
pub(crate) fn unit(values: &[u64]) -> u32 {
    u32::try_from(hash(values) % 1000).expect("below 1000")
}

/// Smooth value noise in 0..1000 on a lattice of `cell` tiles, worked out
/// at a point from the lattice around it: nothing stored, any size of map.
///
/// Each lattice point `(i, j)` is `unit(seed, i, j)`; between points the
/// value is interpolated with an integer smoothstep, so broad patches come
/// out round. Negative coordinates are fine (the lattice is floored).
pub(crate) fn noise(seed: u64, cell: u32, x: i32, y: i32) -> u32 {
    let c = i64::from(cell);
    let (x, y) = (i64::from(x), i64::from(y));
    let (cx, cy) = (x.div_euclid(c), y.div_euclid(c));
    let ease = |f: i64| f * f * (3 * c - 2 * f) / (c * c);
    let (fx, fy) = (ease(x.rem_euclid(c)), ease(y.rem_euclid(c)));
    #[allow(clippy::cast_sign_loss)]
    let at = |i: i64, j: i64| i64::from(unit(&[seed, i as u64, j as u64]));
    let top = at(cx, cy) * (c - fx) + at(cx + 1, cy) * fx;
    let bottom = at(cx, cy + 1) * (c - fx) + at(cx + 1, cy + 1) * fx;
    u32::try_from((top * (c - fy) + bottom * fy) / (c * c)).expect("within 0..1000")
}

/// A tiny deterministic generator (a 64-bit linear congruential stream,
/// Knuth's MMIX constants), so world generation needs no dependency and
/// gives the same valley on every machine.
///
/// A stream's draws depend on every draw before them: a feature that
/// draws one more number moves every feature drawn after it. New code
/// draws from [`hash`] instead.
pub(crate) struct Lcg(u64);

impl Lcg {
    /// A stream started from `seed`.
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1))
    }

    /// The next draw, in `0..bound` (`bound` must not be zero).
    pub(crate) fn next_below(&mut self, bound: u32) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        u32::try_from((self.0 >> 33) % u64::from(bound)).expect("below bound")
    }
}

/// Smooth value noise on a coarse lattice, for what comes in patches
/// rather than salt and pepper (the woods, the wet ground).
///
/// The lattice is drawn from one [`Lcg`] stream, row by row, so its
/// values depend on the map's width: the same seed gives other patches on
/// a wider map. Kept for today's worlds; [`noise`] does not have this flaw.
pub(crate) struct Groves {
    cell: u32,
    columns: u32,
    values: Vec<u32>,
}

impl Groves {
    /// Smooth noise on a lattice of `cell` tiles, for things that come in
    /// patches: the woods' massifs, where the ground is wet.
    pub(crate) fn with_cell(width: u32, height: u32, seed: u64, cell: u32) -> Self {
        let columns = width / cell + 2;
        let rows = height / cell + 2;
        let mut rng = Lcg::new(seed);
        let values = (0..columns * rows).map(|_| rng.next_below(1000)).collect();
        Self {
            cell,
            columns,
            values,
        }
    }

    /// The noise at a tile, in 0..1000, eased between lattice points
    /// (smoothstep) so that broad patches come out round, not diamond.
    pub(crate) fn smooth(&self, x: u32, y: u32) -> u32 {
        let c = u64::from(self.cell);
        let ease = |f: u32| {
            let f = u64::from(f);
            u32::try_from(f * f * (3 * c - 2 * f) / (c * c)).expect("within a cell")
        };
        let (cx, fx) = (x / self.cell, ease(x % self.cell));
        let (cy, fy) = (y / self.cell, ease(y % self.cell));
        let at = |i: u32, j: u32| self.values[(j * self.columns + i) as usize];
        let top = at(cx, cy) * (self.cell - fx) + at(cx + 1, cy) * fx;
        let bottom = at(cx, cy + 1) * (self.cell - fx) + at(cx + 1, cy + 1) * fx;
        (top * (self.cell - fy) + bottom * fy) / (self.cell * self.cell)
    }

    /// The noise at a tile, in 0..1000.
    pub(crate) fn value(&self, x: u32, y: u32) -> u32 {
        let (cx, fx) = (x / self.cell, x % self.cell);
        let (cy, fy) = (y / self.cell, y % self.cell);
        let at = |i: u32, j: u32| self.values[(j * self.columns + i) as usize];
        // Bilinear interpolation, in thousandths.
        let top = at(cx, cy) * (self.cell - fx) + at(cx + 1, cy) * fx;
        let bottom = at(cx, cy + 1) * (self.cell - fx) + at(cx + 1, cy + 1) * fx;
        (top * (self.cell - fy) + bottom * fy) / (self.cell * self.cell)
    }
}

#[cfg(test)]
mod tests {
    //! Golden values: fixed inputs, fixed outputs, taken from the helpers
    //! as they were before they moved here. If one of these fails, every
    //! world has changed: put the helper back, or change the world on
    //! purpose and say so.

    use super::{Groves, Lcg, hash, noise, unit};

    #[test]
    fn hash_is_pinned() {
        assert_eq!(hash(&[]), 0x9e37_79b9_7f4a_7c15);
        assert_eq!(hash(&[0]), 0x3a13_eaf2_0263_2586);
        assert_eq!(hash(&[1]), 0xf6fe_0721_c560_f476);
        assert_eq!(hash(&[1, 2, 3]), 0xf51e_fbb9_4395_9f15);
        assert_eq!(hash(&[u64::MAX, 0x9a7c, 7]), 0x8abb_c423_61c4_0023);
    }

    #[test]
    fn hash_depends_on_order() {
        assert_ne!(hash(&[1, 2]), hash(&[2, 1]));
        assert_ne!(hash(&[0]), hash(&[0, 0]));
    }

    #[test]
    fn unit_is_pinned() {
        assert_eq!(unit(&[1, 2, 3]), 597);
        assert_eq!(unit(&[42]), 854);
    }

    #[test]
    fn noise_is_pinned() {
        let cases = [
            ((0, 0), 730),
            ((7, 3), 834),
            ((-5, 40), 730),
            ((123_456, 7), 336),
            ((-1, -1), 676),
        ];
        for ((x, y), want) in cases {
            assert_eq!(noise(9, 14, x, y), want, "noise at ({x}, {y})");
        }
        assert_eq!(noise(0x77e7, 32, 100, 200), 80);
    }

    #[test]
    fn noise_is_smooth_and_fixed() {
        for (x, y) in [(0, 0), (-5, 40), (123_456, 7)] {
            let n = noise(9, 14, x, y);
            assert!(n < 1000);
            assert_eq!(n, noise(9, 14, x, y));
            assert!(
                n.abs_diff(noise(9, 14, x + 1, y)) < 200,
                "neighbours are near"
            );
        }
    }

    #[test]
    fn lcg_is_pinned() {
        let mut rng = Lcg::new(1);
        let draws: Vec<u32> = (0..8).map(|_| rng.next_below(1000)).collect();
        assert_eq!(draws, [503, 413, 830, 779, 938, 741, 58, 727]);
        let mut rng = Lcg::new(0x5eed_17e5);
        let draws: Vec<u32> = (0..4).map(|_| rng.next_below(u32::MAX)).collect();
        assert_eq!(draws, [361_749_837, 158_216_102, 996_662_187, 891_410_595]);
    }

    #[test]
    fn groves_are_pinned() {
        let groves = Groves::with_cell(100, 60, 7, 16);
        let cases = [
            ((0, 0), (933, 933)),
            ((5, 9), (810, 871)),
            ((16, 16), (87, 87)),
            ((31, 47), (567, 567)),
            ((99, 59), (447, 518)),
        ];
        for ((x, y), (value, smooth)) in cases {
            assert_eq!(groves.value(x, y), value, "value at ({x}, {y})");
            assert_eq!(groves.smooth(x, y), smooth, "smooth at ({x}, {y})");
        }
    }
}
