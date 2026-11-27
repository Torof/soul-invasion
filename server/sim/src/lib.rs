//! Pure world simulation for Soul Invasion.
//!
//! This crate has no I/O, no clock and no network (architecture decision D24).
//! The server feeds it intentions and asks it to advance a number of fixed
//! ticks; it returns the events that happened. Same inputs, same outputs.

// World generation is integer-only, so that a seed gives the same world on
// every machine and every build (D24): float arithmetic is linted in the
// whole crate (an error under `-D warnings`), and `clippy.toml` forbids the
// float maths functions. New code is covered by default. Code that is not
// generation (moving bodies, the time of day, content units) and the known
// exceptions of today's generator allow floats where they stand.
#![warn(clippy::float_arithmetic)]

pub mod ahead;
pub mod brain;
pub mod changes;
pub mod content;
pub mod crops;
pub mod flora;
pub mod geology;
pub mod hands;
pub mod illness;
// `gen` is a reserved keyword in the 2024 edition: the module is `r#gen`,
// its file `gen.rs`.
pub(crate) mod r#gen;
pub mod ground;
pub mod invariants;
pub mod items;
pub mod land;
pub mod life;
pub mod lru;
pub mod map;
pub mod memory;
pub mod open_ground;
pub mod plan;
pub mod relief;
pub mod time;
pub mod trees;
pub mod woods;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

pub use content::{
    Ages, BySeason, Content, ContentError, Food, Life, Look, Material, MaterialKind, Movement,
    Needs, Sex, Spawn,
};
pub use geology::{Geology, Rock};
pub use illness::{Ailment, HearthCount, Immunity, Sickness};
pub use life::Pregnancy;
pub use map::{
    CHUNK_SIZE, ChunkPos, FinePos, MM_PER_TILE, Map, MapSpec, Pos, Side, Terrain, line_clear,
};
pub use open_ground::OpenGround;
pub use time::{Calendar, CalendarError, GameTime, Season, Tick};

/// Identifies a body (an inhabitant). Players and NPCs are the same kind
/// of body; only the pilot differs (D13).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct BodyId(pub u32);

/// Scale of a gauge: points are kept in millionths so that a rate per
/// day divides into whole steps per tick, deterministically.
const GAUGE_SCALE: u64 = 1_000_000;

/// The top of every gauge, in points.
pub const GAUGE_MAX: u32 = 100;

/// A 0–100 gauge, kept finely (D34).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Gauge(u64);

impl Gauge {
    /// A gauge at `points` (0–100).
    pub fn at(points: u32) -> Self {
        Self(u64::from(points.min(GAUGE_MAX)) * GAUGE_SCALE)
    }

    /// Whole points, 0–100.
    pub fn points(self) -> u32 {
        u32::try_from(self.0 / GAUGE_SCALE).expect("at most 100")
    }

    /// Is it at the top?
    pub fn is_full(self) -> bool {
        self.0 >= u64::from(GAUGE_MAX) * GAUGE_SCALE
    }

    /// Is it at the bottom?
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    fn raise(&mut self, by: u64) {
        self.0 = (self.0 + by).min(u64::from(GAUGE_MAX) * GAUGE_SCALE);
    }

    fn lower(&mut self, by: u64) {
        self.0 = self.0.saturating_sub(by);
    }

    /// Sets it finely, in millionths of a point, at most full.
    fn set_fine(&mut self, fine: u64) {
        self.0 = fine.min(u64::from(GAUGE_MAX) * GAUGE_SCALE);
    }
}

/// A part of a body that can be wounded (D33, D34).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Limb {
    /// The head.
    Head,
    /// The torso.
    Torso,
    /// The left arm.
    LeftArm,
    /// The right arm.
    RightArm,
    /// The left leg.
    LeftLeg,
    /// The right leg.
    RightLeg,
}

/// How bad a wound is.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Hampers a little, heals in days.
    Light,
    /// Hampers a lot, heals in weeks, may bleed.
    Grave,
    /// Kills.
    Mortal,
}

/// A wound on a limb. Carried empty until combat (D33) deals them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Wound {
    /// Where.
    pub limb: Limb,
    /// How bad.
    pub severity: Severity,
    /// Still bleeding?
    pub bleeding: bool,
}

/// What killed a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Death {
    /// Vitality drained by hunger.
    Starved,
    /// Vitality drained by cold.
    Froze,
    /// Vitality drained by thirst.
    Dehydrated,
    /// A mother, giving birth (lot 3 C, D49).
    Childbirth,
    /// The background death of a body under the content's old age (D49).
    Illness,
    /// The background death of an old body (D49).
    OldAge,
}

/// The state of a body: what hurts and what kills (D34).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Condition {
    /// Life left; zero is death.
    pub vitality: Gauge,
    /// Need for food: the appetite, what is missing from the fat reserve
    /// and can still be eaten today (lot 3), a point for
    /// [`content::Needs::kj_per_hunger_point`].
    pub hunger: Gauge,
    /// Need for water (D34, amended 13 November 2026).
    pub thirst: Gauge,
    /// Lack of warmth.
    pub cold: Gauge,
    /// Need for sleep; full, the body collapses asleep.
    pub fatigue: Gauge,
    /// Asleep right now?
    pub asleep: bool,
    /// Wounds by limb.
    pub wounds: Vec<Wound>,
    /// What is missing from the fat reserve, in joules: 0 full, the
    /// content's whole reserve spent (lot 3).
    #[serde(default)]
    pub deficit_j: u64,
    /// What it ate since the day began, in joules: a day's intake has a
    /// ceiling.
    #[serde(default)]
    pub eaten_today_j: u64,
    /// Its size against a grown man, in thousandths, from its age and sex
    /// (lot 3 B): its need and what it carries; its reserve is
    /// `reserve_permille`. Derived each tick,
    /// never stored.
    #[serde(skip, default = "permille_1000")]
    pub size_permille: u32,
    /// Its fat reserve against a grown man's, in thousandths, from its age
    /// and sex: a woman holds more than her size (D47, amended 25 November
    /// 2026). Derived each tick, never stored.
    #[serde(skip, default = "permille_1000")]
    pub reserve_permille: u32,
    /// What it burns against a grown man, in thousandths: its size, more
    /// for a nursing woman. Derived, never stored.
    #[serde(skip, default = "permille_1000")]
    pub burn_permille: u32,
    /// Its pace from its age, and from the infant it carries (D56), in
    /// percent. Derived, never stored.
    #[serde(skip, default = "percent_100")]
    pub age_pace_percent: u32,
    /// The episode of illness it is in, if any (D49, amended 26 November
    /// 2026): slower while it lasts. Absent from a snapshot when healthy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sick: Option<Sickness>,
    /// Until when, healed, it cannot fall ill of each ailment again.
    /// Absent from a snapshot when never healed.
    #[serde(default, skip_serializing_if = "Immunity::is_none")]
    pub immune: Immunity,
}

fn permille_1000() -> u32 {
    1000
}

fn percent_100() -> u32 {
    100
}

impl Condition {
    /// A healthy, fed, warm, rested body.
    pub fn healthy() -> Self {
        Self {
            vitality: Gauge::at(GAUGE_MAX),
            hunger: Gauge::at(0),
            thirst: Gauge::at(0),
            cold: Gauge::at(0),
            fatigue: Gauge::at(0),
            asleep: false,
            wounds: Vec::new(),
            deficit_j: 0,
            eaten_today_j: 0,
            size_permille: 1000,
            reserve_permille: 1000,
            burn_permille: 1000,
            age_pace_percent: 100,
            sick: None,
            immune: Immunity::default(),
        }
    }

    /// What is left of the fat reserve, in joules.
    #[must_use]
    pub fn reserve_left_j(&self, needs: &content::Needs) -> u64 {
        self.reserve_j(needs).saturating_sub(self.deficit_j)
    }

    /// Its whole reserve, in joules: the content's for a grown man, to its
    /// age and sex.
    #[must_use]
    pub fn reserve_j(&self, needs: &content::Needs) -> u64 {
        needs.reserve_j() * u64::from(self.reserve_permille) / 1000
    }

    /// What it burns a day, in joules.
    #[must_use]
    pub fn need_j_per_day(&self, needs: &content::Needs) -> u64 {
        needs.need_j_per_day() * u64::from(self.burn_permille) / 1000
    }

    /// It walked a stretch whose length squared is `mm_squared`
    /// (millimetres): the walk's cost, `walk_j_per_m` for a grown body,
    /// less for a smaller one, adds to what it lacks, on top of the day's
    /// need (D50, amended 24 November 2026).
    pub fn walked(&mut self, mm_squared: u64, needs: &content::Needs) {
        let mm = mm_squared.isqrt();
        let size = u64::from(self.size_permille.min(1000));
        let j = u64::from(needs.walk_j_per_m) * mm * size / 1_000_000;
        self.deficit_j = (self.deficit_j + j).min(self.reserve_j(needs));
    }

    /// Thin: under the content's share of its reserve.
    #[must_use]
    pub fn is_thin(&self, needs: &content::Needs) -> bool {
        self.reserve_left_j(needs) * 100 < self.reserve_j(needs) * u64::from(needs.thin_percent)
    }

    /// Weak: under a smaller share; it walks and works slower.
    #[must_use]
    pub fn is_weak(&self, needs: &content::Needs) -> bool {
        self.reserve_left_j(needs) * 100 < self.reserve_j(needs) * u64::from(needs.weak_percent)
    }

    /// Its pace, walking and working, in percent of a grown body's: slower
    /// weak or sick (the slower of the two), slower young or old.
    #[must_use]
    pub fn pace_percent(&self, needs: &content::Needs) -> u32 {
        let weak = if self.is_weak(needs) {
            needs.weak_pace_percent
        } else {
            100
        };
        let sick = match (&self.sick, &needs.illness) {
            (Some(_), Some(illness)) => illness.sick_pace_percent,
            _ => 100,
        };
        weak.min(sick) * self.age_pace_percent / 100
    }

    /// What it can still eat today, in joules: what its reserve misses,
    /// at most what is left of the day's ceiling.
    #[must_use]
    pub fn room_j(&self, needs: &content::Needs) -> u64 {
        let ceiling = self.need_j_per_day(needs) * u64::from(needs.intake_ceiling_percent) / 100;
        self.deficit_j
            .min(ceiling.saturating_sub(self.eaten_today_j))
    }

    /// Sets hunger from the room it has to eat, in points of its own
    /// need: a child is as hungry as a man when it misses as much of its
    /// day as he does of his (lot 3 B).
    fn set_appetite(&mut self, needs: &content::Needs) {
        let per_point_j = u64::from(needs.kj_per_hunger_point) * u64::from(self.burn_permille);
        self.hunger
            .set_fine(self.room_j(needs) * GAUGE_SCALE / per_point_j.max(1));
    }

    /// The whole points of every gauge and the sleep flag: what a client
    /// is told, and what counts as a change.
    pub fn summary(&self) -> (u32, u32, u32, u32, u32, bool) {
        (
            self.vitality.points(),
            self.hunger.points(),
            self.thirst.points(),
            self.cold.points(),
            self.fatigue.points(),
            self.asleep,
        )
    }
}

/// Where a body that walked straight for want of a way in sight is still
/// going, and how near it stood to it when it last looked (D32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Goal {
    /// The tile it was sent to.
    target: Pos,
    /// The square of its distance to the target's centre, in square
    /// millimetres, where it last looked for a way: it looks again only
    /// from nearer.
    looked_from: u64,
}

/// An inhabitant's body: where it is, where it is going, how it fares.
///
/// It stands at a fine position (millimetres, D32) and walks along a
/// route of straight segments at the speed the content gives its terrain.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Body {
    id: BodyId,
    at: FinePos,
    /// Remaining waypoints of the current walk, nearest first.
    route: VecDeque<FinePos>,
    /// The target of a walk that found no way within sight and went
    /// straight toward it: the body looks again when it stops (D32).
    /// Absent from a snapshot when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    goal: Option<Goal>,
    /// Where the body is in skirting an obstacle for its current order
    /// (D32): once only, then it looks once more and goes on by a way it
    /// sees or stops. Cleared with the goal and on a new order; never
    /// stored, a restart forgets it.
    #[serde(skip)]
    skirted: Skirted,
    /// Where its skirts for its current order began and ended (D54): it
    /// never ends one on a tile of these, and the content caps their
    /// number. Cleared with `skirted`; never stored, a restart forgets it.
    #[serde(skip)]
    went: Vec<Pos>,
    condition: Condition,
    /// The item in hand, if any: visible to all, the tool or weapon (D35).
    held: Option<ItemId>,
    /// Everything else carried, in the order it was taken.
    carried: Vec<ItemId>,
    /// Grams carried in all, under the content's masses: kept as the
    /// inventory changes and set again by [`World::take_content`], so a
    /// walking body is not weighed every tick. Derived, never stored.
    #[serde(skip)]
    load_g: u32,
    /// Grams of loose food among what it carries (D35, amended by D57):
    /// what the arms and its containers hold. Derived with `load_g`, never
    /// stored.
    #[serde(skip)]
    food_g: u32,
    /// Grams of loose food its containers keep together (D57), on top of
    /// what its arms hold. Derived with `load_g`, never stored.
    #[serde(skip)]
    holds_g: u32,
    /// The work it is busy at, if any (D45): gathering, a pinch at a time.
    #[serde(default)]
    work: Option<Work>,
    /// The gesture it is busy at, if any (D57): a cut or a fold, until its
    /// time is up. Absent from a snapshot when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    making: Option<hands::Making>,
    /// The day it was born, counted from the world's first (before it:
    /// negative). Lot 3 B.
    #[serde(default = "born_grown")]
    born_day: i64,
    /// Woman or man (lot 3 B).
    #[serde(default = "male")]
    sex: Sex,
    /// The body carrying it, an infant not yet weaned (lot 3 B).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    carried_by: Option<BodyId>,
    /// Grams of the infants it carries, on top of its load. Derived each
    /// tick, never stored.
    #[serde(skip)]
    carrying_g: u32,
    /// Its mother, when born in the world (lot 3 C, D49); kept after her
    /// death, ids never being reused. Absent from a snapshot when unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mother: Option<BodyId>,
    /// Its father, as its mother.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    father: Option<BodyId>,
    /// Its spouse, alive: cleared when the spouse dies (D49).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    spouse: Option<BodyId>,
    /// The dawns in a row its spouse was beyond the content's reach (D49,
    /// amended 24 November 2026): at `part_after_days` the couple parts.
    #[serde(default, skip_serializing_if = "is_zero")]
    apart_days: u32,
    /// The unmarried body it courts and the dawns in a row it slept near
    /// it (D49, amended 24 November 2026): at `together_nights` they wed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    courting: Option<(BodyId, u32)>,
    /// The child a woman carries, not yet born (D49).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pregnant: Option<Pregnancy>,
    /// A woman's reserve left at each of her last dawns, in joules, the
    /// oldest first, at most the content's `conceive_gain_days` (D49,
    /// amended 25 November 2026: fertility follows the gain). Kept only
    /// while life is on (the vivarium's); never stored: a world read back
    /// reads her gain flat until dawns pass again.
    #[serde(skip)]
    reserve_dawns: Vec<u64>,
}

/// A body of before ages, read from a world saved before them: twenty-five
/// age-years old at the world's first day, in the legacy calendar's
/// age-years (one a cycle of fifty days), pinned whatever the default
/// calendar becomes.
fn born_grown() -> i64 {
    -25 * i64::from(Calendar::LEGACY_DAYS_PER_CYCLE / Calendar::LEGACY_AGE_YEARS_PER_CYCLE)
}

fn male() -> Sex {
    Sex::Male
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip_serializing_if
fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Work that lasts (D45): a body gathering from a tree or a plant, by
/// hand, at its kind's rate, until the plant is bare, its hands and back
/// are full, or it is told to do something else.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Work {
    /// The chunk of what it gathers from.
    pub chunk: ChunkPos,
    /// What it gathers from.
    pub bearer: crops::Bearer,
    /// The material it gathers. Shared, not copied, by what reads it a
    /// tick; written and read as the text itself (a snapshot's JSON is as
    /// before, D28).
    #[serde(with = "crops::shared_text")]
    pub gives: Arc<str>,
    /// When it began.
    pub since: Tick,
    /// The grams gathered so far.
    pub done_g: u32,
    /// The carried item they go into, once the first pinch is taken.
    pub into: Option<ItemId>,
    /// The crop it gathers, in the content's order: found by name when
    /// the work begins and kept, so its rate and what is left of it are
    /// asked by number at every tick ([`World::ripe_of_crop`]). Never
    /// stored: found again by name at the first tick after a load, or
    /// after the content changed.
    #[serde(skip)]
    pub crop: Option<usize>,
}

/// Why a body stopped working.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkEnd {
    /// Nothing is left of the crop on the plant (or its season is over).
    Bare,
    /// It carries all it can.
    Full,
    /// It was told to do something else, or fell asleep.
    Interrupted,
}

/// What thirst rises by in one tick (in the gauge's fine units), for a
/// day of `per_day` ticks: faster in the season's heat, and walking or
/// working (`effort`).
pub(crate) fn thirst_raise(
    needs: &content::Needs,
    season: Season,
    effort: bool,
    per_day: u64,
) -> u64 {
    let heat = u64::from(needs.thirst_heat_percent.get(season));
    let effort = if effort {
        u64::from(needs.thirst_effort_percent)
    } else {
        100
    };
    u64::from(needs.thirst_per_day) * GAUGE_SCALE * heat * effort / (per_day * 10_000)
}

/// What the cold rises by a day of night in `season`, without warmth: the
/// night's cold follows the season (24 November 2026).
pub(crate) fn night_cold(needs: &content::Needs, season: Season) -> u32 {
    needs.cold_per_day_at_night * needs.cold_season_percent.get(season) / 100
}

/// The least a body gathers at a time, in grams: a pinch.
const PINCH_G: u32 = 10;

/// How near a tree or plant a body gathers from it, in millimetres: an
/// arm's reach and a step.
pub const GATHER_REACH_MM: i64 = 2500;

impl Body {
    /// Forgets the order it was walking on: its goal, what it skirted and
    /// where it went to look again. The route is its caller's to clear.
    fn forget_way(&mut self) {
        self.goal = None;
        self.skirted = Skirted::Not;
        self.went.clear();
    }

    /// The body's id.
    pub fn id(&self) -> BodyId {
        self.id
    }

    /// How the body fares.
    pub fn condition(&self) -> &Condition {
        &self.condition
    }

    /// The work the body is busy at, if any (D45).
    pub fn work(&self) -> Option<&Work> {
        self.work.as_ref()
    }

    /// The item in hand, if any.
    pub fn held(&self) -> Option<ItemId> {
        self.held
    }

    /// The items carried but not in hand, in the order they were taken.
    pub fn carried(&self) -> &[ItemId] {
        &self.carried
    }

    /// Does the body have this item, in hand or carried?
    pub fn has(&self, item: ItemId) -> bool {
        self.held == Some(item) || self.carried.contains(&item)
    }

    /// Everything the body has, the item in hand first.
    pub fn inventory(&self) -> impl Iterator<Item = ItemId> + '_ {
        self.held.into_iter().chain(self.carried.iter().copied())
    }

    fn let_go(&mut self, item: ItemId) {
        if self.held == Some(item) {
            self.held = None;
        }
        self.carried.retain(|i| *i != item);
    }

    /// The tile the body stands on.
    pub fn pos(&self) -> Pos {
        self.at.tile()
    }

    /// Where the body stands, in millimetres.
    pub fn at(&self) -> FinePos {
        self.at
    }

    /// The waypoints still ahead, nearest first.
    pub fn route(&self) -> impl Iterator<Item = FinePos> + '_ {
        self.route.iter().copied()
    }

    /// Is the body walking somewhere?
    pub fn is_walking(&self) -> bool {
        !self.route.is_empty()
    }

    /// Is the body pausing on its way: stopped where its straight walk
    /// ended, short of a goal it keeps, to look again for a way from
    /// there at the next tick (D32)? Its stop is told as any other
    /// ([`Event::BodyStopped`], so that whoever saw it set off stops it
    /// too), but its walk is not over: a pilot that reads a stop as the
    /// end of the walk reads this one wrong.
    pub fn is_pausing(&self) -> bool {
        self.route.is_empty()
            && self
                .goal
                .is_some_and(|goal| self.at != goal.target.centre())
    }

    /// Grams the body carries in all, as last weighed.
    pub fn load_g(&self) -> u32 {
        self.load_g
    }

    /// The day it was born, counted from the world's first.
    pub fn born_day(&self) -> i64 {
        self.born_day
    }

    /// Woman or man.
    pub fn sex(&self) -> Sex {
        self.sex
    }

    /// The body carrying it, if it is an infant carried.
    pub fn carried_by(&self) -> Option<BodyId> {
        self.carried_by
    }

    /// Its mother, when it was born in the world (D49).
    pub fn mother(&self) -> Option<BodyId> {
        self.mother
    }

    /// Its father, when it was born in the world (D49).
    pub fn father(&self) -> Option<BodyId> {
        self.father
    }

    /// Its living spouse, if any (D49).
    pub fn spouse(&self) -> Option<BodyId> {
        self.spouse
    }

    /// The child it carries unborn, if any (D49).
    pub fn pregnant(&self) -> Option<Pregnancy> {
        self.pregnant
    }

    /// Her reserve's mean gain a day over the dawns kept (D49, amended 25
    /// November 2026: fertility follows the gain), in per cent of her
    /// day's need: her reserve left now against her oldest dawn kept,
    /// spread over the dawns since. Negative when it falls; 0 with no dawn
    /// kept (life off, or a world just read back).
    #[must_use]
    pub fn reserve_gain_percent(&self, needs: &content::Needs) -> i64 {
        let Some(&oldest) = self.reserve_dawns.first() else {
            return 0;
        };
        let now = i128::from(self.condition.reserve_left_j(needs));
        let days = i128::try_from(self.reserve_dawns.len()).unwrap_or(i128::MAX);
        let need = i128::from(self.condition.need_j_per_day(needs).max(1));
        let gain = (now - i128::from(oldest)) * 100 / (days * need);
        i64::try_from(gain).unwrap_or(if gain < 0 { i64::MIN } else { i64::MAX })
    }

    /// What it can carry in all, in grams: the content's load for a grown
    /// body, to its size, less the infants it carries (D35). Whole objects
    /// (a branch, a log, a flint) count against this alone; loose food
    /// also against what its arms and containers hold
    /// ([`Self::food_room_g`]).
    pub fn capacity_g(&self, movement: &Movement) -> u32 {
        let size = u64::from(self.condition.size_permille.min(1000));
        let own = u64::from(movement.carry_capacity_g) * size / 1000;
        u32::try_from(own)
            .unwrap_or(u32::MAX)
            .saturating_sub(self.carrying_g)
    }

    /// What its arms and containers hold of loose food, in grams (D35,
    /// amended 24 November 2026; D57): the content's `hands_g` to its
    /// size, an infant riding on the back and taking nothing from it, and
    /// what each container it carries keeps together.
    pub fn food_capacity_g(&self, movement: &Movement) -> u32 {
        let size = u64::from(self.condition.size_permille.min(1000));
        let arms = u64::from(movement.hands_g) * size / 1000;
        u32::try_from(arms)
            .unwrap_or(u32::MAX)
            .saturating_add(self.holds_g)
    }

    /// The grams of loose food it can still take (D57): what its arms and
    /// containers hold less the food it carries, and no more than its
    /// load leaves.
    pub fn food_room_g(&self, movement: &Movement) -> u32 {
        self.food_capacity_g(movement)
            .saturating_sub(self.food_g)
            .min(self.capacity_g(movement).saturating_sub(self.load_g))
    }

    /// Grams of loose food it carries, as last weighed (D57).
    pub fn food_g(&self) -> u32 {
        self.food_g
    }

    /// Grams of loose food its arms alone hold (D57): `hands_g` by its
    /// size, without the containers.
    pub fn arms_g(&self, movement: &Movement) -> u32 {
        self.food_capacity_g(movement).saturating_sub(self.holds_g)
    }

    /// The gesture it is busy at, if any (D57).
    pub fn making(&self) -> Option<&hands::Making> {
        self.making.as_ref()
    }

    /// What weighs on its walk, in grams: its load and the infants it
    /// carries.
    fn burden_g(&self) -> u32 {
        self.load_g.saturating_add(self.carrying_g)
    }
}

/// Identifies a piece of material lying in the world.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ItemId(pub u64);

/// Where an item is: in exactly one place at a time (D35).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Place {
    /// Lying on a tile.
    Ground {
        /// The tile.
        pos: Pos,
    },
    /// Carried by a body, in hand or on it.
    Carried {
        /// The body.
        body: BodyId,
    },
}

/// A bulk mass of one material (D35): a pebble, a log, a heap of berries.
/// It weighs its own whole grams, which split and merge, and keeps the
/// tick it was made (its date, from which spoilage will be computed, D16).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Item {
    id: ItemId,
    /// Content id of its material.
    material: String,
    /// Its mass, in whole grams; never 0.
    mass_g: u32,
    /// When it was made: picked, dropped by the ground (tick 0, as old as
    /// the world) or left by a death.
    made: Tick,
    place: Place,
}

impl Item {
    /// The item's id.
    pub fn id(&self) -> ItemId {
        self.id
    }

    /// Content id of the material it is made of.
    pub fn material(&self) -> &str {
        &self.material
    }

    /// Its mass, in grams.
    pub fn mass_g(&self) -> u32 {
        self.mass_g
    }

    /// The tick it was made: its date.
    pub fn made(&self) -> Tick {
        self.made
    }

    /// Where it is.
    pub fn place(&self) -> Place {
        self.place
    }

    /// The tile it lies on, if it is on the ground.
    pub fn pos(&self) -> Option<Pos> {
        match self.place {
            Place::Ground { pos } => Some(pos),
            Place::Carried { .. } => None,
        }
    }

    /// Takes `grams` off this pile into a new item `id`, in the same place
    /// and of the same date; the pile keeps the rest. `None`, and nothing
    /// taken, unless both parts weigh something: a split of 0 grams or of
    /// the whole pile is no split.
    #[must_use]
    pub fn split(&mut self, id: ItemId, grams: u32) -> Option<Item> {
        if grams == 0 || grams >= self.mass_g {
            return None;
        }
        self.mass_g -= grams;
        Some(Item {
            id,
            material: self.material.clone(),
            mass_g: grams,
            made: self.made,
            place: self.place,
        })
    }

    /// Pours `other` into this pile: one item of both masses, this one's id
    /// and place, **dated as the older of the two**. A heap is as old as
    /// the oldest matter in it: fresh berries poured on old ones do not
    /// make the old ones fresh, so a store cannot be kept young by topping
    /// it up (a mass-weighted date would allow just that), and the date
    /// stays exact, never rounded. Gives `other` back, untouched, when the
    /// materials differ or the mass would overflow.
    ///
    /// # Errors
    /// `other` itself, when it cannot be merged.
    pub fn merge(&mut self, other: Item) -> Result<(), Item> {
        if other.material != self.material {
            return Err(other);
        }
        let Some(mass) = self.mass_g.checked_add(other.mass_g) else {
            return Err(other);
        };
        self.mass_g = mass;
        self.made = self.made.min(other.made);
        Ok(())
    }
}

/// What a body can do with an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemAct {
    /// Take it off the ground.
    PickUp,
    /// Lay it on the ground.
    Drop,
    /// Take it in hand.
    Hold,
    /// Put it away, out of the hand.
    Stow,
    /// Eat it.
    Eat,
}

/// Why an act on an item was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// No such body or item.
    Unknown,
    /// Not on the body's tile nor next to it.
    OutOfReach,
    /// The body does not have it.
    NotCarried,
    /// It would exceed what the body can carry.
    TooHeavy,
    /// It is not food.
    NotFood,
    /// No running water there to drink.
    NotWater,
    /// Nothing of that crop is on the plant now.
    NothingRipe,
    /// The food has spoiled.
    Spoiled,
    /// The body can eat no more today: its reserve is full, or it ate
    /// the day's ceiling.
    Sated,
}

/// What a body that stopped for good short of its target sees barring
/// its way (D32): only what lies right ahead of it, never what it could
/// not see.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Blockage {
    /// Water too deep to walk through.
    Water,
    /// The cliff.
    Rock,
    /// Nothing it may not enter right ahead, yet no way it can see.
    NoWay,
}

/// What a pilot (player or, later, NPC brain) asks a body to do (D20).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Intention {
    /// Walk to a tile.
    MoveTo {
        /// The body that should walk.
        body: BodyId,
        /// Where to.
        target: Pos,
    },
    /// Eat an item within reach on the ground, or carried.
    Eat {
        /// Who.
        body: BodyId,
        /// What.
        item: ItemId,
    },
    /// Take an item off the ground within reach: into the hand if it is
    /// empty, otherwise carried.
    PickUp {
        /// Who.
        body: BodyId,
        /// What.
        item: ItemId,
    },
    /// Lay a carried item on the body's tile.
    Drop {
        /// Who.
        body: BodyId,
        /// What.
        item: ItemId,
    },
    /// Lay a carried item in a store a brain keeps at the body's tile: a
    /// hearth's reserve (D58, the ledger). It leaves the ground and the
    /// world's items for a record no look reads ([`World::kept`]): eaten
    /// from there, it feeds the body with no item made; taken away, it is
    /// an item again. Refused as a drop is.
    Keep {
        /// Who.
        body: BodyId,
        /// What.
        item: ItemId,
    },
    /// Give a young one standing within reach the food `item` to eat
    /// (D58, amended 26 November 2026: the young are seen, not their
    /// stomach): `item` carried by the body, or lying, or kept in a
    /// store, within its reach; the young eats it as by [`Intention::Eat`],
    /// as much as its own room for food allows. A gesture of the
    /// simulation's own (the brains'); no client asks it.
    Feed {
        /// Who gives.
        body: BodyId,
        /// Who eats.
        young: BodyId,
        /// What.
        item: ItemId,
    },
    /// Give the infant `infant` to `carrier` to carry and nurse (D56,
    /// amended 27 November 2026): an infant nobody carries, lying within
    /// the body's reach, or one the body carries itself; `carrier` the
    /// body itself or one standing within its reach, neither an infant
    /// nor carried. Refused without a word otherwise: who asked looks at
    /// the infant again. A gesture of the simulation's own (the brains');
    /// no client asks it.
    Carry {
        /// Who gives, or takes up.
        body: BodyId,
        /// The infant.
        infant: BodyId,
        /// Who carries it from now on.
        carrier: BodyId,
    },
    /// Take a carried item in hand; what was in hand is carried instead.
    Hold {
        /// Who.
        body: BodyId,
        /// What.
        item: ItemId,
    },
    /// Put the item in hand away, among what is carried.
    Stow {
        /// Who.
        body: BodyId,
        /// What.
        item: ItemId,
    },
    /// Drink from running water (a river, a ford, a brook) on the
    /// body's tile or next to it: thirst is quenched at once.
    Drink {
        /// Who.
        body: BodyId,
        /// The tile of water.
        at: Pos,
    },
    /// Lie down and sleep where one stands.
    Sleep {
        /// Who.
        body: BodyId,
    },
    /// Wake up.
    Wake {
        /// Who.
        body: BodyId,
    },
    /// Development only (the server accepts it only when started with
    /// `--dev`): stand at once on the walkable tile nearest `target`,
    /// within `TELEPORT_REACH`, to explore the map quickly. Refused when
    /// there is none.
    Teleport {
        /// Who.
        body: BodyId,
        /// Where to.
        target: Pos,
    },
    /// Walk behind another body (D56): set off on the way `leader` is
    /// walking when it walks and stands within [`FOLLOW_TILES`] of the
    /// follower, its route taken whole; otherwise walk to `target` as
    /// [`Intention::MoveTo`] would. A walk of the simulation's own (the
    /// brains' groups); no client asks it.
    Follow {
        /// Who walks behind.
        body: BodyId,
        /// Whom it walks behind.
        leader: BodyId,
        /// Where to, when the leader is not walking within reach.
        target: Pos,
    },
    /// Gather by hand the crop giving `gives` from a tree or a plant
    /// within reach (D45): work that lasts, a pinch at a time, into an
    /// item carried, until the plant is bare, the body can carry no more,
    /// or another intention comes.
    Gather {
        /// Who.
        body: BodyId,
        /// The chunk of the tree or plant.
        chunk: ChunkPos,
        /// The tree or plant.
        source: crops::Source,
        /// The material gathered.
        gives: String,
    },
    /// Cut the tree `tree` of `chunk` within reach with the item `with`
    /// it carries, taken in hand (D57): a gesture of the content's
    /// minutes, whatever comes of it.
    Cut {
        /// Who.
        body: BodyId,
        /// The item it cuts with.
        with: ItemId,
        /// The chunk of the tree.
        chunk: ChunkPos,
        /// The tree.
        tree: trees::TreeKey,
    },
    /// Fold the item `item` it carries (D57): a gesture of the content's
    /// minutes, whatever comes of it.
    Fold {
        /// Who.
        body: BodyId,
        /// The item folded.
        item: ItemId,
    },
}

impl Intention {
    /// The act on an item it asks for, and the item, if it is one.
    #[must_use]
    pub fn item_act(&self) -> Option<(ItemAct, ItemId)> {
        match *self {
            Intention::Eat { item, .. } | Intention::Feed { item, .. } => {
                Some((ItemAct::Eat, item))
            }
            Intention::PickUp { item, .. } => Some((ItemAct::PickUp, item)),
            Intention::Drop { item, .. } | Intention::Keep { item, .. } => {
                Some((ItemAct::Drop, item))
            }
            Intention::Hold { item, .. } => Some((ItemAct::Hold, item)),
            Intention::Stow { item, .. } => Some((ItemAct::Stow, item)),
            _ => None,
        }
    }

    /// The body it is for.
    pub fn body(&self) -> BodyId {
        match *self {
            Intention::MoveTo { body, .. }
            | Intention::Eat { body, .. }
            | Intention::Feed { body, .. }
            | Intention::Carry { body, .. }
            | Intention::PickUp { body, .. }
            | Intention::Drop { body, .. }
            | Intention::Keep { body, .. }
            | Intention::Hold { body, .. }
            | Intention::Stow { body, .. }
            | Intention::Drink { body, .. }
            | Intention::Sleep { body }
            | Intention::Wake { body }
            | Intention::Teleport { body, .. }
            | Intention::Follow { body, .. }
            | Intention::Gather { body, .. }
            | Intention::Cut { body, .. }
            | Intention::Fold { body, .. } => body,
        }
    }
}

/// How far ahead of a body that stopped for good it looks for what bars
/// its way, in millimetres ([`Event::BodyBlocked`]): it stops 30 cm short
/// of the first tile it may not enter, or a quarter of a tile when it could
/// not set off at all.
const BLOCKAGE_REACH_MM: i64 = 1500;

/// How far from the asked tile a teleport looks for walkable ground.
pub const TELEPORT_REACH: i32 = 64;

