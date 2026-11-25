//! La chaîne du manque (24 November 2026, the project lead: "c'est les
//! bonnes métriques… Déménager, l'épuisement, aller loin très important,
//! la vie, les couples, explorer très important"): the few measures the
//! data did not hold yet, kept as cheap counters beside the run.
//!
//! What the lab works out from the rest of the data (what is eaten, the
//! distances at noon, the moves, the births and deaths, the couples) is
//! not here. Here: what the people need and walk and how long they work at
//! a plant, by tribe and year; why the grown bodies away from home at dawn
//! lie there; how far each looking day went from home; and (25 November
//! 2026) each outing from home, its nights out and how far it went, and
//! what the bodies chose at their point of no return. A measure only:
//! it reads the world and the minds, and changes nothing.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use sim::brain::{Act, Brains, News, TripChoice, TripWhen};
use sim::content::Content;
use sim::map::FinePos;
use sim::{Blockage, BodyId, Event, World};

use crate::{Person, home_of, tiles_apart, tribe_name};

/// A tribe's year (`(tribe, year)`, the season's year from 1): what the
/// living needed, walked and worked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChainYear {
    /// The living, counted once a game hour: person-hours.
    pub lived_hours: u64,
    /// Of them, those past infancy: the walkers.
    pub walker_hours: u64,
    /// What the living burn, a twenty-fourth of their day's need each
    /// hour, in joules (an infant burns nothing, a nursing woman more).
    pub need_j: u64,
    /// What their walking cost on top, as the world charges it, in joules.
    pub walk_j: u64,
    /// The walkers' walking, in millimetres.
    pub walked_mm: u64,
    /// Ticks a walker spent at a work (gathering at a plant, D45).
    pub gathering_ticks: u64,
}

/// Why a grown body away from home at dawn lies there (the dawns of
/// `nights_out`, by age-year from the first, in the same order), by how it
/// lay down last.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AwayWhy {
    /// Collapsed of fatigue: the world laid it down, its fatigue full.
    pub exhausted: u32,
    /// Lay down by its brain with no way home left, neither by its way
    /// nor for a goal: stopped.
    pub stopped: u32,
    /// Of them, by what barred its last walk since dawn: water, rock, no
    /// way seen, or nothing told.
    pub stopped_by: [u32; 4],
    /// Lay down where it stood, its way being to sleep out (D50).
    pub sleep_out: u32,
    /// Lay down where it stood, holding a goal beyond a day (D56).
    pub goal: u32,
    /// Lay down where it stood out on a trip it chose (25 November 2026).
    pub trip: u32,
    /// Lay down within the fire's reach of its home: the home moved since.
    pub lay_at_home: u32,
    /// Awake at dawn and never lay down since the dawn before: it walked
    /// or stood the night through.
    pub awake: u32,
    /// Awake at dawn, having lain down since the dawn before and risen in
    /// the night (to drink, called away): on its feet away from home.
    pub woke: u32,
    /// Asleep, its lying down not seen (since the run's first tick).
    pub unseen: u32,
    /// The fatigue at lying down of those who lay down, summed, and how
    /// many.
    pub fatigue: (u64, u32),
}

/// How a body lay down last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lay {
    Exhausted,
    Stopped(Option<Blockage>),
    SleepOut,
    Goal,
    Trip,
    AtHome,
}

/// What the probe keeps of a living body between ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Track {
    tribe: u8,
    at: FinePos,
    asleep: bool,
    /// The tick it last rose.
    rose: u64,
    /// Its last lying down, with its fatigue and the tick then.
    lay: Option<(Lay, u32, u64)>,
    /// What barred its last walk since dawn.
    barred: Option<Blockage>,
    /// The farthest it stood from home since its last day closed, on the
    /// longer axis, looked at once a game minute.
    far: u32,
    /// The dawns it woke away from home since it last woke at home.
    out_nights: u32,
    /// The farthest it stood from home since it last woke at home.
    out_far: u32,
    /// Who it was when its outing began (25 November 2026).
    out_who: Option<Who>,
}

