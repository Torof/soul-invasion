//! Food pooled at the hearth (D58; « la mise en commun », validated by the
//! project lead on 25 November 2026, with his corrections the same day).
//!
//! **The hearth** (« le foyer », never « le feu ») is everyone who lies
//! down at the camp's evening gathering: the circle of the home's place
//! (D55, amended 24 November 2026). It has **one reserve**, at the camp:
//! heaps of food at one tile, the home of the first who put food there.
//! **A ledger** (the project lead, 25 November 2026: « c'est simplement
//! un bookkeeping »): a heap laid there leaves the world's items for a
//! record the world keeps off the ground ([`Intention::Keep`],
//! [`World::kept`]), which no look reads; it spoils there as food does
//! anywhere (D45); eaten, it feeds the body with no item made; taken
//! away (the hands may), it is an item again. The brain remembers which
//! records are the hearth's (giving outside the hearth is not built).
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
//!   project lead, 25 November 2026): an older body eats there only
//!   what the reserve holds beyond what the young of the hearth (under
//!   `young_first_until_years`, twelve since the life lot of the same day;
//!   sixteen before) can still eat today; a child from `fed_until_years`
//!   to that age only beyond what the younger ones can.
//! - **The hunger bar** (the project lead, 25 November 2026: "avoir très
//!   très faim, mourir de faim et avoir de la nourriture, ça personne ne le
//!   fera jamais, on mange la nourriture"). How hungry a body is, read by
//!   the share of its fat reserve left (its hunger gauge is full for days
//!   before it dies): from `very_hungry_fat_percent` up it is hungry, and
//!   carries home and leaves the young first as above; below, very hungry,
//!   it eats what it carries for its hearth, and from the reserve despite
//!   the young, unless its will keeps it (`keep_percent_by_will`, drawn
//!   once a game day); below `starving_fat_percent`, starving, it eats it,
//!   always. A child the hearth feeds is not concerned: nobody is ahead
//!   of it.
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
//!   a body past the young's age counts in its own store a share of what
//!   the reserve leaves uncovered of the young of its hearth's next
//!   `care_days` of need. The share is innate, drawn from the run's seed and its id
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
use crate::content::HungerTier;
use crate::r#gen::scatter::{self, purpose};
use crate::map::Pos;
use crate::{Body, BodyId, ItemId, World};

/// The age under which a child is one of its parent's own, whose hunger
/// the parent sees at dawn, in thousandths of a year (as the brain counts
/// it elsewhere): sixteen. Not « the young first » of the hearth, which is
/// the content's `young_first_until_years` (twelve).
const OWN_CHILD_MILLI: u64 = 16_000;

/// Whether food is pooled: the content's section, and the evening circle
/// it rides on.
pub(super) fn on(rules: &Content) -> bool {
    rules.needs.pooling.on() && rules.needs.fire.on()
}

/// Whether `body` is a child fed by its hearth: weaned, younger than the
/// content's `fed_until_years`, and of a hearth: it lay down in its camp's
/// circle last night (before the first dawn, every child is). It does not
/// gather alone. A child that slept away from its camp (lost on its
/// mother's way, out with her) has no hearth that day and fends for itself.
pub(super) fn fed_by_hearth(
    world: &World,
    rules: &Content,
    reserves: &Reserves,
    body: &Body,
) -> bool {
    on(rules)
        && rules.needs.pooling.fed(world.age_milli(body))
        && !world.is_infant(body, rules)
        && (!reserves.dawned
            || reserves
                .young
                .values()
                .any(|young| young.contains(&(body.id(), true))))
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
    /// The heaps put there, in the order put: records the world keeps
    /// ([`World::kept`]), read by their ids, never by a scan.
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
    /// Whether a dawn has been seen: before it, no circle says who is of a
    /// hearth.
    dawned: bool,
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
    heap_of(world, rules, world.item(item)?)
}

/// The heap `item` of a reserve: kept in its ledger, or, the rare one a
/// body took away and let fall there again (its death), lying on the
/// ground.
fn kept_heap(world: &World, rules: &Content, item: ItemId) -> Option<Heap> {
    heap_of(world, rules, world.kept(item).or_else(|| world.item(item))?)
}

