//! The report as data (D41, 22 November 2026): a JSON file beside the HTML
//! page, for the laboratory (D52) to read without parsing the page.
//!
//! The layout, every key with its unit, is documented in
//! `docs/10-vivarium-data.md`; this module is its only writer. In one
//! breath: `meta` (the format marker, the settings, the world, the camps,
//! the timing), `years` (each tribe's year in numbers, with its food),
//! `found` (each tribe's trials by the kind of the step), `people` (a line
//! per person with its life lines only), `curves` (the samples thinned as
//! the page thins them), `presence` (where each tribe's living were, by
//! year and cell), `positions` (every living body at noon, day by day, for
//! the lab's map of the movements), `map` (the page's picture). The
//! chronicle is not in it: it is on the page.
//!
//! Everything is deterministic but `meta.timing` (the wall and birth
//! seconds, and the date of the run from the OS clock).

use serde_json::{Map, Value, json};

use sim::brain::Way;
use sim::map::Pos;
use sim::{BodyId, Death, Sex};

use crate::{
    ACT_KEYS, DEATHS, DecisionYear, Person, Report, STEPS, Step, TrialLine, WILL_BANDS, WorldKind,
    is_fire_line, is_life_line, is_trial_line, name, thinned, tribe_name,
};

/// The top-level keys, in the order they are written.
const ORDER: [&str; 11] = [
    "meta",
    "years",
    "found",
    "people",
    "records",
    "hearth",
    "rumours",
    "curves",
    "presence",
    "positions",
    "map",
];

/// The format marker: raised when a key changes meaning or goes; a new key
/// does not raise it.
pub const FORMAT: u32 = 1;

/// The cause of a death as a key: the page's column heads.
const fn cause_key(cause: Death) -> &'static str {
    match cause {
        Death::Illness => "illness",
        Death::OldAge => "old_age",
        Death::Childbirth => "childbirth",
        Death::Starved => "hunger",
        Death::Dehydrated => "thirst",
        Death::Froze => "cold",
    }
}

/// The kind of a step as a key.
const fn step_key(step: Step) -> &'static str {
    match step {
        Step::Wider => "wider",
        Step::Narrower => "narrower",
        Step::SleepOut => "sleep_out",
        Step::SleepHome => "sleep_home",
        Step::StoreMore => "store_more",
        Step::StoreLess => "store_less",
        Step::HomeMoved => "home_moved",
    }
}

/// `[x, y]`.
const fn way_key(way: Way) -> &'static str {
    match way {
        Way::WorthHalfTiles => "worth_half_tiles",
        Way::SleepOut => "sleep_out",
        Way::StockKj => "stock_kj",
        Way::Home => "home",
    }
}

fn pos(p: Pos) -> Value {
    json!([p.x, p.y])
}

/// `[x, y]` or `null`.
fn pos_or_null(p: Option<Pos>) -> Value {
    p.map_or(Value::Null, pos)
}

/// A person's id as the page names it: its number.
fn id(body: BodyId) -> Value {
    json!(body.0)
}

/// Seven counters by the kind of the step, as an object.
fn by_step(counts: &[u32; 7]) -> Value {
    let mut out = Map::new();
    for (i, step) in STEPS.iter().enumerate() {
        out.insert(step_key(*step).to_owned(), json!(counts[i]));
    }
    Value::Object(out)
}

/// A trial as data: the day, the way's kind, and the judgement.
fn trial(t: &TrialLine) -> Value {
    json!({
        "day": t.day,
        "step": step_key(t.kind),
        "words": t.step,
        "judged_day": t.judged.map(|(day, _)| day),
        "kept": t.judged.map(|(_, kept)| kept),
        "will": t.will,
        "heard": t.heard,
    })
}

