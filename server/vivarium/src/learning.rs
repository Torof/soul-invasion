//! Learning causes under glass (D59): believe without acting. The marks
//! lived (an ailment, or a kin seen sick), who noticed by will, the
//! beliefs come to by what they accuse and whether that is true (the
//! vivarium knows the illness rule: a measure, never a perception), the
//! tales told and the beliefs taken (by the tradition too), the beliefs
//! forgotten, and the beliefs held season by season. Acting (D59,
//! amended 27 November 2026): who meant to act and the levers that would
//! apply, the calls and the trials, from a belief and by chance, kept or
//! abandoned, the moves by their cause and the nights at the camp left,
//! and season by season the gut ills lived against the living days.
//! Section `learning` of the JSON (`docs/10`), new keys only; `on` false
//! and nothing else without the content's `learning` section.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use sim::brain::{Brains, Forgot, LearnStep, News, TrialEnd};
use sim::content::{CLUE_KINDS, Content, Illness, LEVERS};
use sim::memory::{Clue, Record, RecordId, Suspect};
use sim::{Ailment, World};

/// The bands of will a noticer is counted in: timid under 20, willing
/// from 50, between middle.
const BANDS: [&str; 3] = ["timid", "middle", "willing"];

/// The seasons' names, in the calendar's order.
const SEASONS: [&str; 4] = ["spring", "summer", "autumn", "winter"];

fn band(will: u8) -> usize {
    match will {
        0..20 => 0,
        20..50 => 1,
        _ => 2,
    }
}

/// What a suspect is worth against the illness rule: 2 true, 1 partly
/// (a stand-in of the true cause), 0 false.
fn truth(illness: Option<&Illness>, ailment: Option<Ailment>, clue: Clue) -> u8 {
    let (Some(illness), Some(ailment)) = (illness, ailment) else {
        return 0;
    };
    let gut = ailment == Ailment::Gut;
    match clue {
        // The place holds the fouling, but leaving for good is not the
        // cure: staying long is.
        Clue::Camp(_) | Clue::Drank(_) => u8::from(gut && illness.fouling_reference > 0),
        Clue::CampAge(age) => {
            if gut && illness.fouling_reference > 0 && age > 0 {
                2
            } else {
                0
            }
        }
        Clue::Season(s) => {
            let by = if gut {
                &illness.gut_season_percent
            } else {
                &illness.chest_season_percent
            };
            let percent = [by.spring, by.summer, by.autumn, by.winter];
            if percent.get(usize::from(s)).is_some_and(|&p| p > 100) {
                2
            } else {
                0
            }
        }
        Clue::Crowd(c) => match (gut, c) {
            (false, 2) => 2,
            (false, 1) | (true, 2) => 1,
            _ => 0,
        },
        Clue::ColdNight => {
            if !gut && illness.cold_chest_percent > 100 {
                2
            } else {
                0
            }
        }
        Clue::SickAtFire => {
            if illness.contagion_permille > 0 {
                2
            } else {
                0
            }
        }
        Clue::Ate(_) => 0,
    }
}

/// A belief's verdict: `true` when each suspect is true or partly and one
/// is true, `false` when none is, `partly` otherwise.
fn verdict(worths: &[u8]) -> &'static str {
    if worths.iter().all(|&w| w == 0) {
        "false"
    } else if worths.iter().all(|&w| w > 0) && worths.contains(&2) {
        "true"
    } else {
        "partly"
    }
}

/// What a belief accuses, in words that group like beliefs: the camp and
/// the water without their places, what was eaten by its material.
fn words(content: &Content, problem: &str, suspects: &[Suspect]) -> String {
    let one = |s: &Suspect| match s.clue {
        Clue::Camp(_) => "camp".to_owned(),
        Clue::CampAge(a) => format!(
            "camp_age:{}",
            ["new", "settled", "old"][usize::from(a.min(2))]
        ),
        Clue::Season(s) => format!("season:{}", SEASONS[usize::from(s % 4)]),
        Clue::Crowd(c) => format!("crowd:{}", ["few", "some", "many"][usize::from(c.min(2))]),
        Clue::ColdNight => "cold_night".to_owned(),
        Clue::SickAtFire => "sick_at_fire".to_owned(),
        Clue::Ate(m) => format!(
            "ate:{}",
            content
                .materials
                .get(usize::from(m))
                .map_or("?", |m| m.id.as_str())
        ),
        Clue::Drank(_) => "drank_there".to_owned(),
    };
    let accused: Vec<String> = suspects.iter().map(one).collect();
    format!("{problem}: {}", accused.join(" + "))
}

