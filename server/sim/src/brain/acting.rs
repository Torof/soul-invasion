//! Learning causes, its second step: act (D59, amended 27 November 2026;
//! the project lead's answers of that day).
//!
//! **A sign and a strength.** Every belief carries a sign: a fear pushes
//! away from what it accuses, a reward pulls toward it. Illness gives
//! fears; nothing gives a reward yet, its sign is ready. Its **strength**
//! comes from what was lived, its gravest episode: a kin's death of it
//! (`strength_kin_death`) above one's own illness (`strength_own`) above a
//! sick body seen at the fire (`strength_seen`). A belief heard carries
//! `heard_percent` of its teller's. It stays whole for the belief's whole
//! seasons from its last renewal, then counts half.
//!
//! **The lever** is given by the kind of its suspects (`levers`): a place
//! means leaving. Only leaving is built; the others (prepare, keep apart,
//! avoid, drink elsewhere, do otherwise) are recorded where they would
//! apply ([`super::LearnStep::Would`]).
//!
//! **Leaving.** A mark that agrees with a belief of a place makes its
//! holder mean to leave by its will (the chance a distressed body tries,
//! [`super::handwork`]); at its next free nightfall it calls a move as
//! any caller does, to the best place its own notes know that its belief
//! does not accuse, no worse for food than `worse_permille` of a day of
//! need. **The weight**: every holder of a belief of a place, its own,
//! heard or of the tradition, weighs strength × `fear_tenths` / 10 per
//! mille of a day of need on any move weighed at the fire: for leaving a
//! place it fears as it is now, against going to a place it fears (a
//! reward the other way, `reward_tenths`). Everyone else decides as
//! before, and family still pulls. A place feared and told down the
//! generations keeps its weight on who never lived it: nothing blocks a
//! taboo.
//!
//! **The judge** is the problem as the body perceives it: its own ills,
//! its kin's, the sick seen at its fire, each episode once, never the
//! sim's count. A move away begins a trial of a season, from its belief's
//! call or by chance (a move for another reason while carrying the
//! problem). Judged at its end: with `judge_nights` nights away at least
//! and ills before, kept when those perceived are at most `keep_percent`
//! of those of the season before (its belief renewed and told as a
//! lesson; by chance, a belief of the place left), else abandoned (its
//! belief countered).
//!
//! Played only when the content's `learning` section holds `acting`;
//! absent, nothing here weighs or moves anything.

use std::collections::BTreeMap;

use super::learning::{
    Ctx, GRAVE_DEATH, GRAVE_OWN, GRAVE_SEEN, Held, carries_fresh, hold_own, perceive,
};
use super::{
    Brains, Calling, HeardDays, LearnStep, Mind, News, Trial, TrialEnd, Way, grove_side, move_cost,
    place_centre, place_of, place_side, ring, season_grades, season_view, today,
};
use crate::content::{Acting, Content, LEVERS, Learning};
use crate::r#gen::scatter::{self, purpose};
use crate::memory::{Clue, PlaceKey, Record, RecordId, Sign, Store, Suspect};
use crate::{BodyId, Pos, World};

/// What a body's acting keeps (D59, amended 27 November 2026).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Doing {
    /// The problem whose belief it means to act on at its next free
    /// nightfall.
    minded: Option<u8>,
    /// Its call from a belief: the belief, its problem, the place left
    /// and the home called.
    call: Option<(RecordId, u8, PlaceKey, Pos)>,
    /// The place of its home at the last dawn, and its camp then (the
    /// place and its nights in a row).
    home: Option<(PlaceKey, Option<(PlaceKey, u32)>)>,
    /// Its trial against a problem under way.
    trial: Option<Trying>,
    /// The ill episodes it perceived (its own, its kin's, the sick at its
    /// fire): the problem, the day, the episode; each once.
    perceived: Vec<(u8, u32, RecordId)>,
}

/// A trial against a problem (D59, amended 27 November 2026).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Trying {
    /// The problem.
    kind: u8,
    /// Its first day, and the day it is judged.
    began: u32,
    until: u32,
    /// The place left, and its camp age then.
    from: PlaceKey,
    from_age: u8,
    /// The ills perceived the season before.
    before: u32,
    /// The belief it tries; `None` by chance.
    record: Option<RecordId>,
    /// The dawns it woke away from the place left.
    away: u32,
}

/// The section, when acting is played.
fn acting(rules: &Content) -> Option<(&Learning, &Acting)> {
    let learning = rules.needs.learning.as_ref()?;
    Some((learning, learning.acting.as_ref()?))
}

/// A belief's strength from an episode of `grave` gravity, in points; 0
/// without acting.
pub(super) fn strength_of(learning: &Learning, grave: u8) -> u8 {
    let Some(a) = &learning.acting else {
        return 0;
    };
    let points = match grave {
        GRAVE_DEATH => a.strength_kin_death,
        GRAVE_OWN => a.strength_own,
        GRAVE_SEEN => a.strength_seen,
        _ => 0,
    };
    u8::try_from(points.min(100)).unwrap_or(100)
}

/// The strength a belief told with `strength` carries to its listener.
pub(super) fn heard_strength(learning: &Learning, strength: u8) -> u8 {
    let Some(a) = &learning.acting else {
        return 0;
    };
    u8::try_from(u32::from(strength) * a.heard_percent.min(100) / 100).unwrap_or(100)
}

