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

use crate::hands::Part;
use crate::map::{CHUNK_SIZE, Terrain};
use crate::time::{Calendar, Season};
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
    /// Days before it spoils, untreated (D16: computed on demand): a
    /// short keeping, written in days (D09: what rots follows the day).
    /// 0 when it keeps seasons.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub keeps_days: u32,
    /// Seasons before it spoils, untreated: a long keeping (nuts, grain,
    /// seeds), written in seasons so it follows the world's cycle when
    /// the calendar is tuned (D09, D15). 0 when it keeps days; never both.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub keeps_seasons: u32,
}

impl Food {
    /// The days it keeps in a cycle of `days_per_cycle` days: its
    /// seasons in whole days when it keeps seasons, else its days.
    #[must_use]
    pub fn days_kept(&self, days_per_cycle: u32) -> u32 {
        if self.keeps_seasons > 0 {
            season_days(self.keeps_seasons, days_per_cycle)
        } else {
            self.keeps_days
        }
    }

    /// The ticks it keeps under `calendar` ([`Food::days_kept`]).
    #[must_use]
    pub fn ticks_kept(&self, calendar: &Calendar) -> u64 {
        u64::from(self.days_kept(calendar.days_per_cycle())) * calendar.ticks_per_day()
    }

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
    /// What comes of folding a piece of it (D57, amended 27 November
    /// 2026), by what it is: nothing when it is no sheet; a sheet at
    /// least `fold_suppleness` supple takes a hollow, the open box of
    /// the most volume a square sheet of its face folds into (2/27 of
    /// its side cubed); a stiffer sheet breaks.
    #[must_use]
    pub fn folded(&self, fold_suppleness: u8) -> Folded {
        if self.form != Form::Sheet || self.area_dm2 == 0 {
            return Folded::Nothing;
        }
        if self.suppleness < fold_suppleness {
            return Folded::Breaks;
        }
        // Its side in millimetres, then 2/27 of its cube: mm³ to ml.
        let side_mm = u64::from(self.area_dm2 * 10_000).isqrt();
        let ml = side_mm * side_mm * side_mm * 2 / 27 / 1000;
        Folded::Hollow(u32::try_from(ml).unwrap_or(u32::MAX).max(1))
    }

    /// Whether a piece of `self` in hand works a piece of `other` loose
    /// (D57, amended 27 November 2026): it has an edge, at least as keen
    /// as `other` is tough, and is the harder of the two.
    #[must_use]
    pub fn works(&self, other: &Material) -> bool {
        self.edge > 0 && self.edge >= other.toughness && self.hardness > other.hardness
    }

    /// Which of two things struck one against the other breaks (D57,
    /// amended 27 November 2026), by what they are: the less tough of
    /// the two, `Some(true)` when it is `self`; neither when they are as
    /// tough, when one has no toughness to speak of (flesh, fruit), or
    /// when the one that would break is supple enough to fold: it gives
    /// under the blow.
    #[must_use]
    pub fn struck_with(&self, other: &Material, fold_suppleness: u8) -> Option<bool> {
        if self.toughness == 0 || other.toughness == 0 || self.toughness == other.toughness {
            return None;
        }
        let mine = self.toughness < other.toughness;
        let broken = if mine { self } else { other };
        (broken.suppleness < fold_suppleness).then_some(mine)
    }

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

    /// Whether a pile of it is loose (D57, amended 27 November 2026):
    /// many small things, of which a part may go where the whole would
    /// not. Food, as the arms hold it (D35), and whatever comes in bits.
    #[must_use]
    pub fn loose(&self) -> bool {
        self.food.is_some() || self.form == Form::Bits
    }

    /// The days after it was made that it tears, in a cycle of
    /// `days_per_cycle` days: its `wears_seasons` in whole days when set,
    /// else its `wears_days`; 0 when it never wears.
    #[must_use]
    pub fn days_worn(&self, days_per_cycle: u32) -> u32 {
        if self.wears_seasons > 0 {
            season_days(self.wears_seasons, days_per_cycle)
        } else {
            self.wears_days
        }
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
    /// How keen its edge is, 0 (none) to 10 (D57): a thing worked with
    /// it comes away when the edge is at least as keen as that thing is
    /// tough, and the stuff harder.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub edge: u8,
    /// How far it bends before it breaks, 0 (not at all) to 10 (D57).
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub suppleness: u8,
    /// How it holds together against an edge, a tear, a crumbling, 0
    /// (falls apart) to 10 (D57).
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub toughness: u8,
    /// The shape of a piece (D57).
    #[serde(default, skip_serializing_if = "Form::is_block")]
    pub form: Form,
    /// The face of a piece that is a sheet, in square decimetres: what a
    /// fold has to make a hollow of. 0 otherwise.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub area_dm2: u32,
    /// The days after it was folded that it tears at its folds and is
    /// gone (D57): a short wear, written in days. 0: not in days (food
    /// spoils by its own keeping).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub wears_days: u32,
    /// The seasons after it was folded that it tears and is gone: a long
    /// wear, written in seasons so it follows the world's cycle (D09).
    /// 0: not in seasons; never both; neither, it never wears.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub wears_seasons: u32,
    /// What a piece broken off it by a blow is made of (D57, amended 27
    /// November 2026), when the break changes what the eye and the hand
    /// find: a nodule of flint breaks into keen flint. None: its pieces
    /// are of itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breaks_into: Option<String>,
    /// How it looks.
    pub look: Look,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip_serializing_if
fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}

/// The shape of a piece of a material (D57): what a body sees of it, and
/// what a fold can make of it. It says what the piece is, never what it
/// is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Form {
    /// A lump: a stone, a bone, a piece of meat.
    #[default]
    Block,
    /// Flat and wide: a bark, a hide, a big leaf.
    Sheet,
    /// Long and stiff: a branch, a log.
    Stick,
    /// Long and limp: fibres.
    Strand,
    /// Many small things together: berries, nuts, small leaves.
    Bits,
}

impl Form {
    #[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip_serializing_if
    fn is_block(&self) -> bool {
        *self == Form::Block
    }
}

/// What comes of folding a piece (D57), computed from what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folded {
    /// Nothing: it is no sheet, or too small a one.
    Nothing,
    /// It breaks: a sheet too stiff to bend. The piece is lost.
    Breaks,
    /// It takes a hollow of so many millilitres, by its face.
    Hollow(u32),
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip_serializing_if
fn is_zero_u32(n: &u32) -> bool {
    *n == 0
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
    /// The heaviest load a body walks under, in grams (D35): the load at
    /// which it walks at `speed_percent_at_full_load`.
    pub carry_capacity_g: u32,
    /// What a body holds in its arms with no bag, in grams (D35, amended
    /// 24 November 2026): all it can carry while it has nothing to carry
    /// in. An infant rides on the back and takes nothing from it.
    pub hands_g: u32,
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
    /// How many legs along obstacles a body walks for one order before it
    /// stops and says it sees no way (D54): after each, it looks again
    /// from where it stands. 1 is the rule before D54: one skirt, then it
    /// stops.
    #[serde(default = "one")]
    pub skirts_per_order: u32,
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
fn one_night() -> Vec<(u32, u32)> {
    vec![(0, 1)]
}

fn nobody_calm() -> Vec<(u32, u32)> {
    vec![(0, 0)]
}

fn grove_64() -> u32 {
    64
}

fn permille_1000() -> u32 {
    1000
}

fn percent_100() -> u32 {
    100
}

fn reserve_kj_60000() -> u32 {
    60_000
}

fn percent_101() -> u32 {
    101
}

fn percent_150() -> u32 {
    150
}

fn percent_50() -> u32 {
    50
}

/// « The young first » before the life lot (D58): sixteen.
fn sixteen() -> u32 {
    16
}

/// The skirts for one order before D54 (D54).
fn one() -> u32 {
    1
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
    /// What a grown body burns walking, on top of the day's need, in
    /// joules a metre walked (D50, amended 24 November 2026: walking costs
    /// energy); a smaller body by its size. 0 when absent: walking is
    /// free.
    #[serde(default)]
    pub walk_j_per_m: u32,
    /// Cold rises this much per day of night, without warmth.
    pub cold_per_day_at_night: u32,
    /// The night's cold by season, in percent of `cold_per_day_at_night`:
    /// mild summer nights, cold winter ones (24 November 2026). All 100
    /// when absent.
    #[serde(default = "every_season_100")]
    pub cold_season_percent: BySeason<u32>,
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
    /// The evening fire and the common memory (D55): who tells how much,
    /// who keeps how much, the three states of a place, the hearth's
    /// nights. Absent: no fire, nobody tells nor hears anything.
    #[serde(default)]
    pub fire: Fire,
    /// The evening's third part (D56): at the fire, each present says
    /// what it will do tomorrow, groups form for the places told, a few
    /// go looking. Absent: nobody decides anything, each day is its own.
    #[serde(default)]
    pub decisions: Decisions,
    /// The will tempered and the confidence by activity (D50 amended,
    /// D56): failures borne, a late success weighing more, the temperament
    /// coming back, being listened to. Absent: the will moves by the
    /// trials' fixed steps, and no activity has a confidence.
    #[serde(default)]
    pub temperance: Temperance,
    /// Who is believed to know (D56, PR 1 ter): a reputation by activity
    /// held by each body in the common memory, and the questions asked of
    /// it at the fire. Absent: nobody is reputed, nobody asks.
    #[serde(default)]
    pub reputation: Reputation,
    /// Company, a need of the body felt at night at the fire (D53, D55,
    /// amended 24 November 2026): how much a night with too few at one's
    /// fire weighs in the notebook. Absent: nobody feels it.
    #[serde(default)]
    pub company: Company,
    /// Moving home as a consequence (D50, D53, D56, amended 24 November
    /// 2026): the push felt here, the surprise lived there, the tale that
    /// makes others go and see. Absent: nobody ever moves home.
    #[serde(default)]
    pub moving: Moving,
    /// The family's weight when a move is called (D56, amended 24
    /// November 2026), apart from company and added to it. Absent: the
    /// family weighs only as company.
    #[serde(default)]
    pub family: Family,
    /// Food pooled at the hearth (D58, validated by the project lead on 25
    /// November 2026): one reserve a camp, the young fed from it, the
    /// young first, and an innate care for their coming days. Absent:
    /// each eats what it gathers, and a weaned child gathers for itself.
    #[serde(default)]
    pub pooling: Pooling,
    /// The gestures of the hands (D57): how long a cut or a fold takes,
    /// how far around itself a body sees what it may try one on, how many
    /// it tries a day in a trial of its hands. Absent: nobody tries a
    /// gesture.
    #[serde(default)]
    pub hands: Hands,
    /// The refusals of the world remembered as problems (D50 and D57,
    /// amended 26 November 2026). Absent: nothing is remembered as a
    /// problem.
    #[serde(default)]
    pub problems: Problems,
    /// The point of no return and the trips (D53, D56, amended 25
    /// November 2026): away from its camp when the walk home would take
    /// the rest of the light, and each morning away, a body chooses to go
    /// home, to stay out or to go on. Absent: nobody chooses; a body
    /// gathers until night and walks home in the dark, as before.
    #[serde(default)]
    pub trips: Trips,
    /// The passages remembered and told (D53, amended 26 November 2026):
    /// a body that reached a place across water or rock keeps the point of
    /// its way through which it did, tells it at the fire with the place,
    /// and walks there in two legs. Absent: nobody keeps a passage.
    #[serde(default)]
    pub passages: Passages,
    /// Illness as episodes (D49, amended 26 November 2026): a body falls
    /// ill of the gut or of the chest by its age, the season, its camp's
    /// fouling, its hearth's crowd and sick, goes slower a few days, then
    /// heals or dies by its age and its thinness. Absent: the old rule,
    /// a background death drawn by age alone (`life.death_by_age`).
    #[serde(default, deserialize_with = "present")]
    pub illness: Option<Illness>,
    /// Learning causes, its first step: believe without acting (D59): an
    /// ailment lived, or seen in a kin at the fire, leaves an episode of
    /// what the body perceived; one who notices that it comes back cross-
    /// checks its episodes, and what was always there becomes a belief,
    /// told at the fire and copied. Nothing is done from a belief yet.
    /// Absent: nothing is learnt, bit for bit as before.
    #[serde(default, deserialize_with = "present")]
    pub learning: Option<Learning>,
    /// The search for an absent kin (D56, amended 27 November 2026): a
    /// close one not seen at the fire for `after_nights` nights is
    /// noticed, and one of its kin goes to the last place it knew it to
    /// be. Absent: nobody notices, bit for bit as before.
    #[serde(default, deserialize_with = "present")]
    pub search: Option<Search>,
}

/// The search for an absent kin (D56, amended 27 November 2026; the
/// project lead: "people notice that someone is absent from the fire,
/// and go and search the last place they knew her to be"). At the dawn's
/// fire a spouse, a parent or a child not seen for `after_nights` nights
/// is noticed; one kin, grown, goes to the place the absent said it was
/// going to, or the last place it saw it. Every value is a D30
/// placeholder, named and explained in `content/needs.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Search {
    /// The nights a kin is not seen before its absence is noticed.
    pub after_nights: u32,
    /// The age from which one goes searching, in years.
    pub from_years: u32,
    /// The longest absence a mind keeps of a close one come back, in
    /// nights: out no longer than that, it is not missed.
    pub usual_max_nights: u32,
    /// The seasons after which that longest absence is forgotten.
    pub usual_seasons: u32,
    /// The seasons a mind knows where a close one lives, at another
    /// hearth, without hearing of it again.
    pub lives_seasons: u32,
    /// What a searcher tells of its findings at a fire, at most.
    pub told_per_fire: u32,
    /// The findings a searcher keeps to tell, the last ones.
    pub findings_kept: u32,
}

impl Search {
    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            after_nights: 2,
            from_years: 16,
            usual_max_nights: 16,
            usual_seasons: 4,
            lives_seasons: 4,
            told_per_fire: 2,
            findings_kept: 4,
        }
    }

    fn validate(&self) -> Result<(), ContentError> {
        if self.after_nights == 0 {
            return Err(ContentError::Needs("search: after_nights at least 1"));
        }
        Ok(())
    }
}

/// A section read as present when written, without RON's `Some(…)`.
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

/// Food pooled at the hearth (D58; the project lead, 25 November 2026:
/// « le foyer », everyone who lies down at the camp's evening gathering).
/// One reserve a hearth, at the camp: who comes home puts there what it
/// carries beyond its own store, who is hungry at the camp eats there
/// before it eats its own, the young first. A child is fed by the hearth
/// until `fed_until_years` and does not gather alone. Every value is a
/// D30 placeholder, named and explained in `content/needs.ron`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pooling {
    /// Until this age a child is fed by the hearth and does not gather
    /// alone. 0: nothing is pooled.
    pub fed_until_years: u32,
    /// « The young first » (the project lead, 25 November 2026, then the
    /// life lot the same day: "les jeunes d'abord" at twelve, no longer at
    /// sixteen): who is younger than this, in years, eats from the
    /// hearth's reserve before the grown, and its coming days are the
    /// young's the grown care for. Absent: sixteen, as before the field.
    #[serde(default = "sixteen")]
    pub young_first_until_years: u32,
    /// The innate care for the young's coming days (the project lead, 25
    /// November 2026, « les jeunes d'abord »): drawn evenly between these
    /// per cent from the run's seed and the body's id, the share of the
    /// young of its hearth's uncovered need a grown body counts in its
    /// own. Absent: (0, 0), nobody cares beyond what it sees hungry.
    #[serde(default)]
    pub care_percent: (u32, u32),
    /// For its own children, its care times this per cent, at most 100.
    #[serde(default)]
    pub own_care_percent: u32,
    /// How many coming days of the young's need the reserve is weighed
    /// against. 0: no care.
    #[serde(default)]
    pub care_days: u32,
    /// The hunger bar (the project lead, 25 November 2026: "avoir très
    /// faim et porter de la nourriture pour les enfants, la garder, ça va
    /// être plus rare, et ça peut être en fonction de la volonté"): under
    /// this per cent of its fat reserve left, a hungry body is very
    /// hungry, and eats the food it carries for its hearth, or from the
    /// reserve despite the young, unless its will keeps it
    /// (`keep_percent_by_will`). 0: nobody is very hungry.
    #[serde(default)]
    pub very_hungry_fat_percent: u32,
    /// Under this per cent of its fat reserve left, a hungry body is
    /// starving and always eats it ("mourir de faim et avoir de la
    /// nourriture, ça personne ne le fera jamais"). 0: nobody starves so.
    #[serde(default)]
    pub starving_fat_percent: u32,
    /// The chance, in per cent by will, that a very hungry body keeps the
    /// hearth's food: (will, per cent), the wills rising.
    #[serde(default)]
    pub keep_percent_by_will: Vec<(u32, u32)>,
    /// The width of a heap of the hearth's reserve, in journées (amended
    /// 26 November 2026, the reserve as heaps): food laid there joins the
    /// heap of the same food whose band of `heap_days` journées it goes
    /// spoiled in, and the whole heap goes at the start of the band's
    /// first journée. 1 (the default, 0 read as 1): each part goes at the
    /// very journée's start it would have gone at alone; more, fewer
    /// heaps, food spoiling up to `heap_days - 1` journées early.
    #[serde(default = "one_day")]
    pub heap_days: u32,
    /// Whether a body knows its hearth's reserve by what it saw of it
    /// (the project lead, 26 November 2026: "no telepathy: a person
    /// knows what is at the camp when she sees it"; the audit of wakes,
    /// step 4): what the reserve held and how many lay in its circle the
    /// last time it saw it, awake at its camp, at the dawn circle, or
    /// laying a heap there itself. False, or absent: its share read from
    /// anywhere, as it is now, the rule before.
    #[serde(default)]
    pub hearth_look: bool,
    /// Whether the young are seen, not their stomach (the project lead,
    /// 26 November 2026; the audit of wakes, step 5): an awake grown body,
    /// or a child past `fed_until_years`, who sees a young one of its
    /// hearth hungry near it feeds it, from what it carries or from the
    /// reserve, and what the reserve keeps for the young counts the
    /// stomach of those it sees at the camp only, a day of need, less
    /// what was seen eaten at the camp, for the others. False, or absent:
    /// nobody feeds another, and the young's stomach is read from
    /// anywhere, the rule before.
    #[serde(default)]
    pub young_seen: bool,
    /// Whether a short giver gives only as its care says (the project
    /// lead, 26 November 2026: "an old man who goes without for his
    /// grandchildren is a story; an old man who ruins himself for any
    /// child of the camp is a defect"): a body hungry, or under
    /// `giver_short_fat_percent` of its fat reserve, feeds a young one
    /// of its hearth on the journées its care draws it to, its care for
    /// a child or grandchild of its own (`own_care_percent`), its care
    /// for any other; under `giver_kin_only_fat_percent`, its own only.
    /// False, or absent: the hunger bar decides for the giver as for its
    /// own meal, the rule before.
    #[serde(default)]
    pub giver_care: bool,
    /// Under this per cent of its fat reserve left, a giver is short
    /// (`giver_care`), as it is when hungry.
    #[serde(default)]
    pub giver_short_fat_percent: u32,
    /// Under this per cent of its fat reserve left, a short giver feeds
    /// its own young only (`giver_care`).
    #[serde(default)]
    pub giver_kin_only_fat_percent: u32,
    /// Under this per cent of its fat reserve left, a starving giver eats
    /// first and feeds no young one, even its own (`giver_care`; the
    /// project lead's hunger bar, 26 November 2026). Absent: no floor.
    #[serde(default)]
    pub giver_starving_fat_percent: u32,
}

