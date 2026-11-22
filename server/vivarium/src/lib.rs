//! The vivarium (D41): a world of the simulation under glass, run headless
//! and accelerated, with no network and no clock, and an HTML report per
//! run the project lead reads: the map, four curves, the chronicle, a
//! page per person, the speed and the named factors; and, beside the page,
//! the same report as data, a JSON file the laboratory (D52) reads
//! ([`json`], `docs/10-vivarium-data.md`).
//!
//! This crate depends on `sim` alone. The world is born as the server
//! would bear it (the content read from the same files), the people are
//! born at the camp, and the ticks run as fast as the machine allows;
//! what the report says of a seed is what the game would do with it. The
//! same settings give the same report, byte for byte but for the timing
//! line ([`Report::fixed`] against [`Report::html`]), which the
//! determinism test holds.

#![warn(clippy::float_arithmetic)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use sim::brain::{Brains, News, Way, Ways, Why};
use sim::content::{Content, Trials};
use sim::map::{Map, Pos, Terrain};
use sim::time::Calendar;
use sim::{BodyId, Death, Event, Intention, Sex, World};

pub mod continents;
pub mod json;
pub mod png;

/// Where the content is looked for when no directory is given: as the
/// server does, from the working directory up.
pub const CONTENT_DIRS: [&str; 3] = ["content", "../content", "../../content"];

/// Which world a run lives on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldKind {
    /// The vivarium's world (D41 lot 0): zone 2 of the atlas of seed 14,
    /// 2,176 m a side, born in some six seconds.
    Vivarium,
    /// A patch of the first version's valley, `side` tiles a side, born
    /// at once: the small trial the lead sees first (50 × 50).
    Patch {
        /// Tiles a side.
        side: u32,
    },
}

/// What a run is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The world.
    pub world: WorldKind,
    /// People born at the camp at tick zero.
    pub people: u32,
    /// Game days to run.
    pub days: u32,
    /// Ticks a game second: the plan's slow cadence is 1 (the game runs
    /// at 10), a named factor of the run.
    pub ticks_per_second: u32,
    /// The run's seed: recorded, and the brain's when it comes; the
    /// world's own seed is the world's.
    pub seed: u64,
    /// The content directory, or the first of [`CONTENT_DIRS`] that holds
    /// `materials.ron`.
    pub content: Option<PathBuf>,
    /// The people are piloted by the minimal brain ([`sim::brain`]);
    /// false leaves them standing where they were born, the control run.
    pub brain: bool,
    /// How many tribes, each born at its own camp with `people` people:
    /// 1, or 2 (D41: two tribes either side of the river; on the
    /// vivarium's world camp A and camp B of lot 0, on a patch the camp
    /// and its mirror across the patch's middle).
    pub tribes: u32,
    /// Children born at each camp beside the `people` (lot 3 B): from three
    /// to eleven years old, boys and girls in turn.
    pub children: u32,
    /// Infants of half a year at each camp, each carried and nursed by a
    /// woman of the tribe, one a woman (lot 3 B).
    pub infants: u32,
    /// The grown people's ages at the start, in years, spread evenly from
    /// the first to the second (18 to 40 by default; (20, 20) for a couple
    /// of twenty, lot 3 C's first trial).
    pub adult_years: (u32, u32),
    /// Food without limit (lot 3 C, trial 1): hunger a day is set to 0, so
    /// nobody grows thin or weak and the births are read apart from the
    /// food (F1). A named factor of the run, shown in the report.
    pub fed: bool,
    /// The brains try, judge and copy their ways (D50); false is the
    /// control run beside each trial run (`--no-trials`): the content's
    /// `trials` section is replaced by one where nobody tries anything.
    pub trials: bool,
    /// Where the vivarium's continent is kept between runs
    /// ([`continents`]): `None` for no cache (`--no-continent-cache`),
    /// the continent born at every run. The patch worlds never use it.
    pub continents: Option<PathBuf>,
    /// Helper threads placing the chunks' bearers ahead of the bodies
    /// (`sim::ahead`, D51 amended): 0 is the one simulation thread alone.
    /// The pages are the same whatever the number; only the wall changes,
    /// so it is told with the timing, never with the settings.
    pub helpers: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            world: WorldKind::Patch { side: 50 },
            people: 5,
            days: 10,
            ticks_per_second: 1,
            seed: 1,
            content: None,
            brain: true,
            tribes: 1,
            children: 0,
            infants: 0,
            adult_years: (18, 40),
            fed: false,
            trials: true,
            continents: Some(continents::default_dir()),
            helpers: 1,
        }
    }
}

/// Ticks between two hints to the helpers placing bearers ahead
/// (`sim::ahead::Placer::follow`): at the vivarium's cadence a body walks
/// some twenty tiles between two, under the two chunks of lead.
const FOLLOW_EVERY_TICKS: u64 = 16;

/// Why a run could not start.
#[derive(Debug)]
pub enum RunError {
    /// No content directory found or given.
    NoContent,
    /// A content file could not be read.
    Read(PathBuf, std::io::Error),
    /// The content files do not validate together.
    Content(Box<sim::content::ContentError>),
    /// The calendar refuses the cadence.
    Calendar(sim::time::CalendarError),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoContent => write!(
                f,
                "no content directory: none of {} holds materials.ron (give --content DIR)",
                CONTENT_DIRS.join(", ")
            ),
            Self::Read(path, source) => write!(f, "cannot read {}: {source}", path.display()),
            Self::Content(source) => write!(f, "invalid content: {source}"),
            Self::Calendar(source) => write!(f, "calendar: {source:?}"),
        }
    }
}

impl std::error::Error for RunError {}

/// Reads and validates the content files of `dir`, as the server does.
pub fn load_content(dir: &Path) -> Result<Content, RunError> {
    let read = |name: &str| {
        let path = dir.join(name);
        std::fs::read_to_string(&path).map_err(|source| RunError::Read(path, source))
    };
    let materials = read("materials.ron")?;
    let spawns = read("spawns.ron")?;
    let movement = read("movement.ron")?;
    let needs = read("needs.ron")?;
    let species = read("species.ron")?;
    let flora = read("flora.ron")?;
    let geology = read("geology.ron")?;
    let open_ground = read("open_ground.ron")?;
    let crops = read("crops.ron")?;
    Content::from_ron(&materials, &spawns, &movement, &needs)
        .and_then(|content| content.with_species(&species))
        .and_then(|content| content.with_flora(&flora))
        .and_then(|content| content.with_crops(&crops))
        .and_then(|content| content.with_geology(&geology))
        .and_then(|content| content.with_open_ground(&open_ground))
        .map_err(|source| RunError::Content(Box::new(source)))
}

/// The content directory of `settings`: the one given, or the first of
/// [`CONTENT_DIRS`] holding `materials.ron`.
pub fn content_dir(settings: &Settings) -> Result<PathBuf, RunError> {
    if let Some(dir) = &settings.content {
        return Ok(dir.clone());
    }
    CONTENT_DIRS
        .iter()
        .map(PathBuf::from)
        .find(|dir| dir.join("materials.ron").is_file())
        .ok_or(RunError::NoContent)
}

/// A line of the chronicle: when, and what, in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The tick it happened at.
    pub tick: u64,
    /// The day it happened on (from 1).
    pub day: u64,
    /// What happened.
    pub what: String,
}

/// The people 24 times a day: who lives, and the mean of three gauges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    /// The tick sampled.
    pub tick: u64,
    /// Bodies alive.
    pub alive: u32,
    /// Bodies alive of tribe A and of tribe B.
    pub alive_by_tribe: [u32; 2],
    /// The living thin, by tribe: under half their fat reserve (lot 3).
    pub thin_by_tribe: [u32; 2],
    /// The living weak, by tribe: under a fifth of it.
    pub weak_by_tribe: [u32; 2],
    /// The mean fat reserve of the living, by tribe, in percent of full.
    pub reserve_by_tribe: [u32; 2],
    /// Mean hunger of the living, points (0–100).
    pub hunger: u32,
    /// Mean thirst of the living.
    pub thirst: u32,
    /// Mean fatigue of the living.
    pub fatigue: u32,
}

/// A person's gauges at a tick: (tick, vitality, hunger, thirst, fatigue,
/// fat reserve in percent).
pub type Gauges = (u64, u32, u32, u32, u32, u32);

/// One person's page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Person {
    /// Its tribe: 0 for A, 1 for B.
    pub tribe: u8,
    /// Woman or man; its birth day, from the world's first (lot 3 B).
    pub sex: Option<Sex>,
    /// See `sex`.
    pub born_day: i64,
    /// Who carries it, an infant, as last seen.
    pub carrier: Option<BodyId>,
    /// Where it was born.
    pub born_at: Option<Pos>,
    /// Where it stood at the end of the run.
    pub last_at: Option<Pos>,
    /// When and of what it died, and where.
    pub death: Option<(u64, Death, Pos)>,
    /// Born during the run (D49), not at the departure.
    pub born_in_run: bool,
    /// Its mother and father, when known (D49; the departure gives the
    /// camp's children and infants parents among its couples).
    pub mother: Option<BodyId>,
    /// See `mother`.
    pub father: Option<BodyId>,
    /// Its spouses in order, with the day each was taken (from 1): the
    /// last is its spouse, alive or dead.
    pub couplings: Vec<(u64, BodyId)>,
    /// Its children: those whose mother or father it is, in birth order.
    pub children: Vec<BodyId>,
    /// The days it lost an unborn child (from 1).
    pub lost: Vec<u64>,
    /// Carrying an unborn child at the end of the run.
    pub pregnant: bool,
    /// The infants it nursed at the end of the run.
    pub nursing: Vec<BodyId>,
    /// What it gathered over the run, in joules.
    pub gathered_j: u64,
    /// Meals eaten, and grams in all.
    pub meals: (u32, u32),
    /// Times it drank.
    pub drinks: u32,
    /// Times its brain thought.
    pub thoughts: u64,
    /// Its will, 0 to 100 (D50), once its mind was read.
    pub will: Option<u8>,
    /// Its ways at the end of the run, or at its death (D50).
    pub ways: Option<Ways>,
    /// Its trials, in order (D50).
    pub trials: Vec<TrialLine>,
    /// Ways it copied from camp-mates (D50).
    pub copied: u32,
    /// Its own lines of the chronicle.
    pub entries: Vec<Entry>,
    /// Its gauges 24 times a day.
    pub gauges: Vec<Gauges>,
}

/// One trial of a person (D50), as its page tells it: "day 212, searches
/// twice as wide, and keeps it (day 217)".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrialLine {
    /// The day it started (from 1).
    pub day: u64,
    /// The way moved.
    pub way: Way,
    /// The step, in words ([`step_words`]).
    pub step: String,
    /// The step by its kind.
    pub kind: Step,
    /// The day the world judged it (from 1) and whether it was kept;
    /// `None` while under trial, or when the body died mid-trial.
    pub judged: Option<(u64, bool)>,
}

/// What a trial or a copy did, for the counters per year (D50).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Told {
    /// A trial started.
    Trying,
    /// A trial judged: kept or not.
    Tried(bool),
    /// A way copied from a camp-mate.
    Copied,
}

/// A step of a way by its kind (D50 b): what a trial tried, or a copy took,
/// so the page can say which techniques came out of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    /// Searches wider: `worth_half_tiles` up.
    Wider,
    /// Searches narrower: `worth_half_tiles` down.
    Narrower,
    /// Sleeps out where night finds it.
    SleepOut,
    /// Comes home to sleep.
    SleepHome,
    /// Stores more before resting.
    StoreMore,
    /// Stores less before resting.
    StoreLess,
    /// Its home moved.
    HomeMoved,
}

/// Every [`Step`], in the order of the page.
pub const STEPS: [Step; 7] = [
    Step::Wider,
    Step::Narrower,
    Step::SleepOut,
    Step::SleepHome,
    Step::StoreMore,
    Step::StoreLess,
    Step::HomeMoved,
];

impl Step {
    /// The kind of the step that took `way` from `before` to `after`.
    #[must_use]
    pub fn of(way: Way, before: &Ways, after: &Ways) -> Self {
        match way {
            Way::WorthHalfTiles if after.worth_half_tiles > before.worth_half_tiles => Self::Wider,
            Way::WorthHalfTiles => Self::Narrower,
            Way::SleepOut if after.sleep_out => Self::SleepOut,
            Way::SleepOut => Self::SleepHome,
            Way::StockKj if after.stock_kj > before.stock_kj => Self::StoreMore,
            Way::StockKj => Self::StoreLess,
            Way::Home => Self::HomeMoved,
        }
    }

    /// The kind of a step of `way` whose direction is not known (a
    /// judgement with no trial of the body on record): the first of its
    /// way.
    #[must_use]
    pub const fn of_way(way: Way) -> Self {
        match way {
            Way::WorthHalfTiles => Self::Wider,
            Way::SleepOut => Self::SleepOut,
            Way::StockKj => Self::StoreMore,
            Way::Home => Self::HomeMoved,
        }
    }

    /// The step in two or three words.
    #[must_use]
    pub const fn words(self) -> &'static str {
        match self {
            Self::Wider => "search wider",
            Self::Narrower => "search narrower",
            Self::SleepOut => "sleep out",
            Self::SleepHome => "sleep at home",
            Self::StoreMore => "store more",
            Self::StoreLess => "store less",
            Self::HomeMoved => "home moved",
        }
    }
}

/// What a tribe found over a run (D50 b): its trials by the kind of the
/// step, those kept, and the copies, indexed as [`STEPS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Found {
    /// Trials started, by kind.
    pub tried: [u32; 7],
    /// Trials judged and kept, by kind.
    pub kept: [u32; 7],
    /// Ways copied from camp-mates, by kind.
    pub copied: [u32; 7],
}

impl Found {
    /// "search wider 3 / 1 / 2, sleep out 4 / 2 / 7, …": tried, kept and
    /// copied of every kind that happened.
    #[must_use]
    pub fn words(&self) -> String {
        let parts: Vec<String> = STEPS
            .iter()
            .enumerate()
            .filter(|(i, _)| self.tried[*i] + self.kept[*i] + self.copied[*i] > 0)
            .map(|(i, step)| {
                format!(
                    "{} {} / {} / {}",
                    step.words(),
                    self.tried[i],
                    self.kept[i],
                    self.copied[i]
                )
            })
            .collect();
        if parts.is_empty() {
            "nothing".to_owned()
        } else {
            parts.join(", ")
        }
    }
}

/// A trial, a judgement or a copy of the run (D50), for the counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrialEvent {
    /// The tick.
    pub tick: u64,
    /// The day (from 1).
    pub day: u64,
    /// Whose.
    pub body: BodyId,
    /// Its tribe.
    pub tribe: u8,
    /// What.
    pub told: Told,
    /// The way.
    pub way: Way,
    /// The step by its kind.
    pub step: Step,
}

/// A home moved (D50), for the data (22 November 2026): by a trial, by a
/// copy, or back again when a trial of the home is given up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HomeMove {
    /// The tick.
    pub tick: u64,
    /// The day (from 1).
    pub day: u64,
    /// Whose.
    pub body: BodyId,
    /// Its tribe.
    pub tribe: u8,
    /// The home before.
    pub from: Pos,
    /// The home after.
    pub to: Pos,
    /// "trial", "copy" or "reverted".
    pub how: &'static str,
}

/// The side of a cell of the presence grid, in tiles: where each tribe's
/// living stood at noon, counted by the year (22 November 2026, for the
/// lab's map). The vivarium's world of 2,176 tiles is 68 × 68 cells.
pub const PRESENCE_CELL_TILES: u32 = 32;

/// Where a tribe's living were over a year: bodies counted once a day at
/// noon, by cell of [`PRESENCE_CELL_TILES`] a side, keyed `(cx, cy)`.
pub type Presence = BTreeMap<(u32, u32), u32>;

/// A tribe's ways at a year's end (D50): of its living with a mind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WaysInUse {
    /// Minds read.
    pub minds: u32,
    /// How many sleep out.
    pub sleep_out: u32,
    /// The median of `worth_half_tiles`.
    pub median_worth_tiles: u64,
    /// The median of `stock_kj`.
    pub median_stock_kj: u64,
    /// The camp: the median home (median x, median y).
    pub camp: Option<Pos>,
}

/// How a person came into a family (lot 3 C).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// At the departure, day one.
    Start,
    /// Born into it.
    Born,
    /// Turned sixteen: a family of its own.
    Grown,
    /// Married into the family of this woman.
    Married(BodyId),
}

/// Why a person left a family (lot 3 C).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leaving {
    /// Turned sixteen: a family of its own.
    Grown,
    /// Married: into the family of this woman.
    Married(BodyId),
    /// Died.
    Died,
}

/// A person's time in a family (lot 3 C): from a day, until a day and a
/// reason, or to the end of the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Membership {
    /// Who.
    pub who: BodyId,
    /// The first day (from 1).
    pub from_day: u64,
    /// How it came.
    pub arrived: Arrival,
    /// The day it left and why; `None` while it is still in at the end.
    pub until: Option<(u64, Leaving)>,
}

/// A family of the report (lot 3 C, D41): a couple and its children under
/// sixteen, named by the mother ("the family of person 7"); a single adult
/// is a family of one. Worked out from the people: `sim` keeps no family.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Family {
    /// Its tribe.
    pub tribe: u8,
    /// Everyone who was in it, in the order of the ids, then of the days.
    pub members: Vec<Membership>,
    /// Its lines: arrivals, births, lost pregnancies, weanings, departures
    /// and deaths, in order.
    pub lines: Vec<Entry>,
}

/// A tribe's year in numbers (lot 3 C, the proposal's section 2 and 4.2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Year {
    /// The tribe.
    pub tribe: u8,
    /// The year (from 1).
    pub year: u64,
    /// Alive at the year's end (or the run's).
    pub population: u32,
    /// Alive at the year's start.
    pub population_before: u32,
    /// Born in the year.
    pub births: u32,
    /// Died in the year, by cause, in the order of [`DEATHS`].
    pub deaths: [u32; 6],
    /// The season of each death of hunger.
    pub hunger_seasons: Vec<String>,
    /// Days between a birth of the year and its mother's birth before it.
    pub intervals_days: Vec<u64>,
    /// Children born in the run who turned fifteen by the year's end, and
    /// how many of them were alive to.
    pub fifteen: (u32, u32),
    /// Women of the fertile years alive at the year's end, and how many of
    /// them have no living spouse and no man of the tribe they could take.
    pub fertile: (u32, u32),
    /// Trials started in the year (D50).
    pub trials: u32,
    /// Trials judged in the year and kept.
    pub kept: u32,
    /// Ways copied from camp-mates in the year.
    pub copied: u32,
    /// Homes moved in the year: by a trial of the home, or by copying it.
    pub homes_moved: u32,
    /// The ways in use at the year's end.
    pub ways: WaysInUse,
}

/// The causes of death, in the order of [`Year::deaths`].
pub const DEATHS: [Death; 6] = [
    Death::Illness,
    Death::OldAge,
    Death::Childbirth,
    Death::Starved,
    Death::Dehydrated,
    Death::Froze,
];

