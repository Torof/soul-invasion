//! Food pooled at the hearth (D58; « la mise en commun », validated by the
//! project lead on 25 November 2026, with his corrections the same day).
//!
//! **The hearth** (« le foyer », never « le feu ») is everyone who lies
//! down at the camp's evening gathering: the circle of the home's place
//! (D55, amended 24 November 2026). It has **one reserve**, at the camp:
//! heaps of food lying on the ground at one tile, the home of the first
//! who put food there. They are the world's items, nothing more: they
//! spoil there as food does anywhere (D45), and are eaten with the
//! world's own gesture. The brain only remembers which heaps are the
//! hearth's, so that nobody of another hearth takes them for food lying
//! about (giving outside the hearth is not built).
//!
//! - **Carrying for its own.** While a body owes its hearth (what it set
//!   out at dawn to gather for it, below), of each gather its own's share,
//!   `owed / (store + owed)`, is carried for the hearth: not its own to
//!   eat on the way. (Decided in the build: the bodies came home at
//!   nightfall with half a day's food and ate it on the way, never holding
//!   the three days of their store, so « beyond its store » alone put next
//!   to nothing down and the young would have starved.)
//! - **Putting down.** Back at its camp (home from the day, or at night),
//!   before it eats, a body puts down there what it carries for its hearth
//!   and what it carries beyond its own store (its stock way, about three
//!   days): one heap a thought, what spoils first first.
//! - **Eating.** Hungry at its camp, a body eats from the reserve before
//!   it eats its own, what spoils first first. **The young first** (the
//!   project lead, 25 November 2026): a grown body eats there only what
//!   the reserve holds beyond what the young of the hearth (under
//!   sixteen) can still eat today; a child from `fed_until_years` to
//!   sixteen only beyond what the younger ones can.
//! - **The young are fed by the hearth** until `fed_until_years`: they do
//!   not gather alone (a weaned child of two gathered for itself).
//! - **What a body sees** (two true perceptions, both approved): at dawn,
//!   at its hearth, the reserve, and the hunger of its own present (its
//!   spouse, its children under sixteen). One that sees its own hungry
//!   and the reserve short gathers for them too, as many days of their
//!   need as it stores for itself, less what the reserve holds: its true
//!   need, not a steer.
//! - **The care for the young's coming days** (the project lead, 25
//!   November 2026, « les jeunes d'abord », a human trait he asked for):
//!   a grown body counts in its own store a share of what the reserve
//!   leaves uncovered of the young of its hearth's next `care_days` of
//!   need. The share is innate, drawn from the run's seed and its id
//!   (`care_percent`), and greater for its own children
//!   (`own_care_percent`). It gathers the greater of the two, never both:
//!   they weigh the same children.
//!
//! Every number is content (`needs.ron`, `pooling`, D30). It rides on the
//! evening circle: without the fire there is no hearth, nothing pooled.

use std::collections::BTreeMap;

use super::{
    Brains, Content, Decision, Doing, HOME_TILES, HUNGRY, Intention, Mind, News, need_kj_per_day,
    place_centre, place_of, place_side, today,
};
use crate::map::Pos;
use crate::{Body, BodyId, ItemId, World};

/// The age a body is grown at, in thousandths of a year (as the brain
/// counts it elsewhere): sixteen.
const GROWN_MILLI: u64 = 16_000;

/// Whether food is pooled: the content's section, and the evening circle
/// it rides on.
pub(super) fn on(rules: &Content) -> bool {
    rules.needs.pooling.on() && rules.needs.fire.on()
}

/// Whether `body` is a child fed by its hearth: weaned, younger than the
/// content's `fed_until_years`. It does not gather alone.
pub(super) fn fed_by_hearth(world: &World, rules: &Content, body: &Body) -> bool {
    on(rules) && rules.needs.pooling.fed(world.age_milli(body)) && !world.is_infant(body, rules)
}

/// The camp of a home: its place's middle, the circle's and the hearth's
/// key (D55, amended 24 November 2026).
pub(super) fn camp_of(rules: &Content, home: Pos) -> Pos {
    let side = place_side(rules);
    place_centre(place_of(home, side), side)
}