/// A belief come to, as read at its birth.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Born {
    /// Its words.
    words: String,
    /// Its verdict.
    verdict: &'static str,
}

/// One content's counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ByContent {
    verdict: &'static str,
    /// The kinds of its suspects.
    kinds: Vec<&'static str>,
    born: [u32; 3],
    told: u32,
    copied: u32,
    tradition: u32,
    forgotten: u32,
}

/// A season's sample: the beliefs held.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Sample {
    day: u64,
    living: u32,
    holders: u32,
    own: u32,
    heard: u32,
    tradition: u32,
    children_tradition: u32,
    by_verdict: BTreeMap<&'static str, u32>,
    by_content: BTreeMap<String, u32>,
}

/// A season's acting (D59, amended 27 November 2026).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ActSeason {
    /// Moves home by grown bodies from a belief's call (or following
    /// one), and for other reasons.
    moves: [u32; 2],
    /// Their nights in a row at the camp left, summed.
    nights: [u64; 2],
    /// Gut ills lived (own marks).
    gut: u32,
    /// The living's days.
    person_days: u64,
}

/// The trials' ends, in this order after `begun`.
const ENDS: [&str; 4] = ["begun", "kept", "abandoned", "unjudged"];

/// The probe.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LearningProbe {
    on: bool,
    /// Acting is played.
    acting: bool,
    /// The gut's problem index.
    gut: Option<u8>,
    minded: u32,
    /// The levers that would apply, by index.
    would: BTreeMap<u8, u32>,
    called: u32,
    waited: u32,
    stayed: u32,
    lessons: u32,
    mourned: u32,
    /// The trials, from a belief and by chance: begun, kept, abandoned,
    /// unjudged.
    trials: [[u32; 4]; 2],
    /// By season index.
    act_seasons: BTreeMap<u64, ActSeason>,
    season_len: u64,
    /// The problems' refusals, by index.
    problems: Vec<String>,
    /// The ailment of each problem index, if it marks.
    ailments: BTreeMap<u8, Ailment>,
    illness: Option<Illness>,
    content: Option<Content>,
    child_years: u32,
    /// Marks by problem: own, kin.
    marks: BTreeMap<u8, [u64; 2]>,
    /// By band: noticed, saw nothing, believed.
    noticed: [u32; 3],
    saw_nothing: [u32; 3],
    believed: [u32; 3],
    /// Beliefs born by verdict.
    born_verdicts: BTreeMap<&'static str, u32>,
    renewed_lived: u32,
    renewed_heard: u32,
    countered: u32,
    told: u32,
    listeners: u64,
    copied: u32,
    copied_tradition: u32,
    forgotten: BTreeMap<&'static str, u32>,
    records: BTreeMap<RecordId, Born>,
    contents: BTreeMap<String, ByContent>,
    /// The living by band, summed over the samples.
    living_bands: [u64; 3],
    samples: Vec<Sample>,
    last_day: Option<u64>,
    /// The learning's wall time, read at the end: in the timing only.
    pub seconds: f64,
}

impl LearningProbe {
    /// A probe for a run on `content`.
    #[must_use]
    pub fn new(content: &Content) -> Self {
        let Some(learning) = &content.needs.learning else {
            return Self::default();
        };
        let problems: Vec<String> = content
            .needs
            .problems
            .kinds
            .iter()
            .map(|k| k.refusal.clone())
            .collect();
        let ailments = learning
            .ailments
            .iter()
            .filter_map(|(a, p)| {
                let i = problems.iter().position(|r| r == p)?;
                Some((u8::try_from(i).ok()?, *a))
            })
            .collect();
        let gut = problems
            .iter()
            .position(|r| r == "ill_gut")
            .and_then(|i| u8::try_from(i).ok());
        Self {
            on: true,
            acting: learning.acting.is_some(),
            gut,
            problems,
            ailments,
            illness: content.needs.illness.clone(),
            content: Some(content.clone()),
            child_years: learning.child_years,
            ..Self::default()
        }
    }

