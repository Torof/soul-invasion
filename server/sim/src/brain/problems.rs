//! A refusal of the world lived and carried in mind as a problem (D50
//! and D57, amended 26 November 2026; the project lead: "not a feeling
//! added to push them, rather a limitation that leads to thought: my arms
//! are full, I can't take more…"; "one may think about a problem in front
//! of it, or later; and in front of it, it is often when one is free").
//!
//! One kind today, read from the content (`content/needs.ron`, section
//! `problems`): `full`, a gathering stopped with the arms full while the
//! plant still bears. Nothing names what would answer it (the label that
//! tied it to a property of the materials is gone, 27 November 2026).
//!
//! **The things to think about** (the project lead's design, for
//! lightness): each body keeps a short list of the problems it lived, at
//! most `mind_slots`, the last lived on top. A problem lived is pushed on
//! top (already there, it moves up and its count grows; the oldest drops
//! off a full list). Living it, the chance of its will may make it mean to
//! try it. At a free moment the brain already passes through (a calm
//! thought its hands reach, by day, or the nightfall's point where a calm
//! trial may be drawn), free of hunger and thirst, the body pops the top
//! problem it means to try and begins a trial of its hands if the things
//! around allow a gesture its hands do not know; with nothing new around,
//! the problem goes back below the top, for a later free moment. The list
//! is read only when a problem is lived and at those free moments, and
//! only when not empty; nothing scans it by tick. A body may also begin
//! there and then, where it lives the problem, if it is free. The gesture is drawn as in any trial of the hands, nothing
//! weighed toward an answer. The trial ends as soon as a gathering
//! stopped full finds the body carrying more than its arms alone, or
//! when `gives_up_seasons` passed, and has one verdict (amended 28
//! November 2026): kept if and only if a chain of its deeds served the
//! problem ("what served is kept"); else given up when the problem was
//! lived again and nothing moved; unjudged, as
//! [`handwork::end_untried`], when nothing new was tried, or when the
//! problem never came back. It is told at the fire
//! with its problem, and only a listener who carries the same problem has
//! a reason to try it.

use super::{Brains, Judged, Mind, News, Trial, Way, before_trial, handwork, season_bounds, today};
use crate::brain::{HUNGRY, THIRSTY};
use crate::content::Content;
use crate::r#gen::scatter::{self, purpose};
use crate::map::Pos;
use crate::memory::{Record, Store};
use crate::{Body, BodyId, Event, WorkEnd, World};

/// A problem a body lived and carries in mind (D50, amended 26 November
/// 2026): an entry of its things to think about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Problem {
    /// The kind: its index in the content's `problems.kinds`.
    pub kind: u8,
    /// Where it was last lived.
    pub at: Pos,
    /// The day it was last lived.
    pub day: u64,
    /// How many times it was lived since it entered the list.
    pub count: u32,
    /// Its will made it mean to try it, the moment it lived it: it waits
    /// for a free moment with something new around.
    pub minded: bool,
    /// Where and on which day it last looked around in vain at a free
    /// moment: it looks again only beyond the hands' reach or another day.
    looked: Option<(u64, Pos)>,
    /// The first day of the season of the last trial begun from it, and
    /// how many were begun in that season.
    tried: (u64, u32),
}

/// A trial begun from a problem (D50, amended 26 November 2026): the
/// problem judges it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProblemAim {
    /// The kind of problem.
    pub kind: u8,
    /// The times the problem came back with the limit unmoved.
    pub returns: u32,
    /// The day its time is out: `gives_up_seasons` after it began, or
    /// later when it saw something new near its end, never past `last`.
    pub until: u64,
    /// The latest day its time may be out (amended 28 November 2026):
    /// its time and the `trial_days` once, however many deeds it sees.
    pub last: u64,
}

/// What happened of a problem (D50, amended 26 November 2026), for the
/// vivarium's measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemStep {
    /// Lived once more.
    Lived,
    /// Its will made the body try, and it could not begin then (not free
    /// where it lived it, or nothing new to gesture around): it waits on
    /// its list for a later free moment.
    Waited,
    /// A trial of the hands begun from it; `heard`: aimed first at deeds
    /// told at the fire with the same problem; `later`: at a free moment
    /// after it was lived, not there and then.
    Began {
        /// Aimed at deeds told with the same problem.
        heard: bool,
        /// Begun at a later free moment.
        later: bool,
    },
    /// Its trial kept: a gathering stopped full found the body carrying
    /// more than its arms.
    Kept,
    /// Its trial given up: its time out, the problem lived again unmoved.
    Abandoned,
    /// Its trial ended unjudged: nothing new was tried.
    Untried,
    /// Its trial ended unjudged: its time out, the problem never back.
    Lapsed,
}

impl Mind {
    /// Whether it carries the problem of kind `kind` among its things to
    /// think about (D50, amended 26 November 2026).
    pub(super) fn carries(&self, kind: u8) -> bool {
        self.problems.iter().any(|p| p.kind == kind)
    }
}

impl Brains {
    /// The problems the tick's events make lived, and the trials they
    /// judge or begin (D50, amended 26 November 2026).
    pub(super) fn live_problems(&mut self, events: &[Event], world: &World, rules: &Content) {
        let Some(kind) = rules.needs.problems.kind_of("full") else {
            return;
        };
        for event in events {
            let Event::WorkStopped {
                body: id,
                why: WorkEnd::Full,
                ..
            } = event
            else {
                continue;
            };
            let (Some(mind), Some(body)) = (self.minds.get_mut(id), world.body(*id)) else {
                continue;
            };
            let news = stopped_full(
                world,
                rules,
                (&self.memory, self.seed),
                (*id, body, kind),
                mind,
            );
            self.news.extend(news);
        }
    }
}