/// The default width of a heap of the hearth's reserve: one journée.
fn one_day() -> u32 {
    1
}

/// How hungry a body is by its fat reserve left (D58, amended 25 November
/// 2026, the hunger bar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HungerTier {
    /// It carries home and leaves the young first, as before.
    Hungry,
    /// It eats the hearth's food or keeps it, by its will.
    VeryHungry,
    /// It eats it, always.
    Starving,
}

impl Pooling {
    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            fed_until_years: 8,
            young_first_until_years: 12,
            care_percent: (0, 100),
            own_care_percent: 200,
            care_days: 2,
            very_hungry_fat_percent: 50,
            starving_fat_percent: 25,
            keep_percent_by_will: vec![(0, 10), (100, 90)],
            heap_days: 1,
            // The switches of the audit of wakes at the rule before.
            hearth_look: false,
            young_seen: false,
            giver_care: false,
            giver_short_fat_percent: 50,
            giver_kin_only_fat_percent: 25,
            giver_starving_fat_percent: 15,
        }
    }

    /// How hungry a body is with `left_percent` of its fat reserve left.
    #[must_use]
    pub fn tier(&self, left_percent: u64) -> HungerTier {
        if left_percent < u64::from(self.starving_fat_percent) {
            HungerTier::Starving
        } else if left_percent < u64::from(self.very_hungry_fat_percent) {
            HungerTier::VeryHungry
        } else {
            HungerTier::Hungry
        }
    }

    /// The chance, in per cent, that a very hungry body of `will` keeps
    /// the hearth's food.
    #[must_use]
    pub fn keep_percent(&self, will: u8) -> u64 {
        if self.keep_percent_by_will.is_empty() {
            return 100;
        }
        along(&self.keep_percent_by_will, u64::from(will) * 1000, 1)
    }

    /// Whether anything is pooled.
    #[must_use]
    pub fn on(&self) -> bool {
        self.fed_until_years > 0
    }

    /// Whether a child that old is fed by the hearth.
    #[must_use]
    pub fn fed(&self, age_milli: u64) -> bool {
        age_milli < u64::from(self.fed_until_years) * 1000
    }

    /// Whether a body that old is among the young the hearth feeds first
    /// (« the young first »): younger than `young_first_until_years`.
    #[must_use]
    pub fn young_first(&self, age_milli: u64) -> bool {
        age_milli < u64::from(self.young_first_until_years) * 1000
    }

    /// Whether anybody cares for the young's coming days.
    #[must_use]
    pub fn cares(&self) -> bool {
        self.on() && self.care_days > 0 && self.care_percent.1 > 0
    }

    /// The care of a body whose draw is `draw` (0 to 1000), in per cent.
    #[must_use]
    pub fn care_at(&self, draw: u64) -> u8 {
        let (lo, hi) = (
            u64::from(self.care_percent.0),
            u64::from(self.care_percent.1),
        );
        let span = hi.saturating_sub(lo);
        u8::try_from(lo + span * draw.min(1000) / 1000).unwrap_or(100)
    }

    fn validate(&self) -> Result<(), ContentError> {
        let (lo, hi) = self.care_percent;
        if lo > hi || hi > 100 {
            return Err(ContentError::Needs(
                "pooling: care_percent (low, high), low at most high, high at most 100",
            ));
        }
        if self.own_care_percent > 1000 || self.fed_until_years > 100 || self.care_days > 100 {
            return Err(ContentError::Needs(
                "pooling: own_care_percent 1000, fed_until_years and care_days 100 at most",
            ));
        }
        if self.giver_short_fat_percent > 100
            || self.giver_kin_only_fat_percent > self.giver_short_fat_percent
            || self.giver_starving_fat_percent > self.giver_kin_only_fat_percent
        {
            return Err(ContentError::Needs(
                "pooling: giver_starving_fat_percent at most giver_kin_only_fat_percent, at most giver_short_fat_percent, at most 100",
            ));
        }
        if self.on()
            && (self.young_first_until_years < self.fed_until_years
                || self.young_first_until_years > 100)
        {
            return Err(ContentError::Needs(
                "pooling: young_first_until_years at least fed_until_years, at most 100",
            ));
        }
        let bar = &self.keep_percent_by_will;
        if self.starving_fat_percent > self.very_hungry_fat_percent
            || self.very_hungry_fat_percent > 100
            || !bar.windows(2).all(|p| p[0].0 < p[1].0)
            || !bar.iter().all(|p| p.0 <= 100 && p.1 <= 100)
            || (self.very_hungry_fat_percent > self.starving_fat_percent && bar.is_empty())
        {
            return Err(ContentError::Needs(
                "pooling: starving_fat_percent at most very_hungry_fat_percent, at most 100; \
                 keep_percent_by_will given when there is a very hungry tier, its wills \
                 rising, every value at most 100",
            ));
        }
        Ok(())
    }
}

/// The point of no return and the trips (D53, D56, amended 25 November
/// 2026; the project lead: "il faut effectivement qu'ils puissent
/// réfléchir à faire demi-tour, quand c'est nécessaire, mais aussi prendre
/// la décision de dormir sur place et de continuer le voyage"). A body
/// away from its camp chooses, when the walk home would take the rest of
/// the day's light and each morning it wakes away: hungry with no food
/// in sight it goes home; food in sight, it may stay out by
/// `stay_percent_by_will`; not hungry, it may go on by
/// `go_on_percent_by_will`; its way to sleep out (D50) raises either
/// chance to `sleep_out_percent`. Every value is a D30 placeholder, named
/// and explained in `content/needs.ron`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trips {
    /// The chance, in per cent by will, that a body seeing food around it
    /// stays out: (will, per cent). Empty: nobody chooses (off).
    pub stay_percent_by_will: Vec<(u32, u32)>,
    /// The chance, in per cent by will, that a body neither hungry nor
    /// seeing food goes on with its trip.
    pub go_on_percent_by_will: Vec<(u32, u32)>,
    /// With its way to sleep out (D50), either chance is at least this.
    pub sleep_out_percent: u32,
    /// Home nearer than this, in game minutes of walking at its pace
    /// (a game minute a 1,440th of the game day), a body does not choose:
    /// it goes home, even after nightfall (the project lead, 25 November
    /// 2026: nobody sleeps out half an hour from the hearth).
    #[serde(default)]
    pub choice_beyond_minutes: u32,
}

impl Trips {
    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            stay_percent_by_will: vec![(0, 30), (100, 90)],
            go_on_percent_by_will: vec![(0, 0), (100, 60)],
            sleep_out_percent: 90,
            choice_beyond_minutes: 30,
        }
    }

    /// Whether a body chooses at the point of no return.
    #[must_use]
    pub fn on(&self) -> bool {
        !self.stay_percent_by_will.is_empty()
    }

    /// The chance, in per cent, that a body of `will` seeing food stays
    /// out; with its way to sleep out, at least `sleep_out_percent`.
    #[must_use]
    pub fn stay_percent(&self, will: u8, sleeps_out: bool) -> u64 {
        self.with_way(&self.stay_percent_by_will, will, sleeps_out)
    }

    /// The chance, in per cent, that a body of `will` neither hungry nor
    /// seeing food goes on; with its way to sleep out, at least
    /// `sleep_out_percent`.
    #[must_use]
    pub fn go_on_percent(&self, will: u8, sleeps_out: bool) -> u64 {
        self.with_way(&self.go_on_percent_by_will, will, sleeps_out)
    }

    fn with_way(&self, curve: &[(u32, u32)], will: u8, sleeps_out: bool) -> u64 {
        if curve.is_empty() {
            return 0;
        }
        let by_will = along(curve, u64::from(will) * 1000, 1);
        if sleeps_out {
            by_will.max(u64::from(self.sleep_out_percent))
        } else {
            by_will
        }
    }

    fn validate(&self) -> Result<(), ContentError> {
        let fine = |curve: &[(u32, u32)]| {
            curve.windows(2).all(|p| p[0].0 < p[1].0)
                && curve.iter().all(|p| p.0 <= 100 && p.1 <= 100)
        };
        if self.stay_percent_by_will.is_empty() != self.go_on_percent_by_will.is_empty()
            || !fine(&self.stay_percent_by_will)
            || !fine(&self.go_on_percent_by_will)
            || self.sleep_out_percent > 100
            || self.choice_beyond_minutes > 1440
        {
            return Err(ContentError::Needs(
                "trips: stay_percent_by_will and go_on_percent_by_will both empty or both \
                 given, their wills rising, every value at most 100; sleep_out_percent at \
                 most 100; choice_beyond_minutes at most 1440",
            ));
        }
        Ok(())
    }
}

/// The passages remembered and told (D53, amended 26 November 2026; the
/// project lead, 24 November 2026: "Une personne qui trouve le chemin vient
/// le dire"). A body standing on a place of its notebook that a straight
/// walk from its camp could not reach, water or rock in the way, keeps the
/// elbow of its way through which it came: a ford, the end of a lake. It
/// tells it at the fire with the place; a listener who knows the place but
/// not the way holds it; going there, it walks first to the elbow, then to
/// the place. Every value is a D30 placeholder, named and explained in
/// `content/needs.ron`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Passages {
    /// A passage neither found, heard nor walked through for this many
    /// seasons (a quarter of the calendar's cycle, D09) is forgotten. 0:
    /// nobody keeps a passage (off).
    pub fade_seasons: u32,
    /// The passages a body tells at a fire at most, apart from the
    /// places' slots like a mark of seen, not reached.
    pub told_per_night: u32,
}

impl Passages {
    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            fade_seasons: 4,
            told_per_night: 2,
        }
    }

    /// Whether anybody keeps a passage.
    #[must_use]
    pub fn on(&self) -> bool {
        self.fade_seasons > 0
    }

    /// The days a passage is kept in a cycle of `days_per_cycle` days: its
    /// `fade_seasons` in whole days.
    #[must_use]
    pub fn fade_days(&self, days_per_cycle: u32) -> u32 {
        season_days(self.fade_seasons, days_per_cycle)
    }

    fn validate(&self) -> Result<(), ContentError> {
        if self.fade_seasons > 400 || self.told_per_night > 64 {
            return Err(ContentError::Needs(
                "passages: fade_seasons at most 400, told_per_night at most 64",
            ));
        }
        Ok(())
    }
}

/// The circumstances a body may note in an episode (D59), each a thing it
/// perceives itself: the code knows how to perceive each, the content
/// says which are noted and of what kind each is.
pub const CLUES: [&str; 8] = [
    "camp",
    "camp_age",
    "season",
    "crowd",
    "cold_night",
    "sick_at_fire",
    "ate",
    "drank",
];

/// The kinds of cause a circumstance may be of (D59): recorded with a
/// belief, read by nothing yet (its lever comes in the second step).
pub const CLUE_KINDS: [&str; 7] = [
    "place", "season", "weather", "company", "food", "water", "gesture",
];

/// Learning causes, its first step: believe without acting (D59; the
/// study `content/ai-reserve/review/vivarium/lot-5/etude-apprentissage-
/// 2026-11-27.md`, approved by the project lead on 27 November 2026).
/// Every value is a D30 placeholder, named and explained in
/// `content/needs.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Learning {
    /// Each ailment that marks a body, and the problem (a refusal of
    /// `problems.kinds`) it is lived as.
    pub ailments: Vec<(crate::Ailment, String)>,
    /// The circumstances noted in an episode, each a name of [`CLUES`]
    /// and its kind, a name of [`CLUE_KINDS`].
    pub clues: Vec<(String, String)>,
    /// The episodes a problem keeps, at most: the oldest drops off.
    pub episodes_kept: u32,
    /// A camp slept at fewer nights in a row than this is new.
    pub new_camp_nights: u32,
    /// A camp slept at more than this many seasons in a row is old;
    /// between, settled.
    pub old_camp_seasons: u32,
    /// A fire of fewer than this is a few; of `crowd_many` or more, many;
    /// between, some.
    pub crowd_some: u32,
    /// See `crowd_some`.
    pub crowd_many: u32,
    /// A sick body seen at one's fire within this many nights is noted.
    pub sick_seen_nights: u32,
    /// Noticing is drawn only at a mark whose problem keeps this many
    /// episodes or more.
    pub notice_from: u32,
    /// The chance to notice at a mark with a full list of episodes, by
    /// will (0 to 100, percent): smaller lists in proportion.
    pub notice_percent_by_will: Vec<(u32, u32)>,
    /// A child of its own seen sick weighs the draw this many times.
    pub child_times: u32,
    /// The episodes cross-checked, the latest first, by will: the depth
    /// of the search.
    pub depth_by_will: Vec<(u32, u32)>,
    /// With this many episodes or more side by side, a circumstance
    /// missing from one is still a suspect.
    pub all_but_one_from: u32,
    /// The suspects a belief names, at most: the draw picks among more.
    pub suspects_max: u32,
    /// The episodes without its suspects that make a belief fall.
    pub counter_examples: u32,
    /// A belief counts whole this many seasons from its last renewal…
    pub whole_seasons: u32,
    /// …and at half this many more, then it is forgotten.
    pub half_seasons: u32,
    /// The beliefs a body tells at a fire at most.
    pub told_per_night: u32,
    /// The least credit a listener must have in a teller to take its
    /// belief (1 to 100).
    pub copy_credit: u32,
    /// Under this age (years), a listener takes a belief without having
    /// lived its problem: the tradition.
    pub child_years: u32,
    /// Its second step, acting (D59, amended 27 November 2026): a belief
    /// weighs on moves, its lever tried and judged. Absent: believe only.
    #[serde(default, deserialize_with = "present")]
    pub acting: Option<Acting>,
}

/// The levers a kind of cause may give (D59, amended 27 November 2026):
/// only `leave` is built; the others are recorded where they would apply.
pub const LEVERS: [&str; 6] = [
    "leave",
    "prepare",
    "keep_apart",
    "avoid",
    "drink_elsewhere",
    "otherwise",
];

/// Learning causes, its second step: acting (D59, amended 27 November
/// 2026; the project lead: a belief is held through fear or reward, its
/// strength from what was lived). Every value is a D30 placeholder,
/// named and explained in `content/needs.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Acting {
    /// A belief's strength (points) when its gravest episode is a kin's
    /// death of it…
    pub strength_kin_death: u32,
    /// …one's own illness…
    pub strength_own: u32,
    /// …a sick body seen (a kin, a mate at the fire).
    pub strength_seen: u32,
    /// A belief heard carries this share of its teller's strength, in
    /// percent.
    pub heard_percent: u32,
    /// What a point of strength weighs on a move for a fear, in tenths of
    /// a per mille of a day of need…
    pub fear_tenths: u32,
    /// …and for a reward.
    pub reward_tenths: u32,
    /// The place a body leaves for may be worse for food, by its own
    /// notes, by this much at most, per mille of a day of need.
    pub worse_permille: u32,
    /// A trial is kept when the ills perceived in its season are at most
    /// this share of those of the season before, in percent.
    pub keep_percent: u32,
    /// Fewer nights than this away from the camp left in the trial's
    /// season: unjudged.
    pub judge_nights: u32,
    /// Each kind of cause (a name of [`CLUE_KINDS`]) and its lever (a
    /// name of [`LEVERS`]).
    pub levers: Vec<(String, String)>,
}

