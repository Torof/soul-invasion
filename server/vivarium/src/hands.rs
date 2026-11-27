//! The hands under glass (D57): what the vivarium measures of the first
//! invention, the container. Who made the first, when; who carries one,
//! age-year by age-year; the remakes after a tear; the deeds told at the
//! fire and who heard them, missed them, tried them; and what the
//! container changed: the food brought home, how far from home at noon,
//! the homes changed.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use sim::brain::{Brains, Deed, HOLLOW, HOME_TILES, News, Way, kind_hollow, kind_material};
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
    /// Of them, carrying a thing that once served them (a flint that cut,
    /// a container that held food), kept as their own memory of use.
    pub served_carriers: u32,
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
    /// Trials of the hands ended unjudged: nothing new within reach to
    /// try (D57, amended 25 November 2026).
    pub untried: u32,
    /// Trials of the hands that drew a gesture at random: begun on no
    /// tale, and begun on a tale (a free hand where no heard gesture
    /// could be made).
    pub drew_free: u32,
    /// See `drew_free`.
    pub drew_heard: u32,
    /// Tools taken up for a cut known or heard.
    pub tools_taken: u32,
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

impl HandsYear {
    /// A trial of the hands ended unjudged, a gesture drawn, a tool taken
    /// up (D57, amended 25 November 2026).
    fn count(&mut self, news: &News) {
        match news {
            News::Untried { .. } => self.untried += 1,
            News::Drew { heard: true, .. } => self.drew_heard += 1,
            News::Drew { heard: false, .. } => self.drew_free += 1,
            News::ToolTaken { .. } => self.tools_taken += 1,
            _ => {}
        }
    }
}

/// A container made.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// The material it is folded of.
    pub material: String,
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
    /// What it told: `kept trial`, or a novelty by its deed (`cut birch
    /// with flint`).
    pub what: String,
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
    /// Every cut and fold ended, by gesture, what it was done on (the
    /// material folded, the species of the tree cut) and the material of
    /// the tool of a cut (empty for a fold): how many gave something,
    /// gave nothing, broke the thing (27 November 2026).
    pub tried: BTreeMap<(Gesture, String, String), (u32, u32, u32)>,
    /// The containers made, by the material folded.
    pub made_of: BTreeMap<String, u32>,
    /// Heard across tribes: a listener of the other tribe than its teller.
    pub heard_across: u32,
    /// The deeds the living past infancy know, by name, with their
    /// knowers: at the last age-year's end, the run's.
    pub known: BTreeMap<String, u32>,
    /// The cuts, by the material of the tool: (ended, something came of
    /// it).
    pub cuts_with: BTreeMap<String, (u32, u32)>,
    /// Cuts whose bark fell at the cutter's feet, its load too full to
    /// take it (D57, amended 25 November 2026: no longer chosen; only a
    /// load filled during the gesture lets one fall).
    pub barks_dropped: u32,
}

fn tribe(people: &BTreeMap<BodyId, Person>, id: BodyId) -> u8 {
    people.get(&id).map_or(0, |p| p.tribe)
}

/// Whether `body` carries a container.
pub fn carries_container(world: &World, _content: &Content, body: &sim::Body) -> bool {
    body.inventory()
        .any(|i| world.item(i).is_some_and(|i| i.hollow_ml() > 0))
}

