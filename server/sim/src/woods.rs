//! Where the trees grow (D37): a density of trees per tile and the kind of
//! place each tile is, generated with the valley and never stored.
//!
//! A temperate valley untouched by man (10 November 2026): forest covers
//! a little over half the land. Each tile's woods depend only on the
//! seed and what lies within a bounded distance of it: no level chosen
//! for the whole map, no stream drawn across it, so a chunk is made
//! alone (D24): the map makes its woods per chunk (`land`), drafted on
//! the chunk (`Woods::draft`) then finished with the forest's depth.
//!
//! - **Massifs**: a very broad noise (`MASSIF_CELL`), roughened by two
//!   finer ones, thickened a little where the ground is wet and in a band
//!   at the cliff's foot, cut at a fixed `LEVEL`.
//! - **Woods along the waters** (11 November 2026): water draws trees.
//!   The woods' pull is strongest at the water, held for 20 tiles from a
//!   river and fading to nothing at 60, where the land is what it would
//!   be on its own (a brook: held to 6, gone by 12); no line of its own,
//!   the bank has the woods of its region. The bank tile is free
//!   (`trees` stands nothing on a tile touching water), and the water is
//!   no edge for the depth rule below: the wood by a river is as deep as
//!   the woods around it. The first tiles of wood by the water (2 to 5 off a river, 2 to
//!   3 off a brook) are the **riverside** habitat, the riverside kinds'
//!   (willow, alder). A smooth noise on cells of a few hundred tiles
//!   (`RIPARIAN_CELL`) makes one stretch thick and the next thin, and
//!   now and then, rarely, a **bare stretch**: the pull falls there
//!   instead, meadow to the water's edge.
//! - **Dense and light regions** (the project lead, 11 November 2026):
//!   a smooth noise on cells of `DENSITY_CELL` tiles sets how close the
//!   trees stand, region by region (`Woods::stocking`): as today's heart
//!   in a dense region, 2 to 2.5 times farther apart in a light one.
//!   The density below, the habitat and the forest's ground are the
//!   same in both; only the trees' number changes, where they are placed
//!   (`trees`).
//! - **Edge, wood, heart**: past a narrow edge, the trees thicken with
//!   the depth in the forest: sparse over the first `SPARSE_DEPTH` tiles,
//!   full at `FULL_DEPTH`; the heart is the densest.
//! - **Large clearings** open in the woods, and a **camp clearing** around
//!   the place the first bodies start.
//! - **Small woods** (50 to 200 m across) rise in the open between the
//!   massifs; **copses**, tight groups of a few trees, dot the meadows.
//!
//! Round patches (clearings, small woods, copses) are decided cell by
//! cell on a grid in absolute coordinates, each from a hash of the seed
//! and its cell (`gen::scatter`): the same anywhere, in any order.
//!
//! A tile is forest ([`crate::map::Terrain::Forest`]) where the density
//! reaches `FOREST`: the wood and the heart. The edge, copses, riverside and brookside trees
//! stand on grass. Which species grows where comes with the trees
//! themselves (`trees`).
//!
//! All arithmetic is in integers (thousandths), so a seed gives the same
//! woods everywhere.

// Generation is integer-only (D24; linted from `lib.rs` and
// `sim/clippy.toml`). No known exception.

use std::sync::{Mutex, PoisonError};

use crate::r#gen::scatter::{self, key, purpose};
use crate::r#gen::{Groves, noise};
use crate::lru::Lru;
#[cfg(test)]
use crate::map::{Rivers, Terrain};

/// The kind of place a tile is, for the trees that will grow on it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Habitat {
    /// Open grass.
    Meadow,
    /// The rim of a massif, trees thinning out.
    Edge,
    /// Inside a massif.
    Wood,
    /// The densest heart of a massif.
    Heart,
    /// An opening in the woods.
    Clearing,
    /// The opening around the place the first bodies start.
    Camp,
    /// A small group of trees in the meadow.
    Copse,
    /// A tree alone in the meadow.
    Lone,
    /// The wood on the first tiles by a river or a brook: the riverside
    /// kinds' (willow, alder).
    Riverside,
    /// The wood in the band at the cliff's foot.
    CliffFoot,
    /// Water, a ford, a brook or the cliff: no tree.
    None,
}

/// What the woods are drawn from, for one map: the noises' seeds, the
/// wetness, the cliff and the camp. The woods themselves are worked out
/// a rectangle at a time (`Woods::draft`, then `Woods::finish` with
/// the forest's depth): the map makes them per chunk and never holds
/// them for the whole map (D37).
#[derive(Debug, Clone)]
pub struct Woods {
    seed: u64,
    massif: u64,
    detail: u64,
    fine: u64,
    rim: u64,
    riparian: u64,
    /// The same wetness the brooks rise from.
    wet: Groves,
    width: u32,
    height: u32,
    cliff_rows: u32,
    /// The camp's place, in thousandths of a tile.
    camp: (i64, i64),
    /// The round patches decided lately.
    memo: PatchMemo,
}

/// The round patches decided lately, by kind and cell, `None` for a cell
/// holding none: each is decided by its cell alone, once for the many
/// chunks it may reach rather than once a chunk. Bounded
/// (`PATCH_MEMO`), the least recently used out first.
#[derive(Debug)]
struct PatchMemo(Mutex<Lru<(u64, u32, u32), Option<Patch>>>);

/// Round patches' cells remembered at most: the small woods', clearings'
/// and copses' cells within reach of the far view's 2 km (about 3,300
/// copse cells), some 600 KB.
const PATCH_MEMO: usize = 4096;

impl PatchMemo {
    fn new() -> Self {
        Self(Mutex::new(Lru::new(PATCH_MEMO)))
    }

    /// The patch of `purpose` in the cell `(cx, cy)`: remembered, or else
    /// decided now (with the lock released: deciding a clearing or a
    /// copse asks for the small woods around it).
    fn patch(
        &self,
        purpose: u64,
        cx: u32,
        cy: u32,
        decide: impl FnOnce() -> Option<Patch>,
    ) -> Option<Patch> {
        let key = (purpose, cx, cy);
        let lock = || self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(patch) = lock().get(&key) {
            return *patch;
        }
        let patch = decide();
        lock().insert(key, patch);
        patch
    }
}

