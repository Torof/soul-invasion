//! The continent's safety net (the project lead's question of 15 November
//! 2026, after the vivarium's canal-like river and its empty plains:
//! "how do we make sure the whole continent will not have the same
//! problems?"): **measure, don't look**. The vivarium's zone is one cell
//! of a 2,097 km continent all drawn by the same generator (atlas →
//! region → window), so a rule is judged on a tiny zone by the lead, then
//! measured here on windows of one atlas cell (2 km) sampled over the
//! whole continent of seed 14, stratified by biome, river size, coast
//! and relief, the same cells every run.
//!
//! Every window is held to the invariants below, each a named constant
//! with its reason. A window that fails is **reported with its cell**,
//! never fixed here: this module measures the generator, it does not
//! correct it. `windows_sheet` draws the same windows for the lead's eye.
//!
//! The full sampling takes about half an hour in release (108 windows,
//! 16 s each with their trees, up to a minute on a great river), so it
//! is `#[ignore]`d and run by hand:
//!
//! ```text
//! cargo test --release -p sim --lib plan::net -- --ignored --nocapture
//! ```
//!
//! The vivarium's own cell runs in the ordinary suite, so the net is
//! never silently out of order.

// A measuring module: densities are read as floats (allowed in the
// tests, as the vivarium's are), and tile indices are small.
#![allow(
    clippy::float_arithmetic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]

use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::sync::OnceLock;

use super::course::{self, SHARPEST_TURN_COS_MILLI};
use super::measure::{self, Passage};
use super::region::REGION_CELL_M;
use super::{Biome, Continent, VIVARIUM_SEED, Window, ZONE_2, isqrt};
use crate::content::Species;
use crate::r#gen::hash;
use crate::map::{CHUNK_SIZE, ChunkPos, Map, Pos, Terrain};
use crate::open_ground::OpenGround;
use crate::trees::{self, Tree};
use crate::woods::Habitat;

// ------------------------------------------------------------ sampling

/// The purpose the sample's draws are keyed by (`gen::scatter` style).
const NET_PURPOSE: u64 = 0x91a7_4e7e;

/// Cells kept clear of the continent's edge: the region a window is cut
/// from reaches five cells each way, and the atlas's outline is sea
/// there anyway.
const EDGE_CELLS: i32 = 6;

/// A cell is hilly when the water surface over its 3 × 3 neighbourhood
/// spans this much (cm): 40 m over 6 km, the vivarium's hill; flat
/// under [`FLAT_CM`].
const HILLY_CM: i32 = 4_000;
const FLAT_CM: i32 = 1_000;

/// River classes by the cell's discharge (m³/s): none (under the atlas's
/// smallest river, 1 m³/s), small (to 40, the vivarium's 57 just above),
/// middling (to the great rivers), great (`measure::GREAT_RIVER_M3S`).
const SMALL_RIVER_M3S: u64 = 40;

/// Windows drawn per biome and stratum: three of the common kinds, two
/// of the rare, one of anything, for about a hundred windows over the
/// nine land biomes of seed 14 (many strata are empty: no great river
/// runs through its deserts).
const STRATA: [(&str, usize); 7] = [
    ("flat inland without a river", 3),
    ("river of 1 to 40 m³/s", 3),
    ("river of 40 to 500 m³/s", 3),
    ("great river", 2),
    ("coast", 2),
    ("hills", 3),
    ("any cell of the biome", 1),
];

// ---------------------------------------------------------- invariants

/// Trees a hectare on the forest's tiles, at least and at most. The lead
/// judged the vivarium's forest right at about 330 a hectare (measured
/// on its cell, 15 November 2026, light and dense regions mixed); a
/// forest biome under 100 would read as parkland, over 600 the trunks
/// would touch (the big grid is 6.25 m: 256 big trees a hectare at most,
/// the small ones between).
const FOREST_PER_HA: (f64, f64) = (100.0, 600.0);

/// The open ground's trees against what `content/open_ground.ron` asks,
/// as a share: the dense and light regions thin a place down to 40 %,
/// the groups along the woods and the water thicken it (the vivarium
/// test's tolerance).
const OPEN_SHARE: (f64, f64) = (0.4, 2.5);

/// A density is judged only over this much ground (ha): below, a window
/// holds too little of that kind of ground for a count to mean anything.
const LEAST_HA: f64 = 10.0;

/// The forest's fringe: the open ground within this many metres of the
/// forest (D37, 15 November 2026: "groups within 40 m of it, thinning
/// out"), and the fewest trees a hectare it may hold: under two, a
/// kilometre of edge shows fewer than eight trees, and the edge is a
/// wall.
const FRINGE_M: usize = 40;
const FRINGE_PER_HA: f64 = 2.0;

/// A river reach is straight when every point of its centreline lies
/// within a quarter of the river's width of the chord (at least a
/// metre): the eye reads such a bank as a ruled line. A straight reach
/// may run at most 300 m or ten widths, the longer: a 30 m river
/// straight over 300 m is the vivarium's canal; a 200 m river keeps its
/// line over two kilometres in nature.
const STRAIGHT_DEVIATION_PER_MILLE: i64 = 250;
const STRAIGHT_DEVIATION_LEAST_MM: i64 = 1_000;
const STRAIGHT_RUN_M: i64 = 300;
const STRAIGHT_RUN_WIDTHS: i64 = 10;

/// The banks must breathe: over any 500 m of a river (or 25 widths, the
/// longer, so a great river is judged at its scale), the widest width
/// over the narrowest is at least 1.15. The valley's walker moves a bank
/// a quarter of the width at least once a move, and both banks hold
/// their lines only at a cell's ends and around fords and junctions; a
/// reach that stays within 15 % reads as parallel (the lead's verdict on
/// PR #237).
const BREATH_SPAN_M: i64 = 500;
const BREATH_SPAN_WIDTHS: i64 = 25;
const BREATH_FLOOR_PER_MILLE: i64 = 1_150;

