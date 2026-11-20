//! The world's invariants: what must hold whatever the world did (D35's
//! "an item in exactly one place", D41's "invariants each simulated
//! year"). The scenario tests ask for the breaches between their days;
//! the product never calls this, so its cost is the tests' alone.
//!
//! Checked:
//! - every body stands inside the map, on ground that can be walked, is
//!   alive (the dead are taken out of the world), and is listed in the
//!   chunk it stands in;
//! - the load a body keeps is the mass of what it carries, nothing is
//!   both in its hand and in its load, and an infant's carrier exists;
//! - every item is in exactly one place: carried by the one body whose
//!   inventory holds it, or lying on one tile of the map and listed in
//!   that tile's chunk, and never both;
//! - the chunks' ground lists only items that lie there.
//!
//! The needs are gauges of at most 100 points by construction
//! ([`crate::Gauge`]) and need no check here.

use std::collections::BTreeMap;

use crate::content::Content;
use crate::{BodyId, ItemId, Place, World};

/// The invariants broken in `world`, one sentence each; empty when the
/// world is sound.
#[must_use]
pub fn breaches(world: &World, rules: &Content) -> Vec<String> {
    let mut out = Vec::new();
    // Who carries each item, by the bodies' word.
    let mut holders: BTreeMap<ItemId, BodyId> = BTreeMap::new();
    bodies(world, rules, &mut holders, &mut out);
    items(world, &holders, &mut out);
    out
}

/// The bodies' breaches; `holders` learns who carries what.
fn bodies(
    world: &World,
    rules: &Content,
    holders: &mut BTreeMap<ItemId, BodyId>,
    out: &mut Vec<String>,
) {
    for body in &world.bodies {
        let id = body.id.0;
        let pos = body.pos();
        match world.map.get(pos) {
            None => out.push(format!("body {id} stands outside the map at {pos:?}")),
            Some(terrain) if rules.movement.cost(terrain).is_none() => out.push(format!(
                "body {id} stands on {terrain:?} at {pos:?}, which cannot be walked"
            )),
            Some(_) => {}
        }
        if body.condition.vitality.points() == 0 {
            out.push(format!("body {id} is dead and still in the world"));
        }
        let chunk = pos.chunk();
        if world
            .by_chunk
            .get(&chunk)
            .is_none_or(|ids| ids.binary_search(&body.id).is_err())
        {
            out.push(format!("body {id} is not listed in its chunk {chunk:?}"));
        }
        if let Some(held) = body.held
            && body.carried.contains(&held)
        {
            out.push(format!(
                "body {id} has item {} both in hand and in its load",
                held.0
            ));
        }
        if let Some(carrier) = body.carried_by
            && world.body(carrier).is_none()
        {
            out.push(format!(
                "body {id} is carried by body {}, which is gone",
                carrier.0
            ));
        }
        let mut load: u32 = 0;
        for item in body.inventory() {
            match world.items.get(&item) {
                None => out.push(format!(
                    "body {id} carries item {}, which does not exist",
                    item.0
                )),
                Some(it) => {
                    load = load.saturating_add(it.mass_g);
                    if it.place != (Place::Carried { body: body.id }) {
                        out.push(format!(
                            "body {id} carries item {} whose place is {:?}",
                            item.0, it.place
                        ));
                    }
                }
            }
            if let Some(other) = holders.insert(item, body.id) {
                out.push(format!(
                    "item {} is carried by both body {} and body {id}",
                    item.0, other.0
                ));
            }
        }
        if load != body.load_g {
            out.push(format!(
                "body {id} keeps a load of {} g but carries {load} g",
                body.load_g
            ));
        }
    }
}

