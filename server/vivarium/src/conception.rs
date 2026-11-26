//! The conceptions (25 November 2026): measures only, asked by the project
//! lead after the hunger bar (D58, amended): at richness 1.0 no child was
//! born after the thirteenth day on any of five seeds, every birth coming
//! from the founders' pregnancies. Since the same day the rule follows her
//! reserve's gain, not its level (D49, amended 25 November 2026): only a
//! weak woman is barred by her reserve, and the checks that reach the
//! draw are counted by her gain.
//!
//! - **Every check**: at each dawn the world's conception rule runs
//!   (`World::conceive`, the quarter's first `conceive_nights` days), each
//!   woman of the fertile years, by the first condition that bars her, in
//!   the rule's own order: with child already; weak; an infant in her
//!   arms; no spouse (never wed, widowed, parted); her spouse beyond
//!   `mate_within_tiles`; all met but the quarter's draw said no;
//!   conceived.
//! - **By her gain**: the checks that reached the draw and the conceptions,
//!   by her reserve's gain (`Body::reserve_gain_percent`): falling (under
//!   0), slow (0 to under the table's last point), good (at it or over, or
//!   her reserve full). Read on the world just before the tick that runs the rule
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
    /// Weak: under the content's `weak_percent` of her reserve.
    Weak,
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

/// The keys of the counts, in order.
pub const KEYS: [&str; 9] = [
    "pregnant",
    "weak",
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
            Block::Weak => 1,
            Block::Infant => 2,
            Block::NeverWed => 3,
            Block::Widowed => 4,
            Block::Parted => 5,
            Block::SpouseFar => 6,
            Block::DrawNo => 7,
            Block::Conceived => 8,
        }
    }
}

/// Her reserve's gain at a check that reached the draw, as the keys of
/// [`GAINS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gain {
    /// Falling: under 0.
    Falling,
    /// From 0 to under the content's last point.
    Slow,
    /// At the last point or over, or her reserve full: the table's best.
    Good,
}

/// The keys of the gain's counts, in order.
pub const GAINS: [&str; 3] = ["falling", "slow", "good"];

impl Gain {
    /// Her band, from her gain and whether her chance is the table's best.
    #[must_use]
    pub fn of(gain_percent: i64, best: bool) -> Gain {
        if best {
            Gain::Good
        } else if gain_percent < 0 {
            Gain::Falling
        } else {
            Gain::Slow
        }
    }

