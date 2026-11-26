//! The passages remembered and told (D53, amended 26 November 2026):
//! measures only, read from the brain's news and the gathers. A body that
//! reached a place across water or rock keeps the elbow of its way through
//! which it came, tells it at the fire with the place, and walks there in
//! two legs.
//!
//! - **Water**: the dawns (body-days) on which a walk was barred by water,
//!   and the marks of seen, not reached made and lifted (by standing on the
//!   place, or by hearing a passage to it).
//! - **Passages**: found, told and heard; the walks through one begun and
//!   arrived, and the passages dropped when such a walk was barred.
//! - **Beyond**: the places of their notebooks the living stood on, the
//!   first time a day, that a straight walk from their home could not
//!   reach (body-days, and the distinct places); the food gathered where a
//!   straight walk from the gatherer's home could not reach, against all it
//!   gathered; each by the home (the camp), since the land around a camp
//!   decides most of it (the audit of 26 November 2026); and the farthest
//!   place reached through a passage, in game minutes of the walker's own
//!   walk from its home in a straight line.
//!
//! Counted the same with the passages off (`--no-passages`), where only the
//! water, the marks and the beyond can be non-zero. It only reads: a run
//! gives the same world with it as without it.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use sim::brain::{Brains, News, PassageStep, PlaceKey};
use sim::map::{Map, Pos, line_clear};

/// What one home's people reached and gathered beyond water or rock.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CampBeyond {
    /// Body-days standing on a place of the notebook beyond.
    pub days: u64,
    /// The distinct places beyond stood on.
    pub places: BTreeSet<PlaceKey>,
    /// Gathers ended, and those beyond.
    pub gathers: (u64, u64),
    /// Kilojoules gathered, and those beyond.
    pub kj: (u64, u64),
}

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
    /// Passages dropped that were heard, not found.
    pub dropped_heard: u64,
    /// Places stood on, first a day, beyond water or rock from the home.
    pub beyond: u64,
    /// The farthest place reached through a passage, in thousandths of a
    /// game minute of the walker's walk from its home.
    pub farthest_milliminutes: u64,
    /// The beyond, by home.
    pub camps: BTreeMap<Pos, CampBeyond>,
}

impl PassageProbe {
    /// Reads one piece of the brain's news.
    pub fn news(&mut self, news: &News, brains: &Brains) {
        match news {
            News::Barred { cause: Some(_), .. } => self.marks_made += 1,
            News::Reached { .. } => self.marks_lifted_stood += 1,
            News::Passage {
                step, body, place, ..
            } => match *step {
                PassageStep::WaterDay => self.water_dawns += 1,
                PassageStep::Beyond => {
                    self.beyond += 1;
                    if let Some(mind) = brains.mind(*body) {
                        let camp = self.camps.entry(mind.home()).or_default();
                        camp.days += 1;
                        camp.places.insert(*place);
                    }
                }
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
                PassageStep::Failed { heard } => {
                    self.dropped += 1;
                    self.dropped_heard += u64::from(heard);
                }
            },
            _ => {}
        }
    }

    /// A gather of `kj` ended at `at` by a body whose home is `home`:
    /// beyond when the straight line between them crosses water or rock
    /// (the brain's own clear line).
    pub fn gathered(&mut self, map: &Map, (home, at): (Pos, Pos), kj: u64) {
        let beyond = !line_clear(home.centre(), at.centre(), |p| map.is_passable(p));
        let camp = self.camps.entry(home).or_default();
        camp.gathers.0 += 1;
        camp.kj.0 += kj;
        if beyond {
            camp.gathers.1 += 1;
            camp.kj.1 += kj;
        }
    }

    /// The JSON's `passages` (`docs/10-vivarium-data.md`).
    #[must_use]
    pub fn json(&self) -> Value {
        let places: BTreeSet<PlaceKey> = self
            .camps
            .values()
            .flat_map(|c| c.places.iter().copied())
            .collect();
        let (kj, kj_beyond) = self
            .camps
            .values()
            .fold((0, 0), |(a, b), c| (a + c.kj.0, b + c.kj.1));
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
            "dropped_heard": self.dropped_heard,
            "beyond_reached": self.beyond,
            "beyond_places": places.len(),
            "gathered_kj": kj,
            "gathered_kj_beyond": kj_beyond,
            "farthest_through_minutes_x10": self.farthest_milliminutes / 100,
            "camps": self
                .camps
                .iter()
                .map(|(home, c)| {
                    json!({
                        "home": [home.x, home.y],
                        "beyond_reached": c.days,
                        "beyond_places": c.places.len(),
                        "gathers": c.gathers.0,
                        "gathers_beyond": c.gathers.1,
                        "gathered_kj": c.kj.0,
                        "gathered_kj_beyond": c.kj.1,
                    })
                })
                .collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::BodyId;
    use sim::map::Terrain;

    #[test]
    fn the_news_count_into_the_passages() {
        let mut probe = PassageProbe::default();
        let brains = Brains::new(1);
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
            probe.news(&step(s), &brains);
        }
        let json = probe.json();
        assert_eq!(json["dawns_stopped_by_water"], 1);
        assert_eq!(json["marks"]["lifted_heard"], 1);
        assert_eq!(json["walks"]["arrived"], 2);
        assert_eq!(json["farthest_through_minutes_x10"], 123);
    }

    #[test]
    fn a_gather_across_water_counts_beyond_by_home() {
        let (dry, wet) = (
            Map::filled(40, 40, Terrain::Grass),
            Map::filled(40, 40, Terrain::Water),
        );
        let mut probe = PassageProbe::default();
        let home = Pos::new(5, 5);
        probe.gathered(&dry, (home, Pos::new(10, 30)), 100);
        probe.gathered(&wet, (home, Pos::new(30, 30)), 40);
        let json = probe.json();
        assert_eq!(json["gathered_kj"], 140);
        assert_eq!(json["gathered_kj_beyond"], 40);
        assert_eq!(json["camps"][0]["home"], json!([5, 5]));
        assert_eq!(json["camps"][0]["gathers_beyond"], 1);
    }
}
