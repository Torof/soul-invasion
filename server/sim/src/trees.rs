//! The trees (D37, D40): placed **chunk by chunk** ([`trees_in`]), a pure
//! function of the chunk, the map and the seed, the same in any order,
//! nothing stored and nothing planted for the whole map. A felled tree
//! will be stored when felling comes, and laid over these.
//!
//! They grow as a natural forest does, in layers:
//!
//! - **Stands**: where trees grow (the woods' density and habitat, D37),
//!   one kind dominates a stretch of a few hundred metres, the kind whose
//!   smooth noise (`STAND_CELL`, roughened by `STAND_DETAIL_CELL`) is
//!   strongest there among those its habitat suits; two stands blend over
//!   a few dozen metres.
//! - **Mother trees**: one on a jittered grid every `MOTHER_CELL`, of the
//!   kind its stand gives, usually the dominant one, sometimes a
//!   companion. A tree takes the kind of a mother around it, the nearer
//!   the likelier, as far as that kind's seeds fall
//!   ([`Species::spread_m`]: acorns close, birch seeds far), weighed by
//!   how well its own habitat suits that kind; or, with a small weight
//!   (`OWN`), draws its own from the stands. So trees come in groups
//!   around their mothers, a companion's group mixed with the dominant
//!   kind at its rim, the edges richer in the kinds edges suit.
//! - **Big and small**: the big kinds ([`Species::big`]) stand on a wide
//!   grid (`BIG_CELL`, 6.25 m: in the heart their crowns nearly touch), the
//!   others on a narrow one (`CELL`, 3 m) between them, never under a big
//!   tree's crown (`BIG_CLEAR`). A narrow cell whose kind comes out big
//!   holds a tree of the undergrowth instead (`UNDERGROWTH`), else
//!   nothing. A cell's chance follows the woods' density, thin at the
//!   edge as it always was, thickening steadily towards the heart (D37):
//!   a primeval forest. Every other row of both grids is shifted by half a
//!   cell, so no rows show along the view's diagonals (`gen::scatter`).
//! - **Openings**: a square of `GAP_SIDE` is now and then an opening,
//!   where no big tree stands and a few pioneers ([`Species::pioneer`])
//!   grow back.
//! - **Edges** thin out with the woods' density and lean to the kinds the
//!   edge suits.
//! - **The meadows** hold small groups of 3 to 8 trees on their own grid
//!   (`MEADOW_GRID`), by the woods or the water (seeds carried out by the
//!   wind, the animals, the floods), rarer farther (`MEADOW_NEAR`,
//!   `MEADOW_REACH`); far from both, a lone tree about once a square
//!   kilometre (`MEADOW_FAR`). Their kinds are those of the `lone`
//!   habitat and of the mothers near the edge.
//! - **Riverside** trees line the rivers, as before, alders and willows
//!   in runs from their mothers.
//! - Nothing stands on a tile touching a river or a ford, even by a
//!   corner: its trunk and crown would be drawn over the water.
//!
//! A chunk reads the map within `MOST_SPREAD_M` + a mother cell of
//! itself, and the meadow's distance to the woods `MEADOW_REACH` tiles
//! around its meadow trees, on a coarse lattice: bounded, whatever the
//! map's size.

use serde::{Deserialize, Serialize};

use crate::content::{MOST_SPREAD_M, Species};
use crate::r#gen::noise;
use crate::r#gen::scatter::{self, Grid, key, purpose};
use crate::map::{CHUNK_SIZE, ChunkPos, FinePos, MM_PER_TILE, Map, Pos, Terrain};
use crate::woods::{Habitat, Woods};

/// A tree standing on the ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tree {
    /// Where its trunk meets the ground.
    pub at: FinePos,
    /// Its kind: an index into the content's species.
    pub species: usize,
}

/// Side of the narrow grid, in millimetres: one small tree at most a cell.
const CELL: i32 = 3 * MM_PER_TILE;
/// Farthest a small tree stands from its cell's centre, on each axis, in
/// millimetres: a third of a cell, so trees keep 1 m apart.
const JITTER: i32 = 1000;
/// The narrow grid.
const GRID: Grid = Grid {
    cell: CELL,
    jitter: JITTER,
    offset_rows: true,
};
/// The wide grid of the big trees: cells of 6.25 m, the trunks nudged up
/// to 1.8 m, so two big trees keep 2.65 m apart and show no rows; in the
/// heart their crowns nearly touch, the ground glimpsed between them.
const BIG_CELL: i32 = 6250;
const BIG_JITTER: i32 = 1800;
const BIG_GRID: Grid = Grid {
    cell: BIG_CELL,
    jitter: BIG_JITTER,
    offset_rows: true,
};
/// No small tree stands nearer a big one's trunk than this, in
/// millimetres: under its crown.
const BIG_CLEAR: i32 = 2400;
/// A cell holds a tree with a chance of its tile's density (0 to 100)
/// times `CHANCE_PER_DENSITY`, plus `CHANCE_IN_THE_WOOD` for each point
/// above the forest's (35: the edge keeps its old thinness, the wood and
/// the heart close up), in thousandths, never more than `MOST_CHANCE`.
const CHANCE_PER_DENSITY: u32 = 6;
const CHANCE_IN_THE_WOOD: u32 = 12;
/// The wide grid's cells are larger: at the edge their chance a point of
/// density is raised by the ratio of the areas, so the edge keeps as
/// many big trees a square metre.
const BIG_CHANCE_PER_DENSITY: u32 = 8;
const MOST_CHANCE: u32 = 920;
/// A narrow cell whose kind comes out big holds, with this chance in
/// thousandths, a tree of the undergrowth instead (a small kind that is
/// no pioneer: hazel, yew), else nothing: a big kind's stand stays open.
const UNDERGROWTH: u32 = 420;
/// The openings: a square of `GAP_SIDE` millimetres is one with a chance
/// of `GAP_CHANCE` thousandths; in it, a narrow cell holds a pioneer with
/// `GAP_PIONEER` thousandths of its usual chance.
const GAP_SIDE: i32 = 16 * MM_PER_TILE;
const GAP_CHANCE: u32 = 45;
const GAP_PIONEER: u32 = 300;
/// Farthest a riverside tree stands from its tile's centre, in
/// millimetres.
const TILE_JITTER: i32 = 300;
/// The stands' noise, its lattice in tiles, then the finer noise that
/// roughens it and its weight in thousandths; added to a stand's strength
/// (0 to 1000) before it is raised to the fifth power, so the strongest
/// kind clearly dominates: a species that grows here may stand anywhere,
/// only rarely outside its stands.
const STAND_CELL: u32 = 192;
const STAND_DETAIL_CELL: u32 = 48;
const STAND_DETAIL: u64 = 300;
const STAND_FLOOR: u64 = 60;
/// The mother trees' grid: one every 24 m, nudged up to 8 m.
const MOTHER_GRID: Grid = Grid {
    cell: 24 * MM_PER_TILE,
    jitter: 8 * MM_PER_TILE,
    offset_rows: true,
};
/// Weight of a tree's own draw from the stands, against a mother's at
/// its very foot in a habitat that suits it fully (1000 × 100).
const OWN: u64 = 6_000;
/// The meadows' grid: one group of trees at most every 24 m, its centre
/// nudged up to 8 m.
const MEADOW_GRID: Grid = Grid {
    cell: 24 * MM_PER_TILE,
    jitter: 8 * MM_PER_TILE,
    offset_rows: true,
};
/// Chance of a meadow cell's group by the woods or the water, in
/// thousandths, falling to nothing at `MEADOW_REACH` tiles from them (as
/// the square of what is left of the way). Beyond, a lone tree with
/// `MEADOW_FAR` millionths: about one a square kilometre.
const MEADOW_NEAR: u32 = 420;
const MEADOW_REACH: i32 = 40;
const MEADOW_FAR: u64 = 576;
/// A group: 3 to 8 trees within `GROUP_RADIUS` millimetres of its centre,
/// kept `GROUP_APART` apart.
const GROUP_SMALLEST: u64 = 3;
const GROUP_LARGEST: u64 = 8;
const GROUP_RADIUS: i32 = 5000;
const GROUP_APART: i64 = 1800;
/// The lattice, in tiles, on which the meadow looks for the woods and the
/// water.
const MEADOW_STEP: i32 = 4;
/// Within half to one and a half times this many tiles of the rock (the
/// cliff), varying from tile to tile so the band blends into the woods,
/// the woods grow as at the cliff's foot: pines on the poor, stony
/// ground. The rock is looked for on a lattice of `ROCK_STEP` tiles.
const ROCK_REACH: i32 = 36;
const ROCK_STEP: i32 = 8;