impl Acting {
    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            strength_kin_death: 100,
            strength_own: 60,
            strength_seen: 30,
            heard_percent: 50,
            fear_tenths: 50,
            reward_tenths: 25,
            worse_permille: 100,
            keep_percent: 50,
            judge_nights: 8,
            levers: [
                ("place", "leave"),
                ("season", "prepare"),
                ("weather", "prepare"),
                ("company", "keep_apart"),
                ("food", "avoid"),
                ("water", "drink_elsewhere"),
                ("gesture", "otherwise"),
            ]
            .iter()
            .map(|&(k, l)| (k.to_owned(), l.to_owned()))
            .collect(),
        }
    }

    /// The lever of the kind of cause numbered `kind` ([`CLUE_KINDS`]),
    /// its index in [`LEVERS`].
    #[must_use]
    pub fn lever_of(&self, kind: u8) -> Option<u8> {
        let name = CLUE_KINDS.get(usize::from(kind))?;
        let (_, lever) = self.levers.iter().find(|(k, _)| k == name)?;
        LEVERS
            .iter()
            .position(|l| l == lever)
            .and_then(|i| u8::try_from(i).ok())
    }

    /// A trial's length: a season, in game days.
    #[must_use]
    pub fn trial_days(days_per_cycle: u32) -> u32 {
        season_days(1, days_per_cycle).max(1)
    }

    fn validate(&self) -> Result<(), ContentError> {
        let levers = self.levers.iter().enumerate().all(|(i, (k, l))| {
            CLUE_KINDS.contains(&k.as_str())
                && LEVERS.contains(&l.as_str())
                && self.levers[..i].iter().all(|(o, _)| o != k)
        });
        if !levers
            || [
                self.strength_kin_death,
                self.strength_own,
                self.strength_seen,
            ]
            .iter()
            .any(|&s| s > 100)
            || self.heard_percent > 100
            || self.keep_percent > 100
            || self.fear_tenths > 1000
            || self.reward_tenths > 1000
        {
            return Err(ContentError::Needs(
                "learning.acting: strengths and heard_percent and keep_percent at most 100, \
                 fear_tenths and reward_tenths at most 1000, levers each a kind of CLUE_KINDS once \
                 with a lever of LEVERS",
            ));
        }
        Ok(())
    }
}

impl Learning {
    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        let pairs = |list: &[(&str, &str)]| {
            list.iter()
                .map(|&(a, b)| (a.to_owned(), b.to_owned()))
                .collect()
        };
        Self {
            ailments: vec![
                (crate::Ailment::Gut, "ill_gut".to_owned()),
                (crate::Ailment::Chest, "ill_chest".to_owned()),
            ],
            clues: pairs(&[
                ("camp", "place"),
                ("camp_age", "place"),
                ("season", "season"),
                ("crowd", "place"),
                ("cold_night", "weather"),
                ("sick_at_fire", "company"),
                ("ate", "food"),
                ("drank", "water"),
            ]),
            episodes_kept: 8,
            new_camp_nights: 4,
            old_camp_seasons: 1,
            crowd_some: 6,
            crowd_many: 16,
            sick_seen_nights: 2,
            notice_from: 3,
            notice_percent_by_will: vec![(0, 1), (20, 3), (50, 8), (100, 20)],
            child_times: 2,
            depth_by_will: vec![(0, 3), (20, 3), (50, 8)],
            all_but_one_from: 6,
            suspects_max: 2,
            counter_examples: 3,
            whole_seasons: 4,
            half_seasons: 4,
            told_per_night: 2,
            copy_credit: 50,
            child_years: 16,
            acting: Some(Acting::placeholders()),
        }
    }

    /// The problem refusal `ailment` is lived as, if it marks.
    #[must_use]
    pub fn problem_of(&self, ailment: crate::Ailment) -> Option<&str> {
        self.ailments
            .iter()
            .find(|(a, _)| *a == ailment)
            .map(|(_, p)| p.as_str())
    }

    /// Whether `refusal` names a problem an ailment is lived as.
    #[must_use]
    pub fn marks(&self, refusal: &str) -> bool {
        self.ailments.iter().any(|(_, p)| p == refusal)
    }

    /// The kind of the clue named `clue` (its index in [`CLUE_KINDS`]),
    /// if noted.
    #[must_use]
    pub fn kind_of(&self, clue: &str) -> Option<u8> {
        let (_, kind) = self.clues.iter().find(|(c, _)| c == clue)?;
        CLUE_KINDS
            .iter()
            .position(|k| k == kind)
            .and_then(|i| u8::try_from(i).ok())
    }

    /// The chance to notice, in millionths, for `will` with `episodes`
    /// kept, weighed `times`.
    #[must_use]
    pub fn notice_ppm(&self, will: u8, episodes: u32, times: u32) -> u64 {
        let percent = along(&self.notice_percent_by_will, u64::from(will) * 1000, 1);
        let kept = u64::from(self.episodes_kept.max(1));
        let fill = u64::from(episodes).min(kept);
        (percent * 10_000 * fill / kept * u64::from(times.max(1))).min(1_000_000)
    }

    /// The episodes `will` cross-checks.
    #[must_use]
    pub fn depth(&self, will: u8) -> u32 {
        u32::try_from(along(&self.depth_by_will, u64::from(will) * 1000, 1))
            .unwrap_or(u32::MAX)
            .max(1)
    }

    /// The nights after which a camp is old, in a cycle of
    /// `days_per_cycle` days.
    #[must_use]
    pub fn old_camp_nights(&self, days_per_cycle: u32) -> u32 {
        season_days(self.old_camp_seasons, days_per_cycle)
    }

    /// The days a belief counts whole, and in all, in a cycle of
    /// `days_per_cycle` days.
    #[must_use]
    pub fn belief_days(&self, days_per_cycle: u32) -> (u32, u32) {
        let whole = season_days(self.whole_seasons, days_per_cycle).max(1);
        (
            whole,
            whole + season_days(self.half_seasons, days_per_cycle),
        )
    }

    fn validate(&self) -> Result<(), ContentError> {
        if let Some(acting) = &self.acting {
            acting.validate()?;
        }
        let clues_known = self.clues.iter().enumerate().all(|(i, (c, k))| {
            CLUES.contains(&c.as_str())
                && CLUE_KINDS.contains(&k.as_str())
                && self.clues[..i].iter().all(|(o, _)| o != c)
        });
        let curve = |points: &[(u32, u32)], top: u32| {
            !points.is_empty()
                && points.windows(2).all(|w| w[0].0 < w[1].0)
                && points.iter().all(|&(x, y)| x <= 100 && y <= top)
        };
        if !clues_known
            || self.episodes_kept == 0
            || self.episodes_kept > 64
            || self.crowd_some > self.crowd_many
            || !curve(&self.notice_percent_by_will, 100)
            || !curve(&self.depth_by_will, 64)
            || self.suspects_max == 0
            || self.counter_examples == 0
            || !(1..=100).contains(&self.copy_credit)
            || self.told_per_night > 64
        {
            return Err(ContentError::Needs(
                "learning: clues of CLUES once each with a kind of CLUE_KINDS; episodes_kept 1 to 64; \
                 crowd_some at most crowd_many; notice_percent_by_will (to 100) and depth_by_will \
                 (to 64) curves over wills 0 to 100 rising; suspects_max and counter_examples at \
                 least 1; copy_credit 1 to 100; told_per_night at most 64",
            ));
        }
        Ok(())
    }
}

/// The gestures of the hands (D57; the project lead, 24 November 2026:
/// "C'est parfait"): picking up, cutting with what is in hand, folding,
/// putting a thing in another. No recipe: what comes of a gesture is computed from what the
/// materials are on the scales they all share (hardness, edge,
/// suppleness, toughness, form), never from what they are for (amended
/// 27 November 2026). D30 placeholders, named and explained in
/// `content/needs.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hands {
    /// The game minutes a cut or a fold takes.
    pub gesture_minutes: u32,
    /// How far around itself a body sees what it may try a gesture on, in
    /// tiles on each axis.
    pub near_tiles: u32,
    /// The gestures a body in a trial of its hands tries a day, at most.
    /// 0: nobody tries one, and the hands are never a trial.
    pub tries_per_day: u32,
    /// The suppleness from which a sheet folded takes a hollow; a
    /// stiffer one breaks.
    #[serde(default = "Hands::default_fold_suppleness")]
    pub fold_suppleness: u8,
    /// The grams of loose things a litre of hollow keeps together.
    #[serde(default = "Hands::default_loose_g_per_litre")]
    pub loose_g_per_litre: u32,
    /// The gestures that gave nothing, or broke the thing, a body
    /// remembers, at most (D57, amended 27 November 2026): the oldest
    /// goes when one more comes. 0: none remembered.
    #[serde(default = "Hands::default_failures_kept")]
    pub failures_kept: u32,
    /// What a body sees (the project lead, 25 November 2026: "on ne coupe
    /// pas avec une baie"): the edge from which a thing looks like it
    /// could cut.
    #[serde(default = "Hands::default_edge_seen")]
    pub edge_seen: u8,
    /// The suppleness from which a thing that is no sheet feels in hand
    /// like it could bend.
    #[serde(default = "Hands::default_supple_seen")]
    pub supple_seen: u8,
    /// What a standing tree is made of and how it answers a cut (D57,
    /// amended 27 November 2026, trees in parts). Absent: a tree gives
    /// its bark only, has no branch to take and never falls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trees: Option<TreeParts>,
}

/// A standing tree in parts (D57, amended 27 November 2026): a trunk of a
/// thickness, living branches, green wood under the bark. What the world
/// answers to a cut on a part is computed from these and from the scales
/// of the materials named; nothing here says what a tree is for.
/// Explained, with their sources, in `content/needs.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeParts {
    /// The living wood: the material of a branch cut off a standing tree,
    /// and the stuff of the trunk under the bark that an edge must work.
    pub wood: String,
    /// What falls from a trunk an edge bites.
    pub chips: String,
    /// What a trunk is once it lies.
    pub log: String,
    /// A grown tree's height over its trunk's thickness, for a kind that
    /// is not `big`.
    pub slender: u32,
    /// The same for a `big` kind, stouter for its height.
    pub slender_big: u32,
    /// How far a tree's thickness lies from its kind's, either way, in
    /// hundredths: fixed for each tree by the seed and where it stands.
    pub girth_spread_percent: u32,
    /// The square millimetres of a trunk's section one gesture cuts
    /// through, for each point of the tool's edge over the living wood's
    /// toughness, plus one.
    pub bite_mm2: u32,
    /// The grams of chips that fall for each square centimetre cut.
    pub chips_g_per_cm2: u32,
    /// A trunk's volume as hundredths of the cylinder of its thickness
    /// and its tree's height.
    pub stem_form_percent: u32,
    /// The mass of a cubic metre of living wood, in kilograms.
    pub wood_kg_per_m3: u32,
    /// The crown's branches as hundredths of the trunk's mass.
    pub crown_percent: u32,
    /// The branches of a fallen crown big enough to be things, at most.
    pub branches_most: u32,
    /// The height of trunk whose bark a standing body's hands lift, in
    /// millimetres (amended 28 November 2026): with the trunk's girth,
    /// how many pieces of its bark a tree gives before its wood is bare.
    pub bark_reach_mm: u32,
    /// The seasons a notch as wide as the trunk's whole section would
    /// take to close, nobody deepening it (amended 28 November 2026): a
    /// lesser notch closes in proportion.
    pub notch_closes_seasons: u32,
    /// The seasons a trunk stripped at hand height takes to wear all its
    /// bark again (amended 28 November 2026).
    pub bark_grows_seasons: u32,
}

impl TreeParts {
    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            wood: "green_branch".to_owned(),
            chips: "wood_chips".to_owned(),
            log: "log".to_owned(),
            slender: 80,
            slender_big: 40,
            girth_spread_percent: 25,
            bite_mm2: 330,
            chips_g_per_cm2: 5,
            stem_form_percent: 45,
            wood_kg_per_m3: 900,
            crown_percent: 20,
            branches_most: 8,
            bark_reach_mm: 1000,
            notch_closes_seasons: 8,
            bark_grows_seasons: 8,
        }
    }

    fn validate(&self) -> Result<(), ContentError> {
        if !(10..=200).contains(&self.slender)
            || !(10..=200).contains(&self.slender_big)
            || self.girth_spread_percent > 90
            || !(1..=1_000_000).contains(&self.bite_mm2)
            || self.chips_g_per_cm2 > 1000
            || !(1..=100).contains(&self.stem_form_percent)
            || !(100..=2000).contains(&self.wood_kg_per_m3)
            || self.crown_percent > 100
            || self.branches_most > 64
            || !(1..=10_000).contains(&self.bark_reach_mm)
            || !(1..=4000).contains(&self.notch_closes_seasons)
            || !(1..=4000).contains(&self.bark_grows_seasons)
        {
            return Err(ContentError::Needs(
                "hands.trees: slender and slender_big 10 to 200, girth_spread_percent at most 90, bite_mm2 1 to a million, chips_g_per_cm2 at most 1000, stem_form_percent 1 to 100, wood_kg_per_m3 100 to 2000, crown_percent at most 100, branches_most at most 64, bark_reach_mm 1 to 10000, notch_closes_seasons and bark_grows_seasons 1 to 4000",
            ));
        }
        Ok(())
    }

    /// The thickness of the trunk of a tree of `species`, in millimetres,
    /// `roll` being the tree's own number: its kind's height over its
    /// slenderness, the tree's share of the spread either way.
    #[must_use]
    // Not generation: the content gives heights in metres, read once.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::float_arithmetic,
        reason = "a height checked positive and under 100 m; float maths of the simulation, the same result on every run"
    )]
    pub fn trunk_mm(&self, species: &Species, roll: u64) -> u32 {
        let height_mm = (species.height_m * 1000.0).round().clamp(0.0, 100_000.0) as u64;
        let slender = u64::from(if species.big {
            self.slender_big
        } else {
            self.slender
        });
        let spread = u64::from(self.girth_spread_percent);
        let share = 100 - spread + roll % (2 * spread + 1);
        u32::try_from(height_mm * share / (slender.max(1) * 100))
            .unwrap_or(u32::MAX)
            .max(1)
    }

    /// The section of a trunk `trunk_mm` thick, in square millimetres:
    /// what a notch has to cut through for the tree to fall.
    #[must_use]
    pub fn section_mm2(trunk_mm: u32) -> u32 {
        // π / 4 as 355 / 452.
        let d = u64::from(trunk_mm);
        u32::try_from(d * d * 355 / 452).unwrap_or(u32::MAX).max(1)
    }

    /// The mass of the trunk of a tree of `species` `trunk_mm` thick, in
    /// grams: its section, its kind's height, the stem's form, the living
    /// wood's density.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::float_arithmetic,
        reason = "a height checked positive and under 100 m; float maths of the simulation, the same result on every run"
    )]
    pub fn trunk_g(&self, species: &Species, trunk_mm: u32) -> u32 {
        let height_mm = (species.height_m * 1000.0).round().clamp(0.0, 100_000.0) as u64;
        // mm³, then the form; a cubic metre is 10⁹ mm³, a kilogram 10³ g.
        let mm3 =
            u64::from(Self::section_mm2(trunk_mm)) * height_mm * u64::from(self.stem_form_percent)
                / 100;
        u32::try_from(mm3 * u64::from(self.wood_kg_per_m3) / 1_000_000)
            .unwrap_or(u32::MAX)
            .max(1)
    }

    /// The grams of chips a cut through `mm2` of a trunk lets fall.
    #[must_use]
    pub fn chips_g(&self, mm2: u32) -> u32 {
        u32::try_from(u64::from(mm2) * u64::from(self.chips_g_per_cm2) / 100).unwrap_or(u32::MAX)
    }

    /// How many pieces of `bark` a trunk `trunk_mm` thick wears where
    /// the hands reach (amended 28 November 2026): its girth times the
    /// height reached, over the area of one piece; one at least.
    #[must_use]
    pub fn bark_plates(&self, trunk_mm: u32, bark: &Material) -> u32 {
        // π as 355 / 113; a square decimetre is 10,000 mm².
        let girth_mm = u64::from(trunk_mm) * 355 / 113;
        let piece_mm2 = u64::from(bark.area_dm2.max(1)) * 10_000;
        u32::try_from(girth_mm * u64::from(self.bark_reach_mm) / piece_mm2)
            .unwrap_or(u32::MAX)
            .max(1)
    }

    /// The journées a notch as wide as the whole section takes to close,
    /// and a stripped trunk to wear its bark again, under a calendar of
    /// `days_per_cycle` journées a cycle: a journée each at least.
    #[must_use]
    pub fn heal_days(&self, days_per_cycle: u32) -> (u64, u64) {
        let days = |seasons| u64::from(season_days(seasons, days_per_cycle).max(1));
        (
            days(self.notch_closes_seasons),
            days(self.bark_grows_seasons),
        )
    }
}

impl Default for Hands {
    /// Off: nobody tries a gesture.
    fn default() -> Self {
        Self {
            gesture_minutes: 10,
            near_tiles: 4,
            tries_per_day: 0,
            ..Self::placeholders()
        }
    }
}

