//! Placing things on the ground, one scheme for all (D39, the lightness
//! audit's placement report): every thing is decided by a cell or a tile
//! in **absolute coordinates**, from a hash of the seed, a purpose and
//! that cell. Nothing is drawn from a stream, nothing looks at what was
//! placed before, nothing needs the whole map: any chunk, built alone and
//! in any order, places the same things, and the cost follows the chunks
//! asked for, never the map's size.
//!
//! Three building blocks:
//!
//! - a **hashed chance per tile** ([`roll`], [`chance`]): salt and pepper,
//!   the ground's natural items;
//! - a **jittered grid** ([`Grid`]): one thing at most per cell, nudged
//!   off the cell's centre; things keep a spacing without any check
//!   between them (the trees);
//! - a **nested cell** ([`Grid::parent`]): a coarse cell that groups the
//!   fine ones, to decide something for all of them at once (an opening
//!   in the woods; later a clump, a scree).
//!
//! Every random choice names its **purpose**, a constant of [`purpose`]:
//! two layers drawing at the same place never draw the same thing, and a
//! new layer never moves an old one.
//!
//! Integers only (D24).

use super::hash;
use crate::map::FinePos;

/// The purposes of world generation's random choices: one constant per
/// kind of choice, all different (`purposes_are_all_different`). Some are
/// older salts, mixed into the seed (`seed ^ salt`) rather than hashed
/// after it; they are listed so that no new purpose takes one, and keep
/// their value so that the worlds they make do not change.
pub(crate) mod purpose {
    /// The trees' grid: whether a cell holds a tree, where, which kind.
    pub(crate) const TREE_CELL: u64 = 0x7ee_ce11;
    /// The trees' openings: a nested cell left empty.
    pub(crate) const TREE_GAP: u64 = 0x7ee_6a95;
    /// Lone and riverside trees, one on their tile.
    pub(crate) const TREE_TILE: u64 = 0x7ee_711e;
    /// A kind of tree's stands (hashed with the kind's index).
    pub(crate) const TREE_STAND: u64 = 0x7ee_57a4;
    /// The big trees' wide grid: whether a cell holds one, where, which.
    pub(crate) const TREE_BIG: u64 = 0x7ee_b196;
    /// The mother trees: where they stand, and their kind.
    pub(crate) const TREE_MOTHER: u64 = 0x7ee_3077;
    /// The trees of the meadows, on their own grid.
    pub(crate) const TREE_MEADOW: u64 = 0x7ee_3ead;
    /// The ground's natural items (an older salt).
    pub(crate) const NATURAL_ITEM: u64 = 0x5eed_17e5;
    /// The flora's patches and the plants in them (an older salt).
    pub(crate) const FLORA_PATCH: u64 = 0x9a7c;
    /// The mushrooms on their host's trunk (an older salt).
    pub(crate) const FLORA_TRUNK: u64 = 0xf_107a;
    /// A kind of flora's stands (an older salt, hashed with the kind).
    pub(crate) const FLORA_STAND: u64 = 0x5_7a4d;
    /// The ground's breath, for the flora (an older salt).
    pub(crate) const FLORA_BREATH: u64 = 0xb4ea;
    /// The valley's background wetness (an older salt).
    pub(crate) const WETNESS: u64 = 0x77e7;
    /// The woods' massifs, their detail, their fine grain, their ragged
    /// rims, and the clearings and copses (older salts).
    pub(crate) const WOODS_MASSIF: u64 = 0x00f0_4e57;
    pub(crate) const WOODS_DETAIL: u64 = 0x00de_7a11;
    pub(crate) const WOODS_FINE: u64 = 0x0000_f17e;
    pub(crate) const WOODS_RIM: u64 = 0x001a_99ed;
    pub(crate) const WOODS_PATCHES: u64 = 0x0a4_7eee;
    /// The rivers' stream (an older salt, added to the seed).
    pub(crate) const RIVERS: u64 = 0x5157;

    /// Every purpose, to check that they differ.
    #[cfg(test)]
    pub(crate) const ALL: [u64; 19] = [
        TREE_CELL,
        TREE_GAP,
        TREE_TILE,
        TREE_STAND,
        TREE_BIG,
        TREE_MOTHER,
        TREE_MEADOW,
        NATURAL_ITEM,
        FLORA_PATCH,
        FLORA_TRUNK,
        FLORA_STAND,
        FLORA_BREATH,
        WETNESS,
        WOODS_MASSIF,
        WOODS_DETAIL,
        WOODS_FINE,
        WOODS_RIM,
        WOODS_PATCHES,
        RIVERS,
    ];
}

/// Most keys a roll takes after the seed and the purpose.
const MOST_KEYS: usize = 6;