/// A mother tree: where it stands and its kind.
#[derive(Debug, Clone, Copy)]
struct Mother {
    at: FinePos,
    species: usize,
}

/// What a chunk's trees are placed from: the map, the kinds, the seed,
/// and the mother trees around the chunk.
struct Planting<'a> {
    map: &'a Map,
    species: &'a [Species],
    seed: u64,
    mothers: Vec<Mother>,
    /// Each kind's seeds for its stands' noise and their detail.
    stands: Vec<(u64, u64)>,
    /// The lattice points of the woods `MEADOW_REACH` around the chunk,
    /// worked out when its first meadow tree asks.
    woods: Option<Vec<Pos>>,
    /// The lattice points of the rock `ROCK_REACH` around the chunk and
    /// its mothers.
    rock: Vec<Pos>,
}

impl<'a> Planting<'a> {
    /// The mothers that may have seeded a tree of the rectangle from `lo`
    /// to `hi` (millimetres).
    fn new(
        map: &'a Map,
        species: &'a [Species],
        seed: u64,
        lo: (i32, i32),
        hi: (i32, i32),
    ) -> Self {
        let margin = i32::try_from(MOST_SPREAD_M).expect("small") * MM_PER_TILE;
        let mut planting = Self {
            map,
            species,
            seed,
            mothers: Vec::new(),
            stands: (0..species.len() as u64)
                .map(|i| {
                    (
                        scatter::roll(seed, purpose::TREE_STAND, &[i]),
                        scatter::roll(seed, purpose::TREE_STAND, &[i, 1]),
                    )
                })
                .collect(),
            woods: None,
            rock: Vec::new(),
        };
        let step = ROCK_STEP;
        let reach = ROCK_REACH * 3 / 2 * MM_PER_TILE + margin + MOTHER_GRID.cell;
        let (x0, y0) = (
            (lo.0 - reach).div_euclid(MM_PER_TILE),
            (lo.1 - reach).div_euclid(MM_PER_TILE),
        );
        let (x1, y1) = (
            (hi.0 + reach).div_euclid(MM_PER_TILE),
            (hi.1 + reach).div_euclid(MM_PER_TILE),
        );
        for y in (y0.div_euclid(step) * step..=y1).step_by(usize::try_from(step).expect("positive"))
        {
            for x in
                (x0.div_euclid(step) * step..=x1).step_by(usize::try_from(step).expect("positive"))
            {
                if map.get(Pos::new(x, y)) == Some(Terrain::Rock) {
                    planting.rock.push(Pos::new(x, y));
                }
            }
        }
        for (cx, cy) in MOTHER_GRID.cells_over(
            (lo.0 - margin, lo.1 - margin),
            (hi.0 + margin, hi.1 + margin),
        ) {
            let at = MOTHER_GRID.point(seed, purpose::TREE_MOTHER, cx, cy);
            let habitat = planting.habitat(at.tile());
            let roll = scatter::roll(seed, purpose::TREE_MOTHER, &[key(cx), key(cy), 2]);
            if let Some(kind) = planting.stand_pick(at.tile(), habitat, roll, false, |_| true) {
                planting.mothers.push(Mother { at, species: kind });
            }
        }
        planting
    }

    /// The habitat of `tile` for the kinds of tree: the map's, but the
    /// woods near the rock grow as at the cliff's foot.
    fn habitat(&self, tile: Pos) -> Habitat {
        let habitat = self.map.habitat(tile);
        if !matches!(habitat, Habitat::Edge | Habitat::Wood | Habitat::Heart)
            || self.rock.is_empty()
        {
            return habitat;
        }
        let (x, y) = (key(i64::from(tile.x)), key(i64::from(tile.y)));
        let spread = u64::try_from(ROCK_REACH).expect("positive");
        let reach = i64::from(ROCK_REACH / 2)
            + i64::try_from(
                scatter::roll(self.seed, purpose::TREE_TILE, &[x, y, 9]) % (spread + 1),
            )
            .expect("small");
        let near = self.rock.iter().any(|p| {
            let (dx, dy) = (i64::from(p.x - tile.x), i64::from(p.y - tile.y));
            dx * dx + dy * dy <= reach * reach
        });
        if near { Habitat::CliffFoot } else { habitat }
    }

