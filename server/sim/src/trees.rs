//! The trees standing in the woods (D37): placed from the density of
//! trees and the habitat of every tile, with the kinds of tree of the
//! content, **chunk by chunk** ([`trees_in`]): a pure function of the
//! chunk, the map and the seed, the same in any order, nothing stored and
//! nothing planted for the whole map. A felled tree will be stored when
//! felling comes, and laid over these.
//!
//! - **The woods** stand on a jittered grid (`gen::scatter`):
//!   cells of `CELL` a side, every other row shifted by half a cell, so
//!   the trees show no rows along the view's diagonals. A cell holds one
//!   tree at most, nudged up to `JITTER` off its centre, with a chance
//!   set by the density of the tile it lands on, never more than
//!   `MOST_CHANCE`: even a heart has cells left empty. A nested cell of
//!   `GAP_CELLS` × `GAP_CELLS` cells is sometimes left empty whole, a
//!   small opening in the canopy. No tree looks at another: they keep
//!   `CELL - 2 × JITTER` apart by construction.
//! - **Lone and riverside** tiles hold their own tree, as before.
//! - Nothing stands on a tile touching a river or a ford, even by a
//!   corner: its trunk and crown would be drawn over the water.
//! - A tree's species is drawn from the species growing in its habitat,
//!   each weighed by its habitat weight times the square of its
//!   **stand** at the place: a smooth noise per species, about
//!   `STAND_CELL` tiles across, worked out at the point (nothing held for
//!   the map). Trees of one kind come in groups that mix at their
//!   borders.

use serde::{Deserialize, Serialize};

use crate::content::Species;
use crate::r#gen::noise;
use crate::r#gen::scatter::{self, Grid, key, purpose};
use crate::map::{CHUNK_SIZE, ChunkPos, FinePos, MM_PER_TILE, Map, Pos, Terrain};
use crate::woods::Habitat;

/// A tree standing on the ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tree {
    /// Where its trunk meets the ground.
    pub at: FinePos,
    /// Its kind: an index into the content's species.
    pub species: usize,
}

/// Side of the woods' grid, in millimetres: one tree at most a cell.
const CELL: i32 = 4 * MM_PER_TILE;
/// Farthest a tree of the woods stands from its cell's centre, on each
/// axis, in millimetres: 35 % of a cell, so trees keep 1.2 m apart.
const JITTER: i32 = 1400;
/// The woods' grid.
const GRID: Grid = Grid {
    cell: CELL,
    jitter: JITTER,
    offset_rows: true,
};
/// A cell holds a tree with a chance of its tile's density (0 to 100)
/// times this, in thousandths, but never more than `MOST_CHANCE`: a
/// heart leaves about one cell in ten empty.
const CHANCE_PER_DENSITY: u32 = 11;
const MOST_CHANCE: u32 = 900;
/// The openings: a nested cell of `GAP_CELLS` × `GAP_CELLS` cells (12 m)
/// is left empty with a chance of `GAP_CHANCE` thousandths.
const GAP_CELLS: i64 = 3;
const GAP_CHANCE: u32 = 50;
/// Farthest a lone or riverside tree stands from its tile's centre, in
/// millimetres.
const TILE_JITTER: i32 = 300;
/// Width of a species' stands, in tiles.
const STAND_CELL: u32 = 20;
/// Added to a stand's strength (0 to 1000) before it is squared.
const STAND_FLOOR: u64 = 50;

