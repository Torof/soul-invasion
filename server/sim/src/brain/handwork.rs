//! The hands in the mind (D57): the deeds a body knows, tries and
//! practises. A deed is a gesture on a kind of thing (a material, or a
//! tree's species for a cut), with a kind of thing in hand for a cut. No
//! recipe: a body in a trial of its hands draws a gesture and a thing from
//! the run's seed among what it carries and what lies around it, and the
//! world says what comes of it; what came of something is learnt, judged
//! with the trial (D50), and told at the fire (D55). A body redoes the
//! deeds it knows when it lacks what they gave: the plain minimum, never
//! a goal.

use crate::content::Content;
use crate::r#gen::scatter::{self, purpose};
use crate::hands::Gesture;
use crate::map::{ChunkPos, FinePos, Pos};
use crate::trees::TreeKey;
use crate::{BodyId, GATHER_REACH_MM, Intention, Item, ItemId, World};

use super::{AROUND, Decision, Doing, Mind, Way, Why, today, walkable};

/// The most deeds a body holds in its ways.
pub const DEEDS: usize = 6;

/// No thing: a deed done with nothing in hand.
pub const NOTHING: u16 = u16::MAX;

/// A deed of the hands (D57): a gesture on a kind of thing, with a kind
/// of thing in hand when it cuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Deed {
    /// The gesture.
    pub gesture: Gesture,
    /// What it is done on: a material's index in the content (take,
    /// fold), or a tree species' index (cut).
    pub on: u16,
    /// The material in hand it cuts with; [`NOTHING`] otherwise.
    pub with: u16,
}

/// The deeds a body knows (D57), at most [`DEEDS`], kept in order so that
/// two sets of the same deeds are equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Deeds {
    known: [Option<Deed>; DEEDS],
}

impl Deeds {
    /// Its deeds.
    pub fn iter(&self) -> impl Iterator<Item = Deed> + '_ {
        self.known.iter().flatten().copied()
    }

    /// Whether it holds `deed`.
    #[must_use]
    pub fn contains(&self, deed: Deed) -> bool {
        self.iter().any(|d| d == deed)
    }

    /// Whether it holds none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.known[0].is_none()
    }

    /// How many it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// Whether every deed of it is in `other`.
    #[must_use]
    pub fn within(&self, other: &Deeds) -> bool {
        self.iter().all(|d| other.contains(d))
    }

    /// Adds `deed`; false when it held it already or holds [`DEEDS`].
    pub fn add(&mut self, deed: Deed) -> bool {
        if self.contains(deed) {
            return false;
        }
        let Some(free) = self.known.iter_mut().find(|d| d.is_none()) else {
            return false;
        };
        *free = Some(deed);
        self.known.sort_by_key(|d| (d.is_none(), *d));
        true
    }
}

/// What a body steps to, to do a deed on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HandTarget {
    /// A thing lying on the ground, to pick up.
    Item(ItemId, Pos),
    /// A tree to cut, with the thing it carries.
    Tree {
        chunk: ChunkPos,
        key: TreeKey,
        at: FinePos,
        with: ItemId,
    },
}

/// A thing it could not reach or work today: left alone till tomorrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Barred {
    /// A thing on the ground.
    Item(ItemId),
    /// A tree.
    Tree(ChunkPos, TreeKey),
}

impl HandTarget {
    fn barred(self) -> Barred {
        match self {
            HandTarget::Item(item, _) => Barred::Item(item),
            HandTarget::Tree { chunk, key, .. } => Barred::Tree(chunk, key),
        }
    }
}