    /// The kind of a tree at `at` in `habitat`, drawn by `roll` from the
    /// mothers around and its own draw from the stands, among the kinds
    /// `allowed`.
    fn kind(
        &self,
        at: FinePos,
        habitat: Habitat,
        roll: u64,
        allowed: impl Fn(usize) -> bool,
    ) -> Option<usize> {
        let mut weights = vec![0_u64; self.species.len()];
        for mother in &self.mothers {
            let kind = &self.species[mother.species];
            let reach = i64::from(kind.spread_m) * i64::from(MM_PER_TILE);
            let (dx, dy) = (i64::from(at.x - mother.at.x), i64::from(at.y - mother.at.y));
            if dx.abs() >= reach || dy.abs() >= reach || !allowed(mother.species) {
                continue;
            }
            let near = dx * dx + dy * dy;
            if near >= reach * reach {
                continue;
            }
            let suits = u64::from(kind.habitats.get(&habitat).copied().unwrap_or(0));
            let left = u64::try_from(reach - near.isqrt()).expect("within reach");
            let reach = u64::try_from(reach).expect("positive");
            weights[mother.species] += left * left * 1000 / (reach * reach) * suits;
        }
        let from_mothers: u64 = weights.iter().sum();
        let pick = roll % (from_mothers + OWN);
        if pick < from_mothers {
            return scatter::weighted(&weights, pick);
        }
        let again = scatter::roll(roll, purpose::TREE_STAND, &[]);
        // In the open (meadows, copses, riverbanks) no kind rules: a
        // softer draw, more of a mix.
        let sharp = !matches!(habitat, Habitat::Lone | Habitat::Copse | Habitat::Riverside);
        self.stand_pick(at.tile(), habitat, again, sharp, allowed)
    }

    /// A kind for `tile` in `habitat` from the stands alone, drawn by
    /// `roll` among the kinds `allowed`: each weighed by how its habitat
    /// suits it times its stand's strength there to the fifth power
    /// (`sharp`: a tree's own draw, the dominant kind nearly always) or
    /// to the third (a mother: now and then a companion); a
    /// kind the habitat suits less than half as well as the best only
    /// with its stand's floor, a stray.
    fn stand_pick(
        &self,
        tile: Pos,
        habitat: Habitat,
        roll: u64,
        sharp: bool,
        allowed: impl Fn(usize) -> bool,
    ) -> Option<usize> {
        let suits = |i: usize, s: &Species| {
            if allowed(i) {
                u64::from(s.habitats.get(&habitat).copied().unwrap_or(0))
            } else {
                0
            }
        };
        // Only the kinds that grow well here (at least half as well as
        // the best) compete for the stands; the others keep their floor.
        let best = self
            .species
            .iter()
            .enumerate()
            .map(|(i, s)| suits(i, s))
            .max()
            .unwrap_or(0);
        let weights: Vec<u64> = self
            .species
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let suits = suits(i, s);
                if suits == 0 {
                    return 0;
                }
                if suits * 2 < best {
                    let floor = STAND_FLOOR * STAND_FLOOR;
                    return suits * (floor * floor / 1000 * STAND_FLOOR / 1000).max(1);
                }
                let (stand, detail) = self.stands[i];
                let strength = (u64::from(noise(stand, STAND_CELL, tile.x, tile.y))
                    * (1000 - STAND_DETAIL)
                    + u64::from(noise(detail, STAND_DETAIL_CELL, tile.x, tile.y)) * STAND_DETAIL)
                    / 1000
                    + STAND_FLOOR;
                // The fifth power (or the cube), kept under 2^64: strength
                // is at most 1060, so strength^5 / 10^6 is under 1.4 × 10^9.
                let cube = strength * strength * strength / 1000;
                if sharp {
                    suits * (cube * strength / 1000 * strength)
                } else {
                    suits * cube * 1000
                }
            })
            .collect();
        scatter::weighted(&weights, roll)
    }

    /// Whether a cell's tree at `tile` stands, by its density, with a
    /// chance scaled by `share` thousandths, drawn from `keys`.
    fn stands(&self, tile: Pos, share: u32, purpose: u64, keys: &[u64]) -> bool {
        let density = u32::from(self.map.trees(tile));
        let per_density = if purpose == purpose::TREE_BIG {
            BIG_CHANCE_PER_DENSITY
        } else {
            CHANCE_PER_DENSITY
        };
        let chance = (density * per_density
            + density.saturating_sub(u32::from(Woods::FOREST)) * CHANCE_IN_THE_WOOD)
            .min(MOST_CHANCE)
            * share
            / 1000;
        chance > 0 && scatter::chance(chance, self.seed, purpose, keys)
    }

    /// Whether the opening square around `at` is an opening.
    fn opening(&self, at: FinePos) -> bool {
        let (gx, gy) = Grid::parent(
            i64::from(at.x.div_euclid(CELL)),
            i64::from(at.y.div_euclid(CELL)),
            i64::from(GAP_SIDE / CELL),
        );
        scatter::chance(
            GAP_CHANCE,
            self.seed,
            purpose::TREE_GAP,
            &[key(gx), key(gy), 16],
        )
    }

    /// The tiles from `tile` (in `chunk`, or within a group's reach of it)
    /// to the nearest woods or water, looked for on a lattice of
    /// `MEADOW_STEP` tiles as far as `MEADOW_REACH`; `None` beyond.
    fn woods_distance(&mut self, chunk: ChunkPos, tile: Pos) -> Option<i32> {
        let map = self.map;
        let woods = self.woods.get_or_insert_with(|| {
            let step = MEADOW_STEP;
            let origin = chunk.origin();
            let side = i32::try_from(CHUNK_SIZE).expect("small chunk");
            // A group's centre may stand a group's reach outside the chunk.
            let reach = MEADOW_REACH + GROUP_RADIUS / MM_PER_TILE + 1;
            let (x0, y0) = (
                (origin.x - reach).div_euclid(step) * step,
                (origin.y - reach).div_euclid(step) * step,
            );
            let (x1, y1) = (origin.x + side + reach, origin.y + side + reach);
            let mut found = Vec::new();
            for y in (y0..=y1).step_by(usize::try_from(step).expect("positive")) {
                for x in (x0..=x1).step_by(usize::try_from(step).expect("positive")) {
                    let p = Pos::new(x, y);
                    let woods = matches!(
                        map.habitat(p),
                        Habitat::Edge
                            | Habitat::Wood
                            | Habitat::Heart
                            | Habitat::Copse
                            | Habitat::CliffFoot
                    );
                    let water = matches!(
                        map.get(p),
                        Some(Terrain::Water | Terrain::Ford | Terrain::Brook)
                    );
                    if woods || water {
                        found.push(p);
                    }
                }
            }
            found
        });
        let nearest = woods
            .iter()
            .map(|p| {
                let (dx, dy) = (i64::from(p.x - tile.x), i64::from(p.y - tile.y));
                dx * dx + dy * dy
            })
            .min();
        let reach = i64::from(MEADOW_REACH);
        nearest
            .filter(|d| *d <= reach * reach)
            .map(|d| i32::try_from(d.isqrt()).expect("within reach"))
    }
}

