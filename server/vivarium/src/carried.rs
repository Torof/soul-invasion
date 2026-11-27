//! The food carried home (25 November 2026): measures only, asked by the
//! project lead after the hearth's pooling (D58) left half the tribe dead
//! of hunger at richness 1.0, nearly all of them grown. Two suspicions to
//! measure, not to fix: the grown die of hunger with food in their arms
//! carried for their hearth, that they will not eat; and their arms (1 kg,
//! D57) fill up and food is left on the plant. His lead for a later fix
//! is a behaviour by hunger, so the body's hunger gauge is noted wherever
//! it tells something.
//!
//! - **Every death of hunger**: what the body carried, its own and its
//!   hearth's; how far from its camp; how long since its last meal; its
//!   age, will, whether a parent of a child under eight; its hearth's
//!   reserve then. Read at the last tick its mind lived (its vitality
//!   under a point: it thinks no more, eats no more, carries the same).
//! - **The arms**: every gather that ended, those that ended full, why
//!   full (the arms alone, arms and containers, the whole load), and what
//!   the plant still held then.
//! - **The grown's intake**: by season, eaten against the day's need.
//! - **Hunger while carrying for the hearth**: at each thought of a body
//!   holding food for its hearth, its hunger in tenths of the gauge (as
//!   the brain reads it: [`sim::brain::HUNGRY`] is 50), by what it did;
//!   and once a game hour, every body holding some.
//!
//! It only reads the world, the brains and their news: a run gives the
//! same world with it as without it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::{Value, json};
use sim::brain::{Brains, Decision, HUNGRY, News, carried_kj};
use sim::content::Content;
use sim::crops::Source;
use sim::map::{ChunkPos, FinePos, Pos};
use sim::{BodyId, Death, Event, Intention, Place, Season, WorkEnd, World};

use crate::Person;
use crate::chain::Who;

/// The hunger gauge's tenths: 0 (0–9 points) to 10 (100).
pub const TENTHS: usize = 11;

/// A histogram by hunger tenth.
pub type Tenths = [u64; TENTHS];

/// A histogram of the hunger gauge: by tenth, and, the gauge full (the
/// fat burning, D47), by the fat reserve left in quarters (0–24 %, 25–49
/// %, 50–74 %, 75–100 %): the gauge alone cannot tell a body an hour into
/// its fast from one about to starve.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hist {
    /// By tenth of the gauge.
    pub tenths: Tenths,
    /// The gauge full, by quarter of the fat reserve left.
    pub full_by_reserve: [u64; 4],
}

impl Hist {
    /// Notes a body in `condition`.
    pub fn note(&mut self, condition: &sim::Condition, needs: &sim::content::Needs) {
        let points = condition.hunger.points();
        self.tenths[tenth(points)] += 1;
        if condition.hunger.is_full() {
            let quarter = (condition.reserve_left_j(needs) * 4)
                .checked_div(condition.reserve_j(needs))
                .unwrap_or(0)
                .min(3);
            self.full_by_reserve[usize::try_from(quarter).unwrap_or(3)] += 1;
        }
    }

    fn json(&self) -> Value {
        json!({ "tenths": self.tenths, "full_by_reserve": self.full_by_reserve })
    }
}

/// The tenth of a hunger gauge at `points` (0 to 100).
#[must_use]
pub fn tenth(points: u32) -> usize {
    usize::try_from(points / 10).map_or(TENTHS - 1, |t| t.min(TENTHS - 1))
}

/// Grown: sixteen age-years, in thousandths.
const GROWN_MILLI: u64 = 16_000;
/// A young child: under eight age-years, fed by its hearth (D58).
const YOUNG_MILLI: u64 = 8_000;

/// The parents of a living child under eight.
#[must_use]
pub fn parents_of_young(world: &World) -> BTreeSet<BodyId> {
    let mut parents = BTreeSet::new();
    for b in world.bodies() {
        if b.condition().vitality.points() > 0 && world.age_milli(b) < YOUNG_MILLI {
            parents.extend(b.mother());
            parents.extend(b.father());
        }
    }
    parents
}

