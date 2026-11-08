//! Where the trees grow (D37): a density of trees per tile and the kind of
//! place each tile is, generated with the valley and never stored.
//!
//! - **Massifs**: a broad noise (64 tiles), roughened by two finer ones,
//!   thickened a little where the ground is wet and in a band at the
//!   cliff's foot. The level that makes a tile wooded is chosen per map, so
//!   the massifs and their edges always cover `MASSIF_SHARE` of the land:
//!   with so few massifs a map, a fixed level gave anything from a third
//!   to three quarters.
//! - **Edge, wood, heart**: the density rises from the massif's edge
//!   inwards over a few tiles; the heart is the densest.
//! - **Clearings** open in the woods, and a **camp clearing** around the
//!   place the first bodies start.
//! - **Copses** and **lone trees** dot the meadow; **riverside** trees
//!   line the rivers, sparse.
//!
//! A tile is [`Terrain::Forest`] where the density reaches `FOREST`: the
//! wood and the heart. The edge, copses' rims, lone and riverside trees
//! stand on grass. Which species grows where, grouped in stands that mix
//! at their borders, comes with the trees themselves.
//!
//! All arithmetic is in integers (thousandths), so a seed gives the same
//! woods everywhere.

use crate::map::{Groves, Lcg, Rivers, Terrain};

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
    /// Share of the land the massifs cover, edges included, in thousandths.
    pub const MASSIF_SHARE: u32 = 320;
    /// From this density up, a tile is forest: the wood and the heart.
    pub const FOREST: u8 = 35;
    /// From this density up, the heart of a massif.
    pub const HEART: u8 = 75;
    /// Lattice of the massifs' noise, then of the two finer ones that
    /// roughen their outline, in tiles.
    const MASSIF_CELL: u32 = 64;
    const DETAIL_CELL: u32 = 16;
    const FINE_CELL: u32 = 5;
    /// Weight of the finer noises and of the wet ground, in thousandths.
    const DETAIL: i64 = 220;
    const FINE: i64 = 80;
    const WET: i64 = 150;
    /// Rows of wood at the cliff's foot, and their pull at the foot.
    const CLIFF_ROWS: i64 = 10;
    const CLIFF: i64 = 200;
    /// How far above the level the noise climbs from the edge of a massif
    /// to its full density, in thousandths: the width of the edge.
    const EDGE: i64 = 160;
    /// Clearings in the woods, and their radius in thousandths of a tile.
    const CLEARINGS: u32 = 10;
    const CLEARING_RADIUS: (u32, u32) = (3000, 7000);
    /// Radius of the camp clearing, and the rim over which trees come back.
    const CAMP_RADIUS: i64 = 9000;
    const CAMP_RIM: i64 = 3000;
    /// Copses in the meadow, and their radius.
    const COPSES: u32 = 30;
    const COPSE_RADIUS: (u32, u32) = (1500, 4000);
    /// Chance of a lone tree on a meadow tile, in thousandths.
    const LONE: u32 = 4;
    /// Chance of a riverside tree two tiles from a river, then three, in
    /// thousandths: never on the tile beside the water, so a tree and its
    /// crown stand on the bank, not in the river.
    const RIVERSIDE: (u32, u32) = (250, 150);
    /// How ragged a round patch's rim is, in thousandths of its radius.
    const RAGGED: i64 = 350;

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
        let mut plan = Plan {
            massif: Groves::with_cell(width, height, seed ^ 0x00f0_4e57, Self::MASSIF_CELL),
            detail: Groves::with_cell(width, height, seed ^ 0x00de_7a11, Self::DETAIL_CELL),
            fine: Groves::with_cell(width, height, seed ^ 0x0000_f17e, Self::FINE_CELL),
            rim: Groves::with_cell(width, height, seed ^ 0x001a_99ed, 2),
            // The same wetness the brooks rise from.
            wet: Groves::with_cell(width, height, seed ^ 0x77e7, 32),
            cliff_rows,
            width,
            pulls: Vec::new(),
            level: 0,
            clearings: Vec::new(),
            copses: Vec::new(),
            near_river: Self::near_river(width, height, rivers),
            camp: (i64::from(camp.0) * 1000, i64::from(camp.1) * 1000),
        };
        // The pull of every tile, once; the level: the massifs cover
        // MASSIF_SHARE of the land.
        plan.pulls = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| plan.pull(x, y))
            .collect();
        let mut pulls: Vec<i64> = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .filter(|(x, y)| land(*x, *y))
            .map(|(x, y)| plan.pulls[(y * width + x) as usize])
            .collect();
        if pulls.is_empty() {
            return woods;
        }
        pulls.sort_unstable();
        let above = pulls.len() * Self::MASSIF_SHARE as usize / 1000;
        plan.level = pulls[pulls.len() - 1 - above.min(pulls.len() - 1)];
        let mut rng = Lcg::new(seed ^ 0x0a4_7eee);
        let inside = |x: u32, y: u32| land(x, y) && plan.inside(x, y);
        let outside = |x: u32, y: u32| land(x, y) && !plan.inside(x, y);
        let clearings = Self::patches(
            (width, height, cliff_rows),
            &mut rng,
            Self::CLEARINGS,
            Self::CLEARING_RADIUS,
            inside,
        );
        let copses = Self::patches(
            (width, height, cliff_rows),
            &mut rng,
            Self::COPSES,
            Self::COPSE_RADIUS,
            outside,
        );
        plan.clearings = clearings;
        plan.copses = copses;
        for y in 0..height {
            for x in 0..width {
                if land(x, y) {
                    let i = (y * width + x) as usize;
                    let (d, habitat) = plan.tile(x, y, i, &mut rng);
                    woods.density[i] = u8::try_from(d / 10).expect("percent");
                    woods.habitat[i] = habitat;
                }
            }
        }
        woods
    }

    /// Up to `count` round patches, centred where `fits`, of a radius in
    /// `radius` (thousandths): (x, y, radius) in thousandths.
    fn patches(
        (width, height, cliff_rows): (u32, u32, u32),
        rng: &mut Lcg,
        count: u32,
        (smallest, largest): (u32, u32),
        fits: impl Fn(u32, u32) -> bool,
    ) -> Vec<(i64, i64, i64)> {
        let mut out = Vec::new();
        for _ in 0..count * 100 {
            if out.len() >= count as usize {
                break;
            }
            let x = rng.next_below(width * 1000);
            let y = cliff_rows * 1000 + rng.next_below((height - cliff_rows) * 1000);
            let radius = smallest + rng.next_below(largest - smallest + 1);
            if fits(x / 1000, y / 1000) {
                out.push((i64::from(x), i64::from(y), i64::from(radius)));
            }
        }
        out
    }

    /// Per tile, 1 to 3 when a river (water or ford, not a brook) is that
    /// many tiles away, eight-way, else 0.
    fn near_river(width: u32, height: u32, rivers: &Rivers) -> Vec<u8> {
        let mut near = vec![0_u8; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                if !matches!(rivers.at(x, y), Some(Terrain::Water | Terrain::Ford)) {
                    continue;
                }
                for dy in -3_i64..=3 {
                    for dx in -3_i64..=3 {
                        let (nx, ny) = (i64::from(x) + dx, i64::from(y) + dy);
                        if nx < 0 || ny < 0 || nx >= i64::from(width) || ny >= i64::from(height) {
                            continue;
                        }
                        let i = usize::try_from(ny * i64::from(width) + nx).expect("inside");
                        let ring = u8::try_from(dx.abs().max(dy.abs())).expect("small");
                        if ring > 0 && (near[i] == 0 || near[i] > ring) {
                            near[i] = ring;
                        }
                    }
                }
            }
        }
        near
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

