//! The hands in the mind (D57): the deeds a body knows, tries and
//! practises. A deed is a gesture on a kind of thing (a material, or a
//! tree's species for a cut), with a kind of thing in hand for a cut. No
//! recipe: a body in a trial of its hands draws a gesture and a thing from
//! the run's seed among what it carries and what lies around it, as far
//! as what it sees of them makes the gesture plausible (an edge, a sheet,
//! a thing that gives in hand), and the world says what comes of it,
//! which may be nothing, or the thing broken (amended 27 November 2026);
//! what came of something is learnt, judged
//! with the trial (D50), and told at the fire (D55). A body redoes the
//! deeds it knows when it lacks what they gave: the plain minimum, never
//! a goal.
//!
//! Amended 27 November 2026 (step 2 of "acts and discovery"): putting a
//! thing in another is a gesture drawn as the others; a body that knows
//! one puts the loose food of its full arms in the hollow it names
//! without a thought. What gave nothing or broke is remembered, a
//! bounded list, and not drawn again while remembered. A trial kept
//! leaves known only the deeds whose product served.

use crate::content::{Content, Folded, Form};
use crate::r#gen::scatter::{self, purpose};
use crate::hands::{Gesture, Part};
use crate::map::{ChunkPos, FinePos, Pos};
use crate::memory::{Record, Store};
use crate::trees::TreeKey;
use crate::{BodyId, GATHER_REACH_MM, Intention, ItemId, World};

use super::{AROUND, Decision, Doing, Mind, News, Way, Why, today, walkable};

/// The most deeds a body holds in its ways. Sixteen (27 November 2026;
/// six before): a trial holds every deed it tried until its judge
/// forgets those that served nothing, and with six, a trial that had put
/// stones and branches in its hollow had no room left to learn the
/// put-in of food, and was given up while it carried more than its arms
/// (five of thirteen in the run of seed 37).
pub const DEEDS: usize = 16;

/// No thing: a deed done with nothing in hand.
pub const NOTHING: u16 = u16::MAX;

/// A kind of thing, as the hands name it: a material's index in the
/// content, with this bit when the thing is folded into a hollow (D57,
/// amended 27 November 2026: the hollow is a form of the thing, no
/// material of its own).
pub const HOLLOW: u16 = 0x8000;

/// A cut's tree, as the hands name it: a species' index in the content,
/// with this bit when the cut is made on a branch and not on the trunk
/// (D57, amended 27 November 2026: a tree has parts; the part is named
/// the way a hollow is, a flag on the kind).
pub const BRANCH: u16 = 0x4000;

/// The species' index of the tree a cut names, its part left out.
#[must_use]
pub fn cut_species(on: u16) -> usize {
    usize::from(on & !BRANCH)
}

/// The part of the tree a cut names.
#[must_use]
pub fn cut_part(on: u16) -> Part {
    if on != NOTHING && on & BRANCH != 0 {
        Part::Branch
    } else {
        Part::Trunk
    }
}

/// The material's index of a kind of thing, its form left out.
#[must_use]
pub fn kind_material(kind: u16) -> usize {
    usize::from(kind & !HOLLOW)
}

/// Whether a kind of thing is one folded into a hollow.
#[must_use]
pub fn kind_hollow(kind: u16) -> bool {
    kind != NOTHING && kind & HOLLOW != 0
}

/// The material of a kind of thing.
fn stuff(rules: &Content, kind: u16) -> Option<&crate::content::Material> {
    (kind != NOTHING)
        .then(|| rules.materials.get(kind_material(kind)))
        .flatten()
}