/// "of hunger", "in childbirth".
fn cause_words(cause: Death) -> &'static str {
    match cause {
        Death::Starved => "of hunger",
        Death::Froze => "of cold",
        Death::Dehydrated => "of thirst",
        Death::Childbirth => "in childbirth",
        Death::Illness => "of illness",
        Death::OldAge => "of old age",
    }
}

/// Up to this many game days (two years), a person's page shows every
/// line of its life, thoughts and gathers included; past it, only the
/// lines of life (birth, spouse, children, weaning, death), with the count
/// of the rest: a couple's twenty years wrote 640,000 rows and 65 MB.
pub const FULL_LINES_DAYS: u32 = 100;

/// At most this many points on a curve: a sample every so many is drawn
/// when the run has more, so twenty years weigh what two do.
const CURVE_POINTS: usize = 1200;

/// Whether a line of a person's page is one of its life: birth, spouse,
/// children, weaning, loss, death; not a thought, a meal, a drink or a
/// gather.
fn is_life_line(what: &str) -> bool {
    [
        "is born",
        "become spouses",
        "is the child of",
        "is weaned",
        "is taken by",
        "is left alone",
        "loses the child",
        "dies ",
    ]
    .iter()
    .any(|key| what.contains(key))
}

/// Whether a line of a person's page is one of its trials (D50): shown
/// past [`FULL_LINES_DAYS`] like the lines of life.
fn is_trial_line(what: &str) -> bool {
    [
        "tries a way",
        "keeps its way",
        "gives up its way",
        "copies ",
    ]
    .iter()
    .any(|key| what.contains(key))
}

/// Every `n`th of `items`, so that at most [`CURVE_POINTS`] remain.
fn thinned<T>(items: &[T]) -> impl Iterator<Item = &T> {
    let step = items.len().div_ceil(CURVE_POINTS).max(1);
    items.iter().step_by(step)
}

/// What a run produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// What it was given.
    pub settings: Settings,
    /// Tiles a side of the world, and its camp.
    pub map_side: (u32, u32),
    /// The tribes' camps, A first.
    pub camps: Vec<Pos>,
    /// Ticks run.
    pub ticks: u64,
    /// The chronicle, in order.
    pub chronicle: Vec<Entry>,
    /// The people 24 times a day.
    pub samples: Vec<Sample>,
    /// A page per person, by id.
    pub people: BTreeMap<BodyId, Person>,
    /// The map's picture (PNG), one pixel a `map_scale` tiles.
    pub map_png: Vec<u8>,
    /// Tiles a pixel of the picture.
    pub map_scale: u32,
    /// The content's rates the people live by, in points of 100 a game
    /// day, named: shown with the factors so a reader knows the rules of
    /// the run without opening `needs.ron`.
    pub rates: Vec<(&'static str, u32)>,
    /// Each tribe's food, year by year (lot 2): by (tribe, year).
    pub food: BTreeMap<(u8, u64), Balance>,
    /// The trials, judgements and copies of the run, in order (D50).
    pub trials: Vec<TrialEvent>,
    /// Each tribe's ways at each year's end (D50): by (tribe, year).
    pub ways_in_use: BTreeMap<(u8, u64), WaysInUse>,
    /// Where each tribe's living were, by (tribe, year): counted once a
    /// day at noon, by cell of [`PRESENCE_CELL_TILES`].
    pub presence: BTreeMap<(u8, u64), Presence>,
    /// Where every living body stood at noon, day by day (22 November
    /// 2026, for the lab's map of the movements): `(day, [(id, pos), …])`,
    /// days in order, bodies in id order.
    pub positions: Vec<(u64, Vec<(BodyId, Pos)>)>,
    /// The homes moved over the run, in order (D50).
    pub home_moves: Vec<HomeMove>,
    /// Seconds the run took on the wall.
    pub wall_s: f64,
    /// Seconds the world took to be born.
    pub birth_s: f64,
    /// Chunks whose bearers the helper threads placed ahead of the bodies
    /// (`sim::ahead`): 0 without helpers. Timing, never simulation.
    pub placed_ahead: u64,
    /// Chunks whose bearers were placed in all, by the helpers and by the
    /// simulation thread (the same chunk twice when both placed it at
    /// once). Timing too.
    pub placed: u64,
}

/// Runs the vivarium with `settings` and reports.
pub fn run(settings: &Settings) -> Result<Report, RunError> {
    let dir = content_dir(settings)?;
    let content = load_content(&dir)?;
    run_with(settings, &content)
}

/// [`run`] with the content already read.
///
/// # Errors
///
/// As [`run`].
pub fn run_with(settings: &Settings, content: &Content) -> Result<Report, RunError> {
    run_watched(settings, content, &mut |_, _| {})
}

/// The vivarium's map ([`Map::vivarium`]), its continent read from the
/// run's cache of continents when it is kept there, and kept there once
/// born ([`continents`]); born at every run without a cache. The same
/// map either way (D28, amended 19 November 2026).
fn vivarium_map(settings: &Settings) -> Map {
    let Some(dir) = &settings.continents else {
        return Map::vivarium();
    };
    let known = continents::read(dir, sim::plan::VIVARIUM_SEED, sim::plan::CONTINENT_SIDE)
        .into_iter()
        .collect();
    let (map, born) = sim::plan::born_within(known, Map::vivarium);
    for continent in &born {
        if let Err(err) = continents::keep(dir, continent) {
            eprintln!(
                "note: the continent could not be kept under {}: {err}",
                dir.display()
            );
        }
    }
    map
}

/// [`run_with`], with `watch` called on the world at every dawn (the
/// day that begins), where the scenario tests check the world's
/// invariants ([`sim::invariants::breaches`]); the product's run passes
/// nothing and pays nothing.
///
/// # Errors
///
/// As [`run_with`].
#[allow(
    clippy::too_many_lines,
    reason = "one straight reading, top to bottom; splitting it would scatter the rule"
)]
pub fn run_watched(
    settings: &Settings,
    content: &Content,
    watch: &mut dyn FnMut(&World, u64),
) -> Result<Report, RunError> {
    let calendar = Calendar::new(settings.ticks_per_second, Calendar::DEFAULT_SECONDS_PER_DAY)
        .map_err(RunError::Calendar)?;
    // Food without limit (lot 3 C, trial 1): hunger a day is 0, the fat
    // reserve is never drawn on, so the births are read apart from the
    // food. The content is otherwise the one given.
    // The control run (D50, `--no-trials`): the content's trials replaced
    // by none, so nobody tries nor copies anything.
    let own_content;
    let content = if settings.fed || !settings.trials {
        let mut own = content.clone();
        if settings.fed {
            own.needs.hunger_per_day = 0;
        }
        if !settings.trials {
            own.needs.trials = Trials::default();
        }
        own_content = own;
        &own_content
    } else {
        content
    };
    let born = Instant::now();
    let mut world = match settings.world {
        WorldKind::Vivarium => World::on_valley(calendar, content, vivarium_map(settings)),
        WorldKind::Patch { side } => World::with_valley(calendar, content, side),
    };
    // The vivarium's worlds live (D49): births, couples, illness and old
    // age; the game's never do.
    world.set_life(true);
    let birth_s = born.elapsed().as_secs_f64();
    let map_side = (world.map().width(), world.map().height());
    let camps = camps(settings, world.map());
    let mut people: BTreeMap<BodyId, Person> = BTreeMap::new();
    let mut chronicle = Vec::new();
    let mut brains = Brains::new(settings.seed);
    // The helpers placing bearers ahead (D51 amended), told where the
    // bodies are every few ticks; `None` with `--helpers 0`.
    let placer = sim::ahead::Placer::spawn(&world, usize::try_from(settings.helpers).unwrap_or(1));
    // Who is born at each camp: (its id, its tribe, grown or not).
    let mut departure: Vec<(BodyId, u8, bool)> = Vec::new();
    for (tribe, &camp) in camps.iter().enumerate() {
        let tribe = u8::try_from(tribe).expect("two tribes at most");
        // Who is born at the camp (lot 3 B): grown women and men in turn,
        // from `adult_years` (18 to 40 by default); children; infants, each
        // on a woman.
        let per_year = u64::from(calendar.days_per_year());
        let mut born: Vec<(u64, Sex, bool)> = Vec::new();
        let (lo, hi) = settings.adult_years;
        for k in 0..settings.people {
            let years = u64::from(lo)
                + u64::from(k) * u64::from(hi.saturating_sub(lo))
                    / u64::from(settings.people.max(1));
            let sex = if k % 2 == 0 { Sex::Female } else { Sex::Male };
            born.push((years * per_year + u64::from(k) * 7, sex, false));
        }
        for k in 0..settings.children {
            let years =
                3 + u64::from(k) * 8 / u64::from(settings.children.saturating_sub(1).max(1));
            let sex = if k % 2 == 0 { Sex::Male } else { Sex::Female };
            born.push((years * per_year + u64::from(k) * 5, sex, false));
        }
        for k in 0..settings.infants {
            let sex = if k % 2 == 0 { Sex::Female } else { Sex::Male };
            born.push((per_year / 2, sex, true));
        }
        let mut women: Vec<BodyId> = Vec::new();
        for (age_days, sex, infant) in born {
            let Some(id) = world.spawn_near(camp) else {
                continue;
            };
            world.set_birth(id, age_days, sex);
            let mut carrier = None;
            if infant {
                if women.is_empty() {
                    continue;
                }
                let nurse = women.remove(0);
                world.carry(nurse, id);
                carrier = Some(nurse);
            } else if sex == Sex::Female && age_days >= 18 * per_year {
                women.push(id);
            }
            if settings.brain {
                brains.adopt_at(&world, content, id, camp);
            }
            let at = world.body(id).map_or(camp, sim::Body::pos);
            let who = describe(sex, age_days * 1000 / per_year.max(1));
            let what = match carrier {
                Some(nurse) => format!(
                    "{} of tribe {}, {who}, is born at ({}, {}), carried by {}",
                    name(id),
                    tribe_name(tribe),
                    at.x,
                    at.y,
                    name(nurse)
                ),
                None => format!(
                    "{} of tribe {}, {who}, is born at ({}, {}), by its camp",
                    name(id),
                    tribe_name(tribe),
                    at.x,
                    at.y
                ),
            };
            let entry = Entry {
                tick: 0,
                day: 1,
                what,
            };
            chronicle.push(entry.clone());
            let person = people.entry(id).or_default();
            person.tribe = tribe;
            person.born_at = Some(at);
            person.sex = Some(sex);
            person.born_day = world.body(id).map_or(0, sim::Body::born_day);
            person.carrier = carrier;
            person.entries.push(entry);
            departure.push((id, tribe, !infant && age_days >= 16 * per_year));
        }
    }
    // The departure paired (lot 3 C): the grown are coupled at once by the
    // sim's own rule (the nearest of the other sex, kin barred), then each
    // child and infant is given a mother and a father among its camp's
    // couples, the infant's nurse being its mother.
    let mut events = Vec::new();
    world.couple_all(content, &mut events);
    for event in &events {
        if let Event::Coupled { a, b } = event {
            let entry = Entry {
                tick: 0,
                day: 1,
                what: format!("{} and {} become spouses", name(*a), name(*b)),
            };
            chronicle.push(entry.clone());
            for (who, other) in [(*a, *b), (*b, *a)] {
                let person = people.entry(who).or_default();
                person.couplings.push((1, other));
                person.entries.push(entry.clone());
            }
        }
    }
    for tribe in 0..camps.len() {
        let tribe = u8::try_from(tribe).expect("two tribes at most");
        let couples: Vec<(BodyId, BodyId)> = departure
            .iter()
            .filter(|&&(_, t, grown)| t == tribe && grown)
            .filter_map(|&(id, ..)| {
                let body = world.body(id)?;
                (body.sex() == Sex::Female).then_some((id, body.spouse()?))
            })
            .collect();
        let young: Vec<BodyId> = departure
            .iter()
            .filter(|&&(_, t, grown)| t == tribe && !grown)
            .map(|&(id, ..)| id)
            .collect();
        for (k, id) in young.into_iter().enumerate() {
            let parents = match world.body(id).and_then(sim::Body::carried_by) {
                Some(nurse) => Some((nurse, world.body(nurse).and_then(sim::Body::spouse))),
                None if couples.is_empty() => None,
                None => couples
                    .get(k % couples.len())
                    .map(|&(mother, father)| (mother, Some(father))),
            };
            let Some((mother, father)) = parents else {
                continue;
            };
            world.set_parents(id, Some(mother), father);
            let entry = Entry {
                tick: 0,
                day: 1,
                what: match father {
                    Some(father) => format!(
                        "{} is the child of {} and {}",
                        name(id),
                        name(mother),
                        name(father)
                    ),
                    None => format!("{} is the child of {}", name(id), name(mother)),
                },
            };
            chronicle.push(entry.clone());
            let person = people.entry(id).or_default();
            person.mother = Some(mother);
            person.father = father;
            person.entries.push(entry);
            for parent in std::iter::once(mother).chain(father) {
                people.entry(parent).or_default().children.push(id);
            }
        }
    }
    let tribes = camps.len();
    let per_year = calendar.ticks_per_day() * u64::from(calendar.days_per_year());
    let mut food: BTreeMap<(u8, u64), Balance> = BTreeMap::new();
    let mut trials: Vec<TrialEvent> = Vec::new();
    let mut ways_in_use: BTreeMap<(u8, u64), WaysInUse> = BTreeMap::new();
    let mut presence: BTreeMap<(u8, u64), Presence> = BTreeMap::new();
    let mut positions: Vec<(u64, Vec<(BodyId, Pos)>)> = Vec::new();
    let mut home_moves: Vec<HomeMove> = Vec::new();
    // The homes under trial, (before, after), to tell where a given-up
    // home goes back to.
    let mut home_trials: BTreeMap<BodyId, (Pos, Pos)> = BTreeMap::new();
    let first = reserves(&world, content, &people);
    for (tribe, &j) in first.iter().enumerate().take(tribes) {
        food.entry((u8::try_from(tribe).expect("two"), 1))
            .or_default()
            .start_j = j;
    }
    let tribe_of =
        |people: &BTreeMap<BodyId, Person>, body: BodyId| people.get(&body).map_or(0, |p| p.tribe);
    let started = Instant::now();
    // A sample 24 times a day: the day is 7,200 game seconds, so a
    // "hour" of the curves is a twenty-fourth of it, 300 game seconds.
    let ticks_per_sample = (calendar.ticks_per_day() / 24).max(1);
    let ticks = u64::from(settings.days) * calendar.ticks_per_day();
    let mut samples = vec![sample(&world, content, &mut people)];
    // The helpers set off from the camps before the first tick.
    if let Some(placer) = &placer {
        placer.follow(&world, content);
    }
    for _ in 0..ticks {
        events.clear();
        world.step(content, &mut events);
        let tick = world.tick().0;
        if let Some(placer) = &placer
            && tick.is_multiple_of(FOLLOW_EVERY_TICKS)
        {
            placer.follow(&world, content);
        }
        let day = world.time().day;
        // The year whose ticks are elapsing, as the calendar counts (from
        // 1): the tick that starts a year still closes the one before.
        let year = (tick - 1) / per_year + 1;
        if tick.is_multiple_of(per_year) {
            // One plain line a year on stderr, so nobody waits blind on a
            // long run.
            eprintln!(
                "year {year}: {} alive, {} dead so far, {:.0} s, {} chunks placed ahead",
                world.bodies().len(),
                people.values().filter(|p| p.death.is_some()).count(),
                started.elapsed().as_secs_f64(),
                placer.as_ref().map_or(0, sim::ahead::Placer::placed)
            );
        }
        for event in &events {
            match event {
                Event::WorkStopped {
                    body,
                    gives,
                    grams,
                    why,
                } => {
                    let energy = kj(content, gives, *grams);
                    let tribe = tribe_of(&people, *body);
                    food.entry((tribe, year))
                        .or_default()
                        .gathered(content, gives, *grams);
                    let person = people.entry(*body).or_default();
                    person.gathered_j += joules(content, gives, *grams);
                    person.entries.push(Entry {
                        tick,
                        day,
                        what: format!("gathers {grams} g of {gives} ({energy} kJ), stops: {why:?}"),
                    });
                }
                Event::Spoiled {
                    material,
                    grams,
                    holder: Some(body),
                    ..
                } => {
                    let energy = kj(content, material, *grams);
                    let tribe = tribe_of(&people, *body);
                    food.entry((tribe, year))
                        .or_default()
                        .spoiled(content, material, *grams);
                    people.entry(*body).or_default().entries.push(Entry {
                        tick,
                        day,
                        what: format!("{grams} g of {material} it carried spoil ({energy} kJ)"),
                    });
                }
                Event::DayStarted { day } => {
                    chronicle.push(Entry {
                        tick,
                        day: *day,
                        what: format!("day {day} begins"),
                    });
                    watch(&world, *day);
                }
                Event::SeasonStarted { year, season } => chronicle.push(Entry {
                    tick,
                    day,
                    what: format!("{season:?} of year {year} begins"),
                }),
                Event::BodyDied {
                    body, cause, at, ..
                } => {
                    let what = format!(
                        "{} dies {} at ({}, {})",
                        name(*body),
                        cause_words(*cause),
                        at.x,
                        at.y
                    );
                    let entry = Entry { tick, day, what };
                    chronicle.push(entry.clone());
                    let person = people.entry(*body).or_default();
                    person.death = Some((tick, *cause, *at));
                    person.entries.push(entry);
                    // A death of illness, old age or childbirth (D49) comes
                    // with no warning: its mind, still here until the brain's
                    // turn, is read now, or its page would say 0.
                    if let Some(mind) = brains.mind(*body) {
                        person.thoughts = mind.thoughts();
                        person.will = Some(mind.will());
                        person.ways = Some(*mind.ways());
                    }
                }
                Event::BodyAte {
                    body,
                    material,
                    grams,
                    energy_kj,
                    ..
                } => {
                    let tribe = tribe_of(&people, *body);
                    food.entry((tribe, year))
                        .or_default()
                        .eaten(content, material, *grams);
                    let person = people.entry(*body).or_default();
                    person.meals.0 += 1;
                    person.meals.1 += grams;
                    person.entries.push(Entry {
                        tick,
                        day,
                        what: format!("eats {grams} g of {material} ({energy_kj} kJ)"),
                    });
                }
                Event::BodyDrank { body, at } => {
                    let person = people.entry(*body).or_default();
                    person.drinks += 1;
                    person.entries.push(Entry {
                        tick,
                        day,
                        what: format!("drinks at ({}, {})", at.x, at.y),
                    });
                }
                // The registry of kinship (lot 3 C, D49): a birth opens a
                // page in its mother's tribe, carried by her, with a mind
                // for the day it is weaned; the parents gain a child.
                Event::BodyBorn {
                    body,
                    mother,
                    father,
                    at,
                } => {
                    let tribe = tribe_of(&people, *mother);
                    let sex = world.body(*body).map_or(Sex::Female, sim::Body::sex);
                    let entry = Entry {
                        tick,
                        day,
                        what: format!(
                            "{}, {}, is born to {} and {} at ({}, {})",
                            name(*body),
                            if sex == Sex::Male { "a boy" } else { "a girl" },
                            name(*mother),
                            name(*father),
                            at.x,
                            at.y
                        ),
                    };
                    chronicle.push(entry.clone());
                    if settings.brain {
                        brains.adopt_at(&world, content, *body, camps[usize::from(tribe)]);
                    }
                    let child = people.entry(*body).or_default();
                    child.tribe = tribe;
                    child.born_at = Some(*at);
                    child.sex = Some(sex);
                    child.born_day = world.body(*body).map_or(0, sim::Body::born_day);
                    child.born_in_run = true;
                    child.mother = Some(*mother);
                    child.father = Some(*father);
                    child.carrier = Some(*mother);
                    child.entries.push(entry.clone());
                    for parent in [*mother, *father] {
                        let person = people.entry(parent).or_default();
                        person.children.push(*body);
                        person.entries.push(entry.clone());
                    }
                }
                Event::PregnancyLost { mother, at } => {
                    let entry = Entry {
                        tick,
                        day,
                        what: format!(
                            "{}, grown weak, loses the child she carried at ({}, {})",
                            name(*mother),
                            at.x,
                            at.y
                        ),
                    };
                    chronicle.push(entry.clone());
                    let person = people.entry(*mother).or_default();
                    person.lost.push(day);
                    person.entries.push(entry);
                }
                Event::Coupled { a, b } => {
                    let entry = Entry {
                        tick,
                        day,
                        what: format!("{} and {} become spouses", name(*a), name(*b)),
                    };
                    chronicle.push(entry.clone());
                    for (who, other) in [(*a, *b), (*b, *a)] {
                        let person = people.entry(who).or_default();
                        person.couplings.push((day, other));
                        person.entries.push(entry.clone());
                    }
                }
                _ => {}
            }
        }
        // The brain lets a mind go once its body's vitality is under a
        // point, a little before the body dies: what it thought is read
        // now, or its page would say 0 (audit of 20 November 2026, F8).
        for body in world.bodies() {
            if body.condition().vitality.points() == 0
                && let Some(mind) = brains.mind(body.id())
            {
                let person = people.entry(body.id()).or_default();
                person.thoughts = mind.thoughts();
                person.will = Some(mind.will());
                person.ways = Some(*mind.ways());
            }
        }
        for (body, decision) in brains.think(&world, content, &events) {
            let person = people.entry(body).or_default();
            person.entries.push(Entry {
                tick,
                day,
                what: thought(&decision.intention, decision.why),
            });
            world.queue(decision.intention);
        }
        // The trials (D50): what the brains tried, judged and copied, in
        // the chronicle, on the person's page and in the counters.
        for news in brains.news() {
            let home_move = |body: BodyId, from: Pos, to: Pos, how: &'static str| HomeMove {
                tick,
                day,
                body,
                tribe: tribe_of(&people, body),
                from,
                to,
                how,
            };
            match &news {
                News::Trying {
                    body,
                    way: Way::Home,
                    before,
                    after,
                    ..
                } => {
                    home_moves.push(home_move(*body, before.home, after.home, "trial"));
                    home_trials.insert(*body, (before.home, after.home));
                }
                News::Tried {
                    body,
                    way: Way::Home,
                    kept,
                    ..
                } => {
                    if let Some((before, after)) = home_trials.remove(body)
                        && !kept
                    {
                        home_moves.push(home_move(*body, after, before, "reverted"));
                    }
                }
                News::Copied {
                    body,
                    way: Way::Home,
                    before,
                    after,
                    ..
                } => home_moves.push(home_move(*body, before.home, after.home, "copy")),
                _ => {}
            }
            let (body, what, told, way, kind) = match &news {
                News::Trying {
                    body,
                    way,
                    before,
                    after,
                    ..
                } => {
                    let step = step_words(*way, before, after);
                    let kind = Step::of(*way, before, after);
                    let person = people.entry(*body).or_default();
                    person.trials.push(TrialLine {
                        day,
                        way: *way,
                        step: step.clone(),
                        kind,
                        judged: None,
                    });
                    (
                        *body,
                        format!("{} tries a way: {step}", name(*body)),
                        Told::Trying,
                        *way,
                        kind,
                    )
                }
                News::Tried {
                    body, way, kept, ..
                } => {
                    let person = people.entry(*body).or_default();
                    let (step, kind) = person
                        .trials
                        .iter_mut()
                        .rev()
                        .find(|t| t.way == *way && t.judged.is_none())
                        .map_or_else(
                            || (String::new(), Step::of_way(*way)),
                            |t| {
                                t.judged = Some((day, *kept));
                                (t.step.clone(), t.kind)
                            },
                        );
                    let verdict = if *kept {
                        "keeps its way"
                    } else {
                        "gives up its way"
                    };
                    (
                        *body,
                        format!("{} {verdict}: {step}", name(*body)),
                        Told::Tried(*kept),
                        *way,
                        kind,
                    )
                }
                News::Copied {
                    body,
                    from,
                    way,
                    lost,
                    before,
                    after,
                    ..
                } => {
                    let person = people.entry(*body).or_default();
                    person.copied += 1;
                    let what = format!(
                        "{} copies {}{}: {}",
                        name(*body),
                        name(*from),
                        if *lost { ", losing a step" } else { "" },
                        way_words(*way, after)
                    );
                    (
                        *body,
                        what,
                        Told::Copied,
                        *way,
                        Step::of(*way, before, after),
                    )
                }
            };
            let entry = Entry { tick, day, what };
            chronicle.push(entry.clone());
            let tribe = tribe_of(&people, body);
            people.entry(body).or_default().entries.push(entry);
            trials.push(TrialEvent {
                tick,
                day,
                body,
                tribe,
                told,
                way,
                step: kind,
            });
        }
        // Noon: where the living stand, by cell (the lab's map) and one
        // by one (its map of the movements).
        if tick % calendar.ticks_per_day() == calendar.ticks_per_day() / 2 {
            let mut standing: Vec<(BodyId, Pos)> = Vec::new();
            for body in world.bodies() {
                if body.condition().vitality.points() == 0 {
                    continue;
                }
                let tribe = tribe_of(&people, body.id());
                let pos = body.pos();
                let cell = |c: i32| u32::try_from(c).unwrap_or(0) / PRESENCE_CELL_TILES;
                *presence
                    .entry((tribe, year))
                    .or_default()
                    .entry((cell(pos.x), cell(pos.y)))
                    .or_default() += 1;
                standing.push((body.id(), pos));
            }
            standing.sort_unstable_by_key(|&(id, _)| id);
            positions.push((day, standing));
        }
        if tick % ticks_per_sample == 0 {
            for (id, person) in &mut people {
                let Some(body) = world.body(*id) else {
                    continue;
                };
                let now = body.carried_by();
                if now != person.carrier {
                    let what = match now {
                        Some(nurse) => format!("{} is taken by {}", name(*id), name(nurse)),
                        None if world.is_infant(body, content) => {
                            format!("{} is left alone, nobody to carry it", name(*id))
                        }
                        None => format!("{} is weaned: it walks", name(*id)),
                    };
                    let entry = Entry { tick, day, what };
                    chronicle.push(entry.clone());
                    person.entries.push(entry);
                    person.carrier = now;
                }
            }
            let now = sample(&world, content, &mut people);
            for tribe in 0..tribes.min(2) {
                let balance = food
                    .entry((u8::try_from(tribe).expect("two"), year))
                    .or_default();
                balance.thin_samples += u64::from(now.thin_by_tribe[tribe]);
                balance.weak_samples += u64::from(now.weak_by_tribe[tribe]);
            }
            samples.push(now);
        }
        // The year's last tick, or the run's: what the living carry, and
        // the ways in use (D50).
        if tick % per_year == 0 || tick == ticks {
            let carried = reserves(&world, content, &people);
            for (tribe, &j) in carried.iter().enumerate().take(tribes) {
                let t = u8::try_from(tribe).expect("two");
                food.entry((t, year)).or_default().end_j = j;
                if tick < ticks {
                    food.entry((t, year + 1)).or_default().start_j = j;
                }
                ways_in_use.insert((t, year), ways_in_use_of(&world, &brains, &people, t));
            }
        }
    }
    for (id, person) in &mut people {
        if let Some(mind) = brains.mind(*id) {
            person.will = Some(mind.will());
            person.ways = Some(*mind.ways());
        }
        person.thoughts = brains
            .mind(*id)
            .map_or(person.thoughts, sim::brain::Mind::thoughts);
        person.last_at = world
            .body(*id)
            .map(sim::Body::pos)
            .or(person.death.map(|d| d.2));
        person.pregnant = world.body(*id).is_some_and(|b| b.pregnant().is_some());
        person.nursing = world
            .bodies()
            .iter()
            .filter(|b| b.carried_by() == Some(*id))
            .map(sim::Body::id)
            .collect();
    }
    let wall_s = started.elapsed().as_secs_f64();
    let placed_ahead = placer.as_ref().map_or(0, sim::ahead::Placer::placed);
    drop(placer);
    let placed_in_all = world.shared_ground().bearers_placed();
    let (map_png, map_scale) = picture(&world, &camps);
    let needs = &content.needs;
    let rates = vec![
        ("hunger a day", needs.hunger_per_day),
        ("thirst a day", needs.thirst_per_day),
        ("cold a day of night", needs.cold_per_day_at_night),
        ("fatigue a day awake", needs.fatigue_per_day_awake),
        (
            "vitality lost a day starving, the fat reserve spent",
            needs.starving_per_day,
        ),
        ("fat reserve, kJ", needs.reserve_kj),
        (
            "a day's eating at most, % of the need",
            needs.intake_ceiling_percent,
        ),
        (
            "vitality lost a day at full thirst",
            needs.dehydration_per_day,
        ),
        ("vitality lost a day at full cold", needs.freezing_per_day),
    ];
    Ok(Report {
        settings: settings.clone(),
        rates,
        food,
        trials,
        ways_in_use,
        presence,
        positions,
        home_moves,
        map_side,
        camps,
        ticks,
        chronicle,
        samples,
        people,
        map_png,
        map_scale,
        wall_s,
        birth_s,
        placed_ahead,
        placed: placed_in_all,
    })
}