/// Something that happened in the world during a tick.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// A new day began.
    DayStarted {
        /// The day that just started (days count from 1).
        day: u64,
    },
    /// A new season began, at the dawn of its first day (D09).
    SeasonStarted {
        /// The year it belongs to (years count from 1).
        year: u64,
        /// Which.
        season: Season,
    },
    /// A body set off along a route of straight segments.
    BodyWalks {
        /// Who.
        body: BodyId,
        /// Where it starts from.
        from: FinePos,
        /// The waypoints, nearest first, the last being the destination.
        route: Vec<FinePos>,
        /// Its nominal speed, on the reference terrain.
        speed_cm_per_s: u32,
    },
    /// A body stepped onto another tile.
    BodyMoved {
        /// Who.
        body: BodyId,
        /// The tile it now stands on.
        to: Pos,
        /// Exactly where.
        at: FinePos,
        /// Waypoints still ahead of it.
        remaining: usize,
    },
    /// A body stopped walking: arrived, or nothing left to walk.
    BodyStopped {
        /// Who.
        body: BodyId,
        /// Where.
        at: FinePos,
    },
    /// A walk could not start (unknown body, impassable or unreachable target).
    MoveRefused {
        /// Who.
        body: BodyId,
        /// Where it wanted to go.
        target: Pos,
        /// What it sees barring its way, as for [`Event::BodyBlocked`]: a
        /// new order that cannot even set off says why (D32); no way seen
        /// for an unknown body or a teleport with nowhere to land.
        reason: Blockage,
    },
    /// A body that walked straight for want of a way stopped for good
    /// short of its target: it looked again and found no way, and could
    /// get no nearer (D32). Told to its pilot alone.
    BodyBlocked {
        /// Who.
        body: BodyId,
        /// Where it stands.
        at: FinePos,
        /// Where it was going.
        target: Pos,
        /// What it sees barring its way.
        reason: Blockage,
    },
    /// Night fell: cold rises until dawn.
    NightFell {
        /// The day it ends.
        day: u64,
    },
    /// The whole points of a body's gauges changed, or it fell asleep or
    /// woke. Once per body per tick as it fares, and at once when it
    /// drinks.
    ConditionChanged {
        /// Who.
        body: BodyId,
        /// (vitality, hunger, thirst, cold, fatigue, asleep).
        summary: (u32, u32, u32, u32, u32, bool),
    },
    /// A body ate a meal off an item: all of it, which is gone, or a
    /// meal's grams ([`content::Needs::meal_g`]), the rest staying.
    BodyAte {
        /// Who.
        body: BodyId,
        /// What.
        item: ItemId,
        /// Content id of its material.
        material: String,
        /// Grams eaten.
        grams: u32,
        /// Kilojoules they gave.
        energy_kj: u32,
    },
    /// A body drank its fill: its thirst is gone.
    BodyDrank {
        /// Who.
        body: BodyId,
        /// The tile of water.
        at: Pos,
    },
    /// A drink was refused: the tile is out of reach, or no running water.
    DrinkRefused {
        /// Who.
        body: BodyId,
        /// The tile asked.
        at: Pos,
        /// Why.
        reason: Refusal,
    },
    /// An act on an item was refused.
    ActRefused {
        /// Who.
        body: BodyId,
        /// What it tried.
        act: ItemAct,
        /// On what.
        item: ItemId,
        /// Why.
        reason: Refusal,
    },
    /// What a body carries changed: in hand, carried, the load and the
    /// capacity in grams. Told to the pilot.
    InventoryChanged {
        /// Who.
        body: BodyId,
        /// In hand.
        held: Option<ItemId>,
        /// Carried, in order.
        carried: Vec<ItemId>,
        /// Grams carried in all.
        load_g: u32,
        /// Grams the body can carry.
        capacity_g: u32,
    },
    /// A body began to gather (D45). Seen by all.
    WorkStarted {
        /// Who.
        body: BodyId,
        /// What it gathers.
        gives: String,
    },
    /// A crop of `chunk` was picked (D45), and the chunk's version right
    /// after it: the fruit its tree shows may have changed (D48).
    CropPicked {
        /// Which chunk.
        chunk: ChunkPos,
        /// Its version now ([`World::chunk_version`]).
        version: ground::Version,
    },
    /// A body finished a gesture (D57): a cut or a fold, on `item` (the
    /// tool it cut with, or what it folded), and what came of it: a new
    /// item cut, the item folded into another material, or nothing.
    /// Seen by all.
    Gestured {
        /// Who.
        body: BodyId,
        /// The gesture.
        gesture: hands::Gesture,
        /// The tool it cut with, or what it folded.
        item: ItemId,
        /// What came of it, if anything.
        gave: Option<ItemId>,
    },
    /// A body could not begin a gesture (D57). Told to the pilot.
    GestureRefused {
        /// Who.
        body: BodyId,
        /// The gesture.
        gesture: hands::Gesture,
        /// Why.
        reason: Refusal,
    },
    /// A body stopped gathering, and what it gathered. Seen by all.
    WorkStopped {
        /// Who.
        body: BodyId,
        /// What it gathered.
        gives: String,
        /// How much, in grams.
        grams: u32,
        /// Why it stopped.
        why: WorkEnd,
    },
    /// A body could not begin to gather. Told to the pilot.
    GatherRefused {
        /// Who.
        body: BodyId,
        /// What it would have gathered.
        gives: String,
        /// Why not.
        reason: Refusal,
    },
    /// Food spoiled and is gone (D16, D45), wherever it was: on the
    /// ground, carried. Told when the day starts.
    Spoiled {
        /// The item.
        item: ItemId,
        /// Its material.
        material: String,
        /// Its grams.
        grams: u32,
        /// Who carried it, if anyone.
        holder: Option<BodyId>,
    },
    /// What a body holds in hand changed. Seen by all.
    BodyHolds {
        /// Who.
        body: BodyId,
        /// The item in hand now, if any.
        item: Option<ItemId>,
    },
    /// An item appeared on the ground.
    ItemAdded {
        /// Which.
        item: ItemId,
        /// Content id of its material.
        material: String,
        /// Its mass, in grams.
        mass_g: u32,
        /// Where.
        at: Pos,
        /// Its chunk's version right after it ([`World::chunk_version`]).
        version: ground::Version,
    },
    /// Food laid in a store a brain keeps ([`Intention::Keep`], D58), or
    /// what is left of it after a meal from it: off the ground, no chunk
    /// changes.
    Kept {
        /// Which.
        item: ItemId,
        /// The record that holds it now: itself, or the heap of the same
        /// food spoiling the same day at that tile that it joined (D58,
        /// amended 26 November 2026: the ledger keeps heaps).
        into: ItemId,
        /// Content id of its material.
        material: String,
        /// Its mass, in grams: what was laid, or what is left after a
        /// meal.
        mass_g: u32,
        /// The tile it is kept at.
        at: Pos,
    },
    /// An item is gone from the ground.
    ItemRemoved {
        /// Which.
        item: ItemId,
        /// Where it was.
        at: Pos,
        /// Its chunk's version right after it ([`World::chunk_version`]).
        version: ground::Version,
    },
    /// A child was born where its mother stood, carried by her (D49).
    BodyBorn {
        /// The newborn.
        body: BodyId,
        /// Its mother.
        mother: BodyId,
        /// Its father: her spouse when she conceived.
        father: BodyId,
        /// Where.
        at: Pos,
    },
    /// A pregnant woman grew weak and lost the child (D49).
    PregnancyLost {
        /// Who.
        mother: BodyId,
        /// Where she stood.
        at: Pos,
    },
    /// Two bodies became spouses (D49).
    Coupled {
        /// The one whose daily hour it was.
        a: BodyId,
        /// The nearest it could take.
        b: BodyId,
    },
    /// Two spouses parted, apart too long (D49, amended 24 November 2026).
    Parted {
        /// The lower id.
        a: BodyId,
        /// The other.
        b: BodyId,
    },
    /// A body fell ill (D49, amended 26 November 2026).
    FellIll {
        /// Who.
        body: BodyId,
        /// Of what.
        ailment: Ailment,
        /// Whether the draw fell on the share its hearth's sick added.
        caught: bool,
        /// Where it stood.
        at: Pos,
    },
    /// An episode of illness ended, healed or dead (D49, amended 26
    /// November 2026); a death comes after, as `BodyDied`.
    IllnessEnded {
        /// Who.
        body: BodyId,
        /// Of what.
        ailment: Ailment,
        /// Whether it killed.
        died: bool,
        /// Its reserve left, in percent of its whole.
        reserve_percent: u32,
        /// Its age, in thousandths of an age-year.
        age_milli: u64,
    },
    /// A body died; it is now a corpse item.
    BodyDied {
        /// Who.
        body: BodyId,
        /// Of what.
        cause: Death,
        /// Where.
        at: Pos,
        /// The corpse.
        corpse: ItemId,
    },
    /// What a body thinks as it decides how to go on toward where it was
    /// sent (D32), about the place `at`: where it goes, or where it
    /// stands. Told to its pilot alone.
    BodyThinks {
        /// Who.
        body: BodyId,
        /// What.
        thought: Thought,
        /// About where.
        at: FinePos,
    },
}

/// A body's thought as it decides how to go on (D32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Thought {
    /// No way here: it walks along the obstacle, to that side.
    Skirting {
        /// Which way, as it faces its target.
        side: Side,
    },
    /// It sees a way on and takes it.
    WaySeen,
    /// It sees no way and stops: it does not know where to go.
    NoWay,
}

/// Where a body is in skirting an obstacle for its current order (D32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Skirted {
    /// It has not skirted.
    #[default]
    Not,
    /// It walked along the obstacle and looks once more on arriving.
    Along,
    /// It saw a way on from there and took it: it skirts no more.
    Beyond,
}

/// How a body sets off ([`World::plan_walk`]).
enum Plan {
    /// Along a way found to the target.
    Path(Vec<FinePos>),
    /// Along a way found past the obstacle on its line, the target kept.
    Past(Vec<FinePos>),
    /// Along the obstacle, to look once more from there.
    Skirt(Vec<FinePos>, Side),
    /// Straight toward the target; `true` when that is a way seen on
    /// looking once more after skirting.
    Straight(Vec<FinePos>, bool),
    /// Nowhere: it skirted once and sees no way.
    GiveUp,
}

/// The lane of a path search: the players' (bodies with a pilot) or the
/// others' (NPCs, animals), each with its share of the work (D32).
const PLAYERS: usize = 0;
const OTHERS: usize = 1;

/// The path searches waiting for their turn (D32): two lanes, players'
/// and others', each served in the order its searches came; a body has
/// one search waiting at most. The cap counts the tiles searches expand,
/// not the searches: one refused at once costs next to nothing.
#[derive(Debug, Clone, Default)]
struct Searches {
    /// Per lane, by arrival: the body and its order's target, or `None`
    /// for a body looking again.
    lanes: [BTreeMap<u64, (BodyId, Option<Pos>)>; 2],
    /// The lane and place of each body's waiting search.
    waiting: BTreeMap<BodyId, (usize, u64)>,
    /// The next place in a lane: arrivals only ever grow.
    next: u64,
    /// Work spent past the last tick's allowance, repaid by the next.
    owed: u64,
}

impl Searches {
    /// Puts a body's search in its lane. A new order takes the place of
    /// the body's waiting search, if any, keeping its turn; a look again
    /// never displaces what waits.
    fn ask(&mut self, body: BodyId, order: Option<Pos>, lane: usize) {
        if let Some(&(lane, at)) = self.waiting.get(&body) {
            if order.is_some() {
                self.lanes[lane].insert(at, (body, order));
            }
            return;
        }
        let at = self.next;
        self.next += 1;
        self.lanes[lane].insert(at, (body, order));
        self.waiting.insert(body, (lane, at));
    }

    /// The search at the head of a lane, taken out of it.
    fn pop(&mut self, lane: usize) -> Option<(BodyId, Option<Pos>)> {
        let (_, (body, order)) = self.lanes[lane].pop_first()?;
        self.waiting.remove(&body);
        Some((body, order))
    }
}

/// The world state: time, ground, bodies, things on the ground.
///
/// Read back from a snapshot through `Stored`, which rebuilds what is
/// derived from the rest (the bodies by chunk) and is never written.
#[derive(Debug, Clone, serde::Serialize)]
pub struct World {
    calendar: Calendar,
    tick: Tick,
    /// Whether bodies are born, couple and die of illness and old age
    /// (lot 3 C, D49): off in the game, on in the vivarium alone. A world
    /// saved before it has it off.
    #[serde(default)]
    life: bool,
    /// Shared, never changed once made: copying the world (to save it on
    /// another thread) and the network's view of the ground take a handle,
    /// not the tiles.
    #[serde(serialize_with = "write_map")]
    map: Arc<Map>,
    /// Every body, in id order: ids only ever grow, so a body is found by
    /// halving, never by a scan ([`World::body`]).
    bodies: Vec<Body>,
    next_body: u32,
    /// The items a body has touched, made or let fall: stored, by id. The
    /// ground's own (dead wood, pebbles) are made per chunk and never
    /// stored ([`items`]). Written as the list it always was, in id order.
    #[serde(serialize_with = "write_items")]
    items: BTreeMap<ItemId, Item>,
    next_item: u64,
    /// Whether the ground drops natural items ([`items`]); false for worlds
    /// made before them, whose scattered items are all in `items`.
    natural: bool,
    /// What men changed, chunk by chunk (D28, lightness plan D2): the
    /// natural items taken off the ground (they do not come back), and,
    /// when felling, planting and building come, the trees felled and
    /// planted and what was built. Absent for a chunk as the seed made it.
    /// Stored per chunk beside the snapshot, never in it: a checkpoint
    /// writes only the chunks whose version moved. Shared with the copy
    /// handed to the storage thread, copied only when changed.
    #[serde(skip)]
    changes: BTreeMap<ChunkPos, Arc<changes::ChunkChanges>>,
    /// The bodies standing in each chunk, ids in order: kept as bodies
    /// walk, teleport, appear and die, so the bodies of a place are a
    /// lookup, never a scan of them all (D39). Derived: rebuilt on load.
    #[serde(skip)]
    by_chunk: BTreeMap<ChunkPos, Vec<BodyId>>,
    /// The stored items lying in each chunk: kept as items are laid,
    /// taken, dropped and eaten, so the items of a place are a lookup
    /// (D39). Derived: rebuilt on load.
    #[serde(skip)]
    ground: BTreeMap<ChunkPos, BTreeSet<ItemId>>,
    /// The food kept in the stores the brains keep (D58, a hearth's
    /// reserve as a ledger): records, by id, each where it is kept, no
    /// longer items of the world nor lying on the ground, so that no look
    /// reads them. Only the vivarium's brains keep food (D50): never
    /// persisted, as the minds are not; the server's worlds hold none.
    #[serde(skip)]
    kept: BTreeMap<ItemId, Item>,
    /// The records of `kept` by chunk, to find the heap a laid item joins
    /// (`World::keep`). Derived from `kept`.
    #[serde(skip)]
    kept_in: BTreeMap<ChunkPos, BTreeSet<ItemId>>,
    /// How many times `kept` changed (a record laid, joined, eaten from,
    /// taken away, spoiled): a brain's reading of its stores holds while
    /// this does not move ([`World::kept_changes`]). Never persisted, as
    /// `kept` is not.
    #[serde(skip)]
    kept_changes: u64,
    /// How many items each record of `kept` was laid as (D58, amended
    /// 26 November 2026: « une fois 125 noisettes », a heap knows its
    /// count): a bite from a heap is one of them at most, as a bite from
    /// the item laid was. Derived from `kept`'s changes, never persisted.
    #[serde(skip)]
    kept_parts: BTreeMap<ItemId, u32>,
    /// The content's spawn rules the natural items follow, set with the
    /// kinds of tree ([`World::take_content`]). Never persisted.
    #[serde(skip)]
    spawns: Vec<content::Spawn>,
    /// Intentions received since the last tick, applied in order at the
    /// start of the next one. Never persisted: a restart drops them.
    #[serde(skip)]
    pending: Vec<Intention>,
    /// Whether the bodies' size, burn and pace must be set again before
    /// the day's first tick ([`World::grow`]): raised whenever what they
    /// follow may have changed between two days (a body added, carried,
    /// set down, orphaned or dead; a pregnancy; the life rule; the
    /// content; a load). Never persisted: a loaded world grows at once.
    #[serde(skip)]
    grow_due: bool,
    /// The path searches waiting for their turn, orders and looks again,
    /// in two lanes ([`Searches`], D32). Never persisted, as `pending`.
    #[serde(skip)]
    searches: Searches,
    /// The bodies a player pilots, as the server last told
    /// ([`World::set_piloted`]): their searches go in the players' lane.
    /// Never persisted.
    #[serde(skip)]
    piloted: BTreeSet<BodyId>,
    /// Where each mind holds its home, as the brains last told
    /// ([`World::set_homes`]): spouses are apart by their homes (D49,
    /// amended 24 November 2026). Never persisted.
    #[serde(skip)]
    homes: BTreeMap<BodyId, Pos>,
    /// The body-nights slept at each place, in thousandths, fading day by
    /// day (D49, amended 26 November 2026: a camp long slept at fouls).
    /// Only places worth a body-night are kept: never the whole map.
    /// Absent from a snapshot when none.
    #[serde(
        serialize_with = "write_fouling",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    fouling: BTreeMap<(i32, i32), u64>,
    /// The hearths as counted at dawn, for the day's draws of illness.
    /// Never persisted: counted again.
    #[serde(skip)]
    hearths: illness::Hearths,
    /// The content's kinds of tree, the trees of a chunk are placed with
    /// ([`World::trees_in`]); set by [`World::take_content`]. Never
    /// persisted: the trees are placed again, the same, from the map.
    #[serde(skip)]
    species: Vec<content::Species>,
    /// The content's rock regions, the rock under a chunk is drawn with
    /// ([`World::rocks_in`]); set by [`World::take_content`]. Never
    /// persisted: the rocks are drawn again, the same, from the map.
    #[serde(skip)]
    geology: geology::Geology,
    /// The content's open biomes' trees, the trees of a chunk are placed
    /// with on a world cut from the atlas ([`World::trees_in`]); set by
    /// [`World::take_content`]. Never persisted either.
    #[serde(skip)]
    open_ground: open_ground::OpenGround,
    /// The content's flora, the plants bearing crops are placed with
    /// ([`World::bearers_in`]); set by [`World::take_content`]. Never
    /// persisted: the plants are placed again, the same, from the map.
    #[serde(skip)]
    flora: Vec<flora::Kind>,
    /// What the trees and plants bear, when, and how much (D45); set by
    /// [`World::take_content`]. Never persisted: a crop is worked out
    /// from the date, and only what men picked is kept.
    #[serde(skip)]
    crops: Vec<crops::Crop>,
    /// Each crop's kind's key ([`crops::key_of`]), worked out once with
    /// the content: the brains ask what is ripe for every plant they
    /// know at every choice. Not stored; made again with the content.
    #[serde(skip)]
    crop_keys: Vec<u64>,
    /// The trees of the chunks asked for lately, placed once and shared
    /// ([`ground::Ground`]) with the server's threads, which take a handle
    /// as they take the map ([`World::shared_ground`]). Derived from the
    /// map, the seed and the content: never persisted, empty on load.
    #[serde(skip)]
    ground_cache: Arc<ground::Ground>,
}

fn write_map<S: serde::Serializer>(map: &Arc<Map>, serializer: S) -> Result<S::Ok, S::Error> {
    serde::Serialize::serialize(map.as_ref(), serializer)
}

/// The map from its stored spec, checked first (`MapSpec::check`): a
/// crafted size or generator is refused, never made.
fn read_map<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Arc<Map>, D::Error> {
    let spec = <map::MapSpec as serde::Deserialize>::deserialize(deserializer)?;
    spec.check().map_err(serde::de::Error::custom)?;
    Ok(Arc::new(Map::from(spec)))
}

fn write_fouling<S: serde::Serializer>(
    fouling: &BTreeMap<(i32, i32), u64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(fouling.iter().map(|(&(x, y), &f)| (x, y, f)))
}

fn write_items<S: serde::Serializer>(
    items: &BTreeMap<ItemId, Item>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(items.values())
}

/// A world as a snapshot holds it: exactly what [`World`] writes, and
/// nothing that is derived from it.
#[derive(serde::Deserialize)]
struct Stored {
    calendar: Calendar,
    tick: Tick,
    #[serde(deserialize_with = "read_map")]
    map: Arc<Map>,
    bodies: Vec<Body>,
    next_body: u32,
    items: Vec<Item>,
    next_item: u64,
    #[serde(default)]
    natural: bool,
    /// Whether the life rule plays (D49); a world saved before it has it
    /// off (format 16).
    #[serde(default)]
    life: bool,
    /// The places fouled (format 19); none in a world saved before.
    #[serde(default)]
    fouling: Vec<(i32, i32, u64)>,
    /// The natural items taken, as snapshots held them up to format 8:
    /// since, each chunk's changes hold its own, stored beside.
    #[serde(default)]
    taken: BTreeSet<ItemId>,
}

impl<'de> serde::Deserialize<'de> for World {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = Stored::deserialize(deserializer)?;
        let mut world = Self {
            calendar: stored.calendar,
            tick: stored.tick,
            life: stored.life,
            map: stored.map,
            bodies: stored.bodies,
            next_body: stored.next_body,
            items: stored.items.into_iter().map(|i| (i.id, i)).collect(),
            next_item: stored.next_item,
            natural: stored.natural,
            changes: BTreeMap::new(),
            by_chunk: BTreeMap::new(),
            ground: BTreeMap::new(),
            kept: BTreeMap::new(),
            kept_in: BTreeMap::new(),
            kept_changes: 0,
            kept_parts: BTreeMap::new(),
            spawns: Vec::new(),
            pending: Vec::new(),
            grow_due: true,
            searches: Searches::default(),
            piloted: BTreeSet::new(),
            homes: BTreeMap::new(),
            fouling: stored
                .fouling
                .into_iter()
                .map(|(x, y, f)| ((x, y), f))
                .collect(),
            hearths: illness::Hearths::default(),
            species: Vec::new(),
            geology: geology::Geology::default(),
            open_ground: open_ground::OpenGround::default(),
            flora: Vec::new(),
            crops: Vec::new(),
            crop_keys: Vec::new(),
            ground_cache: Arc::new(ground::Ground::default()),
        };
        let standing: Vec<_> = world
            .bodies
            .iter()
            .map(|b| (b.id, b.pos().chunk()))
            .collect();
        for (id, chunk) in standing {
            world.enter_chunk(id, chunk);
        }
        let lying: Vec<_> = world
            .items
            .values()
            .filter_map(|i| Some((i.id, i.pos()?.chunk())))
            .collect();
        for (id, chunk) in lying {
            world.lay(id, chunk);
        }
        // An old snapshot's taken items become their chunks' changes, a
        // version for each item taken.
        for id in stored.taken {
            if items::is_natural(id) {
                world.change(items::chunk_of(id), |c| {
                    c.taken.insert(items::slot_of(id));
                });
            }
        }
        Ok(world)
    }
}

/// Side of the first version's valley, in tiles: about 3 km, thirty-six
/// minutes of walking (D31).
pub const VALLEY_SIZE: u32 = 3072;

/// Seed of the first version's valley: the public development seed, the
/// one every test world and every `--dev` server is cut from. A real
/// world is cut from a seed drawn at its birth and never told (the
/// security audit of 20 November 2026, F1; the project lead's call Q3).
pub const VALLEY_SEED: u64 = 1;

/// How far a body's eyes carry, in metres (a tile is a metre, D31): the
/// project lead's call of 11 November 2026 for how far other bodies are
/// told (the server's creature range takes it from here), and how far an
/// act on an item is answered truly (`World::sight`).
pub const EYES_RANGE_M: i32 = 150;

/// How near its leader, in tiles on each axis, a body must stand to take
/// the leader's way whole ([`Intention::Follow`], D56): within its camp
/// (two of its homes' reach, as the fire's circle) it sees where the
/// leader goes and walks behind it.
pub const FOLLOW_TILES: i32 = 12;

/// Whether `b` lies within the eyes' range of `a` ([`EYES_RANGE_M`]).
#[must_use]
pub fn within_eyes(a: Pos, b: Pos) -> bool {
    let (dx, dy) = (
        i64::from(a.x) - i64::from(b.x),
        i64::from(a.y) - i64::from(b.y),
    );
    dx * dx + dy * dy <= i64::from(EYES_RANGE_M).pow(2)
}

impl World {
    /// Creates the first version's valley at tick zero, with nobody in
    /// it yet (a player's body is born at the camp:
    /// [`World::spawn_at_camp`]) and the content's materials scattered
    /// over the ground by its spawn rules.
    pub fn new(calendar: Calendar, content: &Content) -> Self {
        Self::with_valley(calendar, content, VALLEY_SIZE)
    }

    /// The same, on a valley `size` tiles on a side: a smaller one starts
    /// far faster, for the tests and development (`--valley-size`).
    pub fn with_valley(calendar: Calendar, content: &Content, size: u32) -> Self {
        Self::on_valley(calendar, content, Map::valley(size, size, VALLEY_SEED))
    }

    /// The same, on a given valley: one made by an older generator (D24),
    /// or of another seed.
    pub fn on_valley(calendar: Calendar, content: &Content, map: Map) -> Self {
        let mut world = Self::empty(calendar, map);
        world.natural = true;
        world.take_content(content);
        world
    }

    /// Creates a world on a given map with no bodies.
    pub fn empty(calendar: Calendar, map: Map) -> Self {
        Self {
            calendar,
            tick: Tick(0),
            life: false,
            map: Arc::new(map),
            bodies: Vec::new(),
            next_body: 1,
            items: BTreeMap::new(),
            next_item: 1,
            natural: false,
            changes: BTreeMap::new(),
            by_chunk: BTreeMap::new(),
            ground: BTreeMap::new(),
            kept: BTreeMap::new(),
            kept_in: BTreeMap::new(),
            kept_changes: 0,
            kept_parts: BTreeMap::new(),
            spawns: Vec::new(),
            pending: Vec::new(),
            grow_due: true,
            searches: Searches::default(),
            piloted: BTreeSet::new(),
            homes: BTreeMap::new(),
            fouling: BTreeMap::new(),
            hearths: illness::Hearths::default(),
            species: Vec::new(),
            geology: geology::Geology::default(),
            open_ground: open_ground::OpenGround::default(),
            flora: Vec::new(),
            crops: Vec::new(),
            crop_keys: Vec::new(),
            ground_cache: Arc::new(ground::Ground::default()),
        }
    }

    /// Gives the world the content it places the ground's things with:
    /// the kinds of tree (D37) and the natural items' spawn rules. Called
    /// when the world is created and when it is loaded; nothing is placed
    /// here, only chunk by chunk when asked for. Every body is weighed
    /// again under the content's masses ([`Body::load_g`]), and one
    /// standing in a trunk's tile is let out of it.
    pub fn take_content(&mut self, content: &Content) {
        self.grow_due = true;
        for work in self.bodies.iter_mut().filter_map(|b| b.work.as_mut()) {
            work.crop = None;
        }
        self.species.clone_from(&content.species);
        self.spawns.clone_from(&content.spawns);
        self.geology.clone_from(&content.geology);
        self.open_ground.clone_from(&content.open_ground);
        self.flora.clone_from(&content.flora);
        self.crops.clone_from(&content.crops);
        self.crop_keys = self.crops.iter().map(crops::key_of).collect();
        for index in 0..self.bodies.len() {
            self.refresh_load(index, content);
        }
        self.let_out_of_trunks(content);
    }

    /// The trees standing in `chunk`, placed from the map and the seed
    /// ([`trees::trees_in`]) the first time they are asked for and held a
    /// while ([`ground::Ground`]): the same every time, nothing stored.
    pub fn trees_in(&self, chunk: ChunkPos) -> Arc<[trees::Tree]> {
        self.ground_cache.trees_in(
            &self.map,
            &self.species,
            &self.open_ground,
            self.seed(),
            chunk,
        )
    }

    /// The trees and plants of `chunk` that bear a crop in some season
    /// (D45): its trees, then its plants, in the order they are placed,
    /// the same every time. The plants are placed from the map, the seed
    /// and the trees around (a few milliseconds a chunk), and only when a
    /// kind of the flora bears something.
    pub fn bearers_in(&self, chunk: ChunkPos) -> Arc<[crops::Bearer]> {
        self.ground_cache
            .bearers_in(chunk, || self.placing().place(chunk))
    }

