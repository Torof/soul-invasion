//! The journal of a run (27 November 2026, `--journal FILE`): the news
//! of the brains and the world's events that tell of things and lives,
//! one JSON line each, in a file of its own. The report's counters say
//! how many; the journal says who, when, and in what order, so that a
//! "why" is read on one person's or one thing's story instead of being
//! guessed. Off by default: nothing is written and the run is as it was.
//!
//! A line: `{"t": tick, "d": day, "k": "news" | "event", "b": body or
//! null, "w": what}`. A news' `w` is its name and fields as the brain
//! prints them; an event's is its JSON. The last line tells how many
//! lines were written and the seconds it took. A gathering stopped also
//! writes a `"k": "carry"` line: the grams of food the body carries,
//! those of them in a hollow, what its arms hold, and the things on it
//! that are not food. A plant picked bare is not written.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::json;
use sim::brain::News;
use sim::{Content, Event, WorkEnd, World};

use crate::RunError;

/// The journal being written, or none.
pub struct Journal {
    out: Option<BufWriter<File>>,
    lines: u64,
    spent: Duration,
}

/// Whether `event` tells of a thing or a life: the steps, the looks and
/// the gauges of every tick are left out, they would drown the rest.
fn told(event: &Event) -> bool {
    matches!(
        event,
        Event::Gestured { .. }
            | Event::BarkLifted { .. }
            | Event::TreeBitten { .. }
            | Event::TreeFell { .. }
            | Event::GestureRefused { .. }
            | Event::WorkStopped { .. }
            | Event::GatherRefused { .. }
            | Event::ActRefused { .. }
            | Event::ItemAdded { .. }
            | Event::ItemRemoved { .. }
            | Event::Kept { .. }
            | Event::Spoiled { .. }
            | Event::BodyBorn { .. }
            | Event::BodyDied { .. }
            | Event::FellIll { .. }
            | Event::IllnessEnded { .. }
            | Event::Coupled { .. }
            | Event::Parted { .. }
    )
}

/// The first body a printed news or event names, if any (`body:
/// BodyId(12)` as the brain prints it, `"body":12` in an event's JSON).
fn body_in(text: &str) -> Option<u64> {
    let at = text
        .find("BodyId(")
        .map(|i| i + 7)
        .or_else(|| text.find("\"body\":").map(|i| i + 7))?;
    let digits: String = text[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

impl Journal {
    /// The journal at `path`, or none.
    ///
    /// # Errors
    ///
    /// The file cannot be made.
    pub fn open(path: Option<&Path>) -> Result<Self, RunError> {
        let out = match path {
            Some(path) => Some(BufWriter::new(
                File::create(path).map_err(|e| RunError::Read(path.to_path_buf(), e))?,
            )),
            None => None,
        };
        Ok(Self {
            out,
            lines: 0,
            spent: Duration::ZERO,
        })
    }

    /// The tick's events and news, written.
    pub fn write(
        &mut self,
        (world, rules): (&World, &Content),
        (tick, day): (u64, u64),
        events: &[Event],
        news: &[News],
    ) {
        let Some(out) = self.out.as_mut() else {
            return;
        };
        let start = Instant::now();
        // A plant picked bare is the commonest line and tells nothing of
        // a thing or a life: left out.
        let bare = |e: &Event| {
            matches!(
                e,
                Event::WorkStopped {
                    why: WorkEnd::Bare,
                    ..
                }
            )
        };
        for event in events.iter().filter(|e| told(e) && !bare(e)) {
            let what = serde_json::to_value(event).unwrap_or_default();
            let body = body_in(&what.to_string());
            let line = json!({"t": tick, "d": day, "k": "event", "b": body, "w": what});
            let _ = writeln!(out, "{line}");
            self.lines += 1;
            if let Event::WorkStopped { body: id, .. } = event
                && let Some(body) = world.body(*id)
            {
                let things: Vec<(u64, &str)> = body
                    .held()
                    .into_iter()
                    .chain(body.carried().iter().copied())
                    .filter_map(|i| world.item(i).map(|item| (i.0, item.material())))
                    .filter(|(_, m)| rules.material(m).is_none_or(|m| m.food.is_none()))
                    .collect();
                let what = json!({
                    "food_g": body.food_g(),
                    "stowed_g": body.stowed_g(),
                    "arms_g": body.arms_g(&rules.movement),
                    "things": things,
                });
                let line = json!({"t": tick, "d": day, "k": "carry", "b": id.0, "w": what});
                let _ = writeln!(out, "{line}");
                self.lines += 1;
            }
        }
        for one in news {
            let what = format!("{one:?}");
            let body = body_in(&what);
            let line = json!({"t": tick, "d": day, "k": "news", "b": body, "w": what});
            let _ = writeln!(out, "{line}");
            self.lines += 1;
        }
        self.spent += start.elapsed();
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        if let Some(out) = self.out.as_mut() {
            let line =
                json!({"k": "end", "lines": self.lines, "journal_s": self.spent.as_secs_f64()});
            let _ = writeln!(out, "{line}");
            let _ = out.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::body_in;

    #[test]
    fn the_body_a_line_names_is_read_from_a_news_and_from_an_event() {
        assert_eq!(body_in("Tried { body: BodyId(12), kept: true }"), Some(12));
        assert_eq!(body_in("{\"type\":\"body_died\",\"body\":7}"), Some(7));
        assert_eq!(body_in("Evening { camp: 3 }"), None);
    }
}