impl Hands {
    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            gesture_minutes: 10,
            near_tiles: 4,
            tries_per_day: 4,
            fold_suppleness: Self::default_fold_suppleness(),
            loose_g_per_litre: Self::default_loose_g_per_litre(),
            failures_kept: Self::default_failures_kept(),
            edge_seen: Self::default_edge_seen(),
            supple_seen: Self::default_supple_seen(),
            trees: None,
        }
    }

    fn default_fold_suppleness() -> u8 {
        5
    }

    fn default_loose_g_per_litre() -> u32 {
        500
    }

    fn default_failures_kept() -> u32 {
        16
    }

    fn default_edge_seen() -> u8 {
        3
    }

    fn default_supple_seen() -> u8 {
        5
    }

    /// The grams of loose things a hollow of `hollow_ml` keeps together.
    #[must_use]
    pub fn holds_g(&self, hollow_ml: u32) -> u32 {
        u32::try_from(u64::from(hollow_ml) * u64::from(self.loose_g_per_litre) / 1000)
            .unwrap_or(u32::MAX)
    }

    /// Whether anybody tries a gesture.
    #[must_use]
    pub fn on(&self) -> bool {
        self.tries_per_day > 0
    }

    fn validate(&self) -> Result<(), ContentError> {
        if !(1..=24 * 60).contains(&self.gesture_minutes) || !(1..=16).contains(&self.near_tiles) {
            return Err(ContentError::Needs(
                "hands: gesture_minutes 1 to a day, near_tiles 1 to 16",
            ));
        }
        if self.fold_suppleness > 10
            || self.edge_seen > 10
            || self.supple_seen > 10
            || !(1..=2000).contains(&self.loose_g_per_litre)
            || self.failures_kept > 64
        {
            return Err(ContentError::Needs(
                "hands: fold_suppleness, edge_seen and supple_seen 0 to 10, loose_g_per_litre 1 to 2000, failures_kept at most 64",
            ));
        }
        if let Some(trees) = &self.trees {
            trees.validate()?;
        }
        Ok(())
    }
}

/// The refusals of the world a body remembers as a problem (D50 and D57,
/// amended 26 November 2026; the project lead: "not a feeling added to
/// push them, rather a limitation that leads to thought: my arms are full,
/// I can't take more"). Each kind names a refusal of the world: `full` (a
/// gathering stopped with its arms full, the plant still bearing), and
/// the ailments felt (D59). Nothing names what would answer it: the label
/// that tied `full` to a property of the materials, read by no decision,
/// is gone (amended 27 November 2026). A lived
/// problem lets a willing body try its hands there and then, and that
/// trial is judged by the problem. No word for what would answer it. D30
/// placeholders, named and explained in `content/needs.ron`. Absent (no
/// kinds): nothing is remembered as a problem.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problems {
    /// The kinds remembered, each a refusal.
    pub kinds: Vec<ProblemKind>,
    /// A problem not lived again for this many seasons is forgotten.
    pub fades_seasons: u32,
    /// The things to think about a body keeps, at most (the project
    /// lead's design): a full list lets its oldest drop off.
    pub mind_slots: u32,
}

/// One kind of problem (D50, amended 26 November 2026).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemKind {
    /// The refusal of the world, in the simulation's own word: `full`
    /// (a gathering stopped with all it could carry, [`crate::WorkEnd::Full`]).
    pub refusal: String,
    /// The trials a body begins from this problem in a season, at most.
    pub trials_per_season: u32,
    /// A trial from this problem is given up when this many seasons
    /// passed since it began with the problem lived again and the limit
    /// unmoved (D09: written in seasons).
    pub gives_up_seasons: u32,
}

impl ProblemKind {
    /// An ailment felt (D59): lived by the body, and no trial begun from
    /// it in this first step.
    #[must_use]
    pub fn felt(refusal: &str) -> Self {
        Self {
            refusal: refusal.to_owned(),
            trials_per_season: 0,
            gives_up_seasons: 1,
        }
    }

    /// The days a trial from this problem has before it is given up, in
    /// days of a cycle of `days_per_cycle`, at least one.
    #[must_use]
    pub fn gives_up_days(&self, days_per_cycle: u32) -> u32 {
        season_days(self.gives_up_seasons, days_per_cycle).max(1)
    }
}

/// The refusals a problem may name (D50, amended 26 November 2026).
const PROBLEM_REFUSALS: [&str; 3] = ["full", "ill_gut", "ill_chest"];

impl Problems {
    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            kinds: vec![
                ProblemKind {
                    refusal: "full".to_owned(),
                    trials_per_season: 1,
                    gives_up_seasons: 1,
                },
                ProblemKind::felt("ill_gut"),
                ProblemKind::felt("ill_chest"),
            ],
            fades_seasons: 2,
            mind_slots: 3,
        }
    }

    /// Whether any problem is remembered.
    #[must_use]
    pub fn on(&self) -> bool {
        !self.kinds.is_empty()
    }

    /// The index of the kind named by `refusal`, if remembered.
    #[must_use]
    pub fn kind_of(&self, refusal: &str) -> Option<u8> {
        self.kinds
            .iter()
            .position(|k| k.refusal == refusal)
            .and_then(|i| u8::try_from(i).ok())
    }

    /// The kind at `index`.
    #[must_use]
    pub fn kind(&self, index: u8) -> Option<&ProblemKind> {
        self.kinds.get(usize::from(index))
    }

    /// The days a problem not lived again is kept, in days of a cycle of
    /// `days_per_cycle`.
    #[must_use]
    pub fn fade_days(&self, days_per_cycle: u32) -> u32 {
        season_days(self.fades_seasons, days_per_cycle).max(1)
    }

    fn validate(&self) -> Result<(), ContentError> {
        if !self.on() {
            return Ok(());
        }
        let linked = |k: &ProblemKind| PROBLEM_REFUSALS.contains(&k.refusal.as_str());
        let unique = self
            .kinds
            .iter()
            .enumerate()
            .all(|(i, k)| self.kinds[..i].iter().all(|o| o.refusal != k.refusal));
        if self.kinds.len() > usize::from(u8::MAX)
            || !unique
            || self.fades_seasons == 0
            || !(1..=255).contains(&self.mind_slots)
            || !self
                .kinds
                .iter()
                .all(|k| linked(k) && k.gives_up_seasons > 0)
        {
            return Err(ContentError::Needs(
                "problems: each refusal once, one the simulation knows \
                 (full, ill_gut, ill_chest), gives_up_seasons and fades_seasons at least 1, mind_slots 1 to 255",
            ));
        }
        Ok(())
    }
}

/// What a body's family weighs when it weighs a move called at the fire
/// (D56, amended 24 November 2026; the project lead: "un poids de la
/// famille à part de la compagnie… ça peut s'ajouter à la compagnie…
/// double effet sur une personne qui aime la compagnie et 0 sur une
/// personne qui n'aime pas la compagnie, mais la famille a quand même un
/// impact"; "s'il y a des enfants, il y a encore plus de poids"). Each
/// member it would leave behind, wherever it stands, costs it the same
/// whatever its sociability and however many others are around: a spouse,
/// and each of its children too young to weigh a move, which go where
/// their mother goes. It pulls, never forces: a gain worth more still
/// wins. In per mille of a day of need, D30 placeholders named and
/// explained in `content/needs.ron`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Family {
    /// What leaving its spouse behind costs, a day.
    pub spouse_permille: u32,
    /// What leaving each of its young children behind costs, a day.
    pub child_permille: u32,
}

impl Family {
    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            spouse_permille: 250,
            child_permille: 100,
        }
    }

    fn validate(&self) -> Result<(), ContentError> {
        if self.spouse_permille > 1000 || self.child_permille > 1000 {
            return Err(ContentError::Needs(
                "family: spouse_permille and child_permille 1000 at most (a day of need)",
            ));
        }
        Ok(())
    }
}

/// Moving home as a consequence, never a goal (D50, D53, D56, amended 24
/// November 2026; the project lead: "le déménagement est une conséquence,
/// pas une cause"). A move is called only by a body that feels it is not
/// going well here (the push: its recent days against its usual days of
/// the season) and that lived, itself or through its spouse, days clearly
/// better at a place than its camp-mates who stayed near home that same
/// day (the surprise); a huge surprise alone is enough. The tale of a
/// surprise at the fire makes some go and see, never move. Every value is
/// a D30 placeholder, named and explained in `content/needs.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Moving {
    /// How much more a day at a place must have given than the same day
    /// gave the camp-mates who stayed near home, in per mille of a day of
    /// need, to be a surprise. 0: nobody is ever surprised, nobody moves.
    pub surprise_permille: u32,
    /// A surprise this many times `surprise_permille` calls a move
    /// without any push.
    pub huge_times: u32,
    /// How much worse its recent days must be than its usual days of the
    /// season, in per mille of a day of need, for a body to feel pushed.
    pub push_permille: u32,
    /// What its recent days keep from one day to the next, per mille: a
    /// short memory.
    pub recent_keep_permille: u32,
    /// What its usual days of a season keep from one day of that season
    /// to the next, per mille: a long memory.
    pub usual_keep_permille: u32,
    /// What a spouse's day of surprise counts for in a body's own notes,
    /// in per cent of a day lived by itself.
    pub spouse_day_percent: u32,
    /// The chance, in per cent by will, that a listener who believes a
    /// surprise told at the fire goes and sees it the next day.
    pub see_percent_by_will: Vec<(u32, u32)>,
}

impl Default for Moving {
    /// Off: nobody is surprised, nobody moves home.
    fn default() -> Self {
        Self {
            surprise_permille: 0,
            huge_times: 4,
            push_permille: 100,
            recent_keep_permille: 700,
            usual_keep_permille: 950,
            spouse_day_percent: 100,
            see_percent_by_will: vec![(0, 0)],
        }
    }
}

impl Moving {
    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            surprise_permille: 250,
            huge_times: 4,
            push_permille: 100,
            recent_keep_permille: 700,
            usual_keep_permille: 950,
            spouse_day_percent: 80,
            see_percent_by_will: vec![(0, 20), (100, 80)],
        }
    }

    /// Whether anybody can be surprised, and so move home.
    #[must_use]
    pub fn on(&self) -> bool {
        self.surprise_permille > 0
    }

    /// The chance, in per cent, that a listener of `will` goes and sees a
    /// surprise it believes.
    #[must_use]
    pub fn see_percent(&self, will: u8) -> u64 {
        along(&self.see_percent_by_will, u64::from(will) * 1000, 1)
    }

    fn validate(&self) -> Result<(), ContentError> {
        let rising = self.see_percent_by_will.windows(2).all(|p| p[0].0 < p[1].0);
        if self.see_percent_by_will.is_empty()
            || !rising
            || self
                .see_percent_by_will
                .iter()
                .any(|p| p.0 > 100 || p.1 > 100)
            || self.recent_keep_permille > 1000
            || self.usual_keep_permille > 1000
            || self.spouse_day_percent > 100
            || self.huge_times == 0
        {
            return Err(ContentError::Needs(
                "moving: see_percent_by_will not empty, its wills rising, every value at most \
                 100; recent_keep_permille and usual_keep_permille at most 1000; \
                 spouse_day_percent at most 100; huge_times at least 1",
            ));
        }
        Ok(())
    }
}

/// Company, a sense of the notebook (D53 amended 24 November 2026; the
/// project lead's rules of that day): a need of the body felt only at
/// night at the fire, where anyone present counts and family weighs no
/// more. It saturates fast, and a night fully alone weighs at most a day
/// of need, for the most sociable. It grades places only and never gives
/// an order. Every value is a D30 placeholder, named and explained in
/// `content/needs.ron`; nothing is tuned to a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Company {
    /// What a night fully alone costs the most sociable body (sociability
    /// 100), in per mille of a day of need; a body of sociability `s`
    /// feels `s` per cent of it. 0: nobody feels company. At most 1000.
    pub alone_permille: u32,
    /// How many others at its fire are enough for the least sociable
    /// (sociability 0): more add nothing.
    pub enough_loner: u32,
    /// How many others are enough for the most sociable (100); between
    /// the two, in a straight line.
    pub enough_sociable: u32,
    /// How many companions a spouse counts as when a body weighs a move
    /// called at the fire (D56, the project lead, 24 November 2026): its
    /// absence is felt more, so a spouse who goes pulls the other without
    /// forcing it. Absent: 1, a spouse like anyone.
    #[serde(default = "one")]
    pub spouse_counts: u32,
}

impl Default for Company {
    /// Off: nobody feels company.
    fn default() -> Self {
        Self {
            alone_permille: 0,
            enough_loner: 2,
            enough_sociable: 6,
            spouse_counts: 1,
        }
    }
}

impl Company {
    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            alone_permille: 1000,
            enough_loner: 2,
            enough_sociable: 6,
            spouse_counts: 3,
        }
    }

    /// Whether anybody feels company.
    #[must_use]
    pub fn on(&self) -> bool {
        self.alone_permille > 0
    }

    /// What a night with `others` at its fire costs a body of sociability
    /// `sociable` (0 to 100), in per mille of a day of need: its whole
    /// need alone, nothing once enough are there, in proportion between.
    #[must_use]
    pub fn lacking_permille(&self, sociable: u8, others: u32) -> u64 {
        self.lacking_hundredths(sociable, u64::from(others) * 100)
    }

    /// [`Company::lacking_permille`] with the others counted in
    /// hundredths of a body: a spouse weighs several
    /// ([`Company::spouse_counts`]).
    #[must_use]
    pub fn lacking_hundredths(&self, sociable: u8, others: u64) -> u64 {
        let s = u64::from(sociable.min(100));
        let need = u64::from(self.alone_permille) * s / 100;
        // Enough, in hundredths of a body.
        let (lo, hi) = (
            u64::from(self.enough_loner) * 100,
            u64::from(self.enough_sociable) * 100,
        );
        let enough = if hi >= lo {
            lo + (hi - lo) * s / 100
        } else {
            lo - (lo - hi) * s / 100
        };
        if enough == 0 {
            return 0;
        }
        let short = enough.saturating_sub(others);
        need * short / enough
    }

    fn validate(&self) -> Result<(), ContentError> {
        if self.alone_permille > 1000 || (self.on() && self.enough_loner == 0) {
            return Err(ContentError::Needs(
                "company: alone_permille 1000 at most (a day of need); enough_loner at least 1",
            ));
        }
        if self.enough_loner > 1000 || self.enough_sociable > 1000 || self.spouse_counts > 1000 {
            return Err(ContentError::Needs(
                "company: enough and spouse_counts 1000 at most",
            ));
        }
        Ok(())
    }
}

/// The evening's decisions (D56, decided by the project lead on 23
/// November 2026): after the accounts and the lessons, each present says
/// what it will do tomorrow, in the order of its will, and the others
/// join or not by their own perception. Every value is a D30
/// placeholder, named and explained in `content/needs.ron`; nothing is
/// tuned to a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decisions {
    /// How many of its best places from the camp a present weighs when
    /// it speaks. 0: no decisions.
    pub choices: u32,
    /// How many trees and plants holding food one gatherer takes at a
    /// place: the room a place has is what holds there divided by it.
    pub plants_per_gatherer: u32,
    /// The most a group may hold, its caller with it.
    pub group_max: u32,
    /// A child younger than this, in years, takes its mother's decision.
    pub follows_mother_years: u32,
    /// The chance, in per cent, that a present goes looking tomorrow
    /// instead of to a place it knows, by its will: (will, per cent), the
    /// wills rising, straight lines between.
    pub look_percent_by_will: Vec<(u32, u32)>,
    /// How many legs one who goes looking walks out, each a sight
    /// farther than the last, before it gathers as any day.
    pub look_legs: u32,
    /// How many seasons of what it did a mind keeps (its habit), one
    /// byte a day: a long duration, written in seasons (D09); see
    /// [`Decisions::habit_days`].
    pub habit_seasons: u32,
    /// What each of those days weighs on going looking: a day it looked
    /// and found adds this many per cent of the chance, a day it looked
    /// for nothing takes it away.
    pub habit_weight: u32,
    /// The chance never falls under this per cent of itself.
    pub habit_floor: u32,
    /// The caller goes home with its group when this per cent of the
    /// group carries what it wanted.
    pub home_when_full_percent: u32,
    /// On its way the caller stops and waits while a member not yet there
    /// stands farther from it than this, in tiles on each axis ("look
    /// behind"). 0: it never looks behind.
    pub behind_tiles: u32,
    /// The most it waits so in a day, in game minutes (a 1,440th of the
    /// game day).
    pub behind_wait_minutes: u32,
    /// How many dated facts on its companions a caller keeps (D56, PR 1
    /// ter: the evening's review), the oldest forgotten first. 0: no
    /// review, anybody joins anybody.
    #[serde(default)]
    pub facts_kept: u32,
    /// The review judges a companion on its last outings with the caller
    /// since the caller last told it "not with us tomorrow": this many.
    #[serde(default)]
    pub review_outings: u32,
    /// Of them, this many bad ones (waited for on the way, or back with
    /// less than a day of its need) and it is told "not with us
    /// tomorrow"; this many good ones in a row (neither) and it is called
    /// first, by name.
    #[serde(default)]
    pub review_bad: u32,
    /// One told "not with us tomorrow" weighs first the places it knows
    /// within this many game minutes of walk from the camp (a game minute
    /// a 1,440th of the game day; 40 tiles on each axis until 26 November
    /// 2026, the brain in walking time): those set aside may make their
    /// own group near the camp.
    #[serde(default)]
    pub near_camp_minutes: u32,
    /// The parts of the review in force (lot 5b, a measure of its causes):
    /// `set_aside` (the bad told "not with us tomorrow"), `aside_costs`
    /// (that costs them a failure of their confidence in gathering),
    /// `names` (the good called first, by name, their room kept). The
    /// facts are noted whenever `facts_kept` is above 0. All three when
    /// absent: the review as decided.
    #[serde(default = "review_parts_all")]
    pub review_parts: Vec<String>,
    /// Whether the caller of a group glances behind without a thought
    /// (the audit of wakes, 26 November 2026, step 3): walking to the
    /// group's place it looks behind every few seconds and thinks only
    /// when it sees a member to wait for; waiting at dawn for its members,
    /// it thinks when the last it waits for wakes and comes within reach
    /// (or its hour of waiting is up), and a member waiting by its caller
    /// when the caller sets off. False, or absent: a thought every few
    /// seconds, the rule before.
    #[serde(default)]
    pub behind_glance: bool,
}

