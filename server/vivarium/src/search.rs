//! The search for an absent kin under glass (D56, amended 27 November
//! 2026): the absences noticed at the fire, the searches begun and how
//! each ended (the absent seen alive, seen dead, not found), the infants
//! taken up and brought home, and what became of each a season later.
//! One record an absence noticed (`records`): who, what tie, whether a
//! search began and why not, where it went, how far it walked and how it
//! ended, with what the vivarium alone knows (the absent's true home and
//! state: a measure, never a perception).
//! Section `search` of the JSON (`docs/10`), new keys only; `on` false
//! and nothing else without the content's `search` section.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use sim::brain::{Brains, HOME_TILES, News, Quiet, SearchStep, Stayed, Tie};
use sim::content::{Content, Sex};
use sim::map::Pos;
use sim::{BodyId, World};

/// What the probe keeps of each body, looked at once a day: the dead
/// leave the world, and a record needs who they were.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Known {
    sex: Sex,
    age_milli: u64,
    /// Its home, and the day it was first seen there.
    home: Pos,
    home_since: u64,
    /// The last day it was seen alive.
    seen: u64,
}

/// One absence noticed.
#[derive(Debug, Clone, PartialEq)]
struct Record {
    day: u64,
    absent: BodyId,
    /// Who noticed first, or who went.
    kin: BodyId,
    tie: Tie,
    nights: u32,
    said: bool,
    noticers: u32,
    stayed: Option<Stayed>,
    /// Not noticed, and why: within its usual absence, known to live
    /// elsewhere, seen at the camp, told dead.
    quiet: Option<&'static str>,
    /// The child who told the fire, when the searcher is no kin.
    child: Option<BodyId>,
    /// The absent then, as the vivarium knows it.
    absent_alive: bool,
    /// The two homes then: how far apart, and since which day each.
    homes: Option<(u64, u64)>,
    /// The searcher's position when it set off, the place it went to and
    /// whether that place was said (else last seen).
    began: Option<(Pos, Pos, bool)>,
    /// The tiles walked so far, and where the searcher stood last.
    walked: u64,
    last: Option<Pos>,
    result: Option<&'static str>,
    /// Found alive: where, and how far from the searcher's camp.
    found: Option<(&'static str, u64)>,
    result_day: Option<u64>,
}

fn apart(a: Pos, b: Pos) -> u64 {
    u64::from((a.x - b.x).unsigned_abs().max((a.y - b.y).unsigned_abs()))
}

fn same_hearth(a: Pos, b: Pos) -> bool {
    apart(a, b) <= u64::from(HOME_TILES.unsigned_abs())
}

/// What the searches gave, counted.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchProbe {
    on: bool,
    noticed: u32,
    begun: u32,
    /// The straight tiles to the place of each search begun, summed.
    begun_tiles: u64,
    found_alive: u32,
    found_dead: u32,
    not_found_reached: u32,
    not_found_stopped: u32,
    infants_taken: u32,
    infants_home: u32,
    infants_given: u32,
    /// Each infant brought home, the day it came, and whether it lived a
    /// season from then (none: not known yet).
    brought: Vec<(BodyId, u64, Option<bool>)>,
    last_day: Option<u64>,
    known: BTreeMap<BodyId, Known>,
    records: Vec<Record>,
    /// Absences not noticed, by cause; findings heard alive.
    quiet: BTreeMap<&'static str, u32>,
    heard_alive: u32,
    by_child: u32,
    /// The wall seconds the brains spent in the search.
    pub seconds: f64,
}

impl SearchProbe {
    /// A probe for a run under `content`.
    #[must_use]
    pub fn new(content: &Content) -> Self {
        Self {
            on: content.needs.search.is_some(),
            ..Self::default()
        }
    }