/// The trees of `chunk` on `map`, of the kinds in `species`, for `seed`:
/// those whose trunk stands in the chunk, the woods' first (cell by
/// cell, row by row), then the lone and riverside ones (tile by tile).
/// Reads the map one tile around the chunk.
#[must_use]
pub fn trees_in(map: &Map, species: &[Species], seed: u64, chunk: ChunkPos) -> Vec<Tree> {
    let origin = chunk.origin();
    let side = i32::try_from(CHUNK_SIZE).expect("small chunk");
    let inside = |tile: Pos| {
        (origin.x..origin.x + side).contains(&tile.x)
            && (origin.y..origin.y + side).contains(&tile.y)
    };
    let mut trees = Vec::new();
    // The woods: one candidate a cell.
    let lo = (origin.x * MM_PER_TILE, origin.y * MM_PER_TILE);
    let hi = (
        (origin.x + side) * MM_PER_TILE,
        (origin.y + side) * MM_PER_TILE,
    );
    for (cx, cy) in GRID.cells_over(lo, hi) {
        let at = GRID.point(seed, purpose::TREE_CELL, cx, cy);
        let tile = at.tile();
        if !inside(tile) {
            continue;
        }
        let habitat = map.habitat(tile);
        if matches!(habitat, Habitat::Lone | Habitat::Riverside) || by_the_water(map, tile) {
            continue;
        }
        let chance = (u32::from(map.trees(tile)) * CHANCE_PER_DENSITY).min(MOST_CHANCE);
        if chance == 0 || !scatter::chance(chance, seed, purpose::TREE_CELL, &[key(cx), key(cy), 2])
        {
            continue;
        }
        let (gx, gy) = Grid::parent(cx, cy, GAP_CELLS);
        if scatter::chance(GAP_CHANCE, seed, purpose::TREE_GAP, &[key(gx), key(gy)]) {
            continue;
        }
        let roll = scatter::roll(seed, purpose::TREE_CELL, &[key(cx), key(cy), 3]);
        if let Some(kind) = pick(species, seed, tile, habitat, roll) {
            trees.push(Tree { at, species: kind });
        }
    }
    // Lone and riverside trees: one on their tile.
    for dy in 0..side {
        for dx in 0..side {
            let tile = Pos::new(origin.x + dx, origin.y + dy);
            let habitat = map.habitat(tile);
            if !matches!(habitat, Habitat::Lone | Habitat::Riverside) || by_the_water(map, tile) {
                continue;
            }
            let (x, y) = (key(i64::from(tile.x)), key(i64::from(tile.y)));
            let nudge = |axis: u64| {
                scatter::offset(
                    scatter::roll(seed, purpose::TREE_TILE, &[x, y, axis]),
                    TILE_JITTER,
                )
            };
            let centre = tile.centre();
            let at = FinePos::new(centre.x + nudge(0), centre.y + nudge(1));
            let roll = scatter::roll(seed, purpose::TREE_TILE, &[x, y, 2]);
            if let Some(kind) = pick(species, seed, tile, habitat, roll) {
                trees.push(Tree { at, species: kind });
            }
        }
    }
    trees
}

/// The trees of the whole of `map`, chunk by chunk. Only for the tests
/// and the tools that look at a whole map (the fingerprints): the game
/// places the trees of the chunks it sends ([`trees_in`]).
#[must_use]
pub fn everywhere(map: &Map, species: &[Species], seed: u64) -> Vec<Tree> {
    let (columns, rows) = (
        map.width().div_ceil(CHUNK_SIZE),
        map.height().div_ceil(CHUNK_SIZE),
    );
    let mut trees = Vec::new();
    for cy in 0..rows {
        for cx in 0..columns {
            let chunk = ChunkPos::new(
                i32::try_from(cx).expect("few chunks"),
                i32::try_from(cy).expect("few chunks"),
            );
            trees.extend(trees_in(map, species, seed, chunk));
        }
    }
    trees
}

/// Whether `tile` touches a river or a ford, even by a corner.
fn by_the_water(map: &Map, tile: Pos) -> bool {
    (-1..=1).any(|dy| {
        (-1..=1).any(|dx| {
            matches!(
                map.get(Pos::new(tile.x + dx, tile.y + dy)),
                Some(Terrain::Water | Terrain::Ford)
            )
        })
    })
}

