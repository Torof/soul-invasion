//! The climate, the discharge and the biomes: passes 5 and 6 of
//! [`super`].
//!
//! Temperature falls with latitude and height. Rain is carried by the
//! prevailing winds (westerlies in the middle latitudes, trade winds
//! nearer the tropics, polar easterlies beyond 65°), swept across the
//! continent row by row: the air recharges over the sea, rains a little
//! on every cell, rains hard where it climbs and hardly at all where it
//! comes down. Deserts are born behind ranges and far inland; nothing
//! places them. The same principle as Red Blob Games's mapgen4.

use super::relief::{Relief, fbm};
use super::{CELL_M, Continent, Grid, iroot, isqrt, table};
use crate::r#gen::scatter::{purpose, roll};
use crate::geology::Rock;

/// Latitude of the northern edge, in hundredths of a degree: drawn in
/// this range, so seeds run from cool-temperate to subtropical. The grid
/// spans about 19° of latitude (2,097 km at 111 km a degree).
const NORTH_LATITUDE: (u64, u64) = (4_400, 6_400);

/// Kilometres a degree of latitude, times 100 (111.2 km).
const KM_PER_DEGREE_100: i64 = 11_120;

/// Sea-level yearly mean, in tenths of a degree, by latitude in degrees:
/// Earth's zonal means, rounded.
const SEA_LEVEL_TEMPERATURE: [(i64, i64); 7] = [
    (0, 270),
    (15, 255),
    (30, 195),
    (45, 105),
    (60, -10),
    (75, -150),
    (90, -250),
];

/// Cooling with height: 6.5 °C a kilometre, the standard lapse rate.
const LAPSE_TENTHS_PER_KM: i64 = 65;

/// The warmth's anomalies (warm and cold currents, the lie of the land),
/// up to this much either way in tenths of a degree, over cells of 600
/// and 250 km: isotherms that wander instead of straight latitude lines.
const WARMTH_TENTHS: i64 = 35;
const WARMTH_OCTAVES: [(u32, i64); 2] = [(300, 2), (120, 1)];

/// The latitude of the northern edge for `seed`, in hundredths of a degree.
pub(crate) fn north_latitude(seed: u64) -> i64 {
    i64::try_from(
        NORTH_LATITUDE.0
            + roll(seed, purpose::PLAN_LATITUDE, &[]) % (NORTH_LATITUDE.1 - NORTH_LATITUDE.0 + 1),
    )
    .expect("small")
}

/// The latitude of row `y`, in hundredths of a degree.
fn row_latitude(north: i64, y: i32) -> i64 {
    north - i64::from(y) * CELL_M * 100 * 100 / (KM_PER_DEGREE_100 * 1000)
}

/// The yearly mean at `height_cm` on row `y`, in tenths of a degree, from
/// latitude and height only (the relief's glaciers read it).
pub(crate) fn ground_tenths(north: i64, y: i32, height_cm: i32) -> i64 {
    let sea_level = table(
        &SEA_LEVEL_TEMPERATURE.map(|(l, t)| (l * 100, t)),
        row_latitude(north, y),
    );
    sea_level - i64::from(height_cm.max(0)) * LAPSE_TENTHS_PER_KM / 100_000
}

/// The share of the air's moisture that blows from the west, per mille,
/// by latitude in degrees: trade winds (from the east) up to about 25°,
/// westerlies from 35° to 60°, polar easterlies past 68°. This is Earth's
/// pattern; each seed turns it ([`WIND_TURNS`]) and bends it.
const WESTERLY_SHARE: [(i64, i64); 6] = [(0, 0), (25, 0), (35, 1000), (60, 1000), (68, 0), (90, 0)];

/// How far a seed turns the prevailing winds, in sixteenths of a turn,
/// by a draw per mille: under the first, not at all (the wind still
/// wanders from place to place); under the second, by one or two either
/// way (22.5° or 45°); past it, anywhere (a continent whose rain comes
/// from the north, a monsoon coast): not every continent is wet in the
/// west and dry in the east.
const WIND_TURNS: (u64, u64) = (650, 900);

/// The wind's strength, per mille, drawn per seed: a strong wind carries
/// the sea's moisture further inland (wetter heartlands), a weak one
/// drops it near the coasts (drier heartlands).
const WIND_STRENGTH: (u64, u64) = (800, 1_250);

/// The wind wanders from place to place (weather systems, the lie of
/// the coasts): a vector of up to this per mille of the prevailing one
/// added, over cells of 500 and 250 km.
const WIND_WANDER: i64 = 350;
const WIND_OCTAVES: [(u32, i64); 2] = [(250, 2), (120, 1)];

/// Air meeting a range turns along it instead of climbing: the share of
/// its uphill part turned away, per mille, grows with the smoothed
/// ground's slope, full at this many cm a cell, and is at most this.
const DEFLECT_CM: i64 = 6_000;
const DEFLECT_MOST: i64 = 600;
/// The ground the wind turns on, smoothed this many cells each way.
const DEFLECT_SMOOTHING: usize = 10;