    /// What placing a chunk's bearers reads, borrowed from this world
    /// ([`ahead::Parts`]): the helpers placing ahead hold their own copy
    /// ([`ahead::Placing`]) and place the same.
    fn placing(&self) -> ahead::Parts<'_> {
        ahead::Parts {
            map: &self.map,
            species: &self.species,
            open: &self.open_ground,
            flora: &self.flora,
            crops: &self.crops,
            seed: self.seed(),
            ground: &self.ground_cache,
        }
    }

    /// The crops on `bearer`, of `chunk`, now (D45): those in season,
    /// each with what is on the plant now as it ripens and falls (D48,
    /// [`crops::Ripen`]) less what men picked this year; none empty.
    pub fn ripe_on(&self, chunk: ChunkPos, bearer: &crops::Bearer) -> Vec<crops::Ripe> {
        self.crops
            .iter()
            .enumerate()
            .filter(|(_, crop)| *crop.on == *bearer.kind)
            .filter_map(|(i, crop)| {
                let (year, grams) =
                    self.left_of(self.changes_in(chunk), bearer.source, bearer.at, i)?;
                Some(crops::Ripe {
                    gives: crop.gives.clone(),
                    year,
                    grams,
                })
            })
            .collect()
    }

    /// [`World::ripe_on`] for one crop, the one giving `gives` on the
    /// tree or plant `source` of `chunk`, of `kind`, standing at `at`:
    /// its year and the grams on it now, if any. Nothing is allocated:
    /// the brains and the gatherers ask it often.
    pub fn ripe_of(
        &self,
        chunk: ChunkPos,
        source: crops::Source,
        at: FinePos,
        kind: &str,
        gives: &str,
    ) -> Option<(u64, u32)> {
        self.crops
            .iter()
            .enumerate()
            .filter(|(_, crop)| crop.on == kind && crop.gives == gives)
            .find_map(|(i, _)| self.left_of(self.changes_in(chunk), source, at, i))
    }

    /// [`World::ripe_of`] for the crop at `crop` in the content's order
    /// (the brains hold that number, numbering what each kind bears): its
    /// year and the grams on it now, if any. Asked by number, nothing is
    /// compared: the brains ask it for every plant they know at every
    /// choice. The crop's own, not a twin's of the same kind and material:
    /// the content gives each kind one crop a material.
    pub fn ripe_of_crop(
        &self,
        chunk: ChunkPos,
        source: crops::Source,
        at: FinePos,
        crop: usize,
    ) -> Option<(u64, u32)> {
        (crop < self.crops.len()).then(|| self.left_of(self.changes_in(chunk), source, at, crop))?
    }

    /// [`World::ripe_of_crop`] with the chunk's changes already in hand
    /// ([`World::changes_in`]): a mind looking over a chunk's plants finds
    /// them once, not once a plant.
    pub fn ripe_of_crop_in(
        &self,
        changes: Option<&changes::ChunkChanges>,
        source: crops::Source,
        at: FinePos,
        crop: usize,
    ) -> Option<(u64, u32)> {
        (crop < self.crops.len()).then(|| self.left_of(changes, source, at, crop))?
    }

    /// The crop giving `gives` on the kind `kind`, in the content's order
    /// ([`World::ripe_of_crop`]'s number): the first such, and the only
    /// one, the content giving each kind one crop a material.
    fn crop_of(&self, kind: &str, gives: &str) -> Option<usize> {
        self.crops
            .iter()
            .position(|crop| crop.on == kind && crop.gives == gives)
    }

    /// What is left now of the crop at `crop` on the tree or plant
    /// `source` of `chunk`, standing at `at`: its year and grams, `None`
    /// when nothing.
    fn left_of(
        &self,
        changes: Option<&changes::ChunkChanges>,
        source: crops::Source,
        at: FinePos,
        crop: usize,
    ) -> Option<(u64, u32)> {
        let frame = r#gen::scatter::Frame::at(self.map.origin());
        let key = self.crop_keys.get(crop).copied();
        let crop = &self.crops[crop];
        let key = key.unwrap_or_else(|| crops::key_of(crop));
        let (year, grams) =
            crops::there_keyed(crop, key, &self.calendar, self.tick, self.seed(), frame, at)?;
        // Looked up without making the key's text: the picks of one
        // source sit together, few of them.
        let taken = changes
            .and_then(|c| {
                c.picked
                    .range((source, String::new())..)
                    .take_while(|((s, _), _)| *s == source)
                    .find(|((_, g), _)| *g == crop.gives)
                    .map(|(_, p)| *p)
            })
            .filter(|p| p.year == year)
            .map_or(0, |p| p.grams);
        let left = grams.saturating_sub(taken);
        (left > 0).then_some((year, left))
    }

    /// What the tree or plant `bearer` bears of the crop at `crop` (the
    /// content's order, as [`World::ripe_of_crop`]) in the crop's year
    /// `year`, in grams, before anything is picked (D45): its kind's lean,
    /// usual or rich year around it times its own share. `None` when the
    /// crop is not of its kind. Nothing in the world asks it: a measure's
    /// question (the vivarium's land food, 25 November 2026).
    #[must_use]
    pub fn bears_of_crop(&self, bearer: &crops::Bearer, crop: usize, year: u64) -> Option<u32> {
        let of = self.crops.get(crop).filter(|c| *c.on == *bearer.kind)?;
        let frame = r#gen::scatter::Frame::at(self.map.origin());
        let key = self
            .crop_keys
            .get(crop)
            .copied()
            .unwrap_or_else(|| crops::key_of(of));
        Some(crops::bears_keyed(
            of,
            key,
            &self.calendar,
            self.seed(),
            frame,
            bearer.at,
            year,
        ))
    }

    /// The fruit `tree`, of `chunk`, shows now (D48, [`crops::fruit_on`]).
    pub fn fruit_on(&self, chunk: ChunkPos, tree: &trees::Tree) -> u8 {
        crops::fruit_on(
            &self.species[tree.species],
            &self.crops,
            &self.calendar,
            self.tick,
            self.seed(),
            self.map.origin(),
            tree,
            self.changes_in(chunk).map(|c| &c.picked),
        )
    }

    /// The fruit `plant`, of `chunk`, shows now (D48,
    /// [`crops::plant_fruit_on`]): a berry shrub's, as a tree's.
    pub fn plant_fruit_on(&self, chunk: ChunkPos, plant: &flora::Plant) -> u8 {
        crops::plant_fruit_on(
            &self.flora[usize::from(plant.kind)],
            &self.crops,
            &self.calendar,
            self.tick,
            self.seed(),
            self.map.origin(),
            chunk,
            plant.at,
            self.changes_in(chunk).map(|c| &c.picked),
        )
    }

    /// Picks up to `grams` of the crop giving `gives` on `source`, of
    /// `chunk` (D45): the grams taken, at most what is left on it now, 0
    /// if none is there. Counted in the chunk's changes, whose version
    /// grows; the chunk's trees stay held, as picking changes no tree.
    pub fn pick(&mut self, chunk: ChunkPos, source: crops::Source, gives: &str, grams: u32) -> u32 {
        let Some(bearer) = self
            .bearers_in(chunk)
            .iter()
            .find(|b| b.source == source)
            .cloned()
        else {
            return 0;
        };
        let Some((year, left)) = self.ripe_of(chunk, source, bearer.at, &bearer.kind, gives) else {
            return 0;
        };
        let taken = grams.min(left);
        self.record_pick(chunk, source, gives, year, taken);
        taken
    }

    /// Counts `grams` picked of the crop giving `gives`, of `year`, on
    /// `source` of `chunk` ([`World::pick`]); nothing when 0.
    fn record_pick(
        &mut self,
        chunk: ChunkPos,
        source: crops::Source,
        gives: &str,
        year: u64,
        grams: u32,
    ) {
        if grams == 0 {
            return;
        }
        let changes = Arc::make_mut(self.changes.entry(chunk).or_default());
        // Looked up without making the key's text, as `left_of` reads it:
        // the key is made only for a crop picked for the first time.
        let known = changes
            .picked
            .range_mut((source, String::new())..)
            .take_while(|((s, _), _)| *s == source)
            .find(|((_, g), _)| *g == gives)
            .map(|(_, p)| p);
        match known {
            Some(picked) => {
                let before = if picked.year == year { picked.grams } else { 0 };
                *picked = changes::Picked {
                    year,
                    grams: before + grams,
                };
            }
            None => {
                changes
                    .picked
                    .insert((source, gives.to_owned()), changes::Picked { year, grams });
            }
        }
        changes.version += 1;
    }

    /// The rock under each tile of `chunk`, row by row (D42), drawn from
    /// the seed the first time it is asked for and held a while
    /// ([`ground::Ground`]); `None` on a map made by the first generator
    /// (D24), which has none, and off the map.
    pub fn rocks_in(&self, chunk: ChunkPos) -> Option<Arc<[Rock]>> {
        self.ground_cache
            .rocks_in(&self.map, &self.geology, self.seed(), chunk)
    }

    /// Does an adult tree's trunk block `pos` (D32)? A body never walks
    /// into such a tile.
    pub fn is_trunk(&self, pos: Pos) -> bool {
        self.map.has_chunk(pos.chunk())
            && self
                .ground_cache
                .trunks_in(
                    &self.map,
                    &self.species,
                    &self.open_ground,
                    self.seed(),
                    pos.chunk(),
                )
                .blocks(pos)
    }

    /// A lookup of the trunks tile by tile, for one walk's plan.
    fn trunks(&self) -> ground::TrunkLookup<'_> {
        self.ground_cache
            .trunks_at(&self.map, &self.species, &self.open_ground, self.seed())
    }

    /// The open tile nearest `target`, searching in growing squares out
    /// to [`TELEPORT_REACH`]: ground a body may walk on, with no trunk.
    fn open_near(&self, target: Pos, rules: &Content) -> Option<Pos> {
        let trunks = self.trunks();
        let open = |p: Pos| {
            self.map
                .get(p)
                .is_some_and(|t| rules.movement.cost(t).is_some())
                && !trunks.blocks(p)
        };
        (0..=TELEPORT_REACH).find_map(|r| {
            (-r..=r)
                .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
                .filter(|(dx, dy)| dx.abs() == r || dy.abs() == r)
                .map(|(dx, dy)| Pos::new(target.x.saturating_add(dx), target.y.saturating_add(dy)))
                .find(|p| open(*p))
        })
    }

    /// Lets out every body standing in a trunk's tile, as a snapshot made
    /// before trunks blocked the way may hold them (D32): each is stood on
    /// the nearest open tile, its walk dropped. Never stuck.
    fn let_out_of_trunks(&mut self, rules: &Content) {
        let caught: Vec<(usize, Pos)> = (0..self.bodies.len())
            .filter(|&i| self.is_trunk(self.bodies[i].pos()))
            .filter_map(|i| Some((i, self.open_near(self.bodies[i].pos(), rules)?)))
            .collect();
        for (index, tile) in caught {
            let body = &mut self.bodies[index];
            let (id, before) = (body.id, body.pos().chunk());
            body.at = tile.centre();
            body.route.clear();
            body.forget_way();
            if tile.chunk() != before {
                self.leave_chunk(id, before);
                self.enter_chunk(id, tile.chunk());
            }
        }
    }

    /// The ground's placed trees, shared: the server's threads read the
    /// same cache as the simulation, so a chunk's trees are placed once
    /// for every reader, and a change made here (a tree felled, later)
    /// shows to them at once.
    pub fn shared_ground(&self) -> Arc<ground::Ground> {
        Arc::clone(&self.ground_cache)
    }

    /// The world's calendar.
    pub fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    /// The current tick.
    pub fn tick(&self) -> Tick {
        self.tick
    }

    /// The current game time.
    pub fn time(&self) -> GameTime {
        self.calendar.time_at(self.tick)
    }

    /// The ground.
    pub fn map(&self) -> &Map {
        &self.map
    }

    /// The map, shared: a handle, not a copy of its tiles.
    pub fn shared_map(&self) -> Arc<Map> {
        Arc::clone(&self.map)
    }

    /// Every body, in id order.
    pub fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    /// A body by id.
    pub fn body(&self, id: BodyId) -> Option<&Body> {
        self.index_of(id).map(|index| &self.bodies[index])
    }

    /// The rest of a body's walk, as it will go if nothing changes: the
    /// points where it turns or its pace changes, each with the tick it
    /// stands there ([`timeline`]). Empty for a body standing still or
    /// unknown.
    pub fn walk_timeline(&self, id: BodyId, rules: &Content) -> Vec<(FinePos, u64)> {
        let Some(body) = self.body(id) else {
            return Vec::new();
        };
        timeline(
            body.at,
            body.route.iter().copied(),
            &self.map,
            &rules.movement,
            loaded_speed(&rules.movement, body.burden_g())
                * body.condition.pace_percent(&rules.needs)
                / 100,
            f64::from(self.calendar.ticks_per_second()),
            self.tick.0,
        )
    }

    /// Where a body is in `bodies`, found by halving (they are in id order).
    fn index_of(&self, id: BodyId) -> Option<usize> {
        self.bodies.binary_search_by_key(&id, |b| b.id).ok()
    }

    /// The bodies standing in a chunk, in id order.
    pub fn bodies_in(&self, chunk: ChunkPos) -> Vec<&Body> {
        self.by_chunk.get(&chunk).map_or_else(Vec::new, |ids| {
            ids.iter().filter_map(|id| self.body(*id)).collect()
        })
    }

    /// Notes a body standing in a chunk.
    fn enter_chunk(&mut self, id: BodyId, chunk: ChunkPos) {
        let ids = self.by_chunk.entry(chunk).or_default();
        if let Err(at) = ids.binary_search(&id) {
            ids.insert(at, id);
        }
    }

    /// Notes a body no longer standing in a chunk.
    fn leave_chunk(&mut self, id: BodyId, chunk: ChunkPos) {
        if let Some(ids) = self.by_chunk.get_mut(&chunk) {
            if let Ok(at) = ids.binary_search(&id) {
                ids.remove(at);
            }
            if ids.is_empty() {
                self.by_chunk.remove(&chunk);
            }
        }
    }

    /// The stored items: those a body touched, made or let fall, carried or
    /// on the ground, in id order. The ground's natural items are not
    /// among them until taken ([`World::items_in`]).
    pub fn items(&self) -> impl Iterator<Item = &Item> + '_ {
        self.items.values()
    }

    /// Whether the ground drops natural items (worlds made since 10
    /// November 2026).
    pub fn has_natural_items(&self) -> bool {
        self.natural
    }

    /// The natural items taken off the ground, chunk by chunk.
    pub fn taken(&self) -> impl Iterator<Item = ItemId> + '_ {
        self.changes.iter().flat_map(|(&chunk, changes)| {
            changes
                .taken
                .iter()
                .filter_map(move |&slot| items::natural_in_slot(chunk, slot))
        })
    }

    /// Whether the natural item `id` was taken off the ground.
    pub fn is_taken(&self, id: ItemId) -> bool {
        items::is_natural(id)
            && self
                .changes
                .get(&items::chunk_of(id))
                .is_some_and(|c| c.taken.contains(&items::slot_of(id)))
    }

    /// What men changed in each chunk they changed, in chunk order.
    pub fn chunk_changes(&self) -> impl Iterator<Item = (ChunkPos, &changes::ChunkChanges)> + '_ {
        self.changes.iter().map(|(&chunk, c)| (chunk, c.as_ref()))
    }

    /// What men changed in `chunk`, if anything.
    pub fn changes_in(&self, chunk: ChunkPos) -> Option<&changes::ChunkChanges> {
        self.changes.get(&chunk).map(AsRef::as_ref)
    }

    /// `chunk`'s version: how many times men changed it, 0 while it is as
    /// the seed made it.
    pub fn chunk_version(&self, chunk: ChunkPos) -> ground::Version {
        self.changes.get(&chunk).map_or(0, |c| c.version)
    }

    /// Puts back a chunk's changes as the storage kept them, version
    /// included: how a world is loaded.
    pub fn restore_changes(&mut self, chunk: ChunkPos, changes: changes::ChunkChanges) {
        self.ground_cache.set_version(chunk, changes.version);
        self.changes.insert(chunk, Arc::new(changes));
    }

    /// Changes `chunk` as `change` does, and counts it: the chunk's
    /// version grows by one.
    fn change(&mut self, chunk: ChunkPos, change: impl FnOnce(&mut changes::ChunkChanges)) {
        let changes = Arc::make_mut(self.changes.entry(chunk).or_default());
        change(changes);
        changes.version += 1;
        self.ground_cache.set_version(chunk, changes.version);
    }

    /// The seed the ground's natural items are made with: the map's.
    pub fn seed(&self) -> u64 {
        match self.map.spec() {
            MapSpec::Valley { seed, .. } => seed,
            MapSpec::Filled { .. } => 0,
        }
    }

    /// The ground's natural items of `chunk` still there, if this world
    /// has them.
    fn natural_in(&self, chunk: ChunkPos) -> Vec<Item> {
        if !self.natural {
            return Vec::new();
        }
        let mut natural = items::natural_items(&self.map, &self.spawns, self.seed(), chunk);
        if let Some(changes) = self.changes.get(&chunk) {
            natural.retain(|i| !changes.taken.contains(&items::slot_of(i.id)));
        }
        natural
    }

    /// The items lying on the ground in a chunk, stored and natural, in id
    /// order.
    pub fn items_in(&self, chunk: ChunkPos) -> Vec<Item> {
        let mut here: Vec<Item> = self.ground.get(&chunk).map_or_else(Vec::new, |ids| {
            ids.iter()
                .filter_map(|id| self.items.get(id).cloned())
                .collect()
        });
        here.extend(self.natural_in(chunk));
        here.sort_by_key(|i| i.id);
        here
    }

    /// The stored items lying in `chunk`, in no order: what bodies laid
    /// there, not the ground's own natural items ([`Self::items_in`] gives
    /// both). A brain looking for food reads these, chunk by chunk, rather
    /// than every item of the world.
    pub fn laid_in(&self, chunk: ChunkPos) -> impl Iterator<Item = &Item> + '_ {
        self.ground
            .get(&chunk)
            .into_iter()
            .flatten()
            .filter_map(|id| self.items.get(id))
    }

    /// A stored item by id (natural items are stored once taken).
    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.items.get(&id)
    }

    /// The record of food kept in a store by id ([`Intention::Keep`],
    /// D58): its material, grams, the tick it was made and where it is
    /// kept. Not an item of the world: [`World::item`] does not find it.
    pub fn kept(&self, id: ItemId) -> Option<&Item> {
        self.kept.get(&id)
    }

    /// How many times the records kept in stores changed since the world
    /// was made or read back (D58): what a brain reads of its stores holds
    /// while this does not move, but for the food that spoils meanwhile.
    #[must_use]
    pub fn kept_changes(&self) -> u64 {
        self.kept_changes
    }

    /// An item of the world, or a record kept in a store.
    fn item_or_kept(&self, id: ItemId) -> Option<&Item> {
        self.items.get(&id).or_else(|| self.kept.get(&id))
    }

    /// Lets go of the record `id` kept in a store, and of its chunk's
    /// note of it.
    fn unkeep(&mut self, id: ItemId) -> Option<Item> {
        let record = self.kept.remove(&id)?;
        self.kept_changes += 1;
        self.kept_parts.remove(&id);
        if let Some(pos) = record.pos()
            && let Some(ids) = self.kept_in.get_mut(&pos.chunk())
        {
            ids.remove(&id);
            if ids.is_empty() {
                self.kept_in.remove(&pos.chunk());
            }
        }
        Some(record)
    }

    /// The records kept in stores in `chunk`, in no order.
    pub(crate) fn kept_in(&self, chunk: ChunkPos) -> impl Iterator<Item = &Item> + '_ {
        self.kept_in
            .get(&chunk)
            .into_iter()
            .flatten()
            .filter_map(|id| self.kept.get(id))
    }

    /// Notes a stored item lying in a chunk: a change to what the chunk
    /// holds, which bumps its version as an item taken does (D4).
    fn lay(&mut self, id: ItemId, chunk: ChunkPos) {
        self.ground.entry(chunk).or_default().insert(id);
        self.change(chunk, |_| {});
    }

    /// Notes a stored item no longer lying in a chunk; its version bumps.
    fn lift(&mut self, id: ItemId, chunk: ChunkPos) {
        if let Some(ids) = self.ground.get_mut(&chunk) {
            ids.remove(&id);
            if ids.is_empty() {
                self.ground.remove(&chunk);
            }
        }
        self.change(chunk, |_| {});
    }

    /// Makes the natural item `id` a stored one, lying where it was, and
    /// remembers it was taken: what a body does with it is then done to a
    /// stored item. Nothing when it is not a natural item still there.
    fn materialise(&mut self, id: ItemId) {
        if !self.natural || !items::is_natural(id) || self.is_taken(id) {
            return;
        }
        if let Some(item) = items::natural_item(&self.map, &self.spawns, self.seed(), id) {
            let chunk = items::chunk_of(id);
            self.items.insert(id, item);
            // Laid as a stored item where it lay: one change, counted once.
            self.ground.entry(chunk).or_default().insert(id);
            self.change(chunk, |c| {
                c.taken.insert(items::slot_of(id));
            });
        }
    }

    /// Weighs a body's load again ([`Body::load_g`]), its loose food and
    /// what its containers hold (D57).
    fn refresh_load(&mut self, index: usize, rules: &Content) {
        let body = &self.bodies[index];
        let (mut load, mut food, mut holds) = (0_u32, 0_u32, 0_u32);
        for item in body.inventory().filter_map(|id| self.item(id)) {
            load = load.saturating_add(item.mass_g);
            if let Some(material) = rules.material(&item.material) {
                if material.food.is_some() {
                    food = food.saturating_add(item.mass_g);
                }
                holds = holds.saturating_add(material.holds_g);
            }
        }
        let body = &mut self.bodies[index];
        (body.load_g, body.food_g, body.holds_g) = (load, food, holds);
    }

    /// Grams a body carries in all, its items' own (the body keeps the
    /// result: [`Body::load_g`]).
    pub fn load_g(&self, body: &Body) -> u32 {
        body.inventory()
            .filter_map(|id| self.item(id))
            .map(Item::mass_g)
            .sum()
    }

    /// Lays `mass_g` grams of `material` on the ground at `pos`, made now.
    /// The material is the caller's responsibility: content is validated
    /// before the world is built.
    pub fn spawn_item(&mut self, material: &str, mass_g: u32, pos: Pos) -> ItemId {
        let id = ItemId(self.next_item);
        self.next_item += 1;
        self.items.insert(
            id,
            Item {
                id,
                material: material.to_owned(),
                mass_g: mass_g.max(1),
                made: self.tick,
                place: Place::Ground { pos },
            },
        );
        self.lay(id, pos.chunk());
        id
    }

    /// A record of `mass_g` grams of `material` kept in a store at `pos`,
    /// as a body lays one ([`Intention::Keep`], D58), made now: for the
    /// tests.
    #[cfg(test)]
    pub(crate) fn spawn_kept(&mut self, material: &str, mass_g: u32, pos: Pos) -> ItemId {
        let id = ItemId(self.next_item);
        self.next_item += 1;
        self.kept.insert(
            id,
            Item {
                id,
                material: material.to_owned(),
                mass_g: mass_g.max(1),
                made: self.tick,
                place: Place::Ground { pos },
            },
        );
        self.kept_in.entry(pos.chunk()).or_default().insert(id);
        self.kept_changes += 1;
        self.kept_parts.insert(id, 1);
        id
    }

    /// Adds a body standing at `pos`. The tile must be passable.
    ///
    /// # Panics
    /// If `pos` is not a passable tile of the map.
    pub fn spawn_body(&mut self, pos: Pos) -> BodyId {
        assert!(self.map.is_passable(pos), "spawn on a passable tile");
        self.add_body(pos)
    }

    /// Adds a body standing at `pos`, grown, a man, with no kin: the
    /// callers set what differs.
    fn add_body(&mut self, pos: Pos) -> BodyId {
        let id = BodyId(self.next_body);
        self.next_body += 1;
        self.bodies.push(Body {
            id,
            at: pos.centre(),
            route: VecDeque::new(),
            goal: None,
            skirted: Skirted::Not,
            went: Vec::new(),
            condition: Condition::healthy(),
            held: None,
            carried: Vec::new(),
            load_g: 0,
            food_g: 0,
            holds_g: 0,
            work: None,
            making: None,
            born_day: -25 * i64::from(self.calendar.days_per_age_year()),
            sex: Sex::Male,
            carried_by: None,
            carrying_g: 0,
            mother: None,
            father: None,
            spouse: None,
            apart_days: 0,
            courting: None,
            pregnant: None,
            reserve_dawns: Vec::new(),
        });
        self.enter_chunk(id, pos.chunk());
        self.grow_due = true;
        id
    }

    /// The world's day now, counted from its first.
    pub fn today(&self) -> i64 {
        i64::try_from(self.tick.0 / self.calendar.ticks_per_day().max(1)).unwrap_or(i64::MAX)
    }

    /// How old a body is, in thousandths of an age-year (lot 3 B; D09,
    /// amended 24 November 2026: ages follow the age-year, not the cycle).
    pub fn age_milli(&self, body: &Body) -> u64 {
        let days = u64::try_from(self.today() - body.born_day).unwrap_or(0);
        days * 1000 / u64::from(self.calendar.days_per_age_year().max(1))
    }

    /// Sets a body's birth and sex (lot 3 B): `age_days` old today.
    pub fn set_birth(&mut self, id: BodyId, age_days: u64, sex: Sex) {
        let today = self.today();
        if let Some(index) = self.index_of(id) {
            let body = &mut self.bodies[index];
            body.born_day = today - i64::try_from(age_days).unwrap_or(i64::MAX);
            body.sex = sex;
            self.grow_due = true;
        }
    }

    /// Sets a body's parents (D49) when it was not born in the world: the
    /// vivarium's departure gives its children and infants a mother and a
    /// father among the camp's couples (lot 3 C). Nothing for an unknown
    /// body; a body is never its own parent.
    pub fn set_parents(&mut self, id: BodyId, mother: Option<BodyId>, father: Option<BodyId>) {
        if let Some(index) = self.index_of(id) {
            let body = &mut self.bodies[index];
            body.mother = mother.filter(|&m| m != id);
            body.father = father.filter(|&f| f != id);
        }
    }

    /// Sets a body's fat reserve to `percent` of its whole (the life lot,
    /// 25 November 2026): the vivarium's start with an age pyramid gives
    /// its people unequal reserves rather than all full. Its whole is the
    /// content's for its age and sex (D47), as the next tick sizes it.
    /// Nothing for an unknown body; over 100, full.
    pub fn set_reserve_percent(&mut self, id: BodyId, percent: u32, rules: &Content) {
        let Some(index) = self.index_of(id) else {
            return;
        };
        let age = self.age_milli(&self.bodies[index]);
        let body = &mut self.bodies[index];
        let size = rules.needs.ages.reserve_permille(age, body.sex);
        let whole = rules.needs.reserve_j() * u64::from(size) / 1000;
        body.condition.deficit_j = whole * u64::from(100 - percent.min(100)) / 100;
        self.grow_due = true;
    }

    /// Gives the infant `infant` to `carrier` to carry and nurse (lot 3 B):
    /// it stands where its carrier stands from now on. False when either
    /// is unknown or they are the same.
    pub fn carry(&mut self, carrier: BodyId, infant: BodyId) -> bool {
        if carrier == infant || self.index_of(carrier).is_none() {
            return false;
        }
        let Some(index) = self.index_of(infant) else {
            return false;
        };
        let body = &mut self.bodies[index];
        body.carried_by = Some(carrier);
        body.route.clear();
        body.goal = None;
        body.work = None;
        self.grow_due = true;
        true
    }

    /// Adds a body on the grass nearest the camp where no body stands and
    /// no trunk grows, searching in growing squares: a player's body,
    /// born there (provisional, until souls choose their bodies, D34).
    /// `None` when the map has no such tile.
    pub fn spawn_at_camp(&mut self) -> Option<BodyId> {
        self.spawn_near(self.map.camp())
    }

    /// A body is born on the free grass nearest `camp`, as at the map's
    /// camp ([`World::spawn_at_camp`]): a tribe's own camp (D41, the
    /// vivarium's two tribes). None when no free grass is left.
    pub fn spawn_near(&mut self, camp: Pos) -> Option<BodyId> {
        let taken: BTreeSet<Pos> = self.bodies.iter().map(Body::pos).collect();
        let reach =
            i32::try_from(self.map.width().max(self.map.height())).expect("map fits in i32");
        let trunks = self.trunks();
        let free = |p: Pos| {
            self.map.get(p) == Some(Terrain::Grass) && !taken.contains(&p) && !trunks.blocks(p)
        };
        let pos = (0..=reach).find_map(|r| {
            (-r..=r)
                .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
                .filter(|(dx, dy)| dx.abs() == r || dy.abs() == r)
                .map(|(dx, dy)| Pos::new(camp.x + dx, camp.y + dy))
                .find(|p| free(*p))
        })?;
        Some(self.spawn_body(pos))
    }

    /// Is it night? The day starts at dawn; night covers its last part,
    /// shorter in summer than in winter (D09).
    pub fn is_night(&self, rules: &Content) -> bool {
        let now = self.time();
        now.tick_of_day * 100
            >= u64::from(rules.needs.night_from_percent.get(now.season))
                * self.calendar.ticks_per_day()
    }

    /// Items lying on `pos` or on one of its eight neighbours, stored and
    /// natural, in id order.
    pub fn items_within_reach(&self, pos: Pos) -> Vec<Item> {
        let near = |p: Pos| (p.x - pos.x).abs() <= 1 && (p.y - pos.y).abs() <= 1;
        let mut chunks: Vec<ChunkPos> = [(-1, -1), (1, 1), (-1, 1), (1, -1)]
            .iter()
            .map(|(dx, dy)| Pos::new(pos.x + dx, pos.y + dy).chunk())
            .collect();
        chunks.sort_unstable_by_key(|c| (c.x, c.y));
        chunks.dedup();
        let mut here: Vec<Item> = chunks
            .into_iter()
            .flat_map(|c| self.items_in(c))
            .filter(|i| i.pos().is_some_and(near))
            .collect();
        here.sort_by_key(|i| i.id);
        here
    }

    /// Takes a stored item out of the world, wherever it is.
    fn remove_item(&mut self, id: ItemId) -> Option<Item> {
        let item = self.items.remove(&id)?;
        if let Some(pos) = item.pos() {
            self.lift(id, pos.chunk());
        }
        Some(item)
    }

    /// Queues an intention for the next tick.
    pub fn queue(&mut self, intention: Intention) {
        // A walk's target is brought near the map here, before any lane
        // or search holds it: a client's coordinate at the ends of `i32`
        // (D03) overflowed the walk's arithmetic (the second audit of 20
        // November 2026). A target off the map stays off the map: the
        // walk toward it stops at the map's edge, as before.
        let intention = match intention {
            Intention::MoveTo { body, target } => Intention::MoveTo {
                body,
                target: self.map.clamp(target),
            },
            Intention::Follow {
                body,
                leader,
                target,
            } => Intention::Follow {
                body,
                leader,
                target: self.map.clamp(target),
            },
            other => other,
        };
        self.pending.push(intention);
    }

    /// Every body on a route walks its tick, and pays the walk (D50,
    /// amended 24 November 2026).
    fn walk_all(&mut self, rules: &Content, events: &mut Vec<Event>) {
        let per_second = f64::from(self.calendar.ticks_per_second());
        for index in 0..self.bodies.len() {
            if self.bodies[index].route.is_empty() {
                continue;
            }
            let body = &self.bodies[index];
            let speed = loaded_speed(&rules.movement, body.burden_g())
                * body.condition.pace_percent(&rules.needs)
                / 100;
            let before = self.bodies[index].pos().chunk();
            let from = self.bodies[index].at;
            walk(
                &mut self.bodies[index],
                &self.map,
                &rules.movement,
                speed,
                per_second,
                events,
            );
            let body = &mut self.bodies[index];
            let (dx, dy) = (
                i64::from(body.at.x) - i64::from(from.x),
                i64::from(body.at.y) - i64::from(from.y),
            );
            body.condition.walked(
                dx.unsigned_abs().pow(2) + dy.unsigned_abs().pow(2),
                &rules.needs,
            );
            let after = self.bodies[index].pos().chunk();
            if after != before {
                let id = self.bodies[index].id;
                self.leave_chunk(id, before);
                self.enter_chunk(id, after);
            }
        }
    }

    /// Advances the world by exactly one fixed tick under the content's
    /// rules, appending the events of that tick to `events`.
    pub fn step(&mut self, rules: &Content, events: &mut Vec<Event>) {
        let was_night = self.is_night(rules);
        self.tick.0 += 1;
        let now = self.time();
        if now.is_day_start() {
            events.push(Event::DayStarted { day: now.day });
            self.spoil(rules, events);
        }
        if now.is_season_start() {
            events.push(Event::SeasonStarted {
                year: now.cycle,
                season: now.season,
            });
        }
        if now.is_day_start()
            && now.day_of_age_quarter() < u64::from(rules.needs.life.conceive_nights)
        {
            self.conceive(rules, now.age_year, now.age_quarter);
        }
        if now.is_day_start() {
            self.mark_reserves(rules);
        }
        let night = self.is_night(rules);
        if night && !was_night {
            events.push(Event::NightFell { day: now.day });
        }

        // Orders to walk join their body's lane of searches; any other
        // intention is applied at once. A body that walked straight for
        // want of a way and stopped joins it too, to look again from
        // where it stands. The searches then run in turn under the cap
        // of work (D32).
        let pending = std::mem::take(&mut self.pending);
        for intention in pending {
            // An infant does nothing of itself: carried, or left where its
            // carrier died, it cannot walk (lot 3 B).
            let infant = self
                .body(intention.body())
                .is_some_and(|b| self.is_infant(b, rules));
            if infant {
                if let Intention::MoveTo { body, target } | Intention::Follow { body, target, .. } =
                    intention
                {
                    events.push(Event::MoveRefused {
                        body,
                        target,
                        reason: Blockage::NoWay,
                    });
                }
                continue;
            }
            if let Intention::Follow {
                body,
                leader,
                target,
            } = intention
            {
                if self.follow(body, leader, rules, events) {
                    continue;
                }
                let lane = self.lane(body);
                self.searches.ask(body, Some(target), lane);
            } else if let Intention::MoveTo { body, target } = intention {
                let lane = self.lane(body);
                self.searches.ask(body, Some(target), lane);
            } else {
                self.apply(&intention, rules, events);
            }
        }
        for index in 0..self.bodies.len() {
            let body = &self.bodies[index];
            if body.goal.is_some()
                && body.route.is_empty()
                && !body.condition.asleep
                && !self.searches.waiting.contains_key(&body.id)
            {
                let (id, lane) = (body.id, self.lane(body.id));
                self.searches.ask(id, None, lane);
            }
        }
        self.search(rules, events);

        self.walk_all(rules, events);

        self.follow_carriers();
        self.labour(rules, events);
        self.make(rules, events);
        self.fare(rules, night, now.is_day_start(), events);
        self.live(rules, now.tick_of_day, events);
    }

    /// Tells the world which bodies a player pilots, all of them: their
    /// path searches go in the players' lane, with its share of the work
    /// (D32). The server's input, as intentions are; never persisted.
    pub fn set_piloted(&mut self, bodies: impl IntoIterator<Item = BodyId>) {
        self.piloted = bodies.into_iter().collect();
    }

    /// Tells the world where each mind holds its home (D49, amended 24
    /// November 2026): the brains' input, as intentions are; never
    /// persisted. Spouses are apart by their homes when both have one,
    /// else by where they stand: one sleeping out keeps the couple.
    pub fn set_homes(&mut self, homes: impl IntoIterator<Item = (BodyId, Pos)>) {
        self.homes = homes.into_iter().collect();
    }

    /// The lane of a body's searches.
    fn lane(&self, body: BodyId) -> usize {
        if self.piloted.contains(&body) {
            PLAYERS
        } else {
            OTHERS
        }
    }

    /// Runs the waiting searches under the tick's cap of work (D32): the
    /// players' lane up to its share, the others' up to theirs and what
    /// the players left, the players' again with what the others left.
    /// In each lane, in the order they came; the search at the head runs
    /// as long as any of its share is left, even past it, and the next
    /// tick repays the excess: none waits for ever. Every search counts
    /// one tile at least.
    fn search(&mut self, rules: &Content, events: &mut Vec<Event>) {
        let movement = &rules.movement;
        let cap = u64::from(movement.search_work_per_tick);
        let allowance = cap.saturating_sub(self.searches.owed);
        self.searches.owed = self.searches.owed.saturating_sub(cap);
        let players = allowance * u64::from(movement.player_share_percent.min(100)) / 100;
        let mut used = [0u64; 2];
        let mut run = |world: &mut Self, lane: usize, limit: u64, used: &mut [u64; 2]| {
            while used[lane] < limit {
                let Some((body, order)) = world.searches.pop(lane) else {
                    break;
                };
                used[lane] += u64::from(world.run_search(body, order, rules, events).max(1));
            }
        };
        run(self, PLAYERS, players, &mut used);
        let spare = if self.searches.lanes[PLAYERS].is_empty() {
            players.saturating_sub(used[PLAYERS])
        } else {
            0
        };
        let others = allowance - players + spare;
        run(self, OTHERS, others, &mut used);
        if self.searches.lanes[OTHERS].is_empty() {
            let spare = others.saturating_sub(used[OTHERS]);
            run(self, PLAYERS, players + spare, &mut used);
        }
        self.searches.owed += (used[PLAYERS] + used[OTHERS]).saturating_sub(allowance);
    }

    /// One waiting search: a body's order to walk, or a look again. The
    /// tiles it expanded.
    fn run_search(
        &mut self,
        id: BodyId,
        order: Option<Pos>,
        rules: &Content,
        events: &mut Vec<Event>,
    ) -> u32 {
        let Some(index) = self.index_of(id) else {
            if let Some(target) = order {
                events.push(Event::MoveRefused {
                    body: id,
                    target,
                    reason: Blockage::NoWay,
                });
            }
            return 0;
        };
        if let Some(target) = order {
            self.wake(index, events);
            return self.set_off(index, target, true, rules, events);
        }
        let body = &self.bodies[index];
        match body.goal {
            Some(goal) if body.route.is_empty() && !body.condition.asleep => {
                self.look_again(index, goal, rules, events)
            }
            _ => 0,
        }
    }

    /// Lets every body hunger, freeze, tire, rest, heal — and die.
    /// Whether a body is still an infant, carried and nursed (lot 3 B).
    pub fn is_infant(&self, body: &Body, rules: &Content) -> bool {
        rules.needs.ages.is_infant(self.age_milli(body))
    }

    /// Sets each body's size, burn and pace from its age and sex, and
    /// what its carrier bears of it; weans the infants grown old enough:
    /// set down where their carrier stands, they walk from now on.
    fn grow(&mut self, rules: &Content, events: &mut Vec<Event>) {
        let ages = &rules.needs.ages;
        let life = self.life;
        let mut carrying: BTreeMap<BodyId, u32> = BTreeMap::new();
        for index in 0..self.bodies.len() {
            let age = self.age_milli(&self.bodies[index]);
            let body = &mut self.bodies[index];
            let Some(carrier) = body.carried_by else {
                continue;
            };
            if ages.is_infant(age) {
                *carrying.entry(carrier).or_default() += ages.infant_mass_g(age);
            } else {
                body.carried_by = None;
                events.push(Event::BodyStopped {
                    body: body.id,
                    at: body.at,
                });
            }
        }
        for index in 0..self.bodies.len() {
            let age = self.age_milli(&self.bodies[index]);
            let body = &mut self.bodies[index];
            let size = ages.size_permille(age, body.sex);
            body.carrying_g = carrying.get(&body.id).copied().unwrap_or(0);
            let nursing = body.sex == Sex::Female && body.carrying_g > 0;
            let c = &mut body.condition;
            c.size_permille = size;
            c.reserve_permille = ages.reserve_permille(age, body.sex);
            // A nursing woman needs more; a pregnant one too (D49, when
            // the life rule plays), the nursing taking over once the child
            // is born. One adopting an infant while she carries another
            // unborn needs the nursing's.
            c.burn_permille = if nursing {
                size * ages.nursing_percent / 100
            } else if life && body.pregnant.is_some() {
                size * rules.needs.life.pregnant_percent / 100
            } else {
                size
            };
            // One carrying an infant goes slower (D56).
            let carrying = if body.carrying_g > 0 {
                ages.carrying_pace_percent
            } else {
                100
            };
            c.age_pace_percent = ages.age_pace_percent(age) * carrying / 100;
        }
    }

    /// The infants stand where their carriers stand.
    /// Carriers are never carried themselves: each is looked up where
    /// it stands, no map of every body made a tick.
    fn follow_carriers(&mut self) {
        for index in 0..self.bodies.len() {
            let Some(carrier) = self.bodies[index].carried_by else {
                continue;
            };
            let Some(there) = self.index_of(carrier).map(|i| self.bodies[i].at) else {
                continue;
            };
            let before = self.bodies[index].pos().chunk();
            self.bodies[index].at = there;
            let after = there.tile().chunk();
            if after != before {
                let id = self.bodies[index].id;
                self.leave_chunk(id, before);
                self.enter_chunk(id, after);
            }
        }
    }

    /// The infants a dead carrier bore go to the nearest woman within the
    /// content's reach, grown and not carried herself; with none, they
    /// are left where it died.
    fn orphan(&mut self, carrier: BodyId, at: Pos, rules: &Content) {
        let ages = &rules.needs.ages;
        let reach = i32::try_from(ages.adopt_within_tiles).unwrap_or(i32::MAX);
        let woman_from = u64::from(ages.woman_from_years) * 1000;
        let nurse = self
            .bodies
            .iter()
            .filter(|b| {
                b.sex == Sex::Female && b.carried_by.is_none() && self.age_milli(b) >= woman_from
            })
            .map(|b| {
                let p = b.pos();
                ((p.x - at.x).abs().max((p.y - at.y).abs()), b.id)
            })
            .filter(|&(d, _)| d <= reach)
            .min()
            .map(|(_, id)| id);
        for body in &mut self.bodies {
            if body.carried_by == Some(carrier) {
                body.carried_by = nurse;
            }
        }
        self.grow_due = true;
    }

    /// Every body's state as a carrier, for the infants it nurses: whether
    /// it has milk (a woman whose reserve is not spent), its cold, fatigue
    /// and sleep.
    /// Only the bodies carrying one: a handful, read by id, where every
    /// body's state was gathered each tick for them.
    fn carriers(&self, needs: &content::Needs) -> BTreeMap<BodyId, (bool, Gauge, Gauge, bool)> {
        let by: BTreeSet<BodyId> = self.bodies.iter().filter_map(|b| b.carried_by).collect();
        by.into_iter()
            .filter_map(|id| self.body(id))
            .map(|b| {
                let c = &b.condition;
                let milk = b.sex == Sex::Female && c.deficit_j < c.reserve_j(needs);
                (b.id, (milk, c.cold, c.fatigue, c.asleep))
            })
            .collect()
    }

    fn fare(&mut self, rules: &Content, night: bool, day_start: bool, events: &mut Vec<Event>) {
        let per_day = self.calendar.ticks_per_day();
        let step = |rate_per_day: u32| u64::from(rate_per_day) * GAUGE_SCALE / per_day;
        let needs = &rules.needs;
        let tick = self.tick.0;
        let season = self.time().season;
        // Size, burn and pace follow the age, which moves once a day,
        // and what changed among the bodies since (`grow_due`).
        if day_start || self.grow_due {
            self.grow(rules, events);
            self.grow_due = false;
        }
        // The carriers' state is read for the nursed infants alone.
        let carriers = if self.bodies.iter().any(|b| b.carried_by.is_some()) {
            self.carriers(needs)
        } else {
            BTreeMap::new()
        };
        let mut died = Vec::new();
        for body in &mut self.bodies {
            let before = body.condition.summary();
            let effort = !body.route.is_empty() || body.work.is_some();
            let nursed = body.carried_by.and_then(|by| carriers.get(&by)).copied();
            let c = &mut body.condition;
            // What it burns this tick, in joules: whole, the remainders
            // carried over the day, so a day burns its need exactly. An
            // infant nursed burns nothing of its own: its carrier does.
            let need = c.need_j_per_day(needs);
            let burn = need * tick / per_day - need * tick.saturating_sub(1) / per_day;
            if let Some((true, cold, fatigue, asleep)) = nursed {
                // On its carrier's breast and back: fed, watered, as warm
                // and as rested as she is.
                if day_start {
                    c.eaten_today_j = 0;
                }
                c.deficit_j = c.deficit_j.saturating_sub(burn);
                c.set_appetite(needs);
                c.vitality.raise(step(needs.vitality_regain_per_day));
                c.thirst = Gauge::at(0);
                c.cold = cold;
                c.fatigue = fatigue;
                c.asleep = asleep;
                if c.summary() != before {
                    events.push(Event::ConditionChanged {
                        body: body.id,
                        summary: c.summary(),
                    });
                }
                continue;
            }
            c.deficit_j = (c.deficit_j + burn).min(c.reserve_j(needs));
            if day_start {
                c.eaten_today_j = 0;
            }
            c.set_appetite(needs);
            c.thirst.raise(thirst_raise(needs, season, effort, per_day));
            if night {
                c.cold.raise(step(night_cold(needs, season)));
            } else {
                c.cold.lower(step(needs.cold_relief_per_day));
            }
            if c.asleep {
                c.fatigue.lower(step(needs.rest_per_day_asleep));
                // Rested, one sleeps on through the night and wakes with the day.
                if c.fatigue.is_empty() && !night {
                    c.asleep = false;
                }
            } else {
                c.fatigue.raise(step(needs.fatigue_per_day_awake));
                if c.fatigue.is_full() {
                    c.asleep = true;
                    // Collapsing on its way, it stops there: told as any
                    // stop, or those who saw it set off would walk it on.
                    if !body.route.is_empty() {
                        events.push(Event::BodyStopped {
                            body: body.id,
                            at: body.at,
                        });
                    }
                    body.route.clear();
                    body.forget_way();
                }
            }
            let c = &mut body.condition;
            let mut cause = None;
            // Hunger itself does no harm: the reserve is burnt first, and
            // only a spent one starves (lot 3).
            if c.deficit_j >= c.reserve_j(needs) && needs.starving_per_day > 0 {
                c.vitality.lower(step(needs.starving_per_day));
                cause = Some(Death::Starved);
            }
            if c.cold.is_full() {
                c.vitality.lower(step(needs.freezing_per_day));
                cause = cause.or(Some(Death::Froze));
            }
            if c.thirst.is_full() {
                c.vitality.lower(step(needs.dehydration_per_day));
                cause = cause.or(Some(Death::Dehydrated));
            }
            if cause.is_none() {
                c.vitality.raise(step(needs.vitality_regain_per_day));
            }
            if c.vitality.is_empty() {
                died.push((body.id, cause.expect("only a full need drains vitality")));
            } else if c.summary() != before {
                events.push(Event::ConditionChanged {
                    body: body.id,
                    summary: c.summary(),
                });
            }
        }
        for (id, cause) in died {
            self.die(id, cause, rules, events);
        }
    }

    /// Turns a body into a corpse on the ground.
    fn die(&mut self, id: BodyId, cause: Death, rules: &Content, events: &mut Vec<Event>) {
        let Some(index) = self.index_of(id) else {
            return;
        };
        let body = self.bodies.remove(index);
        self.grow_due = true;
        let at = body.pos();
        self.leave_chunk(id, at.chunk());
        self.orphan(id, at, rules);
        // The dead leaves the world and keeps nothing of it; its children
        // keep its id as their parent, its spouse is widowed (D49).
        let mut widowed = Vec::new();
        for other in &mut self.bodies {
            if other.spouse == Some(id) {
                other.spouse = None;
                other.apart_days = 0;
                widowed.push(other.id);
            }
        }
        // A widower whose second wife lives takes her as his own (D49).
        self.first_wife_again(widowed);
        for item in body.inventory().collect::<Vec<_>>() {
            let Some(dropped) = self.items.get_mut(&item) else {
                continue;
            };
            dropped.place = Place::Ground { pos: at };
            let material = dropped.material.clone();
            let mass_g = dropped.mass_g;
            self.lay(item, at.chunk());
            let version = self.chunk_version(at.chunk());
            events.push(Event::ItemAdded {
                item,
                material,
                mass_g,
                at,
                version,
            });
        }
        let corpse_g = rules.material("corpse").map_or(1, Material::mass_g);
        let corpse = self.spawn_item("corpse", corpse_g, at);
        events.push(Event::ItemAdded {
            item: corpse,
            material: "corpse".to_owned(),
            mass_g: corpse_g,
            at,
            version: self.chunk_version(at.chunk()),
        });
        events.push(Event::BodyDied {
            body: id,
            cause,
            at,
            corpse,
        });
    }

    /// Advances the world by `n` fixed ticks, appending events to `events`.
    pub fn advance(&mut self, n: u64, rules: &Content, events: &mut Vec<Event>) {
        for _ in 0..n {
            self.step(rules, events);
        }
    }

    fn apply(&mut self, intention: &Intention, rules: &Content, events: &mut Vec<Event>) {
        // An act on an item is judged by the body's reach and eyes before
        // anything is done or told of the item (the security audit of 20
        // November 2026, F2 and M1); one on a natural item then makes it
        // a stored one, and goes as for any other.
        if let Some((act, item)) = intention.item_act() {
            let body = intention.body();
            if let Some(index) = self.index_of(body) {
                if let Err(reason) = self.sight(index, item) {
                    return Self::refuse(events, body, act, item, reason);
                }
                self.materialise(item);
            }
        }
        // Any other intention stops the work its body is busy at, and the
        // gesture (D57).
        if !matches!(intention, Intention::Gather { .. })
            && let Some(index) = self.index_of(intention.body())
        {
            self.stop_work(index, WorkEnd::Interrupted, events);
            self.bodies[index].making = None;
        }
        match *intention {
            Intention::Gather {
                body: id,
                chunk,
                source,
                ref gives,
            } => self.gather(id, chunk, source, gives, rules, events),
            Intention::MoveTo { body: id, target } => {
                let Some(index) = self.index_of(id) else {
                    events.push(Event::MoveRefused {
                        body: id,
                        target,
                        reason: Blockage::NoWay,
                    });
                    return;
                };
                self.wake(index, events);
                self.set_off(index, target, true, rules, events);
            }
            Intention::Teleport { body: id, target } => self.teleport(id, target, rules, events),
            Intention::Follow {
                body: id, target, ..
            } => {
                if let Some(index) = self.index_of(id) {
                    self.wake(index, events);
                    self.set_off(index, target, true, rules, events);
                }
            }
            Intention::Eat { body: id, item } => self.eat(id, item, rules, events),
            Intention::Feed {
                body: id,
                young,
                item,
            } => self.feed(id, young, item, rules, events),
            Intention::Carry {
                body: id,
                infant,
                carrier,
            } => self.hand_infant(id, infant, carrier, rules),
            Intention::PickUp { body: id, item } => self.pick_up(id, item, rules, events),
            Intention::Drop { body: id, item } => self.drop(id, item, rules, events),
            Intention::Keep { body: id, item } => self.keep(id, item, rules, events),
            Intention::Hold { body: id, item } => self.hold(id, item, rules, events),
            Intention::Stow { body: id, item } => self.stow(id, item, rules, events),
            Intention::Drink { body: id, at } => self.drink(id, at, events),
            Intention::Cut {
                body: id,
                with,
                chunk,
                tree,
            } => self.cut(id, with, (chunk, tree), rules, events),
            Intention::Fold { body: id, item } => self.fold(id, item, rules, events),
            Intention::Sleep { body: id } => {
                if let Some(index) = self.index_of(id) {
                    let body = &mut self.bodies[index];
                    if !body.condition.asleep {
                        body.condition.asleep = true;
                        body.forget_way();
                        if !body.route.is_empty() {
                            body.route.clear();
                            events.push(Event::BodyStopped {
                                body: id,
                                at: body.at,
                            });
                        }
                        events.push(Event::ConditionChanged {
                            body: id,
                            summary: body.condition.summary(),
                        });
                    }
                }
            }
            Intention::Wake { body: id } => {
                if let Some(index) = self.index_of(id) {
                    self.wake(index, events);
                }
            }
        }
    }

    /// Judges an act on `item` by the body's reach and eyes, before
    /// anything is done or told of the item. `Ok` when the body carries
    /// it, or it lies on the body's tile or next to it (a natural item
    /// not yet made counts where its id says it would lie). Otherwise the
    /// true reason within the eyes' range ([`EYES_RANGE_M`]): `OutOfReach`
    /// for a piece on the ground, or a natural slot whatever it holds,
    /// `NotCarried` for one another body has; past the eyes, one and the
    /// same `Unknown` whether the item exists or not, as for an id that
    /// names nothing: a refusal tells nothing of what lies beyond the
    /// eyes, and a far natural item is never made for a probe (the
    /// security audit of 20 November 2026, F2 and M1).
    fn sight(&self, index: usize, item: ItemId) -> Result<(), Refusal> {
        let body = &self.bodies[index];
        let (at, reason) = match self.item(item).map(|found| found.place) {
            Some(Place::Carried { body: carrier }) if carrier == body.id => return Ok(()),
            Some(Place::Carried { body: carrier }) => {
                let Some(carrier) = self.index_of(carrier) else {
                    return Err(Refusal::Unknown);
                };
                (self.bodies[carrier].pos(), Refusal::NotCarried)
            }
            Some(Place::Ground { pos }) => (pos, Refusal::OutOfReach),
            None if self.kept.contains_key(&item) => match self.kept.get(&item).map(|k| k.place) {
                Some(Place::Ground { pos }) => (pos, Refusal::OutOfReach),
                _ => return Err(Refusal::Unknown),
            },
            None => match items::tile_of(item) {
                Some(tile) => (tile, Refusal::OutOfReach),
                None => return Err(Refusal::Unknown),
            },
        };
        let here = body.pos();
        if (at.x - here.x).abs() <= 1 && (at.y - here.y).abs() <= 1 {
            match reason {
                Refusal::NotCarried => Err(reason),
                _ => Ok(()),
            }
        } else if within_eyes(here, at) {
            Err(reason)
        } else {
            Err(Refusal::Unknown)
        }
    }

    /// Where an item is relative to a body: within reach on the ground,
    /// carried by it, or neither ([`World::sight`] says why not).
    fn reach(&self, index: usize, item: ItemId) -> Result<Place, Refusal> {
        self.sight(index, item)?;
        self.item_or_kept(item)
            .map(|found| found.place)
            .ok_or(Refusal::Unknown)
    }

    fn refuse(events: &mut Vec<Event>, body: BodyId, act: ItemAct, item: ItemId, reason: Refusal) {
        events.push(Event::ActRefused {
            body,
            act,
            item,
            reason,
        });
    }

    /// Why a body cannot let go of an item it does not have.
    fn not_had(&self, item: ItemId) -> Refusal {
        if self.item(item).is_some() {
            Refusal::NotCarried
        } else {
            Refusal::Unknown
        }
    }

    /// Weighs a body's load again and tells what it carries now.
    fn inventory_changed(&mut self, index: usize, rules: &Content, events: &mut Vec<Event>) {
        self.refresh_load(index, rules);
        self.inventory_told(index, rules, events);
    }

    /// Tells that a body's inventory changed, its load kept already.
    fn inventory_told(&self, index: usize, rules: &Content, events: &mut Vec<Event>) {
        let body = &self.bodies[index];
        events.push(Event::InventoryChanged {
            body: body.id,
            held: body.held,
            carried: body.carried.clone(),
            load_g: body.load_g,
            capacity_g: body.capacity_g(&rules.movement),
        });
    }

    /// A development teleport (`Intention::Teleport`): the body stands at
    /// once on the walkable tile nearest `target`, ring by ring, awake and
    /// with no route; it is seen to move there and stop.
    fn teleport(&mut self, id: BodyId, target: Pos, rules: &Content, events: &mut Vec<Event>) {
        let refused = Event::MoveRefused {
            body: id,
            target,
            reason: Blockage::NoWay,
        };
        let Some(index) = self.index_of(id) else {
            events.push(refused);
            return;
        };
        let Some(tile) = self.open_near(target, rules) else {
            events.push(refused);
            return;
        };
        self.wake(index, events);
        let body = &mut self.bodies[index];
        body.route.clear();
        body.forget_way();
        let before = body.pos().chunk();
        body.at = tile.centre();
        if tile.chunk() != before {
            self.leave_chunk(id, before);
            self.enter_chunk(id, tile.chunk());
        }
        let body = &mut self.bodies[index];
        events.push(Event::BodyMoved {
            body: id,
            to: tile,
            at: body.at,
            remaining: 0,
        });
        events.push(Event::BodyStopped {
            body: id,
            at: body.at,
        });
    }

    /// Drinks from the running water at `at`, on the body's tile or next
    /// to it: thirst is quenched at once. A drink takes no time, as a meal
    /// takes none, until acts that last are built; and a body at the water
    /// drinks its fill. Refused, with the reason, out of reach or away
    /// from running water.
    fn drink(&mut self, id: BodyId, at: Pos, events: &mut Vec<Event>) {
        let refuse = |events: &mut Vec<Event>, reason| {
            events.push(Event::DrinkRefused {
                body: id,
                at,
                reason,
            });
        };
        let Some(index) = self.index_of(id) else {
            return refuse(events, Refusal::Unknown);
        };
        let here = self.bodies[index].pos();
        // Unsigned distances: a tile at the ends of i32 (a client's
        // coordinates, D03) is out of reach, not an overflow.
        if at.x.abs_diff(here.x) > 1 || at.y.abs_diff(here.y) > 1 {
            return refuse(events, Refusal::OutOfReach);
        }
        if !self.map.get(at).is_some_and(Terrain::is_drinkable) {
            return refuse(events, Refusal::NotWater);
        }
        self.wake(index, events);
        let body = &mut self.bodies[index];
        let before = body.condition.summary();
        body.condition.thirst = Gauge::at(0);
        events.push(Event::BodyDrank { body: id, at });
        if body.condition.summary() != before {
            events.push(Event::ConditionChanged {
                body: id,
                summary: body.condition.summary(),
            });
        }
    }

    /// Eats a meal off `item`: all of it when it weighs no more than a
    /// meal ([`content::Needs::meal_g`]), which is then gone; else a
    /// meal's grams, the rest staying where it is.
    /// Begins to gather ([`Intention::Gather`]): refused unless the tree
    /// or plant is there, within reach ([`GATHER_REACH_MM`]), bears that
    /// crop now, and the body can carry more. The body stops where it
    /// stands, awake.
    fn gather(
        &mut self,
        id: BodyId,
        chunk: ChunkPos,
        source: crops::Source,
        gives: &str,
        rules: &Content,
        events: &mut Vec<Event>,
    ) {
        let refuse = |events: &mut Vec<Event>, reason| {
            events.push(Event::GatherRefused {
                body: id,
                gives: gives.to_owned(),
                reason,
            });
        };
        let Some(index) = self.index_of(id) else {
            return refuse(events, Refusal::Unknown);
        };
        let Some(bearer) = self
            .bearers_in(chunk)
            .iter()
            .find(|b| b.source == source)
            .cloned()
        else {
            return refuse(events, Refusal::Unknown);
        };
        let at = self.bodies[index].at;
        let (dx, dy) = (i64::from(at.x - bearer.at.x), i64::from(at.y - bearer.at.y));
        if dx * dx + dy * dy > GATHER_REACH_MM * GATHER_REACH_MM {
            return refuse(events, Refusal::OutOfReach);
        }
        let crop = self.crop_of(&bearer.kind, gives);
        if crop.is_none_or(|c| self.ripe_of_crop(chunk, source, bearer.at, c).is_none()) {
            return refuse(events, Refusal::NothingRipe);
        }
        if self.bodies[index].food_room_g(&rules.movement) == 0 {
            return refuse(events, Refusal::TooHeavy);
        }
        self.stop_work(index, WorkEnd::Interrupted, events);
        self.wake(index, events);
        let since = self.tick;
        let body = &mut self.bodies[index];
        body.forget_way();
        if !body.route.is_empty() {
            body.route.clear();
            events.push(Event::BodyStopped {
                body: id,
                at: body.at,
            });
        }
        body.work = Some(Work {
            chunk,
            bearer,
            gives: Arc::from(gives),
            since,
            done_g: 0,
            into: None,
            crop,
        });
        events.push(Event::WorkStarted {
            body: id,
            gives: gives.to_owned(),
        });
    }

    /// Moves every working body's work on (D45): what its kind's rate has
    /// earned since it began, a pinch at least at a time ([`PINCH_G`]),
    /// taken off the plant into the item it carries, until the plant is
    /// bare or its load full. A body that walks or sleeps has stopped.
    fn labour(&mut self, rules: &Content, events: &mut Vec<Event>) {
        let per_hour = (self.calendar.ticks_per_day() / 24).max(1);
        for index in 0..self.bodies.len() {
            let body = &self.bodies[index];
            let Some(work) = &body.work else {
                continue;
            };
            if !body.route.is_empty() || body.condition.asleep {
                self.stop_work(index, WorkEnd::Interrupted, events);
                continue;
            }
            // The crop by number: kept with the work, or found again by
            // name after a load.
            let kept = work.crop;
            let crop = kept.or_else(|| self.crop_of(&work.bearer.kind, &work.gives));
            let rate = crop.map_or(0, |c| self.crops[c].per_hour_g)
                * body.condition.pace_percent(&rules.needs)
                / 100;
            let earned = u64::from(rate) * (self.tick.0 - work.since.0) / per_hour;
            let due =
                u32::try_from(earned.saturating_sub(u64::from(work.done_g))).unwrap_or(u32::MAX);
            if rate > 0 && due < PINCH_G {
                continue;
            }
            let (chunk, source, at) = (work.chunk, work.bearer.source, work.bearer.at);
            let gives = Arc::clone(&work.gives);
            if kept.is_none()
                && let Some(work) = self.bodies[index].work.as_mut()
            {
                work.crop = crop;
            }
            let Some((year, left)) = crop.and_then(|c| self.ripe_of_crop(chunk, source, at, c))
            else {
                self.stop_work(index, WorkEnd::Bare, events);
                continue;
            };
            let room = self.bodies[index].food_room_g(&rules.movement);
            let grams = due.min(left).min(room);
            if grams > 0 {
                self.record_pick(chunk, source, &gives, year, grams);
                events.push(Event::CropPicked {
                    chunk,
                    version: self.chunk_version(chunk),
                });
                self.add_gathered(index, &gives, grams, rules, events);
            }
            if grams == left {
                self.stop_work(index, WorkEnd::Bare, events);
            } else if grams == room {
                self.stop_work(index, WorkEnd::Full, events);
            }
        }
    }

    /// Puts `grams` just gathered in the item the work carries them in,
    /// or in a new one, made now, in hand if the hand is free.
    fn add_gathered(
        &mut self,
        index: usize,
        gives: &str,
        grams: u32,
        rules: &Content,
        events: &mut Vec<Event>,
    ) {
        let id = self.bodies[index].id;
        let into = self.bodies[index]
            .work
            .as_ref()
            .and_then(|w| w.into)
            .filter(|item| {
                self.items.get(item).is_some_and(|i| {
                    i.material == gives && matches!(i.place, Place::Carried { body } if body == id)
                })
            });
        let item = if let Some(item) = into {
            if let Some(heap) = self.items.get_mut(&item) {
                heap.mass_g += grams;
            }
            item
        } else {
            let item = ItemId(self.next_item);
            self.next_item += 1;
            self.items.insert(
                item,
                Item {
                    id: item,
                    material: gives.to_owned(),
                    mass_g: grams,
                    made: self.tick,
                    place: Place::Carried { body: id },
                },
            );
            let body = &mut self.bodies[index];
            if body.held.is_none() {
                body.held = Some(item);
                events.push(Event::BodyHolds {
                    body: id,
                    item: Some(item),
                });
            } else {
                body.carried.push(item);
            }
            item
        };
        if let Some(work) = self.bodies[index].work.as_mut() {
            work.done_g += grams;
            work.into = Some(item);
        }
        // The grams added, the load not re-weighed: every change of an
        // inventory keeps `load_g` the items' sum, `food_g` its food's.
        self.bodies[index].load_g += grams;
        self.bodies[index].food_g += grams;
        self.inventory_told(index, rules, events);
    }

    /// Ends the work of the body at `index`, if it is busy, telling what
    /// it gathered.
    fn stop_work(&mut self, index: usize, why: WorkEnd, events: &mut Vec<Event>) {
        if let Some(work) = self.bodies[index].work.take() {
            events.push(Event::WorkStopped {
                body: self.bodies[index].id,
                gives: work.gives.to_string(),
                grams: work.done_g,
                why,
            });
        }
    }

    /// Has the item `id` spoiled (D16, D45)? False when there is no such
    /// item.
    pub fn is_spoiled(&self, id: ItemId, rules: &Content) -> bool {
        self.item_or_kept(id)
            .is_some_and(|item| self.spoiled(item, rules))
    }

    /// Has `item` spoiled (D16, D45)? Food does, its content's keeping
    /// after it was made; a material that wears tears its wear after
    /// (D57); anything else never. Keeping and wear written in seasons
    /// follow this world's cycle (D09).
    fn spoiled(&self, item: &Item, rules: &Content) -> bool {
        self.spoils_at(item, rules)
            .is_some_and(|at| self.tick.0 >= at)
    }

    /// The tick from which `item` is spoiled (D16, D45), `None` for what
    /// never spoils nor wears. A record kept in a store spoils by the
    /// journée (D58, amended 26 November 2026): good until the start of
    /// the journée its heap goes (by the content's `heap_days`), all of it
    /// then.
    #[must_use]
    pub fn spoils_at(&self, item: &Item, rules: &Content) -> Option<u64> {
        if self.kept.contains_key(&item.id) {
            let day = self.heap_goes_on(item, rules)?;
            return Some(day * self.calendar.ticks_per_day());
        }
        self.spoils_loose_at(item, rules)
    }

    /// The tick from which `item` is spoiled, kept or not: its keeping
    /// (or wear) after it was made.
    fn spoils_loose_at(&self, item: &Item, rules: &Content) -> Option<u64> {
        let material = rules.material(&item.material)?;
        let per_cycle = self.calendar.days_per_cycle();
        let days = match material.food {
            Some(food) => food.days_kept(per_cycle),
            None => match material.days_worn(per_cycle) {
                0 => return None,
                worn => worn,
            },
        };
        Some(item.made.0 + u64::from(days) * self.calendar.ticks_per_day())
    }

    /// Takes away the food that has spoiled, wherever it is (D45): on the
    /// ground, carried. Done as each day starts; till then, spoiled food
    /// is refused to who would eat it.
    fn spoil(&mut self, rules: &Content, events: &mut Vec<Event>) {
        let mut gone: Vec<ItemId> = self
            .items
            .values()
            .filter(|item| self.spoiled(item, rules))
            .map(|item| item.id)
            .collect();
        // Food kept in a store spoils there as anywhere (D58): told in the
        // order of the ids, as if it lay among the items.
        let kept_from = gone.len();
        gone.extend(
            self.kept
                .values()
                .filter(|item| self.spoiled(item, rules))
                .map(|item| item.id),
        );
        if gone.len() > kept_from {
            gone.sort_unstable();
        }
        for id in gone {
            if let Some(record) = self.unkeep(id) {
                events.push(Event::Spoiled {
                    item: id,
                    material: record.material,
                    grams: record.mass_g,
                    holder: None,
                });
                continue;
            }
            let Some(item) = self.remove_item(id) else {
                continue;
            };
            match item.place {
                Place::Ground { pos } => events.push(Event::ItemRemoved {
                    item: id,
                    at: pos,
                    version: self.chunk_version(pos.chunk()),
                }),
                Place::Carried { body } => {
                    if let Some(index) = self.index_of(body) {
                        let was_held = self.bodies[index].held == Some(id);
                        self.bodies[index].let_go(id);
                        self.inventory_changed(index, rules, events);
                        if was_held {
                            events.push(Event::BodyHolds { body, item: None });
                        }
                    }
                }
            }
            let holder = match item.place {
                Place::Carried { body } => Some(body),
                Place::Ground { .. } => None,
            };
            events.push(Event::Spoiled {
                item: id,
                material: item.material,
                grams: item.mass_g,
                holder,
            });
        }
    }

    fn eat(&mut self, id: BodyId, item: ItemId, rules: &Content, events: &mut Vec<Event>) {
        let Some(index) = self.index_of(id) else {
            return Self::refuse(events, id, ItemAct::Eat, item, Refusal::Unknown);
        };
        let place = match self.reach(index, item) {
            Ok(place) => place,
            Err(reason) => return Self::refuse(events, id, ItemAct::Eat, item, reason),
        };
        self.eat_from((index, index), item, place, rules, events);
    }

    /// [`Intention::Carry`]: `id` gives `infant` to `carrier`. Nothing
    /// happens unless the infant is one, lies uncarried within reach of
    /// `id` or is carried by it, and `carrier` is `id` or stands within
    /// its reach, neither an infant nor carried itself.
    fn hand_infant(&mut self, id: BodyId, infant: BodyId, carrier: BodyId, rules: &Content) {
        let (Some(giver), Some(young), Some(taker)) =
            (self.body(id), self.body(infant), self.body(carrier))
        else {
            return;
        };
        let near = |a: Pos, b: Pos| (a.x - b.x).abs() <= 1 && (a.y - b.y).abs() <= 1;
        let free = match young.carried_by {
            None => near(giver.pos(), young.pos()),
            Some(by) => by == id,
        };
        if !free
            || !self.is_infant(young, rules)
            || self.is_infant(taker, rules)
            || taker.carried_by.is_some()
            || !near(giver.pos(), taker.pos())
        {
            return;
        }
        self.carry(carrier, infant);
    }

    /// [`Intention::Feed`]: `id` gives `young`, standing on its tile or
    /// next to it, the food `item` within its own reach; refused as an
    /// eating is, told to the giver.
    fn feed(
        &mut self,
        id: BodyId,
        young: BodyId,
        item: ItemId,
        rules: &Content,
        events: &mut Vec<Event>,
    ) {
        let (Some(index), Some(eater)) = (self.index_of(id), self.index_of(young)) else {
            return Self::refuse(events, id, ItemAct::Eat, item, Refusal::Unknown);
        };
        let (a, b) = (self.bodies[index].pos(), self.bodies[eater].pos());
        if eater == index || (a.x - b.x).abs() > 1 || (a.y - b.y).abs() > 1 {
            return Self::refuse(events, id, ItemAct::Eat, item, Refusal::OutOfReach);
        }
        let place = match self.reach(index, item) {
            Ok(place) => place,
            Err(reason) => return Self::refuse(events, id, ItemAct::Eat, item, reason),
        };
        self.eat_from((eater, index), item, place, rules, events);
    }

    /// The body `eater` eats `item`, which lies at `place` within the
    /// reach of `holder` (itself, or one who feeds it): refusals are told
    /// to the holder, the meal to the eater.
    fn eat_from(
        &mut self,
        (eater, index): (usize, usize),
        item: ItemId,
        place: Place,
        rules: &Content,
        events: &mut Vec<Event>,
    ) {
        let id = self.bodies[index].id;
        let Some((material, mass_g)) = self
            .item_or_kept(item)
            .map(|i| (i.material.clone(), i.mass_g))
        else {
            return Self::refuse(events, id, ItemAct::Eat, item, Refusal::Unknown);
        };
        let Some(food) = rules.material(&material).and_then(|m| m.food) else {
            return Self::refuse(events, id, ItemAct::Eat, item, Refusal::NotFood);
        };
        if self
            .item_or_kept(item)
            .is_some_and(|i| self.spoiled(i, rules))
        {
            return Self::refuse(events, id, ItemAct::Eat, item, Refusal::Spoiled);
        }
        // A kilojoule a kilogram is a joule a gram: what it can still eat
        // today, in grams of this food.
        let needs = &rules.needs;
        let room_g =
            self.bodies[eater].condition.room_j(needs) / u64::from(food.energy_kj_per_kg).max(1);
        let grams = mass_g
            .min(needs.meal_g)
            .min(u32::try_from(room_g).unwrap_or(u32::MAX))
            .min(self.part_g(item));
        if grams == 0 {
            return Self::refuse(events, id, ItemAct::Eat, item, Refusal::Sated);
        }
        self.wake(eater, events);
        let whole = grams == mass_g;
        let eaten_j = u64::from(grams) * u64::from(food.energy_kj_per_kg);
        let c = &mut self.bodies[eater].condition;
        c.deficit_j = c.deficit_j.saturating_sub(eaten_j);
        c.eaten_today_j += eaten_j;
        c.set_appetite(needs);
        match place {
            Place::Ground { .. } if self.kept.contains_key(&item) => {
                self.eaten_kept(item, grams, events);
            }
            Place::Ground { pos } if whole => {
                self.remove_item(item);
                events.push(Event::ItemRemoved {
                    item,
                    at: pos,
                    version: self.chunk_version(pos.chunk()),
                });
            }
            Place::Ground { pos } => {
                // Told as the item gone and laid again, lighter: the
                // chunk changes twice, its holders follow both.
                self.lift(item, pos.chunk());
                events.push(Event::ItemRemoved {
                    item,
                    at: pos,
                    version: self.chunk_version(pos.chunk()),
                });
                if let Some(left) = self.items.get_mut(&item) {
                    left.mass_g -= grams;
                }
                self.lay(item, pos.chunk());
                events.push(Event::ItemAdded {
                    item,
                    material: material.clone(),
                    mass_g: mass_g - grams,
                    at: pos,
                    version: self.chunk_version(pos.chunk()),
                });
            }
            Place::Carried { .. } if whole => {
                self.remove_item(item);
                let was_held = self.bodies[index].held == Some(item);
                self.bodies[index].let_go(item);
                self.inventory_changed(index, rules, events);
                if was_held {
                    events.push(Event::BodyHolds {
                        body: id,
                        item: None,
                    });
                }
            }
            Place::Carried { .. } => {
                if let Some(left) = self.items.get_mut(&item) {
                    left.mass_g -= grams;
                }
                self.inventory_changed(index, rules, events);
            }
        }
        events.push(Event::BodyAte {
            body: self.bodies[eater].id,
            item,
            material,
            grams,
            energy_kj: food.energy_kj(grams),
        });
    }

    fn pick_up(&mut self, id: BodyId, item: ItemId, rules: &Content, events: &mut Vec<Event>) {
        let Some(index) = self.index_of(id) else {
            return Self::refuse(events, id, ItemAct::PickUp, item, Refusal::Unknown);
        };
        let pos = match self.reach(index, item) {
            Ok(Place::Ground { pos }) => pos,
            Ok(Place::Carried { .. }) => {
                return Self::refuse(events, id, ItemAct::PickUp, item, Refusal::NotCarried);
            }
            Err(reason) => return Self::refuse(events, id, ItemAct::PickUp, item, reason),
        };
        let mass = self.item_or_kept(item).map_or(0, Item::mass_g);
        let food = self
            .item_or_kept(item)
            .and_then(|i| rules.material(&i.material))
            .is_some_and(|m| m.food.is_some());
        let body = &self.bodies[index];
        let room = if food {
            body.food_room_g(&rules.movement)
        } else {
            body.capacity_g(&rules.movement).saturating_sub(body.load_g)
        };
        // A heap of a store is taken by the handful (D58, amended 26
        // November 2026): what the arms and the load allow, the rest left.
        let take = if self.kept.contains_key(&item) {
            mass.min(room)
        } else {
            mass
        };
        if take > room || (take == 0 && mass > 0) {
            return Self::refuse(events, id, ItemAct::PickUp, item, Refusal::TooHeavy);
        }
        self.wake(index, events);
        // Taken away from a store (D58): the record is an item again, the
        // same, carried; nothing lay on the ground. Taken in part, the
        // part taken is the item, and the rest stays kept, a record of its
        // own (`Event::Kept` names it).
        let parts = self.kept_parts.get(&item).copied();
        let from_store = if let Some(mut record) = self.unkeep(item) {
            let rest_id = ItemId(self.next_item);
            if let Some(rest) = record.split(rest_id, mass - take) {
                // The rest keeps its share of the heap's count.
                let left = u64::from(parts.unwrap_or(1)) * u64::from(rest.mass_g) / u64::from(mass);
                self.kept_parts
                    .insert(rest_id, u32::try_from(left).unwrap_or(u32::MAX).max(1));
                self.next_item += 1;
                if let Place::Ground { pos: at } = rest.place {
                    events.push(Event::Kept {
                        item,
                        into: rest_id,
                        material: rest.material.clone(),
                        mass_g: rest.mass_g,
                        at,
                    });
                    self.kept_in.entry(at.chunk()).or_default().insert(rest_id);
                }
                self.kept.insert(rest_id, rest);
            }
            record.place = Place::Carried { body: id };
            self.items.insert(item, record);
            true
        } else {
            if let Some(taken) = self.items.get_mut(&item) {
                taken.place = Place::Carried { body: id };
                self.lift(item, pos.chunk());
            }
            false
        };
        let body = &mut self.bodies[index];
        let in_hand = body.held.is_none();
        if in_hand {
            body.held = Some(item);
        } else {
            body.carried.push(item);
        }
        if !from_store {
            events.push(Event::ItemRemoved {
                item,
                at: pos,
                version: self.chunk_version(pos.chunk()),
            });
        }
        self.inventory_changed(index, rules, events);
        if in_hand {
            events.push(Event::BodyHolds {
                body: id,
                item: Some(item),
            });
        }
    }

    fn drop(&mut self, id: BodyId, item: ItemId, rules: &Content, events: &mut Vec<Event>) {
        let Some(index) = self.index_of(id) else {
            return Self::refuse(events, id, ItemAct::Drop, item, Refusal::Unknown);
        };
        if !self.bodies[index].has(item) {
            let reason = self.not_had(item);
            return Self::refuse(events, id, ItemAct::Drop, item, reason);
        }
        self.wake(index, events);
        let at = self.bodies[index].pos();
        let was_held = self.bodies[index].held == Some(item);
        self.bodies[index].let_go(item);
        let Some(dropped) = self.items.get_mut(&item) else {
            return;
        };
        dropped.place = Place::Ground { pos: at };
        let material = dropped.material.clone();
        let mass_g = dropped.mass_g;
        self.lay(item, at.chunk());
        events.push(Event::ItemAdded {
            item,
            material,
            mass_g,
            at,
            version: self.chunk_version(at.chunk()),
        });
        self.inventory_changed(index, rules, events);
        if was_held {
            events.push(Event::BodyHolds {
                body: id,
                item: None,
            });
        }
    }

    /// What is left of the record `item` kept in a store once `grams` of
    /// it are eaten (D58): lighter, or gone; nothing lay on the ground, no
    /// chunk changes.
    fn eaten_kept(&mut self, item: ItemId, grams: u32, events: &mut Vec<Event>) {
        if grams >= self.part_g(item)
            && let Some(parts) = self.kept_parts.get_mut(&item)
        {
            *parts = parts.saturating_sub(1).max(1);
        }
        let Some(left) = self.kept.get_mut(&item) else {
            return;
        };
        if left.mass_g <= grams {
            self.unkeep(item);
            return;
        }
        left.mass_g -= grams;
        let (material, mass_g, place) = (left.material.clone(), left.mass_g, left.place);
        self.kept_changes += 1;
        if let Place::Ground { pos } = place {
            events.push(Event::Kept {
                item,
                into: item,
                material,
                mass_g,
                at: pos,
            });
        }
    }

    /// What one of the items a record kept in a store was laid as weighs,
    /// on average, rounded up (D58, amended 26 November 2026): a bite
    /// from it is that at most. Anything else: no bound.
    fn part_g(&self, item: ItemId) -> u32 {
        match (self.kept.get(&item), self.kept_parts.get(&item)) {
            (Some(record), Some(&parts)) => record.mass_g.div_ceil(parts.max(1)),
            _ => u32::MAX,
        }
    }

    /// The journée at whose start a heap kept in a store holding `item`
    /// goes spoiled, by its number (D58, amended 26 November 2026): the
    /// journée whose start would take `item` away loose (D45: spoiled food
    /// goes as each journée starts), brought down to the first of its
    /// band of the content's `heap_days` journées (1: that very journée).
    /// `None` for what never spoils nor wears.
    fn heap_goes_on(&self, item: &Item, rules: &Content) -> Option<u64> {
        let per_day = self.calendar.ticks_per_day().max(1);
        let day = self.spoils_loose_at(item, rules)?.div_ceil(per_day);
        let band = u64::from(rules.needs.pooling.heap_days.max(1));
        Some(day / band * band)
    }

    /// The record kept at `at` that `record` would join (D58, amended 26
    /// November 2026): of the same food, its heap going spoiled at the
    /// start of the same journée.
    fn heap_for(&self, record: &Item, at: Pos, rules: &Content) -> Option<ItemId> {
        let day = self.heap_goes_on(record, rules)?;
        self.kept_in(at.chunk())
            .filter(|heap| heap.pos() == Some(at) && heap.material == record.material)
            .find(|heap| self.heap_goes_on(heap, rules) == Some(day))
            .map(Item::id)
    }

    /// Lays a carried item in a store a brain keeps ([`Intention::Keep`],
    /// D58): refused as a drop is; the item leaves the world's items for a
    /// record kept at the body's tile, off the ground. **Heaps** (the
    /// project lead, 26 November 2026: « 125 noisettes, ce n'est pas 125
    /// fois une noisette mais une fois 125 noisettes »): a record of the
    /// same food whose heap goes spoiled at the start of the same journée
    /// ([`World::heap_goes_on`]) kept at that tile takes it in, dated as
    /// the older of the two ([`Item::merge`]); the item laid is no more.
    fn keep(&mut self, id: BodyId, item: ItemId, rules: &Content, events: &mut Vec<Event>) {
        let Some(index) = self.index_of(id) else {
            return Self::refuse(events, id, ItemAct::Drop, item, Refusal::Unknown);
        };
        if !self.bodies[index].has(item) {
            let reason = self.not_had(item);
            return Self::refuse(events, id, ItemAct::Drop, item, reason);
        }
        self.wake(index, events);
        let at = self.bodies[index].pos();
        let was_held = self.bodies[index].held == Some(item);
        self.bodies[index].let_go(item);
        let Some(mut record) = self.items.remove(&item) else {
            return;
        };
        record.place = Place::Ground { pos: at };
        let (material, mass_g) = (record.material.clone(), record.mass_g);
        let mut into = item;
        let heap = self
            .heap_for(&record, at, rules)
            .and_then(|heap| Some((heap, self.kept.get_mut(&heap)?)));
        let left = match heap {
            Some((heap, held)) => match held.merge(record) {
                Ok(()) => {
                    into = heap;
                    None
                }
                Err(record) => Some(record),
            },
            None => Some(record),
        };
        if let Some(record) = left {
            self.kept.insert(item, record);
            self.kept_in.entry(at.chunk()).or_default().insert(item);
            self.kept_parts.insert(item, 1);
        } else if let Some(parts) = self.kept_parts.get_mut(&into) {
            *parts = parts.saturating_add(1);
        }
        self.kept_changes += 1;
        events.push(Event::Kept {
            item,
            into,
            material,
            mass_g,
            at,
        });
        self.inventory_changed(index, rules, events);
        if was_held {
            events.push(Event::BodyHolds {
                body: id,
                item: None,
            });
        }
    }

    fn hold(&mut self, id: BodyId, item: ItemId, rules: &Content, events: &mut Vec<Event>) {
        let Some(index) = self.index_of(id) else {
            return Self::refuse(events, id, ItemAct::Hold, item, Refusal::Unknown);
        };
        if self.bodies[index].held == Some(item) {
            return;
        }
        if !self.bodies[index].carried.contains(&item) {
            let reason = self.not_had(item);
            return Self::refuse(events, id, ItemAct::Hold, item, reason);
        }
        let body = &mut self.bodies[index];
        body.carried.retain(|i| *i != item);
        if let Some(previous) = body.held.replace(item) {
            body.carried.push(previous);
        }
        self.inventory_changed(index, rules, events);
        events.push(Event::BodyHolds {
            body: id,
            item: Some(item),
        });
    }

    fn stow(&mut self, id: BodyId, item: ItemId, rules: &Content, events: &mut Vec<Event>) {
        let Some(index) = self.index_of(id) else {
            return Self::refuse(events, id, ItemAct::Stow, item, Refusal::Unknown);
        };
        if self.bodies[index].held != Some(item) {
            let reason = self.not_had(item);
            return Self::refuse(events, id, ItemAct::Stow, item, reason);
        }
        let body = &mut self.bodies[index];
        body.held = None;
        body.carried.push(item);
        self.inventory_changed(index, rules, events);
        events.push(Event::BodyHolds {
            body: id,
            item: None,
        });
    }

    /// `id` sets off behind `leader` (D56): when the leader walks and
    /// stands within [`FOLLOW_TILES`] of it on each axis, it takes the
    /// leader's way whole, from where the leader stands, and its goal if
    /// it walks past its sight; no search of its own. False when the
    /// leader is not walking within reach: the caller walks to its target
    /// as a [`Intention::MoveTo`].
    fn follow(
        &mut self,
        id: BodyId,
        leader: BodyId,
        rules: &Content,
        events: &mut Vec<Event>,
    ) -> bool {
        let (Some(index), Some(lead)) = (self.index_of(id), self.index_of(leader)) else {
            return false;
        };
        if index == lead {
            return false;
        }
        let (from, there) = (self.bodies[index].pos(), self.bodies[lead].pos());
        let near =
            (from.x - there.x).abs() <= FOLLOW_TILES && (from.y - there.y).abs() <= FOLLOW_TILES;
        if !near || self.bodies[lead].route.is_empty() || self.bodies[lead].condition.asleep {
            return false;
        }
        let mut route: Vec<FinePos> = Vec::with_capacity(self.bodies[lead].route.len() + 1);
        route.push(self.bodies[lead].at);
        route.extend(self.bodies[lead].route.iter().copied());
        let goal = self.bodies[lead].goal;
        self.wake(index, events);
        let body = &mut self.bodies[index];
        let at = body.at;
        body.skirted = Skirted::Not;
        body.went.clear();
        body.goal = goal;
        body.route = route.iter().copied().collect();
        events.push(Event::BodyWalks {
            body: id,
            from: at,
            route,
            speed_cm_per_s: rules.movement.walk_speed_cm_per_s,
        });
        true
    }

    /// Sends a body toward `target` (`ordered`: a new order, rather than a
    /// body looking again), saying how many tiles its searches expanded.
    /// A way is looked for within what the body sees from where it stands
    /// ([`World::sight_tiles_at`]) and within the content's budget of
    /// effort (D32), round the trunks of trees. With none, it walks
    /// straight toward the target, stepping round a trunk or a bush in its
    /// way and stopping before the first bigger obstacle
    /// ([`Map::walk_straight`], D31), and keeps the target to look again
    /// from there. Before an obstacle it sees no way past, it walks along
    /// it once ([`Map::skirt`]) and looks once more from there: a way seen
    /// is taken, otherwise it stops for good. It thinks aloud at each
    /// decision ([`Event::BodyThinks`]). A new order that cannot even set
    /// off is refused, saying what the body sees barring its way, a body
    /// looking again that cannot stops for good.
    // One arm for each way of setting off, each telling what it does.
    #[allow(
        clippy::too_many_lines,
        reason = "one straight reading, top to bottom; splitting it would scatter the rule"
    )]
    fn set_off(
        &mut self,
        index: usize,
        target: Pos,
        ordered: bool,
        rules: &Content,
        events: &mut Vec<Event>,
    ) -> u32 {
        let id = self.bodies[index].id;
        let from = self.bodies[index].at;
        self.bodies[index].goal = None;
        if ordered {
            self.bodies[index].skirted = Skirted::Not;
            self.bodies[index].went.clear();
        }
        let (plan, work) = self.plan_walk(index, target, rules);
        let walk_speed = rules.movement.walk_speed_cm_per_s;
        let walks = |route: Vec<FinePos>| Event::BodyWalks {
            body: id,
            from,
            route,
            speed_cm_per_s: walk_speed,
        };
        let thinks = |thought: Thought, at: FinePos| Event::BodyThinks {
            body: id,
            thought,
            at,
        };
        match plan {
            Plan::Path(route) => {
                let body = &mut self.bodies[index];
                let after_skirting = body.skirted == Skirted::Along;
                body.skirted = Skirted::Not;
                body.went.clear();
                body.route = route.iter().copied().collect();
                if let Some(&end) = route.last() {
                    events.push(walks(route));
                    if after_skirting {
                        events.push(thinks(Thought::WaySeen, end));
                    }
                } else {
                    events.push(Event::BodyStopped { body: id, at: from });
                }
            }
            Plan::Past(route) => {
                let end = *route.last().expect("a way past is a walk");
                let body = &mut self.bodies[index];
                let after_skirting = body.skirted == Skirted::Along;
                if after_skirting {
                    body.skirted = Skirted::Beyond;
                }
                body.route = route.iter().copied().collect();
                body.goal = Some(Goal {
                    target,
                    looked_from: squared_mm(from, target.centre()),
                });
                events.push(walks(route));
                if after_skirting {
                    events.push(thinks(Thought::WaySeen, end));
                }
            }
            Plan::Skirt(route, side) => {
                let end = *route.last().expect("a skirt is a walk");
                let body = &mut self.bodies[index];
                body.skirted = Skirted::Along;
                // Where this leg began, for the first, and where it ends.
                if body.went.is_empty() {
                    body.went.push(from.tile());
                }
                body.went.push(end.tile());
                body.route = route.iter().copied().collect();
                // It looks again on arriving, wherever that is.
                body.goal = Some(Goal {
                    target,
                    looked_from: u64::MAX,
                });
                events.push(walks(route));
                events.push(thinks(Thought::Skirting { side }, end));
            }
            Plan::GiveUp => {
                self.bodies[index].skirted = Skirted::Not;
                self.bodies[index].went.clear();
                self.blocked(index, target, rules, events);
            }
            Plan::Straight(route, seen) => {
                let quarter = u64::from(MM_PER_TILE.unsigned_abs() / 4);
                if route
                    .iter()
                    .all(|p| squared_mm(*p, from) < quarter * quarter)
                {
                    self.bodies[index].skirted = Skirted::Not;
                    self.bodies[index].went.clear();
                    if ordered {
                        events.push(Event::MoveRefused {
                            body: id,
                            target,
                            reason: self.blockage(from, target.centre(), rules),
                        });
                    } else {
                        self.blocked(index, target, rules, events);
                    }
                    return work;
                }
                let end = *route.last().expect("a walk");
                let body = &mut self.bodies[index];
                if seen {
                    body.skirted = Skirted::Beyond;
                }
                body.route = route.iter().copied().collect();
                body.goal = Some(Goal {
                    target,
                    looked_from: squared_mm(from, target.centre()),
                });
                events.push(walks(route));
                if seen {
                    events.push(thinks(Thought::WaySeen, end));
                }
            }
        }
        work
    }

    /// How a body standing where it is sets off toward `target`
    /// ([`World::set_off`]), and the tiles its searches expanded: a way
    /// in sight to the target; else straight on while nothing in sight
    /// stops it; else a way in sight past the obstacle that stops the
    /// straight walk, back to its line beyond it; else, having skirted
    /// once, nowhere; else straight to the obstacle when it stands short
    /// of it, and along it when it stands before it ([`Map::skirt`]).
    fn plan_walk(&self, index: usize, target: Pos, rules: &Content) -> (Plan, u32) {
        let movement = &rules.movement;
        let cost = |t: Terrain| movement.cost(t);
        let body = &self.bodies[index];
        let from = body.at;
        let tile = from.tile();
        let sight = i32::try_from(self.sight_tiles_at(tile, rules)).unwrap_or(i32::MAX);
        let trunks = self.trunks();
        let blocked = |p: Pos| trunks.blocks(p);
        let budget = movement.path_budget_tiles;
        // Before an A*, the pre-test by the chunks' pieces (D32): a
        // target no walkable ground joins within the square is refused
        // at once, with no tile expanded.
        let mask = ground::walkable_mask(cost);
        let (path, mut work) =
            self.map
                .search_gated(tile, target, sight, budget, cost, blocked, || {
                    trunks.may_join(tile, target, sight, mask)
                });
        if let Some(path) = path {
            return (
                Plan::Path(self.map.pull_around(from, &path, cost, blocked)),
                work,
            );
        }
        let straight = self
            .map
            .walk_straight(from, target.centre(), sight, cost, blocked);
        let seen = |p: Pos| {
            p.x.abs_diff(tile.x) <= sight.unsigned_abs()
                && p.y.abs_diff(tile.y) <= sight.unsigned_abs()
        };
        let end = straight.last().copied().unwrap_or(from);
        // Straight on while nothing in sight stops it; and a target seen on
        // ground it may not enter is only walked toward.
        if end == target.centre() || !seen(end.tile()) {
            let seen_way = body.skirted == Skirted::Along;
            return (Plan::Straight(straight, seen_way), work);
        }
        if seen(target) && self.map.get(target).and_then(cost).is_none() {
            return (Plan::Straight(straight, false), work);
        }
        let open = |p: Pos| !blocked(p) && self.map.get(p).and_then(cost).is_some();
        if !seen(target)
            && let Some(past) = self.map.past_obstacle(end, target.centre(), sight, open)
        {
            let (path, spent) =
                self.map
                    .search_gated(tile, past, sight, budget, cost, blocked, || {
                        trunks.may_join(tile, past, sight, mask)
                    });
            work += spent;
            if let Some(path) = path.filter(|p| !p.is_empty()) {
                return (
                    Plan::Past(self.map.pull_around(from, &path, cost, blocked)),
                    work,
                );
            }
        }
        // Along the obstacle: as many legs for one order as the content
        // allows, after each a look from where it stands, none ending on
        // a tile a leg of this order began or ended on (it never circles);
        // the legs spent, it stops and says so (D54).
        let legs = body.went.len().saturating_sub(1);
        if legs >= usize::try_from(movement.skirts_per_order).unwrap_or(usize::MAX) {
            return (Plan::GiveUp, work);
        }
        let near = u64::from(MM_PER_TILE.unsigned_abs()) * 3 / 2;
        if squared_mm(end, from) > near * near {
            return (Plan::Straight(straight, false), work);
        }
        let plan = match self.map.skirt(from, target, sight, cost, blocked) {
            Some((_, path)) if path.last().is_some_and(|end| body.went.contains(end)) => {
                Plan::GiveUp
            }
            Some((side, path)) => {
                Plan::Skirt(self.map.pull_around(from, &path, cost, blocked), side)
            }
            None => Plan::Straight(straight, false),
        };
        (plan, work)
    }

    /// A body that walked straight for want of a way and stopped looks
    /// again from where it stands, toward the same target, but only from
    /// nearer to it than where it last looked, or after skirting an
    /// obstacle: it never circles before a wall. Otherwise it stops for
    /// good and says it is blocked; on its target, it simply stays. The
    /// tiles its search expanded.
    fn look_again(
        &mut self,
        index: usize,
        goal: Goal,
        rules: &Content,
        events: &mut Vec<Event>,
    ) -> u32 {
        // Arrived: its stop was told on arrival, once, and nothing bars it.
        if self.bodies[index].at == goal.target.centre() {
            self.bodies[index].forget_way();
            return 0;
        }
        if squared_mm(self.bodies[index].at, goal.target.centre()) >= goal.looked_from {
            self.bodies[index].forget_way();
            self.blocked(index, goal.target, rules, events);
            return 0;
        }
        self.set_off(index, goal.target, false, rules, events)
    }

    /// Tells that a body stopped for good short of `target`, and what it
    /// sees barring its way.
    fn blocked(&self, index: usize, target: Pos, rules: &Content, events: &mut Vec<Event>) {
        let body = &self.bodies[index];
        events.push(Event::BodyBlocked {
            body: body.id,
            at: body.at,
            target,
            reason: self.blockage(body.at, target.centre(), rules),
        });
        events.push(Event::BodyThinks {
            body: body.id,
            thought: Thought::NoWay,
            at: body.at,
        });
    }

    /// What bars the way from `at` toward `toward`, looked for along the
    /// line between them up to [`BLOCKAGE_REACH_MM`] ahead (a body stops
    /// short of the first tile it may not enter, [`map::line_reach`]): the
    /// first tile it may not enter, a little to each side too so a line
    /// slipping between two at a corner sees them. Water or the cliff by
    /// name; anything else, nothing ahead or the map's edge: no way seen.
    fn blockage(&self, at: FinePos, toward: FinePos, rules: &Content) -> Blockage {
        /// Millimetres between two points looked at along the line.
        const STEP: i64 = 25;
        /// How far to each side of the line a point is looked at too.
        const SIDE: i32 = 30;
        let (dx, dy) = (i64::from(toward.x - at.x), i64::from(toward.y - at.y));
        let length = (dx * dx + dy * dy).unsigned_abs().isqrt();
        let length = i64::try_from(length).unwrap_or(i64::MAX);
        if length == 0 {
            return Blockage::NoWay;
        }
        let own = at.tile();
        let reach = length.min(BLOCKAGE_REACH_MM);
        for along in (0..=reach / STEP).map(|i| i * STEP) {
            let point = |ox: i32, oy: i32| {
                let x = i64::from(at.x) + dx * along / length;
                let y = i64::from(at.y) + dy * along / length;
                FinePos::new(
                    i32::try_from(x).unwrap_or(at.x) + ox,
                    i32::try_from(y).unwrap_or(at.y) + oy,
                )
                .tile()
            };
            for p in [
                point(0, 0),
                point(SIDE, 0),
                point(-SIDE, 0),
                point(0, SIDE),
                point(0, -SIDE),
            ] {
                if p == own {
                    continue;
                }
                match self.map.get(p) {
                    None => return Blockage::NoWay,
                    Some(t) if rules.movement.cost(t).is_none() => {
                        return match t {
                            Terrain::Water => Blockage::Water,
                            Terrain::Rock => Blockage::Rock,
                            _ => Blockage::NoWay,
                        };
                    }
                    Some(_) => {}
                }
            }
        }
        Blockage::NoWay
    }

    /// How far a body standing on `pos` sees, in tiles on each axis
    /// (D32): the content's sight from the most open walkable ground it
    /// stands on or touches. In the heart of a wood, the forest's; at its
    /// edge, it sees out, as from the open ground beside it. Public for
    /// the vivarium's measure of the meetings between tribes.
    #[must_use]
    pub fn sight_tiles_at(&self, pos: Pos, rules: &Content) -> u32 {
        let movement = &rules.movement;
        let own = movement.sight_tiles(self.map.get(pos).unwrap_or(Terrain::Rock));
        (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| Pos::new(pos.x + dx, pos.y + dy)))
            .filter_map(|p| self.map.get(p))
            .filter(|t| movement.cost(*t).is_some())
            .map(|t| movement.sight_tiles(t))
            .fold(own, u32::max)
    }

    /// Wakes a body up, if asleep; any deliberate act does.
    fn wake(&mut self, index: usize, events: &mut Vec<Event>) {
        let body = &mut self.bodies[index];
        if body.condition.asleep {
            body.condition.asleep = false;
            events.push(Event::ConditionChanged {
                body: body.id,
                summary: body.condition.summary(),
            });
        }
    }
}

