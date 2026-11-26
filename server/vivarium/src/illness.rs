//! Illness under glass (D49, amended 26 November 2026): the episodes by
//! ailment, season and age, how they ended by age and thinness, the sick
//! body-days, the share caught from a hearth's sick, the fouling of each
//! hearth's place day by day, and the camps moved while a hearth had sick
//! members: what the lead will read to see whether a fouled camp makes
//! people sick, and whether those who leave it fare better. Section
//! `illness` of the JSON (`docs/10`), new keys only; `on` false and
//! nothing else under the old rule.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use sim::content::Content;
use sim::{Ailment, BodyId, Event, HearthCount, World};

/// A hearth's place: the square of `thought.place_tiles` it lies in.
type Place = (i32, i32);

/// The age bands of the counts, from their first age in years.
const BANDS: [(u64, &str); 6] = [
    (0, "0-1"),
    (2, "2-4"),
    (5, "5-14"),
    (15, "15-39"),
    (40, "40-59"),
    (60, "60+"),
];

/// The thinness bands of the ends.
const THINNESS: [&str; 3] = ["fat", "thin", "weak"];

/// The seasons' names, in the calendar's order.
const SEASONS: [&str; 4] = ["spring", "summer", "autumn", "winter"];

fn band(age_milli: u64) -> usize {
    BANDS
        .iter()
        .rposition(|&(from, _)| age_milli / 1000 >= from)
        .unwrap_or(0)
}

fn season_index(season: sim::Season) -> usize {
    match season {
        sim::Season::Spring => 0,
        sim::Season::Summer => 1,
        sim::Season::Autumn => 2,
        sim::Season::Winter => 3,
    }
}

fn name(ailment: Ailment) -> &'static str {
    match ailment {
        Ailment::Gut => "gut",
        Ailment::Chest => "chest",
    }
}

/// Where an episode began: its hearth that day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Began {
    members: u32,
    fouling: u64,
}

/// A death of illness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeathRow {
    /// The day.
    pub day: u64,
    /// Of what.
    pub ailment: Ailment,
    /// Its age band.
    pub band: usize,
    /// Its thinness band.
    pub thinness: usize,
    /// Its reserve left, in percent.
    pub reserve_percent: u32,
    /// Its hearth's members the day it fell ill.
    pub members: u32,
    /// Its hearth's fouling that day, in body-nights.
    pub fouling: u64,
}

/// A camp moved: the homes of a hearth's members moved, the same dawn,
/// from one place to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveRow {
    /// The dawn the homes were told moved.
    pub day: u64,
    /// The place left.
    pub from: (i32, i32),
    /// The place reached.
    pub to: (i32, i32),
    /// Bodies whose home moved so.
    pub movers: u32,
    /// The place left as counted the dawn before: members, sick of each.
    pub before: HearthCount,
    /// Its fouling the dawn before, in body-nights.
    pub fouling_left: u64,
    /// The place reached's fouling the dawn before, in body-nights.
    pub fouling_reached: u64,
}

/// The probe.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IllnessProbe {
    /// Whether the run lives by illness as episodes.
    pub on: bool,
    /// Episodes begun by ailment, season and age band: (all, caught).
    pub episodes: BTreeMap<(Ailment, usize, usize), (u32, u32)>,
    /// Episodes ended by ailment, age band and thinness: (healed, died).
    pub ends: BTreeMap<(Ailment, usize, usize), (u32, u32)>,
    /// The deaths of illness, in order.
    pub deaths: Vec<DeathRow>,
    /// Sick body-days by ailment, counted once a day.
    pub sick_days: [u64; 2],
    /// Body-days counted.
    pub body_days: u64,
    /// By hearth place with two members or more, each dawn: (day, place,
    /// count, fouling in body-nights).
    pub hearths: Vec<(u64, (i32, i32), HearthCount, u64)>,
    /// The camps moved, in order.
    pub moves: Vec<MoveRow>,
    /// The episodes under way, by body: where they began.
    open: BTreeMap<BodyId, Began>,
    /// Each body's hearth place at the dawn before.
    places: BTreeMap<BodyId, (i32, i32)>,
    /// Each hearth place's count and fouling at the dawn before.
    counts: BTreeMap<(i32, i32), (HearthCount, u64)>,
}

