//! The conceptions (25 November 2026): measures only, asked by the project
//! lead after the hunger bar (D58, amended): at richness 1.0 no child was
//! born after the thirteenth day on any of five seeds, every birth coming
//! from the founders' pregnancies. The suspicion to measure, not to fix:
//! the grown eat 62 to 79 % of their need, so a woman's fat reserve stays
//! under the gate of `conceive_reserve_percent` (D49).
//!
//! - **Every check**: at each dawn the world's conception rule runs
//!   (`World::conceive`, the quarter's first `conceive_nights` days), each
//!   woman of the fertile years, by the first condition that bars her, in
//!   the rule's own order: with child already; her reserve at or under the
//!   gate (by share left: under 25 %, 25 to 50 %, 50 % to the gate); an
//!   infant in her arms; no spouse (never wed, widowed, parted); her spouse
//!   beyond `mate_within_tiles`; all met but the quarter's draw said no;
//!   conceived. Read on the world just before the tick that runs the rule
//!   (what runs before it in that tick, the spoiling, touches no body);
//!   whether she conceived is read from the world just after.
//! - **Every woman-quarter**: a woman is checked on several dawns of a
//!   quarter with the one draw, so the checks over-count; a quarter counts
//!   as barred by a condition when it barred her on every check, as
//!   conceived when she conceived on one, else as mixed.
//! - **The pregnancies lost**, with the day and the mother's reserve; **the
//!   births**; **by season**, the fertile women's dawns above the gate.
//!
//! It only reads the world and its events: a run gives the same world with
//! it as without it.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use sim::content::Content;
use sim::time::{GameTime, Season, Tick};
use sim::{Body, BodyId, Event, Sex, World};

use crate::Person;

/// What first barred a woman at a check, in the rule's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Block {
    /// With child already.
    Pregnant,
    /// Her reserve at or under the gate, by share left: 0 under 25 %, 1
    /// from 25 to under 50 %, 2 from 50 % to the gate.
    Reserve(u8),
    /// An infant in her arms.
    Infant,
    /// No spouse, never wed.
    NeverWed,
    /// No spouse, her last one dead.
    Widowed,
    /// No spouse, parted from her last one.
    Parted,
    /// Her spouse beyond `mate_within_tiles`.
    SpouseFar,
    /// Every condition met, the quarter's draw said no.
    DrawNo,
    /// Conceived.
    Conceived,
}

/// The keys of the counts, in order; the reserve's three are the
/// [`Block::Reserve`] buckets.
pub const KEYS: [&str; 11] = [
    "pregnant",
    "reserve_0_25",
    "reserve_25_50",
    "reserve_50_80",
    "infant",
    "never_wed",
    "widowed",
    "parted",
    "spouse_far",
    "draw_no",
    "conceived",
];

impl Block {
    /// Its place in [`KEYS`].
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Block::Pregnant => 0,
            Block::Reserve(b) => 1 + usize::from(b.min(2)),
            Block::Infant => 4,
            Block::NeverWed => 5,
            Block::Widowed => 6,
            Block::Parted => 7,
            Block::SpouseFar => 8,
            Block::DrawNo => 9,
            Block::Conceived => 10,
        }
    }

    /// The same condition, whatever the reserve's bucket: a quarter barred
    /// by her reserve on every check is barred by it.
    fn kind(self) -> Block {
        match self {
            Block::Reserve(_) => Block::Reserve(0),
            other => other,
        }
    }
}

/// Her spouse as the rule sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spouse {
    /// None, never wed.
    NeverWed,
    /// None, the last one dead.
    Widowed,
    /// None, parted from the last one.
    Parted,
    /// Alive, within the rule's reach or not.
    Alive {
        /// Within `mate_within_tiles`.
        near: bool,
    },
}