/// The items' breaches, against who the bodies say carries them.
fn items(world: &World, holders: &BTreeMap<ItemId, BodyId>, out: &mut Vec<String>) {
    for (id, item) in &world.items {
        match item.place {
            Place::Carried { body } => {
                if holders.get(id) != Some(&body) {
                    out.push(format!(
                        "item {} says body {} carries it, which does not",
                        id.0, body.0
                    ));
                }
            }
            Place::Ground { pos } => {
                if let Some(body) = holders.get(id) {
                    out.push(format!(
                        "item {} lies at {pos:?} and is carried by body {}",
                        id.0, body.0
                    ));
                }
                if world.map.get(pos).is_none() {
                    out.push(format!("item {} lies outside the map at {pos:?}", id.0));
                }
                if !world
                    .ground
                    .get(&pos.chunk())
                    .is_some_and(|ids| ids.contains(id))
                {
                    out.push(format!(
                        "item {} lies at {pos:?} and is not listed in its chunk",
                        id.0
                    ));
                }
            }
        }
    }
    for (chunk, ids) in &world.ground {
        for id in ids {
            match world.items.get(id) {
                Some(item) if matches!(item.place, Place::Ground { pos } if pos.chunk() == *chunk) =>
                    {}
                Some(item) => out.push(format!(
                    "chunk {chunk:?} lists item {} on its ground, whose place is {:?}",
                    id.0, item.place
                )),
                None => out.push(format!(
                    "chunk {chunk:?} lists item {} on its ground, which does not exist",
                    id.0
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::Brains;
    use crate::crops::tests::{body_by, run_to, valley_with};
    use crate::map::Pos;
    use crate::{Event, Gauge, Intention};

    /// The valley with a bramble, a body born beside it and piloted by
    /// the brain, hungry enough to gather and eat.
    fn a_piloted_body() -> (World, Content, BodyId, Brains) {
        let (mut world, rules, _, bramble) = valley_with("bramble");
        run_to(&mut world, &rules, 28);
        let id = body_by(&mut world, &bramble);
        let body = world.bodies.iter_mut().find(|b| b.id() == id).unwrap();
        body.condition.deficit_j = 60 * u64::from(rules.needs.kj_per_hunger_point) * 1000;
        body.condition.hunger = Gauge::at(60);
        let mut brains = Brains::new(7);
        brains.adopt(&world, &rules, id);
        (world, rules, id, brains)
    }

    /// A scenario run accelerated: a piloted body gathers, eats, drinks,
    /// walks and sleeps for three days on the valley, and the world holds
    /// its invariants at every tick of it.
    #[test]
    fn a_piloted_body_on_the_valley_breaks_no_invariant_in_three_days() {
        let (mut world, rules, id, mut brains) = a_piloted_body();
        let ticks = 3 * world.calendar().ticks_per_day();
        let mut events = Vec::new();
        let (mut ate, mut picked) = (false, false);
        for tick in 0..ticks {
            events.clear();
            world.step(&rules, &mut events);
            for (_, decision) in brains.think(&world, &rules, &events) {
                world.queue(decision.intention);
            }
            ate |= events
                .iter()
                .any(|e| matches!(e, Event::BodyAte { body, .. } if *body == id));
            picked |= events
                .iter()
                .any(|e| matches!(e, Event::WorkStopped { body, grams, .. } if *body == id && *grams > 0));
            let found = breaches(&world, &rules);
            assert!(found.is_empty(), "tick {tick}: {found:?}");
        }
        assert!(
            picked && ate,
            "the scenario did something: picked {picked}, ate {ate}"
        );
    }

    #[test]
    fn a_sound_world_with_items_laid_and_carried_has_no_breach() {
        let (mut world, rules, id, _) = a_piloted_body();
        let pos = world.body(id).unwrap().pos();
        let laid = world.spawn_item("flint", 400, pos);
        let taken = world.spawn_item("flint", 400, pos);
        world.queue(Intention::PickUp {
            body: id,
            item: taken,
        });
        let mut events = Vec::new();
        world.step(&rules, &mut events);
        assert!(world.body(id).unwrap().has(taken), "{events:?}");
        assert_eq!(world.item(laid).unwrap().place(), Place::Ground { pos });
        assert_eq!(breaches(&world, &rules), Vec::<String>::new());
    }

    #[test]
    fn a_body_off_the_map_or_on_the_river_is_a_breach() {
        let (mut world, rules, id, _) = a_piloted_body();
        let map = world.map();
        let water = (0..map.height())
            .flat_map(|y| (0..map.width()).map(move |x| (x, y)))
            .map(|(x, y)| Pos::new(i32::try_from(x).unwrap(), i32::try_from(y).unwrap()))
            .find(|&p| rules.movement.cost(map.get(p).unwrap()).is_none())
            .expect("the valley has a river");
        let body = world.bodies.iter_mut().find(|b| b.id() == id).unwrap();
        body.at = water.centre();
        let found = breaches(&world, &rules);
        assert!(
            found.iter().any(|b| b.contains("cannot be walked")),
            "{found:?}"
        );
        let body = world.bodies.iter_mut().find(|b| b.id() == id).unwrap();
        body.at = Pos::new(-5, -5).centre();
        let found = breaches(&world, &rules);
        assert!(
            found.iter().any(|b| b.contains("outside the map")),
            "{found:?}"
        );
    }

    #[test]
    fn an_item_in_two_places_or_a_wrong_load_is_a_breach() {
        let (mut world, rules, id, _) = a_piloted_body();
        let pos = world.body(id).unwrap().pos();
        let item = world.spawn_item("flint", 400, pos);
        // Lying on the ground, and in the body's load too: two places.
        let body = world.bodies.iter_mut().find(|b| b.id() == id).unwrap();
        body.carried.push(item);
        let found = breaches(&world, &rules);
        assert!(
            found.iter().any(|b| b.contains("whose place is")),
            "{found:?}"
        );
        assert!(
            found.iter().any(|b| b.contains("is carried by body")),
            "{found:?}"
        );
        assert!(
            found.iter().any(|b| b.contains("keeps a load of")),
            "the load was not counted: {found:?}"
        );
    }
}
