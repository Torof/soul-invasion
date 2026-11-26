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

use sim::brain::{GoalEnd, Unheard, Way};
use sim::map::Pos;
use sim::{BodyId, Death, Sex};

use crate::{
    ACT_KEYS, DEATHS, DecisionYear, Person, ReachYear, Report, STEPS, Step, TrialLine, WILL_BANDS,
    WorldKind, is_fire_line, is_life_line, is_trial_line, name, thinned, tribe_name,
};

/// The top-level keys, in the order they are written.
const ORDER: [&str; 27] = [
    "meta",
    "years",
    "seasons",
    "bilan",
    "found",
    "people",
    "records",
    "hearth",
    "rumours",
    "curves",
    "presence",
    "positions",
    "move_calls",
    "moving",
    "hands",
    "settlements",
    "partings",
    "home_changes",
    "nights_apart",
    "nights_out",
    "pooling",
    "reach",
    "goals",
    "map",
    "bearers",
    "land_food",
    "relief",
];

/// The format marker: raised when a key changes meaning or goes; a new key
/// does not raise it.
pub const FORMAT: u32 = 2;

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
        Step::Hands => "hands",
    }
}

/// `[x, y]`.
const fn way_key(way: Way) -> &'static str {
    match way {
        Way::WorthHalfMinutes => "worth_half_minutes",
        Way::SleepOut => "sleep_out",
        Way::StockKj => "stock_kj",
        Way::Home => "home",
        Way::Hands => "hands",
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

/// The counters by the kind of the step, as an object.
fn by_step(counts: &[u32; 8]) -> Value {
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
        "untried_day": t.untried,
        "will": t.will,
        "heard": t.heard,
        "grades_permille": t.grades.map(|(old, new)| [old, new]),
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
    /// The settlements at each year's end (24 November 2026), the largest
    /// first.
    fn settlements_json(&self) -> Value {
        Value::Array(
            self.settlements
                .iter()
                .map(|(year, groups)| {
                    json!({
                        "year": year,
                        "groups": groups.iter().map(|g| json!({
                            "at": pos(g.at),
                            "people": g.people,
                            "by_tribe": g.by_tribe,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect(),
        )
    }

    /// Each parting with its couple's dawns before it (24 November 2026).
    fn partings_json(&self) -> Value {
        Value::Array(
            self.partings
                .iter()
                .map(|p| {
                    json!({
                        "day": p.day,
                        "pair": [p.pair[0].0, p.pair[1].0],
                        "dawns": p.dawns.iter().map(|d| json!({
                            "day": d.day,
                            "apart": d.apart,
                            "from_home": d.from_home,
                            "homes_apart": d.homes_apart,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect(),
        )
    }

    /// The fertile couples' dawns by age-year, together or apart and why
    /// (24 November 2026).
    fn nights_apart_json(&self) -> Value {
        Value::Array(
            self.nights_apart
                .iter()
                .map(|n| {
                    json!({
                        "dawns": n.dawns,
                        "together": n.together,
                        "homes_apart": n.homes_apart,
                        "sleep_out": n.sleep_out,
                        "goal": n.goal,
                        "home_trial": n.home_trial,
                        "caught_out": n.caught_out,
                    })
                })
                .collect(),
        )
    }

    /// The grown bodies' dawns by age-year, at home or away (25 November
    /// 2026).
    fn nights_out_json(&self) -> Value {
        Value::Array(
            self.nights_out
                .iter()
                .map(|n| {
                    json!({
                        "dawns": n.dawns,
                        "away": n.away,
                        "asleep": n.asleep,
                        "in_company": n.in_company,
                    })
                })
                .collect(),
        )
    }

    /// Each grown body's home changed between two dawns (24 November
    /// 2026).
    fn home_changes_json(&self) -> Value {
        Value::Array(
            self.home_changes
                .iter()
                .map(|c| {
                    json!({
                        "day": c.day,
                        "body": c.body.0,
                        "tribe": c.tribe,
                        "from": pos(c.from),
                        "to": pos(c.to),
                        "with": c.with,
                        "left_people": c.left_people,
                        "joined_people": c.joined_people,
                        "villages": c.villages,
                        "knew_villages": c.knew_villages,
                        "nearest_village": c.nearest_village.map(|(tiles, people)| json!({"tiles": tiles, "people": people})),
                        "known_places": c.known_places,
                    })
                })
                .collect(),
        )
    }

    /// How far the living went, year by year (24 November 2026, the brain
    /// in walking time): the gathers from home and the notebooks' farthest
    /// places, in game minutes of each one's own walk and in tiles, and
    /// the places forgotten for want of room.
    fn reach_json(&self) -> Value {
        // Tenths of a game minute, from thousandths.
        let minutes = |mm: u64| mm / 100;
        let q = ReachYear::quantile;
        Value::Array(
            self.reach
                .iter()
                .map(|(year, r)| {
                    let mean = |sum: u64, n: usize| {
                        sum.checked_div(u64::try_from(n).unwrap_or(1)).unwrap_or(0)
                    };
                    json!({
                        "year": year,
                        "gathers": r.gather_mm.len(),
                        "gather_minutes_x10_median": minutes(q(&r.gather_mm, 500)),
                        "gather_minutes_x10_p90": minutes(q(&r.gather_mm, 900)),
                        "gather_tiles_median": q(&r.gather_tiles, 500),
                        "gather_tiles_p90": q(&r.gather_tiles, 900),
                        "notebook_places_mean": mean(r.places.iter().sum(), r.places.len()),
                        "notebook_farthest_minutes_x10_median": minutes(q(&r.farthest_mm, 500)),
                        "notebook_farthest_minutes_x10_max": minutes(r.farthest_mm.iter().copied().max().unwrap_or(0)),
                        "notebook_farthest_tiles_median": q(&r.farthest_tiles, 500),
                        "notebook_farthest_tiles_max": r.farthest_tiles.iter().copied().max().unwrap_or(0),
                        "forgotten": r.forgotten,
                        "forgotten_tiles_mean": r.forgotten_tiles.checked_div(r.forgotten).unwrap_or(0),
                    })
                })
                .collect(),
        )
    }

    /// Moving as a consequence (amended 24 November 2026): the moves felt
    /// (pushed, or by a huge surprise), the surprises lived, the days gone
    /// to see, and each move's utility (the mover's hungry nights over the
    /// season after, against the stayers' over the same nights).
    fn moving_json(&self) -> Value {
        let m = &self.moving;
        let utility = self.move_utility();
        json!({
            "felt": m.felt.iter().map(|&(day, body, tribe, gain, push, surprise, huge)| json!({
                "day": day,
                "body": id(body),
                "tribe": tribe_name(tribe).to_string(),
                "gain_permille": gain,
                "push_permille": push,
                "surprise_permille": surprise,
                "huge": huge,
            })).collect::<Vec<_>>(),
            "surprised": m.surprised.iter().map(|&(day, body, tribe, gain, stayers, gathered, spouse)| json!({
                "day": day,
                "body": id(body),
                "tribe": tribe_name(tribe).to_string(),
                "gain_permille": gain,
                "stayers": stayers,
                "stayers_gathered_permille": gathered,
                "spouse_took_it": spouse,
            })).collect::<Vec<_>>(),
            "pushed": m.pushed.iter().map(|(&(tribe, year), &(dawns, pushed))| json!({
                "tribe": tribe_name(tribe).to_string(),
                "year": year,
                "dawns": dawns,
                "pushed": pushed,
            })).collect::<Vec<_>>(),
            "go_see": m.go_see.iter().map(|&(day, body, tribe, from)| json!({
                "day": day,
                "body": id(body),
                "tribe": tribe_name(tribe).to_string(),
                "from": id(from),
            })).collect::<Vec<_>>(),
            "utility": utility.iter().map(|u| json!({
                "day": u.day,
                "body": id(u.body),
                "tribe": tribe_name(u.tribe).to_string(),
                "mover_hungry_nights": u.mover,
                "stayers": u.stayers,
                "stayers_hungry_nights_milli": u.stayers_milli,
                "mover_drink_tiles_before": u.mover_drink.0,
                "mover_drink_tiles_after": u.mover_drink.1,
                "stayers_drink_tiles_after": u.stayers_drink,
            })).collect::<Vec<_>>(),
        })
    }

    /// The moves called at the fire (D56, amended 24 November 2026): each
    /// listener's balance, trust, home grade and spouse, and whether it
    /// joined.
    fn move_calls_json(&self) -> Value {
        Value::Array(
            self.move_calls
                .iter()
                .map(|c| {
                    json!({
                        "day": c.day,
                        "tribe": tribe_name(c.tribe).to_string(),
                        "caller": id(c.caller),
                        "caller_spouse": c.caller_spouse.map(id),
                        "caller_home_fare_permille": c.caller_home_fare,
                        "kept_from": c.from.map(id),
                        "camp": pos(c.camp),
                        "to": pos(c.to),
                        "tiles": (i64::from(c.to.x - c.camp.x).pow(2) + i64::from(c.to.y - c.camp.y).pow(2)).isqrt(),
                        "gain_permille": c.gain,
                        "weighed": c.weighed.iter().map(|(w, spouse)| json!({
                            "body": id(w.body),
                            "spouse": spouse.map(id),
                            "balance_permille": w.balance,
                            "trust": w.trust,
                            "home_fare_permille": w.home_fare,
                            "joined": w.joined,
                            "worth_permille": w.worth,
                        })).collect::<Vec<_>>(),
                        "caller_weighed": c.own.map(|w| json!({
                            "balance_permille": w.balance,
                            "joined": w.joined,
                        })),
                        "unheard": c.unheard.iter().map(|(b, why, spouse)| json!({
                            "body": id(*b),
                            "spouse": spouse.map(id),
                            "why": match why {
                                Unheard::Before => "before",
                                Unheard::SpouseAway => "spouse_away",
                                Unheard::Trial => "trial",
                                Unheard::Child => "child",
                            },
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect(),
        )
    }

    /// The goals held (D56, amended 24 November 2026): set, and how each
    /// ended.
    fn goals_json(&self) -> Value {
        Value::Array(
            self.goals
                .iter()
                .map(|g| {
                    json!({
                        "body": id(g.body),
                        "tribe": tribe_name(g.tribe).to_string(),
                        "day": g.day,
                        "to": pos(g.to),
                        "tiles": g.tiles,
                        "load_kj": g.load_kj,
                        "end": g.end.map(|(end, _)| match end {
                            GoalEnd::Arrived => "arrived",
                            GoalEnd::Load => "load",
                            GoalEnd::Hunger => "hunger",
                            GoalEnd::Thirst => "thirst",
                            GoalEnd::Dropped => "dropped",
                        }),
                        "end_day": g.end.map(|(_, day)| day),
                    })
                })
                .collect(),
        )
    }

    /// The report as data, as `docs/10-vivarium-data.md` lays it out,
    /// without `meta.timing`: the same settings give the same value.
    #[must_use]
    pub fn json_value(&self) -> Value {
        let mut value = json!({
            "meta": self.meta(),
            "years": self.years_json(),
            "seasons": self.seasons_json(),
            "bilan": self.bilan_json(),
            "found": self.found_json(),
            "people": self.people_json(),
            "records": self.records_json(),
            "hearth": self.hearth_json(),
            "rumours": self.rumours_json(),
            "curves": self.curves_json(),
            "presence": self.presence_json(),
            "positions": self.positions_json(),
            "move_calls": self.move_calls_json(),
            "moving": self.moving_json(),
            "settlements": self.settlements_json(),
            "goals": self.goals_json(),
            "map": self.map_json(),
            "bearers": self.bearers_json(),
            "land_food": self.land_food.json(),
            "relief": self.relief_json(),
        });
        value["partings"] = self.partings_json();
        value["home_changes"] = self.home_changes_json();
        value["nights_apart"] = self.nights_apart_json();
        value["nights_out"] = self.nights_out_json();
        // --- the scarcity chain (24 November 2026) ---
        value["chain"] = self.chain.json(self.ticks_per_day());
        // --- end of the scarcity chain ---
        // --- the hearth's pooling (25 November 2026) ---
        value["pooling"] = self.pooling.json();
        // --- the food carried home (25 November 2026) ---
        self.carried
            .add_json(&mut value["pooling"], self.ticks_per_day());
        // --- end of the food carried home ---
        // --- end of the hearth's pooling ---
        // --- the conceptions (25 November 2026) ---
        value["conception"] = self.conception.json();
        // --- end of the conceptions ---
        // --- the meetings between tribes (25 November 2026) ---
        value["meetings"] = self.meetings.json();
        // --- end of the meetings between tribes ---
        let changes: Vec<(u64, u8)> = self.home_changes.iter().map(|c| (c.day, c.tribe)).collect();
        let days = u64::from(self.calendar().days_per_age_year().max(1));
        let tribes = u8::try_from(self.camps.len()).unwrap_or(u8::MAX);
        value["hands"] = self.hands.json(&changes, days, tribes);
        value["reach"] = self.reach_json();
        // --- the problems (26 November 2026) ---
        value["problems"] = self.problems.json(&self.hands);
        // --- end of the problems ---
        // --- the passages (26 November 2026) ---
        value["passages"] = self.passages.json();
        // --- end of the passages ---
        // --- illness as episodes (26 November 2026) ---
        value["illness"] = self.illness.json();
        // --- end of illness as episodes ---
        value
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
                    "land_made": self.land_made,
                    "land_kept": self.land_kept,
                    "ground_made": self.ground_made,
                    "ground_kept": self.ground_kept,
                    "land_checked": self.land_checked,
                    "land_prepared_s": self.land_prepared_s,
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
        // --- the scarcity chain (24 November 2026): last, outside ORDER ---
        out.push_str(",\n  \"chain\": ");
        layout(&value["chain"], 1, &mut out);
        // --- end of the scarcity chain ---
        // --- the conceptions (25 November 2026): last, outside ORDER ---
        out.push_str(",\n  \"conception\": ");
        layout(&value["conception"], 1, &mut out);
        // --- end of the conceptions ---
        // --- the meetings between tribes (25 November 2026): last ---
        out.push_str(",\n  \"meetings\": ");
        layout(&value["meetings"], 1, &mut out);
        // --- end of the meetings between tribes ---
        // --- the problems (26 November 2026): last ---
        out.push_str(",\n  \"problems\": ");
        layout(&value["problems"], 1, &mut out);
        // --- end of the problems ---
        // --- the passages (26 November 2026): last ---
        out.push_str(",\n  \"passages\": ");
        layout(&value["passages"], 1, &mut out);
        // --- end of the passages ---
        // --- illness as episodes (26 November 2026): last ---
        out.push_str(",\n  \"illness\": ");
        layout(&value["illness"], 1, &mut out);
        // --- end of illness as episodes ---
        out.push_str("\n}\n");
        out
    }

    fn meta(&self) -> Value {
        let s = &self.settings;
        let (world, side) = match s.world {
            WorldKind::Vivarium => ("vivarium", None),
            WorldKind::VivariumWide => ("vivarium-wide", None),
            WorldKind::VivariumHuge => ("vivarium-huge", None),
            WorldKind::Patch { side } => ("patch", Some(side)),
        };
        json!({
            "format": FORMAT,
            "settings": {
                "world": world,
                "side": side,
                "people": s.people,
                "start": s.start.key(),
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
                "hands_known": s.hands_known,
                "fire": s.fire,
                "decisions": s.decisions,
                "moves": s.moves,
                "passages": s.passages,
                "old_illness": s.old_illness,
                "content": s.content.as_deref().and_then(|p| p.file_name()).and_then(|n| n.to_str()),
            },
            "map_tiles": [self.map_side.0, self.map_side.1],
            "camps": self.camps.iter().map(|&c| pos(c)).collect::<Vec<_>>(),
            "days_per_year": self.calendar().days_per_cycle(),
            "age_years_per_cycle": self.calendar().age_years_per_cycle(),
            "days_per_age_year": self.calendar().days_per_age_year(),
            "seconds_per_day": self.calendar().seconds_per_day(),
            "ticks_per_day": self.ticks_per_day(),
            "ticks": self.ticks,
            "rates": self.rates.iter().map(|(name, rate)| json!({"name": name, "rate": rate})).collect::<Vec<_>>(),
            "factors": self.factors.iter().map(|(key, value)| json!({"key": key, "value": value})).collect::<Vec<_>>(),
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

    /// The seasons' measure (D56, PR 1 ter), by tribe and season from the
    /// first: places believed bearing a noon, will and confidence in
    /// exploring, days, looked, out, bare, and who looked.
    fn seasons_json(&self) -> Value {
        let mean = |sum: u64, n: u32| (n > 0).then(|| sum * 10 / u64::from(n));
        Value::Array(
            self.seasons
                .iter()
                .map(|(&(tribe, season), p)| {
                    json!({
                        "tribe": tribe_name(tribe).to_string(),
                        "season": season,
                        "noons": p.noons,
                        "bearing_known_x10": mean(p.bearing, p.noons),
                        "lived_x10": mean(p.lived, p.noons),
                        "will_x10": mean(p.will, p.noons),
                        "explore_x10": mean(p.explore, p.noons),
                        "days": p.days,
                        "looked": p.looked,
                        "out": p.out,
                        "bare": p.bare,
                        "lookers_will_x10": mean(p.lookers_will, p.looked),
                        "lookers_explore_x10": mean(p.lookers_explore, p.looked),
                    })
                })
                .collect(),
        )
    }

    /// The review's measure (lot 5b): by standing, why each stopped and
    /// what it carried; the groups' stability; the couples at the dawns
    /// of conception; the confidence in gathering of the named and the
    /// set aside, and who they are.
    fn bilan_json(&self) -> Value {
        let b = &self.bilan;
        let mean = |sum: u64, n: u32| (n > 0).then(|| sum * 10 / u64::from(n));
        let standing: serde_json::Map<String, Value> = crate::STANDINGS
            .iter()
            .zip(&b.standing)
            .map(|(name, d)| {
                let not_full = d.why[2] + d.why[3];
                (
                    (*name).to_owned(),
                    json!({
                        "days": d.days,
                        "why": {
                            "never": d.why[0],
                            "full": d.why[1],
                            "group": d.why[2],
                            "nothing_left": d.why[3],
                            "night": d.why[4],
                        },
                        "gathered_x10": mean(d.gathered, d.days),
                        "at_stop_x10": mean(d.at_stop, d.days - d.why[0]),
                        "evening_x10": mean(d.evening, d.days),
                        "cut_short_x10": mean(d.cut_short, not_full),
                    }),
                )
            })
            .collect();
        let curve = |kind: usize| -> Vec<Value> {
            b.curve[kind]
                .iter()
                .map(|&(sum, n)| json!([mean(sum, n), n]))
                .collect()
        };
        let who = |kind: usize| -> serde_json::Map<String, Value> {
            crate::BANDS
                .iter()
                .zip(&b.who[kind])
                .map(|(band, n)| ((*band).to_owned(), json!(n)))
                .collect()
        };
        json!({
            "standing": standing,
            "groups": {
                "stable_permille": (b.stable_n > 0).then(|| b.stable_permille / u64::from(b.stable_n)),
                "members_two_days": b.stable_n,
                "same_caller": b.same_caller,
                "followers_two_days": b.followers_n,
            },
            "couples": {
                "met": b.couples[0],
                "apart": b.couples[1],
                "thin": b.couples[2],
                "apart_and_thin": b.couples[3],
            },
            "confidence": {
                "everyone_x10": mean(b.base.0, b.base.1),
                "set_aside_before_x10": mean(b.before[0].0, b.before[0].1),
                "named_before_x10": mean(b.before[1].0, b.before[1].1),
                "set_aside_after": curve(0),
                "named_after": curve(1),
            },
            "who": {
                "set_aside": who(0),
                "named": who(1),
                "noons": who(2),
            },
        })
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

    /// A tribe's couples parted in a year, each counted once, and the
    /// spouses its members took after their first (D49, amended 24
    /// November 2026).
    fn couples_of_year(&self, tribe: u8, year: u64, per_year: u64) -> Value {
        let in_year = |day: u64| day > (year - 1) * per_year && day <= year * per_year;
        let of_tribe = self.people.iter().filter(|(_, p)| p.tribe == tribe);
        // Each parting is the two's: counted by the lower id.
        let parted = of_tribe
            .clone()
            .flat_map(|(&me, p)| p.partings.iter().map(move |&(day, other)| (me, day, other)))
            .filter(|&(me, day, other)| in_year(day) && me < other)
            .count();
        let taken_again = of_tribe
            .flat_map(|(_, p)| p.couplings.iter().skip(1))
            .filter(|&&(day, _)| in_year(day))
            .count();
        json!({"parted": parted, "taken_again": taken_again})
    }

    fn years_json(&self) -> Value {
        let per_year = u64::from(self.calendar().days_per_cycle());
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
                    "couples": self.couples_of_year(y.tribe, y.year, per_year),
                    "home_moves": self.home_moves.iter()
                        .filter(|m| m.tribe == y.tribe && m.day > (y.year - 1) * per_year && m.day <= y.year * per_year)
                        .map(|m| json!({"day": m.day, "body": id(m.body), "from": pos(m.from), "to": pos(m.to), "how": m.how}))
                        .collect::<Vec<_>>(),
                    "ways": (y.ways.minds > 0).then(|| json!({
                        "minds": y.ways.minds,
                        "sleep_out": y.ways.sleep_out,
                        "median_worth_minutes": y.ways.median_worth_minutes,
                        "median_stock_kj": y.ways.median_stock_kj,
                        "home_grade_permille": y.ways.home_grades.map(|[min, median, max]| json!({"min": min, "median": median, "max": max})),
                        "stock_kj": {
                            "q1": y.ways.stock_kj_quartiles[0],
                            "median": y.ways.stock_kj_quartiles[1],
                            "q3": y.ways.stock_kj_quartiles[2],
                        },
                    })),
                    "decisions": decisions_json(&dec),
                    "reputed": self.reputed.get(&(y.tribe, y.year)).map(|rows| reputed_json(rows)),
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
        // Company (D53, amended 24 November 2026): its sociability, its
        // nights felt and those at no fire.
        let company = json!({
            "sociable": person.sociable,
            "nights": person.company.0,
            "alone": person.company.1,
        });
        let parted: Vec<Value> = person
            .partings
            .iter()
            .map(|(day, spouse)| json!({"day": day, "id": id(*spouse)}))
            .collect();
        let mut row = json!({
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
            "company": company,
            "believes": person.believes.iter().map(|&((x, y), access)| json!({
                "place": [x, y],
                "record": access.record,
                "seen": access.is_own(body),
                "by": id(access.by),
                "day": access.day,
            })).collect::<Vec<_>>(),
            "ways": person.ways.as_ref().map(|w| json!({
                "worth_half_minutes": w.worth_half_minutes,
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
                    "will": counts.will,
                    "confidence": counts.confidence.map(by_activity),
                    "successes": by_activity(counts.successes),
                    "failures": by_activity(counts.failures),
                })).collect::<Vec<_>>(),
            },
            "lines": lines,
        });
        // Beside the macro: one more key passes its recursion limit.
        row["parted"] = Value::Array(parted);
        row
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
    /// The food map (lab 6): the bearers of every chunk holding some, by
    /// season, and its three most common kinds.
    fn bearers_json(&self) -> Value {
        json!({
            "chunk_tiles": sim::map::CHUNK_SIZE,
            "margin_chunks": crate::BEARERS_MARGIN_CHUNKS,
            "seasons": ["spring", "summer", "autumn", "winter"],
            "kinds": self.bearers.kinds,
            "cells": self.bearers.cells.iter().map(|(x, y, seasons, kinds)| {
                json!([x, y, seasons, kinds.iter().map(|(k, n)| json!([k, n])).collect::<Vec<_>>()])
            }).collect::<Vec<_>>(),
        })
    }

    /// The relief map (lab 6): heights every few tiles, row by row.
    fn relief_json(&self) -> Value {
        json!({
            "cell_tiles": self.relief.cell,
            "size": [self.relief.size.0, self.relief.size.1],
            "known": self.relief.known,
            "dm": self.relief.dm,
        })
    }

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
fn by_act(counts: [u32; crate::ACTS]) -> Value {
    let mut map = serde_json::Map::new();
    for (key, count) in ACT_KEYS.iter().zip(counts) {
        map.insert((*key).to_owned(), json!(count));
    }
    Value::Object(map)
}

/// A mean by act: `sums` over the `days` of each act; `null` without a
/// day.
fn by_act_mean(sums: [u64; crate::ACTS], days: [u32; crate::ACTS]) -> Value {
    let mut map = serde_json::Map::new();
    for ((key, sum), n) in ACT_KEYS.iter().zip(sums).zip(days) {
        map.insert(
            (*key).to_owned(),
            json!((n > 0).then(|| sum / u64::from(n))),
        );
    }
    Value::Object(map)
}

/// Counts by activity (D56), keyed as `sim::content::ACTIVITIES` names
/// them.
fn by_activity<T: Into<Value> + Copy>(counts: [T; 3]) -> Value {
    let mut map = serde_json::Map::new();
    for (key, count) in sim::content::ACTIVITIES.iter().zip(counts) {
        map.insert((*key).to_owned(), count.into());
    }
    Value::Object(map)
}

/// The days one followed another a way (D56, PR 1 ter): how many, back
/// fed, back empty, and the mean per cent of its need gathered.
fn follows_json((days, fed, empty, percent): (u32, u32, u32, u64)) -> Value {
    json!({
        "days": days,
        "fed": fed,
        "empty": empty,
        "mean_need_percent": (days > 0).then(|| percent / u64::from(days)),
    })
}

/// Who a tribe's living believed best at each activity at a year's end
/// (D56, PR 1 ter), keyed as `sim::content::ACTIVITIES` names them, the
/// most believed first.
fn reputed_json(rows: &[crate::Reputed]) -> Value {
    let mut map = serde_json::Map::new();
    for (i, key) in sim::content::ACTIVITIES.iter().enumerate() {
        let list: Vec<Value> = rows
            .iter()
            .filter(|r| usize::from(r.activity) == i)
            .map(|r| {
                json!({
                    "who": id(r.who),
                    "believers": r.believers,
                    "mean_credit": r.credit / r.believers.max(1),
                })
            })
            .collect();
        if !list.is_empty() {
            map.insert((*key).to_owned(), Value::Array(list));
        }
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
        "barred": {
            "made": dec.barred[0],
            "heard": dec.barred[1],
            "cleared_stood": dec.barred[2],
            "held": dec.barred_held,
            "causes": {"water": dec.barred_causes[0], "rock": dec.barred_causes[1]},
        },
        "absent": dec.absent,
        "groups": {
            "formed": dec.groups,
            "members": dec.members,
            "followed": dec.followed,
            "full": dec.full,
            "left": dec.left,
            "ended": {"home": dec.ended[0], "bare": dec.ended[1], "night": dec.ended[2], "blocked": dec.ended[3]},
            "moved_on": dec.moved_on,
            "stretch_ticks": {"mean": mean(dec.stretch.0, dec.stretch.1), "max": dec.stretch.2, "groups": dec.stretch.1},
            "together": dec.together,
            "waited_ticks": dec.waited,
            "waited_groups": dec.waited_groups,
        },
        "days_by_act": by_act(dec.days),
        "need_percent_by_act": by_act_mean(dec.need_percent, dec.days),
        "trips": {
            "found": by_act(dec.found),
            "bare": by_act(dec.bare),
            "fed_days": by_act(dec.fed),
        },
        "wills": {"minds": dec.minds, "bold": dec.bold},
        "review": {
            "named": dec.review[0],
            "set_aside": dec.review[1],
            "refused": dec.review[2],
            "weighed_joins": dec.review[3],
            "asked": dec.review[4],
            "answered": dec.review[5],
            "said_nothing": {"stocked": dec.review[6], "no_place": dec.review[7], "all_taken": dec.review[8]},
            "set_aside_days": by_act(dec.aside_days),
            "set_aside_fed_days": by_act(dec.aside_fed),
        },
        "follows": {
            "joined": follows_json(dec.follows[0]),
            "asked": follows_json(dec.follows[1]),
            "told": follows_json(dec.follows[2]),
        },
        "judged": {
            "successes": by_activity(dec.successes),
            "failures": by_activity(dec.failures),
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