/// A deed of the hands (D57): a gesture on a kind of thing, with a
/// second kind of thing when it cuts (the tool in hand) or puts in (the
/// thing put in).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Deed {
    /// The gesture.
    pub gesture: Gesture,
    /// What it is done on: a material's index in the content (take,
    /// fold), or a tree species' index (cut), with [`BRANCH`] when the
    /// cut is on a branch.
    pub on: u16,
    /// The kind in hand it cuts with, or the kind of thing it puts `on`
    /// in; [`NOTHING`] otherwise.
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
        /// The part of the tree it cuts.
        part: Part,
    },
    /// A thing it carries, to put in another it carries.
    Put {
        /// The thing put.
        item: ItemId,
        /// The thing it is put in.
        into: ItemId,
    },
    /// A thing it carries, to strike with another it carries.
    Strike {
        /// The thing struck.
        item: ItemId,
        /// The thing it is struck with.
        with: ItemId,
    },
    /// A thing it carries in a hollow, to take out of it.
    Take {
        /// The thing taken out.
        item: ItemId,
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
            HandTarget::Item(item, _)
            | HandTarget::Put { item, .. }
            | HandTarget::Strike { item, .. }
            | HandTarget::Take { item } => Barred::Item(item),
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
    /// The thing that came of it, when one did: what it sees in its
    /// hands or at its feet (amended 28 November 2026).
    pub(super) gave: Option<ItemId>,
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
    /// The gestures that gave nothing or broke the thing in its hands
    /// (D57, amended 27 November 2026), the oldest first, at most the
    /// content's `failures_kept`: not drawn again while remembered.
    pub failed: Vec<Deed>,
    /// The deeds a kept trial of its own left it: something came of them
    /// that served.
    pub kept: Vec<Deed>,
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

    /// Whether it remembers that `deed` gave nothing in its hands.
    #[must_use]
    pub fn failed(&self, deed: Deed) -> bool {
        self.failed.contains(&deed)
    }

    /// `deed` gave nothing or broke the thing: remembered, the oldest
    /// forgotten beyond `most`.
    fn fail(&mut self, deed: Deed, most: u32) {
        if self.failed.contains(&deed) {
            return;
        }
        self.failed.push(deed);
        let most = usize::try_from(most).unwrap_or(usize::MAX);
        if self.failed.len() > most {
            let over = self.failed.len() - most;
            self.failed.drain(..over);
        }
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

    /// What came of its gesture, seen: something, the thing, or nothing.
    pub(super) fn came(&mut self, gave: Option<ItemId>) {
        self.done = Some(gave.is_some());
        self.gave = gave;
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
        self.gave = None;
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

/// The deeds that make a hollow, read from the content alone: take a
/// material, cut with it a tree whose bark it works loose and which takes
/// a hollow when folded, fold that bark, put the content's first food
/// in that hollow (amended 27 November 2026: a hollow nothing is put in
/// holds nothing); the first such chain in the content's order. Not the experiment's: its diagnostic (the vivarium's
/// `--hands-known`), founders who know them from birth. Empty when the
/// content holds no such chain.
#[must_use]
pub fn container_deeds(rules: &Content) -> Deeds {
    let mut deeds = Deeds::default();
    for (s, species) in rules.species.iter().enumerate() {
        let Ok(s) = u16::try_from(s) else {
            continue;
        };
        for (t, tool) in rules.materials.iter().enumerate() {
            let Ok(t) = u16::try_from(t) else {
                continue;
            };
            let Some(bark) = rules
                .cut_gives(tool, species, Part::Trunk)
                .and_then(|bark| material_index(rules, &bark.id))
            else {
                continue;
            };
            if folds_into(rules, bark).is_none() {
                continue;
            }
            for deed in [
                Deed {
                    gesture: Gesture::PickUp,
                    on: t,
                    with: NOTHING,
                },
                Deed {
                    gesture: Gesture::Cut,
                    on: s,
                    with: t,
                },
                Deed {
                    gesture: Gesture::Fold,
                    on: bark,
                    with: NOTHING,
                },
            ] {
                deeds.add(deed);
            }
            let food = rules
                .materials
                .iter()
                .position(|m| m.food.is_some())
                .and_then(|f| u16::try_from(f).ok());
            if let Some(on) = food {
                deeds.add(Deed {
                    gesture: Gesture::PutIn,
                    on,
                    with: bark | HOLLOW,
                });
            }
            return deeds;
        }
    }
    deeds
}

/// A material's index in the content.
fn material_index(rules: &Content, id: &str) -> Option<u16> {
    rules
        .materials
        .iter()
        .position(|m| m.id == id)
        .and_then(|i| u16::try_from(i).ok())
}

/// What a thing of the kind `kind` becomes when folded: itself with a
/// hollow, when it has none and is a sheet supple enough.
fn folds_into(rules: &Content, kind: u16) -> Option<u16> {
    if kind_hollow(kind) {
        return None;
    }
    let folded = stuff(rules, kind)?.folded(rules.needs.hands.fold_suppleness);
    matches!(folded, Folded::Hollow(_)).then_some(kind | HOLLOW)
}

/// The kind of thing a deed gives, as the world computes it: the thing
/// picked, the bark cut, the thing folded into a hollow. What a body that
/// did the deed, or heard it told, knows of it.
fn product(rules: &Content, deed: Deed) -> Option<u16> {
    match deed.gesture {
        Gesture::PickUp => Some(deed.on),
        Gesture::Cut => {
            let species = rules.species.get(cut_species(deed.on))?;
            let given = rules.cut_gives(stuff(rules, deed.with)?, species, cut_part(deed.on))?;
            material_index(rules, &given.id)
        }
        Gesture::Fold => folds_into(rules, deed.on),
        // The piece a blow breaks off, of the struck or of the striker.
        Gesture::Strike => {
            let piece = rules.strike_gives(stuff(rules, deed.on)?, stuff(rules, deed.with)?)?;
            material_index(rules, &piece.id)
        }
        // A put-in makes no kind of thing: the same things, one in the
        // other. Nor does a take-out.
        Gesture::PutIn | Gesture::TakeOut => None,
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
        // A put-in holds the thing it puts in, not what it puts there.
        if !matches!(
            deed.gesture,
            Gesture::Cut | Gesture::PutIn | Gesture::TakeOut
        ) {
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

/// What a body carries: each item with its kind, its material's index
/// and its form ([`HOLLOW`]).
fn carried(world: &World, rules: &Content, id: BodyId) -> Vec<(ItemId, u16)> {
    let Some(body) = world.body(id) else {
        return Vec::new();
    };
    body.inventory()
        .filter_map(|item| {
            let thing = world.item(item)?;
            let m = material_index(rules, thing.material())?;
            Some((item, if thing.hollow_ml() > 0 { m | HOLLOW } else { m }))
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
/// at its feet, and it still lacked bark, and cut again). Only for a cut
/// it knows or heard, whose bark it knows the weight of: a cut drawn is
/// of a tree it knows nothing of (amended 27 November 2026). True when
/// the cut gives nothing: nothing would fall.
fn bark_fits(world: &World, rules: &Content, id: BodyId, deed: Deed) -> bool {
    let Some(body) = world.body(id) else {
        return false;
    };
    let Some(mass_g) = product(rules, deed)
        .and_then(|bark| stuff(rules, bark))
        .map(|m| m.mass_g().max(1))
    else {
        return true;
    };
    body.load_g().saturating_add(mass_g) <= body.capacity_g(&rules.movement)
}

/// Whether the trunk of `tree` shows its wood bare to a body that would
/// cut it for its bark by `deed` (amended 28 November 2026): the eye
/// sees a stripped trunk, and such a tree is not one to cut bark from.
/// Never so for a cut on a branch, or of a kind whose bark does not come
/// away.
fn shows_bare(
    world: &World,
    rules: &Content,
    deed: Deed,
    (chunk, key): (ChunkPos, TreeKey),
) -> bool {
    cut_part(deed.on) == Part::Trunk
        && rules
            .species
            .get(cut_species(deed.on))
            .is_some_and(|s| s.bark.is_some())
        && world.bark_left(chunk, key, rules) == Some(0)
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

/// The kinds of the things `body` carries in which food lies.
fn holding(world: &World, rules: &Content, body: &crate::Body, have: &[(ItemId, u16)]) -> Vec<u16> {
    body.inventory()
        .filter_map(|i| world.item(i))
        .filter(|i| {
            rules
                .material(i.material())
                .is_some_and(|m| m.food.is_some())
        })
        .filter_map(crate::Item::within)
        .filter_map(|hollow| have.iter().find(|(i, _)| *i == hollow).map(|(_, k)| *k))
        .collect()
}

/// A thing serves when food lies in it and the body carries more than
/// its arms alone would (the project lead, 25 November 2026; amended 27
/// November 2026: food put in it, not the thing merely carried): the
/// body lives it, nothing tells it. Noted at each thought of its hands
/// and when a gathering stops full.
pub(super) fn note_served(world: &World, rules: &Content, id: BodyId, mind: &mut Mind) {
    let Some(body) = world.body(id) else {
        return;
    };
    if body.food_g() > body.arms_g(&rules.movement) {
        let have = carried(world, rules, id);
        for kind in holding(world, rules, body, &have) {
            mind.handwork.serve(kind);
        }
    }
}

/// Whether a thing of the kind `kind` is loose food, as the arms hold
/// it.
fn loose_food(rules: &Content, kind: u16) -> bool {
    !kind_hollow(kind) && stuff(rules, kind).is_some_and(|m| m.food.is_some())
}

/// The put-in of `deeds` to do here (D57, amended 27 November 2026): a
/// body that knows one, its arms full, puts their loose food in a thing
/// it carries of the kind the deed names that has room left, without a
/// thought: it lacks what the put-in gave, room in its arms. The deed
/// names the thing put in; what goes there is whatever loose food its
/// arms hold, which they do not tell apart either. A put-in it only
/// heard is tried so as soon as its arms hold any.
fn put_again(
    world: &World,
    rules: &Content,
    id: BodyId,
    (deeds, have): (&Deeds, &[(ItemId, u16)]),
    mind: &Mind,
) -> Option<(HandTarget, Deed)> {
    let body = world.body(id)?;
    let full = body.arms_food_g() >= body.arms_g(&rules.movement);
    for deed in deeds.iter().filter(|d| d.gesture == Gesture::PutIn) {
        if !loose_food(rules, deed.on) || (!full && mind.ways.hands.contains(deed)) {
            continue;
        }
        let Some(&(into, _)) = have
            .iter()
            .find(|&&(i, k)| k == deed.with && world.hollow_room_g(i, rules) > 0)
        else {
            continue;
        };
        let item = have.iter().find(|&&(i, k)| {
            i != into
                && loose_food(rules, k)
                && !mind.handwork.barred(Barred::Item(i))
                && world.item(i).is_some_and(|t| t.within().is_none())
        });
        if let Some(&(item, _)) = item {
            return Some((HandTarget::Put { item, into }, deed));
        }
    }
    None
}

/// The take-out of `deeds` to do here (D57, amended 27 November 2026):
/// a body that knows one and a put-in of food in the same kind of
/// hollow, its arms full and no hollow of that kind it carries having
/// room left, takes out of one the kind of thing the deed names, without
/// a thought: it lacks what the take-out gave, room in the hollow.
fn take_again(
    world: &World,
    rules: &Content,
    id: BodyId,
    (deeds, have): (&Deeds, &[(ItemId, u16)]),
    mind: &Mind,
) -> Option<(HandTarget, Deed)> {
    let body = world.body(id)?;
    if body.arms_food_g() < body.arms_g(&rules.movement) {
        return None;
    }
    for deed in deeds.iter().filter(|d| d.gesture == Gesture::TakeOut) {
        let fills = deeds
            .iter()
            .any(|d| d.gesture == Gesture::PutIn && d.with == deed.with && loose_food(rules, d.on));
        if !fills || !mind.ways.hands.contains(deed) {
            continue;
        }
        let mut hollows = have.iter().filter(|(_, k)| *k == deed.with);
        if hollows.clone().count() == 0 || hollows.any(|&(i, _)| world.hollow_room_g(i, rules) > 0)
        {
            continue;
        }
        let item = have.iter().find(|&&(i, k)| {
            k == deed.on
                && !mind.handwork.barred(Barred::Item(i))
                && world
                    .item(i)
                    .and_then(crate::Item::within)
                    .is_some_and(|h| have.iter().any(|&(o, kind)| o == h && kind == deed.with))
        });
        if let Some(&(item, _)) = item {
            return Some((HandTarget::Take { item }, deed));
        }
    }
    None
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
    note_served(world, rules, id, mind);
    // What its hands no longer know is let go, one thing a thought: a
    // trial given up leaves what it brought.
    if !trying {
        let keep = used(rules, &mind.ways.hands);
        let food = |m: u16| stuff(rules, m).is_some_and(|m| m.food.is_some());
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
            let drawn = draw(world, rules, (id, here), mind, (seed, &have, near), news);
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

/// A gesture drawn in a trial of the hands (D57): one of the four, on a
/// thing drawn among those it carries or that lie around it, at most the
/// content's tries a day. Only what a body sees makes a gesture plausible
/// ([`cuts_to_the_eye`], [`bends_to_the_eye`], [`holds_to_the_eye`];
/// amended 27 November 2026): the world may still answer nothing, or
/// break the thing. A gesture it remembers gave nothing in its hands is
/// set aside (the news counts them). No suggestion of which gesture.
fn draw(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    mind: &mut Mind,
    (seed, have, near): (u64, &[(ItemId, u16)], i32),
    news: &mut Vec<News>,
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
    let pick = |n: usize| {
        let r = scatter::roll(seed, purpose::HANDS_THING, &[keys[0], keys[1], keys[2], 0]);
        usize::try_from(r % u64::try_from(n.max(1)).unwrap_or(1)).unwrap_or(0)
    };
    // Only what a body sees (the project lead, 25 November 2026: "on ne
    // coupe pas avec une baie"): that a thing has an edge, that a thing
    // is a sheet or gives in hand, that a thing is a hollow or a sheet
    // one may lay things on, on the scales every thing has. Whether the
    // edge is keen enough for this tree, whether this tree's bark comes
    // away, whether the sheet bends or snaps, whether the thing holds
    // what is put on it, it does not see: it tries, and the world answers
    // (amended 27 November 2026). The gesture stays drawn at random among
    // the plausible: nothing says which.
    let plausible = Plausible::seen(world, rules, here, mind, (have, near));
    let things = things_near(world, here, near, mind);
    let open: Vec<Gesture> = Gesture::ALL
        .into_iter()
        .filter(|g| match g {
            Gesture::PickUp => !things.is_empty(),
            Gesture::Cut => !plausible.cuts.is_empty(),
            Gesture::Fold => !plausible.folds.is_empty(),
            Gesture::PutIn => !plausible.puts.is_empty(),
            Gesture::TakeOut => !plausible.takes.is_empty(),
            Gesture::Strike => !plausible.strikes.is_empty(),
        })
        .collect();
    if plausible.skipped > 0 {
        news.push(News::Skipped {
            body: id,
            deeds: plausible.skipped,
            at: world.tick().0,
        });
    }
    let n = u64::try_from(open.len()).ok().filter(|&n| n > 0)?;
    let gesture = open[usize::try_from(r % n).unwrap_or(0)];
    match gesture {
        Gesture::PickUp => {
            let &(thing, pos, material) = things.get(pick(things.len()))?;
            let on = material_index(rules, material)?;
            Some((
                HandTarget::Item(thing, pos),
                Deed {
                    gesture,
                    on,
                    with: NOTHING,
                },
            ))
        }
        Gesture::Cut => plausible.cuts.get(pick(plausible.cuts.len())).copied(),
        Gesture::Fold => plausible.folds.get(pick(plausible.folds.len())).copied(),
        Gesture::PutIn => plausible.puts.get(pick(plausible.puts.len())).copied(),
        Gesture::TakeOut => plausible.takes.get(pick(plausible.takes.len())).copied(),
        Gesture::Strike => plausible
            .strikes
            .get(pick(plausible.strikes.len()))
            .copied(),
    }
}

/// The cuts, folds and put-ins a body sees as plausible where it stands,
/// those it remembers gave nothing in its hands set aside (D57, amended
/// 27 November 2026).
#[derive(Default)]
struct Plausible {
    /// A cut of each tree near with each thing carried that shows an
    /// edge.
    cuts: Vec<(HandTarget, Deed)>,
    /// A fold of each thing carried that looks like it bends.
    folds: Vec<(HandTarget, Deed)>,
    /// A put-in of each thing carried in each other thing carried that
    /// looks like it holds.
    puts: Vec<(HandTarget, Deed)>,
    /// A take-out of each thing it carries in a hollow.
    takes: Vec<(HandTarget, Deed)>,
    /// A blow on each thing carried with each other thing carried one
    /// can hit with.
    strikes: Vec<(HandTarget, Deed)>,
    /// The different gestures set aside for a failure remembered.
    skipped: u32,
}

impl Plausible {
    fn seen(
        world: &World,
        rules: &Content,
        here: Pos,
        mind: &Mind,
        (have, near): (&[(ItemId, u16)], i32),
    ) -> Self {
        let mut out = Self::default();
        let mut aside: Vec<Deed> = Vec::new();
        let mut remembered = |deed: Deed| {
            let failed = mind.handwork.failed(deed);
            if failed && !aside.contains(&deed) {
                aside.push(deed);
            }
            failed
        };
        let tools: Vec<(ItemId, u16)> = have
            .iter()
            .copied()
            .filter(|&(_, m)| cuts_to_the_eye(rules, m))
            .collect();
        if !tools.is_empty() {
            for (chunk, key, at, species) in trees_near(world, here, near, mind) {
                let Ok(on) = u16::try_from(species) else {
                    continue;
                };
                // A tree shows its trunk and its branches: a cut is
                // of one or the other, and nothing says which.
                for &(with, with_m) in &tools {
                    for part in Part::ALL {
                        let deed = Deed {
                            gesture: Gesture::Cut,
                            on: match part {
                                Part::Trunk => on,
                                Part::Branch => on | BRANCH,
                            },
                            with: with_m,
                        };
                        if !remembered(deed) {
                            let target = HandTarget::Tree {
                                chunk,
                                key,
                                at,
                                with,
                                part,
                            };
                            out.cuts.push((target, deed));
                        }
                    }
                }
            }
        }
        for &(item, on) in have.iter().filter(|&&(_, m)| bends_to_the_eye(rules, m)) {
            let deed = Deed {
                gesture: Gesture::Fold,
                on,
                with: NOTHING,
            };
            if !remembered(deed) {
                out.folds.push((HandTarget::Item(item, here), deed));
            }
        }
        for &(into, with) in have.iter().filter(|&&(_, m)| holds_to_the_eye(rules, m)) {
            for &(item, on) in have {
                // What lies in a thing already is not put in another.
                if item == into || world.item(item).is_none_or(|t| t.within().is_some()) {
                    continue;
                }
                // A hollow that shows no room for it is not tried.
                if world.hollow_shows_no_room(into, item, rules) {
                    continue;
                }
                let deed = Deed {
                    gesture: Gesture::PutIn,
                    on,
                    with,
                };
                if !remembered(deed) {
                    out.puts.push((HandTarget::Put { item, into }, deed));
                }
            }
        }
        out.strikes = strikes(world, rules, have, &mut remembered);
        // What lies in a thing it carries is seen there, and may be taken
        // out again (amended 27 November 2026).
        for &(item, on) in have {
            let Some(hollow) = world.item(item).and_then(crate::Item::within) else {
                continue;
            };
            let Some(&(_, with)) = have.iter().find(|(i, _)| *i == hollow) else {
                continue;
            };
            let deed = Deed {
                gesture: Gesture::TakeOut,
                on,
                with,
            };
            if !remembered(deed) {
                out.takes.push((HandTarget::Take { item }, deed));
            }
        }
        out.skipped = u32::try_from(aside.len()).unwrap_or(u32::MAX);
        out
    }
}

/// Whether a thing of the kind `kind` looks like it could cut: it shows
/// an edge (the content's `edge_seen`). How keen against what, a body
/// finds by trying.
fn cuts_to_the_eye(rules: &Content, kind: u16) -> bool {
    stuff(rules, kind).is_some_and(|m| m.edge >= rules.needs.hands.edge_seen.max(1))
}

/// Whether a thing of the kind `kind` looks like it could be folded: it
/// is a sheet, stiff or not, or gives in hand (the content's
/// `supple_seen`), and is not folded already.
fn bends_to_the_eye(rules: &Content, kind: u16) -> bool {
    !kind_hollow(kind)
        && stuff(rules, kind).is_some_and(|m| {
            m.form == Form::Sheet || m.suppleness >= rules.needs.hands.supple_seen.max(1)
        })
}

/// The blows a body sees as plausible (D57, amended 27 November 2026):
/// any thing in the hand may be struck with a thing one can hit with, a
/// block or a stick, no food, no sheet, no strand; those `remembered` as
/// having given nothing set aside. Which of the two breaks, if any, it
/// does not see.
fn strikes(
    world: &World,
    rules: &Content,
    have: &[(ItemId, u16)],
    remembered: &mut impl FnMut(Deed) -> bool,
) -> Vec<(HandTarget, Deed)> {
    let mut out: Vec<(HandTarget, Deed)> = Vec::new();
    let free = |i: ItemId| world.item(i).is_some_and(|t| t.within().is_none());
    for &(with, with_m) in have.iter().filter(|&&(_, m)| hits_to_the_eye(rules, m)) {
        for &(item, on) in have {
            if item == with
                || kind_hollow(on)
                || loose_food(rules, on)
                || !free(item)
                || !free(with)
            {
                continue;
            }
            let deed = Deed {
                gesture: Gesture::Strike,
                on,
                with: with_m,
            };
            if !remembered(deed) && !out.iter().any(|(_, d)| *d == deed) {
                out.push((HandTarget::Strike { item, with }, deed));
            }
        }
    }
    out
}

/// The blow of `deed` to do again: the two kinds of thing it names, both
/// carried and in no hollow.
fn strike_again(world: &World, have: &[(ItemId, u16)], deed: Deed) -> Option<(HandTarget, Deed)> {
    let free = |i: ItemId| world.item(i).is_some_and(|t| t.within().is_none());
    let of = |kind: u16| have.iter().find(|&&(i, m)| m == kind && free(i));
    let (&(item, _), &(with, _)) = (of(deed.on)?, of(deed.with)?);
    (item != with).then_some((HandTarget::Strike { item, with }, deed))
}

/// Whether a thing of the kind `kind` looks like one can hit with it: a
/// block or a stick that is no food and no hollow. What breaks under it,
/// or whether it breaks itself, a body finds by trying.
fn hits_to_the_eye(rules: &Content, kind: u16) -> bool {
    !kind_hollow(kind)
        && stuff(rules, kind)
            .is_some_and(|m| m.food.is_none() && matches!(m.form, Form::Block | Form::Stick))
}

/// Whether a thing of the kind `kind` looks like it could hold what is
/// put in or on it: it is folded into a hollow, or is a sheet. Whether
/// anything stays there, a body finds by trying.
fn holds_to_the_eye(rules: &Content, kind: u16) -> bool {
    kind_hollow(kind) || stuff(rules, kind).is_some_and(|m| m.form == Form::Sheet)
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
    let new = |deed: Deed| !mind.ways.hands.contains(deed);
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
    // The cuts, folds and put-ins it sees as plausible here, those it
    // remembers failed set aside (amended 27 November 2026).
    let plausible = Plausible::seen(world, rules, here, mind, (&have, near));
    [&plausible.folds, &plausible.puts, &plausible.cuts]
        .into_iter()
        .flatten()
        .any(|&(_, deed)| new(deed))
}

/// The first deed of `deeds` it lacks what it gives and can do here: a
/// fold of a thing it carries, a cut of a tree near with a thing it
/// carries, a thing near to pick up. For a cut with no thing of its tool
/// carried, the taking up of one lying near (D57, amended 25 November
/// 2026: "cut a birch with a flint" holds the flint), marked `true`. A
/// put-in comes first ([`put_again`]).
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
    if let Some(again) = take_again(world, rules, id, (deeds, have), mind)
        .or_else(|| put_again(world, rules, id, (deeds, have), mind))
    {
        return Some((again, false));
    }
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
                        Gesture::Fold | Gesture::PutIn | Gesture::TakeOut | Gesture::Strike => None,
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
    for gesture in [
        Gesture::Fold,
        Gesture::Cut,
        Gesture::Strike,
        Gesture::PickUp,
    ] {
        for deed in deeds.iter().filter(|d| d.gesture == gesture) {
            if !lacks(deed) {
                continue;
            }
            match gesture {
                Gesture::PutIn | Gesture::TakeOut => {}
                Gesture::Strike => {
                    if let Some(again) = strike_again(world, have, deed) {
                        return Some((again, false));
                    }
                }
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
                    if let Some(&(chunk, key, at, _)) =
                        trees_near(world, here, near, mind).iter().find(|t| {
                            cut_species(deed.on) == t.3
                                && bark_fits(world, rules, id, deed)
                                && !shows_bare(world, rules, deed, (t.0, t.1))
                        })
                    {
                        let part = cut_part(deed.on);
                        let target = HandTarget::Tree {
                            chunk,
                            key,
                            at,
                            with,
                            part,
                        };
                        return Some(((target, deed), false));
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
    if let HandTarget::Put { item, into } = target {
        begin(mind, deed, item);
        return Decision {
            intention: Intention::PutIn {
                body: id,
                item,
                into,
            },
            why: Why::Hands,
        };
    }
    if let HandTarget::Strike { item, with } = target {
        begin(mind, deed, item);
        return Decision {
            intention: Intention::Strike {
                body: id,
                item,
                with,
            },
            why: Why::Hands,
        };
    }
    if let HandTarget::Take { item } = target {
        begin(mind, deed, item);
        return Decision {
            intention: Intention::TakeOut { body: id, item },
            why: Why::Hands,
        };
    }
    if let Some(decision) = act_on(world, id, here, target, deed, mind) {
        return decision;
    }
    let to = match target {
        HandTarget::Put { .. } | HandTarget::Take { .. } | HandTarget::Strike { .. } => here,
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
        // Done where it stands, never stepped to ([`go_do`]).
        HandTarget::Put { .. } | HandTarget::Take { .. } | HandTarget::Strike { .. } => {
            return None;
        }
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
            part,
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
                part,
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
    mind.handwork.gave = None;
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
/// days; the deeds tried are forgotten with it, save those that served
/// ([`leave_not_kept`]).
pub(super) fn judge_trial(
    rules: &Content,
    per_year: u32,
    body: &crate::Body,
    mind: &mut Mind,
    trial: &super::Trial,
) -> (bool, super::Judged) {
    let (mut kept, mut judged) = super::judge(rules, per_year, body, mind, trial);
    if trial.way == Way::Hands {
        if kept {
            keep_what_served(rules, mind, &trial.before.hands, true);
        }
        kept &= mind.ways.hands != trial.before.hands;
        judged.kept = kept;
        if kept {
            judged.after.hands = mind.ways.hands;
        }
        mind.handwork.drop_aim();
    }
    (kept, judged)
}

/// A trial of the hands not kept (given up, lapsed, judged no better):
/// its ways go back to what they were, save the deeds of which something
/// came that served (D57, amended 27 November 2026, the project lead:
/// "the gesture works" is not "my problem is solved"). A hollow that held
/// food beyond the arms once and tore before the judge came is still a
/// thing its hands know how to make and fill; they are not noted as its
/// own kept trial, and nothing is told for it.
pub(super) fn leave_not_kept(rules: &Content, mind: &mut Mind, trial: &super::Trial) {
    if trial.way == Way::Hands {
        keep_what_served(rules, mind, &trial.before.hands, false);
    } else {
        trial.way.copy(&mut mind.ways, &trial.before);
    }
}

/// Of the deeds `mind` learnt since its hands knew `before`, only those
/// of which something came that served stay known (D57, amended 27
/// November 2026; the run of step 1 left "cut an oak with a flint" known
/// to fifteen people, a cut whose bark only ever broke at the fold): a
/// put-in of food in a kind of thing that served it (food lay in it
/// while it carried more than its arms alone hold), and, step by step
/// back from there, a deed whose product a deed kept is done on, with or
/// in. The rest is forgotten. Called when a
/// trial of the hands is kept, before what it leaves is told; the deeds
/// kept are noted as its own ([`Handwork::kept`]).
pub(super) fn keep_what_served(rules: &Content, mind: &mut Mind, before: &Deeds, own: bool) {
    let mut kept: Vec<Deed> = before.iter().collect();
    let mut left: Vec<Deed> = mind
        .ways
        .hands
        .iter()
        .filter(|d| !before.contains(*d))
        .collect();
    let serves = |kept: &[Deed], deed: Deed| {
        if deed.gesture == Gesture::PutIn {
            return loose_food(rules, deed.on) && mind.handwork.served(deed.with);
        }
        // A take-out served when it made room in a kind of thing a
        // put-in of food kept is done in.
        if deed.gesture == Gesture::TakeOut {
            return !loose_food(rules, deed.on)
                && kept.iter().any(|k| {
                    k.gesture == Gesture::PutIn && k.with == deed.with && loose_food(rules, k.on)
                });
        }
        product(rules, deed).is_some_and(|p| {
            kept.iter().any(|k| {
                k.with == p
                    || (matches!(k.gesture, Gesture::Fold | Gesture::PutIn | Gesture::Strike)
                        && k.on == p)
            })
        })
    };
    while let Some(i) = left.iter().position(|&d| serves(&kept, d)) {
        kept.push(left.remove(i));
    }
    let mut hands = Deeds::default();
    for &deed in &kept {
        hands.add(deed);
        if own && !before.contains(deed) && !mind.handwork.kept.contains(&deed) {
            mind.handwork.kept.push(deed);
        }
    }
    mind.ways.hands = hands;
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
        Gesture::Cut | Gesture::Fold | Gesture::PutIn | Gesture::TakeOut | Gesture::Strike => {
            mind.handwork.done.take()?
        }
    };
    mind.handwork.pending = None;
    // A trunk answers a cut by its state (amended 28 November 2026):
    // bare of its bark it gives its wood, or nothing to an edge too
    // blunt for wood, where the same cut gives bark on a whole tree. What
    // came is then the tree's answer, not the deed's: nothing is learnt
    // of the deed, and nothing remembered against it.
    let came = mind.handwork.gave.take();
    if deed.gesture == Gesture::Cut
        && let Some(gives) = product(rules, deed)
        && came.map_or(!worked, |thing| {
            world
                .item(thing)
                .and_then(|i| material_index(rules, i.material()))
                != Some(gives)
        })
    {
        return None;
    }
    if !worked {
        // What gave nothing or broke the thing is remembered (amended 27
        // November 2026), unless it is a deed its hands know: that one
        // failed for the moment's sake.
        if !mind.ways.hands.contains(deed) {
            mind.handwork.fail(deed, rules.needs.hands.failures_kept);
        }
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
    // A put-in that held, new to its hands, is a novelty as well: the
    // same things, one in the other.
    if deed.gesture == Gesture::PutIn
        && mind.handwork.practised(deed) == 0
        && !mind.ways.hands.contains(deed)
    {
        mind.handwork.novel.push((deed, deed.with));
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
        let day = today(world);
        trial.started_day = super::restarted_day(world, &rules.needs.trials, trial, day);
        // A trial from a problem that has just learnt something is not
        // judged before it has had the days to live its problem again
        // with it (amended 27 November 2026: in the run of seed 37 a hollow
        // folded and filled on the last evening of its season was given
        // up that same night).
        if let Some(aim) = trial.problem.as_mut() {
            aim.until = aim
                .until
                .max(day + u64::from(rules.needs.trials.trial_days));
        }
    }
    tool
}

#[cfg(test)]
mod tests {
    use super::super::{Brains, HeardDays, Lately, News, Trial, Ways, start_trial};
    use super::*;
    use crate::Event;
    use crate::memory::{Access, Record};
    use crate::time::Calendar;
    use std::collections::{BTreeMap, BTreeSet};

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
    fn a_trial_that_tried_eight_things_for_nothing_still_learns_the_ninth() {
        let mut hands = Deeds::default();
        for on in 0..8 {
            assert!(hands.add(Deed {
                gesture: Gesture::PutIn,
                on,
                with: 99,
            }));
        }
        let food = Deed {
            gesture: Gesture::PutIn,
            on: 50,
            with: 99,
        };
        assert!(hands.add(food), "room is left for the deed that serves");
        assert!(hands.contains(food));
    }

    #[test]
    fn the_deeds_of_a_container_are_read_from_the_content() {
        let (rules, _, _, _, birch) = by_a_birch();
        let mut deeds = bark_deeds(&rules, birch);
        deeds.add(Deed {
            gesture: Gesture::PutIn,
            on: material(&rules, "berries"),
            with: material(&rules, "birch_bark") | HOLLOW,
        });
        assert_eq!(container_deeds(&rules), deeds);
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
        brains.notice(&events, &world, &rules);
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
        brains.notice(&events, &world, &rules);
        let mind = brains.minds.get_mut(&id).unwrap();
        // A container carried: nothing more to do with its hands.
        assert_eq!(
            hands_turn(&world, &rules, (id, here), mind, (7, &mut Vec::new())),
            HandsTurn::Free
        );
        assert_eq!(world.item(item).unwrap().material(), "birch_bark");
        assert!(world.item(item).unwrap().hollow_ml() > 0, "folded");
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
    fn a_bare_trunk_is_not_cut_for_bark_and_its_answer_teaches_nothing_of_the_deed() {
        // Amended 28 November 2026: a trunk wears so much bark where the
        // hands reach, and bare, it answers the same cut with its wood.
        let (rules, mut world, mut brains, id, birch) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let flint = picked(&mut world, &rules, id, "flint");
        let knows = |brains: &Brains| {
            let mut mind = brains.minds.get(&id).unwrap().clone();
            mind.ways.hands = bark_deeds(&rules, birch);
            mind
        };
        // It knows the bark and lacks it: it cuts the birch beside it.
        let HandsTurn::Decide(Some(Decision {
            intention: Intention::Cut { chunk, tree, .. },
            ..
        })) = hands_turn(
            &world,
            &rules,
            (id, here),
            &mut knows(&brains),
            (7, &mut Vec::new()),
        )
        else {
            panic!("a cut");
        };
        // Another strips that trunk.
        let other = world.spawn_body(here);
        let its = picked(&mut world, &rules, other, "flint");
        let mut events = Vec::new();
        while world.bark_left(chunk, tree, &rules) != Some(0) {
            world.queue(Intention::Cut {
                body: other,
                with: its,
                chunk,
                tree,
                part: Part::Trunk,
            });
            world.advance(12, &rules, &mut events);
        }
        assert_eq!(world.notch_mm2(chunk, tree, &rules), 0);
        // The eye sees the wood bare: it does not cut that tree for bark.
        let turn = hands_turn(
            &world,
            &rules,
            (id, here),
            &mut knows(&brains),
            (7, &mut Vec::new()),
        );
        assert!(
            !matches!(
                turn,
                HandsTurn::Decide(Some(Decision {
                    intention: Intention::Cut { chunk: c, tree: t, .. },
                    ..
                })) if c == chunk && t == tree
            ),
            "{turn:?}"
        );
        // A cut made on it all the same (drawn: a tree it knows nothing
        // of) is answered by the tree's state, wood to flint and nothing
        // to quartz: neither learnt, practised, told nor remembered as
        // a gesture that gives nothing.
        for kind in ["flint", "quartz"] {
            let with = match kind {
                "flint" => flint,
                _ => picked(&mut world, &rules, id, kind),
            };
            let cut = Deed {
                gesture: Gesture::Cut,
                on: birch,
                with: material(&rules, kind),
            };
            let mind = brains.minds.get_mut(&id).unwrap();
            mind.handwork.pending = Some((cut, with));
            events.clear();
            world.queue(Intention::Cut {
                body: id,
                with,
                chunk,
                tree,
                part: Part::Trunk,
            });
            world.advance(12, &rules, &mut events);
            assert!(
                events.iter().any(|e| matches!(
                    e,
                    Event::Gestured { body, gave, .. }
                        if *body == id && gave.is_some() == (kind == "flint")
                )),
                "{kind}"
            );
            brains.notice(&events, &world, &rules);
            let mind = brains.minds.get_mut(&id).unwrap();
            let _ = hands_turn(&world, &rules, (id, here), mind, (7, &mut Vec::new()));
            assert_eq!(mind.handwork.practised(cut), 0, "{kind}");
            assert!(!mind.handwork.failed(cut), "{kind}");
            assert!(!mind.handwork.served(material(&rules, kind)), "{kind}");
            assert!(mind.handwork.novel.is_empty(), "{kind}");
            assert!(mind.ways.hands.is_empty(), "{kind}");
        }
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
        // A cut drawn in a trial is of a tree it knows nothing of: it
        // cannot foresee a bark, and may cut (amended 27 November 2026).
        mind.ways.hands = Deeds::default();
        let mut drawn_cuts = 0;
        for day in 0..200 {
            mind.handwork.tries = (day, 0);
            if let Some((_, deed)) = draw(
                &world,
                &rules,
                (id, here),
                mind,
                (day, &sharp, 4),
                &mut Vec::new(),
            ) {
                drawn_cuts += u32::from(deed.gesture == Gesture::Cut);
            }
        }
        assert!(drawn_cuts > 0);
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
        let hollow = world.spawn_hollow("birch_bark", 200, here, &rules);
        world.queue(Intention::PickUp {
            body: id,
            item: hollow,
        });
        world.advance(1, &rules, &mut Vec::new());
        let container = material(&rules, "birch_bark") | HOLLOW;
        let mind = brains.minds.get_mut(&id).unwrap();
        let _ = hands_turn(&world, &rules, (id, here), mind, (7, &mut Vec::new()));
        assert!(!mind.handwork.served(container), "empty, it served nothing");
        // Its arms full beside the hollow it carries: nothing holds by
        // itself, a third kilogram is refused, and the hollow served
        // nothing (amended 27 November 2026).
        let berries = world.spawn_item("berries", 1000, here);
        world.queue(Intention::PickUp {
            body: id,
            item: berries,
        });
        world.advance(1, &rules, &mut Vec::new());
        let more = world.spawn_item("berries", 1000, here);
        let mut events = Vec::new();
        world.queue(Intention::PickUp {
            body: id,
            item: more,
        });
        world.advance(1, &rules, &mut events);
        assert_eq!(world.body(id).unwrap().food_g(), 1000, "{events:?}");
        let mind = brains.minds.get_mut(&id).unwrap();
        let _ = hands_turn(&world, &rules, (id, here), mind, (7, &mut Vec::new()));
        assert!(!mind.handwork.served(container));
        // The berries put in it, more in its arms: it served.
        assert!(world.put_now(id, (berries, hollow), &rules).is_some());
        world.queue(Intention::PickUp {
            body: id,
            item: more,
        });
        world.advance(1, &rules, &mut events);
        assert_eq!(world.body(id).unwrap().food_g(), 2000, "{events:?}");
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
            if let Some((_, deed)) = draw(
                &world,
                &rules,
                (id, here),
                mind,
                (day, &blunt, 4),
                &mut Vec::new(),
            ) {
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
        let mut species = BTreeSet::new();
        let mut parts = BTreeSet::new();
        for day in 0..200 {
            mind.handwork.tries = (day, 0);
            if let Some((target, deed)) = draw(
                &world,
                &rules,
                (id, here),
                mind,
                (day, &sharp, 4),
                &mut Vec::new(),
            ) && deed.gesture == Gesture::Cut
            {
                cuts += 1;
                assert_eq!(deed.with, flint_m, "only the thing with an edge cuts");
                species.insert(deed.on & !BRANCH);
                // The deed names the part its target is cut on.
                let HandTarget::Tree { part, .. } = target else {
                    panic!("a cut is of a tree");
                };
                assert_eq!(cut_part(deed.on), part);
                parts.insert(part);
            }
        }
        // A tree shows its trunk and its branches: a cut is drawn on one
        // or the other, and nothing says which (amended 27 November 2026).
        assert_eq!(parts, Part::ALL.into_iter().collect());
        // Any tree near is tried: whether its bark comes away is not seen.
        let near: BTreeSet<u16> = trees_near(&world, here, 4, mind)
            .iter()
            .map(|t| u16::try_from(t.3).unwrap())
            .collect();
        assert_eq!(species, near);
        assert!(
            cuts > 0,
            "the flint beside the birch {birch} is drawn to cut"
        );
    }

    #[test]
    fn what_a_body_sees_makes_a_gesture_plausible_and_it_may_be_wrong() {
        let (rules, ..) = by_a_birch();
        let kind = |id: &str| material(&rules, id);
        // An edge is seen, keen enough for this bark or not.
        for edged in ["flint", "quartz", "slate"] {
            assert!(cuts_to_the_eye(&rules, kind(edged)), "{edged}");
        }
        for blunt in ["pebble", "sandstone", "branch", "berries", "bone"] {
            assert!(!cuts_to_the_eye(&rules, kind(blunt)), "{blunt}");
        }
        // A sheet is seen, supple or stiff; so is what gives in hand.
        for bends in [
            "birch_bark",
            "oak_bark",
            "pine_outer_bark",
            "butterbur_leaf",
            "hide",
            "fibres",
            "nettle_leaves",
        ] {
            assert!(bends_to_the_eye(&rules, kind(bends)), "{bends}");
        }
        for stiff in ["flint", "branch", "berries", "hazelnuts"] {
            assert!(!bends_to_the_eye(&rules, kind(stiff)), "{stiff}");
        }
        assert!(!bends_to_the_eye(&rules, kind("birch_bark") | HOLLOW));
        // Of the plausible, some give nothing: no product to learn.
        let fold = |id: &str| Deed {
            gesture: Gesture::Fold,
            on: kind(id),
            with: NOTHING,
        };
        assert_eq!(
            product(&rules, fold("birch_bark")),
            Some(kind("birch_bark") | HOLLOW)
        );
        assert_eq!(
            product(&rules, fold("butterbur_leaf")),
            Some(kind("butterbur_leaf") | HOLLOW)
        );
        for fails in ["oak_bark", "pine_outer_bark", "fibres", "nettle_leaves"] {
            assert_eq!(product(&rules, fold(fails)), None, "{fails}");
        }
        let species = |id: &str| {
            u16::try_from(rules.species.iter().position(|s| s.id == id).unwrap()).unwrap()
        };
        let cut = |tree: &str, with: &str| Deed {
            gesture: Gesture::Cut,
            on: species(tree),
            with: kind(with),
        };
        assert_eq!(
            product(&rules, cut("birch", "quartz")),
            Some(kind("birch_bark"))
        );
        assert_eq!(product(&rules, cut("birch", "slate")), None);
        assert_eq!(product(&rules, cut("oak", "quartz")), None);
        // A hazel has no bark that comes away: flint gives the chips of
        // its wood (amended 27 November 2026, trees in parts).
        assert_eq!(
            product(&rules, cut("hazel", "flint")),
            Some(kind("wood_chips"))
        );
        assert_eq!(product(&rules, cut("hazel", "quartz")), None);
    }

    #[test]
    fn a_fold_drawn_that_breaks_the_thing_is_lived_and_nothing_is_learnt() {
        let (rules, mut world, mut brains, id, _) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let oak = picked(&mut world, &rules, id, "oak_bark");
        let oak_m = material(&rules, "oak_bark");
        let day = today(&world);
        let mind = brains.minds.get_mut(&id).unwrap();
        in_a_trial(mind, &[], day);
        // The stiff bark is a sheet: folding it is drawn sooner or later.
        let have = [(oak, oak_m)];
        let mut drawn = None;
        for seed in 0..200 {
            mind.handwork.tries = (day, 0);
            if let Some((target, deed)) = draw(
                &world,
                &rules,
                (id, here),
                mind,
                (seed, &have, 4),
                &mut Vec::new(),
            ) && deed.gesture == Gesture::Fold
            {
                drawn = Some((target, deed));
                break;
            }
        }
        let (target, deed) = drawn.expect("a fold of the oak bark drawn");
        assert_eq!(deed.on, oak_m);
        let decision = go_do(&world, &rules, (id, here), (target, deed), mind);
        assert!(
            mind.handwork.gestured,
            "a gesture new to its hands: it tried"
        );
        let mut events = Vec::new();
        world.queue(decision.intention);
        world.advance(12, &rules, &mut events);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Gestured { broke: true, gave: None, on, .. } if on == "oak_bark"
        )));
        assert!(world.item(oak).is_none(), "the bark is lost");
        brains.notice(&events, &world, &rules);
        let mind = brains.minds.get_mut(&id).unwrap();
        let _ = hands_turn(&world, &rules, (id, here), mind, (7, &mut Vec::new()));
        assert!(
            mind.ways.hands.is_empty(),
            "nothing came of it: nothing learnt"
        );
        assert_eq!(mind.handwork.practised(deed), 0);
        assert!(mind.handwork.novel.is_empty(), "nothing to tell");
        // It remembers that folding an oak bark broke it (amended 27
        // November 2026): with another in its hands, the fold is not
        // drawn again, and the draw says what it set aside.
        assert!(mind.handwork.failed(deed));
        let oak = picked(&mut world, &rules, id, "oak_bark");
        let mind = brains.minds.get_mut(&id).unwrap();
        let have = [(oak, oak_m)];
        let mut news = Vec::new();
        for seed in 0..200 {
            mind.handwork.tries = (day, 0);
            let drawn = draw(
                &world,
                &rules,
                (id, here),
                mind,
                (seed, &have, 4),
                &mut news,
            );
            assert!(drawn.is_none_or(|(_, d)| d != deed), "not again");
        }
        assert!(
            news.iter()
                .all(|n| matches!(n, News::Skipped { body, deeds: 1, .. } if *body == id))
                && news.len() == 200,
            "{news:?}"
        );
    }

    #[test]
    fn the_failures_remembered_are_a_bounded_list_the_oldest_forgotten() {
        let deed = |on| Deed {
            gesture: Gesture::Fold,
            on,
            with: NOTHING,
        };
        let mut hands = Handwork::default();
        for on in 0..3 {
            hands.fail(deed(on), 2);
            hands.fail(deed(on), 2);
        }
        assert_eq!(hands.failed, [deed(1), deed(2)]);
        assert!(!hands.failed(deed(0)), "the oldest is forgotten");
        // None kept: none remembered.
        hands.fail(deed(7), 0);
        assert_eq!(hands.failed, []);
        assert_eq!(Content::sample().needs.hands.failures_kept, 16);
    }

    /// Carries a birch bark folded into a hollow: the hollow.
    fn with_a_hollow(world: &mut World, rules: &Content, id: BodyId) -> ItemId {
        let here = world.body(id).unwrap().pos();
        let hollow = world.spawn_hollow("birch_bark", 200, here, rules);
        world.queue(Intention::PickUp {
            body: id,
            item: hollow,
        });
        world.advance(1, rules, &mut Vec::new());
        hollow
    }

    /// Lays `grams` of `material` at its feet and has it picked up.
    fn loaded(
        world: &mut World,
        rules: &Content,
        id: BodyId,
        material: &str,
        grams: u32,
    ) -> ItemId {
        let here = world.body(id).unwrap().pos();
        let item = world.spawn_item(material, grams, here);
        world.queue(Intention::PickUp { body: id, item });
        world.advance(1, rules, &mut Vec::new());
        assert!(world.body(id).unwrap().has(item));
        item
    }

    /// Does `decision`, lets the gesture end, and has the mind see what
    /// came of it.
    fn done(
        world: &mut World,
        rules: &Content,
        brains: &mut Brains,
        id: BodyId,
        decision: &Decision,
    ) {
        let mut events = Vec::new();
        world.queue(decision.intention.clone());
        world.advance(12, rules, &mut events);
        brains.notice(&events, world, rules);
        let here = world.body(id).unwrap().pos();
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.tries = (today(world), u32::MAX);
        let _ = hands_turn(world, rules, (id, here), mind, (7, &mut Vec::new()));
    }

    #[test]
    fn a_put_in_is_drawn_in_what_looks_like_it_holds_and_may_give_nothing() {
        let (rules, mut world, mut brains, id, _) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let hollow = with_a_hollow(&mut world, &rules, id);
        let flat = picked(&mut world, &rules, id, "oak_bark");
        let berries = loaded(&mut world, &rules, id, "berries", 400);
        let (berries_m, oak_m) = (material(&rules, "berries"), material(&rules, "oak_bark"));
        let hollow_m = material(&rules, "birch_bark") | HOLLOW;
        let have = [(hollow, hollow_m), (flat, oak_m), (berries, berries_m)];
        let day = today(&world);
        let mind = brains.minds.get_mut(&id).unwrap();
        in_a_trial(mind, &[], day);
        // A thing is put in a hollow or on a sheet, never in berries: of
        // the six put-ins the eye allows, none is preferred.
        let mut puts = BTreeMap::new();
        for seed in 0..400 {
            mind.handwork.tries = (day, 0);
            if let Some((target, deed)) = draw(
                &world,
                &rules,
                (id, here),
                mind,
                (seed, &have, 4),
                &mut Vec::new(),
            ) && deed.gesture == Gesture::PutIn
            {
                puts.insert(deed, target);
            }
        }
        assert_eq!(puts.len(), 4, "{puts:?}");
        assert!(puts.keys().all(|d| d.with == hollow_m || d.with == oak_m));
        let put = |on, with| Deed {
            gesture: Gesture::PutIn,
            on,
            with,
        };
        // On the flat bark, the berries do not stay: nothing learnt, the
        // failure remembered.
        let on_flat = put(berries_m, oak_m);
        let mind = brains.minds.get_mut(&id).unwrap();
        let decision = go_do(&world, &rules, (id, here), (puts[&on_flat], on_flat), mind);
        assert!(mind.handwork.gestured);
        done(&mut world, &rules, &mut brains, id, &decision);
        let mind = brains.minds.get_mut(&id).unwrap();
        assert!(mind.handwork.failed(on_flat) && !mind.ways.hands.contains(on_flat));
        assert_eq!(world.body(id).unwrap().stowed_g(), 0);
        // In the hollow they stay: learnt in its trial, a novelty to
        // tell, its arms free.
        let in_hollow = put(berries_m, hollow_m);
        let decision = go_do(
            &world,
            &rules,
            (id, here),
            (puts[&in_hollow], in_hollow),
            mind,
        );
        done(&mut world, &rules, &mut brains, id, &decision);
        let mind = &brains.minds[&id];
        assert!(mind.ways.hands.contains(in_hollow));
        assert_eq!(mind.handwork.practised(in_hollow), 1);
        assert_eq!(mind.handwork.novel, [(in_hollow, hollow_m)]);
        let body = world.body(id).unwrap();
        assert_eq!((body.stowed_g(), body.arms_food_g()), (400, 0));
    }

    #[test]
    fn a_body_whose_hollow_is_full_of_stones_takes_one_out_when_its_arms_are_full() {
        let (rules, mut world, mut brains, id, _) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let hollow = with_a_hollow(&mut world, &rules, id);
        let hollow_m = material(&rules, "birch_bark") | HOLLOW;
        let room = world.hollow_room_g(hollow, &rules);
        let pebble = loaded(&mut world, &rules, id, "pebble", room);
        world.queue(Intention::PutIn {
            body: id,
            item: pebble,
            into: hollow,
        });
        world.advance(600, &rules, &mut Vec::new());
        assert_eq!(world.hollow_room_g(hollow, &rules), 0);
        let put = Deed {
            gesture: Gesture::PutIn,
            on: material(&rules, "berries"),
            with: hollow_m,
        };
        let take = Deed {
            gesture: Gesture::TakeOut,
            on: material(&rules, "pebble"),
            with: hollow_m,
        };
        let turn = |world: &World, brains: &mut Brains| {
            let mind = brains.minds.get_mut(&id).unwrap();
            hands_turn(world, &rules, (id, here), mind, (7, &mut Vec::new()))
        };
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.served = vec![hollow_m, material(&rules, "pebble")];
        mind.ways.hands.add(put);
        // It knows the put-in alone: its arms full, the hollow full of
        // stone, nothing is done.
        let nuts = loaded(&mut world, &rules, id, "hazelnuts", 1000);
        assert_eq!(world.body(id).unwrap().food_room_g(&rules.movement), 0);
        assert_eq!(turn(&world, &mut brains), HandsTurn::Free);
        // It knows the take-out as well: the stone comes out, then the
        // food goes in.
        brains.minds.get_mut(&id).unwrap().ways.hands.add(take);
        let HandsTurn::Decide(Some(decision)) = turn(&world, &mut brains) else {
            panic!("a take-out");
        };
        assert!(
            matches!(decision.intention, Intention::TakeOut { item, .. } if item == pebble),
            "{decision:?}"
        );
        done(&mut world, &rules, &mut brains, id, &decision);
        assert!(world.item(pebble).unwrap().within().is_none());
        let HandsTurn::Decide(Some(decision)) = turn(&world, &mut brains) else {
            panic!("a put-in");
        };
        assert!(
            matches!(decision.intention, Intention::PutIn { item, into, .. } if item == nuts && into == hollow),
            "{decision:?}"
        );
    }

    #[test]
    fn a_body_that_knows_a_put_in_empties_its_full_arms_in_the_hollow_without_a_thought() {
        let (rules, mut world, mut brains, id, _) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let hollow = with_a_hollow(&mut world, &rules, id);
        let hollow_m = material(&rules, "birch_bark") | HOLLOW;
        let known = Deed {
            gesture: Gesture::PutIn,
            on: material(&rules, "berries"),
            with: hollow_m,
        };
        let turn = |world: &World, brains: &mut Brains| {
            let mind = brains.minds.get_mut(&id).unwrap();
            hands_turn(world, &rules, (id, here), mind, (7, &mut Vec::new()))
        };
        // It does not know the put-in: its arms full beside the hollow,
        // nothing goes in by itself, and no thought puts it there.
        brains.minds.get_mut(&id).unwrap().handwork.served = vec![hollow_m];
        let nuts = loaded(&mut world, &rules, id, "hazelnuts", 600);
        assert_eq!(turn(&world, &mut brains), HandsTurn::Free);
        // It heard it told and tries it (a tale tried): the loose food of
        // its arms, full or not, goes in the hollow the tale names.
        let day = today(&world);
        in_a_trial(brains.minds.get_mut(&id).unwrap(), &[known], day);
        let HandsTurn::Decide(Some(heard)) = turn(&world, &mut brains) else {
            panic!("the put-in heard, tried");
        };
        assert!(
            matches!(heard.intention, Intention::PutIn { item, into, .. } if item == nuts && into == hollow)
        );
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.trial = None;
        mind.handwork.drop_aim();
        // It knows it, no trial under way: its arms not full, it does
        // nothing; full, it puts what they hold in the hollow, though the
        // deed was learnt with berries.
        brains.minds.get_mut(&id).unwrap().ways.hands.add(known);
        assert_eq!(turn(&world, &mut brains), HandsTurn::Free);
        let more = loaded(&mut world, &rules, id, "hazelnuts", 400);
        assert_eq!(world.body(id).unwrap().food_room_g(&rules.movement), 0);
        let HandsTurn::Decide(Some(decision)) = turn(&world, &mut brains) else {
            panic!("a put-in");
        };
        assert!(
            matches!(decision.intention, Intention::PutIn { into, .. } if into == hollow),
            "{decision:?}"
        );
        done(&mut world, &rules, &mut brains, id, &decision);
        // One pile went in: its arms have room again, it lacks nothing.
        let body = world.body(id).unwrap();
        assert_eq!((body.stowed_g(), body.arms_food_g()), (600, 400));
        assert_eq!(world.item(nuts).unwrap().within(), Some(hollow));
        assert!(world.item(more).unwrap().within().is_none());
        assert_eq!(turn(&world, &mut brains), HandsTurn::Free);
        assert_eq!(brains.minds[&id].handwork.practised(known), 1);
        assert!(brains.minds[&id].trial.is_none(), "no trial for it");
    }

    #[test]
    fn a_kept_trial_leaves_known_only_the_deeds_whose_product_served() {
        let (rules, _, mut brains, id, birch) = by_a_birch();
        let flint = material(&rules, "flint");
        let hollow_m = material(&rules, "birch_bark") | HOLLOW;
        let oak = u16::try_from(rules.species.iter().position(|s| s.id == "oak").unwrap()).unwrap();
        let cut_oak = Deed {
            gesture: Gesture::Cut,
            on: oak,
            with: flint,
        };
        let put = Deed {
            gesture: Gesture::PutIn,
            on: material(&rules, "berries"),
            with: hollow_m,
        };
        let put_flint = Deed {
            gesture: Gesture::PutIn,
            on: flint,
            with: hollow_m,
        };
        let mind = brains.minds.get_mut(&id).unwrap();
        // It learnt the birch's chain, a put-in of berries, a put-in of
        // its flint, and the cut of an oak whose bark only breaks.
        let learn = |mind: &mut Mind| {
            mind.ways.hands = bark_deeds(&rules, birch);
            for deed in [cut_oak, put, put_flint] {
                assert!(mind.ways.hands.add(deed));
            }
        };
        // Nothing served: nothing of it stays.
        learn(mind);
        keep_what_served(&rules, mind, &Deeds::default(), true);
        assert!(mind.ways.hands.is_empty() && mind.handwork.kept.is_empty());
        // The hollow held food beyond its arms: the put-in of food, the
        // fold that gave the hollow, the cut that gave the bark and the
        // flint taken for it stay; the oak's cut and the flint put in do
        // not.
        learn(mind);
        mind.handwork.serve(hollow_m);
        mind.handwork.serve(flint);
        keep_what_served(&rules, mind, &Deeds::default(), true);
        let mut served = bark_deeds(&rules, birch);
        served.add(put);
        assert_eq!(mind.ways.hands, served);
        assert_eq!(mind.handwork.kept.len(), 4);
        // What it knew before the trial is not judged again.
        let mut before = Deeds::default();
        before.add(cut_oak);
        mind.ways.hands.add(cut_oak);
        keep_what_served(&rules, mind, &before, true);
        assert!(mind.ways.hands.contains(cut_oak));
        assert!(
            !mind.handwork.kept.contains(&cut_oak),
            "not from a kept trial"
        );
    }

    #[test]
    fn a_trial_not_kept_leaves_the_deeds_that_served_and_none_as_its_own() {
        // D57, amended 27 November 2026: "the gesture works" is not "my
        // problem is solved". Given up, a trial leaves what served.
        let (rules, _, mut brains, id, birch) = by_a_birch();
        let flint = material(&rules, "flint");
        let hollow_m = material(&rules, "birch_bark") | HOLLOW;
        let put = Deed {
            gesture: Gesture::PutIn,
            on: material(&rules, "berries"),
            with: hollow_m,
        };
        let mind = brains.minds.get_mut(&id).unwrap();
        in_a_trial(mind, &[], 0);
        let trial = mind.trial.expect("in a trial");
        let learn = |mind: &mut Mind| {
            mind.ways.hands = bark_deeds(&rules, birch);
            assert!(mind.ways.hands.add(put));
        };
        // Nothing served: its ways as before.
        learn(mind);
        leave_not_kept(&rules, mind, &trial);
        assert!(mind.ways.hands.is_empty());
        // The hollow held food beyond its arms once: the chain stays
        // known, though the trial is not kept and nothing is its own.
        learn(mind);
        mind.handwork.serve(hollow_m);
        mind.handwork.serve(flint);
        leave_not_kept(&rules, mind, &trial);
        let mut served = bark_deeds(&rules, birch);
        served.add(put);
        assert_eq!(mind.ways.hands, served);
        assert!(mind.handwork.kept.is_empty(), "not from a kept trial");
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
        // D57, amended 25 November 2026: it heard of taking a hide, none
        // lies here; a pebble lies at its feet. Before, its
        // hands rested; now it draws as any trial does.
        let (rules, mut world, mut brains, id, _) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let pebble = laid(&mut world, &rules, id, "pebble");
        let day = today(&world);
        let mind = brains.minds.get_mut(&id).unwrap();
        let heard = Deed {
            gesture: Gesture::PickUp,
            on: material(&rules, "hide"),
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

    #[test]
    fn a_cut_names_the_part_of_the_tree_as_a_flag_on_its_kind() {
        // D57, amended 27 November 2026: trees in parts.
        let (mut rules, world, mut brains, id, birch) = by_a_birch();
        rules.needs.hands.trees = Some(crate::content::TreeParts::placeholders());
        let flint_m = material(&rules, "flint");
        let trunk = Deed {
            gesture: Gesture::Cut,
            on: birch,
            with: flint_m,
        };
        let branch = Deed {
            on: birch | BRANCH,
            ..trunk
        };
        assert_eq!(
            (cut_species(trunk.on), cut_part(trunk.on)),
            (usize::from(birch), Part::Trunk)
        );
        assert_eq!(
            (cut_species(branch.on), cut_part(branch.on)),
            (usize::from(birch), Part::Branch)
        );
        assert_ne!(trunk, branch, "two deeds");
        // What each gives, as the world computes it: the bark, the branch.
        assert_eq!(product(&rules, trunk), Some(material(&rules, "birch_bark")));
        assert_eq!(
            product(&rules, branch),
            Some(material(&rules, "green_branch"))
        );
        // An edge too blunt for living wood takes no branch; on a trunk
        // with no bark that comes away, a keen one gives chips.
        let quartz = Deed {
            with: material(&rules, "quartz"),
            ..branch
        };
        assert_eq!(product(&rules, quartz), None);
        let hazel =
            u16::try_from(rules.species.iter().position(|s| s.id == "hazel").unwrap()).unwrap();
        let chips = Deed { on: hazel, ..trunk };
        assert_eq!(product(&rules, chips), Some(material(&rules, "wood_chips")));
        // Both are seen as plausible on every tree near, with the thing
        // that shows an edge; a failure remembered on one part sets that
        // part aside, not the other.
        let here = world.body(id).unwrap().pos();
        let mut world = world;
        let flint = picked(&mut world, &rules, id, "flint");
        let mind = brains.minds.get_mut(&id).unwrap();
        let have = [(flint, flint_m)];
        let seen = Plausible::seen(&world, &rules, here, mind, (&have, 4));
        let trees = trees_near(&world, here, 4, mind).len();
        assert!(trees > 0);
        assert_eq!(seen.cuts.len(), 2 * trees);
        assert!(seen.cuts.iter().any(|(_, d)| *d == trunk));
        assert!(seen.cuts.iter().any(|(_, d)| *d == branch));
        mind.handwork.fail(branch, rules.needs.hands.failures_kept);
        let seen = Plausible::seen(&world, &rules, here, mind, (&have, 4));
        assert!(seen.cuts.iter().any(|(_, d)| *d == trunk));
        assert!(!seen.cuts.iter().any(|(_, d)| *d == branch));
    }
}