/// The square of the distance between two fine points, in square
/// millimetres.
fn squared_mm(a: FinePos, b: FinePos) -> u64 {
    // Unsigned and saturating: a target at the ends of the fine grid (a
    // client's coordinates, D03) is far, not an overflow.
    let (dx, dy) = (u64::from(a.x.abs_diff(b.x)), u64::from(a.y.abs_diff(b.y)));
    (dx * dx).saturating_add(dy * dy)
}

/// Walking speed under a load, in percent of the unloaded speed: linear
/// from 100 % empty to the content's percent at full capacity.
fn loaded_speed(movement: &Movement, load_g: u32) -> u32 {
    let full = u64::from(movement.carry_capacity_g);
    let load = u64::from(load_g).min(full);
    let slowest = u64::from(movement.speed_percent_at_full_load);
    u32::try_from(100 - (100 - slowest) * load / full).expect("percent")
}

// Not generation: a body's walk is worked out in floats.
#[allow(
    clippy::cast_possible_truncation,
    clippy::float_arithmetic,
    reason = "values bounded by the world size and the content ranges; float maths of the simulation, the same result on every run"
)]
/// Moves a walking body for one tick ([`stride`]), telling the tile it
/// steps onto and its stop.
fn walk(
    body: &mut Body,
    map: &Map,
    movement: &Movement,
    speed_percent: u32,
    ticks_per_second: f64,
    events: &mut Vec<Event>,
) {
    let tile_before = body.at.tile();
    stride(
        &mut body.at,
        &mut body.route,
        map,
        movement,
        speed_percent,
        ticks_per_second,
        |_| {},
    );
    let tile = body.at.tile();
    if tile != tile_before {
        events.push(Event::BodyMoved {
            body: body.id,
            to: tile,
            at: body.at,
            remaining: body.route.len(),
        });
    }
    if body.route.is_empty() {
        events.push(Event::BodyStopped {
            body: body.id,
            at: body.at,
        });
    }
}