/// A step between two neighbouring tiles over this (dm) is a cliff, and
/// a window has no declared cliff: a body cannot climb 1.7 m in a step.
const STEP_DM: i32 = 17;

/// A pit is a dry tile lower than its eight neighbours, this far (tiles)
/// from any water: nearer, the ground drawn down to the water is meant
/// to be the low ground. A pit of one decimetre is the wire's rounding
/// of a smooth hollow (counted, reported); from [`PIT_DM`] it is a hole
/// water would stand in, and a basin the relief should not have.
const PIT_CLEAR_TILES: usize = 3;
const PIT_DM: i32 = 2;

/// The path over a ford is sought from this far off the water on each
/// bank (m, beyond the river's width) and within this reach (tiles).
const FORD_APPROACH_M: i64 = 40;
const FORD_REACH_TILES: i32 = 400;

/// The content's walking costs (`content/movement.ron`).
fn walk(terrain: Terrain) -> Option<u32> {
    match terrain {
        Terrain::Grass => Some(10),
        Terrain::Forest | Terrain::Brook => Some(15),
        Terrain::Ford => Some(20),
        Terrain::Water | Terrain::Rock => None,
    }
}

/// The continent of seed 14, born once for the module.
pub(crate) fn continent() -> &'static Continent {
    static CONTINENT: OnceLock<Continent> = OnceLock::new();
    CONTINENT.get_or_init(|| Continent::new(VIVARIUM_SEED))
}

/// The repository's kinds of tree and open biomes' trees.
pub(crate) fn content() -> (Vec<Species>, OpenGround) {
    (
        ron::from_str(include_str!("../../../../content/species.ron")).expect("species.ron"),
        ron::from_str(include_str!("../../../../content/open_ground.ron"))
            .expect("open_ground.ron"),
    )
}

/// The river class of a discharge in m³/s: 0 none, 1 small, 2 middling,
/// 3 great.
fn river_class(m3s: u64) -> usize {
    if m3s < 1 {
        0
    } else if m3s < SMALL_RIVER_M3S {
        1
    } else if m3s < measure::GREAT_RIVER_M3S {
        2
    } else {
        3
    }
}

/// The sampled cells of `c`: for every land biome present, the
/// highest-rolled cells of each stratum ([`STRATA`]), the vivarium's
/// cell first. Deterministic: the same continent gives the same cells.
pub(crate) fn sample(c: &Continent) -> Vec<(i32, i32)> {
    let grid = c.grid;
    let side = i32::try_from(grid.side).expect("small");
    // Per biome and stratum, the best `(roll, cell)` pairs.
    let mut best: BTreeMap<(u8, usize), Vec<(u64, usize)>> = BTreeMap::new();
    for i in 0..grid.len() {
        let (x, y) = grid.xy(i);
        if c.sea[i]
            || c.lake[i]
            || !Biome::LAND.contains(&c.biome[i])
            || x < EDGE_CELLS
            || y < EDGE_CELLS
            || x >= side - EDGE_CELLS
            || y >= side - EDGE_CELLS
        {
            continue;
        }
        let (mut lo, mut hi, mut coast) = (i32::MAX, i32::MIN, false);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let j = grid.index(x + dx, y + dy);
                coast |= c.sea[j];
                lo = lo.min(c.surface[j]);
                hi = hi.max(c.surface[j]);
            }
        }
        let class = river_class(c.cubic_metres(i));
        let fits = [
            class == 0 && hi - lo < FLAT_CM && !coast,
            class == 1,
            class == 2,
            class == 3,
            coast,
            hi - lo >= HILLY_CM,
            true,
        ];
        let roll = hash(&[c.seed, NET_PURPOSE, i as u64]);
        for (k, fit) in fits.iter().enumerate() {
            if *fit {
                best.entry((c.biome[i] as u8, k))
                    .or_default()
                    .push((roll, i));
            }
        }
    }
    let mut cells = vec![ZONE_2.cell];
    for ((_, k), list) in &mut best {
        list.sort_unstable_by(|a, b| b.cmp(a));
        for &(_, i) in list.iter().take(STRATA[*k].1) {
            let cell = grid.xy(i);
            if !cells.contains(&cell) {
                cells.push(cell);
            }
        }
    }
    cells
}

// -------------------------------------------------------------- judging

/// The invariants a window is held to (see the constants above).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Invariant {
    ForestDensity,
    OpenDensity,
    OpenBare,
    FringeWall,
    TreeInWater,
    SeaAsLand,
    Pits,
    Steps,
    SharpBend,
    StraightReach,
    ParallelBanks,
    FordDrawn,
    FordPath,
    BrookEnd,
}

/// The findings of 15 November 2026 on the vivarium's own cell, left as
/// they are (this module measures, it does not fix; the whole sample's
/// findings are in `docs/06-open-questions.md`). Struck the same day:
/// the walls at the rim of the region's river valleys (`Steps`; the
/// valleys have flanks since, `region::VALLEY_CELLS`) and the river's
/// centreline running straight for 675 m south of the crossing
/// (`StraightReach`; the course is thinned to the river's scale and
/// meandered since, `course::PIECE_WIDTHS`). Each is struck out here
/// the day the generator mends it, and the test says so.
const KNOWN_ON_THE_VIVARIUM_CELL: [Invariant; 0] = [];

/// What a window was measured at, and where it failed.
#[derive(Debug, Default)]
struct Verdict {
    cell: (i32, i32),
    biome: Option<Biome>,
    flow_m3s: u64,
    /// The numbers, for the report.
    measures: String,
    /// One line per invariant broken.
    failures: Vec<(Invariant, String)>,
}

