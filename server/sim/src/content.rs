//! World content: what things are made of, and where they lie (D15, D27).
//!
//! Content is data, never code: materials with properties, and spawn
//! rules that scatter them over the ground. Qualities of objects will be
//! computed from these properties, never from hard-coded recipes. The
//! server reads the RON files and hands their text to [`Content::from_ron`];
//! this crate never touches a file.
//!
//! Values in the public repository are placeholders, tuned by play; hidden
//! rules live in a private repository (D30).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::map::Terrain;
use crate::time::Season;
use crate::woods::Habitat;

/// What family a material belongs to. Guides what can be done with it;
/// the properties decide how well.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialKind {
    /// Stone: knapped, hammered, ground.
    Stone,
    /// Wood: cut, split, burnt.
    Wood,
    /// Plant matter: fibres, leaves, berries.
    Plant,
    /// From an animal: hide, bone, meat.
    Animal,
}

/// What eating a material gives, when it is food.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Food {
    /// Energy of a kilogram, in kilojoules: the one scale all content
    /// shares (D15), so an item gives what its grams are worth.
    pub energy_kj_per_kg: u32,
    /// Days before it spoils, untreated (D16: computed on demand).
    pub keeps_days: u32,
}

impl Food {
    /// Kilojoules in `grams` of it, rounded down.
    #[must_use]
    pub fn energy_kj(&self, grams: u32) -> u32 {
        let kj = u64::from(grams) * u64::from(self.energy_kj_per_kg) / 1000;
        u32::try_from(kj).unwrap_or(u32::MAX)
    }
}

/// How a material looks, for the client. Placeholder graphics: a colour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Look {
    /// `#rrggbb`.
    pub colour: String,
}

impl Material {
    /// Mass of one piece, in grams.
    // Not generation: content gives masses in kilograms, read once.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::float_arithmetic,
        reason = "values bounded by the world size and the content ranges; values checked non-negative by the surrounding rule; float maths of the simulation, the same result on every run"
    )]
    pub fn mass_g(&self) -> u32 {
        (self.mass_kg * 1000.0).round().clamp(0.0, 1e9) as u32
    }
}

/// A material: the stuff pieces are made of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    /// Content id, `snake_case`, unique.
    pub id: String,
    /// Name shown to players.
    pub name: String,
    /// One sentence shown to players, what it is and what it is for.
    pub description: String,
    /// Family.
    pub kind: MaterialKind,
    /// Mass of one piece as the ground drops it or a body dies into it,
    /// in kilograms; an item then keeps its own grams (D35).
    pub mass_kg: f32,
    /// Hardness, 0 (soft) to 10 (hardest).
    pub hardness: u8,
    /// Does it burn?
    pub flammable: bool,
    /// Can it be eaten?
    pub food: Option<Food>,
    /// How it looks.
    pub look: Look,
}

/// A closeness condition: within so many tiles of some terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Near {
    /// The terrain to be close to.
    pub terrain: Terrain,
    /// How many tiles away at most (eight-way steps).
    pub within: u32,
}

/// Where a material lies on the ground when the world is created: on a
/// terrain, at a density, and perhaps only near another terrain — flint
/// at the foot of the cliff, berries at the forest's edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spawn {
    /// The material.
    pub material: String,
    /// On which terrain.
    pub terrain: Terrain,
    /// Pieces per thousand tiles of that terrain (that satisfy `near`).
    pub per_thousand: u32,
    /// Only near this terrain, if given.
    #[serde(default)]
    pub near: Option<Near>,
    /// Grams of the piece it drops: its material's mass, filled when the
    /// content is read ([`Content::from_ron`]), never written in the file.
    #[serde(skip)]
    pub piece_g: u32,
}

/// How bodies move over the ground (D32).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Movement {
    /// Walking speed on the reference terrain (cost 10), in centimetres
    /// per game second.
    pub walk_speed_cm_per_s: u32,
    /// Cost of crossing one tile of each terrain, 10 being the reference;
    /// `None` cannot be walked. Paths weigh tiles by it and bodies slow
    /// down by it.
    pub terrain_costs: BTreeMap<Terrain, Option<u32>>,
    /// What a body can carry, in grams (D35).
    pub carry_capacity_g: u32,
    /// Walking speed at full load, in percent of the unloaded speed.
    pub speed_percent_at_full_load: u32,
    /// How far a body sees, by the terrain it stands on, in tiles on each
    /// axis: a path is searched within that square and no farther (D32).
    pub sight_tiles: BTreeMap<Terrain, u32>,
    /// Tiles a path search may expand at most; past it, the body walks
    /// straight (D32).
    pub path_budget_tiles: u32,
    /// Tiles the path searches of one tick may expand in all; the rest
    /// wait for the next tick, in their lane, in the order they came. The
    /// search at the head of a lane runs even past it, the next tick
    /// repaying the excess (D32).
    pub search_work_per_tick: u32,
    /// The share of that work kept for bodies with a pilot (players), in
    /// percent; the rest goes to the others (NPCs, animals). A lane's
    /// unused share goes to the other (D32).
    pub player_share_percent: u32,
}

impl Movement {
    /// Cost of a tile of this terrain, `None` when it cannot be walked.
    pub fn cost(&self, terrain: Terrain) -> Option<u32> {
        self.terrain_costs.get(&terrain).copied().flatten()
    }

    /// Walking speed on this terrain, in centimetres per game second;
    /// zero when it cannot be walked.
    pub fn speed_cm_per_s(&self, terrain: Terrain) -> u32 {
        self.cost(terrain)
            .map_or(0, |cost| self.walk_speed_cm_per_s * 10 / cost)
    }

    /// How far a body standing on this terrain sees, in tiles on each
    /// axis; the shortest sight there is for a terrain the content does
    /// not name.
    pub fn sight_tiles(&self, terrain: Terrain) -> u32 {
        self.sight_tiles
            .get(&terrain)
            .copied()
            .unwrap_or_else(|| self.sight_tiles.values().copied().min().unwrap_or(1))
    }

    /// The slowest walking speed on any terrain that can be walked, in
    /// centimetres per game second.
    pub fn slowest_speed_cm_per_s(&self) -> u32 {
        Terrain::ALL
            .iter()
            .map(|&t| self.speed_cm_per_s(t))
            .filter(|&speed| speed > 0)
            .min()
            .unwrap_or(self.walk_speed_cm_per_s)
    }
}

/// 100 in every season: a rate as it is.
fn every_season_100() -> BySeason<u32> {
    BySeason {
        spring: 100,
        summer: 100,
        autumn: 100,
        winter: 100,
    }
}

/// 100: a rate as it is.
fn percent_100() -> u32 {
    100
}

fn reserve_kj_60000() -> u32 {
    60_000
}

fn percent_150() -> u32 {
    150
}

fn percent_50() -> u32 {
    50
}

fn percent_20() -> u32 {
    20
}

fn percent_70() -> u32 {
    70
}

/// A value for each season (D09).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BySeason<T> {
    /// In spring.
    pub spring: T,
    /// In summer.
    pub summer: T,
    /// In autumn.
    pub autumn: T,
    /// In winter.
    pub winter: T,
}

impl<T: Copy> BySeason<T> {
    /// The value for `season`.
    pub fn get(&self, season: Season) -> T {
        match season {
            Season::Spring => self.spring,
            Season::Summer => self.summer,
            Season::Autumn => self.autumn,
            Season::Winter => self.winter,
        }
    }
}