/// Who a body is for these measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// Grown, a parent of a living child under eight.
    Parents,
    /// The other grown.
    Others,
    /// Under sixteen.
    Young,
}

impl Class {
    const ALL: [Self; 3] = [Self::Parents, Self::Others, Self::Young];

    fn key(self) -> &'static str {
        match self {
            Self::Parents => "parents",
            Self::Others => "others",
            Self::Young => "young",
        }
    }

    fn of(world: &World, parents: &BTreeSet<BodyId>, body: BodyId) -> Self {
        if world
            .body(body)
            .is_some_and(|b| world.age_milli(b) < GROWN_MILLI)
        {
            Self::Young
        } else if parents.contains(&body) {
            Self::Parents
        } else {
            Self::Others
        }
    }
}

/// A body at its last living tick, its vitality under a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Dying {
    /// The food it carried, not spoiled, in kilojoules.
    carried_kj: u64,
    /// Of it, carried for its hearth.
    hearth_kj: u64,
    /// Its camp, its hearth's key.
    camp: Pos,
    /// Who it was.
    who: Who,
    /// Its hearth's reserve then; `None` without pooling.
    reserve_kj: Option<u64>,
    /// Its hunger gauge.
    hunger: u32,
}

/// A death of hunger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HungerDeath {
    /// The day.
    pub day: u64,
    /// Who.
    pub body: BodyId,
    /// Its tribe.
    pub tribe: u8,
    /// Where it died.
    pub at: Pos,
    /// What its mind held at its last living tick; `None` if it had no
    /// mind then (the brain off).
    last: Option<Dying>,
    /// Game ticks since its last meal; `None` if it never ate in the run.
    pub ticks_since_meal: Option<u64>,
}

/// The gathers of a kind of body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Arms {
    /// Gathers ended, of any grams.
    pub gathers: u64,
    /// Of them, ended full.
    pub full: u64,
    /// Of the full, the arms alone full: no container, its loose food at
    /// `hands_g` by its size (D57).
    pub full_arms: u64,
    /// Of the full, its arms and containers full.
    pub full_containers: u64,
    /// Of the full, its whole load full (`carry_capacity_g`).
    pub full_load: u64,
    /// What the plant still held when a gather ended full, in kilojoules.
    pub left_kj: u64,
    /// Full gathers whose plant could not be read (begun and ended in the
    /// same tick).
    pub left_unknown: u64,
    /// What the full gathers had gathered, in kilojoules.
    pub gathered_kj: u64,
}

impl Arms {
    fn json(&self) -> Value {
        json!({
            "gathers": self.gathers,
            "full": self.full,
            "full_arms": self.full_arms,
            "full_containers": self.full_containers,
            "full_load": self.full_load,
            "left_kj": self.left_kj,
            "left_unknown": self.left_unknown,
            "gathered_kj": self.gathered_kj,
        })
    }
}

/// A grown body's season.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Intake {
    /// Nightfalls it lived.
    days: u64,
    /// Eaten, in kilojoules, from nightfall to nightfall.
    eaten_kj: u64,
    /// Of it, from its hearth's reserve.
    hearth_kj: u64,
    /// Its day's need over those days (its burn, walking aside).
    need_kj: u64,
}

/// The intake of a kind of grown, summed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct IntakeSum {
    adult_seasons: u64,
    body_days: u64,
    eaten_kj: u64,
    hearth_kj: u64,
    need_kj: u64,
    /// Adult-seasons by eaten in per cent of need: under 50, 50 to 75, 75
    /// to 100, 100 and more.
    bands: [u64; 4],
}

impl IntakeSum {
    fn add(&mut self, i: &Intake) {
        self.adult_seasons += 1;
        self.body_days += i.days;
        self.eaten_kj += i.eaten_kj;
        self.hearth_kj += i.hearth_kj;
        self.need_kj += i.need_kj;
        let percent = (i.eaten_kj * 100).checked_div(i.need_kj).unwrap_or(0);
        let band = match percent {
            0..50 => 0,
            50..75 => 1,
            75..100 => 2,
            _ => 3,
        };
        self.bands[band] += 1;
    }