impl Verdict {
    fn fail(&mut self, which: Invariant, line: String) {
        self.failures.push((which, line));
    }

    fn line(&self) -> String {
        let mut s = format!(
            "cell ({}, {}) {:?} {} m³/s:{}",
            self.cell.0, self.cell.1, self.biome, self.flow_m3s, self.measures
        );
        for (which, f) in &self.failures {
            let _ = write!(s, "\n    FAIL {which:?}: {f}");
        }
        s
    }
}

/// The tiles of a window read once: terrain, height (dm), biome and
/// whether the tile is open ground (a meadow of the valley's woods).
struct Tiles {
    side: usize,
    terrain: Vec<Terrain>,
    height: Vec<i16>,
    biome: Vec<Biome>,
    meadow: Vec<bool>,
}

impl Tiles {
    fn read(map: &Map) -> Self {
        let side = usize::try_from(map.width()).expect("small");
        let n = side * side;
        let mut tiles = Self {
            side,
            terrain: vec![Terrain::Rock; n],
            height: vec![0; n],
            biome: vec![Biome::Sea; n],
            meadow: vec![false; n],
        };
        let chunks = i32::try_from(map.chunks_wide()).expect("small");
        let cs = usize::try_from(CHUNK_SIZE).expect("small");
        for cy in 0..chunks {
            for cx in 0..chunks {
                let chunk = ChunkPos::new(cx, cy);
                let terrain = map.chunk(chunk).expect("a chunk of the window");
                let heights = map.chunk_heights(chunk).expect("a height per tile");
                let o = chunk.origin();
                for (k, (t, h)) in terrain.iter().zip(&heights).enumerate() {
                    let (x, y) = (
                        usize::try_from(o.x).expect("inside") + k % cs,
                        usize::try_from(o.y).expect("inside") + k / cs,
                    );
                    let i = y * side + x;
                    tiles.terrain[i] = *t;
                    tiles.height[i] = *h;
                    let p = Pos::new(
                        i32::try_from(x).expect("small"),
                        i32::try_from(y).expect("small"),
                    );
                    tiles.biome[i] = map.biome(p).expect("a biome on the atlas");
                    tiles.meadow[i] = map.habitat(p) == Habitat::Meadow;
                }
            }
        }
        tiles
    }

    fn index(&self, x: usize, y: usize) -> usize {
        y * self.side + x
    }

    fn wet(&self, i: usize) -> bool {
        matches!(
            self.terrain[i],
            Terrain::Water | Terrain::Ford | Terrain::Brook
        )
    }