/// How a body's needs and vitality evolve (D34). Every rate is in points
/// of a 0–100 gauge per game day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Needs {
    /// Where night begins, in percent of the day, by season: the day
    /// starts at dawn, and summer's light lasts longer (D09).
    pub night_from_percent: BySeason<u32>,
    /// Hunger rises this much per day; with `kj_per_hunger_point`, what
    /// a body burns a day, in kilojoules: its need.
    pub hunger_per_day: u32,
    /// Thirst rises this much per day (D34, amended 13 November 2026).
    pub thirst_per_day: u32,
    /// Thirst's rate by season, in percent of `thirst_per_day`: faster in
    /// the heat (D45). All 100 when absent.
    #[serde(default = "every_season_100")]
    pub thirst_heat_percent: BySeason<u32>,
    /// Thirst's rate while walking or working, in percent of the rest's.
    /// 100 when absent.
    #[serde(default = "percent_100")]
    pub thirst_effort_percent: u32,
    /// Cold rises this much per day of night, without warmth.
    pub cold_per_day_at_night: u32,
    /// Cold falls this much per day of daylight.
    pub cold_relief_per_day: u32,
    /// Fatigue rises this much per day awake.
    pub fatigue_per_day_awake: u32,
    /// Fatigue falls this much per day asleep.
    pub rest_per_day_asleep: u32,
    /// Vitality lost per day once the fat reserve is spent.
    pub starving_per_day: u32,
    /// Vitality lost per day while cold is at its maximum.
    pub freezing_per_day: u32,
    /// Vitality lost per day while thirst is at its maximum.
    pub dehydration_per_day: u32,
    /// Vitality regained per day when fed, warm enough and rested.
    pub vitality_regain_per_day: u32,
    /// Kilojoules of food that relieve one point of hunger.
    pub kj_per_hunger_point: u32,
    /// Grams a body eats at most in one act: a meal (D35); the rest of
    /// the item stays.
    pub meal_g: u32,
    /// The fat reserve of a body, full, in kilojoules (lot 3): what it
    /// burns comes out of it, what it eats goes back in. 60,000 when
    /// absent.
    #[serde(default = "reserve_kj_60000")]
    pub reserve_kj: u32,
    /// What a body can eat in a day, in percent of its need: a reserve
    /// is not made again in one meal. 150 when absent.
    #[serde(default = "percent_150")]
    pub intake_ceiling_percent: u32,
    /// Under this share of its reserve, in percent, a body is thin. 50
    /// when absent.
    #[serde(default = "percent_50")]
    pub thin_percent: u32,
    /// Under this share, it is weak: it walks and works slower. 20 when
    /// absent.
    #[serde(default = "percent_20")]
    pub weak_percent: u32,
    /// A weak body's pace, walking and working, in percent of its own.
    /// 70 when absent.
    #[serde(default = "percent_70")]
    pub weak_pace_percent: u32,
    /// How needs, pace and strength go with age and sex, and the carried
    /// infant (lot 3 B). Absent: every body as a grown man.
    #[serde(default)]
    pub ages: Ages,
    /// Births, background deaths and kinship (lot 3 C, D49). Absent: a
    /// world where nobody is born, couples or dies of anything but want;
    /// and even present, it plays only in a world whose `life` is on.
    #[serde(default)]
    pub life: Life,
    /// The trials, selection and imitation of the brains (D50). Absent:
    /// nobody tries anything nor copies anybody.
    #[serde(default)]
    pub trials: Trials,
    /// The rules of thought that keep a mind light (D53). Absent: the
    /// placeholders of `content/needs.ron`.
    #[serde(default)]
    pub thought: Thought,
}

/// The rules of thought that make a mind light (D53, decided by the
/// project lead on 22 November 2026): perceptions and gestures, never an
/// outcome. Placeholders (D30).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thought {
    /// How far, in tiles on each axis, a body sees the trees and plants
    /// bearing food (its sight for water and for others is the terrain's,
    /// `movement.ron`): a bush is told from the grass closer than a lake
    /// or a walker.
    pub plant_sight_tiles: u32,
}

impl Default for Thought {
    /// The placeholders of `content/needs.ron`.
    fn default() -> Self {
        Self {
            plant_sight_tiles: 30,
        }
    }
}

impl Thought {
    fn validate(&self) -> Result<(), ContentError> {
        if self.plant_sight_tiles == 0 {
            return Err(ContentError::Needs(
                "thought: plant_sight_tiles is 1 tile or more",
            ));
        }
        Ok(())
    }
}

/// How the brains try, judge and copy their ways (D50, decided 22 November
/// 2026). Every value is a D30 placeholder, tuned by reading the
/// vivarium's page, never to remove the difficulty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trials {
    /// Days of distress in a row (hungry at the day's end, or a day with
    /// no gather at all) before a body may start a trial.
    pub trial_after_days: u32,
    /// How many days a trial lasts, and how many days before it the
    /// judgement compares them with; also the days imitation looks back on.
    pub trial_days: u32,
    /// The daily chance to start a trial, in percent, by will: (will,
    /// percent), the wills rising, straight lines between the points, the
    /// first held before and the last after.
    pub try_percent_by_will: Vec<(u32, u32)>,
    /// From this will, a trial of the home may set it a whole
    /// `new_home_walk_tiles` away; under it, a quarter of that.
    pub far_home_from_will: u32,
    /// How far a trial moves the home, in tiles, towards where the body
    /// knows the most plants still bearing.
    pub new_home_walk_tiles: u32,
    /// At night at the camp, a body copies only from a camp-mate within
    /// this many tiles.
    pub imitate_tiles: u32,
    /// Its chance to copy one way a night, in percent, scaled by the
    /// mate's will (will 100: the whole percent; will 50: half).
    pub imitate_percent: u32,
    /// The chance, in percent, that a copied numeric way loses a step: the
    /// copy lands halfway between its own value and the mate's (a search
    /// twice as wide copied becomes one and a half times as wide).
    pub imitate_loss_percent: u32,
}

impl Default for Trials {
    /// Nobody tries anything nor copies anybody.
    fn default() -> Self {
        Self {
            trial_after_days: 0,
            trial_days: 1,
            try_percent_by_will: vec![(0, 0)],
            far_home_from_will: 100,
            new_home_walk_tiles: 0,
            imitate_tiles: 0,
            imitate_percent: 0,
            imitate_loss_percent: 0,
        }
    }
}

impl Trials {
    /// The daily chance, in percent, that a body of `will` starts a trial
    /// once in distress long enough.
    #[must_use]
    pub fn try_percent(&self, will: u8) -> u64 {
        along(&self.try_percent_by_will, u64::from(will) * 1000, 1)
    }

    /// Whether anybody ever tries or copies anything.
    #[must_use]
    pub fn on(&self) -> bool {
        self.try_percent_by_will.iter().any(|p| p.1 > 0) || self.imitate_percent > 0
    }

    fn validate(&self) -> Result<(), ContentError> {
        let rising = self.try_percent_by_will.windows(2).all(|p| p[0].0 < p[1].0);
        let percent = |v: u32| v <= 100;
        if self.try_percent_by_will.is_empty()
            || !rising
            || self.try_percent_by_will.iter().any(|p| !percent(p.1))
            || !percent(self.far_home_from_will)
            || !percent(self.imitate_percent)
            || !percent(self.imitate_loss_percent)
            || self.trial_days == 0
        {
            return Err(ContentError::Needs(
                "trials: try_percent_by_will not empty, its wills rising and its percents at \
                 most 100; far_home_from_will, imitate_percent and imitate_loss_percent at \
                 most 100; trial_days positive",
            ));
        }
        Ok(())
    }
}

/// Ages and sexes (lot 3 B, decided 17 November 2026): what a body needs
/// against a grown man, how fast it goes and what it carries, and the
/// infant carried and nursed until it is weaned. Ages in game years
/// ([`crate::time::Calendar::days_per_year`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ages {
    /// A body's need, its reserve and its strength by age, in percent of
    /// a grown man's: (years, percent), the ages rising, straight lines
    /// between them, the last held after.
    pub need_by_age: Vec<(u32, u32)>,
    /// A woman's, in percent of a man's of her age, from `woman_from_years`.
    pub woman_percent: u32,
    /// The age from which a woman needs `woman_percent`.
    pub woman_from_years: u32,
    /// A woman nursing an infant needs this much of her own need, in
    /// percent; the infant then burns nothing of its own.
    pub nursing_percent: u32,
    /// The age an infant is weaned at: until then it is carried, does not
    /// walk, and lives on its carrier's milk.
    pub weaned_years: u32,
    /// Children under this age and the old over `slow_over_years` walk
    /// and gather at `slow_pace_percent` of a grown body's pace.
    pub slow_under_years: u32,
    /// See `slow_under_years`.
    pub slow_over_years: u32,
    /// See `slow_under_years`.
    pub slow_pace_percent: u32,
    /// An infant's mass on its carrier, in grams: (at birth, weaned),
    /// straight between.
    pub infant_mass_g: (u32, u32),
    /// How far an orphaned infant is taken by the nearest woman, in tiles;
    /// with none that near, it is left where its carrier died.
    pub adopt_within_tiles: u32,
}

impl Default for Ages {
    fn default() -> Self {
        Self {
            need_by_age: vec![(0, 100)],
            woman_percent: 100,
            woman_from_years: 0,
            nursing_percent: 100,
            weaned_years: 0,
            slow_under_years: 0,
            slow_over_years: u32::MAX,
            slow_pace_percent: 100,
            infant_mass_g: (0, 0),
            adopt_within_tiles: 0,
        }
    }
}

