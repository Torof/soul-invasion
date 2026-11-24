//! The hands under glass (D57): what the vivarium measures of the first
//! invention, the container. Who made the first, when; who carries one,
//! age-year by age-year; the remakes after a tear; the deeds told at the
//! fire and who heard them, missed them, tried them; and what the
//! container changed: the food brought home, how far from home at noon,
//! the homes changed.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use sim::brain::{Brains, HOME_TILES, News, Way};
use sim::content::Content;
use sim::hands::Gesture;
use sim::memory::{Record, RecordId};
use sim::{BodyId, Event, World};

use crate::Person;

/// One tribe's age-year of the hands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HandsYear {
    /// Living past infancy at the age-year's end.
    pub living: u32,
    /// Of them, carrying a container.
    pub carriers: u32,
    /// Containers made in the age-year.
    pub made: u32,
    /// Of them, by a body whose container had torn before: remakes.
    pub remade: u32,
    /// Containers torn.
    pub torn: u32,
    /// Trials of the hands begun, and of them begun on a tale heard.
    pub trials: u32,
    /// See `trials`.
    pub trials_heard: u32,
    /// Trials of the hands kept.
    pub kept: u32,
    /// Food carried at nightfall by the grown at home with a container:
    /// kJ summed, and the nights.
    pub home_with_kj: u64,
    /// See `home_with_kj`.
    pub home_with_nights: u32,
    /// The same without a container.
    pub home_without_kj: u64,
    /// See `home_without_kj`.
    pub home_without_nights: u32,
    /// Distance from home at noon of the grown, on each axis the larger:
    /// tiles summed, and the noons.
    pub noon_tiles: u64,
    /// See `noon_tiles`.
    pub noons: u32,
}

/// A container made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Made {
    /// The day.
    pub day: u64,
    /// Who.
    pub body: BodyId,
    /// Its tribe.
    pub tribe: u8,
    /// A remake: its own container had torn before.
    pub remake: bool,
    /// It had heard a deed of the hands told before.
    pub heard: bool,
}

/// A deed of the hands told at the fire (D55).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToldHands {
    /// The day.
    pub day: u64,
    /// The record.
    pub record: RecordId,
    /// The teller.
    pub teller: BodyId,
    /// The teller's tribe.
    pub tribe: u8,
    /// Who believed it that night.
    pub heard: u32,
    /// Its tribe's living past infancy not at that fire.
    pub missed: u32,
}

/// What the vivarium measures of the hands (D57).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HandsProbe {
    /// By (tribe, age-year from 1).
    pub years: BTreeMap<(u8, u64), HandsYear>,
    /// Every container made, in order.
    pub made: Vec<Made>,
    /// Every deed of the hands told, in order.
    pub told: Vec<ToldHands>,
    /// Who heard a deed of the hands, with the first day, and from which
    /// tribe's teller.
    pub heard: BTreeMap<BodyId, (u64, u8)>,
    /// Who tried its hands on a tale heard, first day.
    pub tried_heard: BTreeMap<BodyId, u64>,
    /// Whose container tore.
    pub torn_once: BTreeSet<BodyId>,
    /// Each gesture's tries, by gesture: (begun, something came of it).
    pub gestures: BTreeMap<Gesture, (u32, u32)>,
    /// Heard across tribes: a listener of the other tribe than its teller.
    pub heard_across: u32,
}

fn tribe(people: &BTreeMap<BodyId, Person>, id: BodyId) -> u8 {
    people.get(&id).map_or(0, |p| p.tribe)
}

/// Whether `body` carries a container.
pub fn carries_container(world: &World, content: &Content, body: &sim::Body) -> bool {
    body.inventory().any(|i| {
        world
            .item(i)
            .and_then(|i| content.material(i.material()))
            .is_some_and(|m| m.holds_g > 0)
    })
}

fn is_container(content: &Content, material: &str) -> bool {
    content.material(material).is_some_and(|m| m.holds_g > 0)
}

impl HandsProbe {
    fn year(&mut self, tribe: u8, age_year: u64) -> &mut HandsYear {
        self.years.entry((tribe, age_year)).or_default()
    }

    /// An event of the world's tick.
    pub fn event(
        &mut self,
        (world, content): (&World, &Content),
        people: &BTreeMap<BodyId, Person>,
        event: &Event,
        (day, age_year): (u64, u64),
    ) {
        match event {
            Event::Gestured {
                body,
                gesture,
                gave,
                ..
            } => {
                let entry = self.gestures.entry(*gesture).or_default();
                entry.0 += 1;
                entry.1 += u32::from(gave.is_some());
                let made = *gesture == Gesture::Fold
                    && gave
                        .and_then(|g| world.item(g))
                        .is_some_and(|i| is_container(content, i.material()));
                if made {
                    let t = tribe(people, *body);
                    let remake = self.torn_once.contains(body);
                    let heard = self.heard.get(body).is_some_and(|(d, _)| *d <= day);
                    self.made.push(Made {
                        day,
                        body: *body,
                        tribe: t,
                        remake,
                        heard,
                    });
                    let y = self.year(t, age_year);
                    y.made += 1;
                    y.remade += u32::from(remake);
                }
            }
            Event::Spoiled {
                material,
                holder: Some(body),
                ..
            } if is_container(content, material) => {
                self.torn_once.insert(*body);
                let t = tribe(people, *body);
                self.year(t, age_year).torn += 1;
            }
            _ => {}
        }
    }