/// Warm and cold sea currents: the moisture of the air over the sea is
/// multiplied by up to this per mille more or less, over cells of 600
/// and 300 km (a cold current makes a dry coast, a warm one a wet one).
const CURRENT_PER_MILLE: i64 = 250;
const CURRENT_OCTAVES: [(u32, i64); 2] = [(300, 2), (150, 1)];

/// The moisture of air over the sea, by latitude in degrees, per mille of
/// [`SEA_MOISTURE`]: wet tropics, the dry subtropical highs near 25-30°,
/// wet middle latitudes, dry polar air.
const SEA_MOISTURE_BY_LATITUDE: [(i64, i64); 8] = [
    (0, 1500),
    (12, 1300),
    (22, 600),
    (30, 600),
    (40, 1000),
    (60, 900),
    (75, 500),
    (90, 300),
];

/// The air's moisture over the sea, in the sweep's units: a millimetre a
/// year of rain for each [`RAIN_BASE_PER_100K`] hundred-thousandths.
const SEA_MOISTURE: i64 = 103_000;

/// Share of its moisture the air gives back to the sea a cell, per mille:
/// the air off a coast is as wet as the open sea after a few cells.
const SEA_RECHARGE_PER_MILLE: i64 = 150;

/// Rain a land cell receives from the air passing, per hundred thousand
/// of its moisture (1 %): about 800 mm on a western coast at 45°.
const RAIN_BASE_PER_100K: i64 = 1_000;

/// Share of that rain the land gives back to the air (evaporation, the
/// forests' breath), per mille: inland rain fades over a thousand km or
/// so, not a hundred.
const RECYCLE_PER_MILLE: i64 = 850;

/// Air climbing this many cm (of the smoothed ground) loses all its
/// moisture: a climb of a tenth of it wrings out a tenth.
const OROGRAPHIC_CM: i64 = 170_000;

/// Air coming down this many cm a cell gives no rain: the lee side.
const DESCENT_CM: i64 = 15_000;

/// Times a column the rows' air is mixed with its neighbours' (a
/// shadow behind a range of 100 km spreads to about 200 km 500 km on).
const AIR_MIXING: usize = 6;

/// The ground the wind feels: smoothed over this many cells each way,
/// twice, so the rain follows ranges, not every valley.
const WIND_SMOOTHING: usize = 4;

/// The most rain a cell receives, in mm a year (the wettest slopes on
/// Earth's windward sides are past 4,000; a cap keeps the sweep sane).
const RAIN_MOST: i64 = 4_500;

/// What the air could take back from the ground a year, in mm, from the
/// mean temperature in tenths of a degree (a linear fit of
/// Thornthwaite's: 300 mm at 0 °C, 1,200 at 20 °C).
fn potential_evaporation(tenths: i64) -> i64 {
    (300 + tenths * 9 / 2).clamp(30, 2_000)
}

/// What the rain leaves after evaporation, in mm a year: Budyko's curve
/// in Pike's form, `P - P·E / √(P² + E²)`.
fn runoff(rain: i64, evaporation: i64) -> i64 {
    if rain == 0 {
        return 0;
    }
    let root = i64::try_from(isqrt(
        u64::try_from(rain * rain + evaporation * evaporation).expect("square"),
    ))
    .expect("small");
    rain - rain * evaporation / root
}

/// Millilitres a second from a cell for a millimetre a year of runoff:
/// 1 mm over 2,048 m × 2,048 m, over the seconds of a year.
const MILLILITRES_PER_MM_NUM: u64 = 4_194_304_000;
const SECONDS_A_YEAR: u64 = 31_556_952;

// ---------------------------------------------------------------- biomes

/// The biomes of the atlas, with the sea and the lakes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Biome {
    /// The sea.
    Sea,
    /// A lake.
    Lake,
    /// Too dry for grass to close: sand, gravel, scrub.
    Desert,
    /// Dry grass, cold or warm: the steppe.
    Steppe,
    /// The steppe's wetter edge: grass with groves of trees in the
    /// hollows and on the north slopes (forest-steppe, savanna).
    WoodedSteppe,
    /// Grassland and parkland: prairie, meadows, scattered trees.
    Grassland,
    /// Broadleaf forest: oak, beech, lime; the tropics' forest too.
    Broadleaf,
    /// Broadleaf and conifer together, where the one gives way to the
    /// other: oak and pine, beech and fir.
    MixedForest,
    /// Conifer forest: pine, spruce, fir, larch.
    Conifer,
    /// Wet flats: reeds, sedge, alder carr, bog.
    Marsh,
    /// Too cold for trees: moss, lichen, dwarf shrubs.
    Tundra,
    /// Above the trees: rock, alpine meadows, snow.
    Alpine,
}