impl Clone for PatchMemo {
    /// Nothing remembered: it is only a memo.
    fn clone(&self) -> Self {
        Self::new()
    }
}

/// The woods of the tiles of a rectangle before the forest's depth is
/// known: per tile, row by row, the density (0 to 100), the habitat and
/// whether the trees thicken with the depth (`Woods::finish`).
pub(crate) struct Draft {
    pub(crate) density: Vec<u8>,
    pub(crate) habitat: Vec<Habitat>,
    pub(crate) ramped: Vec<bool>,
}

/// A round patch: centre and radius, thousandths of a tile.
type Patch = (i64, i64, i64);

impl Woods {
    /// From this density up, a tile is forest: the wood and the heart.
    pub const FOREST: u8 = 35;
    /// From this density up, the heart of a massif.
    pub const HEART: u8 = 75;
    /// Lattice of the massifs' noise, then of the two finer ones that
    /// roughen their outline, in tiles.
    const MASSIF_CELL: u32 = 320;
    const DETAIL_CELL: u32 = 64;
    const FINE_CELL: u32 = 12;
    /// Weight of the finer noises and of the wet ground, in thousandths.
    const DETAIL: i64 = 260;
    const FINE: i64 = 90;
    const WET: i64 = 120;
    /// Rows of wood at the cliff's foot, and their pull at the foot.
    const CLIFF_ROWS: i64 = 10;
    const CLIFF: i64 = 200;
    /// The pull from which a tile is wooded: fixed, so the woods of a
    /// place never depend on the rest of the map; set so that a little
    /// over half the land is forest.
    const LEVEL: i64 = 505;
    /// How far above the level the pull climbs from the edge of a massif
    /// to its full density, in thousandths: the width of the edge.
    const EDGE: i64 = 86;
    /// The trees thicken from the forest's edge towards its heart: from
    /// the forest's density (`FOREST`) over the first `SPARSE_DEPTH`
    /// tiles in (open crowns, the ground seen between them), rising
    /// steadily to full at `FULL_DEPTH` tiles. Small woods and copses keep
    /// their own, quicker rise.
    const SPARSE_DEPTH: u8 = 18;
    pub(crate) const FULL_DEPTH: u8 = 55;
    /// The woods along a river (11 November 2026): the water's pull,
    /// `RIPARIAN_PULL` at the thickest, held to `RIVER_WOOD` tiles off
    /// the water and fading to nothing at `RIVER_REACH`; the wood on the
    /// tiles `RIVER_NEAR.0` to `RIVER_NEAR.1` off the water is the
    /// riverside habitat (never the bank tile). Along a brook the same,
    /// smaller.
    const RIVER_NEAR: (u8, u8) = (2, 5);
    const RIVER_WOOD: u8 = 20;
    pub(crate) const RIVER_REACH: u8 = 60;
    const BROOK_NEAR: (u8, u8) = (2, 3);
    const BROOK_WOOD: u8 = 6;
    pub(crate) const BROOK_REACH: u8 = 12;
    const RIPARIAN_PULL: i64 = 480;
    const BROOK_PULL: i64 = 300;
    /// A bare stretch takes this much pull away, at its barest, as far
    /// as the water's wood would have reached: meadow to the water.
    const RIPARIAN_OPEN: i64 = 900;
    /// The riparian noise, on cells of `RIPARIAN_CELL` tiles, sets how
    /// thick the water's woods are from one stretch to the next; below
    /// `RIPARIAN_BARE` thousandths a stretch is bare, and the thinnest
    /// wooded stretch keeps `RIPARIAN_LEAST` thousandths of the thickest.
    const RIPARIAN_CELL: u32 = 256;
    const RIPARIAN_BARE: i64 = 140;
    const RIPARIAN_LEAST: i64 = 300;
    /// The density noise, on cells of `DENSITY_CELL` tiles: below
    /// `DENSITY_LIGHT` thousandths a region is light, its cells holding
    /// `STOCKING_LEAST` thousandths of the trees they would (2 to 2.5
    /// times farther apart); above `DENSITY_DENSE` it is dense, the trees
    /// as close as they come; between, the one eases into the other.
    const DENSITY_CELL: u32 = 300;
    const DENSITY_LIGHT: i64 = 100;
    const DENSITY_DENSE: i64 = 350;
    const STOCKING_LEAST: i64 = 400;
    /// Large clearings in the woods: one cell of the grid in two (of 150
    /// tiles) holds one, 12 to 36 tiles in radius, centred well inside a
    /// wood.
    const CLEARINGS: PatchRule = PatchRule {
        cell: 150,
        chance: 550,
        radius: (12_000, 36_000),
    };
    /// Small woods in the open: 25 to 90 tiles in radius, their pull at
    /// the centre `WOODLET_PULL`, an edge all around.
    const WOODLETS: PatchRule = PatchRule {
        cell: 140,
        chance: 650,
        radius: (25_000, 90_000),
    };
    const WOODLET_PULL: i64 = 450;
    /// Copses in the meadow: tight groups of a few trees, 2.5 to 6 tiles
    /// in radius.
    const COPSES: PatchRule = PatchRule {
        cell: 36,
        chance: 500,
        radius: (2500, 6000),
    };
    /// Radius of the camp clearing, and the rim over which trees come back.
    const CAMP_RADIUS: i64 = 9000;
    const CAMP_RIM: i64 = 3000;
    /// How ragged a round patch's rim is, in thousandths of its radius,
    /// and the lattice of the noise that frays it, in tiles: lobes, not a
    /// circle.
    const RAGGED: i64 = 500;
    const RAGGED_CELL: u32 = 7;

    /// The woods of a map of `width` by `height` below `cliff_rows`, with
    /// the camp clearing at `camp`.
    #[must_use]
    pub fn new(width: u32, height: u32, cliff_rows: u32, seed: u64, camp: (u32, u32)) -> Self {
        let noise_seed = |salt: u64| scatter::roll(seed, salt, &[]);
        Self {
            seed,
            massif: noise_seed(purpose::WOODS_MASSIF),
            detail: noise_seed(purpose::WOODS_DETAIL),
            fine: noise_seed(purpose::WOODS_FINE),
            rim: noise_seed(purpose::WOODS_RIM),
            riparian: noise_seed(purpose::WOODS_RIPARIAN),
            wet: Groves::with_cell(width, height, seed ^ purpose::WETNESS, 32),
            width,
            height,
            cliff_rows,
            camp: (i64::from(camp.0) * 1000, i64::from(camp.1) * 1000),
            memo: PatchMemo::new(),
        }
    }