/// The parts of the review a caller may hold to ([`Decisions::review_parts`]).
pub const REVIEW_PARTS: [&str; 3] = ["set_aside", "aside_costs", "names"];

fn review_parts_all() -> Vec<String> {
    REVIEW_PARTS.iter().map(|&p| p.to_owned()).collect()
}

impl Default for Decisions {
    /// No decisions: each day is its own.
    fn default() -> Self {
        Self {
            choices: 0,
            plants_per_gatherer: 2,
            group_max: 6,
            follows_mother_years: 12,
            look_percent_by_will: vec![(0, 0)],
            look_legs: 3,
            habit_seasons: 4,
            habit_weight: 50,
            habit_floor: 10,
            home_when_full_percent: 50,
            behind_tiles: 0,
            behind_wait_minutes: 0,
            facts_kept: 0,
            review_outings: 0,
            review_bad: 0,
            near_camp_minutes: 0,
            review_parts: review_parts_all(),
            behind_glance: false,
        }
    }
}

impl Decisions {
    /// The days of what it did a mind keeps in a cycle of
    /// `days_per_cycle` days: its `habit_seasons` in whole days.
    #[must_use]
    pub fn habit_days(&self, days_per_cycle: u32) -> u32 {
        season_days(self.habit_seasons, days_per_cycle)
    }

    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            choices: 3,
            look_percent_by_will: vec![(0, 0), (100, 40)],
            behind_tiles: 24,
            behind_wait_minutes: 30,
            facts_kept: 32,
            review_outings: 3,
            review_bad: 2,
            near_camp_minutes: 3,
            ..Self::default()
        }
    }

    /// Whether anybody decides anything at the fire.
    #[must_use]
    pub fn on(&self) -> bool {
        self.choices > 0
    }

    /// Whether a caller reviews its companions at the fire (D56, PR 1 ter).
    #[must_use]
    pub fn reviews(&self) -> bool {
        self.on() && self.facts_kept > 0
    }

    /// Whether the review holds to its part `part` ([`REVIEW_PARTS`]).
    #[must_use]
    pub fn review_holds(&self, part: &str) -> bool {
        self.review_parts.iter().any(|p| p == part)
    }

    /// The chance, in per cent, that a body of `will` goes looking.
    #[must_use]
    pub fn look_percent(&self, will: u8) -> u64 {
        along(&self.look_percent_by_will, u64::from(will) * 1000, 1)
    }

    fn validate(&self) -> Result<(), ContentError> {
        let rising = self
            .look_percent_by_will
            .windows(2)
            .all(|p| p[0].0 < p[1].0);
        if self.look_percent_by_will.is_empty()
            || !rising
            || self.look_percent_by_will.iter().any(|p| p.1 > 100)
        {
            return Err(ContentError::Needs(
                "decisions: look_percent_by_will not empty, its wills rising, each 100 at most",
            ));
        }
        if self.on() && (self.plants_per_gatherer == 0 || self.group_max == 0) {
            return Err(ContentError::Needs(
                "decisions: plants_per_gatherer and group_max are 1 or more",
            ));
        }
        if self.habit_floor > 100 || self.home_when_full_percent > 100 {
            return Err(ContentError::Needs(
                "decisions: habit_floor and home_when_full_percent are per cent",
            ));
        }
        if self.reviews() && (self.review_bad == 0 || self.review_bad > self.review_outings) {
            return Err(ContentError::Needs(
                "decisions: with facts_kept, review_bad 1 to review_outings",
            ));
        }
        if self
            .review_parts
            .iter()
            .any(|p| !REVIEW_PARTS.contains(&p.as_str()))
        {
            return Err(ContentError::Needs(
                "decisions: review_parts among set_aside, aside_costs, names",
            ));
        }
        Ok(())
    }
}

/// The will tempered, and a confidence by activity (D50 amended, D56;
/// decided by the project lead on 23 November 2026): four rules, each a
/// perception of oneself, the same for the will (its trials) and for each
/// activity (its days). Every value is a D30 placeholder, named and
/// explained in `content/needs.ron`; nothing is tuned to a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Temperance {
    /// How many failures in a row cost nothing, by innate will: (will,
    /// failures), the wills rising, straight lines between.
    pub bears_by_innate: Vec<(u32, u32)>,
    /// What each failure past those costs, in thousandths of a point of
    /// will, times its rank past them: the first one this, the next twice
    /// this, and so on.
    pub failure_milli: u32,
    /// How often, in per cent, a trial is kept: the rarity a success is
    /// weighed by, so that average luck keeps the will where it is.
    pub kept_percent: u32,
    /// The share of the gap to the innate will a will or a confidence
    /// closes each season, in thousandths.
    pub relax_permille: u32,
    /// What one gains, in thousandths of a point, when another joins its
    /// call at the fire or copies a way it told.
    pub listened_milli: u32,
    /// The activities a confidence is held for, each with how often, in
    /// per cent, a day of it succeeds. Known: `explore` (a day looking:
    /// a place found and told new), `gather` (a day going to a place: a
    /// day of its need gathered), `stay` (a day it said nothing at the
    /// fire: none of its children under `follows_mother_years` hungry at
    /// night). Empty: no confidence by activity.
    pub activities: Vec<(String, u32)>,
    /// A will this high or higher is counted bold in the vivarium's page.
    pub bold_will: u32,
    /// A day of an activity that succeeds after successes in a row weighs
    /// more (D56, PR 1 ter: "reproducibility"): (successes in a row
    /// before it, per cent of its gain), rising, straight lines between,
    /// the last held beyond. Empty: every success weighs the same.
    #[serde(default)]
    pub streak_percent: Vec<(u32, u32)>,
    /// Whether a parent judges its day at the camp (`stay`) only by the
    /// children it sees (the audit of wakes, 26 November 2026, step 6):
    /// those at its camp at nightfall, with it; a child away is not
    /// judged. False, or absent: the hunger of all its children, wherever
    /// they are, the rule before.
    #[serde(default)]
    pub kin_seen: bool,
}

impl Default for Temperance {
    /// Off: the will moves by the trials' fixed steps.
    fn default() -> Self {
        Self {
            bears_by_innate: Vec::new(),
            failure_milli: 0,
            kept_percent: 25,
            relax_permille: 0,
            listened_milli: 0,
            activities: Vec::new(),
            bold_will: 40,
            streak_percent: Vec::new(),
            kin_seen: false,
        }
    }
}

/// The activities a confidence may be held for (D56), as the content
/// names them.
pub const ACTIVITIES: [&str; 4] = ["explore", "gather", "stay", "live"];

impl Temperance {
    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            bears_by_innate: vec![(0, 2), (100, 6)],
            failure_milli: 1000,
            kept_percent: 25,
            relax_permille: 500,
            listened_milli: 500,
            activities: vec![
                ("explore".to_owned(), 75),
                ("gather".to_owned(), 50),
                ("stay".to_owned(), 90),
            ],
            bold_will: 40,
            streak_percent: vec![(0, 100), (4, 200)],
            // The switch of the audit of wakes at the rule before.
            kin_seen: false,
        }
    }

    /// Whether the will is tempered.
    #[must_use]
    pub fn on(&self) -> bool {
        !self.bears_by_innate.is_empty()
    }

    /// How many failures in a row cost nothing to one of `innate` will.
    #[must_use]
    pub fn bears(&self, innate: u8) -> u32 {
        u32::try_from(along(&self.bears_by_innate, u64::from(innate) * 1000, 1)).unwrap_or(u32::MAX)
    }

    /// The index in [`Temperance::activities`] of the activity `name`.
    #[must_use]
    pub fn activity(&self, name: &str) -> Option<usize> {
        self.activities.iter().position(|(n, _)| n == name)
    }

    /// What a day of an activity that succeeds `percent` times in a
    /// hundred gains when it succeeds, in thousandths of a point (D56,
    /// amended): one failure's cost times the odds against the success,
    /// (100 − `percent`) / `percent`, so that a success is worth what it
    /// was against. The failures borne free cost nothing, so average luck
    /// gains a little; the seasons' return to the innate holds it.
    #[must_use]
    pub fn day_success_milli(&self, percent: u32) -> u64 {
        let percent = u64::from(percent.clamp(1, 100));
        u64::from(self.failure_milli) * (100 - percent) / percent
    }

    /// What a day's success gains after `streak` successes in a row of
    /// the same activity (D56, PR 1 ter): [`Temperance::day_success_milli`]
    /// times the content's `streak_percent` at that streak.
    #[must_use]
    pub fn streak_success_milli(&self, percent: u32, streak: u32) -> u64 {
        let base = self.day_success_milli(percent);
        if self.streak_percent.is_empty() {
            return base;
        }
        base * along(&self.streak_percent, u64::from(streak) * 1000, 1) / 100
    }

    /// What a success ending `failures` failures in a row gains, in
    /// thousandths of a point, to one who bears `bears` of them free, when
    /// a success comes `percent` times in a hundred: the gain grows with
    /// the streak it ends, and is such that average luck (a success that
    /// often) gains on average what its failures cost. Integer sums, the
    /// same everywhere (D24).
    #[must_use]
    pub fn success_milli(&self, bears: u32, failures: u32, percent: u32) -> u64 {
        const SCALE: u128 = 1_000_000_000;
        let success = u128::from(percent.clamp(1, 100)) * SCALE / 100;
        let failure = SCALE - success;
        let (free, cost) = (u128::from(bears), u128::from(self.failure_milli));
        // The cost a streak of failures runs to, weighed by its chance.
        let (mut chance, mut loss) = (SCALE, 0_u128);
        for streak in 0..2_000_u128 {
            if streak > free {
                let past = streak - free;
                loss += chance * success / SCALE * cost * past * (past + 1) / 2;
            }
            chance = chance * failure / SCALE;
            if chance == 0 {
                break;
            }
        }
        // A success ends a streak of n one time in 1/success: (n + 1)
        // units.
        let unit = loss / SCALE * success / SCALE;
        u64::try_from(unit * (u128::from(failures) + 1)).unwrap_or(u64::MAX)
    }

    fn validate(&self) -> Result<(), ContentError> {
        let rising = self.bears_by_innate.windows(2).all(|p| p[0].0 < p[1].0);
        if !rising
            || self.kept_percent == 0
            || self.kept_percent > 100
            || self.relax_permille > 1000
        {
            return Err(ContentError::Needs(
                "temperance: bears_by_innate's wills rising, kept_percent 1 to 100, relax_permille 1,000 at most",
            ));
        }
        if self
            .activities
            .iter()
            .any(|(n, p)| !ACTIVITIES.contains(&n.as_str()) || *p == 0 || *p > 100)
        {
            return Err(ContentError::Needs(
                "temperance: activities among explore, gather, stay, each succeeding 1 to 100 per cent",
            ));
        }
        if !self.streak_percent.windows(2).all(|p| p[0].0 < p[1].0) {
            return Err(ContentError::Needs(
                "temperance: streak_percent's streaks rising",
            ));
        }
        Ok(())
    }
}

/// Who is believed to know (D56, PR 1 ter, decided by the project lead
/// on 23 November 2026): "32 knows where berries are" is a record in the
/// common memory (D55), and each body holds, for each activity, only the
/// one it believes best there with its own credit in it. Experience and
/// the fire's accounts form it; followers back empty wear it; time fades
/// it. Never a title (D30). Every value is a D30 placeholder, named and
/// explained in `content/needs.ron`; nothing is tuned to a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reputation {
    /// The activities a reputation is held for, among the temperance's
    /// [`ACTIVITIES`]. Empty: nobody is reputed, nobody asks.
    pub activities: Vec<String>,
    /// What a day one followed another and came back with a day of its
    /// need adds to its credit in that one, in points (0 to 100).
    pub seen_gain: u32,
    /// What an account at the fire adds: a place new told by the one who
    /// found it, a group come home with most of it fed.
    pub heard_gain: u32,
    /// What a day one followed another and came back empty takes from it;
    /// half of it, heard at the fire that a whole group came back empty.
    pub wear: u32,
    /// What a credit loses each day since its record's day.
    pub fade_per_day: u32,
    /// The chance, in per cent of its credit, that one joins first the
    /// call of whom it believes best at gathering, when that call is among
    /// its choices.
    pub join_percent: u32,
    /// How many questions ("where is food?") are answered at one fire,
    /// apart from the accounts' slots. 0: nobody asks.
    pub questions_per_night: u32,
}

impl Default for Reputation {
    /// Off.
    fn default() -> Self {
        Self {
            activities: Vec::new(),
            seen_gain: 0,
            heard_gain: 0,
            wear: 0,
            fade_per_day: 0,
            join_percent: 0,
            questions_per_night: 0,
        }
    }
}

impl Reputation {
    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            activities: vec!["explore".to_owned(), "gather".to_owned(), "live".to_owned()],
            seen_gain: 20,
            heard_gain: 10,
            wear: 15,
            fade_per_day: 1,
            join_percent: 100,
            questions_per_night: 4,
        }
    }

    /// Whether anybody is reputed.
    #[must_use]
    pub fn on(&self) -> bool {
        !self.activities.is_empty()
    }

    /// Whether a reputation is held for the activity `known`, an index in
    /// [`ACTIVITIES`].
    #[must_use]
    pub fn holds(&self, known: usize) -> bool {
        ACTIVITIES
            .get(known)
            .is_some_and(|name| self.activities.iter().any(|a| a == name))
    }

    fn validate(&self) -> Result<(), ContentError> {
        if self
            .activities
            .iter()
            .any(|a| !ACTIVITIES.contains(&a.as_str()))
            || [self.seen_gain, self.heard_gain, self.wear]
                .iter()
                .any(|&g| g > 100)
        {
            return Err(ContentError::Needs(
                "reputation: activities among explore, gather, stay, live; gains and wear 100 at most",
            ));
        }
        Ok(())
    }
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
    /// Hungry from the brain's threshold (50), a body counts as fed again
    /// only under this many points of hunger: one does not re-decide a
    /// meal at every mouthful, nor think of hunger ninety times a day
    /// while it hovers at the threshold.
    pub fed_under: u32,
    /// How long, in game hours, a body sticks to the tree or plant it
    /// chose to gather from: chosen again only after that long, or when
    /// the plant is spent or out of reach, or another need calls. One
    /// looks at the pantry once in a while, not at every step.
    pub plant_choice_hours: u32,
    /// The side, in tiles, of a place a body remembers trees and plants
    /// bearing food by (a corner, not a bush): divides a chunk's side
    /// ([`crate::map::CHUNK_SIZE`]), so a place lies in one chunk.
    pub place_tiles: u32,
    /// How many such places a body remembers: those worth most from
    /// where it last looked.
    pub places_remembered: u32,
    /// How far, in tiles on each axis, a body gathers a **patch** (D53,
    /// amended 25 November 2026): the plants of the same crop around the
    /// one it chose, picked one after the other without walking away. A
    /// small plant is worth the walk when its patch holds ten minutes of
    /// work, not itself alone. 0, or absent: each plant weighed alone.
    #[serde(default)]
    pub patch_tiles: u32,
    /// Whether food laid at or taken from a hearth's reserve (D58) makes
    /// the bodies of that hearth at the camp think again, awake or asleep.
    /// False, or absent: nobody (D58, amended 26 November 2026 by the
    /// project lead: one goes and looks at the hearth when hungry; the
    /// food laid there wakes no one). True: everyone at the camp, as
    /// before (the vivarium's `--wake-all`).
    #[serde(default)]
    pub hearth_wakes: bool,
    /// Whether a gather the body's own new intention stopped, or its
    /// falling asleep (`WorkEnd::Interrupted`), makes it think again
    /// (the audit of wakes, 26 November 2026, step 1: the echo of an
    /// order). True, or absent: it does, the rule before. False: it does
    /// not; it has just decided, and falling asleep has its own threshold.
    #[serde(default = "yes")]
    pub wake_on_interrupt: bool,
    /// Whether a sleeper thinks when it falls asleep, at dawn and dusk,
    /// and at a gauge past a threshold or an event while it sleeps on
    /// (the audit of wakes, step 2). True, or absent: it does, the rule
    /// before. False: it does not; it thinks when it wakes, or when it is
    /// parched and not collapsed (it gets up to drink, D45).
    #[serde(default = "yes")]
    pub wake_asleep: bool,
}

/// True: a switch whose absence keeps the rule before.
fn yes() -> bool {
    true
}