impl Biome {
    /// The biomes of the land, in this order.
    pub const LAND: [Self; 10] = [
        Self::Desert,
        Self::Steppe,
        Self::WoodedSteppe,
        Self::Grassland,
        Self::Broadleaf,
        Self::MixedForest,
        Self::Conifer,
        Self::Marsh,
        Self::Tundra,
        Self::Alpine,
    ];
}

/// Aridity (rain over potential evaporation, per mille) under which the
/// land is desert, steppe, and grassland; over the last, forest. The
/// UNEP aridity classes, rounded.
const DESERT_ARIDITY: i64 = 200;
const STEPPE_ARIDITY: i64 = 450;
/// Between the steppe and the grassland, the wooded steppe.
const WOODED_STEPPE_ARIDITY: i64 = 620;
const GRASSLAND_ARIDITY: i64 = 900;

/// Under this mean temperature (tenths of a degree) trees give way to
/// tundra; under this one, forests are conifers; under this one, mixed.
const TUNDRA_TENTHS: i64 = -70;
const CONIFER_TENTHS: i64 = 40;
const MIXED_TENTHS: i64 = 75;

/// Biome borders are not lines: the aridity a biome is read from wanders
/// by up to this per mille either way, and the warmth by up to this many
/// tenths, over cells of 80 and 30 km, so forests and grasslands
/// interfinger along their border.
const ECOTONE_ARIDITY_PER_MILLE: i64 = 150;
const ECOTONE_TENTHS: i64 = 20;
const ECOTONE_OCTAVES: [(u32, i64); 2] = [(40, 2), (14, 1)];

/// Limestone and chalk drink the rain: where their water table lies
/// deeper than this (cm), the soil dries out like a climate this much
/// drier (per mille of the aridity kept): the great calcareous
/// grasslands.
const KARST_DRY_TABLE_CM: i32 = 4_000;
const KARST_ARIDITY_PER_MILLE: i64 = 800;

// ---------------------------------------------------------------- water underground

/// The share of the runoff each rock lets sink into the ground, per
/// mille: karst limestone and chalk drink most of it (few rivers, dry
/// valleys), sandstone a part, granite little (dense networks); river
/// gravels a fair share.
const INFILTRATION_CHALK: u64 = 750;
const INFILTRATION_LIMESTONE: u64 = 700;
const INFILTRATION_SANDSTONE: u64 = 400;
const INFILTRATION_GRANITE: u64 = 100;
const INFILTRATION_ALLUVIUM: u64 = 300;

/// A rock is an aquifer (the water goes on underground) from this
/// infiltration up; it holds the water up (a spring line at its foot)
/// at this or under.
const AQUIFER_INFILTRATION: u64 = 400;
const SEAL_INFILTRATION: u64 = 150;

/// A cell's water table meets the surface where a stream at least this
/// big (mL/s) runs: the valley floor is at the water table.
const STREAM_ML: u64 = 400_000;

/// Underground, the water runs on until there is this much of it (mL/s)
/// for the rock to hold, then rises: karst carries whole rivers out of
/// sight, sandstone less, granite hardly any.
const HOLD_KARST_ML: u64 = 6_000_000;
const HOLD_SANDSTONE_ML: u64 = 1_500_000;
const HOLD_GRANITE_ML: u64 = 200_000;

/// A spring is where at least this much (mL/s) comes up at once.
const SPRING_ML: u64 = 50_000;

/// The water table's depth below the ground, per mille of the height
/// above the water it drains to: it follows the relief closely in
/// granite and gravels, sits deep under karst plateaus. Never less than
/// [`TABLE_LEAST_CM`] away from water.
const TABLE_GRANITE: i64 = 200;
const TABLE_SANDSTONE: i64 = 450;
const TABLE_LIMESTONE: i64 = 850;
const TABLE_CHALK: i64 = 750;
const TABLE_ALLUVIUM: i64 = 80;
const TABLE_LEAST_CM: i64 = 150;

// ---------------------------------------------------------------- reaches

/// How a river runs through a cell, which sets how shallow its
/// shallowest crossing is (see [`super::measure`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// One channel, pools and riffles.
    Single,
    /// A gravel bar where a tributary drops its load, or where the river
    /// slows down: the channel spreads over it.
    Bar,
    /// A braided reach: many shallow arms over a wide gravel bed, where
    /// the slope is steep for the discharge (Leopold and Wolman 1957) or
    /// where a river leaves the hills and drops its load.
    Braided,
    /// A lake's outlet, spilling over a broad sill.
    Sill,
}

/// Rivers under this discharge (mL/s) are not given a reach: brooks.
const REACH_ML: u64 = 5_000_000;

/// How much wider than its usual width a reach's widest riffle spreads,
/// per mille, by a table on a draw per mille: riffles come every five
/// to seven widths, several to a cell, and some reaches spread over
/// islands and gravel bars more than others; more so on the river's own
/// gravels than between rock banks. A river `k` times wider runs
/// `k^-0.6` as deep (see [`super::measure`]).
const SPREAD_CONFINED: [(i64, i64); 5] = [
    (0, 1_000),
    (800, 1_300),
    (950, 1_900),
    (990, 2_800),
    (1000, 3_500),
];
const SPREAD_ALLUVIAL: [(i64, i64); 5] = [
    (0, 1_100),
    (600, 1_500),
    (850, 2_400),
    (960, 4_000),
    (1000, 6_000),
];
/// The spread of a bar, a braided reach and a lake's sill, per mille.
const SPREAD_BAR: u16 = 4_000;
const SPREAD_BRAIDED: u16 = 8_000;
const SPREAD_SILL: u16 = 6_000;

