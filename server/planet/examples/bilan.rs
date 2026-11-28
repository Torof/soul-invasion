//! The planet's balance sheet: one command, one stable table, written as
//! a JSON file per version of the model. Every later round of work on the
//! plates adds a row here, never a new program.
//!
//! ```text
//! cargo run --release -p planet --example bilan -- \
//!     --settings monde-defaut.ron --seeds 1-30 --version v10 --out bilan-v10.json
//! ```
//!
//! The rows: the land/sea flips per step (all, far from the plates'
//! edges) and their share by cause; whether the rigid pieces of
//! continent stay unchanged; the plates by size class and the
//! short-lived ones; the continental crust kept (area, volume); the
//! ground below sea level inside the continents (linked to the ocean or
//! not) and what makes it change; whether a continent moves with its
//! plate; the rifts (how sinuous the cuts, how far the first cut strays
//! from a straight line, the share of its path in the belts of the
//! mosaic); the collisions of continents; the engine (when the plates
//! start to move, the trenches and ridges through time, the ocean
//! floor's age); the relief and the sea (high ground, the crust under
//! water); the plates' names; the seconds a planet takes.
//!
//! The planets are made on every core (`PLANET_THREADS` to change it),
//! each counted alone, and their counts added in the order of the seeds:
//! the table does not depend on the threads. Only its last row does, the
//! seconds a planet takes while the other cores work too; for a planet's
//! time alone, `PLANET_THREADS=1`.

#![recursion_limit = "256"]
#![allow(
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::needless_range_loop,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::struct_excessive_bools,
    clippy::cast_possible_wrap,
    clippy::naive_bytecount,
    reason = "a measuring tool: one long count, written as rounded shares"
)]

use planet::history::{self, MAXP, OCEAN, Piece, What};
use planet::{CELLS, Planet, Settings, V3, dot, km_between, normalize};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

#[path = "common/par.rs"]
mod par;

const CAUSES: [&str; 16] = [
    "the sea level alone",
    "a rift stretched it",
    "a stretched margin cooled",
    "an arc built",
    "a terrane docked",
    "a plume lifted it",
    "it wore down",
    "it levelled with its neighbours",
    "sediment filled it",
    "a collision's pile rose",
    "a pile spread or wore down",
    "flood basalt",
    "height changed with no event",
    "it came out from under another piece",
    "new crust (silt, an arc's island)",
    "a hollow's link to the ocean changed",
];
const SWITCHES: [&str; 9] = [
    "unnamed",
    "weld after a collision",
    "block escaping a collision (no such thing since v15)",
    "terrane or arc docking",
    "lone cell on the grid's view",
    "large part cut off: its own plate, same motion",
    "small part cut off, taken by the plate beside",
    "crumb or enclosed small plate taken whole",
    "rift",
];
/// Patches of water, km²: small, medium, large, huge.
const SIZES: [(&str, i64); 4] = [
    ("under 50 k km2", 50_000),
    ("50-200 k km2", 200_000),
    ("200 k-1 M km2", 1_000_000),
    ("over 1 M km2", i64::MAX),
];
/// Plates, km²: micro, small, medium, large (the Earth: about 7 large,
/// 8 medium).
const PLATE_KM2: [i64; 3] = [500_000, 2_000_000, 20_000_000];
const PLATE_NAMES: [&str; 4] = [
    "micro under 0.5 M km2",
    "small 0.5-2 M",
    "medium 2-20 M",
    "large over 20 M",
];
/// Water is inland when no nearer than this to the ocean's floor, km.
const INLAND_KM: u32 = 300;
/// Far from the plates' edges, km.
const FAR_KM: u32 = 1000;
/// A cell of crust slides on its plate when it ends a step farther than
/// this from where its plate's turn carries it, km.
const SLIDE_KM: i64 = 2;
/// The plates move once their mean speed, weighted by their areas,
/// reaches this, mm a year.
const MOVING_MM: i64 = 10;
/// A late start: this step or later.
const LATE_STEP: i64 = 16;
/// Ocean floor older than this, My, is old (the Earth has almost none).
const OLD_FLOOR_MY: i64 = 200;
/// The boundary kinds of `History::kind` counted for the engine: a
/// ridge, and the side of a trench that sinks.
const KIND_RIDGE: u8 = 1;
const KIND_TRENCH: u8 = 4;

/// A cell of crust at a step.
#[derive(Clone, Copy)]
struct Seen {
    /// Dry as the model paints it; below the sea's level.
    dry: bool,
    low: bool,
    far: bool,
    under: bool,
    high: i64,
    pile: i64,
    lip: i64,
    pos: V3,
    own: u8,
    /// 0 dry, 1 hollow with no path to the ocean, 2 inland water linked
    /// to the ocean, 3 water near the ocean.
    water: u8,
    /// Its hollow's size class (when `water` is 1).
    size: u8,
}

fn back(p: &Piece, x: V3) -> V3 {
    let m = p.turn;
    normalize([0, 1, 2].map(|j| dot([m[0][j], m[1][j], m[2][j]], x)))
}

#[derive(Default)]
struct Count {
    steps: u64,
    planets: u64,
    followed: u64,
    far_followed: u64,
    flips: [u64; 16],
    far_flips: [u64; 16],
    // Rigid pieces of 30 cells or more: followed and untouched over 1, 5
    // and 20 steps; kept cells moved in their lattice, worst offset km.
    rigid: [[u64; 2]; 3],
    rigid_moved: u64,
    rigid_off: u64,
    plates_today: [u64; 4],
    plates_through: [u64; 4],
    lives: [u64; 4],
    plates_start: u64,
    // Hollows (no path to the ocean) and inland water linked to it.
    hollows: [u64; 4],
    hollow_km2: u64,
    hollow_painted_sea_km2: u64,
    linked_km2: u64,
    // Ground gone below the sea's level, and back above it, km², by
    // where (hollow under 200 k km², larger hollow, linked inland water)
    // and by cause.
    sunk: [[u64; 16]; 3],
    risen: [[u64; 16]; 3],
    // A hollow reached by the sea, and inland water cut from it, km².
    reached: u64,
    cut: u64,
    // Cells of crust followed for their motion, those that slid, those
    // that changed plate by cause, of them those then moving otherwise.
    moved: u64,
    slid: u64,
    slid_worst_km: i64,
    switched: [u64; 9],
    rifts: [u64; 5],
    rift_my: Vec<i64>,
    failed_my: Vec<i64>,
    rift_cells: Vec<i64>,
    unzip_my: Vec<i64>,
    sinuous: Vec<i64>,
    sinuous_first: Vec<i64>,
    collisions: [u64; 2],
    bow_first: Vec<i64>,
    in_belts: Vec<i64>,
    in_belts_first: Vec<i64>,
    belt_land: Vec<i64>,
    belt_wide: Vec<i64>,
    // The engine: each planet's first step with a mean speed of
    // `MOVING_MM`; at each quarter of the history, the mean speed and
    // the cells of trench and of ridge, added over the planets.
    moving: Vec<i64>,
    speed_at: [u64; 5],
    trench_at: [u64; 5],
    ridge_at: [u64; 5],
    trench_cells: u64,
    // Today's ocean floor: the share older than `OLD_FLOOR_MY`, ‰, and
    // the oldest, My, of each planet.
    old_floor: Vec<i64>,
    oldest_floor: Vec<i64>,
    // Collisions that lasted one step, and welds.
    brief: u64,
    welds: u64,
    // Today's land above 2,000 and 4,000 m, ‰ of the land, and the
    // highest point, m, of each planet.
    high: [Vec<i64>; 2],
    highest: Vec<i64>,
    // Cells of continental crust and those under water: at the start,
    // through the history, today.
    crust: [u64; 3],
    wet: [u64; 3],
    // Plates that could not be born for lack of a name.
    unnamed: u64,
    // Trenches born at a passive margin: how many, the step of each,
    // the floor that began to sink, km².
    born: u64,
    born_step: Vec<i64>,
    born_km2: u64,
    // Version 15. Counts added over the planets ([`STOPPED`]...), and
    // lists of a value a planet or an event ([`LASTED`]...).
    tally: [u64; 22],
    // Shelves built out, km2.
    built: u64,
    // What moves the sea, at the five quarters of the history, a value
    // a planet: how far the ocean floor has sunk below a ridge's crest
    // on average, m (0 to 4), and the ocean floor's share of the
    // sphere, per mille (5 to 9).
    causes: [Vec<i64>; 10],
    lists: [Vec<i64>; 28],
}