    /// The woods of the tiles `x0..x1` × `y0..y1` before the forest's
    /// depth: `land` tells a tile the woods may cover (below the cliff,
    /// not water), `to_tile` the tiles from one of them to the nearest
    /// river and brook (each up to its reach + 1), `to_point` the same
    /// anywhere on the map (for the round patches' centres, which may lie
    /// outside). Each tile depends only on the seed and what lies within a
    /// bounded distance of it: the same whatever the rectangle.
    pub(crate) fn draft(
        &self,
        (x0, y0, x1, y1): (u32, u32, u32, u32),
        land: impl Fn(u32, u32) -> bool,
        to_tile: impl Fn(u32, u32) -> (u8, u8),
        to_point: impl Fn(u32, u32) -> (u8, u8),
    ) -> Draft {
        let rect = (x0, y0, x1, y1);
        let area = (self.width, self.height, self.cliff_rows);
        // Small woods rise in the open.
        let woodlet_fits =
            |x: u32, y: u32| land(x, y) && self.pull(x, y, to_point(x, y), &[]) < Self::LEVEL;
        let woodlets = Patches::new(
            Self::patches_in(
                area,
                self.seed,
                purpose::WOODS_WOODLET,
                Self::WOODLETS,
                rect,
                woodlet_fits,
                Some(&self.memo),
            ),
            rect,
        );
        // The pull with the small woods at a patch's centre, which may lie
        // outside the rectangle.
        let full_pull = |x: u32, y: u32| {
            let to = to_point(x, y);
            if woodlets.covers(x, y) {
                self.pull(x, y, to, woodlets.near(x, y))
            } else {
                let near = Self::patches_in(
                    area,
                    self.seed,
                    purpose::WOODS_WOODLET,
                    Self::WOODLETS,
                    (x, y, x + 1, y + 1),
                    woodlet_fits,
                    Some(&self.memo),
                );
                self.pull(x, y, to, &near)
            }
        };
        // A clearing opens well inside a wood, so it never eats a thin
        // wood whole.
        let clearings = Self::patches_in(
            area,
            self.seed,
            purpose::WOODS_CLEARING,
            Self::CLEARINGS,
            rect,
            |x, y| land(x, y) && full_pull(x, y) > Self::LEVEL + Self::EDGE * 3,
            Some(&self.memo),
        );
        // A copse stands in the open.
        let copses = Self::patches_in(
            area,
            self.seed,
            purpose::WOODS_COPSE,
            Self::COPSES,
            rect,
            |x, y| land(x, y) && full_pull(x, y) < Self::LEVEL - Self::EDGE / 2,
            Some(&self.memo),
        );
        let sets = PatchSets {
            woodlets,
            clearings: Patches::new(clearings, rect),
            copses: Patches::new(copses, rect),
        };
        let tiles = ((x1 - x0) * (y1 - y0)) as usize;
        let mut draft = Draft {
            density: vec![0; tiles],
            habitat: vec![Habitat::None; tiles],
            ramped: vec![false; tiles],
        };
        let mut i = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                if land(x, y) {
                    let (d, habitat, ramp) = self.tile(x, y, to_tile(x, y), &sets);
                    draft.density[i] = u8::try_from(d / 10).expect("percent");
                    draft.habitat[i] = habitat;
                    draft.ramped[i] = ramp;
                }
                i += 1;
            }
        }
        draft
    }

    /// A tile's density and habitat once the forest's `depth` there is
    /// known (`Woods::depth`, water no edge: 11 November 2026, the wood by
    /// a river is as deep as the woods around it, so the bank has the
    /// forest of its region, not a rim), from its `Draft`.
    pub(crate) fn finish(density: u8, habitat: Habitat, ramped: bool, depth: u8) -> (u8, Habitat) {
        if !ramped {
            // A small wood's heart still lies behind its edge.
            if habitat == Habitat::Heart && depth < 3 {
                return (density.min(Self::HEART - 1), Habitat::Wood);
            }
            return (density, habitat);
        }
        let density = density.min(Self::thickening(depth)).max(Self::FOREST);
        let habitat = if matches!(habitat, Habitat::CliffFoot | Habitat::Riverside) {
            habitat
        } else if density >= Self::HEART {
            Habitat::Heart
        } else {
            Habitat::Wood
        };
        (density, habitat)
    }

    /// Is a tile of this `Draft` forest ([`crate::map::Terrain::Forest`]) once
    /// finished? Its trees thicken with the depth, never below `FOREST`,
    /// or its density already reaches it: the depth changes neither.
    pub(crate) fn is_forest(density: u8, ramped: bool) -> bool {
        ramped || density >= Self::FOREST
    }

    /// Is a tile of this `Draft` open ground for the forest's depth: not
    /// forest before the depth, and not water?
    pub(crate) fn is_open(density: u8, water: bool) -> bool {
        density < Self::FOREST && !water
    }

    /// The density of trees `depth` tiles inside the forest, past its
    /// edge: the forest's own for `SPARSE_DEPTH` tiles, then rising
    /// steadily to 100 at `FULL_DEPTH`.
    fn thickening(depth: u8) -> u8 {
        if depth <= Self::SPARSE_DEPTH {
            return Self::FOREST;
        }
        let into = u32::from(depth.min(Self::FULL_DEPTH) - Self::SPARSE_DEPTH);
        let span = u32::from(Self::FULL_DEPTH - Self::SPARSE_DEPTH);
        Self::FOREST + u8::try_from(u32::from(100 - Self::FOREST) * into / span).expect("percent")
    }

    #[cfg(test)]
    /// Per tile, how many tiles (eight-way) to the nearest tile where
    /// `open` holds (or off the map), up to `cap`, in two sweeps.
    fn depth(width: u32, height: u32, open: impl Fn(usize) -> bool, cap: u8) -> Vec<u8> {
        let (w, h) = (width as usize, height as usize);
        let mut to = vec![cap; w * h];
        for (i, d) in to.iter_mut().enumerate() {
            let (x, y) = (i % w, i / w);
            if open(i) || x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                *d = 0;
            }
        }
        Self::sweep(&mut to, w, h, cap);
        to
    }

    /// The two sweeps of a distance, eight-way, each tile one more than
    /// its nearest neighbour, never above `cap`.
    pub(crate) fn sweep(to: &mut [u8], w: usize, h: usize, cap: u8) {
        let step = |to: &mut [u8], i: usize, j: usize| {
            let through = to[j].saturating_add(1).min(cap);
            if through < to[i] {
                to[i] = through;
            }
        };
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if x > 0 {
                    step(to, i, i - 1);
                }
                if y > 0 {
                    step(to, i, i - w);
                    if x > 0 {
                        step(to, i, i - w - 1);
                    }
                    if x + 1 < w {
                        step(to, i, i - w + 1);
                    }
                }
            }
        }
        for y in (0..h).rev() {
            for x in (0..w).rev() {
                let i = y * w + x;
                if x + 1 < w {
                    step(to, i, i + 1);
                }
                if y + 1 < h {
                    step(to, i, i + w);
                    if x + 1 < w {
                        step(to, i, i + w + 1);
                    }
                    if x > 0 {
                        step(to, i, i + w - 1);
                    }
                }
            }
        }
    }

    /// The round patches of `rule` for `purpose` that may reach the tiles
    /// `x0..x1` × `y0..y1`: on a grid of `rule.cell` tiles in absolute
    /// coordinates, a cell holds one with `rule.chance` thousandths, its
    /// centre anywhere in the cell, where it `fits`; (x, y, radius) in
    /// thousandths of a tile. Each is decided by its own cell alone, so a
    /// rectangle sees the same patches as the whole map, those too far to
    /// reach it aside.
    fn patches_in(
        (width, height, cliff_rows): (u32, u32, u32),
        seed: u64,
        purpose: u64,
        rule: PatchRule,
        (x0, y0, x1, y1): (u32, u32, u32, u32),
        fits: impl Fn(u32, u32) -> bool,
        memo: Option<&PatchMemo>,
    ) -> Vec<Patch> {
        let mut out = Vec::new();
        let side = u64::from(rule.cell) * 1000;
        let reach = Self::reach(i64::from(rule.radius.1));
        // The cells whose patches may reach the rectangle, on the map.
        let cells = |lo: u32, hi: u32, last: u32| {
            let side = i64::from(rule.cell) * 1000;
            let first = (i64::from(lo) * 1000 - reach).max(0) / side;
            let end = (i64::from(hi) * 1000 + reach) / side;
            let first = u32::try_from(first).expect("on the map");
            let end = u32::try_from(end).map_or(last, |e| e.min(last));
            first..=end
        };
        let decide = |cx: u32, cy: u32| -> Option<Patch> {
            let keys = |n: u64| [key(i64::from(cx)), key(i64::from(cy)), n];
            if !scatter::chance(rule.chance, seed, purpose, &keys(0)) {
                return None;
            }
            let x = u64::from(cx) * side + scatter::roll(seed, purpose, &keys(1)) % side;
            let y = u64::from(cy) * side + scatter::roll(seed, purpose, &keys(2)) % side;
            let (lo, hi) = rule.radius;
            let r = u64::from(lo) + scatter::roll(seed, purpose, &keys(3)) % u64::from(hi - lo + 1);
            let (tx, ty) = (x / 1000, y / 1000);
            if tx >= u64::from(width) || ty >= u64::from(height) || ty < u64::from(cliff_rows) {
                return None;
            }
            let (tx, ty) = (
                u32::try_from(tx).expect("on the map"),
                u32::try_from(ty).expect("on the map"),
            );
            let at = |v: u64| i64::try_from(v).expect("on the map");
            fits(tx, ty).then(|| (at(x), at(y), at(r)))
        };
        for cy in cells(y0, y1, height / rule.cell) {
            for cx in cells(x0, x1, width / rule.cell) {
                let patch = match memo {
                    Some(memo) => memo.patch(purpose, cx, cy, || decide(cx, cy)),
                    None => decide(cx, cy),
                };
                out.extend(patch);
            }
        }
        out
    }

    /// The farthest a patch of radius `r` (thousandths) reaches with its
    /// ragged rim (`Woods::within`): the radius times 1 + RAGGED / 2, and
    /// a tile more for safety.
    fn reach(r: i64) -> i64 {
        r * (1000 + Self::RAGGED / 2 + 1) / 1000 + 1000
    }

    #[cfg(test)]
    /// Per tile, how many tiles (eight-way) to the nearest brook, or to
    /// the nearest river (water or ford), up to `reach + 1`: a distance
    /// only ever that far, worked out in two sweeps.
    pub(crate) fn to_water(
        width: u32,
        height: u32,
        rivers: &Rivers,
        brook: bool,
        reach: u8,
    ) -> Vec<u8> {
        let far = reach + 1;
        let (w, h) = (width as usize, height as usize);
        let mut to = vec![far; w * h];
        for y in 0..height {
            for x in 0..width {
                let water = match rivers.at(x, y) {
                    Some(Terrain::Brook) => brook,
                    Some(Terrain::Water | Terrain::Ford) => !brook,
                    _ => false,
                };
                if water {
                    to[y as usize * w + x as usize] = 0;
                }
            }
        }
        Self::sweep(&mut to, w, h, far);
        to
    }

    /// How much the riparian noise lifts the woods by the water at
    /// `(x, y)`, in thousandths: below nothing on a bare stretch (down to
    /// -1000 at the barest), above nothing where the water draws trees
    /// (up to 1000 on the thickest stretch).
    fn riparian_lift(riparian: u64, x: u32, y: u32) -> i64 {
        let n = i64::from(noise(
            riparian,
            Self::RIPARIAN_CELL,
            i32::try_from(x).expect("map fits in i32"),
            i32::try_from(y).expect("map fits in i32"),
        ));
        let bare = Self::RIPARIAN_BARE;
        if n < bare {
            -(bare - n) * 1000 / bare
        } else {
            (n - bare) * 1000 / (1000 - bare)
        }
    }

    /// How thick the water's woods are for a `lift` at or above nothing,
    /// in thousandths of the thickest: the thinnest stretch keeps
    /// `RIPARIAN_LEAST`.
    fn thickness(lift: i64) -> i64 {
        Self::RIPARIAN_LEAST + (1000 - Self::RIPARIAN_LEAST) * lift.clamp(0, 1000) / 1000
    }

    /// The seed of the density noise for the world's `seed`.
    #[must_use]
    pub fn density_seed(seed: u64) -> u64 {
        scatter::roll(seed, purpose::WOODS_DENSITY, &[])
    }

    /// How close the trees stand at `(x, y)`, in thousandths of the
    /// chance a cell holds a tree: `STOCKING_LEAST` in a light region,
    /// 1000 in a dense one, from the density noise (`density_seed`).
    #[must_use]
    pub fn stocking(density_seed: u64, x: i32, y: i32) -> u32 {
        let n = i64::from(noise(density_seed, Self::DENSITY_CELL, x, y));
        let eased = ((n - Self::DENSITY_LIGHT) * 1000
            / (Self::DENSITY_DENSE - Self::DENSITY_LIGHT))
            .clamp(0, 1000);
        u32::try_from(Self::STOCKING_LEAST + (1000 - Self::STOCKING_LEAST) * eased / 1000)
            .expect("thousandths")
    }
}