/// `mind` perceived the ill episode `record` of `kind` on `day`: kept
/// once, for its judge (the last two seasons).
pub(super) fn perceived(
    learning: &Learning,
    mind: &mut Mind,
    (kind, day, record): (u8, u64, RecordId),
) {
    if learning.acting.is_none() {
        return;
    }
    let log = &mut mind.learner.act.perceived;
    if log.iter().any(|&(_, _, r)| r == record) {
        return;
    }
    log.push((kind, u32::try_from(day).unwrap_or(u32::MAX), record));
}

/// The ills of `kind` `doing` perceived in `[from, to)`.
fn ills(doing: &Doing, kind: u8, (from, to): (u32, u32)) -> u32 {
    let n = doing
        .perceived
        .iter()
        .filter(|&&(k, d, _)| k == kind && d >= from && d < to)
        .count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The levers of a belief's suspects, each once, by index in [`LEVERS`].
fn levers_of(acting: &Acting, suspects: &[Suspect]) -> Vec<u8> {
    let mut levers: Vec<u8> = suspects
        .iter()
        .filter_map(|s| acting.lever_of(s.kind))
        .collect();
    levers.sort_unstable();
    levers.dedup();
    levers
}

/// The index of the lever "leave".
fn leave() -> u8 {
    LEVERS
        .iter()
        .position(|&l| l == "leave")
        .and_then(|i| u8::try_from(i).ok())
        .unwrap_or(u8::MAX)
}

/// Whether a belief accusing `suspects` is one of a place.
fn of_place(acting: &Acting, suspects: &[Suspect]) -> bool {
    levers_of(acting, suspects).contains(&leave())
}

/// The camps a belief accusing `suspects` names.
fn camps_of(suspects: &[Suspect]) -> impl Iterator<Item = PlaceKey> + '_ {
    suspects.iter().filter_map(|s| match s.clue {
        Clue::Camp(key) => Some(key),
        _ => None,
    })
}

/// `id` lived a mark of `kind` whose episode perceived `clues` (D59,
/// amended 27 November 2026): when the belief it holds of the problem
/// accuses only what was there, its lever. Of a place, its will may make
/// it mean to leave; the others are recorded where they would apply.
pub(super) fn after_mark(
    ctx: &Ctx,
    (id, mind): (BodyId, &mut Mind),
    (kind, clues): (u8, &[Clue]),
    memory: &Store,
    news: &mut Vec<News>,
) {
    let Some(acting) = &ctx.learning.acting else {
        return;
    };
    let Some(held) = mind.learner.beliefs.iter().find(|h| h.kind == kind) else {
        return;
    };
    let Some(Record::Belief { suspects, .. }) = memory.get(held.access.record) else {
        return;
    };
    if !suspects.iter().all(|s| clues.contains(&s.clue)) {
        return;
    }
    let will = mind.will;
    let step = |step: LearnStep| News::Learning {
        body: id,
        will,
        step,
        at: ctx.now,
    };
    for lever in levers_of(acting, suspects) {
        if lever != leave() {
            news.push(step(LearnStep::Would { lever }));
            continue;
        }
        let trials = &ctx.rules.needs.trials;
        let chance = (trials.try_percent(will) * 10).max(u64::from(trials.try_floor_permille));
        let roll = scatter::roll(
            ctx.seed,
            purpose::LEARN_ACT,
            &[u64::from(id.0), ctx.day, u64::from(kind)],
        ) % 1000;
        if roll < chance {
            mind.learner.act.minded = Some(kind);
            news.push(step(LearnStep::Minded { kind }));
        }
    }
}

/// What `mind`'s beliefs weigh on its going to `to` (D59, amended 27
/// November 2026), in per mille of a day of need: each belief of a place
/// its strength times its sign's factor, for leaving here when here is as
/// it accuses, against going where it names. 0 without acting.
pub(super) fn move_weight(
    world: &World,
    rules: &Content,
    memory: &Store,
    mind: &Mind,
    to: Pos,
) -> i64 {
    let Some((learning, acting)) = acting(rules) else {
        return 0;
    };
    if mind.learner.beliefs.is_empty() {
        return 0;
    }
    let day = today(world);
    let day32 = u32::try_from(day).unwrap_or(u32::MAX);
    let cycle = world.calendar().days_per_cycle();
    let side = place_side(rules);
    let there = place_of(to, side);
    let mut now: Option<Vec<Clue>> = None;
    let mut weight = 0_i64;
    for held in &mind.learner.beliefs {
        let Some(Record::Belief { sign, suspects, .. }) = memory.get(held.access.record) else {
            continue;
        };
        if !of_place(acting, suspects) {
            continue;
        }
        let tenths = match sign {
            Sign::Fear => acting.fear_tenths,
            Sign::Reward => acting.reward_tenths,
        };
        let w = i64::from(held.strength_on(learning, day32, cycle)) * i64::from(tenths) / 10;
        let clues = now.get_or_insert_with(|| perceive(world, rules, learning, mind, day));
        let here = i64::from(suspects.iter().all(|s| clues.contains(&s.clue)));
        let named = i64::from(camps_of(suspects).any(|k| k == there));
        let push = (here - named) * w;
        weight += match sign {
            Sign::Fear => push,
            Sign::Reward => -push,
        };
    }
    weight
}