/// The trees of `chunk` on `map`, of the kinds in `species`, for `seed`:
/// those whose trunk stands in the chunk, the big ones first, then the
/// small ones (cell by cell, row by row), the riverside ones (tile by
/// tile) and the meadows'. Reads the map within `MOST_SPREAD_M` and a
/// mother cell around the chunk, and `MEADOW_REACH` around its meadow
/// trees.
#[must_use]
pub fn trees_in(map: &Map, species: &[Species], seed: u64, chunk: ChunkPos) -> Vec<Tree> {
    let origin = chunk.origin();
    let side = i32::try_from(CHUNK_SIZE).expect("small chunk");
    let lo = (origin.x * MM_PER_TILE, origin.y * MM_PER_TILE);
    let hi = (
        (origin.x + side) * MM_PER_TILE,
        (origin.y + side) * MM_PER_TILE,
    );
    let mut planting = Planting::new(map, species, seed, lo, hi);
    let bigs = planting.big_trees(lo, hi);
    let mut trees: Vec<Tree> = bigs
        .iter()
        .copied()
        .filter(|t| t.at.tile().chunk() == chunk)
        .collect();
    planting.small_trees(chunk, &bigs, &mut trees);
    planting.riverside_trees(chunk, &mut trees);
    planting.meadow_trees(chunk, &mut trees);
    trees
}