/// Who a body was when an outing began (25 November 2026): its will, a
/// parent of a living child under 8, under 16, its sex, its age.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Who {
    /// Its will, 0 to 100.
    pub will: u8,
    /// A parent of a living child under 8 age-years.
    pub parent_under_8: bool,
    /// A parent of a living child under 16 age-years.
    pub parent_under_16: bool,
    /// A woman.
    pub female: bool,
    /// Its age, in whole age-years.
    pub age_years: u32,
}

impl Who {
    fn of(world: &World, brains: &Brains, body: &sim::Body) -> Self {
        let id = body.id();
        let youngest = world
            .bodies()
            .iter()
            .filter(|c| {
                c.condition().vitality.points() > 0
                    && (c.mother() == Some(id) || c.father() == Some(id))
            })
            .map(|c| world.age_milli(c))
            .min();
        Self {
            will: brains.mind(id).map_or(0, sim::brain::Mind::will),
            parent_under_8: youngest.is_some_and(|a| a < 8_000),
            parent_under_16: youngest.is_some_and(|a| a < 16_000),
            female: body.sex() == sim::content::Sex::Female,
            age_years: u32::try_from(world.age_milli(body) / 1000).unwrap_or(u32::MAX),
        }
    }
}

/// An outing from home (25 November 2026): the dawn it woke home again,
/// the tribe, the nights it slept away (0: back by the next dawn), the
/// farthest from home, in tiles on the longer axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outing {
    /// The day it woke home again.
    pub day: u64,
    /// The tribe, 0 or 1.
    pub tribe: u8,
    /// The nights away.
    pub nights: u32,
    /// The farthest from home, in tiles on the longer axis.
    pub tiles: u32,
    /// Who it was when the outing began.
    pub who: Who,
}

/// A looking day (`Act::Looked`): the day it closed, the tribe, the
/// farthest point from home, in tiles on the longer axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// The day it closed.
    pub day: u64,
    /// The tribe, 0 or 1.
    pub tribe: u8,
    /// The farthest from home, in tiles on the longer axis.
    pub tiles: u32,
}

/// The probe of the scarcity chain.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chain {
    /// By tribe and season's year.
    pub years: BTreeMap<(u8, u64), ChainYear>,
    /// By age-year from the first, as `nights_out`.
    pub away: Vec<AwayWhy>,
    /// Every looking day, in order.
    pub looks: Vec<Look>,
    /// Every outing of a grown body beyond the fire's reach of its home,
    /// as it ended (25 November 2026).
    pub outings: Vec<Outing>,
    /// What the bodies chose away from home (25 November 2026): at the
    /// point of no return, then on a morning away; each home, stay, on.
    pub turns: [[u32; 3]; 2],
    tracks: BTreeMap<BodyId, Track>,
    /// The tick of the last dawn.
    last_dawn: u64,
    ticks_per_hour: u64,
    ticks_per_minute: u64,
}

fn blockage_index(b: Option<Blockage>) -> usize {
    match b {
        Some(Blockage::Water) => 0,
        Some(Blockage::Rock) => 1,
        Some(Blockage::NoWay) => 2,
        None => 3,
    }
}

/// How `body` lies down now: collapsed of fatigue, else by its mind's
/// way, goal, or home within `reach`, else stopped by what barred it.
fn lay_of(
    body: &sim::Body,
    mind: Option<&sim::brain::Mind>,
    (barred, day): (Option<Blockage>, u64),
    reach: u32,
) -> Lay {
    if body.condition().fatigue.is_full() {
        return Lay::Exhausted;
    }
    let Some(m) = mind else {
        return Lay::AtHome;
    };
    let away = tiles_apart(body.pos(), m.home()) > reach;
    if away && m.on_trip(day) {
        Lay::Trip
    } else if m.ways().sleep_out {
        Lay::SleepOut
    } else if m.holds_goal() {
        Lay::Goal
    } else if tiles_apart(body.pos(), m.home()) <= reach {
        Lay::AtHome
    } else {
        Lay::Stopped(barred)
    }
}

