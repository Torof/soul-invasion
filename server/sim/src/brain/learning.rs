//! Learning causes, its first step: believe without acting (D59; the
//! study `content/ai-reserve/review/vivarium/lot-5/etude-apprentissage-
//! 2026-11-27.md`, its five recommendations approved by the project lead
//! on 27 November 2026).
//!
//! **A mark** is an event the body lives, never a feeling added: it falls
//! ill (`Event::FellIll`), or it sees a kin sick at its fire (its spouse,
//! its child under sixteen, its parent). The mark pushes the ailment's
//! problem on its things to think about (D50, amended 26 November 2026)
//! and keeps an **episode**: a record of the well (D55) holding what its
//! body perceived then, at most one of each circumstance the content
//! names (the camp, the nights slept there in a row, the season, the
//! crowd at the fire, a cold night, someone sick at the fire, what it ate
//! most, where it drank). The sick writes the episode; a kin who sees it
//! takes an access to it, and a mate at the fire who carries the same
//! problem too, without a mark. A problem keeps its last
//! `episodes_kept`.
//!
//! **Noticing** is a draw made only at a mark, with `notice_from`
//! episodes or more, by the will (`notice_percent_by_will`, about five
//! times likelier for the willing, but anyone may), by how many episodes
//! it keeps and by what touched it (its own child counts `child_times`).
//! One who notices **cross-checks**: it lays its latest episodes side by
//! side, as many as its will looks at (`depth_by_will`, three for the
//! timid, eight for the willing), and what was there each time (all but
//! once with `all_but_one_from` or more) becomes a **belief**, true or
//! false, at most `suspects_max` suspects drawn among more, each with the
//! kind of cause the content gives it. Nothing reads that kind yet.
//!
//! A belief is **told** at the evening fire the night after it was come
//! to, heard or renewed by a mark, and **taken** by the listeners who
//! carry the same problem and believe the teller enough (`copy_credit`),
//! and by the children without having lived it (the **tradition**). It
//! counts whole `whole_seasons` from its last renewal (a mark that
//! agrees, or the same belief heard), at half `half_seasons` more, then
//! is forgotten; `counter_examples` episodes without its suspects make it
//! fall.
//!
//! **No action**: no lever, no move, no trial comes of a belief in this
//! first step. **No hidden number**: nothing here reads a place's
//! fouling, the odds of falling ill, or any state the body does not
//! perceive; the nights slept at the camp stand for its dirt (tested
//! below). Played only when the content holds a `learning` section;
//! absent, nothing here runs, bit for bit as before.

use std::time::Instant;

use super::acting::{self, Doing};
use super::{Brains, Mind, News, place_of, place_side, problems, season_of_day, today};
use crate::content::{Content, Learning};
use crate::r#gen::scatter::{self, purpose};
use crate::memory::{self, Access, Clue, PlaceKey, Record, RecordId, Sign, Store, Suspect};
use crate::{Ailment, Body, BodyId, Event, World};

/// What a body's learning keeps (D59).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Learner {
    /// The camp it last slept at by its fire, and the nights in a row.
    pub(super) camp: Option<(PlaceKey, u32)>,
    /// What it ate since the last dawn: a material's index and grams.
    ate_today: Vec<(u16, u32)>,
    /// What it ate most the day before.
    ate_yesterday: Option<u16>,
    /// The place where it last drank.
    drank: Option<PlaceKey>,
    /// The last dawn it saw someone sick at its fire.
    sick_seen: Option<u32>,
    /// How many lay at its fire last night.
    fire_size: u32,
    /// Each problem's episodes, the oldest first.
    pub(super) queues: Vec<Queue>,
    /// Its beliefs, one a problem at most.
    pub(super) beliefs: Vec<Held>,
    /// What its acting keeps (D59, amended 27 November 2026).
    pub(super) act: Doing,
}

/// An episode's gravity for its holder (D59, amended 27 November 2026):
/// a sick body seen.
pub(super) const GRAVE_SEEN: u8 = 1;
/// Its own illness.
pub(super) const GRAVE_OWN: u8 = 2;
/// A kin's death of it.
pub(super) const GRAVE_DEATH: u8 = 3;

/// A problem's episodes (D59).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Queue {
    /// The problem.
    pub(super) kind: u8,
    /// The episodes, the oldest first, each with its gravity for its
    /// holder ([`GRAVE_SEEN`], [`GRAVE_OWN`], [`GRAVE_DEATH`]).
    pub(super) episodes: Vec<(Access, u8)>,
}

/// A belief held (D59).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Held {
    /// The problem.
    pub(super) kind: u8,
    /// The belief record, from whom, since when.
    pub(super) access: Access,
    /// The last day it was renewed.
    pub(super) renewed: u32,
    /// Episodes without its suspects since.
    pub(super) against: u32,
    /// Its credit in it: 100 its own.
    pub(super) credit: u8,
    /// To tell at the next fire.
    pub(super) fresh: bool,
    /// Taken without carrying its problem: the tradition.
    pub(super) tradition: bool,
    /// Its strength when last renewed, in points (D59, amended 27
    /// November 2026): from what was lived; 0 without acting.
    pub(super) strength: u8,
    /// A trial from it was kept: told as a lesson at the next fire.
    pub(super) lesson: bool,
}

impl Held {
    /// Its strength on `day` (D59, amended 27 November 2026): whole for
    /// its whole seasons from its renewal, half after.
    pub(super) fn strength_on(&self, learning: &Learning, day: u32, days_per_cycle: u32) -> u8 {
        let (whole, _) = learning.belief_days(days_per_cycle);
        if day.saturating_sub(self.renewed) >= whole {
            self.strength / 2
        } else {
            self.strength
        }
    }
}

/// What happened in a body's learning (D59), for the vivarium's measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LearnStep {
    /// A mark lived: its own ailment, or a kin seen sick.
    Marked {
        /// The problem.
        kind: u8,
        /// Its own ailment.
        own: bool,
    },
    /// It noticed the problem comes back: it cross-checks.
    Noticed {
        /// The problem.
        kind: u8,
    },
    /// It noticed and saw nothing that was always there.
    SawNothing {
        /// The problem.
        kind: u8,
    },
    /// It came to a belief.
    Believed {
        /// The belief.
        record: RecordId,
    },
    /// A belief it holds renewed: by a mark that agrees, by coming to it
    /// again, or (`heard`) by hearing it told.
    Renewed {
        /// The belief.
        record: RecordId,
        /// By hearing it.
        heard: bool,
    },
    /// An episode without a belief's suspects.
    Countered {
        /// The belief.
        record: RecordId,
    },
    /// It told a belief at its fire.
    Told {
        /// The belief.
        record: RecordId,
        /// The others at the fire.
        listeners: u32,
        /// Told as a lesson: a trial from it was kept.
        lesson: bool,
    },
    /// It took a belief told at its fire.
    Copied {
        /// The belief.
        record: RecordId,
        /// Its teller.
        from: BodyId,
        /// Without carrying its problem: the tradition.
        tradition: bool,
    },
    /// It forgot a belief.
    Forgotten {
        /// The belief.
        record: RecordId,
        /// Why.
        why: Forgot,
    },
    /// A kin it holds the episode of died of it: the episode at its
    /// gravest (D59, amended 27 November 2026).
    Mourned {
        /// The problem.
        kind: u8,
    },
    /// Its belief agreeing with a mark, its will made it mean to try its
    /// lever (D59, amended 27 November 2026).
    Minded {
        /// The problem.
        kind: u8,
    },
    /// Its belief agreeing with a mark gives a lever not built yet: where
    /// it would apply (D59, amended 27 November 2026).
    Would {
        /// The lever, its index in `content::LEVERS`.
        lever: u8,
    },
    /// It called a move from its belief, for the dawn's fire (or went
    /// alone without one).
    Called {
        /// The belief.
        record: RecordId,
    },
    /// It meant to leave and knew no place to go: it waits.
    Waited {
        /// The problem.
        kind: u8,
    },
    /// Its call from a belief was dropped: it stayed.
    Stayed {
        /// The belief.
        record: RecordId,
    },
    /// Its home moved to another place (a grown body), from its belief or
    /// for any other reason, after `nights` in a row at its camp.
    Moved {
        /// From its belief's call.
        belief: bool,
        /// Its nights in a row at the camp it left.
        nights: u32,
    },
    /// A trial against a problem began: from a belief, or by chance (a
    /// move for another reason while carrying the problem).
    TrialBegan {
        /// The problem.
        kind: u8,
        /// By chance.
        chance: bool,
    },
    /// A trial against a problem ended, judged by the ills it perceived.
    TrialEnded {
        /// The problem.
        kind: u8,
        /// By chance.
        chance: bool,
        /// How.
        end: TrialEnd,
    },
}