/// `id`'s gathering stopped full (D50, amended 26 November 2026): its
/// trial from that problem kept when it carries more than its arms, a
/// return counted when its arms alone are full again; the problem then
/// pushed on its things to think about, and, by the chance of its will,
/// a trial begun there and then if it is free and something new lies
/// around. The news.
fn stopped_full(
    world: &World,
    rules: &Content,
    (memory, seed): (&Store, u64),
    (id, body, kind): (BodyId, &Body, u8),
    mind: &mut Mind,
) -> Vec<News> {
    let movement = &rules.movement;
    let arms = body.arms_g(movement);
    // The limit moved: it carries more than its arms alone hold.
    let beyond = body.food_g() > arms;
    // Lived: nothing it carries lies in a hollow, and its arms are full.
    let lived = body.stowed_g() == 0 && body.food_g() >= arms;
    let (day, now) = (today(world), world.tick().0);
    let mut news = Vec::new();
    if let Some(trial) = mind
        .trial
        .filter(|t| t.problem.is_some_and(|a| a.kind == kind))
    {
        if beyond {
            // It lives it now: what holds its food served it.
            handwork::note_served(world, rules, id, mind);
            news.extend(judge(rules, (id, day, now), mind, &trial, true));
        } else if lived && let Some(aim) = mind.trial.as_mut().and_then(|t| t.problem.as_mut()) {
            aim.returns += 1;
        }
    }
    if !lived {
        return news;
    }
    push(world, rules, mind, kind, (body.pos(), day));
    news.push(News::Problem {
        body: id,
        kind,
        step: ProblemStep::Lived,
        at: now,
    });
    if !may_begin(world, rules, kind, mind) {
        return news;
    }
    let minded = mind.problems.first().is_some_and(|p| p.minded);
    if !minded {
        let roll = scatter::roll(
            seed,
            purpose::PROBLEM_TRY,
            &[u64::from(id.0), now, u64::from(kind)],
        );
        if roll % 1000 >= handwork::will_chance(rules, mind) {
            return news;
        }
        if let Some(p) = mind.problems.first_mut() {
            p.minded = true;
            p.looked = None;
        }
    }
    let again = handwork::Again::of(rules, seed, id, day);
    if free(body) && handwork::something_new(world, rules, (id, body.pos()), mind, again) {
        news.extend(begin(
            world,
            rules,
            (memory, seed),
            (id, day, now),
            kind,
            mind,
            false,
        ));
    } else if !minded {
        news.push(News::Problem {
            body: id,
            kind,
            step: ProblemStep::Waited,
            at: now,
        });
    }
    news
}

/// Free of an urgent need (D50, amended 26 November 2026): its hunger and
/// its thirst under the lines the day's distress counts (`HUNGRY`,
/// `THIRSTY`).
fn free(body: &Body) -> bool {
    let c = body.condition();
    c.hunger.points() < HUNGRY && c.thirst.points() < THIRSTY
}

/// Whether a trial from the problem `kind` may begin now: the hands able
/// to try, no trial nor call under way, the season's trials from it not
/// spent.
fn may_begin(world: &World, rules: &Content, kind: u8, mind: &Mind) -> bool {
    let Some(per_season) = rules.needs.problems.kind(kind).map(|k| k.trials_per_season) else {
        return false;
    };
    if !rules.needs.hands.on() || mind.trial.is_some() || mind.calling.is_some() {
        return false;
    }
    let (first, _) = season_bounds(world, today(world));
    mind.problems
        .iter()
        .find(|p| p.kind == kind)
        .is_some_and(|p| p.tried.0 != first || p.tried.1 < per_season)
}

/// The problem of kind `kind` lived at `at` on `day`, pushed on top of
/// `mind`'s things to think about (the project lead's design): already
/// there, it moves up and its count grows; a list over `mind_slots` lets
/// its oldest drop off. Faded entries go first ([`fade`]).
pub(super) fn push(
    world: &World,
    rules: &Content,
    mind: &mut Mind,
    kind: u8,
    (at, day): (Pos, u64),
) {
    fade(rules, world.calendar().days_per_cycle(), day, mind);
    let entry = match mind.problems.iter().position(|p| p.kind == kind) {
        Some(i) => {
            let mut p = mind.problems.remove(i);
            p.at = at;
            p.day = day;
            p.count = p.count.saturating_add(1);
            p
        }
        None => Problem {
            kind,
            at,
            day,
            count: 1,
            minded: false,
            looked: None,
            tried: (u64::MAX, 0),
        },
    };
    mind.problems.insert(0, entry);
    let slots = usize::try_from(rules.needs.problems.mind_slots).unwrap_or(usize::MAX);
    mind.problems.truncate(slots.max(1));
}

/// A free moment of `id` at `here` (D50, amended 26 November 2026; the
/// project lead's design, "when one is free", by day as by night): a
/// calm thought its hands reach (no need called it, not at night nor
/// walking home), or the nightfall's point where a calm trial may be
/// drawn. With things to think about, the faded ones go and it pops the
/// top one its will made it mean to try; free and the problem able to
/// begin, it begins a trial of its hands when the things around allow a
/// gesture its hands do not know; with nothing new, the problem goes back
/// below the top, and it looks again only beyond the hands' reach of here
/// or another day. Empty, nothing is done. The news.
pub(super) fn free_moment(
    world: &World,
    rules: &Content,
    (memory, seed): (&Store, u64),
    (id, here, day, now): (BodyId, Pos, u64, u64),
    mind: &mut Mind,
) -> Vec<News> {
    if mind.problems.is_empty() {
        return Vec::new();
    }
    fade(rules, world.calendar().days_per_cycle(), day, mind);
    let Some(top) = mind.problems.iter().position(|p| p.minded) else {
        return Vec::new();
    };
    let p = mind.problems[top];
    let near = i64::from(rules.needs.hands.near_tiles);
    if p.looked
        .is_some_and(|(d, at)| d == day && super::far(at, here) <= near)
        || !world.body(id).is_some_and(free)
        || !may_begin(world, rules, p.kind, mind)
    {
        return Vec::new();
    }
    let again = handwork::Again::of(rules, seed, id, day);
    if handwork::something_new(world, rules, (id, here), mind, again) {
        return begin(
            world,
            rules,
            (memory, seed),
            (id, day, now),
            p.kind,
            mind,
            true,
        );
    }
    // Nothing new around: back below the top.
    let mut p = mind.problems.remove(top);
    p.looked = Some((day, here));
    let below = (top + 1).min(mind.problems.len());
    mind.problems.insert(below, p);
    Vec::new()
}