    /// Chebyshev distance of every tile to the nearest tile `source`
    /// holds, up to `reach` (255 beyond).
    fn distance_to(&self, source: impl Fn(usize) -> bool, reach: u8) -> Vec<u8> {
        let n = self.side * self.side;
        let mut dist = vec![u8::MAX; n];
        let mut queue = VecDeque::new();
        for i in 0..n {
            if source(i) {
                dist[i] = 0;
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            let d = dist[i];
            if d >= reach {
                continue;
            }
            let (x, y) = (i % self.side, i / self.side);
            for dy in -1_i64..=1 {
                for dx in -1_i64..=1 {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= self.side as i64 || ny >= self.side as i64 {
                        continue;
                    }
                    let j = ny as usize * self.side + nx as usize;
                    if dist[j] == u8::MAX {
                        dist[j] = d + 1;
                        queue.push_back(j);
                    }
                }
            }
        }
        dist
    }
}

/// Hectares of `tiles` tiles.
#[allow(clippy::cast_precision_loss)]
fn ha(tiles: u64) -> f64 {
    tiles as f64 / 10_000.0
}

#[allow(clippy::cast_precision_loss)]
fn per_ha(count: usize, tiles: u64) -> f64 {
    count as f64 / ha(tiles)
}

/// The trees asked of a biome's open ground, a hectare.
fn asked_per_ha(open: &OpenGround, biome: Biome) -> Option<f64> {
    open.of(biome).map(|o| {
        (f64::from(o.lone_per_km2)
            + f64::from(o.groves_per_km2) * f64::from(o.grove.0 + o.grove.1) / 2.0)
            / 100.0
    })
}

/// The trees, the forest's density, the open ground's by biome, the
/// fringe, and no tree in or by the water.
fn judge_trees(verdict: &mut Verdict, map: &Map, tiles: &Tiles, trees: &[Tree], open: &OpenGround) {
    let side = tiles.side;
    let n = side * side;
    let fringe = tiles.distance_to(
        |i| tiles.terrain[i] == Terrain::Forest,
        u8::try_from(FRINGE_M).expect("small"),
    );
    let (mut forest_tiles, mut fringe_tiles) = (0_u64, 0_u64);
    let mut open_tiles: BTreeMap<Biome, u64> = BTreeMap::new();
    let mut below_sea = 0_u64;
    for i in 0..n {
        match tiles.terrain[i] {
            Terrain::Forest => forest_tiles += 1,
            Terrain::Grass => {
                if tiles.meadow[i] {
                    *open_tiles.entry(tiles.biome[i]).or_default() += 1;
                    if (1..=FRINGE_M as u8).contains(&fringe[i]) {
                        fringe_tiles += 1;
                    }
                }
                if tiles.height[i] < 0 {
                    below_sea += 1;
                }
            }
            _ => {}
        }
    }
    let (mut on_forest, mut on_fringe, mut wet_trees) = (0_usize, 0_usize, Vec::new());
    let mut on_open: BTreeMap<Biome, usize> = BTreeMap::new();
    for t in trees {
        let p = t.at.tile();
        let (x, y) = (
            usize::try_from(p.x).expect("on the map"),
            usize::try_from(p.y).expect("on the map"),
        );
        let i = tiles.index(x, y);
        match tiles.terrain[i] {
            Terrain::Forest => on_forest += 1,
            Terrain::Grass if tiles.meadow[i] => {
                *on_open.entry(tiles.biome[i]).or_default() += 1;
                if (1..=FRINGE_M as u8).contains(&fringe[i]) {
                    on_fringe += 1;
                }
            }
            _ => {}
        }
        let by_water = map.depth(p) > 0
            || (-1..=1).any(|dy| {
                (-1..=1).any(|dx| {
                    matches!(
                        map.get(Pos::new(p.x + dx, p.y + dy)),
                        Some(Terrain::Water | Terrain::Ford | Terrain::Brook)
                    )
                })
            });
        if by_water || !matches!(tiles.terrain[i], Terrain::Grass | Terrain::Forest) {
            wet_trees.push((x, y));
        }
    }
    let _ = write!(
        verdict.measures,
        " trees {} (forest {:.0}/ha on {:.0} ha; fringe {:.1}/ha on {:.1} ha)",
        trees.len(),
        per_ha(on_forest, forest_tiles.max(1)),
        ha(forest_tiles),
        per_ha(on_fringe, fringe_tiles.max(1)),
        ha(fringe_tiles),
    );
    if ha(forest_tiles) >= LEAST_HA {
        let d = per_ha(on_forest, forest_tiles);
        if !(FOREST_PER_HA.0..=FOREST_PER_HA.1).contains(&d) {
            verdict.fail(
                Invariant::ForestDensity,
                format!(
                    "forest density {d:.0}/ha on {:.0} ha, {} to {} wanted",
                    ha(forest_tiles),
                    FOREST_PER_HA.0,
                    FOREST_PER_HA.1
                ),
            );
        }
    }
    if ha(fringe_tiles) >= LEAST_HA / 5.0 && per_ha(on_fringe, fringe_tiles) < FRINGE_PER_HA {
        verdict.fail(Invariant::FringeWall, format!(
            "forest edge a wall: {:.1} trees/ha on the {FRINGE_M} m fringe ({:.1} ha), {FRINGE_PER_HA} wanted",
            per_ha(on_fringe, fringe_tiles),
            ha(fringe_tiles)
        ));
    }
    for (biome, tiles) in &open_tiles {
        let count = on_open.get(biome).copied().unwrap_or(0);
        let d = per_ha(count, *tiles);
        let asked = asked_per_ha(open, *biome);
        let _ = write!(
            verdict.measures,
            "; open {biome:?} {d:.1}/ha on {:.0} ha{}",
            ha(*tiles),
            asked.map_or_else(String::new, |a| format!(" ({a:.1} asked)"))
        );
        if ha(*tiles) < LEAST_HA {
            continue;
        }
        if let Some(asked) = asked {
            if d < asked * OPEN_SHARE.0 || d > asked * OPEN_SHARE.1 {
                verdict.fail(Invariant::OpenDensity, format!(
                    "open {biome:?} {d:.1} trees/ha on {:.0} ha, {asked:.1} asked ({} to {} of it allowed)",
                    ha(*tiles),
                    OPEN_SHARE.0,
                    OPEN_SHARE.1
                ));
            }
            if count == 0 && matches!(biome, Biome::Grassland | Biome::WoodedSteppe) {
                verdict.fail(
                    Invariant::OpenBare,
                    format!("open {biome:?} bare: no tree on {:.0} ha", ha(*tiles)),
                );
            }
        }
    }
    if !wet_trees.is_empty() {
        verdict.fail(
            Invariant::TreeInWater,
            format!(
                "{} trees in or by the water, the first at {:?}",
                wet_trees.len(),
                &wet_trees[..wet_trees.len().min(3)]
            ),
        );
    }
    if below_sea > 0 {
        verdict.fail(
            Invariant::SeaAsLand,
            format!("{below_sea} dry tiles below the sea's level (the sea drawn as land)"),
        );
    }
}

/// Pits and steps in the heights, away from the water.
fn judge_heights(verdict: &mut Verdict, tiles: &Tiles) {
    let side = tiles.side;
    let near_water = tiles.distance_to(
        |i| tiles.wet(i),
        u8::try_from(PIT_CLEAR_TILES).expect("small"),
    );
    let (mut pits, mut deep_pits, mut deepest, mut first_pit) = (0_u64, 0_u64, 0_i32, None);
    let (mut steps, mut steepest, mut first_step) = (0_u64, 0_i32, None);
    for y in 1..side - 1 {
        for x in 1..side - 1 {
            let i = tiles.index(x, y);
            let h = i32::from(tiles.height[i]);
            // Steps to the right and down, every pair once.
            for j in [i + 1, i + side] {
                let d = (i32::from(tiles.height[j]) - h).abs();
                if d > steepest {
                    steepest = d;
                }
                if d > STEP_DM {
                    steps += 1;
                    first_step.get_or_insert((x, y, d));
                }
            }
            if tiles.wet(i) || near_water[i] <= PIT_CLEAR_TILES as u8 {
                continue;
            }
            let mut lowest_around = i32::MAX;
            for dy in -1_i64..=1 {
                for dx in -1_i64..=1 {
                    if (dx, dy) != (0, 0) {
                        let j = ((y as i64 + dy) as usize) * side + (x as i64 + dx) as usize;
                        lowest_around = lowest_around.min(i32::from(tiles.height[j]));
                    }
                }
            }
            if h < lowest_around {
                pits += 1;
                deepest = deepest.max(lowest_around - h);
                if lowest_around - h >= PIT_DM {
                    deep_pits += 1;
                    first_pit.get_or_insert((x, y, lowest_around - h));
                }
            }
        }
    }
    let _ = write!(
        verdict.measures,
        "; pits {pits} (deepest {deepest} dm); steepest step {steepest} dm"
    );
    if deep_pits > 0 {
        verdict.fail(Invariant::Pits, format!(
            "{deep_pits} pits of {PIT_DM} dm or more off the water (a tile lower than its eight neighbours), deepest {deepest} dm, first at {:?}",
            first_pit.expect("one")
        ));
    }
    if steps > 0 {
        verdict.fail(Invariant::Steps, format!(
            "{steps} steps over {STEP_DM} dm between neighbouring tiles with no cliff declared, steepest {steepest} dm, first at {:?}",
            first_step.expect("one")
        ));
    }
}

/// The river courses through the window, in the window's millimetres,
/// each a run of consecutive nodes inside it.
fn runs_in(c: &Continent, window: &Window, cell: (i32, i32)) -> Vec<Vec<course::Node>> {
    let origin = window.origin();
    let shift = (
        origin.0 * REGION_CELL_M * 1000,
        origin.1 * REGION_CELL_M * 1000,
    );
    let side_mm = i64::from(window.side()) * 1000;
    let inside = |p: (i64, i64)| (0..side_mm).contains(&p.0) && (0..side_mm).contains(&p.1);
    let mut runs = Vec::new();
    // The span the window reads its stems over (`Window::cut`).
    for stem in course::stems(c, cell.0 - 2..cell.0 + 4, cell.1 - 2..cell.1 + 4, None) {
        let mut run: Vec<course::Node> = Vec::new();
        for node in &stem.nodes {
            let p = (node.p.0 - shift.0, node.p.1 - shift.1);
            if inside(p) {
                let mut n = *node;
                n.p = p;
                run.push(n);
            } else if !run.is_empty() {
                runs.push(std::mem::take(&mut run));
            }
        }
        if !run.is_empty() {
            runs.push(run);
        }
    }
    runs
}

fn distance(a: (i64, i64), b: (i64, i64)) -> i64 {
    i64::try_from(isqrt(
        u64::try_from((a.0 - b.0).pow(2) + (a.1 - b.1).pow(2)).expect("a square"),
    ))
    .expect("small")
}

/// How far `p` lies off the line through `a` and `b` (mm).
fn off_line(a: (i64, i64), b: (i64, i64), p: (i64, i64)) -> i64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = distance(a, b).max(1);
    ((dx * (p.1 - a.1) - dy * (p.0 - a.0)) / len).abs()
}