/// How a trial against a problem ended (D59, amended 27 November 2026).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrialEnd {
    /// Fewer ills perceived: kept, told as a lesson.
    Kept,
    /// Not fewer: given up, its belief countered.
    Abandoned,
    /// Too few nights away, or no ill before to compare: unjudged.
    Unjudged,
}

/// Why a belief was forgotten (D59).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forgot {
    /// Not renewed in its seasons.
    Faded,
    /// Too many episodes without its suspects.
    Countered,
    /// Another, believed more, took its place.
    Replaced,
}

/// A belief a body holds, as the vivarium reads it (D59).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeliefHeld {
    /// Who.
    pub body: BodyId,
    /// The problem.
    pub kind: u8,
    /// The belief record.
    pub record: RecordId,
    /// Its own, come to by cross-checking.
    pub own: bool,
    /// Taken without carrying its problem.
    pub tradition: bool,
    /// Its strength when last renewed, in points (0 without acting).
    pub strength: u8,
}

impl Learner {
    /// Drops every access it holds.
    pub(super) fn forget_all(&mut self, memory: &mut Store) {
        for queue in self.queues.drain(..) {
            for (access, _) in queue.episodes {
                memory.release(access.record);
            }
        }
        for held in self.beliefs.drain(..) {
            memory.release(held.access.record);
        }
    }

    pub(super) fn queue(&self, kind: u8) -> Option<&Queue> {
        self.queues.iter().find(|q| q.kind == kind)
    }

    fn holds_episode(&self, kind: u8, record: RecordId) -> bool {
        self.queue(kind)
            .is_some_and(|q| q.episodes.iter().any(|(a, _)| a.record == record))
    }

    /// Keeps `access` among the episodes of `kind`, the oldest dropped
    /// past `kept`.
    fn keep_episode(
        &mut self,
        kind: u8,
        (access, grave): (Access, u8),
        kept: u32,
        memory: &mut Store,
    ) {
        let at = if let Some(at) = self.queues.iter().position(|q| q.kind == kind) {
            at
        } else {
            self.queues.push(Queue {
                kind,
                episodes: Vec::new(),
            });
            self.queues.len() - 1
        };
        let episodes = &mut self.queues[at].episodes;
        episodes.push((access, grave));
        let kept = usize::try_from(kept).unwrap_or(usize::MAX).max(1);
        while episodes.len() > kept {
            let (oldest, _) = episodes.remove(0);
            memory.release(oldest.record);
        }
    }

    /// The camp-age tier of `camp` as lived: 0 new, 1 settled, 2 old.
    pub(super) fn camp_age(&self, learning: &Learning, camp: PlaceKey, days_per_cycle: u32) -> u8 {
        let nights = match self.camp {
            Some((key, n)) if key == camp => n,
            _ => 0,
        };
        if nights < learning.new_camp_nights {
            0
        } else if nights <= learning.old_camp_nights(days_per_cycle) {
            1
        } else {
            2
        }
    }
}

/// Whether `mind` carries the problem `kind`, not yet faded by `day`: the
/// list itself is left as it is (it fades only where it is touched, D50).
pub(super) fn carries_fresh(
    rules: &Content,
    days_per_cycle: u32,
    mind: &Mind,
    kind: u8,
    day: u64,
) -> bool {
    let keep = u64::from(rules.needs.problems.fade_days(days_per_cycle));
    mind.problems
        .iter()
        .any(|p| p.kind == kind && day.saturating_sub(p.day) < keep)
}

/// The circumstances `mind` perceives on `day` (D59): its own memory and
/// senses only.
pub(super) fn perceive(
    world: &World,
    rules: &Content,
    learning: &Learning,
    mind: &Mind,
    day: u64,
) -> Vec<Clue> {
    let side = place_side(rules);
    let camp = place_of(mind.home_at_dusk, side);
    let l = &mind.learner;
    let today = u32::try_from(day).unwrap_or(u32::MAX);
    let mut clues = Vec::with_capacity(learning.clues.len());
    for (name, _) in &learning.clues {
        let clue = match name.as_str() {
            "camp" => Some(Clue::Camp(camp)),
            "camp_age" => Some(Clue::CampAge(l.camp_age(
                learning,
                camp,
                world.calendar().days_per_cycle(),
            ))),
            "season" => Some(Clue::Season(season_of_day(world, day))),
            "crowd" => Some(Clue::Crowd(if l.fire_size < learning.crowd_some {
                0
            } else if l.fire_size < learning.crowd_many {
                1
            } else {
                2
            })),
            "cold_night" => mind
                .days
                .last()
                .is_some_and(|d| d.cold)
                .then_some(Clue::ColdNight),
            "sick_at_fire" => l
                .sick_seen
                .is_some_and(|seen| today.saturating_sub(seen) < learning.sick_seen_nights)
                .then_some(Clue::SickAtFire),
            "ate" => l.ate_yesterday.map(Clue::Ate),
            "drank" => l.drank.map(Clue::Drank),
            _ => None,
        };
        clues.extend(clue);
    }
    clues
}

/// The material named `id`, by its index in the content.
fn material_index(rules: &Content, id: &str) -> Option<u16> {
    rules
        .materials
        .iter()
        .position(|m| m.id == id)
        .and_then(|i| u16::try_from(i).ok())
}

/// The problem `ailment` is lived as, if it marks.
fn kind_of(rules: &Content, learning: &Learning, ailment: Ailment) -> Option<u8> {
    rules.needs.problems.kind_of(learning.problem_of(ailment)?)
}

/// What a mark needs to read of the world and the rules.
pub(super) struct Ctx<'a> {
    pub(super) world: &'a World,
    pub(super) rules: &'a Content,
    pub(super) learning: &'a Learning,
    pub(super) seed: u64,
    pub(super) day: u64,
    pub(super) now: u64,
}