/// `id`, free at nightfall on `day`, acts on the belief it means to act
/// on (D59, amended 27 November 2026): it calls a move to the best place
/// its own notes know that its belief does not accuse, no worse for food
/// than `worse_permille` of a day of need, as a caller does (or goes,
/// without a fire); it waits when it knows none. Its news, empty when it
/// meant nothing.
pub(super) fn belief_call(
    world: &World,
    rules: &Content,
    memory: &Store,
    (id, day, now): (BodyId, u64, u64),
    mind: &mut Mind,
) -> Vec<News> {
    let Some((_, acting)) = acting(rules) else {
        return Vec::new();
    };
    let Some(kind) = mind.learner.act.minded.take() else {
        return Vec::new();
    };
    let will = mind.will;
    let step = |step: LearnStep| News::Learning {
        body: id,
        will,
        step,
        at: now,
    };
    let Some(record) = mind
        .learner
        .beliefs
        .iter()
        .find(|h| h.kind == kind)
        .map(|h| h.access.record)
    else {
        return Vec::new();
    };
    let feared: Vec<PlaceKey> = match memory.get(record) {
        Some(Record::Belief { suspects, .. }) => camps_of(suspects).collect(),
        _ => Vec::new(),
    };
    let Some((to, gain)) = elsewhere(world, rules, mind, (&feared, acting.worse_permille)) else {
        return vec![step(LearnStep::Waited { kind })];
    };
    let side = place_side(rules);
    mind.learner.act.call = Some((record, kind, place_of(mind.ways.home, side), to));
    let per_year = world.calendar().days_per_cycle();
    let trial = Trial {
        way: Way::Home,
        before: mind.ways,
        started_day: day,
        before_days: mind.lately(rules.needs.trials.days_for(Way::Home.key(), per_year)),
        heard: HeardDays::default(),
        caller: None,
        problem: None,
    };
    let mut news = vec![step(LearnStep::Called { record })];
    if rules.needs.fire.on() && rules.needs.decisions.on() {
        mind.calling = Some(Calling { to, gain, trial });
    } else {
        mind.ways.home = to;
        mind.trial = Some(trial);
        news.push(News::Trying {
            body: id,
            way: Way::Home,
            before: trial.before,
            after: mind.ways,
            heard: false,
            at: now,
        });
    }
    news
}

/// The best place `mind`'s own notes know away from its home and from
/// the places `feared`, no worse for food by them than `worse` per mille
/// of a day of need, net of the move's cost: a passable tile there and
/// the gain told. Only a place or grove it lived a day in.
fn elsewhere(
    world: &World,
    rules: &Content,
    mind: &Mind,
    (feared, worse): (&[PlaceKey], u32),
) -> Option<(Pos, i64)> {
    let sides = (place_side(rules), grove_side(rules));
    let home = place_of(mind.ways.home, sides.0);
    let walk = mind.walk(rules);
    let floor = -i64::from(worse);
    let merged = season_view(&mind.lived, None);
    let (_, gain, centre) = merged
        .iter()
        .filter(|l| l.lived() >= 1000)
        .filter_map(|l| {
            let side = if l.grove { sides.1 } else { sides.0 };
            let there = place_centre(l.place, side);
            let key = place_of(there, sides.0);
            if key == home || feared.contains(&key) {
                return None;
            }
            let (old, new) = season_grades(&mind.lived, (l.place, l.grove), home, sides, walk)?;
            let gain = new - old;
            (gain >= floor).then(|| {
                let net = gain - move_cost(world, rules, mind, (mind.ways.home, there));
                (
                    net,
                    gain,
                    there,
                    !l.grove,
                    l.lived(),
                    std::cmp::Reverse(l.place),
                )
            })
        })
        .max_by_key(|&(net, _, _, place, lived, rev)| (net, place, lived, rev))
        .map(|(net, gain, there, ..)| (net, gain, there))?;
    let map = world.map();
    (0..=sides.0 / 2)
        .flat_map(|r| ring(centre, r))
        .find(|&at| map.is_passable(at))
        .map(|at| (at, gain))
}