/// A body's sex (lot 3 B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Sex {
    /// A woman or a girl.
    Female,
    /// A man or a boy.
    Male,
}

/// Births, background deaths and kinship (lot 3 C, decided 21 November
/// 2026; D49). Every value is a D30 placeholder. Ages in game years; what
/// is counted in years follows the season, what is counted in days
/// follows the day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Life {
    /// A woman conceives from the first age to the last, both included.
    pub fertile_years: (u32, u32),
    /// She conceives only with more than this share of her fat reserve, in
    /// percent: a thin woman does not.
    pub conceive_reserve_percent: u32,
    /// Her spouse must be alive within this many tiles; a body takes a
    /// spouse within as many.
    pub mate_within_tiles: u32,
    /// Her chance to conceive at the start of a season, in percent.
    pub conceive_percent: u32,
    /// A pregnancy lasts this many seasons (a quarter of the year each).
    pub pregnancy_seasons: u32,
    /// A pregnant woman needs this much of her own need, in percent.
    pub pregnant_percent: u32,
    /// A weak woman (under `Needs::weak_percent`) loses the child.
    pub lose_when_weak: bool,
    /// The share of boys among the newborn, in percent.
    pub boys_percent: u32,
    /// The mother's chance to die giving birth, in percent.
    pub childbirth_death_percent: u32,
    /// The chance to die in a year by age, in thousandths: (years, per
    /// mille), the ages rising, straight lines between them, the last
    /// held after. Drawn every day with the year's chance divided by its
    /// days.
    pub death_by_age: Vec<(u32, u32)>,
    /// From this age a background death is of old age; before, of illness.
    pub old_age_from_years: u32,
    /// A body takes a spouse from the first age to the last, both
    /// included, and is taken as one between them.
    pub mate_years: (u32, u32),
}

impl Default for Life {
    /// Nobody conceives, couples or dies of age or illness.
    fn default() -> Self {
        Self {
            fertile_years: (0, 0),
            conceive_reserve_percent: 100,
            mate_within_tiles: 0,
            conceive_percent: 0,
            pregnancy_seasons: 1,
            pregnant_percent: 100,
            lose_when_weak: false,
            boys_percent: 50,
            childbirth_death_percent: 0,
            death_by_age: vec![(0, 0)],
            old_age_from_years: 0,
            mate_years: (0, 0),
        }
    }
}

impl Life {
    /// The chance to die within a year at `age_milli` thousandths of a
    /// year, in thousandths.
    #[must_use]
    pub fn death_permille_per_year(&self, age_milli: u64) -> u64 {
        along(&self.death_by_age, age_milli, 1)
    }

    /// The chance to die today, in millionths: the year's, shared out
    /// over its days, so a longer year does not kill more a year.
    #[must_use]
    pub fn death_ppm_per_day(&self, age_milli: u64, days_per_year: u32) -> u64 {
        self.death_permille_per_year(age_milli) * 1000 / u64::from(days_per_year.max(1))
    }

    /// Whether a woman that old conceives.
    #[must_use]
    pub fn is_fertile(&self, age_milli: u64) -> bool {
        let years = age_milli / 1000;
        (u64::from(self.fertile_years.0)..=u64::from(self.fertile_years.1)).contains(&years)
    }

    /// Whether a body that old takes, and is taken as, a spouse.
    #[must_use]
    pub fn mates(&self, age_milli: u64) -> bool {
        let years = age_milli / 1000;
        (u64::from(self.mate_years.0)..=u64::from(self.mate_years.1)).contains(&years)
    }

    fn validate(&self) -> Result<(), ContentError> {
        let rising = self.death_by_age.windows(2).all(|p| p[0].0 < p[1].0);
        let percent = |v: u32| v <= 100;
        if self.death_by_age.is_empty()
            || !rising
            || self.death_by_age.iter().any(|p| p.1 > 1000)
            || self.fertile_years.0 > self.fertile_years.1
            || self.mate_years.0 > self.mate_years.1
            || !percent(self.conceive_reserve_percent)
            || !percent(self.conceive_percent)
            || !percent(self.boys_percent)
            || !percent(self.childbirth_death_percent)
            || self.pregnancy_seasons == 0
            || self.pregnant_percent < 100
        {
            return Err(ContentError::Needs(
                "life: death_by_age not empty, its ages rising and its per mille at most 1000; \
                 fertile_years and mate_years rising; the percents at most 100; \
                 pregnancy_seasons positive, pregnant_percent at least 100",
            ));
        }
        Ok(())
    }
}

/// The value at `age_milli` thousandths of a year along `points`: (years,
/// value), the ages rising, each value scaled by `scale`, straight lines
/// between the points, the first held before and the last after.
fn along(points: &[(u32, u32)], age_milli: u64, scale: u64) -> u64 {
    let milli = |(years, value): (u32, u32)| (u64::from(years) * 1000, u64::from(value) * scale);
    let points: Vec<(u64, u64)> = points.iter().copied().map(milli).collect();
    let mut at = points.last().map_or(0, |p| p.1);
    if let Some(first) = points.first()
        && age_milli <= first.0
    {
        at = first.1;
    }
    for pair in points.windows(2) {
        let ((a, va), (b, vb)) = (pair[0], pair[1]);
        if (a..b).contains(&age_milli) {
            at = if vb >= va {
                va + (vb - va) * (age_milli - a) / (b - a)
            } else {
                va - (va - vb) * (age_milli - a) / (b - a)
            };
            break;
        }
    }
    at
}

impl Ages {
    /// The need, reserve and strength of a body `age_milli` thousandths of
    /// a year old, of `sex`, in thousandths of a grown man's.
    #[must_use]
    pub fn size_permille(&self, age_milli: u64, sex: Sex) -> u32 {
        let mut size = if self.need_by_age.is_empty() {
            1000
        } else {
            along(&self.need_by_age, age_milli, 10)
        };
        if sex == Sex::Female && age_milli >= u64::from(self.woman_from_years) * 1000 {
            size = size * u64::from(self.woman_percent) / 100;
        }
        u32::try_from(size).unwrap_or(u32::MAX)
    }

    /// Whether a body that old is still an infant, carried.
    #[must_use]
    pub fn is_infant(&self, age_milli: u64) -> bool {
        age_milli < u64::from(self.weaned_years) * 1000
    }

    /// Its pace from its age, in percent.
    #[must_use]
    pub fn age_pace_percent(&self, age_milli: u64) -> u32 {
        let years = age_milli / 1000;
        if years < u64::from(self.slow_under_years) || years >= u64::from(self.slow_over_years) {
            self.slow_pace_percent
        } else {
            100
        }
    }

    /// An infant's mass at that age, in grams.
    #[must_use]
    pub fn infant_mass_g(&self, age_milli: u64) -> u32 {
        let (birth, weaned) = (
            u64::from(self.infant_mass_g.0),
            u64::from(self.infant_mass_g.1),
        );
        let span = u64::from(self.weaned_years) * 1000;
        if span == 0 {
            return self.infant_mass_g.1;
        }
        let at = age_milli.min(span);
        u32::try_from(birth + weaned.saturating_sub(birth) * at / span).unwrap_or(u32::MAX)
    }

    fn validate(&self) -> Result<(), ContentError> {
        let rising = self.need_by_age.windows(2).all(|p| p[0].0 < p[1].0);
        if self.need_by_age.is_empty()
            || !rising
            || self.need_by_age.iter().any(|p| p.1 == 0)
            || self.woman_percent == 0
            || self.nursing_percent < 100
            || !(1..=100).contains(&self.slow_pace_percent)
            || self.infant_mass_g.0 > self.infant_mass_g.1
        {
            return Err(ContentError::Needs(
                "ages: need_by_age not empty, its ages rising and its percents positive; \
                 woman_percent positive, nursing_percent at least 100, slow_pace_percent 1 to 100, \
                 infant_mass_g rising",
            ));
        }
        Ok(())
    }
}

impl Needs {
    /// Checks the fat reserve's values (lot 3).
    fn validate_reserve(&self) -> Result<(), ContentError> {
        if self.reserve_kj == 0
            || self.intake_ceiling_percent < 100
            || self.weak_percent > self.thin_percent
            || self.thin_percent > 100
            || !(1..=100).contains(&self.weak_pace_percent)
        {
            return Err(ContentError::Needs(
                "reserve_kj is positive, intake_ceiling_percent at least 100, \
                 weak_percent at most thin_percent at most 100, weak_pace_percent 1 to 100",
            ));
        }
        Ok(())
    }