impl Default for Thought {
    /// The placeholders of `content/needs.ron`, but the switches of the
    /// audit of wakes (26 November 2026) at the rule before.
    fn default() -> Self {
        Self {
            plant_sight_tiles: 30,
            fed_under: 30,
            plant_choice_hours: 1,
            place_tiles: 16,
            places_remembered: 48,
            patch_tiles: 4,
            hearth_wakes: false,
            wake_on_interrupt: true,
            wake_asleep: true,
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
        if self.fed_under > 100 {
            return Err(ContentError::Needs(
                "thought: fed_under is a point of a 0 to 100 gauge",
            ));
        }
        if self.plant_choice_hours == 0 {
            return Err(ContentError::Needs(
                "thought: plant_choice_hours is an hour or more",
            ));
        }
        if self.place_tiles == 0 || !CHUNK_SIZE.is_multiple_of(self.place_tiles) {
            return Err(ContentError::Needs(
                "thought: place_tiles is 1 tile or more and divides a chunk's side",
            ));
        }
        if self.places_remembered == 0 {
            return Err(ContentError::Needs(
                "thought: places_remembered is 1 or more",
            ));
        }
        if self.patch_tiles > self.place_tiles {
            return Err(ContentError::Needs(
                "thought: patch_tiles is place_tiles at most",
            ));
        }
        Ok(())
    }
}

/// The evening fire and the common memory (D55, lot 4 of the simulation,
/// 22 November 2026): at the camp, those who came home tell what changed
/// for them in the day (a place seen, a water found, a way judged, how
/// they fared), and each listener gains an access to the records told,
/// never the world. Every value is a D30 placeholder, named and
/// explained in `content/needs.ron`; nothing is tuned to a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fire {
    /// How many things a present body tells a night at most: its judged
    /// way, the water it found, then the places that changed for it, by
    /// their value from the camp. 0: no fire.
    pub told_per_night: u32,
    /// How many told things a grown listener keeps a night at most, and
    /// how many heard kept ways a mind holds.
    pub heard_max: u32,
    /// How many told things a listener keeps a night by its age: (years,
    /// count), the ages rising, straight lines between the points, the
    /// first held before and the last after; capped by `heard_max`. A
    /// child keeps fewer than a grown one.
    pub heard_by_age: Vec<(u32, u32)>,
    /// The three states of a kind at a place, by how many of its plants
    /// held food worth a step when looked at: bare under the first, some
    /// from the first to under the second, full from the second.
    pub place_buckets: (u32, u32),
    /// How many seasons of nights a camp's hearth keeps: what the tribe
    /// knows is the union of them. A long duration, written in seasons
    /// (D09); see [`Fire::hearth_nights`].
    pub hearth_seasons: u32,
    /// How many days a told state of a kind is worth believing for each
    /// hour of work one of its plants holds (its crop's `grams` of a usual
    /// year over `per_hour_g`, `crops.ron`): over that many days of the
    /// record's age a listener's credit loses its 50 points of age, never
    /// over more days than the crop has left in its season (lot 5a, D55
    /// amended). Leaves picked in minutes are old news by the next
    /// evening, nuts keep for weeks.
    pub told_days_per_hour: u32,
    /// The most days a told state is worth believing, whatever it bears.
    pub told_days_max: u32,
    /// How many denials a present body tells a night at most, besides its
    /// `told_per_night`: places it found bare where the record it held
    /// said food. A denial takes no slot and reaches only those who hold
    /// a record of the place (lot 5a, D55 amended).
    pub denied_per_night: u32,
}

impl Default for Fire {
    /// No fire: nobody tells nor hears anything.
    fn default() -> Self {
        Self {
            told_per_night: 0,
            heard_max: 0,
            heard_by_age: vec![(0, 0)],
            place_buckets: (1, 4),
            hearth_seasons: 0,
            told_days_per_hour: 4,
            told_days_max: 20,
            denied_per_night: 0,
        }
    }
}

impl Fire {
    /// The nights a camp's hearth keeps in a cycle of `days_per_cycle`
    /// days: its `hearth_seasons` in whole days, a night each.
    #[must_use]
    pub fn hearth_nights(&self, days_per_cycle: u32) -> u32 {
        season_days(self.hearth_seasons, days_per_cycle)
    }

    /// The placeholders of `content/needs.ron` (D30).
    #[must_use]
    pub fn placeholders() -> Self {
        Self {
            told_per_night: 6,
            heard_max: 24,
            heard_by_age: vec![(2, 6), (8, 12), (14, 20), (16, 24)],
            place_buckets: (1, 4),
            hearth_seasons: 4,
            told_days_per_hour: 4,
            told_days_max: 20,
            denied_per_night: 6,
        }
    }

    /// Whether anybody ever tells or hears anything.
    #[must_use]
    pub fn on(&self) -> bool {
        self.told_per_night > 0 && self.heard_max > 0
    }

    /// How many told things a listener `age_milli` thousandths of a year
    /// old keeps a night.
    #[must_use]
    pub fn heard_at(&self, age_milli: u64) -> u32 {
        let by_age = along(&self.heard_by_age, age_milli, 1);
        u32::try_from(by_age)
            .unwrap_or(u32::MAX)
            .min(self.heard_max)
    }

    /// The state of a kind at a place from how many of its plants `held`
    /// food: 0 bare, 1 some, 2 full.
    #[must_use]
    pub fn bucket(&self, held: u32) -> u8 {
        if held >= self.place_buckets.1 {
            2
        } else {
            u8::from(held >= self.place_buckets.0)
        }
    }

    /// How many plants holding food a state stands for, at the least:
    /// what a listener takes a told state to mean.
    #[must_use]
    pub fn held_of(&self, bucket: u8) -> u32 {
        match bucket {
            0 => 0,
            1 => self.place_buckets.0,
            _ => self.place_buckets.1,
        }
    }