impl IllnessProbe {
    /// A probe for a run on `content`.
    #[must_use]
    pub fn new(content: &Content) -> Self {
        Self {
            on: content.needs.illness.is_some(),
            ..Self::default()
        }
    }

    /// A tick's events, read after the world's step.
    pub fn tick(&mut self, (world, content): (&World, &Content), events: &[Event]) {
        if !self.on {
            return;
        }
        let day = world.time().day;
        for event in events {
            match *event {
                Event::FellIll {
                    body,
                    ailment,
                    caught,
                    ..
                } => {
                    let age = world.body(body).map_or(0, |b| world.age_milli(b));
                    let season = season_index(world.time().season);
                    let e = self
                        .episodes
                        .entry((ailment, season, band(age)))
                        .or_default();
                    e.0 += 1;
                    e.1 += u32::from(caught);
                    let began = world.body(body).map_or(
                        Began {
                            members: 0,
                            fouling: 0,
                        },
                        |b| {
                            let place = world.hearth_place(b, content);
                            Began {
                                members: world.hearth_count(place).members,
                                fouling: world.fouling(place) / 1000,
                            }
                        },
                    );
                    self.open.insert(body, began);
                }
                Event::IllnessEnded {
                    body,
                    ailment,
                    died,
                    reserve_percent,
                    age_milli,
                } => {
                    let needs = &content.needs;
                    let thinness = if reserve_percent < needs.weak_percent {
                        2
                    } else {
                        usize::from(reserve_percent < needs.thin_percent)
                    };
                    let e = self
                        .ends
                        .entry((ailment, band(age_milli), thinness))
                        .or_default();
                    let began = self.open.remove(&body);
                    if died {
                        e.1 += 1;
                        self.deaths.push(DeathRow {
                            day,
                            ailment,
                            band: band(age_milli),
                            thinness,
                            reserve_percent,
                            members: began.map_or(0, |b| b.members),
                            fouling: began.map_or(0, |b| b.fouling),
                        });
                    } else {
                        e.0 += 1;
                    }
                }
                Event::BodyDied { body, .. } => {
                    self.open.remove(&body);
                }
                Event::DayStarted { .. } => self.dawn((world, content), day),
                _ => {}
            }
        }
    }

    /// A dawn: the sick counted, each hearth's place and fouling kept, the
    /// camps moved since the dawn before.
    fn dawn(&mut self, (world, content): (&World, &Content), day: u64) {
        let mut places = BTreeMap::new();
        let mut counts = BTreeMap::new();
        for b in world.bodies() {
            if let Some(sick) = b.condition().sick {
                self.sick_days[sick.ailment.index()] += 1;
            }
            self.body_days += 1;
            let place = world.hearth_place(b, content);
            places.insert(b.id(), place);
            counts
                .entry(place)
                .or_insert_with(|| (world.hearth_count(place), world.fouling(place) / 1000));
        }
        for (&place, &(count, fouling)) in &counts {
            if count.members >= 2 {
                self.hearths.push((day, place, count, fouling));
            }
        }
        let mut moved: BTreeMap<(Place, Place), u32> = BTreeMap::new();
        for (id, &to) in &places {
            if let Some(&from) = self.places.get(id)
                && from != to
            {
                *moved.entry((from, to)).or_default() += 1;
            }
        }
        let fouled_before = |place| {
            self.counts
                .get(&place)
                .map_or(0, |c: &(HearthCount, u64)| c.1)
        };
        let mut rows = Vec::new();
        for ((from, to), movers) in moved {
            let (before, fouling_left) = self.counts.get(&from).copied().unwrap_or_default();
            rows.push(MoveRow {
                day,
                from,
                to,
                movers,
                before,
                fouling_left,
                fouling_reached: fouled_before(to),
            });
        }
        self.moves.extend(rows);
        self.places = places;
        self.counts = counts;
    }