/// A random number fixed by the seed, a purpose and a few keys (a cell, a
/// tile, an index): `hash(seed, purpose, keys...)`.
pub(crate) fn roll(seed: u64, purpose: u64, keys: &[u64]) -> u64 {
    assert!(
        keys.len() <= MOST_KEYS,
        "a roll takes at most {MOST_KEYS} keys"
    );
    let mut values = [0_u64; MOST_KEYS + 2];
    values[0] = seed;
    values[1] = purpose;
    values[2..2 + keys.len()].copy_from_slice(keys);
    hash(&values[..2 + keys.len()])
}

/// A chance of `per_mille` thousandths, fixed by the seed, a purpose and
/// a few keys: the same answer every time it is asked.
pub(crate) fn chance(per_mille: u32, seed: u64, purpose: u64, keys: &[u64]) -> bool {
    roll(seed, purpose, keys) % 1000 < u64::from(per_mille)
}

/// A number in `-range..=range`, from a roll.
pub(crate) fn offset(roll: u64, range: i32) -> i32 {
    let span = u64::from(range.unsigned_abs()) * 2 + 1;
    i32::try_from(roll % span).expect("within the range") - range
}

/// The index drawn among `weights` by `roll`, each with a chance in
/// proportion to its weight; `None` when every weight is zero.
pub(crate) fn weighted(weights: &[u64], roll: u64) -> Option<usize> {
    let total: u64 = weights.iter().sum();
    if total == 0 {
        return None;
    }
    let mut left = roll % total;
    weights.iter().position(|w| {
        if left < *w {
            true
        } else {
            left -= w;
            false
        }
    })
}

/// A coordinate as a hash key (two's complement: negative cells are keys
/// like any other).
#[allow(clippy::cast_sign_loss)]
pub(crate) const fn key(v: i64) -> u64 {
    v as u64
}

/// A jittered grid on the ground, in millimetres from the grid's origin:
/// cells of `cell` a side, each holding one point at most, nudged up to
/// `jitter` off its cell's centre on each axis. With `offset_rows`, every
/// other row is shifted by half a cell, like the bricks of a wall: the
/// points pack more evenly and show no straight rows along the
/// diagonals, which the isometric view turns into the screen's lines.
/// Two points stand at least `cell - 2 × jitter` apart.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Grid {
    /// Side of a cell, in millimetres.
    pub(crate) cell: i32,
    /// Farthest a point stands from its cell's centre, on each axis, in
    /// millimetres: under half a cell.
    pub(crate) jitter: i32,
    /// Odd rows shifted by half a cell.
    pub(crate) offset_rows: bool,
}

impl Grid {
    /// The point of cell `(cx, cy)` for `seed` and `purpose`.
    pub(crate) fn point(&self, seed: u64, purpose: u64, cx: i64, cy: i64) -> FinePos {
        debug_assert!(self.jitter * 2 < self.cell, "jitter under half a cell");
        let c = i64::from(self.cell);
        let shift = if self.offset_rows && cy.rem_euclid(2) == 1 {
            c / 2
        } else {
            0
        };
        let nudge = |axis: u64| {
            i64::from(offset(
                roll(seed, purpose, &[key(cx), key(cy), axis]),
                self.jitter,
            ))
        };
        let x = cx * c + c / 2 + shift + nudge(0);
        let y = cy * c + c / 2 + nudge(1);
        FinePos::new(
            i32::try_from(x).expect("on the grid of fine positions"),
            i32::try_from(y).expect("on the grid of fine positions"),
        )
    }

    /// The cells whose point may fall in the rectangle from `(x0, y0)`
    /// included to `(x1, y1)` excluded, in millimetres, row by row: a few
    /// more than those whose point does, which the caller keeps by where
    /// the point lands.
    pub(crate) fn cells_over(
        &self,
        (x0, y0): (i32, i32),
        (x1, y1): (i32, i32),
    ) -> impl Iterator<Item = (i64, i64)> {
        let c = i64::from(self.cell);
        // A point lies within its cell, shifted by at most half a cell
        // more: one cell of margin on each side covers it.
        let (cx0, cx1) = (
            i64::from(x0).div_euclid(c) - 1,
            i64::from(x1).div_euclid(c) + 1,
        );
        let (cy0, cy1) = (
            i64::from(y0).div_euclid(c) - 1,
            i64::from(y1).div_euclid(c) + 1,
        );
        (cy0..=cy1).flat_map(move |cy| (cx0..=cx1).map(move |cx| (cx, cy)))
    }

    /// The nested cell of `factor` × `factor` cells that cell `(cx, cy)`
    /// belongs to: what is decided for it holds for all of them.
    pub(crate) fn parent(cx: i64, cy: i64, factor: i64) -> (i64, i64) {
        (cx.div_euclid(factor), cy.div_euclid(factor))
    }
}

