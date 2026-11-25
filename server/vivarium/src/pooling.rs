//! The hearth's measures (D58, 25 November 2026): what each hearth's
//! reserve received, gave and lost, day by day; how many grown went out;
//! the parents of young children against the other grown (tiles walked,
//! fatigue at nightfall, thin and weak days); the children dead before
//! eight and before sixteen.
//!
//! Its own module and its own block of the JSON (`pooling`), apart from
//! the others: it only reads the world, the brains and their news.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use sim::brain::{Brains, HOME_TILES, News};
use sim::content::Content;
use sim::map::Pos;
use sim::{BodyId, Event, World};

use crate::Person;

/// One hearth's day.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HearthDay {
    /// What its reserve held at dawn, in kilojoules.
    pub dawn_kj: u64,
    /// Put down there over the day.
    pub put_kj: u64,
    /// Eaten from it.
    pub eaten_kj: u64,
    /// Of it, eaten by the young (under sixteen).
    pub eaten_young_kj: u64,
    /// Spoiled there.
    pub spoiled_kj: u64,
}

/// A kind of grown body's day: parents of a child under eight, or the
/// other grown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClassDay {
    /// Bodies.
    pub bodies: u32,
    /// Of them, gone out: beyond `HOME_TILES` of their home at some step.
    pub out: u32,
    /// Of them, gathered something that day.
    pub gathered: u32,
    /// Tiles walked, all of them.
    pub tiles: u64,
    /// Fatigue at nightfall, in points, summed.
    pub fatigue: u64,
    /// Thin at nightfall (under half their fat reserve).
    pub thin: u32,
    /// Weak at nightfall.
    pub weak: u32,
}

impl ClassDay {
    fn add(&mut self, other: &Self) {
        self.bodies += other.bodies;
        self.out += other.out;
        self.gathered += other.gathered;
        self.tiles += other.tiles;
        self.fatigue += other.fatigue;
        self.thin += other.thin;
        self.weak += other.weak;
    }

    fn json(&self) -> Value {
        // Means a body-day, rounded to the unit.
        let per = |sum: u64| {
            let n = u64::from(self.bodies);
            (sum + n / 2)
                .checked_div(n)
                .map_or(Value::Null, |m| json!(m))
        };
        json!({
            "body_days": self.bodies,
            "out": self.out,
            "gathered": self.gathered,
            "tiles": self.tiles,
            "tiles_per_day": per(self.tiles),
            "fatigue_at_nightfall": per(self.fatigue),
            "thin": self.thin,
            "weak": self.weak,
        })
    }
}

/// One day of the run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PoolDay {
    /// Each hearth by its camp.
    pub hearths: BTreeMap<Pos, HearthDay>,
    /// The parents of a child under eight.
    pub parents: ClassDay,
    /// The other grown.
    pub others: ClassDay,
    /// Bodies that set out at dawn to gather for their hearth beyond their
    /// own store, and what for: their own seen hungry, their care.
    pub gathering_for: u32,
    /// See `gathering_for`: kilojoules for their own seen hungry.
    pub seen_kj: u64,
    /// See `gathering_for`: kilojoules by their care.
    pub cared_kj: u64,
    /// Who ate at the hearths that day (25 November 2026).
    pub eaters: Eaters,
}

/// Who ate at the hearths, in kilojoules (the food carried home, 25
/// November 2026).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Eaters {
    /// Children under eight, fed by the hearth.
    pub under_eight_kj: u64,
    /// Children from eight to sixteen.
    pub eight_to_sixteen_kj: u64,
    /// The grown parents of a living child under eight.
    pub parents_kj: u64,
    /// The other grown.
    pub others_kj: u64,
}

impl Eaters {
    fn add(&mut self, other: &Self) {
        self.under_eight_kj += other.under_eight_kj;
        self.eight_to_sixteen_kj += other.eight_to_sixteen_kj;
        self.parents_kj += other.parents_kj;
        self.others_kj += other.others_kj;
    }

    fn json(&self) -> Value {
        json!({
            "under_eight_kj": self.under_eight_kj,
            "eight_to_sixteen_kj": self.eight_to_sixteen_kj,
            "parents_kj": self.parents_kj,
            "others_kj": self.others_kj,
        })
    }
}

/// The run's measure of the hearths.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HearthProbe {
    /// Whether food was pooled at all (the content's section on, and the
    /// fire it rides on).
    pub on: bool,
    /// Whether the care for the young's coming days was on.
    pub care: bool,
    /// The age the hearth feeds a child until, in years.
    pub fed_until_years: u32,
    /// By day, from 1.
    pub days: BTreeMap<u64, PoolDay>,
    /// Tiles walked today, by body.
    walked: BTreeMap<BodyId, u64>,
    /// Who went out today.
    out: BTreeSet<BodyId>,
    /// Who gathered something today.
    gathered: BTreeSet<BodyId>,
    /// The children dead: (before eight, before sixteen, born in the run
    /// or young at the start, of them dead before eight, before sixteen).
    pub children: Children,
    /// The parents of a living child under eight, and the day they were
    /// read: who ate at the hearth.
    parents: BTreeSet<BodyId>,
    parents_day: u64,
}

