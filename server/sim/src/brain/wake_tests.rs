//! The audit of wakes (26 November 2026): each switch off keeps the rule
//! before; on, a wake for nothing, or a knowledge without looking, goes.

use super::{Brains, Doing};
use crate::content::Content;
use crate::map::{Map, Terrain};
use crate::{BodyId, Calendar, Event, Pos, World, WorkEnd};

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