    /// What a body burns a day, in joules.
    #[must_use]
    pub fn need_j_per_day(&self) -> u64 {
        u64::from(self.hunger_per_day) * u64::from(self.kj_per_hunger_point) * 1000
    }

    /// The full reserve, in joules.
    #[must_use]
    pub fn reserve_j(&self) -> u64 {
        u64::from(self.reserve_kj) * 1000
    }
}

/// Everything loaded from the content files, validated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Content {
    /// Materials, in file order.
    pub materials: Vec<Material>,
    /// Spawn rules, in file order.
    pub spawns: Vec<Spawn>,
    /// How bodies move.
    pub movement: Movement,
    /// How bodies hunger, freeze, tire and die.
    pub needs: Needs,
    /// The kinds of tree and where each grows (D37), in file order.
    pub species: Vec<Species>,
    /// The shrubs, plants and mushrooms, and the trees still to come
    /// (`flora.ron`), in file order by section.
    #[serde(default)]
    pub flora: Vec<crate::flora::Kind>,
    /// The rock regions under the ground (`geology.ron`, D42): drawn on
    /// worlds of the generator's version 2 on.
    #[serde(default)]
    pub geology: crate::geology::Geology,
    /// The trees of the open biomes' ground (`open_ground.ron`, D37):
    /// alone and in groves on the atlas's grassland, steppe and marsh.
    #[serde(default)]
    pub open_ground: crate::open_ground::OpenGround,
    /// What the trees and plants bear, when, and how much (`crops.ron`,
    /// D45), in file order.
    #[serde(default)]
    pub crops: Vec<crate::crops::Crop>,
}

/// A kind of tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Species {
    /// Content id, `snake_case`, unique.
    pub id: String,
    /// Name shown to players.
    pub name: String,
    /// Height of a grown tree, in metres.
    pub height_m: f32,
    /// How much this species grows in each habitat, against the others
    /// there; habitats not listed have none of it.
    pub habitats: BTreeMap<Habitat, u32>,
    /// A big tree, standing on the woods' wide grid (8 m) with nothing
    /// small right under its crown; the others stand on the narrow one
    /// (4 m), between the big ones (D40).
    #[serde(default)]
    pub big: bool,
    /// How far its seeds fall from the tree that bears them, in metres
    /// (4 to [`MOST_SPREAD_M`]): heavy seeds (acorns, hazelnuts) close by,
    /// in tight groups; light ones on the wind (birch, pine) farther, in
    /// loose ones (D37).
    #[serde(default = "Species::default_spread")]
    pub spread_m: u32,
    /// A pioneer: the first to grow back in the openings of the woods.
    #[serde(default)]
    pub pioneer: bool,
    /// When its fruit shows on the tree (D48), if it shows any: acorns,
    /// nuts, cones. Out of it, or without it, the tree is drawn bare.
    #[serde(default)]
    pub fruit: Option<crate::crops::FruitWindow>,
}

/// Farthest a species' seeds may fall ([`Species::spread_m`]): the trees
/// look for the trees that seeded them this far around, no farther.
pub const MOST_SPREAD_M: u32 = 36;

impl Species {
    fn default_spread() -> u32 {
        20
    }
}

/// Gauge points a day, percents past the ones already bounded, grams of
/// a meal, kilojoules of a point: a thousand times today's values.
const MILLION: u64 = 1_000_000;
/// Weights and small percents, where only the ratio counts.
const WEIGHT: u64 = 10_000;
/// Tiles of sight, within, reach, spread; years of an age; metres.
const THOUSAND: u64 = 1000;

/// [`Content::validate_bounds`]: `value` at most `max`, else the refusal
/// naming the file and the field.
fn at_most(
    file: &'static str,
    field: impl Into<String>,
    value: impl Into<u64>,
    max: u64,
) -> Result<(), ContentError> {
    if value.into() > max {
        return Err(ContentError::TooLarge {
            file,
            field: field.into(),
            max,
        });
    }
    Ok(())
}

/// [`at_most`] for a length in metres: finite, a kilometre at most.
fn metres(file: &'static str, field: impl Into<String>, value: f32) -> Result<(), ContentError> {
    if !(value.is_finite() && value <= 1000.0) {
        return Err(ContentError::TooLarge {
            file,
            field: field.into(),
            max: THOUSAND,
        });
    }
    Ok(())
}

