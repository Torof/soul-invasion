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
use crate::memory::{Record, Store};
use crate::trees::TreeKey;
use crate::{BodyId, GATHER_REACH_MM, Intention, ItemId, World};

use super::{AROUND, Decision, Doing, Mind, News, Way, Why, today, walkable};

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
    /// The kinds of thing that once served it (the lead, 25 November
    /// 2026: "c'est propre d'un cerveau"): a tool whose cut gave
    /// something, a container that held food beyond its arms. Its own
    /// memory of use, no perception: such a thing is kept, never let go.
    pub served: Vec<u16>,
    /// The kinds of thing its hands made or took (D57, 25 November 2026):
    /// one not among them is a novelty, told at the fire that evening.
    pub(super) made_kinds: Vec<u16>,
    /// The novelties to tell at its next fire: the deed and what it made.
    pub(super) novel: Vec<(Deed, u16)>,
    /// The heard deeds it set out to try, and the day: such a deed is
    /// tried again on the extra chance only once heard again after it.
    pub(super) tried_heard: Vec<(Deed, u64)>,
    /// In the trial of its hands under way, it began a gesture its hands
    /// did not know at the trial's start (D57, amended 25 November 2026):
    /// without one, it could not try, and the trial is not judged.
    pub(super) gestured: bool,
    /// In the trial of its hands under way, it drew a gesture at random.
    pub(super) drew: bool,
    /// The thing it set out to take up as the tool a cut it knows or
    /// heard names (D57, amended 25 November 2026).
    pub(super) fetch: Option<ItemId>,
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

    /// Whether a thing of `material` once served it.
    #[must_use]
    pub fn served(&self, material: u16) -> bool {
        self.served.contains(&material)
    }

    /// A thing of `material` served it.
    fn serve(&mut self, material: u16) {
        if !self.served.contains(&material) {
            self.served.push(material);
        }
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

    /// Forgets what it was about: at a trial's start or end.
    pub(super) fn drop_aim(&mut self) {
        self.aim = Deeds::default();
        self.to = None;
        self.pending = None;
        self.done = None;
        self.gestured = false;
        self.drew = false;
        self.fetch = None;
    }
}

/// The deeds of the hands `mind` heard told and does not know (D57, 25
/// November 2026), each with the number of different tellers it heard it
/// from: a novelty told, or a kept trial of the hands. A deed it set out
/// to try is left out until heard again after that day.
pub(super) fn heard_deeds(memory: &Store, mind: &Mind) -> Vec<(Deed, u32)> {
    let mut heard: Vec<(Deed, Vec<BodyId>, u64)> = Vec::new();
    let mut add = |deed: Deed, by: BodyId, day: u64| {
        if mind.ways.hands.contains(deed) {
            return;
        }
        match heard.iter_mut().find(|(d, ..)| *d == deed) {
            Some((_, tellers, last)) => {
                if !tellers.contains(&by) {
                    tellers.push(by);
                }
                *last = (*last).max(day);
            }
            None => heard.push((deed, vec![by], day)),
        }
    };
    for access in &mind.heard_ways {
        let day = u64::from(access.day);
        match memory.get(access.record) {
            Some(Record::Deed { deed, .. }) => add(*deed, access.by, day),
            Some(Record::Trial {
                way: Way::Hands,
                kept: true,
                after,
                problem,
                ..
            }) if problem.is_none_or(|k| mind.carries(k)) => {
                // Told with a problem: only a listener living the same
                // problem has a reason to try it (D50, amended 26
                // November 2026).
                for deed in after.hands.iter() {
                    add(deed, access.by, day);
                }
            }
            _ => {}
        }
    }
    heard
        .into_iter()
        .filter(|(deed, _, last)| {
            !mind
                .handwork
                .tried_heard
                .iter()
                .any(|(d, tried)| d == deed && tried >= last)
        })
        .map(|(deed, tellers, _)| (deed, u32::try_from(tellers.len()).unwrap_or(u32::MAX)))
        .collect()
}

/// The heard deeds `mind` sets out to try on `day`, as its aim: noted
/// tried, so that the extra chance does not draw them again until heard
/// again.
pub(super) fn aim_at(mind: &mut Mind, deeds: &[(Deed, u32)], day: u64) {
    mind.handwork.drop_aim();
    for &(deed, _) in deeds {
        mind.handwork.aim.add(deed);
        match mind
            .handwork
            .tried_heard
            .iter_mut()
            .find(|(d, _)| *d == deed)
        {
            Some((_, tried)) => *tried = day,
            None => mind.handwork.tried_heard.push((deed, day)),
        }
    }
}

/// The extra chance, in per mille, that `mind` sets out on `day` to try
/// the deeds it heard (D57, 25 November 2026: a deed heard is tried more
/// readily than an invention): its usual chance by its will (in distress
/// the percent, else the calm per mille, at least the floor of every
/// trial) times the factor of the most tellers any of them was
/// heard from, at most 1,000. 0 with nothing heard.
pub(super) fn heard_chance(rules: &Content, mind: &Mind, deeds: &[(Deed, u32)]) -> u64 {
    let Some(tellers) = deeds.iter().map(|(_, n)| *n).max() else {
        return 0;
    };
    (will_chance(rules, mind) * rules.needs.trials.heard_factor(tellers)).min(1000)
}