/// The first condition that bars a woman, in `World::conceive`'s order;
/// `None` when all are met (the draw then decides). `share_percent` is her
/// reserve left in per cent of the whole; `above_gate` is whether her
/// reserve leaves her any chance: above the content's
/// `conceive_reserve_percent`, or under it with a chance by her reserve.
#[must_use]
pub fn first_block(
    pregnant: bool,
    (above_gate, share_percent): (bool, u64),
    infant: bool,
    spouse: Spouse,
) -> Option<Block> {
    if pregnant {
        return Some(Block::Pregnant);
    }
    if !above_gate {
        return Some(Block::Reserve(match share_percent {
            0..25 => 0,
            25..50 => 1,
            _ => 2,
        }));
    }
    if infant {
        return Some(Block::Infant);
    }
    match spouse {
        Spouse::NeverWed => Some(Block::NeverWed),
        Spouse::Widowed => Some(Block::Widowed),
        Spouse::Parted => Some(Block::Parted),
        Spouse::Alive { near: false } => Some(Block::SpouseFar),
        Spouse::Alive { near: true } => None,
    }
}

/// A woman's quarter: what barred her at each of its checks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Quarter {
    /// The checks, in order.
    pub checks: Vec<Block>,
    /// Her best reserve share over the checks, in per cent.
    pub best_share: u64,
}

impl Quarter {
    /// What the quarter counts as: conceived on any check; else the one
    /// condition that barred every check (the reserve by her best share);
    /// else `None`, mixed.
    #[must_use]
    pub fn verdict(&self) -> Option<Block> {
        if self.checks.contains(&Block::Conceived) {
            return Some(Block::Conceived);
        }
        let first = *self.checks.first()?;
        if !self.checks.iter().all(|b| b.kind() == first.kind()) {
            return None;
        }
        Some(match first {
            Block::Reserve(_) => {
                first_block(false, (false, self.best_share), false, Spouse::NeverWed)
                    .unwrap_or(first)
            }
            other => other,
        })
    }
}

/// A pregnancy lost (D49: she grew weak).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lost {
    /// The day, from 1.
    pub day: u64,
    /// The mother.
    pub mother: BodyId,
    /// Her reserve left then, in per cent, read at the loss's tick.
    pub reserve_percent: u64,
}

/// A season's fertile women at dawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SeasonDawns {
    /// Fertile women, counted at every dawn.
    pub fertile: u64,
    /// Of them, above the gate.
    pub above_gate: u64,
    /// Fertile women neither with child nor with an infant in her arms.
    pub free: u64,
    /// Of them, above the gate.
    pub free_above_gate: u64,
}

/// The probe.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConceptionProbe {
    /// Every check by what first barred her, as [`KEYS`].
    pub checks: [u64; KEYS.len()],
    /// Each woman's quarters: (woman, age-year, quarter).
    pub quarters: BTreeMap<(BodyId, u64, u8), Quarter>,
    /// The pregnancies lost, in order.
    pub lost: Vec<Lost>,
    /// The days of the births, in order.
    pub births: Vec<u64>,
    /// The dawns by (cycle, season).
    pub seasons: BTreeMap<(u64, Season), SeasonDawns>,
    /// The content's gate, `conceive_reserve_percent`.
    pub gate_percent: u32,
    /// The women who met every condition at the dawn being stepped, with
    /// her quarter's key: the world tells after the tick whether she
    /// conceived.
    pending: Vec<(u64, BodyId, u64, u8)>,
}

/// Her reserve left in per cent of the whole, and whether it is above the
/// gate by the rule's own comparison.
fn reserve(woman: &Body, content: &Content) -> (bool, u64) {
    let needs = &content.needs;
    let c = woman.condition();
    let (left, whole) = (c.reserve_left_j(needs), c.reserve_j(needs));
    let above = left * 100 > whole * u64::from(needs.life.conceive_reserve_percent);
    (above, (left * 100).checked_div(whole).unwrap_or(0))
}

impl ConceptionProbe {
    /// A probe for a run under `content`.
    #[must_use]
    pub fn new(content: &Content) -> Self {
        ConceptionProbe {
            gate_percent: content.needs.life.conceive_reserve_percent,
            ..ConceptionProbe::default()
        }
    }