/// `id` lived a mark of the problem `kind` (D59): the problem pushed on
/// its things to think about, the episode `access` kept, the belief it
/// holds of the problem renewed or countered by it, then, by the draw,
/// it notices and cross-checks. `times`: what touched it weighs the draw.
fn mark(
    ctx: &Ctx,
    (id, body, mind): (BodyId, &Body, &mut Mind),
    (kind, access, times, grave): (u8, Access, u32, u8),
    memory: &mut Store,
    news: &mut Vec<News>,
) {
    let own = grave == GRAVE_OWN;
    let Ctx {
        world,
        rules,
        learning,
        seed,
        day,
        now,
    } = *ctx;
    problems::push(world, rules, mind, kind, (body.pos(), day));
    let will = mind.will;
    let step = |step: LearnStep| News::Learning {
        body: id,
        will,
        step,
        at: now,
    };
    news.push(News::Problem {
        body: id,
        kind,
        step: super::ProblemStep::Lived,
        at: now,
    });
    news.push(step(LearnStep::Marked { kind, own }));
    mind.learner
        .keep_episode(kind, (access, grave), learning.episodes_kept, memory);
    acting::perceived(learning, mind, (kind, day, access.record));
    // The belief it holds of this problem: renewed when its suspects were
    // all there, countered when not.
    let clues: Vec<Clue> = match memory.get(access.record) {
        Some(Record::Episode { clues, .. }) => clues.clone(),
        _ => Vec::new(),
    };
    let today = u32::try_from(day).unwrap_or(u32::MAX);
    if let Some(at) = mind.learner.beliefs.iter().position(|h| h.kind == kind) {
        let held = mind.learner.beliefs[at];
        let agrees = match memory.get(held.access.record) {
            Some(Record::Belief { suspects, .. }) => {
                suspects.iter().all(|s| clues.contains(&s.clue))
            }
            _ => false,
        };
        if agrees {
            let lived = acting::strength_of(learning, grave);
            let cycle = world.calendar().days_per_cycle();
            let h = &mut mind.learner.beliefs[at];
            h.strength = h.strength_on(learning, today, cycle).max(lived);
            (h.renewed, h.against, h.fresh) = (today, 0, true);
            news.push(step(LearnStep::Renewed {
                record: held.access.record,
                heard: false,
            }));
        } else if held.against + 1 >= learning.counter_examples {
            mind.learner.beliefs.remove(at);
            memory.release(held.access.record);
            news.push(step(LearnStep::Forgotten {
                record: held.access.record,
                why: Forgot::Countered,
            }));
        } else {
            mind.learner.beliefs[at].against += 1;
            news.push(step(LearnStep::Countered {
                record: held.access.record,
            }));
        }
    }
    // Noticing: a draw at the mark, with enough episodes.
    let kept = mind
        .learner
        .queue(kind)
        .map_or(0, |q| u32::try_from(q.episodes.len()).unwrap_or(u32::MAX));
    if kept >= learning.notice_from.max(1) {
        let chance = learning.notice_ppm(will, kept, times);
        let roll = scatter::roll(
            seed,
            purpose::LEARN_NOTICE,
            &[
                u64::from(id.0),
                day,
                u64::from(kind),
                u64::from(access.record),
            ],
        ) % 1_000_000;
        if roll < chance {
            news.push(step(LearnStep::Noticed { kind }));
            cross_check(ctx, (id, mind), kind, memory, news);
        }
    }
    // Its lever, when the belief it holds now accuses what was there.
    acting::after_mark(ctx, (id, mind), (kind, &clues), memory, news);
}

/// `id` lays its latest episodes of `kind` side by side (D59), as many
/// as its will looks at: what was there each time (all but once with
/// `all_but_one_from` or more) is suspect; at most `suspects_max`, drawn
/// among more. Its belief of the problem renewed when the same, else a
/// new belief written and held in its place.
fn cross_check(
    ctx: &Ctx,
    (id, mind): (BodyId, &mut Mind),
    kind: u8,
    memory: &mut Store,
    news: &mut Vec<News>,
) {
    let will = mind.will;
    let step = |step: LearnStep| News::Learning {
        body: id,
        will,
        step,
        at: ctx.now,
    };
    let suspects = suspects_of(ctx, (id, mind), kind, memory);
    let strength = acting::strength_of(ctx.learning, gravest(ctx.learning, mind, kind));
    if suspects.is_empty() {
        news.push(step(LearnStep::SawNothing { kind }));
        return;
    }
    let today = u32::try_from(ctx.day).unwrap_or(u32::MAX);
    let held = mind.learner.beliefs.iter().position(|h| h.kind == kind);
    if let Some(at) = held {
        let h = mind.learner.beliefs[at];
        if matches!(memory.get(h.access.record), Some(Record::Belief { suspects: s, .. }) if *s == suspects)
        {
            let cycle = ctx.world.calendar().days_per_cycle();
            let h = &mut mind.learner.beliefs[at];
            h.strength = h.strength_on(ctx.learning, today, cycle).max(strength);
            (h.renewed, h.against, h.fresh) = (today, 0, true);
            news.push(step(LearnStep::Renewed {
                record: h.access.record,
                heard: false,
            }));
            return;
        }
        mind.learner.beliefs.remove(at);
        memory.release(h.access.record);
        news.push(step(LearnStep::Forgotten {
            record: h.access.record,
            why: Forgot::Replaced,
        }));
    }
    let record = hold_own(mind, (id, today, kind), (suspects, strength), memory);
    news.push(step(LearnStep::Believed { record }));
}

/// `id` holds on `today` its own new belief of `kind` accusing
/// `suspects` (a fear: illness is a bad thing), of `strength`: the
/// record written once. The record.
pub(super) fn hold_own(
    mind: &mut Mind,
    (id, today, kind): (BodyId, u32, u8),
    (suspects, strength): (Vec<Suspect>, u8),
    memory: &mut Store,
) -> RecordId {
    let (record, _) = memory.intern(Record::Belief {
        body: id,
        day: today,
        kind,
        sign: Sign::Fear,
        suspects,
    });
    mind.learner.beliefs.push(Held {
        kind,
        access: Access {
            record,
            by: id,
            day: today,
        },
        renewed: today,
        against: 0,
        credit: 100,
        fresh: true,
        tradition: false,
        strength,
        lesson: false,
    });
    record
}

/// The gravest of the episodes of `kind` `mind` would cross-check (D59,
/// amended 27 November 2026).
fn gravest(learning: &Learning, mind: &Mind, kind: u8) -> u8 {
    let Some(queue) = mind.learner.queue(kind) else {
        return 0;
    };
    let depth = usize::try_from(learning.depth(mind.will)).unwrap_or(usize::MAX);
    let from = queue.episodes.len().saturating_sub(depth);
    queue.episodes[from..]
        .iter()
        .map(|&(_, g)| g)
        .max()
        .unwrap_or(0)
}

