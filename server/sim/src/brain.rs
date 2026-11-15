//! The minimal brain of the NPCs (D41, the vivarium's lot 1): a pilot
//! like a player's (D20), that asks its body for what the world already
//! knows how to do, and nothing more.
//!
//! A body with a mind **thinks only when something changes** for it: it
//! is born, a gauge crosses one of the thresholds below, it stops (it
//! arrived, or was stopped), something it asked is refused, it drank or
//! ate, night falls or the day starts. Between two thoughts it costs
//! nothing. When it thinks it first **looks**: from where it stands, as
//! far as the ground lets it see ([`crate::content::Movement`]'s sight,
//! the first version's placeholder until perception comes), it remembers
//! the places it may drink at and the food lying there (its **memory of
//! places**, kept small: the nearest drinking place a chunk, the food it
//! saw). Then it decides, in this order:
//!
//! 1. **Thirsty** (thirst at [`THIRSTY`] or more, or [`PARCHED`] while
//!    asleep, which wakes it): to the nearest drinking place it knows,
//!    and drinks there; knowing none, it goes looking (`explore`).
//! 2. **Hungry** (hunger at [`HUNGRY`] or more): to the nearest food it
//!    knows, and eats it; knowing none, it goes looking.
//! 3. **Night**: home to the camp, and sleeps within [`HOME_TILES`] of it.
//! 4. Otherwise, by day: home to the camp if it is away, and waits there.
//!
//! Every choice comes with its reason ([`Why`]), so the vivarium's report
//! can say why a body did what it did. Everything is deterministic: the
//! same world and the same seed give the same thoughts.

use std::collections::BTreeMap;

use crate::content::Content;
use crate::map::{ChunkPos, Map, Pos, Terrain};
use crate::{BodyId, Event, Intention, ItemId, World};

/// Thirst from which a body goes to drink: half a game day without
/// water (thirst rises 100 points a day), so it drinks before the night
/// and twice a day.
pub const THIRSTY: u32 = 50;
/// Thirst from which a sleeping body wakes to drink: it would not
/// otherwise last the night.
pub const PARCHED: u32 = 80;
/// Hunger from which a body goes to eat.
pub const HUNGRY: u32 = 50;
/// How near the camp a body counts as home, in tiles (a little more than
/// the ring the bodies are born in).
pub const HOME_TILES: i32 = 6;
/// How many drinking places and how many foods a body remembers: the
/// nearest to the camp are kept.
pub const MEMORY_PLACES: usize = 64;

/// Why a body was asked to do something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// Thirsty: it walks to a drinking place it knows, or drinks there.
    Thirst,
    /// Hungry: it walks to food it knows, or eats it.
    Hunger,
    /// Thirsty or hungry and knowing no place: it goes looking.
    Explore,
    /// Night: it walks home, or sleeps there.
    Night,
    /// Day, nothing pressing: it walks home.
    Home,
}

/// A thought: what a body was asked, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// What it was asked.
    pub intention: Intention,
    /// Why.
    pub why: Why,
}

/// A place a body may drink at: the tile it stands on, walkable, and the
/// water within reach of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrinkingPlace {
    /// Where to stand.
    pub stand: Pos,
    /// The water to drink, on `stand` or next to it.
    pub water: Pos,
}

/// What a body is about, between two thoughts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Doing {
    Nothing,
    ToDrink(DrinkingPlace),
    ToEat(ItemId, Pos),
    Exploring(Why),
    Home,
}

/// One body's mind: its home, its memory of places and what it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mind {
    /// Its camp: where it goes at night and by day, and goes looking from.
    home: Pos,
    /// The drinking places it knows, one a chunk.
    water: BTreeMap<ChunkPos, DrinkingPlace>,
    /// The food it saw, and where.
    food: BTreeMap<ItemId, Pos>,
    doing: Doing,
    /// Which thresholds its gauges were past when it last thought:
    /// thirsty, parched, hungry, asleep.
    past: (bool, bool, bool, bool),
    /// Something changed: it thinks at the next turn.
    due: bool,
    /// Its walk ended since it last thought (it arrived, or was stopped).
    stopped: bool,
    /// How many times it went looking: the next direction.
    walks: u64,
    /// How many times it thought.
    thoughts: u64,
}

