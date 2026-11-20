//! The vivarium (D41): a world of the simulation under glass, run headless
//! and accelerated, with no network and no clock, and an HTML report per
//! run the project lead reads: the map, four curves, the chronicle, a
//! page per person, the speed and the named factors.
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

use sim::brain::{Brains, Why};
use sim::content::Content;
use sim::map::{Map, Pos, Terrain};
use sim::time::Calendar;
use sim::{BodyId, Death, Event, Intention, Sex, World};

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
        }
    }
}

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
    /// Meals eaten, and grams in all.
    pub meals: (u32, u32),
    /// Times it drank.
    pub drinks: u32,
    /// Times its brain thought.
    pub thoughts: u64,
    /// Its own lines of the chronicle.
    pub entries: Vec<Entry>,
    /// Its gauges 24 times a day.
    pub gauges: Vec<Gauges>,
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
    /// Seconds the run took on the wall.
    pub wall_s: f64,
    /// Seconds the world took to be born.
    pub birth_s: f64,
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
    let born = Instant::now();
    let mut world = match settings.world {
        WorldKind::Vivarium => World::on_valley(calendar, content, Map::vivarium()),
        WorldKind::Patch { side } => World::with_valley(calendar, content, side),
    };
    let birth_s = born.elapsed().as_secs_f64();
    let map_side = (world.map().width(), world.map().height());
    let camps = camps(settings, world.map());
    let mut people: BTreeMap<BodyId, Person> = BTreeMap::new();
    let mut chronicle = Vec::new();
    let mut brains = Brains::new(settings.seed);
    for (tribe, &camp) in camps.iter().enumerate() {
        let tribe = u8::try_from(tribe).expect("two tribes at most");
        // Who is born at the camp (lot 3 B): grown women and men in turn,
        // from 18 to 40 years old; children; infants, each on a woman.
        let per_year = u64::from(calendar.days_per_year());
        let mut born: Vec<(u64, Sex, bool)> = Vec::new();
        for k in 0..settings.people {
            let years = 18 + u64::from(k) * 22 / u64::from(settings.people.max(1));
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
        }
    }
    let tribes = camps.len();
    let per_year = calendar.ticks_per_day() * u64::from(calendar.days_per_year());
    let mut food: BTreeMap<(u8, u64), Balance> = BTreeMap::new();
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
    let mut events = Vec::new();
    for _ in 0..ticks {
        events.clear();
        world.step(content, &mut events);
        let tick = world.tick().0;
        let day = world.time().day;
        // The year whose ticks are elapsing, as the calendar counts (from
        // 1): the tick that starts a year still closes the one before.
        let year = (tick - 1) / per_year + 1;
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
                    people.entry(*body).or_default().entries.push(Entry {
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
                        match cause {
                            Death::Starved => "of hunger",
                            Death::Froze => "of cold",
                            Death::Dehydrated => "of thirst",
                        },
                        at.x,
                        at.y
                    );
                    let entry = Entry { tick, day, what };
                    chronicle.push(entry.clone());
                    let person = people.entry(*body).or_default();
                    person.death = Some((tick, *cause, *at));
                    person.entries.push(entry);
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
                people.entry(body.id()).or_default().thoughts = mind.thoughts();
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
        // The year's last tick, or the run's: what the living carry.
        if tick % per_year == 0 || tick == ticks {
            let carried = reserves(&world, content, &people);
            for (tribe, &j) in carried.iter().enumerate().take(tribes) {
                let t = u8::try_from(tribe).expect("two");
                food.entry((t, year)).or_default().end_j = j;
                if tick < ticks {
                    food.entry((t, year + 1)).or_default().start_j = j;
                }
            }
        }
    }
    for (id, person) in &mut people {
        person.thoughts = brains
            .mind(*id)
            .map_or(person.thoughts, sim::brain::Mind::thoughts);
        person.last_at = world
            .body(*id)
            .map(sim::Body::pos)
            .or(person.death.map(|d| d.2));
    }
    let wall_s = started.elapsed().as_secs_f64();
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
                "the minimal one (sim::brain): it thinks only when something changes for its body, drinks when thirst passes 50 (wakes for it past 80); when hunger passes 50 it eats what it carries, else the food it saw, else gathers from the nearest tree or plant it knows bearing food; by day it gathers until it carries three days of food (30,000 kJ); goes looking when it knows no place (for food: by day, and only when something is ripe somewhere), goes home at night and sleeps within 6 tiles of the camp; it remembers the drinking places, the food and the trees and plants bearing food it saw"
            } else {
                "none (the control run): the people stand where they were born"
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
        h.push_str("</table><h2>The people</h2>");
        for (id, person) in &self.people {
            let fate = match &person.death {
                Some((tick, cause, at)) => format!(
                    "died on day {} (tick {tick}, {}) {} at ({}, {})",
                    tick / self.ticks_per_day().max(1) + 1,
                    self.season_of(*tick),
                    match cause {
                        Death::Starved => "of hunger",
                        Death::Froze => "of cold",
                        Death::Dehydrated => "of thirst",
                    },
                    at.x,
                    at.y
                ),
                None => "alive at the end".to_owned(),
            };
            let who = person.sex.map_or_else(String::new, |sex| {
                format!(
                    ", {} at the start",
                    describe(sex, Self::age_milli(person, 0))
                )
            });
            let _ = write!(
                h,
                "<details><summary>{} (tribe {}{who}) — {fate}; {} meals ({} g), drank {} times, thought {} times</summary>",
                name(*id),
                tribe_name(person.tribe),
                person.meals.0,
                person.meals.1,
                person.drinks,
                person.thoughts
            );
            h.push_str(&sparkline(&person.gauges, self.ticks));
            h.push_str("<table class=\"chronicle\">");
            for entry in &person.entries {
                let _ = write!(
                    h,
                    "<tr><td>day {} · tick {}</td><td>{}</td></tr>",
                    entry.day,
                    entry.tick,
                    escape(&entry.what)
                );
            }
            h.push_str("</table></details>");
        }
        let _ = write!(
            h,
            "<p class=\"note\">One game year is {days_per_year} days; the speed below counts years of game a minute of wall clock.</p>"
        );
        h
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
            "<p id=\"timing\">The world was born in {:.2} s; the run took {:.2} s on the wall: <b>{speed:.1} game years a minute</b>.</p>",
            self.birth_s, self.wall_s
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
        let people = u64::from(self.settings.people.max(1)) * tribes;
        let x = |tick: u64| tick * W / ticks;
        let line = |name: &str, colour: &str, values: &dyn Fn(&Sample) -> u64, top: u64| {
            let points: Vec<String> = self
                .samples
                .iter()
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
        let points: Vec<String> = gauges
            .iter()
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

    /// On the patch in spring nothing grows, and since lot 3 a body
    /// without food dies on its seventh day: its page must still say what
    /// it thought, read before its mind goes with it.
    #[test]
    fn the_dead_keep_what_they_thought() {
        let settings = Settings { days: 8, ..small() };
        let report = run_checked(&settings, content());
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
    /// 18, the infant she carried, of thirst on day 21. The text predates
    /// D48 and the brain's changes; the band's hunger is the sim phase's
    /// (F1). Until then this test fails and says who died and when.
    #[test]
    #[ignore = "the D47 trial, 30 days on the vivarium's world, some 30 s in release; fails today (F4): cargo test --release -p vivarium -- --ignored the_d47_trial"]
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
    /// kJ a person-day. Whether the vivarium judges at the game's cadence
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
        // Thirst alone: the patch bears nothing in spring, and since lot 3
        // a body without food dies on its seventh day.
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
        let settings = Settings {
            tribes: 2,
            days: 4,
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
}