/// The hands' part of a mind (D57).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Handwork {
    /// The deeds it heard told and tries, in a trial of its hands begun
    /// on a tale (D55); empty when it draws its gestures.
    pub(super) aim: Deeds,
    /// The day of its last drawn gesture, and how many it drew that day.
    pub(super) tries: (u64, u32),
    /// What it steps to, and the deed it would do there.
    pub(super) to: Option<(HandTarget, Deed)>,
    /// The deed it is doing, on what (the thing picked, the tool, the
    /// thing folded): learnt when something comes of it.
    pub(super) pending: Option<(Deed, ItemId)>,
    /// What came of its last cut or fold: something, or nothing.
    pub(super) done: Option<bool>,
    /// The last thing it set to work on: left alone today if refused.
    pub(super) last: Option<HandTarget>,
    /// The things it could not reach or work today, and the day.
    pub(super) barred: (u64, Vec<Barred>),
    /// How many times it did each deed with something coming of it: its
    /// mastery, in its own body (the lead's knowledge model, 22 November
    /// 2026: no levels, practised gestures).
    pub practised: Vec<(Deed, u32)>,
}

impl Handwork {
    /// The times it did `deed`.
    #[must_use]
    pub fn practised(&self, deed: Deed) -> u32 {
        self.practised
            .iter()
            .find(|(d, _)| *d == deed)
            .map_or(0, |(_, n)| *n)
    }

    /// Whether it left `thing` alone today.
    fn barred(&self, thing: Barred) -> bool {
        self.barred.1.contains(&thing)
    }

    /// Leaves `thing` alone for the rest of `day`.
    pub(super) fn bar(&mut self, day: u64, thing: Barred) {
        if self.barred.0 != day {
            self.barred = (day, Vec::new());
        }
        if !self.barred.1.contains(&thing) {
            self.barred.1.push(thing);
        }
    }

    /// Its last thing set to work on, refused: left alone today.
    pub(super) fn refused(&mut self, day: u64) {
        if let Some(target) = self.last.take() {
            self.bar(day, target.barred());
        }
        self.pending = None;
    }

    /// Forgets what it was about.
    pub(super) fn drop_aim(&mut self) {
        self.aim = Deeds::default();
        self.to = None;
        self.pending = None;
        self.done = None;
    }
}

/// A material's index in the content.
/// The deeds that make a container, read from the content alone: take a
/// sharp material, cut a tree whose bark folds into a container with it,
/// fold that bark. Not the experiment's: its diagnostic (the vivarium's
/// `--hands-known`), founders who know them from birth. Empty when the
/// content holds no such chain.
#[must_use]
pub fn container_deeds(rules: &Content) -> Deeds {
    let mut deeds = Deeds::default();
    let index = |id: &str| material_index(rules, id);
    let Some(sharp) = rules.materials.iter().position(|m| m.sharp) else {
        return deeds;
    };
    let Ok(sharp) = u16::try_from(sharp) else {
        return deeds;
    };
    for (s, species) in rules.species.iter().enumerate() {
        let Some(bark) = species.bark.as_deref().and_then(index) else {
            continue;
        };
        let holds = folds_into(rules, bark)
            .and_then(|m| rules.materials.get(usize::from(m)))
            .is_some_and(|m| m.holds_g > 0);
        let Ok(s) = u16::try_from(s) else {
            continue;
        };
        if holds {
            for deed in [
                Deed {
                    gesture: Gesture::Take,
                    on: sharp,
                    with: NOTHING,
                },
                Deed {
                    gesture: Gesture::Cut,
                    on: s,
                    with: sharp,
                },
                Deed {
                    gesture: Gesture::Fold,
                    on: bark,
                    with: NOTHING,
                },
            ] {
                deeds.add(deed);
            }
            return deeds;
        }
    }
    deeds
}

fn material_index(rules: &Content, id: &str) -> Option<u16> {
    rules
        .materials
        .iter()
        .position(|m| m.id == id)
        .and_then(|i| u16::try_from(i).ok())
}

/// What `material` folds into, as an index.
fn folds_into(rules: &Content, material: u16) -> Option<u16> {
    let into = rules
        .materials
        .get(usize::from(material))?
        .folds_into
        .as_deref()?;
    material_index(rules, into)
}

/// The thing a deed gives: the thing picked, the bark cut, the thing
/// folded into.
fn product(rules: &Content, deed: Deed) -> Option<u16> {
    match deed.gesture {
        Gesture::Take => Some(deed.on),
        Gesture::Cut => {
            let bark = rules.species.get(usize::from(deed.on))?.bark.as_deref()?;
            material_index(rules, bark)
        }
        Gesture::Fold => folds_into(rules, deed.on),
    }
}

