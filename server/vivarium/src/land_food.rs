//! The land's food by season (25 November 2026): measures only, asked
//! after the fertile runs, where the tribes collapsed in their second
//! spring. How much food the plants of the land hold, day by day, had
//! nobody picked any, and how much of it lies near each tribe's camp.
//!
//! - **The land measured**: the chunks of the food map (`bearers`: where a
//!   tribe's living stood at some noon, and two chunks around), and every
//!   chunk within [`RING_TILES`]'s wider ring of a tribe's camp at some
//!   noon (its living's median home, read at noon).
//! - **What each crop bears there**: once, after the run, every tree and
//!   plant of that land, crop by crop and crop-year by crop-year, as the
//!   world bears it before anything is picked (`World::bears_of_crop`, D45:
//!   lean, usual or rich year around it, its own share).
//! - **Day by day**: at each noon of the run, what is on the plants then
//!   (D48: each crop's curve, `crops::on_plant`), in kilojoules: on the
//!   whole land measured, and within each ring of each tribe's camp of that
//!   noon. Untouched: what men picked is not taken off (picks are stored by
//!   the crop's year, so a past day's "left" cannot be read after the run).
//!
//! It runs once, after the run, with the food map: the world is not
//! touched while it runs, and a run gives the same world with it as without
//! it.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use sim::World;
use sim::content::Content;
use sim::map::{CHUNK_SIZE, Pos};
use sim::time::Tick;

/// The rings around a tribe's camp, in tiles on the longer axis (a tile is
/// a metre): half a kilometre and a kilometre.
pub const RING_TILES: [u32; 2] = [500, 1000];

/// The land's food (its block of the data file).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LandFood {
    /// The chunks measured.
    pub chunks: u32,
    /// The crops, in the content's order: `"kind/material"`.
    pub crops: Vec<String>,
    /// Each crop's energy, kilojoules a kilogram (0: no food).
    pub kj_per_kg: Vec<u32>,
    /// Each crop's trees or plants on the land measured.
    pub plants: Vec<u64>,
    /// Each crop's grams on the land measured, untouched, crop-year by
    /// crop-year from the first.
    pub bears_g: Vec<Vec<u64>>,
    /// Each noon: the day (from 1), the kilojoules on the plants of the
    /// land measured, and per tribe within each ring of its camp (none when
    /// the tribe had no living).
    pub days: Vec<LandDay>,
    /// Each camp a tribe had at some noon, in the order first met, with
    /// each crop's grams within each ring of it, untouched, crop-year by
    /// crop-year: what a change of the crops' content would give near the
    /// camps, worked out without a run.
    pub camps: Vec<(Pos, Vec<[Vec<u64>; 2]>)>,
    /// What each tribe picked, season by season of the run: the season
    /// (from 0, the first spring), the tribe, and each crop's grams.
    pub picked_g: Vec<(u64, u8, Vec<u64>)>,
}

/// One noon of [`LandFood`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LandDay {
    /// The day, from 1.
    pub day: u64,
    /// On the whole land measured, in kilojoules.
    pub map_kj: u64,
    /// Per tribe: its camp that noon, and the kilojoules within each ring.
    pub near: Vec<Option<(Pos, [u64; 2])>>,
}

/// A chunk's centre tile.
fn centre(cx: u32, cy: u32) -> Pos {
    let half = CHUNK_SIZE / 2;
    Pos::new(
        i32::try_from(cx * CHUNK_SIZE + half).unwrap_or(i32::MAX),
        i32::try_from(cy * CHUNK_SIZE + half).unwrap_or(i32::MAX),
    )
}

/// The tiles between two tiles on the longer axis.
fn apart(a: Pos, b: Pos) -> u32 {
    (a.x - b.x).unsigned_abs().max((a.y - b.y).unsigned_abs())
}