/// The speed a body standing at `at` walks at this tick, in millimetres a
/// tick: the terrain's, under `speed_percent` (its load).
#[allow(
    clippy::float_arithmetic,
    reason = "float maths of the simulation, the same result on every run"
)]
fn pace(
    at: FinePos,
    map: &Map,
    movement: &Movement,
    speed_percent: u32,
    ticks_per_second: f64,
) -> f64 {
    let terrain = map.get(at.tile()).unwrap_or(Terrain::Rock);
    // A straight segment may graze the corner of a tile it may not enter
    // (`line_clear`), and a body placed to the millimetre can stand a few
    // millimetres into it: it goes on at the slowest pace, never stuck.
    let speed = match movement.speed_cm_per_s(terrain) {
        0 => movement.slowest_speed_cm_per_s(),
        speed => speed,
    };
    // Millimetres this tick: 143 cm/s at 10 ticks/s is exactly 143 mm.
    f64::from(speed * 10 * speed_percent) / 100.0 / ticks_per_second
}

/// One tick of a walk: as far along the route as the speed of the
/// terrain the body stands on allows, waypoint after waypoint, each one
/// reached handed to `reached`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::float_arithmetic,
    reason = "values bounded by the world size and the content ranges; float maths of the simulation, the same result on every run"
)]
fn stride(
    at: &mut FinePos,
    route: &mut VecDeque<FinePos>,
    map: &Map,
    movement: &Movement,
    speed_percent: u32,
    ticks_per_second: f64,
    mut reached: impl FnMut(FinePos),
) {
    let mut budget = pace(*at, map, movement, speed_percent, ticks_per_second);
    while budget > 0.0 {
        let Some(&next) = route.front() else {
            break;
        };
        let distance = at.distance_to(next);
        if distance <= budget {
            *at = next;
            route.pop_front();
            reached(next);
            budget -= distance;
        } else {
            let ratio = budget / distance;
            *at = FinePos::new(
                at.x + (f64::from(next.x - at.x) * ratio).round() as i32,
                at.y + (f64::from(next.y - at.y) * ratio).round() as i32,
            );
            budget = 0.0;
        }
    }
}

/// The rest of a body's walk as it will go if nothing changes, worked out
/// with the very steps of the simulation (`stride`): the points where
/// it turns or its pace changes (the terrain under it, D32), each with the
/// tick it stands there, `now` being the tick it stands at `at`. Between
/// two of them it walks straight at one pace, so a client that moves it
/// by time between them shows it where the world has it (A6). The last
/// point is the end of the route.
#[allow(
    clippy::float_arithmetic,
    reason = "float maths of the simulation, the same result on every run"
)]
pub fn timeline(
    at: FinePos,
    route: impl IntoIterator<Item = FinePos>,
    map: &Map,
    movement: &Movement,
    speed_percent: u32,
    ticks_per_second: f64,
    now: u64,
) -> Vec<(FinePos, u64)> {
    let mut route: VecDeque<FinePos> = route.into_iter().collect();
    let (mut at, mut tick) = (at, now);
    let mut points = Vec::new();
    let mut last_pace = pace(at, map, movement, speed_percent, ticks_per_second);
    while !route.is_empty() {
        let this_pace = pace(at, map, movement, speed_percent, ticks_per_second);
        if (this_pace - last_pace).abs() > f64::EPSILON
            && points.last().is_none_or(|&(p, _)| p != at)
        {
            points.push((at, tick));
        }
        last_pace = this_pace;
        // A waypoint reached during the tick is told at the tick's end,
        // within a tick's walk (a few centimetres) of the truth.
        let mut turns = Vec::new();
        stride(
            &mut at,
            &mut route,
            map,
            movement,
            speed_percent,
            ticks_per_second,
            |p| turns.push(p),
        );
        tick += 1;
        points.extend(turns.into_iter().map(|p| (p, tick)));
    }
    points
}

