//! The trees standing in the woods (D37): planted from the density of
//! trees and the habitat of every tile, with the kinds of tree of the
//! content. Regenerated from the map and the content, never stored; a
//! felled tree will be stored when felling comes.
//!
//! - A tile holds a tree with a chance of `CHANCE` percent of its
//!   density (a heart at 100 holds one tile in three or so); a lone or
//!   riverside tile always holds its tree.
//! - The tree stands at a fine point up to `JITTER` from the tile's
//!   centre, so the woods do not show the grid.
//! - Its species is drawn from the species growing in that habitat, each
//!   weighed by its habitat weight times the square of its **stand** at
//!   the place: a smooth noise per species, about `STAND_CELL` tiles
//!   across. Trees of one kind come in groups that mix at their borders.

use serde::{Deserialize, Serialize};

use crate::content::Species;
use crate::map::{FinePos, Groves, Lcg, MM_PER_TILE, Map, Pos, Terrain};
use crate::woods::Habitat;

/// A tree standing on the ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tree {
    /// Where its trunk meets the ground.
    pub at: FinePos,
    /// Its kind: an index into the content's species.
    pub species: usize,
}

/// Percent of a tile's density (0 to 100) that a tree stands there: a
/// heart at 100 holds a tree with a chance of 35 %.
const CHANCE: u32 = 35;
/// Farthest a tree stands from its tile's centre, in millimetres.
const JITTER: u32 = 300;
/// Width of a species' stands, in tiles.
const STAND_CELL: u32 = 20;
/// Added to a stand's strength (0 to 1000) before it is squared.
const STAND_FLOOR: u64 = 50;
/// A tree's crown, as a share of the height it is drawn at (D40): its
/// radius, in thousandths.
const CROWN: u32 = 300;
/// How much two crowns may overlap: trees keep this share of their two
/// radii apart, in thousandths (a wood closes, a stand still shows its
/// trees).
const SPACING: u32 = 750;

/// The height a tree `metres` tall is drawn at (the client's
/// `Scale.shown`, D40): true under 3 m, eased above.
fn shown_m(metres: f32) -> f32 {
    if metres <= 3.0 {
        metres
    } else {
        3.0 * (metres / 3.0).powf(0.7)
    }
}

/// The radius a tree of `species` keeps clear of others' trunks, in
/// millimetres.
fn room_mm(species: &Species) -> i64 {
    #[allow(clippy::cast_possible_truncation)]
    let crown =
        (shown_m(species.height_m) * f32::from(u16::try_from(CROWN).expect("small"))) as i64;
    crown * i64::from(SPACING) / 1000
}