    fn validate(&self) -> Result<(), ContentError> {
        let rising = self.heard_by_age.windows(2).all(|p| p[0].0 < p[1].0);
        if self.heard_by_age.is_empty() || !rising {
            return Err(ContentError::Needs(
                "fire: heard_by_age not empty, its ages rising",
            ));
        }
        if self.place_buckets.0 == 0 || self.place_buckets.0 >= self.place_buckets.1 {
            return Err(ContentError::Needs(
                "fire: place_buckets is (some from, full from), 1 or more and rising",
            ));
        }
        if self.on() && self.hearth_seasons == 0 {
            return Err(ContentError::Needs(
                "fire: hearth_seasons is a season or more when the fire is on",
            ));
        }
        if self.on() && (self.told_days_per_hour == 0 || self.told_days_max == 0) {
            return Err(ContentError::Needs(
                "fire: told_days_per_hour and told_days_max are a day or more when the fire is on",
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
    /// Days of distress in a row (hungry or thirsty at the day's end, or
    /// a day with no gather at all) before a body may start a trial.
    pub trial_after_days: u32,
    /// How many days a trial lasts, and how many days before it the
    /// judgement compares them with; also the days imitation looks back on.
    pub trial_days: u32,
    /// The daily chance to start a trial, in percent, by will: (will,
    /// percent), the wills rising, straight lines between the points, the
    /// first held before and the last after.
    pub try_percent_by_will: Vec<(u32, u32)>,
    /// The daily chance, in per mille, that a body of `will` with nothing
    /// wrong starts a trial all the same, a search for better (D50,
    /// amended 24 November 2026: the bold try when all is well): (will,
    /// per mille), as `try_percent_by_will`. None when absent.
    #[serde(default = "nobody_calm")]
    pub calm_try_permille_by_will: Vec<(u32, u32)>,
    /// The nights a body must have slept at a place before a trial may
    /// set its home there (D50, amended 23 November 2026: the home goes to
    /// a place it lived): (will, nights), the wills rising, straight lines
    /// between the points; the bold trust one night, the timid want more.
    #[serde(default = "one_night")]
    pub home_nights_by_will: Vec<(u32, u32)>,
    /// What its notes of a place keep from one day to the next, in per
    /// mille (D53, amended 23 November 2026): the old nights fade, so a
    /// place that stops doing it good loses its grade.
    #[serde(default = "permille_1000")]
    pub lived_keep_permille: u32,
    /// The side of a grove, in tiles (D53, amended 24 November 2026):
    /// gathering is noted over a grove, drinking and sleeping over a
    /// place of `thought.place_tiles`. 64 when absent.
    #[serde(default = "grove_64")]
    pub grove_tiles: u32,
    /// What a day of light is worth to a body that gathered nothing
    /// lately, in per mille of a day of its need (D53, amended 24 November
    /// 2026): the time it walks is valued at what it gathers in a day, and
    /// this when it has nothing of its own to go by. A day when absent.
    #[serde(default = "permille_1000")]
    pub idle_day_permille: u32,
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
    /// The innate will, drawn skewed (D50, amended 23 November 2026):
    /// (percentile, will), the percentiles rising from 0 to 100, straight
    /// lines between: most bodies timid, one or two adventurers in
    /// twenty-five. Empty: flat from 0 to 100, as before.
    #[serde(default)]
    pub will_innate: Vec<(u32, u32)>,
    /// What a kept trial adds to its body's will (acquired will), 0 to 100.
    #[serde(default)]
    pub will_step_kept: u32,
    /// What an abandoned trial takes from it.
    #[serde(default)]
    pub will_step_lost: u32,
    /// The lessons heard at the fire (D55): what a heard trial of the same
    /// change adds to that candidate's weight if kept, takes from it if
    /// abandoned, in per cent of the base weight of 100.
    #[serde(default)]
    pub lesson_weight: u32,
    /// A lesson older than this many seasons weighs half, older than
    /// twice it nothing: a long duration, written in seasons (D09); see
    /// [`Trials::lesson_days`].
    #[serde(default)]
    pub lesson_seasons: u32,
    /// The least weight a candidate keeps, in per cent of the base: a
    /// weight, never a ban.
    #[serde(default = "percent_100")]
    pub lesson_floor: u32,
    /// How much of the lessons a body heeds by its will, in per cent:
    /// (will, percent), the wills rising; the timid follow them whole, the
    /// adventurer in part.
    #[serde(default)]
    pub lesson_heed_by_will: Vec<(u32, u32)>,
    /// The ways judged on their own horizon rather than on `trial_days`
    /// (D50, amended on 23 November 2026, then counted in seasons the
    /// same day): (way, seasons). A way whose worth shows only over a
    /// season (a stock kept for the lean days, a night out or at the
    /// camp) is tried and judged on that many seasons, converted to days
    /// by the calendar ([`Trials::days_for`]), its own days and those
    /// heard, so that more trials no longer mean more blind draws and a
    /// longer year lengthens them with it. Ways: those of [`TRIAL_WAYS`];
    /// one not named is judged on `trial_days`, and never on fewer. A
    /// home is judged by its grade over its horizon (D50, amended 24
    /// November 2026).
    #[serde(default)]
    pub horizon_seasons: Vec<(String, u32)>,
    /// The seasons over which a move must repay its cost (D56, amended 24
    /// November 2026, the project lead: "il faut vraiment que le
    /// déménagement apporte quelque chose"): a move is worth it only if
    /// its gain a day passes the margin plus the walk there and a day of
    /// settling, spread over these seasons. One when absent.
    #[serde(default = "one")]
    pub move_repay_seasons: u32,
    /// A deed heard is tried more readily than an invention (D57, the
    /// project lead, 25 November 2026: hearing makes one think of it more
    /// often): a body holding a heard deed of the hands it does not know
    /// gets an extra daily chance to try it, its usual chance by its will
    /// times this factor, by the number of different tellers it heard the
    /// deed from: (tellers, factor), the factor of the greatest count not
    /// above. Empty: no extra chance.
    #[serde(default)]
    pub heard_factor_by_tellers: Vec<(u32, u32)>,
    /// The least daily chance, in per mille, that any body starts a trial
    /// of any way, calm or in distress, whatever its will (D50, amended 25
    /// November 2026, the project lead: "au lieu d'avoir 5 fois 0, on a 5
    /// fois 0,1. Donc je pense qu'il faut le garder en permanence pour
    /// tout"): a will of 0 tries too, rarely; the extra chance of a heard
    /// deed multiplies from it when the will gives less. 0 when absent.
    #[serde(default)]
    pub try_floor_permille: u32,
}

/// The ways a trial may move, as the content names them (D50).
pub const TRIAL_WAYS: [&str; 5] = [
    "worth_half_minutes",
    "sleep_out",
    "stock_kj",
    "home",
    "hands",
];

impl Default for Trials {
    /// Nobody tries anything nor copies anybody.
    fn default() -> Self {
        Self {
            trial_after_days: 0,
            trial_days: 1,
            try_percent_by_will: vec![(0, 0)],
            calm_try_permille_by_will: nobody_calm(),
            home_nights_by_will: one_night(),
            lived_keep_permille: 1000,
            grove_tiles: 64,
            idle_day_permille: 1000,
            imitate_tiles: 0,
            imitate_percent: 0,
            imitate_loss_percent: 0,
            will_innate: Vec::new(),
            will_step_kept: 0,
            will_step_lost: 0,
            lesson_weight: 0,
            lesson_seasons: 0,
            lesson_floor: 100,
            lesson_heed_by_will: Vec::new(),
            horizon_seasons: Vec::new(),
            move_repay_seasons: 1,
            heard_factor_by_tellers: Vec::new(),
            try_floor_permille: 0,
        }
    }
}

impl Trials {
    /// The extra chance's factor for a deed heard from `tellers`
    /// different tellers (D57): 0 when none applies.
    #[must_use]
    pub fn heard_factor(&self, tellers: u32) -> u64 {
        self.heard_factor_by_tellers
            .iter()
            .filter(|(n, _)| *n <= tellers)
            .max_by_key(|(n, _)| *n)
            .map_or(0, |(_, f)| u64::from(*f))
    }

    /// The innate will of a body from a draw `permille` (0 to 1,000): read
    /// on the `will_innate` table at that percentile; flat without one.
    #[must_use]
    pub fn will_at(&self, permille: u64) -> u8 {
        let permille = permille.min(1000);
        let will = if self.will_innate.is_empty() {
            permille * 100 / 1000
        } else {
            // The table's percentiles are years to `along`: a tenth of a
            // per mille is a thousandth of a percentile.
            along(&self.will_innate, permille * 100, 1)
        };
        u8::try_from(will.min(100)).expect("under 101")
    }

    /// How much of the lessons a body of `will` heeds, in per cent; the
    /// whole without a table.
    #[must_use]
    pub fn heed_percent(&self, will: u8) -> u64 {
        if self.lesson_heed_by_will.is_empty() {
            return 100;
        }
        along(&self.lesson_heed_by_will, u64::from(will) * 1000, 1)
    }

    /// The daily chance, in percent, that a body of `will` starts a trial
    /// once in distress long enough.
    #[must_use]
    pub fn try_percent(&self, will: u8) -> u64 {
        along(&self.try_percent_by_will, u64::from(will) * 1000, 1)
    }

    /// The daily chance, in per mille, that a body of `will` with nothing
    /// wrong starts a trial all the same.
    #[must_use]
    pub fn calm_try_permille(&self, will: u8) -> u64 {
        along(&self.calm_try_permille_by_will, u64::from(will) * 1000, 1)
    }

    /// Whether anybody ever tries or copies anything.
    #[must_use]
    pub fn on(&self) -> bool {
        self.try_percent_by_will.iter().any(|p| p.1 > 0)
            || self.calm_try_permille_by_will.iter().any(|p| p.1 > 0)
            || self.try_floor_permille > 0
            || self.imitate_percent > 0
    }

    fn validate(&self) -> Result<(), ContentError> {
        let rising = self.try_percent_by_will.windows(2).all(|p| p[0].0 < p[1].0);
        let percent = |v: u32| v <= 100;
        if self.try_percent_by_will.is_empty()
            || !rising
            || self.try_percent_by_will.iter().any(|p| !percent(p.1))
            || self.calm_try_permille_by_will.is_empty()
            || !self
                .calm_try_permille_by_will
                .windows(2)
                .all(|p| p[0].0 < p[1].0)
            || self.calm_try_permille_by_will.iter().any(|p| p.1 > 1000)
            || self.lived_keep_permille > 1000
            || !(1..=1024).contains(&self.grove_tiles)
            || self.home_nights_by_will.is_empty()
            || !self.home_nights_by_will.windows(2).all(|p| p[0].0 < p[1].0)
            || !percent(self.imitate_percent)
            || !percent(self.imitate_loss_percent)
            || self.trial_days == 0
        {
            return Err(ContentError::Needs(
                "trials: try_percent_by_will not empty, its wills rising and its percents at \
                 most 100; calm_try_permille_by_will likewise, at most 1000; home_nights_by_will not empty, its wills rising; \
                 lived_keep_permille at most 1000; grove_tiles 1 to 1024; imitate_percent and imitate_loss_percent at \
                 most 100; trial_days positive",
            ));
        }
        let rising = |t: &[(u32, u32)]| t.windows(2).all(|p| p[0].0 < p[1].0);
        if !rising(&self.will_innate)
            || self
                .will_innate
                .iter()
                .any(|p| !percent(p.0) || !percent(p.1))
            || !percent(self.will_step_kept)
            || !percent(self.will_step_lost)
            || !percent(self.lesson_floor)
            || !rising(&self.lesson_heed_by_will)
            || self
                .lesson_heed_by_will
                .iter()
                .any(|p| !percent(p.0) || !percent(p.1))
        {
            return Err(ContentError::Needs(
                "trials: will_innate and lesson_heed_by_will rising, every value at most 100; \
                 will_step_kept, will_step_lost and lesson_floor at most 100",
            ));
        }
        if self
            .horizon_seasons
            .iter()
            .any(|(way, seasons)| !TRIAL_WAYS.contains(&way.as_str()) || *seasons == 0)
        {
            return Err(ContentError::Needs(
                "trials: horizon_seasons names known ways, each with seasons",
            ));
        }
        Ok(())
    }

    /// How many days a trial of the way `key` lasts and is judged on: its
    /// horizon in seasons, a season being a quarter of `days_per_year`
    /// in whole days (`seasons × days_per_year / 4`: 12 days a season in
    /// a fifty-day year), never under `trial_days`.
    #[must_use]
    pub fn days_for(&self, key: &str, days_per_year: u32) -> u32 {
        self.horizon_seasons
            .iter()
            .find(|(way, _)| way == key)
            .map_or(self.trial_days, |&(_, seasons)| {
                season_days(seasons, days_per_year).max(self.trial_days)
            })
    }

    /// Whether a trial of the way `key` is short: judged on `trial_days`,
    /// not on a horizon in seasons. A short trial is judged within one
    /// season (D50, amended 25 November 2026).
    #[must_use]
    pub fn short(&self, key: &str) -> bool {
        !self.horizon_seasons.iter().any(|(way, _)| way == key)
    }

    /// The days a lesson weighs whole in a cycle of `days_per_cycle`
    /// days: its `lesson_seasons` in whole days.
    #[must_use]
    pub fn lesson_days(&self, days_per_cycle: u32) -> u32 {
        season_days(self.lesson_seasons, days_per_cycle)
    }

    /// The days over which a move must repay its cost, at least one.
    #[must_use]
    pub fn move_repay_days(&self, days_per_year: u32) -> u32 {
        season_days(self.move_repay_seasons, days_per_year).max(1)
    }

    /// The nights a body of `will` must have slept at a place before a
    /// trial may set its home there.
    #[must_use]
    pub fn home_nights(&self, will: u8) -> u32 {
        u32::try_from(along(&self.home_nights_by_will, u64::from(will) * 1000, 1))
            .unwrap_or(u32::MAX)
    }

    /// The longest of the trials' days: what a mind keeps of its days.
    #[must_use]
    pub fn longest_days(&self, days_per_year: u32) -> u32 {
        self.horizon_seasons
            .iter()
            .map(|&(_, seasons)| season_days(seasons, days_per_year))
            .fold(self.trial_days, u32::max)
    }
}

/// `seasons` seasons of a cycle of `days_per_year` days, in whole days
/// (rounded down): how every long duration written in seasons becomes
/// days under the world's calendar (D09).
fn season_days(seasons: u32, days_per_year: u32) -> u32 {
    let days = u64::from(seasons) * u64::from(days_per_year) / 4;
    u32::try_from(days).unwrap_or(u32::MAX)
}

/// Ages and sexes (lot 3 B, decided 17 November 2026): what a body needs
/// against a grown man, how fast it goes and what it carries, and the
/// infant carried and nursed until it is weaned. Ages in age-years
/// ([`crate::time::Calendar::days_per_age_year`]), never in cycles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ages {
    /// A body's need, its reserve and its strength by age, in percent of
    /// a grown man's: (years, percent), the ages rising, straight lines
    /// between them, the last held after.
    pub need_by_age: Vec<(u32, u32)>,
    /// A woman's, in percent of a man's of her age, from `woman_from_years`.
    pub woman_percent: u32,
    /// The age from which a woman needs `woman_percent`, and holds
    /// `woman_reserve_percent`.
    pub woman_from_years: u32,
    /// A woman's fat reserve, in percent of a man's of her age, from
    /// `woman_from_years` (D47, amended 25 November 2026: a woman carries
    /// more fat than a man, though she burns less).
    pub woman_reserve_percent: u32,
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
    /// One carrying an infant walks and gathers at this per cent of its
    /// pace (D56, decided by the project lead on 23 November 2026): a
    /// gesture of the body, never a rule of where to go. Absent: 100.
    #[serde(default = "percent_100")]
    pub carrying_pace_percent: u32,
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
            woman_reserve_percent: 100,
            nursing_percent: 100,
            weaned_years: 0,
            slow_under_years: 0,
            slow_over_years: u32::MAX,
            slow_pace_percent: 100,
            carrying_pace_percent: 100,
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
/// 2026; D49). Every value is a D30 placeholder. Ages in age-years; what
/// is counted in years follows the age-year (D09, amended 24 November
/// 2026), never the cycle of seasons; what is counted in days follows
/// the day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Life {
    /// A woman conceives from the first age to the last, both included.
    pub fertile_years: (u32, u32),
    /// Her spouse must be alive within this many tiles; a body takes a
    /// spouse within as many.
    pub mate_within_tiles: u32,
    /// The dawns over which her reserve's gain is read (D49, amended 25
    /// November 2026, fertility follows the gain): her reserve left now
    /// against its value this many dawns ago, or her oldest dawn kept when
    /// fewer have passed (none: flat).
    #[serde(default = "one")]
    pub conceive_gain_days: u32,
    /// Her chance to conceive in a quarter of an age-year, in percent, by
    /// her reserve's gain (D49, amended 25 November 2026): points (gain,
    /// chance per cent), the gain the mean a day over the last
    /// `conceive_gain_days` dawns in per cent of her day's need, which may
    /// be negative; rising, read between them, the first held below and the
    /// last above. A weak woman (`weak_percent`) never conceives. Empty:
    /// nobody conceives.
    #[serde(default)]
    pub conceive_percent_by_gain: Vec<(i32, u32)>,
    /// With at least this share of her reserve left, in per cent, she has
    /// the table's last chance whatever her gain: a full reserve cannot
    /// rise further. Over 100: never.
    #[serde(default = "percent_101")]
    pub conceive_full_percent: u32,
    /// The quarter's first nights she may conceive in (D49, amended): at
    /// the start of each of these days, a wife not yet with child whose
    /// spouse is back within `mate_within_tiles` meets the quarter's draw.
    /// A night apart no longer loses the quarter; 1 is the quarter's very
    /// start alone.
    #[serde(default = "one")]
    pub conceive_nights: u32,
    /// Spouses apart beyond `mate_within_tiles` at this many dawns in a
    /// row part (D49, amended 24 November 2026): both become free to take
    /// another, the children staying with their mother. 0: never.
    #[serde(default)]
    pub part_after_days: u32,
    /// Two unmarried become spouses once one has slept within
    /// `mate_within_tiles` of the other this many dawns in a row (D49,
    /// amended 24 November 2026): couples form over time, among those who
    /// live together. 0: at once, the nearest at the body's hour. No
    /// courtship, no wedding: nights lived side by side (the project lead,
    /// 24 November 2026; named `court_days` before).
    #[serde(default, alias = "court_days")]
    pub together_nights: u32,
    /// Two unmarried of one hearth wed at dawn, at once (D49, amended 25
    /// November 2026; the project lead, choice A): both homes in one
    /// camp's place (`thought.place_tiles`, the hearth's key, D55, D58),
    /// the seeker taking the one nearest in age, the lower id between
    /// equals; the bodies of no hearth, or none to wed at theirs, court as
    /// `together_nights` says. False: courting alone.
    #[serde(default)]
    pub wed_at_hearth: bool,
    /// A woman of a hearth left unwed at dawn, once its unmarried have
    /// wed (`wed_at_hearth`), becomes the second wife of a married man of
    /// that hearth, the nearest in age, one with a single wife, not her
    /// kin (D49, amended 25 November 2026; the project lead: "je valide la
    /// deuxième femme"). Her spouse is he; his stays his first wife. False:
    /// one wife each.
    #[serde(default)]
    pub second_wife: bool,
    /// A pregnancy lasts this many quarters of an age-year, in whole days
    /// (`quarters × days_per_age_year / 4`; D09, amended 24 November 2026:
    /// seasons while the age-year was the cycle).
    pub pregnancy_quarters: u32,
    /// A pregnant woman needs this much of her own need, in percent.
    pub pregnant_percent: u32,
    /// A weak woman (under `Needs::weak_percent`) loses the child.
    pub lose_when_weak: bool,
    /// The share of boys among the newborn, in percent.
    pub boys_percent: u32,
    /// The mother's chance to die giving birth, in percent.
    pub childbirth_death_percent: u32,
    /// The chance to die in an age-year by age, in thousandths: (age-years,
    /// per mille), the ages rising, straight lines between them, the last
    /// held after. Drawn every day with the age-year's chance divided by
    /// its days.
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
            mate_within_tiles: 0,
            conceive_gain_days: 1,
            conceive_percent_by_gain: Vec::new(),
            conceive_full_percent: 101,
            conceive_nights: 1,
            part_after_days: 0,
            together_nights: 0,
            wed_at_hearth: false,
            second_wife: false,
            pregnancy_quarters: 1,
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
    /// The chance to die within an age-year at `age_milli` thousandths of
    /// an age-year, in thousandths.
    #[must_use]
    pub fn death_permille_per_year(&self, age_milli: u64) -> u64 {
        along(&self.death_by_age, age_milli, 1)
    }

    /// The chance to die today, in millionths: the age-year's, shared out
    /// over its `days_per_age_year` days, so a longer age-year does not
    /// kill more an age-year.
    #[must_use]
    pub fn death_ppm_per_day(&self, age_milli: u64, days_per_age_year: u32) -> u64 {
        self.death_permille_per_year(age_milli) * 1000 / u64::from(days_per_age_year.max(1))
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
            || !self
                .conceive_percent_by_gain
                .windows(2)
                .all(|p| p[0].0 < p[1].0)
            || self.conceive_percent_by_gain.iter().any(|p| p.1 > 100)
            || self.conceive_gain_days == 0
            || !percent(self.boys_percent)
            || !percent(self.childbirth_death_percent)
            || self.pregnancy_quarters == 0
            || self.conceive_nights == 0
            || self.pregnant_percent < 100
        {
            return Err(ContentError::Needs(
                "life: death_by_age not empty, its ages rising and its per mille at most 1000; \
                 fertile_years and mate_years rising; the percents at most 100; \
                 conceive_percent_by_gain's gains rising; \
                 pregnancy_quarters, conceive_nights and conceive_gain_days positive, \
                 pregnant_percent at least 100",
            ));
        }
        Ok(())
    }
}

/// Illness as episodes (D49, amended 26 November 2026; the study
/// `etude-maladie-2026-11-26.md`, validated by the project lead): two
/// ailments, each a chance a day to fall ill, a few days ill, then an end
/// drawn by the body. Every value a D30 placeholder in `needs.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Illness {
    /// Serious episodes of the gut an age-year, by age (years, per mille),
    /// before the season and the exposure: straight lines between the
    /// points, as `death_by_age`.
    pub gut_by_age: Vec<(u32, u32)>,
    /// The same for the chest.
    pub chest_by_age: Vec<(u32, u32)>,
    /// The chance an episode kills a body with more than `thin_percent` of
    /// its reserve, by age (years, per mille), under the life rule's
    /// `old_age_from_years`; from it, the old age curve kills alone.
    pub death_by_age: Vec<(u32, u32)>,
    /// That chance for a thin body, in percent of the above.
    pub thin_death_percent: u32,
    /// That chance for a weak body, in percent of the above.
    pub weak_death_percent: u32,
    /// Days an episode of the gut lasts.
    pub gut_days: u32,
    /// Days an episode of the chest lasts.
    pub chest_days: u32,
    /// The gut's chance by season, in percent.
    pub gut_season_percent: BySeason<u32>,
    /// The chest's chance by season, in percent.
    pub chest_season_percent: BySeason<u32>,
    /// The fouling that doubles the gut's chance, in body-nights slept at
    /// a hearth's place.
    pub fouling_reference: u32,
    /// What a place keeps of its fouling from one dawn to the next, in
    /// thousandths.
    pub fouling_keep_permille: u32,
    /// The gut's chance of an infant nursed by a carrier not weak, in
    /// percent.
    pub nursed_gut_percent: u32,
    /// The hearth's size from which the chest's chance rises: each as many
    /// again doubles it once more.
    pub crowd_reference: u32,
    /// The chest's chance after a night whose cold reached
    /// `cold_points`, in percent.
    pub cold_chest_percent: u32,
    /// The cold at dawn, in points, that makes a cold night.
    pub cold_points: u32,
    /// The chance a day added by each of its hearth's sick of an ailment
    /// to fall ill of it, in per mille.
    pub contagion_permille: u32,
    /// The most the hearth's sick add, in per mille a day.
    pub contagion_max_permille: u32,
    /// Days a body healed of an ailment cannot fall ill of it again.
    pub immune_days: u32,
    /// A sick body's pace, walking and working, in percent of its own.
    pub sick_pace_percent: u32,
}

impl Illness {
    /// The serious episodes of `ailment` an age-year at `age_milli`
    /// thousandths of an age-year, in thousandths, before the season and
    /// the exposure.
    #[must_use]
    pub fn episodes_permille(&self, ailment: crate::Ailment, age_milli: u64) -> u64 {
        match ailment {
            crate::Ailment::Gut => along(&self.gut_by_age, age_milli, 1),
            crate::Ailment::Chest => along(&self.chest_by_age, age_milli, 1),
        }
    }

    /// The chance an episode kills a fat body at `age_milli`, in
    /// thousandths.
    #[must_use]
    pub fn death_permille(&self, age_milli: u64) -> u64 {
        along(&self.death_by_age, age_milli, 1)
    }

    /// Days an episode of `ailment` lasts, one at least.
    #[must_use]
    pub fn days(&self, ailment: crate::Ailment) -> u32 {
        match ailment {
            crate::Ailment::Gut => self.gut_days,
            crate::Ailment::Chest => self.chest_days,
        }
        .max(1)
    }

    /// The chance of `ailment` by season, in percent.
    #[must_use]
    pub fn season_percent(&self, ailment: crate::Ailment, season: Season) -> u32 {
        match ailment {
            crate::Ailment::Gut => self.gut_season_percent.get(season),
            crate::Ailment::Chest => self.chest_season_percent.get(season),
        }
    }

    fn validate(&self) -> Result<(), ContentError> {
        let table = |points: &[(u32, u32)]| {
            !points.is_empty()
                && points.windows(2).all(|p| p[0].0 < p[1].0)
                && points.iter().all(|p| p.1 <= 1000)
        };
        if !table(&self.gut_by_age)
            || !table(&self.chest_by_age)
            || !table(&self.death_by_age)
            || self.fouling_reference == 0
            || self.crowd_reference == 0
            || self.fouling_keep_permille >= 1000
            || self.contagion_max_permille > 1000
            || !(1..=100).contains(&self.sick_pace_percent)
            || self.gut_days == 0
            || self.chest_days == 0
        {
            return Err(ContentError::Needs(
                "illness: the three tables not empty, their ages rising and their per mille at most \
                 1000; fouling_reference and crowd_reference positive; fouling_keep_permille under \
                 1000; contagion_max_permille at most 1000; sick_pace_percent from 1 to 100; \
                 gut_days and chest_days positive",
            ));
        }
        Ok(())
    }
}

/// The value at `age_milli` thousandths of a year along `points`: (years,
/// value), the ages rising, each value scaled by `scale`, straight lines
/// between the points, the first held before and the last after.
impl Life {
    /// Her chance to conceive this quarter, in percent (D49, amended 25
    /// November 2026: fertility follows the gain): read on
    /// `conceive_percent_by_gain` at `gain`, her reserve's mean gain a day
    /// in per cent of her day's need; the table's last chance when `full`
    /// (at or over `conceive_full_percent`); 0 when no table is given. The
    /// weak are barred by the caller.
    #[must_use]
    pub fn conceive_chance(&self, gain: i64, full: bool) -> u64 {
        let points = &self.conceive_percent_by_gain;
        let (Some(first), Some(last)) = (points.first(), points.last()) else {
            return 0;
        };
        if full || gain >= i64::from(last.0) {
            return u64::from(last.1);
        }
        if gain <= i64::from(first.0) {
            return u64::from(first.1);
        }
        for pair in points.windows(2) {
            let ((a, va), (b, vb)) = (
                (i64::from(pair[0].0), i64::from(pair[0].1)),
                (i64::from(pair[1].0), i64::from(pair[1].1)),
            );
            if (a..b).contains(&gain) {
                let at = va + (vb - va) * (gain - a) / (b - a);
                return u64::try_from(at).unwrap_or(0);
            }
        }
        u64::from(last.1)
    }
}

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
        self.scaled_permille(age_milli, sex, self.woman_percent)
    }

    /// The fat reserve of a body `age_milli` thousandths of a year old, of
    /// `sex`, in thousandths of a grown man's: its size by age, and a
    /// woman's own share (D47, amended 25 November 2026).
    #[must_use]
    pub fn reserve_permille(&self, age_milli: u64, sex: Sex) -> u32 {
        self.scaled_permille(age_milli, sex, self.woman_reserve_percent)
    }

    fn scaled_permille(&self, age_milli: u64, sex: Sex, woman_percent: u32) -> u32 {
        let mut size = if self.need_by_age.is_empty() {
            1000
        } else {
            along(&self.need_by_age, age_milli, 10)
        };
        if sex == Sex::Female && age_milli >= u64::from(self.woman_from_years) * 1000 {
            size = size * u64::from(woman_percent) / 100;
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
            || self.woman_reserve_percent == 0
            || self.nursing_percent < 100
            || !(1..=100).contains(&self.slow_pace_percent)
            || !(1..=100).contains(&self.carrying_pace_percent)
            || self.infant_mass_g.0 > self.infant_mass_g.1
        {
            return Err(ContentError::Needs(
                "ages: need_by_age not empty, its ages rising and its percents positive; \
                 woman_percent and woman_reserve_percent positive, nursing_percent at least 100, slow_pace_percent and carrying_pace_percent 1 to 100, \
                 infant_mass_g rising",
            ));
        }
        Ok(())
    }
}

impl Needs {
    /// Checks the fat reserve's values (lot 3).
    /// The sections of the needs, each by its own rules.
    fn validate_sections(&self) -> Result<(), ContentError> {
        self.ages.validate()?;
        self.life.validate()?;
        self.trials.validate()?;
        self.thought.validate()?;
        self.fire.validate()?;
        self.decisions.validate()?;
        self.temperance.validate()?;
        self.reputation.validate()?;
        self.company.validate()?;
        self.moving.validate()?;
        self.family.validate()?;
        self.pooling.validate()?;
        self.trips.validate()?;
        self.problems.validate()?;
        self.passages.validate()?;
        if let Some(illness) = &self.illness {
            illness.validate()?;
        }
        if let Some(learning) = &self.learning {
            learning.validate()?;
        }
        if let Some(search) = &self.search {
            search.validate()?;
        }
        self.hands.validate()
    }

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
    /// The material a body cuts from its trunk with an edge in hand
    /// (D57); `None`: cutting it gives nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bark: Option<String>,
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