/// What `id` finds was there each time in its latest episodes of `kind`
/// (D59): as many as its will looks at, all of them (all but one with
/// `all_but_one_from` or more), at most `suspects_max` drawn among more,
/// each with its kind; none when nothing was.
fn suspects_of(ctx: &Ctx, (id, mind): (BodyId, &Mind), kind: u8, memory: &Store) -> Vec<Suspect> {
    let learning = ctx.learning;
    let Some(queue) = mind.learner.queue(kind) else {
        return Vec::new();
    };
    let depth = usize::try_from(learning.depth(mind.will)).unwrap_or(usize::MAX);
    let from = queue.episodes.len().saturating_sub(depth);
    let mut counts: Vec<(Clue, u32)> = Vec::new();
    let mut looked = 0_u32;
    for (access, _) in &queue.episodes[from..] {
        let Some(Record::Episode { clues, .. }) = memory.get(access.record) else {
            continue;
        };
        looked += 1;
        for &clue in clues {
            match counts.iter_mut().find(|(c, _)| *c == clue) {
                Some((_, n)) => *n += 1,
                None => counts.push((clue, 1)),
            }
        }
    }
    let need = if looked >= learning.all_but_one_from {
        looked - 1
    } else {
        looked
    };
    let mut candidates: Vec<Clue> = counts
        .into_iter()
        .filter(|&(_, n)| looked > 0 && n >= need)
        .map(|(c, _)| c)
        .collect();
    candidates.sort_unstable();
    let most = usize::try_from(learning.suspects_max)
        .unwrap_or(usize::MAX)
        .max(1);
    let mut chosen: Vec<Clue> = Vec::new();
    let mut draw = 0_u64;
    while chosen.len() < most && !candidates.is_empty() {
        let at = if candidates.len() + chosen.len() <= most {
            0
        } else {
            let roll = scatter::roll(
                ctx.seed,
                purpose::LEARN_SUSPECT,
                &[u64::from(id.0), ctx.day, u64::from(kind), draw],
            );
            draw += 1;
            usize::try_from(roll % candidates.len() as u64).unwrap_or(0)
        };
        chosen.push(candidates.remove(at));
    }
    let mut suspects: Vec<Suspect> = chosen
        .into_iter()
        .map(|clue| Suspect {
            clue,
            kind: learning.kind_of(clue.name()).unwrap_or(u8::MAX),
        })
        .collect();
    suspects.sort_unstable();
    suspects
}

/// The kin relation of the sick `sick` to `seer` (D59): its weight in
/// the draw, 0 when not a kin. A child under sixteen of the seer's weighs
/// `child_times`; its spouse and its parent, once.
pub(super) fn kin_times(world: &World, learning: &Learning, sick: &Body, seer: BodyId) -> u32 {
    let parent_of_sick = sick.mother() == Some(seer) || sick.father() == Some(seer);
    if parent_of_sick && world.age_milli(sick) < 16_000 {
        return learning.child_times.max(1);
    }
    if sick.spouse() == Some(seer) {
        return 1;
    }
    let seer_child_of_sick = world
        .body(seer)
        .is_some_and(|b| b.mother() == Some(sick.id()) || b.father() == Some(sick.id()));
    u32::from(seer_child_of_sick)
}

impl Brains {
    /// The tick's events, for learning (D59): what each ate and where it
    /// drank, and who fell ill: a mark.
    pub(super) fn learn_events(&mut self, events: &[Event], world: &World, rules: &Content) {
        let Some(learning) = &rules.needs.learning else {
            return;
        };
        let started = Instant::now();
        let side = place_side(rules);
        for event in events {
            match event {
                Event::BodyAte {
                    body,
                    material,
                    grams,
                    ..
                } => {
                    let (Some(mind), Some(m)) =
                        (self.minds.get_mut(body), material_index(rules, material))
                    else {
                        continue;
                    };
                    let ate = &mut mind.learner.ate_today;
                    match ate.iter_mut().find(|(k, _)| *k == m) {
                        Some((_, g)) => *g = g.saturating_add(*grams),
                        None => ate.push((m, *grams)),
                    }
                }
                Event::BodyDrank { body, at } => {
                    if let Some(mind) = self.minds.get_mut(body) {
                        mind.learner.drank = Some(place_of(*at, side));
                    }
                }
                Event::FellIll { body, ailment, .. } => {
                    self.fell_ill(world, rules, learning, *body, *ailment);
                }
                Event::IllnessEnded {
                    body,
                    ailment,
                    died: true,
                    ..
                } => {
                    if let Some(kind) = kind_of(rules, learning, *ailment) {
                        self.mourn(world, learning, *body, kind);
                    }
                }
                _ => {}
            }
        }
        self.learning_nanos += elapsed_nanos(started);
    }

    /// `id` fell ill of `ailment` (D59): the episode written, a mark.
    fn fell_ill(
        &mut self,
        world: &World,
        rules: &Content,
        learning: &Learning,
        id: BodyId,
        ailment: Ailment,
    ) {
        let Some(kind) = kind_of(rules, learning, ailment) else {
            return;
        };
        let (Some(mind), Some(body)) = (self.minds.get_mut(&id), world.body(id)) else {
            return;
        };
        let day = today(world);
        let clues = perceive(world, rules, learning, mind, day);
        let today = u32::try_from(day).unwrap_or(u32::MAX);
        let (record, _) = self.memory.intern(Record::Episode {
            body: id,
            day: today,
            kind,
            clues,
        });
        let ctx = Ctx {
            world,
            rules,
            learning,
            seed: self.seed,
            day,
            now: world.tick().0,
        };
        let access = Access {
            record,
            by: id,
            day: today,
        };
        mark(
            &ctx,
            (id, body, mind),
            (kind, access, 1, GRAVE_OWN),
            &mut self.memory,
            &mut self.news,
        );
    }

    /// The dawn's learning (D59), before the fire's other parts: what
    /// each ate yesterday rolled, the beliefs and episodes of the faded
    /// let go; then at each fire, the nights slept there counted, the
    /// crowd and the sick seen, the kin seen sick marked, and the beliefs
    /// told and taken.
    pub(super) fn learn_at_dawn(
        &mut self,
        world: &World,
        rules: &Content,
        circles: &[(crate::Pos, Vec<BodyId>)],
    ) {
        let Some(learning) = &rules.needs.learning else {
            return;
        };
        let started = Instant::now();
        let cycle = world.calendar().days_per_cycle();
        let dawn = today(world);
        let dawn32 = u32::try_from(dawn).unwrap_or(u32::MAX);
        let (_, total) = learning.belief_days(cycle);
        let now = world.tick().0;
        for (&id, mind) in &mut self.minds {
            let mut eaten = None::<(u16, u32)>;
            for &(m, g) in &mind.learner.ate_today {
                if eaten.is_none_or(|(_, best)| g > best) {
                    eaten = Some((m, g));
                }
            }
            mind.learner.ate_yesterday = eaten.map(|(m, _)| m);
            mind.learner.ate_today.clear();
            mind.learner.fire_size = 1;
            let will = mind.will;
            let learner = &mut mind.learner;
            let memory = &mut self.memory;
            let news = &mut self.news;
            learner.beliefs.retain(|h| {
                if dawn32.saturating_sub(h.renewed) < total {
                    return true;
                }
                memory.release(h.access.record);
                news.push(News::Learning {
                    body: id,
                    will,
                    step: LearnStep::Forgotten {
                        record: h.access.record,
                        why: Forgot::Faded,
                    },
                    at: now,
                });
                false
            });
            let kinds: Vec<u8> = mind.learner.queues.iter().map(|q| q.kind).collect();
            for kind in kinds {
                if carries_fresh(rules, cycle, mind, kind, dawn) {
                    continue;
                }
                if let Some(at) = mind.learner.queues.iter().position(|q| q.kind == kind) {
                    for (access, _) in mind.learner.queues.remove(at).episodes {
                        self.memory.release(access.record);
                    }
                }
            }
        }
        let side = place_side(rules);
        for (camp, present) in circles {
            let key = place_of(*camp, side);
            let crowd = u32::try_from(present.len()).unwrap_or(u32::MAX);
            for id in present {
                if let Some(mind) = self.minds.get_mut(id) {
                    let l = &mut mind.learner;
                    l.camp = Some(match l.camp {
                        Some((k, n)) if k == key => (k, n.saturating_add(1)),
                        _ => (key, 1),
                    });
                    l.fire_size = crowd;
                }
            }
            self.see_sick(world, rules, learning, present);
            if present.len() >= 2 && rules.needs.fire.on() {
                self.tell_beliefs(world, rules, learning, present);
            }
        }
        self.act_at_dawn(world, rules, learning, circles);
        self.learning_nanos += elapsed_nanos(started);
    }