impl Brains {
    /// A kin of `dead` died of `kind` (D59, amended 27 November 2026):
    /// who holds its episode holds it at its gravest, its belief of the
    /// problem strengthened when the episode agrees with it.
    pub(super) fn mourn(&mut self, world: &World, learning: &Learning, dead: BodyId, kind: u8) {
        if learning.acting.is_none() {
            return;
        }
        let day = u32::try_from(today(world)).unwrap_or(u32::MAX);
        let cycle = world.calendar().days_per_cycle();
        let now = world.tick().0;
        let lived = strength_of(learning, GRAVE_DEATH);
        let dead_body = world.body(dead);
        for (&id, mind) in &mut self.minds {
            if id == dead {
                continue;
            }
            let kin = match dead_body {
                Some(b) => super::learning::kin_times(world, learning, b, id) > 0,
                None => world.body(id).is_some_and(|me| {
                    me.spouse() == Some(dead)
                        || me.mother() == Some(dead)
                        || me.father() == Some(dead)
                }),
            };
            if !kin {
                continue;
            }
            let Some(queue) = mind.learner.queues.iter_mut().find(|q| q.kind == kind) else {
                continue;
            };
            let Some(episode) = queue.episodes.iter_mut().rev().find(|(a, _)| a.by == dead) else {
                continue;
            };
            episode.1 = GRAVE_DEATH;
            let record = episode.0.record;
            let clues: &[Clue] = match self.memory.get(record) {
                Some(Record::Episode { clues, .. }) => clues,
                _ => &[],
            };
            if let Some(held) = mind.learner.beliefs.iter_mut().find(|h| h.kind == kind)
                && matches!(self.memory.get(held.access.record),
                    Some(Record::Belief { suspects, .. })
                        if suspects.iter().all(|s| clues.contains(&s.clue)))
            {
                held.strength = held.strength_on(learning, day, cycle).max(lived);
            }
            self.news.push(News::Learning {
                body: id,
                will: mind.will,
                step: LearnStep::Mourned { kind },
                at: now,
            });
        }
    }

    /// The dawn's acting (D59, amended 27 November 2026), after the
    /// fires' learning: who moved home since the last dawn, from its
    /// belief's call, following it, or for another reason; a trial begun
    /// on a move away; the nights away counted; a trial judged at its
    /// end; a call dropped; the oldest ills let go.
    pub(super) fn act_at_dawn(
        &mut self,
        world: &World,
        rules: &Content,
        learning: &Learning,
        circles: &[(Pos, Vec<BodyId>)],
    ) {
        // Without acting, the moves alone are told (a measure).
        let acting = learning.acting.as_ref();
        let dawn = today(world);
        let dawn32 = u32::try_from(dawn).unwrap_or(u32::MAX);
        let cycle = world.calendar().days_per_cycle();
        let season = Acting::trial_days(cycle);
        let side = place_side(rules);
        let now = world.tick().0;
        let woke: BTreeMap<BodyId, PlaceKey> = circles
            .iter()
            .flat_map(|(camp, present)| present.iter().map(move |&id| (id, place_of(*camp, side))))
            .collect();
        // The callers from a belief whose home moved to the place called.
        let callers: Vec<BodyId> = self
            .minds
            .iter()
            .filter(|(_, m)| {
                m.learner
                    .act
                    .call
                    .is_some_and(|(.., to)| place_of(m.ways.home, side) == place_of(to, side))
            })
            .map(|(&id, _)| id)
            .collect();
        for (&id, mind) in &mut self.minds {
            let Some(body) = world.body(id) else {
                continue;
            };
            let grown = world.age_milli(body) >= 16_000;
            let home = place_of(mind.ways.home, side);
            let will = mind.will;
            let mut said = Vec::new();
            let act = &mut mind.learner.act;
            let keep = 2 * season;
            act.perceived
                .retain(|&(_, d, _)| dawn32.saturating_sub(d) < keep);
            if let Some(t) = &mut act.trial
                && woke.get(&id).is_some_and(|&k| k != t.from)
            {
                t.away += 1;
            }
            let before = act.home.replace((home, mind.learner.camp));
            let call = act.call;
            match before {
                Some((was, camp)) if grown && was != home => {
                    let nights = camp.filter(|&(k, _)| k == was).map_or(0, |(_, n)| n);
                    let own = call.is_some_and(|(.., to)| place_of(to, side) == home);
                    let followed = mind
                        .trial
                        .is_some_and(|t| t.caller.is_some_and(|c| callers.contains(&c)));
                    said.push(LearnStep::Moved {
                        belief: own || followed,
                        nights,
                    });
                    if own {
                        mind.learner.act.call = None;
                    }
                    if acting.is_some() {
                        said.extend(begin(
                            rules,
                            learning,
                            (dawn, dawn32, season, cycle),
                            mind,
                            (was, call.filter(|_| own)),
                        ));
                    }
                }
                _ => {
                    let dropped =
                        call.is_some_and(|(.., to)| mind.calling.is_none_or(|c| c.to != to));
                    if let Some((record, ..)) = call.filter(|_| dropped) {
                        mind.learner.act.call = None;
                        said.push(LearnStep::Stayed { record });
                    }
                }
            }
            if let Some(acting) = acting
                && mind.learner.act.trial.is_some_and(|t| dawn32 >= t.until)
            {
                said.extend(judge(
                    learning,
                    acting,
                    (id, dawn32, cycle),
                    mind,
                    &mut self.memory,
                ));
            }
            self.news
                .extend(said.into_iter().map(|step| News::Learning {
                    body: id,
                    will,
                    step,
                    at: now,
                }));
        }
    }
}

