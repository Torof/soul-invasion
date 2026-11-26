//! The audit of wakes (26 November 2026): each switch off keeps the rule
//! before; on, a wake for nothing, or a knowledge without looking, goes.

use super::{Brains, Doing, Glance, Group, glance};
use crate::content::{Content, Decisions};
use crate::map::{Map, Terrain};
use crate::{BodyId, Calendar, Event, Intention, Pos, WorkEnd, World};
use std::collections::{BTreeMap, BTreeSet};

/// A field of grass 64 tiles a side, the camp at (5, 5), `n` bodies
/// spawned along its sixth row, all piloted.
fn field(n: i32) -> (World, Content, Brains) {
    let map = Map::filled(64, 64, Terrain::Grass).with_camp(5, 5);
    let mut world = World::empty(Calendar::new(1, 7200).unwrap(), map);
    for x in 0..n {
        world.spawn_body(Pos::new(5 + x, 6));
    }
    let rules = Content::sample();
    let mut brains = Brains::new(7);
    for body in world.bodies() {
        brains.adopt(&world, &rules, body.id());
    }
    for mind in brains.minds.values_mut() {
        mind.due = false;
        mind.wake_at = None;
    }
    (world, rules, brains)
}

/// A gather stopped by the body's own new intention (step 1).
fn interrupted(body: BodyId) -> Event {
    Event::WorkStopped {
        body,
        gives: "berries".to_owned(),
        grams: 0,
        why: WorkEnd::Interrupted,
    }
}

#[test]
fn the_echo_of_an_order_wakes_it_only_under_the_rule_before() {
    let (world, mut rules, mut brains) = field(1);
    let id = world.bodies()[0].id();
    brains.notice(&[interrupted(id)], &world, &rules);
    assert!(brains.minds[&id].due, "the rule before: it thinks again");
    brains.minds.get_mut(&id).unwrap().due = false;
    rules.needs.thought.wake_on_interrupt = false;
    brains.notice(&[interrupted(id)], &world, &rules);
    assert!(!brains.minds[&id].due, "its own order's echo wakes nothing");
    // A gather that ends on its own still does.
    let bare = Event::WorkStopped {
        body: id,
        gives: "berries".to_owned(),
        grams: 100,
        why: WorkEnd::Bare,
    };
    brains.notice(&[bare], &world, &rules);
    assert!(brains.minds[&id].due);
    assert_eq!(brains.minds[&id].doing, Doing::Nothing);
}

#[test]
fn a_sleeper_thinks_neither_falling_asleep_nor_at_dawn_but_when_it_wakes() {
    for wake_asleep in [true, false] {
        let (mut world, mut rules, mut brains) = field(1);
        rules.needs.thought.wake_asleep = wake_asleep;
        let id = world.bodies()[0].id();
        let now = world.tick().0;
        brains.minds.get_mut(&id).unwrap().doing = Doing::Home;
        // It falls asleep: a gauge past its threshold.
        world.bodies[0].condition.asleep = true;
        let due = brains.sweep_minds(&world, &rules, now);
        let mind = &brains.minds[&id];
        let woken = !due.is_empty() || mind.wake_at.is_some();
        assert_eq!(
            woken, wake_asleep,
            "falling asleep, rule before {wake_asleep}"
        );
        if !wake_asleep {
            assert_eq!(mind.doing, Doing::Nothing, "it dropped what it was about");
        }
        // The dawn, and an event, while it sleeps on.
        brains.minds.get_mut(&id).unwrap().wake_at = None;
        brains.notice(&[Event::DayStarted { day: 2 }], &world, &rules);
        let stopped = Event::BodyStopped {
            body: id,
            at: world.bodies()[0].at(),
        };
        brains.notice(&[stopped], &world, &rules);
        let mind = &brains.minds[&id];
        assert_eq!(mind.wake_at.is_some() || mind.due, wake_asleep, "the dawn");
        assert_eq!(mind.stopped, wake_asleep);
        // It wakes, its sleep done: it thinks, under both rules.
        world.bodies[0].condition.asleep = false;
        let mind = brains.minds.get_mut(&id).unwrap();
        (mind.due, mind.wake_at) = (false, None);
        let due = brains.sweep_minds(&world, &rules, now);
        let mind = &brains.minds[&id];
        assert!(!due.is_empty() || mind.wake_at.is_some(), "it gets up");
    }
}

