//! The hands under glass (D57): what the vivarium measures of the first
//! invention, the container. Who made the first, when; who carries one,
//! age-year by age-year; the remakes after a tear; the deeds told at the
//! fire and who heard them, missed them, tried them; and what the
//! container changed: the food brought home, how far from home at noon,
//! the homes changed.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use sim::brain::{
    Brains, Deed, HOLLOW, HOME_TILES, News, Way, cut_part, cut_species, kind_hollow, kind_material,
};
use sim::content::Content;
use sim::hands::{Gesture, Part};
use sim::memory::{Record, RecordId};
use sim::{BodyId, Event, World};

use crate::Person;

/// A tree felled (D57, amended 27 November 2026).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Felled {
    /// The day it fell.
    pub day: u64,
    /// Who gave the last cut, and its tribe.
    pub body: BodyId,
    /// See `body`.
    pub tribe: u8,
    /// Its species.
    pub species: String,
    /// How thick its trunk was, in millimetres.
    pub trunk_mm: u32,
    /// The log it left, in grams.
    pub log_g: u32,
}

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

impl KnownResults {
    /// As JSON (`docs/10`, `hands.work.known_results`).
    fn json(&self) -> Value {
        let mut sizes: Vec<u32> = self
            .held
            .iter()
            .map(|(seen, failed)| seen + failed)
            .collect();
        sizes.sort_unstable();
        let mut seen: Vec<u32> = self.held.iter().map(|h| h.0).collect();
        seen.sort_unstable();
        let middle = |v: &[u32]| v.get(v.len().saturating_sub(1) / 2).copied();
        json!({
            "tales_all_known": self.tales_all_known,
            "set_aside_at_draws": self.set_aside,
            "taken_up_again_heard": self.again_heard,
            "taken_up_again_drawn": self.again_drawn,
            "memory": {
                "people": sizes.len(),
                "median": middle(&sizes),
                "max": sizes.last(),
                "gave_a_thing_median": middle(&seen),
                "gave_a_thing_max": seen.last(),
                "bytes_median": middle(&sizes).map(|n| n * 6),
                "bytes_max": sizes.last().map(|n| n * 6),
            },
        })
    }
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
    /// The cuts ended, by the part of the tree cut (D57, amended 27
    /// November 2026, trees in parts): (ended, something came of it).
    pub cuts_by_part: BTreeMap<Part, (u32, u32)>,
    /// The cuts that bit a trunk's living wood, and the chips that fell,
    /// in grams.
    pub bites: u32,
    /// See `bites`.
    pub chips_g: u64,
    /// The branches taken off standing trees.
    pub branches_taken: u32,
    /// The trees felled, in the order they fell.
    pub felled: Vec<Felled>,
    /// Who put a thing in a hollow and it held (D57, amended 27 November
    /// 2026: nothing holds by itself): the first day, and its tribe.
    pub put_first: BTreeMap<BodyId, (u64, u8)>,
    /// The put-ins ended, by tribe: (held, gave nothing).
    pub puts: BTreeMap<u8, (u32, u32)>,
    /// The gestures set aside at a draw because their failure was
    /// remembered, by tribe.
    pub skipped: BTreeMap<u8, u32>,
    /// The cuts, folds and put-ins that gave nothing or broke the thing,
    /// by tribe, body and gesture (the gesture, on what, with or in
    /// what): how many times that body failed that same gesture.
    pub failures: BTreeMap<(u8, BodyId, Gesture, String, String), u32>,
    /// The deeds the living past infancy know at the run's end, by tribe
    /// and name: (knowers, of them holding it from a kept trial of their
    /// own).
    pub known_by: BTreeMap<(u8, String), (u32, u32)>,
    /// The living past infancy who know a put-in of food in a kind of
    /// hollow, by tribe and hollow: each counted once a hollow, whatever
    /// the foods its deeds name.
    pub put_knowers: BTreeMap<(u8, String), u32>,
    /// Who is in a trial of its hands, as the news say (28 November
    /// 2026): begun, till judged, unjudged, or cut short by another.
    pub trying: BTreeSet<BodyId>,
    /// Every gesture ended, in order (28 November 2026): what the round
    /// "seen is not kept" is judged on ([`HandsProbe::work_json`]).
    pub done: Vec<Done>,
    /// The deeds seen give a thing in a trial, each once a trial, and
    /// the deeds kept: as a kept trial's own, and left by a trial not
    /// kept whose chain had served (D57, amended 28 November 2026).
    pub deeds_seen: u32,
    /// See `deeds_seen`.
    pub deeds_kept_own: u32,
    /// See `deeds_seen`.
    pub deeds_kept_left: u32,
    /// Pieces of bark lifted off trunks (D57, amended 28 November 2026).
    pub barks_lifted: u32,
    /// The trials of the hands ended judged, by their one verdict (D50
    /// and D57, amended 28 November 2026): (kept, not kept).
    pub trials_judged: (u32, u32),
    /// The deeds done for what they give on a thing that did not give it
    /// into the hands, remembered of that thing (D57, amended 28
    /// November 2026).
    pub not_as_told: u32,
    /// Free hands (D57, amended 28 November 2026): the things laid at
    /// the feet for a gesture.
    pub free_hands: FreeHands,
    /// A known result is not tried again (D57, amended 28 November
    /// 2026, third round).
    pub results: KnownResults,
}