    /// Counts the steps of the searches told in `batch`, and keeps one
    /// record an absence noticed.
    #[allow(clippy::too_many_lines, reason = "one arm a step of the search")]
    pub fn news(&mut self, world: &World, brains: &Brains, batch: &[News]) {
        if !self.on {
            return;
        }
        let day = world.time().day;
        for news in batch {
            let News::Search {
                body, who, step, ..
            } = news
            else {
                continue;
            };
            match *step {
                SearchStep::Noticed {
                    tie,
                    nights,
                    said,
                    kin,
                    stayed,
                } => {
                    self.noticed += 1;
                    let record = Record {
                        said,
                        noticers: kin,
                        stayed,
                        ..self.record(world, brains, (*who, *body), tie, nights)
                    };
                    self.records.push(record);
                }
                SearchStep::Unnoticed { cause, tie, nights } => {
                    let cause = match cause {
                        Quiet::Usual => "within_usual_absence",
                        Quiet::LivesElsewhere => "known_to_live_elsewhere",
                        Quiet::SeenAtCamp => "seen_at_camp",
                    };
                    *self.quiet.entry(cause).or_default() += 1;
                    let record = Record {
                        quiet: Some(cause),
                        ..self.record(world, brains, (*who, *body), tie, nights)
                    };
                    self.records.push(record);
                }
                SearchStep::Heard { dead: true, .. } => {
                    *self.quiet.entry("told_dead").or_default() += 1;
                    // What the dead was to the listener is not told: the
                    // record says the listener and the dead.
                    let record = Record {
                        quiet: Some("told_dead"),
                        ..self.record(world, brains, (*who, *body), Tie::Spouse, 0)
                    };
                    self.records.push(record);
                }
                SearchStep::Heard { dead: false, .. } => self.heard_alive += 1,
                SearchStep::Begun {
                    tiles,
                    tie,
                    nights,
                    said,
                    to,
                    child,
                } => {
                    self.begun += 1;
                    self.by_child += u32::from(child.is_some());
                    self.begun_tiles += u64::from(tiles);
                    let from = world.body(*body).map(sim::Body::pos);
                    let homes = self.homes(brains, day, *who, *body);
                    if let Some(record) = self
                        .records
                        .iter_mut()
                        .rev()
                        .find(|r| r.absent == *who && r.day == day && r.began.is_none())
                    {
                        (record.kin, record.tie, record.nights) = (*body, tie, nights);
                        record.said = said;
                        record.homes = homes;
                        record.child = child;
                        record.began = from.map(|from| (from, to, said));
                        record.last = from;
                    }
                }
                SearchStep::FoundAlive => {
                    self.found_alive += 1;
                    let found = Self::found_where(world, brains, *body, *who);
                    self.close(world, (*body, *who), "found_alive", found);
                }
                SearchStep::FoundDead => {
                    self.found_dead += 1;
                    self.close(world, (*body, *who), "found_dead", None);
                }
                SearchStep::NotFound { reached: true } => {
                    self.not_found_reached += 1;
                    self.close(world, (*body, *who), "not_found", None);
                }
                SearchStep::NotFound { reached: false } => {
                    self.not_found_stopped += 1;
                    self.close(world, (*body, *who), "stopped_short", None);
                }
                SearchStep::InfantTaken { .. } => self.infants_taken += 1,
                SearchStep::InfantHome { infant } => {
                    self.infants_home += 1;
                    self.brought.push((infant, day, None));
                }
                SearchStep::InfantGiven { .. } => self.infants_given += 1,
            }
        }
    }

    /// A record of `kin` and its absent, nothing else known yet.
    fn record(
        &self,
        world: &World,
        brains: &Brains,
        (absent, kin): (BodyId, BodyId),
        tie: Tie,
        nights: u32,
    ) -> Record {
        let day = world.time().day;
        Record {
            day,
            absent,
            kin,
            tie,
            nights,
            said: false,
            noticers: 0,
            stayed: None,
            quiet: None,
            child: None,
            absent_alive: world.body(absent).is_some(),
            homes: self.homes(brains, day, absent, kin),
            began: None,
            walked: 0,
            last: None,
            result: None,
            found: None,
            result_day: None,
        }
    }

    /// The home of `id` now (its mind's; the last known of one dead), and
    /// the day it was first seen there.
    fn home(&self, brains: &Brains, day: u64, id: BodyId) -> Option<(Pos, u64)> {
        let known = self.known.get(&id);
        match brains.mind(id).map(sim::brain::Mind::home) {
            Some(home) => Some((
                home,
                known
                    .filter(|k| k.home == home)
                    .map_or(day, |k| k.home_since),
            )),
            None => known.map(|k| (k.home, k.home_since)),
        }
    }

    /// How far apart the homes of `absent` and `kin` are, and the later
    /// of the days each took its own.
    fn homes(&self, brains: &Brains, day: u64, absent: BodyId, kin: BodyId) -> Option<(u64, u64)> {
        let (a, k) = (
            self.home(brains, day, absent)?,
            self.home(brains, day, kin)?,
        );
        Some((apart(a.0, k.0), a.1.max(k.1)))
    }

    /// Where the absent `who` stands as `searcher` sees it alive: at the
    /// camp it shares with the searcher, at another camp (its own home),
    /// asleep out or awake out; and how far from the searcher's camp.
    fn found_where(
        world: &World,
        brains: &Brains,
        searcher: BodyId,
        who: BodyId,
    ) -> Option<(&'static str, u64)> {
        let body = world.body(who)?;
        let (theirs, mine) = (brains.mind(who)?.home(), brains.mind(searcher)?.home());
        let at = body.pos();
        let place = if same_hearth(at, mine) {
            "at_searcher_camp"
        } else if same_hearth(at, theirs) {
            "at_another_camp"
        } else if body.condition().asleep {
            "sleeping_out"
        } else {
            "awake_out"
        };
        Some((place, apart(at, mine)))
    }