    fn index(self) -> usize {
        match self {
            Gain::Falling => 0,
            Gain::Slow => 1,
            Gain::Good => 2,
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
/// `None` when all are met (the draw then decides). `weak`: her reserve
/// leaves her no chance (D49, amended 25 November 2026).
#[must_use]
pub fn first_block(pregnant: bool, weak: bool, infant: bool, spouse: Spouse) -> Option<Block> {
    if pregnant {
        return Some(Block::Pregnant);
    }
    if weak {
        return Some(Block::Weak);
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
}

impl Quarter {
    /// What the quarter counts as: conceived on any check; else the one
    /// condition that barred every check; else `None`, mixed.
    #[must_use]
    pub fn verdict(&self) -> Option<Block> {
        if self.checks.contains(&Block::Conceived) {
            return Some(Block::Conceived);
        }
        let first = *self.checks.first()?;
        self.checks.iter().all(|&b| b == first).then_some(first)
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
    /// Of them, her reserve rising (a gain over 0).
    pub rising: u64,
    /// Fertile women neither with child nor with an infant in her arms.
    pub free: u64,
    /// Of them, her reserve rising.
    pub free_rising: u64,
}

/// The probe.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConceptionProbe {
    /// Every check by what first barred her, as [`KEYS`].
    pub checks: [u64; KEYS.len()],
    /// Each woman's quarters: (woman, age-year, quarter).
    pub quarters: BTreeMap<(BodyId, u64, u8), Quarter>,
    /// The checks that reached the draw by her gain, as [`GAINS`]: (the
    /// checks, the conceptions).
    pub by_gain: [(u64, u64); GAINS.len()],
    /// The pregnancies lost, in order.
    pub lost: Vec<Lost>,
    /// The days of the births, in order.
    pub births: Vec<u64>,
    /// The dawns by (cycle, season).
    pub seasons: BTreeMap<(u64, Season), SeasonDawns>,
    /// The content's `conceive_gain_days`: the dawns her gain is read over.
    pub gain_days: u32,
    /// The women who met every condition at the dawn being stepped, with
    /// her quarter's key and her gain's band: the world tells after the
    /// tick whether she conceived.
    pending: Vec<(u64, BodyId, u64, u8, Gain)>,
}

/// Her reserve left in per cent of the whole.
fn reserve(woman: &Body, content: &Content) -> u64 {
    let c = woman.condition();
    let (left, whole) = (
        c.reserve_left_j(&content.needs),
        c.reserve_j(&content.needs),
    );
    (left * 100).checked_div(whole).unwrap_or(0)
}

impl ConceptionProbe {
    /// A probe for a run under `content`.
    #[must_use]
    pub fn new(content: &Content) -> Self {
        ConceptionProbe {
            gain_days: content.needs.life.conceive_gain_days,
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
            let gain = woman.reserve_gain_percent(&content.needs);
            let rising = gain > 0;
            let infant = world
                .bodies()
                .iter()
                .any(|b| b.carried_by() == Some(woman.id()));
            season.fertile += 1;
            season.rising += u64::from(rising);
            if !pregnant && !infant {
                season.free += 1;
                season.free_rising += u64::from(rising);
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
            // The rule's own chance: nil only for a weak woman, or with no
            // table at all.
            let chance = sim::life::conceive_chance(woman, &content.needs);
            if let Some(block) = first_block(pregnant, chance == 0, infant, spouse) {
                self.checks[block.index()] += 1;
                quarter.checks.push(block);
            } else {
                let best = life.conceive_percent_by_gain.last().map(|p| u64::from(p.1));
                let band = Gain::of(gain, best == Some(chance));
                self.by_gain[band.index()].0 += 1;
                self.pending
                    .push((next.day, woman.id(), next.age_year, next.age_quarter, band));
            }
        }
        let s = self.seasons.entry((next.cycle, next.season)).or_default();
        s.fertile += season.fertile;
        s.rising += season.rising;
        s.free += season.free;
        s.free_rising += season.free_rising;
    }

    /// After each tick: whether the women who met every condition
    /// conceived (with child since today; one dead in that very tick counts
    /// as the draw's no), and the tick's losses and births.
    pub fn after(&mut self, world: &World, content: &Content, events: &[Event]) {
        let today = world.today();
        for (_, woman, year, quarter, band) in std::mem::take(&mut self.pending) {
            let conceived = world
                .body(woman)
                .and_then(Body::pregnant)
                .is_some_and(|p| p.since_day == today);
            let block = if conceived {
                self.by_gain[band.index()].1 += 1;
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
                    reserve_percent: world.body(*mother).map_or(0, |b| reserve(b, content)),
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
        let by_gain: serde_json::Map<String, Value> = GAINS
            .iter()
            .zip(&self.by_gain)
            .map(|(k, (met, conceived))| {
                (
                    (*k).to_string(),
                    json!({ "met": met, "conceived": conceived }),
                )
            })
            .collect();
        json!({
            "gain_days": self.gain_days,
            "checks": keyed(&self.checks),
            "by_gain": by_gain,
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
                "rising": s.rising,
                "free_dawns": s.free,
                "free_rising": s.free_rising,
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
            start: crate::Start::Founders,
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
            problems: true,
            wake_all: false,
            old_wakes: false,
            old_illness: false,
            passages: true,
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
                .all(|s| s.rising <= s.fertile && s.free_rising <= s.free)
        );
        let drawn: u64 = probe.by_gain.iter().map(|g| g.0).sum();
        let by_gain: u64 = probe.by_gain.iter().map(|g| g.1).sum();
        assert_eq!(
            drawn,
            probe.checks[Block::DrawNo.index()] + probe.checks[conceived],
            "every check at the draw has a band"
        );
        assert_eq!(by_gain, probe.checks[conceived]);
        let data: Value = serde_json::from_str(&report.json()).expect("the file is JSON");
        let c = &data["conception"];
        assert_eq!(c["woman_quarters"]["total"], json!(probe.quarters.len()));
        assert_eq!(c["checks"]["conceived"], json!(probe.checks[conceived]));
        assert_eq!(c["gain_days"], json!(content.needs.life.conceive_gain_days));
        assert_eq!(c["by_gain"]["good"]["met"], json!(probe.by_gain[2].0));
    }

    #[test]
    fn the_first_bar_follows_the_rules_order() {
        let near = Spouse::Alive { near: true };
        assert_eq!(
            first_block(true, true, true, Spouse::NeverWed),
            Some(Block::Pregnant)
        );
        assert_eq!(first_block(false, true, true, near), Some(Block::Weak));
        assert_eq!(
            first_block(false, false, true, Spouse::Parted),
            Some(Block::Infant)
        );
        assert_eq!(
            first_block(false, false, false, Spouse::NeverWed),
            Some(Block::NeverWed)
        );
        assert_eq!(
            first_block(false, false, false, Spouse::Widowed),
            Some(Block::Widowed)
        );
        assert_eq!(
            first_block(false, false, false, Spouse::Parted),
            Some(Block::Parted)
        );
        assert_eq!(
            first_block(false, false, false, Spouse::Alive { near: false }),
            Some(Block::SpouseFar)
        );
        assert_eq!(first_block(false, false, false, near), None);
        let mut seen: Vec<usize> = [
            Block::Pregnant,
            Block::Weak,
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
        let q = |checks: &[Block]| Quarter {
            checks: checks.to_vec(),
        };
        assert_eq!(q(&[Block::SpouseFar; 4]).verdict(), Some(Block::SpouseFar));
        assert_eq!(
            q(&[
                Block::SpouseFar,
                Block::Conceived,
                Block::Pregnant,
                Block::Pregnant
            ])
            .verdict(),
            Some(Block::Conceived)
        );
        assert_eq!(q(&[Block::SpouseFar, Block::DrawNo]).verdict(), None);
        assert_eq!(q(&[Block::Weak, Block::Weak]).verdict(), Some(Block::Weak));
        assert_eq!(q(&[]).verdict(), None);
    }

    #[test]
    fn a_gain_falls_in_one_band() {
        assert_eq!(Gain::of(-1, false), Gain::Falling);
        assert_eq!(Gain::of(0, false), Gain::Slow);
        assert_eq!(Gain::of(19, false), Gain::Slow);
        assert_eq!(Gain::of(-30, true), Gain::Good, "a full reserve");
        let bands: Vec<usize> = [Gain::Falling, Gain::Slow, Gain::Good]
            .iter()
            .map(|g| g.index())
            .collect();
        assert_eq!(bands, (0..GAINS.len()).collect::<Vec<_>>());
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