/// The trees of `map`, of the kinds in `species`, for `seed`.
#[must_use]
pub fn plant(map: &Map, species: &[Species], seed: u64) -> Vec<Tree> {
    let (width, height) = (map.width(), map.height());
    let stands: Vec<Groves> = (0..species.len())
        .map(|i| {
            Groves::with_cell(
                width,
                height,
                seed ^ (0x5_7a4d + i as u64 * 7919),
                STAND_CELL,
            )
        })
        .collect();
    let mut rng = Lcg::new(seed ^ 0x7_4ee5);
    let mut trees = Vec::new();
    // Trees by tile, to keep each one's room (D40: bigger trees, fewer).
    let rooms: Vec<i64> = species.iter().map(room_mm).collect();
    let widest = rooms.iter().copied().max().unwrap_or(0);
    let reach = i32::try_from(2 * widest / i64::from(MM_PER_TILE) + 1).expect("small");
    let mut near: std::collections::HashMap<(i32, i32), Vec<usize>> =
        std::collections::HashMap::new();
    for y in 0..height {
        for x in 0..width {
            let pos = Pos::new(
                i32::try_from(x).expect("map fits in i32"),
                i32::try_from(y).expect("map fits in i32"),
            );
            let habitat = map.habitat(pos);
            // Nothing stands right beside a river or a ford: its trunk and
            // crown would be drawn over the water.
            let by_the_water = (-1..=1).any(|dy| {
                (-1..=1).any(|dx| {
                    matches!(
                        map.get(Pos::new(pos.x + dx, pos.y + dy)),
                        Some(Terrain::Water | Terrain::Ford)
                    )
                })
            });
            let chance = match habitat {
                _ if by_the_water => 0,
                Habitat::Lone | Habitat::Riverside => 1000,
                _ => u32::from(map.trees(pos)) * CHANCE / 10,
            };
            if chance == 0 || rng.next_below(1000) >= chance {
                continue;
            }
            let weights: Vec<u64> = species
                .iter()
                .zip(&stands)
                .map(|(s, stand)| {
                    // Never quite nothing: a species that grows here may
                    // stand anywhere, only rarely outside its stands.
                    let strength = u64::from(stand.smooth(x, y)) + STAND_FLOOR;
                    u64::from(s.habitats.get(&habitat).copied().unwrap_or(0)) * strength * strength
                })
                .collect();
            let total: u64 = weights.iter().sum();
            if total == 0 {
                continue;
            }
            let mut pick = u64::from(rng.next_below(u32::MAX)) * total / u64::from(u32::MAX);
            let kind = weights
                .iter()
                .position(|w| {
                    if pick < *w {
                        true
                    } else {
                        pick -= w;
                        false
                    }
                })
                .unwrap_or(weights.len() - 1);
            let jitter = |rng: &mut Lcg| {
                i32::try_from(rng.next_below(2 * JITTER + 1)).expect("small")
                    - i32::try_from(JITTER).expect("small")
            };
            let at = FinePos::new(
                pos.x * MM_PER_TILE + MM_PER_TILE / 2 + jitter(&mut rng),
                pos.y * MM_PER_TILE + MM_PER_TILE / 2 + jitter(&mut rng),
            );
            // Room: no other trunk within the two trees' rooms.
            let crowded = (pos.y - reach..=pos.y + reach).any(|ty| {
                (pos.x - reach..=pos.x + reach).any(|tx| {
                    near.get(&(tx, ty)).is_some_and(|list| {
                        list.iter().any(|&i| {
                            let other: &Tree = &trees[i];
                            let (dx, dy) =
                                (i64::from(other.at.x - at.x), i64::from(other.at.y - at.y));
                            let room = rooms[kind] + rooms[other.species];
                            dx * dx + dy * dy < room * room
                        })
                    })
                })
            });
            if crowded {
                continue;
            }
            near.entry((pos.x, pos.y)).or_default().push(trees.len());
            trees.push(Tree { at, species: kind });
        }
    }
    trees
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kinds of tree of the repository's `content/species.ron`.
    const SPECIES: &str = include_str!("../../../content/species.ron");

    fn species() -> Vec<Species> {
        ron::from_str(SPECIES).expect("species.ron parses")
    }

    #[test]
    fn trees_stand_where_the_woods_say_in_stands_that_mix() {
        let kinds = species();
        let (mut trees_total, mut lonely, mut mixed) = (0, 0, 0);
        for seed in 1..=4 {
            let map = Map::valley(256, 256, seed);
            let trees = plant(&map, &kinds, seed);
            assert_eq!(
                trees,
                plant(&map, &kinds, seed),
                "the same trees every time"
            );
            let mut at: std::collections::BTreeMap<Pos, usize> = std::collections::BTreeMap::new();
            for tree in &trees {
                let tile = tree.at.tile();
                // Within its tile, near the centre.
                let off = (
                    tree.at.x - tile.x * MM_PER_TILE - 500,
                    tree.at.y - tile.y * MM_PER_TILE - 500,
                );
                assert!(off.0.unsigned_abs() <= JITTER && off.1.unsigned_abs() <= JITTER);
                // Only where trees grow, and of a kind that grows there.
                let habitat = map.habitat(tile);
                assert!(
                    !matches!(
                        habitat,
                        Habitat::Meadow | Habitat::Clearing | Habitat::Camp | Habitat::None
                    ),
                    "seed {seed}: a tree in the {habitat:?} at {tile:?}"
                );
                assert!(
                    kinds[tree.species]
                        .habitats
                        .get(&habitat)
                        .copied()
                        .unwrap_or(0)
                        > 0
                );
                assert!(at.insert(tile, tree.species).is_none(), "one tree a tile");
                // Never beside the water.
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let near = map.get(Pos::new(tile.x + dx, tile.y + dy));
                        assert!(
                            !matches!(near, Some(Terrain::Water | Terrain::Ford)),
                            "seed {seed}: a tree at {tile:?} beside the water"
                        );
                    }
                }
            }
            // Every lone and riverside tile has its tree, or one near it
            // (trees keep their room, D40); the heart holds one tile in
            // twenty or so: big trees, fewer.
            let (mut heart, mut heart_trees) = (0, 0);
            for y in 0..256 {
                for x in 0..256 {
                    let pos = Pos::new(x, y);
                    match map.habitat(pos) {
                        Habitat::Lone | Habitat::Riverside => {
                            let by_the_water = (-1..=1).any(|dy| {
                                (-1..=1).any(|dx| {
                                    matches!(
                                        map.get(Pos::new(x + dx, y + dy)),
                                        Some(Terrain::Water | Terrain::Ford)
                                    )
                                })
                            });
                            let one_near = (-5..=5).any(|dy| {
                                (-5..=5).any(|dx| at.contains_key(&Pos::new(x + dx, y + dy)))
                            });
                            assert!(one_near || by_the_water, "seed {seed}: {pos:?}");
                        }
                        Habitat::Heart => {
                            heart += 1;
                            heart_trees += usize::from(at.contains_key(&pos));
                        }
                        _ => {}
                    }
                }
            }
            let share = heart_trees * 1000 / heart.max(1);
            eprintln!("seed {seed}: {share} per mille of the heart");
            assert!(
                (20..=120).contains(&share),
                "seed {seed}: {share} per mille of the heart"
            );
            // Stands: most trees share their kind with the trees around.
            for (pos, kind) in &at {
                // Within six tiles: trees stand apart now (D40).
                let around: Vec<usize> = (-6..=6)
                    .flat_map(|dy| (-6..=6).map(move |dx| Pos::new(pos.x + dx, pos.y + dy)))
                    .filter(|p| p != pos)
                    .filter_map(|p| at.get(&p).copied())
                    .collect();
                if around.is_empty() {
                    lonely += 1;
                    continue;
                }
                // Thousandths of its neighbours of another kind.
                let other = around.iter().filter(|k| *k != kind).count();
                mixed += other * 1000 / around.len();
            }
            trees_total += trees.len();
        }
        let grouped = trees_total - lonely;
        let share = mixed / grouped;
        eprintln!(
            "{trees_total} trees on four maps; {share} per mille of their neighbours of another kind"
        );
        // Grouped by kind, yet mixed: about a third of the neighbours of
        // another kind, not one in two.
        assert!((150..=400).contains(&share), "{share} per mille mixed");
    }
}