/// One hearth's reserve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Reserve {
    /// The tile it lies at: the home of the first who put food there.
    tile: Pos,
    /// The heaps put there, in the order put.
    items: Vec<ItemId>,
}

/// Every hearth's reserve, and the young each hearth saw at dawn.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Reserves {
    by_camp: BTreeMap<Pos, Reserve>,
    /// Every heap of a reserve, with its camp.
    camp_of: BTreeMap<ItemId, Pos>,
    /// The young of each hearth at dawn: each, and whether it is fed by
    /// the hearth (under `fed_until_years`).
    young: BTreeMap<Pos, Vec<(BodyId, bool)>>,
}

/// A heap of a reserve, or carried: its id, where it lies, its energy and
/// the tick it spoils at.
#[derive(Debug, Clone, Copy)]
struct Heap {
    item: ItemId,
    at: Option<Pos>,
    kj: u64,
    ends: u64,
}

/// The food heap `item` as [`Heap`], unless it is not food or spoiled.
fn heap(world: &World, rules: &Content, item: ItemId) -> Option<Heap> {
    let found = world.item(item)?;
    let food = rules.material(found.material())?.food?;
    if world.is_spoiled(item, rules) {
        return None;
    }
    Some(Heap {
        item,
        at: found.pos(),
        kj: u64::from(food.energy_kj(found.mass_g())),
        ends: found.made().0 + u64::from(food.keeps_days) * world.calendar().ticks_per_day(),
    })
}

fn near(a: Pos, b: Pos) -> bool {
    (a.x - b.x).abs() <= 1 && (a.y - b.y).abs() <= 1
}

impl Reserves {
    /// The heaps lying in the reserve of `camp`, not spoiled.
    fn heaps(&self, world: &World, rules: &Content, camp: Pos) -> Vec<Heap> {
        let Some(reserve) = self.by_camp.get(&camp) else {
            return Vec::new();
        };
        reserve
            .items
            .iter()
            .filter_map(|&item| heap(world, rules, item))
            .filter(|h| h.at.is_some_and(|at| near(at, reserve.tile)))
            .collect()
    }

    /// What the reserve of `camp` holds, in kilojoules.
    pub(super) fn kj(&self, world: &World, rules: &Content, camp: Pos) -> u64 {
        self.heaps(world, rules, camp).iter().map(|h| h.kj).sum()
    }

    /// The camp whose reserve holds `item`, if one does.
    pub(super) fn camp_holding(&self, item: ItemId) -> Option<Pos> {
        self.camp_of.get(&item).copied()
    }

    /// Lets go of the heaps gone (eaten whole, spoiled) or no longer
    /// lying at their reserve, and of the reserves left empty.
    fn prune(&mut self, world: &World) {
        let camp_of = &mut self.camp_of;
        self.by_camp.retain(|_, reserve| {
            let tile = reserve.tile;
            reserve.items.retain(|&item| {
                let lies = world
                    .item(item)
                    .and_then(crate::Item::pos)
                    .is_some_and(|at| near(at, tile));
                if !lies {
                    camp_of.remove(&item);
                }
                lies
            });
            !reserve.items.is_empty()
        });
    }

    /// Where the reserve of `camp` lies, or would lie if `home` put the
    /// first heap there.
    fn tile(&self, camp: Pos, home: Pos) -> Pos {
        self.by_camp.get(&camp).map_or(home, |r| r.tile)
    }

    fn put(&mut self, camp: Pos, tile: Pos, item: ItemId) {
        self.by_camp
            .entry(camp)
            .or_insert_with(|| Reserve {
                tile,
                items: Vec::new(),
            })
            .items
            .push(item);
        self.camp_of.insert(item, camp);
    }

    /// The camps and what each reserve holds, in kilojoules (a measure).
    pub(super) fn all_kj(&self, world: &World, rules: &Content) -> Vec<(Pos, u64)> {
        self.by_camp
            .keys()
            .map(|&camp| (camp, self.kj(world, rules, camp)))
            .collect()
    }
}