/// `mind` moved home away from `was` (D59, amended 27 November 2026): a
/// trial of a season begins, of its belief's problem when its call made
/// it, else by chance of the first problem it carries; the one under way
/// ends unjudged. Its steps.
fn begin(
    rules: &Content,
    learning: &Learning,
    (dawn, dawn32, season, cycle): (u64, u32, u32, u32),
    mind: &mut Mind,
    (was, call): (PlaceKey, Option<(RecordId, u8, PlaceKey, Pos)>),
) -> Vec<LearnStep> {
    let mut steps = Vec::new();
    let chance_kind = || {
        mind.learner
            .queues
            .iter()
            .map(|q| q.kind)
            .find(|&k| carries_fresh(rules, cycle, mind, k, dawn))
    };
    let (kind, record, from) = match call {
        Some((record, kind, from, _)) => (kind, Some(record), from),
        None => match chance_kind() {
            Some(kind) => (kind, None, was),
            None => return steps,
        },
    };
    if let Some(old) = mind.learner.act.trial.take() {
        steps.push(LearnStep::TrialEnded {
            kind: old.kind,
            chance: old.record.is_none(),
            end: TrialEnd::Unjudged,
        });
    }
    let from_age = mind.learner.camp_age(learning, from, cycle);
    let act = &mut mind.learner.act;
    act.trial = Some(Trying {
        kind,
        began: dawn32,
        until: dawn32.saturating_add(season),
        from,
        from_age,
        before: ills(act, kind, (dawn32.saturating_sub(season), dawn32)),
        record,
        away: 0,
    });
    steps.push(LearnStep::TrialBegan {
        kind,
        chance: record.is_none(),
    });
    steps
}

/// `id`'s trial ends on `dawn32` (D59, amended 27 November 2026), judged
/// by the ills it perceived. Its steps.
fn judge(
    learning: &Learning,
    acting: &Acting,
    (id, dawn32, cycle): (BodyId, u32, u32),
    mind: &mut Mind,
    memory: &mut Store,
) -> Vec<LearnStep> {
    let Some(t) = mind.learner.act.trial.take() else {
        return Vec::new();
    };
    let chance = t.record.is_none();
    let during = ills(&mind.learner.act, t.kind, (t.began, t.until));
    let end = if t.away < acting.judge_nights || t.before == 0 {
        TrialEnd::Unjudged
    } else if u64::from(during) * 100 <= u64::from(t.before) * u64::from(acting.keep_percent) {
        TrialEnd::Kept
    } else {
        TrialEnd::Abandoned
    };
    let mut steps = vec![LearnStep::TrialEnded {
        kind: t.kind,
        chance,
        end,
    }];
    let at = mind.learner.beliefs.iter().position(|h| h.kind == t.kind);
    match (end, at) {
        (TrialEnd::Kept, Some(at)) => {
            let place = matches!(memory.get(mind.learner.beliefs[at].access.record),
                Some(Record::Belief { suspects, .. }) if of_place(acting, suspects));
            if t.record.is_some() || place {
                renew_as_lesson(learning, &mut mind.learner.beliefs[at], dawn32, cycle);
            }
        }
        (TrialEnd::Kept, None) => {
            // By chance, a belief of the place left: what it found there
            // and no more.
            let suspect = |clue: Clue| Suspect {
                clue,
                kind: learning.kind_of(clue.name()).unwrap_or(u8::MAX),
            };
            let mut suspects = vec![
                suspect(Clue::Camp(t.from)),
                suspect(Clue::CampAge(t.from_age)),
            ];
            suspects.sort_unstable();
            let gravest = mind
                .learner
                .queue(t.kind)
                .and_then(|q| q.episodes.iter().map(|&(_, g)| g).max())
                .unwrap_or(GRAVE_SEEN);
            let strength = strength_of(learning, gravest);
            let record = hold_own(mind, (id, dawn32, t.kind), (suspects, strength), memory);
            mind.learner.beliefs.last_mut().expect("just held").lesson = true;
            steps.push(LearnStep::Believed { record });
        }
        (TrialEnd::Abandoned, Some(at)) if t.record.is_some() => {
            let held = mind.learner.beliefs[at];
            if held.against + 1 >= learning.counter_examples {
                mind.learner.beliefs.remove(at);
                memory.release(held.access.record);
                steps.push(LearnStep::Forgotten {
                    record: held.access.record,
                    why: super::Forgot::Countered,
                });
            } else {
                mind.learner.beliefs[at].against += 1;
                steps.push(LearnStep::Countered {
                    record: held.access.record,
                });
            }
        }
        _ => {}
    }
    steps
}

/// `held` renewed on `day` by a trial kept: whole again, told as a lesson
/// at the next fire.
fn renew_as_lesson(learning: &Learning, held: &mut Held, day: u32, cycle: u32) {
    held.strength = held.strength_on(learning, day, cycle);
    (held.renewed, held.against, held.fresh, held.lesson) = (day, 0, true, true);
}

#[cfg(test)]
mod tests {
    use super::super::learning::tests::{gut, ill, learning_mut, on_day, steps, valley};
    use super::super::learning::{GRAVE_SEEN, Held};
    use super::super::{Lived, place_centre};
    use super::*;
    use crate::content::Sex;
    use crate::memory::Access;