impl Mind {
    fn new(home: Pos) -> Self {
        Self {
            home,
            water: BTreeMap::new(),
            food: BTreeMap::new(),
            doing: Doing::Nothing,
            past: (false, false, false, false),
            due: true,
            stopped: false,
            walks: 0,
            thoughts: 0,
        }
    }

    /// The drinking places it knows.
    pub fn drinking_places(&self) -> impl Iterator<Item = &DrinkingPlace> {
        self.water.values()
    }

    /// The food it knows.
    pub fn known_food(&self) -> impl Iterator<Item = (&ItemId, &Pos)> {
        self.food.iter()
    }

    /// How many times it thought.
    pub fn thoughts(&self) -> u64 {
        self.thoughts
    }

    /// Its camp.
    pub fn home(&self) -> Pos {
        self.home
    }
}

/// How far from the camp the camp's water is looked for, in tiles.
pub const CAMP_WATER_TILES: i32 = 2048;

/// The minds of the bodies the brain pilots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brains {
    minds: BTreeMap<BodyId, Mind>,
    seed: u64,
    /// Each camp's water, found once: its nearest drinking place.
    camp_water: BTreeMap<Pos, Option<DrinkingPlace>>,
}

impl Brains {
    /// No minds yet; `seed` chooses where they go looking.
    pub fn new(seed: u64) -> Self {
        Self {
            minds: BTreeMap::new(),
            seed,
            camp_water: BTreeMap::new(),
        }
    }

    /// The brain pilots `body` from now on: it thinks at the next turn.
    /// Born at the camp, it knows **where the camp drinks**: the drinking
    /// place nearest the camp, within [`CAMP_WATER_TILES`] (the tribe's
    /// knowledge of its land, until knowledge is passed on by people, V3).
    pub fn adopt(&mut self, world: &World, rules: &Content, body: BodyId) {
        self.adopt_at(world, rules, body, world.map().camp());
    }

    /// The same, the body's home being the camp at `home` (a tribe's own
    /// camp, D41: the vivarium's two tribes live on two camps).
    pub fn adopt_at(&mut self, world: &World, rules: &Content, body: BodyId, home: Pos) {
        let water = *self
            .camp_water
            .entry(home)
            .or_insert_with(|| nearest_drinking_place(world.map(), rules, home));
        let mind = self.minds.entry(body).or_insert_with(|| Mind::new(home));
        if let Some(place) = water {
            mind.water.insert(place.stand.chunk(), place);
        }
    }

    /// The mind of `body`, if the brain pilots it.
    pub fn mind(&self, body: BodyId) -> Option<&Mind> {
        self.minds.get(&body)
    }

    /// One turn, after a tick of `world` that produced `events`: the
    /// minds something changed for think, and what they decide is given
    /// back, to be queued for the next tick. Minds whose body is gone
    /// (dead) are forgotten.
    pub fn think(
        &mut self,
        world: &World,
        rules: &Content,
        events: &[Event],
    ) -> Vec<(BodyId, Decision)> {
        self.notice(events);
        let mut decisions = Vec::new();
        let seed = self.seed;
        self.minds.retain(|&id, mind| {
            let Some(body) = world.body(id) else {
                return false;
            };
            let condition = body.condition();
            if condition.vitality.points() == 0 {
                return false;
            }
            let past = pasts(world, id);
            if past != mind.past {
                mind.past = past;
                mind.due = true;
            }
            if !mind.due {
                return true;
            }
            mind.due = false;
            mind.thoughts += 1;
            look(world, rules, body.pos(), mind);
            if let Some(decision) = decide(world, rules, id, mind, seed) {
                decisions.push((id, decision));
            }
            mind.stopped = false;
            true
        });
        decisions
    }

    /// Marks the minds the tick's events concern.
    fn notice(&mut self, events: &[Event]) {
        for event in events {
            let body = match event {
                Event::DayStarted { .. } | Event::NightFell { .. } => {
                    for mind in self.minds.values_mut() {
                        mind.due = true;
                    }
                    continue;
                }
                Event::BodyStopped { body, .. }
                | Event::BodyBlocked { body, .. }
                | Event::MoveRefused { body, .. }
                | Event::DrinkRefused { body, .. }
                | Event::ActRefused { body, .. }
                | Event::BodyDrank { body, .. }
                | Event::BodyAte { body, .. } => *body,
                _ => continue,
            };
            if let Some(mind) = self.minds.get_mut(&body) {
                mind.due = true;
                if matches!(
                    event,
                    Event::BodyStopped { .. }
                        | Event::BodyBlocked { .. }
                        | Event::MoveRefused { .. }
                ) {
                    mind.stopped = true;
                }
                if let Event::BodyAte { item, .. } | Event::ActRefused { item, .. } = event {
                    // Eaten whole, refused or gone: looked at again when seen.
                    mind.food.remove(item);
                }
                if let Event::DrinkRefused { at, .. } = event {
                    mind.water.retain(|_, place| place.water != *at);
                }
            }
        }
    }
}