/// A decision in words, for the person's page.
fn thought(intention: &Intention, why: Why) -> String {
    let reason = match why {
        Why::Thirst => "thirsty",
        Why::Hunger => "hungry",
        Why::Explore => "knows no place for it, goes looking",
        Why::Night => "night",
        Why::Home => "nothing pressing",
        Why::Gather => "by day, short of a store",
    };
    let act = match intention {
        Intention::MoveTo { target, .. } => format!("walks to ({}, {})", target.x, target.y),
        Intention::Drink { at, .. } => format!("drinks at ({}, {})", at.x, at.y),
        Intention::Eat { item, .. } => format!("eats item {}", item.0),
        Intention::Sleep { .. } => "lies down to sleep".to_owned(),
        Intention::Wake { .. } => "gets up".to_owned(),
        Intention::Gather { gives, .. } => format!("gathers {gives}"),
        other => format!("{other:?}"),
    };
    format!("{reason}: {act}")
}

/// A step of a trial in words (D50): what moved, from what to what.
/// "searches twice as wide (worth halved at 64 tiles instead of 32)".
#[must_use]
pub fn step_words(way: Way, before: &Ways, after: &Ways) -> String {
    let twice = |a: u64, b: u64| if b > a { "twice" } else { "half" };
    match way {
        Way::WorthHalfTiles => format!(
            "searches {} as wide (worth halved at {} tiles instead of {})",
            twice(before.worth_half_tiles, after.worth_half_tiles),
            after.worth_half_tiles,
            before.worth_half_tiles
        ),
        Way::StockKj => format!(
            "stores {} as much ({} kJ instead of {})",
            twice(before.stock_kj, after.stock_kj),
            after.stock_kj,
            before.stock_kj
        ),
        Way::SleepOut | Way::Home => way_words(way, after),
    }
}

/// A way in words (D50), as it stands in `ways`: "sleeps out where night
/// finds it", "sets its home at (120, 44)".
#[must_use]
pub fn way_words(way: Way, ways: &Ways) -> String {
    match way {
        Way::WorthHalfTiles => format!(
            "searches with worth halved at {} tiles",
            ways.worth_half_tiles
        ),
        Way::SleepOut if ways.sleep_out => "sleeps out where night finds it".to_owned(),
        Way::SleepOut => "comes home to sleep".to_owned(),
        Way::StockKj => format!("stores {} kJ before resting", ways.stock_kj),
        Way::Home => format!("sets its home at ({}, {})", ways.home.x, ways.home.y),
    }
}

/// The ways of tribe `tribe`'s living with a mind (D50): how many sleep
/// out, the medians, the camp as the median home.
fn ways_in_use_of(
    world: &World,
    brains: &Brains,
    people: &BTreeMap<BodyId, Person>,
    tribe: u8,
) -> WaysInUse {
    let mut ways: Vec<Ways> = world
        .bodies()
        .iter()
        .filter(|b| b.condition().vitality.points() > 0)
        .filter(|b| people.get(&b.id()).is_some_and(|p| p.tribe == tribe))
        .filter_map(|b| brains.mind(b.id()).map(|m| *m.ways()))
        .collect();
    if ways.is_empty() {
        return WaysInUse::default();
    }
    let median = |values: &mut Vec<i64>| -> i64 {
        values.sort_unstable();
        values[values.len() / 2]
    };
    let mid = ways.len() / 2;
    ways.sort_by_key(|w| w.worth_half_tiles);
    let median_worth_tiles = ways[mid].worth_half_tiles;
    ways.sort_by_key(|w| w.stock_kj);
    let median_stock_kj = ways[mid].stock_kj;
    let mut xs: Vec<i64> = ways.iter().map(|w| i64::from(w.home.x)).collect();
    let mut ys: Vec<i64> = ways.iter().map(|w| i64::from(w.home.y)).collect();
    let camp = Pos::new(
        i32::try_from(median(&mut xs)).unwrap_or(0),
        i32::try_from(median(&mut ys)).unwrap_or(0),
    );
    WaysInUse {
        minds: u32::try_from(ways.len()).unwrap_or(u32::MAX),
        sleep_out: u32::try_from(ways.iter().filter(|w| w.sleep_out).count()).unwrap_or(u32::MAX),
        median_worth_tiles,
        median_stock_kj,
        camp: Some(camp),
    }
}

/// A tribe's food over a year (lot 2, D45), in joules: what came in,
/// what went out, and what it carried at the year's start and end.
/// Joules, not kilojoules: a gather of a few grams rounded down to the
/// kilojoule, eaten later as one stack rounded down once, left the
/// balance short, and "lost" went negative (audit of 20 November 2026,
/// F5); counted whole, the flows add up exactly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Balance {
    /// Carried by its living at the year's start.
    pub start_j: u64,
    /// Gathered.
    pub gathered_j: u64,
    /// Eaten.
    pub eaten_j: u64,
    /// Spoiled while carried.
    pub spoiled_j: u64,
    /// Carried by its living at the year's end (or the run's).
    pub end_j: u64,
    /// Samples (24 a day) of a living person thin: a person-day is 24.
    pub thin_samples: u64,
    /// Samples of a living person weak.
    pub weak_samples: u64,
}

impl Balance {
    /// Counts `grams` of `material` gathered.
    pub fn gathered(&mut self, content: &Content, material: &str, grams: u32) {
        self.gathered_j += joules(content, material, grams);
    }

    /// Counts `grams` of `material` eaten.
    pub fn eaten(&mut self, content: &Content, material: &str, grams: u32) {
        self.eaten_j += joules(content, material, grams);
    }

    /// Counts `grams` of `material` spoiled while carried.
    pub fn spoiled(&mut self, content: &Content, material: &str, grams: u32) {
        self.spoiled_j += joules(content, material, grams);
    }

    /// What is not told by the flows, in joules: lost with the dead (what
    /// they carried is left on the ground). Never negative: nothing
    /// enters a pack outside the flows.
    pub fn lost_j(&self) -> i64 {
        let flows = i128::from(self.start_j) + i128::from(self.gathered_j)
            - i128::from(self.eaten_j)
            - i128::from(self.spoiled_j)
            - i128::from(self.end_j);
        i64::try_from(flows).unwrap_or(i64::MAX)
    }
}

/// The energy of `grams` of `material`, in kilojoules rounded down: 0
/// if it is no food. For the chronicle's words; the balance counts in
/// joules ([`joules`]).
fn kj(content: &Content, material: &str, grams: u32) -> u64 {
    content
        .material(material)
        .and_then(|m| m.food)
        .map_or(0, |food| u64::from(food.energy_kj(grams)))
}

/// The energy of `grams` of `material`, in joules, exact: 0 if it is no
/// food.
fn joules(content: &Content, material: &str, grams: u32) -> u64 {
    content
        .material(material)
        .and_then(|m| m.food)
        .map_or(0, |food| {
            u64::from(grams) * u64::from(food.energy_kj_per_kg)
        })
}

/// The food each tribe's living carry, spoiled or not, in joules.
fn reserves(world: &World, content: &Content, people: &BTreeMap<BodyId, Person>) -> [u64; 2] {
    let mut out = [0; 2];
    for body in world.bodies() {
        if body.condition().vitality.points() == 0 {
            continue;
        }
        let tribe = people.get(&body.id()).map_or(0, |p| p.tribe);
        out[usize::from(tribe.min(1))] += body
            .inventory()
            .filter_map(|item| world.item(item))
            .map(|item| joules(content, item.material(), item.mass_g()))
            .sum::<u64>();
    }
    out
}

/// The camps of the run's tribes, A first: the map's camp; then, for a
/// second tribe, camp B of the vivarium's zone, or on a patch the camp's
/// mirror across the patch's middle.
fn camps(settings: &Settings, map: &Map) -> Vec<Pos> {
    let a = map.camp();
    let mut camps = vec![a];
    if settings.tribes >= 2 {
        camps.push(match settings.world {
            WorldKind::Vivarium => {
                let (x, y) = sim::plan::ZONE_2.tile(sim::plan::ZONE_2.camp_b);
                Pos::new(
                    i32::try_from(x).expect("on the map"),
                    i32::try_from(y).expect("on the map"),
                )
            }
            WorldKind::Patch { side } => {
                let last = i32::try_from(side).expect("small") - 1;
                Pos::new(last - a.x, last - a.y)
            }
        });
    }
    camps
}

/// "A", "B".
fn tribe_name(tribe: u8) -> char {
    char::from(b'A' + tribe)
}