    /// `id` holds its own belief of `kind`, of `sign`, accusing `clues`,
    /// of `strength`, renewed on day 0.
    fn believe(
        brains: &mut Brains,
        id: BodyId,
        (kind, sign): (u8, Sign),
        clues: &[Clue],
        strength: u8,
    ) -> RecordId {
        let suspects = clues
            .iter()
            .map(|&clue| Suspect {
                clue,
                kind: u8::from(!matches!(clue, Clue::Camp(_) | Clue::CampAge(_))),
            })
            .collect();
        let (record, _) = brains.memory.intern(Record::Belief {
            body: id,
            day: 0,
            kind,
            sign,
            suspects,
        });
        brains
            .minds
            .get_mut(&id)
            .unwrap()
            .learner
            .beliefs
            .push(Held {
                kind,
                access: Access {
                    record,
                    by: id,
                    day: 0,
                },
                renewed: 0,
                against: 0,
                credit: 100,
                fresh: false,
                tradition: false,
                strength,
                lesson: false,
            });
        record
    }

    fn here(rules: &Content, brains: &Brains, id: BodyId) -> PlaceKey {
        place_of(brains.minds[&id].home_at_dusk, place_side(rules))
    }

    fn weight(world: &World, rules: &Content, brains: &Brains, id: BodyId, to: PlaceKey) -> i64 {
        let to = place_centre(to, place_side(rules));
        move_weight(world, rules, &brains.memory, &brains.minds[&id], to)
    }

    #[test]
    fn strength_comes_from_what_was_lived_and_a_belief_heard_carries_half() {
        let (mut rules, mut world, mut brains, ids) = valley(2);
        let (teller, carrier) = (ids[0], ids[1]);
        let l = learning_mut(&mut rules);
        l.notice_percent_by_will = vec![(0, 100)];
        l.episodes_kept = 3;
        l.copy_credit = 1;
        for day in 0..3 {
            ill(&mut world, &mut brains, &rules, teller, day);
        }
        ill(&mut world, &mut brains, &rules, carrier, 2);
        let own = brains.minds[&teller].learner.beliefs[0];
        assert_eq!(own.strength, 60, "its own illness");
        on_day(&mut world, 3);
        let learning = rules.needs.learning.clone().unwrap();
        brains.tell_beliefs(&world, &rules, &learning, &ids);
        let heard = brains.minds[&carrier]
            .learner
            .beliefs
            .iter()
            .find(|h| h.access.record == own.access.record)
            .copied()
            .unwrap();
        assert_eq!(heard.strength, 30, "half of the teller's");
        // Whole for its whole seasons, half after.
        let (whole, _) = learning.belief_days(world.calendar().days_per_cycle());
        assert_eq!(own.strength_on(&learning, own.renewed + whole, 1), 30);
        // Without acting, no strength.
        let (mut rules, mut world, mut brains, ids) = valley(1);
        let l = learning_mut(&mut rules);
        l.notice_percent_by_will = vec![(0, 100)];
        l.episodes_kept = 3;
        l.acting = None;
        for day in 0..3 {
            ill(&mut world, &mut brains, &rules, ids[0], day);
        }
        assert_eq!(brains.minds[&ids[0]].learner.beliefs[0].strength, 0);
    }

    #[test]
    fn a_fear_of_here_pushes_to_leave_and_a_fear_of_there_holds_back() {
        let (mut rules, world, mut brains, ids) = valley(1);
        let id = ids[0];
        let kind = gut(&rules);
        let home = here(&rules, &brains, id);
        let (other, third) = ((home.0 + 1, home.1), (home.0, home.1 + 1));
        believe(&mut brains, id, (kind, Sign::Fear), &[Clue::Camp(home)], 60);
        assert_eq!(weight(&world, &rules, &brains, id, other), 300, "60 × 5");
        assert_eq!(weight(&world, &rules, &brains, id, home), 0, "here to here");
        brains.minds.get_mut(&id).unwrap().learner.beliefs.clear();
        believe(
            &mut brains,
            id,
            (kind, Sign::Fear),
            &[Clue::Camp(other)],
            60,
        );
        assert_eq!(weight(&world, &rules, &brains, id, other), -300);
        assert_eq!(weight(&world, &rules, &brains, id, third), 0);
        brains.minds.get_mut(&id).unwrap().learner.beliefs.clear();
        believe(
            &mut brains,
            id,
            (kind, Sign::Reward),
            &[Clue::Camp(other)],
            60,
        );
        assert_eq!(
            weight(&world, &rules, &brains, id, other),
            150,
            "half a fear"
        );
        // A belief of the season only is no belief of a place.
        brains.minds.get_mut(&id).unwrap().learner.beliefs.clear();
        let season = super::super::season_of_day(&world, today(&world));
        believe(
            &mut brains,
            id,
            (kind, Sign::Fear),
            &[Clue::Season(season)],
            60,
        );
        assert_eq!(weight(&world, &rules, &brains, id, other), 0);
        // Believe only: nothing weighs.
        brains.minds.get_mut(&id).unwrap().learner.beliefs.clear();
        believe(&mut brains, id, (kind, Sign::Fear), &[Clue::Camp(home)], 60);
        learning_mut(&mut rules).acting = None;
        assert_eq!(weight(&world, &rules, &brains, id, other), 0);
    }