    fn json(&self) -> Value {
        json!({
            "adult_seasons": self.adult_seasons,
            "body_days": self.body_days,
            "eaten_kj": self.eaten_kj,
            "hearth_kj": self.hearth_kj,
            "need_kj": self.need_kj,
            "eaten_percent": (self.eaten_kj * 100).checked_div(self.need_kj).map_or(Value::Null, |p| json!(p)),
            "under_half": self.bands[0],
            "half_to_three_quarters": self.bands[1],
            "three_quarters_to_full": self.bands[2],
            "full_or_more": self.bands[3],
        })
    }
}

/// Hunger while carrying food for the hearth: histograms by tenth.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Carrying {
    /// A thought to eat food it carries, while it holds food for its
    /// hearth.
    pub ate_own: Hist,
    /// A thought to eat at its hearth's reserve, holding some.
    pub ate_hearth: Hist,
    /// A thought to eat food lying about, holding some.
    pub ate_lying: Hist,
    /// A thought, hungry ([`HUNGRY`] or more), holding some, to put it
    /// down at its hearth.
    pub hungry_putting_down: Hist,
    /// Any other thought, hungry, holding some, all it carries its
    /// hearth's: nothing of its own to eat.
    pub hungry_blocked: Hist,
    /// Any other thought, hungry, holding some, with food of its own
    /// beyond it.
    pub hungry_own_left: Hist,
    /// Once a game hour, every body holding some, all it carries its
    /// hearth's.
    pub hours_blocked: Hist,
    /// Once a game hour, every body holding some, with food of its own
    /// beyond it.
    pub hours_own_left: Hist,
}

/// The work a body began: the plant it gathers from.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Working {
    chunk: ChunkPos,
    source: Source,
    at: FinePos,
    kind: Arc<str>,
    gives: Arc<str>,
}

/// The run's measure of the food carried home.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CarryProbe {
    /// Whether food was pooled (the content's section and the fire).
    on: bool,
    /// Ticks a game hour.
    ticks_per_hour: u64,
    /// The deaths of hunger.
    pub hunger_deaths: Vec<HungerDeath>,
    /// The gathers, by [`Class`].
    pub arms: BTreeMap<Class, Arms>,
    /// The hunger at each gather ended with the arms alone full.
    pub hunger_at_full_arms: Hist,
    /// Hunger while carrying for the hearth.
    pub carrying: Carrying,
    /// The grown's seasons: (cycle, season, class, body).
    intake: BTreeMap<(u64, Season, Class, BodyId), Intake>,
    /// Eaten since the last nightfall, by body: (all, from a hearth).
    eaten_today: BTreeMap<BodyId, (u64, u64)>,
    /// The tick of each body's last meal.
    last_meal: BTreeMap<BodyId, u64>,
    /// The bodies at their last living tick.
    dying: BTreeMap<BodyId, Dying>,
    /// The plants being gathered.
    works: BTreeMap<BodyId, Working>,
    /// The parents of a child under eight, and the day they were read.
    parents: BTreeSet<BodyId>,
    parents_day: u64,
}

/// The kilojoules of `grams` of `material`; 0 if it is no food.
fn kj(content: &Content, material: &str, grams: u32) -> u64 {
    content
        .material(material)
        .and_then(|m| m.food)
        .map_or(0, |f| u64::from(f.energy_kj(grams)))
}

impl CarryProbe {
    /// The measure of a run living by `content`, of `ticks_per_day`.
    #[must_use]
    pub fn new(content: &Content, ticks_per_day: u64) -> Self {
        Self {
            on: content.needs.pooling.on() && content.needs.fire.on(),
            ticks_per_hour: (ticks_per_day / 24).max(1),
            ..Self::default()
        }
    }

