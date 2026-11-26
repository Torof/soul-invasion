//! The passages remembered and told (D53, amended 26 November 2026):
//! measures only, read from the brain's news. A body that reached a place
//! across water or rock keeps the elbow of its way through which it came,
//! tells it at the fire with the place, and walks there in two legs.
//!
//! - **Water**: the dawns (body-days) on which a walk was barred by water,
//!   and the marks of seen, not reached made and lifted (by standing on the
//!   place, or by hearing a passage to it).
//! - **Passages**: found, told and heard; the walks through one begun and
//!   arrived, and the passages dropped when such a walk was barred.
//! - **Beyond**: the places of their notebooks the living stood on, the
//!   first time a day, that a straight walk from their camp could not
//!   reach; and the farthest place reached through a passage, in game
//!   minutes of the walker's own walk from its home in a straight line.
//!
//! Counted the same with the passages off (`--no-passages`), where only the
//! water, the marks and the places beyond can be non-zero. It only reads
//! the news: a run gives the same world with it as without it.

use serde_json::{Value, json};
use sim::brain::{News, PassageStep};

/// The run's passage counters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PassageProbe {
    /// Body-days a walk was barred by water.
    pub water_dawns: u64,
    /// Marks of seen, not reached made by a body's own walk.
    pub marks_made: u64,
    /// Marks lifted by standing on the place.
    pub marks_lifted_stood: u64,
    /// Marks lifted by a passage heard.
    pub marks_lifted_heard: u64,
    /// Passages found.
    pub found: u64,
    /// Passages told at a fire (each telling once).
    pub told: u64,
    /// Passages heard and held.
    pub heard: u64,
    /// Walks through a passage begun.
    pub begun: u64,
    /// Walks through a passage that arrived at the place.
    pub arrived: u64,
    /// Passages dropped, a walk through one barred by water or rock.
    pub dropped: u64,
    /// Places stood on, first a day, beyond water or rock from the camp.
    pub beyond: u64,
    /// The farthest place reached through a passage, in thousandths of a
    /// game minute of the walker's walk from its home.
    pub farthest_milliminutes: u64,
}

impl PassageProbe {
    /// Reads one piece of the brain's news.
    pub fn news(&mut self, news: &News) {
        match news {
            News::Barred { cause: Some(_), .. } => self.marks_made += 1,
            News::Reached { .. } => self.marks_lifted_stood += 1,
            News::Passage { step, .. } => match *step {
                PassageStep::WaterDay => self.water_dawns += 1,
                PassageStep::Beyond => self.beyond += 1,
                PassageStep::Found => self.found += 1,
                PassageStep::Told => self.told += 1,
                PassageStep::Heard { lifted } => {
                    self.heard += 1;
                    self.marks_lifted_heard += u64::from(lifted);
                }
                PassageStep::Begun => self.begun += 1,
                PassageStep::Arrived { milliminutes } => {
                    self.arrived += 1;
                    self.farthest_milliminutes = self.farthest_milliminutes.max(milliminutes);
                }
                PassageStep::Failed => self.dropped += 1,
            },
            _ => {}
        }
    }

    /// The JSON's `passages` (`docs/10-vivarium-data.md`).
    #[must_use]
    pub fn json(&self) -> Value {
        json!({
            "dawns_stopped_by_water": self.water_dawns,
            "marks": {
                "made": self.marks_made,
                "lifted_stood": self.marks_lifted_stood,
                "lifted_heard": self.marks_lifted_heard,
            },
            "found": self.found,
            "told": self.told,
            "heard": self.heard,
            "walks": {"begun": self.begun, "arrived": self.arrived, "dropped": self.dropped},
            "beyond_reached": self.beyond,
            "farthest_through_minutes_x10": self.farthest_milliminutes / 100,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::BodyId;

    #[test]
    fn the_news_count_into_the_passages() {
        let mut probe = PassageProbe::default();
        let step = |step| News::Passage {
            body: BodyId(1),
            place: (0, 0),
            step,
            at: 0,
        };
        for s in [
            PassageStep::WaterDay,
            PassageStep::Found,
            PassageStep::Told,
            PassageStep::Heard { lifted: true },
            PassageStep::Begun,
            PassageStep::Arrived {
                milliminutes: 12_345,
            },
            PassageStep::Arrived {
                milliminutes: 2_000,
            },
        ] {
            probe.news(&step(s));
        }
        let json = probe.json();
        assert_eq!(json["dawns_stopped_by_water"], 1);
        assert_eq!(json["marks"]["lifted_heard"], 1);
        assert_eq!(json["walks"]["arrived"], 2);
        assert_eq!(json["farthest_through_minutes_x10"], 123);
    }
}