/// `id` begins on `day` a trial of its hands from the problem `kind`
/// (D50, amended 26 November 2026): counted against the season, aimed
/// first at the deeds told at the fire with the same problem, those
/// whose result it knows left out (D57, amended 28 November 2026: its
/// memory brings them back when a deed aimed at needs what they give),
/// else its gestures drawn as any trial of the hands; its time
/// `gives_up_seasons` from today. `later`: at a free moment after it was
/// lived. The news.
fn begin(
    world: &World,
    rules: &Content,
    (memory, seed): (&Store, u64),
    (id, day, now): (BodyId, u64, u64),
    kind: u8,
    mind: &mut Mind,
    later: bool,
) -> Vec<News> {
    let trials = &rules.needs.trials;
    let (first, _) = season_bounds(world, day);
    let span = rules
        .needs
        .problems
        .kind(kind)
        .map_or(1, |k| k.gives_up_days(world.calendar().days_per_cycle()));
    if let Some(p) = mind.problems.iter_mut().find(|p| p.kind == kind) {
        let spent = if p.tried.0 == first { p.tried.1 } else { 0 };
        p.tried = (first, spent + 1);
        p.minded = false;
        p.looked = None;
    }
    let before = mind.ways;
    let told = told_with(memory, mind, kind);
    let sifted = handwork::sift(mind, handwork::Again::of(rules, seed, id, day), &told);
    if sifted.new.is_empty() {
        mind.handwork.drop_aim();
    } else {
        handwork::aim_at(mind, &sifted.new, day);
    }
    handwork::note_heard(mind, &told, day);
    mind.trial = Some(Trial {
        way: Way::Hands,
        before,
        started_day: day,
        before_days: before_trial(world, trials, mind, Way::Hands, day),
        heard: super::HeardDays::default(),
        caller: None,
        problem: Some(ProblemAim {
            kind,
            returns: 0,
            until: day + u64::from(span),
            last: day + u64::from(span) + u64::from(trials.trial_days),
        }),
    });
    mind.changed_day = Some(day);
    let heard = !sifted.new.is_empty();
    let mut news = vec![
        News::Trying {
            body: id,
            way: Way::Hands,
            before,
            after: mind.ways,
            heard,
            at: now,
        },
        News::Problem {
            body: id,
            kind,
            step: ProblemStep::Began { heard, later },
            at: now,
        },
    ];
    news.extend(handwork::again_news(id, &sifted.again, now));
    news
}

/// The deeds of the hands told at the fire with the problem `kind`, kept,
/// that `mind` does not know (D50, amended 26 November 2026: "problem X,
/// deed Y, it worked"), each with its number of tellers.
fn told_with(memory: &Store, mind: &Mind, kind: u8) -> Vec<(super::Deed, u32)> {
    let mut told: Vec<(super::Deed, Vec<BodyId>)> = Vec::new();
    for access in &mind.heard_ways {
        let Some(Record::Trial {
            way: Way::Hands,
            kept: true,
            after,
            problem: Some(k),
            ..
        }) = memory.get(access.record)
        else {
            continue;
        };
        if *k != kind {
            continue;
        }
        for deed in after.hands.iter() {
            if mind.ways.hands.contains(deed) {
                continue;
            }
            match told.iter_mut().find(|(d, _)| *d == deed) {
                Some((_, by)) if !by.contains(&access.by) => by.push(access.by),
                Some(_) => {}
                None => told.push((deed, vec![access.by])),
            }
        }
    }
    told.into_iter()
        .map(|(d, by)| (d, u32::try_from(by.len()).unwrap_or(u32::MAX)))
        .collect()
}

/// `mind`'s trial from a problem ended (D50, amended 26 and 28 November
/// 2026). Unjudged when nothing new was tried
/// ([`handwork::end_untried`]). Else by the one verdict of a trial of the
/// hands: kept if and only if a chain of its deeds served the problem it
/// lived ([`handwork::keep_what_served`]), at whichever ending: the
/// gathering stopped beyond its arms, its time out with the problem back
/// or never back. Kept, its will learns a success and the tale told is a
/// success with its problem. Not kept and `judged` (the problem came
/// back unmoved in its time), it is given up: its will learns a failure,
/// told with its problem. Not kept and never judged by its problem, it
/// lapses: nothing told, its will as it was. The news.
fn judge(
    rules: &Content,
    (id, day, now): (BodyId, u64, u64),
    mind: &mut Mind,
    trial: &Trial,
    judged: bool,
) -> Vec<News> {
    let Some(kind) = trial.problem.map(|a| a.kind) else {
        return Vec::new();
    };
    if let Some(untried) = handwork::end_untried(mind, trial, (id, now)) {
        return vec![
            untried,
            News::Problem {
                body: id,
                kind,
                step: ProblemStep::Untried,
                at: now,
            },
        ];
    }
    let kept = handwork::keep_what_served(rules, mind);
    mind.handwork.drop_aim();
    mind.trial = None;
    if !kept && !judged {
        return vec![
            News::Untried { body: id, at: now },
            News::Problem {
                body: id,
                kind,
                step: ProblemStep::Lapsed,
                at: now,
            },
        ];
    }
    mind.judged = Some(Judged {
        way: Way::Hands,
        after: mind.ways,
        up: true,
        kept,
        before: trial.before_days,
        during: mind.lately(rules.needs.trials.trial_days),
        grades: None,
        problem: Some(kind),
    });
    mind.learn_will(rules, kept);
    if kept {
        mind.changed_day = Some(day);
    }
    vec![
        News::Tried {
            body: id,
            way: Way::Hands,
            kept,
            heard_days: trial.heard.days,
            heard_nights: trial.heard.nights,
            grades: None,
            at: now,
        },
        News::Problem {
            body: id,
            kind,
            step: if kept {
                ProblemStep::Kept
            } else {
                ProblemStep::Abandoned
            },
            at: now,
        },
    ]
}