/// The chance, in per mille, that `mind` sets out to try by its will
/// alone: in distress the percent, else the calm per mille, at least the
/// floor of every trial (D50, amended).
pub(super) fn will_chance(rules: &Content, mind: &Mind) -> u64 {
    let trials = &rules.needs.trials;
    if mind.distress_days >= trials.trial_after_days {
        trials.try_percent(mind.will) * 10
    } else {
        trials.calm_try_permille(mind.will)
    }
    .max(u64::from(trials.try_floor_permille))
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
                    gesture: Gesture::PickUp,
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
        Gesture::PickUp => Some(deed.on),
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
/// those it could not reach today: each its id, its tile and its
/// material.
fn things_near<'w>(
    world: &'w World,
    here: Pos,
    near: i32,
    mind: &Mind,
) -> Vec<(ItemId, Pos, &'w str)> {
    let mut out: Vec<(ItemId, Pos, &str)> = world
        .things_near(here, near, |_| true)
        .into_iter()
        .filter(|&(id, _, _)| !mind.handwork.barred(Barred::Item(id)))
        .collect();
    out.sort_by_key(|&(id, p, _)| (super::far(p, here), id));
    out
}

/// Of the things lying within `near` tiles of `here` that it could reach
/// today, the nearest of each of `materials` (the id breaking ties): the
/// thing [`things_near`] lists first of that material, looked for in one
/// pass over the things of those materials alone (D51, amended 26
/// November 2026: the hands looked at everything around once a deed, and
/// copied it all).
fn nearest_of(
    world: &World,
    rules: &Content,
    (here, near): (Pos, i32),
    mind: &Mind,
    materials: &[u16],
) -> Vec<(u16, ItemId, Pos)> {
    // A material is known by the first of its name in the content, as
    // `material_index` reads it.
    let names: Vec<(u16, &str)> = materials
        .iter()
        .filter_map(|&m| {
            let id = rules.materials.get(usize::from(m))?.id.as_str();
            (material_index(rules, id) == Some(m)).then_some((m, id))
        })
        .collect();
    let mut best: Vec<(u16, (i64, ItemId), Pos)> = Vec::new();
    for (id, p, material) in world.things_near(here, near, |m| names.iter().any(|n| n.1 == m)) {
        if mind.handwork.barred(Barred::Item(id)) {
            continue;
        }
        let Some(&(m, _)) = names.iter().find(|n| n.1 == material) else {
            continue;
        };
        let key = (super::far(p, here), id);
        match best.iter_mut().find(|b| b.0 == m) {
            Some(b) if key < b.1 => *b = (m, key, p),
            Some(_) => {}
            None => best.push((m, key, p)),
        }
    }
    best.into_iter().map(|(m, (_, id), p)| (m, id, p)).collect()
}