    fn problem(&self, kind: u8) -> &str {
        self.problems
            .get(usize::from(kind))
            .map_or("?", String::as_str)
    }

    /// The brain's news of a turn.
    pub fn news(&mut self, brains: &Brains, batch: &[News]) {
        if !self.on {
            return;
        }
        for news in batch {
            let News::Learning { will, step, .. } = news else {
                continue;
            };
            let b = band(*will);
            let season = self.last_day.unwrap_or(0) / self.season_len.max(1);
            match *step {
                LearnStep::Marked { kind, own } => {
                    self.marks.entry(kind).or_default()[usize::from(!own)] += 1;
                    if own && Some(kind) == self.gut {
                        self.act_seasons.entry(season).or_default().gut += 1;
                    }
                }
                LearnStep::Noticed { .. } => self.noticed[b] += 1,
                LearnStep::SawNothing { .. } => self.saw_nothing[b] += 1,
                LearnStep::Believed { record } => {
                    self.believed[b] += 1;
                    let Some(Record::Belief { kind, suspects, .. }) = brains.memory().get(record)
                    else {
                        continue;
                    };
                    let ailment = self.ailments.get(kind).copied();
                    let worths: Vec<u8> = suspects
                        .iter()
                        .map(|s| truth(self.illness.as_ref(), ailment, s.clue))
                        .collect();
                    let verdict = verdict(&worths);
                    let content = self.content.as_ref().expect("on");
                    let words = words(content, self.problem(*kind), suspects);
                    *self.born_verdicts.entry(verdict).or_default() += 1;
                    let c = self.contents.entry(words.clone()).or_default();
                    c.verdict = verdict;
                    c.kinds = suspects
                        .iter()
                        .map(|s| CLUE_KINDS.get(usize::from(s.kind)).copied().unwrap_or("?"))
                        .collect();
                    c.born[b] += 1;
                    self.records.insert(record, Born { words, verdict });
                }
                LearnStep::Renewed { heard, .. } => {
                    if heard {
                        self.renewed_heard += 1;
                    } else {
                        self.renewed_lived += 1;
                    }
                }
                LearnStep::Countered { .. } => self.countered += 1,
                LearnStep::Told {
                    record,
                    listeners,
                    lesson,
                } => {
                    self.told += 1;
                    self.lessons += u32::from(lesson);
                    self.listeners += u64::from(listeners);
                    if let Some(c) = self.content_of(record) {
                        c.told += 1;
                    }
                }
                LearnStep::Copied {
                    record, tradition, ..
                } => {
                    self.copied += 1;
                    self.copied_tradition += u32::from(tradition);
                    if let Some(c) = self.content_of(record) {
                        c.copied += 1;
                        c.tradition += u32::from(tradition);
                    }
                }
                LearnStep::Forgotten { record, why } => {
                    let why = match why {
                        Forgot::Faded => "faded",
                        Forgot::Countered => "countered",
                        Forgot::Replaced => "replaced",
                    };
                    *self.forgotten.entry(why).or_default() += 1;
                    if let Some(c) = self.content_of(record) {
                        c.forgotten += 1;
                    }
                }
                _ => self.act_step(*step, season),
            }
        }
    }

    /// An acting step of `season` (D59, amended 27 November 2026).
    fn act_step(&mut self, step: LearnStep, season: u64) {
        match step {
            LearnStep::Mourned { .. } => self.mourned += 1,
            LearnStep::Minded { .. } => self.minded += 1,
            LearnStep::Would { lever } => *self.would.entry(lever).or_default() += 1,
            LearnStep::Called { .. } => self.called += 1,
            LearnStep::Waited { .. } => self.waited += 1,
            LearnStep::Stayed { .. } => self.stayed += 1,
            LearnStep::Moved { belief, nights } => {
                let s = self.act_seasons.entry(season).or_default();
                let cause = usize::from(!belief);
                s.moves[cause] += 1;
                s.nights[cause] += u64::from(nights);
            }
            LearnStep::TrialBegan { chance, .. } => self.trials[usize::from(chance)][0] += 1,
            LearnStep::TrialEnded { chance, end, .. } => {
                let at = match end {
                    TrialEnd::Kept => 1,
                    TrialEnd::Abandoned => 2,
                    TrialEnd::Unjudged => 3,
                };
                self.trials[usize::from(chance)][at] += 1;
            }
            _ => {}
        }
    }