    /// At a fire at dawn (D59): who is sick there is seen by the others,
    /// the infants its present carry too (not in the fire's circle, but
    /// in their carrier's arms: the lead, 27 November 2026); a kin who
    /// sees it lives a mark and takes an access to its episode, a mate
    /// who carries the same problem takes the access alone.
    fn see_sick(
        &mut self,
        world: &World,
        rules: &Content,
        learning: &Learning,
        present: &[BodyId],
    ) {
        let dawn = today(world);
        let dawn32 = u32::try_from(dawn).unwrap_or(u32::MAX);
        let cycle = world.calendar().days_per_cycle();
        let carried = present
            .iter()
            .filter_map(|id| self.carried.get(id))
            .flatten();
        let sick: Vec<(BodyId, Ailment)> = present
            .iter()
            .chain(carried)
            .filter_map(|&id| {
                let s = world.body(id)?.condition().sick?;
                Some((id, s.ailment))
            })
            .collect();
        if sick.is_empty() {
            return;
        }
        for &id in present {
            if sick.iter().any(|&(s, _)| s != id)
                && let Some(mind) = self.minds.get_mut(&id)
            {
                mind.learner.sick_seen = Some(dawn32);
            }
        }
        let ctx = Ctx {
            world,
            rules,
            learning,
            seed: self.seed,
            day: dawn,
            now: world.tick().0,
        };
        for &(s, ailment) in &sick {
            let Some(kind) = kind_of(rules, learning, ailment) else {
                continue;
            };
            let (Some(sick_body), Some(sick_mind)) = (world.body(s), self.minds.get(&s)) else {
                continue;
            };
            // Its episode of this ailment: its last of the problem, its own.
            let Some(episode) = sick_mind
                .learner
                .queue(kind)
                .and_then(|q| q.episodes.last())
                .filter(|(a, _)| a.by == s)
                .map(|(a, _)| a.record)
            else {
                continue;
            };
            for &seer in present {
                if seer == s {
                    continue;
                }
                let times = kin_times(world, learning, sick_body, seer);
                let (Some(mind), Some(body)) = (self.minds.get_mut(&seer), world.body(seer)) else {
                    continue;
                };
                acting::perceived(learning, mind, (kind, dawn, episode));
                if mind.learner.holds_episode(kind, episode) {
                    continue;
                }
                let access = Access {
                    record: episode,
                    by: s,
                    day: dawn32,
                };
                if times > 0 {
                    self.memory.hold(episode);
                    mark(
                        &ctx,
                        (seer, body, mind),
                        (kind, access, times, GRAVE_SEEN),
                        &mut self.memory,
                        &mut self.news,
                    );
                } else if carries_fresh(rules, cycle, mind, kind, dawn) {
                    self.memory.hold(episode);
                    mind.learner.keep_episode(
                        kind,
                        (access, GRAVE_SEEN),
                        learning.episodes_kept,
                        &mut self.memory,
                    );
                }
            }
        }
    }

    /// At a fire at dawn (D59): each present tells its fresh beliefs, at
    /// most `told_per_night`; each other present who carries the same
    /// problem, or is a child, takes one it believes the teller enough
    /// for (`copy_credit`, half after its whole seasons), unless it holds
    /// its own of that problem or one believed more; hearing the same
    /// belief renews it.
    pub(super) fn tell_beliefs(
        &mut self,
        world: &World,
        rules: &Content,
        learning: &Learning,
        present: &[BodyId],
    ) {
        let dawn = today(world);
        let dawn32 = u32::try_from(dawn).unwrap_or(u32::MAX);
        let cycle = world.calendar().days_per_cycle();
        let (whole, _) = learning.belief_days(cycle);
        let now = world.tick().0;
        let most = usize::try_from(learning.told_per_night).unwrap_or(usize::MAX);
        // Who tells what: the teller, its will, the belief and its age.
        let mut tellings: Vec<(BodyId, u8, Held)> = Vec::new();
        for &id in present {
            let Some(mind) = self.minds.get_mut(&id) else {
                continue;
            };
            let will = mind.will;
            for held in mind
                .learner
                .beliefs
                .iter_mut()
                .filter(|h| h.fresh)
                .take(most)
            {
                tellings.push((id, will, *held));
                (held.fresh, held.lesson) = (false, false);
            }
        }
        let listeners = u32::try_from(present.len() - 1).unwrap_or(u32::MAX);
        for (teller, teller_will, told) in tellings {
            let record = told.access.record;
            let Some(Record::Belief {
                day: rec_day,
                suspects,
                ..
            }) = self.memory.get(record)
            else {
                continue;
            };
            let (rec_day, suspects) = (*rec_day, suspects.clone());
            self.news.push(News::Learning {
                body: teller,
                will: teller_will,
                step: LearnStep::Told {
                    record,
                    listeners,
                    lesson: told.lesson,
                },
                at: now,
            });
            let half = dawn32.saturating_sub(told.renewed) >= whole;
            for &id in present {
                if id != teller {
                    self.hear_belief(
                        world,
                        (rules, learning),
                        (id, teller, teller_will),
                        (told, rec_day, &suspects, half),
                    );
                }
            }
        }
    }