/// "a woman of 25", "a boy of 7", "an infant girl of 0.5": a person's sex
/// and age in years.
fn describe(sex: Sex, age_milli: u64) -> String {
    let years = age_milli / 1000;
    let noun = match (sex, years) {
        (Sex::Female, 0..=1) => "an infant girl",
        (Sex::Male, 0..=1) => "an infant boy",
        (Sex::Female, 2..=13) => "a girl",
        (Sex::Male, 2..=13) => "a boy",
        (Sex::Female, _) => "a woman",
        (Sex::Male, _) => "a man",
    };
    if years < 2 {
        format!("{noun} of {}.{}", years, age_milli % 1000 / 100)
    } else {
        format!("{noun} of {years}")
    }
}

/// "person 3".
fn name(id: BodyId) -> String {
    format!("person {}", id.0)
}

/// The people now, and each one's gauges into its page.
fn sample(world: &World, content: &Content, people: &mut BTreeMap<BodyId, Person>) -> Sample {
    let tick = world.tick().0;
    let needs = &content.needs;
    let (mut alive, mut hunger, mut thirst, mut fatigue) = (0_u32, 0_u32, 0_u32, 0_u32);
    let mut alive_by_tribe = [0_u32; 2];
    let (mut thin_by_tribe, mut weak_by_tribe) = ([0_u32; 2], [0_u32; 2]);
    let mut reserve_sum = [0_u32; 2];
    for body in world.bodies() {
        let condition = body.condition();
        if condition.vitality.points() == 0 {
            continue;
        }
        alive += 1;
        let tribe = people.get(&body.id()).map_or(0, |p| p.tribe);
        let t = usize::from(tribe.min(1));
        alive_by_tribe[t] += 1;
        thin_by_tribe[t] += u32::from(condition.is_thin(needs));
        weak_by_tribe[t] += u32::from(condition.is_weak(needs));
        let reserve =
            u32::try_from(condition.reserve_left_j(needs) * 100 / needs.reserve_j().max(1))
                .expect("a percent");
        reserve_sum[t] += reserve;
        hunger += condition.hunger.points();
        thirst += condition.thirst.points();
        fatigue += condition.fatigue.points();
        people.entry(body.id()).or_default().gauges.push((
            tick,
            condition.vitality.points(),
            condition.hunger.points(),
            condition.thirst.points(),
            condition.fatigue.points(),
            reserve,
        ));
    }
    let mean = |sum: u32| sum.checked_div(alive).unwrap_or(0);
    let reserve_by_tribe =
        [0, 1].map(|t| reserve_sum[t].checked_div(alive_by_tribe[t]).unwrap_or(0));
    Sample {
        tick,
        alive,
        alive_by_tribe,
        thin_by_tribe,
        weak_by_tribe,
        reserve_by_tribe,
        hunger: mean(hunger),
        thirst: mean(thirst),
        fatigue: mean(fatigue),
    }
}

/// The map as a picture: one pixel a few tiles, so the picture is at
/// most 512 px a side; the terrain under the pixel's centre, the camp in
/// red, the living in white, the dead in black.
fn picture(world: &World, camps: &[Pos]) -> (Vec<u8>, u32) {
    let map = world.map();
    let scale = map.width().max(map.height()).div_ceil(512).max(1);
    let (w, h) = (map.width().div_ceil(scale), map.height().div_ceil(scale));
    let mut rgb = vec![0_u8; usize::try_from(w * h * 3).expect("a small picture")];
    let at = |x: u32, y: u32| usize::try_from((y * w + x) * 3).expect("in the picture");
    for py in 0..h {
        for px in 0..w {
            let pos = Pos::new(
                i32::try_from(px * scale + scale / 2).expect("fits"),
                i32::try_from(py * scale + scale / 2).expect("fits"),
            );
            let colour = match map.get(pos) {
                Some(Terrain::Grass) => [120, 160, 70],
                Some(Terrain::Forest) => [50, 100, 45],
                Some(Terrain::Water) => [60, 110, 170],
                Some(Terrain::Ford) => [120, 150, 180],
                Some(Terrain::Brook) => [90, 140, 190],
                Some(Terrain::Rock) => [130, 125, 115],
                _ => [40, 40, 40],
            };
            rgb[at(px, py)..at(px, py) + 3].copy_from_slice(&colour);
        }
    }
    let mut dot = |pos: Pos, colour: [u8; 3], size: u32| {
        let (cx, cy) = (
            u32::try_from(pos.x).unwrap_or(0) / scale,
            u32::try_from(pos.y).unwrap_or(0) / scale,
        );
        for dy in 0..size {
            for dx in 0..size {
                let (x, y) = (cx + dx, cy + dy);
                if x < w && y < h {
                    rgb[at(x, y)..at(x, y) + 3].copy_from_slice(&colour);
                }
            }
        }
    };
    for &camp in camps {
        dot(camp, [220, 40, 40], 3);
    }
    for body in world.bodies() {
        let dead = body.condition().vitality.points() == 0;
        dot(
            body.pos(),
            if dead { [0, 0, 0] } else { [255, 255, 255] },
            2,
        );
    }
    (png::encode(w, h, &rgb), scale)
}