    fn content_of(&mut self, record: RecordId) -> Option<&mut ByContent> {
        let words = &self.records.get(&record)?.words;
        self.contents.get_mut(words)
    }

    /// A tick: at each season's first day, the beliefs held.
    pub fn tick(&mut self, world: &World, brains: &Brains) {
        if !self.on {
            return;
        }
        let day = world.time().day;
        if self.last_day == Some(day) {
            return;
        }
        self.last_day = Some(day);
        let season = u64::from(world.calendar().days_per_cycle() / 4).max(1);
        self.season_len = season;
        let living = world
            .bodies()
            .iter()
            .filter(|b| b.condition().vitality.points() > 0)
            .count();
        self.act_seasons
            .entry(day / season)
            .or_default()
            .person_days += u64::try_from(living).unwrap_or(u64::MAX);
        if !day.is_multiple_of(season) {
            return;
        }
        let mut s = Sample {
            day,
            ..Sample::default()
        };
        for b in world.bodies() {
            if b.condition().vitality.points() == 0 {
                continue;
            }
            s.living += 1;
            if let Some(m) = brains.mind(b.id()) {
                self.living_bands[band(m.will())] += 1;
            }
        }
        let mut holders = std::collections::BTreeSet::new();
        for held in brains.beliefs() {
            holders.insert(held.body);
            if held.own {
                s.own += 1;
            } else {
                s.heard += 1;
            }
            if held.tradition {
                s.tradition += 1;
                let child = world
                    .body(held.body)
                    .is_some_and(|b| world.age_milli(b) < u64::from(self.child_years) * 1000);
                s.children_tradition += u32::from(child);
            }
            if let Some(born) = self.records.get(&held.record) {
                *s.by_verdict.entry(born.verdict).or_default() += 1;
                *s.by_content.entry(born.words.clone()).or_default() += 1;
            }
        }
        s.holders = u32::try_from(holders.len()).unwrap_or(u32::MAX);
        self.samples.push(s);
    }