/// `material` and what the folds of `deeds` turn it into.
fn descendants(rules: &Content, deeds: &Deeds, material: u16) -> Vec<u16> {
    let mut out = vec![material];
    let mut i = 0;
    while i < out.len() && out.len() <= DEEDS {
        let m = out[i];
        for deed in deeds.iter() {
            if deed.gesture == Gesture::Fold
                && deed.on == m
                && let Some(into) = folds_into(rules, m)
                && !out.contains(&into)
            {
                out.push(into);
            }
        }
        i += 1;
    }
    out
}

/// The materials `deeds` use or give: kept, the rest let go.
fn used(rules: &Content, deeds: &Deeds) -> Vec<u16> {
    let mut out = Vec::new();
    for deed in deeds.iter() {
        if deed.gesture != Gesture::Cut {
            out.push(deed.on);
        }
        if deed.with != NOTHING {
            out.push(deed.with);
        }
        if let Some(p) = product(rules, deed) {
            out.extend(descendants(rules, deeds, p));
        }
    }
    out
}

/// What a body carries: each item with its material's index.
fn carried(world: &World, rules: &Content, id: BodyId) -> Vec<(ItemId, u16)> {
    let Some(body) = world.body(id) else {
        return Vec::new();
    };
    body.inventory()
        .filter_map(|item| {
            let m = material_index(rules, world.item(item)?.material())?;
            Some((item, m))
        })
        .collect()
}

/// The things lying within `near` tiles of `here`, nearest first, but
/// those it could not reach today.
fn things_near(world: &World, here: Pos, near: i32, mind: &Mind) -> Vec<Item> {
    let mut out: Vec<Item> = world
        .items_near(here, near)
        .into_iter()
        .filter(|i| !mind.handwork.barred(Barred::Item(i.id())))
        .collect();
    out.sort_by_key(|i| (i.pos().map(|p| super::far(p, here)), i.id()));
    out
}

/// The trees standing within `near` tiles of `here`, nearest first: their
/// chunk, key, trunk and species.
fn trees_near(
    world: &World,
    here: Pos,
    near: i32,
    mind: &Mind,
) -> Vec<(ChunkPos, TreeKey, FinePos, usize)> {
    let mut out: Vec<(ChunkPos, TreeKey, FinePos, usize)> = chunks_near(here, near)
        .into_iter()
        .flat_map(|c| {
            world
                .trees_in(c)
                .iter()
                .map(|t| (c, t.key, t.at, t.species))
                .collect::<Vec<_>>()
        })
        .filter(|(chunk, key, at, _)| {
            let p = at.tile();
            (p.x - here.x).abs() <= near
                && (p.y - here.y).abs() <= near
                && !mind.handwork.barred(Barred::Tree(*chunk, *key))
        })
        .collect();
    out.sort_by_key(|(_, key, at, _)| (super::far(at.tile(), here), *key));
    out
}

/// The chunks a square of `near` tiles round `here` touches.
fn chunks_near(here: Pos, near: i32) -> Vec<ChunkPos> {
    let (low, high) = (
        Pos::new(here.x - near, here.y - near).chunk(),
        Pos::new(here.x + near, here.y + near).chunk(),
    );
    let mut out = Vec::new();
    for y in low.y..=high.y {
        for x in low.x..=high.x {
            out.push(ChunkPos::new(x, y));
        }
    }
    out
}

/// What a body's hands do at a thought (D57).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum HandsTurn {
    /// Nothing for its hands: its day goes on.
    Free,
    /// It does this (a step, a gesture), or goes on with it (`None`).
    Decide(Option<Decision>),
}