/// The longest straight reach of `run` (mm): the longest chord every
/// node between whose ends lies within the deviation allowed.
fn longest_straight(run: &[course::Node]) -> i64 {
    let mut longest = 0;
    for i in 0..run.len() {
        let tolerance =
            (run[i].width * STRAIGHT_DEVIATION_PER_MILLE / 1000).max(STRAIGHT_DEVIATION_LEAST_MM);
        for j in i + 1..run.len() {
            let straight = (i + 1..j).all(|k| off_line(run[i].p, run[j].p, run[k].p) <= tolerance);
            if !straight {
                break;
            }
            longest = longest.max(distance(run[i].p, run[j].p));
        }
    }
    longest
}

/// The rivers: no bend sharper than the course rule, no straight reach
/// over the limit, and banks that breathe over every span.
fn judge_rivers(verdict: &mut Verdict, runs: &[Vec<course::Node>]) {
    let (mut longest, mut widest_width, mut least_breath) = (0_i64, 0_i64, i64::MAX);
    let mut spans_judged = 0;
    for run in runs {
        if run.len() < 3 {
            continue;
        }
        if !course::course_turns_within(run, SHARPEST_TURN_COS_MILLI) {
            verdict.fail(
                Invariant::SharpBend,
                format!(
                    "a bend sharper than 60° on the river near ({}, {})",
                    run[run.len() / 2].p.0 / 1000,
                    run[run.len() / 2].p.1 / 1000
                ),
            );
        }
        let width = run.iter().map(|n| n.width).max().unwrap_or(0);
        let straight = longest_straight(run);
        let allowed = (STRAIGHT_RUN_M * 1000).max(STRAIGHT_RUN_WIDTHS * width);
        if straight > allowed {
            verdict.fail(
                Invariant::StraightReach,
                format!(
                    "a straight reach of {} m on a river {} m wide near ({}, {}), {} m allowed",
                    straight / 1000,
                    width / 1000,
                    run[run.len() / 2].p.0 / 1000,
                    run[run.len() / 2].p.1 / 1000,
                    allowed / 1000
                ),
            );
        }
        longest = longest.max(straight);
        widest_width = widest_width.max(width);
        // The breath: along-distances, then every span.
        let mut along = vec![0_i64; run.len()];
        for k in 1..run.len() {
            along[k] = along[k - 1] + distance(run[k - 1].p, run[k].p);
        }
        for i in 0..run.len() {
            if run[i].ford {
                continue;
            }
            let span = (BREATH_SPAN_M * 1000).max(BREATH_SPAN_WIDTHS * run[i].width);
            if along[run.len() - 1] - along[i] < span {
                break;
            }
            let (mut lo, mut hi) = (i64::MAX, 0);
            for k in i..run.len() {
                if along[k] - along[i] > span {
                    break;
                }
                if !run[k].ford {
                    let w = run[k].half.0 + run[k].half.1;
                    lo = lo.min(w);
                    hi = hi.max(w);
                }
            }
            if hi > 0 {
                let breath = hi * 1000 / lo.max(1);
                spans_judged += 1;
                if breath < least_breath {
                    least_breath = breath;
                }
                if breath < BREATH_FLOOR_PER_MILLE {
                    verdict.fail(Invariant::ParallelBanks, format!(
                        "banks parallel: width within {breath} per mille over {} m from ({}, {}), {BREATH_FLOOR_PER_MILLE} wanted",
                        span / 1000,
                        run[i].p.0 / 1000,
                        run[i].p.1 / 1000
                    ));
                    break;
                }
            }
        }
    }
    let _ = write!(
        verdict.measures,
        "; river runs {} (widest {} m, longest straight {} m, least breath {} per mille over {spans_judged} spans)",
        runs.len(),
        widest_width / 1000,
        longest / 1000,
        if least_breath == i64::MAX {
            0
        } else {
            least_breath
        }
    );
}