impl Default for World {
    /// The standard valley with the sample content.
    fn default() -> Self {
        Self::new(Calendar::default(), &Content::sample())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Content {
        Content::sample()
    }

    /// The sample rules as before D54: one skirt for an order, then the body
    /// stops.
    fn rules_before_d54() -> Content {
        let mut rules = Content::sample();
        rules.movement.skirts_per_order = 1;
        rules
    }

    fn small_world() -> World {
        // 4 ticks per day: easy to reason about. A valley of 768: the same
        // rules as the game's, made in a fraction of the time. Body 1
        // born at the camp, as for a player.
        let mut world = World::with_valley(Calendar::new(2, 2).unwrap(), &rules(), 768);
        world.spawn_at_camp().expect("grass by the camp");
        world
    }

    /// The pre-test by the chunks' pieces (D32) against the plain A*, over
    /// random pairs within a sight: the same path or none in every case,
    /// the same work on a path found, and a refusal by the pre-test only
    /// where the A* found nothing either. The pairs found, the pairs the
    /// pre-test refused.
    fn pieces_against_the_search(
        world: &World,
        seed: u64,
        count: usize,
        reach: i32,
        budget: u32,
    ) -> (usize, usize) {
        let rules = rules();
        let movement = &rules.movement;
        let cost = |t: Terrain| movement.cost(t);
        let trunks = world.trunks();
        let blocked = |p: Pos| trunks.blocks(p);
        let mask = ground::walkable_mask(cost);
        let map = world.map();
        let mut rng = crate::r#gen::Lcg::new(seed);
        let (mut found, mut refused, mut done) = (0, 0, 0);
        let side = |n: u32| i32::try_from(n).expect("small map");
        let span = u32::try_from(2 * reach + 1).expect("small reach");
        while done < count {
            let a = Pos::new(
                side(rng.next_below(map.width())),
                side(rng.next_below(map.height())),
            );
            let b = Pos::new(
                a.x + side(rng.next_below(span)) - reach,
                a.y + side(rng.next_below(span)) - reach,
            );
            if map.get(b).is_none() {
                continue;
            }
            done += 1;
            let plain = map.search(a, b, reach, budget, cost, blocked);
            let mut gate_said = None;
            let gated = map.search_gated(a, b, reach, budget, cost, blocked, || {
                let may = trunks.may_join(a, b, reach, mask);
                gate_said = Some(may);
                may
            });
            assert_eq!(gated.0, plain.0, "{a:?} -> {b:?}: the same path or none");
            match gate_said {
                Some(false) => {
                    assert_eq!(plain.0, None, "{a:?} -> {b:?}: refused, yet a way");
                    assert_eq!(gated.1, budget.saturating_add(1));
                    refused += 1;
                }
                _ => assert_eq!(gated.1, plain.1, "{a:?} -> {b:?}: the same work"),
            }
            found += usize::from(plain.0.is_some());
        }
        (found, refused)
    }

    #[test]
    fn the_pieces_pre_test_refuses_only_what_no_path_joins_on_the_valley() {
        // The game's valley, trunks and all, with no budget: a refusal by
        // the pieces is a pair truly apart within the square.
        let world = World::with_valley(Calendar::new(2, 2).unwrap(), &rules(), 768);
        let (found, refused) = pieces_against_the_search(&world, 11, 600, 60, u32::MAX);
        assert!(
            found > 200 && refused > 10,
            "both cases seen: {found}, {refused}"
        );
        // And under the game's budget: the refusals the search pays for.
        let (found, refused) = pieces_against_the_search(&world, 12, 600, 60, 1_500);
        assert!(
            found > 200 && refused > 10,
            "both cases seen: {found}, {refused}"
        );
    }

    #[test]
    fn the_pieces_pre_test_refuses_only_what_no_path_joins_on_the_vivarium() {
        let map = Map::on_window_by(
            crate::plan::zone_window().clone(),
            crate::plan::VIVARIUM_SEED,
            map::VIVARIUM_GENERATOR,
        );
        let world = World::on_valley(Calendar::new(2, 2).unwrap(), &rules(), map);
        // The zone is one land for the most part: few pairs are apart.
        let (found, refused) = pieces_against_the_search(&world, 13, 600, 60, 1_500);
        assert!(
            found > 200 && refused > 0,
            "both cases seen: {found}, {refused}"
        );
        let (found, _) = pieces_against_the_search(&world, 14, 300, 25, u32::MAX);
        assert!(found > 50, "ways found in the forest: {found}");
    }

    fn open_field() -> World {
        let mut world = World::empty(
            Calendar::new(10, 100).unwrap(),
            Map::filled(8, 8, Terrain::Grass),
        );
        world.spawn_body(Pos::new(0, 0));
        world
    }

    fn go(world: &mut World, target: Pos) {
        world.queue(Intention::MoveTo {
            body: BodyId(1),
            target,
        });
    }

    /// Runs until body 1 stops, returning the ticks it took.
    fn until_stopped(world: &mut World, events: &mut Vec<Event>) -> u64 {
        let rules = rules();
        for n in 1..10_000 {
            world.step(&rules, events);
            if events.iter().any(|e| {
                matches!(
                    e,
                    Event::BodyStopped {
                        body: BodyId(1),
                        ..
                    }
                )
            }) {
                return n;
            }
        }
        panic!("body 1 never stopped");
    }

    #[test]
    fn new_world_starts_at_tick_zero_on_day_one_with_nobody() {
        let w = World::default();
        assert_eq!(w.tick(), Tick(0));
        assert_eq!(w.time().day, 1);
        assert!(w.bodies().is_empty(), "no idle body: a player's is born");
        assert_eq!(
            (w.map().width(), w.map().height()),
            (VALLEY_SIZE, VALLEY_SIZE)
        );
        let text = serde_json::to_string(&w).unwrap();
        assert!(
            !text.contains("\"tiles\""),
            "terrain is not in the snapshot"
        );
        assert!(text.contains("\"items\""), "items are");
    }

    #[test]
    fn a_body_is_born_on_free_grass_nearest_the_camp() {
        let mut w = World::with_valley(Calendar::new(2, 2).unwrap(), &rules(), 768);
        let camp = w.map().camp();
        let first = w.spawn_at_camp().expect("grass by the camp");
        let second = w.spawn_at_camp().expect("grass by the camp");
        assert_eq!((first, second), (BodyId(1), BodyId(2)));
        let (a, b) = (w.body(first).unwrap(), w.body(second).unwrap());
        assert_ne!(a.pos(), b.pos(), "never on a tile a body stands on");
        for body in [a, b] {
            assert_eq!(w.map().get(body.pos()), Some(Terrain::Grass));
            assert!(!w.is_trunk(body.pos()));
            assert_eq!(body.at(), body.pos().centre());
        }
        let (dx, dy) = (a.pos().x - camp.x, a.pos().y - camp.y);
        assert!(dx.abs().max(dy.abs()) <= 3, "by the camp: {dx}, {dy}");
        let near = |p: Pos| (p.x - camp.x).abs().max((p.y - camp.y).abs());
        assert!(near(a.pos()) <= near(b.pos()), "the nearest first");
    }

    #[test]
    fn a_day_started_event_fires_on_each_day_boundary() {
        let mut w = small_world();
        let mut events = Vec::new();
        w.advance(12, &rules(), &mut events); // three full days
        events.retain(|e| matches!(e, Event::DayStarted { .. }));
        assert_eq!(
            events,
            vec![
                Event::DayStarted { day: 2 },
                Event::DayStarted { day: 3 },
                Event::DayStarted { day: 4 },
            ]
        );
        assert_eq!(w.time().day, 4);
        assert!(w.time().is_day_start());
    }

    #[test]
    fn no_event_before_the_first_boundary() {
        let mut w = small_world();
        let mut events = Vec::new();
        w.advance(3, &rules(), &mut events);
        assert!(!events.iter().any(|e| matches!(e, Event::DayStarted { .. })));
        assert_eq!(w.time().day, 1);
    }

    /// The second audit of 20 November 2026 (report 6): on a real valley,
    /// whose waters and woods make the walk skirt obstacles, a `move_to`
    /// at the ends of `i32` went through `FinePos::distance_to` and
    /// overflowed, killing the lead's debug server with one message. The
    /// open field above never skirts anything, so it never saw it.
    #[test]
    fn a_walk_toward_the_ends_of_i32_on_a_valley_never_overflows() {
        let rules = rules();
        let mut w = World::with_valley(Calendar::new(10, 100).unwrap(), &rules, 256);
        let start = (0..256 * 256)
            .map(|i| Pos::new(i % 256, i / 256))
            .find(|&p| {
                w.map()
                    .get(p)
                    .is_some_and(|t| rules.movement.cost(t).is_some())
            })
            .expect("a walkable tile");
        let body = w.spawn_body(start);
        for target in [
            Pos::new(i32::MIN, 0),
            Pos::new(i32::MIN, i32::MIN),
            Pos::new(i32::MAX, i32::MAX),
            Pos::new(0, i32::MAX),
            Pos::new(-2_145_000, 7),
        ] {
            let mut events = Vec::new();
            w.queue(Intention::MoveTo { body, target });
            // Sixty ticks: the overflow came on the first one; the body
            // must stay alive on this cold valley long enough.
            w.advance(60, &rules, &mut events);
            let at = w.body(body).expect("the body").pos();
            assert!(w.map().get(at).is_some(), "move_to {target:?}: at {at:?}");
        }
    }

    /// A client's coordinates reach the simulation unchecked (D03): a
    /// target at the ends of `i32` or just past the map's edge never
    /// overflows (the debug build keeps overflow checks on). A walk toward
    /// it stops at the map's edge, as toward any target off the map; a
    /// teleport stands the body on the map or is refused; a drink is
    /// refused.
    #[test]
    fn targets_off_the_map_are_refused_or_stopped_not_overflowed() {
        let rules = rules();
        let edge = [
            Pos::new(i32::MIN, i32::MIN),
            Pos::new(i32::MAX, i32::MAX),
            Pos::new(i32::MIN, 0),
            Pos::new(0, i32::MAX),
            Pos::new(3_000_000, 0),
            Pos::new(8, 0),
            Pos::new(0, 8),
            Pos::new(-1, 0),
            Pos::new(0, -1),
        ];
        for target in edge {
            let mut w = open_field();
            let mut events = Vec::new();
            go(&mut w, target);
            w.advance(300, &rules, &mut events);
            let at = w.body(BodyId(1)).expect("body 1").pos();
            assert!(w.map().get(at).is_some(), "move_to {target:?}: at {at:?}");
            assert!(
                events.iter().any(|e| matches!(
                    e,
                    Event::BodyStopped {
                        body: BodyId(1),
                        ..
                    } | Event::MoveRefused {
                        body: BodyId(1),
                        ..
                    }
                )),
                "move_to {target:?}: {events:?}"
            );

            events.clear();
            w.queue(Intention::Teleport {
                body: BodyId(1),
                target,
            });
            w.advance(1, &rules, &mut events);
            let at = w.body(BodyId(1)).expect("body 1").pos();
            assert!(w.map().get(at).is_some(), "teleport {target:?}: at {at:?}");
            assert!(
                events.iter().any(|e| matches!(
                    e,
                    Event::MoveRefused { body: BodyId(1), target: t, reason: Blockage::NoWay } if *t == target
                ) || matches!(e, Event::BodyMoved { body: BodyId(1), .. })),
                "teleport {target:?}: {events:?}"
            );

            events.clear();
            w.queue(Intention::Drink {
                body: BodyId(1),
                at: target,
            });
            w.advance(1, &rules, &mut events);
            assert!(
                events.iter().any(|e| matches!(
                    e,
                    Event::DrinkRefused { body: BodyId(1), at, reason: Refusal::OutOfReach | Refusal::NotWater } if *at == target
                )),
                "drink {target:?}: {events:?}"
            );
        }
    }

    #[test]
    fn a_teleport_stands_the_body_on_walkable_ground_nearby() {
        let mut w = small_world();
        let rules = rules();
        let mut events = Vec::new();
        // Into the cliff (rock), 20 rows above its foot: the nearest
        // walkable tile below it.
        let foot = i32::try_from(w.map().height() / 8).expect("small");
        let target = Pos::new(100, foot - 20);
        w.queue(Intention::Teleport {
            body: BodyId(1),
            target,
        });
        w.advance(1, &rules, &mut events);
        let at = w.body(BodyId(1)).expect("body 1").pos();
        assert!(
            rules
                .movement
                .cost(w.map().get(at).expect("on the map"))
                .is_some(),
            "{at:?}"
        );
        assert!(
            (at.x - target.x).abs() <= TELEPORT_REACH && (at.y - target.y).abs() <= TELEPORT_REACH
        );
        assert!(events.iter().any(
            |e| matches!(e, Event::BodyMoved { body: BodyId(1), to, remaining: 0, .. } if *to == at)
        ));
        assert!(events.iter().any(|e| matches!(
            e,
            Event::BodyStopped {
                body: BodyId(1),
                ..
            }
        )));
        // Far past the map: nothing walkable within reach, refused.
        let far = Pos::new(-1000, -1000);
        events.clear();
        w.queue(Intention::Teleport {
            body: BodyId(1),
            target: far,
        });
        w.advance(1, &rules, &mut events);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::MoveRefused { target, .. } if *target == far))
        );
        assert_eq!(w.body(BodyId(1)).expect("body 1").pos(), at);
    }

    #[test]
    fn advancing_is_deterministic() {
        let mut a = small_world();
        let mut b = small_world();
        let (mut ea, mut eb) = (Vec::new(), Vec::new());
        // Some grass near body 1, whatever the valley's size.
        let start = a.body(BodyId(1)).expect("body 1").pos();
        let target = a
            .map()
            .nearest(Pos::new(start.x + 20, start.y), Terrain::Grass)
            .expect("grass near the camp");
        go(&mut a, target);
        go(&mut b, target);
        a.advance(1_000, &rules(), &mut ea);
        b.advance(1_000, &rules(), &mut eb);
        assert_eq!(a.tick(), b.tick());
        assert_eq!(ea, eb);
        assert_eq!(a.bodies(), b.bodies());
        assert!(ea.iter().any(|e| matches!(e, Event::BodyMoved { .. })));
    }

    #[test]
    fn a_body_walks_at_the_content_speed_and_arrives() {
        // 143 cm/s at 10 ticks/s: 143 mm per tick; two tiles east is
        // 2 000 mm from centre to centre, so 14 ticks.
        let mut w = open_field();
        let mut events = Vec::new();
        go(&mut w, Pos::new(2, 0));
        w.step(&rules(), &mut events); // tick 1: the walk starts and the body moves at once
        assert_eq!(
            events[0],
            Event::BodyWalks {
                body: BodyId(1),
                from: Pos::new(0, 0).centre(),
                route: vec![Pos::new(2, 0).centre()],
                speed_cm_per_s: 143,
            },
            "one straight segment, no step through tile (1, 0)"
        );
        assert_eq!(w.body(BodyId(1)).unwrap().at(), FinePos::new(643, 500));
        events.clear();
        let took = 1 + until_stopped(&mut w, &mut events);
        assert_eq!(took, 14);
        let body = w.body(BodyId(1)).unwrap();
        assert_eq!(body.at(), Pos::new(2, 0).centre());
        assert!(!body.is_walking());
        let moved: Vec<Pos> = events
            .iter()
            .filter_map(|e| match e {
                Event::BodyMoved { to, .. } => Some(*to),
                _ => None,
            })
            .collect();
        assert_eq!(
            moved,
            vec![Pos::new(1, 0), Pos::new(2, 0)],
            "one event per tile crossed"
        );
        assert_eq!(
            events.last(),
            Some(&Event::BodyStopped {
                body: BodyId(1),
                at: Pos::new(2, 0).centre()
            })
        );
    }

    #[test]
    fn a_diagonal_walk_is_one_straight_segment() {
        let mut w = open_field();
        let mut events = Vec::new();
        go(&mut w, Pos::new(3, 3));
        // 3 000√2 = 4 243 mm at 143 mm per tick: 30 ticks.
        let took = until_stopped(&mut w, &mut events);
        assert_eq!(took, 30);
        assert!(matches!(&events[0], Event::BodyWalks { route, .. } if route.len() == 1));
        assert_eq!(w.body(BodyId(1)).unwrap().at(), Pos::new(3, 3).centre());
    }

    #[test]
    fn forest_slows_the_walk_down() {
        let mut w = World::empty(
            Calendar::new(10, 100).unwrap(),
            Map::filled(8, 1, Terrain::Forest),
        );
        w.spawn_body(Pos::new(0, 0));
        let mut events = Vec::new();
        go(&mut w, Pos::new(2, 0));
        // 95 cm/s in the forest: 95 mm per tick, 2 000 mm: 22 ticks.
        assert_eq!(until_stopped(&mut w, &mut events), 22);
    }

    #[test]
    fn a_new_intention_replaces_the_current_walk() {
        let mut w = open_field();
        let mut events = Vec::new();
        go(&mut w, Pos::new(5, 0));
        w.step(&rules(), &mut events);
        go(&mut w, Pos::new(1, 3));
        w.advance(60, &rules(), &mut events);
        assert_eq!(w.body(BodyId(1)).unwrap().pos(), Pos::new(1, 3));
        assert_eq!(w.body(BodyId(1)).unwrap().at(), Pos::new(1, 3).centre());
    }

    #[test]
    fn a_follower_within_reach_takes_its_leaders_way_and_one_too_far_walks_its_own() {
        let mut w = World::empty(
            Calendar::new(10, 100).unwrap(),
            Map::filled(48, 48, Terrain::Grass),
        );
        w.spawn_body(Pos::new(2, 2));
        let rules = rules();
        let leader = w.body(BodyId(1)).unwrap().pos();
        let near = w.spawn_body(Pos::new(leader.x + 2, leader.y));
        let far = w.spawn_body(Pos::new(leader.x + FOLLOW_TILES + 3, leader.y));
        let mut events = Vec::new();
        let target = Pos::new(leader.x + 20, leader.y + 12);
        w.queue(Intention::MoveTo {
            body: BodyId(1),
            target,
        });
        w.step(&rules, &mut events);
        let way: Vec<FinePos> = w.body(BodyId(1)).unwrap().route().collect();
        assert_ne!(way, Vec::<FinePos>::new());
        events.clear();
        for body in [near, far] {
            w.queue(Intention::Follow {
                body,
                leader: BodyId(1),
                target,
            });
        }
        w.step(&rules, &mut events);
        // Within reach: the leader's way whole, from where it stood.
        let followed: Vec<FinePos> = w.body(near).unwrap().route().collect();
        assert_eq!(followed.last(), way.last());
        assert!(followed.len() >= way.len());
        // Too far: a way of its own to the same target.
        assert!(w.body(far).unwrap().is_walking());
        assert_eq!(
            w.body(far).unwrap().route().last().map(FinePos::tile),
            Some(target)
        );
    }

    #[test]
    fn walking_to_where_one_stands_stops_at_once() {
        let mut w = open_field();
        let mut events = Vec::new();
        go(&mut w, Pos::new(0, 0));
        w.step(&rules(), &mut events);
        assert_eq!(
            events,
            vec![Event::BodyStopped {
                body: BodyId(1),
                at: Pos::new(0, 0).centre()
            }]
        );
    }

    #[test]
    fn walks_without_a_body_are_refused_and_off_the_map_stop_at_its_edge() {
        let mut w = open_field();
        let mut events = Vec::new();
        go(&mut w, Pos::new(50, 50));
        w.queue(Intention::MoveTo {
            body: BodyId(9),
            target: Pos::new(1, 1),
        });
        w.step(&rules(), &mut events);
        assert_eq!(
            events,
            vec![
                // No path off the map: straight to its edge, 30 cm short.
                Event::BodyWalks {
                    body: BodyId(1),
                    from: Pos::new(0, 0).centre(),
                    route: vec![FinePos::new(7788, 7788)],
                    speed_cm_per_s: rules().movement.walk_speed_cm_per_s,
                },
                Event::MoveRefused {
                    body: BodyId(9),
                    target: Pos::new(1, 1),
                    reason: Blockage::NoWay,
                },
            ]
        );
    }

    /// Runs `world` until body 1 stops, at most `ticks` ticks.
    fn walk_until_stopped(world: &mut World, ticks: u32) -> Vec<Event> {
        let rules = rules();
        let mut events = Vec::new();
        for _ in 0..ticks {
            world.step(&rules, &mut events);
            if events.iter().any(|e| {
                matches!(
                    e,
                    Event::BodyStopped {
                        body: BodyId(1),
                        ..
                    } | Event::MoveRefused { .. }
                )
            }) {
                return events;
            }
        }
        panic!("body 1 never stopped");
    }

    #[test]
    fn with_no_path_within_reach_the_body_walks_straight_to_the_obstacle() {
        // A target farther than a path may go, walled in by rock: the
        // body walks straight toward it and stops before the rock. One
        // tick a second, days long enough not to tire.
        let mut map = Map::filled(1500, 1500, Terrain::Grass);
        for (x, y) in [(0, 1), (1, 0), (1, 1)] {
            map.set(Pos::new(x, y), Terrain::Rock);
        }
        const { assert!(1499 > map::PATH_REACH) };
        let mut w = World::empty(Calendar::new(1, 1_000_000).unwrap(), map);
        w.spawn_body(Pos::new(1499, 1499));
        go(&mut w, Pos::new(0, 0));
        let events = walk_until_stopped(&mut w, 5000);
        assert!(matches!(events[0], Event::BodyWalks { ref route, .. } if route.len() == 1));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::MoveRefused { .. }))
        );
        assert_eq!(w.body(BodyId(1)).expect("body 1").pos(), Pos::new(2, 2));
    }

    #[test]
    fn a_new_order_while_walking_sets_off_straight_from_where_the_body_is() {
        // Found in play (12 November 2026): a new destination clicked
        // while walking could send the body toward the centre of a tile
        // next to it first. Walking east through one tile of trees in a
        // field, past its middle, it is sent elsewhere three times over:
        // each walk sets off from where it is, to the millimetre, in one
        // straight segment, the way being clear (leaving the tile it
        // stands on costs what it costs, dearer or not).
        let rules = rules();
        let mut map = Map::filled(40, 40, Terrain::Grass);
        map.set(Pos::new(10, 10), Terrain::Forest);
        let mut w = World::empty(Calendar::new(10, 1_000_000).unwrap(), map);
        w.spawn_body(Pos::new(5, 10));
        go(&mut w, Pos::new(30, 10));
        let mut events = Vec::new();
        while w.body(BodyId(1)).expect("body 1").at().x < 10_700 {
            w.step(&rules, &mut events);
        }
        for target in [Pos::new(4, 20), Pos::new(25, 14), Pos::new(10, 0)] {
            let at = w.body(BodyId(1)).expect("body 1").at();
            assert_eq!(at.tile(), Pos::new(10, 10), "still under the trees");
            go(&mut w, target);
            events.clear();
            w.step(&rules, &mut events);
            let walks = events.iter().find_map(|e| match e {
                Event::BodyWalks { from, route, .. } => Some((*from, route.clone())),
                _ => None,
            });
            assert_eq!(
                walks,
                Some((at, vec![target.centre()])),
                "toward {target:?}: from where it is, straight"
            );
        }
    }

    #[test]
    fn a_short_way_round_in_sight_is_always_found() {
        // Seen in play (12 November 2026): a wall two tiles ahead, its
        // end two tiles aside, the target eight to twenty tiles away.
        // Within the sight and the budget the way round is taken at once,
        // on either side, in the open and from the edge of a wood, which
        // sees out (D32).
        let rules = rules();
        for (under, distances) in [
            (Terrain::Grass, &[8, 12, 16, 20][..]),
            (Terrain::Forest, &[8, 10, 12][..]),
        ] {
            for &distance in distances {
                for side in [-1, 1] {
                    let mut map = Map::filled(60, 60, Terrain::Grass);
                    // The body at (20, 30) facing east, the wood (or the
                    // field) behind it; the wall at x 22, from one tile
                    // on one side to three on the other: its end, two
                    // tiles aside, is the way round.
                    for x in 10..=20 {
                        for y in 20..=40 {
                            map.set(Pos::new(x, y), under);
                        }
                    }
                    for dy in -3..=1 {
                        map.set(Pos::new(22, 30 - side * dy), Terrain::Water);
                    }
                    let mut w = World::empty(Calendar::new(10, 1_000_000).unwrap(), map);
                    w.spawn_body(Pos::new(20, 30));
                    let target = Pos::new(20 + distance, 30);
                    assert_eq!(
                        walk_with(&mut w, &rules, target),
                        target,
                        "from {under:?}, {distance} tiles, side {side}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_body_grazing_a_water_corner_walks_on() {
        // Found on the 768 valley: a straight walk passing 3 cm from the
        // corner of a water tile left the body a few millimetres into it,
        // its speed zero, stuck for good. It must walk on.
        let mut map = Map::filled(20, 20, Terrain::Grass);
        map.set(Pos::new(5, 5), Terrain::Water);
        let mut w = World::empty(Calendar::new(1, 1_000_000).unwrap(), map);
        w.spawn_body(Pos::new(2, 2));
        let body = &mut w.bodies[0];
        body.at = FinePos::new(5020, 5030);
        body.route = VecDeque::from([Pos::new(9, 9).centre()]);
        walk_until_stopped(&mut w, 100);
        assert_eq!(w.body(BodyId(1)).expect("body 1").pos(), Pos::new(9, 9));
    }

    #[test]
    fn across_a_river_with_its_ford_out_of_reach_the_body_walks_to_the_bank() {
        // A river with one ford upstream (10 November 2026: the sight is
        // the content's 60 tiles in the open, under a budget of 1,500
        // tiles, where it was the view's 288 with no budget). A ford
        // fifteen tiles aside is seen and taken; one fifty tiles aside is
        // in sight but the way round it costs more than the budget; one
        // seventy tiles aside is out of sight. Since 12 November 2026 a
        // body stopped at the bank skirts it once, about fifty tiles to the
        // side where the river ends sooner in sight, and looks once more:
        // the ford fifty tiles aside is then found; the one seventy tiles
        // aside is not, the two sides alike in sight, and the body stops
        // after one skirt (D32).
        let river = |ford: i32| {
            let mut map = Map::filled(900, 200, Terrain::Grass);
            for x in 0..900 {
                map.set(Pos::new(x, 100), Terrain::Water);
            }
            map.set(Pos::new(ford, 100), Terrain::Ford);
            map
        };
        for (ford, reaches, why) in [
            (
                815,
                true,
                "a ford in sight, the way round within the budget",
            ),
            (850, true, "a ford in sight, the way round over the budget"),
            (870, false, "a ford out of sight"),
        ] {
            let mut w = World::empty(Calendar::new(1, 1_000_000).unwrap(), river(ford));
            w.spawn_body(Pos::new(800, 90));
            let events = walk_until_done(&mut w, &rules(), Pos::new(800, 110), 20_000);
            let at = w.body(BodyId(1)).expect("body 1").pos();
            if reaches {
                assert_eq!(at, Pos::new(800, 110), "{why}");
            } else {
                assert_eq!(blocks(&events).len(), 1, "{why}");
                assert!(at.y == 99 && (at.x - 800).abs() <= 50, "{why}: {at:?}");
            }
        }
    }

    #[test]
    fn an_order_across_the_water_from_the_bank_is_refused_saying_why() {
        // Found in play (12 November 2026): the body already at the bank,
        // a click across the river refused at once said nothing. It says
        // what it sees barring its way, as a blocked body does (D32).
        // The body stands on a rock in the river, a single tile: no bank
        // to skirt along either (D32).
        let mut map = Map::filled(100, 100, Terrain::Grass);
        for y in 45..=55 {
            for x in 0..100 {
                map.set(Pos::new(x, y), Terrain::Water);
            }
        }
        map.set(Pos::new(50, 49), Terrain::Grass);
        let mut w = World::empty(Calendar::new(1, 1_000_000).unwrap(), map);
        w.spawn_body(Pos::new(50, 49));
        go(&mut w, Pos::new(50, 60));
        let mut events = Vec::new();
        w.step(&rules(), &mut events);
        assert!(
            events.contains(&Event::MoveRefused {
                body: BodyId(1),
                target: Pos::new(50, 60),
                reason: Blockage::Water,
            }),
            "{events:?}"
        );
    }

    /// A field with a wall of water across the middle, and a body `from`
    /// its west side with its target east of it, forty tiles away: the
    /// way round exists, thirty tiles aside. The body stands in a patch
    /// of `terrain_under_the_body` five tiles across.
    fn walled_field(terrain_under_the_body: Terrain) -> (World, Pos) {
        let mut map = Map::filled(80, 80, Terrain::Grass);
        for y in 25..=55 {
            map.set(Pos::new(40, y), Terrain::Water);
        }
        for y in 38..=42 {
            for x in 18..=22 {
                map.set(Pos::new(x, y), terrain_under_the_body);
            }
        }
        let mut w = World::empty(Calendar::new(1, 1_000_000).unwrap(), map);
        w.spawn_body(Pos::new(20, 40));
        (w, Pos::new(60, 40))
    }

    fn walk_with(w: &mut World, rules: &Content, target: Pos) -> Pos {
        go(w, target);
        let mut events = Vec::new();
        for _ in 0..5000 {
            w.step(rules, &mut events);
            if events.iter().any(|e| {
                matches!(
                    e,
                    Event::BodyStopped {
                        body: BodyId(1),
                        ..
                    } | Event::MoveRefused { .. }
                )
            }) {
                return w.body(BodyId(1)).expect("body 1").pos();
            }
        }
        panic!("body 1 never stopped");
    }

    /// The route body 1 sets off along toward `target`.
    fn first_route(w: &mut World, rules: &Content, target: Pos) -> Vec<FinePos> {
        go(w, target);
        let mut events = Vec::new();
        w.step(rules, &mut events);
        events
            .into_iter()
            .find_map(|e| match e {
                Event::BodyWalks { route, .. } => Some(route),
                _ => None,
            })
            .expect("body 1 sets off")
    }

    #[test]
    fn sight_follows_the_ground_around_the_body() {
        // The same ground: from the grass (sight 60) the way round the
        // water is seen and taken at once; from the heart of a wood (sight
        // 25) the target is out of sight, but the water's end is not: the
        // body goes round it back to its line beyond the water, then on
        // (12 November 2026).
        let mut rules = rules();
        rules.movement.path_budget_tiles = u32::MAX;
        let (mut w, target) = walled_field(Terrain::Grass);
        assert!(first_route(&mut w, &rules, target).len() > 1, "a way round");
        assert_eq!(walk_with(&mut w, &rules, target), target, "round the water");
        let (mut w, target) = walled_field(Terrain::Forest);
        let route = first_route(&mut w, &rules, target);
        assert!(route.len() > 1, "round the water's end");
        assert_eq!(
            route.last(),
            Some(&FinePos::new(41_500, 40_500)),
            "back on its line"
        );
        let mut events = Vec::new();
        for _ in 0..500 {
            w.step(&rules, &mut events);
        }
        assert_eq!(
            w.body(BodyId(1)).expect("body 1").pos(),
            target,
            "then round it"
        );
        // In the heart of a wood a near way round is seen: a pond two
        // tiles ahead, the target ten tiles away.
        let mut map = Map::filled(80, 80, Terrain::Forest);
        for y in 38..=42 {
            map.set(Pos::new(23, y), Terrain::Water);
        }
        let mut w = World::empty(Calendar::new(1, 1_000_000).unwrap(), map);
        w.spawn_body(Pos::new(20, 40));
        assert!(first_route(&mut w, &rules, Pos::new(30, 40)).len() > 1);
        assert_eq!(
            walk_with(&mut w, &rules, Pos::new(30, 40)),
            Pos::new(30, 40),
            "round the pond"
        );
    }

    #[test]
    fn at_a_woods_edge_the_body_sees_out() {
        // Sight from the most open walkable ground the body stands on or
        // touches: the open's 60 at a wood's edge, even across a corner;
        // the forest's 25 in its heart; water beside it is no opening.
        let rules = rules();
        let mut map = Map::filled(20, 20, Terrain::Forest);
        map.set(Pos::new(15, 15), Terrain::Grass);
        map.set(Pos::new(5, 6), Terrain::Water);
        let w = World::empty(Calendar::new(1, 1_000_000).unwrap(), map);
        assert_eq!(
            w.sight_tiles_at(Pos::new(15, 15), &rules),
            60,
            "in the open"
        );
        assert_eq!(
            w.sight_tiles_at(Pos::new(14, 14), &rules),
            60,
            "at the edge"
        );
        assert_eq!(
            w.sight_tiles_at(Pos::new(15, 13), &rules),
            25,
            "two tiles in"
        );
        assert_eq!(w.sight_tiles_at(Pos::new(5, 5), &rules), 25, "beside water");
        assert_eq!(
            w.sight_tiles_at(Pos::new(0, 0), &rules),
            25,
            "at the map's corner"
        );
    }

    #[test]
    fn stopped_by_a_wall_the_body_looks_again_and_goes_round() {
        // A wall of water seventy tiles long, from the map's edge, its end
        // eight tiles aside of the way; the target eighty tiles off, out of
        // sight. The wall's end in sight, the body goes round it back to
        // its line beyond the wall (12 November 2026), looks again from
        // there, sees the target and goes on.
        let rules = rules();
        let mut map = Map::filled(140, 140, Terrain::Grass);
        for y in 0..=70 {
            map.set(Pos::new(70, y), Terrain::Water);
        }
        let mut w = World::empty(Calendar::new(10, 1_000_000).unwrap(), map);
        w.spawn_body(Pos::new(20, 62));
        let target = Pos::new(100, 62);
        let route = first_route(&mut w, &rules, target);
        assert_eq!(
            route.last(),
            Some(&FinePos::new(71_500, 62_500)),
            "past the wall"
        );
        assert!(route.len() > 1, "round its end");
        let mut events = Vec::new();
        for _ in 0..2000 {
            w.step(&rules, &mut events);
        }
        let walks = events
            .iter()
            .filter(|e| matches!(e, Event::BodyWalks { .. }))
            .count();
        assert_eq!(walks, 1, "one look again, past the wall");
        assert_eq!(w.body(BodyId(1)).expect("body 1").pos(), target);
    }

    #[test]
    fn a_body_looks_again_only_from_nearer() {
        // A target walled in: the body walks straight to the wall, looks
        // again, finds no way, skirts the wall once, looks once more from
        // there and stops for good: no second skirt.
        let rules = rules_before_d54();
        let mut map = Map::filled(60, 60, Terrain::Grass);
        for i in 35..=45 {
            for p in [(i, 35), (i, 45), (35, i), (45, i)] {
                map.set(Pos::new(p.0, p.1), Terrain::Water);
            }
        }
        let mut w = World::empty(Calendar::new(10, 1_000_000).unwrap(), map);
        w.spawn_body(Pos::new(10, 40));
        let route = first_route(&mut w, &rules, Pos::new(40, 40));
        assert_eq!(route, vec![FinePos::new(34_700, 40_500)]);
        let mut events = Vec::new();
        for _ in 0..1000 {
            w.step(&rules, &mut events);
        }
        let walks = events
            .iter()
            .filter(|e| matches!(e, Event::BodyWalks { .. }))
            .count();
        assert_eq!(walks, 1, "one skirt along the wall");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::MoveRefused { .. }))
        );
        let body = w.body(BodyId(1)).expect("body 1");
        assert_eq!((body.goal, body.skirted), (None, Skirted::Not));
        // Stopped no nearer than where it last looked (pushed back, say):
        // it does not look again. From nearer, it does.
        for (looked_from, looks) in [(0, false), (u64::MAX, true)] {
            w.bodies[0].at = Pos::new(30, 40).centre();
            w.bodies[0].goal = Some(Goal {
                target: Pos::new(40, 40),
                looked_from,
            });
            events.clear();
            w.step(&rules, &mut events);
            let walked = events.iter().any(|e| matches!(e, Event::BodyWalks { .. }));
            assert_eq!(walked, looks, "looked from {looked_from}");
        }
    }

    /// The blocked signals among `events`: (where, target, reason).
    fn blocks(events: &[Event]) -> Vec<(FinePos, Pos, Blockage)> {
        events
            .iter()
            .filter_map(|e| match *e {
                Event::BodyBlocked {
                    body: BodyId(1),
                    at,
                    target,
                    reason,
                } => Some((at, target, reason)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_body_stopped_for_good_says_once_what_bars_its_way() {
        // A target walled in by water, then by the cliff: the body walks
        // straight to the wall, stops, looks again, finds nothing and
        // stops for good: told once, after its stop, with what it sees.
        for (wall, reason) in [
            (Terrain::Water, Blockage::Water),
            (Terrain::Rock, Blockage::Rock),
        ] {
            let rules = rules_before_d54();
            let mut map = Map::filled(60, 60, Terrain::Grass);
            for i in 35..=45 {
                for p in [(i, 35), (i, 45), (35, i), (45, i)] {
                    map.set(Pos::new(p.0, p.1), wall);
                }
            }
            let mut w = World::empty(Calendar::new(10, 1_000_000).unwrap(), map);
            w.spawn_body(Pos::new(10, 40));
            let target = Pos::new(40, 40);
            go(&mut w, target);
            let mut events = Vec::new();
            for _ in 0..1000 {
                w.step(&rules, &mut events);
            }
            // At the end of its one skirt along the wall.
            let stop = w.body(BodyId(1)).expect("body 1").at;
            assert_eq!(blocks(&events), vec![(stop, target, reason)], "{wall:?}");
            let stopped = events
                .iter()
                .rposition(|e| matches!(e, Event::BodyStopped { .. }))
                .expect("it stops");
            let blocked = events
                .iter()
                .position(|e| matches!(e, Event::BodyBlocked { .. }))
                .expect("it is blocked");
            assert!(stopped < blocked, "told once it stands, not before");
        }
    }

    #[test]
    fn a_body_with_nothing_ahead_and_no_way_says_it_sees_none() {
        // Stopped no nearer than where it last looked, open grass ahead:
        // it stops for good and sees no way, nothing barring it by name.
        let rules = rules();
        let mut w = open_field();
        let target = Pos::new(7, 0);
        w.bodies[0].goal = Some(Goal {
            target,
            looked_from: 0,
        });
        let mut events = Vec::new();
        w.step(&rules, &mut events);
        assert_eq!(
            blocks(&events),
            vec![(Pos::new(0, 0).centre(), target, Blockage::NoWay)]
        );
        assert_eq!(w.bodies[0].goal, None);
    }

    #[test]
    fn no_blocked_signal_on_arrival_nor_at_a_stop_the_body_goes_on_from() {
        // Arriving on open ground: never blocked.
        let rules = rules();
        let mut w = open_field();
        go(&mut w, Pos::new(7, 7));
        let mut events = Vec::new();
        for _ in 0..500 {
            w.step(&rules, &mut events);
        }
        assert_eq!(w.body(BodyId(1)).expect("body 1").pos(), Pos::new(7, 7));
        assert_eq!(blocks(&events), vec![]);
        // Stopped by a wall with a way round in sight from there: it
        // stops, looks again, goes round and arrives, never blocked.
        let mut map = Map::filled(140, 140, Terrain::Grass);
        for y in 0..=70 {
            map.set(Pos::new(70, y), Terrain::Water);
        }
        let mut w = World::empty(Calendar::new(10, 1_000_000).unwrap(), map);
        w.spawn_body(Pos::new(20, 62));
        let target = Pos::new(100, 62);
        go(&mut w, target);
        let mut events = Vec::new();
        for _ in 0..2000 {
            w.step(&rules, &mut events);
        }
        let stops = events
            .iter()
            .filter(|e| matches!(e, Event::BodyStopped { .. }))
            .count();
        assert_eq!(stops, 2, "at the wall, then at the target");
        assert_eq!(w.body(BodyId(1)).expect("body 1").pos(), target);
        assert_eq!(blocks(&events), vec![]);
    }

    #[test]
    fn a_search_over_its_budget_walks_straight() {
        // The same way round, in sight: found under a generous budget,
        // given up under a tight one, the body walking straight instead.
        let mut rules = rules();
        rules.movement.path_budget_tiles = u32::MAX;
        let (mut w, target) = walled_field(Terrain::Grass);
        assert_eq!(walk_with(&mut w, &rules, target), target);
        rules.movement.path_budget_tiles = 100;
        let (mut w, target) = walled_field(Terrain::Grass);
        assert_eq!(walk_with(&mut w, &rules, target), Pos::new(39, 40));
    }

    /// The bodies setting off among `events`, in the order they did.
    fn walkers(events: &[Event]) -> Vec<BodyId> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::BodyWalks { body, .. } => Some(*body),
                _ => None,
            })
            .collect()
    }

    /// An open field of `side` tiles with `count` bodies on a grid two
    /// tiles apart from (10, 10), a hundred to a row.
    fn crowd(side: u32, count: i32) -> World {
        let mut w = World::empty(
            Calendar::new(10, 1_000_000).unwrap(),
            Map::filled(side, side, Terrain::Grass),
        );
        for i in 0..count {
            w.spawn_body(Pos::new(10 + (i % 100) * 2, 10 + (i / 100) * 2));
        }
        w
    }

    #[test]
    fn a_wave_of_five_thousand_orders_clears_in_a_few_ticks() {
        // 5,000 bodies ordered in one tick, half to a target in sight
        // thirty tiles off, half to one out of sight: under the cap of
        // work, a search refused at once costing next to nothing, every
        // one sets off within two ticks (forty searches a tick took 125).
        // The same twice over, to the event.
        let rules = rules();
        let run = || {
            let mut w = crowd(400, 5000);
            for id in 1..=5000u32 {
                let at = w.body(BodyId(id)).expect("body").pos();
                let target = if id % 2 == 0 {
                    Pos::new(at.x + 30, at.y)
                } else {
                    Pos::new(at.x, at.y + 150)
                };
                w.queue(Intention::MoveTo {
                    body: BodyId(id),
                    target,
                });
            }
            let mut ticks = Vec::new();
            for _ in 0..3 {
                let mut events = Vec::new();
                w.step(&rules, &mut events);
                ticks.push(events);
            }
            ticks
        };
        let ticks = run();
        let set_off: Vec<usize> = ticks.iter().map(|t| walkers(t).len()).collect();
        assert_eq!(set_off.iter().sum::<usize>(), 5000, "{set_off:?}");
        assert_eq!(set_off[2], 0, "all set off within two ticks: {set_off:?}");
        assert!(set_off[0] >= 2500, "most in the first: {set_off:?}");
        assert_eq!(run(), ticks);
    }

    #[test]
    fn players_are_served_before_npcs_under_saturation() {
        // A cap of 400 tiles a tick, half for players; each walk here
        // costs some thirty tiles. Forty NPC orders at once: the NPCs
        // take the whole cap while no player asks. Five players order
        // in the second tick, behind the NPCs' backlog: they all set off
        // at once, first, and the NPCs keep their own half.
        let mut rules = rules();
        rules.movement.search_work_per_tick = 400;
        rules.movement.player_share_percent = 50;
        let mut w = crowd(200, 45);
        w.set_piloted((41..=45).map(BodyId));
        let order = |w: &mut World, id: u32| {
            let at = w.body(BodyId(id)).expect("body").pos();
            w.queue(Intention::MoveTo {
                body: BodyId(id),
                target: Pos::new(at.x + 30, at.y),
            });
        };
        for id in 1..=40 {
            order(&mut w, id);
        }
        let mut first = Vec::new();
        w.step(&rules, &mut first);
        let npcs_first = walkers(&first).len();
        assert!(npcs_first > 6, "the players' spare share went to the NPCs");
        for id in 41..=45 {
            order(&mut w, id);
        }
        let mut second = Vec::new();
        w.step(&rules, &mut second);
        let walked = walkers(&second);
        assert_eq!(walked[..5], (41..=45).map(BodyId).collect::<Vec<_>>()[..]);
        let npcs = walked.len() - 5;
        assert!(npcs > 0, "the NPCs keep their share: {walked:?}");
        assert!(
            npcs_first + npcs < 40,
            "a backlog of NPCs still waits: {walked:?}"
        );
    }

    #[test]
    fn a_look_again_waits_its_turn_and_never_starves() {
        // A cap of 100 tiles a tick and a flood of five new orders every
        // tick, each some thirty tiles: the orders alone always fill it.
        // The last body stopped with a goal: its look again joins the
        // lane after the orders of its tick and runs before any order
        // that came later; it never waits behind the bodies listed first.
        let mut rules = rules();
        rules.movement.search_work_per_tick = 100;
        let mut w = crowd(200, 61);
        let looker = BodyId(61);
        let at = w.body(looker).expect("looker").pos();
        let index = w.index_of(looker).expect("looker");
        w.bodies[index].goal = Some(Goal {
            target: Pos::new(at.x + 30, at.y + 30),
            looked_from: u64::MAX,
        });
        let mut served = Vec::new();
        for tick in 0..12u32 {
            for id in tick * 5 + 1..=tick * 5 + 5 {
                let at = w.body(BodyId(id)).expect("body").pos();
                w.queue(Intention::MoveTo {
                    body: BodyId(id),
                    target: Pos::new(at.x + 30, at.y),
                });
            }
            let mut events = Vec::new();
            w.step(&rules, &mut events);
            served.extend(walkers(&events));
        }
        let turn = served
            .iter()
            .position(|b| *b == looker)
            .expect("the look again ran");
        assert_eq!(
            served[..turn],
            (1..=5).map(BodyId).collect::<Vec<_>>()[..],
            "after the orders of its own tick, before any later one"
        );
    }

    /// Runs body 1 toward `target` until it arrives or says it is blocked,
    /// at most `ticks` ticks; all the events.
    fn walk_until_done(w: &mut World, rules: &Content, target: Pos, ticks: u32) -> Vec<Event> {
        go(w, target);
        let mut events = Vec::new();
        for _ in 0..ticks {
            w.step(rules, &mut events);
            let body = w.body(BodyId(1)).expect("body 1");
            if body.pos() == target && body.route.is_empty() || !blocks(&events).is_empty() {
                break;
            }
        }
        events
    }

    /// Body 1's thoughts among `events`, in order.
    fn thoughts(events: &[Event]) -> Vec<Thought> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::BodyThinks {
                    body: BodyId(1),
                    thought,
                    ..
                } => Some(*thought),
                _ => None,
            })
            .collect()
    }

    /// A lake from the map's west edge to x = 180, from y = 100 to 250,
    /// and a body at (150, 80) sent to (150, 280), behind it, out of
    /// sight; its east end within sight of the bank, its far shore not.
    fn lake_with_one_end_in_sight() -> (World, Pos) {
        let mut map = Map::filled(300, 300, Terrain::Grass);
        for y in 100..=250 {
            for x in 0..=180 {
                map.set(Pos::new(x, y), Terrain::Water);
            }
        }
        let mut w = World::empty(Calendar::new(10, 1_000_000).unwrap(), map);
        w.spawn_body(Pos::new(150, 80));
        (w, Pos::new(150, 280))
    }

    #[test]
    fn at_an_obstacle_with_no_way_in_sight_the_body_skirts_toward_its_end() {
        // The body walks straight to the shore, sees no way past the lake
        // and walks along it toward the end it sees (to its left, facing
        // the target), some fifty tiles, staying by the water.
        let rules = rules();
        let (mut w, target) = lake_with_one_end_in_sight();
        go(&mut w, target);
        let mut events = Vec::new();
        let mut skirt = None;
        for _ in 0..2000 {
            w.step(&rules, &mut events);
            skirt = events.iter().find_map(|e| match e {
                Event::BodyWalks { from, route, .. } if from.tile().y == 99 => {
                    Some((*from, route.clone()))
                }
                _ => None,
            });
            if skirt.is_some() {
                break;
            }
        }
        let (from, route) = skirt.expect("it skirts from the shore");
        assert_eq!(
            thoughts(&events),
            vec![Thought::Skirting { side: Side::Left }]
        );
        let end = route.last().expect("a walk").tile();
        assert!(end.x > from.tile().x, "toward the lake's east end: {end:?}");
        let along = (end.x - from.tile().x)
            .abs()
            .max((end.y - from.tile().y).abs());
        assert!(
            (30..=50).contains(&along),
            "about fifty tiles along: {end:?}"
        );
        for p in &route {
            let t = p.tile();
            let by_water = (-3..=3).any(|dy| {
                (-3..=3).any(|dx| w.map().get(Pos::new(t.x + dx, t.y + dy)) == Some(Terrain::Water))
            });
            assert!(by_water, "{t:?} stays by the water");
        }
    }

    #[test]
    fn with_one_skirt_for_an_order_the_body_stops_after_it_and_says_it() {
        // The same lake, under the rule before D54: from the end of its
        // skirt the far shore is still out of sight. It looks once more,
        // sees no way, stops for good and says so: no second skirt, no
        // wandering.
        let rules = rules_before_d54();
        let (mut w, target) = lake_with_one_end_in_sight();
        go(&mut w, target);
        let mut events = Vec::new();
        for _ in 0..5000 {
            w.step(&rules, &mut events);
        }
        assert_eq!(blocks(&events).len(), 1, "stops once, for good");
        assert_eq!(
            thoughts(&events),
            vec![Thought::Skirting { side: Side::Left }, Thought::NoWay]
        );
        let walks = events
            .iter()
            .filter(|e| matches!(e, Event::BodyWalks { .. }))
            .count();
        assert_eq!(walks, 2, "to the shore, then along it once");
        let body = w.body(BodyId(1)).expect("body 1");
        assert_eq!((body.goal, body.skirted), (None, Skirted::Not));
        let blocked = events
            .iter()
            .position(|e| matches!(e, Event::BodyBlocked { .. }))
            .expect("blocked");
        assert!(
            matches!(
                events[blocked + 1],
                Event::BodyThinks {
                    thought: Thought::NoWay,
                    ..
                }
            ),
            "it thinks it right after"
        );
    }

    #[test]
    fn after_its_skirt_the_body_goes_on_and_skirts_again_until_it_sees_a_way() {
        // The same lake, under D54: from the end of its skirt the far
        // shore is out of sight, so instead of stopping it goes straight
        // on down the lake's east shore, skirts the shore again where it
        // turns, and so on till it sees a way and arrives: never blocked,
        // within the legs the content allows.
        let rules = rules();
        let (mut w, target) = lake_with_one_end_in_sight();
        go(&mut w, target);
        let mut events = Vec::new();
        for _ in 0..30_000 {
            w.step(&rules, &mut events);
            if w.body(BodyId(1)).expect("body 1").pos() == target {
                break;
            }
        }
        assert_eq!(blocks(&events), vec![], "never blocked");
        assert_eq!(w.body(BodyId(1)).expect("body 1").pos(), target);
        let thoughts = thoughts(&events);
        assert_eq!(thoughts[0], Thought::Skirting { side: Side::Left });
        assert_eq!(thoughts.last(), Some(&Thought::WaySeen), "{thoughts:?}");
        assert!(!thoughts.contains(&Thought::NoWay), "{thoughts:?}");
        let legs = thoughts
            .iter()
            .filter(|t| matches!(t, Thought::Skirting { .. }))
            .count();
        let cap = usize::try_from(rules.movement.skirts_per_order).expect("small");
        assert!((2..=cap).contains(&legs), "{legs} legs along the shore");
        let body = w.body(BodyId(1)).expect("body 1");
        assert!(body.went.is_empty() && body.skirted == Skirted::Not);
    }

    #[test]
    fn with_its_target_shut_in_the_body_stops_after_the_legs_the_content_allows() {
        // A ring of water round the target: no leg finds a way, and after
        // the legs the content allows (or sooner, were a leg to end where
        // one began or ended) the body stops once and says it sees no
        // way; under the rule before D54 it stopped after the first.
        for (rules, most) in [(rules(), 6), (rules_before_d54(), 1)] {
            let mut map = Map::filled(120, 120, Terrain::Grass);
            for i in 55..=65 {
                for p in [(i, 55), (i, 65), (55, i), (65, i)] {
                    map.set(Pos::new(p.0, p.1), Terrain::Water);
                }
            }
            let mut w = World::empty(Calendar::new(10, 1_000_000).unwrap(), map);
            w.spawn_body(Pos::new(20, 60));
            let target = Pos::new(60, 60);
            go(&mut w, target);
            let mut events = Vec::new();
            for _ in 0..20_000 {
                w.step(&rules, &mut events);
            }
            assert_eq!(blocks(&events).len(), 1, "stops once, for good");
            let thoughts = thoughts(&events);
            assert_eq!(thoughts.last(), Some(&Thought::NoWay), "{thoughts:?}");
            let legs = thoughts
                .iter()
                .filter(|t| matches!(t, Thought::Skirting { .. }))
                .count();
            assert!((1..=most).contains(&legs), "{legs} legs, at most {most}");
            assert_eq!(usize::try_from(rules.movement.skirts_per_order), Ok(most));
            let body = w.body(BodyId(1)).expect("body 1");
            assert!(body.went.is_empty() && body.skirted == Skirted::Not);
        }
    }

    #[test]
    fn from_the_end_of_its_skirt_the_body_takes_a_ford_it_sees() {
        // A river across the whole map, its ford 70 tiles east of where
        // the body meets it: out of sight from there. The target lies a
        // little east, across, out of sight. The body skirts east (the
        // bank toward the target), sees the ford from the end of its skirt,
        // crosses and arrives; it thinks "skirting", then "a way seen",
        // and nothing more.
        let rules = rules();
        let mut map = Map::filled(400, 300, Terrain::Grass);
        for y in 100..=104 {
            for x in 0..400 {
                map.set(Pos::new(x, y), Terrain::Water);
            }
            for x in 268..=272 {
                map.set(Pos::new(x, y), Terrain::Ford);
            }
        }
        let mut w = World::empty(Calendar::new(10, 1_000_000).unwrap(), map);
        w.spawn_body(Pos::new(200, 90));
        let target = Pos::new(230, 200);
        go(&mut w, target);
        let mut events = Vec::new();
        for _ in 0..10_000 {
            w.step(&rules, &mut events);
            if w.body(BodyId(1)).expect("body 1").pos() == target && blocks(&events).is_empty() {
                break;
            }
        }
        assert_eq!(blocks(&events), vec![], "never blocked");
        assert_eq!(w.body(BodyId(1)).expect("body 1").pos(), target);
        assert_eq!(
            thoughts(&events),
            vec![Thought::Skirting { side: Side::Left }, Thought::WaySeen]
        );
    }

    #[test]
    fn thoughts_come_in_the_order_of_the_decisions() {
        // Each thought follows the walk it decides, at the place it is
        // about: skirting at the end of the skirt, the stop where it
        // stands (under the rule before D54, which stops after one skirt).
        let rules = rules_before_d54();
        let (mut w, target) = lake_with_one_end_in_sight();
        go(&mut w, target);
        let mut events = Vec::new();
        for _ in 0..5000 {
            w.step(&rules, &mut events);
        }
        let mut last_walk_end = None;
        let mut seen = Vec::new();
        for e in &events {
            match e {
                Event::BodyWalks { route, .. } => last_walk_end = route.last().copied(),
                Event::BodyThinks { thought, at, .. } => {
                    match thought {
                        Thought::Skirting { .. } => assert_eq!(Some(*at), last_walk_end),
                        _ => assert_eq!(*at, w.body(BodyId(1)).expect("body 1").at),
                    }
                    seen.push(*thought);
                }
                _ => {}
            }
        }
        assert_eq!(seen.len(), 2);
    }

    #[test]
    fn a_new_world_has_rock_under_its_ground_and_an_old_one_none() {
        let content = Content::sample();
        let new = World::with_valley(Calendar::default(), &content, 256);
        assert_eq!(new.map().generator(), map::GENERATOR);
        let chunk = ChunkPos::new(3, 4);
        let rocks = new.rocks_in(chunk).expect("version 2 has rocks");
        assert_eq!(rocks.len(), 32 * 32);
        assert_eq!(
            *rocks,
            *content.geology.rocks_in(VALLEY_SEED, chunk, (0, 0))
        );
        assert!(new.rocks_in(ChunkPos::new(8, 0)).is_none(), "off the map");
        let old = World::on_valley(
            Calendar::default(),
            &content,
            Map::valley_by(map::FIRST_GENERATOR, 256, 256, VALLEY_SEED),
        );
        assert!(old.rocks_in(chunk).is_none(), "version 1 has none");
        // The rest of the ground is the same in both.
        for c in [chunk, ChunkPos::new(0, 0), ChunkPos::new(7, 7)] {
            assert_eq!(old.map().chunk(c), new.map().chunk(c));
            assert_eq!(*old.trees_in(c), *new.trees_in(c));
            assert_eq!(old.items_in(c), new.items_in(c));
        }
    }

    #[test]
    fn the_ground_drops_natural_items_by_the_spawn_rules() {
        let content = Content::sample();
        let a = World::with_valley(Calendar::default(), &content, 768);
        // None stored: the ground makes them per chunk.
        assert_eq!(a.items().count(), 0);
        assert!(a.has_natural_items());
        let chunks = || (0..24).flat_map(|y| (0..24).map(move |x| ChunkPos::new(x, y)));
        let all: Vec<Item> = chunks().flat_map(|c| a.items_in(c)).collect();
        let b = World::with_valley(Calendar::default(), &content, 768);
        assert_eq!(
            all,
            chunks().flat_map(|c| b.items_in(c)).collect::<Vec<_>>()
        );
        // Every item obeys a rule: on its terrain, near what it must be.
        for item in &all {
            let pos = item.pos().unwrap();
            let rule = content
                .spawns
                .iter()
                .find(|s| s.material == item.material())
                .expect("a rule for each material laid down");
            assert_eq!(a.map().get(pos), Some(rule.terrain), "{item:?}");
            if let Some(near) = rule.near {
                let r = i32::try_from(near.within).unwrap();
                assert!(
                    (-r..=r).any(|dy| (-r..=r)
                        .any(|dx| a.map().get(Pos::new(pos.x + dx, pos.y + dy))
                            == Some(near.terrain))),
                    "{item:?} is far from its {:?}",
                    near.terrain
                );
            }
        }
        // Densities land near their rule, over tens of thousands of tiles.
        let forest = a
            .map()
            .tiles()
            .iter()
            .filter(|t| **t == Terrain::Forest)
            .count();
        let branches = all.iter().filter(|i| i.material() == "branch").count();
        let per_thousand = branches * 1000 / forest;
        assert!(
            (45..=75).contains(&per_thousand),
            "{per_thousand}‰ branches"
        );
        assert_eq!(a.items_in(ChunkPos::new(-1, -1)), [] as [Item; 0]);
    }

    /// What a body is told when it tries an act on `item`.
    fn probe(w: &mut World, content: &Content, body: BodyId, intention: Intention) -> Refusal {
        let mut events = Vec::new();
        w.queue(intention);
        w.step(content, &mut events);
        events
            .iter()
            .find_map(|e| match e {
                Event::ActRefused {
                    body: b, reason, ..
                } if *b == body => Some(*reason),
                _ => None,
            })
            .unwrap_or_else(|| panic!("not refused: {events:?}"))
    }

    /// A natural item lying between `min` and `max` tiles of `from`.
    fn natural_item_from(w: &World, from: Pos, min: i64, max: i64) -> Item {
        (0..24)
            .flat_map(|y| (0..24).map(move |x| ChunkPos::new(x, y)))
            .flat_map(|c| w.items_in(c))
            .find(|i| {
                let p = i.pos().unwrap();
                let (dx, dy) = (i64::from(p.x - from.x), i64::from(p.y - from.y));
                let d2 = dx * dx + dy * dy;
                d2 >= min * min && d2 <= max * max && w.map().is_passable(p)
            })
            .expect("a natural item at that distance")
    }

    #[test]
    fn an_act_on_an_item_past_the_eyes_is_refused_unknown_and_makes_nothing() {
        // The security audit of 20 November 2026, F2 and M1: past the
        // eyes, a refusal is the same whether the item lies there, is
        // carried by someone, or does not exist, and a probe writes
        // nothing into the world.
        let content = Content::sample();
        let mut w = World::with_valley(Calendar::new(10, 100_000).unwrap(), &content, 768);
        let body = w.spawn_at_camp().expect("grass by the camp");
        let here = w.body(body).unwrap().pos();
        let far = natural_item_from(&w, here, i64::from(EYES_RANGE_M) + 40, 600);
        let there = far.pos().unwrap();
        assert!(!within_eyes(here, there));
        // A slot of the same chunk that holds nothing: a rule no content has.
        let empty = items::natural_in_slot(there.chunk(), items::slot_of(far.id()) | 63).unwrap();
        assert!(items::natural_item(w.map(), &content.spawns, w.seed(), empty).is_none());

        for item in [far.id(), empty] {
            for intention in [
                Intention::PickUp { body, item },
                Intention::Eat { body, item },
                Intention::Hold { body, item },
                Intention::Stow { body, item },
                Intention::Drop { body, item },
            ] {
                assert_eq!(probe(&mut w, &content, body, intention), Refusal::Unknown);
            }
        }
        // Nothing made, nothing marked, no chunk changed.
        assert!(w.item(far.id()).is_none());
        assert!(!w.is_taken(far.id()));
        assert_eq!(w.chunk_version(there.chunk()), 0);
        assert_eq!(w.chunk_changes().count(), 0);
        assert_eq!(w.items().count(), 0);

        // A stored item lying there, and one a body there carries: the same.
        let dropped = w.spawn_item("flint", 100, there);
        let other = w.spawn_body(there);
        let mut events = Vec::new();
        w.queue(Intention::PickUp {
            body: other,
            item: far.id(),
        });
        w.step(&content, &mut events);
        assert!(w.body(other).unwrap().has(far.id()));
        for item in [dropped, far.id()] {
            for intention in [
                Intention::PickUp { body, item },
                Intention::Hold { body, item },
                Intention::Drop { body, item },
            ] {
                assert_eq!(probe(&mut w, &content, body, intention), Refusal::Unknown);
            }
        }
    }

    #[test]
    fn within_the_eyes_an_act_is_refused_for_its_true_reason() {
        let content = Content::sample();
        let mut w = World::with_valley(Calendar::new(10, 100_000).unwrap(), &content, 768);
        let body = w.spawn_at_camp().expect("grass by the camp");
        let here = w.body(body).unwrap().pos();
        let near = natural_item_from(&w, here, 4, i64::from(EYES_RANGE_M) - 20);
        let there = near.pos().unwrap();
        assert!(within_eyes(here, there));
        // Lying within the eyes but not within reach: out of reach, and
        // still not made.
        assert_eq!(
            probe(
                &mut w,
                &content,
                body,
                Intention::PickUp {
                    body,
                    item: near.id()
                }
            ),
            Refusal::OutOfReach
        );
        assert!(w.item(near.id()).is_none(), "judged from its id alone");
        // A slot there that holds nothing: the same as one that does.
        let empty = items::natural_in_slot(there.chunk(), items::slot_of(near.id()) | 63).unwrap();
        assert_eq!(
            probe(
                &mut w,
                &content,
                body,
                Intention::PickUp { body, item: empty }
            ),
            Refusal::OutOfReach
        );
        // Carried by a body within the eyes: not carried.
        let other = w.spawn_body(there);
        let mut events = Vec::new();
        w.queue(Intention::PickUp {
            body: other,
            item: near.id(),
        });
        w.step(&content, &mut events);
        assert!(w.body(other).unwrap().has(near.id()));
        assert_eq!(
            probe(
                &mut w,
                &content,
                body,
                Intention::Hold {
                    body,
                    item: near.id()
                }
            ),
            Refusal::NotCarried
        );
        // An id that names nothing at all: unknown, wherever one stands.
        assert_eq!(
            probe(
                &mut w,
                &content,
                body,
                Intention::PickUp {
                    body,
                    item: ItemId(999)
                }
            ),
            Refusal::Unknown
        );
    }

    #[test]
    fn a_natural_item_taken_is_stored_and_never_comes_back() {
        let content = Content::sample();
        let mut w = World::with_valley(Calendar::new(10, 100_000).unwrap(), &content, 768);
        // A natural item, and a body set beside it.
        let item = (0..24)
            .flat_map(|y| (0..24).map(move |x| ChunkPos::new(x, y)))
            .flat_map(|c| w.items_in(c))
            .find(|i| {
                let p = i.pos().unwrap();
                w.map().is_passable(p)
            })
            .expect("a natural item on walkable ground");
        let at = item.pos().unwrap();
        assert!(items::is_natural(item.id()));
        let body = w.spawn_body(at);
        w.queue(Intention::PickUp {
            body,
            item: item.id(),
        });
        let mut events = Vec::new();
        w.step(&content, &mut events);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ItemRemoved { item: i, .. } if *i == item.id())),
            "{events:?}"
        );
        // Carried now, stored, and gone from its chunk for good.
        assert!(w.body(body).unwrap().has(item.id()));
        assert!(w.item(item.id()).is_some());
        assert!(w.is_taken(item.id()));
        assert_eq!(w.taken().collect::<Vec<_>>(), [item.id()]);
        assert_eq!(w.chunk_version(at.chunk()), 2, "taken, then lifted");
        assert!(w.items_in(at.chunk()).iter().all(|i| i.id() != item.id()));
        // Through a save and a load: the same.
        // (The snapshot holds no changes: the storage keeps them beside.)
        let mut back: World = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
        assert_eq!(back.chunk_changes().count(), 0);
        for (chunk, changes) in w.chunk_changes() {
            back.restore_changes(chunk, changes.clone());
        }
        back.take_content(&content);
        assert_eq!(back.chunk_version(at.chunk()), 2);
        assert!(
            back.items_in(at.chunk())
                .iter()
                .all(|i| i.id() != item.id())
        );
        assert!(back.item(item.id()).is_some());
    }

    fn fed_world() -> World {
        let mut w = World::empty(
            Calendar::new(10, 100).unwrap(), // 1000 ticks per day
            Map::filled(4, 4, Terrain::Grass),
        );
        w.spawn_body(Pos::new(1, 1));
        w
    }

    fn condition(w: &World) -> &Condition {
        w.body(BodyId(1)).unwrap().condition()
    }

    #[test]
    fn needs_rise_with_time_at_the_content_rates() {
        let mut w = fed_world();
        let mut events = Vec::new();
        // Half a day awake: hunger 16, thirst 50, fatigue 60; by day cold
        // stays 0.
        w.advance(500, &rules(), &mut events);
        assert_eq!(condition(&w).summary(), (100, 16, 50, 0, 60, false));
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::ConditionChanged {
                    body: BodyId(1),
                    ..
                }
            )),
            "clients are told"
        );
    }

    #[test]
    fn night_brings_cold_and_dawn_relieves_it() {
        let mut w = fed_world();
        let mut events = Vec::new();
        w.advance(580, &rules(), &mut events); // dusk at 58 %
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::NightFell { day: 1 }))
        );
        assert!(w.is_night(&rules()));
        w.advance(419, &rules(), &mut events); // the rest of the night: 42 % of a day
        assert_eq!(condition(&w).cold.points(), 100, "{:?}", condition(&w));
        w.step(&rules(), &mut events);
        assert!(!w.is_night(&rules()), "dawn");
        w.advance(200, &rules(), &mut events);
        assert_eq!(condition(&w).cold.points(), 0, "daylight undoes it");
    }

    #[test]
    fn full_fatigue_forces_sleep_and_sleep_rests() {
        let mut w = fed_world();
        let mut events = Vec::new();
        go(&mut w, Pos::new(3, 3));
        // 120 per day awake: full after 5/6 of a day.
        w.advance(840, &rules(), &mut events);
        assert!(condition(&w).asleep, "{:?}", condition(&w));
        assert!(!w.body(BodyId(1)).unwrap().is_walking(), "collapsed");
        // 300 per day asleep: empty after a third of a day, then awake
        // (it is day), tiring again a little.
        w.advance(340, &rules(), &mut events);
        assert!(!condition(&w).asleep);
        assert!(condition(&w).fatigue.points() <= 1);
    }

    #[test]
    fn a_body_collapsing_on_its_way_is_told_to_stop_there() {
        let mut w = World::empty(
            Calendar::new(10, 100).unwrap(), // 1000 ticks per day
            Map::filled(40, 4, Terrain::Grass),
        );
        w.spawn_body(Pos::new(1, 1));
        let mut events = Vec::new();
        // Nearly worn out (full after 5/6 of a day), then sent far.
        w.advance(825, &rules(), &mut events);
        events.clear();
        go(&mut w, Pos::new(38, 1));
        w.advance(20, &rules(), &mut events);
        assert!(condition(&w).asleep, "{:?}", condition(&w));
        let body = w.body(BodyId(1)).unwrap();
        assert!(!body.is_walking(), "collapsed");
        assert!(body.pos().x < 38, "short of the target");
        let stop = events.iter().position(
            |e| matches!(e, Event::BodyStopped { body: BodyId(1), at } if *at == body.at()),
        );
        let walk = events.iter().position(|e| {
            matches!(
                e,
                Event::BodyWalks {
                    body: BodyId(1),
                    ..
                }
            )
        });
        assert!(walk < stop && walk.is_some(), "{events:?}");
    }

    #[test]
    fn eating_within_reach_relieves_hunger_and_removes_the_item() {
        let mut w = fed_world();
        let mut events = Vec::new();
        w.advance(500, &rules(), &mut events); // hunger 16
        let berries = w.spawn_item("berries", 100, Pos::new(2, 2)); // diagonal neighbour
        let far = w.spawn_item("berries", 100, Pos::new(3, 3));
        let stone = w.spawn_item("flint", 400, Pos::new(1, 1));
        events.clear();
        w.queue(Intention::Eat {
            body: BodyId(1),
            item: far,
        });
        w.queue(Intention::Eat {
            body: BodyId(1),
            item: stone,
        });
        w.queue(Intention::Eat {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        assert!(events.contains(&Event::ActRefused {
            body: BodyId(1),
            act: ItemAct::Eat,
            item: far,
            reason: Refusal::OutOfReach,
        }));
        assert!(events.contains(&Event::ActRefused {
            body: BodyId(1),
            act: ItemAct::Eat,
            item: stone,
            reason: Refusal::NotFood,
        }));
        assert!(events.iter().any(|e| matches!(
            e,
            Event::ItemRemoved { item, at, .. } if *item == berries && *at == Pos::new(2, 2)
        )));
        assert!(events.contains(&Event::BodyAte {
            body: BodyId(1),
            item: berries,
            material: "berries".to_owned(),
            grams: 100,
            energy_kj: 250,
        }));
        // 250 kJ at 25 kJ per point: 10 points off 16.
        assert_eq!(condition(&w).hunger.points(), 6);
        assert!(w.items().all(|i| i.id() != berries));
        assert_eq!(w.items().count(), 2);
    }

    /// A fed world whose body carries `grams` of berries, kept in a store
    /// at its tile ([`Intention::Keep`], D58), and the events of the keep.
    fn kept_berries(grams: u32) -> (World, ItemId, Vec<Event>, ground::Version) {
        let mut w = fed_world();
        let mut events = Vec::new();
        w.advance(500, &rules(), &mut events); // hunger 16
        let berries = w.spawn_item("berries", grams, Pos::new(1, 1));
        w.queue(Intention::PickUp {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        events.clear();
        let version = w.chunk_version(ChunkPos::new(0, 0));
        w.queue(Intention::Keep {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        (w, berries, events, version)
    }

    #[test]
    fn food_kept_in_a_store_leaves_the_world_and_feeds_the_body_directly() {
        let (mut w, berries, events, version) = kept_berries(100);
        assert!(events.contains(&Event::Kept {
            item: berries,
            into: berries,
            material: "berries".to_owned(),
            mass_g: 100,
            at: Pos::new(1, 1),
        }));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::ItemAdded { .. } | Event::ItemRemoved { .. })),
            "nothing lies on the ground"
        );
        // A record, not an item: no look reads it, nor the hands (D58,
        // amended 26 November 2026: a store is not picked up from).
        assert!(w.item(berries).is_none());
        assert!(!w.body(BodyId(1)).unwrap().has(berries));
        assert_eq!(w.kept(berries).map(Item::pos), Some(Some(Pos::new(1, 1))));
        assert_eq!(w.items_in(ChunkPos::new(0, 0)).len(), 0);
        assert_eq!(w.laid_in(ChunkPos::new(0, 0)).count(), 0);
        assert!(
            !w.items_near(Pos::new(1, 1), 2)
                .iter()
                .any(|i| i.id() == berries)
        );
        assert_eq!(w.chunk_version(ChunkPos::new(0, 0)), version);
        // Kept again: refused as a drop of what it does not have.
        let mut events = Vec::new();
        w.queue(Intention::Keep {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        assert!(events.contains(&Event::ActRefused {
            body: BodyId(1),
            act: ItemAct::Drop,
            item: berries,
            reason: Refusal::Unknown,
        }));
        // Eaten from the store: the body is fed, the record gone, no item
        // made, nothing on the ground told.
        events.clear();
        w.queue(Intention::Eat {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        assert!(events.contains(&Event::BodyAte {
            body: BodyId(1),
            item: berries,
            material: "berries".to_owned(),
            grams: 100,
            energy_kj: 250,
        }));
        assert_eq!(condition(&w).hunger.points(), 6);
        assert!(w.kept(berries).is_none());
        assert!(w.item(berries).is_none());
        assert!(!events.iter().any(|e| matches!(
            e,
            Event::ItemAdded { .. } | Event::ItemRemoved { .. } | Event::Kept { .. }
        )));
    }

    #[test]
    fn a_meal_from_a_store_leaves_the_rest_kept_lighter() {
        let (mut w, berries, _, _) = kept_berries(400);
        assert!(w.kept(berries).is_some());
        let mut events = Vec::new();
        w.queue(Intention::Eat {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        let eaten = events
            .iter()
            .find_map(|e| match e {
                Event::BodyAte { grams, .. } => Some(*grams),
                _ => None,
            })
            .unwrap();
        assert!(eaten > 0 && eaten < 400);
        assert!(events.contains(&Event::Kept {
            item: berries,
            into: berries,
            material: "berries".to_owned(),
            mass_g: 400 - eaten,
            at: Pos::new(1, 1),
        }));
        assert_eq!(w.kept(berries).map(Item::mass_g), Some(400 - eaten));
    }

    /// Carried by the body of [`kept_berries`] and kept at its tile:
    /// `grams` of `material`, made at `made` (now when `None`).
    fn lay(w: &mut World, material: &str, grams: u32, made: Option<Tick>) -> (ItemId, Vec<Event>) {
        let item = w.spawn_item(material, grams, Pos::new(1, 1));
        if let Some(made) = made {
            w.items.get_mut(&item).unwrap().made = made;
        }
        let mut events = Vec::new();
        w.queue(Intention::PickUp {
            body: BodyId(1),
            item,
        });
        w.step(&rules(), &mut events);
        events.clear();
        w.queue(Intention::Keep {
            body: BodyId(1),
            item,
        });
        w.step(&rules(), &mut events);
        (item, events)
    }

    #[test]
    fn food_laid_in_a_store_joins_the_heap_of_the_same_food_spoiling_the_same_day() {
        // Berries made at tick ~500 keep three days: taken away spoiled
        // as day 4 starts (tick 4000).
        let (mut w, berries, _, _) = kept_berries(100);
        let made = w.kept(berries).unwrap().made();
        // More of the same day: one heap of both, dated as the older.
        let (more, events) = lay(&mut w, "berries", 50, None);
        assert!(events.contains(&Event::Kept {
            item: more,
            into: berries,
            material: "berries".to_owned(),
            mass_g: 50,
            at: Pos::new(1, 1),
        }));
        assert!(w.kept(more).is_none() && w.item(more).is_none());
        assert_eq!(
            w.kept(berries).map(|h| (h.mass_g(), h.made())),
            Some((150, made))
        );
        // Made at the start of day 0, taken away as day 3 starts: a heap
        // of its own. A stone never spoils: never joined either.
        let (older, events) = lay(&mut w, "berries", 20, Some(Tick(0)));
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Kept { item, into, .. } if *item == older && *into == older
        )));
        let (stone, _) = lay(&mut w, "flint", 400, None);
        let (stone2, _) = lay(&mut w, "flint", 400, None);
        assert!(w.kept(stone).is_some() && w.kept(stone2).is_some());
        assert!(w.kept(stone).is_some() && w.kept(stone2).is_some());
        // Each goes as it would have apart.
        let mut events = Vec::new();
        let left = 3001 - w.tick().0;
        w.advance(left, &rules(), &mut events);
        assert!(w.kept(older).is_none() && w.kept(berries).is_some());
        // Kept, a heap spoils by the journée: good until its journée's
        // start, where loose berries made at ~500 spoil at ~3500.
        w.advance(3990 - w.tick().0, &rules(), &mut events);
        assert!(!w.is_spoiled(berries, &rules()));
        w.advance(1000, &rules(), &mut events);
        assert!(events.contains(&Event::Spoiled {
            item: berries,
            material: "berries".to_owned(),
            grams: 150,
            holder: None,
        }));
        assert!(w.kept(berries).is_none());
    }

    #[test]
    fn a_bite_from_a_heap_is_one_of_the_items_it_was_laid_as_at_most() {
        // 100 g and 50 g laid: one heap of two, a bite 75 g at most.
        let (mut w, berries, _, _) = kept_berries(100);
        lay(&mut w, "berries", 50, None);
        assert_eq!(w.part_g(berries), 75);
        let mut events = Vec::new();
        w.queue(Intention::Eat {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::BodyAte { item, grams: 75, .. } if *item == berries
        )));
        // One item eaten: the rest is one.
        assert_eq!(w.part_g(berries), 75);
        assert_eq!(w.kept_parts.get(&berries), Some(&1));
    }

    #[test]
    fn a_heap_too_heavy_for_the_arms_is_taken_by_the_handful() {
        let mut w = fed_world();
        let heap = w.spawn_kept("berries", 50_000, Pos::new(1, 1));
        let room = w.body(BodyId(1)).unwrap().food_room_g(&rules().movement);
        assert!(room > 0 && room < 50_000);
        let mut events = Vec::new();
        w.queue(Intention::PickUp {
            body: BodyId(1),
            item: heap,
        });
        w.step(&rules(), &mut events);
        // The part taken is the item, carried; the rest stays kept.
        let taken = w.item(heap).unwrap();
        assert_eq!(taken.place(), Place::Carried { body: BodyId(1) });
        assert_eq!(taken.mass_g(), room);
        let rest = events
            .iter()
            .find_map(|e| match e {
                Event::Kept {
                    item, into, mass_g, ..
                } if *item == heap => Some((*into, *mass_g)),
                _ => None,
            })
            .unwrap();
        assert_eq!(rest.1, 50_000 - room);
        assert_eq!(w.kept(rest.0).map(Item::mass_g), Some(50_000 - room));
        assert!(w.kept(heap).is_none());
        // Its arms full, nothing more: refused.
        events.clear();
        w.queue(Intention::PickUp {
            body: BodyId(1),
            item: rest.0,
        });
        w.step(&rules(), &mut events);
        assert!(events.contains(&Event::ActRefused {
            body: BodyId(1),
            act: ItemAct::PickUp,
            item: rest.0,
            reason: Refusal::TooHeavy,
        }));
    }

    #[test]
    fn food_kept_spoils_and_taken_away_is_an_item_again() {
        let (mut w, berries, _, _) = kept_berries(100);
        let made = w.kept(berries).unwrap().made();
        // Taken away: the same item again, carried, as heavy and as old.
        let mut events = Vec::new();
        w.queue(Intention::PickUp {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        assert!(w.kept(berries).is_none());
        let item = w.item(berries).unwrap();
        assert_eq!(item.place(), Place::Carried { body: BodyId(1) });
        assert_eq!((item.mass_g(), item.made()), (100, made));
        assert!(w.body(BodyId(1)).unwrap().has(berries));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::ItemRemoved { .. }))
        );
        // Kept again, it spoils in the store as anywhere (berries keep
        // three days), told with no holder and nothing on the ground.
        w.queue(Intention::Keep {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        events.clear();
        w.advance(4000, &rules(), &mut events);
        assert!(events.contains(&Event::Spoiled {
            item: berries,
            material: "berries".to_owned(),
            grams: 100,
            holder: None,
        }));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::ItemRemoved { .. }))
        );
        assert!(w.kept(berries).is_none());
        assert!(
            w.items_near(Pos::new(1, 1), 2)
                .iter()
                .all(|i| i.id() != berries)
        );
    }

    #[test]
    fn a_store_is_deterministic() {
        let run = || {
            let (mut w, berries, mut events, _) = kept_berries(400);
            for _ in 0..3 {
                w.queue(Intention::Eat {
                    body: BodyId(1),
                    item: berries,
                });
                w.step(&rules(), &mut events);
            }
            (events, w.kept(berries).cloned())
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn splitting_and_merging_keep_the_mass_and_the_older_date_wins() {
        let mut w = fed_world();
        let old = w.spawn_item("berries", 700, Pos::new(2, 2));
        w.advance(50, &rules(), &mut Vec::new());
        let young = w.spawn_item("berries", 250, Pos::new(2, 2));
        let mut heap = w.item(old).unwrap().clone();
        let fresh = w.item(young).unwrap().clone();
        assert_eq!(heap.made(), Tick(0));
        assert_eq!(fresh.made(), Tick(50));
        // A split: two parts of the same date, the mass kept.
        let part = heap.split(ItemId(99), 300).expect("a part");
        assert_eq!((heap.mass_g(), part.mass_g()), (400, 300));
        assert_eq!(part.made(), heap.made());
        assert_eq!(
            (part.material(), part.place()),
            (heap.material(), heap.place())
        );
        // No split of nothing, nor of the whole pile.
        assert!(heap.split(ItemId(100), 0).is_none());
        assert!(heap.split(ItemId(100), 400).is_none());
        assert_eq!(heap.mass_g(), 400);
        // Merged, young into old or old into young: the mass of both, and
        // the older date.
        let mut young_first = fresh.clone();
        young_first.merge(part.clone()).expect("same material");
        assert_eq!((young_first.mass_g(), young_first.made()), (550, Tick(0)));
        heap.merge(part).expect("same material");
        heap.merge(fresh).expect("same material");
        assert_eq!((heap.mass_g(), heap.made()), (950, Tick(0)));
        assert_eq!(heap.id(), old, "it keeps its id");
        // Another material is given back untouched.
        let stone = w.spawn_item("flint", 400, Pos::new(1, 1));
        let flint = w.item(stone).unwrap().clone();
        assert_eq!(heap.merge(flint.clone()), Err(flint));
        assert_eq!(heap.mass_g(), 950);
    }

    #[test]
    fn the_nights_cold_follows_the_season() {
        let mut rules = rules();
        rules.needs.cold_per_day_at_night = 300;
        rules.needs.cold_season_percent = BySeason {
            spring: 74,
            summer: 32,
            autumn: 63,
            winter: 100,
        };
        let cold = |s| night_cold(&rules.needs, s);
        assert_eq!(
            [
                Season::Spring,
                Season::Summer,
                Season::Autumn,
                Season::Winter
            ]
            .map(cold),
            [222, 96, 189, 300]
        );
        // A night in spring at a quarter of the rate: a quarter of the cold.
        let night = |percent: u32| {
            let mut rules = rules.clone();
            rules.needs.cold_per_day_at_night = 100;
            rules.needs.cold_season_percent.spring = percent;
            let mut w = fed_world();
            let mut events = Vec::new();
            let mut most = 0;
            for _ in 0..1000 {
                w.step(&rules, &mut events);
                most = most.max(condition(&w).cold.points());
            }
            most
        };
        let (full, quarter) = (night(100), night(25));
        assert!(full > 20, "{full}");
        assert!(
            quarter * 4 <= full + 4 && quarter * 4 + 4 >= full,
            "{quarter} {full}"
        );
    }

    #[test]
    fn walking_costs_energy_by_the_metre_on_top_of_the_days_need() {
        let mut rules = rules();
        rules.needs.walk_j_per_m = 130;
        let (mut walker, mut stander) = (open_field(), open_field());
        go(&mut walker, Pos::new(7, 0));
        let mut events = Vec::new();
        for _ in 0..200 {
            walker.step(&rules, &mut events);
            stander.step(&rules, &mut events);
            if walker.bodies[0].route.is_empty() {
                break;
            }
        }
        assert_eq!(walker.body(BodyId(1)).unwrap().pos(), Pos::new(7, 0));
        let lack = |w: &World| w.body(BodyId(1)).unwrap().condition().deficit_j;
        // Seven metres at 130 J a metre: 910 J more than the one standing,
        // short of a joule a tick for the rounding.
        let extra = lack(&walker) - lack(&stander);
        assert!((880..=910).contains(&extra), "{extra}");
        // A child of half the size pays half; nothing when absent.
        let mut c = Condition::healthy();
        c.size_permille = 500;
        c.walked(1000 * 1000, &rules.needs);
        assert_eq!(c.deficit_j, 65);
        rules.needs.walk_j_per_m = 0;
        c.walked(1000 * 1000, &rules.needs);
        assert_eq!(c.deficit_j, 65);
    }

    #[test]
    fn a_meal_takes_at_most_its_grams_and_leaves_the_rest() {
        let mut w = fed_world();
        let mut events = Vec::new();
        // Room to eat it all: a reserve short of 5,000 kJ, no ceiling to
        // speak of.
        let rules = || {
            let mut rules = rules();
            rules.needs.reserve_kj = 100_000;
            rules.needs.intake_ceiling_percent = 10_000;
            rules
        };
        w.advance(1000, &rules(), &mut events);
        w.bodies[0].condition.deficit_j = 5_000_000;
        let meal = rules().needs.meal_g;
        assert_eq!(meal, 300);
        // A heap on the ground: a meal off it, the rest stays, told as
        // gone and laid again, lighter.
        let heap = w.spawn_item("berries", 350, Pos::new(2, 2));
        let before = condition(&w).deficit_j;
        events.clear();
        w.queue(Intention::Eat {
            body: BodyId(1),
            item: heap,
        });
        w.step(&rules(), &mut events);
        assert_eq!(w.item(heap).map(Item::mass_g), Some(50));
        // 300 g at 2,500 kJ a kilogram: 750 kJ back in the reserve, a
        // tick's need (825 J) out.
        assert_eq!(condition(&w).deficit_j, before + 825 - 750_000);
        assert!(events.contains(&Event::BodyAte {
            body: BodyId(1),
            item: heap,
            material: "berries".to_owned(),
            grams: 300,
            energy_kj: 750,
        }));
        let removed = events
            .iter()
            .position(|e| matches!(e, Event::ItemRemoved { item, .. } if *item == heap));
        let added = events
            .iter()
            .position(|e| matches!(e, Event::ItemAdded { item, mass_g: 50, .. } if *item == heap));
        assert!(removed < added && removed.is_some(), "{events:?}");
        assert_eq!(w.items_in(ChunkPos::new(0, 0)).len(), 1);
        // The rest is less than a meal: eaten whole, gone.
        w.queue(Intention::Eat {
            body: BodyId(1),
            item: heap,
        });
        w.step(&rules(), &mut events);
        assert!(w.item(heap).is_none());
        // Carried in hand: a meal at a time, the load lighter each time,
        // the hand empty once it is all eaten.
        let carried = w.spawn_item("berries", 700, Pos::new(1, 1));
        w.queue(Intention::PickUp {
            body: BodyId(1),
            item: carried,
        });
        w.step(&rules(), &mut events);
        let mut left = Vec::new();
        for _ in 0..3 {
            events.clear();
            w.queue(Intention::Eat {
                body: BodyId(1),
                item: carried,
            });
            w.step(&rules(), &mut events);
            left.push(w.body(BodyId(1)).unwrap().load_g());
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, Event::ItemRemoved { .. } | Event::ItemAdded { .. })),
                "{events:?}"
            );
        }
        assert_eq!(left, vec![400, 100, 0]);
        assert!(w.item(carried).is_none());
        assert_eq!(w.body(BodyId(1)).unwrap().held(), None);
        assert!(events.contains(&Event::BodyHolds {
            body: BodyId(1),
            item: None,
        }));
    }

    /// The repository's needs (`content/needs.ron`) on the sample's
    /// other rules, thirst and cold off: a body's food alone.
    fn reserve_rules() -> Content {
        let mut rules = rules();
        rules.needs = ron::from_str(include_str!("../../../content/needs.ron")).unwrap();
        rules.needs.thirst_per_day = 0;
        rules.needs.cold_per_day_at_night = 0;
        rules
    }

    #[test]
    fn without_food_a_body_grows_thin_then_weak_and_dies_when_its_reserve_is_spent() {
        let rules = reserve_rules();
        let mut w = World::empty(
            Calendar::new(1, 7200).unwrap(),
            Map::filled(4, 4, Terrain::Grass),
        );
        w.spawn_body(Pos::new(1, 1));
        let day = w.calendar().ticks_per_day();
        let mut events = Vec::new();
        let at = |w: &World| w.body(BodyId(1)).map(|b| b.condition().clone());
        // A day without food: hungry, its reserve a thirtieth short (300
        // MJ since 22 November 2026, D46 amended; it was 60 MJ, six days).
        w.advance(day, &rules, &mut events);
        let c = at(&w).unwrap();
        assert_eq!(c.hunger.points(), 100);
        assert!(!c.is_thin(&rules.needs));
        // Thin after fifteen days, weak after twenty-four.
        w.advance(14 * day + 1, &rules, &mut events);
        assert!(at(&w).unwrap().is_thin(&rules.needs));
        assert!(!at(&w).unwrap().is_weak(&rules.needs));
        w.advance(9 * day, &rules, &mut events);
        let c = at(&w).unwrap();
        assert!(c.is_weak(&rules.needs));
        assert_eq!(c.pace_percent(&rules.needs), 70);
        assert_eq!(c.vitality.points(), 100, "the reserve is burnt first");
        // Spent after thirty days, dead in the thirty-first.
        w.advance(6 * day, &rules, &mut events);
        assert_eq!(at(&w).unwrap().reserve_left_j(&rules.needs), 0);
        assert!(at(&w).is_some());
        w.advance(day + 1, &rules, &mut events);
        assert!(at(&w).is_none(), "dead");
        assert!(events.iter().any(|e| matches!(
            e,
            Event::BodyDied {
                cause: Death::Starved,
                ..
            }
        )));
    }

    #[test]
    fn eating_fills_the_reserve_at_most_the_days_ceiling_and_a_sated_body_refuses() {
        // A need of 10,000 kJ a day: 30,000 a day at most (D46, 300 %).
        let rules = reserve_rules();
        let mut w = fed_world();
        let body = BodyId(1);
        // A fifth of its reserve spent: room for 60,000 kJ, more than a day's.
        w.bodies[0].condition.deficit_j = 60_000_000;
        // 20 kg of berries, 2,500 kJ a kilogram.
        let food = w.spawn_item("berries", 20_000, Pos::new(1, 1));
        let mut events = Vec::new();
        let mut eaten = 0;
        for _ in 0..50 {
            events.clear();
            w.queue(Intention::Eat { body, item: food });
            w.step(&rules, &mut events);
            eaten += events
                .iter()
                .filter_map(|e| match e {
                    Event::BodyAte { energy_kj, .. } => Some(*energy_kj),
                    _ => None,
                })
                .sum::<u32>();
        }
        // The day's ceiling: 300 % of 10,000 kJ.
        assert!((29_900..=30_000).contains(&eaten), "{eaten}");
        assert!(events.contains(&Event::ActRefused {
            body,
            act: ItemAct::Eat,
            item: food,
            reason: Refusal::Sated,
        }));
        assert_eq!(condition(&w).hunger.points(), 0, "no appetite left today");
        // The next day, room again.
        w.advance(1000, &rules, &mut events);
        assert!(condition(&w).hunger.points() > 0);
    }

    #[test]
    fn a_weak_body_walks_slower() {
        let rules = reserve_rules();
        let mut w = World::empty(
            Calendar::new(10, 7200).unwrap(),
            Map::filled(64, 4, Terrain::Grass),
        );
        let fit = w.spawn_body(Pos::new(1, 1));
        let weak = w.spawn_body(Pos::new(1, 2));
        w.bodies[1].condition.deficit_j = rules.needs.reserve_j() * 9 / 10;
        for (body, y) in [(fit, 1), (weak, 2)] {
            w.queue(Intention::MoveTo {
                body,
                target: Pos::new(60, y),
            });
        }
        let mut events = Vec::new();
        w.advance(100, &rules, &mut events);
        let x = |id| w.body(id).unwrap().at().x;
        let (fit_mm, weak_mm) = (x(fit) - 1500, x(weak) - 1500);
        assert!(fit_mm > 0);
        let percent = weak_mm * 100 / fit_mm;
        assert!((68..=72).contains(&percent), "{percent} %");
    }

    /// A grass strip with a woman of 25 and an infant of half a year she
    /// carries, under the repository's needs (lot 3 B).
    fn mother_and_infant() -> (World, Content, BodyId, BodyId) {
        let rules = reserve_rules();
        // Age-years of 50 days: an infant of half a year grows by a
        // hundredth of a year a day.
        let mut w = World::empty(
            Calendar::new(1, 7200).unwrap().with_cycle(50, 1).unwrap(),
            Map::filled(64, 8, Terrain::Grass),
        );
        let year = u64::from(w.calendar().days_per_age_year());
        let mother = w.spawn_body(Pos::new(1, 1));
        let infant = w.spawn_body(Pos::new(2, 1));
        w.set_birth(mother, 25 * year, Sex::Female);
        w.set_birth(infant, year / 2, Sex::Male);
        assert!(w.carry(mother, infant));
        (w, rules, mother, infant)
    }

    #[test]
    fn needs_go_with_age_and_sex() {
        let ages = reserve_rules().needs.ages;
        let size = |years: u64, sex| ages.size_permille(years * 1000, sex);
        assert_eq!(size(0, Sex::Male), 300, "an infant");
        assert_eq!(size(2, Sex::Male), 400, "a child of two");
        assert_eq!(size(8, Sex::Male), 650, "halfway to fourteen");
        assert_eq!(size(14, Sex::Male), 900);
        assert_eq!(size(30, Sex::Male), 1000, "a man");
        assert_eq!(size(30, Sex::Female), 850, "a woman");
        assert_eq!(size(60, Sex::Male), 850, "the old");
        assert_eq!(size(8, Sex::Female), 650, "a girl as a boy");
        assert_eq!(ages.age_pace_percent(8_000), 70);
        assert_eq!(ages.age_pace_percent(30_000), 100);
        assert_eq!(ages.age_pace_percent(60_000), 70);
        assert_eq!(ages.infant_mass_g(0), 3500);
        assert_eq!(ages.infant_mass_g(2000), 12000);
    }

    #[test]
    fn a_woman_holds_more_fat_than_a_man_though_she_burns_less() {
        let ages = reserve_rules().needs.ages;
        let reserve = |years: u64, sex| ages.reserve_permille(years * 1000, sex);
        assert_eq!(reserve(30, Sex::Male), 1000, "a man");
        assert_eq!(reserve(30, Sex::Female), 1400, "a woman");
        assert_eq!(reserve(8, Sex::Female), 650, "a girl as a boy");
        assert_eq!(ages.size_permille(30_000, Sex::Female), 850, "her need");
        let (mut w, rules, mother, _) = mother_and_infant();
        w.advance(1, &rules, &mut Vec::new());
        let mum = w.body(mother).unwrap().condition();
        assert_eq!(
            mum.reserve_j(&rules.needs),
            rules.needs.reserve_j() * 14 / 10
        );
        assert_eq!(mum.size_permille, 850);
    }

    #[test]
    fn a_nursing_woman_burns_for_two_and_her_infant_burns_nothing() {
        let (mut w, rules, mother, infant) = mother_and_infant();
        let day = w.calendar().ticks_per_day();
        let mut events = Vec::new();
        w.advance(day, &rules, &mut events);
        let need = rules.needs.need_j_per_day();
        let mum = w.body(mother).unwrap().condition();
        // 85 % of a man's need, and a quarter more for the milk.
        let expected = need * (850 * 125 / 100) / 1000;
        assert!(
            mum.deficit_j.abs_diff(expected) <= 2,
            "{} against {expected}",
            mum.deficit_j
        );
        let baby = w.body(infant).unwrap().condition();
        assert_eq!(baby.deficit_j, 0, "fed");
        assert!(baby.thirst.is_empty(), "and watered");
        assert_eq!(
            baby.size_permille, 326,
            "half a year and a day: 30 to 40 %, a quarter of the way and more"
        );
    }

    #[test]
    fn carrying_an_infant_slows_its_carrier() {
        let (mut w, rules, mother, infant) = mother_and_infant();
        let mut events = Vec::new();
        w.advance(1, &rules, &mut events);
        let pace = |w: &World| {
            w.body(mother)
                .unwrap()
                .condition()
                .pace_percent(&rules.needs)
        };
        assert_eq!(rules.needs.ages.carrying_pace_percent, 75);
        assert_eq!(pace(&w), 75, "a woman of 25 carrying");
        // She walks a third slower than alone over the same ground.
        let walked = |w: &mut World| {
            let from = w.body(mother).unwrap().at();
            w.queue(Intention::MoveTo {
                body: mother,
                target: Pos::new(60, 1),
            });
            let mut events = Vec::new();
            w.advance(20, &rules, &mut events);
            w.body(mother).unwrap().at().x - from.x
        };
        let carrying = walked(&mut w);
        let (mut alone, _, lone, _) = mother_and_infant();
        alone.bodies.retain(|b| b.id() != infant);
        alone.advance(1, &rules, &mut events);
        assert_eq!(
            alone
                .body(lone)
                .unwrap()
                .condition()
                .pace_percent(&rules.needs),
            100
        );
        let free = {
            let from = alone.body(lone).unwrap().at();
            alone.queue(Intention::MoveTo {
                body: lone,
                target: Pos::new(60, 1),
            });
            alone.advance(20, &rules, &mut events);
            alone.body(lone).unwrap().at().x - from.x
        };
        // 75 % of the pace, and the infant's weight on top (D45's load).
        assert!(
            carrying * 100 <= free * 75 && carrying * 100 >= free * 60,
            "{carrying} of {free}"
        );
    }

    #[test]
    fn an_infant_goes_where_its_carrier_goes_and_weighs_on_her() {
        let (mut w, rules, mother, infant) = mother_and_infant();
        let mut events = Vec::new();
        w.advance(1, &rules, &mut events);
        let capacity = w.body(mother).unwrap().capacity_g(&rules.movement);
        let alone = rules.movement.carry_capacity_g * 850 / 1000;
        assert!(capacity < alone - 5000, "{capacity} g left of {alone}");
        w.queue(Intention::MoveTo {
            body: mother,
            target: Pos::new(40, 1),
        });
        w.queue(Intention::MoveTo {
            body: infant,
            target: Pos::new(1, 6),
        });
        events.clear();
        w.advance(200, &rules, &mut events);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::MoveRefused { body, .. } if *body == infant)),
            "an infant does not walk"
        );
        let (m, i) = (w.body(mother).unwrap().at(), w.body(infant).unwrap().at());
        assert!(m.x > 10_000, "she walked");
        assert_eq!(i, m, "carried along");
    }

    #[test]
    fn an_orphan_is_taken_by_the_nearest_woman_or_left_to_starve() {
        let (mut w, rules, mother, infant) = mother_and_infant();
        let year = u64::from(w.calendar().days_per_age_year());
        let aunt = w.spawn_body(Pos::new(30, 1));
        w.set_birth(aunt, 30 * year, Sex::Female);
        let man = w.spawn_body(Pos::new(3, 1));
        w.set_birth(man, 30 * year, Sex::Male);
        let mut events = Vec::new();
        w.die(mother, Death::Starved, &rules, &mut events);
        assert_eq!(
            w.body(infant).unwrap().carried_by(),
            Some(aunt),
            "a woman, not the nearer man"
        );
        w.die(aunt, Death::Starved, &rules, &mut events);
        assert_eq!(w.body(infant).unwrap().carried_by(), None, "nobody left");
        w.advance(w.calendar().ticks_per_day(), &rules, &mut events);
        assert!(w.body(infant).unwrap().condition().deficit_j > 0, "no milk");
    }

    #[test]
    fn a_child_is_as_hungry_as_a_man_missing_as_much_of_his_day() {
        let rules = reserve_rules();
        let mut w = World::empty(
            Calendar::new(1, 7200).unwrap(),
            Map::filled(16, 4, Terrain::Grass),
        );
        let year = u64::from(w.calendar().days_per_age_year());
        let man = w.spawn_body(Pos::new(1, 1));
        let boy = w.spawn_body(Pos::new(2, 1));
        w.set_birth(boy, 2 * year + 1, Sex::Male);
        let mut events = Vec::new();
        w.advance(w.calendar().ticks_per_day() / 2, &rules, &mut events);
        let hunger = |id| w.body(id).unwrap().condition().hunger.points();
        assert_eq!(hunger(man), 50, "half a day missed");
        assert_eq!(hunger(boy), hunger(man), "the same share of his own");
    }

    #[test]
    fn an_infant_is_weaned_at_two_and_walks() {
        let (mut w, rules, mother, infant) = mother_and_infant();
        let year = u64::from(w.calendar().days_per_age_year());
        w.set_birth(infant, 2 * year - 1, Sex::Male);
        let mut events = Vec::new();
        w.advance(w.calendar().ticks_per_day() + 1, &rules, &mut events);
        let child = w.body(infant).unwrap();
        assert_eq!(child.carried_by(), None);
        assert!(!w.is_infant(child, &rules));
        assert_eq!(
            child.condition().age_pace_percent,
            70,
            "a child goes slower"
        );
        let mum = w.body(mother).unwrap();
        assert_eq!(mum.condition().burn_permille, 850, "no more milk to make");
        w.queue(Intention::MoveTo {
            body: infant,
            target: Pos::new(1, 6),
        });
        w.advance(200, &rules, &mut events);
        assert_ne!(w.body(infant).unwrap().at(), w.body(mother).unwrap().at());
    }

    /// The sample rules with a body that never thirsts: for the tests of
    /// another need, which a thirst death would cut short.
    fn without_thirst() -> Content {
        let mut rules = rules();
        rules.needs.thirst_per_day = 0;
        rules
    }

    #[test]
    fn thirst_fills_in_a_day_and_kills_two_days_later() {
        let mut w = fed_world();
        let mut rules = rules();
        // Thirst alone: no hunger, no cold at night.
        rules.needs.hunger_per_day = 0;
        rules.needs.cold_per_day_at_night = 0;
        let mut events = Vec::new();
        w.advance(500, &rules, &mut events);
        assert_eq!(condition(&w).thirst.points(), 50, "half a day, half full");
        w.advance(500, &rules, &mut events);
        assert!(
            condition(&w).thirst.is_full(),
            "a day without water fills it"
        );
        assert_eq!(condition(&w).vitality.points(), 99, "it starts to hurt");
        w.advance(1_000, &rules, &mut events);
        assert_eq!(condition(&w).vitality.points(), 49, "a day at full thirst");
        w.advance(990, &rules, &mut events);
        assert!(w.body(BodyId(1)).is_some(), "alive on the third day");
        events.clear();
        w.advance(20, &rules, &mut events);
        assert!(
            w.body(BodyId(1)).is_none(),
            "dead before the third day ends"
        );
        assert!(events.iter().any(|e| matches!(
            e,
            Event::BodyDied {
                body: BodyId(1),
                cause: Death::Dehydrated,
                ..
            }
        )));
    }

    #[test]
    fn drinking_at_running_water_quenches_thirst_and_elsewhere_is_refused() {
        // A river east of body 1, a ford south, a brook beyond its reach,
        // and body 2 standing in that brook.
        let mut map = Map::filled(8, 8, Terrain::Grass);
        map.set(Pos::new(2, 1), Terrain::Water);
        map.set(Pos::new(1, 2), Terrain::Ford);
        map.set(Pos::new(4, 4), Terrain::Brook);
        let mut w = World::empty(Calendar::new(10, 100).unwrap(), map); // 1000 ticks per day
        w.spawn_body(Pos::new(1, 1));
        w.spawn_body(Pos::new(4, 4));
        let mut events = Vec::new();
        w.advance(600, &rules(), &mut events); // thirst 60
        assert_eq!(condition(&w).thirst.points(), 60);
        let drink = |w: &mut World, at: Pos, events: &mut Vec<Event>| {
            events.clear();
            w.queue(Intention::Drink {
                body: BodyId(1),
                at,
            });
            w.step(&rules(), events);
        };
        // At the river beside it: thirst gone, told to the pilot at once.
        drink(&mut w, Pos::new(2, 1), &mut events);
        assert_eq!(condition(&w).thirst.points(), 0);
        assert!(events.contains(&Event::BodyDrank {
            body: BodyId(1),
            at: Pos::new(2, 1),
        }));
        assert!(events.iter().any(|e| matches!(
            e,
            Event::ConditionChanged { body: BodyId(1), summary } if summary.2 == 0
        )));
        // At a ford, and asleep: it wakes to drink.
        w.advance(300, &rules(), &mut events);
        w.queue(Intention::Sleep { body: BodyId(1) });
        w.step(&rules(), &mut events);
        drink(&mut w, Pos::new(1, 2), &mut events);
        assert_eq!(condition(&w).thirst.points(), 0);
        assert!(!condition(&w).asleep);
        // A brook is running water too, but this one is out of reach; the
        // grass is no water; another body is unknown.
        w.advance(300, &rules(), &mut events);
        for (body, at, reason) in [
            (BodyId(1), Pos::new(4, 4), Refusal::OutOfReach),
            (BodyId(1), Pos::new(0, 0), Refusal::NotWater),
            (BodyId(1), Pos::new(1, 1), Refusal::NotWater),
            (BodyId(9), Pos::new(2, 1), Refusal::Unknown),
        ] {
            events.clear();
            w.queue(Intention::Drink { body, at });
            w.step(&rules(), &mut events);
            assert!(
                events.contains(&Event::DrinkRefused { body, at, reason }),
                "{at:?}: {reason:?}"
            );
            assert_eq!(condition(&w).thirst.points(), 30, "still thirsty");
        }
        // Standing in a brook, one drinks where one stands.
        events.clear();
        w.queue(Intention::Drink {
            body: BodyId(2),
            at: Pos::new(4, 4),
        });
        w.step(&rules(), &mut events);
        let second = w.body(BodyId(2)).unwrap().condition();
        assert_eq!(second.thirst.points(), 0);
    }

    #[test]
    fn summer_days_are_longer_than_winter_days() {
        // 1000 ticks a day, 8 days a year: two days a season.
        let calendar = Calendar::new(10, 100).unwrap().with_cycle(8, 1).unwrap();
        let mut w = World::empty(calendar, Map::filled(4, 4, Terrain::Grass));
        let mut events = Vec::new();
        let mut nightfall = BTreeMap::new();
        let mut seasons = Vec::new();
        for _ in 0..8 * 1000 {
            events.clear();
            w.step(&rules(), &mut events);
            for e in &events {
                match e {
                    Event::NightFell { .. } => {
                        nightfall.insert(w.time().season, w.time().tick_of_day);
                    }
                    Event::SeasonStarted { year, season } => seasons.push((*year, *season)),
                    _ => {}
                }
            }
        }
        let needs = &rules().needs.night_from_percent;
        for season in Season::ALL {
            assert_eq!(
                nightfall[&season],
                u64::from(needs.get(season)) * 10,
                "{season:?}"
            );
        }
        assert!(nightfall[&Season::Summer] > nightfall[&Season::Spring]);
        assert!(nightfall[&Season::Winter] < nightfall[&Season::Autumn]);
        // Each season told as it starts at dawn, and the next year's
        // spring; the world's first spring began with it, untold.
        assert_eq!(
            seasons,
            [
                (1, Season::Summer),
                (1, Season::Autumn),
                (1, Season::Winter),
                (2, Season::Spring)
            ]
        );
    }

    #[test]
    fn starving_drains_vitality_and_kills_leaving_a_corpse() {
        let mut w = fed_world();
        let mut events = Vec::new();
        let rules = without_thirst;
        // Four days to spend the reserve (3,300 kJ at 825 a day), then a
        // day on nothing to die.
        w.advance(4_100, &rules(), &mut events);
        assert_eq!(condition(&w).deficit_j, rules().needs.reserve_j(), "spent");
        assert_eq!(condition(&w).hunger.points(), 100);
        // Four nights outside have cost vitality, but not life.
        assert!(condition(&w).vitality.points() > 0);
        events.clear();
        w.advance(1_000, &rules(), &mut events);
        assert!(w.body(BodyId(1)).is_none(), "dead and gone");
        let died = events.iter().find_map(|e| match e {
            Event::BodyDied {
                body,
                cause,
                at,
                corpse,
            } => Some((*body, *cause, *at, *corpse)),
            _ => None,
        });
        let (body, cause, at, corpse) = died.expect("a death event");
        assert_eq!(
            (body, cause, at),
            (BodyId(1), Death::Starved, Pos::new(1, 1))
        );
        assert!(
            w.items()
                .any(|i| i.id() == corpse && i.material() == "corpse")
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ItemAdded { item, .. } if *item == corpse))
        );
        // Nothing more happens to a dead body.
        events.clear();
        w.advance(10, &rules(), &mut events);
        assert_eq!(events, Vec::new());
    }

    #[test]
    fn sleeping_stops_a_walk_and_any_act_wakes() {
        let mut w = fed_world();
        let mut events = Vec::new();
        w.advance(100, &rules(), &mut events); // a little tired: sleep holds
        go(&mut w, Pos::new(3, 3));
        w.step(&rules(), &mut events);
        w.queue(Intention::Sleep { body: BodyId(1) });
        w.step(&rules(), &mut events);
        assert!(condition(&w).asleep);
        assert!(!w.body(BodyId(1)).unwrap().is_walking());
        assert!(events.iter().any(|e| matches!(
            e,
            Event::BodyStopped {
                body: BodyId(1),
                ..
            }
        )));
        go(&mut w, Pos::new(0, 0));
        w.step(&rules(), &mut events);
        assert!(!condition(&w).asleep, "walking wakes");
        assert!(w.body(BodyId(1)).unwrap().is_walking());
    }

    fn pick(w: &mut World, item: ItemId, events: &mut Vec<Event>) {
        w.queue(Intention::PickUp {
            body: BodyId(1),
            item,
        });
        w.step(&rules(), events);
    }

    #[test]
    fn the_chunk_version_bumps_on_drop_as_on_take() {
        // D4: the one version a chunk frame carries moves with anything
        // the frame shows, an item dropped as an item taken, and each
        // event tells the version right after it.
        let mut w = fed_world();
        let mut events = Vec::new();
        let flint = w.spawn_item("flint", 400, Pos::new(2, 2));
        let chunk = Pos::new(2, 2).chunk();
        let laid = w.chunk_version(chunk);
        assert!(laid > 0, "an item laid is a change");
        pick(&mut w, flint, &mut events);
        let taken = w.chunk_version(chunk);
        assert!(taken > laid, "taking bumps the version");
        assert!(events.contains(&Event::ItemRemoved {
            item: flint,
            at: Pos::new(2, 2),
            version: taken,
        }));
        events.clear();
        w.queue(Intention::Drop {
            body: BodyId(1),
            item: flint,
        });
        w.step(&rules(), &mut events);
        let at = w.item(flint).unwrap().pos().unwrap();
        let dropped = w.chunk_version(at.chunk());
        assert!(
            events.contains(&Event::ItemAdded {
                item: flint,
                material: "flint".to_owned(),
                mass_g: 400,
                at,
                version: dropped,
            }),
            "{events:?}"
        );
        assert_eq!(at.chunk(), chunk, "a map of one chunk");
        assert!(dropped > taken, "dropping bumps the version");
    }

    #[test]
    fn picking_up_carries_an_item_and_puts_the_first_in_hand() {
        let mut w = fed_world();
        let mut events = Vec::new();
        let flint = w.spawn_item("flint", 400, Pos::new(2, 2)); // within reach
        let branch = w.spawn_item("branch", 1500, Pos::new(1, 2));
        let far = w.spawn_item("branch", 1500, Pos::new(3, 3));
        pick(&mut w, flint, &mut events);
        let body = w.body(BodyId(1)).unwrap();
        assert_eq!(body.held(), Some(flint), "the first item goes in hand");
        assert_eq!(
            w.item(flint).unwrap().place(),
            Place::Carried { body: BodyId(1) }
        );
        assert!(events.iter().any(|e| matches!(
            e,
            Event::ItemRemoved { item, at, .. } if *item == flint && *at == Pos::new(2, 2)
        )));
        assert!(events.contains(&Event::BodyHolds {
            body: BodyId(1),
            item: Some(flint)
        }));
        assert!(events.iter().any(|e| matches!(
            e,
            Event::InventoryChanged { body: BodyId(1), held: Some(f), load_g: 400, capacity_g: 20000, .. } if *f == flint
        )));
        assert_eq!(
            w.items_within_reach(Pos::new(1, 1)).len(),
            1,
            "only the branch is left on the ground nearby"
        );

        events.clear();
        pick(&mut w, branch, &mut events);
        pick(&mut w, far, &mut events);
        let body = w.body(BodyId(1)).unwrap();
        assert_eq!(body.carried(), &[branch], "the second item is carried");
        assert!(events.contains(&Event::ActRefused {
            body: BodyId(1),
            act: ItemAct::PickUp,
            item: far,
            reason: Refusal::OutOfReach,
        }));
        assert_eq!(w.load_g(body), 1900);
    }

    #[test]
    fn the_timeline_of_a_walk_is_where_the_world_has_the_body_tick_by_tick() {
        // Grass, then forest (slower), then grass, on a walk with a turn.
        let mut map = Map::filled(16, 8, Terrain::Grass);
        for x in 5..9 {
            for y in 0..8 {
                map.set(Pos::new(x, y), Terrain::Forest);
            }
        }
        let mut w = World::empty(Calendar::new(10, 100).unwrap(), map);
        w.spawn_body(Pos::new(0, 0));
        let rules = rules();
        let mut events = Vec::new();
        go(&mut w, Pos::new(12, 4));
        w.step(&rules, &mut events);
        let timeline = w.walk_timeline(BodyId(1), &rules);
        let (start, now) = (w.body(BodyId(1)).unwrap().at(), w.tick().0);
        assert!(timeline.len() >= 3, "the pace changes twice: {timeline:?}");
        assert!(
            timeline.windows(2).all(|p| p[0].1 < p[1].1),
            "in time order"
        );
        assert_eq!(timeline.last().unwrap().0, Pos::new(12, 4).centre());
        // Moved by time between the points, the body is where the world
        // has it at every tick, within a tick's walk.
        let shown = |tick: u64| {
            let (mut from, mut from_tick) = (start, now);
            for &(to, at) in &timeline {
                if tick < at {
                    #[allow(
                        clippy::cast_precision_loss,
                        reason = "small counts and coordinates, far below the float precision limit"
                    )]
                    let f = (tick - from_tick) as f64 / (at - from_tick) as f64;
                    return (
                        f64::from(from.x) + f64::from(to.x - from.x) * f,
                        f64::from(from.y) + f64::from(to.y - from.y) * f,
                    );
                }
                (from, from_tick) = (to, at);
            }
            (f64::from(from.x), f64::from(from.y))
        };
        loop {
            let tick = w.tick().0;
            let at = w.body(BodyId(1)).unwrap().at();
            let (x, y) = shown(tick);
            let (dx, dy) = (x - f64::from(at.x), y - f64::from(at.y));
            assert!(
                dx * dx + dy * dy <= 150.0 * 150.0,
                "({dx}, {dy}) mm off at tick {tick}"
            );
            if !w.body(BodyId(1)).unwrap().is_walking() {
                assert_eq!(tick, timeline.last().unwrap().1, "it stops on time");
                break;
            }
            w.step(&rules, &mut events);
        }
    }

    #[test]
    fn too_heavy_a_load_is_refused_and_a_load_slows_the_walk() {
        let mut w = fed_world();
        let mut events = Vec::new();
        // Thirteen branches of 1.5 kg fit in 20 kg; the fourteenth does not.
        let branches: Vec<ItemId> = (0..15)
            .map(|_| w.spawn_item("branch", 1500, Pos::new(1, 1)))
            .collect();
        for item in &branches {
            pick(&mut w, *item, &mut events);
        }
        let body = w.body(BodyId(1)).unwrap();
        assert_eq!(
            body.inventory().count(),
            13,
            "19.5 kg: one more would make 21"
        );
        assert!(events.iter().any(|e| matches!(
            e,
            Event::ActRefused {
                act: ItemAct::PickUp,
                reason: Refusal::TooHeavy,
                ..
            }
        )));
        // At 19.5 of 20 kg the walk is nearly at 60 %: two tiles take 23 ticks, not 14.
        go(&mut w, Pos::new(3, 1));
        events.clear();
        let took = until_stopped(&mut w, &mut events);
        assert!((22..=24).contains(&took), "{took} ticks");
    }

    #[test]
    fn holding_stowing_dropping_and_eating_what_is_carried() {
        let mut w = fed_world();
        let mut events = Vec::new();
        w.advance(500, &rules(), &mut events); // hungry enough to notice eating
        let flint = w.spawn_item("flint", 400, Pos::new(1, 1));
        let berries = w.spawn_item("berries", 100, Pos::new(1, 1));
        pick(&mut w, flint, &mut events);
        pick(&mut w, berries, &mut events);
        let body = w.body(BodyId(1)).unwrap();
        assert_eq!((body.held(), body.carried()), (Some(flint), &[berries][..]));

        // Hold the berries: the flint goes among the carried.
        events.clear();
        w.queue(Intention::Hold {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        let body = w.body(BodyId(1)).unwrap();
        assert_eq!((body.held(), body.carried()), (Some(berries), &[flint][..]));
        assert!(events.contains(&Event::BodyHolds {
            body: BodyId(1),
            item: Some(berries)
        }));

        // Stow them: nothing in hand.
        w.queue(Intention::Stow {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        let body = w.body(BodyId(1)).unwrap();
        assert_eq!((body.held(), body.carried()), (None, &[flint, berries][..]));

        // Eat the carried berries: gone, hunger relieved, nothing on the ground.
        let hunger_before = condition(&w).hunger.points();
        events.clear();
        w.queue(Intention::Eat {
            body: BodyId(1),
            item: berries,
        });
        w.step(&rules(), &mut events);
        assert!(w.item(berries).is_none());
        assert_eq!(condition(&w).hunger.points(), hunger_before - 10);
        assert!(events.contains(&Event::BodyAte {
            body: BodyId(1),
            item: berries,
            material: "berries".to_owned(),
            grams: 100,
            energy_kj: 250,
        }));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::ItemRemoved { .. })),
            "it was not on the ground"
        );

        // Drop the flint: on the body's tile, out of the inventory.
        events.clear();
        w.queue(Intention::Drop {
            body: BodyId(1),
            item: flint,
        });
        w.step(&rules(), &mut events);
        assert_eq!(
            w.item(flint).unwrap().place(),
            Place::Ground {
                pos: Pos::new(1, 1)
            }
        );
        assert_eq!(w.body(BodyId(1)).unwrap().inventory().count(), 0);
        assert!(events.iter().any(
            |e| matches!(e, Event::ItemAdded { item, at, .. } if *item == flint && *at == Pos::new(1, 1))
        ));
        // Dropping it again, or stowing what is not in hand, is refused.
        events.clear();
        w.queue(Intention::Drop {
            body: BodyId(1),
            item: flint,
        });
        w.queue(Intention::Stow {
            body: BodyId(1),
            item: flint,
        });
        w.queue(Intention::Hold {
            body: BodyId(1),
            item: ItemId(999),
        });
        w.step(&rules(), &mut events);
        for (act, item, reason) in [
            (ItemAct::Drop, flint, Refusal::NotCarried),
            (ItemAct::Stow, flint, Refusal::NotCarried),
            (ItemAct::Hold, ItemId(999), Refusal::Unknown),
        ] {
            assert!(events.contains(&Event::ActRefused {
                body: BodyId(1),
                act,
                item,
                reason
            }));
        }
    }

    #[test]
    fn a_dead_body_drops_what_it_carried() {
        let mut w = fed_world();
        let mut events = Vec::new();
        let flint = w.spawn_item("flint", 400, Pos::new(1, 1));
        pick(&mut w, flint, &mut events);
        w.advance(4_200, &rules(), &mut events); // starves
        assert!(w.body(BodyId(1)).is_none());
        assert_eq!(
            w.item(flint).unwrap().place(),
            Place::Ground {
                pos: Pos::new(1, 1)
            }
        );
    }

    #[test]
    fn bodies_are_found_by_id_and_by_chunk() {
        // Bodies across chunks, found by id and by the chunk they stand
        // in; a walk over a chunk's edge, a teleport and a death move
        // them; a world read back knows the same.
        let map = Map::filled(96, 96, Terrain::Grass);
        let mut w = World::empty(Calendar::new(1, 1_000_000).unwrap(), map);
        let a = w.spawn_body(Pos::new(5, 5));
        let b = w.spawn_body(Pos::new(40, 5));
        let c = w.spawn_body(Pos::new(41, 6));
        assert_eq!(w.body(b).map(Body::id), Some(b));
        assert_eq!(w.body(BodyId(9)), None);
        let ids = |w: &World, x, y| -> Vec<BodyId> {
            w.bodies_in(ChunkPos::new(x, y))
                .iter()
                .map(|b| b.id)
                .collect()
        };
        assert_eq!(ids(&w, 0, 0), vec![a]);
        assert_eq!(ids(&w, 1, 0), vec![b, c]);
        assert_eq!(ids(&w, 2, 0), vec![]);
        let mut events = Vec::new();
        w.queue(Intention::MoveTo {
            body: a,
            target: Pos::new(34, 5),
        });
        for _ in 0..200 {
            w.step(&rules(), &mut events);
            if !w.body(a).unwrap().is_walking() {
                break;
            }
        }
        assert_eq!(w.body(a).unwrap().pos(), Pos::new(34, 5));
        assert_eq!(ids(&w, 0, 0), vec![]);
        assert_eq!(ids(&w, 1, 0), vec![a, b, c]);
        w.queue(Intention::Teleport {
            body: c,
            target: Pos::new(70, 70),
        });
        w.step(&rules(), &mut events);
        assert_eq!(ids(&w, 2, 2), vec![c]);
        assert_eq!(ids(&w, 1, 0), vec![a, b]);
        w.die(b, Death::Starved, &rules(), &mut events);
        assert_eq!(w.body(b), None);
        assert_eq!(ids(&w, 1, 0), vec![a]);
        assert_eq!(w.body(c).map(Body::id), Some(c));
        let back: World = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
        assert_eq!(ids(&back, 1, 0), vec![a]);
        assert_eq!(ids(&back, 2, 2), vec![c]);
        assert_eq!(back.body(c), w.body(c));
        assert_eq!(back.bodies(), w.bodies());
    }

    #[test]
    fn pending_intentions_are_not_serialized_but_routes_are() {
        let mut w = open_field();
        go(&mut w, Pos::new(3, 3));
        w.step(&rules(), &mut Vec::new());
        w.queue(Intention::MoveTo {
            body: BodyId(1),
            target: Pos::new(5, 5),
        });
        let text = serde_json::to_string(&w).unwrap();
        let back: World = serde_json::from_str(&text).unwrap();
        assert_eq!(back.pending, Vec::new());
        assert_eq!(back.bodies(), w.bodies());
        assert!(back.body(BodyId(1)).unwrap().is_walking());
        assert_eq!(back.map(), w.map());
        assert_eq!(
            back.items().collect::<Vec<_>>(),
            w.items().collect::<Vec<_>>()
        );
    }

    #[test]
    fn items_are_found_by_id_and_by_chunk_and_the_load_is_kept() {
        // Stored items found by id and by the chunk they lie in; taking,
        // stowing, holding, dropping and eating keep the chunks and the
        // body's weighed load right; a world read back knows the chunks,
        // and the content gives it the loads again.
        let rules = rules();
        let map = Map::filled(96, 96, Terrain::Grass);
        let mut w = World::empty(Calendar::new(1, 1_000_000).unwrap(), map);
        let body = w.spawn_body(Pos::new(5, 5));
        let flint = w.spawn_item("flint", 400, Pos::new(5, 6));
        let branch = w.spawn_item("branch", 1500, Pos::new(6, 5));
        let far = w.spawn_item("flint", 400, Pos::new(40, 40));
        let ids = |w: &World, x, y| -> Vec<ItemId> {
            w.items_in(ChunkPos::new(x, y))
                .iter()
                .map(Item::id)
                .collect()
        };
        let load = |w: &World| w.body(body).expect("body").load_g();
        let branch_g = rules.material("branch").expect("branch").mass_g();
        assert_eq!(w.item(far).map(Item::id), Some(far));
        assert_eq!(w.item(ItemId(99)), None);
        assert_eq!(ids(&w, 0, 0), vec![flint, branch]);
        assert_eq!(ids(&w, 1, 1), vec![far]);
        assert_eq!(ids(&w, 2, 2), vec![]);
        assert_eq!(load(&w), 0);
        let mut events = Vec::new();
        let mut act = |w: &mut World, intention: Intention| {
            w.queue(intention);
            w.step(&rules, &mut events);
        };
        act(&mut w, Intention::PickUp { body, item: flint });
        assert_eq!(ids(&w, 0, 0), vec![branch]);
        assert_eq!(load(&w), 400);
        act(&mut w, Intention::PickUp { body, item: branch });
        assert_eq!(ids(&w, 0, 0), vec![]);
        assert_eq!(load(&w), 400 + branch_g);
        act(&mut w, Intention::Stow { body, item: flint });
        assert_eq!(load(&w), 400 + branch_g);
        act(&mut w, Intention::Hold { body, item: branch });
        assert_eq!(load(&w), 400 + branch_g);
        act(&mut w, Intention::Drop { body, item: flint });
        assert_eq!(ids(&w, 0, 0), vec![flint]);
        assert_eq!(load(&w), branch_g);
        let berries = w.spawn_item("berries", 100, Pos::new(5, 5));
        act(
            &mut w,
            Intention::PickUp {
                body,
                item: berries,
            },
        );
        assert_eq!(load(&w), branch_g + 100);
        // Hungry enough to eat them.
        let index = w.index_of(body).unwrap();
        w.bodies[index].condition.deficit_j = 1_000_000;
        act(
            &mut w,
            Intention::Eat {
                body,
                item: berries,
            },
        );
        assert_eq!(load(&w), branch_g);
        assert_eq!(w.item(berries), None);
        assert_eq!(ids(&w, 0, 0), vec![flint]);
        let mut back: World = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
        assert_eq!(ids(&back, 0, 0), vec![flint]);
        assert_eq!(ids(&back, 1, 1), vec![far]);
        assert_eq!(load(&back), 0, "derived, not stored");
        back.take_content(&rules);
        assert_eq!(load(&back), branch_g);
        assert_eq!(back.item(branch), w.item(branch));
        w.die(body, Death::Starved, &rules, &mut events);
        assert_eq!(ids(&w, 0, 0).len(), 3, "the flint, the branch, the corpse");
        assert!(
            w.items_in(ChunkPos::new(0, 0))
                .iter()
                .all(|i| i.pos().is_some())
        );
    }
    /// The sample rules with the repository's kinds of tree, and its
    /// materials, which hold the birch's bark (D57).
    fn wooded_rules() -> Content {
        Content::from_ron(
            include_str!("../../../content/materials.ron"),
            content::SAMPLE_SPAWNS,
            content::SAMPLE_MOVEMENT,
            content::SAMPLE_NEEDS,
        )
        .expect("repository materials parse")
        .with_species(include_str!("../../../content/species.ron"))
        .expect("species.ron parses")
    }

    /// The small valley with its trees, body 1 born at the camp.
    fn wooded_world() -> World {
        let mut world =
            World::with_valley(Calendar::new(10, 1_000_000).unwrap(), &wooded_rules(), 768);
        world.spawn_at_camp().expect("grass by the camp");
        world
    }

    /// Does any segment of `route`, from `from`, enter one of `tiles`?
    fn route_enters(from: FinePos, route: &[FinePos], tiles: &[Pos]) -> bool {
        let mut at = from;
        route.iter().any(|&next| {
            let hit = tiles.iter().any(|&t| map::enters(at, next, t));
            at = next;
            hit
        })
    }

    /// How deep `at` stands inside `tile`, in millimetres from its
    /// nearest edge; 0 outside it.
    fn depth_in(at: FinePos, tile: Pos) -> i32 {
        if at.tile() != tile {
            return 0;
        }
        let (x0, y0) = (tile.x * MM_PER_TILE, tile.y * MM_PER_TILE);
        (at.x - x0)
            .min(x0 + MM_PER_TILE - at.x)
            .min(at.y - y0)
            .min(y0 + MM_PER_TILE - at.y)
    }

    /// The chunks of a world's map, row by row.
    fn every_chunk(w: &World) -> Vec<ChunkPos> {
        let wide = i32::try_from(w.map().chunks_wide()).expect("small");
        let high = i32::try_from(w.map().chunks_high()).expect("small");
        (0..high)
            .flat_map(|y| (0..wide).map(move |x| ChunkPos::new(x, y)))
            .collect()
    }

    /// The tiles of `chunk` and of the chunks around it.
    fn tiles_around(chunk: ChunkPos) -> Vec<Pos> {
        let origin = chunk.origin();
        let side = i32::try_from(CHUNK_SIZE).expect("small");
        (origin.y - side..origin.y + 2 * side)
            .flat_map(|y| (origin.x - side..origin.x + 2 * side).map(move |x| Pos::new(x, y)))
            .collect()
    }

    /// The chunk of `w` with the most trunks.
    fn densest_woods(w: &World) -> ChunkPos {
        every_chunk(w)
            .into_iter()
            .max_by_key(|&c| w.trees_in(c).len())
            .expect("chunks")
    }

    #[test]
    fn pulled_routes_never_cross_a_trunk_in_the_densest_woods() {
        let w = wooded_world();
        let rules = wooded_rules();
        let cost = |t: Terrain| rules.movement.cost(t);
        let chunk = densest_woods(&w);
        let trunks: Vec<Pos> = tiles_around(chunk)
            .into_iter()
            .filter(|&p| w.is_trunk(p))
            .collect();
        let open: Vec<Pos> = tiles_around(chunk)
            .into_iter()
            .filter(|&p| p.chunk() == chunk)
            .filter(|&p| w.map().get(p).and_then(cost).is_some() && !w.is_trunk(p))
            .collect();
        assert!(trunks.len() > 100, "dense woods: {} trunks", trunks.len());
        let lookup = w.trunks();
        let blocked = |p: Pos| lookup.blocks(p);
        let (mut found, mut blind_crossings) = (0, 0);
        let mut roll: u64 = 7;
        let mut next = |n: usize| {
            roll = roll
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            usize::try_from(roll >> 33).expect("small") % n
        };
        for _ in 0..300 {
            let (a, b) = (open[next(open.len())], open[next(open.len())]);
            let Some(path) = w.map().path_around(a, b, (60, u32::MAX), cost, blocked) else {
                continue;
            };
            found += 1;
            assert!(path.iter().all(|&p| !blocked(p)), "{a:?} to {b:?}");
            let route = w.map().pull_around(a.centre(), &path, cost, blocked);
            assert!(
                !route_enters(a.centre(), &route, &trunks),
                "{a:?} to {b:?}: {route:?}"
            );
            let blind = w.map().pull(a.centre(), &path, cost);
            if route_enters(a.centre(), &blind, &trunks) {
                blind_crossings += 1;
            }
        }
        assert!(found > 250, "most pairs joined: {found}");
        assert!(
            blind_crossings > 0,
            "pulled blind, some would cross a trunk"
        );
    }

    #[test]
    fn a_body_walking_straight_at_a_lone_trunk_steps_round_it_and_arrives() {
        let mut w = wooded_world();
        let mut rules = wooded_rules();
        let open = |w: &World, p: Pos| {
            w.map()
                .get(p)
                .and_then(|t| rules.movement.cost(t))
                .is_some()
                && !w.is_trunk(p)
        };
        let lone = every_chunk(&w)
            .into_iter()
            .flat_map(|c| {
                w.trees_in(c)
                    .iter()
                    .map(|t| t.at.tile())
                    .collect::<Vec<_>>()
            })
            .find(|&t| {
                (-3..=3).all(|dy| {
                    (-8..=8).all(|dx| (dx, dy) == (0, 0) || open(&w, Pos::new(t.x + dx, t.y + dy)))
                })
            })
            .expect("a lone tree in the meadow");
        let (start, target) = (Pos::new(lone.x - 8, lone.y), Pos::new(lone.x + 8, lone.y));
        let mut events = Vec::new();
        w.queue(Intention::Teleport {
            body: BodyId(1),
            target: start,
        });
        w.advance(1, &rules, &mut events);
        assert_eq!(w.body(BodyId(1)).expect("body 1").pos(), start);
        // No search: the body walks straight, as with none in sight.
        rules.movement.path_budget_tiles = 1;
        let route = first_route(&mut w, &rules, target);
        assert!(route.len() > 2, "a step aside and back: {route:?}");
        assert!(
            route
                .windows(2)
                .all(|p| p[1].x > p[0].x || (p[1].x == p[0].x && p[1].y != p[0].y)),
            "never a step back: {route:?}"
        );
        assert_eq!(route.last(), Some(&target.centre()));
        let mut events = Vec::new();
        for _ in 0..2000 {
            w.step(&rules, &mut events);
            let at = w.body(BodyId(1)).expect("body 1").at();
            assert!(depth_in(at, lone) <= 2, "into the trunk at {at:?}");
            if events.iter().any(|e| {
                matches!(
                    e,
                    Event::BodyStopped {
                        body: BodyId(1),
                        ..
                    }
                )
            }) {
                break;
            }
        }
        assert_eq!(w.body(BodyId(1)).expect("body 1").at(), target.centre());
    }

    #[test]
    fn a_long_straight_walk_through_the_woods_never_goes_through_a_trunk() {
        let mut w = wooded_world();
        let mut rules = wooded_rules();
        rules.movement.path_budget_tiles = 1;
        let chunk = densest_woods(&w);
        let origin = chunk.origin();
        let mut events = Vec::new();
        w.queue(Intention::Teleport {
            body: BodyId(1),
            target: Pos::new(origin.x - 20, origin.y + 16),
        });
        w.advance(1, &rules, &mut events);
        let start = w.body(BodyId(1)).expect("body 1").at();
        let target = Pos::new(origin.x + 52, start.tile().y);
        go(&mut w, target);
        let trunks: Vec<Pos> = tiles_around(chunk)
            .into_iter()
            .filter(|&p| w.is_trunk(p))
            .collect();
        let mut sidestepped = false;
        for _ in 0..5000 {
            let mut events = Vec::new();
            w.step(&rules, &mut events);
            let body = w.body(BodyId(1)).expect("body 1");
            let at = body.at();
            let tile = at.tile();
            assert!(
                !w.is_trunk(tile) || depth_in(at, tile) <= 2,
                "into a trunk at {at:?}"
            );
            sidestepped |= events
                .iter()
                .any(|e| matches!(e, Event::BodyWalks { route, .. } if route.len() > 1));
            if !body.is_walking() && body.goal.is_none() {
                break;
            }
        }
        let end = w.body(BodyId(1)).expect("body 1").at();
        assert!(sidestepped, "trunks stepped round on the way");
        assert!(
            squared_mm(end, target.centre()) * 4 < squared_mm(start, target.centre()),
            "most of the way through the woods: {start:?} to {end:?}, {} trunks",
            trunks.len()
        );
    }

    #[test]
    fn a_body_saved_in_a_trunk_tile_is_let_out_and_walks_on() {
        let mut w = wooded_world();
        let rules = wooded_rules();
        let trunk = w
            .trees_in(densest_woods(&w))
            .first()
            .expect("a tree")
            .at
            .tile();
        assert!(w.is_trunk(trunk));
        // As a snapshot made before trunks blocked the way may hold it.
        let caught = w.spawn_body(trunk);
        let text = serde_json::to_string(&w).unwrap();
        let mut back: World = serde_json::from_str(&text).unwrap();
        back.take_content(&rules);
        let body = back.body(caught).expect("the body");
        let out = body.pos();
        assert!(!back.is_trunk(out), "let out of the trunk");
        assert!(
            (out.x - trunk.x).abs() <= 1 && (out.y - trunk.y).abs() <= 1,
            "beside it: {out:?}"
        );
        assert_eq!(
            back.bodies_in(out.chunk())
                .iter()
                .filter(|b| b.id() == caught)
                .count(),
            1
        );
        // It walks.
        let target = Pos::new(out.x + 3, out.y);
        back.queue(Intention::MoveTo {
            body: caught,
            target,
        });
        let mut events = Vec::new();
        back.step(&rules, &mut events);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::BodyWalks { body, .. } if *body == caught)),
            "{events:?}"
        );
        // Sent onto a trunk, a body stands beside it.
        back.queue(Intention::Teleport {
            body: caught,
            target: trunk,
        });
        back.advance(1, &rules, &mut events);
        assert!(!back.is_trunk(back.body(caught).expect("the body").pos()));
    }
    #[test]
    fn a_straight_walk_to_its_target_tells_one_stop() {
        let mut w = World::empty(
            Calendar::new(10, 1_000_000).unwrap(),
            Map::filled(40, 40, Terrain::Grass),
        );
        w.spawn_body(Pos::new(2, 2));
        let mut rules = rules();
        rules.movement.path_budget_tiles = 1;
        go(&mut w, Pos::new(30, 20));
        let mut events = Vec::new();
        for _ in 0..1000 {
            w.step(&rules, &mut events);
        }
        let stops = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::BodyStopped {
                        body: BodyId(1),
                        ..
                    }
                )
            })
            .count();
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::BodyWalks {
                    body: BodyId(1),
                    ..
                }
            )),
            "it walked"
        );
        assert_eq!(w.body(BodyId(1)).expect("body 1").pos(), Pos::new(30, 20));
        assert_eq!(stops, 1, "{events:?}");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::BodyBlocked { .. })),
            "arrived, not blocked"
        );
        assert!(w.body(BodyId(1)).expect("body 1").goal.is_none(), "done");
    }
}