/// What `id` does with its hands now (D57), when it is not called to
/// drink, eat or sleep: let go what its hands no longer know, a deed it
/// knows or heard when it lacks what the deed gives, or, in a trial of its
/// hands on nothing heard, a gesture drawn.
pub(super) fn hands_turn(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    mind: &mut Mind,
    seed: u64,
) -> HandsTurn {
    let hands = &rules.needs.hands;
    let Some(body) = world.body(id).filter(|_| hands.on()) else {
        return HandsTurn::Free;
    };
    if body.making().is_some() {
        return HandsTurn::Decide(None);
    }
    resolve(world, id, mind);
    let day = today(world);
    if mind.handwork.barred.0 != day {
        mind.handwork.barred = (day, Vec::new());
    }
    let trying = mind.trial.is_some_and(|t| t.way == Way::Hands);
    let have = carried(world, rules, id);
    // What its hands no longer know is let go, one thing a thought: a
    // trial given up leaves what it brought.
    if !trying {
        let keep = used(rules, &mind.ways.hands);
        let food = |m: u16| {
            rules
                .materials
                .get(usize::from(m))
                .is_some_and(|m| m.food.is_some())
        };
        if let Some(&(item, _)) = have.iter().find(|(_, m)| !food(*m) && !keep.contains(m)) {
            mind.handwork.to = None;
            return HandsTurn::Decide(Some(Decision {
                intention: Intention::Drop { body: id, item },
                why: Why::Hands,
            }));
        }
    }
    if mind.doing == Doing::ToHands {
        if !mind.stopped {
            return HandsTurn::Decide(None);
        }
        mind.doing = Doing::Nothing;
        if let Some((target, deed)) = mind.handwork.to.take() {
            if let Some(decision) = act_on(world, id, here, target, deed, mind) {
                return HandsTurn::Decide(Some(decision));
            }
            // Stopped short of it: left alone today.
            mind.handwork.bar(day, target.barred());
        }
    }
    let near = i32::try_from(hands.near_tiles).unwrap_or(4);
    // The deeds it knows and those it heard and tries, the later steps
    // first, when it lacks what they give.
    let mut deeds = mind.ways.hands;
    for deed in mind.handwork.aim.iter() {
        deeds.add(deed);
    }
    let found = if deeds.is_empty() {
        None
    } else {
        needed(world, rules, (id, here), (&deeds, &have, near), mind)
    };
    let found = found.or_else(|| {
        (trying && mind.handwork.aim.is_empty())
            .then(|| draw(world, rules, (id, here), mind, (seed, &have, near)))
            .flatten()
    });
    match found {
        Some(todo) => HandsTurn::Decide(Some(go_do(world, rules, (id, here), todo, mind))),
        None => HandsTurn::Free,
    }
}

/// A gesture drawn in a trial of the hands on nothing heard (D57): one of
/// the three, on a thing drawn among those it carries or that lie around
/// it, at most the content's tries a day. No suggestion: any thing, any
/// tool.
fn draw(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    mind: &mut Mind,
    (seed, have, near): (u64, &[(ItemId, u16)], i32),
) -> Option<(HandTarget, Deed)> {
    let day = today(world);
    if mind.handwork.tries.0 != day {
        mind.handwork.tries = (day, 0);
    }
    if mind.handwork.tries.1 >= rules.needs.hands.tries_per_day {
        return None;
    }
    let keys = [u64::from(id.0), day, u64::from(mind.handwork.tries.1)];
    mind.handwork.tries.1 += 1;
    let r = scatter::roll(seed, purpose::HANDS_GESTURE, &keys);
    let pick = |n: usize, salt: u64| {
        let r = scatter::roll(
            seed,
            purpose::HANDS_THING,
            &[keys[0], keys[1], keys[2], salt],
        );
        usize::try_from(r % u64::try_from(n.max(1)).unwrap_or(1)).unwrap_or(0)
    };
    let gesture = Gesture::ALL[usize::try_from(r % 3).unwrap_or(0)];
    let deed = |on, with| Deed { gesture, on, with };
    match gesture {
        Gesture::Take => {
            let things = things_near(world, here, near, mind);
            let thing = things.get(pick(things.len(), 0))?;
            let on = material_index(rules, thing.material())?;
            Some((
                HandTarget::Item(thing.id(), thing.pos()?),
                deed(on, NOTHING),
            ))
        }
        Gesture::Cut => {
            let trees = trees_near(world, here, near, mind);
            let &(chunk, key, at, species) = trees.get(pick(trees.len(), 0))?;
            let &(with, with_m) = have.get(pick(have.len(), 1))?;
            Some((
                HandTarget::Tree {
                    chunk,
                    key,
                    at,
                    with,
                },
                deed(u16::try_from(species).ok()?, with_m),
            ))
        }
        Gesture::Fold => {
            let &(item, on) = have.get(pick(have.len(), 0))?;
            Some((HandTarget::Item(item, here), deed(on, NOTHING)))
        }
    }
}