    /// The section as JSON.
    #[must_use]
    pub fn json(&self) -> Value {
        if !self.on {
            return json!({ "on": false });
        }
        let episodes: Vec<Value> = self
            .episodes
            .iter()
            .map(|(&(a, season, b), &(all, caught))| {
                json!({
                    "ailment": name(a),
                    "season": SEASONS[season],
                    "age": BANDS[b].1,
                    "episodes": all,
                    "caught": caught,
                })
            })
            .collect();
        let ends: Vec<Value> = self
            .ends
            .iter()
            .map(|(&(a, b, t), &(healed, died))| {
                json!({
                    "ailment": name(a),
                    "age": BANDS[b].1,
                    "thinness": THINNESS[t],
                    "healed": healed,
                    "died": died,
                })
            })
            .collect();
        let deaths: Vec<Value> = self
            .deaths
            .iter()
            .map(|d| {
                json!([
                    d.day,
                    name(d.ailment),
                    BANDS[d.band].1,
                    THINNESS[d.thinness],
                    d.reserve_percent,
                    d.members,
                    d.fouling
                ])
            })
            .collect();
        let hearths: Vec<Value> = self
            .hearths
            .iter()
            .map(|(day, (x, y), c, fouling)| {
                json!([day, x, y, c.members, c.sick[0], c.sick[1], fouling])
            })
            .collect();
        let moves: Vec<Value> = self
            .moves
            .iter()
            .map(|m| {
                json!([
                    m.day,
                    m.from.0,
                    m.from.1,
                    m.to.0,
                    m.to.1,
                    m.movers,
                    m.before.members,
                    m.before.sick[0],
                    m.before.sick[1],
                    m.fouling_left,
                    m.fouling_reached
                ])
            })
            .collect();
        let with_sick = self
            .moves
            .iter()
            .filter(|m| m.before.sick.iter().sum::<u32>() > 0)
            .count();
        let caught: u32 = self.episodes.values().map(|e| e.1).sum();
        let all: u32 = self.episodes.values().map(|e| e.0).sum();
        let places: BTreeSet<(i32, i32)> = self.hearths.iter().map(|h| h.1).collect();
        json!({
            "on": true,
            "body_days": self.body_days,
            "sick_body_days": { "gut": self.sick_days[0], "chest": self.sick_days[1] },
            "episodes_all": all,
            "episodes_caught": caught,
            "episodes": episodes,
            "ends": ends,
            "deaths_columns": ["day", "ailment", "age", "thinness", "reserve_percent", "hearth_members", "hearth_fouling"],
            "deaths": deaths,
            "hearth_places": places.len(),
            "hearths_columns": ["day", "x", "y", "members", "sick_gut", "sick_chest", "fouling"],
            "hearths": hearths,
            "moves_all": self.moves.len(),
            "moves_with_sick": with_sick,
            "moves_columns": ["day", "from_x", "from_y", "to_x", "to_y", "movers", "members_before", "sick_gut_before", "sick_chest_before", "fouling_left", "fouling_reached"],
            "moves": moves,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bands_read_the_age_in_years() {
        assert_eq!(BANDS[band(0)].1, "0-1");
        assert_eq!(BANDS[band(1_999)].1, "0-1");
        assert_eq!(BANDS[band(2_000)].1, "2-4");
        assert_eq!(BANDS[band(14_999)].1, "5-14");
        assert_eq!(BANDS[band(39_500)].1, "15-39");
        assert_eq!(BANDS[band(59_999)].1, "40-59");
        assert_eq!(BANDS[band(80_000)].1, "60+");
    }

    #[test]
    fn off_it_says_so_alone() {
        let probe = IllnessProbe::default();
        assert_eq!(probe.json(), json!({ "on": false }));
    }
}