/// The nearest dry tile to `(x, y)`, searched in growing squares.
fn dry_near(tiles: &Tiles, x: i64, y: i64) -> Option<Pos> {
    let side = tiles.side as i64;
    for r in 0..100_i64 {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dy.abs() != r {
                    continue;
                }
                let (nx, ny) = (x + dx, y + dy);
                if (1..side - 1).contains(&nx)
                    && (1..side - 1).contains(&ny)
                    && !tiles.wet(tiles.index(nx as usize, ny as usize))
                {
                    return Some(Pos::new(
                        i32::try_from(nx).expect("small"),
                        i32::try_from(ny).expect("small"),
                    ));
                }
            }
        }
    }
    None
}

/// The atlas crossing of the window's cell is drawn as a ford and a path
/// wades it from bank to bank.
fn judge_ford(
    verdict: &mut Verdict,
    map: &Map,
    tiles: &Tiles,
    runs: &[Vec<course::Node>],
    crossing: bool,
) {
    let side = tiles.side;
    let fords: Vec<(usize, usize)> = (0..side * side)
        .filter(|&i| tiles.terrain[i] == Terrain::Ford)
        .map(|i| (i % side, i / side))
        .collect();
    let _ = write!(
        verdict.measures,
        "; crossing {crossing}, ford tiles {}",
        fords.len()
    );
    if !crossing {
        return;
    }
    if fords.is_empty() {
        verdict.fail(
            Invariant::FordDrawn,
            "the atlas crossing is drawn with no ford".into(),
        );
        return;
    }
    let n = fords.len() as i64;
    let (fx, fy) = fords.iter().fold((0_i64, 0_i64), |s, &(x, y)| {
        (s.0 + x as i64, s.1 + y as i64)
    });
    let f = (fx / n * 1000 + 500, fy / n * 1000 + 500);
    // The river's direction at the ford: the nearest node's segment.
    let mut nearest: Option<(i64, (i64, i64), i64)> = None;
    for run in runs {
        for pair in run.windows(2) {
            let d = distance(pair[0].p, f);
            if nearest.is_none_or(|(bd, _, _)| d < bd) {
                nearest = Some((
                    d,
                    (pair[1].p.0 - pair[0].p.0, pair[1].p.1 - pair[0].p.1),
                    pair[0].width,
                ));
            }
        }
    }
    let Some((_, (dx, dy), width)) = nearest else {
        verdict.fail(
            Invariant::FordPath,
            "a ford with no river course near it".into(),
        );
        return;
    };
    let len = distance((0, 0), (dx, dy)).max(1);
    let off = width + FORD_APPROACH_M * 1000;
    let banks = [
        (f.0 - dy * off / len, f.1 + dx * off / len),
        (f.0 + dy * off / len, f.1 - dx * off / len),
    ]
    .map(|(x, y)| dry_near(tiles, x.div_euclid(1000), y.div_euclid(1000)));
    let (Some(a), Some(b)) = (banks[0], banks[1]) else {
        verdict.fail(
            Invariant::FordPath,
            "no dry ground found on both banks of the ford".into(),
        );
        return;
    };
    match map.path_within(a, b, FORD_REACH_TILES, u32::MAX, walk) {
        Some(path) if path.iter().any(|p| map.get(*p) == Some(Terrain::Ford)) => {}
        Some(_) => verdict.fail(
            Invariant::FordPath,
            format!("a path from {a:?} to {b:?} crosses the river without wading the ford"),
        ),
        None => verdict.fail(
            Invariant::FordPath,
            format!(
                "no path from {a:?} to {b:?} over the ford at ({}, {})",
                f.0 / 1000,
                f.1 / 1000
            ),
        ),
    }
}

/// Every brook ends in water or off the window.
fn judge_brooks(verdict: &mut Verdict, window: &Window, tiles: &Tiles) {
    let side = tiles.side as i64;
    let ends = window.brook_ends();
    let dry: Vec<(i64, i64)> = ends
        .iter()
        .copied()
        .filter(|&(x, y)| {
            (0..side).contains(&x)
                && (0..side).contains(&y)
                && !tiles.wet(tiles.index(x as usize, y as usize))
        })
        .collect();
    let _ = write!(verdict.measures, "; brook ends {}", ends.len());
    if !dry.is_empty() {
        verdict.fail(
            Invariant::BrookEnd,
            format!(
                "{} brooks end on dry ground, the first at {:?}",
                dry.len(),
                &dry[..dry.len().min(3)]
            ),
        );
    }
}

/// The window of `cell` measured against every invariant.
fn judge(
    c: &Continent,
    passages: &[Passage],
    cell: (i32, i32),
    species: &[Species],
    open: &OpenGround,
) -> Verdict {
    let i = c.grid.index(cell.0, cell.1);
    let mut verdict = Verdict {
        cell,
        biome: Some(c.biome[i]),
        flow_m3s: c.cubic_metres(i),
        ..Verdict::default()
    };
    let window = Window::of_cell(c, cell);
    let map = Map::on_window(window.clone(), c.seed);
    let tiles = Tiles::read(&map);
    let trees = trees::everywhere(&map, species, open, c.seed);
    judge_trees(&mut verdict, &map, &tiles, &trees, open);
    judge_heights(&mut verdict, &tiles);
    let runs = runs_in(c, &window, cell);
    judge_rivers(&mut verdict, &runs);
    judge_ford(
        &mut verdict,
        &map,
        &tiles,
        &runs,
        passages[i] == Passage::Crossing,
    );
    judge_brooks(&mut verdict, &window, &tiles);
    verdict
}