/// The first deed of `deeds` it lacks what it gives and can do here: a
/// fold of a thing it carries, a cut of a tree near with a thing it
/// carries, a thing near to pick up.
fn needed(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    (deeds, have, near): (&Deeds, &[(ItemId, u16)], i32),
    mind: &Mind,
) -> Option<(HandTarget, Deed)> {
    let lacks = |deed: Deed| {
        product(rules, deed).is_some_and(|p| {
            descendants(rules, deeds, p)
                .iter()
                .all(|m| !have.iter().any(|(_, h)| h == m))
        })
    };
    let held = world.body(id)?.held();
    for gesture in [Gesture::Fold, Gesture::Cut, Gesture::Take] {
        for deed in deeds.iter().filter(|d| d.gesture == gesture) {
            if !lacks(deed) {
                continue;
            }
            match gesture {
                Gesture::Fold => {
                    if let Some(&(item, _)) = have.iter().find(|(_, m)| *m == deed.on) {
                        return Some((HandTarget::Item(item, here), deed));
                    }
                }
                Gesture::Cut => {
                    // The tool in hand first.
                    let mut tools: Vec<ItemId> = have
                        .iter()
                        .filter(|(_, m)| *m == deed.with)
                        .map(|(i, _)| *i)
                        .collect();
                    tools.sort_by_key(|i| Some(*i) != held);
                    let Some(&with) = tools.first() else {
                        continue;
                    };
                    if let Some(&(chunk, key, at, _)) = trees_near(world, here, near, mind)
                        .iter()
                        .find(|t| usize::from(deed.on) == t.3)
                    {
                        return Some((
                            HandTarget::Tree {
                                chunk,
                                key,
                                at,
                                with,
                            },
                            deed,
                        ));
                    }
                }
                Gesture::Take => {
                    if let Some(thing) = things_near(world, here, near, mind)
                        .iter()
                        .find(|i| material_index(rules, i.material()) == Some(deed.on))
                    {
                        return Some((HandTarget::Item(thing.id(), thing.pos()?), deed));
                    }
                }
            }
        }
    }
    None
}

/// Does `deed` on `target` if within reach, else steps there.
fn go_do(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    (target, deed): (HandTarget, Deed),
    mind: &mut Mind,
) -> Decision {
    if deed.gesture == Gesture::Fold
        && let HandTarget::Item(item, _) = target
    {
        mind.handwork.pending = Some((deed, item));
        mind.handwork.done = None;
        return Decision {
            intention: Intention::Fold { body: id, item },
            why: Why::Hands,
        };
    }
    if let Some(decision) = act_on(world, id, here, target, deed, mind) {
        return decision;
    }
    let to = match target {
        HandTarget::Item(_, pos) => pos,
        HandTarget::Tree { at, .. } => {
            // A trunk takes its tile: it stands beside it.
            let tile = at.tile();
            AROUND
                .iter()
                .filter(|&&(dx, dy)| (dx, dy) != (0, 0))
                .map(|&(dx, dy)| Pos::new(tile.x + dx, tile.y + dy))
                .filter(|&p| walkable(world.map(), rules, p))
                .min_by_key(|&p| (super::far(p, here), p.y, p.x))
                .unwrap_or(tile)
        }
    };
    mind.doing = Doing::ToHands;
    mind.handwork.to = Some((target, deed));
    Decision {
        intention: Intention::MoveTo {
            body: id,
            target: to,
        },
        why: Why::Hands,
    }
}