/// Whether a body standing at `here` is at its camp: near its home, or
/// on its home's place.
fn at_camp(rules: &Content, here: Pos, mind: &Mind) -> bool {
    let home = mind.ways.home;
    let side = place_side(rules);
    ((here.x - home.x).abs() <= HOME_TILES && (here.y - home.y).abs() <= HOME_TILES)
        || place_of(here, side) == place_of(home, side)
}

/// What the young ahead of `body` at its hearth can still eat today, in
/// kilojoules: for a grown body every young one of the hearth; for a
/// young one past `fed_until_years`, the younger fed by the hearth; for
/// those, nobody.
fn young_ahead(world: &World, rules: &Content, reserves: &Reserves, camp: Pos, body: &Body) -> u64 {
    let age = world.age_milli(body);
    if rules.needs.pooling.fed(age) {
        return 0;
    }
    let grown = age >= GROWN_MILLI;
    reserves.young.get(&camp).map_or(0, |young| {
        young
            .iter()
            .filter(|&&(y, fed)| y != body.id() && (grown || fed))
            .filter_map(|&(y, _)| world.body(y))
            .filter(|y| y.condition().vitality.points() > 0)
            .map(|y| y.condition().room_j(&rules.needs) / 1000)
            .sum()
    })
}

/// Hungry at its camp, `id` eats from its hearth's reserve before its own,
/// the young first: walks to the heap that spoils first and eats it.
/// `None` when it is not at its camp, the reserve holds nothing it may
/// eat, or it could not reach it today.
pub(super) fn eat_at_hearth(
    world: &World,
    rules: &Content,
    reserves: &Reserves,
    (id, body, here): (BodyId, &Body, Pos),
    mind: &mut Mind,
) -> Option<Decision> {
    if !on(rules) || !at_camp(rules, here, mind) {
        return None;
    }
    let day = today(world);
    if mind.hearth_barred == Some(day) {
        return None;
    }
    let camp = camp_of(rules, mind.ways.home);
    let heaps = reserves.heaps(world, rules, camp);
    let holds: u64 = heaps.iter().map(|h| h.kj).sum();
    if holds == 0 || holds <= young_ahead(world, rules, reserves, camp, body) {
        return None;
    }
    let first = heaps.iter().min_by_key(|h| (h.ends, h.item))?;
    let at = first.at?;
    let decision = |intention| {
        Some(Decision {
            intention,
            why: super::Why::Hunger,
        })
    };
    if near(at, here) {
        mind.doing = Doing::Nothing;
        return decision(Intention::Eat {
            body: id,
            item: first.item,
        });
    }
    if mind.doing == Doing::ToReserve(at) && mind.stopped {
        // Stopped short of it: not again today.
        mind.hearth_barred = Some(day);
        mind.doing = Doing::Nothing;
        return None;
    }
    mind.doing = Doing::ToReserve(at);
    decision(Intention::MoveTo {
        body: id,
        target: at,
    })
}