/// Whether a thing of `material` that spoiled was a container torn: only
/// a thing folded into a hollow wears (D57, amended 27 November 2026),
/// and food spoils by its own keeping.
fn tore(content: &Content, material: &str) -> bool {
    content
        .material(material)
        .is_some_and(|m| m.food.is_none() && m.days_worn(4) > 0)
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
                item,
                on,
                gave,
                broke,
            } => {
                let entry = self.gestures.entry(*gesture).or_default();
                entry.0 += 1;
                entry.1 += u32::from(gave.is_some());
                // Every cut and fold ended, by what it was done on and
                // with, and what came of it (27 November 2026).
                let with = if *gesture == Gesture::Cut {
                    world
                        .item(*item)
                        .map_or_else(|| "?".to_owned(), |i| i.material().to_owned())
                } else {
                    String::new()
                };
                let tried = self.tried.entry((*gesture, on.clone(), with)).or_default();
                match (gave.is_some(), *broke) {
                    (true, _) => tried.0 += 1,
                    (false, false) => tried.1 += 1,
                    (false, true) => tried.2 += 1,
                }
                if *gesture == Gesture::Cut {
                    let tool = world
                        .item(*item)
                        .map_or_else(|| "?".to_owned(), |i| i.material().to_owned());
                    let entry = self.cuts_with.entry(tool).or_default();
                    entry.0 += 1;
                    entry.1 += u32::from(gave.is_some());
                    self.barks_dropped += u32::from(
                        gave.and_then(|g| world.item(g))
                            .is_some_and(|i| i.pos().is_some()),
                    );
                }
                let made = *gesture == Gesture::Fold
                    && gave
                        .and_then(|g| world.item(g))
                        .is_some_and(|i| i.hollow_ml() > 0);
                if made {
                    *self.made_of.entry(on.clone()).or_default() += 1;
                    let t = tribe(people, *body);
                    let remake = self.torn_once.contains(body);
                    let heard = self.heard.get(body).is_some_and(|(d, _)| *d <= day);
                    self.made.push(Made {
                        day,
                        body: *body,
                        tribe: t,
                        remake,
                        heard,
                        material: on.clone(),
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
            } if tore(content, material) => {
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
                Some(
                    Record::Trial {
                        way: Way::Hands,
                        kept: true,
                        ..
                    } | Record::Deed { .. }
                )
            )
        };
        // What a telling says: a kept trial, or a novelty by its deed.
        let what = |record: RecordId| match memory.get(record) {
            Some(Record::Deed { deed, .. }) => deed_name(content, *deed),
            _ => "kept trial".to_owned(),
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
                News::Untried { body, .. }
                | News::Drew { body, .. }
                | News::ToolTaken { body, .. } => {
                    self.year(tribe(people, *body), age_year).count(news);
                }
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
                        what: what(*record),
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
        (brains, people): (&Brains, &BTreeMap<BodyId, Person>),
        tribes: u8,
        age_year: u64,
    ) {
        for t in 0..tribes {
            self.year(t, age_year);
        }
        self.known.clear();
        for b in world.bodies() {
            if world.is_infant(b, content) || b.condition().vitality.points() == 0 {
                continue;
            }
            let mut served = false;
            if let Some(mind) = brains.mind(b.id()) {
                for deed in mind.ways().hands.iter() {
                    *self.known.entry(deed_name(content, deed)).or_default() += 1;
                }
                served = b.inventory().any(|i| {
                    world
                        .item(i)
                        .and_then(|i| {
                            let m = content
                                .materials
                                .iter()
                                .position(|m| m.id == i.material())?;
                            let m = u16::try_from(m).ok()?;
                            Some(if i.hollow_ml() > 0 { m | HOLLOW } else { m })
                        })
                        .is_some_and(|m| mind.handwork().served(m))
                });
            }
            let with = carries_container(world, content, b);
            let y = self.year(tribe(people, b.id()), age_year);
            y.served_carriers += u32::from(served);
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
                        "material": m.material,
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
                "what": t.what,
                "heard": t.heard,
                "missed": t.missed,
            })).collect::<Vec<_>>(),
            "heard": self.heard.len(),
            "heard_never_tried": heard_never_tried,
            "heard_across_tribes": self.heard_across,
            "gestures": gestures,
            "tried": self.tried.iter().map(|((g, on, with), (gave, nothing, broke))| json!({
                "gesture": g.key(),
                "on": on,
                "with": (!with.is_empty()).then_some(with),
                "gave": gave,
                "nothing": nothing,
                "broke": broke,
            })).collect::<Vec<_>>(),
            "made_of": self.made_of,
            "known": self.known,
            "cuts_with": self.cuts_with.iter().map(|(m, (done, gave))| (m.clone(), json!({"done": done, "gave": gave}))).collect::<serde_json::Map<String, Value>>(),
            "barks_dropped": self.barks_dropped,
            "tribes": tribes_json,
        })
    }
}

