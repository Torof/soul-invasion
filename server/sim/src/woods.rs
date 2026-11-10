//! Where the trees grow (D37): a density of trees per tile and the kind of
//! place each tile is, generated with the valley and never stored.
//!
//! A temperate valley untouched by man (10 November 2026): forest covers
//! a little over half the land. Each tile's woods depend only on the
//! seed and what lies within a bounded distance of it: no level chosen
//! for the whole map, no stream drawn across it, so a chunk could be made
//! alone (D24).
//!
//! - **Massifs**: a very broad noise (`MASSIF_CELL`), roughened by two
//!   finer ones, thickened a little where the ground is wet and in a band
//!   at the cliff's foot, cut at a fixed `LEVEL`.
//! - **Meadows along the rivers**: the woods' pull falls near a river
//!   (water or ford), so the floodplains are open grass (`RIVER_REACH`).
//! - **Edge, wood, heart**: past a narrow edge, the trees thicken with
//!   the depth in the forest: sparse over the first `SPARSE_DEPTH` tiles,
//!   full at `FULL_DEPTH`; the heart is the densest.
//! - **Large clearings** open in the woods, and a **camp clearing** around
//!   the place the first bodies start.
//! - **Small woods** (50 to 200 m across) rise in the open between the
//!   massifs; **copses**, tight groups of a few trees, dot the meadows;
//!   **riverside** trees line the rivers, sparse, and **lines of trees**
//!   follow the brooks.
//!
//! Round patches (clearings, small woods, copses) are decided cell by
//! cell on a grid in absolute coordinates, each from a hash of the seed
//! and its cell (`gen::scatter`): the same anywhere, in any order.
//!
//! A tile is [`Terrain::Forest`] where the density reaches `FOREST`: the
//! wood and the heart. The edge, copses, riverside and brookside trees
//! stand on grass. Which species grows where comes with the trees
//! themselves (`trees`).
//!
//! All arithmetic is in integers (thousandths), so a seed gives the same
//! woods everywhere.

// Generation is integer-only (D24; linted from `lib.rs` and
// `sim/clippy.toml`). No known exception.

use crate::r#gen::scatter::{self, key, purpose};
use crate::r#gen::{Groves, noise};
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
    /// Trees along a river.
    Riverside,
    /// The wood in the band at the cliff's foot.
    CliffFoot,
    /// Water, a ford, a brook or the cliff: no tree.
    None,
}