/// Back at its camp, `id` puts down at its hearth's reserve a heap of
/// what it carries for its hearth or beyond its own store: walks to the
/// reserve, then lays the heap that spoils first among those it can spare
/// (a heap goes when most of it is beyond what it keeps). One heap a
/// thought; it thinks again at once for the next.
pub(super) fn put_down(
    world: &World,
    rules: &Content,
    reserves: &mut Reserves,
    (id, body, night): (BodyId, &Body, bool),
    mind: &mut Mind,
    news: &mut Vec<News>,
) -> Option<Decision> {
    if !on(rules) || body.condition().asleep {
        return None;
    }
    let day = today(world);
    if mind.hearth_barred == Some(day) {
        return None;
    }
    let mut carried: Vec<Heap> = body
        .inventory()
        .filter_map(|item| heap(world, rules, item))
        .collect();
    let total: u64 = carried.iter().map(|h| h.kj).sum();
    // What it keeps, weighed once as it comes home: its store, and of
    // what it gathered for its hearth beyond it, only its own share.
    // What it keeps: its store at most, and never what it carries for
    // its hearth.
    mind.hearth_carried_kj = mind.hearth_carried_kj.min(total);
    let keep = total
        .saturating_sub(mind.hearth_carried_kj)
        .min(mind.ways.stock_kj);
    if total <= keep {
        return None;
    }
    carried.sort_by_key(|h| (h.ends, h.item));
    // A heap goes down when most of it is beyond what it keeps.
    let spare = *carried.iter().find(|h| total - h.kj / 2 >= keep)?;
    let camp = camp_of(rules, mind.ways.home);
    let tile = reserves.tile(camp, mind.ways.home);
    let here = body.pos();
    let why = if night {
        super::Why::Night
    } else {
        super::Why::Home
    };
    if near(tile, here) {
        reserves.put(camp, tile, spare.item);
        mind.hearth_put_kj += spare.kj;
        mind.hearth_carried_kj = mind.hearth_carried_kj.saturating_sub(spare.kj);
        mind.doing = Doing::Nothing;
        mind.due = true;
        news.push(News::PutDown {
            body: id,
            camp,
            kj: spare.kj,
            at: world.tick().0,
        });
        return Some(Decision {
            intention: Intention::Drop {
                body: id,
                item: spare.item,
            },
            why,
        });
    }
    if mind.doing == Doing::ToReserve(tile) && mind.stopped {
        mind.hearth_barred = Some(day);
        mind.doing = Doing::Nothing;
        return None;
    }
    mind.doing = Doing::ToReserve(tile);
    Some(Decision {
        intention: Intention::MoveTo {
            body: id,
            target: tile,
        },
        why,
    })
}

/// Whether a hungry body may eat from what it carries (D58): only what
/// is beyond what it carries for its hearth.
pub(super) fn may_eat_carried(world: &World, rules: &Content, id: BodyId, mind: &Mind) -> bool {
    !on(rules)
        || mind.hearth_carried_kj == 0
        || super::carried_kj(world, rules, id) > mind.hearth_carried_kj
}

/// What of `kj` just gathered a body carries for its hearth (D58): while
/// it owes its hearth `owed` beyond its own `store`, its own's share of it,
/// `owed / (store + owed)`, at most what it still owes. The bodies came
/// home at nightfall with half a day's food and ate it on the way, never
/// holding the three days of their store: « beyond its store » alone put
/// nothing down in a trial run, and the children of the hearth would have
/// starved.
fn for_hearth(kj: u64, store: u64, owed: u64) -> u64 {
    let share = u128::from(kj) * u128::from(owed) / u128::from((store + owed).max(1));
    owed.min(u64::try_from(share).unwrap_or(u64::MAX))
}

/// Whether `other` is one of `body`'s own: its spouse, or its child under
/// sixteen.
fn is_own(world: &World, body: &Body, other: &Body) -> bool {
    body.spouse() == Some(other.id())
        || ((other.mother() == Some(body.id()) || other.father() == Some(body.id()))
            && world.age_milli(other) < GROWN_MILLI)
}

