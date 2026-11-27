//! The problems under glass (D50 and D57, amended 26 November 2026): the
//! refusals of the world the bodies lived and remembered, the trials of
//! the hands begun from them and how each ended, and what the lead reads
//! beside them: the first container, the carriers and the mean will
//! cycle by cycle, the free draws of the hands. Section `problems` of the
//! JSON (`docs/10`), new keys only.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use sim::World;
use sim::brain::{Brains, HUNGRY, News, ProblemStep};
use sim::content::Content;

use crate::hands::{HandsProbe, carries_container};

/// One kind's counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KindCount {
    /// Times lived.
    pub lived: u64,
    /// Of them, lived at `HUNGRY` or more.
    pub lived_hungry: u64,
    /// Times a body whose will made it try could not begin (not free, or
    /// nothing new around): the problem waits on its list.
    pub waited: u32,
    /// Trials of the hands begun from it.
    pub began: u32,
    /// Of them, begun there and then, where it was lived.
    pub began_on_the_spot: u32,
    /// Of them, begun at the nightfall's free moment.
    pub began_later: u32,
    /// Of them, aimed first at deeds told with the same problem.
    pub began_heard: u32,
    /// Trials kept.
    pub kept: u32,
    /// Trials given up.
    pub abandoned: u32,
    /// Trials ended unjudged, nothing new tried.
    pub untried: u32,
    /// Trials ended unjudged, their season over.
    pub lapsed: u32,
}

/// A cycle's end.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CycleRow {
    /// The living, infants aside.
    pub living: u32,
    /// Of them, carrying a container.
    pub carriers: u32,
    /// Their wills summed.
    pub will_sum: u64,
}

/// The probe.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProblemsProbe {
    /// By kind, the content's index.
    pub kinds: BTreeMap<u8, KindCount>,
    /// The living counted each day: the person-days.
    pub person_days: u64,
    /// The last day counted.
    last_day: u64,
    /// By cycle.
    pub cycles: BTreeMap<u64, CycleRow>,
    /// The content's kinds, each its refusal: none when off.
    pub names: Vec<String>,
}

impl ProblemsProbe {
    /// A probe for a run on `content`.
    #[must_use]
    pub fn new(content: &Content) -> Self {
        Self {
            names: content
                .needs
                .problems
                .kinds
                .iter()
                .map(|k| k.refusal.clone())
                .collect(),
            ..Self::default()
        }
    }

    /// A tick: the living counted once a day.
    pub fn tick(&mut self, world: &World) {
        let day = world.time().day;
        if day == self.last_day {
            return;
        }
        self.last_day = day;
        let living = world
            .bodies()
            .iter()
            .filter(|b| b.condition().vitality.points() > 0)
            .count();
        self.person_days += u64::try_from(living).unwrap_or(u64::MAX);
    }

    /// The brain's news of a turn.
    pub fn news(&mut self, world: &World, batch: &[News]) {
        for news in batch {
            let News::Problem {
                body, kind, step, ..
            } = news
            else {
                continue;
            };
            let k = self.kinds.entry(*kind).or_default();
            match step {
                ProblemStep::Lived => {
                    k.lived += 1;
                    let hungry = world
                        .body(*body)
                        .is_some_and(|b| b.condition().hunger.points() >= HUNGRY);
                    k.lived_hungry += u64::from(hungry);
                }
                ProblemStep::Waited => k.waited += 1,
                ProblemStep::Began { heard, later } => {
                    k.began += 1;
                    k.began_heard += u32::from(*heard);
                    k.began_later += u32::from(*later);
                    k.began_on_the_spot += u32::from(!*later);
                }
                ProblemStep::Kept => k.kept += 1,
                ProblemStep::Abandoned => k.abandoned += 1,
                ProblemStep::Untried => k.untried += 1,
                ProblemStep::Lapsed => k.lapsed += 1,
            }
        }
    }

    /// A cycle's last tick, or the run's: who carries a container, and
    /// the mean will of the living, infants aside.
    pub fn cycle_end(&mut self, (world, content): (&World, &Content), brains: &Brains, cycle: u64) {
        let mut row = CycleRow::default();
        for b in world.bodies() {
            if world.is_infant(b, content) || b.condition().vitality.points() == 0 {
                continue;
            }
            row.living += 1;
            row.carriers += u32::from(carries_container(world, content, b));
            row.will_sum += brains.mind(b.id()).map_or(0, |m| u64::from(m.will()));
        }
        self.cycles.insert(cycle, row);
    }

    /// The section as JSON: on or off, by kind, the trials, the first
    /// container's day and the free draws (read from the hands' probe),
    /// the cycles.
    #[must_use]
    pub fn json(&self, hands: &HandsProbe) -> Value {
        // Per thousand person-days, rounded down: the vivarium writes no
        // float.
        let per_thousand = |n: u64| n.saturating_mul(1000) / self.person_days.max(1);
        let kinds: Vec<Value> = self
            .names
            .iter()
            .enumerate()
            .map(|(i, refusal)| {
                let k = u8::try_from(i)
                    .ok()
                    .and_then(|i| self.kinds.get(&i))
                    .copied()
                    .unwrap_or_default();
                json!({
                    "refusal": refusal,
                    "lived": k.lived,
                    "lived_per_1000_person_days": per_thousand(k.lived),
                    "lived_hungry": k.lived_hungry,
                    "trials": {
                        "waited": k.waited,
                        "began": k.began,
                        "began_on_the_spot": k.began_on_the_spot,
                        "began_later": k.began_later,
                        "began_on_a_tale": k.began_heard,
                        "kept": k.kept,
                        "abandoned": k.abandoned,
                        "untried": k.untried,
                        "lapsed": k.lapsed,
                    },
                })
            })
            .collect();
        let (free, heard) = hands
            .years
            .values()
            .fold((0, 0), |(f, h), y| (f + y.drew_free, h + y.drew_heard));
        let cycles: Vec<Value> = self
            .cycles
            .iter()
            .map(|(cycle, row)| {
                // Rounded to the nearest whole point.
                let living = u64::from(row.living);
                let mean = (row.will_sum + living / 2) / living.max(1);
                json!({
                    "cycle": cycle,
                    "living": row.living,
                    "carriers": row.carriers,
                    "mean_will": mean,
                })
            })
            .collect();
        json!({
            "on": !self.names.is_empty(),
            "person_days": self.person_days,
            "kinds": kinds,
            "first_container_day": hands.made.first().map(|m| m.day),
            "drew_free": free,
            "drew_heard": heard,
            "cycles": cycles,
        })
    }
}