/// At low water, late in a dry season, a river carries what comes up
/// from the ground and this share of the rest, per mille: rain-fed
/// rivers on granite shrink to a fifth, spring-fed ones on chalk hardly
/// shrink.
const LOW_RUNOFF_PER_MILLE: u64 = 150;

/// Leopold and Wolman's braiding threshold, `S = 0.013 Q^-0.44`, as a
/// drop over a cell: `2,662 cm / Q^(4/9)`, `Q` in m³/s.
const BRAID_DROP_CM: i64 = 2_662;

/// A river leaving the hills: the reach above dropped at least this
/// (cm a cell) and this many times more than this one.
const FAN_ABOVE_CM: i32 = 1_500;
const FAN_RATIO: i32 = 4;

/// A tributary carrying at least this share of the river below (per
/// mille) builds a bar at the junction; and so does a slowing-down, where
/// the reach above dropped this many times more than this one.
const BAR_TRIBUTARY_PER_MILLE: u64 = 120;
const BAR_SLOWING: i32 = 3;
const BAR_ABOVE_CM: i32 = 300;

/// Above this height (cm) and under this temperature (tenths), the land
/// is above the trees.
const ALPINE_CM: i32 = 150_000;
const ALPINE_TENTHS: i64 = -10;

/// A wet flat: draining at least this many millilitres a second, dropping
/// at most this many cm a cell, wetter than this aridity; or a filled
/// hollow too shallow to be a lake.
const MARSH_DISCHARGE: u64 = 20_000_000;
const MARSH_DROP_CM: i32 = 60;
const MARSH_ARIDITY: i64 = 600;

/// Passes 5 and 6: climate, discharge and biomes over a relief.
#[allow(clippy::too_many_lines)]
pub(crate) fn settle(seed: u64, grid: Grid, relief: Relief) -> Continent {
    let n = grid.len();
    let north = north_latitude(seed);
    let latitude = |y: i32| row_latitude(north, y);
    let surface = &relief.routing.filled;

    let temperature: Vec<i16> = (0..n)
        .map(|i| {
            let (x, y) = grid.xy(i);
            let h = if relief.sea[i] { 0 } else { surface[i] };
            let anomaly = fbm(seed, purpose::PLAN_WARMTH, grid.side, x, y, &WARMTH_OCTAVES)
                * WARMTH_TENTHS
                / 250;
            i16::try_from((ground_tenths(north, y, h) + anomaly).clamp(-600, 400)).expect("clamped")
        })
        .collect();

    let rain = rain(seed, grid, &relief.sea, surface, &latitude);

    // Runoff, split by the rock between the rivers and the ground.
    let mut runoff_ml = vec![0_u64; n];
    for i in 0..n {
        if !relief.sea[i] {
            let r = runoff(
                i64::from(rain[i]),
                potential_evaporation(i64::from(temperature[i])),
            );
            runoff_ml[i] =
                u64::try_from(r).expect("never negative") * MILLILITRES_PER_MM_NUM / SECONDS_A_YEAR;
        }
    }
    let ground = underground(&relief, &runoff_ml);
    let discharge = ground.discharge;

    let biome: Vec<Biome> = (0..n)
        .map(|i| {
            if relief.sea[i] {
                return Biome::Sea;
            }
            if relief.lake[i] {
                return Biome::Lake;
            }
            let (x, y) = grid.xy(i);
            let t = i64::from(temperature[i]);
            let mut aridity = i64::from(rain[i]) * 1000 / potential_evaporation(t);
            let wander = |dx: i32| {
                fbm(
                    seed,
                    purpose::PLAN_ECOTONE,
                    grid.side,
                    x + dx,
                    y,
                    &ECOTONE_OCTAVES,
                )
            };
            aridity = aridity * (1000 + wander(0) * ECOTONE_ARIDITY_PER_MILLE / 250) / 1000;
            let karst = matches!(relief.rock[i], Rock::Limestone | Rock::Chalk)
                && !relief.alluvium[i]
                && ground.table[i] >= KARST_DRY_TABLE_CM;
            if karst {
                aridity = aridity * KARST_ARIDITY_PER_MILLE / 1000;
            }
            let t = t + wander(0x4000) * ECOTONE_TENTHS / 250;
            let r = relief.routing.receiver[i] as usize;
            let drop = surface[i] - surface[r];
            let hollow = surface[i] > relief.height[i];
            let wet_flat = (hollow || (discharge[i] >= MARSH_DISCHARGE && drop <= MARSH_DROP_CM))
                && aridity >= MARSH_ARIDITY;
            if t < ALPINE_TENTHS && relief.height[i] >= ALPINE_CM {
                Biome::Alpine
            } else if t < TUNDRA_TENTHS {
                Biome::Tundra
            } else if aridity < DESERT_ARIDITY {
                Biome::Desert
            } else if aridity < STEPPE_ARIDITY {
                Biome::Steppe
            } else if wet_flat {
                Biome::Marsh
            } else if aridity < WOODED_STEPPE_ARIDITY {
                Biome::WoodedSteppe
            } else if aridity < GRASSLAND_ARIDITY {
                Biome::Grassland
            } else if t < CONIFER_TENTHS {
                Biome::Conifer
            } else if t < MIXED_TENTHS {
                Biome::MixedForest
            } else {
                Biome::Broadleaf
            }
        })
        .collect();

    let (reach, spread) = reaches(seed, &relief, &discharge);
    let low_discharge: Vec<u64> = (0..n)
        .map(|i| {
            ground.baseflow[i] + (discharge[i] - ground.baseflow[i]) * LOW_RUNOFF_PER_MILLE / 1000
        })
        .collect();
    Continent {
        seed,
        grid,
        north_latitude: i32::try_from(north).expect("small"),
        water_table: ground.table,
        spring: ground.spring,
        reach,
        spread,
        low_discharge,
        surface: relief.routing.filled,
        receiver: relief.routing.receiver,
        order: relief.routing.order,
        sea: relief.sea,
        height: relief.height,
        lake: relief.lake,
        uplift: relief.uplift,
        exhumed: relief.exhumed,
        rock: relief.rock,
        alluvium: relief.alluvium,
        temperature,
        rain,
        discharge,
        biome,
        strokes: relief.strokes,
    }
}