/// What the memory of results did (D57, amended 28 November 2026, third
/// round): the tales that opened no trial, the gestures set aside, the
/// rare takings up again, and the memory's size.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownResults {
    /// Tales of the hands that would have opened a trial and did not:
    /// the listener knew what every deed of them gives.
    pub tales_all_known: u32,
    /// Gestures set aside at a draw as giving a thing that brought the
    /// body nothing, summed over the draws.
    pub set_aside: u32,
    /// Deeds of a known result taken up again by the rare chance: as an
    /// aim, heard told.
    pub again_heard: u32,
    /// The same, drawn in a trial.
    pub again_drawn: u32,
    /// The results each living person remembers at the last age-year's
    /// end: (gave a thing that brought nothing, gave nothing).
    pub held: Vec<(u32, u32)>,
}

/// What became of the things laid at the feet to free the hands for a
/// gesture (D57, amended 28 November 2026).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FreeHands {
    /// Things laid down.
    pub laid: u32,
    /// Taken up again.
    pub taken_back: u32,
    /// Of them, after the body had gone beyond its hands' sight of them.
    pub taken_back_late: u32,
    /// Left behind: the body went beyond its hands' sight of them.
    pub left: u32,
    /// Taken by another body.
    pub taken_by_another: u32,
    /// Gone where they lay: eaten, spoiled, torn.
    pub gone: u32,
    /// Let be: not wanted any more, or not carried with what was
    /// preferred.
    pub let_be: u32,
}

/// A gesture ended, as the round's figures need it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Done {
    /// The tool it cut with, what it folded, struck or put.
    pub item: sim::ItemId,
    /// The thing it was put in.
    pub into: Option<sim::ItemId>,
    /// What came of it.
    pub gave: Option<sim::ItemId>,
    /// A put-in of food.
    pub food_put: bool,
    /// Done in a trial of the hands.
    pub in_trial: bool,
}

/// Whether a gesture ended is a put-in of food: `on` is then the
/// material put.
fn food_put(content: &Content, gesture: Gesture, on: &str) -> bool {
    gesture == Gesture::PutIn && content.material(on).is_some_and(|m| m.food.is_some())
}