/// How round patches of one kind are laid: a grid of `cell` tiles, a
/// chance a cell in thousandths, a radius range in thousandths of a tile.
#[derive(Debug, Clone, Copy)]
struct PatchRule {
    cell: u32,
    chance: u32,
    radius: (u32, u32),
}

/// The round patches that may reach a rectangle, of each kind.
struct PatchSets {
    woodlets: Patches,
    clearings: Patches,
    copses: Patches,
}

/// Round patches (clearings, small woods, copses: centre and radius,
/// thousandths of a tile), filed by the square cells of `PATCH_CELL`
/// tiles of a rectangle their ragged rims can reach: a tile looks only at
/// those of its own cell.
struct Patches {
    cells: Vec<Vec<Patch>>,
    cells_wide: u32,
    /// The rectangle filed: `x0, y0, x1, y1`.
    rect: (u32, u32, u32, u32),
}

/// Side of a cell of `Patches`, in tiles.
const PATCH_CELL: u32 = 16;

impl Patches {
    fn new(patches: Vec<Patch>, rect: (u32, u32, u32, u32)) -> Self {
        let (x0, y0, x1, y1) = rect;
        let (wide, high) = (
            (x1 - x0).div_ceil(PATCH_CELL),
            (y1 - y0).div_ceil(PATCH_CELL),
        );
        let mut cells = vec![Vec::new(); (wide * high) as usize];
        let cell = |mm: i64, origin: u32, last: u32| {
            let tile = (mm / 1000 - i64::from(origin)).max(0);
            u32::try_from(tile).map_or(last, |t| (t / PATCH_CELL).min(last))
        };
        for (cx, cy, r) in patches {
            let reach = Woods::reach(r);
            for y in cell(cy - reach, y0, high - 1)..=cell(cy + reach, y0, high - 1) {
                for x in cell(cx - reach, x0, wide - 1)..=cell(cx + reach, x0, wide - 1) {
                    cells[(y * wide + x) as usize].push((cx, cy, r));
                }
            }
        }
        Self {
            cells,
            cells_wide: wide,
            rect,
        }
    }

