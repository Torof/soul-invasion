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
use crate::map::{FinePos, Groves, Lcg, MM_PER_TILE, Map, Pos};
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
const STAND_CELL: u32 = 10;
/// Added to a stand's strength (0 to 1000) before it is squared.
const STAND_FLOOR: u64 = 50;

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
    for y in 0..height {
        for x in 0..width {
            let pos = Pos::new(
                i32::try_from(x).expect("map fits in i32"),
                i32::try_from(y).expect("map fits in i32"),
            );
            let habitat = map.habitat(pos);
            let chance = match habitat {
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
            }
            // Every lone and riverside tile has its tree; the heart holds
            // one tile in three or so.
            let (mut heart, mut heart_trees) = (0, 0);
            for y in 0..256 {
                for x in 0..256 {
                    let pos = Pos::new(x, y);
                    match map.habitat(pos) {
                        Habitat::Lone | Habitat::Riverside => {
                            assert!(
                                at.contains_key(&pos)
                                    || kinds
                                        .iter()
                                        .all(|k| !k.habitats.contains_key(&map.habitat(pos)))
                            );
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
            assert!(
                (200..=400).contains(&share),
                "seed {seed}: {share} per mille of the heart"
            );
            // Stands: most trees share their kind with the trees around.
            for (pos, kind) in &at {
                let around: Vec<usize> = (-2..=2)
                    .flat_map(|dy| (-2..=2).map(move |dx| Pos::new(pos.x + dx, pos.y + dy)))
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
        // Grouped by kind, yet mixed: about one neighbour in four of
        // another kind, not one in two.
        assert!((180..=330).contains(&share), "{share} per mille mixed");
    }
}