    /// A tick's events, after the world stepped and before the brains
    /// think.
    pub fn tick(
        &mut self,
        (world, brains, content): (&World, &Brains, &Content),
        events: &[Event],
        people: &BTreeMap<BodyId, Person>,
    ) {
        let day = world.time().day;
        if self.parents_day != day {
            self.parents = parents_of_young(world);
            self.parents_day = day;
        }
        let now = world.tick().0;
        for event in events {
            match event {
                Event::BodyAte {
                    body,
                    item,
                    energy_kj,
                    ..
                } => {
                    self.last_meal.insert(*body, now);
                    let e = self.eaten_today.entry(*body).or_default();
                    e.0 += u64::from(*energy_kj);
                    if brains.hearth_holding(*item).is_some() {
                        e.1 += u64::from(*energy_kj);
                    }
                }
                Event::WorkStarted { body, .. } => {
                    if let Some(w) = world.body(*body).and_then(sim::Body::work) {
                        self.works.insert(
                            *body,
                            Working {
                                chunk: w.chunk,
                                source: w.bearer.source,
                                at: w.bearer.at,
                                kind: Arc::clone(&w.bearer.kind),
                                gives: Arc::clone(&w.gives),
                            },
                        );
                    }
                }
                Event::WorkStopped {
                    body,
                    gives,
                    grams,
                    why,
                } => self.stopped(world, content, (*body, gives, *grams, *why)),
                Event::BodyDied {
                    body, cause, at, ..
                } => {
                    let last = self.dying.remove(body);
                    self.works.remove(body);
                    self.eaten_today.remove(body);
                    let meal = self.last_meal.remove(body);
                    if *cause == Death::Starved {
                        self.hunger_deaths.push(HungerDeath {
                            day,
                            body: *body,
                            tribe: people.get(body).map_or(0, |p| p.tribe),
                            at: *at,
                            last,
                            ticks_since_meal: meal.map(|t| now - t),
                        });
                    }
                }
                Event::NightFell { .. } => self.nightfall(world, content),
                _ => {}
            }
        }
        if self.on && now.is_multiple_of(self.ticks_per_hour) {
            for b in world.bodies() {
                let Some(mind) = brains.mind(b.id()) else {
                    continue;
                };
                if mind.hearth_carried_kj() == 0 {
                    continue;
                }
                let carried = carried_kj(world, content, b.id());
                let held = mind.hearth_carried_kj().min(carried);
                if held == 0 {
                    continue;
                }
                let h = if carried <= held {
                    &mut self.carrying.hours_blocked
                } else {
                    &mut self.carrying.hours_own_left
                };
                h.note(b.condition(), &content.needs);
            }
        }
    }

    /// A gather ended.
    fn stopped(
        &mut self,
        world: &World,
        content: &Content,
        (body, gives, grams, why): (BodyId, &str, u32, WorkEnd),
    ) {
        let working = self.works.remove(&body);
        let class = Class::of(world, &self.parents, body);
        let arms = self.arms.entry(class).or_default();
        arms.gathers += 1;
        if why != WorkEnd::Full {
            return;
        }
        arms.full += 1;
        arms.gathered_kj += kj(content, gives, grams);
        if let Some(b) = world.body(body) {
            let movement = &content.movement;
            if b.arms_food_g() >= b.arms_g(movement) {
                if b.stowed_g() == 0 {
                    arms.full_arms += 1;
                    self.hunger_at_full_arms.note(b.condition(), &content.needs);
                } else {
                    arms.full_containers += 1;
                }
            } else {
                arms.full_load += 1;
            }
        }
        match working {
            Some(w) => {
                let left = world
                    .ripe_of(w.chunk, w.source, w.at, &w.kind, &w.gives)
                    .map_or(0, |(_, g)| g);
                arms.left_kj += kj(content, &w.gives, left);
            }
            None => arms.left_unknown += 1,
        }
    }

    /// At nightfall each grown body's day of eating against its need.
    fn nightfall(&mut self, world: &World, content: &Content) {
        let time = world.time();
        for b in world.bodies() {
            if b.condition().vitality.points() == 0 || world.age_milli(b) < GROWN_MILLI {
                continue;
            }
            let class = Class::of(world, &self.parents, b.id());
            let (eaten, hearth) = self.eaten_today.get(&b.id()).copied().unwrap_or((0, 0));
            let i = self
                .intake
                .entry((time.cycle, time.season, class, b.id()))
                .or_default();
            i.days += 1;
            i.eaten_kj += eaten;
            i.hearth_kj += hearth;
            i.need_kj += b.condition().need_j_per_day(&content.needs) / 1000;
        }
        self.eaten_today.clear();
    }