    /// Is `(x, y)` in the rectangle filed?
    fn covers(&self, x: u32, y: u32) -> bool {
        let (x0, y0, x1, y1) = self.rect;
        (x0..x1).contains(&x) && (y0..y1).contains(&y)
    }

    /// The patches that may reach the tile `(x, y)` of the rectangle.
    fn near(&self, x: u32, y: u32) -> &[Patch] {
        let (x0, y0, _, _) = self.rect;
        self.cells
            .get((((y - y0) / PATCH_CELL) * self.cells_wide + (x - x0) / PATCH_CELL) as usize)
            .map_or(&[], Vec::as_slice)
    }
}

impl Woods {
    /// How strongly the woods pull at `(x, y)`, `to` tiles from the
    /// nearest river and brook, in thousandths: the massifs' noise,
    /// roughened, wetter ground and the cliff's foot, the small `woodlets`
    /// near (none to leave them out), and the water's draw (or a bare
    /// stretch's opening) near a river or a brook.
    fn pull(&self, x: u32, y: u32, to: (u8, u8), woodlets: &[Patch]) -> i64 {
        let (ix, iy) = (
            i32::try_from(x).expect("map fits in i32"),
            i32::try_from(y).expect("map fits in i32"),
        );
        let centred = |v: u32| i64::from(v) - 500;
        let mut m = i64::from(noise(self.massif, Self::MASSIF_CELL, ix, iy))
            + Self::DETAIL * centred(noise(self.detail, Self::DETAIL_CELL, ix, iy)) / 1000
            + Self::FINE * centred(noise(self.fine, Self::FINE_CELL, ix, iy)) / 1000
            + Self::WET * centred(self.wet.value(x, y)) / 1000;
        let below = i64::from(y) - i64::from(self.cliff_rows);
        if below < Self::CLIFF_ROWS {
            m += Self::CLIFF * (Self::CLIFF_ROWS - below) / Self::CLIFF_ROWS;
        }
        m += self.woodlet_pull(x, y, woodlets);
        m + self.water_pull(x, y, to)
    }