/// Density of trees, per tile, and its habitat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Woods {
    width: u32,
    /// Per tile, 0 to 100.
    density: Vec<u8>,
    habitat: Vec<Habitat>,
}

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
    const LEVEL: i64 = 432;
    /// How far above the level the pull climbs from the edge of a massif
    /// to its full density, in thousandths: the width of the edge.
    const EDGE: i64 = 86;
    /// The trees thicken from the forest's edge towards its heart: from
    /// the forest's density (`FOREST`) over the first `SPARSE_DEPTH`
    /// tiles in (open crowns, the ground seen between them), rising
    /// steadily to full at `FULL_DEPTH` tiles. Small woods and copses keep
    /// their own, quicker rise.
    const SPARSE_DEPTH: u8 = 18;
    const FULL_DEPTH: u8 = 55;
    /// Within this many tiles of a river the woods' pull falls, by
    /// `RIVER_OPEN` at the bank, to nothing at the reach: meadows along
    /// the rivers.
    const RIVER_REACH: u8 = 40;
    const RIVER_OPEN: i64 = 480;
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
    /// Chance of a riverside tree two tiles from a river, then three, in
    /// thousandths: never on the tile beside the water, so a tree and its
    /// crown stand on the bank, not in the river.
    const RIVERSIDE: (u32, u32) = (110, 60);
    /// Chance of a tree on an open tile one or two tiles from a brook, in
    /// thousandths: lines of trees along the brooks.
    const BROOKSIDE: u32 = 450;
    /// How ragged a round patch's rim is, in thousandths of its radius,
    /// and the lattice of the noise that frays it, in tiles: lobes, not a
    /// circle.
    const RAGGED: i64 = 500;
    const RAGGED_CELL: u32 = 7;

    /// The woods of a map of `width` by `height` below `cliff_rows`, around
    /// `rivers`, with the camp clearing at `camp`.
    #[must_use]
    pub fn generate(
        width: u32,
        height: u32,
        cliff_rows: u32,
        seed: u64,
        rivers: &Rivers,
        camp: (u32, u32),
    ) -> Self {
        let tiles = (width * height) as usize;
        let mut woods = Self {
            width,
            density: vec![0; tiles],
            habitat: vec![Habitat::None; tiles],
        };
        let land = |x: u32, y: u32| y >= cliff_rows && rivers.at(x, y).is_none();
        let noise_seed = |salt: u64| scatter::roll(seed, salt, &[]);
        let mut plan = Plan {
            seed,
            massif: noise_seed(purpose::WOODS_MASSIF),
            detail: noise_seed(purpose::WOODS_DETAIL),
            fine: noise_seed(purpose::WOODS_FINE),
            rim: noise_seed(purpose::WOODS_RIM),
            // The same wetness the brooks rise from.
            wet: Groves::with_cell(width, height, seed ^ purpose::WETNESS, 32),
            cliff_rows,
            width,
            height,
            to_river: Self::to_river(width, height, rivers),
            woodlets: Patches::default(),
            clearings: Patches::default(),
            copses: Patches::default(),
            camp: (i64::from(camp.0) * 1000, i64::from(camp.1) * 1000),
        };
        let area = (width, height, cliff_rows);
        // Small woods rise in the open.
        let woodlets = Self::patches(
            area,
            seed,
            purpose::WOODS_WOODLET,
            Self::WOODLETS,
            |x, y| land(x, y) && plan.pull(x, y, false) < Self::LEVEL,
        );
        plan.woodlets = Patches::new(woodlets, width, height);
        // A clearing opens well inside a wood, so it never eats a thin
        // wood whole.
        let clearings = Self::patches(
            area,
            seed,
            purpose::WOODS_CLEARING,
            Self::CLEARINGS,
            |x, y| land(x, y) && plan.pull(x, y, true) > Self::LEVEL + Self::EDGE * 3,
        );
        // A copse stands in the open.
        let copses = Self::patches(area, seed, purpose::WOODS_COPSE, Self::COPSES, |x, y| {
            land(x, y) && plan.pull(x, y, true) < Self::LEVEL - Self::EDGE / 2
        });
        plan.clearings = Patches::new(clearings, width, height);
        plan.copses = Patches::new(copses, width, height);
        let mut ramped = vec![false; tiles];
        for y in 0..height {
            for x in 0..width {
                if land(x, y) {
                    let i = (y * width + x) as usize;
                    let (d, habitat, ramp) = plan.tile(x, y, rivers);
                    woods.density[i] = u8::try_from(d / 10).expect("percent");
                    woods.habitat[i] = habitat;
                    ramped[i] = ramp;
                }
            }
        }
        // How deep in the forest each tile is, and the trees thickening
        // with it.
        let depth = Self::depth(
            width,
            height,
            |i| woods.density[i] < Self::FOREST,
            Self::FULL_DEPTH,
        );
        for i in 0..tiles {
            if !ramped[i] {
                // A small wood's heart still lies behind its edge.
                if woods.habitat[i] == Habitat::Heart && depth[i] < 3 {
                    woods.habitat[i] = Habitat::Wood;
                    woods.density[i] = woods.density[i].min(Self::HEART - 1);
                }
                continue;
            }
            let ramp = Self::thickening(depth[i]);
            woods.density[i] = woods.density[i].min(ramp).max(Self::FOREST);
            if woods.habitat[i] != Habitat::CliffFoot {
                woods.habitat[i] = if woods.density[i] >= Self::HEART {
                    Habitat::Heart
                } else {
                    Habitat::Wood
                };
            }
        }
        woods
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
    fn sweep(to: &mut [u8], w: usize, h: usize, cap: u8) {
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

    /// The round patches of `rule` for `purpose`: on a grid of
    /// `rule.cell` tiles in absolute coordinates, a cell holds one with
    /// `rule.chance` thousandths, its centre anywhere in the cell, where
    /// it `fits`; (x, y, radius) in thousandths of a tile. Each is decided
    /// by its own cell alone.
    fn patches(
        (width, height, cliff_rows): (u32, u32, u32),
        seed: u64,
        purpose: u64,
        rule: PatchRule,
        fits: impl Fn(u32, u32) -> bool,
    ) -> Vec<(i64, i64, i64)> {
        let mut out = Vec::new();
        let side = u64::from(rule.cell) * 1000;
        for cy in 0..=height / rule.cell {
            for cx in 0..=width / rule.cell {
                let keys = |n: u64| [key(i64::from(cx)), key(i64::from(cy)), n];
                if !scatter::chance(rule.chance, seed, purpose, &keys(0)) {
                    continue;
                }
                let x = u64::from(cx) * side + scatter::roll(seed, purpose, &keys(1)) % side;
                let y = u64::from(cy) * side + scatter::roll(seed, purpose, &keys(2)) % side;
                let (lo, hi) = rule.radius;
                let r =
                    u64::from(lo) + scatter::roll(seed, purpose, &keys(3)) % u64::from(hi - lo + 1);
                let (tx, ty) = (x / 1000, y / 1000);
                if tx >= u64::from(width) || ty >= u64::from(height) || ty < u64::from(cliff_rows) {
                    continue;
                }
                let (tx, ty) = (
                    u32::try_from(tx).expect("on the map"),
                    u32::try_from(ty).expect("on the map"),
                );
                if fits(tx, ty) {
                    let at = |v: u64| i64::try_from(v).expect("on the map");
                    out.push((at(x), at(y), at(r)));
                }
            }
        }
        out
    }

    /// Per tile, how many tiles (eight-way) to the nearest river (water or
    /// ford, not a brook), up to `RIVER_REACH + 1`: a distance only ever
    /// that far, worked out in two sweeps.
    fn to_river(width: u32, height: u32, rivers: &Rivers) -> Vec<u8> {
        let far = Self::RIVER_REACH + 1;
        let (w, h) = (width as usize, height as usize);
        let mut to = vec![far; w * h];
        for y in 0..height {
            for x in 0..width {
                if matches!(rivers.at(x, y), Some(Terrain::Water | Terrain::Ford)) {
                    to[y as usize * w + x as usize] = 0;
                }
            }
        }
        Self::sweep(&mut to, w, h, far);
        to
    }

    /// The density of trees at `(x, y)`, 0 to 100.
    #[must_use]
    pub fn density(&self, x: u32, y: u32) -> u8 {
        self.density[(y * self.width + x) as usize]
    }

    /// The habitat of `(x, y)`.
    #[must_use]
    pub fn habitat(&self, x: u32, y: u32) -> Habitat {
        self.habitat[(y * self.width + x) as usize]
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

/// What the woods are drawn from: the noises' seeds, the wetness, the
/// distance to the rivers, the round patches and the camp's place.
struct Plan {
    seed: u64,
    massif: u64,
    detail: u64,
    fine: u64,
    rim: u64,
    wet: Groves,
    cliff_rows: u32,
    width: u32,
    height: u32,
    /// Per tile, the tiles to the nearest river, up to its reach.
    to_river: Vec<u8>,
    woodlets: Patches,
    clearings: Patches,
    copses: Patches,
    camp: (i64, i64),
}

/// Round patches (clearings, small woods, copses: centre and radius,
/// thousandths of a tile), filed by the square cells of `PATCH_CELL`
/// tiles their ragged rims can reach: a tile looks only at those of its
/// own cell.
#[derive(Default)]
struct Patches {
    cells: Vec<Vec<(i64, i64, i64)>>,
    cells_wide: u32,
}

/// Side of a cell of `Patches`, in tiles.
const PATCH_CELL: u32 = 16;

impl Patches {
    fn new(patches: Vec<(i64, i64, i64)>, width: u32, height: u32) -> Self {
        let (wide, high) = (width.div_ceil(PATCH_CELL), height.div_ceil(PATCH_CELL));
        let mut cells = vec![Vec::new(); (wide * high) as usize];
        let cell = |mm: i64, last: u32| {
            u32::try_from(mm.max(0) / 1000).map_or(last, |t| (t / PATCH_CELL).min(last))
        };
        for (cx, cy, r) in patches {
            // The farthest a ragged rim reaches (`Plan::within`): the
            // radius times 1 + RAGGED / 2, and a tile more for safety.
            let reach = r * (1000 + Woods::RAGGED / 2 + 1) / 1000 + 1000;
            for y in cell(cy - reach, high - 1)..=cell(cy + reach, high - 1) {
                for x in cell(cx - reach, wide - 1)..=cell(cx + reach, wide - 1) {
                    cells[(y * wide + x) as usize].push((cx, cy, r));
                }
            }
        }
        Self {
            cells,
            cells_wide: wide,
        }
    }

    /// The patches that may reach the tile `(x, y)`.
    fn near(&self, x: u32, y: u32) -> &[(i64, i64, i64)] {
        self.cells
            .get(((y / PATCH_CELL) * self.cells_wide + x / PATCH_CELL) as usize)
            .map_or(&[], Vec::as_slice)
    }
}

impl Plan {
    /// How strongly the woods pull at `(x, y)`, in thousandths: the
    /// massifs' noise, roughened, wetter ground and the cliff's foot, the
    /// small woods (`with_woodlets`), less near a river.
    fn pull(&self, x: u32, y: u32, with_woodlets: bool) -> i64 {
        let (ix, iy) = (
            i32::try_from(x).expect("map fits in i32"),
            i32::try_from(y).expect("map fits in i32"),
        );
        let centred = |v: u32| i64::from(v) - 500;
        let mut m = i64::from(noise(self.massif, Woods::MASSIF_CELL, ix, iy))
            + Woods::DETAIL * centred(noise(self.detail, Woods::DETAIL_CELL, ix, iy)) / 1000
            + Woods::FINE * centred(noise(self.fine, Woods::FINE_CELL, ix, iy)) / 1000
            + Woods::WET * centred(self.wet.value(x, y)) / 1000;
        let below = i64::from(y) - i64::from(self.cliff_rows);
        if below < Woods::CLIFF_ROWS {
            m += Woods::CLIFF * (Woods::CLIFF_ROWS - below) / Woods::CLIFF_ROWS;
        }
        if with_woodlets {
            m += self.woodlet_pull(x, y);
        }
        let river = self.to_river[(y * self.width + x) as usize];
        if river <= Woods::RIVER_REACH {
            m -= Woods::RIVER_OPEN * i64::from(Woods::RIVER_REACH - river)
                / i64::from(Woods::RIVER_REACH);
        }
        m
    }

    /// What the small woods add to the pull at `(x, y)`.
    fn woodlet_pull(&self, x: u32, y: u32) -> i64 {
        Woods::WOODLET_PULL * self.within(self.woodlets.near(x, y), x, y) / 1000
    }

    /// How deep inside one of `patches` `(x, y)` is: 1000 at a centre, 0
    /// at and past a rim made ragged by a fine noise.
    fn within(&self, patches: &[(i64, i64, i64)], x: u32, y: u32) -> i64 {
        if patches.is_empty() {
            return 0;
        }
        let (px, py) = (i64::from(x) * 1000, i64::from(y) * 1000);
        let rim = noise(
            self.rim,
            Woods::RAGGED_CELL,
            i32::try_from(x).expect("map fits in i32"),
            i32::try_from(y).expect("map fits in i32"),
        );
        let ragged = 1000 + Woods::RAGGED * (i64::from(rim) - 500) / 1000;
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

    /// A chance of `per_mille` for the tile `(x, y)` and `purpose`.
    fn chance(&self, per_mille: u32, purpose: u64, x: u32, y: u32) -> bool {
        scatter::chance(per_mille, self.seed, purpose, &[u64::from(x), u64::from(y)])
    }

    /// Whether a brook runs one or two tiles from `(x, y)`, eight-way.
    fn by_a_brook(&self, rivers: &Rivers, x: u32, y: u32) -> bool {
        (-2_i64..=2).any(|dy| {
            (-2_i64..=2).any(|dx| {
                let (nx, ny) = (i64::from(x) + dx, i64::from(y) + dy);
                u32::try_from(nx).is_ok_and(|nx| {
                    u32::try_from(ny).is_ok_and(|ny| {
                        nx < self.width
                            && ny < self.height
                            && rivers.at(nx, ny) == Some(Terrain::Brook)
                    })
                })
            })
        })
    }

    /// The density (thousandths) and habitat of the land tile `(x, y)`,
    /// and whether its trees thicken with its depth in the forest (not in
    /// a small wood, which keeps its quicker rise).
    fn tile(&self, x: u32, y: u32, rivers: &Rivers) -> (i64, Habitat, bool) {
        let pull = self.pull(x, y, true);
        let in_a_woodlet = pull - self.woodlet_pull(x, y) < Woods::LEVEL;
        let mut d = ((pull - Woods::LEVEL) * 1000 / Woods::EDGE).clamp(0, 1000);
        let mut habitat = if d >= i64::from(Woods::HEART) * 10 {
            Habitat::Heart
        } else if d >= i64::from(Woods::FOREST) * 10 {
            Habitat::Wood
        } else if d > 0 {
            Habitat::Edge
        } else {
            Habitat::Meadow
        };
        // Clearings: the woods open, with an edge around.
        if d > 0 {
            let open = self.within(self.clearings.near(x, y), x, y);
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
        if from_camp < Woods::CAMP_RADIUS {
            let back = (from_camp - (Woods::CAMP_RADIUS - Woods::CAMP_RIM)).max(0);
            d = d * back / Woods::CAMP_RIM;
            if d < 100 {
                d = 0;
                habitat = Habitat::Camp;
            }
        }
        // The wood at the cliff's foot is its own place.
        let below = i64::from(y) - i64::from(self.cliff_rows);
        if below < Woods::CLIFF_ROWS
            && matches!(habitat, Habitat::Edge | Habitat::Wood | Habitat::Heart)
        {
            habitat = Habitat::CliffFoot;
        }
        // Copses in the meadow, tight.
        if habitat == Habitat::Meadow {
            let copse = self.within(self.copses.near(x, y), x, y);
            if copse > 0 {
                d = 800 + 200 * copse / 1000;
                habitat = Habitat::Copse;
            }
        }
        // Riverside trees, sparse, where the bank is open; lines of trees
        // along the brooks.
        let river = self.to_river[(y * self.width + x) as usize];
        if matches!(habitat, Habitat::Meadow | Habitat::Edge | Habitat::Copse) {
            let chance = match river {
                2 => Woods::RIVERSIDE.0,
                3 => Woods::RIVERSIDE.1,
                _ => 0,
            };
            if chance > 0 && self.chance(chance, purpose::WOODS_BANK, x, y) {
                d = d.max(300);
                habitat = Habitat::Riverside;
            }
        }
        if habitat == Habitat::Meadow
            && self.by_a_brook(rivers, x, y)
            && self.chance(Woods::BROOKSIDE, purpose::WOODS_BROOK, x, y)
        {
            d = 300;
            habitat = Habitat::Riverside;
        }
        let ramped =
            !in_a_woodlet && matches!(habitat, Habitat::Wood | Habitat::Heart | Habitat::CliffFoot);
        (d, habitat, ramped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Map, Pos};

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
    fn a_wild_valley_is_mostly_forest_with_meadows_along_its_rivers() {
        const SIZE: i32 = 512;
        let (mut land_all, mut forest_all) = (0, 0);
        let mut habitats = std::collections::BTreeSet::new();
        let (mut riverside_maps, mut brook_lines) = (0, 0);
        let (mut by_river, mut by_river_open, mut away, mut away_open) =
            (0_u64, 0_u64, 0_u64, 0_u64);
        for seed in 1..=4 {
            let map = Map::valley(512, 512, seed);
            let (seen, land, forest) = tally(&map, SIZE);
            land_all += land;
            forest_all += forest;
            habitats.extend(seen.keys().copied());
            riverside_maps += usize::from(seen.contains_key(&Habitat::Riverside));
            let near = |x: i32, y: i32, reach: i32, what: &dyn Fn(Pos) -> bool| {
                (-reach..=reach).any(|dy| (-reach..=reach).any(|dx| what(Pos::new(x + dx, y + dy))))
            };
            let river = |p: Pos| matches!(map.get(p), Some(Terrain::Water | Terrain::Ford));
            let brook = |p: Pos| map.get(p) == Some(Terrain::Brook);
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let habitat = map.habitat(Pos::new(x, y));
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
                        // Trees by the water stand on the bank of a river
                        // (two or three tiles off) or of a brook, never
                        // beside a river.
                        Habitat::Riverside => {
                            assert!(
                                (near(x, y, 3, &river) && !near(x, y, 1, &river))
                                    || near(x, y, 2, &brook),
                                "seed {seed}: riverside tree at ({x}, {y})"
                            );
                            brook_lines +=
                                usize::from(near(x, y, 2, &brook) && !near(x, y, 3, &river));
                        }
                        _ => {}
                    }
                    // Open land near a river, against open land away from
                    // it: the meadows follow the rivers.
                    if habitat != Habitat::None {
                        let open =
                            u64::from(matches!(habitat, Habitat::Meadow | Habitat::Clearing));
                        if near(x, y, 24, &river) {
                            by_river += 1;
                            by_river_open += open;
                        } else {
                            away += 1;
                            away_open += open;
                        }
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
        assert!(brook_lines > 50, "{brook_lines} trees along the brooks");
        let (near_river, far) = (
            by_river_open * 1000 / by_river.max(1),
            away_open * 1000 / away.max(1),
        );
        eprintln!("open land: {near_river} per mille by the rivers, {far} away");
        // With more open land between the woods (55 % forest), the rivers'
        // banks are still far more open than the rest.
        assert!(
            near_river * 10 > far * 17,
            "{near_river} by the rivers, {far} away"
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
            let depth = Woods::depth(512, 512, |i| !forest(i), 64);
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
        let small = Woods::patches((256, 256, 0), 9, purpose::WOODS_COPSE, rule, |_, _| true);
        let large = Woods::patches((768, 512, 0), 9, purpose::WOODS_COPSE, rule, |_, _| true);
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