/// The species of a tree at `tile` in `habitat`, drawn by `roll` among
/// those growing there, weighed by their stands.
fn pick(species: &[Species], seed: u64, tile: Pos, habitat: Habitat, roll: u64) -> Option<usize> {
    let weights: Vec<u64> = species
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let weight = s.habitats.get(&habitat).copied().unwrap_or(0);
            if weight == 0 {
                return 0;
            }
            let stand_seed = scatter::roll(seed, purpose::TREE_STAND, &[i as u64]);
            // Never quite nothing: a species that grows here may stand
            // anywhere, only rarely outside its stands.
            let strength = u64::from(noise(stand_seed, STAND_CELL, tile.x, tile.y)) + STAND_FLOOR;
            u64::from(weight) * strength * strength
        })
        .collect();
    scatter::weighted(&weights, roll)
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
            let trees = everywhere(&map, &kinds, seed);
            assert_eq!(
                trees,
                everywhere(&map, &kinds, seed),
                "the same trees every time"
            );
            let mut at: std::collections::BTreeMap<Pos, usize> = std::collections::BTreeMap::new();
            for tree in &trees {
                let tile = tree.at.tile();
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
                at.insert(tile, tree.species);
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
            // Every lone and riverside tile has its tree, or one near it;
            // the heart holds one tile in twenty or so (a tree a cell of
            // 4 × 4 tiles, some cells empty).
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
        // Grouped by kind, yet mixed: fewer neighbours of another kind
        // than if every tree drew its kind alone. On the grid, a tree's
        // neighbours stand a cell (4 m) away: 621 per mille on these maps
        // (the trees planted tile by tile gave 425).
        assert!((150..=650).contains(&share), "{share} per mille mixed");
    }

    #[test]
    fn a_chunk_places_its_own_trees_alone_and_in_any_order() {
        let kinds = species();
        let map = Map::valley(256, 256, 3);
        let mut forward = Vec::new();
        for cy in 0..8 {
            for cx in 0..8 {
                let chunk = ChunkPos::new(cx, cy);
                let trees = trees_in(&map, &kinds, 3, chunk);
                assert!(trees.iter().all(|t| t.at.tile().chunk() == chunk));
                forward.extend(trees);
            }
        }
        let mut backward = Vec::new();
        for cy in (0..8).rev() {
            for cx in (0..8).rev() {
                backward.extend(trees_in(&map, &kinds, 3, ChunkPos::new(cx, cy)));
            }
        }
        let order = |t: &Tree| (t.at.y, t.at.x);
        forward.sort_by_key(order);
        backward.sort_by_key(order);
        assert_eq!(forward, backward);
        // Never two trunks in one place: each cell's tree belongs to one
        // chunk only.
        let mut places: Vec<_> = forward.iter().map(order).collect();
        places.dedup();
        assert_eq!(places.len(), forward.len());
        assert!(forward.len() > 500, "{} trees", forward.len());
    }

    #[test]
    fn the_woods_keep_their_spacing_and_their_openings() {
        let kinds = species();
        let map = Map::valley(256, 256, 2);
        let woods: Vec<Tree> = everywhere(&map, &kinds, 2)
            .into_iter()
            .filter(|t| !matches!(map.habitat(t.at.tile()), Habitat::Lone | Habitat::Riverside))
            .collect();
        for (i, a) in woods.iter().enumerate() {
            for b in &woods[i + 1..] {
                let (dx, dy) = (i64::from(a.at.x - b.at.x), i64::from(a.at.y - b.at.y));
                if dx.abs() < 4000 && dy.abs() < 4000 {
                    let apart = i64::from(CELL - 2 * JITTER);
                    assert!(dx * dx + dy * dy >= apart * apart, "{a:?} {b:?}");
                }
            }
        }
        // In the heart, cells are left empty: about one in seven.
        let (mut cells, mut empty) = (0, 0);
        for cy in 0..64 {
            for cx in 0..64 {
                let at = GRID.point(2, purpose::TREE_CELL, cx, cy);
                if map.habitat(at.tile()) != Habitat::Heart || by_the_water(&map, at.tile()) {
                    continue;
                }
                cells += 1;
                empty += usize::from(!woods.iter().any(|t| t.at == at));
            }
        }
        let share = empty * 1000 / cells.max(1);
        eprintln!("{cells} cells in the heart, {share} per mille empty");
        assert!(cells > 100, "{cells} cells in the heart");
        assert!((80..=250).contains(&share), "{share} per mille empty");
    }
}
