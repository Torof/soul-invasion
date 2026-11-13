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

use super::relief::Relief;
use super::{CELL_M, Continent, Grid, isqrt, table};
use crate::r#gen::scatter::{purpose, roll};

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

/// The share of the air's moisture that blows from the west, per mille,
/// by latitude in degrees: trade winds (from the east) up to about 25°,
/// westerlies from 35° to 60°, polar easterlies past 68°.
const WESTERLY_SHARE: [(i64, i64); 6] = [(0, 0), (25, 0), (35, 1000), (60, 1000), (68, 0), (90, 0)];

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
const SEA_MOISTURE: i64 = 95_000;

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
const OROGRAPHIC_CM: i64 = 250_000;

/// Air coming down this many cm a cell gives no rain: the lee side.
const DESCENT_CM: i64 = 25_000;

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
    /// Grassland and parkland: prairie, meadows, scattered trees.
    Grassland,
    /// Broadleaf forest: oak, beech, lime; the tropics' forest too.
    Broadleaf,
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
    pub const LAND: [Self; 8] = [
        Self::Desert,
        Self::Steppe,
        Self::Grassland,
        Self::Broadleaf,
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
const GRASSLAND_ARIDITY: i64 = 900;

/// Under this mean temperature (tenths of a degree) trees give way to
/// tundra; under this one, forests are conifers.
const TUNDRA_TENTHS: i64 = -70;
const CONIFER_TENTHS: i64 = 60;

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
pub(crate) fn settle(seed: u64, grid: Grid, relief: Relief) -> Continent {
    let n = grid.len();
    let north = i64::try_from(
        NORTH_LATITUDE.0
            + roll(seed, purpose::PLAN_LATITUDE, &[]) % (NORTH_LATITUDE.1 - NORTH_LATITUDE.0 + 1),
    )
    .expect("small");
    // Latitude of a row, in hundredths of a degree.
    let latitude = |y: i32| north - i64::from(y) * CELL_M * 100 * 100 / (KM_PER_DEGREE_100 * 1000);
    let surface = &relief.routing.filled;

    let temperature: Vec<i16> = (0..n)
        .map(|i| {
            let (_, y) = grid.xy(i);
            let sea_level = table(
                &SEA_LEVEL_TEMPERATURE.map(|(l, t)| (l * 100, t)),
                latitude(y),
            );
            let h = if relief.sea[i] {
                0
            } else {
                i64::from(surface[i])
            };
            i16::try_from((sea_level - h * LAPSE_TENTHS_PER_KM / 100_000).clamp(-600, 400))
                .expect("clamped")
        })
        .collect();

    let rain = rain(grid, &relief.sea, surface, &latitude);

    // Discharge: runoff gathered down the network, through the lakes.
    let mut discharge = vec![0_u64; n];
    for i in 0..n {
        if !relief.sea[i] {
            let r = runoff(
                i64::from(rain[i]),
                potential_evaporation(i64::from(temperature[i])),
            );
            discharge[i] =
                u64::try_from(r).expect("never negative") * MILLILITRES_PER_MM_NUM / SECONDS_A_YEAR;
        }
    }
    for &c in relief.routing.order.iter().rev() {
        let c = c as usize;
        let r = relief.routing.receiver[c] as usize;
        if r != c && !relief.sea[c] && !relief.sea[r] {
            discharge[r] += discharge[c];
        }
    }

    let biome: Vec<Biome> = (0..n)
        .map(|i| {
            if relief.sea[i] {
                return Biome::Sea;
            }
            if relief.lake[i] {
                return Biome::Lake;
            }
            let t = i64::from(temperature[i]);
            let aridity = i64::from(rain[i]) * 1000 / potential_evaporation(t);
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
            } else if aridity < GRASSLAND_ARIDITY {
                Biome::Grassland
            } else if t < CONIFER_TENTHS {
                Biome::Conifer
            } else {
                Biome::Broadleaf
            }
        })
        .collect();

    Continent {
        seed,
        grid,
        north_latitude: i32::try_from(north).expect("small"),
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

/// The rain of every cell, in mm a year: two sweeps of moist air, one
/// from the west and one from the east, mixed by the latitude's winds.
fn rain(grid: Grid, sea: &[bool], surface: &[i32], latitude: &dyn Fn(i32) -> i64) -> Vec<u16> {
    let side = grid.side;
    let ground: Vec<i32> = surface
        .iter()
        .zip(sea)
        .map(|(&h, &s)| if s { 0 } else { h.max(0) })
        .collect();
    let ground = smooth(grid, &ground, WIND_SMOOTHING);
    let rows: Vec<i64> = (0..side)
        .map(|y| latitude(i32::try_from(y).expect("small")).abs() / 100)
        .collect();
    let sea_moisture: Vec<i64> = rows
        .iter()
        .map(|&l| SEA_MOISTURE * table(&SEA_MOISTURE_BY_LATITUDE, l) / 1000)
        .collect();
    let sweep = |from_west: bool| -> Vec<i64> {
        let mut out = vec![0_i64; grid.len()];
        let mut air = sea_moisture.clone();
        let mut mixed = air.clone();
        for step in 0..side {
            let x = if from_west { step } else { side - 1 - step };
            // The rows' air mixes with its neighbours': the wind is not a
            // ruler, and a range's shadow spreads as it goes.
            for _ in 0..AIR_MIXING {
                for y in 0..side {
                    let (up, down) = (air[y.saturating_sub(1)], air[(y + 1).min(side - 1)]);
                    mixed[y] = (up + 2 * air[y] + down) / 4;
                }
                std::mem::swap(&mut air, &mut mixed);
            }
            for y in 0..side {
                let c = y * side + x;
                if sea[c] {
                    air[y] += (sea_moisture[y] - air[y]) * SEA_RECHARGE_PER_MILLE / 1000;
                    continue;
                }
                let behind = if from_west {
                    c.saturating_sub(1)
                } else {
                    (c + 1).min(grid.len() - 1)
                };
                let climb = i64::from(ground[c]) - i64::from(ground[behind]);
                let mut base = air[y] * RAIN_BASE_PER_100K / 100_000;
                if climb < 0 {
                    base = base * (DESCENT_CM + climb).max(0) / DESCENT_CM;
                }
                let lifted = if climb > 0 {
                    (air[y] * climb / OROGRAPHIC_CM).min(air[y] / 2)
                } else {
                    0
                };
                out[c] = base + lifted;
                air[y] -= base + lifted - base * RECYCLE_PER_MILLE / 1000;
            }
        }
        out
    };
    let (west, east) = (sweep(true), sweep(false));
    (0..grid.len())
        .map(|i| {
            let (_, y) = grid.xy(i);
            let w = table(&WESTERLY_SHARE, rows[usize::try_from(y).expect("small")]);
            let r = (west[i] * w + east[i] * (1000 - w)) / 1000;
            u16::try_from(r.clamp(0, RAIN_MOST)).expect("clamped")
        })
        .collect()
}