/// What the woods are drawn from: the noises, the level that makes a
/// tile wooded, the round patches, and the rivers' and camp's places.
struct Plan {
    massif: Groves,
    detail: Groves,
    fine: Groves,
    rim: Groves,
    wet: Groves,
    cliff_rows: u32,
    width: u32,
    /// Per tile, its pull, once computed.
    pulls: Vec<i64>,
    level: i64,
    clearings: Vec<(i64, i64, i64)>,
    copses: Vec<(i64, i64, i64)>,
    near_river: Vec<u8>,
    camp: (i64, i64),
}

impl Plan {
    /// How strongly the woods pull at `(x, y)`, in thousandths: the
    /// massifs' noise, roughened, wetter ground and the cliff's foot.
    fn pull(&self, x: u32, y: u32) -> i64 {
        let centred = |v: u32| i64::from(v) - 500;
        let mut m = i64::from(self.massif.smooth(x, y))
            + Woods::DETAIL * centred(self.detail.smooth(x, y)) / 1000
            + Woods::FINE * centred(self.fine.smooth(x, y)) / 1000
            + Woods::WET * centred(self.wet.value(x, y)) / 1000;
        let below = i64::from(y) - i64::from(self.cliff_rows);
        if below < Woods::CLIFF_ROWS {
            m += Woods::CLIFF * (Woods::CLIFF_ROWS - below) / Woods::CLIFF_ROWS;
        }
        m
    }

    /// Well inside a massif, past half its edge.
    fn inside(&self, x: u32, y: u32) -> bool {
        self.pulls[(y * self.width + x) as usize] > self.level + Woods::EDGE / 2
    }