/// The gesture on `target`, when it is within reach.
fn act_on(
    world: &World,
    id: BodyId,
    here: Pos,
    target: HandTarget,
    deed: Deed,
    mind: &mut Mind,
) -> Option<Decision> {
    let body = world.body(id)?;
    let intention = match target {
        HandTarget::Item(item, pos) => {
            if (pos.x - here.x).abs() > 1 || (pos.y - here.y).abs() > 1 {
                return None;
            }
            mind.handwork.pending = Some((deed, item));
            Intention::PickUp { body: id, item }
        }
        HandTarget::Tree {
            chunk,
            key,
            at,
            with,
        } => {
            let (dx, dy) = (i64::from(body.at().x - at.x), i64::from(body.at().y - at.y));
            if dx * dx + dy * dy > GATHER_REACH_MM * GATHER_REACH_MM || !body.has(with) {
                return None;
            }
            mind.handwork.pending = Some((deed, with));
            Intention::Cut {
                body: id,
                with,
                chunk,
                tree: key,
            }
        }
    };
    mind.handwork.done = None;
    mind.handwork.last = Some(target);
    Some(Decision {
        intention,
        why: Why::Hands,
    })
}

/// The judge of a trial (D50), the hands' rule added (D57): a trial of
/// the hands in which nothing came of anything is not kept, whatever the
/// days; the deeds tried are forgotten with it.
pub(super) fn judge_trial(
    rules: &Content,
    per_year: u32,
    body: &crate::Body,
    mind: &mut Mind,
    trial: &super::Trial,
) -> (bool, super::Judged) {
    let (mut kept, mut judged) = super::judge(rules, per_year, body, mind, trial);
    if trial.way == Way::Hands {
        kept &= mind.ways.hands != trial.before.hands;
        judged.kept = kept;
        mind.handwork.drop_aim();
    }
    (kept, judged)
}