    /// What the water adds to the pull at `(x, y)`, `(river, brook)`
    /// tiles from each: the water's full pull from its edge through its
    /// wood, fading to nothing at its reach, as thick as the stretch is;
    /// on a bare stretch, taken away instead, over the same reach. A
    /// river and a brook each pull; the stronger counts.
    fn water_pull(&self, x: u32, y: u32, (to_river, to_brook): (u8, u8)) -> i64 {
        let lift = self.lift(x, y);
        let shape = |d: u8, (wood, reach): (u8, u8), pull: i64| -> i64 {
            if d > reach {
                return 0;
            }
            let band = if d <= wood {
                1000
            } else {
                1000 * i64::from(reach - d) / i64::from(reach - wood)
            };
            if lift < 0 {
                // Bare: open as far as the wood would have gone.
                Self::RIPARIAN_OPEN * lift / 1000 * band / 1000
            } else {
                pull * Self::thickness(lift) / 1000 * band / 1000
            }
        };
        let river = shape(
            to_river,
            (Self::RIVER_WOOD, Self::RIVER_REACH),
            Self::RIPARIAN_PULL,
        );
        let brook = shape(
            to_brook,
            (Self::BROOK_WOOD, Self::BROOK_REACH),
            Self::BROOK_PULL,
        );
        if lift < 0 {
            river.min(brook)
        } else {
            river.max(brook)
        }
    }

    /// The riparian lift at `(x, y)` (`Woods::riparian_lift`).
    fn lift(&self, x: u32, y: u32) -> i64 {
        Self::riparian_lift(self.riparian, x, y)
    }

    /// What the small `woodlets` near add to the pull at `(x, y)`.
    fn woodlet_pull(&self, x: u32, y: u32, woodlets: &[Patch]) -> i64 {
        Self::WOODLET_PULL * self.within(woodlets, x, y) / 1000
    }

    /// How deep inside one of `patches` `(x, y)` is: 1000 at a centre, 0
    /// at and past a rim made ragged by a fine noise.
    fn within(&self, patches: &[Patch], x: u32, y: u32) -> i64 {
        if patches.is_empty() {
            return 0;
        }
        let (px, py) = (i64::from(x) * 1000, i64::from(y) * 1000);
        let rim = noise(
            self.rim,
            Self::RAGGED_CELL,
            i32::try_from(x).expect("map fits in i32"),
            i32::try_from(y).expect("map fits in i32"),
        );
        let ragged = 1000 + Self::RAGGED * (i64::from(rim) - 500) / 1000;
        patches
            .iter()
            .map(|(cx, cy, r)| {
                let r = r * ragged / 1000;
                if (px - cx).abs() >= r || (py - cy).abs() >= r {
                    return 0;
                }
                let d = ((px - cx).pow(2) + (py - cy).pow(2)).isqrt();
                if d < r { 1000 - d * 1000 / r } else { 0 }
            })
            .max()
            .unwrap_or(0)
    }

    /// The density (thousandths) and habitat of the land tile `(x, y)`,
    /// `to` tiles from the nearest river and brook, and whether its trees
    /// thicken with its depth in the forest (not in a small wood, which
    /// keeps its quicker rise).
    fn tile(&self, x: u32, y: u32, to: (u8, u8), sets: &PatchSets) -> (i64, Habitat, bool) {
        let woodlets = sets.woodlets.near(x, y);
        let pull = self.pull(x, y, to, woodlets);
        let in_a_woodlet = pull - self.woodlet_pull(x, y, woodlets) < Self::LEVEL;
        let mut d = ((pull - Self::LEVEL) * 1000 / Self::EDGE).clamp(0, 1000);
        let mut habitat = if d >= i64::from(Self::HEART) * 10 {
            Habitat::Heart
        } else if d >= i64::from(Self::FOREST) * 10 {
            Habitat::Wood
        } else if d > 0 {
            Habitat::Edge
        } else {
            Habitat::Meadow
        };
        // Clearings: the woods open, with an edge around.
        if d > 0 {
            let open = self.within(sets.clearings.near(x, y), x, y);
            if open > 0 {
                d = d * (1000 - (3 * open).min(1000)) / 1000;
                habitat = if d < 100 {
                    d = 0;
                    Habitat::Clearing
                } else {
                    Habitat::Edge
                };
            }
        }
        // The camp clearing, trees coming back over its rim.
        let from_camp = ((i64::from(x) * 1000 - self.camp.0).pow(2)
            + (i64::from(y) * 1000 - self.camp.1).pow(2))
        .isqrt();
        if from_camp < Self::CAMP_RADIUS {
            let back = (from_camp - (Self::CAMP_RADIUS - Self::CAMP_RIM)).max(0);
            d = d * back / Self::CAMP_RIM;
            if d < 100 {
                d = 0;
                habitat = Habitat::Camp;
            }
        }
        // The wood at the cliff's foot is its own place.
        let below = i64::from(y) - i64::from(self.cliff_rows);
        if below < Self::CLIFF_ROWS
            && matches!(habitat, Habitat::Edge | Habitat::Wood | Habitat::Heart)
        {
            habitat = Habitat::CliffFoot;
        }
        // Copses in the meadow, tight.
        if habitat == Habitat::Meadow {
            let copse = self.within(sets.copses.near(x, y), x, y);
            if copse > 0 {
                d = 800 + 200 * copse / 1000;
                habitat = Habitat::Copse;
            }
        }
        // The wood on the first tiles by the water is the riverside
        // habitat, the riverside kinds' (willow, alder): no tree of its
        // own, the woods' density as around it; never the tile beside
        // any water, never on a bare stretch.
        let (river, brook) = to;
        let near = |d: u8, (lo, hi): (u8, u8)| (lo..=hi).contains(&d);
        if matches!(habitat, Habitat::Wood | Habitat::Heart)
            && river > 1
            && brook > 1
            && (near(river, Self::RIVER_NEAR) || near(brook, Self::BROOK_NEAR))
            && self.lift(x, y) >= 0
        {
            habitat = Habitat::Riverside;
        }
        let ramped = !in_a_woodlet
            && matches!(
                habitat,
                Habitat::Wood | Habitat::Heart | Habitat::CliffFoot | Habitat::Riverside
            );
        (d, habitat, ramped)
    }
}

/// The woods of a whole map at once, as they were made before the map
/// made them per chunk: the reference the chunks are checked against,
/// and what the woods' own tests read.
#[cfg(test)]
pub(crate) struct WholeWoods {
    width: u32,
    density: Vec<u8>,
    habitat: Vec<Habitat>,
}