/// The least, the median (the lower of two) and the most of `values`;
/// null when empty.
fn spread(values: &mut [u64]) -> Value {
    values.sort_unstable();
    let (Some(min), Some(max)) = (values.first(), values.last()) else {
        return Value::Null;
    };
    json!({
        "min": min,
        "median": values[(values.len() - 1) / 2],
        "max": max,
    })
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

    /// What an event tells of the trees in parts (D57, amended 27
    /// November 2026): the cuts by part, the bites and their chips, the
    /// branches taken, the trees felled.
    fn trees_event(&mut self, people: &BTreeMap<BodyId, Person>, event: &Event, day: u64) {
        match event {
            Event::Gestured {
                part: Some(part),
                gave,
                ..
            } => {
                let cuts = self.cuts_by_part.entry(*part).or_default();
                cuts.0 += 1;
                cuts.1 += u32::from(gave.is_some());
                self.branches_taken += u32::from(*part == Part::Branch && gave.is_some());
            }
            Event::BarkLifted { .. } => self.barks_lifted += 1,
            Event::TreeBitten { chips_g, .. } => {
                self.bites += 1;
                self.chips_g += u64::from(*chips_g);
            }
            Event::TreeFell {
                body,
                species,
                trunk_mm,
                log_g,
                ..
            } => self.felled.push(Felled {
                day,
                body: *body,
                tribe: tribe(people, *body),
                species: species.clone(),
                trunk_mm: *trunk_mm,
                log_g: *log_g,
            }),
            _ => {}
        }
    }

    /// A gesture ended, kept for the work's figures (28 November 2026).
    fn work_event(&mut self, content: &Content, event: &Event) {
        if let Event::Gestured {
            body,
            gesture,
            item,
            on,
            into,
            gave,
            ..
        } = event
        {
            self.done.push(Done {
                item: *item,
                into: *into,
                gave: *gave,
                food_put: food_put(content, *gesture, on),
                in_trial: self.trying.contains(body),
            });
        }
    }

    /// What a news tells of the work of the hands (28 November 2026):
    /// who is in a trial of its hands (one trial at a time, and another
    /// begun, a move followed, cuts it short), the deeds seen and kept.
    fn work_news(&mut self, news: &News) {
        match news {
            News::Trying { body, way, .. } => {
                if *way == Way::Hands {
                    self.trying.insert(*body);
                } else {
                    self.trying.remove(body);
                }
            }
            News::Tried {
                body, way, kept, ..
            } => {
                self.trying.remove(body);
                if *way == Way::Hands {
                    if *kept {
                        self.trials_judged.0 += 1;
                    } else {
                        self.trials_judged.1 += 1;
                    }
                }
            }
            News::Untried { body, .. } => {
                self.trying.remove(body);
            }
            News::NotAsTold { .. } => self.not_as_told += 1,
            News::TaleKnown { .. } => self.results.tales_all_known += 1,
            News::Skipped { known, .. } => self.results.set_aside += known,
            News::TakenUpAgain { heard: true, .. } => self.results.again_heard += 1,
            News::TakenUpAgain { heard: false, .. } => self.results.again_drawn += 1,
            News::Laid { .. } => self.free_hands.laid += 1,
            News::LaidBack { late, .. } => {
                self.free_hands.taken_back += 1;
                self.free_hands.taken_back_late += u32::from(*late);
            }
            News::LaidLeft { .. } => self.free_hands.left += 1,
            News::LaidLost { by: Some(_), .. } => self.free_hands.taken_by_another += 1,
            News::LaidLost { by: None, .. } => self.free_hands.gone += 1,
            News::LaidLetBe { .. } => self.free_hands.let_be += 1,
            News::DeedSeen { .. } => self.deeds_seen += 1,
            News::DeedKept { own: true, .. } => self.deeds_kept_own += 1,
            News::DeedKept { own: false, .. } => self.deeds_kept_left += 1,
            _ => {}
        }
    }

    /// The work of the hands in a trial and outside (`docs/10`,
    /// `hands.work`; 28 November 2026): the gestures ended, and of them
    /// those that led to food put in a hollow, read backwards as
    /// `tools/lab/journal.py gestures` reads them (a put-in of food, the
    /// thing it was put in, the gesture that gave that thing, the thing
    /// that gesture was done with or on, and so on); the deeds seen and
    /// kept; the bark lifted and the trees felled.
    fn work_json(&self) -> Value {
        let mut useful: BTreeSet<sim::ItemId> = BTreeSet::new();
        // (gestures, of them led to food): outside a trial, in one.
        let mut counts = [(0_u32, 0_u32); 2];
        for done in self.done.iter().rev() {
            let side = &mut counts[usize::from(done.in_trial)];
            side.0 += 1;
            let Some(gave) = done.gave else {
                continue;
            };
            if done.food_put {
                useful.extend(done.into);
            } else if useful.contains(&gave) {
                useful.insert(done.item);
                useful.extend(done.into);
            } else {
                continue;
            }
            side.1 += 1;
        }
        let side = |(gestures, led): (u32, u32)| {
            json!({
                "gestures": gestures,
                "led_to_food": led,
                "led_percent": (gestures > 0).then(|| u64::from(led) * 100 / u64::from(gestures)),
            })
        };
        json!({
            "in_a_trial": side(counts[1]),
            "outside": side(counts[0]),
            "deeds_seen": self.deeds_seen,
            "deeds_kept": self.deeds_kept_own + self.deeds_kept_left,
            "deeds_kept_own": self.deeds_kept_own,
            "barks_lifted": self.barks_lifted,
            "trees_felled": self.felled.len(),
            "trials_kept": self.trials_judged.0,
            "trials_not_kept": self.trials_judged.1,
            "not_as_told": self.not_as_told,
            "known_results": self.results.json(),
            "free_hands": {
                "laid": self.free_hands.laid,
                "taken_back": self.free_hands.taken_back,
                "taken_back_late": self.free_hands.taken_back_late,
                "left": self.free_hands.left,
                "taken_by_another": self.free_hands.taken_by_another,
                "gone": self.free_hands.gone,
                "let_be": self.free_hands.let_be,
            },
        })
    }

    /// The trees' figures as JSON (`docs/10`, `hands.trees`).
    fn trees_json(&self) -> Value {
        json!({
            "cuts": Part::ALL.iter().map(|p| {
                let (done, gave) = self.cuts_by_part.get(p).copied().unwrap_or_default();
                (p.key().to_owned(), json!({"done": done, "gave": gave}))
            }).collect::<serde_json::Map<String, Value>>(),
            "bites": self.bites,
            "chips_g": self.chips_g,
            "branches_taken": self.branches_taken,
            "felled": self.felled.iter().map(|f| json!({
                "day": f.day,
                "body": f.body.0,
                "tribe": tribe_name(f.tribe),
                "species": f.species,
                "trunk_mm": f.trunk_mm,
                "log_g": f.log_g,
            })).collect::<Vec<_>>(),
        })
    }

    /// An event of the world's tick.
    pub fn event(
        &mut self,
        (world, content): (&World, &Content),
        people: &BTreeMap<BodyId, Person>,
        event: &Event,
        (day, age_year): (u64, u64),
    ) {
        self.trees_event(people, event, day);
        self.work_event(content, event);
        match event {
            Event::Gestured {
                body,
                gesture,
                item,
                on,
                into,
                part,
                gave,
                broke,
            } => {
                let on = &part_named(on, *part);
                let entry = self.gestures.entry(*gesture).or_default();
                entry.0 += 1;
                entry.1 += u32::from(gave.is_some());
                // Every cut and fold ended, by what it was done on and
                // with, and what came of it (27 November 2026).
                let with = match gesture {
                    Gesture::Cut => world
                        .item(*item)
                        .map_or_else(|| "?".to_owned(), |i| i.material().to_owned()),
                    // What it was put in: a hollow thing, or not.
                    Gesture::PutIn => into.and_then(|i| world.item(i)).map_or_else(
                        || "?".to_owned(),
                        |i| {
                            if i.hollow_ml() > 0 {
                                format!("hollow {}", i.material())
                            } else {
                                i.material().to_owned()
                            }
                        },
                    ),
                    _ => String::new(),
                };
                let tried = self
                    .tried
                    .entry((*gesture, on.clone(), with.clone()))
                    .or_default();
                match (gave.is_some(), *broke) {
                    (true, _) => tried.0 += 1,
                    (false, false) => tried.1 += 1,
                    (false, true) => tried.2 += 1,
                }
                let t = tribe(people, *body);
                if gave.is_none() {
                    *self
                        .failures
                        .entry((t, *body, *gesture, on.clone(), with))
                        .or_default() += 1;
                }
                if *gesture == Gesture::PutIn {
                    let puts = self.puts.entry(t).or_default();
                    if gave.is_some() {
                        puts.0 += 1;
                        self.put_first.entry(*body).or_insert((day, t));
                    } else {
                        puts.1 += 1;
                    }
                }
                if *gesture == Gesture::Cut {
                    let tool = world
                        .item(*item)
                        .map_or_else(|| "?".to_owned(), |i| i.material().to_owned());
                    let entry = self.cuts_with.entry(tool).or_default();
                    entry.0 += 1;
                    entry.1 += u32::from(gave.is_some());
                    self.barks_dropped += u32::from(dropped(world, content, *gave));
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
            self.work_news(news);
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
                News::Skipped { body, deeds, .. } => {
                    *self.skipped.entry(tribe(people, *body)).or_default() += deeds;
                }
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
        self.known_by.clear();
        self.put_knowers.clear();
        self.results.held.clear();
        for b in world.bodies() {
            if world.is_infant(b, content) || b.condition().vitality.points() == 0 {
                continue;
            }
            let mut served = false;
            if let Some(mind) = brains.mind(b.id()) {
                let hands = mind.handwork();
                let n = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
                self.results
                    .held
                    .push((n(hands.seen.len()), n(hands.failed.len())));
                let mut hollows: Vec<u16> = Vec::new();
                for deed in mind.ways().hands.iter() {
                    let food = content
                        .materials
                        .get(kind_material(deed.on))
                        .is_some_and(|m| m.food.is_some());
                    if deed.gesture == Gesture::PutIn
                        && food
                        && !kind_hollow(deed.on)
                        && !hollows.contains(&deed.with)
                    {
                        hollows.push(deed.with);
                        let hollow = deed_name(
                            content,
                            Deed {
                                gesture: Gesture::Fold,
                                on: deed.with,
                                with: deed.with,
                            },
                        );
                        let hollow = hollow.trim_start_matches("fold ").to_owned();
                        *self
                            .put_knowers
                            .entry((tribe(people, b.id()), hollow))
                            .or_default() += 1;
                    }
                    let name = deed_name(content, deed);
                    *self.known.entry(name.clone()).or_default() += 1;
                    let by = self
                        .known_by
                        .entry((tribe(people, b.id()), name))
                        .or_default();
                    by.0 += 1;
                    by.1 += u32::from(mind.handwork().kept.contains(&deed));
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

    /// "Put in" as a gesture (D57, amended 27 November 2026), for a tribe
    /// or for all (`docs/10`, `hands.put_in`): who made a hollow, who of
    /// them then found that things go in it, how long after; the put-ins
    /// by what came of them; the failures remembered and repeated; the
    /// deeds known and how many hold them from a kept trial of their own.
    fn put_in_json(&self, t: Option<u8>) -> Value {
        let of = |tribe: u8| t.is_none_or(|t| t == tribe);
        let mut first_hollow: BTreeMap<BodyId, u64> = BTreeMap::new();
        for m in self.made.iter().filter(|m| of(m.tribe)) {
            first_hollow.entry(m.body).or_insert(m.day);
        }
        let mut days: Vec<u64> = first_hollow
            .iter()
            .filter_map(|(body, made)| {
                self.put_first
                    .get(body)
                    .map(|(put, _)| put.saturating_sub(*made))
            })
            .collect();
        let found = days.len();
        let (held, nothing) = self
            .puts
            .iter()
            .filter(|(tribe, _)| of(**tribe))
            .fold((0, 0), |sum, (_, n)| (sum.0 + n.0, sum.1 + n.1));
        let skipped: u32 = self
            .skipped
            .iter()
            .filter(|(tribe, _)| of(**tribe))
            .map(|(_, n)| n)
            .sum();
        // Each person's most repeated failure of one same gesture.
        let mut worst: BTreeMap<BodyId, u64> = BTreeMap::new();
        for ((tribe, body, ..), n) in &self.failures {
            if of(*tribe) {
                let most = worst.entry(*body).or_default();
                *most = (*most).max(u64::from(*n));
            }
        }
        let mut worst: Vec<u64> = worst.into_values().collect();
        let mut known: BTreeMap<&str, (u32, u32)> = BTreeMap::new();
        for ((tribe, name), n) in &self.known_by {
            if of(*tribe) {
                let sum = known.entry(name).or_default();
                *sum = (sum.0 + n.0, sum.1 + n.1);
            }
        }
        let mut knowers: BTreeMap<&str, u32> = BTreeMap::new();
        for ((tribe, hollow), n) in &self.put_knowers {
            if of(*tribe) {
                *knowers.entry(hollow).or_default() += n;
            }
        }
        json!({
            "knowers_by_hollow": knowers,
            "hollow_makers": first_hollow.len(),
            "found_put_in": found,
            "never_found": first_hollow.len() - found,
            "days_from_hollow_to_put_in": spread(&mut days),
            "people_who_put_in": self.put_first.values().filter(|(_, tribe)| of(*tribe)).count(),
            "tried": {"held": held, "nothing": nothing},
            "skipped_for_a_failure_remembered": skipped,
            "people_who_failed": worst.len(),
            "same_failure_repeated": spread(&mut worst),
            "known": known.iter().map(|(deed, (knowers, kept))| json!({
                "deed": deed,
                "knowers": knowers,
                "from_a_kept_trial": kept,
            })).collect::<Vec<_>>(),
        })
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
                    "put_in": self.put_in_json(Some(t)),
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
            "trees": self.trees_json(),
            "work": self.work_json(),
            "put_in": self.put_in_json(None),
            "tribes": tribes_json,
        })
    }
}

/// A deed's name: its gesture, what it is done on, and what it cuts with
/// (`cut birch with flint`, `pick up flint`, `fold birch_bark`), or what
/// it is put in (`put berries in hollow birch_bark`); a thing folded into
/// a hollow is named `hollow` and its material.
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
                .get(cut_species(deed.on))
                .map_or("?", |s| s.id.as_str());
            let part = match cut_part(deed.on) {
                Part::Trunk => "",
                Part::Branch => " branch",
            };
            format!("cut {tree}{part} with {}", material(deed.with))
        }
        Gesture::PutIn => format!("put {} in {}", material(deed.on), material(deed.with)),
        Gesture::Strike => format!("strike {} with {}", material(deed.on), material(deed.with)),
        Gesture::TakeOut => format!("take {} out of {}", material(deed.on), material(deed.with)),
        g => format!("{} {}", g.key(), material(deed.on)),
    }
}

/// What a cut was done on, as the figures name it: the species, with
/// its part when it is a branch (D57, amended 27 November 2026).
fn part_named(on: &str, part: Option<Part>) -> String {
    match part {
        Some(Part::Branch) => format!("{on} branch"),
        _ => on.to_owned(),
    }
}

/// Whether what a cut gave fell at the cutter's feet, its load too full
/// to take it: not the chips, which always fall.
fn dropped(world: &World, content: &Content, gave: Option<sim::ItemId>) -> bool {
    let chips = content.needs.hands.trees.as_ref().map(|t| t.chips.as_str());
    gave.and_then(|g| world.item(g))
        .is_some_and(|i| i.pos().is_some() && Some(i.material()) != chips)
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
            into: None,
            part: (gesture == Gesture::Cut).then_some(Part::Trunk),
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

    #[test]
    fn the_trees_figures_count_the_cuts_by_part_the_chips_the_branches_and_the_felled() {
        // D57, amended 27 November 2026: trees in parts.
        let content = Content::sample();
        let mut world = World::new(sim::Calendar::default(), &content);
        let stone = content.materials[0].id.clone();
        let tool = world.spawn_item(&stone, 400, sim::Pos::new(1, 1));
        let people = BTreeMap::new();
        let mut probe = HandsProbe::default();
        let body = BodyId(7);
        let cut = |part, gave: Option<sim::ItemId>| Event::Gestured {
            body,
            gesture: Gesture::Cut,
            item: tool,
            on: "hazel".to_owned(),
            into: None,
            part: Some(part),
            gave,
            broke: false,
        };
        let chunk = sim::ChunkPos::new(0, 0);
        let tree = sim::trees::TreeKey::from_bits(8).expect("a key");
        let bitten = |chips_g, notch_mm2| Event::TreeBitten {
            body,
            chunk,
            tree,
            chips_g,
            notch_mm2,
            section_mm2: 2640,
            version: 1,
        };
        let fell = Event::TreeFell {
            body,
            chunk,
            tree,
            at: sim::FinePos { x: 500, y: 500 },
            species: "hazel".to_owned(),
            trunk_mm: 58,
            log_g: 5300,
            version: 2,
        };
        for event in [
            cut(Part::Trunk, Some(tool)),
            bitten(66, 1320),
            cut(Part::Branch, Some(tool)),
            cut(Part::Branch, None),
            cut(Part::Trunk, Some(tool)),
            bitten(66, 2640),
            fell,
        ] {
            probe.event((&world, &content), &people, &event, (12, 1));
        }
        let json = probe.json(&[], 8, 1);
        assert_eq!(
            json["trees"],
            json!({
                "cuts": {
                    "trunk": {"done": 2, "gave": 2},
                    "branch": {"done": 2, "gave": 1},
                },
                "bites": 2,
                "chips_g": 132,
                "branches_taken": 1,
                "felled": [{
                    "day": 12, "body": 7, "tribe": "A", "species": "hazel",
                    "trunk_mm": 58, "log_g": 5300,
                }],
            })
        );
        // A cut on a branch is told apart in what was tried.
        let tried = json["tried"].as_array().unwrap();
        assert!(
            tried
                .iter()
                .any(|t| t["on"] == "hazel branch" && t["gave"] == 1)
        );
        assert!(tried.iter().any(|t| t["on"] == "hazel" && t["gave"] == 2));
        // And in a deed's name.
        let deed = |on| Deed {
            gesture: Gesture::Cut,
            on,
            with: 0,
        };
        assert_eq!(deed_name(&content, deed(0)).matches(" branch").count(), 0);
        assert_eq!(
            deed_name(&content, deed(sim::brain::BRANCH))
                .matches(" branch with ")
                .count(),
            1
        );
    }

    #[test]
    fn the_put_ins_are_counted_from_the_first_hollow_with_the_failures_repeated() {
        let content = Content::sample();
        let mut world = World::new(sim::Calendar::default(), &content);
        let stone = content.materials[0].id.clone();
        let thing = world.spawn_item(&stone, 400, sim::Pos::new(1, 1));
        let people = BTreeMap::new();
        let mut probe = HandsProbe::default();
        let (first, second) = (BodyId(1), BodyId(2));
        for (body, day) in [(first, 3), (second, 5), (first, 9)] {
            probe.made.push(Made {
                day,
                body,
                tribe: 0,
                remake: false,
                heard: false,
                material: "birch_bark".to_owned(),
            });
        }
        let ended = |body, gesture, gave| Event::Gestured {
            body,
            gesture,
            item: thing,
            on: "berries".to_owned(),
            into: Some(thing),
            part: None,
            gave,
            broke: false,
        };
        // The first puts berries in a stone three times for nothing, then
        // in something that holds, on day 10; the second only cuts for
        // nothing, once.
        for day in 4..7 {
            let event = ended(first, Gesture::PutIn, None);
            probe.event((&world, &content), &people, &event, (day, 1));
        }
        for day in [10, 12] {
            let event = ended(first, Gesture::PutIn, Some(thing));
            probe.event((&world, &content), &people, &event, (day, 2));
        }
        let event = ended(second, Gesture::Cut, None);
        probe.event((&world, &content), &people, &event, (6, 1));
        probe.skipped.insert(0, 4);
        probe
            .known_by
            .insert((0, "fold birch_bark".to_owned()), (2, 1));
        let json = probe.json(&[], 8, 1);
        for put in [&json["put_in"], &json["tribes"][0]["put_in"]] {
            assert_eq!(put["hollow_makers"], 2);
            assert_eq!(
                (&put["found_put_in"], &put["never_found"]),
                (&json!(1), &json!(1))
            );
            assert_eq!(
                put["days_from_hollow_to_put_in"],
                json!({"min": 7, "median": 7, "max": 7})
            );
            assert_eq!(put["people_who_put_in"], 1);
            assert_eq!(put["tried"], json!({"held": 2, "nothing": 3}));
            assert_eq!(put["skipped_for_a_failure_remembered"], 4);
            assert_eq!(put["people_who_failed"], 2);
            assert_eq!(
                put["same_failure_repeated"],
                json!({"min": 1, "median": 1, "max": 3})
            );
            assert_eq!(
                put["known"],
                json!([{"deed": "fold birch_bark", "knowers": 2, "from_a_kept_trial": 1}])
            );
        }
        let tried = json["tried"].as_array().unwrap();
        let put = tried.iter().find(|t| t["gesture"] == "put in").unwrap();
        assert_eq!((&put["with"], &put["gave"]), (&json!(stone), &json!(2)));
        // Nobody made a hollow: nothing to spread.
        let empty = HandsProbe::default().json(&[], 8, 1);
        assert_eq!(empty["put_in"]["days_from_hollow_to_put_in"], Value::Null);
        assert_eq!(empty["put_in"]["same_failure_repeated"], Value::Null);
    }
    #[test]
    fn the_memory_of_results_is_counted_with_what_it_set_aside() {
        let results = KnownResults {
            tales_all_known: 4,
            set_aside: 30,
            again_heard: 1,
            again_drawn: 2,
            held: vec![(3, 1), (10, 2), (40, 9)],
        };
        assert_eq!(
            results.json(),
            json!({
                "tales_all_known": 4,
                "set_aside_at_draws": 30,
                "taken_up_again_heard": 1,
                "taken_up_again_drawn": 2,
                "memory": {
                    "people": 3,
                    "median": 12,
                    "max": 49,
                    "gave_a_thing_median": 10,
                    "gave_a_thing_max": 40,
                    "bytes_median": 72,
                    "bytes_max": 294,
                },
            })
        );
    }

    #[test]
    fn the_work_of_the_hands_is_counted_in_a_trial_and_outside_with_what_led_to_food() {
        // 28 November 2026, "seen is not kept": the figures a run of the
        // round is judged on.
        let content = Content::sample();
        let mut world = World::new(sim::Calendar::default(), &content);
        let stone = content.materials[0].id.clone();
        let food = content
            .materials
            .iter()
            .find(|m| m.food.is_some())
            .expect("a food")
            .id
            .clone();
        let mut thing = || world.spawn_item(&stone, 400, sim::Pos::new(1, 1));
        let (tool, bark, chips, hollow, berries) = (thing(), thing(), thing(), thing(), thing());
        let people = BTreeMap::new();
        let mut probe = HandsProbe::default();
        let body = BodyId(3);
        let ended = |gesture, item, on: &str, into, gave| Event::Gestured {
            body,
            gesture,
            item,
            on: on.to_owned(),
            into,
            part: (gesture == Gesture::Cut).then_some(Part::Trunk),
            gave,
            broke: false,
        };
        let chunk = sim::ChunkPos::new(0, 0);
        let tree = sim::trees::TreeKey::from_bits(8).expect("a key");
        // In a trial: a cut that gives bark, lifted off the trunk, a cut
        // that gives chips, a fold for nothing, the bark folded.
        probe.trying.insert(body);
        for event in [
            ended(Gesture::Cut, tool, "birch", None, Some(bark)),
            Event::BarkLifted {
                body,
                chunk,
                tree,
                left: 3,
                version: 1,
            },
            ended(Gesture::Cut, tool, "hazel", None, Some(chips)),
            ended(Gesture::Fold, chips, "wood_chips", None, None),
            ended(Gesture::Fold, bark, "birch_bark", None, Some(hollow)),
        ] {
            probe.event((&world, &content), &people, &event, (1, 1));
        }
        // Outside: its food put in the hollow, and a stone.
        probe.trying.remove(&body);
        for event in [
            ended(Gesture::PutIn, berries, &food, Some(hollow), Some(berries)),
            ended(Gesture::PutIn, tool, &stone, Some(hollow), Some(tool)),
        ] {
            probe.event((&world, &content), &people, &event, (2, 1));
        }
        probe.deeds_seen = 3;
        probe.deeds_kept_own = 2;
        probe.deeds_kept_left = 1;
        probe.trials_judged = (2, 5);
        probe.not_as_told = 7;
        probe.free_hands = FreeHands {
            laid: 9,
            taken_back: 6,
            taken_back_late: 1,
            left: 2,
            taken_by_another: 1,
            gone: 1,
            let_be: 1,
        };
        assert_eq!(
            probe.json(&[], 4, 1)["work"],
            json!({
                "in_a_trial": {"gestures": 4, "led_to_food": 2, "led_percent": 50},
                "outside": {"gestures": 2, "led_to_food": 1, "led_percent": 50},
                "deeds_seen": 3,
                "deeds_kept": 3,
                "deeds_kept_own": 2,
                "barks_lifted": 1,
                "trees_felled": 0,
                "trials_kept": 2,
                "trials_not_kept": 5,
                "not_as_told": 7,
                "known_results": KnownResults::default().json(),
                "free_hands": {
                    "laid": 9,
                    "taken_back": 6,
                    "taken_back_late": 1,
                    "left": 2,
                    "taken_by_another": 1,
                    "gone": 1,
                    "let_be": 1,
                },
            })
        );
    }
}