/// The children's lives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Children {
    /// Every body under sixteen at some point of the run.
    pub young: u32,
    /// Every body under eight at some point of the run.
    pub under_eight: u32,
    /// Dead before eight.
    pub dead_before_eight: u32,
    /// Dead before sixteen.
    pub dead_before_sixteen: u32,
    /// Born in the run.
    pub born: u32,
}

impl HearthProbe {
    /// The measure of a run living by `content`.
    #[must_use]
    pub fn new(content: &Content) -> Self {
        let p = &content.needs.pooling;
        Self {
            on: p.on() && content.needs.fire.on(),
            care: p.cares() && content.needs.fire.on(),
            fed_until_years: p.fed_until_years,
            ..Self::default()
        }
    }

    fn day(&mut self, day: u64) -> &mut PoolDay {
        self.days.entry(day).or_default()
    }

    /// A tick's events: the steps walked, who went out, the dawn's
    /// reserves, the nightfall's classes.
    pub fn tick(
        &mut self,
        (world, brains, content): (&World, &Brains, &Content),
        events: &[Event],
        day: u64,
    ) {
        if self.parents_day != day {
            self.parents = crate::carried::parents_of_young(world);
            self.parents_day = day;
        }
        for event in events {
            match event {
                Event::BodyMoved { body, to, .. } => {
                    *self.walked.entry(*body).or_default() += 1;
                    if let Some(mind) = brains.mind(*body) {
                        let home = mind.home();
                        if (to.x - home.x).abs() > HOME_TILES || (to.y - home.y).abs() > HOME_TILES
                        {
                            self.out.insert(*body);
                        }
                    }
                }
                Event::WorkStopped { body, grams, .. } if *grams > 0 => {
                    self.gathered.insert(*body);
                }
                Event::DayStarted { .. } => {
                    for (camp, kj) in brains.reserves_kj(world, content) {
                        self.day(day).hearths.entry(camp).or_default().dawn_kj = kj;
                    }
                }
                Event::NightFell { .. } => self.nightfall(world, content, day),
                _ => {}
            }
        }
    }

    /// At nightfall each grown body's day: a parent of a child under
    /// eight, or not.
    fn nightfall(&mut self, world: &World, content: &Content, day: u64) {
        let young_milli = u64::from(self.fed_until_years.max(8)) * 1000;
        let mut parents: BTreeSet<BodyId> = BTreeSet::new();
        for b in world.bodies() {
            if b.condition().vitality.points() > 0 && world.age_milli(b) < young_milli {
                parents.extend(b.mother());
                parents.extend(b.father());
            }
        }
        let needs = &content.needs;
        let mut parent_day = ClassDay::default();
        let mut other_day = ClassDay::default();
        for b in world.bodies() {
            if b.condition().vitality.points() == 0 || world.age_milli(b) < 16_000 {
                continue;
            }
            let class = if parents.contains(&b.id()) {
                &mut parent_day
            } else {
                &mut other_day
            };
            let c = b.condition();
            class.bodies += 1;
            class.out += u32::from(self.out.contains(&b.id()));
            class.gathered += u32::from(self.gathered.contains(&b.id()));
            class.tiles += self.walked.get(&b.id()).copied().unwrap_or(0);
            class.fatigue += u64::from(c.fatigue.points());
            class.thin += u32::from(c.is_thin(needs));
            class.weak += u32::from(c.is_weak(needs));
        }
        self.walked.clear();
        self.out.clear();
        self.gathered.clear();
        let today = self.day(day);
        today.parents.add(&parent_day);
        today.others.add(&other_day);
    }