#[cfg(test)]
impl WholeWoods {
    /// The woods of a map of `width` by `height` below `cliff_rows`,
    /// around `rivers`, with the camp clearing at `camp`: every tile at
    /// once, the distances to the waters and the forest's depth swept
    /// over the whole map.
    pub(crate) fn generate(
        width: u32,
        height: u32,
        cliff_rows: u32,
        seed: u64,
        rivers: &Rivers,
        camp: (u32, u32),
    ) -> Self {
        let woods = Woods::new(width, height, cliff_rows, seed, camp);
        let w = width as usize;
        let to_river = Woods::to_water(width, height, rivers, false, Woods::RIVER_REACH);
        let to_brook = Woods::to_water(width, height, rivers, true, Woods::BROOK_REACH);
        let to = |x: u32, y: u32| {
            let i = y as usize * w + x as usize;
            (to_river[i], to_brook[i])
        };
        let land = |x: u32, y: u32| y >= cliff_rows && rivers.at(x, y).is_none();
        let draft = woods.draft((0, 0, width, height), land, to, to);
        let depth = Woods::depth(
            width,
            height,
            |i| {
                let (x, y) = (
                    u32::try_from(i % w).expect("fits"),
                    u32::try_from(i / w).expect("fits"),
                );
                Woods::is_open(draft.density[i], rivers.at(x, y).is_some())
            },
            Woods::FULL_DEPTH,
        );
        let (mut density, mut habitat) = (draft.density, draft.habitat);
        for i in 0..density.len() {
            (density[i], habitat[i]) =
                Woods::finish(density[i], habitat[i], draft.ramped[i], depth[i]);
        }
        Self {
            width,
            density,
            habitat,
        }
    }

    /// The density of trees at `(x, y)`, 0 to 100.
    pub(crate) fn density(&self, x: u32, y: u32) -> u8 {
        self.density[(y * self.width + x) as usize]
    }

    /// The habitat of `(x, y)`.
    pub(crate) fn habitat(&self, x: u32, y: u32) -> Habitat {
        self.habitat[(y * self.width + x) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Map, Pos, Rivers};

    /// A valley's habitats: each one's tiles, the land's, the forest's.
    fn tally(map: &Map, size: i32) -> (std::collections::BTreeMap<Habitat, usize>, usize, usize) {
        let mut seen = std::collections::BTreeMap::new();
        let (mut land, mut forest) = (0, 0);
        for y in 0..size {
            for x in 0..size {
                let pos = Pos::new(x, y);
                let habitat = map.habitat(pos);
                *seen.entry(habitat).or_insert(0) += 1;
                if habitat != Habitat::None {
                    land += 1;
                    // Forest exactly where the density reaches FOREST.
                    let is_forest = map.get(pos) == Some(Terrain::Forest);
                    assert_eq!(is_forest, map.trees(pos) >= Woods::FOREST, "{pos:?}");
                    forest += usize::from(is_forest);
                }
            }
        }
        (seen, land, forest)
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn a_wild_valley_is_mostly_forest_with_woods_along_its_waters() {
        const SIZE: i32 = 512;
        let (mut land_all, mut forest_all) = (0, 0);
        let mut habitats = std::collections::BTreeSet::new();
        let (mut riverside_maps, mut brook_lines) = (0, 0);
        // The density of trees: over the land, on the band 2 to 10 tiles
        // off a river of each stretch of the riparian noise, and along
        // the bare stretches.
        let (mut land_sum, mut near_sum, mut near_tiles, mut bare_sum, mut bare_tiles) =
            (0_u64, 0_u64, 0_u64, 0_u64, 0_u64);
        let mut stretches: std::collections::BTreeMap<(u64, i32, i32), (u64, u64)> =
            std::collections::BTreeMap::new();
        for seed in 1..=4 {
            let map = Map::valley(512, 512, seed);
            let (seen, land, forest) = tally(&map, SIZE);
            land_all += land;
            forest_all += forest;
            habitats.extend(seen.keys().copied());
            riverside_maps += usize::from(seen.contains_key(&Habitat::Riverside));
            let rivers = Rivers::generate(512, 512, 64, seed);
            let to_river = Woods::to_water(512, 512, &rivers, false, Woods::RIVER_REACH);
            let to_brook = Woods::to_water(512, 512, &rivers, true, Woods::BROOK_REACH);
            let riparian = scatter::roll(seed, purpose::WOODS_RIPARIAN, &[]);
            let near = |x: i32, y: i32, reach: i32, what: &dyn Fn(Pos) -> bool| {
                (-reach..=reach).any(|dy| (-reach..=reach).any(|dx| what(Pos::new(x + dx, y + dy))))
            };
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let pos = Pos::new(x, y);
                    let habitat = map.habitat(pos);
                    let i = usize::try_from(y * SIZE + x).unwrap();
                    let (river, brook) = (to_river[i], to_brook[i]);
                    let (ux, uy) = (u32::try_from(x).unwrap(), u32::try_from(y).unwrap());
                    let lift = Woods::riparian_lift(riparian, ux, uy);
                    match habitat {
                        // The heart lies behind an edge: no open meadow
                        // right beside it.
                        Habitat::Heart => assert!(
                            !near(x, y, 1, &|p| map.habitat(p) == Habitat::Meadow),
                            "seed {seed}: heart at ({x}, {y}) touches the meadow"
                        ),
                        // A clearing opens in the woods.
                        Habitat::Clearing => assert!(
                            near(x, y, 40, &|p| map.trees(p) >= Woods::FOREST),
                            "seed {seed}: clearing at ({x}, {y}) far from any wood"
                        ),
                        // The riverside habitat is the wood 2 to 5 tiles
                        // off a river, or 2 to 3 off a brook, never on the
                        // tile beside any water, never on a bare stretch.
                        Habitat::Riverside => {
                            let (lo, hi) = Woods::RIVER_NEAR;
                            let (blo, bhi) = Woods::BROOK_NEAR;
                            assert!(
                                river >= 2
                                    && brook >= 2
                                    && ((lo..=hi).contains(&river) || (blo..=bhi).contains(&brook)),
                                "seed {seed}: riverside at ({x}, {y}), {river} from a river, {brook} from a brook"
                            );
                            assert!(lift >= 0, "seed {seed}: riverside on a bare stretch");
                            assert!(
                                map.trees(pos) >= Woods::FOREST,
                                "seed {seed}: riverside at ({x}, {y}) is no wood"
                            );
                            brook_lines += usize::from(river > hi);
                        }
                        _ => {}
                    }
                    if habitat == Habitat::None {
                        continue;
                    }
                    let density = u64::from(map.trees(pos));
                    land_sum += density;
                    if (2..=10).contains(&river) && lift >= 0 {
                        near_sum += density;
                        near_tiles += 1;
                        let stretch = stretches.entry((seed, x / 256, y / 256)).or_default();
                        stretch.0 += density;
                        stretch.1 += 1;
                    }
                    if (1..=Woods::RIVER_WOOD).contains(&river) && lift < -300 {
                        bare_sum += density;
                        bare_tiles += 1;
                    }
                }
            }
            // The camp is open: no tree within four tiles of it.
            let camp = map.camp();
            assert!(
                !near(camp.x, camp.y, 4, &|p| map.trees(p) > 0),
                "seed {seed}: trees at the camp"
            );
        }
        // A little over half the land forest.
        let share = forest_all * 1000 / land_all;
        eprintln!("forest {share} per mille of the land");
        assert!((430..=680).contains(&share), "forest {share} per mille");
        for habitat in [
            Habitat::Heart,
            Habitat::Wood,
            Habitat::Edge,
            Habitat::Meadow,
            Habitat::Clearing,
            Habitat::Camp,
            Habitat::Copse,
            Habitat::CliffFoot,
        ] {
            assert!(habitats.contains(&habitat), "no {habitat:?} on four maps");
        }
        assert!(
            riverside_maps >= 3,
            "riverside trees on {riverside_maps} maps"
        );
        assert!(
            brook_lines > 50,
            "{brook_lines} riverside tiles along the brooks"
        );
        // The riparian gradient: the band by the rivers is at least as
        // dense as the land as a whole on most stretches (the wood
        // reaches the water), and a bare stretch, now and then, is open
        // meadow to the water.
        let land_mean = land_sum / u64::try_from(land_all).unwrap();
        let near_mean = near_sum / near_tiles.max(1);
        let bare_mean = bare_sum / bare_tiles.max(1);
        let judged: Vec<bool> = stretches
            .values()
            .filter(|(_, n)| *n >= 100)
            .map(|(sum, n)| sum / n >= land_mean)
            .collect();
        let denser = judged.iter().filter(|d| **d).count();
        eprintln!(
            "density: land {land_mean}, by the rivers {near_mean} ({near_tiles} tiles; denser on {denser} of {} stretches), bare stretches {bare_mean} ({bare_tiles} tiles)",
            judged.len()
        );
        assert!(
            near_mean >= land_mean,
            "by the rivers {near_mean}, land {land_mean}"
        );
        assert!(
            denser * 10 >= judged.len() * 6,
            "{denser} of {}",
            judged.len()
        );
        assert!(bare_tiles >= 100, "{bare_tiles} tiles on bare stretches");
        assert!(
            bare_mean * 2 < land_mean,
            "bare {bare_mean}, land {land_mean}"
        );
    }