/// Why content was refused.
#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    /// A file does not parse.
    #[error("{file}: {source}")]
    Parse {
        /// Which file.
        file: &'static str,
        /// The parser's error, with its position.
        #[source]
        source: ron::error::SpannedError,
    },
    /// Two materials share an id.
    #[error("material `{0}` is defined twice")]
    DuplicateMaterial(String),
    /// A rule names a material that does not exist.
    #[error("spawn rule {index} names unknown material `{material}`")]
    UnknownMaterial {
        /// Position of the rule in the file, from 0.
        index: usize,
        /// The id it used.
        material: String,
    },
    /// A value is outside what makes sense.
    #[error("material `{id}`: {what}")]
    InvalidValue {
        /// Which material.
        id: String,
        /// What is wrong.
        what: &'static str,
    },
    /// A spawn density above one piece per tile.
    #[error("spawn rule {index}: per_thousand {per_thousand} is above 1000")]
    TooDense {
        /// Position of the rule in the file, from 0.
        index: usize,
        /// The value.
        per_thousand: u32,
    },
    /// The rock regions make no sense.
    #[error("geology: {0}")]
    Geology(&'static str),
    /// The open ground's trees make no sense.
    #[error("open ground: {0}")]
    OpenGround(&'static str),
    /// A crop names a kind or a material that does not exist, or makes
    /// no sense.
    #[error("crop `{gives}` on `{on}`: {what}")]
    Crop {
        /// The kind bearing it.
        on: String,
        /// The material it gives.
        gives: String,
        /// What is wrong.
        what: &'static str,
    },
    /// A kind of tree is defined twice or makes no sense.
    #[error("species `{id}`: {what}")]
    Species {
        /// Which species.
        id: String,
        /// What is wrong.
        what: &'static str,
    },
    /// A value is past what the simulation can carry in its arithmetic
    /// (a rate, a mass, a budget): the file and the field are named.
    #[error("{file}: {field} is at most {max}")]
    TooLarge {
        /// The file.
        file: &'static str,
        /// The field, with the entry's id where there are several.
        field: String,
        /// The ceiling.
        max: u64,
    },
    /// The movement rules are incomplete or senseless.
    #[error("movement: {0}")]
    Movement(&'static str),
    /// The needs rules are senseless.
    #[error("needs: {0}")]
    Needs(&'static str),
    /// A spawn rule is senseless.
    #[error("spawn rule {index}: {what}")]
    Spawn {
        /// Position of the rule in the file, from 0.
        index: usize,
        /// What is wrong.
        what: &'static str,
    },
}

impl Content {
    /// Parses the content files and validates them together.
    pub fn from_ron(
        materials: &str,
        spawns: &str,
        movement: &str,
        needs: &str,
    ) -> Result<Self, ContentError> {
        let materials = ron::from_str(materials).map_err(|source| ContentError::Parse {
            file: "materials.ron",
            source,
        })?;
        let spawns = ron::from_str(spawns).map_err(|source| ContentError::Parse {
            file: "spawns.ron",
            source,
        })?;
        let movement = ron::from_str(movement).map_err(|source| ContentError::Parse {
            file: "movement.ron",
            source,
        })?;
        let needs = ron::from_str(needs).map_err(|source| ContentError::Parse {
            file: "needs.ron",
            source,
        })?;
        let mut content = Self {
            materials,
            spawns,
            movement,
            needs,
            species: Vec::new(),
            flora: Vec::new(),
            geology: crate::geology::Geology::default(),
            open_ground: crate::open_ground::OpenGround::default(),
            crops: Vec::new(),
        };
        content.validate()?;
        content.validate_bounds()?;
        for index in 0..content.spawns.len() {
            content.spawns[index].piece_g = content
                .material(&content.spawns[index].material)
                .map_or(0, Material::mass_g);
        }
        Ok(content)
    }

    /// The same content with the kinds of tree of `species` (the text of
    /// `species.ron`).
    pub fn with_species(mut self, species: &str) -> Result<Self, ContentError> {
        self.species = ron::from_str(species).map_err(|source| ContentError::Parse {
            file: "species.ron",
            source,
        })?;
        self.validate()?;
        Ok(self)
    }

    /// The same content with the flora of `flora` (the text of
    /// `flora.ron`).
    pub fn with_flora(mut self, flora: &str) -> Result<Self, ContentError> {
        self.flora = crate::flora::parse(flora).map_err(|source| ContentError::Parse {
            file: "flora.ron",
            source,
        })?;
        self.validate()?;
        Ok(self)
    }

    /// The same content with the rock regions of `geology` (the text of
    /// `geology.ron`).
    pub fn with_geology(mut self, geology: &str) -> Result<Self, ContentError> {
        self.geology = ron::from_str(geology).map_err(|source| ContentError::Parse {
            file: "geology.ron",
            source,
        })?;
        self.geology.check().map_err(ContentError::Geology)?;
        self.validate_bounds()?;
        Ok(self)
    }

    /// The same content with the open biomes' trees of `open_ground`
    /// (the text of `open_ground.ron`).
    pub fn with_open_ground(mut self, open_ground: &str) -> Result<Self, ContentError> {
        self.open_ground = ron::from_str(open_ground).map_err(|source| ContentError::Parse {
            file: "open_ground.ron",
            source,
        })?;
        self.open_ground.check().map_err(ContentError::OpenGround)?;
        Ok(self)
    }

    /// The same content with the crops of `crops` (the text of
    /// `crops.ron`): after the trees and the flora, whose kinds bear them.
    pub fn with_crops(mut self, crops: &str) -> Result<Self, ContentError> {
        self.crops = crate::crops::parse(crops).map_err(|source| ContentError::Parse {
            file: "crops.ron",
            source,
        })?;
        self.validate_crops()?;
        self.validate_bounds()?;
        Ok(self)
    }

    /// Checks the crops: each borne by a tree or a plant of the content,
    /// giving one of its materials, sensible (`crops::check`), and no
    /// kind giving the same material twice.
    fn validate_crops(&self) -> Result<(), ContentError> {
        let mut seen = BTreeSet::new();
        for crop in &self.crops {
            let invalid = |what| ContentError::Crop {
                on: crop.on.clone(),
                gives: crop.gives.clone(),
                what,
            };
            let borne = self.species.iter().any(|s| s.id == crop.on)
                || self.flora.iter().any(|k| k.id == crop.on);
            if !borne {
                return Err(invalid("no tree or plant of that id"));
            }
            if self.material(&crop.gives).is_none() {
                return Err(invalid("no material of that id"));
            }
            if !seen.insert((crop.on.as_str(), crop.gives.as_str())) {
                return Err(invalid("defined twice"));
            }
            if let Some(what) = crate::crops::check(crop) {
                return Err(invalid(what));
            }
        }
        Ok(())
    }

    /// Checks the flora: unique ids (among the trees too), and each kind
    /// sensible (`flora::check`).
    fn validate_flora(&self) -> Result<(), ContentError> {
        // A host may be a tree of the game or one still to come.
        let trees: Vec<&str> = self
            .species
            .iter()
            .map(|s| s.id.as_str())
            .chain(
                self.flora
                    .iter()
                    .filter(|k| k.family == crate::flora::Family::Tree)
                    .map(|k| k.id.as_str()),
            )
            .collect();
        let mut ids: BTreeSet<&str> = self.species.iter().map(|s| s.id.as_str()).collect();
        for kind in &self.flora {
            let invalid = |what| ContentError::Species {
                id: kind.id.clone(),
                what,
            };
            if !ids.insert(kind.id.as_str()) {
                return Err(invalid("defined twice (in flora.ron or species.ron)"));
            }
            if let Some(what) = crate::flora::check(kind, &trees) {
                return Err(invalid(what));
            }
        }
        Ok(())
    }

    /// Checks the kinds of tree: unique ids, a name, a sensible height,
    /// and somewhere to grow.
    fn validate_species(&self) -> Result<(), ContentError> {
        let mut species_ids = BTreeSet::new();
        for species in &self.species {
            let invalid = |what| ContentError::Species {
                id: species.id.clone(),
                what,
            };
            if !species_ids.insert(species.id.as_str()) {
                return Err(invalid("defined twice"));
            }
            if species.id.is_empty()
                || !species
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                return Err(invalid("id must be snake_case ASCII"));
            }
            if species.name.trim().is_empty() {
                return Err(invalid("name is empty"));
            }
            if !(species.height_m.is_finite() && (0.5..=40.0).contains(&species.height_m)) {
                return Err(invalid("height_m is 0.5 to 40"));
            }
            if species.habitats.values().all(|w| *w == 0) {
                return Err(invalid("grows nowhere"));
            }
            if !(4..=MOST_SPREAD_M).contains(&species.spread_m) {
                return Err(invalid("spread_m is 4 to 36"));
            }
            if let Some(what) = species.fruit.as_ref().and_then(crate::crops::check_window) {
                return Err(invalid(what));
            }
        }
        Ok(())
    }

    /// Checks ids are unique, references resolve and values make sense.
    pub fn validate(&self) -> Result<(), ContentError> {
        let mut ids = BTreeSet::new();
        for material in &self.materials {
            if !ids.insert(material.id.as_str()) {
                return Err(ContentError::DuplicateMaterial(material.id.clone()));
            }
            let invalid = |what| ContentError::InvalidValue {
                id: material.id.clone(),
                what,
            };
            if material.id.is_empty()
                || !material
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                return Err(invalid("id must be snake_case ASCII"));
            }
            if material.name.trim().is_empty() {
                return Err(invalid("name is empty"));
            }
            if material.description.trim().is_empty() {
                return Err(invalid("description is empty"));
            }
            if !(material.mass_kg.is_finite() && material.mass_kg > 0.0) {
                return Err(invalid("mass_kg must be positive"));
            }
            if material.hardness > 10 {
                return Err(invalid("hardness is 0 to 10"));
            }
            if !is_hex_colour(&material.look.colour) {
                return Err(invalid("look.colour must be #rrggbb"));
            }
        }
        self.validate_species()?;
        self.validate_flora()?;
        for (index, spawn) in self.spawns.iter().enumerate() {
            if !ids.contains(spawn.material.as_str()) {
                return Err(ContentError::UnknownMaterial {
                    index,
                    material: spawn.material.clone(),
                });
            }
            if spawn.per_thousand > 1000 {
                return Err(ContentError::TooDense {
                    index,
                    per_thousand: spawn.per_thousand,
                });
            }
            if spawn.near.is_some_and(|near| near.within == 0) {
                return Err(ContentError::Spawn {
                    index,
                    what: "near.within is 1 or more",
                });
            }
            // A natural item's id holds its rule's index (crate::items).
            if index >= crate::items::MOST_RULES {
                return Err(ContentError::Spawn {
                    index,
                    what: "at most 64 spawn rules",
                });
            }
            if spawn.near.is_some_and(|near| near.within > 16) {
                return Err(ContentError::Spawn {
                    index,
                    what: "near.within is 16 at most (a chunk looks that far round)",
                });
            }
        }
        self.validate_movement()?;
        if Season::ALL
            .iter()
            .any(|s| !(1..100).contains(&self.needs.night_from_percent.get(*s)))
        {
            return Err(ContentError::Needs("night_from_percent is 1 to 99"));
        }
        if Season::ALL
            .iter()
            .any(|s| !(1..=1000).contains(&self.needs.thirst_heat_percent.get(*s)))
            || !(1..=1000).contains(&self.needs.thirst_effort_percent)
        {
            return Err(ContentError::Needs(
                "thirst_heat_percent and thirst_effort_percent are 1 to 1000",
            ));
        }
        if self.needs.kj_per_hunger_point == 0 {
            return Err(ContentError::Needs("kj_per_hunger_point must be positive"));
        }
        if self.needs.meal_g == 0 {
            return Err(ContentError::Needs("meal_g must be positive"));
        }
        self.needs.validate_reserve()?;
        self.needs.ages.validate()?;
        self.needs.life.validate()?;
        self.needs.trials.validate()?;
        self.needs.thought.validate()
    }

    /// Refuses the values the simulation cannot carry (audit of 20
    /// November 2026, S4): a rate, a mass or a budget at `u32::MAX`
    /// overflowed the gauges' arithmetic or let one search run a whole
    /// tick. Each ceiling is far above today's values (ten times at the
    /// least, mostly a thousand) and far below what overflows.
    pub fn validate_bounds(&self) -> Result<(), ContentError> {
        self.validate_bounds_of_bodies()?;
        self.validate_bounds_of_land()
    }

    /// [`Content::validate_bounds`]: the needs and the movement.
    fn validate_bounds_of_bodies(&self) -> Result<(), ContentError> {
        let movement = &self.movement;
        at_most(
            "movement.ron",
            "walk_speed_cm_per_s",
            movement.walk_speed_cm_per_s,
            100 * THOUSAND,
        )?;
        for (terrain, cost) in &movement.terrain_costs {
            if let Some(cost) = cost {
                at_most(
                    "movement.ron",
                    format!("terrain_costs.{terrain:?}"),
                    *cost,
                    WEIGHT,
                )?;
            }
        }
        at_most(
            "movement.ron",
            "carry_capacity_g",
            movement.carry_capacity_g,
            100 * MILLION,
        )?;
        for (terrain, sight) in &movement.sight_tiles {
            at_most(
                "movement.ron",
                format!("sight_tiles.{terrain:?}"),
                *sight,
                WEIGHT,
            )?;
        }
        at_most(
            "movement.ron",
            "path_budget_tiles",
            movement.path_budget_tiles,
            MILLION,
        )?;
        at_most(
            "movement.ron",
            "search_work_per_tick",
            movement.search_work_per_tick,
            10 * MILLION,
        )?;

        let needs = &self.needs;
        for (field, value) in [
            ("hunger_per_day", needs.hunger_per_day),
            ("thirst_per_day", needs.thirst_per_day),
            ("cold_per_day_at_night", needs.cold_per_day_at_night),
            ("cold_relief_per_day", needs.cold_relief_per_day),
            ("fatigue_per_day_awake", needs.fatigue_per_day_awake),
            ("rest_per_day_asleep", needs.rest_per_day_asleep),
            ("starving_per_day", needs.starving_per_day),
            ("freezing_per_day", needs.freezing_per_day),
            ("dehydration_per_day", needs.dehydration_per_day),
            ("vitality_regain_per_day", needs.vitality_regain_per_day),
            ("kj_per_hunger_point", needs.kj_per_hunger_point),
            ("meal_g", needs.meal_g),
        ] {
            at_most("needs.ron", field, value, MILLION)?;
        }
        at_most("needs.ron", "reserve_kj", needs.reserve_kj, 1000 * MILLION)?;
        at_most(
            "needs.ron",
            "intake_ceiling_percent",
            needs.intake_ceiling_percent,
            WEIGHT,
        )?;
        let ages = &needs.ages;
        for &(years, percent) in &ages.need_by_age {
            at_most("needs.ron", "ages.need_by_age (years)", years, THOUSAND)?;
            at_most("needs.ron", "ages.need_by_age (percent)", percent, WEIGHT)?;
        }
        for (field, value) in [
            ("ages.woman_percent", ages.woman_percent),
            ("ages.nursing_percent", ages.nursing_percent),
            ("ages.adopt_within_tiles", ages.adopt_within_tiles),
        ] {
            at_most("needs.ron", field, value, WEIGHT)?;
        }
        // Not `slow_over_years`: only compared with an age, and `u32::MAX`
        // is its "never" when the ages are absent.
        for (field, value) in [
            ("ages.woman_from_years", ages.woman_from_years),
            ("ages.weaned_years", ages.weaned_years),
            ("ages.slow_under_years", ages.slow_under_years),
        ] {
            at_most("needs.ron", field, value, THOUSAND)?;
        }
        at_most(
            "needs.ron",
            "ages.infant_mass_g",
            ages.infant_mass_g.1,
            MILLION,
        )?;
        self.validate_bounds_of_life()
    }

    /// [`Content::validate_bounds`]: the life rule (D49), whose ages and
    /// shares are small numbers.
    fn validate_bounds_of_life(&self) -> Result<(), ContentError> {
        let life = &self.needs.life;
        for &(years, permille) in &life.death_by_age {
            at_most("needs.ron", "life.death_by_age (years)", years, THOUSAND)?;
            at_most(
                "needs.ron",
                "life.death_by_age (per mille)",
                permille,
                THOUSAND,
            )?;
        }
        for (field, value) in [
            ("life.fertile_years", life.fertile_years.1),
            ("life.mate_years", life.mate_years.1),
            ("life.old_age_from_years", life.old_age_from_years),
            ("life.pregnancy_seasons", life.pregnancy_seasons),
        ] {
            at_most("needs.ron", field, value, THOUSAND)?;
        }
        for (field, value) in [
            ("life.mate_within_tiles", life.mate_within_tiles),
            ("life.pregnant_percent", life.pregnant_percent),
        ] {
            at_most("needs.ron", field, value, WEIGHT)?;
        }
        let trials = &self.needs.trials;
        for (field, value) in [
            ("trials.trial_after_days", trials.trial_after_days),
            ("trials.trial_days", trials.trial_days),
            ("trials.imitate_tiles", trials.imitate_tiles),
        ] {
            at_most("needs.ron", field, value, THOUSAND)?;
        }
        at_most(
            "needs.ron",
            "trials.new_home_walk_tiles",
            trials.new_home_walk_tiles,
            MILLION,
        )?;
        at_most(
            "needs.ron",
            "thought.plant_sight_tiles",
            self.needs.thought.plant_sight_tiles,
            THOUSAND,
        )?;
        Ok(())
    }

    /// [`Content::validate_bounds`]: the materials, what lies on the
    /// ground, the trees, the plants, the rock and the crops.
    fn validate_bounds_of_land(&self) -> Result<(), ContentError> {
        for material in &self.materials {
            let field = |name: &str| format!("material `{}`: {name}", material.id);
            metres("materials.ron", field("mass_kg"), material.mass_kg)?;
            if let Some(food) = &material.food {
                at_most(
                    "materials.ron",
                    field("food.energy_kj_per_kg"),
                    food.energy_kj_per_kg,
                    MILLION,
                )?;
                at_most(
                    "materials.ron",
                    field("food.keeps_days"),
                    food.keeps_days,
                    MILLION,
                )?;
            }
        }
        for (index, spawn) in self.spawns.iter().enumerate() {
            if let Some(near) = spawn.near {
                at_most(
                    "spawns.ron",
                    format!("rule {index}: near.within"),
                    near.within,
                    THOUSAND,
                )?;
            }
        }

        for species in &self.species {
            let field = |name: &str| format!("species `{}`: {name}", species.id);
            metres("species.ron", field("height_m"), species.height_m)?;
            at_most("species.ron", field("spread_m"), species.spread_m, THOUSAND)?;
            for (habitat, weight) in &species.habitats {
                at_most(
                    "species.ron",
                    field(&format!("habitats.{habitat:?}")),
                    *weight,
                    WEIGHT,
                )?;
            }
        }
        for kind in &self.flora {
            let field = |name: &str| format!("plant `{}`: {name}", kind.id);
            metres("flora.ron", field("height_m"), kind.height_m)?;
            at_most(
                "flora.ron",
                field("per_thousand"),
                kind.per_thousand,
                THOUSAND,
            )?;
            for (habitat, weight) in &kind.habitats {
                at_most(
                    "flora.ron",
                    field(&format!("habitats.{habitat}")),
                    *weight,
                    WEIGHT,
                )?;
            }
            if let Some(host) = &kind.host {
                metres("flora.ron", field("host.within_m"), host.within_m)?;
            }
        }
        let geology = &self.geology;
        for (field, value) in [
            ("region_m", geology.region_m),
            ("border_cell_m", geology.border_cell_m),
            ("border_shift_m", geology.border_shift_m),
        ] {
            at_most("geology.ron", field, value, MILLION)?;
        }
        for (rock, weight) in &geology.weights {
            at_most("geology.ron", format!("weights.{rock:?}"), *weight, WEIGHT)?;
        }
        for crop in &self.crops {
            let field = |name: &str| format!("crop `{}` on `{}`: {name}", crop.gives, crop.on);
            for (name, grams) in [
                ("grams (lean)", crop.grams.0),
                ("grams (usual)", crop.grams.1),
                ("grams (rich)", crop.grams.2),
                ("per_hour_g", crop.per_hour_g),
            ] {
                at_most("crops.ron", field(name), grams, MILLION)?;
            }
        }
        Ok(())
    }

    /// Checks the movement rules (D32).
    fn validate_movement(&self) -> Result<(), ContentError> {
        if self.movement.walk_speed_cm_per_s == 0 {
            return Err(ContentError::Movement(
                "walk_speed_cm_per_s must be positive",
            ));
        }
        for terrain in Terrain::ALL {
            match self.movement.terrain_costs.get(&terrain) {
                None => return Err(ContentError::Movement("every terrain needs a cost")),
                Some(Some(0)) => return Err(ContentError::Movement("a cost is 1 or more")),
                Some(_) => {}
            }
        }
        if self.movement.carry_capacity_g == 0 {
            return Err(ContentError::Movement("carry_capacity_g must be positive"));
        }
        if !(1..=100).contains(&self.movement.speed_percent_at_full_load) {
            return Err(ContentError::Movement(
                "speed_percent_at_full_load is 1 to 100",
            ));
        }
        for terrain in Terrain::ALL {
            match self.movement.sight_tiles.get(&terrain) {
                None => return Err(ContentError::Movement("every terrain needs a sight")),
                Some(0) => return Err(ContentError::Movement("a sight is 1 tile or more")),
                Some(_) => {}
            }
        }
        if self.movement.path_budget_tiles == 0 {
            return Err(ContentError::Movement("path_budget_tiles must be positive"));
        }
        if self.movement.search_work_per_tick == 0 {
            return Err(ContentError::Movement(
                "search_work_per_tick must be positive",
            ));
        }
        if self.movement.player_share_percent > 100 {
            return Err(ContentError::Movement("player_share_percent is 0 to 100"));
        }
        Ok(())
    }

    /// A material by id.
    pub fn material(&self, id: &str) -> Option<&Material> {
        self.materials.iter().find(|m| m.id == id)
    }

    /// A small, valid content for tests: a stone, a branch and a berry.
    /// The real valley's content lives in the `content/` directory.
    pub fn sample() -> Self {
        Self::from_ron(
            SAMPLE_MATERIALS,
            SAMPLE_SPAWNS,
            SAMPLE_MOVEMENT,
            SAMPLE_NEEDS,
        )
        .expect("sample content is valid")
    }
}

fn is_hex_colour(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Materials of [`Content::sample`].
pub const SAMPLE_MATERIALS: &str = r##"[
    (id: "flint", name: "Flint", description: "A hard stone that flakes to a sharp edge.", kind: stone, mass_kg: 0.4, hardness: 7, flammable: false, food: None, look: (colour: "#6e6a62")),
    (id: "branch", name: "Branch", description: "Dead wood, dry enough to burn.", kind: wood, mass_kg: 1.5, hardness: 3, flammable: true, food: None, look: (colour: "#7a5a3a")),
    (id: "berries", name: "Berries", description: "A handful of ripe berries.", kind: plant, mass_kg: 0.1, hardness: 0, flammable: false, food: Some((energy_kj_per_kg: 2500, keeps_days: 3)), look: (colour: "#8a2a5a")),
]"##;

/// Movement rules of [`Content::sample`]: the valley's own.
pub const SAMPLE_MOVEMENT: &str = r"(
    walk_speed_cm_per_s: 143,
    terrain_costs: { grass: Some(10), forest: Some(15), ford: Some(20), brook: Some(15), water: None, rock: None },
    carry_capacity_g: 20000,
    speed_percent_at_full_load: 60,
    sight_tiles: { grass: 60, forest: 25, ford: 60, brook: 60, water: 60, rock: 60 },
    path_budget_tiles: 1500,
    search_work_per_tick: 60000,
    player_share_percent: 50,
)";