/// [`heap`] of an item or a record found.
fn heap_of(world: &World, rules: &Content, found: &crate::Item) -> Option<Heap> {
    let food = rules.material(found.material())?.food?;
    let keeps = u64::from(food.keeps_days) * world.calendar().ticks_per_day();
    // Spoiled as the world says it (D45), read from the item at hand.
    if world.tick().0.saturating_sub(found.made().0) >= keeps {
        return None;
    }
    Some(Heap {
        item: found.id(),
        at: found.pos(),
        kj: u64::from(food.energy_kj(found.mass_g())),
        ends: found.made().0 + keeps,
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
            .filter_map(|&item| kept_heap(world, rules, item))
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
                    .kept(item)
                    .or_else(|| world.item(item))
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
/// kilojoules: for a body past `young_first_until_years` every young one
/// of the hearth; for a young one past `fed_until_years`, the younger fed
/// by the hearth; for those, nobody.
fn young_ahead(world: &World, rules: &Content, reserves: &Reserves, camp: Pos, body: &Body) -> u64 {
    let age = world.age_milli(body);
    if rules.needs.pooling.fed(age) {
        return 0;
    }
    let grown = !rules.needs.pooling.young_first(age);
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
///
/// The hunger bar: very hungry and not kept by its will, or starving, it
/// eats from the reserve despite the young ([`gives_way`]), and tells it.
pub(super) fn eat_at_hearth(
    world: &World,
    rules: &Content,
    (reserves, news): (&Reserves, &mut Vec<News>),
    (id, body, here): (BodyId, &Body, Pos),
    mind: &mut Mind,
    seed: u64,
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
    if holds == 0 {
        return None;
    }
    // The young first, unless its hunger gives way.
    let despite = if holds <= young_ahead(world, rules, reserves, camp, body) {
        Some(gives_way(world, rules, body, mind, seed)?)
    } else {
        None
    };
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
        if let Some(tier) = despite {
            news.push(News::HungerBar {
                body: id,
                starving: tier == HungerTier::Starving,
                reserve: true,
                at: world.tick().0,
            });
        }
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
            item: spare.item,
            camp,
            kj: spare.kj,
            at: world.tick().0,
        });
        return Some(Decision {
            intention: Intention::Keep {
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

/// Whether all a body carries is carried for its hearth (D58): hungry,
/// it eats only what is beyond it.
pub(super) fn holds_only_hearth_food(
    world: &World,
    rules: &Content,
    id: BodyId,
    mind: &Mind,
) -> bool {
    on(rules)
        && mind.hearth_carried_kj > 0
        && super::carried_kj(world, rules, id) <= mind.hearth_carried_kj
}

/// Whether a hungry body may eat from what it carries (D58): what is
/// beyond what it carries for its hearth, or that too when its hunger
/// gives way ([`gives_way`]).
pub(super) fn may_eat_carried(
    world: &World,
    rules: &Content,
    (id, body): (BodyId, &Body),
    mind: &Mind,
    seed: u64,
) -> bool {
    !holds_only_hearth_food(world, rules, id, mind)
        || gives_way(world, rules, body, mind, seed).is_some()
}

/// The share of its fat reserve `body` has left, in per cent: what is
/// left of it (`Condition::reserve_left_j`) against its whole reserve
/// (`Condition::reserve_j`, the content's to its size). 100 with no
/// reserve at all.
pub(super) fn fat_left_percent(rules: &Content, body: &Body) -> u64 {
    let c = body.condition();
    let full = c.reserve_j(&rules.needs);
    if full == 0 {
        return 100;
    }
    c.reserve_left_j(&rules.needs) * 100 / full
}

/// The hunger bar (D58, amended 25 November 2026; the project lead:
/// "quelqu'un qui n'a pas de volonté va manger cette nourriture,
/// quelqu'un qui a beaucoup de volonté va la ramener. Par contre, avoir
/// très très faim, mourir de faim et avoir de la nourriture, ça personne
/// ne le fera jamais"): whether a hungry `body` eats its hearth's food,
/// what it carries for it or what the reserve keeps for the young, and
/// how hungry it is then. By the share of its fat reserve left: hungry,
/// it keeps it (`None`); very hungry, it eats it unless its will keeps it,
/// by `keep_percent_by_will`, drawn once a game day so that it does not
/// change its mind each thought; starving, it eats it. A child the hearth
/// feeds is not concerned.
pub(super) fn gives_way(
    world: &World,
    rules: &Content,
    body: &Body,
    mind: &Mind,
    seed: u64,
) -> Option<HungerTier> {
    let pooling = &rules.needs.pooling;
    if !on(rules) || pooling.fed(world.age_milli(body)) {
        return None;
    }
    match pooling.tier(fat_left_percent(rules, body)) {
        HungerTier::Hungry => None,
        HungerTier::Starving => Some(HungerTier::Starving),
        HungerTier::VeryHungry => {
            let roll = scatter::roll(
                seed,
                purpose::HUNGER_BAR,
                &[u64::from(body.id().0), today(world)],
            ) % 100;
            (roll >= pooling.keep_percent(mind.will)).then_some(HungerTier::VeryHungry)
        }
    }
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
            && world.age_milli(other) < OWN_CHILD_MILLI)
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
        self.reserves.dawned = true;
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
                .filter(|b| pooling.young_first(world.age_milli(b)))
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
                if pooling.fed(world.age_milli(body)) && !world.is_infant(body, rules) {
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
                let cared = if pooling.cares()
                    && !pooling.young_first(world.age_milli(body))
                    && uncovered > 0
                {
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

    /// The camp whose hearth's reserve holds `item`, if one does (D58, for
    /// the vivarium's food balance).
    #[must_use]
    pub fn hearth_holding(&self, item: ItemId) -> Option<Pos> {
        self.reserves.camp_holding(item)
    }

    /// What each hearth's reserve holds now, in kilojoules, by camp (D58,
    /// a measure for the vivarium).
    #[must_use]
    pub fn reserves_kj(&self, world: &World, rules: &Content) -> Vec<(Pos, u64)> {
        self.reserves.all_kj(world, rules)
    }

    /// The camp of the hearth a body whose home is `home` belongs to: its
    /// place's middle, the key of [`Brains::reserves_kj`] (D58, a measure
    /// for the vivarium).
    #[must_use]
    pub fn hearth_camp(rules: &Content, home: Pos) -> Pos {
        camp_of(rules, home)
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
                    } else if let Some(mind) = self.minds.get_mut(body)
                        && mind.hearth_bar_meal.is_some()
                    {
                        // Its hunger gave way (the hunger bar): what it ate
                        // was its hearth's, and it carries for its hearth
                        // no more than it holds now; the rest it still
                        // owes. Only then, so that without the bar nothing
                        // changes.
                        mind.hearth_bar_meal = None;
                        mind.hearth_carried_kj = mind
                            .hearth_carried_kj
                            .min(super::carried_kj(world, rules, *body));
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
                crate::Event::Kept { item, .. } | crate::Event::ItemAdded { item, .. } => {
                    // Food laid at a reserve (kept in its ledger, or what
                    // is left of a heap after a meal; lying on the ground
                    // only when one taken away was let fall there again):
                    // who lives at that hearth
                    // and stands at the camp sees it, and thinks again
                    // (a hungry child waiting there, its gauges unmoved,
                    // would not, and starved beside a full reserve in the
                    // first trial).
                    if let Some(camp) = self.reserves.camp_holding(*item) {
                        for (id, mind) in &mut self.minds {
                            if camp_of(rules, mind.ways.home) == camp
                                && world
                                    .body(*id)
                                    .is_some_and(|b| at_camp(rules, b.pos(), mind))
                            {
                                mind.due = true;
                            }
                        }
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
        // Its hearth: none known yet (before the first dawn), lay down in
        // its camp's circle, or slept away from it (lost on its mother's
        // way, 25 November 2026: fed by no hearth, it fends for itself).
        for (years, pooling, circle, gathers) in [
            (5, true, None, false),
            (5, true, Some(true), false),
            (5, true, Some(false), true),
            (5, false, None, true),
            (9, true, Some(true), true),
        ] {
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
            if let Some(lay) = circle {
                brains.reserves.dawned = true;
                if lay {
                    let camp = camp_of(&rules, brains.minds[&id].ways.home);
                    let fed = rules
                        .needs
                        .pooling
                        .fed(world.age_milli(world.body(id).unwrap()));
                    brains.reserves.young.insert(camp, vec![(id, fed)]);
                }
            }
            let (mut gathered, mut events) = (false, Vec::new());
            for _ in 0..30 {
                events.clear();
                world.step(&rules, &mut events);
                for (_, d) in brains.think(&world, &rules, &events) {
                    gathered |= matches!(d.intention, Intention::Gather { .. });
                    world.queue(d.intention);
                }
            }
            assert_eq!(
                gathered, gathers,
                "{years} years, pooling {pooling}, in its circle {circle:?}"
            );
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
        let Intention::Keep { item, .. } = decision.intention else {
            panic!("it puts down: {decision:?}");
        };
        world.queue(decision.intention.clone());
        world.step(&rules, &mut Vec::new());
        // The ledger: the heap left the world's items for a record kept
        // at the reserve, and nothing lies on the ground.
        assert!(world.item(item).is_none(), "no item any more");
        assert_eq!(world.kept(item).map(crate::Item::mass_g), Some(300));
        assert!(world.items_in(at.chunk()).iter().all(|i| i.id() != item));
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
            !decision.is_some_and(|d| matches!(d.intention, Intention::Keep { .. })),
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
            !may_eat_carried(&world, &rules, (id, world.body(id).unwrap()), mind, 7),
            "all of it is its own's"
        );
        mind.hearth_carried_kj = 500;
        assert!(may_eat_carried(
            &world,
            &rules,
            (id, world.body(id).unwrap()),
            mind,
            7
        ));
        mind.hearth_carried_kj = 1000;
        let decision = think(&world, &rules, &mut brains, id);
        assert!(
            !decision.is_some_and(|d| matches!(d.intention, Intention::Eat { .. })),
            "hungry, it leaves its own's share alone"
        );
    }

    #[test]
    fn a_hungry_child_waiting_at_the_camp_eats_what_is_put_down() {
        let rules = pooled();
        let at = Pos::new(5, 5);
        let (mut world, mut brains, ids) = field(&rules, &[(at, 30), (Pos::new(7, 6), 5)]);
        let (grown, child) = (ids[0], ids[1]);
        let item = world.spawn_item("berries", 400, at);
        world.queue(Intention::PickUp { body: grown, item });
        world.step(&rules, &mut Vec::new());
        hungry(&mut world, &rules, child, 70);
        let mut events = Vec::new();
        // The child waits: nothing to eat, nothing to gather.
        for _ in 0..20 {
            events.clear();
            world.step(&rules, &mut events);
            for (_, d) in brains.think(&world, &rules, &events) {
                world.queue(d.intention);
            }
        }
        // Its mother comes home with 1000 kJ for her own.
        brains.minds.get_mut(&grown).unwrap().hearth_carried_kj = 1000;
        brains.minds.get_mut(&grown).unwrap().due = true;
        let mut ate = false;
        for _ in 0..300 {
            events.clear();
            world.step(&rules, &mut events);
            ate |= events
                .iter()
                .any(|e| matches!(e, crate::Event::BodyAte { body, item: i, .. } if *body == child && *i == item));
            for (_, d) in brains.think(&world, &rules, &events) {
                world.queue(d.intention);
            }
        }
        assert!(ate, "the child saw the food put down and ate it");
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

    #[test]
    fn the_young_first_are_those_under_the_content_s_age_twelve() {
        let at = Pos::new(5, 5);
        // A woman of 30, children of 5, 11 and 13: the hearth's young are
        // those under twelve, the one of 13 eats as the grown do.
        let rules = pooled();
        assert_eq!(rules.needs.pooling.young_first_until_years, 12);
        let (mut world, mut brains, ids) = field(&rules, &[(at, 30), (at, 5), (at, 11), (at, 13)]);
        let (grown, small, eleven, thirteen) = (ids[0], ids[1], ids[2], ids[3]);
        let camp = camp_of(&rules, at);
        brains.see_hearths(&world, &rules, &[(camp, ids.clone())]);
        assert_eq!(
            brains.reserves.young[&camp],
            vec![(small, true), (eleven, false)],
            "under twelve only"
        );
        // A short reserve, the eleven-year-old hungry: the one of 13 leaves
        // it to it as the grown do; the eleven-year-old eats.
        let pooled = world.spawn_item("berries", 100, at);
        brains.reserves.put(camp, at, pooled);
        for id in [grown, eleven, thirteen] {
            hungry(&mut world, &rules, id, 70);
        }
        let eats = |d: Option<Decision>, body| {
            d.is_some_and(|d| d.intention == Intention::Eat { body, item: pooled })
        };
        assert!(!eats(
            think(&world, &rules, &mut brains, thirteen),
            thirteen
        ));
        assert!(!eats(think(&world, &rules, &mut brains, grown), grown));
        assert!(eats(think(&world, &rules, &mut brains, eleven), eleven));
        // Old content, the field absent: sixteen, as before it.
        let old: Pooling = ron::from_str(
            "(fed_until_years: 8, care_percent: (0, 100), own_care_percent: 200, care_days: 2)",
        )
        .unwrap();
        assert_eq!(old.young_first_until_years, 16);
        assert!(old.young_first(15_999) && !old.young_first(16_000));
    }

    /// `id` hungry with `left` per cent of its fat reserve left.
    fn thin(world: &mut World, rules: &Content, id: BodyId, left: u64) {
        let body = world.bodies.iter_mut().find(|b| b.id() == id).unwrap();
        let full = body.condition.reserve_j(&rules.needs);
        body.condition.deficit_j = full * (100 - left) / 100;
        body.condition.hunger = Gauge::at(90);
    }

    /// The pooled content, its hunger bar's will table at its two
    /// extremes: the will-less always eat, the strong-willed always keep.
    fn barred() -> Content {
        let mut rules = pooled();
        rules.needs.pooling.keep_percent_by_will = vec![(0, 0), (100, 100)];
        rules
    }

    /// A grown body far from its camp carrying 1000 kJ of berries, all
    /// of it for its hearth, with `left` per cent of its fat and `will`.
    fn carrying_home(rules: &Content, left: u64, will: u8) -> (World, Brains, BodyId) {
        let at = Pos::new(40, 40);
        let (mut world, mut brains, ids) = field(rules, &[(at, 30)]);
        let id = ids[0];
        let item = world.spawn_item("berries", 400, at);
        world.queue(Intention::PickUp { body: id, item });
        world.step(rules, &mut Vec::new());
        thin(&mut world, rules, id, left);
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.hearth_carried_kj = 1000;
        mind.will = will;
        (world, brains, id)
    }

    fn eats(decision: Option<&Decision>) -> bool {
        decision.is_some_and(|d| matches!(d.intention, Intention::Eat { .. }))
    }

    fn bar_told(news: &[News]) -> Option<(bool, bool)> {
        news.iter().find_map(|n| match n {
            News::HungerBar {
                starving, reserve, ..
            } => Some((*starving, *reserve)),
            _ => None,
        })
    }

    #[test]
    fn the_hunger_bar_is_read_by_the_fat_left() {
        let p = Pooling::placeholders();
        assert_eq!(p.tier(100), HungerTier::Hungry);
        assert_eq!(p.tier(50), HungerTier::Hungry);
        assert_eq!(p.tier(49), HungerTier::VeryHungry);
        assert_eq!(p.tier(25), HungerTier::VeryHungry);
        assert_eq!(p.tier(24), HungerTier::Starving);
        assert_eq!((p.keep_percent(0), p.keep_percent(50)), (10, 50));
        assert_eq!(p.keep_percent(100), 90);
        // Without the bar in the content, nobody is ever more than hungry.
        let old = Pooling {
            very_hungry_fat_percent: 0,
            starving_fat_percent: 0,
            keep_percent_by_will: Vec::new(),
            ..Pooling::placeholders()
        };
        assert_eq!(old.tier(0), HungerTier::Hungry);
        let rules = pooled();
        let (world, _, ids) = field(&rules, &[(Pos::new(5, 5), 30)]);
        let mut world = world;
        thin(&mut world, &rules, ids[0], 40);
        assert_eq!(fat_left_percent(&rules, world.body(ids[0]).unwrap()), 40);
    }

    #[test]
    fn hungry_it_carries_its_hearth_s_food_home() {
        let rules = barred();
        let (world, mut brains, id) = carrying_home(&rules, 60, 0);
        let (decision, news) = think_told(&world, &rules, &mut brains, id);
        assert!(!eats(decision.as_ref()), "hungry, it carries it home");
        assert_eq!(bar_told(&news), None);
    }

    #[test]
    fn very_hungry_it_eats_its_hearth_s_food_or_carries_it_by_its_will() {
        let rules = barred();
        let (world, mut brains, id) = carrying_home(&rules, 40, 100);
        let decision = think(&world, &rules, &mut brains, id);
        assert!(!eats(decision.as_ref()), "a strong will carries it home");
        let (mut world, mut brains, id) = carrying_home(&rules, 40, 0);
        let (decision, news) = think_told(&world, &rules, &mut brains, id);
        let decision = decision.unwrap();
        assert!(eats(Some(&decision)), "no will, it eats: {decision:?}");
        assert_eq!(bar_told(&news), Some((false, false)));
        // Eaten, it carries for its hearth no more than it holds.
        let mut events = Vec::new();
        world.queue(decision.intention);
        world.step(&rules, &mut events);
        brains.think(&world, &rules, &events);
        let held = super::super::carried_kj(&world, &rules, id);
        assert!(held < 1000, "it ate: {held}");
        assert_eq!(brains.minds[&id].hearth_carried_kj, held);
        // The placeholders' draw is made once a game day: a will of 50
        // does not change its mind from one tick to the next.
        let rules = pooled();
        let (mut world, mut brains, id) = carrying_home(&rules, 40, 50);
        let first = gives_way(
            &world,
            &rules,
            world.body(id).unwrap(),
            &brains.minds[&id],
            7,
        );
        for _ in 0..20 {
            world.step(&rules, &mut Vec::new());
            let mind = brains.minds.get_mut(&id).unwrap();
            assert_eq!(
                gives_way(&world, &rules, world.body(id).unwrap(), mind, 7),
                first
            );
        }
    }

    #[test]
    fn starving_it_always_eats_its_hearth_s_food() {
        let rules = barred();
        let (world, mut brains, id) = carrying_home(&rules, 22, 100);
        let (decision, news) = think_told(&world, &rules, &mut brains, id);
        assert!(eats(decision.as_ref()), "starving, even a strong will eats");
        assert_eq!(bar_told(&news), Some((true, false)));
    }

    /// A grown body and a hungry child of five at the camp, the reserve
    /// 250 kJ, less than the child can eat; the grown with `left` per cent
    /// of its fat and `will`.
    fn short_reserve(rules: &Content, left: u64, will: u8) -> (World, Brains, BodyId, ItemId) {
        let at = Pos::new(5, 5);
        let (mut world, mut brains, ids) = field(rules, &[(at, 30), (at, 5)]);
        let (grown, child) = (ids[0], ids[1]);
        let camp = camp_of(rules, at);
        let pooled = world.spawn_item("berries", 100, at);
        brains.reserves.put(camp, at, pooled);
        brains.reserves.young.insert(camp, vec![(child, true)]);
        hungry(&mut world, rules, child, 70);
        thin(&mut world, rules, grown, left);
        brains.minds.get_mut(&grown).unwrap().will = will;
        (world, brains, grown, pooled)
    }

    #[test]
    fn hungry_at_the_camp_it_leaves_a_short_reserve_to_the_young() {
        let rules = barred();
        let (world, mut brains, grown, pooled) = short_reserve(&rules, 60, 0);
        let decision = think(&world, &rules, &mut brains, grown);
        assert!(
            !decision.is_some_and(|d| d.intention
                == Intention::Eat {
                    body: grown,
                    item: pooled
                }),
            "hungry, the young first"
        );
    }

    #[test]
    fn very_hungry_at_the_camp_it_eats_despite_the_young_by_its_will() {
        let rules = barred();
        let (world, mut brains, grown, pooled) = short_reserve(&rules, 40, 100);
        let decision = think(&world, &rules, &mut brains, grown);
        assert!(
            !decision.is_some_and(|d| d.intention
                == Intention::Eat {
                    body: grown,
                    item: pooled
                }),
            "a strong will leaves it to the young"
        );
        let (world, mut brains, grown, pooled) = short_reserve(&rules, 40, 0);
        let (decision, news) = think_told(&world, &rules, &mut brains, grown);
        assert_eq!(
            decision.unwrap().intention,
            Intention::Eat {
                body: grown,
                item: pooled
            }
        );
        assert_eq!(bar_told(&news), Some((false, true)));
    }

    #[test]
    fn a_starving_grown_at_the_camp_eats_from_the_reserve_the_young_would_keep() {
        let rules = barred();
        let (world, mut brains, grown, pooled) = short_reserve(&rules, 22, 100);
        let (decision, news) = think_told(&world, &rules, &mut brains, grown);
        assert_eq!(
            decision.unwrap().intention,
            Intention::Eat {
                body: grown,
                item: pooled
            },
            "starving, nobody leaves food it can eat"
        );
        assert_eq!(bar_told(&news), Some((true, true)));
    }

    #[test]
    fn a_child_the_hearth_feeds_is_not_concerned_by_the_bar() {
        let rules = barred();
        let (mut world, brains, ids) = field(&rules, &[(Pos::new(5, 5), 5)]);
        thin(&mut world, &rules, ids[0], 10);
        let body = world.body(ids[0]).unwrap();
        assert_eq!(
            gives_way(&world, &rules, body, &brains.minds[&ids[0]], 7),
            None
        );
    }
}