    /// Before each tick: when the tick to come is a dawn, the fertile
    /// women's season; when it is a dawn the rule runs at, each fertile
    /// woman's first bar, read on the world as the rule will find it.
    pub fn before(&mut self, world: &World, content: &Content, people: &BTreeMap<BodyId, Person>) {
        if !world.life() {
            return;
        }
        let next: GameTime = world.calendar().time_at(Tick(world.tick().0 + 1));
        if !next.is_day_start() {
            return;
        }
        let life = &content.needs.life;
        let checking = next.day_of_age_quarter() < u64::from(life.conceive_nights);
        let reach = i32::try_from(life.mate_within_tiles).unwrap_or(i32::MAX);
        let mut season = SeasonDawns::default();
        for woman in world.bodies() {
            if woman.sex() != Sex::Female || !life.is_fertile(world.age_milli(woman)) {
                continue;
            }
            let pregnant = woman.pregnant().is_some();
            let (above, share) = reserve(woman, content);
            let infant = world
                .bodies()
                .iter()
                .any(|b| b.carried_by() == Some(woman.id()));
            season.fertile += 1;
            season.above_gate += u64::from(above);
            if !pregnant && !infant {
                season.free += 1;
                season.free_above_gate += u64::from(above);
            }
            if !checking {
                continue;
            }
            let spouse = match woman.spouse().and_then(|s| world.body(s)) {
                Some(s) => Spouse::Alive {
                    near: tiles_apart(s, woman) <= reach,
                },
                None => unwed(people.get(&woman.id()), woman.spouse().is_some()),
            };
            let quarter = self
                .quarters
                .entry((woman.id(), next.age_year, next.age_quarter))
                .or_default();
            quarter.best_share = quarter.best_share.max(share);
            // Under the gate, a woman with a chance by her reserve (D49,
            // amended 25 November 2026) is not barred by it: the draw
            // decides.
            let open = above
                || life.conceive_chance(
                    woman.condition().reserve_left_j(&content.needs),
                    woman.condition().reserve_j(&content.needs),
                ) > 0;
            match first_block(pregnant, (open, share), infant, spouse) {
                Some(block) => {
                    self.checks[block.index()] += 1;
                    quarter.checks.push(block);
                }
                None => self
                    .pending
                    .push((next.day, woman.id(), next.age_year, next.age_quarter)),
            }
        }
        let s = self.seasons.entry((next.cycle, next.season)).or_default();
        s.fertile += season.fertile;
        s.above_gate += season.above_gate;
        s.free += season.free;
        s.free_above_gate += season.free_above_gate;
    }

    /// After each tick: whether the women who met every condition
    /// conceived (with child since today; one dead in that very tick counts
    /// as the draw's no), and the tick's losses and births.
    pub fn after(&mut self, world: &World, content: &Content, events: &[Event]) {
        let today = world.today();
        for (_, woman, year, quarter) in std::mem::take(&mut self.pending) {
            let conceived = world
                .body(woman)
                .and_then(Body::pregnant)
                .is_some_and(|p| p.since_day == today);
            let block = if conceived {
                Block::Conceived
            } else {
                Block::DrawNo
            };
            self.checks[block.index()] += 1;
            self.quarters
                .entry((woman, year, quarter))
                .or_default()
                .checks
                .push(block);
        }
        let day = world.time().day;
        for event in events {
            match event {
                Event::PregnancyLost { mother, .. } => self.lost.push(Lost {
                    day,
                    mother: *mother,
                    reserve_percent: world.body(*mother).map_or(0, |b| reserve(b, content).1),
                }),
                Event::BodyBorn { .. } => self.births.push(day),
                _ => {}
            }
        }
    }

    /// The woman-quarters by verdict, as [`KEYS`] and then the mixed.
    #[must_use]
    pub fn by_quarter(&self) -> ([u64; KEYS.len()], u64) {
        let mut counts = [0; KEYS.len()];
        let mut mixed = 0;
        for q in self.quarters.values() {
            match q.verdict() {
                Some(b) => counts[b.index()] += 1,
                None => mixed += 1,
            }
        }
        (counts, mixed)
    }