    #[test]
    fn an_agreeing_mark_makes_the_willing_mean_to_leave_and_other_levers_are_recorded() {
        let (mut rules, mut world, mut brains, ids) = valley(2);
        let kind = gut(&rules);
        learning_mut(&mut rules).notice_percent_by_will = vec![(0, 0)];
        rules.needs.trials.try_percent_by_will = vec![(0, 100)];
        let home = here(&rules, &brains, ids[0]);
        believe(
            &mut brains,
            ids[0],
            (kind, Sign::Fear),
            &[Clue::Camp(home)],
            60,
        );
        on_day(&mut world, 1);
        let season = super::super::season_of_day(&world, 1);
        believe(
            &mut brains,
            ids[1],
            (kind, Sign::Fear),
            &[Clue::Season(season)],
            60,
        );
        ill(&mut world, &mut brains, &rules, ids[0], 1);
        ill(&mut world, &mut brains, &rules, ids[1], 1);
        let news = steps(&mut brains);
        assert!(news.contains(&LearnStep::Minded { kind }));
        assert_eq!(brains.minds[&ids[0]].learner.act.minded, Some(kind));
        let prepare = u8::try_from(LEVERS.iter().position(|&l| l == "prepare").unwrap()).unwrap();
        assert!(news.contains(&LearnStep::Would { lever: prepare }));
        assert_eq!(brains.minds[&ids[1]].learner.act.minded, None);
        // The timid never: a will of 0 tries at the floor.
        let (mut rules, mut world, mut brains, ids) = valley(1);
        rules.needs.trials.try_percent_by_will = vec![(0, 0)];
        rules.needs.trials.try_floor_permille = 0;
        learning_mut(&mut rules).notice_percent_by_will = vec![(0, 0)];
        let home = here(&rules, &brains, ids[0]);
        believe(
            &mut brains,
            ids[0],
            (kind, Sign::Fear),
            &[Clue::Camp(home)],
            60,
        );
        ill(&mut world, &mut brains, &rules, ids[0], 1);
        assert_eq!(brains.minds[&ids[0]].learner.act.minded, None);
    }

    #[test]
    fn one_minded_calls_a_move_to_a_place_it_knows_and_not_to_one_it_fears() {
        let (rules, world, mut brains, ids) = valley(1);
        let id = ids[0];
        let kind = gut(&rules);
        let home = here(&rules, &brains, id);
        let (known, feared) = ((home.0 + 1, home.1), (home.0, home.1 + 1));
        let record = believe(
            &mut brains,
            id,
            (kind, Sign::Fear),
            &[Clue::Camp(home), Clue::Camp(feared)],
            60,
        );
        let lived = |place: PlaceKey| Lived {
            place,
            days: 1000,
            ..Lived::default()
        };
        let side = place_side(&rules);
        let call = |brains: &mut Brains| {
            let mind = brains.minds.get_mut(&id).unwrap();
            mind.learner.act.minded = Some(kind);
            let news = belief_call(&world, &rules, &brains.memory, (id, 1, 0), mind);
            let steps: Vec<LearnStep> = news
                .into_iter()
                .filter_map(|n| match n {
                    News::Learning { step, .. } => Some(step),
                    _ => None,
                })
                .collect();
            (steps, mind.calling.map(|c| place_of(c.to, side)))
        };
        brains.minds.get_mut(&id).unwrap().lived = vec![lived(feared)];
        assert_eq!(call(&mut brains), (vec![LearnStep::Waited { kind }], None));
        brains.minds.get_mut(&id).unwrap().lived = vec![lived(feared), lived(known)];
        assert_eq!(
            call(&mut brains),
            (vec![LearnStep::Called { record }], Some(known))
        );
        let mind = &brains.minds[&id];
        assert_eq!(mind.learner.act.minded, None);
        assert_eq!(mind.learner.act.call.map(|c| c.2), Some(home));
    }

    /// `id`'s home moved from `from` to `to` after twenty nights at
    /// `from`, its call from `call` if given; then `dawns` dawns woken at
    /// `to`, with `ills` ill episodes perceived on the trial's first day.
    /// The steps.
    fn move_and_wait(
        (rules, world, brains): (&Content, &mut World, &mut Brains),
        id: BodyId,
        (from, to, call): (PlaceKey, PlaceKey, Option<(RecordId, u8)>),
        (dawns, ills): (u32, u32),
    ) -> Vec<LearnStep> {
        let side = place_side(rules);
        let learning = rules.needs.learning.clone().unwrap();
        let season = Acting::trial_days(world.calendar().days_per_cycle());
        let to_pos = place_centre(to, side);
        let start = u64::from(season) + 1;
        on_day(world, start);
        {
            let mind = brains.minds.get_mut(&id).unwrap();
            mind.ways.home = to_pos;
            mind.learner.act.home = Some((from, Some((from, 20))));
            mind.learner.act.call = call.map(|(r, k)| (r, k, from, to_pos));
        }
        let circles = vec![(to_pos, vec![id])];
        brains.act_at_dawn(world, rules, &learning, &circles);
        let kind = gut(rules);
        let start32 = u32::try_from(start).unwrap();
        for n in 0..ills {
            let mind = brains.minds.get_mut(&id).unwrap();
            mind.learner.act.perceived.push((kind, start32, 10_000 + n));
        }
        for day in 1..=season {
            on_day(world, start + u64::from(day));
            let woke = if day <= dawns {
                circles.clone()
            } else {
                Vec::new()
            };
            brains.act_at_dawn(world, rules, &learning, &woke);
        }
        steps(brains)
    }

