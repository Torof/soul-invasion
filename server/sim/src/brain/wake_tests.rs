//! The audit of wakes (26 November 2026): each switch off keeps the rule
//! before; on, a wake for nothing, or a knowledge without looking, goes.

use super::{Brains, Doing};
use crate::content::Content;
use crate::map::{Map, Terrain};
use crate::{BodyId, Calendar, Event, Pos, WorkEnd, World};

/// A field of grass 32 tiles a side, the camp at (5, 5), `n` bodies
/// spawned along its sixth row, all piloted.
fn field(n: i32) -> (World, Content, Brains) {
    let map = Map::filled(32, 32, Terrain::Grass).with_camp(5, 5);
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