/// Needs rules of [`Content::sample`]: the valley's own.
pub const SAMPLE_NEEDS: &str = r"(
    night_from_percent: (spring: 58, summer: 64, autumn: 58, winter: 56),
    hunger_per_day: 33,
    thirst_per_day: 100,
    cold_per_day_at_night: 300,
    cold_relief_per_day: 600,
    fatigue_per_day_awake: 120,
    rest_per_day_asleep: 300,
    starving_per_day: 100,
    freezing_per_day: 400,
    dehydration_per_day: 50,
    vitality_regain_per_day: 50,
    kj_per_hunger_point: 25,
    meal_g: 300,
    reserve_kj: 3300,
    intake_ceiling_percent: 400,
)";

/// Spawn rules of [`Content::sample`].
pub const SAMPLE_SPAWNS: &str = r#"[
    (material: "branch", terrain: forest, per_thousand: 60),
    (material: "berries", terrain: grass, per_thousand: 40, near: Some((terrain: forest, within: 2))),
]"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closeness_condition_needs_a_reach() {
        let zero = SAMPLE_SPAWNS.replacen("within: 2", "within: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, &zero, SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Spawn {
                index: 1,
                what: "near.within is 1 or more"
            }
        ));
        let content = Content::sample();
        assert_eq!(content.spawns[0].near, None, "near is optional");
        assert_eq!(
            content.spawns[1].near,
            Some(Near {
                terrain: Terrain::Forest,
                within: 2
            })
        );
    }

    #[test]
    fn movement_and_carrying_rules_are_checked() {
        let movement = Content::sample().movement;
        assert_eq!(movement.cost(Terrain::Grass), Some(10));
        assert_eq!(movement.cost(Terrain::Water), None);
        assert_eq!(movement.speed_cm_per_s(Terrain::Forest), 95);
        assert_eq!(movement.speed_cm_per_s(Terrain::Rock), 0);
        let no_rock = SAMPLE_MOVEMENT.replacen(", rock: None", "", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &no_rock, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("every terrain needs a cost")
        ));
        let weak = SAMPLE_MOVEMENT.replacen("carry_capacity_g: 20000", "carry_capacity_g: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &weak, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("carry_capacity_g must be positive")
        ));
        let frozen = SAMPLE_MOVEMENT.replacen(
            "speed_percent_at_full_load: 60",
            "speed_percent_at_full_load: 0",
            1,
        );
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &frozen, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("speed_percent_at_full_load is 1 to 100")
        ));
        assert_eq!(Content::sample().material("flint").unwrap().mass_g(), 400);
    }

    #[test]
    fn sight_budget_and_cap_are_checked() {
        let movement = Content::sample().movement;
        assert_eq!(movement.sight_tiles(Terrain::Grass), 60);
        assert_eq!(movement.sight_tiles(Terrain::Forest), 25);
        assert_eq!(movement.path_budget_tiles, 1500);
        assert_eq!(movement.search_work_per_tick, 60_000);
        assert_eq!(movement.player_share_percent, 50);
        let blind = SAMPLE_MOVEMENT.replacen("forest: 25, ", "", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &blind, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("every terrain needs a sight")
        ));
        let dark = SAMPLE_MOVEMENT.replacen("forest: 25", "forest: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &dark, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("a sight is 1 tile or more")
        ));
        let idle = SAMPLE_MOVEMENT.replacen("path_budget_tiles: 1500", "path_budget_tiles: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &idle, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("path_budget_tiles must be positive")
        ));
        let stuck =
            SAMPLE_MOVEMENT.replacen("search_work_per_tick: 60000", "search_work_per_tick: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &stuck, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("search_work_per_tick must be positive")
        ));
        let greedy =
            SAMPLE_MOVEMENT.replacen("player_share_percent: 50", "player_share_percent: 101", 1);
        let endless = SAMPLE_MOVEMENT.replacen(
            "search_work_per_tick: 60000",
            "search_work_per_tick: 4294967295",
            1,
        );
        let err =
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &endless, SAMPLE_NEEDS).unwrap_err();
        assert_eq!(
            err.to_string(),
            "movement.ron: search_work_per_tick is at most 10000000"
        );
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &greedy, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("player_share_percent is 0 to 100")
        ));
    }

    #[test]
    fn needs_rules_are_checked() {
        let late = SAMPLE_NEEDS.replacen("winter: 56", "winter: 100", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, SAMPLE_MOVEMENT, &late).unwrap_err(),
            ContentError::Needs("night_from_percent is 1 to 99")
        ));
        let free = SAMPLE_NEEDS.replacen("kj_per_hunger_point: 25", "kj_per_hunger_point: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, SAMPLE_MOVEMENT, &free).unwrap_err(),
            ContentError::Needs("kj_per_hunger_point must be positive")
        ));
        let starved = SAMPLE_NEEDS.replacen("meal_g: 300", "meal_g: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, SAMPLE_MOVEMENT, &starved)
                .unwrap_err(),
            ContentError::Needs("meal_g must be positive")
        ));
        for (from, to) in [
            ("reserve_kj: 3300", "reserve_kj: 0"),
            ("intake_ceiling_percent: 400", "intake_ceiling_percent: 99"),
            (
                "meal_g: 300,",
                "meal_g: 300, thin_percent: 10, weak_percent: 20,",
            ),
            ("meal_g: 300,", "meal_g: 300, weak_pace_percent: 0,"),
        ] {
            let bad = SAMPLE_NEEDS.replacen(from, to, 1);
            assert!(
                matches!(
                    Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, SAMPLE_MOVEMENT, &bad)
                        .unwrap_err(),
                    ContentError::Needs(what) if what.starts_with("reserve_kj")
                ),
                "{to}"
            );
        }
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, SAMPLE_MOVEMENT, "nope")
                .unwrap_err(),
            ContentError::Parse {
                file: "needs.ron",
                ..
            }
        ));
    }

    #[test]
    fn sample_content_parses_and_validates() {
        let content = Content::sample();
        assert_eq!(content.materials.len(), 3);
        assert_eq!(content.spawns.len(), 2);
        let berries = content.material("berries").unwrap();
        assert_eq!(berries.kind, MaterialKind::Plant);
        assert_eq!(berries.food.map(|f| f.keeps_days), Some(3));
        // 2,500 kJ a kilogram: a 100 g handful gives 250 kJ, as one piece
        // did before energy was by weight; 1 g gives 2 (rounded down).
        let food = berries.food.unwrap();
        assert_eq!(food.energy_kj(berries.mass_g()), 250);
        assert_eq!(food.energy_kj(1), 2);
        // A spawn rule knows the grams of the piece it drops.
        let flint = content.material("flint").unwrap().mass_g();
        assert!(content.spawns.iter().all(|s| s.piece_g > 0));
        assert!(
            content
                .spawns
                .iter()
                .filter(|s| s.material == "flint")
                .all(|s| s.piece_g == flint)
        );
        assert!(content.material("gold").is_none());
    }

    #[test]
    fn a_parse_error_names_the_file() {
        let err = Content::from_ron("[(id: oops", SAMPLE_SPAWNS, SAMPLE_MOVEMENT, SAMPLE_NEEDS)
            .unwrap_err();
        assert!(matches!(
            err,
            ContentError::Parse {
                file: "materials.ron",
                ..
            }
        ));
        let err =
            Content::from_ron(SAMPLE_MATERIALS, "nope", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err();
        assert!(matches!(
            err,
            ContentError::Parse {
                file: "spawns.ron",
                ..
            }
        ));
    }

    #[test]
    fn duplicate_and_unknown_ids_are_refused() {
        let twice = format!(
            "[{0}, {0}]",
            r##"(id: "flint", name: "Flint", description: "A stone.", kind: stone, mass_kg: 1, hardness: 7, flammable: false, food: None, look: (colour: "#000000"))"##
        );
        assert!(matches!(
            Content::from_ron(&twice, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::DuplicateMaterial(id) if id == "flint"
        ));
        let err = Content::from_ron(
            SAMPLE_MATERIALS,
            r#"[(material: "gold", terrain: grass, per_thousand: 1)]"#,
            SAMPLE_MOVEMENT,
            SAMPLE_NEEDS,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContentError::UnknownMaterial { index: 0, material } if material == "gold"
        ));
    }

    #[test]
    fn values_outside_sense_are_refused() {
        let bad = |field: &str| {
            let text = SAMPLE_MATERIALS.replacen(field, "", 1);
            Content::from_ron(&text, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err()
        };
        // Removing "flammable: false, " makes the struct incomplete: a parse error.
        assert!(matches!(
            bad("flammable: false, "),
            ContentError::Parse { .. }
        ));

        let heavy = SAMPLE_MATERIALS.replacen("mass_kg: 0.4", "mass_kg: 0", 1);
        assert!(matches!(
            Content::from_ron(&heavy, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::InvalidValue {
                what: "mass_kg must be positive",
                ..
            }
        ));
        let hard = SAMPLE_MATERIALS.replacen("hardness: 7", "hardness: 11", 1);
        assert!(matches!(
            Content::from_ron(&hard, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::InvalidValue {
                what: "hardness is 0 to 10",
                ..
            }
        ));
        let colour = SAMPLE_MATERIALS.replacen("#6e6a62", "grey", 1);
        assert!(matches!(
            Content::from_ron(&colour, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::InvalidValue {
                what: "look.colour must be #rrggbb",
                ..
            }
        ));
        let mute = SAMPLE_MATERIALS.replacen(
            "description: \"A hard stone that flakes to a sharp edge.\"",
            "description: \" \"",
            1,
        );
        assert!(matches!(
            Content::from_ron(&mute, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::InvalidValue {
                what: "description is empty",
                ..
            }
        ));
        let shouting = SAMPLE_MATERIALS.replacen("id: \"flint\"", "id: \"Flint\"", 1);
        assert!(matches!(
            Content::from_ron(&shouting, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::InvalidValue {
                what: "id must be snake_case ASCII",
                ..
            }
        ));
        let dense = SAMPLE_SPAWNS.replacen("per_thousand: 60", "per_thousand: 1001", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, &dense, SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::TooDense {
                index: 0,
                per_thousand: 1001
            }
        ));
    }

    #[test]
    fn species_are_checked() {
        let base = || Content::sample();
        let ok = r#"[(id: "oak", name: "Oak", height_m: 4.0, habitats: { heart: 50, lone: 60 })]"#;
        let content = base().with_species(ok).unwrap();
        assert_eq!(content.species[0].habitats[&Habitat::Lone], 60);
        let twice = r#"[(id: "oak", name: "Oak", height_m: 4.0, habitats: { heart: 1 }),
                        (id: "oak", name: "Oak", height_m: 4.0, habitats: { heart: 1 })]"#;
        assert!(matches!(
            base().with_species(twice).unwrap_err(),
            ContentError::Species { .. }
        ));
        let nowhere = r#"[(id: "oak", name: "Oak", height_m: 4.0, habitats: { heart: 0 })]"#;
        assert!(matches!(
            base().with_species(nowhere).unwrap_err(),
            ContentError::Species { .. }
        ));
        let far =
            r#"[(id: "oak", name: "Oak", height_m: 4.0, habitats: { heart: 1 }, spread_m: 80)]"#;
        assert!(matches!(
            base().with_species(far).unwrap_err(),
            ContentError::Species { .. }
        ));
        let fruit = r#"[(id: "oak", name: "Oak", height_m: 4.0, habitats: { heart: 1 },
                        fruit: Some((from: (autumn, 0), until: (winter, 0))))]"#;
        let content = base().with_species(fruit).unwrap();
        assert_eq!(
            content.species[0].fruit.map(|w| (w.from, w.until)),
            Some(((Season::Autumn, 0), (Season::Winter, 0)))
        );
        let no_time = r#"[(id: "oak", name: "Oak", height_m: 4.0, habitats: { heart: 1 },
                          fruit: Some((from: (autumn, 0), until: (autumn, 0))))]"#;
        assert!(matches!(
            base().with_species(no_time).unwrap_err(),
            ContentError::Species { .. }
        ));
        let giant = r#"[(id: "oak", name: "Oak", height_m: 400.0, habitats: { heart: 1 })]"#;
        assert!(matches!(
            base().with_species(giant).unwrap_err(),
            ContentError::Species { .. }
        ));
        assert!(matches!(
            base().with_species("nope").unwrap_err(),
            ContentError::Parse {
                file: "species.ron",
                ..
            }
        ));
        // The repository's own file is valid.
        base()
            .with_species(include_str!("../../../content/species.ron"))
            .unwrap();
    }
}

#[cfg(test)]
mod bounds_tests {
    use super::*;

    #[test]
    fn a_rate_the_gauges_cannot_carry_is_refused_naming_the_file_and_field() {
        let flood = SAMPLE_NEEDS.replacen("thirst_per_day: 100", "thirst_per_day: 4294967295", 1);
        assert_ne!(flood, SAMPLE_NEEDS);
        let err = Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, SAMPLE_MOVEMENT, &flood)
            .unwrap_err();
        assert!(
            matches!(
                err,
                ContentError::TooLarge {
                    file: "needs.ron",
                    ref field,
                    max: 1_000_000
                } if field == "thirst_per_day"
            ),
            "{err}"
        );
        assert_eq!(
            err.to_string(),
            "needs.ron: thirst_per_day is at most 1000000"
        );
        // The sample itself, and so the repository's placeholders, pass.
        Content::from_ron(
            SAMPLE_MATERIALS,
            SAMPLE_SPAWNS,
            SAMPLE_MOVEMENT,
            SAMPLE_NEEDS,
        )
        .unwrap()
        .validate_bounds()
        .unwrap();
    }
}