impl Brains {
    /// At dawn, each hearth's circle (D55) sees its reserve (D58): who lay
    /// down there sees what it holds and the hunger of its own present,
    /// and sets what it will gather today beyond its own store for them:
    /// its own seen hungry, as many days of their need as it stores for
    /// itself, less the reserve; or, the greater, its care for the young's
    /// uncovered coming days. Who lay down at no hearth saw nothing.
    pub(super) fn see_hearths(
        &mut self,
        world: &World,
        rules: &Content,
        circles: &[(Pos, Vec<BodyId>)],
    ) {
        if !on(rules) {
            return;
        }
        let pooling = &rules.needs.pooling;
        self.reserves.prune(world);
        self.reserves.young.clear();
        for mind in self.minds.values_mut() {
            mind.hearth_extra_kj = 0;
            mind.hearth_put_kj = 0;
        }
        let now = world.tick().0;
        for (camp, present) in circles {
            let camp = *camp;
            let reserve_kj = self.reserves.kj(world, rules, camp);
            let bodies: Vec<&Body> = present.iter().filter_map(|&b| world.body(b)).collect();
            let young: Vec<(&Body, u64)> = bodies
                .iter()
                .filter(|b| world.age_milli(b) < GROWN_MILLI)
                .map(|b| (*b, need_kj_per_day(rules, b)))
                .collect();
            self.reserves.young.insert(
                camp,
                young
                    .iter()
                    .map(|(b, _)| (b.id(), pooling.fed(world.age_milli(b))))
                    .collect(),
            );
            let days = u64::from(pooling.care_days);
            let young_need: u64 = young.iter().map(|(_, need)| need * days).sum();
            let uncovered = young_need.saturating_sub(reserve_kj);
            for body in &bodies {
                if fed_by_hearth(world, rules, body) {
                    continue;
                }
                let Some(mind) = self.minds.get_mut(&body.id()) else {
                    continue;
                };
                let own_need = need_kj_per_day(rules, body).max(1);
                let stock = mind.ways.stock_kj;
                let hungry_own: u64 = bodies
                    .iter()
                    .filter(|o| o.id() != body.id() && is_own(world, body, o))
                    .filter(|o| o.condition().hunger.points() >= HUNGRY)
                    .map(|o| need_kj_per_day(rules, o) * stock / own_need)
                    .sum();
                let seen = hungry_own.saturating_sub(reserve_kj);
                let cared =
                    if pooling.cares() && world.age_milli(body) >= GROWN_MILLI && uncovered > 0 {
                        let care = u64::from(mind.care);
                        let own_care = (care * u64::from(pooling.own_care_percent) / 100).min(100);
                        let weighed: u64 = young
                            .iter()
                            .map(|(y, need)| {
                                let w = if is_own(world, body, y) {
                                    own_care
                                } else {
                                    care
                                };
                                need * days * w / 100
                            })
                            .sum();
                        weighed * uncovered / young_need.max(1)
                    } else {
                        0
                    };
                mind.hearth_extra_kj = seen.max(cared);
                if mind.hearth_extra_kj > 0 {
                    self.news.push(News::SawHearth {
                        body: body.id(),
                        camp,
                        reserve_kj,
                        seen_kj: seen,
                        cared_kj: cared,
                        at: now,
                    });
                }
            }
        }
    }

    /// What each hearth's reserve holds now, in kilojoules, by camp (D58,
    /// a measure for the vivarium).
    #[must_use]
    pub fn reserves_kj(&self, world: &World, rules: &Content) -> Vec<(Pos, u64)> {
        self.reserves.all_kj(world, rules)
    }
}