    /// The `conception` block of the data.
    #[must_use]
    pub fn json(&self) -> Value {
        let keyed = |counts: &[u64; KEYS.len()]| {
            let map: serde_json::Map<String, Value> = KEYS
                .iter()
                .zip(counts)
                .map(|(k, n)| ((*k).to_string(), json!(n)))
                .collect();
            Value::Object(map)
        };
        let (quarters, mixed) = self.by_quarter();
        let mut woman_quarters = keyed(&quarters);
        woman_quarters["mixed"] = json!(mixed);
        woman_quarters["total"] = json!(self.quarters.len());
        json!({
            "gate_percent": self.gate_percent,
            "checks": keyed(&self.checks),
            "woman_quarters": woman_quarters,
            "lost": self.lost.iter().map(|l| json!({
                "day": l.day,
                "mother": l.mother.0,
                "reserve_percent": l.reserve_percent,
            })).collect::<Vec<_>>(),
            "births": self.births.len(),
            "birth_days": self.births,
            "seasons": self.seasons.iter().map(|(&(cycle, season), s)| json!({
                "cycle": cycle,
                "season": season,
                "fertile_dawns": s.fertile,
                "above_gate": s.above_gate,
                "free_dawns": s.free,
                "free_above_gate": s.free_above_gate,
            })).collect::<Vec<_>>(),
        })
    }
}

/// The tiles between two bodies, on the longer axis, as the rule counts.
fn tiles_apart(a: &Body, b: &Body) -> i32 {
    let (p, q) = (a.pos(), b.pos());
    (p.x - q.x).abs().max((p.y - q.y).abs())
}