    /// The search of `searcher` for `who` ended.
    fn close(
        &mut self,
        world: &World,
        (searcher, who): (BodyId, BodyId),
        result: &'static str,
        found: Option<(&'static str, u64)>,
    ) {
        if let Some(record) = self.records.iter_mut().rev().find(|r| {
            r.kin == searcher && r.absent == who && r.began.is_some() && r.result.is_none()
        }) {
            record.result = Some(result);
            record.found = found;
            record.result_day = Some(world.time().day);
        }
    }

    /// Each tick: the tiles the searchers on their way walked. Once a
    /// day: who each body is and where its home is, and whether each
    /// infant brought home lives, a season on.
    pub fn tick(&mut self, world: &World, brains: &Brains) {
        if !self.on {
            return;
        }
        for record in &mut self.records {
            if record.began.is_none() || record.result.is_some() {
                continue;
            }
            if let (Some(last), Some(body)) = (record.last, world.body(record.kin)) {
                record.walked += apart(last, body.pos());
                record.last = Some(body.pos());
            }
        }
        let day = world.time().day;
        if self.last_day == Some(day) {
            return;
        }
        self.last_day = Some(day);
        for body in world.bodies() {
            let Some(home) = brains.mind(body.id()).map(sim::brain::Mind::home) else {
                continue;
            };
            let home_since = self
                .known
                .get(&body.id())
                .filter(|k| k.home == home)
                .map_or(day, |k| k.home_since);
            self.known.insert(
                body.id(),
                Known {
                    sex: body.sex(),
                    age_milli: world.age_milli(body),
                    home,
                    home_since,
                    seen: day,
                },
            );
        }
        let season = u64::from(world.calendar().days_per_cycle()) / 4;
        for (infant, home_day, lived) in &mut self.brought {
            if lived.is_some() {
                continue;
            }
            let alive = world
                .body(*infant)
                .is_some_and(|b| b.condition().vitality.points() > 0);
            if !alive {
                *lived = Some(false);
            } else if day >= *home_day + season {
                *lived = Some(true);
            }
        }
    }

    /// One record of `records`.
    fn record_json(&self, r: &Record) -> Value {
        let who = |id: BodyId| {
            let known = self.known.get(&id);
            json!({
                "id": id.0,
                "age_years": known.map(|k| k.age_milli / 1000),
                "sex": known.map(|k| if k.sex == Sex::Female { "female" } else { "male" }),
            })
        };
        let tie = match r.tie {
            Tie::Spouse => "spouse",
            Tie::Child => "child",
            Tie::Parent => "parent",
        };
        let stayed = r.stayed.map(|s| match s {
            Stayed::Camp => "last_known_place_is_the_camp",
            Stayed::Young => "too_young",
            Stayed::Searching => "already_searching",
            Stayed::Goal => "holds_a_goal",
            Stayed::Moving => "moving_home",
            Stayed::AdultKnows => "a_grown_kin_at_the_fire_knows",
        });
        let near = u64::from(HOME_TILES.unsigned_abs());
        let same = r.homes.map(|(apart, _)| apart <= near);
        let other_days = r
            .homes
            .filter(|(apart, _)| *apart > near)
            .map(|(_, since)| r.day.saturating_sub(since));
        let pos = |p: Pos| json!([p.x, p.y]);
        json!({
            "day": r.day,
            "absent": who(r.absent),
            "kin": who(r.kin),
            "absent_is_kin_s": if r.quiet == Some("told_dead") { None } else { Some(tie) },
            "noticed": r.quiet.is_none(),
            "not_noticed_because": r.quiet,
            "told_by_child": r.child.map(|c| c.0),
            "noticers": r.noticers,
            "nights_since_seen": r.nights,
            "absent_alive": r.absent_alive,
            "same_hearth": same,
            "homes_apart_tiles": r.homes.map(|h| h.0),
            "other_hearth_days": other_days,
            "said_where": r.said,
            "search_began": r.began.is_some(),
            "why_not": stayed,
            "went_to": r.began.map(|(_, _, said)| if said { "place_said" } else { "last_seen" }),
            "to": r.began.map(|(_, to, _)| pos(to)),
            "to_tiles": r.began.map(|(from, to, _)| apart(from, to)),
            "walked_tiles": r.began.map(|_| r.walked),
            "result": r.result,
            "result_day": r.result_day,
            "found_where": r.found.map(|f| f.0),
            "found_tiles_from_kin_camp": r.found.map(|f| f.1),
        })
    }

    /// The section `search` of the JSON.
    #[must_use]
    pub fn json(&self) -> Value {
        if !self.on {
            return json!({ "on": false });
        }
        let count = |what: Option<bool>| self.brought.iter().filter(|b| b.2 == what).count();
        let quiet = |cause: &str| self.quiet.get(cause).copied().unwrap_or(0);
        json!({
            "on": true,
            "absences_noticed": self.noticed,
            "searches_begun": self.begun,
            "search_tiles_mean": if self.begun == 0 { 0 } else { self.begun_tiles / u64::from(self.begun) },
            "found_alive": self.found_alive,
            "found_dead": self.found_dead,
            "not_found": self.not_found_reached + self.not_found_stopped,
            "not_found_stopped_short": self.not_found_stopped,
            "infants_taken": self.infants_taken,
            "infants_brought_home": self.infants_home,
            "infants_given_to_a_woman": self.infants_given,
            "infants_survived_a_season": count(Some(true)),
            "infants_died_within_a_season": count(Some(false)),
            "infants_season_not_over": count(None),
            "searches_told_by_a_child": self.by_child,
            "heard_alive": self.heard_alive,
            "not_noticed": {
                "within_usual_absence": quiet("within_usual_absence"),
                "known_to_live_elsewhere": quiet("known_to_live_elsewhere"),
                "seen_at_camp": quiet("seen_at_camp"),
                "told_dead": quiet("told_dead"),
            },
            "records": self.records.iter().map(|r| self.record_json(r)).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_the_content_s_section_the_probe_says_off_and_nothing_else() {
        let mut content = Content::sample();
        content.needs.search = None;
        assert_eq!(SearchProbe::new(&content).json(), json!({ "on": false }));
    }

    #[test]
    fn the_steps_told_are_counted_each_under_its_key() {
        let mut probe = SearchProbe {
            on: true,
            ..SearchProbe::default()
        };
        let world = World::empty(
            sim::time::Calendar::new(1, 7200).unwrap(),
            sim::map::Map::filled(8, 8, sim::map::Terrain::Grass),
        );
        let told = |step| News::Search {
            body: BodyId(1),
            who: BodyId(2),
            step,
            at: 0,
        };
        let brains = Brains::new(7);
        probe.news(
            &world,
            &brains,
            &[
                told(SearchStep::Noticed {
                    tie: Tie::Spouse,
                    nights: 2,
                    said: true,
                    kin: 3,
                    stayed: None,
                }),
                told(SearchStep::Begun {
                    tiles: 40,
                    tie: Tie::Spouse,
                    nights: 2,
                    said: true,
                    to: Pos::new(4, 4),
                    child: None,
                }),
                told(SearchStep::Unnoticed {
                    cause: Quiet::Usual,
                    tie: Tie::Parent,
                    nights: 3,
                }),
                told(SearchStep::FoundDead),
                told(SearchStep::InfantTaken { infant: BodyId(3) }),
                told(SearchStep::InfantHome { infant: BodyId(3) }),
                told(SearchStep::NotFound { reached: false }),
            ],
        );
        let json = probe.json();
        assert_eq!(json["absences_noticed"], 1);
        assert_eq!(json["searches_begun"], 1);
        assert_eq!(json["search_tiles_mean"], 40);
        assert_eq!(json["found_dead"], 1);
        assert_eq!(json["not_found"], 1);
        assert_eq!(json["not_found_stopped_short"], 1);
        assert_eq!(json["infants_brought_home"], 1);
        assert_eq!(json["infants_season_not_over"], 1);
        // The infant is no body of this world: gone at the day's look.
        probe.tick(&world, &brains);
        let json = probe.json();
        assert_eq!(json["infants_died_within_a_season"], 1);
        // One record an absence: its notice, and no search without a
        // searcher in the world to set off.
        let records = json["records"].as_array().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[1]["noticed"], false);
        assert_eq!(records[1]["not_noticed_because"], "within_usual_absence");
        assert_eq!(json["not_noticed"]["within_usual_absence"], 1);
        assert_eq!(records[0]["absent"]["id"], 2);
        assert_eq!(records[0]["absent_is_kin_s"], "spouse");
        assert_eq!(records[0]["noticers"], 3);
        assert_eq!(records[0]["nights_since_seen"], 2);
        assert_eq!(records[0]["said_where"], true);
        assert_eq!(records[0]["absent_alive"], false);
    }
}