impl Brains {
    /// What the world did to the reserves this tick (D58, measures): the
    /// heaps of a reserve eaten, and those spoiled.
    pub(super) fn hearth_events(
        &mut self,
        events: &[crate::Event],
        world: &World,
        rules: &Content,
    ) {
        if !on(rules) {
            return;
        }
        let at = world.tick().0;
        for event in events {
            match event {
                crate::Event::BodyAte {
                    body,
                    item,
                    energy_kj,
                    ..
                } => {
                    if let Some(camp) = self.reserves.camp_holding(*item) {
                        self.news.push(News::AteAtHearth {
                            body: *body,
                            camp,
                            kj: u64::from(*energy_kj),
                            at,
                        });
                    }
                }
                crate::Event::Spoiled {
                    item,
                    material,
                    grams,
                    holder: None,
                } => {
                    if let Some(camp) = self.reserves.camp_holding(*item) {
                        let kj = rules
                            .material(material)
                            .and_then(|m| m.food)
                            .map_or(0, |f| u64::from(f.energy_kj(*grams)));
                        self.news.push(News::SpoiledAtHearth { camp, kj, at });
                    }
                }
                crate::Event::WorkStopped {
                    body, gives, grams, ..
                } => {
                    let Some(mind) = self.minds.get_mut(body) else {
                        continue;
                    };
                    let owed = mind.hearth_owed().saturating_sub(mind.hearth_carried_kj);
                    if owed == 0 {
                        continue;
                    }
                    let kj = rules
                        .material(gives)
                        .and_then(|m| m.food)
                        .map_or(0, |f| u64::from(f.energy_kj(*grams)));
                    mind.hearth_carried_kj += for_hearth(kj, mind.ways.stock_kj, owed);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{BEARER_CHUNKS, Food, Land, Lru, Placed, Store, decide};
    use super::*;
    use crate::content::{Fire, Pooling};
    use crate::map::{Map, Terrain};
    use crate::time::Calendar;
    use crate::{Gauge, Sex};

    /// The sample content with the fire lit and the hearth's pooling on.
    fn pooled() -> Content {
        let mut rules = Content::sample();
        rules.needs.fire = Fire::placeholders();
        rules.needs.pooling = Pooling::placeholders();
        rules
    }

    /// A field of grass, the camp at (5, 5), a body at each of `at`, each
    /// `years` old, piloted.
    fn field(rules: &Content, bodies: &[(Pos, u64)]) -> (World, Brains, Vec<BodyId>) {
        let map = Map::filled(64, 64, Terrain::Grass).with_camp(5, 5);
        let mut world = World::empty(Calendar::new(1, 7200).unwrap(), map);
        let year = u64::from(world.calendar().days_per_age_year());
        let ids: Vec<BodyId> = bodies
            .iter()
            .map(|&(at, years)| {
                let id = world.spawn_body(at);
                world.set_birth(id, years * year, Sex::Female);
                id
            })
            .collect();
        world.step(rules, &mut Vec::new());
        let mut brains = Brains::new(7);
        for &id in &ids {
            brains.adopt(&world, rules, id);
        }
        (world, brains, ids)
    }

    fn hungry(world: &mut World, rules: &Content, id: BodyId, points: u32) {
        let body = world.bodies.iter_mut().find(|b| b.id() == id).unwrap();
        body.condition.deficit_j =
            u64::from(points) * u64::from(rules.needs.kj_per_hunger_point) * 1000;
        body.condition.hunger = Gauge::at(points);
    }

    fn think(world: &World, rules: &Content, brains: &mut Brains, id: BodyId) -> Option<Decision> {
        think_told(world, rules, brains, id).0
    }

    fn think_told(
        world: &World,
        rules: &Content,
        brains: &mut Brains,
        id: BodyId,
    ) -> (Option<Decision>, Vec<News>) {
        let mut food = Food::default();
        food.refresh(world, rules);
        let (mut bearers, mut shores) = (Placed::new(), Lru::new(BEARER_CHUNKS));
        let (mut memory, mut news) = (Store::new(), Vec::new());
        let mut land = Land {
            groups: &mut BTreeMap::new(),
            bearers: &mut bearers,
            shores: &mut shores,
            food: &mut food,
            memory: &mut memory,
            news: &mut news,
            reserves: &mut brains.reserves,
        };
        let mind = brains.minds.get_mut(&id).unwrap();
        let decision = decide(world, rules, &mut land, id, mind, 7);
        (decision, news)
    }

    #[test]
    fn a_hungry_child_under_eight_does_not_gather_one_of_eight_does() {
        use crate::crops::tests::{body_by, run_to, valley_with};
        for (years, pooling, gathers) in [(5, true, false), (5, false, true), (9, true, true)] {
            let (mut world, mut rules, _, bramble) = valley_with("bramble");
            rules.needs.fire = Fire::placeholders();
            rules.needs.pooling = if pooling {
                Pooling::placeholders()
            } else {
                Pooling::default()
            };
            run_to(&mut world, &rules, 28);
            let id = body_by(&mut world, &bramble);
            let year = u64::from(world.calendar().days_per_age_year());
            world.set_birth(id, years * year, Sex::Female);
            world.step(&rules, &mut Vec::new());
            hungry(&mut world, &rules, id, 70);
            let mut brains = Brains::new(7);
            brains.adopt(&world, &rules, id);
            brains.minds.get_mut(&id).unwrap().ways.worth_half_tiles = 1;
            let (mut gathered, mut events) = (false, Vec::new());
            for _ in 0..30 {
                events.clear();
                world.step(&rules, &mut events);
                for (_, d) in brains.think(&world, &rules, &events) {
                    gathered |= matches!(d.intention, Intention::Gather { .. });
                    world.queue(d.intention);
                }
            }
            assert_eq!(gathered, gathers, "{years} years, pooling {pooling}");
        }
    }

    #[test]
    fn hungry_at_its_camp_it_eats_from_the_reserve_before_its_own() {
        let rules = pooled();
        let (mut world, mut brains, ids) = field(&rules, &[(Pos::new(5, 5), 30)]);
        let id = ids[0];
        let own = world.spawn_item("berries", 200, Pos::new(5, 5));
        world.queue(Intention::PickUp {
            body: id,
            item: own,
        });
        world.step(&rules, &mut Vec::new());
        assert!(world.body(id).unwrap().inventory().any(|i| i == own));
        let camp = camp_of(&rules, Pos::new(5, 5));
        let pooled = world.spawn_item("berries", 200, Pos::new(7, 5));
        brains.reserves.put(camp, Pos::new(7, 5), pooled);
        hungry(&mut world, &rules, id, 70);
        let decision = think(&world, &rules, &mut brains, id).unwrap();
        assert_eq!(
            decision.intention,
            Intention::MoveTo {
                body: id,
                target: Pos::new(7, 5)
            },
            "to the reserve first"
        );
        // Not seen as food lying about: only the hearth's way eats it.
        assert!(!brains.minds[&id].food.contains_key(&pooled));
        // Without pooling it eats its own.
        let mut plain = rules.clone();
        plain.needs.pooling = Pooling::default();
        let decision = think(&world, &plain, &mut brains, id).unwrap();
        assert_eq!(
            decision.intention,
            Intention::Eat {
                body: id,
                item: own
            }
        );
    }

    #[test]
    fn the_young_eat_from_a_short_reserve_first() {
        let rules = pooled();
        let at = Pos::new(5, 5);
        let (mut world, mut brains, ids) = field(&rules, &[(at, 30), (at, 5)]);
        let (grown, child) = (ids[0], ids[1]);
        let camp = camp_of(&rules, at);
        // 100 g of berries, 250 kJ: less than the hungry child can eat.
        let pooled = world.spawn_item("berries", 100, at);
        brains.reserves.put(camp, at, pooled);
        brains.reserves.young.insert(camp, vec![(child, true)]);
        hungry(&mut world, &rules, grown, 70);
        hungry(&mut world, &rules, child, 70);
        let decision = think(&world, &rules, &mut brains, grown);
        assert!(
            !decision.is_some_and(|d| d.intention
                == Intention::Eat {
                    body: grown,
                    item: pooled
                }),
            "the grown leaves it to the child"
        );
        let decision = think(&world, &rules, &mut brains, child).unwrap();
        assert_eq!(
            decision.intention,
            Intention::Eat {
                body: child,
                item: pooled
            }
        );
        // The child fed, the grown eats from it.
        let c = &mut world
            .bodies
            .iter_mut()
            .find(|b| b.id() == child)
            .unwrap()
            .condition;
        c.deficit_j = 0;
        c.hunger = Gauge::at(0);
        let decision = think(&world, &rules, &mut brains, grown).unwrap();
        assert_eq!(
            decision.intention,
            Intention::Eat {
                body: grown,
                item: pooled
            }
        );
    }

    #[test]
    fn at_its_camp_it_puts_down_what_it_carries_beyond_its_store() {
        let rules = pooled();
        let at = Pos::new(5, 5);
        let (mut world, mut brains, ids) = field(&rules, &[(at, 30)]);
        let id = ids[0];
        // Two heaps of 750 kJ, 1000 kJ of them gathered for its hearth;
        // its store 500 kJ: it keeps 500 kJ and puts one heap down.
        for _ in 0..2 {
            let item = world.spawn_item("berries", 300, at);
            world.queue(Intention::PickUp { body: id, item });
            world.step(&rules, &mut Vec::new());
        }
        brains.minds.get_mut(&id).unwrap().ways.stock_kj = 500;
        brains.minds.get_mut(&id).unwrap().hearth_extra_kj = 1000;
        brains.minds.get_mut(&id).unwrap().hearth_carried_kj = 1000;
        // Home from its day.
        brains.minds.get_mut(&id).unwrap().doing = Doing::Home;
        let (decision, news) = think_told(&world, &rules, &mut brains, id);
        let decision = decision.unwrap();
        let Intention::Drop { item, .. } = decision.intention else {
            panic!("it puts down: {decision:?}");
        };
        world.queue(decision.intention.clone());
        world.step(&rules, &mut Vec::new());
        let camp = camp_of(&rules, at);
        assert_eq!(brains.reserves.kj(&world, &rules, camp), 750);
        assert_eq!(brains.reserves.camp_holding(item), Some(camp));
        assert_eq!(
            brains.minds[&id].hearth_owed(),
            250,
            "what it brought counts"
        );
        assert!(
            matches!(news.last(), Some(News::PutDown { kj: 750, .. })),
            "told"
        );
        // What it keeps is its own share: nothing more to put down.
        let decision = think(&world, &rules, &mut brains, id);
        assert!(
            !decision.is_some_and(|d| matches!(d.intention, Intention::Drop { .. })),
            "it keeps its store"
        );
    }

    #[test]
    fn what_it_gathers_for_its_own_is_carried_for_them_not_eaten() {
        // Owing its hearth as much as its store: half of each gather.
        assert_eq!(for_hearth(1000, 30_000, 30_000), 500);
        // Never more than it still owes.
        assert_eq!(for_hearth(100_000, 30_000, 2_000), 2_000);
        assert_eq!(for_hearth(1000, 30_000, 0), 0);
        let rules = pooled();
        let at = Pos::new(5, 5);
        let (mut world, mut brains, ids) = field(&rules, &[(at, 30)]);
        let id = ids[0];
        let item = world.spawn_item("berries", 400, at);
        world.queue(Intention::PickUp { body: id, item });
        world.step(&rules, &mut Vec::new());
        hungry(&mut world, &rules, id, 70);
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.hearth_carried_kj = 1000;
        assert!(
            !may_eat_carried(&world, &rules, id, mind),
            "all of it is its own's"
        );
        mind.hearth_carried_kj = 500;
        assert!(may_eat_carried(&world, &rules, id, mind));
        mind.hearth_carried_kj = 1000;
        let decision = think(&world, &rules, &mut brains, id);
        assert!(
            !decision.is_some_and(|d| matches!(d.intention, Intention::Eat { .. })),
            "hungry, it leaves its own's share alone"
        );
    }

    #[test]
    fn at_dawn_it_sees_the_reserve_its_hungry_own_and_cares_for_the_young() {
        let at = Pos::new(5, 5);
        for (care, child_hungry) in [(false, true), (true, false), (true, true)] {
            let mut rules = pooled();
            if !care {
                rules.needs.pooling.care_days = 0;
            }
            let (mut world, mut brains, ids) = field(&rules, &[(at, 30), (at, 5)]);
            let (mother, child) = (ids[0], ids[1]);
            world.set_parents(child, Some(mother), None);
            if child_hungry {
                hungry(&mut world, &rules, child, 70);
            }
            brains.minds.get_mut(&mother).unwrap().care = 50;
            let camp = camp_of(&rules, at);
            let pooled = world.spawn_item("berries", 400, at);
            brains.reserves.put(camp, at, pooled);
            brains.see_hearths(&world, &rules, &[(camp, vec![mother, child])]);
            let need = |id| need_kj_per_day(&rules, world.body(id).unwrap());
            let stock = brains.minds[&mother].ways.stock_kj;
            // Its own hungry, as many days as it stores, less the 1000 kJ.
            let seen = if child_hungry {
                (need(child) * stock / need(mother)).saturating_sub(1000)
            } else {
                0
            };
            // Its own child at twice its care (100 %): the two coming days
            // the reserve leaves uncovered.
            let cared = if care {
                (need(child) * 2).saturating_sub(1000)
            } else {
                0
            };
            assert_eq!(
                brains.minds[&mother].hearth_extra_kj,
                seen.max(cared),
                "care {care}, hungry {child_hungry}"
            );
            assert_eq!(
                brains.minds[&child].hearth_extra_kj, 0,
                "a child gathers nothing for others"
            );
            assert_eq!(brains.reserves.young[&camp], vec![(child, true)]);
        }
    }
}