    /// `id` hears `told` from `teller` at dawn (D59): taken when it
    /// carries the problem or is a child and believes the teller enough,
    /// unless it holds its own belief of the problem or one believed
    /// more; the same belief heard renews it.
    fn hear_belief(
        &mut self,
        world: &World,
        (rules, learning): (&Content, &Learning),
        (id, teller, teller_will): (BodyId, BodyId, u8),
        (told, rec_day, suspects, half): (Held, u32, &[Suspect], bool),
    ) {
        let dawn = today(world);
        let dawn32 = u32::try_from(dawn).unwrap_or(u32::MAX);
        let cycle = world.calendar().days_per_cycle();
        let now = world.tick().0;
        let record = told.access.record;
        let (Some(mind), Some(body)) = (self.minds.get_mut(&id), world.body(id)) else {
            return;
        };
        let carries = carries_fresh(rules, cycle, mind, told.kind, dawn);
        let child = world.age_milli(body) < u64::from(learning.child_years) * 1000;
        if !carries && !child {
            return;
        }
        let mut credit = memory::credit(
            self.seed,
            (id, mind.will),
            (teller, teller_will),
            rec_day,
            dawn32.saturating_sub(1),
        );
        if half {
            credit /= 2;
        }
        let will = mind.will;
        let heard = acting::heard_strength(learning, told.strength_on(learning, dawn32, cycle));
        let held = mind
            .learner
            .beliefs
            .iter()
            .position(|h| h.kind == told.kind);
        if let Some(at) = held {
            let h = mind.learner.beliefs[at];
            let same = h.access.record == record
                || matches!(self.memory.get(h.access.record),
                    Some(Record::Belief { suspects: s, .. }) if s.as_slice() == suspects);
            if same {
                let again = &mut mind.learner.beliefs[at];
                again.strength = again.strength_on(learning, dawn32, cycle).max(heard);
                again.renewed = dawn32;
                self.news.push(News::Learning {
                    body: id,
                    will,
                    step: LearnStep::Renewed {
                        record: h.access.record,
                        heard: true,
                    },
                    at: now,
                });
                return;
            }
            if h.access.by == id || credit <= h.credit || u32::from(credit) < learning.copy_credit {
                return;
            }
            mind.learner.beliefs.remove(at);
            self.memory.release(h.access.record);
            self.news.push(News::Learning {
                body: id,
                will,
                step: LearnStep::Forgotten {
                    record: h.access.record,
                    why: Forgot::Replaced,
                },
                at: now,
            });
        } else if u32::from(credit) < learning.copy_credit {
            return;
        }
        self.memory.hold(record);
        mind.learner.beliefs.push(Held {
            kind: told.kind,
            access: Access {
                record,
                by: teller,
                day: dawn32,
            },
            renewed: dawn32,
            against: 0,
            credit,
            fresh: true,
            tradition: !carries,
            strength: heard,
            lesson: false,
        });
        self.news.push(News::Learning {
            body: id,
            will,
            step: LearnStep::Copied {
                record,
                from: teller,
                tradition: !carries,
            },
            at: now,
        });
    }

    /// The wall time the learning took so far, in nanoseconds (D59): a
    /// measure, never read by a rule.
    #[must_use]
    pub fn learning_nanos(&self) -> u64 {
        self.learning_nanos
    }

    /// Every belief held (D59), by body.
    #[must_use]
    pub fn beliefs(&self) -> Vec<BeliefHeld> {
        self.minds
            .iter()
            .flat_map(|(&body, mind)| {
                mind.learner.beliefs.iter().map(move |h| BeliefHeld {
                    body,
                    kind: h.kind,
                    record: h.access.record,
                    own: h.access.by == body,
                    tradition: h.tradition,
                    strength: h.strength,
                })
            })
            .collect()
    }
}