/// At nightfall on `day` (a cycle of `per_year` days): a trial from a
/// problem whose time is out ends ([`judge`]): kept when a chain of it
/// served, else given up when the problem came back unmoved in it, else
/// lapsed unjudged; its news in `news` and `true` then: nothing else
/// changes that night, as after any trial ended; any other trial under
/// way is due, returned, when its days are over.
pub(super) fn nightfall(
    rules: &Content,
    (id, day, now, per_year): (BodyId, u64, u64, u32),
    mind: &mut Mind,
    news: &mut Vec<News>,
) -> (Option<Trial>, bool) {
    let Some(trial) = mind.trial else {
        return (None, false);
    };
    if let Some(aim) = trial.problem {
        let over = day >= aim.until;
        if over {
            news.extend(judge(rules, (id, day, now), mind, &trial, aim.returns > 0));
        }
        return (None, over);
    }
    let days = rules.needs.trials.days_for(trial.way.key(), per_year);
    (
        (day >= trial.started_day + u64::from(days)).then_some(trial),
        false,
    )
}

/// The problems among `mind`'s things to think about not lived for the
/// content's `fades_seasons` by `day` are forgotten (D50, amended 26
/// November 2026), as the lessons fade: checked only when the list is
/// touched.
pub(super) fn fade(rules: &Content, days_per_cycle: u32, day: u64, mind: &mut Mind) {
    if mind.problems.is_empty() {
        return;
    }
    let keep = u64::from(rules.needs.problems.fade_days(days_per_cycle));
    mind.problems.retain(|p| day.saturating_sub(p.day) < keep);
}

#[cfg(test)]
mod tests {
    use super::super::{Brains, Deed, Deeds, Lately, NOTHING, Ways};
    use super::*;
    use crate::Intention;
    use crate::hands::Gesture;
    use crate::memory::Access;
    use crate::time::Calendar;

    /// The repository's content, its trials made certain for the willing
    /// in distress and never for a will of 0 (no floor), a valley of
    /// minute ticks and a body with a mind: the rules, world, brains and
    /// body.
    fn valley() -> (Content, World, Brains, BodyId) {
        let mut rules = crate::crops::tests::content();
        rules.needs.trials.try_percent_by_will = vec![(0, 0), (100, 100)];
        rules.needs.trials.try_floor_permille = 0;
        let mut world = World::with_valley(Calendar::new(1, 1440).unwrap(), &rules, 768);
        let side = i32::try_from(world.map().chunks_wide()).unwrap() * 32;
        let stand = (0..side)
            .flat_map(|y| (0..side).map(move |x| Pos::new(x, y)))
            .find(|&p| world.map().is_passable(p))
            .unwrap();
        let id = world.spawn_body(stand);
        let mut brains = Brains::new(7);
        brains.adopt(&world, &rules, id);
        (rules, world, brains, id)
    }

    /// `mind`'s will and its days of distress.
    fn willing(brains: &mut Brains, id: BodyId, will: u8) {
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.will = will;
        mind.distress_days = 10;
    }

    /// Lays `grams` of `material` at the body's feet and has it picked up.
    fn take(world: &mut World, rules: &Content, id: BodyId, material: &str, grams: u32) {
        let pos = world.body(id).unwrap().pos();
        let item = world.spawn_item(material, grams, pos);
        world.queue(Intention::PickUp { body: id, item });
        world.advance(1, rules, &mut Vec::new());
    }

    /// Has it take a birch bark folded into a hollow: the hollow.
    fn take_hollow(world: &mut World, rules: &Content, id: BodyId) -> crate::ItemId {
        let pos = world.body(id).unwrap().pos();
        let item = world.spawn_hollow("birch_bark", 200, pos, rules);
        world.queue(Intention::PickUp { body: id, item });
        world.advance(1, rules, &mut Vec::new());
        item
    }

    /// Puts the food of its arms in `hollow`.
    fn stow(world: &mut World, rules: &Content, id: BodyId, hollow: crate::ItemId) {
        let food: Vec<crate::ItemId> = world
            .body(id)
            .unwrap()
            .inventory()
            .filter(|i| *i != hollow && world.item(*i).unwrap().within().is_none())
            .collect();
        for item in food {
            assert!(world.put_now(id, (item, hollow), rules).is_some());
        }
    }

    fn put_berries(rules: &Content) -> Deed {
        let index = |id: &str| {
            u16::try_from(rules.materials.iter().position(|m| m.id == id).unwrap()).unwrap()
        };
        Deed {
            gesture: Gesture::PutIn,
            on: index("berries"),
            with: index("birch_bark") | handwork::HOLLOW,
        }
    }

    /// Lays a flint at its feet: something new to take.
    fn flint_here(world: &mut World, id: BodyId) {
        let pos = world.body(id).unwrap().pos();
        world.spawn_item("flint", 300, pos);
    }