/// The chunks within the wider ring of each camp, `(row, column)` as the
/// food map orders them.
#[must_use]
pub fn ring_chunks(world: &World, camps: &[(u64, Vec<Option<Pos>>)]) -> BTreeSet<(u32, u32)> {
    let map = world.map();
    let (columns, rows) = (
        map.width().div_ceil(CHUNK_SIZE),
        map.height().div_ceil(CHUNK_SIZE),
    );
    let mut seen: BTreeSet<(i32, i32)> = BTreeSet::new();
    let mut out = BTreeSet::new();
    let reach = RING_TILES[1];
    for camp in camps.iter().flat_map(|(_, c)| c.iter().flatten()) {
        if !seen.insert((camp.x, camp.y)) {
            continue;
        }
        let lo = |v: i32| u32::try_from(v).unwrap_or(0).saturating_sub(reach) / CHUNK_SIZE;
        let hi = |v: i32, n: u32| {
            ((u32::try_from(v).unwrap_or(0) + reach) / CHUNK_SIZE).min(n.saturating_sub(1))
        };
        for cy in lo(camp.y)..=hi(camp.y, rows) {
            for cx in lo(camp.x)..=hi(camp.x, columns) {
                if apart(centre(cx, cy), *camp) <= reach {
                    out.insert((cy, cx));
                }
            }
        }
    }
    out
}

/// A chunk's grams, untouched, by `(crop, crop-year)`.
type Grams = BTreeMap<(usize, u64), u64>;

/// The land's food as it is measured, chunk by chunk ([`Measure::add`]),
/// then day by day ([`Measure::finish`]).
#[derive(Debug, Clone, Default)]
pub struct Measure {
    /// The crops of each kind, by the content's order.
    of_kind: BTreeMap<String, Vec<usize>>,
    /// Crop-years counted.
    years: u64,
    /// Per chunk measured: its centre and its grams by `(crop, year)`.
    chunks: Vec<(Pos, Grams)>,
    /// What is measured on the whole land.
    out: LandFood,
}