/// A box blur of `radius` cells, twice, edges clamped.
fn smooth(grid: Grid, values: &[i32], radius: usize) -> Vec<i32> {
    let side = grid.side;
    let mut a: Vec<i64> = values.iter().map(|&v| i64::from(v)).collect();
    let mut b = vec![0_i64; a.len()];
    let r = radius;
    for _ in 0..2 {
        for y in 0..side {
            for x in 0..side {
                let (lo, hi) = (x.saturating_sub(r), (x + r).min(side - 1));
                let sum: i64 = a[y * side + lo..=y * side + hi].iter().sum();
                b[y * side + x] = sum / i64::try_from(hi - lo + 1).expect("small");
            }
        }
        for y in 0..side {
            for x in 0..side {
                let (lo, hi) = (y.saturating_sub(r), (y + r).min(side - 1));
                let sum: i64 = (lo..=hi).map(|yy| b[yy * side + x]).sum();
                a[y * side + x] = sum / i64::try_from(hi - lo + 1).expect("small");
            }
        }
    }
    a.iter()
        .map(|&v| i32::try_from(v).expect("an average of i32"))
        .collect()
}

/// The rain of every cell, in mm a year: eight sweeps of moist air, one
/// from each way the wind can blow ([`SWEEPS`]), mixed at each cell by
/// the wind there: Earth's zonal pattern turned by the seed, wandering
/// from place to place, and turned along the ranges it meets.
#[allow(clippy::too_many_lines)]
fn rain(
    seed: u64,
    grid: Grid,
    sea: &[bool],
    surface: &[i32],
    latitude: &dyn Fn(i32) -> i64,
) -> Vec<u16> {
    let side = grid.side;
    let n = grid.len();
    let ground: Vec<i32> = surface
        .iter()
        .zip(sea)
        .map(|(&h, &s)| if s { 0 } else { h.max(0) })
        .collect();
    let rows: Vec<i64> = (0..side)
        .map(|y| latitude(i32::try_from(y).expect("small")).abs() / 100)
        .collect();
    // The sea's moisture, by latitude and by the currents.
    let sea_air: Vec<i64> = (0..n)
        .map(|i| {
            let (x, y) = grid.xy(i);
            let current = fbm(seed, purpose::PLAN_CURRENT, side, x, y, &CURRENT_OCTAVES);
            SEA_MOISTURE
                * table(
                    &SEA_MOISTURE_BY_LATITUDE,
                    rows[usize::try_from(y).expect("small")],
                )
                / 1000
                * (1000 + current * CURRENT_PER_MILLE / 250).clamp(300, 1800)
                / 1000
        })
        .collect();
    let strength = i64::try_from(
        WIND_STRENGTH.0
            + roll(seed, purpose::PLAN_WIND, &[1]) % (WIND_STRENGTH.1 - WIND_STRENGTH.0 + 1),
    )
    .expect("small");
    let rain_base = RAIN_BASE_PER_100K * 1000 / strength;
    let smoothed = smooth(grid, &ground, WIND_SMOOTHING);
    let sweep = |dx: i32, dy: i32| sweep(grid, sea, &sea_air, &smoothed, rain_base, dx, dy);
    let sweeps: Vec<Vec<i64>> = SWEEPS.iter().map(|&(dx, dy)| sweep(dx, dy)).collect();
    // The seed's turn of the winds.
    let draw = roll(seed, purpose::PLAN_WIND, &[0]) % 1000;
    let way = roll(seed, purpose::PLAN_WIND, &[2]);
    let turn = if draw < WIND_TURNS.0 {
        0
    } else if draw < WIND_TURNS.1 {
        [14, 15, 1, 2][usize::try_from(way % 4).expect("small")]
    } else {
        way % 16
    };
    let (cos, sin) = super::relief::HEADINGS[usize::try_from(turn).expect("small")];
    let broad = smooth(grid, &ground, DEFLECT_SMOOTHING);
    (0..n)
        .map(|i| {
            let (x, y) = grid.xy(i);
            let row = usize::try_from(y).expect("small");
            // Earth's pattern: +x blows toward the east, +y toward the south.
            let zonal = 2 * table(&WESTERLY_SHARE, rows[row]) - 1000;
            let mut wx = zonal * cos / 1000;
            let mut wy = zonal * sin / 1000;
            wx += fbm(seed, purpose::PLAN_WIND, side, x, y, &WIND_OCTAVES) * WIND_WANDER / 250;
            wy += fbm(seed, purpose::PLAN_WIND, side, x + 0x4000, y, &WIND_OCTAVES) * WIND_WANDER
                / 250;
            // Turned along a range rather than over it.
            let at = |dx: i32, dy: i32| {
                let (nx, ny) = (
                    (x + dx).clamp(0, i32::try_from(side).expect("small") - 1),
                    (y + dy).clamp(0, i32::try_from(side).expect("small") - 1),
                );
                i64::from(broad[grid.index(nx, ny)])
            };
            let (gx, gy) = ((at(1, 0) - at(-1, 0)) / 2, (at(0, 1) - at(0, -1)) / 2);
            let g2 = gx * gx + gy * gy;
            let uphill = wx * gx + wy * gy;
            if uphill > 0 && g2 > 0 {
                let g = i64::try_from(isqrt(u64::try_from(g2).expect("square"))).expect("small");
                let f = (g * 1000 / DEFLECT_CM).min(DEFLECT_MOST);
                wx -= uphill * gx / g2 * f / 1000;
                wy -= uphill * gy / g2 * f / 1000;
            }
            // The sweeps nearest the wind's heading, sharply: the two
            // either side of it, hardly any other.
            let length = i64::try_from(isqrt(u64::try_from(wx * wx + wy * wy).expect("square")))
                .expect("small");
            let weights: Vec<i64> = SWEEPS
                .iter()
                .map(|&(dx, dy)| {
                    if length == 0 {
                        return 0;
                    }
                    let unit = if dx != 0 && dy != 0 { 707 } else { 1000 };
                    let dot = ((wx * i64::from(dx) + wy * i64::from(dy)) * unit / length).max(0);
                    let d2 = dot * dot / 1000;
                    d2 * d2 / 1000
                })
                .collect();
            let total: i64 = weights.iter().sum();
            let r = if total == 0 {
                sweeps.iter().map(|s| s[i]).sum::<i64>() / 8
            } else {
                weights
                    .iter()
                    .zip(&sweeps)
                    .map(|(w, s)| w * s[i])
                    .sum::<i64>()
                    / total
            };
            u16::try_from(r.clamp(0, RAIN_MOST)).expect("clamped")
        })
        .collect()
}