/// [`Count::tally`]: collisions their pile stopped, collisions still
/// going on today, troughs begun on a plate with little continent.
const STOPPED: usize = 0;
const GOING: usize = 1;
const ON_CRUMBS: usize = 2;
/// Collisions between two plates with a continent each (over
/// [`CRUMB_KM2`]): begun, of one step only, stopped by their pile.
const BIG: usize = 3;
const BIG_BRIEF: usize = 4;
const BIG_STOPPED: usize = 5;
/// [`Count::lists`]: how long each collision lasted, My; the same for
/// those of more than a step; each planet's thickest pile, tenths of a
/// km; its continents today and the largest, k km2; the most continent
/// one of its stopped collisions made one plate of, k km2; how long ago
/// its oldest standing range was last squeezed, My; the pull at each
/// trough's start, per mille of what the crust there holds; today's
/// thickest crust, tenths of a km.
const LASTED: usize = 0;
const LASTED_LONG: usize = 1;
const PILE: usize = 2;
const CONTINENTS: usize = 3;
const LARGEST: usize = 4;
const WELDED: usize = 5;
const OLD_RANGE: usize = 6;
const PULL: usize = 7;
const THICKEST: usize = 8;
/// How long each collision between two continents lasted, My.
const LASTED_BIG: usize = 9;
/// Version 16, [`Count::lists`], a value a planet. The land, per mille
/// of the sphere: at the start, the mean of the steps, today, the least
/// and the most of its history. The sea's level, m above the datum: at
/// the five quarters of the history ([`SEA_Q`] and the four after), its
/// lowest and highest. Today's sediment on the continents' crust: the
/// median and the most of the cells that carry any, m, and their share,
/// per mille; the most on the ocean floor, m. The shelf's width today,
/// km. The water the map holds today, m over the whole sphere.
const LAND_START: usize = 10;
const LAND_MEAN: usize = 11;
const LAND_TODAY: usize = 12;
const LAND_LEAST: usize = 13;
const LAND_MOST: usize = 14;
const SEA_Q: usize = 15;
const SEA_LOW: usize = 20;
const SEA_HIGH: usize = 21;
const SILT_MEDIAN: usize = 22;
const SILT_MOST: usize = 23;
const SILT_SHARE: usize = 24;
const SILT_FLOOR: usize = 25;
const SHELF_KM: usize = 26;
const MAP_WATER: usize = 27;
/// [`Count::tally`], version 16. The sediment, km3: worn, laid on dry
/// land, on crust under the sea, on the ocean floor, waiting at the
/// end, floor become shelf; standing today on the floor and on the
/// continents. Cells of crust below the sea's level: reached by the sea
/// (at the start, through the history, today) and in closed basins (the
/// same three).
const WORN: usize = 6;
const ON_LAND: usize = 7;
const ON_SHELF: usize = 8;
const ON_FLOOR: usize = 9;
const WAITING: usize = 10;
const TO_SHELF: usize = 11;
const FLOOR_NOW: usize = 12;
const CRUST_NOW: usize = 13;
const REACHED: usize = 14;
const CLOSED: usize = 17;
/// Inland water linked to the ocean and closed basins today, km2.
const LINKED_NOW: usize = 20;
const HOLLOW_NOW: usize = 21;
/// A plate with less continent than this, km2, carries a crumb.
const CRUMB_KM2: i64 = 1_000_000;
/// A range still stands where the crust is thicker than this, tenths of
/// a km (10 km of root left: 1,500 m of height from it).
const RANGE_THICK: u16 = 450;

impl Count {
    /// Another planet's count added to this one.
    fn add(&mut self, o: Self) {
        fn sum<const N: usize>(a: &mut [u64; N], b: [u64; N]) {
            for (x, y) in a.iter_mut().zip(b) {
                *x += y;
            }
        }
        let Self {
            steps,
            planets,
            followed,
            far_followed,
            flips,
            far_flips,
            rigid,
            rigid_moved,
            rigid_off,
            plates_today,
            plates_through,
            lives,
            plates_start,
            hollows,
            hollow_km2,
            hollow_painted_sea_km2,
            linked_km2,
            sunk,
            risen,
            reached,
            cut,
            moved,
            slid,
            slid_worst_km,
            switched,
            rifts,
            rift_my,
            failed_my,
            rift_cells,
            unzip_my,
            sinuous,
            sinuous_first,
            collisions,
            bow_first,
            in_belts,
            in_belts_first,
            belt_land,
            belt_wide,
            moving,
            speed_at,
            trench_at,
            ridge_at,
            trench_cells,
            old_floor,
            oldest_floor,
            brief,
            welds,
            high: [high2, high4],
            highest,
            crust,
            wet,
            unnamed,
            born,
            tally,
            lists,
            born_step,
            born_km2,
            built,
            causes,
        } = o;
        self.built += built;
        for (mine, theirs) in self.causes.iter_mut().zip(causes) {
            mine.extend(theirs);
        }
        self.steps += steps;
        self.planets += planets;
        self.followed += followed;
        self.far_followed += far_followed;
        sum(&mut self.flips, flips);
        sum(&mut self.far_flips, far_flips);
        for k in 0..3 {
            sum(&mut self.rigid[k], rigid[k]);
            sum(&mut self.sunk[k], sunk[k]);
            sum(&mut self.risen[k], risen[k]);
        }
        self.rigid_moved += rigid_moved;
        self.rigid_off = self.rigid_off.max(rigid_off);
        sum(&mut self.plates_today, plates_today);
        sum(&mut self.plates_through, plates_through);
        sum(&mut self.lives, lives);
        self.plates_start += plates_start;
        sum(&mut self.hollows, hollows);
        self.hollow_km2 += hollow_km2;
        self.hollow_painted_sea_km2 += hollow_painted_sea_km2;
        self.linked_km2 += linked_km2;
        self.reached += reached;
        self.cut += cut;
        self.moved += moved;
        self.slid += slid;
        self.slid_worst_km = self.slid_worst_km.max(slid_worst_km);
        sum(&mut self.switched, switched);
        sum(&mut self.rifts, rifts);
        self.rift_my.extend(rift_my);
        self.failed_my.extend(failed_my);
        self.rift_cells.extend(rift_cells);
        self.unzip_my.extend(unzip_my);
        self.sinuous.extend(sinuous);
        self.sinuous_first.extend(sinuous_first);
        sum(&mut self.collisions, collisions);
        self.bow_first.extend(bow_first);
        self.in_belts.extend(in_belts);
        self.in_belts_first.extend(in_belts_first);
        self.belt_land.extend(belt_land);
        self.belt_wide.extend(belt_wide);
        self.moving.extend(moving);
        sum(&mut self.speed_at, speed_at);
        sum(&mut self.trench_at, trench_at);
        sum(&mut self.ridge_at, ridge_at);
        self.trench_cells += trench_cells;
        self.old_floor.extend(old_floor);
        self.oldest_floor.extend(oldest_floor);
        self.brief += brief;
        self.welds += welds;
        self.high[0].extend(high2);
        self.high[1].extend(high4);
        self.highest.extend(highest);
        sum(&mut self.crust, crust);
        sum(&mut self.wet, wet);
        self.unnamed += unnamed;
        sum(&mut self.tally, tally);
        for (mine, theirs) in self.lists.iter_mut().zip(lists) {
            mine.extend(theirs);
        }
        self.born += born;
        self.born_step.extend(born_step);
        self.born_km2 += born_km2;
    }
}