impl Measure {
    /// A measure of `content`'s crops over `years` crop-years.
    #[must_use]
    pub fn new(content: &Content, years: u64) -> Self {
        let mut of_kind: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, crop) in content.crops.iter().enumerate() {
            of_kind.entry(crop.on.clone()).or_default().push(i);
        }
        let n = content.crops.len();
        let years = years.max(1);
        Self {
            of_kind,
            years,
            chunks: Vec::new(),
            out: LandFood {
                crops: content
                    .crops
                    .iter()
                    .map(|c| format!("{}/{}", c.on, c.gives))
                    .collect(),
                kj_per_kg: content
                    .crops
                    .iter()
                    .map(|c| {
                        content
                            .material(&c.gives)
                            .and_then(|m| m.food)
                            .map_or(0, |f| f.energy_kj_per_kg)
                    })
                    .collect(),
                plants: vec![0; n],
                bears_g: vec![vec![0; usize::try_from(years).unwrap_or(1)]; n],
                ..LandFood::default()
            },
        }
    }

    /// Counts the chunk at column `cx`, row `cy`, with its bearers.
    pub fn add(&mut self, world: &World, (cx, cy): (u32, u32), bearers: &[sim::crops::Bearer]) {
        let mut here = Grams::new();
        for bearer in bearers {
            let Some(crops) = self.of_kind.get(&*bearer.kind) else {
                continue;
            };
            for &crop in crops {
                self.out.plants[crop] += 1;
                for year in 1..=self.years {
                    let g = u64::from(world.bears_of_crop(bearer, crop, year).unwrap_or(0));
                    *here.entry((crop, year)).or_insert(0) += g;
                    self.out.bears_g[crop][usize::try_from(year - 1).unwrap_or(0)] += g;
                }
            }
        }
        self.out.chunks += 1;
        self.chunks.push((centre(cx, cy), here));
    }

    /// The days' noons, with each tribe's camp at each: what is on the
    /// plants then, on the whole land and near each camp.
    #[must_use]
    pub fn finish(
        mut self,
        world: &World,
        content: &Content,
        camps: &[(u64, Vec<Option<Pos>>)],
        picks: &BTreeMap<(u64, u8, String), u64>,
    ) -> LandFood {
        // What was picked, by the crop giving it (a material is given by
        // one kind in the content, else by the first that gives it).
        for (&(season, tribe, ref gives), &grams) in picks {
            let Some(crop) = content.crops.iter().position(|c| c.gives == *gives) else {
                continue;
            };
            match self.out.picked_g.last_mut() {
                Some((s, t, row)) if *s == season && *t == tribe => row[crop] += grams,
                _ => {
                    let mut row = vec![0; self.out.crops.len()];
                    row[crop] = grams;
                    self.out.picked_g.push((season, tribe, row));
                }
            }
        }
        let per_day = world.calendar().ticks_per_day();
        let kj = |crop: usize, grams: u64, share: u64| {
            u128::from(grams) * u128::from(share) * u128::from(self.out.kj_per_kg[crop])
                / (u128::from(sim::crops::WHOLE) * 1000)
        };
        for (day, tribes) in camps {
            let tick = Tick(day.saturating_sub(1) * per_day + per_day / 2);
            // Each crop on its plant at that noon: its year and share.
            let on: Vec<Option<(u64, u64)>> = content
                .crops
                .iter()
                .map(|c| {
                    sim::crops::on_plant(c, world.calendar(), tick)
                        .filter(|&(y, share)| y >= 1 && y <= self.years && share > 0)
                })
                .collect();
            let mut map_kj: u128 = 0;
            for (crop, o) in on.iter().enumerate() {
                if let Some((year, share)) = *o {
                    let g = self.out.bears_g[crop][usize::try_from(year - 1).unwrap_or(0)];
                    map_kj += kj(crop, g, share);
                }
            }
            let near = tribes
                .iter()
                .map(|camp| {
                    let camp = (*camp)?;
                    let mut rings = [0_u128; 2];
                    for (at, grams) in &self.chunks {
                        let d = apart(*at, camp);
                        if d > RING_TILES[1] {
                            continue;
                        }
                        let mut sum = 0;
                        for (&(crop, year), &g) in grams {
                            if on[crop].is_some_and(|(y, _)| y == year) {
                                sum += kj(crop, g, on[crop].map_or(0, |o| o.1));
                            }
                        }
                        rings[1] += sum;
                        if d <= RING_TILES[0] {
                            rings[0] += sum;
                        }
                    }
                    let fit = |v: u128| u64::try_from(v).unwrap_or(u64::MAX);
                    Some((camp, [fit(rings[0]), fit(rings[1])]))
                })
                .collect();
            self.out.days.push(LandDay {
                day: *day,
                map_kj: u64::try_from(map_kj).unwrap_or(u64::MAX),
                near,
            });
        }
        // Each camp met, with each crop's grams within each ring.
        let years = usize::try_from(self.years).unwrap_or(1);
        for camp in camps.iter().flat_map(|(_, c)| c.iter().flatten()) {
            if self.out.camps.iter().any(|(at, _)| at == camp) {
                continue;
            }
            let mut by_crop = vec![[vec![0; years], vec![0; years]]; self.out.crops.len()];
            for (at, grams) in &self.chunks {
                let d = apart(*at, *camp);
                for (&(crop, year), &g) in grams {
                    let y = usize::try_from(year - 1).unwrap_or(0);
                    for (ring, &reach) in RING_TILES.iter().enumerate() {
                        if d <= reach {
                            by_crop[crop][ring][y] += g;
                        }
                    }
                }
            }
            self.out.camps.push((*camp, by_crop));
        }
        self.out
    }
}

impl LandFood {
    /// The block of the data file.
    #[must_use]
    pub fn json(&self) -> Value {
        json!({
            "ring_tiles": RING_TILES,
            "chunks": self.chunks,
            "crops": self.crops,
            "kj_per_kg": self.kj_per_kg,
            "plants": self.plants,
            "bears_g": self.bears_g,
            "days": self.days.iter().map(|d| json!([
                d.day,
                d.map_kj,
                d.near.iter().map(|n| n.map(|(camp, rings)| json!([[camp.x, camp.y], rings])))
                    .collect::<Vec<_>>(),
            ])).collect::<Vec<_>>(),
            "picked_g": self.picked_g.iter().map(|(season, tribe, row)| json!([season, tribe, row]))
                .collect::<Vec<_>>(),
            "camps": self.camps.iter().map(|(at, by_crop)| json!({
                "at": [at.x, at.y],
                "bears_g": by_crop,
            })).collect::<Vec<_>>(),
        })
    }
}
