//! What a tick costs with a crowd walking on the game's valley (D32, D39):
//! a measure, not a rule. Ignored, run by hand in release, one size at a
//! time so each run's memory is its own:
//! `cargo test --release -p sim --test crowd -- --ignored --nocapture crowd_of_1000`.
//!
//! Every body walks to a new random place 20 to 200 tiles away whenever
//! it stops, half the time into the forest, half onto open ground, for
//! three game minutes. The walking is the world's own; beside it, this
//! file keeps a mirror of what `World::step` searches each tick (the
//! orders under the cap, then the bodies looking again), checked against
//! the events, and runs each of those searches again outside the timed
//! step to time it alone.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use sim::map::line_reach;
use sim::{
    BodyId, Calendar, Content, Event, FinePos, Intention, MM_PER_TILE, Map, Pos, Terrain,
    VALLEY_SEED, VALLEY_SIZE, World,
};

/// Game time measured: three minutes.
const GAME_SECONDS: u64 = 180;

/// Nearest and farthest a destination is drawn, in tiles.
const NEAR: i32 = 20;
const FAR: i32 = 200;

/// A small deterministic generator (splitmix64): no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `lo..=hi`.
    fn within(&mut self, lo: i32, hi: i32) -> i32 {
        let span = u64::try_from(hi - lo + 1).expect("a range");
        lo + i32::try_from(self.next() % span).expect("a small range")
    }
}

/// What the world searched for a body, as the mirror saw it.
struct Search {
    body: BodyId,
    from: FinePos,
    target: Pos,
    ordered: bool,
}

/// A body walking straight for want of a way, as `World` keeps it.
#[derive(Clone, Copy)]
struct Goal {
    target: Pos,
    looked_from: u64,
}

fn squared_mm(a: FinePos, b: FinePos) -> u64 {
    let (dx, dy) = (i64::from(a.x - b.x), i64::from(a.y - b.y));
    (dx * dx + dy * dy).unsigned_abs()
}

/// `World::sight_tiles_at`, which is private.
fn sight_at(map: &Map, rules: &Content, pos: Pos) -> u32 {
    let movement = &rules.movement;
    let own = movement.sight_tiles(map.get(pos).unwrap_or(Terrain::Rock));
    (-1..=1)
        .flat_map(|dy| (-1..=1).map(move |dx| Pos::new(pos.x + dx, pos.y + dy)))
        .filter_map(|p| map.get(p))
        .filter(|t| movement.cost(*t).is_some())
        .map(|t| movement.sight_tiles(t))
        .fold(own, u32::max)
}

/// What a search came to: the route it set off on (empty: none), the goal
/// it keeps, and whether an order was refused. Times `path_within` and
/// `pull` alone.
struct Outcome {
    route: Option<Vec<FinePos>>,
    goal: Option<Goal>,
    refused: bool,
    found: bool,
    took: Duration,
}

/// `World::set_off`, done again on the same map from the same place.
fn set_off(map: &Map, rules: &Content, from: FinePos, target: Pos, ordered: bool) -> Outcome {
    let cost = |t: Terrain| rules.movement.cost(t);
    let sight = i32::try_from(sight_at(map, rules, from.tile())).unwrap_or(i32::MAX);
    let budget = rules.movement.path_budget_tiles;
    let started = Instant::now();
    let path = map.path_within(from.tile(), target, sight, budget, cost);
    let route = path.map(|path| map.pull(from, &path, cost));
    let took = started.elapsed();
    if let Some(route) = route {
        let route = (!route.is_empty()).then_some(route);
        return Outcome {
            route,
            goal: None,
            refused: false,
            found: true,
            took,
        };
    }
    let walkable = |p: Pos| map.get(p).is_some_and(|t| cost(t).is_some());
    let end = line_reach(from, target.centre(), walkable);
    let (dx, dy) = (i64::from(end.x - from.x), i64::from(end.y - from.y));
    let quarter = i64::from(MM_PER_TILE / 4);
    if dx * dx + dy * dy < quarter * quarter {
        return Outcome {
            route: None,
            goal: None,
            refused: ordered,
            found: false,
            took,
        };
    }
    Outcome {
        route: Some(vec![end]),
        goal: Some(Goal {
            target,
            looked_from: squared_mm(from, target.centre()),
        }),
        refused: false,
        found: false,
        took,
    }
}