/// One planet, counted alone: the planets are made on several threads and
/// their counts added in the order of the seeds.
fn planet(seed: u64, settings: &Settings) -> (Count, [(i64, i64); 2], f64) {
    let mut n = Count::default();
    let mut volume = (1000_i64, 1000_i64);
    let mut area = (1000_i64, 1000_i64);
    let clock = std::time::Instant::now();
    let world = Planet::with(seed, settings);
    let seconds = clock.elapsed().as_secs_f64();
    let h = &world.history;
    let sphere = &world.sphere;
    let links = &sphere.links;
    let last = h.last();
    n.planets += 1;
    n.plates_start += h.plates_at_start.1 as u64;
    let mut prev: Option<HashMap<u32, Seen>> = None;
    let mut volume0 = 0_i64;
    let mut sea_before = world.at_step(0).sea_rise_m;
    let mut moving: Option<i64> = None;
    let mut lands: Vec<i64> = Vec::new();
    let mut first_t = [usize::MAX; MAXP];
    let mut last_t = [0_usize; MAXP];
    for t in 0..=last {
        let now = world.at_step(t);
        let pieces = now.pieces();
        // The crust kept.
        let vol: i64 = pieces
            .iter()
            .flat_map(|p| &p.tiles)
            .map(|x| (i64::from(x.thick) * 100 + i64::from(x.silt)) * i64::from(x.area_km2))
            .sum();
        if t == 0 {
            volume0 = vol;
        }
        let share = vol * 1000 / volume0;
        volume = (volume.0.min(share), volume.1.max(share));
        let share = h.continental_km2[t] * 1000 / h.continental_km2[0];
        area = (area.0.min(share), area.1.max(share));
        // The plates.
        let mut km2 = [0_i64; MAXP];
        for c in 0..CELLS {
            km2[h.plate[t][c] as usize] += i64::from(sphere.area_km2[c]);
        }
        for p in 0..MAXP {
            if km2[p] == 0 {
                continue;
            }
            first_t[p] = first_t[p].min(t);
            last_t[p] = t;
            let class = PLATE_KM2.iter().filter(|&&k| km2[p] >= k).count();
            n.plates_through[class] += 1;
            if t == last {
                n.plates_today[class] += 1;
            }
        }
        // The engine: how fast the plates go, their trenches and ridges.
        let speed = (0..MAXP)
            .map(|p| km2[p] / 1000 * h.motion[t][p].speed_mm)
            .sum::<i64>()
            / (km2.iter().sum::<i64>() / 1000);
        if speed >= MOVING_MM && moving.is_none() {
            moving = Some(t as i64);
        }
        let trench = h.kind[t].iter().filter(|&&k| k == KIND_TRENCH).count() as u64;
        let ridge = h.kind[t].iter().filter(|&&k| k == KIND_RIDGE).count() as u64;
        n.trench_cells += trench;
        for k in 0..5 {
            if t == last * k / 4 {
                n.speed_at[k] += speed as u64;
                n.trench_at[k] += trench;
                n.ridge_at[k] += ridge;
            }
        }
        // The crust under water.
        let crust = (0..CELLS).filter(|&c| h.class[t][c] != OCEAN).count() as u64;
        let wet = (0..CELLS)
            .filter(|&c| h.class[t][c] != OCEAN && !now.dry(c))
            .count() as u64;
        let below = |basin: bool| {
            (0..CELLS)
                .filter(|&c| {
                    h.class[t][c] != OCEAN && now.elevation[c] <= 0 && now.basin[c] == basin
                })
                .count() as u64
        };
        for (k, counted) in [t == 0, true, t == last].into_iter().enumerate() {
            if counted {
                n.crust[k] += crust;
                n.wet[k] += wet;
                n.tally[REACHED + k] += below(false);
                n.tally[CLOSED + k] += below(true);
            }
        }
        // The land and the sea (version 16): the share of land is a
        // result, the sea's level follows the basins.
        let whole: i64 = sphere.area_km2.iter().map(|&a| i64::from(a)).sum();
        let land = now.lands.iter().map(|l| l.area_km2).sum::<i64>() * 1000 / whole;
        lands.push(land);
        let b = h.budget[t];
        for (k, v) in [b.worn, b.on_land, b.on_shelf, b.on_floor, 0, b.to_shelf]
            .into_iter()
            .enumerate()
        {
            n.tally[WORN + k] += (v / 1000) as u64;
        }
        for k in 0..5 {
            if t == last * k / 4 {
                n.lists[SEA_Q + k].push(now.sea);
                let (mut sunk, mut km2) = (0_i64, 0_i64);
                for c in (0..CELLS).filter(|&c| h.class[t][c] == OCEAN) {
                    let a = i64::from(sphere.area_km2[c]);
                    sunk += history::sinking(i64::from(h.age[t][c])) * a;
                    km2 += a;
                }
                n.causes[k].push(sunk / km2.max(1));
                n.causes[5 + k].push(km2 * 1000 / whole);
            }
        }
        if t == last {
            n.tally[WAITING] += (b.waiting / 1000) as u64;
            let mut silt: Vec<i64> = (0..CELLS)
                .filter(|&c| h.class[t][c] != OCEAN && h.silt_at[t][c] > 0)
                .map(|c| i64::from(h.silt_at[t][c]))
                .collect();
            silt.sort_unstable();
            n.lists[SILT_MEDIAN].push(silt.get(silt.len() / 2).copied().unwrap_or(0));
            n.lists[SILT_MOST].push(silt.last().copied().unwrap_or(0));
            n.lists[SILT_SHARE].push((silt.len() as u64 * 1000 / crust.max(1)) as i64);
            let mut on_floor = 0_i64;
            let mut most = 0_i64;
            for c in (0..CELLS).filter(|&c| h.class[t][c] == OCEAN) {
                let m = i64::from(h.silt_at[t][c]);
                on_floor += m * i64::from(sphere.area_km2[c]);
                most = most.max(m);
            }
            n.lists[SILT_FLOOR].push(most);
            n.tally[FLOOR_NOW] += (on_floor / 1000) as u64;
            n.tally[CRUST_NOW] += pieces
                .iter()
                .flat_map(|p| &p.tiles)
                .map(|x| u64::from(x.silt) * u64::from(x.area_km2) / 1000)
                .sum::<u64>();
            let held: i64 = (0..CELLS)
                .filter(|&c| now.elevation[c] < 0 && !now.basin[c])
                .map(|c| -i64::from(now.elevation[c]) * i64::from(sphere.area_km2[c]))
                .sum();
            n.lists[MAP_WATER].push(held / whole);
        }
        if t == last {
            // Today's ocean floor and today's relief.
            let floor: Vec<i64> = (0..CELLS)
                .filter(|&c| h.class[t][c] == OCEAN)
                .map(|c| i64::from(h.age[t][c]))
                .collect();
            let old = floor.iter().filter(|&&a| a > OLD_FLOOR_MY).count();
            n.old_floor.push((old * 1000 / floor.len().max(1)) as i64);
            n.oldest_floor
                .push(floor.iter().copied().max().unwrap_or(0));
            let land = now.elevation.iter().filter(|&&e| e > 0).count().max(1);
            for (k, m) in [2_000, 4_000].into_iter().enumerate() {
                let above = now.elevation.iter().filter(|&&e| e > m).count();
                n.high[k].push((above * 1000 / land) as i64);
            }
            n.highest
                .push(i64::from(now.elevation.iter().copied().max().unwrap_or(0)));
        }
        // The water on the continents: ground below the sea's level,
        // linked to the ocean (a path of such ground to the ocean's
        // floor) or not (a hollow).
        let low_cell: Vec<bool> = now.elevation.iter().map(|&e| e <= 0).collect();
        let floor: Vec<(usize, u32)> = (0..CELLS)
            .filter(|&c| h.class[t][c] == OCEAN)
            .map(|c| (c, 0))
            .collect();
        let (from_ocean, _) = sphere.spread(&floor, |_, _| true, 2_000);
        let mut water = vec![0_u8; CELLS];
        for c in 0..CELLS {
            if h.class[t][c] != OCEAN && low_cell[c] {
                water[c] = if now.basin[c] {
                    1
                } else if from_ocean[c] >= INLAND_KM {
                    2
                } else {
                    3
                };
            }
        }
        let mut size = vec![0_u8; CELLS];
        let mut done = vec![false; CELLS];
        for c0 in 0..CELLS {
            if water[c0] != 1 || done[c0] {
                continue;
            }
            let (mut stack, mut cells) = (vec![c0], Vec::new());
            done[c0] = true;
            while let Some(c) = stack.pop() {
                cells.push(c);
                for &(m, _) in &links[c] {
                    let m = m as usize;
                    if water[m] == 1 && !done[m] {
                        done[m] = true;
                        stack.push(m);
                    }
                }
            }
            let km2: i64 = cells.iter().map(|&c| i64::from(sphere.area_km2[c])).sum();
            let class = SIZES.iter().position(|s| km2 < s.1).expect("a size");
            n.hollows[class] += 1;
            n.hollow_km2 += km2 as u64;
            for c in cells {
                size[c] = class as u8;
            }
        }
        for c in 0..CELLS {
            if water[c] == 2 {
                n.linked_km2 += u64::from(sphere.area_km2[c]);
            }
            if t == last && (water[c] == 1 || water[c] == 2) {
                n.tally[if water[c] == 2 {
                    LINKED_NOW
                } else {
                    HOLLOW_NOW
                }] += u64::from(sphere.area_km2[c]);
            }
        }
        if t == last {
            // The shelf's width: from each dry cell of crust on the
            // sea's shore to the ocean's floor.
            let mut wide: Vec<i64> = (0..CELLS)
                .filter(|&c| {
                    h.class[t][c] != OCEAN
                        && now.dry(c)
                        && links[c].iter().any(|&(m, _)| !now.dry(m as usize))
                })
                .map(|c| i64::from(from_ocean[c].min(2_000)))
                .collect();
            wide.sort_unstable();
            n.lists[SHELF_KM].push(wide.get(wide.len() / 2).copied().unwrap_or(0));
        }
        // Every cell of crust, followed by its name in its piece.
        let d_sea = now.sea_rise_m - sea_before;
        sea_before = now.sea_rise_m;
        let mut drawn: HashMap<u32, usize> = HashMap::new();
        for c in 0..CELLS {
            if h.class[t][c] != OCEAN {
                drawn.insert(h.id_at[t][c], c);
            }
        }
        let mut seen: HashMap<u32, Seen> = HashMap::new();
        if prev.is_some() {
            n.steps += 1;
        }
        for (k, piece) in pieces.iter().enumerate() {
            for (i, x) in piece.tiles.iter().enumerate() {
                let pos = piece.place(sphere, i);
                let cell = sphere.cell_of(pos);
                let shown = drawn.get(&x.id).copied();
                let me = Seen {
                    dry: now.tile_dry(k, i),
                    low: !x.under && now.tile_height(x) <= 0,
                    far: now.near[cell].0 > FAR_KM,
                    under: x.under,
                    high: i64::from(x.high),
                    pile: (i64::from(x.thick) - 350).max(0) * history::ISOSTASY_M / 10,
                    lip: i64::from(x.margins & history::LIP != 0) * 350,
                    pos,
                    own: x.own,
                    water: shown.map_or(0, |c| water[c]),
                    size: shown.map_or(0, |c| size[c]),
                };
                seen.insert(x.id, me);
                let Some(was) = &prev else {
                    continue;
                };
                // Does it move with its plate?
                if let Some(b) = was.get(&x.id) {
                    n.moved += 1;
                    let with = |p: u8| {
                        let w = h.turned_at[t][p as usize];
                        km_between(history::carried(w, settings.step_my, b.pos), pos)
                    };
                    let off = with(b.own);
                    if x.switch != 0 || x.own != b.own {
                        n.switched[x.switch as usize] += 1;
                    } else if off > SLIDE_KM {
                        n.slid += 1;
                        n.slid_worst_km = n.slid_worst_km.max(off);
                    }
                }
                if x.under {
                    continue;
                }
                let (cause, b) = match was.get(&x.id) {
                    None => (14, None),
                    Some(b) if b.under => (13, None),
                    Some(b) => {
                        let known: i64 = x.why.iter().map(|&w| i64::from(w)).sum();
                        let dp = me.pile - b.pile;
                        let mut parts = vec![
                            (-d_sea, 0),
                            (dp, if dp > 0 { 9 } else { 10 }),
                            (me.lip - b.lip, 11),
                            (me.high - b.high - known, 12),
                        ];
                        parts.extend((0..8).map(|k| (i64::from(x.why[k]), k + 1)));
                        let sign = if me.low { -1 } else { 1 };
                        let &(_, cause) = parts
                            .iter()
                            .max_by_key(|p| (p.0 * sign, std::cmp::Reverse(p.1)))
                            .expect("parts");
                        (cause, Some(*b))
                    }
                };
                let (was_dry, was_low, far) =
                    b.map_or((false, true, me.far), |b| (b.dry, b.low, me.far && b.far));
                n.followed += 1;
                n.far_followed += u64::from(far);
                if me.dry != was_dry {
                    // Painted otherwise with no change of level: the
                    // hollow's link to the ocean.
                    let cause = if me.low == was_low && b.is_some() {
                        15
                    } else {
                        cause
                    };
                    n.flips[cause] += 1;
                    n.far_flips[cause] += u64::from(far);
                }
                let Some(b) = b else {
                    continue;
                };
                let km2 = u64::from(x.area_km2);
                let place = |w: u8, s: u8| match w {
                    1 => Some(usize::from(s >= 2)),
                    2 => Some(2),
                    _ => None,
                };
                if me.low && !b.low {
                    if let Some(k) = place(me.water, me.size) {
                        n.sunk[k][cause] += km2;
                    }
                } else if !me.low && b.low {
                    if let Some(k) = place(b.water, b.size) {
                        n.risen[k][cause] += km2;
                    }
                } else if me.low {
                    n.reached += km2 * u64::from(b.water == 1 && me.water >= 2);
                    n.cut += km2 * u64::from(b.water >= 2 && me.water == 1);
                }
            }
        }
        for c in 0..CELLS {
            if water[c] == 1 && seen.get(&h.id_at[t][c]).is_some_and(|x| !x.dry) {
                n.hollow_painted_sea_km2 += u64::from(sphere.area_km2[c]);
            }
        }
        prev = Some(seen);
    }
    for p in 0..MAXP {
        if first_t[p] != usize::MAX && last_t[p] < last {
            let life = last_t[p] - first_t[p] + 1;
            n.lives[[1, 3, 9].iter().filter(|&&k| life > k).count()] += 1;
        }
    }
    n.built += h.built_km2.iter().sum::<i64>() as u64;
    n.lists[LAND_START].push(lands[0]);
    n.lists[LAND_MEAN].push(lands.iter().sum::<i64>() / lands.len() as i64);
    n.lists[LAND_TODAY].push(lands[last]);
    n.lists[LAND_LEAST].push(lands.iter().copied().min().unwrap_or(0));
    n.lists[LAND_MOST].push(lands.iter().copied().max().unwrap_or(0));
    n.lists[SEA_LOW].push(h.sea_at.iter().map(|&v| i64::from(v)).min().unwrap_or(0));
    n.lists[SEA_HIGH].push(h.sea_at.iter().map(|&v| i64::from(v)).max().unwrap_or(0));
    // The rigid pieces.
    for (k, span) in [1_usize, 5, 20].into_iter().enumerate() {
        for t in 0..=last.saturating_sub(span) {
            for a in h.pieces_at[t].iter().filter(|a| a.tiles.len() >= 30) {
                let Some(b) = h.pieces_at[t + span].iter().find(|b| b.frame == a.frame) else {
                    continue;
                };
                n.rigid[k][0] += 1;
                let then: HashMap<u32, usize> =
                    a.tiles.iter().enumerate().map(|(i, x)| (x.id, i)).collect();
                let mut kept = 0;
                for (j, x) in b.tiles.iter().enumerate() {
                    let Some(&i) = then.get(&x.id) else {
                        continue;
                    };
                    kept += 1;
                    n.rigid_moved += u64::from(a.tiles[i].at != x.at);
                    let off = km_between(back(a, a.place(sphere, i)), back(b, b.place(sphere, j)));
                    n.rigid_off = n.rigid_off.max(off.unsigned_abs());
                }
                n.rigid[k][1] += u64::from(kept == a.tiles.len() && b.tiles.len() == kept);
            }
        }
    }
    // A planet that never moves counts as starting after its last step.
    n.moving.push(moving.unwrap_or(last as i64 + 1));
    n.unnamed += u64::from(h.unnamed);
    for (t, events) in h.journal.iter().enumerate() {
        for e in events.iter().filter(|e| e.what == What::BornOfTrench) {
            n.born += 1;
            n.born_step.push(t as i64);
            n.born_km2 += e.km2 as u64;
        }
    }
    rifts(&world, &mut n);
    (n, [volume, area], seconds)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let settings: Settings = arg("--settings").map_or_else(Settings::default, |path| {
        let text = std::fs::read_to_string(&path).expect("a settings file");
        ron::from_str(&text).expect("settings")
    });
    let seeds_arg = arg("--seeds").unwrap_or_else(|| "1-30".to_owned());
    let seeds: Vec<u64> = seeds_arg
        .split(',')
        .flat_map(|part| {
            let ends: Vec<u64> = part
                .split('-')
                .map(|s| s.parse().expect("a seed"))
                .collect();
            ends[0]..=ends[ends.len() - 1]
        })
        .collect();
    let version = arg("--version").unwrap_or_else(|| "work".to_owned());
    let mut n = Count::default();
    let mut volume = (1000_i64, 1000_i64);
    let mut area = (1000_i64, 1000_i64);
    let mut seconds = Vec::new();
    for (one, [v, a], s) in par::par_map(&seeds, par::threads(), |&seed| planet(seed, &settings)) {
        n.add(one);
        volume = (volume.0.min(v.0), volume.1.max(v.1));
        area = (area.0.min(a.0), area.1.max(a.1));
        seconds.push(s);
    }
    let round = |x: f64| (x * 1000.0).round() / 1000.0;
    let pc = |a: u64, b: u64| round(100.0 * a as f64 / b.max(1) as f64);
    let per = |a: u64, b: u64| round(a as f64 / b.max(1) as f64);
    let named = |names: &[&str], values: Vec<Value>| -> Value {
        Value::Object(
            names
                .iter()
                .zip(values)
                .map(|(k, v)| ((*k).to_owned(), v))
                .collect::<Map<String, Value>>(),
        )
    };
    let all: u64 = n.flips.iter().sum();
    let far: u64 = n.far_flips.iter().sum();
    let by_cause = |v: &[u64; 16]| {
        let total: u64 = v.iter().sum();
        let mut out = Map::new();
        out.insert(
            "k km2 per step and planet".to_owned(),
            json!(per(total / 1000, n.steps)),
        );
        for k in 0..16 {
            if v[k] > 0 {
                out.insert(format!("% {}", CAUSES[k]), json!(pc(v[k], total)));
            }
        }
        Value::Object(out)
    };
    seconds.sort_by(f64::total_cmp);
    let spread = |v: &mut Vec<i64>| {
        v.sort_unstable();
        [
            v.first().copied().unwrap_or(0),
            v.get(v.len() / 2).copied().unwrap_or(0),
            v.last().copied().unwrap_or(0),
        ]
    };
    let switched: u64 = n.switched.iter().sum();
    let late = n.moving.iter().filter(|&&t| t >= LATE_STEP).count();
    let quarters = |v: &[u64; 5]| v.map(|x| per(x, n.planets));
    let table = json!({
        "version": version,
        "seeds": seeds_arg,
        "planets": n.planets,
        "steps per planet": settings.steps,
        "1 land/sea flips, % of crust cells per step": {
            "all": pc(all, n.followed),
            "far from the plates' edges (over 1000 km)": pc(far, n.far_followed),
            "share by cause, % of flips": named(&CAUSES, n.flips.iter().map(|&v| json!(pc(v, all))).collect()),
            "share by cause far from edges, % of flips there": named(&CAUSES, n.far_flips.iter().map(|&v| json!(pc(v, far))).collect()),
        },
        "2 rigid pieces (30 cells or more)": {
            "followed, and with no cell gained or lost, over 1 step": n.rigid[0],
            "over 5 steps": n.rigid[1],
            "over 20 steps": n.rigid[2],
            "kept cells that changed place in their piece (must be 0)": n.rigid_moved,
            "worst offset of a kept cell, km (must be 0)": n.rigid_off,
        },
        "3 plates": {
            "at the start, per planet": per(n.plates_start, n.planets),
            "today, per planet": named(&PLATE_NAMES, n.plates_today.iter().map(|&v| json!(per(v, n.planets))).collect()),
            "through the history, per step": named(&PLATE_NAMES, n.plates_through.iter().map(|&v| json!(per(v, n.steps + n.planets))).collect()),
            "ended, per planet, by life": named(
                &["1 step", "2-3 steps", "4-9 steps", "10 steps or more"],
                n.lives.iter().map(|&v| json!(per(v, n.planets))).collect(),
            ),
        },
        "4 continental crust kept, per mille of the start": {
            "area not under another piece, lowest and highest": [area.0, area.1],
            "volume, lowest and highest": [volume.0, volume.1],
        },
        "5 ground below the sea's level inside the continents": {
            "hollows with no path to the ocean, per step and planet": named(
                &SIZES.map(|s| s.0),
                n.hollows.iter().map(|&v| json!(per(v, n.steps + n.planets))).collect(),
            ),
            "hollows, k km2 per step": per(n.hollow_km2 / 1000, n.steps + n.planets),
            "of which the model paints as sea, %": pc(n.hollow_painted_sea_km2, n.hollow_km2),
            "inland water linked to the ocean (over 300 km from its floor), k km2 per step": per(n.linked_km2 / 1000, n.steps + n.planets),
            "ground gone below the sea's level, in a hollow under 200 k km2": by_cause(&n.sunk[0]),
            "ground gone below the sea's level, in a larger hollow": by_cause(&n.sunk[1]),
            "ground gone below the sea's level, in linked inland water": by_cause(&n.sunk[2]),
            "ground back above the sea's level, in a hollow under 200 k km2": by_cause(&n.risen[0]),
            "ground back above the sea's level, in a larger hollow": by_cause(&n.risen[1]),
            "ground back above the sea's level, in linked inland water": by_cause(&n.risen[2]),
            "hollow reached by the sea, k km2 per step and planet": per(n.reached / 1000, n.steps),
            "inland water cut from the sea, k km2 per step and planet": per(n.cut / 1000, n.steps),
        },
        "6 does a continent move with its plate": {
            "cells of crust that slid on their plate (over 2 km off its turn), % per step": pc(n.slid, n.moved),
            "worst slide, km": n.slid_worst_km,
            "cells of crust that changed plate, % per step": pc(switched, n.moved),
            "share by cause, % of changes": named(&SWITCHES, n.switched.iter().map(|&v| json!(pc(v, switched))).collect()),
        },
        "9 collisions of continents, per planet": {
            "begun": per(n.collisions[0], n.planets),
            "of which a front of 40 cells or more (about 3,700 km)": per(n.collisions[1], n.planets),
            "of which lasted one step only": per(n.brief, n.planets),
            "that welded their two plates": per(n.welds, n.planets),
            "ended because their pile stopped them (the two plates are one)": per(n.tally[STOPPED], n.planets),
            "still going on today": per(n.tally[GOING], n.planets),
            "between two plates with over 1 M km2 of continent each: begun, one step only, stopped by their pile": [
                per(n.tally[BIG], n.planets),
                per(n.tally[BIG_BRIEF], n.planets),
                per(n.tally[BIG_STOPPED], n.planets),
            ],
            "how long those last, My: least, median, most": spread(&mut n.lists[LASTED_BIG]),
            "how long a collision lasts, My: least, median, most": spread(&mut n.lists[LASTED]),
            "the same, of those of more than a step": spread(&mut n.lists[LASTED_LONG]),
            "the thickest crust a pile reached, km, each planet's: least, median, most": spread(&mut n.lists[PILE]).map(|v| v / 10),
            "the most continent a stopped collision made one plate of, M km2, each planet's: least, median, most": spread(&mut n.lists[WELDED]).map(|v| v / 1000),
            "continents today (bodies of continental crust over 1 M km2): least, median, most": spread(&mut n.lists[CONTINENTS]),
            "the largest continent today, M km2: least, median, most": spread(&mut n.lists[LARGEST]).map(|v| v / 1000),
        },
        "10 the engine": {
            "step at which the plates' mean speed (weighted by area) first reaches 10 mm a year: least, median, most": spread(&mut n.moving),
            "planets where that is step 16 or later": late,
            "mean speed, mm a year, at steps 0, 1/4, 1/2, 3/4 of the history and today: mean of the planets": quarters(&n.speed_at),
            "cells of trench (the side that sinks) at the same steps, per planet": quarters(&n.trench_at),
            "cells of ridge at the same steps, per planet": quarters(&n.ridge_at),
            "cells of trench per step, through the history": per(n.trench_cells, n.steps + n.planets),
            "today's ocean floor older than 200 My, per mille of the floor: least, median, most": spread(&mut n.old_floor),
            "today's oldest ocean floor, My: least, median, most": spread(&mut n.oldest_floor),
            "trenches born at a passive margin (its old floor gave way), per planet": per(n.born, n.planets),
            "the step a trench is born at: least, median, most": spread(&mut n.born_step),
            "floor that began to sink at those births, M km2 per planet": per(n.born_km2 / 1_000_000, n.planets),
            "plates not born for lack of a name (must be 0)": n.unnamed,
        },
        "11 relief and sea": {
            "today's land above 2,000 m, per mille of the land: least, median, most": spread(&mut n.high[0]),
            "today's land above 4,000 m, per mille of the land: least, median, most": spread(&mut n.high[1]),
            "today's highest point, m: least, median, most": spread(&mut n.highest),
            "today's thickest crust, km: least, median, most": spread(&mut n.lists[THICKEST]).map(|v| v / 10),
            "the oldest range still standing (crust thicker than 45 km today): My since it was last squeezed, 0 none: least, median, most": spread(&mut n.lists[OLD_RANGE]),
            "continental crust under water, % of its cells: at the start, through the history, today": [
                pc(n.wet[0], n.crust[0]),
                pc(n.wet[1], n.crust[1]),
                pc(n.wet[2], n.crust[2]),
            ],
        },
        "12 the sea and the sediment": {
            "the planet's water, m over the whole sphere (a setting)": settings.water_m,
            "the water the map holds today, m over the whole sphere (the trenches' deeps and the hotspots' chains are drawn, not counted): least, median, most": spread(&mut n.lists[MAP_WATER]),
            "land, per mille of the sphere, at the start: least, median, most": spread(&mut n.lists[LAND_START]),
            "land, the mean of a planet's steps: least, median, most": spread(&mut n.lists[LAND_MEAN]),
            "land today: least, median, most": spread(&mut n.lists[LAND_TODAY]),
            "the least land a planet had in its history: least, median, most": spread(&mut n.lists[LAND_LEAST]),
            "the most land a planet had in its history: least, median, most": spread(&mut n.lists[LAND_MOST]),
            "the sea's level, m above the datum, at steps 0, 1/4, 1/2, 3/4 of the history and today: each least, median, most": [
                spread(&mut n.lists[SEA_Q]),
                spread(&mut n.lists[SEA_Q + 1]),
                spread(&mut n.lists[SEA_Q + 2]),
                spread(&mut n.lists[SEA_Q + 3]),
                spread(&mut n.lists[SEA_Q + 4]),
            ],
            "what moves it. How far the ocean floor has sunk below a ridge's crest on average, m, at the same steps: each least, median, most": [
                spread(&mut n.causes[0]),
                spread(&mut n.causes[1]),
                spread(&mut n.causes[2]),
                spread(&mut n.causes[3]),
                spread(&mut n.causes[4]),
            ],
            "the ocean floor's share of the sphere, per mille, at the same steps: each least, median, most": [
                spread(&mut n.causes[5]),
                spread(&mut n.causes[6]),
                spread(&mut n.causes[7]),
                spread(&mut n.causes[8]),
                spread(&mut n.causes[9]),
            ],
            "a planet's lowest sea: least, median, most": spread(&mut n.lists[SEA_LOW]),
            "a planet's highest sea: least, median, most": spread(&mut n.lists[SEA_HIGH]),
            "crust below the sea's level and reached by the sea, % of the crust's cells: at the start, through the history, today": [
                pc(n.tally[REACHED], n.crust[0]),
                pc(n.tally[REACHED + 1], n.crust[1]),
                pc(n.tally[REACHED + 2], n.crust[2]),
            ],
            "crust below the sea's level in a closed basin (not sea), % of the crust's cells: the same three": [
                pc(n.tally[CLOSED], n.crust[0]),
                pc(n.tally[CLOSED + 1], n.crust[1]),
                pc(n.tally[CLOSED + 2], n.crust[2]),
            ],
            "today: inland water the sea reaches (over 300 km from the ocean's floor), k km2 per planet": per(n.tally[LINKED_NOW] / 1000, n.planets),
            "today: closed basins, k km2 per planet": per(n.tally[HOLLOW_NOW] / 1000, n.planets),
            "the shelf's width today, km (from a dry shore to the ocean's floor, each planet's median): least, median, most": spread(&mut n.lists[SHELF_KM]),
            "worn off the continents, k km3 per step and planet": per(n.tally[WORN] / 1000, n.steps),
            "laid on dry land (hollows, plains), % of the worn": pc(n.tally[ON_LAND], n.tally[WORN]),
            "laid on crust under the sea (shelves, inland seas), % of the worn": pc(n.tally[ON_SHELF], n.tally[WORN]),
            "laid on the ocean floor, % of the worn": pc(n.tally[ON_FLOOR], n.tally[WORN]),
            "laid or waiting to be laid, per mille of the worn (must be 1000)": per(
                1000 * (n.tally[ON_LAND] + n.tally[ON_SHELF] + n.tally[ON_FLOOR] + n.tally[WAITING]),
                n.tally[WORN],
            ),
            "of the sediment laid on the ocean floor: % still on it today, % become shelf (floor buried to the sea's level), the rest gone with the floor (sunk at a trench, or covered)": [
                pc(n.tally[FLOOR_NOW], n.tally[ON_FLOOR]),
                pc(n.tally[TO_SHELF], n.tally[ON_FLOOR]),
            ],
            "sediment on the continents today, k km3 per planet": per(n.tally[CRUST_NOW] / 1000, n.planets),
            "cells of crust that carry sediment today, per mille: least, median, most": spread(&mut n.lists[SILT_SHARE]),
            "its thickness there, m, each planet's median: least, median, most": spread(&mut n.lists[SILT_MEDIAN]),
            "the thickest sediment on a continent today, m: least, median, most": spread(&mut n.lists[SILT_MOST]),
            "the thickest sediment on the ocean floor today, m: least, median, most": spread(&mut n.lists[SILT_FLOOR]),
            "shelves built out, k km2 per planet": per(n.built / 1000, n.planets),
        },
        "7 rifts, per planet": {
            "started (a trough)": per(n.rifts[0], n.planets),
            "cut through (the crust gave way, the plate is two)": per(n.rifts[1], n.planets),
            "begun on a plate with under 1 M km2 of continent, % of the troughs": pc(n.tally[ON_CRUMBS], n.rifts[0]),
            "the pull at a trough's start, per mille of what its crust holds: least, median, most": spread(&mut n.lists[PULL]),
            "of which an open ocean (shores touch nowhere)": per(n.rifts[2], n.planets),
            "failed (abandoned trough)": per(n.rifts[3], n.planets),
            "still a trough at the end": per(n.rifts[4], n.planets),
            "from first trough to the cut, My: least, median, most": spread(&mut n.rift_my),
            "from first trough to abandon, My: least, median, most": spread(&mut n.failed_my),
            "trough's greatest length, cells of about 92 km: least, median, most": spread(&mut n.rift_cells),
            "from the cut to shores that touch nowhere, My: least, median, most": spread(&mut n.unzip_my),
            "sinuosity of the cut, per mille (length along the trough over the straight distance between its ends; troughs over 1000 km): least, median, most": spread(&mut n.sinuous),
            "sinuosity of the planet's first cut (the supercontinent's), per mille: least, median, most": spread(&mut n.sinuous_first),
            "how far the first cut strays from the straight line through its ends, at most, per mille of that line's length: least, median, most": spread(&mut n.bow_first),
            "share of the first cut's path that lies in a belt of the mosaic, per mille: least, median, most": spread(&mut n.in_belts_first),
            "share of every cut's path that lies in a belt, per mille: least, median, most": spread(&mut n.in_belts),
            "share of the supercontinent's crust that lies in a belt at the start, per mille (what a path blind to the crust would cross): least, median, most": spread(&mut n.belt_land),
            "belts' width at the start, km (each planet's median along the axes): least, median, most": spread(&mut n.belt_wide),
        },
        "8 seconds per planet": {
            "median": round(seconds[seconds.len() / 2]),
            "most": round(seconds[seconds.len() - 1]),
        },
    });
    let text = serde_json::to_string_pretty(&table).expect("a table");
    println!("{text}");
    if let Some(path) = arg("--out") {
        std::fs::write(&path, text + "\n").expect("the table written");
    }
}

