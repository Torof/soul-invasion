//! What the simulation costs at scale (D39): measures, not rules. Each is
//! ignored and run by hand in release, and prints what it found:
//! `cargo test --release -p sim --test scale -- --ignored --nocapture`.
//!
//! The quickest of a few runs is kept, against the noise of a busy machine.

use std::time::{Duration, Instant};

use sim::{BodyId, Calendar, Content, Event, Intention, ItemId, Map, Pos, Terrain, World};

/// A quiet calendar: a thousand ticks a second, so nothing tires or
/// hungers over the few ticks measured.
fn calendar() -> Calendar {
    Calendar::new(10, 1_000_000).expect("a valid calendar")
}

/// The quickest of `runs` timings of `run`.
fn quickest(runs: usize, mut run: impl FnMut()) -> Duration {
    (0..runs)
        .map(|_| {
            let started = Instant::now();
            run();
            started.elapsed()
        })
        .min()
        .expect("at least one run")
}

/// A field of grass with `bodies` bodies standing one tile apart in rows.
fn field(side: i32, bodies: u32) -> World {
    let map = Map::filled(
        u32::try_from(side).expect("small"),
        u32::try_from(side).expect("small"),
        Terrain::Grass,
    );
    let mut world = World::empty(calendar(), map);
    for n in 0..i32::try_from(bodies).expect("few bodies") {
        world.spawn_body(Pos::new(n % side, n / side));
    }
    world
}

/// Finding a body by its id, for the last of many: the cost of every act
/// and of every event's bookkeeping.
#[test]
#[ignore = "a measure, run by hand in release"]
fn body_lookups_by_id() {
    for bodies in [1_000, 20_000] {
        let world = field(200, bodies);
        let last = BodyId(bodies);
        let lookups = 100_000;
        let took = quickest(3, || {
            for _ in 0..lookups {
                assert!(world.body(last).is_some());
            }
        });
        println!(
            "{bodies} bodies: a lookup of the last one {:?}",
            took / lookups
        );
    }
}

/// A tick with many bodies walking, each carrying three items among many
/// stored ones: the cost of knowing each walker's load every tick.
#[test]
#[ignore = "a measure, run by hand in release"]
fn a_tick_of_loaded_walkers_among_stored_items() {
    let rules = Content::sample();
    let side = 200;
    let walkers = 1_000;
    for stored in [0, 10_000] {
        let mut world = field(side, walkers);
        // The stored items far away first, then three stones under each
        // walker's feet: a scan for a walker's stone passes them all.
        for n in 0..stored {
            world.spawn_item("flint", Pos::new(n % side, 100 + n / side));
        }
        let mut under_feet = Vec::new();
        for n in 0..i32::try_from(walkers).expect("few bodies") {
            let at = Pos::new(n % side, n / side);
            under_feet.push([
                world.spawn_item("flint", at),
                world.spawn_item("flint", at),
                world.spawn_item("flint", at),
            ]);
        }
        let mut events = Vec::new();
        for pick in 0..3 {
            for (n, items) in under_feet.iter().enumerate() {
                world.queue(Intention::PickUp {
                    body: BodyId(u32::try_from(n + 1).expect("few bodies")),
                    item: items[pick],
                });
            }
            world.step(&rules, &mut events);
        }
        let carried = world
            .bodies()
            .iter()
            .filter(|b| b.inventory().count() == 3)
            .count();
        assert_eq!(carried, walkers as usize, "every walker carries three");
        // Everyone sets off southwards (over as many ticks as the searches
        // a tick allow), then walks for a while.
        for n in 0..i32::try_from(walkers).expect("few bodies") {
            world.queue(Intention::MoveTo {
                body: BodyId(u32::try_from(n + 1).expect("few bodies")),
                target: Pos::new(n % side, 60 + n / side),
            });
        }
        while !world.bodies().iter().all(sim::Body::is_walking) {
            events.clear();
            world.step(&rules, &mut events);
        }
        let ticks = 20;
        let started = Instant::now();
        for _ in 0..ticks {
            events.clear();
            world.step(&rules, &mut events);
        }
        let per_tick = started.elapsed() / ticks;
        assert!(world.bodies().iter().all(sim::Body::is_walking));
        println!("{walkers} loaded walkers, {stored} other stored items: a tick {per_tick:?}");
    }
}

/// An act on a natural item: the ground's own items of its chunk must
/// be known for it to become a stored one.
#[test]
#[ignore = "a measure, run by hand in release"]
fn acts_on_natural_items() {
    let rules = Content::sample();
    let mut world = World::with_valley(calendar(), &rules, 768);
    let body = world.bodies()[0].id();
    let at = world.bodies()[0].pos().chunk();
    // Natural items of the chunks around the camp, each acted on once
    // (an act on an item already taken costs nothing to measure).
    let natural: Vec<ItemId> = (-2..=2)
        .flat_map(|dy| (-2..=2).map(move |dx| sim::ChunkPos::new(at.x + dx, at.y + dy)))
        .flat_map(|c| world.items_in(c))
        .filter(|i| sim::items::is_natural(i.id()))
        .map(|i| i.id())
        .take(200)
        .collect();
    assert_eq!(natural.len(), 200, "the camp's chunks have natural items");
    let mut events = Vec::new();
    let started = Instant::now();
    for item in &natural {
        // Out of reach: refused once the item is made a stored one.
        world.queue(Intention::Hold { body, item: *item });
        world.step(&rules, &mut events);
    }
    let took = started.elapsed();
    assert!(events.iter().any(|e| matches!(e, Event::ActRefused { .. })));
    assert_eq!(world.items().len(), 200, "each is stored once taken");
    println!(
        "an act on a natural item not yet touched: {:?}",
        took / u32::try_from(natural.len()).expect("few")
    );
}

/// A tick in which many bodies ask for a path nothing can give: the
/// worst a tick costs when players click across the water.
#[test]
#[ignore = "a measure, run by hand in release"]
fn a_tick_of_refused_searches() {
    let rules = Content::sample();
    let mut world = World::with_valley(calendar(), &rules, 1536);
    let camp = world.map().camp();
    let cost = |t: Terrain| rules.movement.cost(t);
    let walkable = |p: Pos| world.map().get(p).is_some_and(|t| cost(t).is_some());
    // The nearest walkable tile the camp is not joined to within the
    // search's reach, ring by ring (each probe is itself a search).
    let target = (200..288)
        .flat_map(|r| {
            (-r..=r).flat_map(move |d| {
                [(d, -r), (d, r), (-r, d), (r, d)]
                    .map(|(dx, dy)| Pos::new(camp.x + dx, camp.y + dy))
            })
        })
        .filter(|&p| walkable(p))
        .find(|&p| world.map().path(camp, p, cost).is_none())
        .expect("a bank with no ford in reach");
    let bodies = 20;
    let mut spawned = Vec::new();
    for n in 0..bodies {
        let at = world
            .map()
            .nearest(Pos::new(camp.x + n, camp.y), Terrain::Grass)
            .expect("grass by the camp");
        spawned.push(world.spawn_body(at));
    }
    let mut events = Vec::new();
    let took = quickest(2, || {
        for body in &spawned {
            world.queue(Intention::MoveTo {
                body: *body,
                target,
            });
        }
        events.clear();
        world.step(&rules, &mut events);
    });
    let walks = events
        .iter()
        .filter(|e| matches!(e, Event::BodyWalks { .. }))
        .count();
    println!(
        "{bodies} searches refused toward {target:?} ({} tiles from the camp) in one tick: {took:?} ({walks} walk straight)",
        (target.x - camp.x).abs().max((target.y - camp.y).abs())
    );
}