impl Report {
    /// The food, year by year and tribe by tribe (lot 2's measure).
    fn food_table(&self) -> String {
        let mut h = String::from(
            "<h2>The food, year by year</h2><p>In kilojoules (a person needs about 10,000 a day). Carried: what the tribe's living carry. Lost: what the flows do not tell, left on the ground by the dead. Days thin and weak: person-days under half and under a fifth of the fat reserve.</p>\
<table><tr><th>Tribe</th><th>Year</th><th>Carried at the start</th><th>Gathered</th><th>Eaten</th><th>Spoiled</th><th>Carried at the end</th><th>Lost</th><th>Days thin</th><th>Days weak</th></tr>",
        );
        for (&(tribe, year), b) in &self.food {
            let days = |samples: u64| {
                let tenths = samples * 10 / 24;
                format!("{}.{}", tenths / 10, tenths % 10)
            };
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{year}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                tribe_name(tribe),
                b.start_j / 1000,
                b.gathered_j / 1000,
                b.eaten_j / 1000,
                b.spoiled_j / 1000,
                b.end_j / 1000,
                b.lost_j() / 1000,
                days(b.thin_samples),
                days(b.weak_samples)
            );
        }
        h.push_str("</table>");
        h
    }

    /// The part of the report that depends on the settings alone: the
    /// same settings give the same text, byte for byte.
    #[allow(
        clippy::too_many_lines,
        reason = "one straight reading, top to bottom; splitting it would scatter the rule"
    )]
    pub fn fixed(&self) -> String {
        let s = &self.settings;
        let days_per_year = f64::from(Calendar::DEFAULT_DAYS_PER_YEAR);
        let mut h = String::new();
        let world = match s.world {
            WorldKind::Vivarium => "the vivarium's world (zone 2 of seed 14)".to_owned(),
            WorldKind::Patch { side } => format!("a patch of the valley, {side} × {side} tiles"),
        };
        let _ = write!(
            h,
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Vivarium — {world}, seed {}, {} people, {} days</title>\
<style>body{{font:15px/1.4 system-ui,sans-serif;margin:24px;max-width:1100px;color:#222}}h1,h2{{font-weight:600}}\
table{{border-collapse:collapse}}td,th{{padding:2px 10px;text-align:left;border-bottom:1px solid #ddd}}\
.chronicle td:first-child{{white-space:nowrap;color:#666}}details{{margin:6px 0}}summary{{cursor:pointer}}\
.factors li{{margin:2px 0}}img.map{{image-rendering:pixelated;border:1px solid #999}}</style></head><body>",
            s.seed, s.people, s.days
        );
        let _ = write!(h, "<h1>Vivarium — {world}</h1>");
        let deaths = self.people.values().filter(|p| p.death.is_some()).count();
        let alive = self.samples.last().map_or(0, |last| last.alive);
        let _ = write!(
            h,
            "<p>Seed {}, {} people born at each of {} camp(s), {} game days ({} ticks). At the end: {alive} alive, {deaths} dead.</p>",
            s.seed,
            s.people,
            self.camps.len(),
            s.days,
            self.ticks
        );
        let _ = write!(
            h,
            "<h2>The factors</h2><ul class=\"factors\">\
<li><b>Cadence</b>: {} tick(s) a game second (the game runs at {}), the day {} ticks long. The needs and the gathering are worked out by the game day and hour, so they do not depend on it; but a thought takes a tick and a refused act costs one, so the outcome moves with the cadence (20 November 2026, F2): judge at the game's.</li>\
<li><b>The world</b>: {world}, {} × {} tiles of 1 m; {}.</li>\
<li><b>The year</b>: {} days of {} game seconds (a day of the game is two hours of its clock), four seasons.</li>\
<li><b>The brain</b>: {}.</li>\
<li><b>The trials</b> (D50): {}.</li>\
<li><b>Life</b> (D49): on, as in every vivarium world and never in the game: women conceive, people couple and die of illness and old age, by content/needs.ron's life section; the departure pairs the grown by the sim's own rule and gives the children a mother and a father among the camp's couples{}.</li>\
<li><b>The food</b>: the crops of content/crops.ron (D45), on the trees and plants in their season, more some years than others, whole again the next year; gathered by hand at their rate an hour; spoiled after their days. Hunger is the appetite on a fat reserve (D46): the reserve spent, the body starves.</li>\
<li><b>The needs</b> (content/needs.ron, points of 100 a game day): {}.</li></ul>",
            s.ticks_per_second,
            Calendar::DEFAULT_TICKS_PER_SECOND,
            u64::from(s.ticks_per_second) * u64::from(Calendar::DEFAULT_SECONDS_PER_DAY),
            self.map_side.0,
            self.map_side.1,
            self.camps_words(),
            Calendar::DEFAULT_DAYS_PER_YEAR,
            Calendar::DEFAULT_SECONDS_PER_DAY,
            if s.brain {
                "the minimal one (sim::brain): it thinks only when something changes for its body, drinks when thirst passes 50 (wakes for it past 80); when hunger passes 50 (and until it falls under content/needs.ron's fed_under, D53) it eats what it carries, else the food it saw, else goes to the place it knows bearing food worth most and picks a tree or plant there, chosen once an hour; by day it gathers until it carries its store (three days of food, 30,000 kJ, by default); goes looking when it knows no place (for food: by day, and only when something is ripe somewhere), goes home at night and sleeps within 6 tiles of the camp (or sleeps out, if that is its way); it remembers the drinking places, the food and the places where it saw trees and plants bearing food (corners of place_tiles, not bushes), seen within plant_sight_tiles. Its ways (how far a plant is worth going, whether it sleeps out, how much it stores, where home is) are its own, D50"
            } else {
                "none (the control run): the people stand where they were born"
            },
            if s.trials && s.brain {
                "on: a body in distress (hungry at nightfall, or a day without a gather) for content/needs.ron's trial_after_days running may try one way moved by one step, with a daily chance by its will; after trial_days the world has judged on every distress (kept if nothing worsened and one thing improved over gathered a day, thin days, thirsty days and cold nights, else reverted); a home trial goes towards the plants known bearing or the nearest water known, half and half, never past the map's edge; at night at the camp a body may copy one way from the camp-mate who fared best lately (the fewest days of distress, gathered kJ breaking ties), the more readily the greater that mate's will, a copy losing a step at times; children follow their mother's home every night"
            } else {
                "off (the control run, --no-trials): nobody tries nor copies anything"
            },
            if s.fed {
                "; <b>food without limit</b>: hunger a day is 0, nobody grows thin or weak, so the births are read apart from the food"
            } else {
                ""
            },
            self.rates
                .iter()
                .map(|(name, rate)| format!("{name} {rate}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let _ = write!(
            h,
            "<h2>The map</h2><p>One pixel is {} tile{}; the camp red, the living white, the dead black.</p><img class=\"map\" alt=\"the map\" src=\"data:image/png;base64,{}\">",
            self.map_scale,
            if self.map_scale == 1 { "" } else { "s" },
            png::base64(&self.map_png)
        );
        h.push_str("<h2>The curves</h2>");
        h.push_str(&self.curves());
        h.push_str(&self.food_table());
        h.push_str(&self.tribes_table());
        h.push_str(&self.ages_table());
        let _ = write!(
            h,
            "<h2>The chronicle</h2><p>{} lines.</p><table class=\"chronicle\">",
            self.chronicle.len()
        );
        for entry in &self.chronicle {
            let _ = write!(
                h,
                "<tr><td>day {} · tick {}</td><td>{}</td></tr>",
                entry.day,
                entry.tick,
                escape(&entry.what)
            );
        }
        h.push_str("</table>");
        h.push_str(&self.families_html());
        let _ = write!(
            h,
            "<p class=\"note\">One game year is {days_per_year} days; the speed below counts years of game a minute of wall clock.</p>"
        );
        h
    }

    /// The day (from 1) of `tick`.
    fn day_of(&self, tick: u64) -> u64 {
        tick / self.ticks_per_day().max(1) + 1
    }

    /// The day (from 1) a person was born on, 1 for those born before the
    /// world.
    fn birth_day(person: &Person) -> u64 {
        u64::try_from(person.born_day + 1).unwrap_or(0).max(1)
    }

    /// The day (from 1) a person died on, if it did.
    fn death_day(&self, person: &Person) -> Option<u64> {
        person.death.map(|(tick, ..)| self.day_of(tick))
    }

    /// Whether a person is alive at the end of `day`.
    fn alive_on(&self, person: &Person, day: u64) -> bool {
        Self::birth_day(person) <= day && self.death_day(person).is_none_or(|d| d > day)
    }

    /// How old a person is at the end of `day` (from 1), in years.
    fn years_on(person: &Person, day: u64) -> u64 {
        Self::age_milli(person, i64::try_from(day).unwrap_or(i64::MAX) - 1) / 1000
    }

    /// The day (from 1) a person turns `years`.
    fn birthday(person: &Person, years: i64) -> u64 {
        u64::try_from(person.born_day + years * i64::from(Calendar::DEFAULT_DAYS_PER_YEAR) + 1)
            .unwrap_or(0)
    }

    /// The last day of the run (from 1).
    fn last_day(&self) -> u64 {
        u64::from(self.settings.days).max(1)
    }

    /// A person's living spouse at the end of `day`, if any: its last
    /// coupling by then, when that spouse is alive.
    fn spouse_on(&self, person: &Person, day: u64) -> Option<BodyId> {
        person
            .couplings
            .iter()
            .rev()
            .find(|(d, _)| *d <= day)
            .map(|(_, s)| *s)
            .filter(|s| self.people.get(s).is_some_and(|sp| self.alive_on(sp, day)))
    }

    /// Whether two people are kin too near to couple (D49): parent and
    /// child, or siblings, half-siblings included.
    fn kin(a: (BodyId, &Person), b: (BodyId, &Person)) -> bool {
        let parent = |p: BodyId, c: &Person| c.mother == Some(p) || c.father == Some(p);
        let share = |x: Option<BodyId>, y: Option<BodyId>| x.is_some() && x == y;
        parent(a.0, b.1)
            || parent(b.0, a.1)
            || share(a.1.mother, b.1.mother)
            || share(a.1.father, b.1.father)
    }

    /// The families (lot 3 C, D41): a couple and its children under
    /// sixteen, named by the mother; a single adult a family of one. A
    /// child is in its mother's family until sixteen, then in its own; a
    /// man joins his wife's when he marries and stays in it widowed; a
    /// woman of sixteen heads her own. Worked out from the people.
    #[must_use]
    pub fn families(&self) -> BTreeMap<BodyId, Family> {
        let mut families: BTreeMap<BodyId, Family> = BTreeMap::new();
        for (&id, person) in &self.people {
            let start = if person.born_in_run {
                Self::birth_day(person)
            } else {
                1
            };
            let sixteen = Self::birthday(person, 16);
            let woman = person.sex == Some(Sex::Female);
            let end = self.death_day(person);
            let last = end.unwrap_or(self.last_day());
            // The head of its first family, and how it came.
            let (mut head, mut arrived) = if start < sixteen {
                (
                    person.mother.unwrap_or(id),
                    if person.born_in_run {
                        Arrival::Born
                    } else {
                        Arrival::Start
                    },
                )
            } else if woman {
                (id, Arrival::Start)
            } else {
                let wife = person
                    .couplings
                    .first()
                    .filter(|(day, _)| *day == start)
                    .map(|(_, s)| *s);
                (wife.unwrap_or(id), Arrival::Start)
            };
            let mut from = start;
            // Its moves: sixteen, then each coupling, in the order of the days.
            let mut moves: Vec<(u64, Option<BodyId>)> = Vec::new();
            if sixteen > start && sixteen <= last {
                moves.push((sixteen, None));
            }
            for &(day, spouse) in &person.couplings {
                if day > start && day <= last {
                    moves.push((day, Some(spouse)));
                }
            }
            moves.sort_by_key(|&(day, spouse)| (day, spouse.is_some()));
            for (day, spouse) in moves {
                let (next, leaving, coming) = match spouse {
                    None => (id, Leaving::Grown, Arrival::Grown),
                    Some(s) if woman => (id, Leaving::Married(s), Arrival::Married(s)),
                    Some(s) => (s, Leaving::Married(s), Arrival::Married(s)),
                };
                if next == head {
                    continue;
                }
                families.entry(head).or_default().members.push(Membership {
                    who: id,
                    from_day: from,
                    arrived,
                    until: Some((day, leaving)),
                });
                head = next;
                arrived = coming;
                from = day;
            }
            families.entry(head).or_default().members.push(Membership {
                who: id,
                from_day: from,
                arrived,
                until: end.map(|day| (day, Leaving::Died)),
            });
        }
        for (&head, family) in &mut families {
            family.tribe = self.people.get(&head).map_or(0, |p| p.tribe);
            family.members.sort_by_key(|m| (m.who, m.from_day));
            family.lines = self.family_lines(head, &family.members);
        }
        families
    }

    /// A family's lines, in order: who came and how, who was born, lost a
    /// child, was weaned, left and why, died and of what.
    fn family_lines(&self, head: BodyId, members: &[Membership]) -> Vec<Entry> {
        let per_day = self.ticks_per_day();
        let mut lines: Vec<Entry> = Vec::new();
        let line = |day: u64, what: String| Entry {
            tick: day.saturating_sub(1) * per_day,
            day,
            what,
        };
        for m in members {
            let Some(person) = self.people.get(&m.who) else {
                continue;
            };
            let who = name(m.who);
            let what = match m.arrived {
                Arrival::Start => format!(
                    "{who}, {}, at the start",
                    person
                        .sex
                        .map_or_else(String::new, |sex| describe(sex, Self::age_milli(person, 0)))
                ),
                Arrival::Born => format!(
                    "{who}, {}, is born to {} and {}",
                    match person.sex {
                        Some(Sex::Male) => "a boy",
                        _ => "a girl",
                    },
                    person.mother.map_or_else(|| "nobody".to_owned(), name),
                    person.father.map_or_else(|| "nobody".to_owned(), name)
                ),
                Arrival::Grown => format!("{who} turns 16 and founds this family"),
                Arrival::Married(s) if s == head => format!("{who} marries {} and joins", name(s)),
                Arrival::Married(s) => format!("{who} marries {} and heads this family", name(s)),
            };
            lines.push(line(m.from_day, what));
            let until = m.until.map_or(self.last_day(), |(day, _)| day);
            for &day in person
                .lost
                .iter()
                .filter(|d| (m.from_day..=until).contains(d))
            {
                lines.push(line(day, format!("{who} loses the child she carried")));
            }
            for entry in person.entries.iter().filter(|e| {
                (m.from_day..=until).contains(&e.day)
                    && (e.what.contains("is weaned")
                        || e.what.contains("is taken by")
                        || e.what.contains("is left alone"))
            }) {
                lines.push(entry.clone());
            }
            match m.until {
                Some((day, Leaving::Grown)) => {
                    lines.push(line(
                        day,
                        format!("{who} turns 16 and leaves for a family of its own"),
                    ));
                }
                Some((day, Leaving::Married(s))) => {
                    lines.push(line(
                        day,
                        format!(
                            "{who} marries {} and leaves for the family of {}",
                            name(s),
                            name(s)
                        ),
                    ));
                }
                Some((day, Leaving::Died)) => {
                    let (tick, cause, at) = person.death.expect("died");
                    lines.push(Entry {
                        tick,
                        day,
                        what: format!(
                            "{who} dies {} at ({}, {}), {}",
                            cause_words(cause),
                            at.x,
                            at.y,
                            describe(
                                person.sex.unwrap_or(Sex::Male),
                                Self::age_milli(person, i64::try_from(day).unwrap_or(0) - 1)
                            )
                        ),
                    });
                }
                None => {}
            }
        }
        lines.sort_by_key(|e| e.tick);
        lines
    }

    /// What `tribe` found over the run (D50 b): its trials, those kept and
    /// its copies, by the kind of the step.
    #[must_use]
    pub fn found(&self, tribe: u8) -> Found {
        let mut found = Found::default();
        for t in self.trials.iter().filter(|t| t.tribe == tribe) {
            let i = STEPS
                .iter()
                .position(|s| *s == t.step)
                .expect("one of seven");
            match t.told {
                Told::Trying => found.tried[i] += 1,
                Told::Tried(kept) => found.kept[i] += u32::from(kept),
                Told::Copied => found.copied[i] += 1,
            }
        }
        found
    }

    /// Each tribe's years in numbers (lot 3 C).
    #[must_use]
    pub fn years(&self) -> Vec<Year> {
        let per_year = u64::from(Calendar::DEFAULT_DAYS_PER_YEAR);
        let last = self.last_day();
        let count = last.div_ceil(per_year);
        let people = |tribe: u8| self.people.iter().filter(move |(_, p)| p.tribe == tribe);
        let mut out = Vec::new();
        for tribe in 0..u8::try_from(self.camps.len()).unwrap_or(1) {
            for year in 1..=count {
                let before = (year - 1) * per_year;
                let end = (year * per_year).min(last);
                let alive = |day: u64| {
                    u32::try_from(people(tribe).filter(|(_, p)| self.alive_on(p, day)).count())
                        .unwrap_or(u32::MAX)
                };
                let mut y = Year {
                    tribe,
                    year,
                    population: alive(end),
                    population_before: if year == 1 { alive(1) } else { alive(before) },
                    ..Year::default()
                };
                for (_, p) in people(tribe) {
                    let born = Self::birth_day(p);
                    if p.born_in_run && born > before && born <= end {
                        y.births += 1;
                        if let Some(earlier) = p
                            .mother
                            .and_then(|m| self.people.get(&m))
                            .map(|m| m.children.iter().filter_map(|c| self.people.get(c)))
                            .and_then(|children| {
                                children.map(Self::birth_day).filter(|&d| d < born).max()
                            })
                        {
                            y.intervals_days.push(born - earlier);
                        }
                    }
                    if let Some((tick, cause, _)) = p.death {
                        let day = self.day_of(tick);
                        if day > before && day <= end {
                            if let Some(i) = DEATHS.iter().position(|&c| c == cause) {
                                y.deaths[i] += 1;
                            }
                            if cause == Death::Starved {
                                y.hunger_seasons.push(self.season_name(tick));
                            }
                        }
                    }
                    if p.born_in_run {
                        let fifteen = Self::birthday(p, 15);
                        if fifteen <= end {
                            y.fifteen.1 += 1;
                            y.fifteen.0 += u32::from(self.alive_on(p, fifteen));
                        }
                    }
                }
                // The fertile women without a possible spouse at the year's end.
                for (&id, w) in people(tribe) {
                    if w.sex != Some(Sex::Female)
                        || !self.alive_on(w, end)
                        || !(16..=42).contains(&Self::years_on(w, end))
                    {
                        continue;
                    }
                    y.fertile.1 += 1;
                    if self.spouse_on(w, end).is_some() {
                        continue;
                    }
                    let possible = people(tribe).any(|(&m, man)| {
                        man.sex == Some(Sex::Male)
                            && self.alive_on(man, end)
                            && (16..=50).contains(&Self::years_on(man, end))
                            && self.spouse_on(man, end).is_none()
                            && !Self::kin((id, w), (m, man))
                    });
                    y.fertile.0 += u32::from(!possible);
                }
                // The trials (D50).
                for t in self
                    .trials
                    .iter()
                    .filter(|t| t.tribe == tribe && t.day > before && t.day <= end)
                {
                    match t.told {
                        Told::Trying => {
                            y.trials += 1;
                            y.homes_moved += u32::from(t.way == Way::Home);
                        }
                        Told::Tried(kept) => y.kept += u32::from(kept),
                        Told::Copied => {
                            y.copied += 1;
                            y.homes_moved += u32::from(t.way == Way::Home);
                        }
                    }
                }
                y.ways = self
                    .ways_in_use
                    .get(&(tribe, year))
                    .copied()
                    .unwrap_or_default();
                out.push(y);
            }
        }
        out
    }

    /// "spring", "winter": the season of `tick`.
    fn season_name(&self, tick: u64) -> String {
        Calendar::new(
            self.settings.ticks_per_second,
            Calendar::DEFAULT_SECONDS_PER_DAY,
        )
        .map_or_else(
            |_| String::new(),
            |calendar| format!("{:?}", calendar.time_at(sim::Tick(tick)).season).to_lowercase(),
        )
    }

    /// The tribes, year by year (lot 3 C): population and its growth,
    /// births, deaths by cause, the seasons of the deaths of hunger, the
    /// mean interval between births, the children reaching fifteen and the
    /// fertile women without a possible spouse.
    fn tribes_table(&self) -> String {
        let mut h = String::from(
            "<h2>The tribes, year by year</h2><p>Population at the year's end and its growth over the year; births; deaths by cause, with the season of each death of hunger; the mean years between a birth and its mother's birth before it; the children born in the run who turned fifteen by then, and how many lived to it; the women of 16 to 42 alive, and how many of them have no living spouse and no man of the tribe of 16 to 50, unwed and not their kin, to take. The trials (D50): trials started in the year, those judged and kept, the ways copied from camp-mates, the homes moved (by a trial or a copy), the ways in use at the year's end among the living, and the camp (the median home of the living).</p>\
<table><tr><th>Tribe</th><th>Year</th><th>Population</th><th>Growth</th><th>Births</th><th>Illness</th><th>Old age</th><th>Childbirth</th><th>Hunger</th><th>Thirst</th><th>Cold</th><th>Birth interval</th><th>Reached 15</th><th>Fertile women without a possible spouse</th><th>Trials</th><th>Kept</th><th>Copied</th><th>Homes moved</th><th>Ways at the year's end</th><th>Camp</th></tr>",
        );
        for y in self.years() {
            let growth = if y.population_before == 0 {
                "—".to_owned()
            } else {
                let tenths = (i64::from(y.population) - i64::from(y.population_before)) * 1000
                    / i64::from(y.population_before);
                format!(
                    "{}{}.{} %",
                    if tenths < 0 { "−" } else { "+" },
                    tenths.abs() / 10,
                    tenths.abs() % 10
                )
            };
            let interval = if y.intervals_days.is_empty() {
                "—".to_owned()
            } else {
                let mean_tenths = y.intervals_days.iter().sum::<u64>() * 10
                    / (u64::from(Calendar::DEFAULT_DAYS_PER_YEAR)
                        * u64::try_from(y.intervals_days.len()).unwrap_or(1));
                format!(
                    "{}.{} years ({} births)",
                    mean_tenths / 10,
                    mean_tenths % 10,
                    y.intervals_days.len()
                )
            };
            let hunger = if y.hunger_seasons.is_empty() {
                y.deaths[3].to_string()
            } else {
                format!("{} ({})", y.deaths[3], y.hunger_seasons.join(", "))
            };
            let fifteen = if y.fifteen.1 == 0 {
                "—".to_owned()
            } else {
                format!("{} of {}", y.fifteen.0, y.fifteen.1)
            };
            let ways = if y.ways.minds == 0 {
                "—".to_owned()
            } else {
                format!(
                    "{} of {} sleep out; worth halved at {} tiles, store {} kJ (medians)",
                    y.ways.sleep_out,
                    y.ways.minds,
                    y.ways.median_worth_tiles,
                    y.ways.median_stock_kj
                )
            };
            let camp = y
                .ways
                .camp
                .map_or_else(|| "—".to_owned(), |c| format!("({}, {})", c.x, c.y));
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{growth}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{hunger}</td><td>{}</td><td>{}</td><td>{interval}</td><td>{fifteen}</td><td>{} of {}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{ways}</td><td>{camp}</td></tr>",
                tribe_name(y.tribe),
                y.year,
                y.population,
                y.births,
                y.deaths[0],
                y.deaths[1],
                y.deaths[2],
                y.deaths[4],
                y.deaths[5],
                y.fertile.0,
                y.fertile.1,
                y.trials,
                y.kept,
                y.copied,
                y.homes_moved
            );
        }
        h.push_str("</table>");
        if self.trials.is_empty() {
            return h;
        }
        h.push_str("<p>What each tribe found over the run (D50 b), by the kind of the step: trials started / kept / ways copied.</p><ul>");
        for tribe in 0..u8::try_from(self.camps.len()).unwrap_or(u8::MAX) {
            let _ = write!(
                h,
                "<li>Tribe {}: {}.</li>",
                tribe_name(tribe),
                self.found(tribe).words()
            );
        }
        h.push_str("</ul>");
        h
    }

    /// The person's page: its summary line, its sparkline and its lines.
    fn person_html(&self, id: BodyId, person: &Person) -> String {
        let mut h = String::new();
        let fate = match &person.death {
            Some((tick, cause, at)) => format!(
                "died on day {} (tick {tick}, {}) {} at ({}, {})",
                self.day_of(*tick),
                self.season_of(*tick),
                cause_words(*cause),
                at.x,
                at.y
            ),
            None => "alive at the end".to_owned(),
        };
        let who = person.sex.map_or_else(String::new, |sex| {
            if person.born_in_run {
                format!(", born on day {}", Self::birth_day(person))
            } else {
                format!(
                    ", {} at the start",
                    describe(sex, Self::age_milli(person, 0))
                )
            }
        });
        let _ = write!(
            h,
            "<details><summary>{} (tribe {}{who}) — {fate}; {} meals ({} g), drank {} times, thought {} times, gathered {} kJ{}</summary>",
            name(id),
            tribe_name(person.tribe),
            person.meals.0,
            person.meals.1,
            person.drinks,
            person.thoughts,
            person.gathered_j / 1000,
            person
                .will
                .map_or_else(String::new, |will| format!("; will {will}"))
        );
        h.push_str(&sparkline(&person.gauges, self.ticks));
        if let Some(ways) = &person.ways {
            let _ = write!(
                h,
                "<p>Its ways (D50) at the end: worth halved at {} tiles, {}, stores {} kJ before resting, home at ({}, {}); {} trials, {} ways copied.</p>",
                ways.worth_half_tiles,
                if ways.sleep_out {
                    "sleeps out where night finds it"
                } else {
                    "comes home to sleep"
                },
                ways.stock_kj,
                ways.home.x,
                ways.home.y,
                person.trials.len(),
                person.copied
            );
        }
        if !person.trials.is_empty() {
            h.push_str("<ul class=\"trials\">");
            for trial in &person.trials {
                let _ = write!(h, "<li>{}</li>", escape(&Self::trial_words(trial)));
            }
            h.push_str("</ul>");
        }
        let full = self.settings.days <= FULL_LINES_DAYS;
        if !full {
            let _ = write!(
                h,
                "<p>The lines of its life; its {} lines of thought, meals, drinks and gathers are not shown (a run over {FULL_LINES_DAYS} days).</p>",
                person
                    .entries
                    .iter()
                    .filter(|e| !is_life_line(&e.what) && !is_trial_line(&e.what))
                    .count()
            );
        }
        h.push_str("<table class=\"chronicle\">");
        for entry in person
            .entries
            .iter()
            .filter(|e| full || is_life_line(&e.what) || is_trial_line(&e.what))
        {
            let _ = write!(
                h,
                "<tr><td>day {} · tick {}</td><td>{}</td></tr>",
                entry.day,
                entry.tick,
                escape(&entry.what)
            );
        }
        h.push_str("</table></details>");
        h
    }

    /// A trial in words (D50): "day 212, searches twice as wide (worth
    /// halved at 64 tiles instead of 32), and keeps it (day 217)".
    #[must_use]
    pub fn trial_words(trial: &TrialLine) -> String {
        let verdict = match trial.judged {
            Some((day, true)) => format!(", and keeps it (day {day})"),
            Some((day, false)) => format!(", and gives it up (day {day})"),
            None => ", still under trial".to_owned(),
        };
        format!("day {}, {}{verdict}", trial.day, trial.step)
    }

    /// Days a person was thin and weak, in tenths, from its gauges (24 a
    /// day): its fat reserve under half, under a fifth.
    fn thin_weak_days(person: &Person) -> (u64, u64) {
        let count = |under: u32| {
            u64::try_from(person.gauges.iter().filter(|g| g.5 < under).count()).unwrap_or(0) * 10
                / 24
        };
        (count(50), count(20))
    }

    /// The families' pages (lot 3 C): each with its members, their age, sex
    /// and state, its births and deaths, its food and its lines.
    fn families_html(&self) -> String {
        let families = self.families();
        let mut h = format!(
            "<h2>The families</h2><p>{} families: a couple and its children under sixteen, named by the mother; a single adult is a family of one. A child leaves at sixteen for a family of its own, a man for his wife's when he marries; each move is a line in both. Days thin and weak: under half and under a fifth of the fat reserve. The reserve curve is the mean fat reserve of the members alive, in percent; gathered: what its members gathered over the whole run.</p>",
            families.len()
        );
        for (head, family) in &families {
            let in_at_end: Vec<&Membership> = family
                .members
                .iter()
                .filter(|m| m.until.is_none())
                .collect();
            let births = family
                .members
                .iter()
                .filter(|m| m.arrived == Arrival::Born)
                .count();
            let deaths = family
                .members
                .iter()
                .filter(|m| matches!(m.until, Some((_, Leaving::Died))))
                .count();
            let _ = write!(
                h,
                "<details class=\"family\"><summary><b>The family of {}</b> (tribe {}): {} in it at the end, {} born into it, {} died in it</summary>",
                name(*head),
                tribe_name(family.tribe),
                in_at_end.len(),
                births,
                deaths
            );
            h.push_str("<table><tr><th>Member</th><th>Who</th><th>In the family</th><th>State at the end</th><th>Days thin</th><th>Days weak</th><th>Gathered, kJ</th></tr>");
            let mut gathered = 0;
            for m in &family.members {
                if let Some(person) = self.people.get(&m.who) {
                    gathered += person.gathered_j;
                    h.push_str(&self.member_row(*head, m, person));
                }
            }
            let _ = write!(
                h,
                "</table><p>Gathered by its members over the run: {} kJ.</p>",
                gathered / 1000
            );
            h.push_str(&self.family_curve(family));
            h.push_str("<table class=\"chronicle\">");
            for entry in &family.lines {
                let _ = write!(
                    h,
                    "<tr><td>day {} · tick {}</td><td>{}</td></tr>",
                    entry.day,
                    entry.tick,
                    escape(&entry.what)
                );
            }
            h.push_str("</table>");
            let mut shown = std::collections::BTreeSet::new();
            for m in &family.members {
                if let Some(person) = self.people.get(&m.who)
                    && shown.insert(m.who)
                {
                    h.push_str(&self.person_html(m.who, person));
                }
            }
            h.push_str("</details>");
        }
        h
    }

    /// A member's row of its family's table: who it is, its time in the
    /// family, its state at the end, its days thin and weak, what it
    /// gathered.
    fn member_row(&self, head: BodyId, m: &Membership, person: &Person) -> String {
        let last = self.last_day();
        let mut h = String::new();
        let on = m.until.map_or(last, |(day, _)| day);
        let role = if m.who == head {
            if person.sex == Some(Sex::Female) {
                "the mother"
            } else {
                "alone"
            }
        } else if person.couplings.iter().any(|(_, s)| *s == head) {
            "the father"
        } else if person.mother == Some(head)
            || person.father.is_some_and(|f| {
                self.people
                    .get(&head)
                    .is_some_and(|hd| hd.couplings.iter().any(|(_, s)| *s == f))
            })
        {
            if person.sex == Some(Sex::Male) {
                "a son"
            } else {
                "a daughter"
            }
        } else {
            "—"
        };
        let who = format!(
            "{role}, {}",
            describe(
                person.sex.unwrap_or(Sex::Male),
                Self::age_milli(person, i64::try_from(on).unwrap_or(0) - 1)
            )
        );
        let span = match m.until {
            None => format!("from day {} to the end", m.from_day),
            Some((day, Leaving::Grown)) => {
                format!("from day {} to day {day}, left at 16", m.from_day)
            }
            Some((day, Leaving::Married(s))) => {
                format!("from day {} to day {day}, married {}", m.from_day, name(s))
            }
            Some((day, Leaving::Died)) => format!(
                "from day {} to day {day}, died {}",
                m.from_day,
                person.death.map_or("", |d| cause_words(d.1))
            ),
        };
        let mut state: Vec<String> = Vec::new();
        if m.until.is_none() {
            if person.pregnant {
                state.push("pregnant".to_owned());
            }
            for infant in &person.nursing {
                state.push(format!("nursing {}", name(*infant)));
            }
            if person.carrier.is_some() {
                state.push("carried".to_owned());
            }
            if state.is_empty() {
                state.push("alive".to_owned());
            }
        } else {
            state.push("gone".to_owned());
        }
        let (thin, weak) = Self::thin_weak_days(person);
        let _ = write!(
            h,
            "<tr><td>{}</td><td>{who}</td><td>{span}</td><td>{}</td><td>{}.{}</td><td>{}.{}</td><td>{}</td></tr>",
            name(m.who),
            state.join(", "),
            thin / 10,
            thin % 10,
            weak / 10,
            weak % 10,
            person.gathered_j / 1000
        );
        h
    }

    /// A family's curve: the members alive (black, against the most it
    /// held) and their mean fat reserve (orange, percent), from the
    /// members' own gauges while they were in it.
    fn family_curve(&self, family: &Family) -> String {
        const W: u64 = 600;
        const H: u64 = 60;
        let per_day = self.ticks_per_day();
        let mut at: BTreeMap<u64, (u64, u64)> = BTreeMap::new();
        for m in &family.members {
            let Some(person) = self.people.get(&m.who) else {
                continue;
            };
            let from = (m.from_day - 1) * per_day;
            let until = m.until.map_or(u64::MAX, |(day, _)| day * per_day);
            for g in person
                .gauges
                .iter()
                .filter(|g| (from..until).contains(&g.0))
            {
                let slot = at.entry(g.0).or_default();
                slot.0 += 1;
                slot.1 += u64::from(g.5);
            }
        }
        let most = at.values().map(|s| s.0).max().unwrap_or(1).max(1);
        let ticks = self.ticks.max(1);
        let at: Vec<(u64, (u64, u64))> = at.into_iter().collect();
        let line = |colour: &str, pick: &dyn Fn(&(u64, u64)) -> u64| {
            let points: Vec<String> = thinned(&at)
                .map(|(tick, slot)| {
                    format!("{},{}", tick * W / ticks, H - pick(slot).min(100) * H / 100)
                })
                .collect();
            format!(
                "<polyline fill=\"none\" stroke=\"{colour}\" stroke-width=\"1\" points=\"{}\"/>",
                points.join(" ")
            )
        };
        format!(
            "<svg width=\"{W}\" height=\"{H}\" viewBox=\"0 0 {W} {H}\">{}{}</svg><p><span style=\"color:#222\">■ members alive (of {most})</span> <span style=\"color:#d80\">■ mean fat reserve, %</span></p>",
            line("#222", &|s| s.0 * 100 / most),
            line("#d80", &|s| s.1 / s.0.max(1)),
        )
    }

    /// "camp A at (x, y), camp B at (x, y), 2443 m apart".
    fn camps_words(&self) -> String {
        let mut words: Vec<String> = self
            .camps
            .iter()
            .enumerate()
            .map(|(i, c)| {
                format!(
                    "camp {} at ({}, {})",
                    tribe_name(u8::try_from(i).expect("two at most")),
                    c.x,
                    c.y
                )
            })
            .collect();
        if let [a, b] = self.camps[..] {
            let (dx, dy) = (i64::from(a.x - b.x), i64::from(a.y - b.y));
            words.push(format!(
                "{} m apart as the crow flies",
                (dx * dx + dy * dy).isqrt()
            ));
        }
        words.join(", ")
    }

    /// Ticks a game day of this run.
    /// The season and year of `tick`, as "winter of year 2".
    fn season_of(&self, tick: u64) -> String {
        Calendar::new(
            self.settings.ticks_per_second,
            Calendar::DEFAULT_SECONDS_PER_DAY,
        )
        .map_or_else(
            |_| String::new(),
            |calendar| {
                let time = calendar.time_at(sim::Tick(tick));
                format!(
                    "{} of year {}",
                    format!("{:?}", time.season).to_lowercase(),
                    time.year
                )
            },
        )
    }

    /// How old a person is on `day` (from 0), in thousandths of a year.
    fn age_milli(person: &Person, day: i64) -> u64 {
        let days = u64::try_from(day - person.born_day).unwrap_or(0);
        days * 1000 / u64::from(Calendar::DEFAULT_DAYS_PER_YEAR)
    }

    /// The people by age and sex, at the start and the living at the end
    /// (lot 3 B).
    fn ages_table(&self) -> String {
        const BANDS: [(u64, u64, &str); 7] = [
            (0, 2, "0–1"),
            (2, 6, "2–5"),
            (6, 12, "6–11"),
            (12, 18, "12–17"),
            (18, 40, "18–39"),
            (40, 55, "40–54"),
            (55, u64::MAX, "55 and over"),
        ];
        if self.people.values().all(|p| p.sex.is_none()) {
            return String::new();
        }
        let end = i64::try_from(self.ticks / self.ticks_per_day().max(1)).unwrap_or(0);
        let count = |day: i64, living: bool, sex: Sex, (lo, hi): (u64, u64)| {
            self.people
                .values()
                .filter(|p| p.sex == Some(sex) && (!living || p.death.is_none()))
                .filter(|p| living || !p.born_in_run)
                .filter(|p| (lo..hi).contains(&(Self::age_milli(p, day) / 1000)))
                .count()
        };
        let mut h = String::from(
            "<h2>The ages</h2><p>The people by age and sex, in game years (50 days): at the start, and the living at the end.</p>\
             <table class=\"food\"><tr><th>Years</th><th>Women, start</th><th>Men, start</th><th>Women, end</th><th>Men, end</th></tr>",
        );
        for (lo, hi, label) in BANDS {
            let _ = write!(
                h,
                "<tr><td>{label}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                count(0, false, Sex::Female, (lo, hi)),
                count(0, false, Sex::Male, (lo, hi)),
                count(end, true, Sex::Female, (lo, hi)),
                count(end, true, Sex::Male, (lo, hi)),
            );
        }
        h.push_str("</table>");
        h
    }

    fn ticks_per_day(&self) -> u64 {
        u64::from(self.settings.ticks_per_second) * u64::from(Calendar::DEFAULT_SECONDS_PER_DAY)
    }

    /// The timing line: what changes from one machine or run to the next.
    // Display only, never simulation: a speed for the lead to read.
    #[allow(
        clippy::float_arithmetic,
        reason = "float maths of the simulation, the same result on every run"
    )]
    pub fn timing(&self) -> String {
        let days_per_year = f64::from(Calendar::DEFAULT_DAYS_PER_YEAR);
        let years = f64::from(self.settings.days) / days_per_year;
        let minutes = self.wall_s / 60.0;
        let speed = if minutes > 0.0 { years / minutes } else { 0.0 };
        format!(
            "<p id=\"timing\">The world was born in {:.2} s; the run took {:.2} s on the wall: <b>{speed:.1} game years a minute</b>. {} chunks' plants placed, {} of them ahead by {} helper thread{}.</p>",
            self.birth_s,
            self.wall_s,
            self.placed,
            self.placed_ahead,
            self.settings.helpers,
            if self.settings.helpers == 1 { "" } else { "s" }
        )
    }

    /// The whole page.
    pub fn html(&self) -> String {
        format!("{}{}</body></html>\n", self.fixed(), self.timing())
    }

    /// The four curves as one SVG: the living (against the people born),
    /// and the mean hunger, thirst and fatigue in points.
    fn curves(&self) -> String {
        const W: u64 = 900;
        const H: u64 = 220;
        let ticks = self.ticks.max(1);
        let tribes = u64::try_from(self.camps.len()).expect("small");
        let people = (u64::from(self.settings.people.max(1)) * tribes).max(
            self.samples
                .iter()
                .map(|s| u64::from(s.alive))
                .max()
                .unwrap_or(0),
        );
        let x = |tick: u64| tick * W / ticks;
        let line = |name: &str, colour: &str, values: &dyn Fn(&Sample) -> u64, top: u64| {
            let points: Vec<String> = thinned(&self.samples)
                .map(|s| format!("{},{}", x(s.tick), H - values(s) * H / top.max(1)))
                .collect();
            format!(
                "<polyline fill=\"none\" stroke=\"{colour}\" stroke-width=\"1.5\" points=\"{}\"><title>{name}</title></polyline>",
                points.join(" ")
            )
        };
        let step = usize::try_from(self.settings.days.max(1) / 10 + 1).expect("small");
        let mut days = String::new();
        for d in (0..=u64::from(self.settings.days)).step_by(step) {
            let xd = x(d * self.ticks_per_day());
            let _ = write!(
                days,
                "<line x1=\"{xd}\" y1=\"0\" x2=\"{xd}\" y2=\"{H}\" stroke=\"#eee\"/><text x=\"{xd}\" y=\"{}\" font-size=\"10\" fill=\"#666\">day {d}</text>",
                H + 12
            );
        }
        format!(
            "<svg width=\"{W}\" height=\"{}\" viewBox=\"0 0 {W} {}\" style=\"font-family:system-ui\">{days}{}{}{}{}{}</svg>\
<p><span style=\"color:#222\">■ alive (of {people})</span>{} <span style=\"color:#c33\">■ hunger</span> <span style=\"color:#d80\">■ fat reserve, tribe A</span>{} <span style=\"color:#36c\">■ thirst</span> <span style=\"color:#993\">■ fatigue</span>, the gauges in points of 100.</p>",
            H + 16,
            H + 16,
            line("alive", "#222", &|s| u64::from(s.alive), people),
            if tribes == 2 {
                line(
                    "alive, tribe A",
                    "#a55",
                    &|s| u64::from(s.alive_by_tribe[0]),
                    people,
                ) + &line(
                    "alive, tribe B",
                    "#5a5",
                    &|s| u64::from(s.alive_by_tribe[1]),
                    people,
                )
            } else {
                String::new()
            },
            line("hunger", "#c33", &|s| u64::from(s.hunger), 100)
                + &line(
                    "fat reserve, tribe A",
                    "#d80",
                    &|s| u64::from(s.reserve_by_tribe[0]),
                    100,
                )
                + &if tribes == 2 {
                    line(
                        "fat reserve, tribe B",
                        "#a6d",
                        &|s| u64::from(s.reserve_by_tribe[1]),
                        100,
                    )
                } else {
                    String::new()
                },
            line("thirst", "#36c", &|s| u64::from(s.thirst), 100),
            line("fatigue", "#993", &|s| u64::from(s.fatigue), 100),
            if tribes == 2 {
                " <span style=\"color:#a55\">■ alive, tribe A</span> <span style=\"color:#5a5\">■ alive, tribe B</span>"
            } else {
                ""
            },
            if tribes == 2 {
                " <span style=\"color:#a6d\">■ fat reserve, tribe B</span>"
            } else {
                ""
            },
        )
    }
}

/// A person's gauges over the run as a small SVG: vitality black,
/// hunger red, thirst blue, fatigue olive, fat reserve orange.
fn sparkline(gauges: &[Gauges], ticks: u64) -> String {
    const W: u64 = 600;
    const H: u64 = 60;
    let ticks = ticks.max(1);
    let line = |colour: &str, pick: &dyn Fn(&Gauges) -> u32| {
        let points: Vec<String> = thinned(gauges)
            .map(|g| format!("{},{}", g.0 * W / ticks, H - u64::from(pick(g)) * H / 100))
            .collect();
        format!(
            "<polyline fill=\"none\" stroke=\"{colour}\" stroke-width=\"1\" points=\"{}\"/>",
            points.join(" ")
        )
    };
    format!(
        "<svg width=\"{W}\" height=\"{H}\" viewBox=\"0 0 {W} {H}\">{}{}{}{}{}</svg>",
        line("#d80", &|g| g.5),
        line("#222", &|g| g.1),
        line("#c33", &|g| g.2),
        line("#36c", &|g| g.3),
        line("#993", &|g| g.4),
    )
}

/// `text` safe inside HTML.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use super::*;
    use serde_json::json;

    /// The content, read once from the repository's `content/`.
    fn content() -> &'static Content {
        static CONTENT: OnceLock<Content> = OnceLock::new();
        CONTENT.get_or_init(|| {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
            load_content(&dir).expect("the repository's content reads and validates")
        })
    }

    fn small() -> Settings {
        Settings {
            world: WorldKind::Patch { side: 50 },
            people: 5,
            days: 2,
            ticks_per_second: 1,
            seed: 1,
            content: None,
            brain: true,
            tribes: 1,
            children: 0,
            infants: 0,
            adult_years: (18, 40),
            fed: false,
            trials: true,
            continents: None,
            helpers: 1,
        }
    }

    /// The run as a scenario test sees it: the world's invariants
    /// checked at every dawn ([`sim::invariants::breaches`]), no food
    /// lost outside the flows, and every dead person's thoughts kept.
    fn run_checked(settings: &Settings, content: &Content) -> Report {
        let report = run_watched(settings, content, &mut |world, day| {
            let found = sim::invariants::breaches(world, content);
            assert!(found.is_empty(), "day {day}: {found:?}");
        })
        .unwrap();
        for (&(tribe, year), balance) in &report.food {
            assert!(
                balance.lost_j() >= 0,
                "tribe {tribe} year {year}: food came from nowhere, lost {} J",
                balance.lost_j()
            );
        }
        if settings.brain {
            for (id, person) in &report.people {
                assert!(
                    person.death.is_none() || person.thoughts > 0,
                    "person {} died and its page says it never thought",
                    id.0
                );
            }
        }
        report
    }

    /// A person's deaths as "name, day N, of cause", in id order.
    fn deaths(report: &Report) -> Vec<String> {
        report
            .people
            .iter()
            .filter_map(|(id, person)| {
                person.death.map(|(tick, cause, _)| {
                    format!(
                        "{}, day {}, {cause:?}",
                        name(*id),
                        tick / report.ticks_per_day().max(1) + 1
                    )
                })
            })
            .collect()
    }

    /// Gathers of a few grams rounded down to the kilojoule, eaten as one
    /// stack rounded down once, left "lost" negative in the audit's runs
    /// (−1,068 kJ over 158,891 gathers of about 9 g). Counted in joules,
    /// the flows add up exactly.
    #[test]
    fn the_balance_counts_in_joules_so_rounding_never_loses_food() {
        let content = content();
        let (material, kj_per_kg) = content
            .materials
            .iter()
            .find_map(|m| m.food.map(|f| (m.id.clone(), f.energy_kj_per_kg)))
            .expect("a food");
        let mut balance = Balance::default();
        for _ in 0..3 {
            balance.gathered(content, &material, 9);
        }
        balance.eaten(content, &material, 27);
        assert_eq!(balance.gathered_j, 27 * u64::from(kj_per_kg));
        assert_eq!(balance.lost_j(), 0, "{balance:?}");
    }

    /// On the patch in spring little grows, and since lot 3 a body
    /// without food dies when its fat reserve is spent: its page must
    /// still say what it thought, read before its mind goes with it. The
    /// reserve is set short here (60,000 kJ, six days, the value before 22
    /// November 2026: with 300 MJ nobody starves in a month on the patch),
    /// so that somebody dies in eight days: the rule under test is the
    /// page, not the reserve.
    #[test]
    fn the_dead_keep_what_they_thought() {
        let settings = Settings { days: 8, ..small() };
        let mut content = content().clone();
        content.needs.reserve_kj = 60_000;
        let report = run_checked(&settings, &content);
        let dead: Vec<_> = report
            .people
            .values()
            .filter(|p| p.death.is_some())
            .collect();
        assert!(
            !dead.is_empty(),
            "somebody starves on the bare patch in eight days"
        );
        for person in dead {
            assert!(person.thoughts > 0, "{:?}", person.death);
        }
    }

    /// `05`'s D47 trial (lot 3 B, 17 November 2026): two tribes of five
    /// grown, three children and an infant, "all eighteen live". Run on
    /// 20 November 2026 (audit F4, 30 days, cadence 1, release, 32 s):
    /// **two die**, person 10 of tribe B of hunger on day 19 and person
    /// 18, the infant she carried, of thirst on day 21. The whole tribe
    /// was under-fed (4,100 kJ a person-day gathered against some 8,000
    /// of need): the brains walked from crumb to crumb, ranking a plant
    /// by what an hour of a full one gives, and a hungry body never
    /// slept. **Holds again since 21 November 2026** (the point zero of
    /// the sim phase): nobody dies, 18 alive, the run in 15 s instead of
    /// 34, and it says who died and when if it fails.
    #[test]
    #[ignore = "the D47 trial, 30 days on the vivarium's world, some 15 s in release: cargo test --release -p vivarium -- --ignored the_d47_trial"]
    fn the_d47_trial_two_tribes_with_children_and_infants_live_thirty_days() {
        let settings = Settings {
            world: WorldKind::Vivarium,
            tribes: 2,
            children: 3,
            infants: 1,
            days: 30,
            ..small()
        };
        let report = run_checked(&settings, content());
        let deaths = deaths(&report);
        for death in &deaths {
            eprintln!("{death}");
        }
        assert!(
            deaths.is_empty(),
            "all eighteen live (05, lot 3 B): {deaths:?}"
        );
    }

    /// The vivarium's continent kept on disk and read back gives the very
    /// pages the born one gives (D28 amended 19 November 2026; lightening
    /// 4's S7): the report's data, timings apart, is the same born, kept
    /// and read, or born with no cache at all.
    #[test]
    #[ignore = "the vivarium's continent born twice, some 20 s in release: cargo test --release -p vivarium -- --ignored the_kept_continent"]
    fn the_kept_continent_gives_the_pages_the_born_one_gives() {
        let dir = std::env::temp_dir().join(format!(
            "soul-vivarium-kept-continent-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let settings = Settings {
            world: WorldKind::Vivarium,
            tribes: 2,
            days: 2,
            continents: Some(dir.clone()),
            ..small()
        };
        let data = |report: &Report| {
            let mut value = report.json_value();
            value
                .as_object_mut()
                .and_then(|o| o.get_mut("meta"))
                .and_then(|m| m.as_object_mut())
                .map(|m| m.remove("timing"));
            value
        };
        let born = run_checked(&settings, content());
        assert!(
            continents::path_of(&dir, sim::plan::VIVARIUM_SEED, sim::plan::CONTINENT_SIDE)
                .is_file(),
            "the born continent is kept"
        );
        let kept = run_checked(&settings, content());
        let apart = run_checked(
            &Settings {
                continents: None,
                ..settings
            },
            content(),
        );
        assert_eq!(data(&kept), data(&born), "kept and read: the same data");
        assert_eq!(data(&apart), data(&born), "no cache: the same data");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// D24: the cadence (ticks a game second) must not change the outcome.
    /// It does (audit of 20 November 2026, F2): the needs and the gathering
    /// are integrated by the game day and hour, but every thought becomes
    /// an intention on the next tick, a refused act costs a whole tick,
    /// and a walk drops its leftover budget on the tick it ends
    /// (`sim::stride`), so a tick is time a body loses, and a game second
    /// is one tick at cadence 1 and a tenth at cadence 10. Measured on
    /// the D47 trial, 30 days, release: cadence 1, 2 dead (person 10,
    /// hunger, day 19; person 18, thirst, day 21), tribes A and B gathered
    /// 2,123,166 and 1,960,304 kJ; cadence 10, 0 dead, 2,135,674 and
    /// 2,229,475 kJ. The audit's 68 people over 14
    /// days: 53 dead at cadence 1, 38 at cadence 10; 3,765 against 4,827
    /// kJ a person-day. On 21 November 2026, the point zero regained:
    /// nobody dies at either cadence, and the tribes gather 2,333,626 and
    /// 2,303,091 kJ at cadence 1 against 2,342,799 and 2,198,479 at 10:
    /// the gap is the food alone. Whether the vivarium judges at the game's cadence
    /// (10, about 1.3× the computing time) or an act is to cost game
    /// time rather than ticks is the lead's call; until then this test
    /// fails and shows the gap.
    #[test]
    #[ignore = "two 30-day runs on the vivarium's world, some 75 s in release; fails today (F2): cargo test --release -p vivarium -- --ignored the_cadence"]
    fn the_cadence_does_not_change_who_lives_nor_what_they_gather() {
        let trial = |ticks_per_second: u32| {
            let settings = Settings {
                world: WorldKind::Vivarium,
                tribes: 2,
                children: 3,
                infants: 1,
                days: 30,
                ticks_per_second,
                ..small()
            };
            let report = run_checked(&settings, content());
            let gathered: Vec<u64> = report.food.values().map(|b| b.gathered_j / 1000).collect();
            (deaths(&report), gathered)
        };
        let slow = trial(1);
        let game = trial(10);
        eprintln!("cadence 1: {:?}, gathered {:?} kJ", slow.0, slow.1);
        eprintln!("cadence 10: {:?}, gathered {:?} kJ", game.0, game.1);
        assert_eq!(slow.0, game.0, "the same deaths at both cadences");
        assert_eq!(slow.1, game.1, "the same food gathered at both cadences");
    }

    /// The content's trials forced (D50): a day of distress is enough, the
    /// chances are whole, a trial lasts two days.
    fn forced_trials() -> Content {
        let mut content = content().clone();
        content.needs.trials = Trials {
            trial_after_days: 1,
            trial_days: 2,
            try_percent_by_will: vec![(0, 100)],
            far_home_from_will: 70,
            new_home_walk_tiles: 10,
            imitate_tiles: 12,
            imitate_percent: 100,
            imitate_loss_percent: 50,
        };
        content
    }

    /// D50 in the report: every trial, judgement and copy is a line of the
    /// chronicle and of the person's page, the person's page tells its
    /// will and its ways, and the tribe's year counts them; the control
    /// run (`--no-trials`) counts and tells nothing.
    #[test]
    fn the_trials_are_told_on_the_pages_and_counted_by_the_year() {
        let settings = Settings { days: 6, ..small() };
        let report = run_checked(&settings, &forced_trials());
        let started = report
            .trials
            .iter()
            .filter(|t| t.told == Told::Trying)
            .count();
        assert!(started > 0, "hungry on the patch, somebody tries");
        let judged = report
            .trials
            .iter()
            .filter(|t| matches!(t.told, Told::Tried(_)))
            .count();
        assert!(
            judged > 0,
            "and is judged within six days: {:?}",
            report.trials
        );
        let year = &report.years()[0];
        assert_eq!(usize::try_from(year.trials).unwrap(), started);
        assert!(year.kept <= year.trials);
        assert!(year.ways.minds > 0 && year.ways.camp.is_some());
        let homes = report
            .trials
            .iter()
            .filter(|t| {
                t.way == Way::Home && t.told != Told::Tried(true) && t.told != Told::Tried(false)
            })
            .count();
        assert_eq!(usize::try_from(year.homes_moved).unwrap(), homes);
        let html = report.fixed();
        assert!(
            html.contains(" tries a way: "),
            "the chronicle names each trial"
        );
        assert!(
            html.contains(" keeps its way: ") || html.contains(" gives up its way: "),
            "and each judgement"
        );
        assert!(html.contains("<th>Trials</th><th>Kept</th><th>Copied</th><th>Homes moved</th>"));
        assert!(html.contains("<b>The trials</b> (D50): on"));
        for (id, person) in &report.people {
            assert!(
                person.will.is_some(),
                "person {}: its will on its page",
                id.0
            );
            assert!(person.ways.is_some());
            let page = report.person_html(*id, person);
            assert!(page.contains("; will ") && page.contains("Its ways (D50) at the end: "));
            for trial in &person.trials {
                let words = Report::trial_words(trial);
                assert!(
                    words.starts_with(&format!("day {}, ", trial.day)),
                    "{words}"
                );
                assert!(page.contains(&escape(&words)), "{words}");
                assert!(
                    person.entries.iter().any(|e| e.what.contains(&trial.step)),
                    "its own line: {trial:?}"
                );
            }
        }
        let twice = report
            .people
            .values()
            .flat_map(|p| p.trials.iter())
            .find(|t| t.way == Way::WorthHalfTiles)
            .map(|t| t.step.clone());
        if let Some(step) = twice {
            assert!(
                step.contains("searches twice as wide") || step.contains("searches half as wide"),
                "{step}"
            );
        }
    }

    /// The control run (`--no-trials`, D50): nothing happens, nothing is
    /// counted, and the page says so.
    #[test]
    fn the_control_run_tries_and_copies_nothing() {
        let settings = Settings {
            days: 6,
            trials: false,
            ..small()
        };
        let control = run_checked(&settings, &forced_trials());
        assert!(control.trials.is_empty(), "{:?}", control.trials);
        let year = &control.years()[0];
        assert_eq!(
            (year.trials, year.kept, year.copied, year.homes_moved),
            (0, 0, 0, 0)
        );
        assert!(
            control
                .people
                .values()
                .all(|p| p.trials.is_empty() && p.copied == 0)
        );
        assert!(
            control
                .people
                .values()
                .all(|p| p.ways == Some(Ways::at(control.camps[0])))
        );
        let html = control.fixed();
        assert!(!html.contains(" tries a way: "));
        assert!(html.contains("<b>The trials</b> (D50): off"));
    }

    /// D50's tiny first (the brief of 22 November 2026): one band of 25
    /// (15 grown, 7 children, 3 infants) at camp A of the vivarium's
    /// world, five years, cadence 1, seed 1, with and without trials. With
    /// trials at least one trial is kept and one way copied, the band is
    /// not extinct at five years, and two runs with the same seed give the
    /// same trials (D24). The living of both runs are numbers written on
    /// the page and in `docs/05`, not an assertion: the project lead
    /// expects more deaths at first and fewer after ("au début plus de
    /// morts, après moins de morts… et on grandit", 22 November 2026), and
    /// nothing is tuned here. Measured on 22 November 2026 (release, seed
    /// 1): with trials 14 alive and 17 dead at five years (49 trials, 27
    /// kept, 114 copied, 6 homes moved), without 19 alive and 12 dead.
    #[test]
    #[ignore = "three 5-year runs on the vivarium's world, some 5 min in release: cargo test --release -p vivarium -- --ignored the_d50_band"]
    fn the_d50_band_of_25_five_years_with_and_without_trials() {
        let band = |trials: bool| {
            let settings = Settings {
                world: WorldKind::Vivarium,
                tribes: 1,
                people: 15,
                children: 7,
                infants: 3,
                days: 250,
                trials,
                ..small()
            };
            // The world's invariants at every dawn; not the food balance:
            // the living eat the food the dead left on the ground, which
            // the balance counts as come from nowhere the year after (a
            // limit of the balance, not of D50).
            let content = content();
            let report = run_watched(&settings, content, &mut |world, day| {
                let found = sim::invariants::breaches(world, content);
                assert!(found.is_empty(), "day {day}: {found:?}");
            })
            .unwrap();
            let years = report.years();
            let alive = report.samples.last().map_or(0, |s| s.alive);
            let (trials, kept, copied, homes) = years.iter().fold((0, 0, 0, 0), |a, y| {
                (
                    a.0 + y.trials,
                    a.1 + y.kept,
                    a.2 + y.copied,
                    a.3 + y.homes_moved,
                )
            });
            eprintln!(
                "trials {}: {alive} alive at five years, {} dead ({}); {trials} trials, {kept} kept, {copied} copied, {homes} homes moved; camp {:?}; {:.0} s",
                if settings.trials { "on" } else { "off" },
                deaths(&report).len(),
                deaths(&report).join("; "),
                years.last().and_then(|y| y.ways.camp),
                report.wall_s
            );
            (alive, kept, copied, report.trials)
        };
        let (alive, kept, copied, trials) = band(true);
        let (alive_without, ..) = band(false);
        eprintln!("alive at five years: {alive} with trials, {alive_without} without");
        assert!(kept > 0, "at least one trial kept in five years");
        assert!(copied > 0, "at least one way copied in five years");
        assert!(alive > 0, "the band is not extinct at five years");
        let again = band(true);
        assert_eq!(trials, again.3, "the same seed gives the same trials");
        assert_eq!(alive, again.0);
    }

    /// The departure paired (lot 3 C): the grown are coupled on day one by
    /// the sim's own rule, every child has a mother and a father among the
    /// camp's couples, an infant's nurse is its mother, and the families
    /// follow: the woman heads, her husband and her children are in hers.
    #[test]
    fn the_departure_pairs_the_grown_and_gives_the_young_parents() {
        let settings = Settings {
            people: 4,
            children: 2,
            infants: 1,
            days: 1,
            ..small()
        };
        let report = run_checked(&settings, content());
        assert_eq!(report.people.len(), 7);
        let spouses = report
            .chronicle
            .iter()
            .filter(|e| e.day == 1 && e.what.contains("become spouses"))
            .count();
        assert_eq!(spouses, 2, "two couples of the four grown");
        let grown: Vec<_> = report
            .people
            .values()
            .filter(|p| !p.born_in_run && Report::years_on(p, 1) >= 16)
            .collect();
        assert_eq!(grown.len(), 4);
        for person in &grown {
            assert_eq!(person.couplings.len(), 1, "{person:?}");
            assert_eq!(person.couplings[0].0, 1);
        }
        for (id, child) in report
            .people
            .iter()
            .filter(|(_, p)| Report::years_on(p, 1) < 16)
        {
            let mother = child.mother.expect("a mother");
            let father = child.father.expect("a father");
            let she = &report.people[&mother];
            assert_eq!(she.sex, Some(Sex::Female));
            assert_eq!(
                she.couplings.first().map(|c| c.1),
                Some(father),
                "the father is her spouse"
            );
            assert!(she.children.contains(id) && report.people[&father].children.contains(id));
            if let Some(nurse) = child.carrier {
                assert_eq!(nurse, mother, "the nurse is the mother");
            }
        }
        let families = report.families();
        assert_eq!(families.len(), 2, "two couples, two families: {families:?}");
        for (head, family) in &families {
            assert_eq!(
                report.people[head].sex,
                Some(Sex::Female),
                "named by the mother"
            );
            let husband = report.people[head].couplings[0].1;
            assert!(
                family
                    .members
                    .iter()
                    .any(|m| m.who == husband && m.until.is_none())
            );
            for m in &family.members {
                let p = &report.people[&m.who];
                assert!(
                    m.who == *head || m.who == husband || p.mother == Some(*head),
                    "{m:?} in the family of {head:?}"
                );
            }
        }
        let html = report.fixed();
        assert!(
            html.contains("<h2>The families</h2>")
                && html.contains("<b>The family of person 1</b>")
        );
        assert!(html.contains("is the child of") && html.contains("become spouses"));
        assert!(html.contains("<h2>The tribes, year by year</h2>"));
    }

    /// The registry (lot 3 C): the families and the years are worked out
    /// from the people. A child is in its mother's family from its birth to
    /// its death or its sixteenth year, then in its own; a man joins his
    /// wife's when he marries; the year counts births, deaths by cause, the
    /// interval between births and the fertile women without a spouse.
    fn peopled_report() -> (Report, BodyId, BodyId) {
        let mut report = run_with(&small(), content()).unwrap();
        let per_day = report.ticks_per_day();
        let (mother, father) = (BodyId(1), report.people[&BodyId(1)].couplings[0].1);
        assert_eq!(report.people[&mother].sex, Some(Sex::Female));
        let child = |born_day: i64, sex: Sex, born_in_run: bool| Person {
            tribe: 0,
            sex: Some(sex),
            born_day,
            mother: Some(mother),
            father: Some(father),
            born_in_run,
            ..Person::default()
        };
        // Born on day 1, dies of illness on day 2.
        let mut first = child(0, Sex::Male, true);
        first.death = Some((per_day + 10, Death::Illness, Pos::new(1, 1)));
        report.people.insert(BodyId(100), first);
        // Born on day 2, alive.
        report
            .people
            .insert(BodyId(101), child(1, Sex::Female, true));
        // Turns sixteen on day 2.
        report.people.insert(
            BodyId(102),
            child(
                1 - 16 * i64::from(Calendar::DEFAULT_DAYS_PER_YEAR),
                Sex::Male,
                false,
            ),
        );
        report.people.get_mut(&mother).unwrap().children =
            vec![BodyId(100), BodyId(101), BodyId(102)];
        // A single woman marries a man on day 2.
        let single = *report
            .people
            .iter()
            .find(|(_, p)| p.sex == Some(Sex::Female) && p.couplings.is_empty())
            .map(|(id, _)| id)
            .expect("five people, three women, one unwed");
        let mut groom = Person {
            tribe: 0,
            sex: Some(Sex::Male),
            born_day: -30 * i64::from(Calendar::DEFAULT_DAYS_PER_YEAR),
            couplings: vec![(2, single)],
            ..Person::default()
        };
        groom.entries.clear();
        report.people.insert(BodyId(103), groom);
        report
            .people
            .get_mut(&single)
            .unwrap()
            .couplings
            .push((2, BodyId(103)));
        (report, mother, single)
    }

    #[test]
    fn the_families_follow_the_people() {
        let (report, mother, single) = peopled_report();
        let families = report.families();
        let hers = &families[&mother];
        let of = |who: u32| {
            hers.members
                .iter()
                .find(|m| m.who == BodyId(who))
                .copied()
                .unwrap_or_else(|| panic!("person {who} in the family of the mother"))
        };
        assert_eq!(of(100).arrived, Arrival::Born);
        assert_eq!(of(100).until, Some((2, Leaving::Died)));
        assert_eq!((of(101).from_day, of(101).until), (2, None));
        assert_eq!(of(102).until, Some((2, Leaving::Grown)));
        assert_eq!(
            families[&BodyId(102)].members,
            vec![Membership {
                who: BodyId(102),
                from_day: 2,
                arrived: Arrival::Grown,
                until: None
            }]
        );
        let wife = &families[&single];
        assert_eq!(wife.members.len(), 2, "{wife:?}");
        assert!(wife.members.iter().any(|m| m.who == BodyId(103)
            && m.from_day == 2
            && m.arrived == Arrival::Married(single)));
        assert_eq!(
            families[&BodyId(103)].members[0].until,
            Some((2, Leaving::Married(single))),
            "a line in the old family"
        );
        let words: Vec<&str> = hers.lines.iter().map(|e| e.what.as_str()).collect();
        assert!(
            words
                .iter()
                .any(|w| w.starts_with("person 100, a boy, is born to person 1 and"))
        );
        assert!(
            words
                .iter()
                .any(|w| w.starts_with("person 100 dies of illness"))
        );
        assert!(words.contains(&"person 102 turns 16 and leaves for a family of its own"));
        assert!(
            families[&BodyId(103)]
                .lines
                .iter()
                .any(|e| e.what.contains("leaves for the family of"))
        );
    }

    #[test]
    fn the_years_count_births_deaths_intervals_and_lone_women() {
        let (mut report, _, single) = peopled_report();
        let years = report.years();
        assert_eq!(years.len(), 1);
        let y = &years[0];
        assert_eq!(
            (y.population_before, y.population),
            (5 + 1 + 1 + 1, 5 + 1 + 1 + 1)
        );
        assert_eq!(y.births, 2);
        assert_eq!(y.deaths, [1, 0, 0, 0, 0, 0]);
        assert_eq!(
            y.intervals_days,
            vec![1],
            "the second child a day after the first"
        );
        assert_eq!(y.fifteen, (0, 0), "nobody is fifteen yet");
        // Three women of the fertile years; the single one married on day 2.
        assert_eq!(y.fertile, (0, 3));
        report.people.get_mut(&single).unwrap().couplings.clear();
        report.people.remove(&BodyId(103));
        assert_eq!(
            report.years()[0].fertile,
            (0, 3),
            "the boy of sixteen, no kin of hers, is a possible spouse"
        );
        report.people.remove(&BodyId(102));
        assert_eq!(
            report.years()[0].fertile,
            (1, 3),
            "no unwed man left for her"
        );
        let html = report.fixed();
        assert!(
            html.contains("+60.0 %") || html.contains("+0.0 %"),
            "{}",
            &html[html.find("The tribes").unwrap()..][..600]
        );
    }

    /// Lot 3 C's first trial (the proposal's section 4.1): a couple of
    /// twenty alone on the patch, food without limit, twenty years. Births
    /// some three years apart; the children weaned at two, who walk; deaths
    /// of the curve alone (illness, old age, childbirth), none of want;
    /// the couple's children never pairing with each other; the family
    /// neither dying out nor exploding. Prints the numbers for `docs/05`;
    /// writes the page to `$VIVARIUM_OUT` when set.
    #[test]
    #[ignore = "twenty years at cadence 1, some 50 s in release: VIVARIUM_OUT=page.html cargo test --release -p vivarium -- --ignored one_couple"]
    fn one_couple_alone_and_fed_twenty_years() {
        let settings = Settings {
            people: 2,
            adult_years: (20, 20),
            fed: true,
            days: 20 * Calendar::DEFAULT_DAYS_PER_YEAR,
            ..small()
        };
        let report = run_checked(&settings, content());
        if let Ok(out) = std::env::var("VIVARIUM_OUT") {
            std::fs::write(&out, report.html()).expect("the page is written");
        }
        let (wife, husband) = (BodyId(1), BodyId(2));
        assert_eq!(
            report.people[&wife].couplings.first().map(|c| c.1),
            Some(husband)
        );
        let per_year = u64::from(Calendar::DEFAULT_DAYS_PER_YEAR);
        let children: Vec<(BodyId, &Person)> = report
            .people
            .iter()
            .filter(|(_, p)| p.born_in_run)
            .map(|(id, p)| (*id, p))
            .collect();
        assert!(
            children.len() >= 3,
            "births in twenty years: {}",
            children.len()
        );
        for (_, child) in &children {
            assert_eq!((child.mother, child.father), (Some(wife), Some(husband)));
        }
        let mut intervals = Vec::new();
        for pair in children.windows(2) {
            let (earlier, later) = (pair[0].1, pair[1].1);
            let gap = Report::birth_day(later) - Report::birth_day(earlier);
            intervals.push(gap);
            let earlier_lived_to_two = report
                .death_day(earlier)
                .is_none_or(|d| d >= Report::birthday(earlier, 2));
            if earlier_lived_to_two {
                assert!(gap * 10 >= 25 * per_year, "{gap} days between two births");
            }
        }
        let mean_tenths = intervals.iter().sum::<u64>() * 10 / (per_year * intervals.len() as u64);
        assert!(
            (20..=50).contains(&mean_tenths),
            "mean interval {mean_tenths} tenths of a year"
        );
        let weaned = weaned_at_two(&report, &children);
        let deaths = deaths_of_the_curve(&report);
        for (id, p) in &report.people {
            for &(day, spouse) in p.couplings.iter().filter(|(day, _)| *day > 1) {
                let other = &report.people[&spouse];
                assert!(
                    !Report::kin((*id, p), (spouse, other)),
                    "person {} and person {} are kin and became spouses on day {day}",
                    id.0,
                    spouse.0
                );
            }
        }
        let alive = report.samples.last().map_or(0, |s| s.alive);
        assert!((1..=30).contains(&alive), "{alive} alive at the end");
        eprintln!(
            "trial 1: {} births, intervals {intervals:?} days (mean {}.{} years), {weaned} weaned at two, {alive} alive at the end, deaths: {deaths:?}, {:.1} s on the wall",
            children.len(),
            mean_tenths / 10,
            mean_tenths % 10,
            report.wall_s
        );
    }

    /// How many of `children` were weaned at two, each checked: within a
    /// day of its second birthday, and walking and thinking after it.
    fn weaned_at_two(report: &Report, children: &[(BodyId, &Person)]) -> usize {
        let mut weaned = 0;
        for (_, child) in children {
            let two = Report::birthday(child, 2);
            if two + 1 > report.last_day() || report.death_day(child).is_some_and(|d| d <= two) {
                continue;
            }
            let day = child
                .entries
                .iter()
                .find(|e| e.what.contains("is weaned"))
                .map(|e| e.day)
                .expect("weaned at two");
            assert!(
                day.abs_diff(two) <= 1,
                "weaned on day {day}, two on day {two}"
            );
            assert!(
                child.thoughts > 0 && child.entries.iter().any(|e| e.what.contains("walks to")),
                "a weaned child walks"
            );
            weaned += 1;
        }
        weaned
    }

    /// Every death in words, each checked to be of the curve (illness, old
    /// age, childbirth): with food without limit nobody dies of want.
    fn deaths_of_the_curve(report: &Report) -> Vec<String> {
        let mut deaths = Vec::new();
        for (id, p) in &report.people {
            if let Some((tick, cause, _)) = p.death {
                assert!(
                    matches!(cause, Death::Illness | Death::OldAge | Death::Childbirth),
                    "person {} died {cause:?} with food without limit",
                    id.0
                );
                deaths.push(format!(
                    "person {} {} on day {} at {}",
                    id.0,
                    cause_words(cause),
                    report.day_of(tick),
                    Report::years_on(p, report.day_of(tick))
                ));
            }
        }
        deaths
    }

    #[test]
    fn the_content_is_found_from_the_working_directory_or_given() {
        let given = Settings {
            content: Some(PathBuf::from("/nowhere/content")),
            ..small()
        };
        assert_eq!(
            content_dir(&given).unwrap(),
            PathBuf::from("/nowhere/content")
        );
        assert!(matches!(
            load_content(Path::new("/nowhere/content")),
            Err(RunError::Read(path, _)) if path.ends_with("materials.ron")
        ));
    }

    #[test]
    fn the_same_settings_give_the_same_report_byte_for_byte() {
        let a = run_with(&small(), content()).unwrap();
        let b = run_with(&small(), content()).unwrap();
        assert_eq!(a.fixed(), b.fixed());
        assert_eq!(a.chronicle, b.chronicle);
        assert_eq!(a.samples, b.samples);
        assert_eq!(a.map_png, b.map_png);
        assert!(a.html().contains("game years a minute"));
    }

    #[test]
    fn the_people_are_born_at_the_camp_and_counted_once_an_hour() {
        let report = run_with(&small(), content()).unwrap();
        assert_eq!(report.people.len(), 5, "five people, five pages");
        assert_eq!(
            report.ticks,
            2 * 7200,
            "two days of 7,200 ticks at one tick a second"
        );
        // Tick 0 and then 24 times a day: 48 in two days.
        assert_eq!(report.samples.len(), 1 + 48);
        assert_eq!(report.samples[0].alive, 5);
        let births = report
            .chronicle
            .iter()
            .filter(|e| e.what.contains("is born at"))
            .count();
        assert_eq!(births, 5);
        for person in report.people.values() {
            let at = person.born_at.expect("born somewhere");
            assert!(
                (at.x - report.camps[0].x).abs() <= 3 && (at.y - report.camps[0].y).abs() <= 3,
                "born by the camp: {at:?} against {:?}",
                report.camps[0]
            );
            let expected = match person.death {
                None => 1 + 48,
                Some((tick, ..)) => usize::try_from(tick / 300).expect("small") + 1,
            };
            assert!(
                person.gauges.len() <= 1 + 48 && person.gauges.len() + 1 >= expected,
                "its gauges 24 times a day while it lives: {} for {:?}",
                person.gauges.len(),
                person.death
            );
        }
        assert!(
            report.chronicle.iter().any(|e| e.what == "day 2 begins"),
            "the days are in the chronicle"
        );
    }

    #[test]
    fn every_death_has_its_cause_in_the_chronicle_and_on_the_page() {
        // Standing still with no brain, nobody drinks and nobody shelters:
        // the people die within the run, of thirst (D34: mortal in two to
        // three days) or of the nights' cold on the way there.
        let settings = Settings {
            days: 6,
            brain: false,
            ..small()
        };
        let report = run_with(&settings, content()).unwrap();
        let dead: Vec<_> = report
            .people
            .values()
            .filter(|p| p.death.is_some())
            .collect();
        assert_eq!(dead.len(), 5, "nobody drinks without a brain: all five die");
        for person in dead {
            let (tick, cause, _) = person.death.expect("dead");
            assert!(
                matches!(cause, Death::Dehydrated | Death::Froze),
                "no food is counted yet (hunger a day is 0): {cause:?}"
            );
            assert!(
                person
                    .entries
                    .iter()
                    .any(|e| e.tick == tick && e.what.contains("dies of"))
            );
        }
        let lines = report
            .chronicle
            .iter()
            .filter(|e| e.what.contains("dies of"))
            .count();
        assert_eq!(lines, 5);
        assert_eq!(report.samples.last().map(|s| s.alive), Some(0));
        let html = report.fixed();
        assert!(html.contains("<h2>The map</h2>") && html.contains("data:image/png;base64,"));
        assert!(html.contains("<polyline") && html.contains("<h2>The chronicle</h2>"));
        assert!(html.contains(
            "<details><summary>person 1 (tribe A, a woman of 18 at the start) — died on day"
        ));
        assert!(html.contains("none (the control run)"));
        assert!(
            html.contains("thirst a day 100"),
            "the needs' rates are named with the factors"
        );
    }

    #[test]
    fn the_picture_is_the_map_at_most_512_px_a_side() {
        let report = run_with(&small(), content()).unwrap();
        assert_eq!(report.map_scale, 1);
        assert_eq!(&report.map_png[16..24], &[0, 0, 0, 50, 0, 0, 0, 50]);
    }

    #[test]
    fn with_the_brain_nobody_dies_of_thirst_beside_the_river() {
        // The plan's small trial: five people, the 50 × 50 patch, ten days.
        // Thirst alone: the patch bears nothing in spring (and hunger is
        // off here, so the fat reserve, 300 MJ since 22 November 2026, is
        // not even drawn on).
        let settings = Settings {
            days: 10,
            ..small()
        };
        let mut content = content().clone();
        content.needs.hunger_per_day = 0;
        let report = run_checked(&settings, &content);
        for (id, person) in &report.people {
            assert!(
                !matches!(person.death, Some((_, Death::Dehydrated, _))),
                "person {} died of thirst beside the river",
                id.0
            );
            assert!(
                person.drinks >= 10,
                "person {} drank {} times in ten days",
                id.0,
                person.drinks
            );
            assert!(
                person.thoughts > 0
                    && person
                        .entries
                        .iter()
                        .any(|e| e.what.starts_with("thirsty: "))
            );
        }
        assert!(report.fixed().contains("the minimal one (sim::brain)"));
    }

    #[test]
    fn two_tribes_are_born_at_their_two_camps_and_live_there() {
        // The control run (D50): with trials, a body in distress on the
        // bare patch may set its home elsewhere by the fourth day.
        let settings = Settings {
            tribes: 2,
            days: 4,
            trials: false,
            ..small()
        };
        let report = run_checked(&settings, content());
        assert_eq!(report.camps.len(), 2);
        assert_eq!(report.people.len(), 10, "five a tribe");
        let far = |a: Pos, b: Pos| (a.x - b.x).abs().max((a.y - b.y).abs());
        for person in report.people.values() {
            let home = report.camps[usize::from(person.tribe)];
            assert!(far(person.born_at.unwrap(), home) <= 3, "born by its camp");
            assert!(person.death.is_none(), "nobody dies in four days");
            let end = person.last_at.unwrap();
            assert!(
                // The patch's camps are four tiles apart, nearer than
                // [`HOME_TILES`]: at home by its own camp, which may be
                // as near the other (a body that gathers walks about
                // its camp, since the brain's gather loop was fixed).
                far(end, home) <= sim::brain::HOME_TILES,
                "it lives at its own camp: {end:?}, its camp {home:?}"
            );
        }
        assert_eq!(report.samples[0].alive_by_tribe, [5, 5]);
        let html = report.fixed();
        assert!(html.contains("camp B at") && html.contains("(tribe B, "));
    }

    /// The plan's small trial on the real world: two tribes of five, either
    /// side of the river, ten days. Run by hand: the continent is born
    /// first, some fifteen seconds in release.
    #[test]
    #[ignore = "the vivarium's world takes some 15 s to be born; run by hand: cargo test --release -p vivarium -- --ignored"]
    fn two_tribes_of_five_live_ten_days_either_side_of_the_river() {
        let settings = Settings {
            world: WorldKind::Vivarium,
            tribes: 2,
            days: 10,
            ..small()
        };
        let report = run_with(&settings, content()).unwrap();
        for (id, person) in &report.people {
            assert!(
                person.death.is_none(),
                "person {} died: {:?}",
                id.0,
                person.death
            );
            let home = report.camps[usize::from(person.tribe)];
            let end = person.last_at.unwrap();
            assert!(
                (end.x - home.x).abs() <= 64 && (end.y - home.y).abs() <= 64,
                "person {} ends by its own camp, not stuck away: {end:?} against {home:?}",
                id.0
            );
        }
    }

    /// The keys of `docs/10-vivarium-data.md`: the first column of its
    /// table, `[]` for every element of an array, `*` for any key.
    fn documented_keys() -> Vec<String> {
        let doc = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/10-vivarium-data.md"),
        )
        .expect("docs/10 is in the repository");
        doc.lines()
            .filter_map(|line| {
                let rest = line.strip_prefix("| `")?;
                Some(rest[..rest.find('`')?].to_owned())
            })
            .collect()
    }

    /// Whether `path` is in `value`: a key present (`null` counts, and a
    /// `null` or an empty array on the way has nothing more to check) in
    /// every element of every array on the way.
    fn has_path(value: &serde_json::Value, path: &str) -> bool {
        let (head, tail) = path
            .split_once('.')
            .map_or((path, None), |(h, t)| (h, Some(t)));
        let (key, array) = head.strip_suffix("[]").map_or((head, false), |k| (k, true));
        let rest = |v: &serde_json::Value| v.is_null() || tail.is_none_or(|t| has_path(v, t));
        let Some(object) = value.as_object() else {
            return false;
        };
        let targets: Vec<&serde_json::Value> = if key == "*" {
            object.values().collect()
        } else {
            match object.get(key) {
                Some(v) => vec![v],
                None => return false,
            }
        };
        targets.iter().all(|next| {
            if array {
                next.as_array().is_some_and(|items| items.iter().all(rest))
            } else {
                rest(next)
            }
        })
    }

    /// Every key path of `value`, `[]` for an array of objects; arrays
    /// of scalars and of arrays are leaves.
    fn paths_of(value: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, item) in fields {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    out.push(path.clone());
                    paths_of(item, &path, out);
                }
            }
            serde_json::Value::Array(items) if items.iter().any(serde_json::Value::is_object) => {
                let last = out.pop().expect("the array's own path");
                out.push(format!("{last}[]"));
                for item in items {
                    paths_of(item, &format!("{last}[]"), out);
                }
            }
            _ => {}
        }
    }

    /// Whether `path` is one of `documented`, or leads to one (`years[]`
    /// to `years[].tribe`), `*` matching any one key.
    fn documented(path: &str, documented: &[String]) -> bool {
        documented.iter().any(|doc| {
            let a: Vec<&str> = doc.split('.').collect();
            let b: Vec<&str> = path.split('.').collect();
            a.len() >= b.len()
                && a.iter()
                    .zip(&b)
                    .all(|(x, y)| *x == "*" || x.trim_end_matches("[]") == y.trim_end_matches("[]"))
        })
    }

    #[test]
    fn the_data_holds_the_documented_keys_and_no_other() {
        let settings = Settings {
            days: 6,
            tribes: 2,
            ..small()
        };
        let report = run_checked(&settings, &forced_trials());
        assert!(
            !report.trials.is_empty(),
            "trials are forced, so there are some"
        );
        let text = report.json();
        let value: serde_json::Value = serde_json::from_str(&text).expect("the file parses");
        assert_eq!(value["meta"]["format"], json::FORMAT);
        let date = value["meta"]["timing"]["run_date"].as_str().unwrap();
        assert!(date.len() == 20 && date.ends_with('Z'), "{date}");
        assert_eq!(
            value["people"].as_array().unwrap().len(),
            report.people.len()
        );
        assert_eq!(
            value["curves"]["alive_by_tribe"].as_array().unwrap().len(),
            2
        );
        assert!(value["curves"]["points"].as_u64().unwrap() <= 1200);
        let keys = documented_keys();
        assert!(keys.len() > 100, "docs/10 lists {} keys", keys.len());
        for key in &keys {
            assert!(has_path(&value, key), "{key} is documented but not written");
        }
        let mut written = Vec::new();
        paths_of(&value, "", &mut written);
        written.sort();
        written.dedup();
        for path in &written {
            assert!(
                documented(path, &keys),
                "{path} is written but not documented"
            );
        }
    }

    #[test]
    fn the_data_years_agree_with_the_page() {
        let settings = Settings {
            days: 6,
            tribes: 2,
            ..small()
        };
        let report = run_checked(&settings, &forced_trials());
        let value = report.json_value();
        let html = report.html();
        let years = value["years"].as_array().unwrap();
        assert_eq!(years.len(), report.years().len());
        for (y, year) in years.iter().zip(report.years()) {
            let d = &y["deaths"];
            let row = format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td>",
                y["tribe"].as_str().unwrap(),
                y["year"],
                y["population"]
            );
            assert!(html.contains(&row), "{row} is not on the page");
            let causes = format!(
                "<td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}",
                y["births"], d["illness"], d["old_age"], d["childbirth"], d["hunger"]
            );
            assert!(html.contains(&causes), "{causes} is not on the page");
            assert_eq!(y["population"], year.population);
            assert_eq!(y["births"], year.births);
            assert_eq!(y["deaths_total"], year.deaths.iter().sum::<u32>());
            assert_eq!(y["trials"], year.trials);
            assert_eq!(y["kept"], year.kept);
            assert_eq!(y["copied"], year.copied);
            let food = &report.food[&(year.tribe, year.year)];
            assert_eq!(y["food"]["gathered_kj"], food.gathered_j / 1000);
            let food_row = format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>",
                y["tribe"].as_str().unwrap(),
                y["year"],
                y["food"]["start_kj"],
                y["food"]["gathered_kj"],
                y["food"]["eaten_kj"]
            );
            assert!(html.contains(&food_row), "{food_row} is not on the page");
        }
        for (id, person) in &report.people {
            let p = value["people"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["id"] == id.0)
                .expect("every person is in the data");
            match person.death {
                Some((tick, ..)) => assert_eq!(p["died"]["day"], tick / report.ticks_per_day() + 1),
                None => assert!(p["died"].is_null()),
            }
            assert_eq!(p["trials"].as_array().unwrap().len(), person.trials.len());
        }
    }

    #[test]
    fn the_presence_counts_the_living_once_a_day_at_noon() {
        let settings = Settings { days: 3, ..small() };
        let report = run_checked(&settings, content());
        let counted: u32 = report.presence.values().flat_map(|c| c.values()).sum();
        let people = u32::try_from(report.people.len()).unwrap();
        assert!(
            counted > 0 && counted <= people * 3,
            "{counted} standings for {people} people over three noons"
        );
        assert!(
            report
                .presence
                .keys()
                .all(|&(tribe, year)| tribe == 0 && year == 1)
        );
        for &(cx, cy) in report.presence.values().flat_map(BTreeMap::keys) {
            assert!(cx < 50_u32.div_ceil(PRESENCE_CELL_TILES));
            assert!(cy < 50_u32.div_ceil(PRESENCE_CELL_TILES));
        }
        let value = report.json_value();
        assert_eq!(value["presence"][0]["cell_tiles"], PRESENCE_CELL_TILES);
        assert!(
            value["map"]["data_uri"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert_eq!(value["map"]["tiles"], json!([50, 50]));
    }

    #[test]
    fn the_positions_are_the_living_at_each_noon_in_id_order() {
        let settings = Settings { days: 3, ..small() };
        let report = run_checked(&settings, content());
        let per_day = report.ticks_per_day();
        assert_eq!(
            report
                .positions
                .iter()
                .map(|(day, _)| *day)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        let mut listed = 0_u32;
        for (day, standing) in &report.positions {
            assert!(!standing.is_empty(), "day {day}: nobody at noon");
            let noon = (day - 1) * per_day + per_day / 2;
            for pair in standing.windows(2) {
                assert!(pair[0].0 < pair[1].0, "day {day}: ids out of order");
            }
            for &(id, pos) in standing {
                let person = report.people.get(&id).expect("a body is a person");
                assert!(
                    person.death.is_none_or(|(tick, ..)| tick >= noon),
                    "day {day}: person {} was dead at noon",
                    id.0
                );
                assert!((0..50).contains(&pos.x) && (0..50).contains(&pos.y));
            }
            listed += u32::try_from(standing.len()).unwrap();
        }
        // The presence grid counts the same bodies at the same noons.
        let counted: u32 = report.presence.values().flat_map(|c| c.values()).sum();
        assert_eq!(listed, counted);
        let value = report.json_value();
        let days = value["positions"].as_array().unwrap();
        assert_eq!(days.len(), 3);
        assert_eq!(days[0][0], 1);
        let first = &report.positions[0].1[0];
        assert_eq!(days[0][1][0], json!([first.0.0, first.1.x, first.1.y]));
        let text = report.json();
        assert!(
            text.contains("\n  \"positions\": [\n    [1, [["),
            "a day a line: {}",
            &text[text.find("\"positions\"").unwrap()..][..40]
        );
    }

    #[test]
    fn the_same_settings_give_the_same_data() {
        let a = run_with(&small(), content()).unwrap();
        let b = run_with(&small(), content()).unwrap();
        assert_eq!(a.json_value(), b.json_value());
        let strip = |text: String| -> serde_json::Value {
            let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
            v["meta"].as_object_mut().unwrap().remove("timing");
            v
        };
        assert_eq!(strip(a.json()), strip(b.json()));
    }
}