/// Whether the bark a cut of a tree of `species` gives would go on the
/// body (D57, amended 25 November 2026): a body feels its load, and does
/// not cut what it could not carry away (the world lets such a bark fall
/// at its feet, and it still lacked bark, and cut again). True when the
/// tree has no bark: nothing would fall.
fn bark_fits(world: &World, rules: &Content, id: BodyId, species: usize) -> bool {
    let Some(body) = world.body(id) else {
        return false;
    };
    let Some(mass_g) = rules
        .species
        .get(species)
        .and_then(|s| s.bark.as_deref())
        .and_then(|bark| rules.material(bark))
        .map(|m| m.mass_g().max(1))
    else {
        return true;
    };
    body.load_g().saturating_add(mass_g) <= body.capacity_g(&rules.movement)
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
/// knows or heard when it lacks what the deed gives (taking up first the
/// tool a cut names, when it carries none), or, in a trial of its hands
/// with no such deed to do here, a gesture drawn (amended 25 November
/// 2026: a trial begun on a tale keeps a free hand).
pub(super) fn hands_turn(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    mind: &mut Mind,
    (seed, news): (u64, &mut Vec<News>),
) -> HandsTurn {
    let hands = &rules.needs.hands;
    let Some(body) = world.body(id).filter(|_| hands.on()) else {
        return HandsTurn::Free;
    };
    if body.making().is_some() {
        return HandsTurn::Decide(None);
    }
    let now = world.tick().0;
    if let Some(tool) = resolve(world, rules, id, mind) {
        news.push(News::ToolTaken {
            body: id,
            tool,
            at: now,
        });
    }
    let day = today(world);
    if mind.handwork.barred.0 != day {
        mind.handwork.barred = (day, Vec::new());
    }
    let trying = mind.trial.is_some_and(|t| t.way == Way::Hands);
    let have = carried(world, rules, id);
    // A container serves when it holds food beyond what the arms alone
    // would: the body lives it, nothing tells it.
    if body.food_g() > body.arms_g(&rules.movement) {
        for &(_, m) in &have {
            if rules
                .materials
                .get(usize::from(m))
                .is_some_and(|m| m.holds_g > 0)
            {
                mind.handwork.serve(m);
            }
        }
    }
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
        let served = &mind.handwork;
        if let Some(&(item, _)) = have
            .iter()
            .find(|(_, m)| !food(*m) && !keep.contains(m) && !served.served(*m))
        {
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
    if let Some(((HandTarget::Item(item, _), _), true)) = found {
        mind.handwork.fetch = Some(item);
    }
    // In a trial, with no deed it knows or heard to do here, it draws a
    // gesture, whether the trial began on a tale or not (amended 25
    // November 2026: before, a trial on a tale only ever redid the
    // gestures heard).
    let found = match found {
        Some((todo, _)) => Some(todo),
        None if trying => {
            let drawn = draw(world, rules, (id, here), mind, (seed, &have, near));
            if drawn.is_some() && !mind.handwork.drew {
                mind.handwork.drew = true;
                news.push(News::Drew {
                    body: id,
                    heard: !mind.handwork.aim.is_empty(),
                    at: now,
                });
            }
            drawn
        }
        None => None,
    };
    match found {
        Some(todo) => HandsTurn::Decide(Some(go_do(world, rules, (id, here), todo, mind))),
        None => HandsTurn::Free,
    }
}

/// A gesture drawn in a trial of the hands on nothing heard (D57): one of
/// the three the things allow, on a thing drawn among those it carries or
/// that lie around it, at most the content's tries a day. A cut only with
/// a sharp thing and only on a tree with bark, a fold only of what folds
/// (amended 25 November 2026); no suggestion of which gesture.
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
    // Only what the things allow (the project lead, 25 November 2026:
    // "on ne coupe pas avec une baie"): a body sees truly that a stone is
    // sharp, that a sheet bends, that a tree has bark to cut. It cuts with
    // a sharp thing only, a tree with bark only; it folds what folds. The
    // gesture stays drawn at random among those the things allow: nothing
    // says which.
    let material = |m: u16| rules.materials.get(usize::from(m));
    let tools: Vec<(ItemId, u16)> = have
        .iter()
        .copied()
        .filter(|&(_, m)| material(m).is_some_and(|m| m.sharp))
        .collect();
    let folding: Vec<(ItemId, u16)> = have
        .iter()
        .copied()
        .filter(|&(_, m)| material(m).is_some_and(|m| m.folds_into.is_some()))
        .collect();
    let things = things_near(world, here, near, mind);
    let trees: Vec<_> = if tools.is_empty() {
        Vec::new()
    } else {
        trees_near(world, here, near, mind)
            .into_iter()
            .filter(|t| {
                rules.species.get(t.3).is_some_and(|s| s.bark.is_some())
                    && bark_fits(world, rules, id, t.3)
            })
            .collect()
    };
    let open: Vec<Gesture> = Gesture::ALL
        .into_iter()
        .filter(|g| match g {
            Gesture::PickUp => !things.is_empty(),
            Gesture::Cut => !trees.is_empty(),
            Gesture::Fold => !folding.is_empty(),
        })
        .collect();
    let n = u64::try_from(open.len()).ok().filter(|&n| n > 0)?;
    let gesture = open[usize::try_from(r % n).unwrap_or(0)];
    let deed = |on, with| Deed { gesture, on, with };
    match gesture {
        Gesture::PickUp => {
            let &(thing, pos, material) = things.get(pick(things.len(), 0))?;
            let on = material_index(rules, material)?;
            Some((HandTarget::Item(thing, pos), deed(on, NOTHING)))
        }
        Gesture::Cut => {
            let &(chunk, key, at, species) = trees.get(pick(trees.len(), 0))?;
            let &(with, with_m) = tools.get(pick(tools.len(), 1))?;
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
            let &(item, on) = folding.get(pick(folding.len(), 0))?;
            Some((HandTarget::Item(item, here), deed(on, NOTHING)))
        }
    }
}

/// Whether `id` at `here` could begin a gesture its hands do not know
/// (D50 and D57, amended 26 November 2026): among the gestures the things
/// around allow, as [`draw`] sees them, one whose deed is not among its
/// ways. Nothing is drawn, no try spent: a trial from a problem waits for
/// such a moment.
pub(super) fn something_new(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    mind: &Mind,
) -> bool {
    let near = i32::try_from(rules.needs.hands.near_tiles).unwrap_or(4);
    let have = carried(world, rules, id);
    let material = |m: u16| rules.materials.get(usize::from(m));
    let new = |deed: Deed| !mind.ways.hands.contains(deed);
    let fold = have.iter().any(|&(_, m)| {
        material(m).is_some_and(|m| m.folds_into.is_some())
            && new(Deed {
                gesture: Gesture::Fold,
                on: m,
                with: NOTHING,
            })
    });
    if fold {
        return true;
    }
    let take = things_near(world, here, near, mind)
        .iter()
        .any(|&(_, _, m)| {
            material_index(rules, m).is_some_and(|on| {
                new(Deed {
                    gesture: Gesture::PickUp,
                    on,
                    with: NOTHING,
                })
            })
        });
    if take {
        return true;
    }
    let tools: Vec<u16> = have
        .iter()
        .map(|&(_, m)| m)
        .filter(|&m| material(m).is_some_and(|m| m.sharp))
        .collect();
    if tools.is_empty() {
        return false;
    }
    trees_near(world, here, near, mind).iter().any(|t| {
        rules.species.get(t.3).is_some_and(|s| s.bark.is_some())
            && bark_fits(world, rules, id, t.3)
            && u16::try_from(t.3).is_ok_and(|on| {
                tools.iter().any(|&with| {
                    new(Deed {
                        gesture: Gesture::Cut,
                        on,
                        with,
                    })
                })
            })
    })
}

/// The first deed of `deeds` it lacks what it gives and can do here: a
/// fold of a thing it carries, a cut of a tree near with a thing it
/// carries, a thing near to pick up. For a cut with no thing of its tool
/// carried, the taking up of one lying near (D57, amended 25 November
/// 2026: "cut a birch with a flint" holds the flint), marked `true`.
fn needed(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    (deeds, have, near): (&Deeds, &[(ItemId, u16)], i32),
    mind: &Mind,
) -> Option<((HandTarget, Deed), bool)> {
    let lacks = |deed: Deed| {
        product(rules, deed).is_some_and(|p| {
            descendants(rules, deeds, p)
                .iter()
                .all(|m| !have.iter().any(|(_, h)| h == m))
        })
    };
    let held = world.body(id)?.held();
    // The things lying near are looked at once, at the first deed that
    // needs one, for the materials any deed might take up.
    let mut nearest: Option<Vec<(u16, ItemId, Pos)>> = None;
    let mut lying = |m: u16| {
        nearest
            .get_or_insert_with(|| {
                let mut materials: Vec<u16> = deeds
                    .iter()
                    .filter_map(|d| match d.gesture {
                        Gesture::Cut => Some(d.with),
                        Gesture::PickUp => Some(d.on),
                        Gesture::Fold => None,
                    })
                    .collect();
                materials.sort_unstable();
                materials.dedup();
                nearest_of(world, rules, (here, near), mind, &materials)
            })
            .iter()
            .find(|n| n.0 == m)
            .map(|&(_, item, pos)| (item, pos))
    };
    for gesture in [Gesture::Fold, Gesture::Cut, Gesture::PickUp] {
        for deed in deeds.iter().filter(|d| d.gesture == gesture) {
            if !lacks(deed) {
                continue;
            }
            match gesture {
                Gesture::Fold => {
                    if let Some(&(item, _)) = have.iter().find(|(_, m)| *m == deed.on) {
                        return Some(((HandTarget::Item(item, here), deed), false));
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
                        // The deed names its tool: one seen lying within
                        // its hands' sight is taken up for the cut, a
                        // tree near or not. A true gesture on a thing
                        // there, never a goal set.
                        if let Some((tool, pos)) = lying(deed.with) {
                            let take = Deed {
                                gesture: Gesture::PickUp,
                                on: deed.with,
                                with: NOTHING,
                            };
                            return Some(((HandTarget::Item(tool, pos), take), true));
                        }
                        continue;
                    };
                    if let Some(&(chunk, key, at, _)) = trees_near(world, here, near, mind)
                        .iter()
                        .find(|t| usize::from(deed.on) == t.3 && bark_fits(world, rules, id, t.3))
                    {
                        return Some((
                            (
                                HandTarget::Tree {
                                    chunk,
                                    key,
                                    at,
                                    with,
                                },
                                deed,
                            ),
                            false,
                        ));
                    }
                }
                Gesture::PickUp => {
                    if let Some((thing, pos)) = lying(deed.on) {
                        return Some(((HandTarget::Item(thing, pos), deed), false));
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
        begin(mind, deed, item);
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
            begin(mind, deed, item);
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
            begin(mind, deed, with);
            Intention::Cut {
                body: id,
                with,
                chunk,
                tree: key,
            }
        }
    };
    mind.handwork.last = Some(target);
    Some(Decision {
        intention,
        why: Why::Hands,
    })
}

/// `deed` begun on `item`: pending till what comes of it is seen. In a
/// trial of its hands, a deed its hands did not know at the trial's
/// start makes the trial one it could try (amended 25 November 2026).
fn begin(mind: &mut Mind, deed: Deed, item: ItemId) {
    mind.handwork.pending = Some((deed, item));
    mind.handwork.done = None;
    if mind
        .trial
        .is_some_and(|t| t.way == Way::Hands && !t.before.hands.contains(deed))
    {
        mind.handwork.gestured = true;
    }
}

/// Whether `trial`, ending, is one of the hands in which `mind` could
/// not try (D57, amended 25 November 2026, the project lead: "je n'ai
/// pas pu essayer" is not "ça n'a pas marché"): no gesture its hands did
/// not know was begun in its days, nothing new being within its reach.
/// Such a trial is not judged: no lesson, no will moved, no failure.
pub(super) fn untried(mind: &Mind, trial: &super::Trial) -> bool {
    trial.way == Way::Hands && !mind.handwork.gestured
}

/// Ends `mind`'s trial `trial` unjudged if it could not try ([`untried`]):
/// its ways as before, nothing told, its will as it was; the news.
pub(super) fn end_untried(
    mind: &mut Mind,
    trial: &super::Trial,
    (id, now): (BodyId, u64),
) -> Option<News> {
    if !untried(mind, trial) {
        return None;
    }
    trial.way.copy(&mut mind.ways, &trial.before);
    mind.handwork.drop_aim();
    mind.trial = None;
    Some(News::Untried { body: id, at: now })
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
/// case. The tool's material, when it was a tool taken up for a cut.
fn resolve(world: &World, rules: &Content, id: BodyId, mind: &mut Mind) -> Option<u16> {
    let (deed, item) = mind.handwork.pending?;
    let body = world.body(id)?;
    // Not done yet (the act waits for the next tick): it stays pending,
    // until done or another deed is begun.
    let worked = match deed.gesture {
        Gesture::PickUp if body.has(item) => true,
        Gesture::PickUp => return None,
        Gesture::Cut | Gesture::Fold => mind.handwork.done.take()?,
    };
    mind.handwork.pending = None;
    if !worked {
        return None;
    }
    let tool = (deed.gesture == Gesture::PickUp && mind.handwork.fetch == Some(item)).then(|| {
        mind.handwork.fetch = None;
        deed.on
    });
    // A new kind of thing made or taken: a novelty, told tonight (the
    // project lead, 25 November 2026), whatever the trial's judgment.
    if let Some(made) = product(rules, deed)
        && !mind.handwork.made_kinds.contains(&made)
    {
        mind.handwork.made_kinds.push(made);
        mind.handwork.novel.push((deed, made));
    }
    // A tool whose cut gave something served it (25 November 2026).
    if deed.gesture == Gesture::Cut && deed.with != NOTHING {
        mind.handwork.serve(deed.with);
    }
    match mind.handwork.practised.iter_mut().find(|(d, _)| *d == deed) {
        Some((_, n)) => *n += 1,
        None => mind.handwork.practised.push((deed, 1)),
    }
    if let Some(trial) = mind.trial.as_mut().filter(|t| t.way == Way::Hands)
        && mind.ways.hands.add(deed)
    {
        trial.started_day = super::restarted_day(world, &rules.needs.trials, trial, today(world));
    }
    tool
}

#[cfg(test)]
mod tests {
    use super::super::{Brains, HeardDays, Lately, News, Trial, Ways, start_trial};
    use super::*;
    use crate::memory::{Access, Record};
    use crate::time::Calendar;
    use std::collections::BTreeSet;

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
                gesture: Gesture::PickUp,
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
            gesture: Gesture::PickUp,
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
        let HandsTurn::Decide(Some(decision)) =
            hands_turn(&world, &rules, (id, here), mind, (7, &mut Vec::new()))
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
        let HandsTurn::Decide(Some(decision)) =
            hands_turn(&world, &rules, (id, here), mind, (7, &mut Vec::new()))
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
            hands_turn(&world, &rules, (id, here), mind, (7, &mut Vec::new())),
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
    fn a_body_whose_load_cannot_take_the_bark_does_not_cut() {
        let (rules, mut world, mut brains, id, birch) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let flint = picked(&mut world, &rules, id, "flint");
        let sharp = [(flint, material(&rules, "flint"))];
        let bark_g = rules.material("birch_bark").unwrap().mass_g();
        let set_load = |world: &mut World, room_g: u32| {
            let body = world.bodies.iter_mut().find(|b| b.id() == id).unwrap();
            body.load_g = body.capacity_g(&rules.movement) - room_g;
        };
        let cuts = |world: &World, mind: &mut Mind| {
            matches!(
                hands_turn(world, &rules, (id, here), mind, (7, &mut Vec::new())),
                HandsTurn::Decide(Some(Decision {
                    intention: Intention::Cut { .. },
                    ..
                }))
            )
        };
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.ways.hands = bark_deeds(&rules, birch);
        // Room for less than a bark: the deed it knows is not done (the
        // bark would fall at its feet, and it would lack bark still).
        set_load(&mut world, bark_g - 1);
        assert!(!cuts(&world, mind), "no cut it cannot carry");
        // Nor drawn in a trial.
        mind.ways.hands = Deeds::default();
        for day in 0..200 {
            mind.handwork.tries = (day, 0);
            if let Some((_, deed)) = draw(&world, &rules, (id, here), mind, (day, &sharp, 4)) {
                assert_ne!(deed.gesture, Gesture::Cut, "day {day}");
            }
        }
        // Room for the bark: it cuts.
        mind.ways.hands = bark_deeds(&rules, birch);
        set_load(&mut world, bark_g);
        assert!(cuts(&world, mind), "a bark it can carry");
    }

    #[test]
    fn what_its_hands_no_longer_know_is_let_go() {
        let (rules, mut world, mut brains, id, _) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let pebble = picked(&mut world, &rules, id, "pebble");
        picked(&mut world, &rules, id, "berries");
        let mind = brains.minds.get_mut(&id).unwrap();
        let HandsTurn::Decide(Some(decision)) =
            hands_turn(&world, &rules, (id, here), mind, (7, &mut Vec::new()))
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
    fn a_flint_that_cut_is_kept_though_its_hands_know_no_deed() {
        let (rules, mut world, mut brains, id, birch) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let flint = picked(&mut world, &rules, id, "flint");
        let mut idle = brains.minds.get(&id).unwrap().clone();
        let HandsTurn::Decide(Some(decision)) =
            hands_turn(&world, &rules, (id, here), &mut idle, (7, &mut Vec::new()))
        else {
            panic!("a deed");
        };
        assert_eq!(
            decision.intention,
            Intention::Drop {
                body: id,
                item: flint
            },
            "a flint that never served is let go"
        );
        // The same body, its last cut with the flint having given bark.
        let mind = brains.minds.get_mut(&id).unwrap();
        let cut = Deed {
            gesture: Gesture::Cut,
            on: birch,
            with: material(&rules, "flint"),
        };
        mind.handwork.pending = Some((cut, flint));
        mind.handwork.done = Some(true);
        let turn = hands_turn(&world, &rules, (id, here), mind, (7, &mut Vec::new()));
        assert!(mind.handwork.served(material(&rules, "flint")));
        assert!(mind.ways.hands.is_empty(), "served, not learnt: no trial");
        assert_eq!(turn, HandsTurn::Free, "the flint stays in its hands");
    }

    #[test]
    fn a_container_that_held_food_beyond_the_arms_served() {
        let (rules, mut world, mut brains, id, _) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        picked(&mut world, &rules, id, "folded_birch_bark");
        let container = material(&rules, "folded_birch_bark");
        let mind = brains.minds.get_mut(&id).unwrap();
        let _ = hands_turn(&world, &rules, (id, here), mind, (7, &mut Vec::new()));
        assert!(!mind.handwork.served(container), "empty, it served nothing");
        let berries = world.spawn_item("berries", 3000, here);
        let mut events = Vec::new();
        world.queue(Intention::PickUp {
            body: id,
            item: berries,
        });
        world.advance(1, &rules, &mut events);
        assert!(world.body(id).unwrap().food_g() > 1000, "{events:?}");
        let mind = brains.minds.get_mut(&id).unwrap();
        let _ = hands_turn(&world, &rules, (id, here), mind, (7, &mut Vec::new()));
        assert!(mind.handwork.served(container));
    }

    #[test]
    fn a_gesture_is_drawn_only_with_what_allows_it() {
        let (rules, mut world, mut brains, id, birch) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let berries = picked(&mut world, &rules, id, "berries");
        let pebble = picked(&mut world, &rules, id, "pebble");
        let mind = brains.minds.get_mut(&id).unwrap();
        let (berries_m, pebble_m) = (material(&rules, "berries"), material(&rules, "pebble"));
        let blunt = [(berries, berries_m), (pebble, pebble_m)];
        let mut gestures = BTreeSet::new();
        for day in 0..200 {
            mind.handwork.tries = (day, 0);
            if let Some((_, deed)) = draw(&world, &rules, (id, here), mind, (day, &blunt, 4)) {
                gestures.insert(deed.gesture);
            }
        }
        assert!(
            !gestures.contains(&Gesture::Cut) && !gestures.contains(&Gesture::Fold),
            "no cut with a berry or a pebble, no fold of them: {gestures:?}"
        );
        let flint = picked(&mut world, &rules, id, "flint");
        let flint_m = material(&rules, "flint");
        let sharp = [(berries, berries_m), (flint, flint_m)];
        let mut cuts = 0;
        for day in 0..200 {
            mind.handwork.tries = (day, 0);
            if let Some((_, deed)) = draw(&world, &rules, (id, here), mind, (day, &sharp, 4))
                && deed.gesture == Gesture::Cut
            {
                cuts += 1;
                assert_eq!(deed.with, flint_m, "only the sharp thing cuts");
                assert!(
                    rules.species[usize::from(deed.on)].bark.is_some(),
                    "a tree with bark"
                );
            }
        }
        assert!(
            cuts > 0,
            "the flint beside the birch {birch} is drawn to cut"
        );
    }

    #[test]
    fn a_body_that_knows_a_deed_tinkers_on_from_it_in_a_trial() {
        // It knows how to take a flint and holds one, beside a birch: out
        // of a trial its hands rest; in a trial of its hands it draws new
        // gestures with what it holds, the flint among them, and comes to
        // cut the birch with it. Nothing prefers the cut: it is drawn
        // among the gestures the things allow.
        let (rules, mut world, mut brains, id, birch) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let flint = picked(&mut world, &rules, id, "flint");
        let flint_m = material(&rules, "flint");
        let day = today(&world);
        let mind = brains.minds.get_mut(&id).unwrap();
        assert!(mind.ways.hands.add(Deed {
            gesture: Gesture::PickUp,
            on: flint_m,
            with: NOTHING,
        }));
        assert_eq!(
            hands_turn(&world, &rules, (id, here), mind, (7, &mut Vec::new())),
            HandsTurn::Free,
            "out of a trial it only does what it knows, and holds the flint"
        );
        mind.trial = Some(Trial {
            way: Way::Hands,
            before: mind.ways,
            started_day: day,
            before_days: Lately::default(),
            heard: HeardDays::default(),
            caller: None,
            problem: None,
        });
        let mut cut = false;
        for seed in 0..200 {
            mind.handwork.tries = (day, 0);
            mind.handwork.barred = (day, Vec::new());
            mind.handwork.to = None;
            mind.handwork.pending = None;
            mind.doing = Doing::Nothing;
            if let HandsTurn::Decide(Some(d)) =
                hands_turn(&world, &rules, (id, here), mind, (seed, &mut Vec::new()))
                && let Intention::Cut { with, .. } = d.intention
            {
                assert_eq!(with, flint, "it cuts with the flint it knew to take");
                cut = true;
            }
        }
        assert!(cut, "the birch {birch} is cut with the flint at some draw");
    }

    /// Lays `material` at the body's feet, not picked up.
    fn laid(world: &mut World, rules: &Content, id: BodyId, material: &str) -> ItemId {
        let pos = world.body(id).unwrap().pos();
        let grams = rules.material(material).unwrap().mass_g();
        world.spawn_item(material, grams, pos)
    }

    /// `mind` in a trial of its hands begun on `day` on the deeds
    /// `heard` (none: a trial on no tale).
    fn in_a_trial(mind: &mut Mind, heard: &[Deed], day: u64) {
        let heard: Vec<(Deed, u32)> = heard.iter().map(|&d| (d, 1)).collect();
        aim_at(mind, &heard, day);
        mind.trial = Some(Trial {
            way: Way::Hands,
            before: mind.ways,
            started_day: day,
            before_days: Lately::default(),
            heard: HeardDays::default(),
            caller: None,
            problem: None,
        });
    }

    #[test]
    fn a_trial_begun_on_a_tale_draws_a_gesture_when_no_gesture_heard_can_be_made() {
        // D57, amended 25 November 2026: it heard of taking a folded
        // bark, none lies here; a pebble lies at its feet. Before, its
        // hands rested; now it draws as any trial does.
        let (rules, mut world, mut brains, id, _) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let pebble = laid(&mut world, &rules, id, "pebble");
        let day = today(&world);
        let mind = brains.minds.get_mut(&id).unwrap();
        let heard = Deed {
            gesture: Gesture::PickUp,
            on: material(&rules, "folded_birch_bark"),
            with: NOTHING,
        };
        in_a_trial(mind, &[heard], day);
        let mut news = Vec::new();
        let HandsTurn::Decide(Some(decision)) =
            hands_turn(&world, &rules, (id, here), mind, (7, &mut news))
        else {
            panic!("a gesture drawn");
        };
        assert_eq!(decision.why, Why::Hands);
        assert!(
            matches!(news[..], [News::Drew { heard: true, body, .. }] if body == id),
            "{news:?}"
        );
        // The pebble is the one thing it may take here (a cut needs an
        // edge carried, a fold a thing that folds): it goes for it.
        assert!(
            matches!(
                decision.intention,
                Intention::PickUp { item, .. } if item == pebble
            ) || matches!(decision.intention, Intention::MoveTo { .. }),
            "{decision:?}"
        );
        assert!(mind.handwork.drew);
        // Told once a trial.
        mind.handwork.pending = None;
        mind.handwork.to = None;
        mind.doing = Doing::Nothing;
        let mut again = Vec::new();
        let _ = hands_turn(&world, &rules, (id, here), mind, (8, &mut again));
        assert!(
            !again.iter().any(|n| matches!(n, News::Drew { .. })),
            "{again:?}"
        );
    }

    #[test]
    fn a_body_takes_up_the_tool_a_cut_it_heard_names_then_cuts() {
        // D57, amended 25 November 2026: "cut a birch with a flint"
        // holds the flint; a flint lies at its feet, by the birch.
        let (rules, mut world, mut brains, id, birch) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let flint = laid(&mut world, &rules, id, "flint");
        let flint_m = material(&rules, "flint");
        let day = today(&world);
        let cut = Deed {
            gesture: Gesture::Cut,
            on: birch,
            with: flint_m,
        };
        let mind = brains.minds.get_mut(&id).unwrap();
        in_a_trial(mind, &[cut], day);
        let mut news = Vec::new();
        let HandsTurn::Decide(Some(decision)) =
            hands_turn(&world, &rules, (id, here), mind, (7, &mut news))
        else {
            panic!("the flint taken up");
        };
        assert!(
            matches!(decision.intention, Intention::PickUp { item, .. } if item == flint),
            "{decision:?}"
        );
        assert!(news.is_empty(), "no gesture drawn: {news:?}");
        assert!(mind.handwork.gestured, "a gesture new to its hands");
        let mut events = Vec::new();
        world.queue(decision.intention);
        world.advance(1, &rules, &mut events);
        assert!(world.body(id).unwrap().has(flint));
        let mind = brains.minds.get_mut(&id).unwrap();
        let mut news = Vec::new();
        let HandsTurn::Decide(Some(decision)) =
            hands_turn(&world, &rules, (id, here), mind, (7, &mut news))
        else {
            panic!("the cut");
        };
        assert!(
            matches!(news[..], [News::ToolTaken { tool, .. }] if tool == flint_m),
            "{news:?}"
        );
        assert!(
            matches!(decision.intention, Intention::Cut { with, .. } if with == flint),
            "{decision:?}"
        );
    }

    #[test]
    fn a_deed_known_before_the_trial_does_not_make_it_one_it_could_try() {
        // A trial whose days only redo what the hands knew at its start
        // is one in which nothing new could be tried (amended 25
        // November 2026); a deed new to them is.
        let (rules, mut world, mut brains, id, _) = by_a_birch();
        let flint = laid(&mut world, &rules, id, "flint");
        let take = Deed {
            gesture: Gesture::PickUp,
            on: material(&rules, "flint"),
            with: NOTHING,
        };
        let day = today(&world);
        let mind = brains.minds.get_mut(&id).unwrap();
        assert!(mind.ways.hands.add(take));
        in_a_trial(mind, &[], day);
        begin(mind, take, flint);
        let trial = mind.trial.unwrap();
        assert!(untried(mind, &trial), "known before the trial");
        mind.ways.hands = Deeds::default();
        in_a_trial(mind, &[], day);
        assert!(!mind.handwork.gestured, "a new trial starts afresh");
        begin(mind, take, flint);
        let trial = mind.trial.unwrap();
        assert!(!untried(mind, &trial), "new to its hands");
    }

    #[test]
    fn a_novelty_of_the_hands_is_told_that_evening_once() {
        let (rules, mut world, mut brains, id, _) = by_a_birch();
        let flint = picked(&mut world, &rules, id, "flint");
        let take = Deed {
            gesture: Gesture::PickUp,
            on: material(&rules, "flint"),
            with: NOTHING,
        };
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.pending = Some((take, flint));
        resolve(&world, &rules, id, mind);
        assert_eq!(mind.handwork.novel, vec![(take, material(&rules, "flint"))]);
        let told = mind.tell_own(id, 3, &mut brains.memory);
        assert_eq!(told.len(), 1, "told without a judgment");
        assert!(matches!(
            brains.memory.get(told[0]),
            Some(Record::Deed { deed, body, .. }) if *deed == take && *body == id
        ));
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.pending = Some((take, flint));
        resolve(&world, &rules, id, mind);
        assert!(
            mind.handwork.novel.is_empty(),
            "a second flint is nothing new"
        );
    }

    #[test]
    fn a_deed_heard_is_tried_more_readily_the_more_tellers_and_once_till_heard_again() {
        let (rules, world, mut brains, id, birch) = by_a_birch();
        let flint = material(&rules, "flint");
        let cut = Deed {
            gesture: Gesture::Cut,
            on: birch,
            with: flint,
        };
        let mut records = Vec::new();
        for teller in 1..=4 {
            let (record, _) = brains.memory.intern(Record::Deed {
                body: BodyId(100 + teller),
                day: 1,
                deed: cut,
                made: material(&rules, "birch_bark"),
                will: 50,
            });
            records.push((record, BodyId(100 + teller)));
        }
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.will = 10;
        mind.distress_days = 0;
        let trials = &rules.needs.trials;
        assert_eq!(
            trials.calm_try_permille(10),
            0,
            "no blind invention at will 10"
        );
        let mut chances = Vec::new();
        for &(record, by) in &records {
            mind.heard_ways.push(Access { record, by, day: 2 });
            let heard = heard_deeds(&brains.memory, mind);
            assert_eq!(heard.len(), 1);
            chances.push(heard_chance(&rules, mind, &heard));
        }
        // The floor (1 per mille) times 5, 10, 10, 20.
        assert_eq!(chances, vec![5, 10, 10, 20]);
        mind.distress_days = trials.trial_after_days;
        let heard = heard_deeds(&brains.memory, mind);
        assert_eq!(
            heard_chance(&rules, mind, &heard),
            (trials.try_percent(10) * 10 * 20).min(1000),
            "in distress: its own percent times the factor"
        );
        // Set out to try on day 5: not again on the extra chance until
        // heard again after that.
        aim_at(mind, &heard, 5);
        assert!(mind.handwork.aim.contains(cut));
        assert_eq!(heard_deeds(&brains.memory, mind), Vec::new());
        mind.heard_ways.push(Access {
            record: records[0].0,
            by: records[0].1,
            day: 6,
        });
        assert_eq!(heard_deeds(&brains.memory, mind).len(), 1, "heard again");
        // Known, it is no longer a heard deed to try.
        mind.ways.hands.add(cut);
        assert_eq!(heard_deeds(&brains.memory, mind), Vec::new());
        let _ = world;
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
            problem: None,
        });
        let cut = Deed {
            gesture: Gesture::Cut,
            on: birch,
            with: material(&rules, "pebble"),
        };
        mind.handwork.pending = Some((cut, ItemId(1)));
        mind.handwork.done = Some(false);
        resolve(&world, &rules, id, mind);
        assert!(mind.ways.hands.is_empty(), "a cut with a pebble: nothing");
        let fold = Deed {
            gesture: Gesture::Fold,
            on: material(&rules, "birch_bark"),
            with: NOTHING,
        };
        mind.handwork.pending = Some((fold, ItemId(1)));
        // Not done yet: it waits.
        resolve(&world, &rules, id, mind);
        assert_eq!(mind.handwork.pending, Some((fold, ItemId(1))));
        mind.handwork.done = Some(true);
        resolve(&world, &rules, id, mind);
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
            problem: None,
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