/// Which thresholds the body's gauges are past: (thirsty, parched,
/// hungry, asleep).
fn pasts(world: &World, id: BodyId) -> (bool, bool, bool, bool) {
    world.body(id).map_or((false, false, false, false), |body| {
        let c = body.condition();
        (
            c.thirst.points() >= THIRSTY,
            c.thirst.points() >= PARCHED,
            c.hunger.points() >= HUNGRY,
            c.asleep,
        )
    })
}

/// Can a body stand on `pos`?
fn walkable(map: &Map, rules: &Content, pos: Pos) -> bool {
    map.get(pos)
        .is_some_and(|t| rules.movement.cost(t).is_some())
}

/// The drinkable tile `pos` can drink from (itself first, then its
/// neighbours in a fixed order), if any.
fn water_by(map: &Map, pos: Pos) -> Option<Pos> {
    std::iter::once((0, 0))
        .chain([
            (0, -1),
            (1, 0),
            (0, 1),
            (-1, 0),
            (1, -1),
            (1, 1),
            (-1, 1),
            (-1, -1),
        ])
        .map(|(dx, dy)| Pos::new(pos.x + dx, pos.y + dy))
        .find(|p| map.get(*p).is_some_and(Terrain::is_drinkable))
}

/// The drinking place nearest `from`: the squares around it, growing a
/// tile at a time, up to [`CAMP_WATER_TILES`]; on a square, the nearest
/// tile first, ties in reading order.
fn nearest_drinking_place(map: &Map, rules: &Content, from: Pos) -> Option<DrinkingPlace> {
    for r in 0..=CAMP_WATER_TILES {
        let mut best: Option<DrinkingPlace> = None;
        for y in from.y - r..=from.y + r {
            let edge = y == from.y - r || y == from.y + r;
            let xs: Vec<i32> = if edge {
                (from.x - r..=from.x + r).collect()
            } else {
                vec![from.x - r, from.x + r]
            };
            for x in xs {
                let stand = Pos::new(x, y);
                if !walkable(map, rules, stand) {
                    continue;
                }
                if let Some(water) = water_by(map, stand)
                    && best.is_none_or(|b| far(stand, from) < far(b.stand, from))
                {
                    best = Some(DrinkingPlace { stand, water });
                }
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

/// Squared distance between two tiles.
fn far(a: Pos, b: Pos) -> i64 {
    let (dx, dy) = (i64::from(a.x - b.x), i64::from(a.y - b.y));
    dx * dx + dy * dy
}

/// The body at `at` looks around: the drinking places and the food in
/// sight go into its memory, the food it remembers in sight but no longer
/// sees is forgotten. Sight is the ground's under its feet, a square of
/// that half-side.
fn look(world: &World, rules: &Content, at: Pos, mind: &mut Mind) {
    let map = world.map();
    let camp = mind.home;
    let sight = i32::try_from(
        map.get(at)
            .map_or(1, |terrain| rules.movement.sight_tiles(terrain)),
    )
    .unwrap_or(1);
    let in_sight = |p: Pos| (p.x - at.x).abs() <= sight && (p.y - at.y).abs() <= sight;
    // Water: the nearest drinking place of each chunk in sight.
    for y in at.y - sight..=at.y + sight {
        for x in at.x - sight..=at.x + sight {
            let stand = Pos::new(x, y);
            if !walkable(map, rules, stand) {
                continue;
            }
            let Some(water) = water_by(map, stand) else {
                continue;
            };
            let place = DrinkingPlace { stand, water };
            mind.water
                .entry(stand.chunk())
                .and_modify(|known| {
                    if far(stand, at) < far(known.stand, at) {
                        *known = place;
                    }
                })
                .or_insert(place);
        }
    }
    // Food: what lies in the chunks in sight, within sight.
    mind.food.retain(|_, pos| !in_sight(*pos));
    let (low, high) = (
        Pos::new(at.x - sight, at.y - sight).chunk(),
        Pos::new(at.x + sight, at.y + sight).chunk(),
    );
    let is_food = |material: &str| rules.material(material).is_some_and(|m| m.food.is_some());
    if rules.spawns.iter().any(|spawn| is_food(&spawn.material)) {
        // The ground lays food of itself: every item of the chunks in sight.
        for cy in low.y..=high.y {
            for cx in low.x..=high.x {
                for item in world.items_in(ChunkPos::new(cx, cy)) {
                    if let Some(pos) = item.pos().filter(|p| in_sight(*p))
                        && is_food(item.material())
                    {
                        mind.food.insert(item.id(), pos);
                    }
                }
            }
        }
    } else {
        // It lays none (dead wood and pebbles, today): only the items
        // bodies laid can be food, and making the ground's own for every
        // chunk in sight would cost a thought forty milliseconds.
        for item in world.items() {
            if let Some(pos) = item.pos().filter(|p| in_sight(*p))
                && is_food(item.material())
            {
                mind.food.insert(item.id(), pos);
            }
        }
    }
    // Kept small: the nearest to the camp.
    while mind.water.len() > MEMORY_PLACES {
        let farthest = *mind
            .water
            .iter()
            .max_by_key(|(chunk, place)| (far(place.stand, camp), **chunk))
            .map(|(chunk, _)| chunk)
            .expect("not empty");
        mind.water.remove(&farthest);
    }
    while mind.food.len() > MEMORY_PLACES {
        let farthest = *mind
            .food
            .iter()
            .max_by_key(|(id, pos)| (far(**pos, camp), **id))
            .map(|(id, _)| id)
            .expect("not empty");
        mind.food.remove(&farthest);
    }
}

/// What the body decides, now that it looked.
fn decide(
    world: &World,
    rules: &Content,
    id: BodyId,
    mind: &mut Mind,
    seed: u64,
) -> Option<Decision> {
    let body = world.body(id)?;
    let c = body.condition();
    let here = body.pos();
    let map = world.map();
    let camp = mind.home;
    let decision = |intention, why| Some(Decision { intention, why });
    let (thirsty, parched, hungry, asleep) = pasts(world, id);
    if asleep && !parched {
        mind.doing = Doing::Nothing;
        return None;
    }
    if thirsty {
        // At a drinking place: drink.
        if let Some(water) = water_by(map, here) {
            mind.doing = Doing::Nothing;
            return decision(
                Intention::Drink {
                    body: id,
                    at: water,
                },
                Why::Thirst,
            );
        }
        let nearest = mind
            .water
            .values()
            .min_by_key(|place| (far(place.stand, here), place.stand.y, place.stand.x))
            .copied();
        if let Some(place) = nearest {
            if mind.doing == Doing::ToDrink(place) && mind.stopped {
                // It stopped short of it: forget it, look again.
                mind.water.retain(|_, p| *p != place);
                mind.due = true;
                return None;
            }
            mind.doing = Doing::ToDrink(place);
            return decision(
                Intention::MoveTo {
                    body: id,
                    target: place.stand,
                },
                Why::Thirst,
            );
        }
        return Some(explore(map, rules, id, here, mind, seed, Why::Thirst));
    }
    if hungry {
        let nearest = mind
            .food
            .iter()
            .min_by_key(|(item, pos)| (far(**pos, here), **item))
            .map(|(item, pos)| (*item, *pos));
        if let Some((item, pos)) = nearest {
            if (pos.x - here.x).abs() <= 1 && (pos.y - here.y).abs() <= 1 {
                mind.doing = Doing::Nothing;
                return decision(Intention::Eat { body: id, item }, Why::Hunger);
            }
            if mind.doing == Doing::ToEat(item, pos) && mind.stopped {
                mind.food.remove(&item);
                mind.due = true;
                return None;
            }
            mind.doing = Doing::ToEat(item, pos);
            return decision(
                Intention::MoveTo {
                    body: id,
                    target: pos,
                },
                Why::Hunger,
            );
        }
        return Some(explore(map, rules, id, here, mind, seed, Why::Hunger));
    }
    let home = (here.x - camp.x).abs() <= HOME_TILES && (here.y - camp.y).abs() <= HOME_TILES;
    let night = world.is_night(rules);
    if home {
        mind.doing = Doing::Nothing;
        if night && !c.asleep {
            return decision(Intention::Sleep { body: id }, Why::Night);
        }
        return None;
    }
    if mind.doing == Doing::Home {
        // On its way (something else made it think): it goes on. Stopped
        // short of the camp: it stays there, and tries again at the next
        // change rather than asking the same walk at once.
        if mind.stopped {
            mind.doing = Doing::Nothing;
        }
        return None;
    }
    mind.doing = Doing::Home;
    let why = if night { Why::Night } else { Why::Home };
    decision(
        Intention::MoveTo {
            body: id,
            target: camp,
        },
        why,
    )
}

/// The body goes looking for what it needs: a spiral out of its camp,
/// eight legs a ring, each ring a sight's length wider than the last (the
/// first direction chosen by the seed and the body), clamped to the map;
/// from each leg's end it looks, so the rings it walks sweep the land.
fn explore(
    map: &Map,
    rules: &Content,
    id: BodyId,
    here: Pos,
    mind: &mut Mind,
    seed: u64,
    need: Why,
) -> Decision {
    const DIRECTIONS: [(i32, i32); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];
    let first = usize::try_from(mix(seed ^ u64::from(id.0)) % 8).expect("under eight");
    let turn = usize::try_from(mind.walks % 8).expect("under eight");
    let ring = i32::try_from(mind.walks / 8 + 1).unwrap_or(i32::MAX);
    let (dx, dy) = DIRECTIONS[(first + turn) % 8];
    mind.walks += 1;
    let reach = i32::try_from(
        map.get(here)
            .map_or(1, |terrain| rules.movement.sight_tiles(terrain)),
    )
    .unwrap_or(1);
    let (w, h) = (
        i32::try_from(map.width()).expect("fits") - 1,
        i32::try_from(map.height()).expect("fits") - 1,
    );
    let camp = mind.home;
    let out = reach.saturating_mul(ring);
    let target = Pos::new(
        camp.x.saturating_add(dx.saturating_mul(out)).clamp(0, w),
        camp.y.saturating_add(dy.saturating_mul(out)).clamp(0, h),
    );
    mind.doing = Doing::Exploring(need);
    Decision {
        intention: Intention::MoveTo { body: id, target },
        why: Why::Explore,
    }
}

/// `SplitMix64`'s finaliser: a seed spread over 64 bits.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Gauge;
    use crate::time::Calendar;

    fn rules() -> Content {
        Content::sample()
    }

    /// A field `side` tiles of grass, the camp at (5, 5), a river down
    /// column `river` (none when it is off the map), one body at `at`.
    fn field(side: u32, river: i32, at: Pos) -> World {
        let mut map = Map::filled(side, side, Terrain::Grass).with_camp(5, 5);
        for y in 0..i32::try_from(side).unwrap() {
            map.set(Pos::new(river, y), Terrain::Water);
        }
        let mut world = World::empty(Calendar::new(1, 7200).unwrap(), map);
        world.spawn_body(at);
        world
    }

    /// Runs `ticks` ticks with the brain piloting every body: each tick's
    /// events go to the brain, its decisions are queued for the next.
    /// Gives back every event and every decision.
    fn run(
        world: &mut World,
        brains: &mut Brains,
        ticks: u64,
    ) -> (Vec<Event>, Vec<(BodyId, Decision)>) {
        let rules = rules();
        let (mut all, mut decided) = (Vec::new(), Vec::new());
        let mut events = Vec::new();
        for _ in 0..ticks {
            events.clear();
            world.step(&rules, &mut events);
            for (body, decision) in brains.think(world, &rules, &events) {
                world.queue(decision.intention.clone());
                decided.push((body, decision));
            }
            all.extend(events.iter().cloned());
        }
        (all, decided)
    }

    fn thirst(world: &mut World, points: u32) {
        world.bodies[0].condition.thirst = Gauge::at(points);
    }

    fn piloted(world: &World) -> Brains {
        let mut brains = Brains::new(7);
        let rules = rules();
        for body in world.bodies() {
            brains.adopt(world, &rules, body.id());
        }
        brains
    }

    #[test]
    fn a_thirsty_body_walks_to_the_water_it_sees_and_drinks() {
        let mut world = field(64, 40, Pos::new(5, 5));
        thirst(&mut world, 60);
        let mut brains = piloted(&world);
        let (events, decided) = run(&mut world, &mut brains, 600);
        assert!(
            matches!(
                decided[0].1,
                Decision { intention: Intention::MoveTo { target, .. }, why: Why::Thirst } if target.x == 39
            ),
            "it walks to the bank: {:?}",
            decided[0]
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::BodyDrank { at, .. } if at.x == 40)),
            "it drank at the river"
        );
        assert!(world.bodies[0].condition.thirst.points() < THIRSTY);
        assert!(
            decided.iter().any(|(_, d)| d.why == Why::Home),
            "and walks home after: {decided:?}"
        );
        let mind = brains.mind(BodyId(1)).unwrap();
        assert!(
            mind.drinking_places().count() >= 2,
            "it remembers the bank, a place a chunk"
        );
    }

    #[test]
    fn it_thinks_only_when_something_changes() {
        let mut world = field(64, 40, Pos::new(5, 5));
        let mut brains = piloted(&world);
        // Born: it thinks once; home by day, nothing pressing.
        let (_, decided) = run(&mut world, &mut brains, 1000);
        assert!(
            decided.is_empty(),
            "nothing to do at home by day: {decided:?}"
        );
        assert_eq!(
            brains.mind(BodyId(1)).unwrap().thoughts(),
            1,
            "its gauges rose, no threshold crossed"
        );
        // Thirst crosses the threshold: one more thought, a walk.
        thirst(&mut world, THIRSTY);
        let (_, decided) = run(&mut world, &mut brains, 1);
        assert_eq!(decided.len(), 1);
        assert_eq!(brains.mind(BodyId(1)).unwrap().thoughts(), 2);
    }

    #[test]
    fn knowing_no_water_it_goes_looking_and_finds_it() {
        // The river 150 tiles east: out of sight (60 on grass) from the camp.
        let mut world = field(200, 150, Pos::new(5, 5));
        thirst(&mut world, 60);
        let mut brains = piloted(&world);
        // Born at the camp it knows where the camp drinks: forgotten here,
        // to see it go looking.
        brains.minds.get_mut(&BodyId(1)).unwrap().water.clear();
        let (events, decided) = run(&mut world, &mut brains, 7000);
        assert_eq!(
            decided[0].1.why,
            Why::Explore,
            "nothing known: it goes looking"
        );
        assert!(
            events.iter().any(|e| matches!(e, Event::BodyDrank { .. })),
            "and drinks within the day: {:?}",
            decided.iter().map(|(_, d)| d.why).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_hungry_body_eats_the_food_it_saw() {
        let mut world = field(64, 60, Pos::new(5, 5));
        let berries = world.spawn_item("berries", 100, Pos::new(15, 9));
        world.bodies[0].condition.hunger = Gauge::at(HUNGRY + 10);
        let mut brains = piloted(&world);
        let (events, decided) = run(&mut world, &mut brains, 600);
        assert_eq!(decided[0].1.why, Why::Hunger);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::BodyAte { item, .. } if *item == berries)),
            "it ate the berries: {decided:?}"
        );
    }

    #[test]
    fn at_night_it_walks_home_and_sleeps_there() {
        let mut world = field(64, 60, Pos::new(30, 30));
        let mut brains = piloted(&world);
        // Born away from the camp by day: home first.
        let (_, decided) = run(&mut world, &mut brains, 1);
        assert_eq!(decided[0].1.why, Why::Home);
        let (events, decided) = run(&mut world, &mut brains, 7200);
        assert!(events.iter().any(|e| matches!(e, Event::NightFell { .. })));
        assert!(
            decided
                .iter()
                .any(|(_, d)| d.why == Why::Night && matches!(d.intention, Intention::Sleep { .. })),
            "it sleeps at night: {decided:?}"
        );
        let at = world.bodies[0].pos();
        assert!(
            (at.x - 5).abs() <= HOME_TILES && (at.y - 5).abs() <= HOME_TILES,
            "at home: {at:?}"
        );
    }

    #[test]
    fn a_sleeper_wakes_only_when_parched() {
        let mut world = field(64, 40, Pos::new(5, 5));
        // Asleep and tired: it would sleep on (a rested body wakes with the day).
        world.bodies[0].condition.asleep = true;
        world.bodies[0].condition.fatigue = Gauge::at(90);
        thirst(&mut world, THIRSTY + 5);
        let mut brains = piloted(&world);
        let (_, decided) = run(&mut world, &mut brains, 10);
        assert!(decided.is_empty(), "thirsty but not parched: it sleeps on");
        thirst(&mut world, PARCHED);
        let (events, decided) = run(&mut world, &mut brains, 600);
        assert_eq!(
            decided[0].1.why,
            Why::Thirst,
            "parched: it gets up and goes to drink"
        );
        assert!(events.iter().any(|e| matches!(e, Event::BodyDrank { .. })));
    }

    #[test]
    fn the_same_world_gives_the_same_thoughts_and_the_dead_are_forgotten() {
        let twice = || {
            let mut world = field(200, 150, Pos::new(5, 5));
            world.spawn_body(Pos::new(6, 5));
            thirst(&mut world, 60);
            let mut brains = piloted(&world);
            run(&mut world, &mut brains, 3000).1
        };
        assert_eq!(twice(), twice());
        let mut world = field(64, 40, Pos::new(5, 5));
        let mut brains = piloted(&world);
        world.bodies[0].condition.vitality = Gauge::at(0);
        run(&mut world, &mut brains, 1);
        assert!(brains.mind(BodyId(1)).is_none());
    }

    #[test]
    fn born_at_the_camp_it_knows_where_the_camp_drinks() {
        // The river 150 tiles east, out of sight of the camp.
        let mut world = field(200, 150, Pos::new(5, 5));
        thirst(&mut world, 60);
        let mut brains = piloted(&world);
        let places: Vec<_> = brains
            .mind(BodyId(1))
            .unwrap()
            .drinking_places()
            .copied()
            .collect();
        assert_eq!(
            places,
            vec![DrinkingPlace {
                stand: Pos::new(149, 5),
                water: Pos::new(150, 5)
            }]
        );
        let (events, decided) = run(&mut world, &mut brains, 400);
        assert_eq!(decided[0].1.why, Why::Thirst, "it walks straight there");
        assert!(events.iter().any(|e| matches!(e, Event::BodyDrank { .. })));
    }

    #[test]
    fn going_looking_it_walks_out_of_the_camp_in_rings() {
        let world = field(1000, 999, Pos::new(5, 5));
        let mut mind = Mind::new(Pos::new(5, 5));
        let targets: Vec<Pos> = (0..16)
            .map(|_| {
                match explore(
                    world.map(),
                    &rules(),
                    BodyId(1),
                    Pos::new(500, 500),
                    &mut mind,
                    7,
                    Why::Thirst,
                )
                .intention
                {
                    Intention::MoveTo { target, .. } => target,
                    other => panic!("{other:?}"),
                }
            })
            .collect();
        let ring = |p: Pos| (p.x - 5).abs().max((p.y - 5).abs());
        assert!(
            targets[..8].iter().all(|p| ring(*p) <= 60),
            "the first ring a sight wide: {targets:?}"
        );
        assert!(
            targets[8..].iter().any(|p| ring(*p) > 60)
                && targets[8..].iter().all(|p| ring(*p) <= 120),
            "the second twice as wide: {targets:?}"
        );
    }

    #[test]
    fn each_body_goes_home_to_its_own_camp() {
        let mut world = field(200, 100, Pos::new(5, 5));
        world.spawn_body(Pos::new(150, 150));
        let rules = rules();
        let mut brains = Brains::new(7);
        brains.adopt(&world, &rules, BodyId(1));
        brains.adopt_at(&world, &rules, BodyId(2), Pos::new(150, 150));
        assert_eq!(brains.mind(BodyId(2)).unwrap().home(), Pos::new(150, 150));
        // Its own camp's water: the river at column 100, the bank at 101.
        let places: Vec<_> = brains
            .mind(BodyId(2))
            .unwrap()
            .drinking_places()
            .copied()
            .collect();
        assert_eq!(
            places,
            vec![DrinkingPlace {
                stand: Pos::new(101, 150),
                water: Pos::new(100, 150)
            }]
        );
        world.bodies[1].condition.thirst = Gauge::at(60);
        let (events, _) = run(&mut world, &mut brains, 7200);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::BodyDrank {
                body: BodyId(2),
                ..
            }
        )));
        let at = world.bodies[1].pos();
        assert!(
            (at.x - 150).abs() <= HOME_TILES && (at.y - 150).abs() <= HOME_TILES,
            "back at its camp for the night: {at:?}"
        );
    }
}