impl Chain {
    /// A probe for a day of `ticks_per_day` ticks.
    #[must_use]
    pub fn new(ticks_per_day: u64) -> Self {
        Self {
            ticks_per_hour: (ticks_per_day / 24).max(1),
            ticks_per_minute: (ticks_per_day / 1440).max(1),
            ..Self::default()
        }
    }

    /// After a tick and its events were read: the walk, the work, the
    /// need, how each body lay down, the dawn's bodies away. `year` is the
    /// season's year whose ticks elapse.
    pub fn tick(
        &mut self,
        (world, brains, content): (&World, &Brains, &Content),
        events: &[Event],
        people: &BTreeMap<BodyId, Person>,
        (tick, year): (u64, u64),
    ) {
        let mut dawn = false;
        for event in events {
            match event {
                Event::BodyBlocked { body, reason, .. }
                | Event::MoveRefused { body, reason, .. } => {
                    if let Some(t) = self.tracks.get_mut(body) {
                        t.barred = Some(*reason);
                    }
                }
                Event::DayStarted { .. } => dawn = true,
                _ => {}
            }
        }
        let hourly = tick.is_multiple_of(self.ticks_per_hour);
        let minute = tick.is_multiple_of(self.ticks_per_minute);
        let needs = &content.needs;
        let reach = needs.trials.imitate_tiles;
        let mut acc = [ChainYear::default(); 2];
        for b in world.bodies() {
            let id = b.id();
            let c = b.condition();
            if c.vitality.points() == 0 {
                continue;
            }
            let t = self.tracks.entry(id).or_insert_with(|| Track {
                tribe: people.get(&id).map_or(0, |p| p.tribe),
                at: b.at(),
                asleep: c.asleep,
                lay: None,
                barred: None,
                far: 0,
                out_nights: 0,
                out_far: 0,
                out_who: None,
                rose: 0,
            });
            let y = &mut acc[usize::from(t.tribe.min(1))];
            let walker = !world.is_infant(b, content);
            if walker {
                let (dx, dy) = (i64::from(b.at().x - t.at.x), i64::from(b.at().y - t.at.y));
                let mm = u64::try_from(dx * dx + dy * dy).unwrap_or(0).isqrt();
                if mm > 0 {
                    y.walked_mm += mm;
                    y.walk_j +=
                        u64::from(needs.walk_j_per_m) * mm * u64::from(c.size_permille.min(1000))
                            / 1_000_000;
                }
                y.gathering_ticks += u64::from(b.work().is_some());
            }
            t.at = b.at();
            if hourly {
                y.lived_hours += 1;
                y.walker_hours += u64::from(walker);
                y.need_j += c.need_j_per_day(needs) / 24;
            }
            let mind = brains.mind(id);
            if c.asleep && !t.asleep {
                let day = tick / world.calendar().ticks_per_day().max(1);
                let lay = lay_of(b, mind, (t.barred, day), reach);
                t.lay = Some((lay, c.fatigue.points(), tick));
            }
            if t.asleep && !c.asleep {
                t.rose = tick;
            }
            t.asleep = c.asleep;
            if minute
                && walker
                && let Some(m) = mind
            {
                let from_home = tiles_apart(b.pos(), m.home());
                t.far = t.far.max(from_home);
                t.out_far = t.out_far.max(from_home);
                if from_home > reach && t.out_who.is_none() && world.age_milli(b) >= 16_000 {
                    t.out_who = Some(Who::of(world, brains, b));
                }
            }
        }
        for (tribe, add) in acc.iter().enumerate() {
            if *add == ChainYear::default() {
                continue;
            }
            let y = self
                .years
                .entry((u8::try_from(tribe).expect("two"), year))
                .or_default();
            y.lived_hours += add.lived_hours;
            y.walker_hours += add.walker_hours;
            y.need_j += add.need_j;
            y.walk_j += add.walk_j;
            y.walked_mm += add.walked_mm;
            y.gathering_ticks += add.gathering_ticks;
        }
        if dawn {
            self.dawn(world, brains, content, tick);
            self.last_dawn = tick;
        }
    }