    /// Its hunger at `points`.
    fn hunger(world: &mut World, rules: &Content, id: BodyId, points: u32) {
        let b = world.bodies.iter_mut().find(|b| b.id == id).unwrap();
        // Its reserve short of so many points' worth: as hungry.
        b.condition.deficit_j =
            u64::from(points) * u64::from(rules.needs.kj_per_hunger_point) * 1000;
        b.condition.hunger = crate::Gauge::at(points);
    }

    /// At nightfall on `day`: the trial's end, if any.
    fn night(
        world: &World,
        brains: &mut Brains,
        rules: &Content,
        id: BodyId,
        day: u64,
    ) -> Vec<News> {
        let cycle = world.calendar().days_per_cycle();
        let mind = brains.minds.get_mut(&id).unwrap();
        let mut news = Vec::new();
        let _ = nightfall(rules, (id, day, 0, cycle), mind, &mut news);
        news
    }

    /// Its arms full of berries, nothing to hold more.
    fn arms_full(world: &mut World, rules: &Content, id: BodyId) {
        let arms = world.body(id).unwrap().arms_g(&rules.movement);
        take(world, rules, id, "berries", arms);
        let body = world.body(id).unwrap();
        assert_eq!(body.food_g(), arms, "its arms full");
    }

    /// A gathering stopped full, a tick later.
    fn stopped(world: &mut World, rules: &Content, brains: &mut Brains, id: BodyId) {
        world.advance(1, rules, &mut Vec::new());
        let event = Event::WorkStopped {
            body: id,
            gives: "berries".to_owned(),
            grams: 100,
            why: WorkEnd::Full,
        };
        brains.live_problems(&[event], world, rules);
    }

    fn fold_bark(rules: &Content) -> Deed {
        Deed {
            gesture: Gesture::Fold,
            on: u16::try_from(
                rules
                    .materials
                    .iter()
                    .position(|m| m.id == "birch_bark")
                    .unwrap(),
            )
            .unwrap(),
            with: NOTHING,
        }
    }