impl Planting<'_> {
    /// The big trees around the rectangle from `lo` to `hi`: those in it,
    /// and those whose crown may reach over its small cells.
    fn big_trees(&self, lo: (i32, i32), hi: (i32, i32)) -> Vec<Tree> {
        let mut bigs = Vec::new();
        let reach = BIG_CLEAR + CELL;
        let around = ((lo.0 - reach, lo.1 - reach), (hi.0 + reach, hi.1 + reach));
        for (cx, cy) in BIG_GRID.cells_over(around.0, around.1) {
            let at = BIG_GRID.point(self.seed, purpose::TREE_BIG, cx, cy);
            let tile = at.tile();
            let habitat = self.habitat(tile);
            if !in_the_woods(habitat)
                || by_the_water(self.map, tile)
                || !self.stands(tile, 1000, purpose::TREE_BIG, &[key(cx), key(cy), 2])
                || self.opening(at)
            {
                continue;
            }
            let roll = scatter::roll(self.seed, purpose::TREE_BIG, &[key(cx), key(cy), 3]);
            if let Some(kind) = self
                .kind(at, habitat, roll, |_| true)
                .filter(|k| self.species[*k].big)
            {
                bigs.push(Tree { at, species: kind });
            }
        }
        bigs
    }

    /// The small trees of `chunk`, between the big ones `bigs`.
    fn small_trees(&self, chunk: ChunkPos, bigs: &[Tree], trees: &mut Vec<Tree>) {
        let origin = chunk.origin();
        let side = i32::try_from(CHUNK_SIZE).expect("small chunk");
        let lo = (origin.x * MM_PER_TILE, origin.y * MM_PER_TILE);
        let hi = (
            (origin.x + side) * MM_PER_TILE,
            (origin.y + side) * MM_PER_TILE,
        );
        let big = |k: usize| self.species[k].big;
        let clear = i64::from(BIG_CLEAR);
        for (cx, cy) in GRID.cells_over(lo, hi) {
            let at = GRID.point(self.seed, purpose::TREE_CELL, cx, cy);
            let tile = at.tile();
            if tile.chunk() != chunk {
                continue;
            }
            let habitat = self.habitat(tile);
            if !in_the_woods(habitat) || by_the_water(self.map, tile) {
                continue;
            }
            let under_a_crown = bigs.iter().any(|b| {
                let (dx, dy) = (i64::from(b.at.x - at.x), i64::from(b.at.y - at.y));
                dx * dx + dy * dy < clear * clear
            });
            let opening = self.opening(at);
            let share = if opening { GAP_PIONEER } else { 1000 };
            if under_a_crown
                || !self.stands(tile, share, purpose::TREE_CELL, &[key(cx), key(cy), 2])
            {
                continue;
            }
            let roll = scatter::roll(self.seed, purpose::TREE_CELL, &[key(cx), key(cy), 3]);
            let kind = if opening {
                self.stand_pick(tile, habitat, roll, true, |k| self.species[k].pioneer)
            } else {
                self.kind(at, habitat, roll, |_| true).and_then(|k| {
                    if !big(k) {
                        Some(k)
                    } else if scatter::chance(
                        UNDERGROWTH,
                        self.seed,
                        purpose::TREE_CELL,
                        &[key(cx), key(cy), 4],
                    ) {
                        let again =
                            scatter::roll(self.seed, purpose::TREE_CELL, &[key(cx), key(cy), 5]);
                        self.stand_pick(tile, habitat, again, true, |k| {
                            !big(k) && !self.species[k].pioneer
                        })
                    } else {
                        None
                    }
                })
            };
            if let Some(kind) = kind.filter(|k| !big(*k)) {
                trees.push(Tree { at, species: kind });
            }
        }
    }

    /// The riverside trees of `chunk`: one on their tile.
    fn riverside_trees(&self, chunk: ChunkPos, trees: &mut Vec<Tree>) {
        let origin = chunk.origin();
        let side = i32::try_from(CHUNK_SIZE).expect("small chunk");
        for dy in 0..side {
            for dx in 0..side {
                let tile = Pos::new(origin.x + dx, origin.y + dy);
                if self.map.habitat(tile) != Habitat::Riverside || by_the_water(self.map, tile) {
                    continue;
                }
                let (x, y) = (key(i64::from(tile.x)), key(i64::from(tile.y)));
                let nudge = |axis: u64| {
                    scatter::offset(
                        scatter::roll(self.seed, purpose::TREE_TILE, &[x, y, axis]),
                        TILE_JITTER,
                    )
                };
                let centre = tile.centre();
                let at = FinePos::new(centre.x + nudge(0), centre.y + nudge(1));
                let roll = scatter::roll(self.seed, purpose::TREE_TILE, &[x, y, 2]);
                if let Some(kind) = self.kind(at, Habitat::Riverside, roll, |_| true) {
                    trees.push(Tree { at, species: kind });
                }
            }
        }
    }

    /// The meadows' trees of `chunk`: small groups by the woods and the
    /// water, rarer farther; far from both, a lone tree now and then.
    fn meadow_trees(&mut self, chunk: ChunkPos, trees: &mut Vec<Tree>) {
        let origin = chunk.origin();
        let side = i32::try_from(CHUNK_SIZE).expect("small chunk");
        let (lo, hi) = (
            (
                origin.x * MM_PER_TILE - GROUP_RADIUS,
                origin.y * MM_PER_TILE - GROUP_RADIUS,
            ),
            (
                (origin.x + side) * MM_PER_TILE + GROUP_RADIUS,
                (origin.y + side) * MM_PER_TILE + GROUP_RADIUS,
            ),
        );
        let open = |map: &Map, tile: Pos| {
            matches!(
                map.habitat(tile),
                Habitat::Meadow | Habitat::Lone | Habitat::Clearing
            ) && !by_the_water(map, tile)
        };
        for (cx, cy) in MEADOW_GRID.cells_over(lo, hi) {
            let centre = MEADOW_GRID.point(self.seed, purpose::TREE_MEADOW, cx, cy);
            let near_the_chunk =
                (lo.0..hi.0).contains(&centre.x) && (lo.1..hi.1).contains(&centre.y);
            if !near_the_chunk || !open(self.map, centre.tile()) {
                continue;
            }
            let keys = |n: u64| [key(cx), key(cy), n];
            let roll = scatter::roll(self.seed, purpose::TREE_MEADOW, &keys(2));
            let count = match self.woods_distance(chunk, centre.tile()) {
                Some(d) => {
                    let left = u32::try_from(MEADOW_REACH - d).expect("within reach");
                    let reach = u32::try_from(MEADOW_REACH).expect("positive");
                    if roll % 1000 >= u64::from(MEADOW_NEAR * left * left / (reach * reach)) {
                        continue;
                    }
                    GROUP_SMALLEST + roll / 1000 % (GROUP_LARGEST - GROUP_SMALLEST + 1)
                }
                None if roll % 1_000_000 < MEADOW_FAR => 1,
                None => continue,
            };
            // The group's trees around its centre, kept apart; a place too
            // near one already taken is drawn again, twice at most.
            let mut group: Vec<FinePos> = Vec::new();
            for n in 0..count {
                for attempt in 0..3_u64 {
                    let at = if count == 1 {
                        centre
                    } else {
                        let draw = |axis: u64| {
                            scatter::offset(
                                scatter::roll(
                                    self.seed,
                                    purpose::TREE_MEADOW,
                                    &keys(16 + n * 8 + attempt * 2 + axis),
                                ),
                                GROUP_RADIUS,
                            )
                        };
                        FinePos::new(centre.x + draw(0), centre.y + draw(1))
                    };
                    let crowded = group.iter().any(|g| {
                        let (dx, dy) = (i64::from(g.x - at.x), i64::from(g.y - at.y));
                        dx * dx + dy * dy < GROUP_APART * GROUP_APART
                    });
                    if crowded {
                        continue;
                    }
                    group.push(at);
                    break;
                }
            }
            for (n, at) in group.into_iter().enumerate() {
                if at.tile().chunk() != chunk || !open(self.map, at.tile()) {
                    continue;
                }
                let roll = scatter::roll(self.seed, purpose::TREE_MEADOW, &keys(200 + n as u64));
                if let Some(kind) = self.kind(at, Habitat::Lone, roll, |_| true) {
                    trees.push(Tree { at, species: kind });
                }
            }
        }
    }
}