/// Judges `cells`, prints every verdict, and fails when a window broke
/// an invariant not in `known`, naming them all; and when an invariant
/// of `known` no longer breaks anywhere (a finding mended: strike it).
fn net_over(cells: &[(i32, i32)], known: &[Invariant]) {
    let c = continent();
    let passages = measure::passages(c, measure::DRY_CM);
    let (species, open) = content();
    let mut failed = Vec::new();
    for (k, &cell) in cells.iter().enumerate() {
        let start = std::time::Instant::now();
        let verdict = judge(c, &passages, cell, &species, &open);
        eprintln!(
            "[{}/{}] {} ({} ms)",
            k + 1,
            cells.len(),
            verdict.line(),
            start.elapsed().as_millis()
        );
        if !verdict.failures.is_empty() {
            failed.push(verdict);
        }
    }
    let mended: Vec<Invariant> = known
        .iter()
        .copied()
        .filter(|k| {
            !failed
                .iter()
                .any(|v| v.failures.iter().any(|(w, _)| w == k))
        })
        .collect();
    assert!(
        mended.is_empty(),
        "{mended:?} no longer break on these windows: strike them from the known findings"
    );
    let unexpected: Vec<&Verdict> = failed
        .iter()
        .filter(|v| v.failures.iter().any(|(w, _)| !known.contains(w)))
        .collect();
    assert!(
        unexpected.is_empty(),
        "{} of {} windows break an invariant:\n{}",
        unexpected.len(),
        cells.len(),
        unexpected
            .iter()
            .map(|v| v.line())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The sample is stratified and deterministic: about a hundred cells,
/// every land biome of the continent among them, the vivarium's first.
#[test]
fn the_sample_covers_the_continent() {
    let c = continent();
    let cells = sample(c);
    assert_eq!(cells, sample(c), "the same cells every run");
    assert_eq!(cells[0], ZONE_2.cell);
    assert!(
        (60..=140).contains(&cells.len()),
        "{} windows sampled",
        cells.len()
    );
    let biomes: std::collections::BTreeSet<Biome> = cells
        .iter()
        .map(|&(x, y)| c.biome[c.grid.index(x, y)])
        .collect();
    let present: std::collections::BTreeSet<Biome> = Biome::LAND
        .iter()
        .copied()
        .filter(|b| c.biome.iter().filter(|&&x| x == *b).count() >= 50)
        .collect();
    assert!(
        present.is_subset(&biomes),
        "every land biome present is sampled: {biomes:?} of {present:?}"
    );
    let mut by_class = [0; 4];
    for &(x, y) in &cells {
        by_class[river_class(c.cubic_metres(c.grid.index(x, y)))] += 1;
    }
    assert!(
        by_class.iter().all(|&n| n > 0),
        "every river class sampled: {by_class:?}"
    );
    eprintln!("{} windows: {cells:?}", cells.len());
}

/// The vivarium's own cell holds every invariant but the known findings:
/// the net's quick check, in the ordinary suite.
#[test]
fn the_vivarium_cell_holds_the_invariants() {
    net_over(&[ZONE_2.cell], &KNOWN_ON_THE_VIVARIUM_CELL);
}

/// The heights alone of a window (no trees: seconds), judged.
fn judge_heights_of(cell: (i32, i32)) -> Verdict {
    let c = continent();
    let window = Window::of_cell(c, cell);
    let map = Map::on_window(window, c.seed);
    let tiles = Tiles::read(&map);
    let mut verdict = Verdict {
        cell,
        ..Verdict::default()
    };
    judge_heights(&mut verdict, &tiles);
    verdict
}

/// The mountain cell the first sampling found worst (47 m a tile,
/// 137,677 steps, 15 November 2026): no wall since the region's valleys
/// have flanks, the rivers settle on the ground and the ground has its
/// ceiling (`region::MAX_RISE_PER_MILLE`); and no step at all since the
/// window's waters pull the ground by a mean that drops none of them
/// between two tiles, its brooks run on the ground they cross and end
/// where they meet a river (seven steps of 1.8 m beside converging
/// brooks the same day, blamed on the diagonal at first).
#[test]
fn the_mountain_cell_has_no_wall() {
    let verdict = judge_heights_of((631, 624));
    eprintln!("{}", verdict.line());
    assert!(
        verdict.failures.iter().all(|(w, _)| *w != Invariant::Steps),
        "{}",
        verdict.line()
    );
}

/// The windows of the sample whose ground stepped on 15 November 2026
/// after the rivers took their natural courses (nine of 1.8 to 2.7 m
/// on bare flanks, three in the mountains at 3.5 to 4.4 m, two of a
/// step or so), measured that day: every step stood beside water, none
/// came from the ground's ceiling or the interpolation between cells.
/// The waters' pull dropped a water from its mean between two tiles
/// (`window::pulled`), a brook's level fell evenly along a line that
/// wandered across a flank (22 m under the ground), a tile of a steep
/// river took the level of a stretch half a width downstream, and a
/// brook ran up a falling river to its receiver's point 2 m under the
/// surface. Judged on their heights alone: no step left.
#[test]
#[ignore = "fourteen windows' heights, minutes in release; run by hand: cargo test --release -p sim --lib plan::net::the_windows_whose -- --ignored --nocapture"]
fn the_windows_whose_flanks_stepped_hold_no_step() {
    const STEPPED: [(i32, i32); 14] = [
        (224, 504),
        (234, 464),
        (389, 474),
        (396, 467),
        (436, 520),
        (583, 581),
        (605, 580),
        (623, 599),
        (627, 625),
        (629, 624),
        (631, 624),
        (697, 94),
        (724, 201),
        (787, 719),
    ];
    let sampled = sample(continent());
    let mut failed = Vec::new();
    for &cell in &STEPPED {
        assert!(
            sampled.contains(&cell),
            "{cell:?} is a window of the sample"
        );
        let verdict = judge_heights_of(cell);
        eprintln!("{}", verdict.line());
        if verdict.failures.iter().any(|(w, _)| *w == Invariant::Steps) {
            failed.push(verdict.line());
        }
    }
    assert!(
        failed.is_empty(),
        "{} of {} windows still step:\n{}",
        failed.len(),
        STEPPED.len(),
        failed.join("\n")
    );
}

/// The region's ground never rises faster than its ceiling between two
/// cells, on the vivarium's region: the window's tiles, interpolated
/// between cells, then never step over the net's 1.7 m.
#[test]
fn the_vivarium_region_rises_under_the_ceiling() {
    let c = continent();
    let region = super::Region::around(c, ZONE_2.cell, 10 * 32);
    let side = i32::try_from(region.grid.side).expect("small");
    let rise =
        i32::try_from(super::region::MAX_RISE_PER_MILLE * REGION_CELL_M / 10).expect("small");
    let (mut orth, mut diag) = (0, 0);
    for i in 0..region.grid.len() {
        let (x, y) = region.grid.xy(i);
        for (dx, dy) in [(1, 0), (0, 1), (1, 1), (1, -1)] {
            if x + dx < side && (0..side).contains(&(y + dy)) {
                let d = (region.height[region.grid.index(x + dx, y + dy)] - region.height[i]).abs();
                if dx != 0 && dy != 0 {
                    diag = diag.max(d);
                } else {
                    orth = orth.max(d);
                }
            }
        }
    }
    eprintln!("steepest rise between cells: {orth} cm orthogonal, {diag} cm diagonal");
    assert!(orth <= rise, "{orth} cm a cell over the ceiling {rise}");
    assert!(diag <= rise * 1414 / 1000, "{diag} cm over a diagonal");
}

/// A tile is the same through two windows (D39): the vivarium's cell cut
/// from its own region and from the region centred one cell east, whose
/// stems, valleys and ceiling were worked out over a span shifted by
/// 2 km, give the same heights and the same waters.
#[test]
fn a_tile_is_the_same_through_two_windows() {
    let c = continent();
    let own = Window::of_cell(c, ZONE_2.cell);
    let per = usize::try_from(super::region::PER_CELL).expect("positive");
    let shifted = super::Region::around(c, (ZONE_2.cell.0 + 1, ZONE_2.cell.1), 10 * per);
    let corner = (
        usize::try_from(ZONE_2.cell.0 - shifted.origin.0).expect("inside") * per,
        usize::try_from(ZONE_2.cell.1 - shifted.origin.1).expect("inside") * per,
    );
    let other = Window::cut(c, &shifted, corner, own.side());
    assert_eq!(own.origin(), other.origin());
    let side = i64::from(own.side());
    let (mut tiles, mut differ, mut worst) = (0, 0, 0);
    for y in (3..side).step_by(7) {
        for x in (5..side).step_by(7) {
            tiles += 1;
            let (a, b) = (own.height_dm(x, y), other.height_dm(x, y));
            if a != b || own.wet(x, y).map(|w| w.terrain) != other.wet(x, y).map(|w| w.terrain) {
                differ += 1;
                worst = worst.max((i32::from(a) - i32::from(b)).abs());
            }
        }
    }
    eprintln!("{tiles} tiles compared, {differ} differ, worst {worst} dm");
    assert_eq!(
        differ, 0,
        "{differ} of {tiles} tiles differ, worst {worst} dm"
    );
}

/// The whole sample (minutes: run by hand, see the module).
#[test]
#[ignore = "about a hundred windows with their trees, minutes; run by hand: cargo test --release -p sim --lib plan::net -- --ignored --nocapture"]
fn the_sampled_windows_hold_the_invariants() {
    net_over(&sample(continent()), &[]);
}

/// The 26 windows of the sample whose rivers ran straight on 15 November
/// 2026 (a quarter width off a chord over 300 to 1,177 m; the brooks and
/// the great rivers' banks of the same finding are judged by the
/// `course` and `window` tests), judged on their rivers alone: none runs
/// straight, bends or stops breathing since the course is thinned to
/// the river's scale and meandered (`course::PIECE_WIDTHS`).
#[test]
#[ignore = "26 windows' rivers, about a minute in release; run by hand: cargo test --release -p sim --lib plan::net::the_formerly_straight -- --ignored --nocapture"]
fn the_formerly_straight_windows_meander() {
    const FORMERLY_STRAIGHT: [(i32, i32); 26] = [
        (141, 609),
        (178, 659),
        (222, 683),
        (391, 441),
        (391, 717),
        (433, 351),
        (436, 520),
        (438, 376),
        (440, 763),
        (465, 374),
        (465, 576),
        (528, 333),
        (538, 659),
        (558, 483),
        (563, 672),
        (600, 438),
        (602, 385),
        (611, 351),
        (617, 149),
        (628, 621),
        (697, 709),
        (715, 822),
        (716, 491),
        (724, 201),
        (743, 328),
        (784, 582),
    ];
    let c = continent();
    let mut failed = Vec::new();
    for &cell in &FORMERLY_STRAIGHT {
        let window = Window::of_cell(c, cell);
        let mut verdict = Verdict {
            cell,
            ..Verdict::default()
        };
        judge_rivers(&mut verdict, &runs_in(c, &window, cell));
        eprintln!("{}", verdict.line());
        if !verdict.failures.is_empty() {
            failed.push(verdict.line());
        }
    }
    assert!(
        failed.is_empty(),
        "{} of {} windows still break a river invariant:\n{}",
        failed.len(),
        FORMERLY_STRAIGHT.len(),
        failed.join("\n")
    );
}