/// A walkable tile 20 to 200 tiles from `from`, in the forest or on open
/// ground as asked (any walkable one if none is found).
fn destination(map: &Map, rng: &mut Rng, from: Pos, forest: bool) -> Pos {
    let mut any = None;
    for _ in 0..400 {
        let (dx, dy) = (rng.within(-FAR, FAR), rng.within(-FAR, FAR));
        let d2 = dx * dx + dy * dy;
        if !(NEAR * NEAR..=FAR * FAR).contains(&d2) {
            continue;
        }
        let pos = Pos::new(from.x + dx, from.y + dy);
        let Some(t) = map.get(pos) else { continue };
        if !t.is_passable() {
            continue;
        }
        if (t == Terrain::Forest) == forest {
            return pos;
        }
        any.get_or_insert(pos);
    }
    any.unwrap_or(from)
}

/// Kilobytes of the process's memory now and at its peak (Linux).
fn memory_kb() -> (u64, u64) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |name: &str| {
        status
            .lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    (field("VmRSS:"), field("VmHWM:"))
}

fn percentile(sorted: &[Duration], percent: usize) -> Duration {
    sorted[(sorted.len() - 1) * percent / 100]
}

/// One tick as measured.
#[derive(Default, Clone, Copy)]
struct Tick {
    step: Duration,
    search: Duration,
    searches: u32,
    orders: u32,
    looked_again: u32,
    deferred: usize,
    walking: usize,
}