    #[test]
    fn the_trees_thicken_from_the_edge_to_the_heart() {
        // The density of the forest's tiles by their depth in it: sparse
        // in the first tiles, full deep inside, rising between.
        let (mut sums, mut counts) = ([0_u64; 4], [0_u64; 4]);
        for seed in 1..=3 {
            let map = Map::valley(512, 512, seed);
            let forest = |i: usize| {
                let p = Pos::new(
                    i32::try_from(i % 512).unwrap(),
                    i32::try_from(i / 512).unwrap(),
                );
                map.get(p) == Some(Terrain::Forest)
            };
            // Water is no edge: the wood by a river is as deep as the
            // woods around it.
            let water = |i: usize| {
                let p = Pos::new(
                    i32::try_from(i % 512).unwrap(),
                    i32::try_from(i / 512).unwrap(),
                );
                matches!(
                    map.get(p),
                    Some(Terrain::Water | Terrain::Ford | Terrain::Brook)
                )
            };
            let depth = Woods::depth(512, 512, |i| !forest(i) && !water(i), 64);
            for (i, d) in depth.iter().enumerate() {
                let p = Pos::new(
                    i32::try_from(i % 512).unwrap(),
                    i32::try_from(i / 512).unwrap(),
                );
                if !forest(i) || !matches!(map.habitat(p), Habitat::Wood | Habitat::Heart) {
                    continue;
                }
                let band = match *d {
                    3..=15 => 0,
                    25..=35 => 1,
                    36..=50 => 2,
                    56.. => 3,
                    _ => continue,
                };
                sums[band] += u64::from(map.trees(p));
                counts[band] += 1;
            }
        }
        let mean: Vec<u64> = (0..4).map(|b| sums[b] / counts[b].max(1)).collect();
        eprintln!("density by depth: {mean:?} over {counts:?} tiles");
        assert!(counts.iter().all(|c| *c > 100), "{counts:?}");
        assert!(
            mean[0] < mean[1] && mean[1] < mean[2] && mean[2] < mean[3],
            "{mean:?}"
        );
        assert!(mean[0] <= 50 && mean[3] >= 90, "{mean:?}");
    }

    #[test]
    fn round_patches_are_decided_cell_by_cell_whatever_the_map() {
        // The same patches where two maps overlap: each cell decides its
        // own, the map's size changes nothing.
        let rule = Woods::COPSES;
        let all = |(w, h)| {
            Woods::patches_in(
                (w, h, 0),
                9,
                purpose::WOODS_COPSE,
                rule,
                (0, 0, w, h),
                |_, _| true,
                None,
            )
        };
        let small = all((256, 256));
        let large = all((768, 512));
        let inside: Vec<_> = large
            .iter()
            .copied()
            .filter(|(x, y, _)| *x < 256_000 && *y < 256_000)
            .collect();
        assert!(small.len() > 10, "{} copses", small.len());
        assert_eq!(small, inside);
        // About one cell in two holds a copse.
        let cells = (768 / rule.cell + 1) * (512 / rule.cell + 1);
        let share = u32::try_from(large.len()).unwrap() * 1000 / cells;
        assert!(
            (380..=620).contains(&share),
            "{share} per mille of the cells"
        );
    }
}