    /// A dawn: each grown body away from home, as `nights_out` counts it,
    /// by how it lay down last, asleep still or since the dawn before (the
    /// rested wake with the day, at the dawn's own tick); then the day's
    /// barring forgotten.
    fn dawn(&mut self, world: &World, brains: &Brains, content: &Content, now: u64) {
        let reach = content.needs.trials.imitate_tiles;
        let days_a_year = u64::from(world.calendar().days_per_age_year().max(1));
        let year = usize::try_from(u64::try_from(world.today()).unwrap_or(0) / days_a_year)
            .unwrap_or(usize::MAX);
        let mut why = AwayWhy::default();
        // The outings: a night more away, or one ended at home.
        let day = u64::try_from(world.today()).unwrap_or(0);
        for b in world.bodies() {
            if b.condition().vitality.points() == 0 || world.age_milli(b) < 16_000 {
                continue;
            }
            let Some(home) = home_of(world, brains, b.id()) else {
                continue;
            };
            let Some(t) = self.tracks.get_mut(&b.id()) else {
                continue;
            };
            if tiles_apart(b.pos(), home) > reach {
                t.out_nights += 1;
                continue;
            }
            if t.out_nights > 0 || t.out_far > reach {
                let who = t.out_who.unwrap_or_else(|| Who::of(world, brains, b));
                self.outings.push(Outing {
                    day,
                    tribe: t.tribe,
                    nights: t.out_nights,
                    tiles: t.out_far,
                    who,
                });
            }
            (t.out_nights, t.out_far, t.out_who) = (0, 0, None);
        }
        for b in world.bodies() {
            if b.condition().vitality.points() == 0 || world.age_milli(b) < 16_000 {
                continue;
            }
            let Some(home) = home_of(world, brains, b.id()) else {
                continue;
            };
            if tiles_apart(b.pos(), home) <= reach {
                continue;
            }
            let Some(track) = self.tracks.get(&b.id()) else {
                continue;
            };
            let asleep = b.condition().asleep;
            // Slept till dawn: asleep still, or risen with the day (the
            // rested rise at the dawn's own tick).
            let till_dawn = asleep || track.rose + self.ticks_per_minute >= now;
            let Some((lay, fatigue, at)) = track.lay else {
                if asleep {
                    why.unseen += 1;
                } else {
                    why.awake += 1;
                }
                continue;
            };
            if !till_dawn {
                if at > self.last_dawn {
                    why.woke += 1;
                } else {
                    why.awake += 1;
                }
                continue;
            }
            why.fatigue.0 += u64::from(fatigue);
            why.fatigue.1 += 1;
            match lay {
                Lay::Exhausted => why.exhausted += 1,
                Lay::Stopped(by) => {
                    why.stopped += 1;
                    why.stopped_by[blockage_index(by)] += 1;
                }
                Lay::SleepOut => why.sleep_out += 1,
                Lay::Goal => why.goal += 1,
                Lay::Trip => why.trip += 1,
                Lay::AtHome => why.lay_at_home += 1,
            }
        }
        if self.away.len() <= year {
            self.away.resize(year + 1, AwayWhy::default());
        }
        let n = &mut self.away[year];
        n.exhausted += why.exhausted;
        n.stopped += why.stopped;
        for (sum, add) in n.stopped_by.iter_mut().zip(why.stopped_by) {
            *sum += add;
        }
        n.sleep_out += why.sleep_out;
        n.goal += why.goal;
        n.trip += why.trip;
        n.lay_at_home += why.lay_at_home;
        n.awake += why.awake;
        n.woke += why.woke;
        n.unseen += why.unseen;
        n.fatigue.0 += why.fatigue.0;
        n.fatigue.1 += why.fatigue.1;
        let living: BTreeSet<BodyId> = world.bodies().iter().map(sim::Body::id).collect();
        self.tracks.retain(|id, _| living.contains(id));
        for t in self.tracks.values_mut() {
            t.barred = None;
        }
    }