    /// A body whose vitality fell under a point, read while its mind is
    /// still here (the brain lets it go at its next turn): it thinks no
    /// more, so what it carries and holds is what it dies with.
    pub fn dying(&mut self, (world, brains, content): (&World, &Brains, &Content), body: BodyId) {
        let (Some(b), Some(mind)) = (world.body(body), brains.mind(body)) else {
            return;
        };
        let carried = carried_kj(world, content, body);
        let camp = Brains::hearth_camp(content, mind.home());
        let reserve_kj = self.on.then(|| {
            brains
                .reserves_kj(world, content)
                .into_iter()
                .find_map(|(c, kj)| (c == camp).then_some(kj))
                .unwrap_or(0)
        });
        self.dying.insert(
            body,
            Dying {
                carried_kj: carried,
                hearth_kj: mind.hearth_carried_kj().min(carried),
                camp,
                who: Who::of(world, brains, b),
                reserve_kj,
                hunger: b.condition().hunger.points(),
            },
        );
    }

    /// The thoughts of a tick, after the brains thought (`news`, what
    /// they told then): each body holding food for its hearth, its hunger
    /// by what it chose.
    pub fn thoughts(
        &mut self,
        (world, brains, content): (&World, &Brains, &Content),
        decisions: &[(BodyId, Decision)],
        news: &[News],
    ) {
        if !self.on {
            return;
        }
        for (body, decision) in decisions {
            let Some(mind) = brains.mind(*body) else {
                continue;
            };
            // What it put down this turn it held as it thought.
            let put: u64 = news
                .iter()
                .filter_map(|n| match n {
                    News::PutDown { body: b, kj, .. } if b == body => Some(*kj),
                    _ => None,
                })
                .sum();
            let raw = mind.hearth_carried_kj() + put;
            if raw == 0 {
                continue;
            }
            let Some(b) = world.body(*body) else {
                continue;
            };
            let hunger = b.condition().hunger.points();
            // A body not hungry and not eating is not counted, whatever
            // it carries: what it carries is not weighed then.
            if hunger < HUNGRY && !matches!(decision.intention, Intention::Eat { .. }) {
                continue;
            }
            let carried = carried_kj(world, content, *body);
            let held = raw.min(carried);
            if held == 0 {
                continue;
            }
            let c = &mut self.carrying;
            let h = match decision.intention {
                Intention::Eat { item, .. } => {
                    let place = world.item(item).map(sim::Item::place);
                    if place == Some(Place::Carried { body: *body }) {
                        &mut c.ate_own
                    } else if brains.hearth_holding(item).is_some() {
                        &mut c.ate_hearth
                    } else {
                        &mut c.ate_lying
                    }
                }
                _ if hunger < HUNGRY => continue,
                Intention::Drop { .. } | Intention::Keep { .. } if put > 0 => {
                    &mut c.hungry_putting_down
                }
                _ if carried <= held => &mut c.hungry_blocked,
                _ => &mut c.hungry_own_left,
            };
            h.note(b.condition(), &content.needs);
        }
    }

    /// The deaths of hunger, as the JSON's `pooling.hunger_deaths`.
    fn deaths_json(&self, ticks_per_day: u64) -> Value {
        self.hunger_deaths
            .iter()
            .map(|d| {
                let last = d.last.as_ref();
                json!({
                    "day": d.day,
                    "body": d.body.0,
                    "tribe": d.tribe,
                    "at": [d.at.x, d.at.y],
                    "carried_kj": last.map(|l| l.carried_kj),
                    "own_kj": last.map(|l| l.carried_kj - l.hearth_kj),
                    "hearth_kj": last.map(|l| l.hearth_kj),
                    "camp_tiles": last.map(|l| (d.at.x - l.camp.x).abs().max((d.at.y - l.camp.y).abs())),
                    "ticks_since_meal": d.ticks_since_meal,
                    "hours_since_meal": d.ticks_since_meal.map(|t| t * 24 / ticks_per_day.max(1)),
                    "age_years": last.map(|l| l.who.age_years),
                    "will": last.map(|l| l.who.will),
                    "parent_under_8": last.map(|l| l.who.parent_under_8),
                    "reserve_kj": last.and_then(|l| l.reserve_kj),
                    "hunger": last.map(|l| l.hunger),
                })
            })
            .collect::<Vec<_>>()
            .into()
    }