    #[test]
    fn a_trial_from_a_belief_is_kept_with_fewer_ills_and_abandoned_without() {
        for (ills, end) in [(0, TrialEnd::Kept), (2, TrialEnd::Abandoned)] {
            let (rules, mut world, mut brains, ids) = valley(1);
            let id = ids[0];
            let kind = gut(&rules);
            let home = here(&rules, &brains, id);
            let to = (home.0 + 1, home.1);
            let record = believe(&mut brains, id, (kind, Sign::Fear), &[Clue::Camp(home)], 60);
            // Two ills the season before.
            for (n, day) in [(0, 1), (1, 2)] {
                let mind = brains.minds.get_mut(&id).unwrap();
                mind.learner.act.perceived.push((kind, day, 20_000 + n));
            }
            let news = move_and_wait(
                (&rules, &mut world, &mut brains),
                id,
                (home, to, Some((record, kind))),
                (20, ills),
            );
            assert!(news.contains(&LearnStep::Moved {
                belief: true,
                nights: 20
            }));
            assert!(news.contains(&LearnStep::TrialBegan {
                kind,
                chance: false
            }));
            assert!(
                news.contains(&LearnStep::TrialEnded {
                    kind,
                    chance: false,
                    end
                }),
                "{news:?}"
            );
            let held = brains.minds[&id].learner.beliefs[0];
            match end {
                TrialEnd::Kept => assert!(held.lesson && held.fresh, "told as a lesson"),
                _ => assert_eq!(held.against, 1, "countered"),
            }
        }
    }

    #[test]
    fn a_trial_with_too_few_nights_away_is_not_judged() {
        let (rules, mut world, mut brains, ids) = valley(1);
        let id = ids[0];
        let kind = gut(&rules);
        let home = here(&rules, &brains, id);
        let record = believe(&mut brains, id, (kind, Sign::Fear), &[Clue::Camp(home)], 60);
        brains
            .minds
            .get_mut(&id)
            .unwrap()
            .learner
            .act
            .perceived
            .push((kind, 1, 20_000));
        let news = move_and_wait(
            (&rules, &mut world, &mut brains),
            id,
            (home, (home.0 + 1, home.1), Some((record, kind))),
            (2, 0),
        );
        assert!(news.contains(&LearnStep::TrialEnded {
            kind,
            chance: false,
            end: TrialEnd::Unjudged
        }));
    }

    #[test]
    fn a_move_for_another_reason_while_ill_is_a_trial_by_chance_that_may_teach() {
        let (mut rules, mut world, mut brains, ids) = valley(1);
        let id = ids[0];
        let kind = gut(&rules);
        learning_mut(&mut rules).notice_percent_by_will = vec![(0, 0)];
        let season = u64::from(Acting::trial_days(world.calendar().days_per_cycle()));
        ill(&mut world, &mut brains, &rules, id, season);
        let _ = brains.news();
        let home = here(&rules, &brains, id);
        let news = move_and_wait(
            (&rules, &mut world, &mut brains),
            id,
            (home, (home.0 + 1, home.1), None),
            (20, 0),
        );
        assert!(news.contains(&LearnStep::Moved {
            belief: false,
            nights: 20
        }));
        assert!(news.contains(&LearnStep::TrialBegan { kind, chance: true }));
        assert!(news.contains(&LearnStep::TrialEnded {
            kind,
            chance: true,
            end: TrialEnd::Kept
        }));
        let held = brains.minds[&id].learner.beliefs[0];
        let Some(Record::Belief { suspects, .. }) = brains.memory.get(held.access.record) else {
            panic!("a belief");
        };
        assert!(suspects.iter().any(|s| s.clue == Clue::Camp(home)));
        assert!(held.lesson && held.strength == 60, "its own illness");
    }

    #[test]
    fn a_kins_death_holds_its_episode_at_its_gravest() {
        let (rules, mut world, mut brains, ids) = valley(3);
        let (child, mother, stranger) = (ids[0], ids[1], ids[2]);
        let kind = gut(&rules);
        let years = u64::from(world.calendar().days_per_age_year());
        world.set_birth(child, 8 * years, Sex::Male);
        world.set_parents(child, Some(mother), None);
        ill(&mut world, &mut brains, &rules, child, 1);
        let episode = brains.minds[&child].learner.queue(kind).unwrap().episodes[0].0;
        for seer in [mother, stranger] {
            brains.memory.hold(episode.record);
            let mind = brains.minds.get_mut(&seer).unwrap();
            mind.learner.queues.push(super::super::learning::Queue {
                kind,
                episodes: vec![(episode, GRAVE_SEEN)],
            });
        }
        let home = here(&rules, &brains, mother);
        believe(
            &mut brains,
            mother,
            (kind, Sign::Fear),
            &[Clue::Camp(home)],
            30,
        );
        let _ = brains.news();
        let learning = rules.needs.learning.clone().unwrap();
        brains.mourn(&world, &learning, child, kind);
        let news = steps(&mut brains);
        assert_eq!(news, [LearnStep::Mourned { kind }], "the mother alone");
        let mind = &brains.minds[&mother];
        assert_eq!(mind.learner.queue(kind).unwrap().episodes[0].1, GRAVE_DEATH);
        assert_eq!(mind.learner.beliefs[0].strength, 100);
    }
}