/// Nanoseconds since `started`, saturated.
fn elapsed_nanos(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::content::Sex;
    use crate::map::Pos;
    use crate::time::{Calendar, Tick};
    use crate::{Sickness, World};

    /// The repository's content, a valley of minute ticks, and `n` bodies
    /// with minds on one tile, grown (thirty years).
    pub(in crate::brain) fn valley(n: usize) -> (Content, World, Brains, Vec<BodyId>) {
        let rules = crate::crops::tests::content();
        let mut world = World::with_valley(Calendar::new(1, 1440).unwrap(), &rules, 768);
        let side = i32::try_from(world.map().chunks_wide()).unwrap() * 32;
        let stand = (0..side)
            .flat_map(|y| (0..side).map(move |x| Pos::new(x, y)))
            .find(|&p| world.map().is_passable(p))
            .unwrap();
        let mut brains = Brains::new(7);
        let years = u64::from(world.calendar().days_per_age_year());
        let ids: Vec<BodyId> = (0..n)
            .map(|_| {
                let id = world.spawn_body(stand);
                world.set_birth(id, 30 * years, Sex::Female);
                brains.adopt(&world, &rules, id);
                id
            })
            .collect();
        (rules, world, brains, ids)
    }

    pub(in crate::brain) fn learning_mut(rules: &mut Content) -> &mut Learning {
        rules
            .needs
            .learning
            .as_mut()
            .expect("the repository learns")
    }

    /// The world on `day`, a little after its dawn.
    pub(in crate::brain) fn on_day(world: &mut World, day: u64) {
        world.tick = Tick(day * world.calendar().ticks_per_day() + 10);
    }

    /// `id` falls ill of `ailment` on `day`.
    pub(in crate::brain) fn ill(
        world: &mut World,
        brains: &mut Brains,
        rules: &Content,
        id: BodyId,
        day: u64,
    ) {
        on_day(world, day);
        let at = world.body(id).unwrap().pos();
        let event = Event::FellIll {
            body: id,
            ailment: Ailment::Gut,
            caught: false,
            at,
        };
        brains.learn_events(&[event], world, rules);
    }

    pub(in crate::brain) fn gut(rules: &Content) -> u8 {
        rules.needs.problems.kind_of("ill_gut").unwrap()
    }

    pub(in crate::brain) fn steps(brains: &mut Brains) -> Vec<LearnStep> {
        brains
            .news()
            .into_iter()
            .filter_map(|n| match n {
                News::Learning { step, .. } => Some(step),
                _ => None,
            })
            .collect()
    }

    fn episodes(brains: &Brains, id: BodyId, kind: u8) -> Vec<Vec<Clue>> {
        brains.minds[&id]
            .learner
            .queue(kind)
            .map(|q| {
                q.episodes
                    .iter()
                    .map(|(a, _)| match brains.memory.get(a.record) {
                        Some(Record::Episode { clues, .. }) => clues.clone(),
                        other => panic!("an episode, not {other:?}"),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn suspects(brains: &Brains, record: RecordId) -> Vec<Suspect> {
        match brains.memory.get(record) {
            Some(Record::Belief { suspects, .. }) => suspects.clone(),
            other => panic!("a belief, not {other:?}"),
        }
    }

    #[test]
    fn falling_ill_pushes_its_problem_and_keeps_an_episode_of_what_the_body_perceived() {
        let (rules, mut world, mut brains, ids) = valley(1);
        let id = ids[0];
        let kind = gut(&rules);
        ill(&mut world, &mut brains, &rules, id, 1);
        let mind = &brains.minds[&id];
        assert!(mind.carries(kind), "the gut on its things to think about");
        let camp = place_of(mind.home_at_dusk, place_side(&rules));
        let eps = episodes(&brains, id, kind);
        assert_eq!(eps.len(), 1);
        assert!(eps[0].contains(&Clue::Camp(camp)));
        assert!(eps[0].contains(&Clue::CampAge(0)), "never slept there: new");
        assert!(eps[0].contains(&Clue::Season(season_of_day(&world, 1))));
        assert!(eps[0].contains(&Clue::Crowd(0)));
        assert!(!eps[0].contains(&Clue::ColdNight));
        assert_eq!(
            steps(&mut brains),
            [LearnStep::Marked { kind, own: true }],
            "one episode: nothing to notice"
        );
    }

    #[test]
    fn a_problem_keeps_its_last_episodes_and_lets_the_oldest_go() {
        let (mut rules, mut world, mut brains, ids) = valley(1);
        learning_mut(&mut rules).notice_percent_by_will = vec![(0, 0)];
        let kept = learning_mut(&mut rules).episodes_kept;
        for day in 0..u64::from(kept) + 3 {
            ill(&mut world, &mut brains, &rules, ids[0], day);
        }
        let q = brains.minds[&ids[0]].learner.queue(gut(&rules)).unwrap();
        assert_eq!(q.episodes.len(), kept as usize);
        assert_eq!(q.episodes[0].0.day, 3, "the three oldest dropped");
        // Its own records only: those dropped are freed.
        let live = brains
            .memory
            .records()
            .filter(|(_, r, _)| matches!(r, Record::Episode { .. }))
            .count();
        assert_eq!(live, kept as usize);
    }

    #[test]
    fn noticing_a_problem_that_comes_back_makes_a_belief_of_what_was_there_each_time() {
        let (mut rules, mut world, mut brains, ids) = valley(1);
        let id = ids[0];
        let l = learning_mut(&mut rules);
        l.notice_percent_by_will = vec![(0, 100)];
        l.episodes_kept = 3;
        ill(&mut world, &mut brains, &rules, id, 0);
        ill(&mut world, &mut brains, &rules, id, 1);
        assert!(
            !steps(&mut brains)
                .iter()
                .any(|s| matches!(s, LearnStep::Noticed { .. }))
        );
        ill(&mut world, &mut brains, &rules, id, 2);
        let kind = gut(&rules);
        let news = steps(&mut brains);
        assert!(news.contains(&LearnStep::Noticed { kind }));
        let Some(&LearnStep::Believed { record }) = news
            .iter()
            .find(|s| matches!(s, LearnStep::Believed { .. }))
        else {
            panic!("a belief: {news:?}");
        };
        let accused = suspects(&brains, record);
        assert_eq!(accused.len(), 2, "two suspects drawn among more");
        for s in &accused {
            for ep in episodes(&brains, id, kind) {
                assert!(ep.contains(&s.clue), "{s:?} there each time");
            }
            let name = s.clue.name();
            assert_eq!(
                Some(s.kind),
                rules.needs.learning.as_ref().unwrap().kind_of(name)
            );
        }
        let held = brains.beliefs();
        assert_eq!(held.len(), 1);
        assert!(held[0].own && !held[0].tradition && held[0].record == record);
    }

    #[test]
    fn the_willing_lay_more_episodes_side_by_side_than_the_timid() {
        let (mut rules, mut world, mut brains, ids) = valley(1);
        let id = ids[0];
        let l = learning_mut(&mut rules);
        l.notice_percent_by_will = vec![(0, 0)];
        l.suspects_max = 16;
        let (a, b) = ((100, 100), (200, 200));
        for day in 0..8 {
            brains.minds.get_mut(&id).unwrap().learner.drank = Some(if day < 5 { a } else { b });
            ill(&mut world, &mut brains, &rules, id, day);
        }
        let (learning, now) = (rules.needs.learning.clone().unwrap(), world.tick().0);
        let c = Ctx {
            world: &world,
            rules: &rules,
            learning: &learning,
            seed: 7,
            day: 8,
            now,
        };
        let kind = gut(&rules);
        let mut mind = brains.minds[&id].clone();
        mind.will = 0;
        let timid = suspects_of(&c, (id, &mind), kind, &brains.memory);
        mind.will = 100;
        let willing = suspects_of(&c, (id, &mind), kind, &brains.memory);
        assert!(
            timid.iter().any(|s| s.clue == Clue::Drank(b)),
            "three alike"
        );
        assert!(
            !willing.iter().any(|s| s.clue == Clue::Drank(b)),
            "five of eight elsewhere"
        );
        assert!(willing.iter().any(|s| matches!(s.clue, Clue::Camp(_))));
    }

    #[test]
    fn episodes_without_its_suspects_make_a_belief_fall_and_one_with_them_renews_it() {
        let (mut rules, mut world, mut brains, ids) = valley(1);
        let id = ids[0];
        let kind = gut(&rules);
        learning_mut(&mut rules).notice_percent_by_will = vec![(0, 0)];
        let make = |brains: &mut Brains, clue: Clue| {
            let (record, _) = brains.memory.intern(Record::Belief {
                body: id,
                day: 0,
                kind,
                sign: Sign::Fear,
                suspects: vec![Suspect { clue, kind: 0 }],
            });
            brains.minds.get_mut(&id).unwrap().learner.beliefs = vec![Held {
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
                strength: 0,
                lesson: false,
            }];
            record
        };
        let record = make(&mut brains, Clue::ColdNight);
        for day in 1..=3 {
            ill(&mut world, &mut brains, &rules, id, day);
        }
        let news = steps(&mut brains);
        assert_eq!(
            news.iter()
                .filter(|s| matches!(s, LearnStep::Countered { .. }))
                .count(),
            2
        );
        assert!(news.contains(&LearnStep::Forgotten {
            record,
            why: Forgot::Countered
        }));
        assert_eq!(brains.beliefs(), []);
        let record = make(&mut brains, Clue::CampAge(0));
        ill(&mut world, &mut brains, &rules, id, 4);
        assert!(steps(&mut brains).contains(&LearnStep::Renewed {
            record,
            heard: false
        }));
        let h = brains.minds[&id].learner.beliefs[0];
        assert!(h.fresh && h.renewed == 4, "renewed, to tell tonight");
    }

    #[test]
    fn a_belief_told_at_the_fire_is_taken_by_who_carries_its_problem_and_by_the_children() {
        let (mut rules, mut world, mut brains, ids) = valley(4);
        let (teller, carrier, child) = (ids[0], ids[1], ids[3]);
        let l = learning_mut(&mut rules);
        l.notice_percent_by_will = vec![(0, 100)];
        l.episodes_kept = 3;
        l.copy_credit = 1;
        let years = u64::from(world.calendar().days_per_age_year());
        world.set_birth(child, 8 * years, Sex::Male);
        for day in 0..3 {
            ill(&mut world, &mut brains, &rules, teller, day);
        }
        ill(&mut world, &mut brains, &rules, carrier, 2);
        let record = brains.beliefs()[0].record;
        let _ = brains.news();
        on_day(&mut world, 3);
        let learning = rules.needs.learning.clone().unwrap();
        brains.tell_beliefs(&world, &rules, &learning, &ids);
        let news = steps(&mut brains);
        assert!(news.contains(&LearnStep::Told {
            record,
            listeners: 3,
            lesson: false,
        }));
        assert!(news.contains(&LearnStep::Copied {
            record,
            from: teller,
            tradition: false
        }));
        assert!(news.contains(&LearnStep::Copied {
            record,
            from: teller,
            tradition: true
        }));
        let held: Vec<BodyId> = brains.beliefs().iter().map(|b| b.body).collect();
        assert_eq!(held, [teller, carrier, child], "the stranger lives no gut");
        assert_eq!(brains.memory.refs(record), 3);
        // Told once: the teller's is no longer fresh; the copies are, and
        // hearing them back renews it without a tale more.
        brains.tell_beliefs(&world, &rules, &learning, &ids);
        let news = steps(&mut brains);
        let told = news
            .iter()
            .filter(|s| matches!(s, LearnStep::Told { .. }))
            .count();
        assert_eq!(told, 2, "the carrier and the child tell it");
        brains.tell_beliefs(&world, &rules, &learning, &ids);
        assert_eq!(steps(&mut brains), [], "nothing fresh: silence");
    }

    #[test]
    fn a_belief_not_renewed_fades_after_its_seasons() {
        let (mut rules, mut world, mut brains, ids) = valley(1);
        let l = learning_mut(&mut rules);
        l.notice_percent_by_will = vec![(0, 100)];
        l.episodes_kept = 3;
        for day in 0..3 {
            ill(&mut world, &mut brains, &rules, ids[0], day);
        }
        let record = brains.beliefs()[0].record;
        let _ = brains.news();
        let (_, total) = rules
            .needs
            .learning
            .as_ref()
            .unwrap()
            .belief_days(world.calendar().days_per_cycle());
        on_day(&mut world, 2 + u64::from(total) - 1);
        brains.learn_at_dawn(&world, &rules, &[]);
        assert_eq!(brains.beliefs().len(), 1);
        on_day(&mut world, 2 + u64::from(total));
        brains.learn_at_dawn(&world, &rules, &[]);
        assert!(steps(&mut brains).contains(&LearnStep::Forgotten {
            record,
            why: Forgot::Faded
        }));
        assert_eq!(brains.beliefs(), []);
        assert_eq!(brains.memory.refs(record), 0, "freed");
    }

    #[test]
    fn a_kin_seen_sick_at_the_fire_marks_and_a_stranger_does_not() {
        let (rules, mut world, mut brains, ids) = valley(3);
        let (sick, spouse, stranger) = (ids[0], ids[1], ids[2]);
        ill(&mut world, &mut brains, &rules, sick, 1);
        let _ = brains.news();
        for b in &mut world.bodies {
            if b.id == sick {
                b.spouse = Some(spouse);
                b.condition.sick = Some(Sickness {
                    ailment: Ailment::Gut,
                    until_day: 10,
                });
            }
        }
        on_day(&mut world, 2);
        let learning = rules.needs.learning.clone().unwrap();
        brains.see_sick(&world, &rules, &learning, &ids);
        let kind = gut(&rules);
        assert!(steps(&mut brains).contains(&LearnStep::Marked { kind, own: false }));
        assert!(brains.minds[&spouse].carries(kind));
        assert_eq!(
            episodes(&brains, spouse, kind),
            episodes(&brains, sick, kind)
        );
        assert!(!brains.minds[&stranger].carries(kind));
        assert_eq!(episodes(&brains, stranger, kind), Vec::<Vec<Clue>>::new());
        assert_eq!(brains.minds[&stranger].learner.sick_seen, Some(2));
        // Seen again the next dawn: the same episode, no second mark.
        on_day(&mut world, 3);
        brains.see_sick(&world, &rules, &learning, &ids);
        assert_eq!(steps(&mut brains), []);
    }

    #[test]
    fn an_infant_sick_in_its_mothers_arms_is_seen_by_its_mother_and_its_father() {
        let (rules, mut world, mut brains, ids) = valley(4);
        let (infant, mother, father, stranger) = (ids[0], ids[1], ids[2], ids[3]);
        let days = u64::from(world.calendar().days_per_age_year());
        world.set_birth(infant, days / 2, Sex::Male);
        world.set_parents(infant, Some(mother), Some(father));
        ill(&mut world, &mut brains, &rules, infant, 1);
        let _ = brains.news();
        for b in &mut world.bodies {
            if b.id == infant {
                b.condition.sick = Some(Sickness {
                    ailment: Ailment::Gut,
                    until_day: 10,
                });
            }
        }
        // In its mother's arms, not in the fire's circle.
        brains.carried.insert(mother, vec![infant]);
        on_day(&mut world, 2);
        let learning = rules.needs.learning.clone().unwrap();
        let present = [mother, father, stranger];
        brains.see_sick(&world, &rules, &learning, &present);
        let kind = gut(&rules);
        let marked = steps(&mut brains)
            .iter()
            .filter(|s| **s == LearnStep::Marked { kind, own: false })
            .count();
        assert_eq!(marked, 2, "its mother and its father");
        for parent in [mother, father] {
            assert_eq!(
                episodes(&brains, parent, kind),
                episodes(&brains, infant, kind)
            );
        }
        assert!(!brains.minds[&stranger].carries(kind));
        assert_eq!(
            brains.minds[&stranger].learner.sick_seen,
            Some(2),
            "seen all the same"
        );
        // Its own child: the draw weighs it `child_times`.
        let body = world.body(infant).unwrap();
        for parent in [mother, father] {
            assert_eq!(
                kin_times(&world, &learning, body, parent),
                learning.child_times
            );
        }
        assert_eq!(kin_times(&world, &learning, body, stranger), 0);
    }

    #[test]
    fn without_the_section_nothing_is_learnt() {
        let (mut rules, mut world, mut brains, ids) = valley(1);
        rules.needs.learning = None;
        let before = brains.minds[&ids[0]].clone();
        ill(&mut world, &mut brains, &rules, ids[0], 1);
        brains.learn_at_dawn(&world, &rules, &[(Pos::new(0, 0), ids.clone())]);
        assert_eq!(brains.minds[&ids[0]], before);
        assert_eq!(brains.news(), []);
        assert_eq!(brains.learning_nanos(), 0);
    }

    /// The proof that the brain reads no hidden number (D59): two worlds
    /// alike but for their camps' fouling and their hearths' counts give
    /// the same episodes, the same beliefs, the same minds.
    #[test]
    fn the_brain_cannot_read_the_fouling_of_its_camp() {
        let run = |fouled: bool| {
            let (mut rules, mut world, mut brains, ids) = valley(2);
            let l = learning_mut(&mut rules);
            l.notice_percent_by_will = vec![(0, 100)];
            l.episodes_kept = 3;
            if fouled {
                let side = place_side(&rules);
                let home = brains.minds[&ids[0]].home_at_dusk;
                let place = crate::illness::place_of(&rules, home);
                world.fouling.insert(place, 1_000_000);
                world.fouling.insert(place_of(home, side), 1_000_000);
            }
            for day in 0..3 {
                ill(&mut world, &mut brains, &rules, ids[0], day);
            }
            on_day(&mut world, 3);
            brains.learn_at_dawn(&world, &rules, &[(Pos::new(0, 0), ids.clone())]);
            let records: Vec<Record> = brains
                .memory
                .records()
                .filter(|(_, r, _)| matches!(r, Record::Episode { .. } | Record::Belief { .. }))
                .map(|(_, r, _)| r.clone())
                .collect();
            let news = brains.news();
            (records, brains.minds.clone(), news)
        };
        let clean = run(false);
        assert_eq!(clean.0.len(), 4, "three episodes and a belief");
        assert_eq!(clean, run(true));
    }

    /// And the code itself (D59): nothing of the brain's code names a
    /// hidden number of illness; only the vivarium reads them, for its
    /// report.
    #[test]
    fn no_line_of_the_brain_names_a_hidden_number_of_illness() {
        let sources = [
            include_str!("../brain.rs"),
            include_str!("learning.rs"),
            include_str!("acting.rs"),
            include_str!("problems.rs"),
            include_str!("handwork.rs"),
            include_str!("pooling.rs"),
        ];
        // Spelt in pieces, so that this test does not name them itself.
        let hidden = [
            concat!("foul", "ing("),
            concat!("foul", "ing."),
            concat!("fouled", "_places"),
            concat!("hearth", "_count("),
            concat!("episodes", "_permille"),
            concat!("contagion", "_permille"),
            concat!("hearths", "_for_today"),
            concat!("illness", "_day("),
        ];
        for source in sources {
            // Its code, the tests apart (the proof above sets a fouling).
            let code = source.split("\n#[cfg(test)]\nmod tests").next().unwrap();
            let code = code
                .split("\n#[cfg(test)]\npub(super) mod tests")
                .next()
                .unwrap();
            for word in hidden {
                assert!(!code.contains(word), "the brain names {word}");
            }
        }
    }
}
