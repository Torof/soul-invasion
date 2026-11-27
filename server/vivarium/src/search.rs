//! The search for an absent kin under glass (D56, amended 27 November
//! 2026): the absences noticed at the fire, the searches begun and how
//! each ended (the absent seen alive, seen dead, not found), the infants
//! taken up and brought home, and what became of each a season later.
//! Section `search` of the JSON (`docs/10`), new keys only; `on` false
//! and nothing else without the content's `search` section.

use serde_json::{Value, json};
use sim::brain::{News, SearchStep};
use sim::content::Content;
use sim::{BodyId, World};

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

    /// Counts the steps of the searches told in `batch`.
    pub fn news(&mut self, world: &World, batch: &[News]) {
        if !self.on {
            return;
        }
        for news in batch {
            let News::Search { step, .. } = news else {
                continue;
            };
            match *step {
                SearchStep::Noticed => self.noticed += 1,
                SearchStep::Begun { tiles } => {
                    self.begun += 1;
                    self.begun_tiles += u64::from(tiles);
                }
                SearchStep::FoundAlive => self.found_alive += 1,
                SearchStep::FoundDead => self.found_dead += 1,
                SearchStep::NotFound { reached: true } => self.not_found_reached += 1,
                SearchStep::NotFound { reached: false } => self.not_found_stopped += 1,
                SearchStep::InfantTaken { .. } => self.infants_taken += 1,
                SearchStep::InfantHome { infant } => {
                    self.infants_home += 1;
                    self.brought.push((infant, world.time().day, None));
                }
                SearchStep::InfantGiven { .. } => self.infants_given += 1,
            }
        }
    }

    /// Once a day: whether each infant brought home lives, a season on.
    pub fn tick(&mut self, world: &World) {
        if !self.on || self.brought.is_empty() {
            return;
        }
        let day = world.time().day;
        if self.last_day == Some(day) {
            return;
        }
        self.last_day = Some(day);
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

    /// The section `search` of the JSON.
    #[must_use]
    pub fn json(&self) -> Value {
        if !self.on {
            return json!({ "on": false });
        }
        let count = |what: Option<bool>| self.brought.iter().filter(|b| b.2 == what).count();
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
        probe.news(
            &world,
            &[
                told(SearchStep::Noticed),
                told(SearchStep::Begun { tiles: 40 }),
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
        probe.tick(&world);
        assert_eq!(probe.json()["infants_died_within_a_season"], 1);
    }
}