    /// The brains' news of a turn: the trials of the hands, the deeds told
    /// and heard.
    pub fn news(
        &mut self,
        brains: &Brains,
        (world, content): (&World, &Content),
        people: &BTreeMap<BodyId, Person>,
        batch: &[News],
        (day, age_year): (u64, u64),
    ) {
        let memory = brains.memory();
        let is_hands = |record: RecordId| {
            matches!(
                memory.get(record),
                Some(Record::Trial {
                    way: Way::Hands,
                    kept: true,
                    ..
                })
            )
        };
        for news in batch {
            match news {
                News::Trying {
                    body,
                    way: Way::Hands,
                    heard,
                    ..
                } => {
                    let y = self.year(tribe(people, *body), age_year);
                    y.trials += 1;
                    y.trials_heard += u32::from(*heard);
                    if *heard {
                        self.tried_heard.entry(*body).or_insert(day);
                    }
                }
                News::Tried {
                    body,
                    way: Way::Hands,
                    kept: true,
                    ..
                } => self.year(tribe(people, *body), age_year).kept += 1,
                News::Told {
                    body, record, camp, ..
                } if is_hands(*record) => {
                    let t = tribe(people, *body);
                    let heard = batch
                        .iter()
                        .filter(|n| {
                            matches!(n, News::Heard { record: r, from, .. }
                                if r == record && from == body)
                        })
                        .count();
                    let present = brains
                        .hearths()
                        .get(camp)
                        .and_then(|ids| ids.last())
                        .and_then(|id| match memory.get(*id) {
                            Some(Record::Night { present, .. }) => Some(present.len()),
                            _ => None,
                        })
                        .unwrap_or(0);
                    let members = world
                        .bodies()
                        .iter()
                        .filter(|b| {
                            tribe(people, b.id()) == t
                                && !world.is_infant(b, content)
                                && b.condition().vitality.points() > 0
                        })
                        .count();
                    self.told.push(ToldHands {
                        day,
                        record: *record,
                        teller: *body,
                        tribe: t,
                        heard: u32::try_from(heard).unwrap_or(u32::MAX),
                        missed: u32::try_from(members.saturating_sub(present)).unwrap_or(u32::MAX),
                    });
                }
                News::Heard {
                    body, record, from, ..
                } if is_hands(*record) => {
                    let (listener, teller) = (tribe(people, *body), tribe(people, *from));
                    self.heard_across += u32::from(listener != teller);
                    self.heard.entry(*body).or_insert((day, teller));
                }
                _ => {}
            }
        }
    }

    /// Nightfall: the food the grown at home carry, with a container or
    /// without.
    pub fn nightfall(
        &mut self,
        (world, content): (&World, &Content),
        brains: &Brains,
        people: &BTreeMap<BodyId, Person>,
        age_year: u64,
    ) {
        for b in world.bodies() {
            if world.age_milli(b) < 16_000 || b.condition().vitality.points() == 0 {
                continue;
            }
            let Some(home) = brains.mind(b.id()).map(sim::brain::Mind::home) else {
                continue;
            };
            let here = b.pos();
            if (here.x - home.x).abs() > HOME_TILES || (here.y - home.y).abs() > HOME_TILES {
                continue;
            }
            let kj = sim::brain::carried_kj(world, content, b.id());
            let with = carries_container(world, content, b);
            let y = self.year(tribe(people, b.id()), age_year);
            if with {
                y.home_with_kj += kj;
                y.home_with_nights += 1;
            } else {
                y.home_without_kj += kj;
                y.home_without_nights += 1;
            }
        }
    }

    /// Noon: how far from home the grown stand.
    pub fn noon(
        &mut self,
        world: &World,
        brains: &Brains,
        people: &BTreeMap<BodyId, Person>,
        age_year: u64,
    ) {
        for b in world.bodies() {
            if world.age_milli(b) < 16_000 || b.condition().vitality.points() == 0 {
                continue;
            }
            let Some(home) = brains.mind(b.id()).map(sim::brain::Mind::home) else {
                continue;
            };
            let here = b.pos();
            let tiles = (here.x - home.x).abs().max((here.y - home.y).abs());
            let y = self.year(tribe(people, b.id()), age_year);
            y.noon_tiles += u64::try_from(tiles).unwrap_or(0);
            y.noons += 1;
        }
    }