    /// Checks a material's keeping and wear are written in days or in
    /// seasons, never both (D09).
    fn validate_keeping(&self) -> Result<(), ContentError> {
        for material in &self.materials {
            let invalid = |what| ContentError::InvalidValue {
                id: material.id.clone(),
                what,
            };
            if material.wears_days > 0 && material.wears_seasons > 0 {
                return Err(invalid("wears in days or in seasons, not both"));
            }
            if material
                .food
                .is_some_and(|food| food.keeps_days > 0 && food.keeps_seasons > 0)
            {
                return Err(invalid("food keeps days or seasons, not both"));
            }
        }
        Ok(())
    }

    /// Checks what the gestures of the hands read (D57): the scales of
    /// a material, and that a tree's bark is a material.
    fn validate_gestures(&self, ids: &BTreeSet<&str>) -> Result<(), ContentError> {
        if self.materials.len() >= usize::from(crate::brain::HOLLOW) {
            return Err(ContentError::Needs("too many materials"));
        }
        if self.species.len() >= usize::from(crate::brain::BRANCH) {
            return Err(ContentError::Needs("too many species"));
        }
        if let Some(trees) = &self.needs.hands.trees
            && [&trees.wood, &trees.chips, &trees.log]
                .iter()
                .any(|id| !ids.contains(id.as_str()))
        {
            return Err(ContentError::Needs(
                "hands.trees: wood, chips and log name materials",
            ));
        }
        for material in &self.materials {
            let invalid = |what| ContentError::InvalidValue {
                id: material.id.clone(),
                what,
            };
            if material.edge > 10 || material.suppleness > 10 || material.toughness > 10 {
                return Err(invalid("edge, suppleness and toughness are 0 to 10"));
            }
            if (material.form == Form::Sheet) != (material.area_dm2 > 0) || material.area_dm2 > 400
            {
                return Err(invalid(
                    "a sheet has an area_dm2 of 1 to 400, nothing else has one",
                ));
            }
            if material.days_worn(4) > 0 && material.food.is_some() {
                return Err(invalid("food spoils by its keeping, it does not wear"));
            }
        }
        for species in &self.species {
            if species
                .bark
                .as_ref()
                .is_some_and(|bark| !ids.contains(bark.as_str()))
            {
                return Err(ContentError::Species {
                    id: species.id.clone(),
                    what: "bark names no material",
                });
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
        self.validate_gestures(&ids)?;
        self.validate_keeping()?;
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
        if Season::ALL.iter().any(|s| {
            !(1..=1000).contains(&self.needs.thirst_heat_percent.get(*s))
                || self.needs.cold_season_percent.get(*s) > 1000
        }) || !(1..=1000).contains(&self.needs.thirst_effort_percent)
            || self.needs.walk_j_per_m > 10_000
        {
            return Err(ContentError::Needs(
                "thirst_heat_percent and thirst_effort_percent are 1 to 1000, cold_season_percent \
                 at most 1000, walk_j_per_m at most 10,000",
            ));
        }
        if self.needs.kj_per_hunger_point == 0 {
            return Err(ContentError::Needs("kj_per_hunger_point must be positive"));
        }
        if self.needs.meal_g == 0 {
            return Err(ContentError::Needs("meal_g must be positive"));
        }
        self.needs.validate_reserve()?;
        self.needs.validate_sections()
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
            ("ages.woman_reserve_percent", ages.woman_reserve_percent),
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
            ("life.pregnancy_quarters", life.pregnancy_quarters),
            ("life.part_after_days", life.part_after_days),
            ("life.together_nights", life.together_nights),
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
        for (field, value) in [
            (
                "thought.plant_sight_tiles",
                self.needs.thought.plant_sight_tiles,
            ),
            (
                "thought.plant_choice_hours",
                self.needs.thought.plant_choice_hours,
            ),
            (
                "thought.places_remembered",
                self.needs.thought.places_remembered,
            ),
        ] {
            at_most("needs.ron", field, value, THOUSAND)?;
        }
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
                at_most(
                    "materials.ron",
                    field("food.keeps_seasons"),
                    food.keeps_seasons,
                    THOUSAND,
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
            if let Some(richness) = crop.richness_permille {
                at_most("crops.ron", "richness_permille", richness, MILLION)?;
            }
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
        if !(1..=self.movement.carry_capacity_g).contains(&self.movement.hands_g) {
            return Err(ContentError::Movement("hands_g is 1 to carry_capacity_g"));
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

    /// What a cut of a part of a tree of `species` gives with a piece of
    /// `tool` in hand (D57, amended 27 November 2026), the tree being as
    /// the seed made it. On the trunk: the tree's bark, when it has one
    /// that comes away and the tool works it loose ([`Material::works`]);
    /// on a tree with no such bark, the chips of the living wood, when
    /// the tool bites it ([`Content::bite_mm2`]). The wood under a bark
    /// is cut only where the bark is gone (amended 28 November 2026):
    /// that is the tree's state, the world's to know
    /// ([`crate::World::bark_left`]). On a branch: the branch, of living
    /// wood, when the tool works that. Nothing otherwise.
    #[must_use]
    pub fn cut_gives(&self, tool: &Material, species: &Species, part: Part) -> Option<&Material> {
        let trees = self.needs.hands.trees.as_ref();
        match part {
            Part::Trunk => {
                if let Some(bark) = species.bark.as_deref().and_then(|b| self.material(b)) {
                    return tool.works(bark).then_some(bark);
                }
                self.bite_mm2(tool)?;
                self.material(&trees?.chips)
            }
            Part::Branch => self.material(&trees?.wood).filter(|wood| tool.works(wood)),
        }
    }

    /// The square millimetres of a living trunk's section one gesture
    /// with a piece of `tool` in hand cuts through (D57, amended 27
    /// November 2026): nothing when the tool does not work the living
    /// wood; else the content's bite for each point of its edge over the
    /// wood's toughness, plus one.
    #[must_use]
    pub fn bite_mm2(&self, tool: &Material) -> Option<u32> {
        let trees = self.needs.hands.trees.as_ref()?;
        let wood = self.material(&trees.wood)?;
        tool.works(wood).then(|| {
            trees
                .bite_mm2
                .saturating_mul(u32::from(tool.edge - wood.toughness) + 1)
        })
    }

    /// The material of the piece a blow of `with` on `on` breaks off
    /// ([`Material::struck_with`]), whichever of the two breaks.
    #[must_use]
    pub fn strike_gives(&self, on: &Material, with: &Material) -> Option<&Material> {
        let broken = if on.struck_with(with, self.needs.hands.fold_suppleness)? {
            on
        } else {
            with
        };
        self.material(broken.breaks_into.as_deref().unwrap_or(&broken.id))
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
    hands_g: 3000,
    speed_percent_at_full_load: 60,
    sight_tiles: { grass: 60, forest: 25, ford: 60, brook: 60, water: 60, rock: 60 },
    path_budget_tiles: 1500,
    search_work_per_tick: 60000,
    player_share_percent: 50,
    skirts_per_order: 6,
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

    /// Company (D53, amended 24 November 2026): a night fully alone costs
    /// at most a day of need, for the most sociable, less as sociability
    /// falls; it saturates at about two others for a loner and six for a
    /// sociable, and more add nothing; off, nothing.
    #[test]
    fn her_chance_follows_her_reserves_gain_and_a_full_reserve_has_the_best() {
        let life = Life {
            conceive_percent_by_gain: vec![(-10, 1), (0, 3), (10, 15), (20, 40)],
            conceive_full_percent: 95,
            ..Life::default()
        };
        assert_eq!(life.conceive_chance(-50, false), 1, "falling fast");
        assert_eq!(life.conceive_chance(-10, false), 1);
        assert_eq!(
            life.conceive_chance(-5, false),
            2,
            "read between the points"
        );
        assert_eq!(life.conceive_chance(0, false), 3, "flat");
        assert_eq!(life.conceive_chance(5, false), 9);
        assert_eq!(life.conceive_chance(10, false), 15);
        assert_eq!(life.conceive_chance(20, false), 40, "a good gain");
        assert_eq!(life.conceive_chance(45, false), 40, "the last held above");
        assert_eq!(life.conceive_chance(-50, true), 40, "a full reserve");
        assert_eq!(Life::default().conceive_chance(30, true), 0, "no table");
        let mut good = Life {
            death_by_age: vec![(0, 10)],
            ..life
        };
        assert!(good.validate().is_ok());
        good.conceive_percent_by_gain = vec![(0, 3), (0, 15)];
        assert!(good.validate().is_err(), "gains rising");
        good.conceive_percent_by_gain = vec![(0, 101)];
        assert!(good.validate().is_err(), "a chance at most 100");
        good.conceive_percent_by_gain = vec![(0, 3)];
        good.conceive_gain_days = 0;
        assert!(good.validate().is_err(), "a window of a dawn at least");
    }

    #[test]
    fn company_saturates_and_a_night_alone_weighs_a_day_at_most() {
        let c = Company::placeholders();
        assert_eq!(c.lacking_permille(100, 0), 1000, "alone: a day of need");
        assert_eq!(c.lacking_permille(50, 0), 500);
        assert_eq!(c.lacking_permille(0, 0), 0, "a loner never lacks");
        // The most sociable: six are enough, more add nothing.
        assert_eq!(c.lacking_permille(100, 3), 500);
        assert_eq!(c.lacking_permille(100, 6), 0);
        assert_eq!(c.lacking_permille(100, 40), 0);
        // Half: four are enough.
        assert_eq!(c.lacking_permille(50, 2), 250);
        assert_eq!(c.lacking_permille(50, 4), 0);
        // A loner of 10: a little over two are enough.
        assert!(c.lacking_permille(10, 2) > 0);
        assert_eq!(c.lacking_permille(10, 3), 0);
        for others in 0..8 {
            for s in 0..100 {
                assert!(c.lacking_permille(s, others) <= c.lacking_permille(s + 1, others));
                assert!(c.lacking_permille(s, others + 1) <= c.lacking_permille(s, others));
            }
        }
        let off = Company::default();
        assert!(!off.on());
        assert_eq!(off.lacking_permille(100, 0), 0);
        // More than a day of need is refused.
        let too_much = Company {
            alone_permille: 1001,
            ..Company::placeholders()
        };
        assert!(too_much.validate().is_err());
        assert!(c.validate().is_ok());
    }

    /// Moving as a consequence (amended 24 November 2026): off by
    /// default; the placeholders on; a spouse's day never more than one's
    /// own; the chance to go and see rising with the will.
    #[test]
    fn moving_is_off_without_its_section_and_its_placeholders_hold() {
        let off = Moving::default();
        assert!(!off.on());
        assert!(off.validate().is_ok());
        let m = Moving::placeholders();
        assert!(m.on() && m.validate().is_ok());
        assert_eq!((m.see_percent(0), m.see_percent(100)), (20, 80));
        assert!(m.see_percent(50) > 20 && m.see_percent(50) < 80);
        let too_much = Moving {
            spouse_day_percent: 101,
            ..Moving::placeholders()
        };
        assert!(too_much.validate().is_err());
    }

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
        let empty = SAMPLE_MOVEMENT.replacen("hands_g: 3000", "hands_g: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &empty, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("hands_g is 1 to carry_capacity_g")
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
    fn a_slow_way_is_tried_on_its_horizon_in_seasons_and_never_under_trial_days() {
        let mut trials = Trials {
            trial_days: 5,
            horizon_seasons: vec![("stock_kj".to_owned(), 1), ("sleep_out".to_owned(), 2)],
            ..Trials::default()
        };
        assert_eq!(trials.days_for("stock_kj", 50), 12, "a season of 50 days");
        assert_eq!(trials.days_for("stock_kj", 365), 91, "the year sets it");
        assert_eq!(trials.days_for("sleep_out", 50), 25);
        assert_eq!(trials.days_for("stock_kj", 8), 5, "never under trial_days");
        assert_eq!(
            trials.days_for("worth_half_minutes", 50),
            5,
            "not named: trial_days"
        );
        assert_eq!(
            trials.days_for("home", 50),
            5,
            "a home tried alone: a short stay, not a season"
        );
        assert_eq!(trials.longest_days(50), 25);
        assert_eq!(trials.longest_days(8), 5);
        assert!(trials.validate().is_ok());
        trials.horizon_seasons.push(("hunting".to_owned(), 1));
        assert!(trials.validate().is_err(), "an unknown way");
        trials.horizon_seasons = vec![("stock_kj".to_owned(), 0)];
        assert!(trials.validate().is_err(), "no seasons");
    }

    #[test]
    fn the_young_first_age_is_at_least_the_fed_age() {
        let mut pooling = Pooling::placeholders();
        assert_eq!(pooling.young_first_until_years, 12);
        assert!(pooling.validate().is_ok());
        assert!(pooling.young_first(11_999) && !pooling.young_first(12_000));
        pooling.young_first_until_years = 6;
        assert!(pooling.validate().is_err(), "younger than the fed children");
        pooling.young_first_until_years = 101;
        assert!(pooling.validate().is_err(), "past a hundred");
        // Nothing pooled: the field is not read.
        pooling.fed_until_years = 0;
        pooling.young_first_until_years = 0;
        assert!(pooling.validate().is_ok());
    }

    /// A long duration written in seasons is its seasons in whole days of
    /// the world's cycle (D09): calendar v1's 16 days a season, and it
    /// follows a cycle tuned otherwise; one in days stays its days.
    #[test]
    fn a_duration_in_seasons_follows_the_cycle_and_one_in_days_does_not() {
        let nuts = Food {
            energy_kj_per_kg: 11_500,
            keeps_days: 0,
            keeps_seasons: 4,
        };
        let berries = Food {
            keeps_days: 3,
            keeps_seasons: 0,
            ..nuts
        };
        assert_eq!(nuts.days_kept(64), 64);
        assert_eq!(nuts.days_kept(50), 50);
        assert_eq!(
            Food {
                keeps_seasons: 6,
                ..nuts
            }
            .days_kept(64),
            96
        );
        assert_eq!(berries.days_kept(64), 3);
        assert_eq!(berries.days_kept(50), 3);
        let calendar = Calendar::new(1, 7200).unwrap();
        assert_eq!(nuts.ticks_kept(&calendar), 64 * 7200);
        let fire = Fire {
            hearth_seasons: 1,
            ..Fire::placeholders()
        };
        assert_eq!(fire.hearth_nights(64), 16);
        assert_eq!(Fire::placeholders().hearth_nights(64), 64);
        let mut trials = Trials::default();
        assert_eq!(trials.lesson_days(64), 0, "no lessons: none kept");
        trials.lesson_seasons = 4;
        assert_eq!(trials.lesson_days(64), 64);
        let decisions = Decisions::placeholders();
        assert_eq!(decisions.habit_days(64), 64);
        assert_eq!(decisions.habit_days(50), 50);
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
        // A keeping is written in days or in seasons, never both (D09).
        let both = SAMPLE_MATERIALS.replacen("keeps_days: 3", "keeps_days: 3, keeps_seasons: 4", 1);
        assert!(matches!(
            Content::from_ron(&both, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::InvalidValue {
                what: "food keeps days or seasons, not both",
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
        // A bark names a material (D57).
        let barked = r#"[(id: "oak", name: "Oak", height_m: 4.0, habitats: { heart: 1 }, bark: Some("gold"))]"#;
        assert!(matches!(
            base().with_species(barked).unwrap_err(),
            ContentError::Species {
                what: "bark names no material",
                ..
            }
        ));
        // The repository's own file is valid, with its own materials.
        Content::from_ron(
            include_str!("../../../content/materials.ron"),
            SAMPLE_SPAWNS,
            SAMPLE_MOVEMENT,
            SAMPLE_NEEDS,
        )
        .unwrap()
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

    #[test]
    fn the_problems_name_refusals_the_simulation_knows_and_no_property() {
        let repo = crate::crops::tests::content();
        assert_eq!(repo.needs.problems, Problems::placeholders());
        assert_eq!(repo.needs.problems.kind_of("full"), Some(0));
        assert!(!Problems::default().on() && Problems::default().validate().is_ok());
        let mut p = Problems::placeholders();
        assert!(p.validate().is_ok());
        p.kinds[0].refusal = "water".to_owned();
        assert!(p.validate().is_err(), "no such refusal");
        // The label that named what answers `full` is gone (D57, amended
        // 27 November 2026): nothing in the content says it.
        let needs = include_str!("../../../content/needs.ron");
        assert!(!needs.contains("property:") && !needs.contains("holds_g"));
        let mut p = Problems::placeholders();
        p.kinds.push(p.kinds[0].clone());
        assert!(p.validate().is_err(), "each refusal once");
        let mut p = Problems::placeholders();
        p.kinds[0].gives_up_seasons = 0;
        assert!(p.validate().is_err());
        let mut p = Problems::placeholders();
        p.fades_seasons = 0;
        assert!(p.validate().is_err());
        let mut p = Problems::placeholders();
        p.mind_slots = 0;
        assert!(p.validate().is_err());
    }
}