#[allow(clippy::too_many_lines)]
fn crowd(count: u32) {
    let mut rules = Content::sample();
    // No hunger, cold or sleep: nobody dies or lies down over the run.
    rules.needs.hunger_per_day = 0;
    rules.needs.cold_per_day_at_night = 0;
    rules.needs.fatigue_per_day_awake = 0;
    let cap = rules.movement.searches_per_tick;

    let made = Instant::now();
    let map = Map::valley(VALLEY_SIZE, VALLEY_SIZE, VALLEY_SEED);
    let mut world = World::empty(Calendar::default(), map);
    world.take_content(&rules);
    let made = made.elapsed();
    let (map_rss, _) = memory_kb();

    let mut rng = Rng(0x5eed_c0de ^ u64::from(count));
    let side = i32::try_from(VALLEY_SIZE).expect("small");
    let mut in_forest = 0;
    for _ in 0..count {
        let pos = loop {
            let pos = Pos::new(rng.within(0, side - 1), rng.within(0, side - 1));
            if world.map().get(pos).is_some_and(Terrain::is_passable) {
                break pos;
            }
        };
        in_forest += u32::from(world.map().get(pos) == Some(Terrain::Forest));
        world.spawn_body(pos);
    }

    let ticks = GAME_SECONDS * u64::from(world.calendar().ticks_per_second());
    let mut goals: BTreeMap<BodyId, Goal> = BTreeMap::new();
    let mut backlog: VecDeque<(BodyId, Pos)> = VecDeque::new();
    let mut ordered: BTreeSet<BodyId> = BTreeSet::new();
    let mut events = Vec::new();
    let mut measured = Vec::with_capacity(usize::try_from(ticks).expect("few ticks"));
    let (mut found, mut straight, mut refused, mut mismatches) = (0u64, 0u64, 0u64, 0u64);
    let mut forest_targets = 0u64;
    let mut orders_total = 0u64;
    let mut draw = 0u64;

    for _ in 0..ticks {
        // Whoever stands still with nowhere to go gets a new place.
        let idle: Vec<(BodyId, Pos)> = world
            .bodies()
            .iter()
            .filter(|b| {
                !b.is_walking() && !goals.contains_key(&b.id()) && !ordered.contains(&b.id())
            })
            .map(|b| (b.id(), b.pos()))
            .collect();
        for (id, pos) in idle {
            draw += 1;
            let forest = draw.is_multiple_of(2);
            let target = destination(world.map(), &mut rng, pos, forest);
            forest_targets += u64::from(world.map().get(target) == Some(Terrain::Forest));
            world.queue(Intention::MoveTo { body: id, target });
            backlog.push_back((id, target));
            ordered.insert(id);
        }

        // What the step will search: orders first, under the cap, then
        // the bodies that walked straight and stopped, in body order.
        let mut searches = Vec::new();
        while searches.len() < cap as usize {
            let Some((body, target)) = backlog.pop_front() else {
                break;
            };
            ordered.remove(&body);
            let from = world.body(body).expect("alive").at();
            searches.push(Search {
                body,
                from,
                target,
                ordered: true,
            });
        }
        let orders = u32::try_from(searches.len()).expect("few");
        let mut cheap_drops = Vec::new();
        for body in world.bodies() {
            if searches.len() + cheap_drops.len() >= cap as usize {
                break;
            }
            let Some(goal) = goals.get(&body.id()).copied() else {
                continue;
            };
            if body.is_walking() || body.condition().asleep {
                continue;
            }
            if squared_mm(body.at(), goal.target.centre()) >= goal.looked_from {
                cheap_drops.push(body.id());
            } else {
                searches.push(Search {
                    body: body.id(),
                    from: body.at(),
                    target: goal.target,
                    ordered: false,
                });
            }
        }
        let looked_again = u32::try_from(searches.len() + cheap_drops.len()).expect("few") - orders;

        let started = Instant::now();
        world.step(&rules, &mut events);
        let step = started.elapsed();

        // The same searches again, timed alone, checked against the events.
        let mut walks: BTreeMap<BodyId, Vec<FinePos>> = BTreeMap::new();
        let mut refusals = 0u64;
        for event in &events {
            match event {
                Event::BodyWalks { body, route, .. } => {
                    walks.insert(*body, route.clone());
                }
                Event::MoveRefused { .. } => refusals += 1,
                _ => {}
            }
        }
        events.clear();
        for id in cheap_drops {
            goals.remove(&id);
        }
        let mut search = Duration::ZERO;
        let mut predicted_refusals = 0u64;
        let mut predicted_walks = 0usize;
        for s in &searches {
            let out = set_off(world.map(), &rules, s.from, s.target, s.ordered);
            search += out.took;
            if out.found {
                found += 1;
            } else {
                straight += 1;
            }
            if out.refused {
                refused += 1;
                predicted_refusals += 1;
            }
            match out.goal {
                Some(goal) => goals.insert(s.body, goal),
                None => goals.remove(&s.body),
            };
            if let Some(route) = out.route {
                predicted_walks += 1;
                if walks.get(&s.body) != Some(&route) {
                    mismatches += 1;
                }
            }
        }
        if predicted_walks != walks.len() || predicted_refusals != refusals {
            mismatches += 1;
        }
        orders_total += u64::from(orders);
        measured.push(Tick {
            step,
            search,
            searches: u32::try_from(searches.len()).expect("few"),
            orders,
            looked_again,
            deferred: backlog.len(),
            walking: world.bodies().iter().filter(|b| b.is_walking()).count(),
        });
    }
    let (rss, peak) = memory_kb();

    let report = |label: &str, part: &[Tick]| {
        let mut steps: Vec<Duration> = part.iter().map(|t| t.step).collect();
        steps.sort_unstable();
        let total: Duration = steps.iter().sum();
        let searched: Duration = part.iter().map(|t| t.search).sum();
        let n = u32::try_from(part.len()).expect("few");
        let worst = part.iter().max_by_key(|t| t.step).expect("ticks");
        println!(
            "  {label}: {} ticks, step median {:?} p95 {:?} max {:?} mean {:?}; \
             search {:?} = {}% of step time; searches/tick mean {:.1} max {} \
             (orders {}, look-again {}); deferred max {} final {}; \
             walking mean {} min {}; worst tick: {} searches, {} deferred, search {:?}",
            part.len(),
            percentile(&steps, 50),
            percentile(&steps, 95),
            steps[steps.len() - 1],
            total / n,
            searched,
            searched.as_micros() * 100 / total.as_micros().max(1),
            f64::from(part.iter().map(|t| t.searches).sum::<u32>()) / f64::from(n),
            part.iter().map(|t| t.searches).max().unwrap_or(0),
            part.iter().map(|t| t.orders).sum::<u32>(),
            part.iter().map(|t| t.looked_again).sum::<u32>(),
            part.iter().map(|t| t.deferred).max().unwrap_or(0),
            part.last().map_or(0, |t| t.deferred),
            part.iter().map(|t| t.walking).sum::<usize>() / part.len(),
            part.iter().map(|t| t.walking).min().unwrap_or(0),
            worst.searches,
            worst.deferred,
            worst.search,
        );
    };
    println!(
        "{count} bodies ({in_forest} spawned in forest), valley {VALLEY_SIZE} made in {made:?}, \
         {ticks} ticks ({GAME_SECONDS} game s), cap {cap}"
    );
    report("whole run", &measured);
    let settle = usize::try_from(30 * u64::from(world.calendar().ticks_per_second())).expect("few");
    report("after 30 s", &measured[settle..]);
    println!(
        "  orders {orders_total} (forest targets {forest_targets}), searches: \
         {found} found, {straight} straight, {refused} refused; mirror mismatches {mismatches}"
    );
    println!(
        "  memory: {} MB with the map alone, {} MB at the end, peak {} MB",
        map_rss / 1024,
        rss / 1024,
        peak / 1024
    );
    assert_eq!(mismatches, 0, "the mirror follows the world's searches");
}

#[test]
#[ignore = "a measure, run by hand in release"]
fn crowd_of_1000() {
    crowd(1_000);
}

#[test]
#[ignore = "a measure, run by hand in release"]
fn crowd_of_5000() {
    crowd(5_000);
}