/// A deed's name: its gesture, what it is done on, and what it cuts with
/// (`cut birch with flint`, `pick up flint`, `fold birch_bark`); a thing
/// folded into a hollow is named `hollow` and its material.
fn deed_name(content: &Content, deed: Deed) -> String {
    let material = |m: u16| {
        let id = content
            .materials
            .get(kind_material(m))
            .map_or("?", |m| m.id.as_str());
        if kind_hollow(m) {
            format!("hollow {id}")
        } else {
            id.to_owned()
        }
    };
    match deed.gesture {
        Gesture::Cut => {
            let tree = content
                .species
                .get(usize::from(deed.on))
                .map_or("?", |s| s.id.as_str());
            format!("cut {tree} with {}", material(deed.with))
        }
        g => format!("{} {}", g.key(), material(deed.on)),
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
        "served_carriers": y.served_carriers,
        "made": y.made,
        "remade": y.remade,
        "torn": y.torn,
        "trials": y.trials,
        "trials_heard": y.trials_heard,
        "kept": y.kept,
        "untried": y.untried,
        "drew_free": y.drew_free,
        "drew_heard": y.drew_heard,
        "tools_taken": y.tools_taken,
        "home_with_kj": mean(y.home_with_kj, y.home_with_nights),
        "home_with_nights": y.home_with_nights,
        "home_without_kj": mean(y.home_without_kj, y.home_without_nights),
        "home_without_nights": y.home_without_nights,
        "noon_tiles_x10": (y.noons > 0).then(|| y.noon_tiles * 10 / u64::from(y.noons)),
        "home_changes": homes,
    })
}

impl HandsProbe {
    /// The full page's section (D57), from the probe's JSON: the first
    /// container, the tellings, and each tribe's age-years.
    #[must_use]
    pub fn html(&self, data: &Value) -> String {
        use std::fmt::Write as _;
        let mut h = String::from("<h2>The hands</h2>");
        let first = &data["first"];
        if first.is_null() {
            h.push_str("<p>No container was made.</p>");
        } else {
            let _ = write!(
                h,
                "<p>The first container: day {}, by {} of tribe {}. {} made in all ({} remakes after a tear) by {} bodies; {} tellings at the fire, {} listeners, {} of them never tried; heard across the tribes {} times.</p>",
                first["day"],
                first["body"],
                first["tribe"].as_str().unwrap_or("?"),
                data["made"],
                data["remakes"],
                data["makers"],
                data["told"].as_array().map_or(0, Vec::len),
                data["heard"],
                data["heard_never_tried"],
                data["heard_across_tribes"]
            );
        }
        let known: Vec<String> = data["known"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(deed, n)| format!("{deed}: {n}"))
            .collect();
        let _ = write!(
            h,
            "<p>Deeds known at the end: {}.</p>",
            if known.is_empty() {
                "none".to_owned()
            } else {
                known.join(", ")
            }
        );
        let show = |v: &Value| {
            if v.is_null() {
                "–".to_owned()
            } else {
                v.to_string()
            }
        };
        for tribe in data["tribes"].as_array().into_iter().flatten() {
            let _ = write!(
                h,
                "<h3>Tribe {}</h3><p>First container: {}.</p><table><tr><th>age-year</th><th>carriers / living</th><th>made</th><th>remade</th><th>torn</th><th>trials (heard)</th><th>kept</th><th>untried</th><th>drew free / on a tale</th><th>tools taken</th><th>kJ home with / without</th><th>noon tiles</th><th>homes changed</th></tr>",
                tribe["tribe"].as_str().unwrap_or("?"),
                if tribe["first"].is_null() {
                    "none".to_owned()
                } else {
                    format!("day {}", tribe["first"]["day"])
                }
            );
            for y in tribe["years"].as_array().into_iter().flatten() {
                let noon = y["noon_tiles_x10"]
                    .as_u64()
                    .map_or("–".to_owned(), |n| format!("{}.{}", n / 10, n % 10));
                let _ = write!(
                    h,
                    "<tr><td>{}</td><td>{} / {}</td><td>{}</td><td>{}</td><td>{}</td><td>{} ({})</td><td>{}</td><td>{}</td><td>{} / {}</td><td>{}</td><td>{} / {}</td><td>{noon}</td><td>{}</td></tr>",
                    y["age_year"],
                    y["carriers"],
                    y["living"],
                    y["made"],
                    y["remade"],
                    y["torn"],
                    y["trials"],
                    y["trials_heard"],
                    y["kept"],
                    y["untried"],
                    y["drew_free"],
                    y["drew_heard"],
                    y["tools_taken"],
                    show(&y["home_with_kj"]),
                    show(&y["home_without_kj"]),
                    y["home_changes"]
                );
            }
            h.push_str("</table>");
        }
        h
    }
}