    /// How deep inside one of `patches` `(x, y)` is: 1000 at a centre, 0
    /// at and past a rim made ragged by a fine noise.
    fn within(&self, patches: &[(i64, i64, i64)], x: u32, y: u32) -> i64 {
        let (px, py) = (i64::from(x) * 1000, i64::from(y) * 1000);
        let ragged = 1000 + Woods::RAGGED * (i64::from(self.rim.value(x, y)) - 500) / 1000;
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
    /// index `i`.
    fn tile(&self, x: u32, y: u32, i: usize, rng: &mut Lcg) -> (i64, Habitat) {
        let mut d = ((self.pulls[i] - self.level) * 1000 / Woods::EDGE).clamp(0, 1000);
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
            let open = self.within(&self.clearings, x, y);
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
        // Copses and lone trees in the meadow.
        if habitat == Habitat::Meadow {
            let copse = self.within(&self.copses, x, y);
            if copse > 0 {
                d = 300 + 400 * copse / 1000;
                habitat = Habitat::Copse;
            } else if rng.next_below(1000) < Woods::LONE {
                d = 200;
                habitat = Habitat::Lone;
            }
        }
        // Riverside trees, sparse, where the bank is open.
        let open_bank = matches!(
            habitat,
            Habitat::Meadow | Habitat::Edge | Habitat::Lone | Habitat::Copse
        );
        if open_bank && self.near_river[i] > 0 {
            let chance = match self.near_river[i] {
                2 => Woods::RIVERSIDE.0,
                3 => Woods::RIVERSIDE.1,
                _ => 0,
            };
            if rng.next_below(1000) < chance {
                d = d.max(300);
                habitat = Habitat::Riverside;
            }
        }
        (d, habitat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Map, Pos};

    /// Over six valleys: each habitat's tiles, the land's, the forest's.
    fn tally(map: &Map) -> (std::collections::BTreeMap<Habitat, usize>, usize, usize) {
        let mut seen = std::collections::BTreeMap::new();
        let (mut land, mut forest) = (0, 0);
        for y in 32..256 {
            for x in 0..256 {
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
    fn the_woods_come_in_massifs_with_edges_clearings_and_copses() {
        let mut riverside_maps = 0;
        for seed in 1..=6 {
            let map = Map::valley(256, 256, seed);
            let (seen, land, forest) = tally(&map);
            let wooded: usize = seen
                .iter()
                .filter(|(h, _)| {
                    !matches!(
                        h,
                        Habitat::Meadow | Habitat::Clearing | Habitat::Camp | Habitat::None
                    )
                })
                .map(|(_, n)| n)
                .sum();
            // A third of the land wooded, a fifth or so forest proper.
            assert!(
                (300..=400).contains(&(wooded * 1000 / land)),
                "seed {seed}: {wooded} of {land}"
            );
            assert!(
                (120..=300).contains(&(forest * 1000 / land)),
                "seed {seed}: forest {forest} of {land}"
            );
            for habitat in [
                Habitat::Heart,
                Habitat::Wood,
                Habitat::Edge,
                Habitat::Clearing,
                Habitat::Camp,
                Habitat::Copse,
                Habitat::Lone,
            ] {
                assert!(seen.contains_key(&habitat), "seed {seed}: no {habitat:?}");
            }
            riverside_maps += usize::from(seen.contains_key(&Habitat::Riverside));
            let near = |x: i32, y: i32, reach: i32, what: &dyn Fn(Pos) -> bool| {
                (-reach..=reach).any(|dy| (-reach..=reach).any(|dx| what(Pos::new(x + dx, y + dy))))
            };
            for y in 32..256 {
                for x in 0..256 {
                    match map.habitat(Pos::new(x, y)) {
                        // The heart lies behind an edge: no open meadow
                        // within two tiles of it.
                        Habitat::Heart => assert!(
                            !near(x, y, 2, &|p| map.habitat(p) == Habitat::Meadow),
                            "seed {seed}: heart at ({x}, {y}) touches the meadow"
                        ),
                        // A clearing opens in the woods.
                        Habitat::Clearing => assert!(
                            near(x, y, 8, &|p| map.trees(p) >= Woods::FOREST),
                            "seed {seed}: clearing at ({x}, {y}) far from any wood"
                        ),
                        // Riverside trees stand by a river, on the bank:
                        // two or three tiles from the water, never beside it.
                        Habitat::Riverside => {
                            let river =
                                |p: Pos| matches!(map.get(p), Some(Terrain::Water | Terrain::Ford));
                            assert!(
                                near(x, y, 3, &river) && !near(x, y, 1, &river),
                                "seed {seed}: riverside tree at ({x}, {y})"
                            );
                        }
                        _ => {}
                    }
                }
            }
            // The camp is open: no tree within six tiles of it.
            let camp = map.camp();
            assert!(
                !near(camp.x, camp.y, 4, &|p| map.trees(p) > 0),
                "seed {seed}: trees at the camp"
            );
        }
        assert!(
            riverside_maps >= 5,
            "riverside trees on {riverside_maps} maps"
        );
    }
}