    /// The brains' news of the hearths.
    pub fn news(&mut self, news: &News, world: &World, day: u64) {
        match news {
            News::PutDown { camp, kj, .. } => {
                self.day(day).hearths.entry(*camp).or_default().put_kj += kj;
            }
            News::AteAtHearth { body, camp, kj, .. } => {
                let age = world.body(*body).map(|b| world.age_milli(b));
                let young = age.is_some_and(|a| a < 16_000);
                let parent = self.parents.contains(body);
                let eaters = &mut self.day(day).eaters;
                match age {
                    Some(a) if a < 8_000 => eaters.under_eight_kj += kj,
                    Some(a) if a < 16_000 => eaters.eight_to_sixteen_kj += kj,
                    _ if parent => eaters.parents_kj += kj,
                    _ => eaters.others_kj += kj,
                }
                let h = self.day(day).hearths.entry(*camp).or_default();
                h.eaten_kj += kj;
                if young {
                    h.eaten_young_kj += kj;
                }
            }
            News::SpoiledAtHearth { camp, kj, .. } => {
                self.day(day).hearths.entry(*camp).or_default().spoiled_kj += kj;
            }
            News::SawHearth {
                seen_kj, cared_kj, ..
            } => {
                let d = self.day(day);
                d.gathering_for += 1;
                d.seen_kj += seen_kj;
                d.cared_kj += cared_kj;
            }
            _ => {}
        }
    }

    /// The children's lives, read at the end from the people's pages.
    pub fn finish(
        &mut self,
        people: &BTreeMap<BodyId, Person>,
        (ticks_per_day, days_per_year, start_day, last_day): (u64, u64, i64, i64),
    ) {
        let mut c = Children::default();
        let year = i64::try_from(days_per_year.max(1)).unwrap_or(i64::MAX);
        for p in people.values() {
            if p.sex.is_none() {
                continue;
            }
            let end = p.death.map_or(last_day, |(tick, ..)| {
                start_day + i64::try_from(tick / ticks_per_day.max(1)).unwrap_or(0)
            });
            // Its age at the start of the run, and at its end or death,
            // in days.
            let first = (start_day - p.born_day).max(0);
            let last = end - p.born_day;
            c.born += u32::from(p.born_in_run);
            if first < 16 * year {
                c.young += 1;
                if p.death.is_some() && last < 16 * year {
                    c.dead_before_sixteen += 1;
                }
            }
            if first < 8 * year {
                c.under_eight += 1;
                if p.death.is_some() && last < 8 * year {
                    c.dead_before_eight += 1;
                }
            }
        }
        self.children = c;
    }

    /// The block `pooling` of the JSON (`docs/10-vivarium-data.md`).
    #[must_use]
    pub fn json(&self) -> Value {
        let mut parents = ClassDay::default();
        let mut others = ClassDay::default();
        let (mut put, mut eaten, mut young, mut spoiled) = (0_u64, 0_u64, 0_u64, 0_u64);
        let mut eaters = Eaters::default();
        let days: Vec<Value> = self
            .days
            .iter()
            .map(|(day, d)| {
                parents.add(&d.parents);
                others.add(&d.others);
                eaters.add(&d.eaters);
                let grown = d.parents.bodies + d.others.bodies;
                let out = d.parents.out + d.others.out;
                let gathered = d.parents.gathered + d.others.gathered;
                let (mut dawn, mut p, mut e, mut s) = (0, 0, 0, 0);
                for h in d.hearths.values() {
                    dawn += h.dawn_kj;
                    p += h.put_kj;
                    e += h.eaten_kj;
                    s += h.spoiled_kj;
                    young += h.eaten_young_kj;
                }
                put += p;
                eaten += e;
                spoiled += s;
                json!({
                    "day": day,
                    "reserve_dawn_kj": dawn,
                    "put_kj": p,
                    "eaten_kj": e,
                    "spoiled_kj": s,
                    "grown": grown,
                    "out": out,
                    "out_percent": (out * 100).checked_div(grown).map_or(Value::Null, |p| json!(p)),
                    "gathered": gathered,
                    "gathered_percent": (gathered * 100).checked_div(grown).map_or(Value::Null, |p| json!(p)),
                    "gathering_for": d.gathering_for,
                    "seen_kj": d.seen_kj,
                    "cared_kj": d.cared_kj,
                    "eaters": d.eaters.json(),
                    "hearths": d.hearths.iter().map(|(camp, h)| json!({
                        "camp": [camp.x, camp.y],
                        "dawn_kj": h.dawn_kj,
                        "put_kj": h.put_kj,
                        "eaten_kj": h.eaten_kj,
                        "eaten_young_kj": h.eaten_young_kj,
                        "spoiled_kj": h.spoiled_kj,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        let c = &self.children;
        json!({
            "on": self.on,
            "care": self.care,
            "fed_until_years": self.fed_until_years,
            "totals": {
                "put_kj": put,
                "eaten_kj": eaten,
                "eaten_young_kj": young,
                "spoiled_kj": spoiled,
            },
            "children": {
                "young": c.young,
                "under_eight": c.under_eight,
                "dead_before_eight": c.dead_before_eight,
                "dead_before_sixteen": c.dead_before_sixteen,
                "born": c.born,
            },
            "parents": parents.json(),
            "others": others.json(),
            "eaters": eaters.json(),
            "days": days,
        })
    }
}