fn tribe_name(t: u8) -> String {
    char::from(b'A' + t).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_age_year_counts_the_untried_the_drawn_and_the_tools_taken() {
        let body = BodyId(1);
        let mut y = HandsYear::default();
        for news in [
            News::Untried { body, at: 1 },
            News::Drew {
                body,
                heard: true,
                at: 2,
            },
            News::Drew {
                body,
                heard: false,
                at: 3,
            },
            News::Drew {
                body,
                heard: true,
                at: 4,
            },
            News::ToolTaken {
                body,
                tool: 0,
                at: 5,
            },
        ] {
            y.count(&news);
        }
        assert_eq!(
            (y.untried, y.drew_free, y.drew_heard, y.tools_taken),
            (1, 1, 2, 1)
        );
        let json = year_json(1, &y, &[], 0, 8);
        assert_eq!(json["untried"], 1);
        assert_eq!(json["drew_heard"], 2);
        assert_eq!(json["tools_taken"], 1);
    }

    #[test]
    fn every_cut_and_fold_is_counted_by_what_it_was_done_on_and_what_came_of_it() {
        use sim::ItemId;
        let content = Content::sample();
        let mut world = World::new(sim::Calendar::default(), &content);
        let stone = content.materials[0].id.clone();
        let tool = world.spawn_item(&stone, 400, sim::Pos::new(1, 1));
        let people = BTreeMap::new();
        let mut probe = HandsProbe::default();
        let body = BodyId(1);
        let ended = |gesture, on: &str, gave: Option<ItemId>, broke| Event::Gestured {
            body,
            gesture,
            item: tool,
            on: on.to_owned(),
            gave,
            broke,
        };
        for event in [
            ended(Gesture::Cut, "oak", None, false),
            ended(Gesture::Cut, "oak", None, false),
            ended(Gesture::Cut, "birch", Some(tool), false),
            ended(Gesture::Fold, "oak_bark", None, true),
            ended(Gesture::Fold, "fibres", None, false),
        ] {
            probe.event((&world, &content), &people, &event, (3, 1));
        }
        let json = probe.json(&[], 8, 1);
        let tried = json["tried"].as_array().unwrap();
        let find = |gesture: &str, on: &str| {
            tried
                .iter()
                .find(|t| t["gesture"] == gesture && t["on"] == on)
                .unwrap_or_else(|| panic!("{gesture} {on}"))
        };
        let oak = find("cut", "oak");
        assert_eq!((&oak["with"], &oak["nothing"]), (&json!(stone), &json!(2)));
        assert_eq!(find("cut", "birch")["gave"], 1);
        let broken = find("fold", "oak_bark");
        assert_eq!(
            (&broken["broke"], &broken["with"]),
            (&json!(1), &Value::Null)
        );
        assert_eq!(find("fold", "fibres")["nothing"], 1);
        assert_eq!(json["made_of"], json!({}));
        assert_eq!(json["first"], Value::Null);
    }
}