/// Why a woman has no living spouse, from her record: never wed, parted
/// from her last spouse, or widowed (her last spouse not parted from, or
/// a spouse still named but no longer in the world).
fn unwed(person: Option<&Person>, named: bool) -> Spouse {
    if named {
        return Spouse::Widowed;
    }
    let Some(last) = person.and_then(|p| p.couplings.last()) else {
        return Spouse::NeverWed;
    };
    let parted = person.is_some_and(|p| {
        p.partings
            .iter()
            .any(|&(day, o)| o == last.1 && day >= last.0)
    });
    if parted {
        Spouse::Parted
    } else {
        Spouse::Widowed
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use sim::time::Calendar;

    use super::*;
    use crate::{Settings, WorldKind, load_content, run_with};

    /// Two fed tribes of six grown on a patch, a fortnight: the departure's
    /// couples meet the quarter's dawns. Every check falls in one count,
    /// every woman-quarter in one verdict; the conceptions are read from
    /// the world's own pregnancies; the births and losses are the run's.
    #[test]
    fn every_check_and_every_quarter_is_counted_once() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = load_content(&dir).expect("the repository's content reads and validates");
        let settings = Settings {
            world: WorldKind::Patch { side: 60 },
            people: 6,
            days: 14,
            ticks_per_second: 1,
            seconds_per_day: Calendar::DEFAULT_SECONDS_PER_DAY,
            days_per_cycle: Calendar::DEFAULT_DAYS_PER_CYCLE,
            age_years_per_cycle: Calendar::DEFAULT_AGE_YEARS_PER_CYCLE,
            seed: 5,
            content: None,
            lands: None,
            brain: true,
            tribes: 2,
            children: 1,
            infants: 1,
            adult_years: (18, 30),
            fed: true,
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
        };
        let report = run_with(&settings, &content).expect("the run runs");
        let probe = &report.conception;
        let checks: u64 = probe.checks.iter().sum();
        assert!(checks > 0, "{probe:?}");
        let listed: usize = probe.quarters.values().map(|q| q.checks.len()).sum();
        assert_eq!(checks, listed as u64);
        let nights = content.needs.life.conceive_nights as usize;
        assert!(
            probe
                .quarters
                .values()
                .all(|q| !q.checks.is_empty() && q.checks.len() <= nights)
        );
        let (quarters, mixed) = probe.by_quarter();
        assert_eq!(
            quarters.iter().sum::<u64>() + mixed,
            probe.quarters.len() as u64
        );
        let conceived = Block::Conceived.index();
        assert!(
            probe.checks[conceived] > 0,
            "the fed couples conceive: {probe:?}"
        );
        assert_eq!(
            quarters[conceived], probe.checks[conceived],
            "once a quarter at most"
        );
        let born = report.people.values().filter(|p| p.born_in_run).count();
        assert_eq!(probe.births.len(), born);
        let lost: usize = report.people.values().map(|p| p.lost.len()).sum();
        assert_eq!(probe.lost.len(), lost);
        assert!(
            probe
                .seasons
                .values()
                .all(|s| s.above_gate <= s.fertile && s.free_above_gate <= s.free)
        );
        let data: Value = serde_json::from_str(&report.json()).expect("the file is JSON");
        let c = &data["conception"];
        assert_eq!(c["woman_quarters"]["total"], json!(probe.quarters.len()));
        assert_eq!(c["checks"]["conceived"], json!(probe.checks[conceived]));
        assert_eq!(
            c["gate_percent"],
            json!(content.needs.life.conceive_reserve_percent)
        );
    }

    #[test]
    fn the_first_bar_follows_the_rules_order() {
        let near = Spouse::Alive { near: true };
        assert_eq!(
            first_block(true, (false, 10), true, Spouse::NeverWed),
            Some(Block::Pregnant)
        );
        assert_eq!(
            first_block(false, (false, 24), true, near),
            Some(Block::Reserve(0))
        );
        assert_eq!(
            first_block(false, (false, 25), true, near),
            Some(Block::Reserve(1))
        );
        assert_eq!(
            first_block(false, (false, 50), false, near),
            Some(Block::Reserve(2))
        );
        assert_eq!(
            first_block(false, (false, 80), false, near),
            Some(Block::Reserve(2))
        );
        assert_eq!(
            first_block(false, (true, 81), true, Spouse::Parted),
            Some(Block::Infant)
        );
        assert_eq!(
            first_block(false, (true, 90), false, Spouse::NeverWed),
            Some(Block::NeverWed)
        );
        assert_eq!(
            first_block(false, (true, 90), false, Spouse::Widowed),
            Some(Block::Widowed)
        );
        assert_eq!(
            first_block(false, (true, 90), false, Spouse::Parted),
            Some(Block::Parted)
        );
        assert_eq!(
            first_block(false, (true, 90), false, Spouse::Alive { near: false }),
            Some(Block::SpouseFar)
        );
        assert_eq!(first_block(false, (true, 90), false, near), None);
        let mut seen: Vec<usize> = [
            Block::Pregnant,
            Block::Reserve(0),
            Block::Reserve(1),
            Block::Reserve(2),
            Block::Infant,
            Block::NeverWed,
            Block::Widowed,
            Block::Parted,
            Block::SpouseFar,
            Block::DrawNo,
            Block::Conceived,
        ]
        .iter()
        .map(|b| b.index())
        .collect();
        seen.dedup();
        assert_eq!(seen, (0..KEYS.len()).collect::<Vec<_>>());
    }

    #[test]
    fn a_quarter_is_barred_only_by_what_barred_every_check() {
        let q = |checks: &[Block], best_share| Quarter {
            checks: checks.to_vec(),
            best_share,
        };
        assert_eq!(
            q(&[Block::SpouseFar; 4], 90).verdict(),
            Some(Block::SpouseFar)
        );
        assert_eq!(
            q(
                &[
                    Block::SpouseFar,
                    Block::Conceived,
                    Block::Pregnant,
                    Block::Pregnant
                ],
                90
            )
            .verdict(),
            Some(Block::Conceived)
        );
        assert_eq!(q(&[Block::SpouseFar, Block::DrawNo], 90).verdict(), None);
        assert_eq!(
            q(
                &[Block::Reserve(0), Block::Reserve(1), Block::Reserve(1)],
                30
            )
            .verdict(),
            Some(Block::Reserve(1)),
            "the reserve by her best share"
        );
        assert_eq!(q(&[], 0).verdict(), None);
    }

    #[test]
    fn no_spouse_is_told_from_her_record() {
        let mut p = Person::default();
        assert_eq!(unwed(Some(&p), false), Spouse::NeverWed);
        assert_eq!(unwed(None, false), Spouse::NeverWed);
        p.couplings.push((1, BodyId(7)));
        assert_eq!(unwed(Some(&p), false), Spouse::Widowed);
        p.partings.push((20, BodyId(7)));
        assert_eq!(unwed(Some(&p), false), Spouse::Parted);
        p.couplings.push((30, BodyId(9)));
        assert_eq!(unwed(Some(&p), false), Spouse::Widowed);
        assert_eq!(unwed(Some(&p), true), Spouse::Widowed);
    }
}