/// What came of the deed it was doing: learnt, in a trial of its hands,
/// when something came of it (D57), its trial's days counted from today
/// so that the judge sees the days with the new thing; practised in any
/// case.
fn resolve(world: &World, id: BodyId, mind: &mut Mind) {
    let Some((deed, item)) = mind.handwork.pending else {
        return;
    };
    let Some(body) = world.body(id) else {
        return;
    };
    // Not done yet (the act waits for the next tick): it stays pending,
    // until done or another deed is begun.
    let worked = match deed.gesture {
        Gesture::Take if body.has(item) => true,
        Gesture::Take => return,
        Gesture::Cut | Gesture::Fold => match mind.handwork.done.take() {
            Some(gave) => gave,
            None => return,
        },
    };
    mind.handwork.pending = None;
    if !worked {
        return;
    }
    match mind.handwork.practised.iter_mut().find(|(d, _)| *d == deed) {
        Some((_, n)) => *n += 1,
        None => mind.handwork.practised.push((deed, 1)),
    }
    if let Some(trial) = mind.trial.as_mut().filter(|t| t.way == Way::Hands)
        && mind.ways.hands.add(deed)
    {
        trial.started_day = today(world);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Brains, HeardDays, Lately, News, Trial, Ways, start_trial};
    use super::*;
    use crate::memory::{Access, Record};
    use crate::time::Calendar;

    /// The repository's content, a valley of minute ticks, a body beside
    /// a birch with a mind: the rules, world, brains, body, and the
    /// birch's species index.
    fn by_a_birch() -> (Content, World, Brains, BodyId, u16) {
        let rules = crate::crops::tests::content();
        let mut world = World::with_valley(Calendar::new(1, 1440).unwrap(), &rules, 768);
        let birch = rules.species.iter().position(|s| s.id == "birch").unwrap();
        let side = i32::try_from(world.map().chunks_wide()).unwrap();
        let mut stand = None;
        'search: for y in 0..side {
            for x in 0..side {
                for tree in world.trees_in(ChunkPos::new(x, y)).iter() {
                    if tree.species != birch {
                        continue;
                    }
                    let trunk = tree.at.tile();
                    if let Some(p) = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .into_iter()
                        .map(|(dx, dy)| Pos::new(trunk.x + dx, trunk.y + dy))
                        .find(|&p| {
                            world.map().is_passable(p)
                                && !world.trees_in(p.chunk()).iter().any(|t| t.at.tile() == p)
                        })
                    {
                        stand = Some(p);
                        break 'search;
                    }
                }
            }
        }
        let id = world.spawn_body(stand.expect("a birch with room beside it"));
        let mut brains = Brains::new(7);
        brains.adopt(&world, &rules, id);
        (rules, world, brains, id, u16::try_from(birch).unwrap())
    }

    fn material(rules: &Content, id: &str) -> u16 {
        material_index(rules, id).unwrap()
    }

    /// Lays `material` at the body's feet and has it picked up.
    fn picked(world: &mut World, rules: &Content, id: BodyId, material: &str) -> ItemId {
        let pos = world.body(id).unwrap().pos();
        let grams = rules.material(material).unwrap().mass_g();
        let item = world.spawn_item(material, grams, pos);
        let mut events = Vec::new();
        world.queue(Intention::PickUp { body: id, item });
        world.advance(1, rules, &mut events);
        item
    }

    /// The three deeds of the bark: take a flint, cut a birch with it,
    /// fold the bark.
    fn bark_deeds(rules: &Content, birch: u16) -> Deeds {
        let flint = material(rules, "flint");
        let mut deeds = Deeds::default();
        for deed in [
            Deed {
                gesture: Gesture::Fold,
                on: material(rules, "birch_bark"),
                with: NOTHING,
            },
            Deed {
                gesture: Gesture::Take,
                on: flint,
                with: NOTHING,
            },
            Deed {
                gesture: Gesture::Cut,
                on: birch,
                with: flint,
            },
        ] {
            assert!(deeds.add(deed));
        }
        deeds
    }

    #[test]
    fn the_deeds_of_a_container_are_read_from_the_content() {
        let (rules, _, _, _, birch) = by_a_birch();
        assert_eq!(container_deeds(&rules), bark_deeds(&rules, birch));
        assert!(container_deeds(&Content::sample()).is_empty());
    }

    #[test]
    fn deeds_are_a_set() {
        let deed = |on| Deed {
            gesture: Gesture::Take,
            on,
            with: NOTHING,
        };
        let (mut a, mut b) = (Deeds::default(), Deeds::default());
        assert!(a.is_empty());
        assert!(a.add(deed(1)) && a.add(deed(2)));
        assert!(b.add(deed(2)) && b.add(deed(1)));
        assert_eq!(a, b, "the same deeds in any order");
        assert!(!a.add(deed(1)), "known already");
        assert!(a.within(&b) && b.within(&a));
        for on in 3..=u16::try_from(DEEDS).unwrap() {
            assert!(a.add(deed(on)));
        }
        assert!(!a.add(deed(99)), "full");
        assert_eq!(a.len(), DEEDS);
    }

    #[test]
    fn a_body_that_knows_the_bark_and_lacks_a_container_makes_one_and_then_rests_its_hands() {
        let (rules, mut world, mut brains, id, birch) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let flint = picked(&mut world, &rules, id, "flint");
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.ways.hands = bark_deeds(&rules, birch);
        // Beside the birch, the flint carried: it cuts.
        let HandsTurn::Decide(Some(decision)) = hands_turn(&world, &rules, (id, here), mind, 7)
        else {
            panic!("a deed");
        };
        assert!(
            matches!(decision.intention, Intention::Cut { with, .. } if with == flint),
            "{decision:?}"
        );
        assert_eq!(decision.why, Why::Hands);
        let mut events = Vec::new();
        world.queue(decision.intention);
        world.advance(12, &rules, &mut events);
        brains.notice(&events, &world);
        let mind = brains.minds.get_mut(&id).unwrap();
        // The bark carried: it folds it.
        let HandsTurn::Decide(Some(decision)) = hands_turn(&world, &rules, (id, here), mind, 7)
        else {
            panic!("a deed");
        };
        let Intention::Fold { item, .. } = decision.intention else {
            panic!("{decision:?}");
        };
        assert_eq!(world.item(item).unwrap().material(), "birch_bark");
        events.clear();
        world.queue(decision.intention);
        world.advance(12, &rules, &mut events);
        brains.notice(&events, &world);
        let mind = brains.minds.get_mut(&id).unwrap();
        // A container carried: nothing more to do with its hands.
        assert_eq!(
            hands_turn(&world, &rules, (id, here), mind, 7),
            HandsTurn::Free
        );
        assert_eq!(world.item(item).unwrap().material(), "folded_birch_bark");
        // Practised, outside any trial: nothing learnt.
        let cut = Deed {
            gesture: Gesture::Cut,
            on: birch,
            with: material(&rules, "flint"),
        };
        assert_eq!(mind.handwork.practised(cut), 1);
        assert_eq!(mind.ways.hands, bark_deeds(&rules, birch));
    }

    #[test]
    fn what_its_hands_no_longer_know_is_let_go() {
        let (rules, mut world, mut brains, id, _) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let pebble = picked(&mut world, &rules, id, "pebble");
        picked(&mut world, &rules, id, "berries");
        let mind = brains.minds.get_mut(&id).unwrap();
        let HandsTurn::Decide(Some(decision)) = hands_turn(&world, &rules, (id, here), mind, 7)
        else {
            panic!("a deed");
        };
        assert_eq!(
            decision.intention,
            Intention::Drop {
                body: id,
                item: pebble
            },
            "the pebble, not the food"
        );
    }

    #[test]
    fn a_trial_of_the_hands_learns_only_what_came_to_something() {
        let (rules, world, mut brains, id, birch) = by_a_birch();
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.trial = Some(Trial {
            way: Way::Hands,
            before: mind.ways,
            started_day: 0,
            before_days: Lately::default(),
            heard: HeardDays::default(),
            caller: None,
        });
        let cut = Deed {
            gesture: Gesture::Cut,
            on: birch,
            with: material(&rules, "pebble"),
        };
        mind.handwork.pending = Some((cut, ItemId(1)));
        mind.handwork.done = Some(false);
        resolve(&world, id, mind);
        assert!(mind.ways.hands.is_empty(), "a cut with a pebble: nothing");
        let fold = Deed {
            gesture: Gesture::Fold,
            on: material(&rules, "birch_bark"),
            with: NOTHING,
        };
        mind.handwork.pending = Some((fold, ItemId(1)));
        // Not done yet: it waits.
        resolve(&world, id, mind);
        assert_eq!(mind.handwork.pending, Some((fold, ItemId(1))));
        mind.handwork.done = Some(true);
        resolve(&world, id, mind);
        assert!(mind.ways.hands.contains(fold));
        assert_eq!(mind.handwork.practised(fold), 1);
        assert_eq!(
            mind.trial.unwrap().started_day,
            today(&world),
            "judged on the days with the new thing"
        );
    }

    #[test]
    fn a_deed_heard_is_tried_never_copied() {
        let (rules, world, mut brains, id, birch) = by_a_birch();
        let mut theirs = Ways::at(Pos::new(5, 5));
        theirs.hands = bark_deeds(&rules, birch);
        let (record, _) = brains.memory.intern(Record::Trial {
            body: BodyId(99),
            day: 1,
            way: Way::Hands,
            after: theirs,
            up: true,
            kept: true,
            will: 50,
            before: Lately::default(),
            during: Lately::default(),
        });
        let mut mind = brains.minds.get(&id).unwrap().clone();
        mind.heard_ways.push(Access {
            record,
            by: BodyId(99),
            day: 1,
        });
        let news = start_trial(&world, &rules, (&brains.memory, 7), id, &mut mind, 2);
        assert!(
            matches!(
                news,
                Some(News::Trying {
                    way: Way::Hands,
                    heard: true,
                    ..
                })
            ),
            "{news:?}"
        );
        assert!(mind.ways.hands.is_empty(), "hearing does not make able");
        assert_eq!(mind.handwork.aim, theirs.hands, "it tries what it heard");
        assert_eq!(mind.trial.map(|t| t.way), Some(Way::Hands));
    }
}