    /// The grown's intake, as the JSON's `pooling.intake`.
    fn intake_json(&self) -> Value {
        let mut seasons: BTreeMap<(u64, Season), [IntakeSum; 2]> = BTreeMap::new();
        let mut run = [IntakeSum::default(); 2];
        for (&(cycle, season, class, _), i) in &self.intake {
            let k = usize::from(class != Class::Parents);
            seasons.entry((cycle, season)).or_default()[k].add(i);
            run[k].add(i);
        }
        json!({
            "parents": run[0].json(),
            "others": run[1].json(),
            "seasons": seasons.iter().map(|(&(cycle, season), s)| json!({
                "cycle": cycle,
                "season": season,
                "parents": s[0].json(),
                "others": s[1].json(),
            })).collect::<Vec<_>>(),
        })
    }

    /// The keys this measure adds to the JSON's block `pooling`
    /// (`docs/10-vivarium-data.md`).
    pub fn add_json(&self, pooling: &mut Value, ticks_per_day: u64) {
        let mut arms = serde_json::Map::new();
        for class in Class::ALL {
            arms.insert(
                class.key().to_owned(),
                self.arms.get(&class).copied().unwrap_or_default().json(),
            );
        }
        arms.insert(
            "hunger_at_full_arms".to_owned(),
            self.hunger_at_full_arms.json(),
        );
        let c = &self.carrying;
        pooling["hunger_deaths"] = self.deaths_json(ticks_per_day);
        pooling["arms"] = Value::Object(arms);
        pooling["intake"] = self.intake_json();
        pooling["carrying"] = json!({
            "ate_own": c.ate_own.json(),
            "ate_hearth": c.ate_hearth.json(),
            "ate_lying": c.ate_lying.json(),
            "hungry_putting_down": c.hungry_putting_down.json(),
            "hungry_blocked": c.hungry_blocked.json(),
            "hungry_own_left": c.hungry_own_left.json(),
            "hours_blocked": c.hours_blocked.json(),
            "hours_own_left": c.hours_own_left.json(),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::OnceLock;

    use sim::time::Calendar;

    use super::*;
    use crate::{Report, Settings, WorldKind, load_content, run_with};

    fn content() -> &'static Content {
        static CONTENT: OnceLock<Content> = OnceLock::new();
        CONTENT.get_or_init(|| {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
            load_content(&dir).expect("the repository's content reads and validates")
        })
    }

    /// Two tribes of six grown with two children and an infant on a bare
    /// patch, the fat reserve short (60,000 kJ, as the vivarium's
    /// `the_dead_keep_what_they_thought`): somebody starves in eight days.
    fn starving() -> Report {
        let settings = Settings {
            world: WorldKind::Patch { side: 60 },
            people: 6,
            start: crate::Start::Founders,
            days: 8,
            ticks_per_second: 1,
            seconds_per_day: Calendar::DEFAULT_SECONDS_PER_DAY,
            days_per_cycle: Calendar::DEFAULT_DAYS_PER_CYCLE,
            age_years_per_cycle: Calendar::DEFAULT_AGE_YEARS_PER_CYCLE,
            seed: 3,
            content: None,
            lands: None,
            brain: true,
            tribes: 2,
            children: 2,
            infants: 1,
            adult_years: (18, 40),
            fed: false,
            trials: true,
            hands_known: 0,
            fire: true,
            decisions: true,
            moves: true,
            pooling: true,
            care: true,
            trips: true,
            problems: true,
            wake_all: false,
            old_wakes: false,
            old_illness: false,
            passages: true,
            learning: true,
            learning_act: true,
            search: true,
            continents: None,
            helpers: 0,
        };
        let mut content = content().clone();
        content.needs.reserve_kj = 60_000;
        run_with(&settings, &content).expect("the run runs")
    }

    #[test]
    fn the_tenths_cut_the_gauge_by_ten_points() {
        assert_eq!(tenth(0), 0);
        assert_eq!(tenth(9), 0);
        assert_eq!(tenth(HUNGRY), 5);
        assert_eq!(tenth(99), 9);
        assert_eq!(tenth(100), 10);
        assert_eq!(tenth(u32::MAX), TENTHS - 1);
    }

    #[test]
    fn an_adult_season_falls_in_one_band_by_its_own_intake() {
        let mut sum = IntakeSum::default();
        for eaten in [0, 4_999, 5_000, 7_499, 7_500, 9_999, 10_000, 15_000] {
            sum.add(&Intake {
                days: 1,
                eaten_kj: eaten,
                hearth_kj: 0,
                need_kj: 10_000,
            });
        }
        assert_eq!(sum.bands, [2, 2, 2, 2]);
        assert_eq!(sum.adult_seasons, 8);
        assert_eq!(sum.body_days, 8);
        assert_eq!(sum.json()["eaten_percent"], json!(74));
    }

    /// Every death of hunger is in the list, read at its last living tick
    /// whenever one was seen; the arms' counts add up; the grown's
    /// nightfalls are the hearth's measure's; who ate at the hearth sums to
    /// what was eaten there; the hungry histograms hold only the hungry.
    #[test]
    fn the_food_carried_home_is_counted_whole() {
        let report = starving();
        let starved = report
            .people
            .values()
            .filter(|p| matches!(p.death, Some((_, Death::Starved, _))))
            .count();
        assert!(starved > 0, "somebody starves on the bare patch");
        let probe = &report.carried;
        assert_eq!(probe.hunger_deaths.len(), starved);
        for d in &probe.hunger_deaths {
            let last = d.last.expect("its last living tick was seen");
            assert!(last.hearth_kj <= last.carried_kj, "{d:?}");
            assert!(last.reserve_kj.is_some(), "pooled: {d:?}");
            assert!(last.who.age_years <= 100 && last.who.will <= 100);
        }
        let mut gathers = 0;
        for arms in probe.arms.values() {
            assert!(arms.full <= arms.gathers, "{arms:?}");
            assert_eq!(
                arms.full,
                arms.full_arms + arms.full_containers + arms.full_load
            );
            gathers += arms.gathers;
        }
        assert!(gathers > 0, "they gather");
        let full_arms: u64 = probe.arms.values().map(|a| a.full_arms).sum();
        assert_eq!(
            probe.hunger_at_full_arms.tenths.iter().sum::<u64>(),
            full_arms
        );
        assert!(
            probe
                .hunger_at_full_arms
                .full_by_reserve
                .iter()
                .sum::<u64>()
                <= probe.hunger_at_full_arms.tenths[TENTHS - 1]
        );
        let data = report.json_value();
        let pooling = &data["pooling"];
        let days = |key: &str| pooling["intake"][key]["body_days"].as_u64().unwrap();
        let grown = |key: &str| pooling[key]["body_days"].as_u64().unwrap();
        assert_eq!(
            days("parents") + days("others"),
            grown("parents") + grown("others"),
            "the same grown at the same nightfalls"
        );
        let e = &pooling["eaters"];
        let eaten: u64 = [
            "under_eight_kj",
            "eight_to_sixteen_kj",
            "parents_kj",
            "others_kj",
        ]
        .iter()
        .map(|k| e[k].as_u64().unwrap())
        .sum();
        assert_eq!(eaten, pooling["totals"]["eaten_kj"].as_u64().unwrap());
        let c = &probe.carrying;
        for h in [c.hungry_putting_down, c.hungry_blocked, c.hungry_own_left] {
            assert_eq!(h.tenths[..tenth(HUNGRY)].iter().sum::<u64>(), 0, "{h:?}");
        }
    }
}