/// Whether trees of the woods grow in `habitat` (the riverside and the
/// meadows have their own).
fn in_the_woods(habitat: Habitat) -> bool {
    !matches!(
        habitat,
        Habitat::Riverside | Habitat::Lone | Habitat::Meadow
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The kinds of tree of the repository's `content/species.ron`.
    const SPECIES: &str = include_str!("../../../content/species.ron");

    fn species() -> Vec<Species> {
        ron::from_str(SPECIES).expect("species.ron parses")
    }

    /// The habitat a tree at `at` was given its kind in, as `trees_in`
    /// sees it: the meadows' trees take the lone trees' kinds.
    fn seen_habitat(map: &Map, kinds: &[Species], seed: u64, at: FinePos) -> Habitat {
        let chunk = at.tile().chunk();
        let origin = chunk.origin();
        let side = i32::try_from(CHUNK_SIZE).unwrap();
        let planting = Planting::new(
            map,
            kinds,
            seed,
            (origin.x * MM_PER_TILE, origin.y * MM_PER_TILE),
            (
                (origin.x + side) * MM_PER_TILE,
                (origin.y + side) * MM_PER_TILE,
            ),
        );
        match planting.habitat(at.tile()) {
            Habitat::Meadow => Habitat::Lone,
            other => other,
        }
    }

    #[test]
    fn trees_stand_where_their_kind_grows_never_by_the_water() {
        let kinds = species();
        for seed in 1..=3 {
            let map = Map::valley(256, 256, seed);
            let trees = everywhere(&map, &kinds, seed);
            assert_eq!(
                trees,
                everywhere(&map, &kinds, seed),
                "the same trees every time"
            );
            assert!(trees.len() > 300, "seed {seed}: {} trees", trees.len());
            for tree in &trees {
                let tile = tree.at.tile();
                let habitat = seen_habitat(&map, &kinds, seed, tree.at);
                assert!(
                    !matches!(habitat, Habitat::Clearing | Habitat::Camp | Habitat::None),
                    "seed {seed}: a tree in the {habitat:?} at {tile:?}"
                );
                assert!(
                    kinds[tree.species]
                        .habitats
                        .get(&habitat)
                        .copied()
                        .unwrap_or(0)
                        > 0,
                    "seed {seed}: a {} in the {habitat:?}",
                    kinds[tree.species].id
                );
                assert!(
                    !by_the_water(&map, tile),
                    "seed {seed}: a tree at {tile:?} beside the water"
                );
            }
        }
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
        // Never two trunks in one place.
        let mut places: Vec<_> = forward.iter().map(order).collect();
        places.dedup();
        assert_eq!(places.len(), forward.len());
        assert!(forward.len() > 300, "{} trees", forward.len());
    }

    #[test]
    fn big_trees_stand_apart_and_nothing_small_under_their_crowns() {
        let kinds = species();
        let map = Map::valley(512, 512, 2);
        let trees = everywhere(&map, &kinds, 2);
        let woods: Vec<&Tree> = trees
            .iter()
            .filter(|t| {
                !matches!(
                    map.habitat(t.at.tile()),
                    Habitat::Riverside | Habitat::Lone | Habitat::Meadow
                )
            })
            .collect();
        let (bigs, smalls): (Vec<&Tree>, Vec<&Tree>) =
            woods.iter().partition(|t| kinds[t.species].big);
        assert!(
            bigs.len() > 100 && smalls.len() > 100,
            "{} big, {} small",
            bigs.len(),
            smalls.len()
        );
        let apart = |a: &Tree, b: &Tree| {
            let (dx, dy) = (i64::from(a.at.x - b.at.x), i64::from(a.at.y - b.at.y));
            dx * dx + dy * dy
        };
        let min_big = i64::from(BIG_CELL - 2 * BIG_JITTER);
        let mut spacing = 0_i64;
        for (i, a) in bigs.iter().enumerate() {
            let mut nearest = i64::MAX;
            for (j, b) in bigs.iter().enumerate() {
                if i != j {
                    let d = apart(a, b);
                    assert!(d >= min_big * min_big, "{a:?} {b:?}");
                    nearest = nearest.min(d);
                }
            }
            spacing += nearest.isqrt();
        }
        // On the wide grid of 5.5 m: their crowns touch, the nearest about
        // 4.5 m off.
        let mean = spacing / i64::try_from(bigs.len()).unwrap();
        eprintln!("{} big trees, {mean} mm to the nearest", bigs.len());
        assert!((3800..=6500).contains(&mean), "{mean} mm");
        let clear = i64::from(BIG_CLEAR);
        for s in &smalls {
            for b in &bigs {
                assert!(apart(s, b) >= clear * clear, "{s:?} under {b:?}");
            }
        }
    }

    #[test]
    fn meadow_trees_come_in_groups_by_the_woods_and_the_water() {
        let kinds = species();
        // Open land sampled every fourth tile, and the trees on it, by
        // distance to the woods or the water: under 12 tiles, 12 to 24,
        // 24 to 40, beyond.
        let (mut sampled, mut trees_by_band) = ([0_u32; 4], [0_u32; 4]);
        let (mut grouped, mut meadow_trees) = (0_u32, 0_u32);
        let band = |d: Option<i32>| match d {
            Some(d) if d < 12 => 0,
            Some(d) if d < 24 => 1,
            Some(_) => 2,
            None => 3,
        };
        let open = |map: &Map, t: Pos| {
            matches!(
                map.habitat(t),
                Habitat::Meadow | Habitat::Lone | Habitat::Clearing
            ) && !by_the_water(map, t)
        };
        for seed in 1..=4 {
            let map = Map::valley(512, 512, seed);
            let all = everywhere(&map, &kinds, seed);
            let in_the_open: Vec<&Tree> = all.iter().filter(|t| open(&map, t.at.tile())).collect();
            for cy in 0..16 {
                for cx in 0..16 {
                    let chunk = ChunkPos::new(cx, cy);
                    let origin = chunk.origin();
                    let mut planting = Planting::new(
                        &map,
                        &kinds,
                        seed,
                        (origin.x * MM_PER_TILE, origin.y * MM_PER_TILE),
                        ((origin.x + 32) * MM_PER_TILE, (origin.y + 32) * MM_PER_TILE),
                    );
                    for y in (origin.y..origin.y + 32).step_by(4) {
                        for x in (origin.x..origin.x + 32).step_by(4) {
                            let t = Pos::new(x, y);
                            if open(&map, t) {
                                sampled[band(planting.woods_distance(chunk, t))] += 1;
                            }
                        }
                    }
                    for tree in in_the_open.iter().filter(|t| t.at.tile().chunk() == chunk) {
                        trees_by_band[band(planting.woods_distance(chunk, tree.at.tile()))] += 1;
                    }
                }
            }
            // A meadow tree has a fellow within 7 m: groups, not loners.
            for a in &in_the_open {
                if map.habitat(a.at.tile()) == Habitat::Clearing {
                    continue;
                }
                meadow_trees += 1;
                grouped += u32::from(in_the_open.iter().any(|b| {
                    let (dx, dy) = (i64::from(a.at.x - b.at.x), i64::from(a.at.y - b.at.y));
                    (dx, dy) != (0, 0) && dx * dx + dy * dy <= 7000 * 7000
                }));
            }
        }
        // Trees ten thousand tiles of open land, by band (a sample stands
        // for 16 tiles).
        let rate: Vec<u32> = (0..4)
            .map(|b| trees_by_band[b] * 10_000 / (sampled[b] * 16).max(1))
            .collect();
        eprintln!(
            "open land sampled {sampled:?}, trees {trees_by_band:?}, per ten thousand tiles {rate:?}; {grouped} of {meadow_trees} meadow trees in groups"
        );
        assert!(sampled[0] > 500 && sampled[1] > 50, "{sampled:?}");
        assert!(rate[0] > rate[1] && rate[1] > rate[2], "{rate:?}");
        assert!(rate[0] >= 15, "{rate:?}");
        // Far from the woods and the water, about one tree a square
        // kilometre: on four maps of a quarter of one, a handful at most.
        assert!(trees_by_band[3] <= 4, "{trees_by_band:?}");
        assert!(
            grouped * 1000 >= meadow_trees * 800,
            "{grouped} of {meadow_trees} grouped"
        );
    }

    #[test]
    fn stands_are_ruled_by_a_kind_with_mixed_groves_around_mothers() {
        let kinds = species();
        let (mut ruled, mut windows) = (0_u64, 0_u64);
        let (mut near_same, mut near_count, mut wide_same, mut wide_count) =
            (0_u64, 0_u64, 0_u64, 0_u64);
        for seed in 1..=3 {
            let map = Map::valley(512, 512, seed);
            let woods: Vec<Tree> = everywhere(&map, &kinds, seed)
                .into_iter()
                .filter(|t| matches!(map.habitat(t.at.tile()), Habitat::Wood | Habitat::Heart))
                .collect();
            // In windows of 64 m, the commonest big kind holds most of the
            // big trees: stands.
            let mut by_window: std::collections::BTreeMap<(i32, i32), Vec<usize>> =
                std::collections::BTreeMap::new();
            for t in woods.iter().filter(|t| kinds[t.species].big) {
                by_window
                    .entry((t.at.x.div_euclid(64_000), t.at.y.div_euclid(64_000)))
                    .or_default()
                    .push(t.species);
            }
            for list in by_window.values().filter(|l| l.len() >= 20) {
                let mut count = vec![0_u64; kinds.len()];
                for k in list {
                    count[*k] += 1;
                }
                ruled += count.iter().max().unwrap() * 1000 / list.len() as u64;
                windows += 1;
            }
            // A big tree shares its kind with its neighbours within 12 m
            // more often than with the big trees within 40 m: groups; but
            // not always: mixed.
            let bigs: Vec<&Tree> = woods.iter().filter(|t| kinds[t.species].big).collect();
            for a in &bigs {
                for b in &bigs {
                    let (dx, dy) = (i64::from(a.at.x - b.at.x), i64::from(a.at.y - b.at.y));
                    let d = dx * dx + dy * dy;
                    if d == 0 || d > 40_000 * 40_000 {
                        continue;
                    }
                    let same = u64::from(a.species == b.species);
                    if d <= 12_000 * 12_000 {
                        near_same += same;
                        near_count += 1;
                    }
                    wide_same += same;
                    wide_count += 1;
                }
            }
        }
        let ruled = ruled / windows.max(1);
        let near = near_same * 1000 / near_count.max(1);
        let wide = wide_same * 1000 / wide_count.max(1);
        eprintln!(
            "{windows} windows, the ruling kind {ruled} per mille; same kind {near} per mille within 12 m, {wide} within 40 m"
        );
        assert!(windows > 20, "{windows} windows");
        assert!(ruled >= 550, "stands: {ruled} per mille");
        assert!(
            near >= wide + 30,
            "groups: {near} within 12 m, {wide} within 40 m"
        );
        assert!(
            near < 900,
            "mixed: {near} per mille of the same kind within 12 m"
        );
    }

    /// Colours of the kinds of tree on the map, in `species.ron`'s order
    /// (oak, pine, birch, hazel, willow, wild apple, alder, ash, linden,
    /// yew); any more are grey.
    const COLOURS: [[u8; 3]; 10] = [
        [150, 90, 30],
        [20, 95, 70],
        [235, 225, 90],
        [205, 110, 205],
        [50, 200, 225],
        [230, 35, 35],
        [35, 60, 205],
        [125, 205, 55],
        [250, 155, 30],
        [95, 20, 100],
    ];

    /// The trees of a valley seen from above, a dot per tree, a colour per
    /// kind, the big ones larger, on the terrain muted; and the time
    /// `trees_in` takes a chunk. Writes a binary PPM (any image tool turns
    /// it into a PNG) at `SOUL_INVASION_FOREST_DUMP`. Also read:
    /// `SOUL_INVASION_FOREST_SIZE` (the valley's side, 768 unless given),
    /// `SOUL_INVASION_SEED` (the world's seed, the game's unless given),
    /// `SOUL_INVASION_FOREST_SCALE` (pixels a tile, 1 unless given) and
    /// `SOUL_INVASION_FOREST_CROP` (`x,y,w,h` in tiles, the whole valley
    /// unless given). In release:
    /// `cargo test --release -p sim dump_forest -- --ignored --nocapture`.
    #[test]
    #[ignore = "writes the forest as an image for a look; run by hand"]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::many_single_char_names,
        clippy::too_many_lines
    )]
    fn dump_forest() {
        let var = |name: &str| std::env::var(name).ok();
        let num =
            |name: &str, default: u32| var(name).and_then(|v| v.parse().ok()).unwrap_or(default);
        let size = num("SOUL_INVASION_FOREST_SIZE", 768);
        let seed = var("SOUL_INVASION_SEED")
            .and_then(|v| v.parse().ok())
            .unwrap_or(crate::VALLEY_SEED);
        let scale = num("SOUL_INVASION_FOREST_SCALE", 1).max(1);
        let crop: Vec<u32> = var("SOUL_INVASION_FOREST_CROP")
            .map(|c| c.split(',').filter_map(|v| v.parse().ok()).collect())
            .filter(|c: &Vec<u32>| c.len() == 4)
            .unwrap_or_else(|| vec![0, 0, size, size]);
        let (x0, y0, w, h) = (crop[0], crop[1], crop[2], crop[3]);
        let kinds = species();
        let started = std::time::Instant::now();
        let map = Map::valley(size, size, seed);
        eprintln!("valley {size}: {:?}", started.elapsed());
        // Every chunk, timed.
        let started = std::time::Instant::now();
        let trees = everywhere(&map, &kinds, seed);
        let chunks = size.div_ceil(CHUNK_SIZE).pow(2);
        let spent = started.elapsed();
        eprintln!(
            "{} trees, {chunks} chunks: {:.1} µs a chunk",
            trees.len(),
            spent.as_secs_f64() * 1e6 / f64::from(chunks)
        );
        // The land (grass and forest: water, brooks and the cliff left
        // out), the forest's share of it, the trees a square metre in the
        // heart and the wood, the edge's share of the wooded land.
        let (mut land, mut forest, mut heart, mut wood, mut edge, mut wooded, mut rim) =
            (0_u64, 0_u64, 0_u64, 0_u64, 0_u64, 0_u64, 0_u64);
        for y in 0..size as i32 {
            for x in 0..size as i32 {
                let p = Pos::new(x, y);
                match map.get(p) {
                    Some(Terrain::Forest) => {
                        land += 1;
                        forest += 1;
                    }
                    Some(Terrain::Grass) => land += 1,
                    _ => {}
                }
                match map.habitat(p) {
                    Habitat::Heart => heart += 1,
                    Habitat::Wood | Habitat::CliffFoot => wood += 1,
                    Habitat::Edge => {
                        edge += 1;
                        let open = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
                            matches!(
                                map.habitat(Pos::new(x + dx, y + dy)),
                                Habitat::Meadow | Habitat::Clearing
                            )
                        });
                        rim += u64::from(open);
                    }
                    _ => {}
                }
                wooded += u64::from(map.trees(p) > 0);
            }
        }
        let (mut in_heart, mut in_wood) = (0_u64, 0_u64);
        for t in &trees {
            match map.habitat(t.at.tile()) {
                Habitat::Heart => in_heart += 1,
                Habitat::Wood | Habitat::CliffFoot => in_wood += 1,
                _ => {}
            }
        }
        eprintln!(
            "forest {} per mille of the land; wooded {} per mille; edge {} per mille of the wooded, {:.1} tiles deep; trees a 100 m²: heart {:.1}, wood {:.1}",
            forest * 1000 / land.max(1),
            wooded * 1000 / land.max(1),
            edge * 1000 / wooded.max(1),
            edge as f64 / rim.max(1) as f64,
            in_heart as f64 * 100.0 / heart.max(1) as f64,
            in_wood as f64 * 100.0 / wood.max(1) as f64,
        );
        // A chunk's ground, as the server makes it when it sends one: its
        // natural items, and its plants among the trees of it and its
        // eight neighbours; on every third chunk each way.
        let spawns: Vec<crate::content::Spawn> =
            ron::from_str(include_str!("../../../content/spawns.ron")).expect("spawns.ron parses");
        let flora: Vec<crate::flora::Kind> =
            crate::flora::parse(include_str!("../../../content/flora.ron"))
                .expect("flora.ron parses")
                .into_iter()
                .filter(|k| k.family != crate::flora::Family::Tree)
                .collect();
        let chunks_wide = size.div_ceil(CHUNK_SIZE) as i32;
        let (mut sampled, mut items, mut plants) = (0_u32, 0_usize, 0_usize);
        let (mut alone, mut ground) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
        for cy in (0..chunks_wide).step_by(3) {
            for cx in (0..chunks_wide).step_by(3) {
                let chunk = ChunkPos::new(cx, cy);
                let started = std::time::Instant::now();
                let own = trees_in(&map, &kinds, seed, chunk);
                alone += started.elapsed();
                let started = std::time::Instant::now();
                items += crate::items::natural_items(&map, &spawns, seed, chunk).len();
                let around: Vec<Tree> = (-1..=1)
                    .flat_map(|dy| (-1..=1).map(move |dx| ChunkPos::new(cx + dx, cy + dy)))
                    .flat_map(|c| trees_in(&map, &kinds, seed, c))
                    .collect();
                let near: Vec<crate::flora::TreeAt<'_>> = around
                    .iter()
                    .map(|t| crate::flora::TreeAt {
                        at: t.at,
                        species: kinds[t.species].id.as_str(),
                    })
                    .collect();
                plants += crate::flora::plants_in(&map, &flora, &near, seed, chunk).len();
                ground += started.elapsed();
                std::hint::black_box(own);
                sampled += 1;
            }
        }
        eprintln!(
            "{sampled} chunks sampled: trees_in {:.1} µs; the ground (items, then plants among the trees of nine chunks) {:.1} µs; {:.1} items, {:.1} plants a chunk",
            alone.as_secs_f64() * 1e6 / f64::from(sampled),
            ground.as_secs_f64() * 1e6 / f64::from(sampled),
            items as f64 / f64::from(sampled),
            plants as f64 / f64::from(sampled),
        );
        let mut counts = vec![0_usize; kinds.len()];
        for t in &trees {
            counts[t.species] += 1;
        }
        for (k, n) in kinds.iter().zip(&counts) {
            eprintln!("{:>10}: {n}", k.id);
        }
        let (pw, ph) = (w * scale, h * scale);
        let mut px = vec![0_u8; (pw * ph * 3) as usize];
        for ty in 0..h {
            for tx in 0..w {
                let rgb: [u8; 3] = match map.get(Pos::new((x0 + tx) as i32, (y0 + ty) as i32)) {
                    Some(Terrain::Forest) => [178, 184, 166],
                    Some(Terrain::Water) => [112, 132, 160],
                    Some(Terrain::Ford) => [140, 156, 176],
                    Some(Terrain::Brook) => [128, 150, 172],
                    Some(Terrain::Rock) => [152, 144, 138],
                    _ => [214, 214, 198],
                };
                for sy in 0..scale {
                    for sx in 0..scale {
                        let i = (((ty * scale + sy) * pw + tx * scale + sx) * 3) as usize;
                        px[i..i + 3].copy_from_slice(&rgb);
                    }
                }
            }
        }
        let per_mm = f64::from(scale) / f64::from(MM_PER_TILE);
        for t in &trees {
            let x = (f64::from(t.at.x) - f64::from(x0 * MM_PER_TILE as u32)) * per_mm;
            let y = (f64::from(t.at.y) - f64::from(y0 * MM_PER_TILE as u32)) * per_mm;
            let big = kinds[t.species].height_m >= 20.0;
            let r =
                (f64::from(scale) * if big { 1.1 } else { 0.6 }).max(if big { 2.2 } else { 1.3 });
            let rgb = COLOURS.get(t.species).copied().unwrap_or([128, 128, 128]);
            let (lx, hx) = ((x - r).floor() as i64, (x + r).ceil() as i64);
            let (ly, hy) = ((y - r).floor() as i64, (y + r).ceil() as i64);
            for py in ly..=hy {
                for pxx in lx..=hx {
                    let (dx, dy) = (pxx as f64 + 0.5 - x, py as f64 + 0.5 - y);
                    if dx * dx + dy * dy > r * r + 0.25
                        || pxx < 0
                        || py < 0
                        || pxx >= i64::from(pw)
                        || py >= i64::from(ph)
                    {
                        continue;
                    }
                    let i = ((py as u32 * pw + pxx as u32) * 3) as usize;
                    px[i..i + 3].copy_from_slice(&rgb);
                }
            }
        }
        let path = var("SOUL_INVASION_FOREST_DUMP").unwrap_or_else(|| "forest.ppm".into());
        let mut out = format!("P6\n{pw} {ph}\n255\n").into_bytes();
        out.extend_from_slice(&px);
        std::fs::write(path, out).unwrap();
    }
}