    /// The section.
    #[must_use]
    pub fn json(&self) -> Value {
        if !self.on {
            return json!({ "on": false });
        }
        let bands = |counts: &[u32; 3]| -> Value {
            BANDS
                .iter()
                .zip(counts)
                .map(|(b, n)| ((*b).to_owned(), json!(n)))
                .collect::<serde_json::Map<_, _>>()
                .into()
        };
        let marks: serde_json::Map<String, Value> = self
            .marks
            .iter()
            .map(|(k, [own, kin])| (self.problem(*k).to_owned(), json!({"own": own, "kin": kin})))
            .collect();
        let mut contents: Vec<(&String, &ByContent)> = self.contents.iter().collect();
        contents.sort_by_key(|(w, c)| (std::cmp::Reverse(c.born.iter().sum::<u32>()), *w));
        let contents: Vec<Value> = contents
            .into_iter()
            .map(|(w, c)| {
                json!({
                    "belief": w,
                    "verdict": c.verdict,
                    "kinds": c.kinds,
                    "born": bands(&c.born),
                    "told": c.told,
                    "copied": c.copied,
                    "copied_tradition": c.tradition,
                    "forgotten": c.forgotten,
                })
            })
            .collect();
        let samples: Vec<Value> = self
            .samples
            .iter()
            .map(|s| {
                let mut top: Vec<(&String, &u32)> = s.by_content.iter().collect();
                top.sort_by_key(|(w, n)| (std::cmp::Reverse(**n), *w));
                json!({
                    "day": s.day,
                    "living": s.living,
                    "holders": s.holders,
                    "own": s.own,
                    "heard": s.heard,
                    "tradition": s.tradition,
                    "children_tradition": s.children_tradition,
                    "by_verdict": s.by_verdict,
                    "by_content": top.into_iter().take(12).map(|(w, n)| json!([w, n])).collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({
            "on": true,
            "marks": marks,
            "noticed": bands(&self.noticed),
            "saw_nothing": bands(&self.saw_nothing),
            "believed": bands(&self.believed),
            "living_by_band": BANDS.iter().zip(self.living_bands).map(|(b, n)| ((*b).to_owned(), json!(n))).collect::<serde_json::Map<_, _>>(),
            "born_by_verdict": self.born_verdicts,
            "renewed": {"lived": self.renewed_lived, "heard": self.renewed_heard},
            "countered": self.countered,
            "told": self.told,
            "listeners": self.listeners,
            "copied": self.copied,
            "copied_tradition": self.copied_tradition,
            "forgotten": self.forgotten,
            "beliefs": contents,
            "seasons": samples,
            "acting": self.acting_json(),
        })
    }

    /// The acting's part of the section (D59, amended 27 November 2026).
    fn acting_json(&self) -> Value {
        let ends = |t: &[u32; 4]| -> Value {
            ENDS.iter()
                .zip(t)
                .map(|(e, n)| ((*e).to_owned(), json!(n)))
                .collect::<serde_json::Map<_, _>>()
                .into()
        };
        let would: serde_json::Map<String, Value> = self
            .would
            .iter()
            .map(|(l, n)| {
                let name = LEVERS.get(usize::from(*l)).copied().unwrap_or("?");
                (name.to_owned(), json!(n))
            })
            .collect();
        let (mut belief, mut other) = (0, 0);
        let seasons: Vec<Value> = self
            .act_seasons
            .iter()
            .map(|(i, s)| {
                belief += s.moves[0];
                other += s.moves[1];
                json!({
                    "day": i * self.season_len.max(1),
                    "moves_belief": s.moves[0],
                    "moves_other": s.moves[1],
                    "nights_at_leaving_belief": s.nights[0],
                    "nights_at_leaving_other": s.nights[1],
                    "gut_ills": s.gut,
                    "person_days": s.person_days,
                    "gut_per_10k_person_days": u64::from(s.gut) * 10_000 / s.person_days.max(1),
                })
            })
            .collect();
        json!({
            "on": self.acting,
            "minded": self.minded,
            "would": would,
            "called": self.called,
            "waited": self.waited,
            "stayed": self.stayed,
            "lessons_told": self.lessons,
            "mourned": self.mourned,
            "trials": {"belief": ends(&self.trials[0]), "chance": ends(&self.trials[1])},
            "moves": {"belief": belief, "other": other},
            "seasons": seasons,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_suspect_is_judged_against_the_illness_rule_and_a_belief_by_its_suspects() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = crate::load_content(&dir).unwrap();
        let illness = content.needs.illness.as_ref();
        let (gut, chest) = (Some(Ailment::Gut), Some(Ailment::Chest));
        assert_eq!(truth(illness, gut, Clue::CampAge(2)), 2, "the fouling");
        assert_eq!(truth(illness, gut, Clue::CampAge(0)), 0);
        assert_eq!(truth(illness, chest, Clue::CampAge(2)), 0);
        assert_eq!(truth(illness, gut, Clue::Camp((0, 0))), 1);
        assert_eq!(
            truth(illness, gut, Clue::Ate(0)),
            0,
            "food makes nobody ill"
        );
        assert_eq!(truth(illness, chest, Clue::ColdNight), 2);
        assert_eq!(
            truth(illness, chest, Clue::Season(3)),
            2,
            "the chest's winter"
        );
        assert_eq!(
            truth(None, gut, Clue::SickAtFire),
            0,
            "no illness, no cause"
        );
        assert_eq!(verdict(&[2, 1]), "true");
        assert_eq!(verdict(&[1, 1]), "partly");
        assert_eq!(verdict(&[2, 0]), "partly");
        assert_eq!(verdict(&[0, 0]), "false");
    }
}