#[cfg(test)]
mod tests {
    //! Golden values: a change that moves one of them moves every tree.

    use super::{Grid, chance, offset, purpose, roll, weighted};

    #[test]
    fn purposes_are_all_different() {
        let mut all = purpose::ALL.to_vec();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), purpose::ALL.len());
    }

    #[test]
    fn rolls_are_pinned() {
        assert_eq!(roll(1, purpose::TREE_CELL, &[]), 0x0d42_15e4_84e6_a944);
        assert_eq!(
            roll(1, purpose::TREE_CELL, &[3, 4, 0]),
            0xe444_9316_9c82_5893
        );
        assert_eq!(
            roll(7, purpose::TREE_TILE, &[u64::MAX]),
            0x2204_87bb_2d6c_d595
        );
        // The same as the hash of the whole list.
        assert_eq!(roll(9, 2, &[5, 6]), crate::r#gen::hash(&[9, 2, 5, 6]));
    }

    #[test]
    fn chances_hold_their_share() {
        let hits = (0..10_000_u64)
            .filter(|i| chance(300, 1, purpose::TREE_GAP, &[*i]))
            .count();
        assert!((2_800..3_200).contains(&hits), "{hits}");
        assert!(!chance(0, 1, purpose::TREE_GAP, &[1]));
        assert!(chance(1000, 1, purpose::TREE_GAP, &[1]));
    }

    #[test]
    fn offsets_stay_in_their_range() {
        for r in 0..2_000_u64 {
            let o = offset(roll(1, 2, &[r]), 7);
            assert!((-7..=7).contains(&o));
        }
        assert_eq!(offset(12_345, 0), 0);
    }

    #[test]
    fn weighted_draws_follow_the_weights() {
        assert_eq!(weighted(&[0, 0], 5), None);
        assert_eq!(weighted(&[0, 3, 0], 7), Some(1));
        let mut count = [0; 3];
        for r in 0..9_000_u64 {
            count[weighted(&[1, 2, 6], roll(3, 4, &[r])).expect("some weight")] += 1;
        }
        assert!((800..1_200).contains(&count[0]), "{count:?}");
        assert!((1_700..2_300).contains(&count[1]), "{count:?}");
        assert!((5_600..6_400).contains(&count[2]), "{count:?}");
    }

    #[test]
    fn grid_points_keep_their_spacing_and_their_rows_shift() {
        let grid = Grid {
            cell: 4000,
            jitter: 1400,
            offset_rows: true,
        };
        let mut points = Vec::new();
        for cy in -6..6 {
            for cx in -6..6 {
                let p = grid.point(1, purpose::TREE_CELL, cx, cy);
                // Within its cell, shifted half a cell on odd rows.
                let shift = if cy.rem_euclid(2) == 1 { 2000 } else { 0 };
                let centre = (cx * 4000 + 2000 + shift, cy * 4000 + 2000);
                assert!((i64::from(p.x) - centre.0).abs() <= 1400, "{p:?}");
                assert!((i64::from(p.y) - centre.1).abs() <= 1400, "{p:?}");
                points.push(p);
            }
        }
        for (i, a) in points.iter().enumerate() {
            for b in &points[i + 1..] {
                let (dx, dy) = (i64::from(a.x - b.x), i64::from(a.y - b.y));
                assert!(dx * dx + dy * dy >= 1200 * 1200, "{a:?} {b:?}");
            }
        }
        assert_eq!(
            grid.point(1, purpose::TREE_CELL, 3, -2),
            crate::map::FinePos::new(14_770, -5_258)
        );
    }

    #[test]
    fn the_cells_over_a_rectangle_hold_every_point_in_it() {
        let grid = Grid {
            cell: 4000,
            jitter: 1400,
            offset_rows: true,
        };
        let (lo, hi) = ((32_000, -32_000), (64_000, 0));
        let listed: Vec<_> = grid.cells_over(lo, hi).collect();
        for cy in -20..20 {
            for cx in -5..25 {
                let p = grid.point(5, purpose::TREE_CELL, cx, cy);
                if (lo.0..hi.0).contains(&p.x) && (lo.1..hi.1).contains(&p.y) {
                    assert!(listed.contains(&(cx, cy)), "{cx}, {cy}");
                }
            }
        }
    }

    #[test]
    fn nested_cells_group_their_cells() {
        assert_eq!(Grid::parent(0, 0, 3), (0, 0));
        assert_eq!(Grid::parent(2, 5, 3), (0, 1));
        assert_eq!(Grid::parent(-1, -3, 3), (-1, -1));
    }
}