    /// The age-year's end: who carries a container.
    pub fn age_year_end(
        &mut self,
        (world, content): (&World, &Content),
        people: &BTreeMap<BodyId, Person>,
        tribes: u8,
        age_year: u64,
    ) {
        for t in 0..tribes {
            self.year(t, age_year);
        }
        for b in world.bodies() {
            if world.is_infant(b, content) || b.condition().vitality.points() == 0 {
                continue;
            }
            let with = carries_container(world, content, b);
            let y = self.year(tribe(people, b.id()), age_year);
            y.living += 1;
            y.carriers += u32::from(with);
        }
    }

    /// The probe as JSON (`docs/10`, section `hands`), with the homes
    /// changed by age-year (`changes`: day and tribe of each) and the days
    /// of an age-year.
    #[must_use]
    pub fn json(&self, changes: &[(u64, u8)], days_per_age_year: u64, tribes: u8) -> Value {
        let per = days_per_age_year.max(1);
        let first = |t: Option<u8>| {
            self.made
                .iter()
                .find(|m| t.is_none_or(|t| m.tribe == t))
                .map(|m| {
                    json!({
                        "day": m.day,
                        "age_year_x10": m.day * 10 / per,
                        "body": m.body.0,
                        "tribe": tribe_name(m.tribe),
                        "heard_before": m.heard,
                    })
                })
        };
        let first_made = self.made.first();
        // The first other body to make one after hearing a deed told.
        let first_listener = first_made.and_then(|f| {
            self.made
                .iter()
                .find(|m| m.body != f.body && m.heard)
                .map(|m| {
                    json!({
                        "day": m.day,
                        "body": m.body.0,
                        "tribe": tribe_name(m.tribe),
                        "nights_after_first": m.day - f.day,
                    })
                })
        });
        let heard_never_tried = self
            .heard
            .keys()
            .filter(|b| !self.tried_heard.contains_key(b))
            .count();
        let makers: BTreeSet<BodyId> = self.made.iter().map(|m| m.body).collect();
        let tribes_json: Vec<Value> = (0..tribes)
            .map(|t| {
                let years: Vec<Value> = self
                    .years
                    .iter()
                    .filter(|((tt, _), _)| *tt == t)
                    .map(|((_, ay), y)| year_json(*ay, y, changes, t, per))
                    .collect();
                let heard = self.heard.values().filter(|(_, from)| *from == t).count();
                json!({
                    "tribe": tribe_name(t),
                    "first": first(Some(t)),
                    "makers": self.made.iter().filter(|m| m.tribe == t).map(|m| m.body).collect::<BTreeSet<_>>().len(),
                    "told": self.told.iter().filter(|x| x.tribe == t).count(),
                    "heard_from_tribe": heard,
                    "years": years,
                })
            })
            .collect();
        let gestures: Value = self
            .gestures
            .iter()
            .map(|(g, (tried, gave))| (g.key().to_owned(), json!({"done": tried, "gave": gave})))
            .collect::<serde_json::Map<String, Value>>()
            .into();
        json!({
            "first": first(None),
            "first_listener_made": first_listener,
            "made": self.made.len(),
            "remakes": self.made.iter().filter(|m| m.remake).count(),
            "makers": makers.len(),
            "told": self.told.iter().map(|t| json!({
                "day": t.day,
                "record": t.record,
                "teller": t.teller.0,
                "tribe": tribe_name(t.tribe),
                "heard": t.heard,
                "missed": t.missed,
            })).collect::<Vec<_>>(),
            "heard": self.heard.len(),
            "heard_never_tried": heard_never_tried,
            "heard_across_tribes": self.heard_across,
            "gestures": gestures,
            "tribes": tribes_json,
        })
    }
}

/// One tribe's age-year as JSON (`docs/10`, `hands.tribes[].years[]`).
fn year_json(ay: u64, y: &HandsYear, changes: &[(u64, u8)], t: u8, per: u64) -> Value {
    let homes = changes
        .iter()
        .filter(|(d, tt)| *tt == t && d / per + 1 == ay)
        .count();
    let mean = |kj: u64, n: u32| (n > 0).then(|| kj / u64::from(n));
    json!({
        "age_year": ay,
        "living": y.living,
        "carriers": y.carriers,
        "made": y.made,
        "remade": y.remade,
        "torn": y.torn,
        "trials": y.trials,
        "trials_heard": y.trials_heard,
        "kept": y.kept,
        "home_with_kj": mean(y.home_with_kj, y.home_with_nights),
        "home_with_nights": y.home_with_nights,
        "home_without_kj": mean(y.home_without_kj, y.home_without_nights),
        "home_without_nights": y.home_without_nights,
        "noon_tiles_x10": (y.noons > 0).then(|| y.noon_tiles * 10 / u64::from(y.noons)),
        "home_changes": homes,
    })
}

fn tribe_name(t: u8) -> String {
    char::from(b'A' + t).to_string()
}