/// The rifts of a planet: started, cut through (the crust gave way and
/// the plate was cut in two), open (an ocean: the two shores touch
/// nowhere), failed (abandoned troughs), still troughs at the end; and
/// how long from the first trough to the cut.
fn rifts(world: &Planet, n: &mut Count) {
    let never = history::NEVER;
    for c in &world.history.collisions {
        n.collisions[0] += 1;
        n.collisions[1] += u64::from(c.front_cells >= 40);
        n.brief += u64::from(c.steps <= 1);
        n.tally[STOPPED] += u64::from(c.stopped);
        n.tally[GOING] += u64::from(c.ended_my == never);
        let ended = if c.ended_my == never { 0 } else { c.ended_my };
        let lasted = i64::from(c.from_my) - i64::from(ended);
        n.lists[LASTED].push(lasted);
        if c.steps > 1 {
            n.lists[LASTED_LONG].push(lasted);
        }
        if c.km2.0.min(c.km2.1) >= CRUMB_KM2 {
            n.tally[BIG] += 1;
            n.tally[BIG_BRIEF] += u64::from(c.steps <= 1);
            n.tally[BIG_STOPPED] += u64::from(c.stopped);
            n.lists[LASTED_BIG].push(lasted);
        }
    }
    let h = &world.history;
    n.lists[PILE].push(
        h.collisions
            .iter()
            .map(|c| i64::from(c.thick))
            .max()
            .unwrap_or(0),
    );
    n.lists[WELDED].push(
        h.welds
            .iter()
            .map(|w| (w.kept_km2 + w.absorbed_km2) / 1000)
            .max()
            .unwrap_or(0),
    );
    n.welds += h.welds.len() as u64;
    // Today's continents: the bodies of continental crust, whatever
    // plates carry them; today's thickest crust and oldest range.
    let last = h.last();
    let sphere = &world.sphere;
    let mut seen = vec![false; CELLS];
    let mut bodies: Vec<i64> = Vec::new();
    for c in 0..CELLS {
        if seen[c] || h.class[last][c] == history::OCEAN {
            continue;
        }
        let (mut km2, mut todo) = (0_i64, vec![c]);
        seen[c] = true;
        while let Some(q) = todo.pop() {
            km2 += i64::from(sphere.area_km2[q]);
            for &(m, _) in &sphere.links[q] {
                let m = m as usize;
                if !seen[m] && h.class[last][m] != history::OCEAN {
                    seen[m] = true;
                    todo.push(m);
                }
            }
        }
        bodies.push(km2 / 1000);
    }
    n.lists[CONTINENTS].push(bodies.iter().filter(|&&b| b >= CRUMB_KM2 / 1000).count() as i64);
    n.lists[LARGEST].push(bodies.iter().copied().max().unwrap_or(0));
    let crust = || (0..CELLS).filter(|&c| h.class[last][c] != history::OCEAN);
    n.lists[THICKEST].push(
        crust()
            .map(|c| i64::from(h.thick_at[last][c]))
            .max()
            .unwrap_or(0),
    );
    n.lists[OLD_RANGE].push(
        crust()
            .filter(|&c| {
                h.thick_at[last][c] > RANGE_THICK
                    && usize::from(h.orogeny_my[c]) <= last * world.settings.step_my as usize
            })
            .map(|c| i64::from(h.orogeny_my[c]))
            .max()
            .unwrap_or(0),
    );
    for r in &world.history.rifts {
        n.rifts[0] += 1;
        n.tally[ON_CRUMBS] += u64::from(r.area_km2 < CRUMB_KM2);
        n.lists[PULL].push(i64::from(r.pull));
        n.rifts[1] += u64::from(r.cut_my != never);
        n.rifts[2] += u64::from(r.parted_my != never);
        n.rifts[3] += u64::from(r.aborted);
        n.rifts[4] += u64::from(!r.aborted && r.cut_my == never);
        if r.cut_my != never {
            n.rift_my.push(i64::from(r.my) - i64::from(r.cut_my));
        }
        if r.aborted {
            n.failed_my.push(i64::from(r.my) - i64::from(r.failed_my));
        }
        n.rift_cells.push(i64::from(r.cells));
        if r.parted_my != never {
            n.unzip_my
                .push(i64::from(r.cut_my) - i64::from(r.parted_my));
        }
        if r.straight_km >= 1000 {
            n.sinuous
                .push(i64::from(r.along_km) * 1000 / i64::from(r.straight_km));
            n.in_belts
                .push(i64::from(r.belt_cells) * 1000 / i64::from(r.cut_cells.max(1)));
        }
    }
    // The first cut of the planet (the supercontinent's): the earliest
    // trough over 1000 km that cut its plate.
    let first = world
        .history
        .rifts
        .iter()
        .filter(|r| r.cut_my != never && r.straight_km >= 1000)
        .max_by_key(|r| (r.cut_my, r.my));
    if let Some(r) = first {
        n.sinuous_first
            .push(i64::from(r.along_km) * 1000 / i64::from(r.straight_km));
        n.bow_first
            .push(i64::from(r.bow_km) * 1000 / i64::from(r.straight_km));
        n.in_belts_first
            .push(i64::from(r.belt_cells) * 1000 / i64::from(r.cut_cells.max(1)));
    }
    // The mosaic under the supercontinent at the start: the share of its
    // crust that lies in a belt, and the belts' widths.
    let m = &world.mosaic;
    let crust: Vec<usize> = (0..CELLS)
        .filter(|&c| world.history.class[0][c] == history::CONTINENT)
        .collect();
    let mut wide: Vec<i64> = crust
        .iter()
        .filter(|&&c| m.edge[c].is_some())
        .map(|&c| 2 * i64::from(m.half_km[c]))
        .collect();
    wide.sort_unstable();
    n.belt_land.push(
        i64::try_from(crust.iter().filter(|&&c| m.belt[c] > 0).count() * 1000 / crust.len().max(1))
            .unwrap_or(0),
    );
    if !wide.is_empty() {
        n.belt_wide.push(wide[wide.len() / 2]);
    }
}