/// `"2026-11-22T15:04:00Z"` for `secs` since the Unix epoch, UTC: the
/// civil date by Howard Hinnant's algorithm, integers only.
fn iso_utc(secs: u64) -> String {
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rest = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

/// Writes `value` with objects on their own lines and arrays inline:
/// `"camps": [[12, 40], [80, 40]]` reads, and a curve of 1,200 points
/// takes one line instead of 1,200. Scalars are written by `serde_json`.
fn layout(value: &Value, depth: usize, out: &mut String) {
    match value {
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                layout(item, depth, out);
            }
            out.push(']');
        }
        Value::Object(fields) if fields.is_empty() => out.push_str("{}"),
        Value::Object(fields) => {
            out.push('{');
            for (i, (key, item)) in fields.iter().enumerate() {
                out.push_str(if i > 0 { ",\n" } else { "\n" });
                out.push_str(&"  ".repeat(depth + 1));
                out.push_str(&Value::String(key.clone()).to_string());
                out.push_str(": ");
                layout(item, depth + 1, out);
            }
            out.push('\n');
            out.push_str(&"  ".repeat(depth));
            out.push('}');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}

/// Writes an array with one item a line, each item laid out by [`layout`]:
/// `positions` is a thousand days of fifty bodies, one line a day reads
/// and `grep`s, where one line of a megabyte would not.
fn layout_lines(value: &Value, depth: usize, out: &mut String) {
    let Value::Array(items) = value else {
        return layout(value, depth, out);
    };
    if items.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push('[');
    for (i, item) in items.iter().enumerate() {
        out.push_str(if i > 0 { ",\n" } else { "\n" });
        out.push_str(&"  ".repeat(depth + 1));
        layout(item, depth + 1, out);
    }
    out.push('\n');
    out.push_str(&"  ".repeat(depth));
    out.push(']');
}

impl Report {
    /// The report as data, as `docs/10-vivarium-data.md` lays it out,
    /// without `meta.timing`: the same settings give the same value.
    #[must_use]
    pub fn json_value(&self) -> Value {
        json!({
            "meta": self.meta(),
            "years": self.years_json(),
            "found": self.found_json(),
            "people": self.people_json(),
            "records": self.records_json(),
            "hearth": self.hearth_json(),
            "rumours": self.rumours_json(),
            "curves": self.curves_json(),
            "presence": self.presence_json(),
            "positions": self.positions_json(),
            "map": self.map_json(),
        })
    }

    /// The whole file: [`Report::json_value`] with `meta.timing` (the wall
    /// and birth seconds, and the run's date from the OS clock, the one
    /// field no seed decides), laid out by `layout`, ending in a newline.
    #[must_use]
    pub fn json(&self) -> String {
        let mut value = self.json_value();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if let Some(meta) = value.get_mut("meta").and_then(Value::as_object_mut) {
            meta.insert(
                "timing".to_owned(),
                json!({
                    "wall_s": self.wall_s,
                    "birth_s": self.birth_s,
                    "run_date": iso_utc(now),
                    "helpers": self.settings.helpers,
                    "placed": self.placed,
                    "placed_ahead": self.placed_ahead,
                }),
            );
        }
        // The top level in the order of the doc, not of the alphabet; the
        // keys below it are sorted by `serde_json`.
        let mut out = String::from("{");
        for (i, key) in ORDER.iter().enumerate() {
            out.push_str(if i > 0 { ",\n  " } else { "\n  " });
            out.push_str(&Value::String((*key).to_owned()).to_string());
            out.push_str(": ");
            if *key == "positions" {
                layout_lines(&value[*key], 1, &mut out);
            } else {
                layout(&value[*key], 1, &mut out);
            }
        }
        out.push_str("\n}\n");
        out
    }

    fn meta(&self) -> Value {
        let s = &self.settings;
        let (world, side) = match s.world {
            WorldKind::Vivarium => ("vivarium", None),
            WorldKind::Patch { side } => ("patch", Some(side)),
        };
        json!({
            "format": FORMAT,
            "settings": {
                "world": world,
                "side": side,
                "people": s.people,
                "children": s.children,
                "infants": s.infants,
                "adult_years": [s.adult_years.0, s.adult_years.1],
                "fed": s.fed,
                "brain": s.brain,
                "tribes": s.tribes,
                "days": s.days,
                "ticks_per_second": s.ticks_per_second,
                "seed": s.seed,
                "trials": s.trials,
                "fire": s.fire,
                "decisions": s.decisions,
                "content": s.content.as_deref().and_then(|p| p.file_name()).and_then(|n| n.to_str()),
            },
            "map_tiles": [self.map_side.0, self.map_side.1],
            "camps": self.camps.iter().map(|&c| pos(c)).collect::<Vec<_>>(),
            "days_per_year": sim::time::Calendar::DEFAULT_DAYS_PER_YEAR,
            "ticks_per_day": self.ticks_per_day(),
            "ticks": self.ticks,
            "rates": self.rates.iter().map(|(name, rate)| json!({"name": name, "rate": rate})).collect::<Vec<_>>(),
            "memory": {
                "written": self.memory_counts.0,
                "freed": self.memory_counts.1,
                "held": self.records.len(),
            },
        })
    }

    /// The records of the common memory at the end (D55); a key of another
    /// kind is `null`.
    fn records_json(&self) -> Value {
        Value::Array(
            self.records
                .iter()
                .map(|r| {
                    json!({
                        "id": r.id,
                        "kind": r.kind,
                        "day": r.day,
                        "witness": r.witness.map(id),
                        "refs": r.refs,
                        "place": r.place.map(|(x, y)| [x, y]),
                        "states": (r.kind == "place").then(|| r.states.iter().map(|&(k, b)| [u32::from(k), u32::from(b)]).collect::<Vec<_>>()),
                        "way": r.way.map(way_key),
                        "up": r.up,
                        "kept": r.kept,
                        "will": r.will,
                        "present": (r.kind == "night").then_some(r.present),
                        "told": (r.kind == "night").then_some(r.told),
                    })
                })
                .collect(),
        )
    }

    /// The nights the hearths hold at the end (D55), by tribe.
    fn hearth_json(&self) -> Value {
        let tribes = u8::try_from(self.camps.len()).unwrap_or(u8::MAX);
        Value::Array(
            (0..tribes)
                .map(|tribe| {
                    let nights: Vec<Value> = self
                        .nights
                        .iter()
                        .filter(|n| n.tribe == tribe)
                        .map(|n| {
                            json!({
                                "day": n.day,
                                "camp": pos(n.camp),
                                "present": n.present,
                                "told": n.told.iter().map(|&(record, by)| json!({"record": record, "by": id(by)})).collect::<Vec<_>>(),
                            })
                        })
                        .collect();
                    json!({"tribe": tribe_name(tribe).to_string(), "nights": nights})
                })
                .collect(),
        )
    }

    /// The rumours dispelled (D55), in order.
    fn rumours_json(&self) -> Value {
        Value::Array(
            self.rumours
                .iter()
                .map(|r| {
                    json!({
                        "day": r.day,
                        "tribe": tribe_name(r.tribe).to_string(),
                        "body": id(r.body),
                        "place": [r.place.0, r.place.1],
                        "record": r.record,
                        "from": id(r.from),
                    })
                })
                .collect(),
        )
    }

    fn years_json(&self) -> Value {
        let per_year = u64::from(sim::time::Calendar::DEFAULT_DAYS_PER_YEAR);
        let years: Vec<Value> = self
            .years()
            .iter()
            .map(|y| {
                let growth_permille = (y.population_before > 0).then(|| {
                    (i64::from(y.population) - i64::from(y.population_before)) * 1000
                        / i64::from(y.population_before)
                });
                let interval_days = (!y.intervals_days.is_empty()).then(|| {
                    y.intervals_days.iter().sum::<u64>()
                        / u64::try_from(y.intervals_days.len()).unwrap_or(1)
                });
                let mut deaths = Map::new();
                for (i, cause) in DEATHS.iter().enumerate() {
                    deaths.insert(cause_key(*cause).to_owned(), json!(y.deaths[i]));
                }
                let food = self.food.get(&(y.tribe, y.year)).map(|b| {
                    json!({
                        "start_kj": b.start_j / 1000,
                        "gathered_kj": b.gathered_j / 1000,
                        "eaten_kj": b.eaten_j / 1000,
                        "spoiled_kj": b.spoiled_j / 1000,
                        "end_kj": b.end_j / 1000,
                        "lost_kj": b.lost_j() / 1000,
                        "thin_hours": b.thin_samples,
                        "weak_hours": b.weak_samples,
                        "stored_kj": {
                            "noon_mean": b.noon_mean_j().map(|j| j / 1000),
                            "noon_min": b.noon_min_j.map(|j| j / 1000),
                            "end": b.end_j / 1000,
                        },
                    })
                });
                let fire = self.fire.get(&(y.tribe, y.year)).copied().unwrap_or_default();
                let dec = self.decisions.get(&(y.tribe, y.year)).copied().unwrap_or_default();
                let knowledge = self.knowledge.get(&(y.tribe, y.year)).copied();
                let known = self.known_places.get(&(y.tribe, y.year));
                json!({
                    "tribe": tribe_name(y.tribe).to_string(),
                    "year": y.year,
                    "first_day": (y.year - 1) * per_year + 1,
                    "last_day": (y.year * per_year).min(self.last_day()),
                    "population": y.population,
                    "population_before": y.population_before,
                    "growth_permille": growth_permille,
                    "births": y.births,
                    "deaths": Value::Object(deaths),
                    "deaths_total": y.deaths.iter().sum::<u32>(),
                    "hunger_seasons": y.hunger_seasons,
                    "intervals_days": y.intervals_days,
                    "interval_days_mean": interval_days,
                    "fifteen": {"turned": y.fifteen.1, "alive": y.fifteen.0},
                    "fertile_women": {"total": y.fertile.1, "without_spouse": y.fertile.0},
                    "trials": y.trials,
                    "kept": y.kept,
                    "copied": y.copied,
                    "homes_moved": y.homes_moved,
                    "home_moves": self.home_moves.iter()
                        .filter(|m| m.tribe == y.tribe && m.day > (y.year - 1) * per_year && m.day <= y.year * per_year)
                        .map(|m| json!({"day": m.day, "body": id(m.body), "from": pos(m.from), "to": pos(m.to), "how": m.how}))
                        .collect::<Vec<_>>(),
                    "ways": (y.ways.minds > 0).then(|| json!({
                        "minds": y.ways.minds,
                        "sleep_out": y.ways.sleep_out,
                        "median_worth_tiles": y.ways.median_worth_tiles,
                        "median_stock_kj": y.ways.median_stock_kj,
                        "stock_kj": {
                            "q1": y.ways.stock_kj_quartiles[0],
                            "median": y.ways.stock_kj_quartiles[1],
                            "q3": y.ways.stock_kj_quartiles[2],
                        },
                    })),
                    "decisions": decisions_json(&dec),
                    "fire": {
                        "nights": fire.nights,
                        "told": fire.told,
                        "heard": fire.heard,
                        "dispelled": fire.dispelled,
                    },
                    "knowledge": knowledge.map(|k| json!({
                        "places": k.places,
                        "recorded": k.recorded,
                        "heard_only": k.heard_only,
                        "accesses": k.accesses,
                        "accesses_heard": k.accesses_heard,
                        "where": known.map_or_else(Vec::new, |places| places
                            .iter()
                            .map(|&((x, y), seen)| json!([x, y, u8::from(seen)]))
                            .collect::<Vec<_>>()),
                    })),
                    "camp": pos_or_null(y.ways.camp),
                    "food": food,
                })
            })
            .collect();
        Value::Array(years)
    }

    fn found_json(&self) -> Value {
        let tribes = u8::try_from(self.camps.len()).unwrap_or(u8::MAX);
        Value::Array(
            (0..tribes)
                .map(|tribe| {
                    let found = self.found(tribe);
                    json!({
                        "tribe": tribe_name(tribe).to_string(),
                        "tried": by_step(&found.tried),
                        "kept": by_step(&found.kept),
                        "copied": by_step(&found.copied),
                        "heard": {
                            "trials": found.heard_trials,
                            "copies": found.heard_copies,
                            "dispelled": found.dispelled,
                        },
                        "by_will": WILL_BANDS.iter().zip(found.by_will.iter()).map(|(&(band, from, to), &(tried, kept))| json!({
                            "band": band,
                            "from": from,
                            "to": to,
                            "tried": tried,
                            "kept": kept,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect(),
        )
    }

    fn person_json(&self, body: BodyId, person: &Person) -> Value {
        let (thin, weak) = Self::thin_weak_hours(person);
        let died = person.death.map(|(tick, cause, at)| {
            json!({
                "day": self.day_of(tick),
                "tick": tick,
                "cause": cause_key(cause),
                "season": self.season_name(tick),
                "at": pos(at),
            })
        });
        let born = if person.born_in_run {
            json!(Self::birth_day(person))
        } else {
            json!("start")
        };
        let lines: Vec<Value> = person
            .entries
            .iter()
            .filter(|e| is_life_line(&e.what) || is_trial_line(&e.what) || is_fire_line(&e.what))
            .map(|e| json!({"day": e.day, "tick": e.tick, "what": e.what}))
            .collect();
        json!({
            "id": id(body),
            "name": name(body),
            "tribe": tribe_name(person.tribe).to_string(),
            "sex": person.sex.map(|sex| match sex {
                Sex::Female => "woman",
                Sex::Male => "man",
            }),
            "born": born,
            "born_day": Self::birth_day(person),
            "age_days_at_start": (!person.born_in_run).then(|| -person.born_day),
            "born_at": pos_or_null(person.born_at),
            "died": died,
            "mother": person.mother.map(id),
            "father": person.father.map(id),
            "spouses": person.couplings.iter().map(|(day, spouse)| json!({"day": day, "id": id(*spouse)})).collect::<Vec<_>>(),
            "children": person.children.len(),
            "lost": person.lost,
            "will": person.will,
            "will_start": person.will_start,
            "believes": person.believes.iter().map(|&((x, y), access)| json!({
                "place": [x, y],
                "record": access.record,
                "seen": access.is_own(body),
                "by": id(access.by),
                "day": access.day,
            })).collect::<Vec<_>>(),
            "ways": person.ways.as_ref().map(|w| json!({
                "worth_half_tiles": w.worth_half_tiles,
                "sleep_out": w.sleep_out,
                "stock_kj": w.stock_kj,
                "home": pos(w.home),
            })),
            "trials": person.trials.iter().map(trial).collect::<Vec<_>>(),
            "copied": person.copied,
            "thin_hours": thin,
            "weak_hours": weak,
            "gathered_kj": person.gathered_j / 1000,
            "meals": person.meals.0,
            "drinks": person.drinks,
            "thoughts": person.thoughts,
            "habit": {
                "days": ACT_KEYS.iter().zip(person.habit).map(|(k, n)| ((*k).to_owned(), json!(n))).collect::<serde_json::Map<String, Value>>(),
                "found": person.habit_found,
                "bare": person.habit_bare,
                "years": person.habit_years.iter().map(|(year, counts)| json!({
                    "year": year,
                    "days": by_act(counts.days),
                    "fed_days": by_act(counts.fed),
                    "found": by_act(counts.found),
                })).collect::<Vec<_>>(),
            },
            "lines": lines,
        })
    }

    fn people_json(&self) -> Value {
        Value::Array(
            self.people
                .iter()
                .map(|(&body, person)| self.person_json(body, person))
                .collect(),
        )
    }

    /// The samples thinned as the page's curves thin them, one array a
    /// gauge, the tribes in the order of the camps.
    fn curves_json(&self) -> Value {
        let tribes = self.camps.len().clamp(1, 2);
        let samples: Vec<_> = thinned(&self.samples).collect();
        let per_tribe = |pick: &dyn Fn(&crate::Sample) -> [u32; 2]| -> Vec<Vec<u32>> {
            (0..tribes)
                .map(|t| samples.iter().map(|s| pick(s)[t]).collect())
                .collect()
        };
        json!({
            "points": samples.len(),
            "every": self.samples.len().div_ceil(crate::CURVE_POINTS).max(1),
            "ticks": samples.iter().map(|s| s.tick).collect::<Vec<_>>(),
            "days": samples.iter().map(|s| self.day_of(s.tick)).collect::<Vec<_>>(),
            "alive": samples.iter().map(|s| s.alive).collect::<Vec<_>>(),
            "alive_by_tribe": per_tribe(&|s| s.alive_by_tribe),
            "thin_by_tribe": per_tribe(&|s| s.thin_by_tribe),
            "weak_by_tribe": per_tribe(&|s| s.weak_by_tribe),
            "reserve_by_tribe": per_tribe(&|s| s.reserve_by_tribe),
            "hunger": samples.iter().map(|s| s.hunger).collect::<Vec<_>>(),
            "thirst": samples.iter().map(|s| s.thirst).collect::<Vec<_>>(),
            "fatigue": samples.iter().map(|s| s.fatigue).collect::<Vec<_>>(),
        })
    }

    /// Where each tribe's living were, by year: the noon counts by cell,
    /// zeros omitted.
    fn presence_json(&self) -> Value {
        Value::Array(
            self.presence
                .iter()
                .map(|(&(tribe, year), cells)| {
                    json!({
                        "tribe": tribe_name(tribe).to_string(),
                        "year": year,
                        "cell_tiles": crate::PRESENCE_CELL_TILES,
                        "cells": cells.iter().map(|(&(cx, cy), &n)| json!([cx, cy, n])).collect::<Vec<_>>(),
                    })
                })
                .collect(),
        )
    }

    /// Every living body at noon, day by day: `[[day, [[id, x, y], …]], …]`,
    /// days in order, bodies in id order.
    fn positions_json(&self) -> Value {
        Value::Array(
            self.positions
                .iter()
                .map(|(day, standing)| {
                    let bodies: Vec<Value> = standing
                        .iter()
                        .map(|&(body, p)| json!([body.0, p.x, p.y]))
                        .collect();
                    json!([day, bodies])
                })
                .collect(),
        )
    }

    /// The page's picture of the map, as the page embeds it, with what is
    /// needed to draw on it.
    fn map_json(&self) -> Value {
        let scale = self.map_scale.max(1);
        json!({
            "data_uri": format!("data:image/png;base64,{}", crate::png::base64(&self.map_png)),
            "tiles_per_pixel": scale,
            "tiles": [self.map_side.0, self.map_side.1],
            "pixels": [self.map_side.0.div_ceil(scale), self.map_side.1.div_ceil(scale)],
            "legend": {"camp": [220, 40, 40], "alive": [255, 255, 255], "dead": [0, 0, 0]},
        })
    }

    /// Hours a person was thin and weak, from its gauges (one a game hour,
    /// 24 a day): its fat reserve under half, under a fifth.
    fn thin_weak_hours(person: &Person) -> (u64, u64) {
        let count = |under: u32| {
            u64::try_from(person.gauges.iter().filter(|g| g.5 < under).count()).unwrap_or(0)
        };
        (count(50), count(20))
    }
}

/// Counts by [`sim::brain::Act`], keyed as the report says them.
fn by_act(counts: [u32; 5]) -> Value {
    let mut map = serde_json::Map::new();
    for (key, count) in ACT_KEYS.iter().zip(counts) {
        map.insert((*key).to_owned(), json!(count));
    }
    Value::Object(map)
}

/// A tribe-year's decisions at the fire, its groups, its days by what was
/// done and the mothers' distance at noon (D56).
fn decisions_json(dec: &DecisionYear) -> Value {
    let mean = |sum: u64, n: u32| (n > 0).then(|| sum / u64::from(n));
    json!({
        "evenings": dec.evenings,
        "full_told_evenings": dec.full_told,
        "said": by_act(dec.said),
        "calls_full": dec.calls_full,
        "calls_skipped": dec.skipped,
        "absent": dec.absent,
        "groups": {
            "formed": dec.groups,
            "members": dec.members,
            "followed": dec.followed,
            "full": dec.full,
            "left": dec.left,
            "ended": {"home": dec.ended[0], "bare": dec.ended[1], "night": dec.ended[2]},
            "moved_on": dec.moved_on,
            "stretch_ticks": {"mean": mean(dec.stretch.0, dec.stretch.1), "max": dec.stretch.2, "groups": dec.stretch.1},
            "together": dec.together,
            "waited_ticks": dec.waited,
            "waited_groups": dec.waited_groups,
        },
        "days_by_act": by_act(dec.days),
        "trips": {
            "found": by_act(dec.found),
            "bare": by_act(dec.bare),
            "fed_days": by_act(dec.fed),
        },
        "distance_noon_tiles": {
            "mothers": mean(dec.distance.0, dec.distance.1),
            "mother_noons": dec.distance.1,
            "others": mean(dec.distance.2, dec.distance.3),
            "other_noons": dec.distance.3,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_date_is_civil_and_utc() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso_utc(1_795_359_840), "2026-11-22T15:04:00Z");
    }

    #[test]
    fn the_layout_puts_objects_on_lines_and_arrays_inline() {
        let value = json!({"a": [1, [2, 3]], "b": {"c": null}, "d": {}});
        let mut out = String::new();
        layout(&value, 0, &mut out);
        assert_eq!(
            out,
            "{\n  \"a\": [1, [2, 3]],\n  \"b\": {\n    \"c\": null\n  },\n  \"d\": {}\n}"
        );
    }

    #[test]
    fn the_lines_layout_puts_one_item_a_line() {
        let value = json!([[1, [[7, 2, 3]]], [2, []]]);
        let mut out = String::new();
        layout_lines(&value, 1, &mut out);
        assert_eq!(out, "[\n    [1, [[7, 2, 3]]],\n    [2, []]\n  ]");
        let mut empty = String::new();
        layout_lines(&json!([]), 1, &mut empty);
        assert_eq!(empty, "[]");
    }
}