    /// A brain's news: a day closed (`News::Habit`) ends its farthest
    /// point; a looking day's is kept.
    pub fn news(&mut self, news: &News, day: u64) {
        if let News::Turned { choice, when, .. } = news {
            let w = usize::from(*when == TripWhen::Morning);
            let c = match choice {
                TripChoice::Home => 0,
                TripChoice::Stay => 1,
                TripChoice::On => 2,
            };
            self.turns[w][c] += 1;
        }
        if let News::Habit { body, act, .. } = news
            && let Some(t) = self.tracks.get_mut(body)
        {
            if *act == Act::Looked {
                self.looks.push(Look {
                    day,
                    tribe: t.tribe,
                    tiles: t.far,
                });
            }
            t.far = 0;
        }
    }

    /// The data's `chain` (`docs/10-vivarium-data.md`).
    #[must_use]
    pub fn json(&self, ticks_per_day: u64) -> Value {
        let years: Vec<Value> = self
            .years
            .iter()
            .map(|(&(tribe, year), y)| {
                json!({
                    "tribe": tribe_name(tribe),
                    "year": year,
                    "lived_hours": y.lived_hours,
                    "walker_hours": y.walker_hours,
                    "need_kj": y.need_j / 1000,
                    "walk_kj": y.walk_j / 1000,
                    "walked_m": y.walked_mm / 1000,
                    "gathering_ticks": y.gathering_ticks,
                })
            })
            .collect();
        let away: Vec<Value> = self
            .away
            .iter()
            .map(|a| {
                json!({
                    "exhausted": a.exhausted,
                    "stopped": a.stopped,
                    "stopped_by": {
                        "water": a.stopped_by[0],
                        "rock": a.stopped_by[1],
                        "no_way": a.stopped_by[2],
                        "none": a.stopped_by[3],
                    },
                    "sleep_out": a.sleep_out,
                    "goal": a.goal,
                    "trip": a.trip,
                    "lay_at_home": a.lay_at_home,
                    "awake": a.awake,
                    "woke": a.woke,
                    "unseen": a.unseen,
                    "fatigue_mean": (a.fatigue.1 > 0).then(|| a.fatigue.0 / u64::from(a.fatigue.1)),
                })
            })
            .collect();
        let looks: Vec<Value> = self
            .looks
            .iter()
            .map(|l| json!([l.day, l.tribe, l.tiles]))
            .collect();
        let outings: Vec<Value> = self
            .outings
            .iter()
            .map(|o| {
                json!([
                    o.day,
                    o.tribe,
                    o.nights,
                    o.tiles,
                    o.who.will,
                    o.who.parent_under_8,
                    o.who.parent_under_16,
                    if o.who.female { "female" } else { "male" },
                    o.who.age_years,
                ])
            })
            .collect();
        let turns = |t: [u32; 3]| json!({"home": t[0], "stay": t[1], "on": t[2]});
        json!({
            "ticks_per_hour": (ticks_per_day / 24).max(1),
            "years": years,
            "away": away,
            "looks": looks,
            "outings": outings,
            "turns": {
                "no_return": turns(self.turns[0]),
                "morning": turns(self.turns[1]),
            },
        })
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

    fn settings(days: u32) -> Settings {
        Settings {
            world: WorldKind::Patch { side: 60 },
            people: 6,
            days,
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
            continents: None,
            helpers: 0,
        }
    }

    fn run(days: u32, content: &Content) -> Report {
        run_with(&settings(days), content).expect("the run runs")
    }

    #[test]
    fn the_living_are_counted_each_hour_and_their_need_with_them() {
        let report = run(4, content());
        let per_day = report.calendar().ticks_per_day();
        let hours: u64 = report.chain.years.values().map(|y| y.lived_hours).sum();
        // Every living body once a game hour: the curves' samples count the
        // same living (one sample a game hour, the first at tick 0 aside).
        let sampled: u64 = report
            .samples
            .iter()
            .skip(1)
            .map(|s| u64::from(s.alive))
            .sum();
        assert_eq!(hours, sampled, "{per_day} ticks a day");
        for y in report.chain.years.values() {
            assert!(y.walker_hours < y.lived_hours, "the infant is no walker");
            // A grown body burns a day's need; nobody burns more than a
            // nursing woman's share of it.
            let day = content().needs.need_j_per_day();
            assert!(y.need_j > 0 && y.need_j <= y.lived_hours * day / 24 * 2);
            assert!(y.walked_mm > 0 && y.walk_j > 0, "they walk");
            assert!(y.gathering_ticks > 0, "they gather");
        }
    }

    #[test]
    fn every_dawn_away_has_one_why_and_every_looking_day_its_distance() {
        let report = run(6, content());
        assert_ne!(report.chain.away, Vec::new());
        for (why, out) in report.chain.away.iter().zip(&report.nights_out) {
            let lay =
                why.exhausted + why.stopped + why.sleep_out + why.goal + why.trip + why.lay_at_home;
            assert_eq!(
                lay + why.awake + why.woke + why.unseen,
                out.away,
                "{why:?} against {out:?}"
            );
            // The rested wake at the dawn's tick: those still asleep are
            // among those who lay down.
            assert!(lay + why.unseen >= out.asleep, "{why:?} against {out:?}");
            assert_eq!(why.stopped_by.iter().sum::<u32>(), why.stopped);
            assert_eq!(why.fatigue.1, lay);
        }
        let looked: u32 = report
            .decisions
            .values()
            .map(|d| d.days[crate::act_index(Act::Looked)])
            .sum();
        assert_eq!(
            u32::try_from(report.chain.looks.len()).unwrap(),
            looked,
            "a look a looking day"
        );
        // Every outing ended went beyond the fire's reach or slept away.
        let reach = content().needs.trials.imitate_tiles;
        assert_ne!(report.chain.outings, Vec::new());
        for o in &report.chain.outings {
            assert!(o.nights > 0 || o.tiles > reach, "{o:?}");
            // Who went: grown, its will a will, a parent of one under 8 a
            // parent of one under 16.
            assert!(o.who.age_years >= 16 && o.who.will <= 100, "{o:?}");
            assert!(!o.who.parent_under_8 || o.who.parent_under_16, "{o:?}");
        }
    }

    #[test]
    fn a_body_that_collapses_of_fatigue_is_exhausted() {
        // Fatigue rising forty times faster and resting twenty times
        // slower: everyone collapses where it stands and sleeps long, home
        // or away; away is beyond two tiles.
        let mut tired = content().clone();
        tired.needs.fatigue_per_day_awake *= 40;
        tired.needs.rest_per_day_asleep = (tired.needs.rest_per_day_asleep / 20).max(1);
        tired.needs.trials.imitate_tiles = 2;
        let report = run(3, &tired);
        let exhausted: u32 = report.chain.away.iter().map(|a| a.exhausted).sum();
        let away: u32 = report.nights_out.iter().map(|n| n.asleep).sum();
        assert!(
            away > 0,
            "some lie away at dawn: {:?} {:?}",
            report.nights_out,
            report.chain.away
        );
        assert!(exhausted > 0, "{:?}", report.chain.away);
        for a in &report.chain.away {
            if a.fatigue.1 > 0 {
                assert!(a.fatigue.0 / u64::from(a.fatigue.1) >= 50, "{a:?}");
            }
        }
    }
}