/// The eight ways the air is swept across the continent, as the step it
/// moves by: east, south-east, south, south-west, west, north-west,
/// north, north-east.
const SWEEPS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// The rain a wind moving by `(dx, dy)` a step leaves on every cell, if
/// it blew that way everywhere: lines of air side by side, each moving
/// one step at a time, recharged over the sea, raining on the land, hard
/// where it climbs and hardly where it comes down, and mixing with the
/// lines beside it as it goes.
fn sweep(
    grid: Grid,
    sea: &[bool],
    sea_air: &[i64],
    ground: &[i32],
    rain_base: i64,
    dx: i32,
    dy: i32,
) -> Vec<i64> {
    let side = i32::try_from(grid.side).expect("small");
    let n = grid.len();
    // How far along the wind (progress) and across it (line) a cell is,
    // both shifted to start at 0.
    let span = 2 * side - 1;
    let low = |a: i32, b: i32| a.min(0) * (side - 1) + b.min(0) * (side - 1);
    let (p0, l0) = (low(dx, dy), low(dy, -dx));
    let progress = |x: i32, y: i32| dx * x + dy * y - p0;
    let line = |x: i32, y: i32| dy * x - dx * y - l0;
    let mut starts = vec![0_usize; usize::try_from(span + 1).expect("small")];
    for i in 0..n {
        let (x, y) = grid.xy(i);
        starts[usize::try_from(progress(x, y)).expect("small") + 1] += 1;
    }
    for k in 1..starts.len() {
        starts[k] += starts[k - 1];
    }
    let mut cells = vec![0_u32; n];
    let mut fill = starts.clone();
    for i in 0..n {
        let (x, y) = grid.xy(i);
        let p = usize::try_from(progress(x, y)).expect("small");
        cells[fill[p]] = u32::try_from(i).expect("small");
        fill[p] += 1;
    }
    let step_per_mille = if dx != 0 && dy != 0 { 1_414 } else { 1_000 };
    let lines = usize::try_from(span).expect("small");
    let mut air = vec![-1_i64; lines];
    let mut mixed = air.clone();
    let mut out = vec![0_i64; n];
    for p in 0..lines {
        // The lines' air mixes with its neighbours': the wind is not a
        // ruler, and a range's shadow spreads as it goes.
        for _ in 0..AIR_MIXING {
            for k in 0..lines {
                if air[k] < 0 {
                    mixed[k] = air[k];
                    continue;
                }
                let up = air[k.saturating_sub(1)];
                let down = air[(k + 1).min(lines - 1)];
                let (up, down) = (
                    if up < 0 { air[k] } else { up },
                    if down < 0 { air[k] } else { down },
                );
                mixed[k] = (up + 2 * air[k] + down) / 4;
            }
            std::mem::swap(&mut air, &mut mixed);
        }
        for &c in &cells[starts[p]..starts[p + 1]] {
            let c = c as usize;
            let (x, y) = grid.xy(c);
            let k = usize::try_from(line(x, y)).expect("small");
            if air[k] < 0 {
                air[k] = sea_air[c];
            }
            if sea[c] {
                air[k] +=
                    (sea_air[c] - air[k]) * SEA_RECHARGE_PER_MILLE / 1000 * step_per_mille / 1000;
                continue;
            }
            let (bx, by) = (x - dx, y - dy);
            let behind = if grid.inside(bx, by) {
                grid.index(bx, by)
            } else {
                c
            };
            let climb = i64::from(ground[c]) - i64::from(ground[behind]);
            let mut base = air[k] * rain_base / 100_000 * step_per_mille / 1000;
            if climb < 0 {
                base = base * (DESCENT_CM + climb).max(0) / DESCENT_CM;
            }
            let lifted = if climb > 0 {
                (air[k] * climb / OROGRAPHIC_CM).min(air[k] / 2)
            } else {
                0
            };
            out[c] = base + lifted;
            air[k] -= base + lifted - base * RECYCLE_PER_MILLE / 1000;
        }
    }
    out
}