/// A group of `members` called by the first, set off or not.
fn group(members: Vec<BodyId>, started: bool) -> Group {
    Group {
        day: 0,
        called: (0, 0),
        place: (0, 0),
        members,
        taken: BTreeMap::new(),
        full: BTreeSet::new(),
        followed: BTreeSet::new(),
        left: BTreeSet::new(),
        arrived: BTreeMap::new(),
        dawn: 0,
        home: false,
        ended: None,
        held: false,
        waited: 0,
        started,
        waited_for: BTreeSet::new(),
        moves: None,
    }
}

/// Whether `id`'s glance makes it due now.
fn glanced(world: &World, rules: &Content, brains: &mut Brains, id: BodyId, watch: Glance) -> bool {
    let body = world.body(id).unwrap();
    let mind = brains.minds.get_mut(&id).unwrap();
    (mind.due, mind.glance) = (false, Some(watch));
    glance(
        world,
        rules,
        &brains.groups,
        (id, body),
        mind,
        world.tick().0,
    );
    mind.due
}

/// One tick with `body` walking to `to`.
fn walk(world: &mut World, rules: &Content, body: BodyId, to: Pos) {
    world.queue(Intention::MoveTo { body, target: to });
    world.step(rules, &mut Vec::new());
}

#[test]
fn the_caller_is_woken_by_the_member_it_waits_for_and_glances_behind_without_a_thought() {
    let (mut world, mut rules, mut brains) = field(2);
    rules.needs.decisions = Decisions {
        behind_glance: true,
        ..Decisions::placeholders()
    };
    let (lead, member) = (world.bodies()[0].id(), world.bodies()[1].id());
    brains.groups.insert(lead, group(vec![lead, member], false));
    // At dawn the caller waits while its member sleeps.
    world.bodies[1].condition.asleep = true;
    assert!(!glanced(&world, &rules, &mut brains, lead, Glance::Members));
    world.bodies[1].condition.asleep = false;
    assert!(
        glanced(&world, &rules, &mut brains, lead, Glance::Members),
        "it woke"
    );
    // The member waits by its caller until the caller sets off.
    assert!(!glanced(
        &world,
        &rules,
        &mut brains,
        member,
        Glance::Caller(lead)
    ));
    walk(&mut world, &rules, lead, Pos::new(40, 6));
    assert!(glanced(
        &world,
        &rules,
        &mut brains,
        member,
        Glance::Caller(lead)
    ));
    // On its way, nobody behind: a glance, no thought, another glance later.
    brains.groups.get_mut(&lead).unwrap().started = true;
    let now = world.tick().0;
    assert!(!glanced(
        &world,
        &rules,
        &mut brains,
        lead,
        Glance::Behind(now)
    ));
    assert!(matches!(brains.minds[&lead].glance, Some(Glance::Behind(at)) if at > now));
    // Too early: nothing looked at.
    assert!(!glanced(
        &world,
        &rules,
        &mut brains,
        lead,
        Glance::Behind(now + 1)
    ));
    // A member it sees walking behind, out of reach: a thought.
    for _ in 0..400 {
        walk(&mut world, &rules, lead, Pos::new(60, 6));
    }
    walk(&mut world, &rules, member, Pos::new(6, 6));
    let (a, b) = (
        world.body(lead).unwrap().pos(),
        world.body(member).unwrap().pos(),
    );
    assert!((a.x - b.x).abs() > 24, "{a:?} {b:?}");
    let now = world.tick().0;
    assert!(glanced(
        &world,
        &rules,
        &mut brains,
        lead,
        Glance::Behind(now)
    ));
}