    fn steps(news: &[News]) -> Vec<ProblemStep> {
        news.iter()
            .filter_map(|n| match n {
                News::Problem { step, .. } => Some(*step),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_gathering_stopped_with_the_arms_full_is_remembered_as_a_problem_and_fades() {
        let (rules, mut world, mut brains, id) = valley();
        willing(&mut brains, id, 0);
        arms_full(&mut world, &rules, id);
        stopped(&mut world, &rules, &mut brains, id);
        stopped(&mut world, &rules, &mut brains, id);
        let at = world.body(id).unwrap().pos();
        let mind = &brains.minds[&id];
        assert_eq!(mind.problems.len(), 1);
        let p = mind.problems[0];
        assert_eq!((p.kind, p.at, p.day, p.count), (0, at, 0, 2));
        assert!(mind.carries(0));
        assert_eq!(steps(&brains.news()), [ProblemStep::Lived; 2]);
        // Not lived for the content's seasons: forgotten.
        let cycle = world.calendar().days_per_cycle();
        let keep = u64::from(rules.needs.problems.fade_days(cycle));
        let mind = brains.minds.get_mut(&id).unwrap();
        fade(&rules, cycle, keep - 1, mind);
        assert!(mind.carries(0));
        fade(&rules, cycle, keep, mind);
        assert!(!mind.carries(0));
    }

    #[test]
    fn carrying_beyond_the_arms_or_without_the_section_is_no_problem() {
        let (mut rules, mut world, mut brains, id) = valley();
        willing(&mut brains, id, 100);
        let hollow = take_hollow(&mut world, &rules, id);
        take(&mut world, &rules, id, "berries", 1000);
        stow(&mut world, &rules, id, hollow);
        take(&mut world, &rules, id, "berries", 1000);
        let body = world.body(id).unwrap();
        assert!(body.food_g() > body.arms_g(&rules.movement));
        stopped(&mut world, &rules, &mut brains, id);
        assert_eq!(brains.minds[&id].problems.len(), 0);
        // A hollow merely carried moves no limit (amended 27 November
        // 2026): its arms full beside it, the problem is lived.
        let (_, mut world, mut brains, id) = valley();
        willing(&mut brains, id, 0);
        take_hollow(&mut world, &rules, id);
        arms_full(&mut world, &rules, id);
        stopped(&mut world, &rules, &mut brains, id);
        assert_eq!(brains.minds[&id].problems.len(), 1);
        let (_, mut world, mut brains, id) = valley();
        rules.needs.problems = crate::content::Problems::default();
        willing(&mut brains, id, 100);
        arms_full(&mut world, &rules, id);
        stopped(&mut world, &rules, &mut brains, id);
        assert_eq!(brains.minds[&id].problems.len(), 0);
        assert!(brains.minds[&id].trial.is_none());
        assert_eq!(brains.news().len(), 0);
    }

    #[test]
    fn only_a_willing_body_begins_a_trial_of_its_hands_from_its_problem() {
        let (rules, mut world, mut brains, id) = valley();
        willing(&mut brains, id, 0);
        flint_here(&mut world, id);
        arms_full(&mut world, &rules, id);
        for _ in 0..30 {
            stopped(&mut world, &rules, &mut brains, id);
        }
        assert!(brains.minds[&id].trial.is_none(), "a will of 0 never tries");
        willing(&mut brains, id, 100);
        stopped(&mut world, &rules, &mut brains, id);
        let trial = brains.minds[&id].trial.expect("the willing tries");
        assert_eq!(trial.way, Way::Hands);
        let aim = trial.problem.expect("from its problem");
        assert_eq!((aim.kind, aim.returns), (0, 0));
        assert!(brains.minds[&id].handwork.aim.is_empty(), "nothing told");
        let news = brains.news();
        assert!(
            steps(&news).contains(&ProblemStep::Began {
                heard: false,
                later: false
            }),
            "{news:?}"
        );
    }

    #[test]
    fn a_trial_from_a_problem_is_kept_when_it_carries_more_than_its_arms_next_time() {
        let (rules, mut world, mut brains, id) = valley();
        willing(&mut brains, id, 100);
        flint_here(&mut world, id);
        arms_full(&mut world, &rules, id);
        stopped(&mut world, &rules, &mut brains, id);
        let deed = fold_bark(&rules);
        let mind = brains.minds.get_mut(&id).unwrap();
        assert!(mind.trial.is_some());
        // In its trial it folded the bark and put its berries in it,
        // gestures new to its hands; it also took a pebble, of which
        // nothing came that served.
        let put = put_berries(&rules);
        let pebble = Deed {
            gesture: Gesture::PickUp,
            on: u16::try_from(
                rules
                    .materials
                    .iter()
                    .position(|m| m.id == "pebble")
                    .unwrap(),
            )
            .unwrap(),
            with: NOTHING,
        };
        mind.handwork.gestured = true;
        // Seen in its trial, not kept (amended 28 November 2026).
        for seen in [deed, put, pebble] {
            mind.handwork.see(seen, 32);
        }
        let _ = brains.news();
        // The hollow merely carried, its arms full again: the limit has
        // not moved (amended 27 November 2026).
        let hollow = take_hollow(&mut world, &rules, id);
        stopped(&mut world, &rules, &mut brains, id);
        assert_eq!(
            brains.minds[&id]
                .trial
                .and_then(|t| t.problem)
                .map(|a| a.returns),
            Some(1)
        );
        let _ = brains.news();
        stow(&mut world, &rules, id, hollow);
        take(&mut world, &rules, id, "berries", 1000);
        stopped(&mut world, &rules, &mut brains, id);
        let mind = &brains.minds[&id];
        assert!(mind.trial.is_none());
        assert!(mind.ways.hands.contains(deed), "what served is kept");
        assert!(mind.ways.hands.contains(put));
        assert!(
            !mind.ways.hands.contains(pebble) && mind.handwork.saw(pebble),
            "a deed of which nothing came that served is not kept: seen"
        );
        assert_eq!(
            mind.handwork.kept,
            [put, deed],
            "its own, from a kept trial"
        );
        let judged = mind.judged.expect("told tonight");
        assert!(judged.kept);
        assert_eq!(judged.problem, Some(0), "told with its problem");
        let news = brains.news();
        assert_eq!(steps(&news), [ProblemStep::Kept]);
        assert!(news.iter().any(|n| matches!(
            n,
            News::Tried {
                way: Way::Hands,
                kept: true,
                ..
            }
        )));
    }

    #[test]
    fn a_trial_from_a_problem_is_given_up_when_its_season_passed_with_the_problem_unmoved() {
        let (rules, mut world, mut brains, id) = valley();
        let cycle = world.calendar().days_per_cycle();
        let span = u64::from(rules.needs.problems.kinds[0].gives_up_days(cycle));
        assert_eq!(span, u64::from(cycle) / 4, "a season");
        willing(&mut brains, id, 100);
        flint_here(&mut world, id);
        arms_full(&mut world, &rules, id);
        stopped(&mut world, &rules, &mut brains, id);
        let deed = fold_bark(&rules);
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.gestured = true;
        mind.handwork.see(deed, 32);
        let _ = brains.news();
        // The problem comes back unmoved, again and again: it waits.
        for back in 1..=5 {
            stopped(&mut world, &rules, &mut brains, id);
            let aim = brains.minds[&id].trial.and_then(|t| t.problem);
            assert_eq!(aim.map(|a| a.returns), Some(back));
        }
        assert_eq!(night(&world, &mut brains, &rules, id, span - 1).len(), 0);
        assert!(brains.minds[&id].trial.is_some(), "not before its season");
        let news = night(&world, &mut brains, &rules, id, span);
        let mind = &brains.minds[&id];
        assert!(mind.trial.is_none());
        assert!(
            !mind.ways.hands.contains(deed),
            "given up, its ways as before"
        );
        assert!(mind.judged.is_some_and(|j| !j.kept && j.problem == Some(0)));
        assert_eq!(steps(&news), [ProblemStep::Abandoned]);
        // A trial in which nothing new was tried is not judged.
        let (rules, mut world, mut brains, id) = valley();
        willing(&mut brains, id, 100);
        flint_here(&mut world, id);
        arms_full(&mut world, &rules, id);
        stopped(&mut world, &rules, &mut brains, id);
        stopped(&mut world, &rules, &mut brains, id);
        let news = night(&world, &mut brains, &rules, id, span);
        let mind = &brains.minds[&id];
        assert!(mind.trial.is_none() && mind.judged.is_none());
        assert_eq!(steps(&news), [ProblemStep::Untried]);
    }

    #[test]
    fn at_most_the_contents_trials_a_season_begin_from_one_problem() {
        let (rules, mut world, mut brains, id) = valley();
        assert_eq!(rules.needs.problems.kinds[0].trials_per_season, 1);
        willing(&mut brains, id, 100);
        flint_here(&mut world, id);
        arms_full(&mut world, &rules, id);
        stopped(&mut world, &rules, &mut brains, id);
        assert!(brains.minds[&id].trial.is_some());
        // Ended (here unjudged), within the problem's memory.
        let until = brains.minds[&id].trial.unwrap().problem.unwrap().until;
        let _ = night(&world, &mut brains, &rules, id, until);
        assert!(brains.minds[&id].trial.is_none());
        for _ in 0..20 {
            stopped(&mut world, &rules, &mut brains, id);
        }
        let mind = brains.minds.get_mut(&id).unwrap();
        let news = free_moment(
            &world,
            &rules,
            (&brains.memory, 0),
            (id, world.body(id).unwrap().pos(), 0, 0),
            mind,
        );
        assert_eq!(news.len(), 0);
        assert!(brains.minds[&id].trial.is_none(), "one a season");
        let began = |news: &[News]| {
            steps(news)
                .iter()
                .filter(|s| matches!(s, ProblemStep::Began { .. }))
                .count()
        };
        assert_eq!(began(&brains.news()), 1);
        // Another season: it may begin again.
        brains.minds.get_mut(&id).unwrap().problems[0].tried.0 = u64::MAX;
        stopped(&mut world, &rules, &mut brains, id);
        assert!(brains.minds[&id].trial.is_some());
        assert_eq!(began(&brains.news()), 1);
    }

    #[test]
    fn a_body_not_free_where_it_lives_its_problem_tries_at_a_later_free_moment() {
        let (rules, mut world, mut brains, id) = valley();
        willing(&mut brains, id, 100);
        arms_full(&mut world, &rules, id);
        // Hungry where it lives it: not free, it waits on its list.
        hunger(&mut world, &rules, id, HUNGRY + 5);
        flint_here(&mut world, id);
        stopped(&mut world, &rules, &mut brains, id);
        assert!(brains.minds[&id].trial.is_none());
        assert_eq!(brains.minds[&id].problems.len(), 1);
        assert_eq!(
            steps(&brains.news()),
            [ProblemStep::Lived, ProblemStep::Waited]
        );
        // Still hungry at nightfall: not a free moment.
        let mind = brains.minds.get_mut(&id).unwrap();
        let news = free_moment(
            &world,
            &rules,
            (&brains.memory, 0),
            (id, world.body(id).unwrap().pos(), 0, 0),
            mind,
        );
        assert_eq!(news.len(), 0);
        // Fed, at the nightfall's free moment, something new around: it
        // begins.
        hunger(&mut world, &rules, id, 0);
        let mind = brains.minds.get_mut(&id).unwrap();
        let news = free_moment(
            &world,
            &rules,
            (&brains.memory, 0),
            (id, world.body(id).unwrap().pos(), 0, 0),
            mind,
        );
        assert_eq!(
            steps(&news),
            [ProblemStep::Began {
                heard: false,
                later: true
            }]
        );
        assert!(brains.minds[&id].trial.is_some_and(|t| t.problem.is_some()));
        // An empty list costs nothing.
        let (rules, world, mut brains, id) = valley();
        willing(&mut brains, id, 100);
        let mind = brains.minds.get_mut(&id).unwrap();
        let news = free_moment(
            &world,
            &rules,
            (&brains.memory, 0),
            (id, world.body(id).unwrap().pos(), 0, 0),
            mind,
        );
        assert_eq!(news.len(), 0);
        assert!(brains.minds[&id].trial.is_none());
    }

    #[test]
    fn with_nothing_new_around_the_problem_goes_back_below_the_top() {
        let (mut rules, mut world, mut brains, id) = valley();
        willing(&mut brains, id, 100);
        arms_full(&mut world, &rules, id);
        // What lies around is known to its hands: nothing new here.
        let here = world.body(id).unwrap().pos();
        let index = |id: &str| {
            u16::try_from(rules.materials.iter().position(|m| m.id == id).unwrap()).unwrap()
        };
        let near: Vec<u16> = world
            .things_near(here, 4, |_| true)
            .iter()
            .map(|t| index(t.2))
            .collect();
        assert!(!near.contains(&index("birch_bark")));
        for on in near {
            brains.minds.get_mut(&id).unwrap().ways.hands.add(Deed {
                gesture: Gesture::PickUp,
                on,
                with: NOTHING,
            });
        }
        stopped(&mut world, &rules, &mut brains, id);
        assert!(brains.minds[&id].trial.is_none(), "not spent untried");
        assert_eq!(
            steps(&brains.news()),
            [ProblemStep::Lived, ProblemStep::Waited]
        );
        // A second kind on the list, under the first: with nothing new,
        // the top goes back below it.
        let second = rules.needs.problems.kinds[0].clone();
        rules.needs.problems.kinds.push(second);
        let mind = brains.minds.get_mut(&id).unwrap();
        push(&world, &rules, mind, 1, (here, 0));
        push(&world, &rules, mind, 0, (here, 0));
        let kinds = |mind: &Mind| mind.problems.iter().map(|p| p.kind).collect::<Vec<_>>();
        assert_eq!(kinds(mind), [0, 1]);
        let news = free_moment(&world, &rules, (&brains.memory, 0), (id, here, 0, 0), mind);
        assert_eq!(news.len(), 0);
        assert_eq!(kinds(mind), [1, 0]);
        // Bark laid here: the same place the same day it does not look
        // again; another day, at a free moment, it begins.
        world.spawn_item("birch_bark", 200, here);
        let mind = brains.minds.get_mut(&id).unwrap();
        let news = free_moment(&world, &rules, (&brains.memory, 0), (id, here, 0, 0), mind);
        assert_eq!(news.len(), 0);
        let news = free_moment(&world, &rules, (&brains.memory, 0), (id, here, 1, 0), mind);
        assert!(matches!(
            steps(&news)[..],
            [ProblemStep::Began { later: true, .. }]
        ));
    }

    #[test]
    fn the_things_to_think_about_are_a_short_list_the_last_lived_on_top() {
        let (mut rules, world, mut brains, id) = valley();
        rules.needs.problems.mind_slots = 2;
        for _ in 0..2 {
            let kind = rules.needs.problems.kinds[0].clone();
            rules.needs.problems.kinds.push(kind);
        }
        let here = world.body(id).unwrap().pos();
        let mind = brains.minds.get_mut(&id).unwrap();
        let list = |mind: &Mind| {
            mind.problems
                .iter()
                .map(|p| (p.kind, p.count))
                .collect::<Vec<_>>()
        };
        push(&world, &rules, mind, 0, (here, 0));
        push(&world, &rules, mind, 1, (here, 0));
        assert_eq!(list(mind), [(1, 1), (0, 1)]);
        push(&world, &rules, mind, 0, (here, 1));
        assert_eq!(list(mind), [(0, 2), (1, 1)], "moved up, no double");
        push(&world, &rules, mind, 2, (here, 1));
        assert_eq!(list(mind), [(2, 1), (0, 2)], "the oldest dropped off");
        // Faded entries go when the list is touched.
        let cycle = world.calendar().days_per_cycle();
        let keep = u64::from(rules.needs.problems.fade_days(cycle));
        push(&world, &rules, mind, 2, (here, keep + 1));
        assert_eq!(list(mind), [(2, 1)], "its own faded entry starts anew");
    }

    #[test]
    fn a_listener_living_the_problem_tries_the_deed_told_with_it_first() {
        let (rules, mut world, mut brains, id) = valley();
        let deed = fold_bark(&rules);
        let mut theirs = Ways::at(Pos::new(5, 5));
        theirs.hands = Deeds::default();
        theirs.hands.add(deed);
        let (record, _) = brains.memory.intern(Record::Trial {
            body: BodyId(99),
            day: 0,
            way: Way::Hands,
            after: theirs,
            up: true,
            kept: true,
            will: 50,
            before: Lately::default(),
            during: Lately::default(),
            problem: Some(0),
        });
        willing(&mut brains, id, 0);
        brains.minds.get_mut(&id).unwrap().heard_ways.push(Access {
            record,
            by: BodyId(99),
            day: 0,
        });
        flint_here(&mut world, id);
        // Without the problem, no reason to try it.
        assert_eq!(
            handwork::heard_deeds(&brains.memory, &brains.minds[&id]).len(),
            0
        );
        // Living it, the deed is heard as any deed told.
        arms_full(&mut world, &rules, id);
        stopped(&mut world, &rules, &mut brains, id);
        let heard = handwork::heard_deeds(&brains.memory, &brains.minds[&id]);
        assert_eq!(heard, [(deed, 1)]);
        // Willing, it tries it first, there and then.
        willing(&mut brains, id, 100);
        stopped(&mut world, &rules, &mut brains, id);
        let mind = &brains.minds[&id];
        assert!(mind.trial.is_some_and(|t| t.problem.is_some()));
        assert!(mind.handwork.aim.contains(deed), "the deed told, first");
        assert!(steps(&brains.news()).contains(&ProblemStep::Began {
            heard: true,
            later: false
        }));
    }

    #[test]
    fn a_trial_from_a_problem_never_back_lapses_unjudged_when_its_time_is_out() {
        let (rules, mut world, mut brains, id) = valley();
        willing(&mut brains, id, 100);
        flint_here(&mut world, id);
        arms_full(&mut world, &rules, id);
        stopped(&mut world, &rules, &mut brains, id);
        let until = brains.minds[&id].trial.unwrap().problem.unwrap().until;
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.gestured = true;
        mind.handwork.see(fold_bark(&rules), 32);
        let news = night(&world, &mut brains, &rules, id, until);
        let mind = &brains.minds[&id];
        assert!(mind.trial.is_none() && mind.judged.is_none());
        assert!(mind.ways.hands.is_empty(), "its ways as before");
        assert_eq!(steps(&news), [ProblemStep::Lapsed]);
    }

    #[test]
    fn a_trial_from_a_problem_whose_chain_served_is_kept_at_whichever_ending() {
        // One verdict (28 November 2026): a trial of the hands is kept
        // if and only if a chain of it served its lived problem. Before,
        // a trial whose time ran out was given up or lapsed even when
        // its hollow had carried food beyond its arms, its will lowered,
        // the tale told a failure, and the chain kept all the same as no
        // trial of its own.
        let put = put_berries(&rules_of());
        for returns in [0, 3] {
            let (rules, mut world, mut brains, id) = valley();
            willing(&mut brains, id, 100);
            flint_here(&mut world, id);
            arms_full(&mut world, &rules, id);
            stopped(&mut world, &rules, &mut brains, id);
            for _ in 0..returns {
                stopped(&mut world, &rules, &mut brains, id);
            }
            let until = brains.minds[&id].trial.unwrap().problem.unwrap().until;
            let mind = brains.minds.get_mut(&id).unwrap();
            mind.handwork.gestured = true;
            mind.handwork.see(put, 32);
            mind.handwork.served.push(put.with);
            let mut taught = mind.clone();
            taught.learn_will(&rules, true);
            let _ = brains.news();
            let news = night(&world, &mut brains, &rules, id, until);
            let mind = &brains.minds[&id];
            assert!(mind.trial.is_none());
            assert!(mind.ways.hands.contains(put), "returns {returns}");
            assert_eq!(mind.handwork.kept, [put], "its own");
            assert!(mind.judged.is_some_and(|j| j.kept && j.problem == Some(0)));
            assert_eq!(
                (mind.will, mind.will_rest, mind.will_failures),
                (taught.will, taught.will_rest, taught.will_failures),
                "a success learnt"
            );
            assert_eq!(steps(&news), [ProblemStep::Kept], "returns {returns}");
            assert!(
                news.iter()
                    .any(|n| matches!(n, News::Tried { kept: true, .. }))
            );
        }
    }

    fn rules_of() -> Content {
        valley().0
    }
}