/// What the rock does with the runoff: the surface discharge of every
/// cell, the water table's depth and the springs.
struct Underground {
    discharge: Vec<u64>,
    /// The part of the discharge that came up from the ground.
    baseflow: Vec<u64>,
    table: Vec<i32>,
    spring: Vec<bool>,
}

fn infiltration(rock: Rock, alluvium: bool) -> u64 {
    if alluvium {
        return INFILTRATION_ALLUVIUM;
    }
    match rock {
        Rock::Chalk => INFILTRATION_CHALK,
        Rock::Limestone => INFILTRATION_LIMESTONE,
        Rock::Sandstone => INFILTRATION_SANDSTONE,
        Rock::Granite => INFILTRATION_GRANITE,
    }
}

/// The water underground: each cell's runoff split by its rock between
/// the surface and the ground; the ground's water runs down the same
/// valleys out of sight, and comes up (a spring) where its rock ends
/// against one that holds it up, where there is more of it than the rock
/// can carry, or where it meets a stream, a lake or the sea. The water
/// table lies at the surface where water does, and elsewhere at a depth
/// that grows with the height above that water, by the rock.
fn underground(relief: &Relief, runoff: &[u64]) -> Underground {
    let n = relief.grid.len();
    let routing = &relief.routing;
    let mut surface = vec![0_u64; n];
    let mut below = vec![0_u64; n];
    let mut at_surface = vec![false; n];
    let mut spring = vec![false; n];
    let mut baseflow = vec![0_u64; n];
    for &c in routing.order.iter().rev() {
        let c = c as usize;
        if relief.sea[c] {
            at_surface[c] = true;
            continue;
        }
        let soak = infiltration(relief.rock[c], relief.alluvium[c]);
        let sinks = runoff[c] * soak / 1000;
        surface[c] += runoff[c] - sinks;
        below[c] += sinks;
        let r = routing.receiver[c] as usize;
        let hold = match relief.rock[c] {
            _ if relief.alluvium[c] => HOLD_SANDSTONE_ML,
            Rock::Chalk | Rock::Limestone => HOLD_KARST_ML,
            Rock::Sandstone => HOLD_SANDSTONE_ML,
            Rock::Granite => HOLD_GRANITE_ML,
        };
        let sealed = soak >= AQUIFER_INFILTRATION
            && !relief.sea[r]
            && infiltration(relief.rock[r], relief.alluvium[r]) <= SEAL_INFILTRATION;
        let wet = relief.lake[c] || routing.filled[c] > relief.height[c] || relief.sea[r];
        let stream = surface[c] >= STREAM_ML;
        if wet || stream || sealed || below[c] >= hold {
            // A spring where the water comes up out of dry ground, or
            // where an aquifer ends against a rock that holds it.
            spring[c] =
                !wet && below[c] >= SPRING_ML && (sealed || !stream || below[c] >= surface[c]);
            surface[c] += below[c];
            baseflow[c] += below[c];
            below[c] = 0;
            at_surface[c] = true;
        }
        at_surface[c] |= surface[c] >= STREAM_ML;
        if r != c {
            surface[r] += surface[c];
            below[r] += below[c];
            baseflow[r] += baseflow[c];
        }
    }
    // The water each cell drains to, from the sea upward.
    let mut base = vec![0_i32; n];
    let mut table = vec![0_i32; n];
    for &c in &routing.order {
        let c = c as usize;
        let r = routing.receiver[c] as usize;
        base[c] = if at_surface[c] {
            routing.filled[c]
        } else if at_surface[r] {
            routing.filled[r]
        } else {
            base[r]
        };
        if at_surface[c] || relief.sea[c] {
            continue;
        }
        let ratio = if relief.alluvium[c] {
            TABLE_ALLUVIUM
        } else {
            match relief.rock[c] {
                Rock::Granite => TABLE_GRANITE,
                Rock::Sandstone => TABLE_SANDSTONE,
                Rock::Limestone => TABLE_LIMESTONE,
                Rock::Chalk => TABLE_CHALK,
            }
        };
        let above = i64::from(relief.height[c] - base[c]).max(0);
        table[c] = i32::try_from((above * ratio / 1000).max(TABLE_LEAST_CM)).expect("small");
    }
    // Sea cells carry nothing.
    for c in 0..n {
        if relief.sea[c] {
            surface[c] = 0;
            baseflow[c] = 0;
        }
    }
    Underground {
        discharge: surface,
        baseflow,
        table,
        spring,
    }
}

/// The reach of every river cell (see [`Reach`]), and how much wider
/// than its usual width its widest riffle spreads, per mille.
fn reaches(seed: u64, relief: &Relief, discharge: &[u64]) -> (Vec<Reach>, Vec<u16>) {
    let n = relief.grid.len();
    let routing = &relief.routing;
    let surface = &routing.filled;
    // Each cell's largest inflow, the cell it comes from, and whether a
    // lake spills into it.
    let mut main = vec![(0_u64, u32::MAX); n];
    let mut second = vec![0_u64; n];
    let mut sill = vec![false; n];
    for c in 0..n {
        if relief.sea[c] {
            continue;
        }
        let r = routing.receiver[c] as usize;
        if r == c || relief.sea[r] {
            continue;
        }
        if relief.lake[c] && !relief.lake[r] {
            sill[r] = true;
        }
        let q = discharge[c];
        if q > main[r].0 {
            second[r] = second[r].max(main[r].0);
            main[r] = (q, u32::try_from(c).expect("small"));
        } else {
            second[r] = second[r].max(q);
        }
    }
    let reach: Vec<Reach> = (0..n)
        .map(|c| {
            if relief.sea[c] || relief.lake[c] || discharge[c] < REACH_ML {
                return Reach::Single;
            }
            if sill[c] {
                return Reach::Sill;
            }
            let r = routing.receiver[c] as usize;
            let drop = surface[c] - surface[r];
            let q = discharge[c] / 1_000_000;
            let braid = BRAID_DROP_CM
                / i64::try_from(iroot(u128::from(q.max(1)).pow(4), 9).max(1)).expect("small");
            let above = if main[c].1 == u32::MAX {
                0
            } else {
                surface[main[c].1 as usize] - surface[c]
            };
            if i64::from(drop) >= braid
                || (above >= FAN_ABOVE_CM && above >= FAN_RATIO * drop.max(1))
            {
                Reach::Braided
            } else if second[c] * 1000 >= discharge[c] * BAR_TRIBUTARY_PER_MILLE
                || (above >= BAR_ABOVE_CM && above >= BAR_SLOWING * drop.max(1))
            {
                Reach::Bar
            } else {
                Reach::Single
            }
        })
        .collect();
    let spread = (0..n)
        .map(|c| {
            if relief.sea[c] || relief.lake[c] {
                return 1000;
            }
            let draw =
                i64::try_from(roll(seed, purpose::PLAN_REACH, &[c as u64]) % 1001).expect("small");
            let natural = table(
                if relief.alluvium[c] {
                    &SPREAD_ALLUVIAL
                } else {
                    &SPREAD_CONFINED
                },
                draw,
            );
            let natural = u16::try_from(natural).expect("small");
            match reach[c] {
                Reach::Single => natural,
                Reach::Bar => natural.max(SPREAD_BAR),
                Reach::Braided => natural.max(SPREAD_BRAIDED),
                Reach::Sill => natural.max(SPREAD_SILL),
            }
        })
        .collect();
    (reach, spread)
}
