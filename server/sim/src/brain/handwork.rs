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
//!
//! Amended 28 November 2026 (the project lead, after the audit of the
//! trials: "a success they want to keep, they keep and redo again and
//! again; but success must be well defined: if it is 'I got bark' and
//! that bark is good for nothing, it has no interest. Humans are lazy"):
//! seen is not kept. A deed that gives a thing in a trial is seen, a
//! memory "this gives that" ([`Handwork::seen`]); the deeds its hands
//! keep (`ways.hands`) are only those of a chain that relieved a problem
//! it lived ([`keep_what_served`]), and only those are done again when
//! their product is missing, with the seen deeds that feed them or feed
//! what the trial under way is after ([`wanted`]).
//!
//! Amended the same day, three rules more. **One verdict**: a trial of
//! the hands is kept if and only if a chain of it served its lived
//! problem ([`keep_what_served`]), at every ending. **Not as told**: a
//! deed done for what it gives, on a thing that did not give it into its
//! hands, is remembered of that thing and not done again on it
//! ([`Handwork::amiss`]). **Free hands** (the project lead: "with full
//! arms the hands are not free; one cannot do an action. He lays the
//! thing down, does his trial, and takes it back afterwards, if he does
//! not forget"): before a gesture other than a taking up, what its arms
//! hold beside the gesture's own things is laid at its feet, a thing a
//! thought ([`act_on`]); its hands idle again, it takes up what it still
//! wants of them, by what it prefers ([`take_back`]).
//!
//! Amended the same day, third round (the project lead: "one tries, and
//! if it does not work and brings nothing, one stops. One can take it up
//! again later... I cut wood, for now it is useless, I stop. Three weeks
//! later, a year later, fire is found, and fire needs wood, and I know
//! that cutting a tree gives wood: it is in my memory"). **A known
//! result is not tried again**: a deed whose result a body knows from
//! before the trial under way, and that it does not keep, is no aim of a
//! tale heard and is set aside by the draw ([`Handwork::knows`],
//! [`sift`], [`Plausible::seen`]), but for a rare chance to take it up
//! again ([`Again`]). **The memory of results lasts**: what gave a thing
//! ([`Handwork::seen`]) and what gave nothing ([`Handwork::failed`]) are
//! one entry a deed, never pushed out by a newer one in a life. **The
//! link by what a deed gives** works from that memory ([`wanted`]).
//!
//! Amended the same day, fourth round. **The chain starts from a lack**:
//! its hands walk back from the thing a deed done for its own sake
//! lacks, stop at the first thing held or lying at hand, and never come
//! twice to the same kind of thing ([`needed`], [`Chain`], [`ends`]).
//! **Credit to the things that took part**: a trial's end keeps the
//! deeds on the thread of the very thing that served, not every deed
//! whose product is of a kind a kept deed uses ([`Handwork::thread`],
//! [`keep_what_served`]).

use crate::content::{Content, Folded, Form};
use crate::r#gen::scatter::{self, purpose};
use crate::hands::{Gesture, Part};
use crate::map::{ChunkPos, FinePos, Pos};
use crate::memory::{Record, Store};
use crate::trees::TreeKey;
use crate::{BodyId, GATHER_REACH_MM, Intention, ItemId, World};

use super::{AROUND, Decision, Doing, Mind, News, Way, Why, today, walkable};

/// The most deeds a body holds in its ways. Sixteen (27 November 2026;
/// six before): a trial then held every deed it tried until its judge
/// forgot those that served nothing, and with six, a trial that had put
/// stones and branches in its hollow had no room left to learn the
/// put-in of food, and was given up while it carried more than its arms
/// (five of thirteen in the run of seed 37). Since 28 November 2026 its
/// ways hold only the deeds kept, and what a trial saw is held apart
/// ([`Handwork::seen`]).
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
    /// The thing a deed done there is done on, as its hands remember it
    /// when the deed did not give what was told: the tree, the thing
    /// folded, struck or taken out, the hollow put into.
    fn thing(self) -> Barred {
        match self {
            HandTarget::Put { into, .. } => Barred::Item(into),
            other => other.barred(),
        }
    }

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

/// A thing a body laid at its feet to free its hands (D57, amended 28
/// November 2026).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Laid {
    /// The thing.
    pub item: ItemId,
    /// Where it laid it.
    pub at: Pos,
    /// It went beyond its hands' sight of it since.
    pub left: bool,
}

/// A gesture of the trial under way that gave a thing (D57, amended 28
/// November 2026, fourth round): the thread a trial's end walks back
/// from the thing that served.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Made {
    /// The deed.
    pub(super) deed: Deed,
    /// The things it was done on and with or in.
    pub(super) used: [Option<ItemId>; 2],
    /// The thing that came of it.
    pub(super) gave: ItemId,
}

/// The most gestures a trial's thread holds, the oldest going beyond: a
/// guard, a trial makes a few dozen things.
const THREAD: usize = 256;

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
    /// 2026: "c'est propre d'un cerveau"): a container that held food
    /// beyond its arms, the tool of a cut of the chain kept for it
    /// (amended 28 November 2026: before, any tool whose cut gave
    /// something). Its own memory of use, no perception: such a thing is
    /// kept, never let go.
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
    /// (D57, amended 27 November 2026), the oldest first: "this gives
    /// nothing". A result known, as lasting as [`Handwork::seen`]
    /// (amended 28 November 2026, third round): not drawn nor aimed at
    /// again, but for the rare chance ([`Again`]). A deed is in one of
    /// the two lists at most; the content's `failures_kept` is a guard
    /// no life reaches.
    pub failed: Vec<Deed>,
    /// The deeds a kept trial of its own left it: something came of them
    /// that served.
    pub kept: Vec<Deed>,
    /// The deeds it saw give a thing in its own hands and does not keep
    /// (D57, amended 28 November 2026: seen is not kept): a memory,
    /// "this gives that, and it brought me nothing", the oldest first.
    /// It lasts (third round, the project lead: "it is in my memory a
    /// year later"): one entry a deed, six bytes, never pushed out by a
    /// newer one in a life; the content's `seen_kept` is a guard no life
    /// reaches. Its hands do not practise them, and one seen before the
    /// trial under way is neither drawn nor aimed at again, but for the
    /// rare chance ([`Again`]); it is done again when a deed it keeps,
    /// or one its trial is after, needs what it gives ([`wanted`]).
    pub seen: Vec<Deed>,
    /// Of them, and of the deeds it keeps, those that gave a thing in
    /// the trial of its hands under way: what the trial's end may keep.
    pub(super) trial_seen: Vec<Deed>,
    /// The thread of the trial under way (amended 28 November 2026,
    /// fourth round): each gesture that gave a thing, with the very
    /// things it was done on and with and the very thing that came, the
    /// oldest first. Dropped at the trial's end.
    pub(super) thread: Vec<Made>,
    /// The things that held its food while it carried more than its arms
    /// alone hold, in the trial under way: what really served.
    pub(super) served_things: Vec<ItemId>,
    /// The deeds a trial's end just left kept, and whether as its own
    /// kept trial (always so since 28 November 2026: one verdict): said
    /// in the news at its hands' next thought.
    pub(super) kept_news: Vec<(Deed, bool)>,
    /// The deed it set out to do for what it gives, and on what thing
    /// (D57, amended 28 November 2026): never a gesture drawn.
    pub(super) sought: Option<(Deed, Barred)>,
    /// The things on which a deed done for what it gives did not give it
    /// into its hands (D57, amended 28 November 2026: "told bark, here
    /// it gave no bark"), the oldest first, at most the content's
    /// `amiss_kept`: the deed is not done again on that thing while
    /// remembered; elsewhere it may be.
    pub(super) amiss: Vec<(Deed, Barred)>,
    /// The things it laid at its feet to free its hands and has not taken
    /// up again (D57, amended 28 November 2026), the oldest first, at
    /// most the content's `laid_kept`: its own memory of where it put
    /// them. Nothing makes it walk back to them from beyond its hands'
    /// sight.
    pub laid: Vec<Laid>,
    /// The gesture it is freeing its hands for, and where it stood.
    pub(super) clearing: Option<(HandTarget, Deed, Pos)>,
    /// The thing laid it set out to take up again at its last thought.
    pub(super) taking: Option<ItemId>,
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

    /// Whether it knows what `deed` gives from before the trial under
    /// way, and does not keep it (D57, amended 28 November 2026, third
    /// round: a known result is not tried again): it saw it give a thing
    /// that brought it nothing, or give nothing. A deed that gave a
    /// thing in the trial under way is not yet judged: that trial's end
    /// says whether it served.
    #[must_use]
    pub fn knows(&self, deed: Deed) -> bool {
        self.failed(deed) || (self.saw(deed) && !self.trial_seen.contains(&deed))
    }

    /// `deed` gave nothing or broke the thing: remembered, unless it
    /// remembers that it gives a thing (one result a deed: that one
    /// failed for the moment's sake). `most` is a guard, the oldest
    /// forgotten beyond it.
    fn fail(&mut self, deed: Deed, most: u32) {
        if self.failed.contains(&deed) || self.seen.contains(&deed) {
            return;
        }
        self.failed.push(deed);
        let most = usize::try_from(most).unwrap_or(usize::MAX);
        if self.failed.len() > most {
            let over = self.failed.len() - most;
            self.failed.drain(..over);
        }
    }

    /// Whether it remembers that `deed` gave a thing in its hands, and
    /// does not keep it.
    #[must_use]
    pub fn saw(&self, deed: Deed) -> bool {
        self.seen.contains(&deed)
    }

    /// `deed` gave a thing in a trial of its hands: remembered as seen
    /// (`most` is a guard, the oldest forgotten beyond it), no longer as
    /// a gesture that gives nothing, and noted of the trial under way.
    /// True when it is the first time in this trial.
    pub(super) fn see(&mut self, deed: Deed, most: u32) -> bool {
        self.failed.retain(|d| *d != deed);
        if !self.seen.contains(&deed) {
            self.seen.push(deed);
            let most = usize::try_from(most).unwrap_or(usize::MAX);
            if self.seen.len() > most {
                let over = self.seen.len() - most;
                self.seen.drain(..over);
            }
        }
        let first = !self.trial_seen.contains(&deed);
        if first {
            self.trial_seen.push(deed);
        }
        first
    }

    /// Whether it remembers that `deed`, done on `thing` for what it
    /// gives, did not give it into its hands.
    fn amiss(&self, deed: Deed, thing: Barred) -> bool {
        self.amiss.contains(&(deed, thing))
    }

    /// `deed` done on `thing` did not give what was told: remembered of
    /// that thing, the oldest forgotten beyond `most`.
    fn not_as_told(&mut self, deed: Deed, thing: Barred, most: u32) {
        if self.amiss(deed, thing) {
            return;
        }
        self.amiss.push((deed, thing));
        let most = usize::try_from(most).unwrap_or(usize::MAX);
        if self.amiss.len() > most {
            let over = self.amiss.len() - most;
            self.amiss.drain(..over);
        }
    }

    /// `deed`, just done on its last target, gave `gave` in the trial
    /// under way: a step of its thread. A put-in is one step a thing put
    /// into, the last; the oldest step goes beyond [`THREAD`].
    fn spin(&mut self, deed: Deed, gave: ItemId) {
        let used = match self.last {
            Some(HandTarget::Item(item, _) | HandTarget::Take { item }) => [Some(item), None],
            Some(HandTarget::Put { item, into }) => [Some(item), Some(into)],
            Some(HandTarget::Strike { item, with }) => [Some(item), Some(with)],
            Some(HandTarget::Tree { with, .. }) => [Some(with), None],
            None => [None, None],
        };
        if deed.gesture == Gesture::PutIn {
            self.thread
                .retain(|m| !(m.deed == deed && m.used[1] == used[1]));
        }
        self.thread.push(Made { deed, used, gave });
        if self.thread.len() > THREAD {
            let over = self.thread.len() - THREAD;
            self.thread.drain(..over);
        }
    }

    /// A test's gesture of the trial under way: `deed`, done on and with
    /// `used`, gave `gave`; seen.
    #[cfg(test)]
    pub(super) fn made(&mut self, deed: Deed, used: [Option<ItemId>; 2], gave: ItemId) {
        self.see(deed, u32::MAX);
        self.thread.push(Made { deed, used, gave });
    }

    /// A test's gestures of the trial under way, each done with or on
    /// the thing the one before gave, things numbered from `first`: a
    /// taking up, a cut with the thing taken, the fold of what the cut
    /// gave, a put-in into the thing folded. The last thing made.
    #[cfg(test)]
    pub(super) fn threaded(&mut self, deeds: impl IntoIterator<Item = Deed>, first: u64) -> ItemId {
        let mut last = ItemId(first);
        for (next, deed) in (first + 1..).zip(deeds) {
            let (used, gave) = match deed.gesture {
                Gesture::PickUp => ([None, None], ItemId(next)),
                Gesture::Cut | Gesture::Strike => ([Some(last), None], ItemId(next)),
                Gesture::Fold | Gesture::TakeOut => ([Some(last), None], last),
                Gesture::PutIn => ([Some(ItemId(next)), Some(last)], last),
            };
            self.made(deed, used, gave);
            last = gave;
        }
        last
    }

    /// It lays `item` at `at` to free its hands: remembered, the oldest
    /// forgotten beyond `most`.
    fn lay(&mut self, item: ItemId, at: Pos, most: u32) {
        self.laid.retain(|l| l.item != item);
        self.laid.push(Laid {
            item,
            at,
            left: false,
        });
        let most = usize::try_from(most).unwrap_or(usize::MAX).max(1);
        if self.laid.len() > most {
            let over = self.laid.len() - most;
            self.laid.drain(..over);
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
        self.trial_seen.clear();
        self.thread.clear();
        self.served_things.clear();
        self.sought = None;
        self.clearing = None;
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
    }
    note_heard(mind, deeds, day);
}

/// The heard `deeds` are noted as weighed by `mind` on `day`, tried or
/// set aside as known: the extra chance does not come back to them until
/// they are heard again.
pub(super) fn note_heard(mind: &mut Mind, deeds: &[(Deed, u32)], day: u64) {
    for &(deed, _) in deeds {
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

/// The rare chance that a body takes up again, on one journée, a deed
/// whose result it knows and that brought it nothing (D57, amended 28
/// November 2026, third round, the project lead: "one tries, and if it
/// does not work and brings nothing, one stops. One can take it up again
/// later"): the content's `again_permille`, drawn from the run's seed for
/// that body, that journée and that deed, so the same all that journée
/// whether the deed is heard at the fire or met in a draw.
#[derive(Debug, Clone, Copy)]
pub(super) struct Again {
    seed: u64,
    body: BodyId,
    day: u64,
    permille: u32,
}

impl Again {
    /// The chance of `body` on `day`.
    pub(super) fn of(rules: &Content, seed: u64, body: BodyId, day: u64) -> Self {
        Self {
            seed,
            body,
            day,
            permille: rules.needs.hands.again_permille,
        }
    }

    /// No chance: a deed known is set aside.
    #[cfg(test)]
    fn never() -> Self {
        Self {
            seed: 0,
            body: BodyId(0),
            day: 0,
            permille: 0,
        }
    }

    /// Whether it would take `deed` up again that journée.
    fn takes(self, deed: Deed) -> bool {
        self.permille > 0
            && scatter::roll(
                self.seed,
                purpose::HANDS_AGAIN,
                &[
                    u64::from(self.body.0),
                    self.day,
                    deed.gesture as u64,
                    u64::from(deed.on),
                    u64::from(deed.with),
                ],
            ) % 1000
                < u64::from(self.permille)
    }
}

/// The deeds heard, sifted by what the listener already knows of them
/// ([`sift`]).
#[derive(Debug, Default)]
pub(super) struct Sifted {
    /// Those it may aim at: the deeds whose result it does not know, and
    /// those of `again`.
    pub new: Vec<(Deed, u32)>,
    /// Of `new`, the deeds whose result it knows and takes up again by
    /// the rare chance.
    pub again: Vec<Deed>,
    /// The deeds set aside: it knows what they give, and that it brought
    /// it nothing.
    pub known: u32,
}

/// The heard `deeds` sifted (D57, amended 28 November 2026, third round:
/// a known result is not tried again): a deed whose result `mind` knows
/// ([`Handwork::knows`]) is no aim, but for the rare chance of `again`.
/// A tale left with nothing new opens no trial.
pub(super) fn sift(mind: &Mind, again: Again, deeds: &[(Deed, u32)]) -> Sifted {
    let mut out = Sifted::default();
    for &(deed, tellers) in deeds {
        if !mind.handwork.knows(deed) {
            out.new.push((deed, tellers));
        } else if again.takes(deed) {
            out.new.push((deed, tellers));
            out.again.push(deed);
        } else {
            out.known += 1;
        }
    }
    out
}

/// The news that `id` takes up again the deeds of `again`, heard told.
pub(super) fn again_news(id: BodyId, again: &[Deed], at: u64) -> impl Iterator<Item = News> + '_ {
    again.iter().map(move |&deed| News::TakenUpAgain {
        body: id,
        deed,
        heard: true,
        at,
    })
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
fn descendants(rules: &Content, deeds: &[Deed], material: u16) -> Vec<u16> {
    let mut out = vec![material];
    let mut i = 0;
    while i < out.len() && out.len() <= DEEDS {
        let m = out[i];
        for &deed in deeds {
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
fn used(rules: &Content, deeds: &[Deed]) -> Vec<u16> {
    let mut out = Vec::new();
    for &deed in deeds {
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

/// The kind of `thing`, as the hands name it: its material's index and
/// its form ([`HOLLOW`]).
fn kind_of(rules: &Content, thing: &crate::Item) -> Option<u16> {
    let m = material_index(rules, thing.material())?;
    Some(if thing.hollow_ml() > 0 { m | HOLLOW } else { m })
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
/// today, the nearest of each of `kinds` (a material, folded into a
/// hollow or not; the id breaking ties), looked for in one pass over the
/// things of those materials alone.
fn nearest_kinds(
    world: &World,
    rules: &Content,
    (here, near): (Pos, i32),
    mind: &Mind,
    kinds: &[u16],
) -> Vec<(u16, ItemId, Pos)> {
    let names: Vec<&str> = kinds
        .iter()
        .filter_map(|&k| Some(rules.materials.get(kind_material(k))?.id.as_str()))
        .collect();
    let mut best: Vec<(u16, (i64, ItemId), Pos)> = Vec::new();
    for (id, p, _) in world.things_near(here, near, |m| names.contains(&m)) {
        if mind.handwork.barred(Barred::Item(id)) {
            continue;
        }
        let Some(kind) = world.item(id).and_then(|t| kind_of(rules, t)) else {
            continue;
        };
        if !kinds.contains(&kind) {
            continue;
        }
        let key = (super::far(p, here), id);
        match best.iter_mut().find(|b| b.0 == kind) {
            Some(b) if key < b.1 => *b = (kind, key, p),
            Some(_) => {}
            None => best.push((kind, key, p)),
        }
    }
    best.into_iter().map(|(k, (_, id), p)| (k, id, p)).collect()
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
fn holding(
    world: &World,
    rules: &Content,
    body: &crate::Body,
    have: &[(ItemId, u16)],
) -> Vec<(ItemId, u16)> {
    body.inventory()
        .filter_map(|i| world.item(i))
        .filter(|i| {
            rules
                .material(i.material())
                .is_some_and(|m| m.food.is_some())
        })
        .filter_map(crate::Item::within)
        .filter_map(|hollow| have.iter().find(|(i, _)| *i == hollow).copied())
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
        let trying = mind.trial.is_some_and(|t| t.way == Way::Hands);
        for (thing, kind) in holding(world, rules, body, &have) {
            mind.handwork.serve(kind);
            // In a trial of its hands: this very thing served (amended
            // 28 November 2026, fourth round).
            if trying
                && !mind.handwork.served_things.contains(&thing)
                && mind.handwork.served_things.len() < THREAD
            {
                mind.handwork.served_things.push(thing);
            }
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
    (deeds, have): (&[Deed], &[(ItemId, u16)]),
    mind: &Mind,
) -> Option<(HandTarget, Deed)> {
    let body = world.body(id)?;
    let full = body.arms_food_g() >= body.arms_g(&rules.movement);
    for &deed in deeds.iter().filter(|d| d.gesture == Gesture::PutIn) {
        // Only heard, and not yet come to anything in this trial.
        let untried = !mind.ways.hands.contains(deed) && !mind.handwork.trial_seen.contains(&deed);
        if !loose_food(rules, deed.on) || (!full && !untried) {
            continue;
        }
        let Some(&(into, _)) = have.iter().find(|&&(i, k)| {
            k == deed.with
                && world.hollow_room_g(i, rules) > 0
                && !mind.handwork.amiss(deed, Barred::Item(i))
        }) else {
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
    (deeds, have): (&[Deed], &[(ItemId, u16)]),
    mind: &Mind,
) -> Option<(HandTarget, Deed)> {
    let body = world.body(id)?;
    if body.arms_food_g() < body.arms_g(&rules.movement) {
        return None;
    }
    for &deed in deeds.iter().filter(|d| d.gesture == Gesture::TakeOut) {
        let fills = deeds
            .iter()
            .any(|d| d.gesture == Gesture::PutIn && d.with == deed.with && loose_food(rules, d.on));
        if !fills || !(mind.ways.hands.contains(deed) || mind.handwork.saw(deed)) {
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
                && !mind.handwork.amiss(deed, Barred::Item(i))
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
/// drink, eat or sleep: let go what its hands do not keep, a deed it
/// keeps or its trial is after when it lacks what the deed gives
/// ([`wanted`]; taking up first the tool a cut names, when it carries
/// none), or, in a trial of its hands with no such deed to do here, a
/// gesture drawn (amended 25 November 2026: a trial begun on a tale
/// keeps a free hand).
///
/// A thing taken up or laid down takes a tick and tells its mind nothing
/// (amended 28 November 2026): its hands think again at the next tick,
/// where before they waited for whatever next woke the mind, and a body
/// that had laid its things down for a gesture would have stood over
/// them.
pub(super) fn hands_turn(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    mind: &mut Mind,
    (seed, news): (u64, &mut Vec<News>),
) -> HandsTurn {
    let turn = hands_think(world, rules, (id, here), mind, (seed, news));
    if let HandsTurn::Decide(Some(Decision {
        intention: Intention::PickUp { .. } | Intention::Drop { .. },
        ..
    })) = turn
    {
        let next = world.tick().0 + 1;
        mind.wake_at = Some(mind.wake_at.map_or(next, |w| w.min(next)));
    }
    turn
}

/// [`hands_turn`], but for the thought it asks for after a taking up or
/// a laying down.
fn hands_think(
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
    for (deed, own) in std::mem::take(&mut mind.handwork.kept_news) {
        news.push(News::DeedKept {
            body: id,
            deed,
            own,
            at: now,
        });
    }
    if let Some(tool) = resolve(world, rules, id, mind, news) {
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
    let near = i32::try_from(hands.near_tiles).unwrap_or(4);
    look_at_laid(world, (id, here), near, mind, news);
    if !trying && let Some(decision) = let_go(rules, id, mind, &have) {
        return HandsTurn::Decide(Some(decision));
    }
    // The gesture it is freeing its hands for (amended 28 November
    // 2026): the next thing in its way laid down, or the gesture itself.
    if let Some((target, deed, at)) = mind.handwork.clearing.take()
        && at == here
        && let Some(decision) = act_on(world, rules, (id, here), (target, deed), mind, news)
    {
        return HandsTurn::Decide(Some(decision));
    }
    if mind.doing == Doing::ToHands {
        if !mind.stopped {
            return HandsTurn::Decide(None);
        }
        mind.doing = Doing::Nothing;
        if let Some((target, deed)) = mind.handwork.to.take() {
            if let Some(decision) = act_on(world, rules, (id, here), (target, deed), mind, news) {
                return HandsTurn::Decide(Some(decision));
            }
            // Stopped short of it: left alone today.
            mind.handwork.bar(day, target.barred());
        }
    }
    // The deeds it keeps, and those its trial is after, the later steps
    // first, when it lacks what they give ([`wanted`]).
    let deeds = wanted(rules, mind, trying);
    let found = if deeds.is_empty() {
        None
    } else {
        let problem = trying && mind.trial.is_some_and(|t| t.problem.is_some());
        let ends = ends(rules, mind, (trying, problem));
        needed(
            world,
            rules,
            (id, here),
            (&deeds, &ends),
            (&have, near),
            mind,
        )
    };
    if let Some(((HandTarget::Item(item, _), _), true)) = found {
        mind.handwork.fetch = Some(item);
    }
    // A deed done for what it gives is remembered of the thing it is
    // done on, should that thing not give it (amended 28 November 2026).
    mind.handwork.sought = found
        .filter(|((_, deed), _)| deed.gesture != Gesture::PickUp)
        .map(|((target, deed), _)| (deed, target.thing()));
    // Its hands idle, it takes up again what it laid down for a gesture,
    // before it draws another: a draw is among all it carries.
    if found.is_none()
        && let Some(decision) = take_back(world, rules, (id, here), mind, (trying, near), news)
    {
        return HandsTurn::Decide(Some(decision));
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
        Some(todo) => HandsTurn::Decide(Some(go_do(world, rules, (id, here), todo, mind, news))),
        None => HandsTurn::Free,
    }
}

/// What its hands no longer know is let go, one thing a thought: a
/// trial given up leaves what it brought. Food, the things a deed it
/// keeps uses or gives, those of the deeds it remembers that feed them
/// ([`wanted`]), and the kinds that served it stay.
fn let_go(
    rules: &Content,
    id: BodyId,
    mind: &mut Mind,
    have: &[(ItemId, u16)],
) -> Option<Decision> {
    // The deeds it keeps, and those it remembers that feed them (amended
    // 28 November 2026, third round): the tool of a cut seen long ago
    // that a kept deed needs the product of is not let go, else it was
    // taken up for the cut and let go again without end.
    let known = wanted(rules, mind, false);
    let keep = used(rules, &known);
    let food = |m: u16| stuff(rules, m).is_some_and(|m| m.food.is_some());
    let served = &mind.handwork;
    let &(item, _) = have
        .iter()
        .find(|(_, m)| !food(*m) && !keep.contains(m) && !served.served(*m))?;
    mind.handwork.to = None;
    Some(Decision {
        intention: Intention::Drop { body: id, item },
        why: Why::Hands,
    })
}

/// Whether `a` lies within `near` tiles of `b` on each axis: the hands'
/// sight ([`things_near`]).
fn in_sight(a: Pos, b: Pos, near: i32) -> bool {
    (a.x - b.x).abs() <= near && (a.y - b.y).abs() <= near
}

/// What became of the things `id` laid at its feet (D57, amended 28
/// November 2026), looked at when its hands think and only while it
/// remembers some: taken up again; taken by another, eaten or gone,
/// and then forgotten; or left, the first time its hands think beyond
/// their sight of it. A thing left stays remembered: nothing makes it
/// walk back, but its hands take it up again if a later thought finds it
/// within their sight, still lying there.
fn look_at_laid(
    world: &World,
    (id, here): (BodyId, Pos),
    near: i32,
    mind: &mut Mind,
    news: &mut Vec<News>,
) {
    let hands = &mut mind.handwork;
    let taking = hands.taking.take();
    if hands.laid.is_empty() {
        return;
    }
    let at = world.tick().0;
    hands.laid.retain_mut(|laid| {
        let item = laid.item;
        match world.item(item).map(|i| i.place) {
            Some(crate::Place::Ground { pos }) if pos == laid.at => {
                if taking == Some(item) {
                    // It could not take it up after all: it lets it be.
                    news.push(News::LaidLetBe { body: id, item, at });
                    return false;
                }
                if !laid.left && !in_sight(laid.at, here, near) {
                    laid.left = true;
                    news.push(News::LaidLeft { body: id, item, at });
                }
                true
            }
            Some(crate::Place::Carried { body }) if body == id => {
                news.push(News::LaidBack {
                    body: id,
                    item,
                    late: laid.left,
                    at,
                });
                false
            }
            Some(crate::Place::Carried { body }) => {
                news.push(News::LaidLost {
                    body: id,
                    item,
                    by: Some(body),
                    at,
                });
                false
            }
            _ => {
                news.push(News::LaidLost {
                    body: id,
                    item,
                    by: None,
                    at,
                });
                false
            }
        }
    });
}

/// The thing `id` takes up again of those it laid at its feet (D57,
/// amended 28 November 2026, the project lead: "takes it back
/// afterwards, if he does not forget. And if there are several things he
/// wants to take, it is for him to choose which he prefers"), its hands
/// idle: among those within its hands' sight that it still wants (in a
/// trial of its hands, all; else food, a kind that served it, a thing a
/// deed it keeps uses or gives; the rest is let be), the one it prefers
/// that its arms and load can take, stepped to when not at its feet.
/// What it prefers, by what the code already knows of it: hungry, food
/// first (a hollow holding food is food); then a kind of thing that
/// served it; then a thing a deed it keeps uses or gives; then food;
/// then, in a trial, the rest; the first laid first among equals. When
/// nothing it wants fits any more, what is left within sight is let be.
fn take_back(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    mind: &mut Mind,
    (trying, near): (bool, i32),
    news: &mut Vec<News>,
) -> Option<Decision> {
    if mind.handwork.laid.is_empty() {
        return None;
    }
    let body = world.body(id)?;
    let now = world.tick().0;
    let kept: Vec<Deed> = mind.ways.hands.iter().collect();
    let keeps = used(rules, &kept);
    let hungry = body.condition().hunger.points() >= super::HUNGRY;
    let is_food = |i: &crate::Item| {
        rules
            .material(i.material())
            .is_some_and(|m| m.food.is_some())
    };
    let mut best: Option<((u8, usize), Laid)> = None;
    let mut let_be: Vec<ItemId> = Vec::new();
    let mut too_much: Vec<ItemId> = Vec::new();
    for (n, &laid) in mind.handwork.laid.iter().enumerate() {
        if !in_sight(laid.at, here, near) {
            continue;
        }
        let Some((thing, kind)) = world
            .item(laid.item)
            .and_then(|t| Some((t, kind_of(rules, t)?)))
        else {
            continue;
        };
        let inside: Vec<&crate::Item> = world
            .inside_on_ground(laid.item, laid.at)
            .into_iter()
            .filter_map(|i| world.item(i))
            .collect();
        let food = is_food(thing) || inside.iter().any(|i| is_food(i));
        let rank = if food && hungry {
            0
        } else if mind.handwork.served(kind) {
            1
        } else if keeps.contains(&kind) {
            2
        } else if food {
            3
        } else if trying {
            4
        } else {
            let_be.push(laid.item);
            continue;
        };
        let mass = inside
            .iter()
            .fold(thing.mass_g(), |sum, i| sum.saturating_add(i.mass_g()));
        let room = if is_food(thing) {
            body.food_room_g(&rules.movement)
        } else {
            body.capacity_g(&rules.movement)
                .saturating_sub(body.load_g())
        };
        if mass > room {
            too_much.push(laid.item);
            continue;
        }
        if best.is_none_or(|b| (rank, n) < b.0) {
            best = Some(((rank, n), laid));
        }
    }
    // What it does not want, and, when nothing it wants fits any more,
    // what it cannot carry with the rest: let be, as any thing lying.
    if best.is_none() {
        let_be.append(&mut too_much);
    }
    for &item in &let_be {
        news.push(News::LaidLetBe {
            body: id,
            item,
            at: now,
        });
    }
    mind.handwork.laid.retain(|l| !let_be.contains(&l.item));
    let (_, laid) = best?;
    mind.wake_at = Some(mind.wake_at.map_or(now + 1, |w| w.min(now + 1)));
    if (laid.at.x - here.x).abs() > 1 || (laid.at.y - here.y).abs() > 1 {
        mind.doing = Doing::ToHands;
        mind.handwork.to = None;
        return Some(Decision {
            intention: Intention::MoveTo {
                body: id,
                target: laid.at,
            },
            why: Why::Hands,
        });
    }
    mind.handwork.taking = Some(laid.item);
    Some(Decision {
        intention: Intention::PickUp {
            body: id,
            item: laid.item,
        },
        why: Why::Hands,
    })
}

/// A gesture drawn in a trial of the hands (D57): one of the four, on a
/// thing drawn among those it carries or that lie around it, at most the
/// content's tries a day. Only what a body sees makes a gesture plausible
/// ([`cuts_to_the_eye`], [`bends_to_the_eye`], [`holds_to_the_eye`];
/// amended 27 November 2026): the world may still answer nothing, or
/// break the thing. A gesture whose result it knows from before this
/// trial, nothing or a thing that brought it nothing, is set aside (the
/// news counts them; amended 28 November 2026, third round), but for the
/// rare chance to take it up again ([`Again`]). No suggestion of which
/// gesture.
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
    let again = Again::of(rules, seed, id, day);
    let plausible = Plausible::seen(world, rules, here, mind, (have, near), again);
    let open: Vec<Gesture> = Gesture::ALL
        .into_iter()
        .filter(|g| match g {
            Gesture::PickUp => !plausible.picks.is_empty(),
            Gesture::Cut => !plausible.cuts.is_empty(),
            Gesture::Fold => !plausible.folds.is_empty(),
            Gesture::PutIn => !plausible.puts.is_empty(),
            Gesture::TakeOut => !plausible.takes.is_empty(),
            Gesture::Strike => !plausible.strikes.is_empty(),
        })
        .collect();
    if plausible.skipped > 0 || plausible.known > 0 {
        news.push(News::Skipped {
            body: id,
            deeds: plausible.skipped,
            known: plausible.known,
            at: world.tick().0,
        });
    }
    let n = u64::try_from(open.len()).ok().filter(|&n| n > 0)?;
    let gesture = open[usize::try_from(r % n).unwrap_or(0)];
    let among = match gesture {
        Gesture::PickUp => &plausible.picks,
        Gesture::Cut => &plausible.cuts,
        Gesture::Fold => &plausible.folds,
        Gesture::PutIn => &plausible.puts,
        Gesture::TakeOut => &plausible.takes,
        Gesture::Strike => &plausible.strikes,
    };
    let drawn = among.get(pick(among.len())).copied()?;
    if plausible.again.contains(&drawn.1) {
        news.push(News::TakenUpAgain {
            body: id,
            deed: drawn.1,
            heard: false,
            at: world.tick().0,
        });
    }
    Some(drawn)
}

/// The gestures a body sees as plausible where it stands, those whose
/// result it knows set aside (D57, amended 27 and 28 November 2026): a
/// gesture it remembers gave nothing in its hands, or gave a thing that
/// brought it nothing, before the trial under way ([`Handwork::knows`]),
/// but for the rare chance to take it up again ([`Again`]). True of the
/// six gestures alike, the taking up of a thing among them.
#[derive(Default)]
struct Plausible {
    /// The taking up of each thing lying near.
    picks: Vec<(HandTarget, Deed)>,
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
    /// The different gestures set aside because it knows the thing they
    /// give, which brought it nothing.
    known: u32,
    /// The gestures of a known result left in by the rare chance.
    again: Vec<Deed>,
}

impl Plausible {
    fn seen(
        world: &World,
        rules: &Content,
        here: Pos,
        mind: &Mind,
        (have, near): (&[(ItemId, u16)], i32),
        again: Again,
    ) -> Self {
        let mut out = Self::default();
        let mut aside: Vec<Deed> = Vec::new();
        let mut back: Vec<Deed> = Vec::new();
        let mut remembered = |deed: Deed| {
            if !mind.handwork.knows(deed) {
                return false;
            }
            if again.takes(deed) {
                if !back.contains(&deed) {
                    back.push(deed);
                }
                return false;
            }
            if !aside.contains(&deed) {
                aside.push(deed);
            }
            true
        };
        out.picks = picks(world, rules, (here, near), mind, &mut remembered);
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
        out.takes = takes_out(world, have, &mut remembered);
        let failed = aside.iter().filter(|d| mind.handwork.failed(**d)).count();
        out.skipped = u32::try_from(failed).unwrap_or(u32::MAX);
        out.known = u32::try_from(aside.len() - failed).unwrap_or(u32::MAX);
        out.again = back;
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

/// The takings out a body sees as plausible (D57, amended 27 November
/// 2026): what lies in a thing it carries is seen there, and may be
/// taken out again; those `remembered` as a known result set aside.
fn takes_out(
    world: &World,
    have: &[(ItemId, u16)],
    remembered: &mut impl FnMut(Deed) -> bool,
) -> Vec<(HandTarget, Deed)> {
    let mut out = Vec::new();
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
            out.push((HandTarget::Take { item }, deed));
        }
    }
    out
}

/// The takings up a body sees as plausible (D57): of each thing lying
/// within its hands' sight, those `remembered` as a known result set
/// aside (amended 28 November 2026, third round).
fn picks(
    world: &World,
    rules: &Content,
    (here, near): (Pos, i32),
    mind: &Mind,
    remembered: &mut impl FnMut(Deed) -> bool,
) -> Vec<(HandTarget, Deed)> {
    let mut out = Vec::new();
    for (thing, pos, material) in things_near(world, here, near, mind) {
        let Some(on) = material_index(rules, material) else {
            continue;
        };
        let deed = Deed {
            gesture: Gesture::PickUp,
            on,
            with: NOTHING,
        };
        if !remembered(deed) {
            out.push((HandTarget::Item(thing, pos), deed));
        }
    }
    out
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
fn strike_again(
    world: &World,
    have: &[(ItemId, u16)],
    deed: Deed,
    mind: &Mind,
) -> Option<(HandTarget, Deed)> {
    let free = |i: ItemId| world.item(i).is_some_and(|t| t.within().is_none());
    let of = |kind: u16| have.iter().find(|&&(i, m)| m == kind && free(i));
    // The thing struck: one it does not remember gave nothing under this
    // blow (amended 28 November 2026).
    let struck = have
        .iter()
        .find(|&&(i, m)| m == deed.on && free(i) && !mind.handwork.amiss(deed, Barred::Item(i)));
    let (&(item, _), &(with, _)) = (struck?, of(deed.with)?);
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
/// ways, those whose result it knows set aside as the draw sets them
/// aside (amended 28 November 2026, third round); or, the link by what a
/// deed gives, a deed it remembers that answers a lived problem, or one
/// that feeds it, that it could do here ([`wanted`], [`needed`]). Nothing
/// is drawn, no try spent: a trial from a problem waits for such a
/// moment.
pub(super) fn something_new(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    mind: &Mind,
    again: Again,
) -> bool {
    let near = i32::try_from(rules.needs.hands.near_tiles).unwrap_or(4);
    let have = carried(world, rules, id);
    let new = |deed: Deed| !mind.ways.hands.contains(deed);
    let plausible = Plausible::seen(world, rules, here, mind, (&have, near), again);
    if [
        &plausible.picks,
        &plausible.folds,
        &plausible.puts,
        &plausible.cuts,
    ]
    .into_iter()
    .flatten()
    .any(|&(_, deed)| new(deed))
    {
        return true;
    }
    mind.handwork.seen.iter().any(|d| answers(rules, *d)) && {
        let deeds = wanted_in(rules, mind, (true, true));
        let ends = ends(rules, mind, (true, true));
        needed(
            world,
            rules,
            (id, here),
            (&deeds, &ends),
            (&have, near),
            mind,
        )
        .is_some()
    }
}

/// Whether what `deed` gives is what one of `wanted` is done on, with or
/// in: the flake a cut is made with, the bark a fold is made of, the
/// hollow a put-in fills; for a take-out, the room it makes in a kind of
/// hollow a put-in of food among them fills.
fn feeds(rules: &Content, wanted: &[Deed], deed: Deed) -> bool {
    if deed.gesture == Gesture::TakeOut {
        return !loose_food(rules, deed.on)
            && wanted.iter().any(|k| {
                k.gesture == Gesture::PutIn && k.with == deed.with && loose_food(rules, k.on)
            });
    }
    product(rules, deed).is_some_and(|p| {
        wanted.iter().any(|k| {
            k.with == p
                || (matches!(k.gesture, Gesture::Fold | Gesture::PutIn | Gesture::Strike)
                    && k.on == p)
        })
    })
}

/// Whether `deed` is one that answers a problem a body lives: today the
/// one such problem with an answer, the full arms, and so a put-in of
/// loose food in a thing (D50). The same deed [`keep_what_served`] keeps
/// a chain from.
fn answers(rules: &Content, deed: Deed) -> bool {
    deed.gesture == Gesture::PutIn && loose_food(rules, deed.on)
}

/// The deeds `mind` does again when it lacks what they give (D57,
/// amended 28 November 2026, the project lead: "a success they want to
/// keep, they keep and redo again and again... humans are lazy"):
///
/// - the deeds it keeps, always;
/// - in a trial of its hands (`trying`), the deeds it heard and set out
///   to try that have not yet given a thing in this trial, and the deeds
///   that gave one in this trial and answer a problem ([`answers`]): what
///   the trial is after;
/// - in a trial of its hands begun from a problem it lives, the deeds
///   it remembers having seen that answer a problem (amended 28 November
///   2026, third round, the link from its lasting memory: "fire needs
///   wood, and I know that cutting a tree gives wood");
/// - and, step by step back from those, the deeds it only saw or heard
///   whose product one of them is done on, with or in ([`feeds`]),
///   however long ago it saw them.
///
/// A deed only seen whose product serves none of them is not among them:
/// nobody works for a thing that leads nowhere (the run of seed 41: 8,239
/// cuts of 9,678 were a cut once seen done again for chips that fall to
/// the ground).
fn wanted(rules: &Content, mind: &Mind, trying: bool) -> Vec<Deed> {
    let problem = trying && mind.trial.is_some_and(|t| t.problem.is_some());
    wanted_in(rules, mind, (trying, problem))
}

/// [`wanted`], in a trial of its hands or not (`trying`), begun from a
/// lived problem or not (`problem`).
fn wanted_in(rules: &Content, mind: &Mind, (trying, problem): (bool, bool)) -> Vec<Deed> {
    let hands = &mind.handwork;
    let mut out: Vec<Deed> = mind.ways.hands.iter().collect();
    let mut left: Vec<Deed> = hands.seen.clone();
    if trying {
        for deed in hands.aim.iter() {
            if hands.trial_seen.contains(&deed) {
                left.push(deed);
            } else {
                out.push(deed);
            }
        }
        out.extend(hands.trial_seen.iter().filter(|d| answers(rules, **d)));
    }
    if problem {
        out.extend(hands.seen.iter().filter(|d| answers(rules, **d)));
    }
    left.retain(|d| !out.contains(d));
    out.sort_unstable();
    out.dedup();
    while let Some(i) = left.iter().position(|&d| feeds(rules, &out, d)) {
        let deed = left.remove(i);
        if !out.contains(&deed) {
            out.push(deed);
        }
    }
    out.sort_unstable();
    out
}

/// The deeds `mind` does for their own sake, where a chain ends (D57,
/// amended 28 November 2026, fourth round: the chain starts from a real
/// lack): of the deeds it keeps, those whose product no other deed it
/// keeps is done on, with or in (the put-in of food of a container's
/// chain; the fold, for one who keeps no put-in); in a trial of its
/// hands, the deeds heard and not yet tried in it, and those that answer
/// a problem, seen in this trial or, in a trial from a lived problem,
/// remembered. The others of [`wanted`] are only ways to what these
/// lack. A put-in first, then the fold, the cut, the blow, the taking
/// up.
fn ends(rules: &Content, mind: &Mind, (trying, problem): (bool, bool)) -> Vec<Deed> {
    let hands = &mind.handwork;
    let kept: Vec<Deed> = mind.ways.hands.iter().collect();
    let mut out: Vec<Deed> = kept
        .iter()
        .copied()
        .filter(|d| !feeds(rules, &kept, *d))
        .collect();
    if trying {
        out.extend(hands.aim.iter().filter(|d| !hands.trial_seen.contains(d)));
        out.extend(hands.trial_seen.iter().filter(|d| answers(rules, **d)));
    }
    if problem {
        out.extend(hands.seen.iter().filter(|d| answers(rules, **d)));
    }
    let rank = |d: &Deed| match d.gesture {
        Gesture::PutIn => 0,
        Gesture::Fold => 1,
        Gesture::Cut => 2,
        Gesture::Strike => 3,
        Gesture::PickUp => 4,
        Gesture::TakeOut => 5,
    };
    out.sort_unstable_by_key(|d| (rank(d), *d));
    out.dedup();
    out
}

/// What its hands do here for what they lack (D57, amended 28 November
/// 2026, fourth round, the project lead's "fire needs wood, cutting a
/// tree gives wood, I will cut"): a put-in or a take-out first
/// ([`put_again`], [`take_again`]); else, for each of the `ends` in turn,
/// the chain walked back **from the thing lacked** ([`Chain`]): is it in
/// its hands? does it lie within its hands' sight? else what gives it,
/// among the deeds of `deeds`, and does it have what that deed is done
/// on and with? The walk stops at the first thing held or seen, and
/// never comes twice to the same kind of thing. The gesture to make now,
/// marked `true` when it is the taking up of the tool a cut names (D57,
/// amended 25 November 2026).
///
/// Before, every deed of `deeds` was asked "do I lack what *you* give",
/// whatever the end of the chain held: person 69 of the run of seed 43,
/// its flint in hand, cut 2,094 branches in 47 journées for a blow it
/// had once seen give a flint.
fn needed(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    (deeds, ends): (&[Deed], &[Deed]),
    (have, near): (&[(ItemId, u16)], i32),
    mind: &Mind,
) -> Option<((HandTarget, Deed), bool)> {
    if let Some(again) = take_again(world, rules, id, (deeds, have), mind)
        .or_else(|| put_again(world, rules, id, (deeds, have), mind))
    {
        return Some((again, false));
    }
    let mut chain = Chain {
        world,
        rules,
        mind,
        id,
        here,
        near,
        deeds,
        have,
        held: world.body(id)?.held(),
        lying: None,
        visited: Vec::new(),
    };
    for &end in ends {
        chain.visited.clear();
        let step = match end.gesture {
            // A put-in of food needs the thing it puts in; what goes in
            // it is whatever loose food its arms hold.
            Gesture::PutIn if loose_food(rules, end.on) => chain.thing(end.with, false),
            // A put-in of another thing, heard and not yet tried: the
            // thing it puts in and the thing put, then the put-in.
            Gesture::PutIn if !mind.ways.hands.contains(end) => chain.deed(end),
            Gesture::PutIn | Gesture::TakeOut => Step::Cannot,
            Gesture::PickUp | Gesture::Cut | Gesture::Fold | Gesture::Strike => {
                let lacked = product(rules, end).is_some_and(|p| {
                    descendants(rules, deeds, p)
                        .iter()
                        .all(|m| !have.iter().any(|(_, h)| h == m))
                });
                if !lacked {
                    continue;
                }
                if mind.ways.hands.contains(end) {
                    // A deed kept: the thing it gives, by whichever way.
                    product(rules, end).map_or(Step::Cannot, |p| chain.thing(p, false))
                } else {
                    // A deed its trial is after: that very deed.
                    chain.deed(end)
                }
            }
        };
        if let Step::Do(todo, tool) = step {
            return Some((todo, tool));
        }
    }
    None
}

/// Where the walk back from a thing lacked stands ([`Chain`]).
enum Step {
    /// It holds the thing.
    Have,
    /// The gesture to make now; `true` for the taking up of a cut's tool.
    Do((HandTarget, Deed), bool),
    /// Nothing here gives it.
    Cannot,
}

/// The walk back from a thing lacked (D57, amended 28 November 2026,
/// fourth round): [`needed`].
struct Chain<'a> {
    world: &'a World,
    rules: &'a Content,
    mind: &'a Mind,
    id: BodyId,
    here: Pos,
    near: i32,
    /// The deeds it keeps, remembers or is after: what may give a thing.
    deeds: &'a [Deed],
    have: &'a [(ItemId, u16)],
    held: Option<ItemId>,
    /// The nearest thing of each kind a deed names lying within its
    /// hands' sight, looked for once, at the first thing not held.
    lying: Option<Vec<(u16, ItemId, Pos)>>,
    /// The kinds of thing the walk came to: never twice (a flint is not
    /// sought through a branch that takes a flint to cut).
    visited: Vec<u16>,
}

impl Chain<'_> {
    /// The nearest thing of `kind` lying within its hands' sight.
    fn lying(&mut self, kind: u16) -> Option<(ItemId, Pos)> {
        let (world, rules, mind) = (self.world, self.rules, self.mind);
        let (here, near, deeds) = (self.here, self.near, self.deeds);
        self.lying
            .get_or_insert_with(|| {
                let mut kinds: Vec<u16> = Vec::new();
                for d in deeds {
                    if d.gesture != Gesture::Cut {
                        kinds.push(d.on);
                    }
                    if d.with != NOTHING {
                        kinds.push(d.with);
                    }
                    kinds.extend(product(rules, *d));
                }
                kinds.sort_unstable();
                kinds.dedup();
                nearest_kinds(world, rules, (here, near), mind, &kinds)
            })
            .iter()
            .find(|n| n.0 == kind)
            .map(|&(_, item, pos)| (item, pos))
    }

    /// A thing of `kind`: held, or lying at hand and taken up, or given
    /// by a deed it can do or work towards here. `tool`: it is the tool
    /// of a cut.
    fn thing(&mut self, kind: u16, tool: bool) -> Step {
        if self.have.iter().any(|(_, k)| *k == kind) {
            return Step::Have;
        }
        if kind == NOTHING || self.visited.contains(&kind) {
            return Step::Cannot;
        }
        self.visited.push(kind);
        if let Some((item, pos)) = self.lying(kind) {
            let take = Deed {
                gesture: Gesture::PickUp,
                on: kind & !HOLLOW,
                with: NOTHING,
            };
            return Step::Do((HandTarget::Item(item, pos), take), tool);
        }
        let deeds = self.deeds;
        for &deed in deeds {
            if deed.gesture == Gesture::PickUp || product(self.rules, deed) != Some(kind) {
                continue;
            }
            if let Step::Do(todo, tool) = self.deed(deed) {
                return Step::Do(todo, tool);
            }
        }
        Step::Cannot
    }

    /// The deed `deed`: done here when it holds what it is done on and
    /// with; else the first of those it lacks, sought in turn.
    fn deed(&mut self, deed: Deed) -> Step {
        let (world, mind, have) = (self.world, self.mind, self.have);
        match deed.gesture {
            Gesture::TakeOut => Step::Cannot,
            Gesture::PutIn => {
                for kind in [deed.with, deed.on] {
                    match self.thing(kind, false) {
                        Step::Have => {}
                        step => return step,
                    }
                }
                let into = have.iter().find(|&&(i, k)| {
                    k == deed.with
                        && world.hollow_room_g(i, self.rules) > 0
                        && !mind.handwork.amiss(deed, Barred::Item(i))
                });
                let item = have.iter().find(|&&(i, k)| {
                    k == deed.on
                        && into.is_some_and(|&(o, _)| o != i)
                        && !mind.handwork.barred(Barred::Item(i))
                        && world.item(i).is_some_and(|t| t.within().is_none())
                });
                match (item, into) {
                    (Some(&(item, _)), Some(&(into, _))) => {
                        Step::Do((HandTarget::Put { item, into }, deed), false)
                    }
                    _ => Step::Cannot,
                }
            }
            Gesture::PickUp => {
                if self.visited.contains(&deed.on) {
                    return Step::Cannot;
                }
                self.visited.push(deed.on);
                self.lying(deed.on).map_or(Step::Cannot, |(item, pos)| {
                    Step::Do((HandTarget::Item(item, pos), deed), false)
                })
            }
            Gesture::Fold => match self.thing(deed.on, false) {
                Step::Have => have
                    .iter()
                    .find(|&&(i, m)| m == deed.on && !mind.handwork.amiss(deed, Barred::Item(i)))
                    .map_or(Step::Cannot, |&(item, _)| {
                        Step::Do((HandTarget::Item(item, self.here), deed), false)
                    }),
                step => step,
            },
            Gesture::Cut => match self.thing(deed.with, true) {
                Step::Have => {
                    // The tool in hand first.
                    let mut tools: Vec<ItemId> = have
                        .iter()
                        .filter(|(_, m)| *m == deed.with)
                        .map(|(i, _)| *i)
                        .collect();
                    tools.sort_by_key(|i| Some(*i) != self.held);
                    tools
                        .first()
                        .and_then(|&with| {
                            tree_to_cut(world, self.rules, (self.id, self.here), (deed, with), mind)
                        })
                        .map_or(Step::Cannot, |target| Step::Do((target, deed), false))
                }
                step => step,
            },
            Gesture::Strike => {
                for kind in [deed.on, deed.with] {
                    match self.thing(kind, false) {
                        Step::Have => {}
                        step => return step,
                    }
                }
                strike_again(world, have, deed, mind)
                    .map_or(Step::Cannot, |again| Step::Do(again, false))
            }
        }
    }
}

/// The nearest tree within its hands' sight that the cut `deed` is made
/// on with `with`: of its species, whose bark would go on the body, not
/// showing its wood bare to a cut for bark, and not one it remembers did
/// not give what the cut was done for (amended 28 November 2026).
fn tree_to_cut(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    (deed, with): (Deed, ItemId),
    mind: &Mind,
) -> Option<HandTarget> {
    let near = i32::try_from(rules.needs.hands.near_tiles).unwrap_or(4);
    let &(chunk, key, at, _) = trees_near(world, here, near, mind).iter().find(|t| {
        cut_species(deed.on) == t.3
            && bark_fits(world, rules, id, deed)
            && !shows_bare(world, rules, deed, (t.0, t.1))
            && !mind.handwork.amiss(deed, Barred::Tree(t.0, t.1))
    })?;
    Some(HandTarget::Tree {
        chunk,
        key,
        at,
        with,
        part: cut_part(deed.on),
    })
}

/// Does `deed` on `target` if within reach, else steps there.
fn go_do(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    (target, deed): (HandTarget, Deed),
    mind: &mut Mind,
    news: &mut Vec<News>,
) -> Decision {
    if let Some(decision) = act_on(world, rules, (id, here), (target, deed), mind, news) {
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

/// The gesture on `target`, when it is within reach. A gesture other
/// than a taking up needs its hands free (D57, amended 28 November 2026,
/// the project lead: "with full arms the hands are not free; one cannot
/// do an action. He lays the thing down, does his trial, and takes it
/// back afterwards"): while its arms hold anything but the gesture's own
/// things ([`World::in_the_way`]), it lays one such thing at its feet, a
/// thing a thought, remembered ([`Handwork::laid`]), and thinks again at
/// the next tick; then the gesture. A taking up is made with loaded
/// arms, as before.
fn act_on(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    (target, deed): (HandTarget, Deed),
    mind: &mut Mind,
    news: &mut Vec<News>,
) -> Option<Decision> {
    let body = world.body(id)?;
    let (intention, things, item) = match target {
        HandTarget::Item(item, _) if deed.gesture == Gesture::Fold => {
            (Intention::Fold { body: id, item }, vec![item], item)
        }
        HandTarget::Put { item, into } => (
            Intention::PutIn {
                body: id,
                item,
                into,
            },
            vec![item, into],
            item,
        ),
        HandTarget::Strike { item, with } => (
            Intention::Strike {
                body: id,
                item,
                with,
            },
            vec![item, with],
            item,
        ),
        HandTarget::Take { item } => (
            Intention::TakeOut { body: id, item },
            std::iter::once(item)
                .chain(world.item(item).and_then(crate::Item::within))
                .collect(),
            item,
        ),
        HandTarget::Item(item, pos) => {
            if (pos.x - here.x).abs() > 1 || (pos.y - here.y).abs() > 1 {
                return None;
            }
            begin(mind, deed, item);
            mind.handwork.last = Some(target);
            return Some(Decision {
                intention: Intention::PickUp { body: id, item },
                why: Why::Hands,
            });
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
            (
                Intention::Cut {
                    body: id,
                    with,
                    chunk,
                    tree: key,
                    part,
                },
                vec![with],
                with,
            )
        }
    };
    if let Some(rest) = world.in_the_way(id, &things) {
        let now = world.tick().0;
        mind.handwork.lay(rest, here, rules.needs.hands.laid_kept);
        mind.handwork.clearing = Some((target, deed, here));
        news.push(News::Laid {
            body: id,
            item: rest,
            at: now,
        });
        return Some(Decision {
            intention: Intention::Drop {
                body: id,
                item: rest,
            },
            why: Why::Hands,
        });
    }
    begin(mind, deed, item);
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

/// The judge of a trial (D50), the hands' rule added (D57, amended 28
/// November 2026, one verdict): a trial of the hands is kept if and only
/// if a chain of its deeds served its lived problem
/// ([`keep_what_served`]), whatever its days say; before, the days could
/// say no to a chain its hands kept all the same, and a success lowered
/// the will and was told at the fire as a failure.
pub(super) fn judge_trial(
    rules: &Content,
    per_year: u32,
    body: &crate::Body,
    mind: &mut Mind,
    trial: &super::Trial,
) -> (bool, super::Judged) {
    let (mut kept, mut judged) = super::judge(rules, per_year, body, mind, trial);
    if trial.way == Way::Hands {
        kept = keep_what_served(rules, mind);
        judged.kept = kept;
        judged.after.hands = mind.ways.hands;
        mind.handwork.drop_aim();
    }
    (kept, judged)
}

/// A trial not kept: its way goes back as before. A trial of the hands
/// has nothing to put back: its hands keep what they kept before, and
/// what it saw stays seen ([`keep_what_served`] said it kept no chain).
pub(super) fn leave_not_kept(mind: &mut Mind, trial: &super::Trial) {
    if trial.way != Way::Hands {
        trial.way.copy(&mut mind.ways, &trial.before);
    }
}

/// A trial of `mind` cut short, by a move followed or by another trial
/// begun (D50 and D57, amended 28 November 2026, one verdict): any other
/// way goes back as before, unjudged. A trial of the hands ends as every
/// trial of the hands does: kept if and only if a chain of it served its
/// lived problem, and then its will learns a success, the deeds are its
/// own and the tale told at the fire is a success, its news returned;
/// else unjudged, its will as it was, nothing told. What it was after is
/// dropped either way.
pub(super) fn cut_short(
    rules: &Content,
    (id, day, now): (BodyId, u64, u64),
    mind: &mut Mind,
    trial: &super::Trial,
) -> Option<News> {
    if trial.way != Way::Hands {
        leave_not_kept(mind, trial);
        return None;
    }
    let kept = keep_what_served(rules, mind);
    mind.handwork.drop_aim();
    if !kept {
        return None;
    }
    mind.judged = Some(super::Judged {
        way: Way::Hands,
        after: mind.ways,
        up: true,
        kept: true,
        before: trial.before_days,
        during: mind.lately(rules.needs.trials.trial_days),
        grades: None,
        problem: trial.problem.map(|a| a.kind),
    });
    mind.learn_will(rules, true);
    mind.changed_day = Some(day);
    Some(News::Tried {
        body: id,
        way: Way::Hands,
        kept: true,
        heard_days: trial.heard.days,
        heard_nights: trial.heard.nights,
        grades: None,
        at: now,
    })
}

/// Of the deeds that gave a thing in `mind`'s trial of the hands, those
/// that served become deeds its hands keep (D57, amended 27 and 28
/// November 2026; the run of step 1 left "cut an oak with a flint" known
/// to fifteen people, a cut whose bark only ever broke at the fold): a
/// put-in of food in a thing that served it (food lay in that very
/// thing while it carried more than its arms alone hold: the lived
/// problem relieved), and, step by step back along the trial's thread
/// ([`Handwork::thread`]), the gesture that gave that thing, those that
/// gave the things it was done on and with. By the things themselves,
/// not their kinds (fourth round of 28 November 2026: before, any deed
/// of the trial whose product was of a kind a kept deed uses was kept,
/// and "take wood chips out of the hollow" was kept by ten people of
/// the run of seed 43, "strike a quartz with a flint" by six). The rest
/// stays what it was, seen. The tool of a cut so kept served it (the project lead, 28
/// November 2026: a tool served inside such a chain, not for having
/// cut). Called once at the end of every trial of the hands, before what
/// it leaves is told. **The one verdict of a trial of the hands**
/// (amended 28 November 2026): true when a chain was so kept, and the
/// trial is then kept, the deeds its own ([`Handwork::kept`]); false,
/// and it is not. Before, two judges disagreed: "the limit moved while
/// the trial lasted" or "its days were better" said kept or not, while
/// this kept the chain at every ending; person 57 of the run of seed 42
/// filled a hollow beyond its arms 43 times and its trial was told as a
/// failure.
pub(super) fn keep_what_served(rules: &Content, mind: &mut Mind) -> bool {
    // Back along the thread, from the things that held its food beyond
    // its arms: the put-in of food into such a thing, the gesture that
    // gave that thing, the things that gesture was done on and with, the
    // gestures that gave those. A taking out gives back a thing put in:
    // it made nothing that served.
    let mut useful: Vec<ItemId> = mind.handwork.served_things.clone();
    let mut served: Vec<Deed> = Vec::new();
    for step in mind.handwork.thread.iter().rev() {
        let took_part = match step.deed.gesture {
            Gesture::TakeOut => false,
            Gesture::PutIn => {
                answers(rules, step.deed) && step.used[1].is_some_and(|into| useful.contains(&into))
            }
            Gesture::PickUp | Gesture::Cut | Gesture::Fold | Gesture::Strike => {
                useful.contains(&step.gave)
            }
        };
        if !took_part {
            continue;
        }
        if !served.contains(&step.deed) {
            served.push(step.deed);
        }
        for thing in step.used.into_iter().flatten() {
            if !useful.contains(&thing) {
                useful.push(thing);
            }
        }
    }
    let mut any = false;
    // From the answer back, as the chain was walked.
    for &deed in &served {
        if !mind.ways.hands.add(deed) {
            continue;
        }
        any = true;
        mind.handwork.seen.retain(|d| *d != deed);
        if deed.gesture == Gesture::Cut && deed.with != NOTHING {
            mind.handwork.serve(deed.with);
        }
        if !mind.handwork.kept.contains(&deed) {
            mind.handwork.kept.push(deed);
        }
        mind.handwork.kept_news.push((deed, true));
    }
    any
}

/// What came of the deed it was doing (D57). In a trial of its hands, a
/// deed that gave a thing is seen, not kept (amended 28 November 2026,
/// the project lead: "si c'est « j'ai eu de l'écorce » et que cette
/// écorce ne sert à rien, ça n'a aucun intérêt"): a memory, told in the
/// news the first time in the trial, its trial's days counted from today
/// so that the judge sees the days with the new thing. Practised in any
/// case. The tool's material, when it was a tool taken up for a cut.
fn resolve(
    world: &World,
    rules: &Content,
    id: BodyId,
    mind: &mut Mind,
    news: &mut Vec<News>,
) -> Option<u16> {
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
    // A deed done for what it gives, on a thing that did not give it
    // into its hands (nothing came, something else came, or it fell at
    // its feet), is remembered of that thing (amended 28 November 2026:
    // "told bark, here it gave no bark"), and not done again on it;
    // elsewhere it may be. True of the six gestures alike.
    if let Some((_, thing)) = mind.handwork.sought.take().filter(|(d, _)| *d == deed) {
        let came = mind.handwork.gave;
        let as_told = match product(rules, deed) {
            Some(told) => {
                worked
                    && came.is_some_and(|t| {
                        body.has(t) && world.item(t).and_then(|i| kind_of(rules, i)) == Some(told)
                    })
            }
            None => worked,
        };
        if !as_told {
            mind.handwork
                .not_as_told(deed, thing, rules.needs.hands.amiss_kept);
            news.push(News::NotAsTold {
                body: id,
                deed,
                at: world.tick().0,
            });
        }
    }
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
        // November 2026), unless it is a deed its hands keep, or one
        // that gave a thing in this trial: that one failed for the
        // moment's sake.
        if !mind.ways.hands.contains(deed) && !mind.handwork.trial_seen.contains(&deed) {
            mind.handwork.fail(deed, rules.needs.hands.failures_kept);
        }
        return None;
    }
    // In a trial of its hands, the thread: this gesture, the very things
    // it was done on and with, the very thing that came (amended 28
    // November 2026, fourth round): what the trial's end walks back.
    if mind.trial.is_some_and(|t| t.way == Way::Hands) {
        let gave = if deed.gesture == Gesture::PickUp {
            Some(item)
        } else {
            came
        };
        if let Some(gave) = gave {
            mind.handwork.spin(deed, gave);
        }
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
    match mind.handwork.practised.iter_mut().find(|(d, _)| *d == deed) {
        Some((_, n)) => *n += 1,
        None => mind.handwork.practised.push((deed, 1)),
    }
    if let Some(trial) = mind.trial.as_mut().filter(|t| t.way == Way::Hands)
        && !mind.ways.hands.contains(deed)
        && mind.handwork.see(deed, rules.needs.hands.seen_kept)
    {
        news.push(News::DeedSeen {
            body: id,
            deed,
            at: world.tick().0,
        });
        let day = today(world);
        trial.started_day = super::restarted_day(world, &rules.needs.trials, trial, day);
        // A trial from a problem that has just seen something new is
        // not judged before it has had the days to live its problem
        // again with it (amended 27 November 2026: in the run of seed 37
        // a hollow folded and filled on the last evening of its season
        // was given up that same night), but never later than its time
        // and those days once (amended 28 November 2026: each deed
        // pushed it again, and a trial of a season of 16 days lasted 33
        // in the run of seed 41).
        if let Some(aim) = trial.problem.as_mut() {
            let pushed = (day + u64::from(rules.needs.trials.trial_days)).min(aim.last);
            aim.until = aim.until.max(pushed);
        }
    }
    tool
}

#[cfg(test)]
mod tests {
    use super::super::{Act, Brains, HeardDays, Lately, News, Said, Trial, Ways, start_trial};
    use super::*;
    use crate::Event;
    use crate::memory::{Access, Record};
    use crate::time::Calendar;
    use std::collections::{BTreeMap, BTreeSet};

    /// The repository's content, a valley of minute ticks, a body beside
    /// a birch with a mind: the rules, world, brains, body, and the
    /// birch's species index.
    fn by_a_birch() -> (Content, World, Brains, BodyId, u16) {
        beside(|rules, species| rules.species[species].id == "birch", 0)
    }

    /// The same beside a tree of a species `of` takes, its trunk at least
    /// `thick` millimetres: with that tree's species index.
    fn beside(
        of: impl Fn(&Content, usize) -> bool,
        thick: u32,
    ) -> (Content, World, Brains, BodyId, u16) {
        let rules = crate::crops::tests::content();
        let mut world = World::with_valley(Calendar::new(1, 1440).unwrap(), &rules, 768);
        let mut birch = 0;
        let side = i32::try_from(world.map().chunks_wide()).unwrap();
        let mut stand = None;
        'search: for y in 0..side {
            for x in 0..side {
                for tree in world.trees_in(ChunkPos::new(x, y)).iter() {
                    if !of(&rules, tree.species)
                        || world.trunk_mm(tree, &rules).is_some_and(|mm| mm < thick)
                    {
                        continue;
                    }
                    birch = tree.species;
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
        let id = world.spawn_body(stand.expect("such a tree with room beside it"));
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
        // The bark carried: it folds it, its flint laid at its feet for
        // the fold (free hands, 28 November 2026).
        let HandsTurn::Decide(Some(decision)) = thinks(&mut world, &rules, &mut brains, id) else {
            panic!("a deed");
        };
        let Intention::Fold { item, .. } = decision.intention else {
            panic!("{decision:?}");
        };
        assert_eq!(world.item(item).unwrap().material(), "birch_bark");
        assert!(!world.body(id).unwrap().has(flint), "laid for the fold");
        events.clear();
        world.queue(decision.intention);
        world.advance(12, &rules, &mut events);
        brains.notice(&events, &world, &rules);
        // A container carried: nothing more to do with its hands, once
        // its flint is taken up again. It lacks nothing that a flint
        // would give (fourth round of 28 November 2026: the chain starts
        // from a real lack), so it is no deed done again: it takes back
        // what it laid down, the tool of a deed it keeps.
        assert_eq!(thinks(&mut world, &rules, &mut brains, id), HandsTurn::Free);
        assert!(world.body(id).unwrap().has(flint), "taken up again");
        assert_eq!(brains.minds[&id].handwork.laid, []);
        let mind = brains.minds.get_mut(&id).unwrap();
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
        // Bounded: a cut refused for ever must fail the test, not fill
        // the memory with its refusals.
        for cuts in 0.. {
            if world.bark_left(chunk, tree, &rules) == Some(0) {
                break;
            }
            assert!(cuts < 64, "the trunk never bare");
            events.clear();
            world.lay_all_but(other, &[its], &rules);
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
            world.lay_all_but(id, &[with], &rules);
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
    fn a_flint_that_cut_is_let_go_when_no_chain_of_its_cut_was_kept() {
        // Amended 28 November 2026: a tool served inside a chain that
        // relieved a lived problem, not for having cut (before, any cut
        // that gave a thing marked it, and it was never let go: twelve
        // flints on one person in the run of seed 41).
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
        assert!(!mind.handwork.served(material(&rules, "flint")));
        assert!(mind.ways.hands.is_empty() && mind.handwork.seen.is_empty());
        assert!(
            matches!(
                turn,
                HandsTurn::Decide(Some(Decision {
                    intention: Intention::Drop { item, .. },
                    ..
                })) if item == flint
            ),
            "a flint that cut and served nothing is let go: {turn:?}"
        );
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
        let decision = go_do(
            &world,
            &rules,
            (id, here),
            (target, deed),
            mind,
            &mut Vec::new(),
        );
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
        // The rare chance to take it up again left out of this test.
        let mut rules = rules;
        rules.needs.hands.again_permille = 0;
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
    fn the_memory_of_results_lasts_one_entry_a_deed() {
        // The project lead, 28 November 2026: "it is in my memory a year
        // later". Before, what gave a thing was a list of 32 and what
        // gave nothing a list of 16, the oldest pushed out.
        let deed = |on| Deed {
            gesture: Gesture::Fold,
            on,
            with: NOTHING,
        };
        let hands_of = Content::sample().needs.hands;
        let (seen_most, failed_most) = (hands_of.seen_kept, hands_of.failures_kept);
        assert_eq!((seen_most, failed_most), (1024, 1024), "guards, far off");
        let mut hands = Handwork::default();
        // Two hundred results of each kind, each lived twice: all are
        // still there, the first as the last, one entry each.
        for on in 0..200 {
            for _ in 0..2 {
                hands.see(deed(on), seen_most);
                hands.fail(deed(1000 + on), failed_most);
            }
        }
        hands.trial_seen.clear();
        assert_eq!((hands.seen.len(), hands.failed.len()), (200, 200));
        assert!(hands.saw(deed(0)) && hands.failed(deed(1000)), "the oldest");
        assert!(hands.knows(deed(0)) && hands.knows(deed(1000)));
        assert!(!hands.knows(deed(5000)), "never done");
        // Six bytes a deed: 2,400 bytes for these four hundred.
        assert_eq!(std::mem::size_of::<Deed>(), 6);
        // A result known is a result known, one a deed: "gives nothing"
        // and "gives a thing that brought me nothing" stay apart. A deed
        // seen to give a thing that fails one day is not forgotten as
        // giving it; a gesture that gave nothing and gives a thing at
        // last is known by the thing.
        hands.fail(deed(0), failed_most);
        assert!(hands.saw(deed(0)) && !hands.failed(deed(0)));
        hands.see(deed(1000), seen_most);
        assert!(hands.saw(deed(1000)) && !hands.failed(deed(1000)));
        assert_eq!((hands.seen.len(), hands.failed.len()), (201, 199));
        // What gave a thing in the trial under way is not yet a result
        // that brought nothing: its trial has not ended.
        assert!(!hands.knows(deed(1000)), "seen in this trial");
        hands.trial_seen.clear();
        assert!(hands.knows(deed(1000)));
        // The guards still bound both lists, the oldest going; none
        // kept, none remembered.
        let mut small = Handwork::default();
        for on in 0..3 {
            small.fail(deed(on), 2);
            small.see(deed(100 + on), 2);
        }
        assert_eq!(small.failed, [deed(1), deed(2)]);
        assert_eq!(small.seen, [deed(101), deed(102)]);
        small.fail(deed(7), 0);
        assert_eq!(small.failed, []);
    }

    /// A deed told as a novelty by `teller`: its record.
    fn told(brains: &mut Brains, rules: &Content, teller: u32, deed: Deed) -> Access {
        let (record, _) = brains.memory.intern(Record::Deed {
            body: BodyId(teller),
            day: 1,
            deed,
            made: material(rules, "birch_bark"),
            will: 50,
        });
        Access {
            record,
            by: BodyId(teller),
            day: 2,
        }
    }

    #[test]
    fn a_deed_whose_result_it_knows_is_set_aside_by_the_draw_and_is_no_aim_of_a_tale() {
        // The project lead, 28 November 2026: "one tries, and if it does
        // not work and brings nothing, one stops". In the run of seed 42,
        // 875 of 996 trunk cuts were made in 424 trials begun on a tale
        // by people who lived no problem: a cut already seen was aimed at
        // again each time it was told again, and drawn again in each
        // trial.
        let (mut rules, mut world, mut brains, id, birch) = by_a_birch();
        rules.needs.hands.again_permille = 0;
        let here = world.body(id).unwrap().pos();
        let flint = picked(&mut world, &rules, id, "flint");
        let flint_m = material(&rules, "flint");
        let stone = laid(&mut world, &rules, id, "quartz");
        let cut = Deed {
            gesture: Gesture::Cut,
            on: birch,
            with: flint_m,
        };
        let branch = Deed {
            on: birch | BRANCH,
            ..cut
        };
        let take = Deed {
            gesture: Gesture::PickUp,
            on: material(&rules, "quartz"),
            with: NOTHING,
        };
        let day = today(&world);
        let have = [(flint, flint_m)];
        let most = rules.needs.hands.seen_kept;
        // In an earlier trial it saw that the cut gives bark and that
        // the stone can be taken up, which brought it nothing, and that
        // the cut of a branch gave nothing.
        let mind = brains.minds.get_mut(&id).unwrap();
        in_a_trial(mind, &[], day);
        let all = Plausible::seen(&world, &rules, here, mind, (&have, 4), Again::never());
        assert!(all.cuts.iter().any(|(_, d)| *d == cut));
        assert!(
            all.picks
                .iter()
                .any(|(t, d)| *d == take && t.barred() == Barred::Item(stone))
        );
        mind.handwork.see(cut, most);
        mind.handwork.see(take, most);
        mind.handwork.fail(branch, rules.needs.hands.failures_kept);
        // Still that trial: what it has just seen is not yet judged, and
        // may be drawn again.
        let now = Plausible::seen(&world, &rules, here, mind, (&have, 4), Again::never());
        assert!(now.cuts.iter().any(|(_, d)| *d == cut) && now.known == 0);
        assert!(!keep_what_served(&rules, mind) && mind.handwork.saw(cut));
        mind.handwork.drop_aim();
        mind.trial = None;
        // A later trial: none of the three is drawn, true of the taking
        // up as of the cuts, and the draw says what it set aside.
        in_a_trial(mind, &[], day);
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
            assert!(
                drawn.is_none_or(|(_, d)| ![cut, branch, take].contains(&d)),
                "{drawn:?}"
            );
        }
        assert!(
            news.len() == 200
                && news.iter().all(|n| matches!(
                    n,
                    News::Skipped { body, deeds: 1, known: 2, .. } if *body == id
                )),
            "{:?}",
            news.first()
        );
    }

    #[test]
    fn a_tale_whose_every_deed_it_knows_opens_no_trial_one_with_something_new_does() {
        // "A deed heard is tried more readily" (25 November 2026) stays
        // true of a deed whose result the listener does not know.
        let (mut rules, world, mut brains, id, birch) = by_a_birch();
        rules.needs.hands.again_permille = 0;
        let cut = Deed {
            gesture: Gesture::Cut,
            on: birch,
            with: material(&rules, "flint"),
        };
        let fold = Deed {
            gesture: Gesture::Fold,
            on: material(&rules, "birch_bark"),
            with: NOTHING,
        };
        let take = Deed {
            gesture: Gesture::PickUp,
            on: material(&rules, "quartz"),
            with: NOTHING,
        };
        // The cut told at the fire again, which it saw give bark in an
        // earlier trial, as it saw a stone taken up: every deed of the
        // tale is known to it, no trial is begun, and the tale is
        // weighed until told again.
        let first = told(&mut brains, &rules, 101, cut);
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.seen.extend([cut, take]);
        mind.heard_ways.push(first);
        let heard = heard_deeds(&brains.memory, mind);
        assert_eq!(heard, [(cut, 1)]);
        let mut news = Vec::new();
        super::super::weigh_tale(&world, &rules, 7, (id, 3, 0), mind, (&heard, &mut news));
        assert!(
            matches!(news[..], [News::TaleKnown { body, deeds: 1, .. }] if body == id),
            "{news:?}"
        );
        assert!(mind.trial.is_none() && mind.handwork.aim.is_empty());
        assert_eq!(heard_deeds(&brains.memory, mind), Vec::new());
        // Told again with something new to it, the fold: a trial, aimed
        // at the fold alone.
        let mut again = told(&mut brains, &rules, 101, cut);
        again.day = 4;
        let mut new = told(&mut brains, &rules, 102, fold);
        new.day = 4;
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.heard_ways.extend([again, new]);
        let heard = heard_deeds(&brains.memory, mind);
        assert_eq!(heard.len(), 2);
        let mut news = Vec::new();
        super::super::weigh_tale(&world, &rules, 7, (id, 5, 0), mind, (&heard, &mut news));
        assert!(
            matches!(
                news[..],
                [News::Trying {
                    way: Way::Hands,
                    heard: true,
                    ..
                }]
            ),
            "{news:?}"
        );
        assert!(mind.handwork.aim.contains(fold) && !mind.handwork.aim.contains(cut));
        // The link: the fold it is after needs bark, and it remembers
        // that the cut gives bark. The stone feeds nothing: left alone.
        let deeds = wanted(&rules, mind, true);
        assert!(deeds.contains(&cut) && deeds.contains(&fold) && !deeds.contains(&take));
    }

    #[test]
    fn a_deed_known_is_taken_up_again_by_a_rare_chance_drawn_from_the_seed() {
        // "One can take it up again later": the content's
        // `again_permille`, 10: one journée in a hundred for a person and
        // a deed.
        let (mut rules, mut world, mut brains, id, birch) = by_a_birch();
        assert_eq!(rules.needs.hands.again_permille, 10);
        let here = world.body(id).unwrap().pos();
        let flint = picked(&mut world, &rules, id, "flint");
        let flint_m = material(&rules, "flint");
        let cut = Deed {
            gesture: Gesture::Cut,
            on: birch,
            with: flint_m,
        };
        // Over 10,000 journées: about a hundred, the same at each
        // asking, others for another seed, another body or another deed.
        let days = |seed: u64, body: BodyId, deed: Deed| -> Vec<u64> {
            (0..10_000)
                .filter(|&day| Again::of(&rules, seed, body, day).takes(deed))
                .collect()
        };
        let open_days = days(7, id, cut);
        assert!((60..=140).contains(&open_days.len()), "{}", open_days.len());
        assert_eq!(open_days, days(7, id, cut), "from the seed alone");
        assert_ne!(open_days, days(8, id, cut));
        assert_ne!(open_days, days(7, BodyId(id.0 + 1), cut));
        let branch = Deed {
            on: birch | BRANCH,
            ..cut
        };
        assert_ne!(open_days, days(7, id, branch));
        // Heard on such a journée, a known deed is an aim again, and the
        // news says so; on another it is not.
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.seen.push(cut);
        let open = open_days[0];
        let shut = (0..10_000u64).find(|d| !open_days.contains(d)).unwrap();
        let sifted = sift(mind, Again::of(&rules, 7, id, open), &[(cut, 1)]);
        assert_eq!(
            (sifted.new, sifted.again, sifted.known),
            (vec![(cut, 1)], vec![cut], 0)
        );
        let sifted = sift(mind, Again::of(&rules, 7, id, shut), &[(cut, 1)]);
        assert_eq!(
            (sifted.new, sifted.again, sifted.known),
            (vec![], vec![], 1)
        );
        let mut news = Vec::new();
        super::super::weigh_tale(
            &world,
            &rules,
            7,
            (id, open, 0),
            mind,
            (&[(cut, 1)], &mut news),
        );
        assert!(
            matches!(
                news[..],
                [
                    News::Trying { heard: true, .. },
                    News::TakenUpAgain {
                        deed,
                        heard: true,
                        ..
                    }
                ] if deed == cut
            ),
            "{news:?}"
        );
        assert!(mind.handwork.aim.contains(cut));
        mind.handwork.drop_aim();
        mind.trial = None;
        // Drawn in a trial: with the chance at one in two for the test,
        // the cut is drawn for some seeds, each time said in the news,
        // and never on a seed whose chance says no.
        rules.needs.hands.again_permille = 500;
        let day = today(&world);
        let have = [(flint, flint_m)];
        in_a_trial(mind, &[], day);
        let mut taken = 0;
        for seed in 0..400 {
            mind.handwork.tries = (day, 0);
            let mut news = Vec::new();
            let drawn = draw(
                &world,
                &rules,
                (id, here),
                mind,
                (seed, &have, 4),
                &mut news,
            );
            if drawn.is_some_and(|(_, d)| d == cut) {
                taken += 1;
                assert!(Again::of(&rules, seed, id, day).takes(cut));
                assert!(
                    news.iter().any(|n| matches!(
                        n,
                        News::TakenUpAgain { deed, heard: false, .. } if *deed == cut
                    )),
                    "{news:?}"
                );
            } else {
                assert!(!news.iter().any(|n| matches!(n, News::TakenUpAgain { .. })));
            }
        }
        assert!(taken > 0, "taken up again at times");
        // Never, when the content says never.
        rules.needs.hands.again_permille = 0;
        assert!((0..10_000).all(|day| !Again::of(&rules, 7, id, day).takes(cut)));
    }

    #[test]
    fn a_cut_seen_long_ago_is_made_again_when_a_deed_it_keeps_needs_what_it_gives() {
        // The project lead, 28 November 2026: "I cut wood, for now it is
        // useless, I stop. Three weeks later, a year later, fire is
        // found, and fire needs wood, and I know that cutting a tree
        // gives wood: it is in my memory."
        let (rules, mut world, mut brains, id, birch) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let flint = picked(&mut world, &rules, id, "flint");
        let cut = Deed {
            gesture: Gesture::Cut,
            on: birch,
            with: material(&rules, "flint"),
        };
        let fold = Deed {
            gesture: Gesture::Fold,
            on: material(&rules, "birch_bark"),
            with: NOTHING,
        };
        // Long ago, in a trial, it saw that this cut gives bark. The
        // bark brought it nothing; it has seen a hundred other things
        // since (the memory held 32 before, the oldest pushed out).
        let most = rules.needs.hands.seen_kept;
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.see(cut, most);
        for on in 0..100 {
            let other = Deed {
                gesture: Gesture::TakeOut,
                on: 1000 + on,
                with: NOTHING,
            };
            mind.handwork.see(other, most);
        }
        mind.handwork.drop_aim();
        assert!(mind.handwork.saw(cut) && mind.handwork.seen.len() == 101);
        // No trial, no tale, nothing it keeps: beside the birch, its
        // flint in hand, there is nothing its hands would do. Nobody
        // works for a thing that leads nowhere.
        assert!(mind.trial.is_none() && mind.heard_ways.is_empty());
        assert_eq!(wanted(&rules, mind, false), []);
        // Much later a deed it keeps needs bark: the fold. It remembers
        // what gives bark, and goes and cuts, with no tale and no trial;
        // and it does not let go of the flint that cut is made with.
        assert!(mind.ways.hands.add(fold));
        assert_eq!(wanted(&rules, mind, false), [cut, fold]);
        assert!(mind.handwork.aim.is_empty());
        let HandsTurn::Decide(Some(decision)) = thinks(&mut world, &rules, &mut brains, id) else {
            panic!("the cut again");
        };
        assert!(
            matches!(decision.intention, Intention::Cut { with, .. } if with == flint),
            "{decision:?}"
        );
        done(&mut world, &rules, &mut brains, id, &decision);
        // The bark in hand, the deed it keeps is done on it.
        let HandsTurn::Decide(Some(decision)) = thinks(&mut world, &rules, &mut brains, id) else {
            panic!("the fold");
        };
        assert!(
            matches!(decision.intention, Intention::Fold { .. }),
            "{decision:?}"
        );
        let mind = &brains.minds[&id];
        assert!(mind.trial.is_none() && mind.handwork.saw(cut));
        assert!(!mind.ways.hands.contains(cut), "seen, not kept");
        let _ = here;
    }

    #[test]
    fn a_detour_that_comes_back_to_the_thing_sought_is_not_walked() {
        // Person 69 of the run of seed 43 kept its bark hollow and had
        // seen once that a green branch struck on a flint breaks a piece
        // of flint off. "A flint serves my cut; a blow with a branch
        // gives one; a cut gives a branch; I have no branch": its flint
        // in hand it cut 2,094 branches in 47 journées, each let go at
        // once for being of no deed it kept.
        let (rules, mut world, mut brains, id, birch) = by_a_birch();
        let flint_m = material(&rules, "flint");
        let cut_branch = Deed {
            gesture: Gesture::Cut,
            on: birch | BRANCH,
            with: flint_m,
        };
        let branch_m = product(&rules, cut_branch).expect("a flint cuts a branch off");
        let strike = [(flint_m, branch_m), (branch_m, flint_m)]
            .into_iter()
            .map(|(on, with)| Deed {
                gesture: Gesture::Strike,
                on,
                with,
            })
            .find(|d| product(&rules, *d) == Some(flint_m))
            .expect("a blow of branch and flint breaks flint off");
        let kept = bark_deeds(&rules, birch);
        let cut = kept.iter().find(|d| d.gesture == Gesture::Cut).unwrap();
        let most = rules.needs.hands.seen_kept;
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.ways.hands = kept;
        mind.handwork.see(strike, most);
        mind.handwork.see(cut_branch, most);
        mind.handwork.drop_aim();
        // Both feed, by their kinds, the cut it keeps.
        let deeds = wanted(&rules, mind, false);
        assert!(deeds.contains(&strike) && deeds.contains(&cut_branch));
        // Neither flint nor hollow, and no flint in sight: a flint is
        // sought by the blow, the blow wants a branch, the branch wants
        // a flint to be cut: the walk has come to the flint already, and
        // stops. Its hands do nothing.
        assert_eq!(thinks(&mut world, &rules, &mut brains, id), HandsTurn::Free);
        // A flint in hand, no hollow: it lacks a hollow, and cuts the
        // bark for it. Never a branch.
        let flint = picked(&mut world, &rules, id, "flint");
        let HandsTurn::Decide(Some(decision)) = thinks(&mut world, &rules, &mut brains, id) else {
            panic!("the cut for bark");
        };
        assert!(
            matches!(decision.intention, Intention::Cut { with, part: Part::Trunk, .. } if with == flint),
            "{decision:?}"
        );
        assert_eq!(brains.minds[&id].handwork.sought.map(|s| s.0), Some(cut));
        // The hollow in hand as well: it lacks nothing. Before, its
        // hands went and cut a branch here, at every thought.
        brains.minds.get_mut(&id).unwrap().handwork.drop_aim();
        with_a_hollow(&mut world, &rules, id);
        for _ in 0..3 {
            assert_eq!(thinks(&mut world, &rules, &mut brains, id), HandsTurn::Free);
        }
        assert!(world.body(id).unwrap().has(flint), "its flint kept in hand");
    }

    #[test]
    fn a_body_living_its_problem_remakes_from_memory_the_chain_whose_end_answers_it() {
        // The same link, from a lived problem: it once saw food hold in
        // a hollow, the fold that gives the hollow and the cut that gives
        // the bark, at a time its arms were not full, so nothing was
        // kept. Its arms full one day, its trial does not wait for a
        // draw: it remembers.
        let (rules, mut world, mut brains, id, birch) = by_a_birch();
        let _flint = picked(&mut world, &rules, id, "flint");
        let bark = material(&rules, "birch_bark");
        let cut = Deed {
            gesture: Gesture::Cut,
            on: birch,
            with: material(&rules, "flint"),
        };
        let fold = Deed {
            gesture: Gesture::Fold,
            on: bark,
            with: NOTHING,
        };
        let put = Deed {
            gesture: Gesture::PutIn,
            on: material(&rules, "berries"),
            with: bark | HOLLOW,
        };
        assert!(answers(&rules, put));
        let most = rules.needs.hands.seen_kept;
        let day = today(&world);
        let mind = brains.minds.get_mut(&id).unwrap();
        for deed in [cut, fold, put] {
            mind.handwork.see(deed, most);
        }
        mind.handwork.drop_aim();
        // A trial on no problem: the three are known, none is done.
        in_a_trial(mind, &[], day);
        assert_eq!(again(&mut world, &rules, &mut brains, id), HandsTurn::Free);
        // A trial from its full arms: the chain is what it is after.
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.drop_aim();
        in_a_trial_of_full_arms(&rules, mind, day, 16);
        let deeds = wanted(&rules, mind, true);
        assert!(
            [cut, fold, put].iter().all(|d| deeds.contains(d)),
            "{deeds:?}"
        );
        let turn = again(&mut world, &rules, &mut brains, id);
        assert!(
            matches!(
                turn,
                HandsTurn::Decide(Some(Decision {
                    intention: Intention::Cut { .. },
                    ..
                }))
            ),
            "{turn:?}"
        );
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

    /// While `turn` lays a thing at its feet to free its hands for a
    /// gesture, or takes up again a thing so laid, does it (a tick) and
    /// lets its hands think again: the first other thing its hands
    /// decide.
    fn settled(
        world: &mut World,
        rules: &Content,
        brains: &mut Brains,
        id: BodyId,
        mut turn: HandsTurn,
    ) -> HandsTurn {
        for _ in 0..256 {
            let hands = &brains.minds[&id].handwork;
            let HandsTurn::Decide(Some(decision)) = &turn else {
                return turn;
            };
            let frees = match decision.intention {
                Intention::Drop { .. } => hands.clearing.is_some(),
                Intention::PickUp { .. } => hands.taking.is_some(),
                _ => false,
            };
            if !frees {
                return turn;
            }
            let mut events = Vec::new();
            world.queue(decision.intention.clone());
            world.advance(1, rules, &mut events);
            brains.notice(&events, world, rules);
            let here = world.body(id).unwrap().pos();
            let mind = brains.minds.get_mut(&id).unwrap();
            turn = hands_turn(world, rules, (id, here), mind, (7, &mut Vec::new()));
        }
        panic!("its hands never done laying down and taking up");
    }

    /// Its hands' thought, the layings down and takings up it asks for
    /// done ([`settled`]).
    fn thinks(world: &mut World, rules: &Content, brains: &mut Brains, id: BodyId) -> HandsTurn {
        let here = world.body(id).unwrap().pos();
        let mind = brains.minds.get_mut(&id).unwrap();
        let turn = hands_turn(world, rules, (id, here), mind, (7, &mut Vec::new()));
        settled(world, rules, brains, id, turn)
    }

    /// Does `decision` (its hands freed first when it lays a thing down
    /// for it), lets the gesture end, has the mind see what came of it,
    /// and take up again what it laid down.
    fn done(
        world: &mut World,
        rules: &Content,
        brains: &mut Brains,
        id: BodyId,
        decision: &Decision,
    ) {
        let freed = settled(
            world,
            rules,
            brains,
            id,
            HandsTurn::Decide(Some(decision.clone())),
        );
        let HandsTurn::Decide(Some(decision)) = freed else {
            panic!("its hands freed, the gesture: {freed:?}");
        };
        let mut events = Vec::new();
        world.queue(decision.intention.clone());
        world.advance(12, rules, &mut events);
        brains.notice(&events, world, rules);
        let here = world.body(id).unwrap().pos();
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.tries = (today(world), u32::MAX);
        let turn = hands_turn(world, rules, (id, here), mind, (7, &mut Vec::new()));
        let _ = settled(world, rules, brains, id, turn);
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
        let decision = go_do(
            &world,
            &rules,
            (id, here),
            (puts[&on_flat], on_flat),
            mind,
            &mut Vec::new(),
        );
        done(&mut world, &rules, &mut brains, id, &decision);
        let mind = brains.minds.get_mut(&id).unwrap();
        assert!(mind.handwork.gestured);
        assert!(mind.handwork.failed(on_flat) && !mind.ways.hands.contains(on_flat));
        assert_eq!(world.body(id).unwrap().stowed_g(), 0);
        // In the hollow they stay: seen in its trial, not kept yet, a
        // novelty to tell, its arms free.
        let in_hollow = put(berries_m, hollow_m);
        let decision = go_do(
            &world,
            &rules,
            (id, here),
            (puts[&in_hollow], in_hollow),
            mind,
            &mut Vec::new(),
        );
        done(&mut world, &rules, &mut brains, id, &decision);
        let mind = &brains.minds[&id];
        assert!(mind.handwork.saw(in_hollow) && !mind.ways.hands.contains(in_hollow));
        assert_eq!(mind.handwork.practised(in_hollow), 1);
        assert_eq!(mind.handwork.novel, [(in_hollow, hollow_m)]);
        let body = world.body(id).unwrap();
        assert_eq!((body.stowed_g(), body.arms_food_g()), (400, 0));
    }

    #[test]
    fn a_body_whose_hollow_is_full_of_stones_takes_one_out_when_its_arms_are_full() {
        let (rules, mut world, mut brains, id, _) = by_a_birch();
        let _here = world.body(id).unwrap().pos();
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
        let turn = |world: &mut World, brains: &mut Brains| thinks(world, &rules, brains, id);
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.served = vec![hollow_m, material(&rules, "pebble")];
        mind.ways.hands.add(put);
        // It knows the put-in alone: its arms full, the hollow full of
        // stone, nothing is done.
        let nuts = loaded(&mut world, &rules, id, "hazelnuts", 1000);
        assert_eq!(world.body(id).unwrap().food_room_g(&rules.movement), 0);
        assert_eq!(turn(&mut world, &mut brains), HandsTurn::Free);
        // It knows the take-out as well: the stone comes out, then the
        // food goes in.
        brains.minds.get_mut(&id).unwrap().ways.hands.add(take);
        let HandsTurn::Decide(Some(decision)) = turn(&mut world, &mut brains) else {
            panic!("a take-out");
        };
        assert!(
            matches!(decision.intention, Intention::TakeOut { item, .. } if item == pebble),
            "{decision:?}"
        );
        done(&mut world, &rules, &mut brains, id, &decision);
        assert!(world.item(pebble).unwrap().within().is_none());
        let HandsTurn::Decide(Some(decision)) = turn(&mut world, &mut brains) else {
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
        let _here = world.body(id).unwrap().pos();
        let hollow = with_a_hollow(&mut world, &rules, id);
        let hollow_m = material(&rules, "birch_bark") | HOLLOW;
        let known = Deed {
            gesture: Gesture::PutIn,
            on: material(&rules, "berries"),
            with: hollow_m,
        };
        let turn = |world: &mut World, brains: &mut Brains| thinks(world, &rules, brains, id);
        // It does not know the put-in: its arms full beside the hollow,
        // nothing goes in by itself, and no thought puts it there.
        brains.minds.get_mut(&id).unwrap().handwork.served = vec![hollow_m];
        let nuts = loaded(&mut world, &rules, id, "hazelnuts", 600);
        assert_eq!(turn(&mut world, &mut brains), HandsTurn::Free);
        // It heard it told and tries it (a tale tried): the loose food of
        // its arms, full or not, goes in the hollow the tale names.
        let day = today(&world);
        in_a_trial(brains.minds.get_mut(&id).unwrap(), &[known], day);
        let HandsTurn::Decide(Some(heard)) = turn(&mut world, &mut brains) else {
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
        assert_eq!(turn(&mut world, &mut brains), HandsTurn::Free);
        let more = loaded(&mut world, &rules, id, "hazelnuts", 400);
        assert_eq!(world.body(id).unwrap().food_room_g(&rules.movement), 0);
        let HandsTurn::Decide(Some(decision)) = turn(&mut world, &mut brains) else {
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
        assert_eq!(turn(&mut world, &mut brains), HandsTurn::Free);
        assert_eq!(brains.minds[&id].handwork.practised(known), 1);
        assert!(brains.minds[&id].trial.is_none(), "no trial for it");
    }

    #[test]
    fn a_kept_trial_leaves_kept_only_the_deeds_whose_product_served() {
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
        // In its trial it saw the birch's chain give its things, a
        // put-in of berries, a put-in of its flint, and the cut of an
        // oak whose bark only breaks.
        let hollow = mind
            .handwork
            .threaded(bark_deeds(&rules, birch).iter().chain([put]), 900);
        mind.handwork.threaded([cut_oak], 950);
        mind.handwork
            .made(put_flint, [Some(ItemId(960)), Some(hollow)], ItemId(960));
        // And two gestures whose product is of a kind the chain uses,
        // made on other things than the chain's: a flake struck off a
        // nodule, when the flint that cut was one taken up; the flint
        // taken out of the hollow again.
        let strike = Deed {
            gesture: Gesture::Strike,
            on: material(&rules, "flint_nodule"),
            with: material(&rules, hammer(&rules)),
        };
        assert_eq!(product(&rules, strike), Some(flint));
        let take_out = Deed {
            gesture: Gesture::TakeOut,
            on: flint,
            with: hollow_m,
        };
        mind.handwork.threaded([strike], 970);
        mind.handwork
            .made(take_out, [Some(ItemId(960)), None], ItemId(960));
        // Nothing served: its hands keep nothing, and it remembers all
        // of it as seen.
        keep_what_served(&rules, mind);
        assert!(mind.ways.hands.is_empty() && mind.handwork.kept.is_empty());
        assert_eq!(mind.handwork.seen.len(), 8);
        assert!(!mind.handwork.served(flint), "a flint that only cut");
        // The hollow held food beyond its arms: the put-in of food, the
        // fold that gave the hollow, the cut that gave the bark and the
        // flint taken for it are kept, and the flint served; the oak's
        // cut and the flint put in stay seen. By the things themselves:
        // a hollow of the same kind made in another trial, had it held
        // the food, would keep none of this.
        let mut other = mind.clone();
        other.handwork.served_things.push(ItemId(999));
        assert!(!keep_what_served(&rules, &mut other) && other.ways.hands.is_empty());
        mind.handwork.served_things.push(hollow);
        keep_what_served(&rules, mind);
        let mut served = bark_deeds(&rules, birch);
        served.add(put);
        assert_eq!(mind.ways.hands, served);
        assert_eq!(mind.handwork.kept.len(), 4);
        assert_eq!(
            mind.handwork.seen,
            [cut_oak, put_flint, strike, take_out],
            "by kind, the blow and the taking out were kept too"
        );
        assert!(mind.handwork.served(flint), "the tool of a chain kept");
        assert_eq!(mind.handwork.kept_news.len(), 4, "said in the news");
        // What it kept before the trial is not judged again.
        mind.handwork.seen.clear();
        mind.ways.hands.add(cut_oak);
        keep_what_served(&rules, mind);
        assert!(mind.ways.hands.contains(cut_oak));
        assert!(
            !mind.handwork.kept.contains(&cut_oak),
            "not from a kept trial"
        );
    }

    #[test]
    fn a_trial_of_the_hands_is_kept_if_and_only_if_a_chain_of_it_served() {
        // One verdict (28 November 2026): person 57 of the run of seed
        // 42 filled a hollow beyond its arms 43 times, its trial was
        // judged not kept by its days, and the chain was kept the next
        // day as no trial of its own. The judge of the days now says
        // what the chain says.
        let (rules, world, mut brains, id, birch) = by_a_birch();
        let flint = material(&rules, "flint");
        let hollow_m = material(&rules, "birch_bark") | HOLLOW;
        let put = Deed {
            gesture: Gesture::PutIn,
            on: material(&rules, "berries"),
            with: hollow_m,
        };
        let body = world.body(id).unwrap().clone();
        let mind = brains.minds.get_mut(&id).unwrap();
        in_a_trial(mind, &[], 0);
        let trial = mind.trial.expect("in a trial");
        let hollow = mind
            .handwork
            .threaded(bark_deeds(&rules, birch).iter().chain([put]), 900);
        // Nothing served: not kept, its hands as before.
        let mut nothing = mind.clone();
        let (kept, judged) = judge_trial(&rules, 64, &body, &mut nothing, &trial);
        assert!(!kept && !judged.kept);
        assert!(nothing.ways.hands.is_empty() && nothing.handwork.kept.is_empty());
        // The hollow held food beyond its arms: kept, whatever its days
        // say (they say nothing better: no day was lived), the chain its
        // own, and the judgement told is a success with the chain.
        mind.handwork.served_things.push(hollow);
        let (kept, judged) = judge_trial(&rules, 64, &body, mind, &trial);
        let mut served = bark_deeds(&rules, birch);
        served.add(put);
        assert!(kept && judged.kept, "the chain served");
        assert_eq!(judged.after.hands, served);
        assert_eq!(mind.ways.hands, served);
        assert_eq!(mind.handwork.kept.len(), 4, "its own");
        assert!(mind.handwork.served(flint));
        assert!(mind.handwork.kept_news.iter().all(|(_, own)| *own));
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
            matches!(
                news[..],
                [News::DeedSeen { .. }, News::ToolTaken { tool, .. }] if tool == flint_m
            ),
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
        resolve(&world, &rules, id, mind, &mut Vec::new());
        assert_eq!(mind.handwork.novel, vec![(take, material(&rules, "flint"))]);
        let told = mind.tell_own(id, 3, &mut brains.memory);
        assert_eq!(told.len(), 1, "told without a judgment");
        assert!(matches!(
            brains.memory.get(told[0]),
            Some(Record::Deed { deed, body, .. }) if *deed == take && *body == id
        ));
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.pending = Some((take, flint));
        resolve(&world, &rules, id, mind, &mut Vec::new());
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
    fn a_trial_of_the_hands_sees_what_came_to_something_and_keeps_none_of_it_yet() {
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
        resolve(&world, &rules, id, mind, &mut Vec::new());
        assert!(
            mind.handwork.seen.is_empty(),
            "a cut with a pebble: nothing"
        );
        let fold = Deed {
            gesture: Gesture::Fold,
            on: material(&rules, "birch_bark"),
            with: NOTHING,
        };
        mind.handwork.pending = Some((fold, ItemId(1)));
        // Not done yet: it waits.
        resolve(&world, &rules, id, mind, &mut Vec::new());
        assert_eq!(mind.handwork.pending, Some((fold, ItemId(1))));
        mind.handwork.done = Some(true);
        let mut news = Vec::new();
        resolve(&world, &rules, id, mind, &mut news);
        // Rule 1 (28 November 2026): seen is not kept.
        assert!(mind.handwork.saw(fold) && mind.ways.hands.is_empty());
        assert!(
            matches!(news[..], [News::DeedSeen { deed, .. }] if deed == fold),
            "{news:?}"
        );
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
        let news = start_trial(
            &world,
            &rules,
            (&brains.memory, 7),
            id,
            &mut mind,
            (2, &mut Vec::new()),
        );
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
        let seen = Plausible::seen(&world, &rules, here, mind, (&have, 4), Again::never());
        let trees = trees_near(&world, here, 4, mind).len();
        assert!(trees > 0);
        assert_eq!(seen.cuts.len(), 2 * trees);
        assert!(seen.cuts.iter().any(|(_, d)| *d == trunk));
        assert!(seen.cuts.iter().any(|(_, d)| *d == branch));
        mind.handwork.fail(branch, rules.needs.hands.failures_kept);
        let seen = Plausible::seen(&world, &rules, here, mind, (&have, 4), Again::never());
        assert!(seen.cuts.iter().any(|(_, d)| *d == trunk));
        assert!(!seen.cuts.iter().any(|(_, d)| *d == branch));
    }
    /// `mind` in a trial of its hands begun on `day` from the problem of
    /// the full arms, its time `span` days.
    fn in_a_trial_of_full_arms(rules: &Content, mind: &mut Mind, day: u64, span: u64) {
        in_a_trial(mind, &[], day);
        let days = u64::from(rules.needs.trials.trial_days);
        if let Some(trial) = mind.trial.as_mut() {
            trial.problem = Some(super::super::problems::ProblemAim {
                kind: 0,
                returns: 0,
                until: day + span,
                last: day + span + days,
            });
        }
    }

    /// Its hands' thought with no gesture left to draw today: what its
    /// hands do again by themselves, if anything.
    fn again(world: &mut World, rules: &Content, brains: &mut Brains, id: BodyId) -> HandsTurn {
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.tries = (today(world), rules.needs.hands.tries_per_day);
        thinks(world, rules, brains, id)
    }

    #[test]
    fn a_cut_that_gives_chips_is_seen_once_never_done_again_and_fells_nothing() {
        // Rules 1 and 3 (28 November 2026). In the run of seed 41 a cut
        // once seen was done again at every thought, its chips falling
        // to the ground and so for ever missing: 8,239 cuts of 9,678,
        // 738 trees felled.
        let chips = |rules: &Content, species: &crate::content::Species| {
            rules
                .cut_gives(rules.material("flint").unwrap(), species, Part::Trunk)
                .is_some_and(|m| m.id == "wood_chips")
        };
        // A tree with no bark to lift, thick enough that no single cut
        // fells it.
        let (rules, mut world, mut brains, id, tree) = beside(
            |rules, s| rules.species[s].bark.is_none() && chips(rules, &rules.species[s]),
            200,
        );
        let here = world.body(id).unwrap().pos();
        let _flint = picked(&mut world, &rules, id, "flint");
        let cut = Deed {
            gesture: Gesture::Cut,
            on: tree,
            with: material(&rules, "flint"),
        };
        let day = today(&world);
        // The tree, as a draw sees it while the cut is unknown to it:
        // once seen, the cut is set aside by the draws of a later trial
        // (third round of 28 November 2026).
        let have = carried(&world, &rules, id);
        let target = Plausible::seen(
            &world,
            &rules,
            here,
            &brains.minds[&id],
            (&have, 4),
            Again::never(),
        )
        .cuts
        .into_iter()
        .find(|(_, deed)| *deed == cut)
        .expect("the cut is plausible")
        .0;
        let HandTarget::Tree { chunk, key, .. } = target else {
            panic!("a tree");
        };
        for heard in [false, true] {
            // Drawn in a trial on no tale, then tried in one aimed at
            // that very cut (as the rare chance aims at a known deed).
            let mind = brains.minds.get_mut(&id).unwrap();
            let tale = [cut];
            in_a_trial(mind, if heard { &tale } else { &[] }, day);
            if heard {
                let have = carried(&world, &rules, id);
                let seen = Plausible::seen(&world, &rules, here, mind, (&have, 4), Again::never());
                assert!(
                    !seen.cuts.iter().any(|(_, deed)| *deed == cut) && seen.known == 1,
                    "seen in an earlier trial, the cut is set aside by the draw"
                );
            }
            if heard {
                // Heard and not yet tried: its hands go to it by
                // themselves, once.
                assert!(
                    matches!(
                        again(&mut world, &rules, &mut brains, id),
                        HandsTurn::Decide(Some(Decision {
                            intention: Intention::Cut { .. },
                            ..
                        }))
                    ),
                    "a cut heard is tried"
                );
            }
            let mind = brains.minds.get_mut(&id).unwrap();
            let decision = go_do(
                &world,
                &rules,
                (id, here),
                (target, cut),
                mind,
                &mut Vec::new(),
            );
            assert!(
                matches!(decision.intention, Intention::Cut { .. }),
                "{decision:?}"
            );
            done(&mut world, &rules, &mut brains, id, &decision);
            // The chips fell at its feet; the cut is seen, not kept, and
            // with nothing that needs chips its hands do not cut again.
            for _ in 0..3 {
                assert_eq!(
                    again(&mut world, &rules, &mut brains, id),
                    HandsTurn::Free,
                    "heard: {heard}"
                );
            }
            let mind = &brains.minds[&id];
            assert!(mind.handwork.saw(cut) && !mind.ways.hands.contains(cut));
            assert_eq!(mind.handwork.practised(cut), 1 + u32::from(heard));
            assert!(!mind.handwork.served(cut.with), "a flint that only cut");
            assert!(
                world.trees_in(chunk).iter().any(|t| t.key == key),
                "the tree stands"
            );
        }
    }

    /// Does `deed` as a gesture drawn: on a thing among those that what
    /// it sees makes plausible for it, and sees what came of it.
    fn as_drawn(world: &mut World, rules: &Content, brains: &mut Brains, id: BodyId, deed: Deed) {
        let here = world.body(id).unwrap().pos();
        let mind = brains.minds.get_mut(&id).unwrap();
        let have = carried(world, rules, id);
        let seen = Plausible::seen(world, rules, here, mind, (&have, 4), Again::never());
        let target = [seen.strikes, seen.cuts, seen.folds, seen.puts]
            .into_iter()
            .flatten()
            .find(|(_, d)| *d == deed)
            .unwrap_or_else(|| panic!("{deed:?} is plausible"))
            .0;
        let decision = go_do(
            world,
            rules,
            (id, here),
            (target, deed),
            mind,
            &mut Vec::new(),
        );
        done(world, rules, brains, id, &decision);
    }

    /// A gathering of `id` stopped full, lived by its mind.
    fn stopped_full(world: &World, rules: &Content, brains: &mut Brains, id: BodyId) {
        let stop = Event::WorkStopped {
            body: id,
            gives: "berries".to_owned(),
            grams: 100,
            why: crate::WorkEnd::Full,
        };
        brains.live_problems(&[stop], world, rules);
    }

    /// A stone that breaks flint off a nodule struck with it.
    fn hammer(rules: &Content) -> &'static str {
        let nodule = rules.material("flint_nodule").unwrap();
        ["quartz", "pebble", "sandstone", "slate"]
            .into_iter()
            .find(|m| {
                rules
                    .strike_gives(nodule, rules.material(m).unwrap())
                    .is_some_and(|piece| piece.id == "flint")
            })
            .expect("a stone that breaks flint off a nodule")
    }

    /// The chain of the run of seed 40 beside a birch: strike a flint
    /// nodule with `hammer`, cut the birch with the flake, fold the
    /// bark, put berries in the hollow.
    fn flake_chain(rules: &Content, birch: u16, hammer: &str) -> [Deed; 4] {
        let bark = material(rules, "birch_bark");
        [
            Deed {
                gesture: Gesture::Strike,
                on: material(rules, "flint_nodule"),
                with: material(rules, hammer),
            },
            Deed {
                gesture: Gesture::Cut,
                on: birch,
                with: material(rules, "flint"),
            },
            Deed {
                gesture: Gesture::Fold,
                on: bark,
                with: NOTHING,
            },
            Deed {
                gesture: Gesture::PutIn,
                on: material(rules, "berries"),
                with: bark | HOLLOW,
            },
        ]
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one chain, walked from its first gesture to its judgement"
    )]
    fn full_arms_find_the_flake_the_bark_the_hollow_and_keep_the_whole_chain() {
        // The discovery of the run of seed 40, walked under the rules of
        // 28 November 2026: strike a nodule, cut bark with the flake,
        // fold it, put food in it. Each deed is only seen until the
        // hollow has carried food beyond the arms; then the whole chain
        // is kept, and nothing else.
        let (rules, mut world, mut brains, id, birch) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let (flint_m, bark_m) = (material(&rules, "flint"), material(&rules, "birch_bark"));
        let hammer = hammer(&rules);
        picked(&mut world, &rules, id, "flint_nodule");
        picked(&mut world, &rules, id, hammer);
        let arms = world.body(id).unwrap().arms_g(&rules.movement);
        loaded(&mut world, &rules, id, "berries", arms);
        let day = today(&world);
        in_a_trial_of_full_arms(&rules, brains.minds.get_mut(&id).unwrap(), day, 16);
        let [strike, cut, fold, put] = flake_chain(&rules, birch, hammer);
        // Free hands (28 November 2026): its arms full of berries, each
        // gesture of the chain asks for them laid at its feet first, and
        // they are taken up again after it.
        let berries = carried(&world, &rules, id)
            .into_iter()
            .find(|(_, k)| *k == material(&rules, "berries"))
            .expect("its berries")
            .0;
        let struck: Vec<ItemId> = carried(&world, &rules, id)
            .into_iter()
            .map(|(i, _)| i)
            .filter(|i| *i != berries)
            .collect();
        assert_eq!(world.in_the_way(id, &struck), Some(berries));
        // Each gesture is one it may draw from what it sees, done as
        // drawn; none is done again by its hands before the put-in.
        for (n, deed) in [strike, cut, fold, put].into_iter().enumerate() {
            as_drawn(&mut world, &rules, &mut brains, id, deed);
            let (body, mind) = (world.body(id).unwrap(), &brains.minds[&id]);
            assert!(
                mind.handwork.laid.is_empty(),
                "{deed:?}: all taken up again"
            );
            assert_eq!(body.food_g(), arms, "{deed:?}: its food with it again");

            assert_eq!(
                again(&mut world, &rules, &mut brains, id),
                HandsTurn::Free,
                "{deed:?}"
            );
            let mind = &brains.minds[&id];
            assert!(mind.handwork.saw(deed), "{deed:?} seen");
            assert!(mind.ways.hands.is_empty(), "seen is not kept");
            assert_eq!(mind.handwork.trial_seen.len(), n + 1);
        }
        assert!(!brains.minds[&id].handwork.served(flint_m));
        // The put-in answers its problem: from now the trial is after
        // it, and what feeds it is made again when it lacks it. Its
        // hollow let go, its hands cut bark again.
        let hollow = carried(&world, &rules, id)
            .into_iter()
            .find(|(_, k)| *k == bark_m | HOLLOW)
            .expect("the hollow")
            .0;
        let mut without = world.clone();
        without.queue(Intention::Drop {
            body: id,
            item: hollow,
        });
        without.advance(1, &rules, &mut Vec::new());
        let mut lost = brains.minds[&id].clone();
        lost.handwork.tries = (day, rules.needs.hands.tries_per_day);
        // The hollow lies at its feet: the chain stops at the first
        // thing seen at hand (fourth round of 28 November 2026), and its
        // hands take it up again rather than make another.
        let turn = hands_turn(
            &without,
            &rules,
            (id, here),
            &mut lost.clone(),
            (7, &mut Vec::new()),
        );
        assert!(
            matches!(
                turn,
                HandsTurn::Decide(Some(Decision {
                    intention: Intention::PickUp { item, .. },
                    ..
                })) if item == hollow
            ),
            "{turn:?}"
        );
        // Out of its reach today, it has none: bark is cut again.
        lost.handwork.bar(day, Barred::Item(hollow));
        let mut turn = hands_turn(
            &without,
            &rules,
            (id, here),
            &mut lost,
            (7, &mut Vec::new()),
        );
        // Its hands freed for the cut: all but the flake laid at its
        // feet, its food too.
        let mut laid = 0;
        while let HandsTurn::Decide(Some(Decision {
            intention: intention @ Intention::Drop { .. },
            ..
        })) = &turn
        {
            laid += 1;
            assert!(laid < 64, "never done laying down");
            without.queue(intention.clone());
            without.advance(1, &rules, &mut Vec::new());
            turn = hands_turn(
                &without,
                &rules,
                (id, here),
                &mut lost,
                (7, &mut Vec::new()),
            );
        }
        assert!(laid >= 2, "{laid}");
        assert!(
            matches!(
                turn,
                HandsTurn::Decide(Some(Decision {
                    intention: Intention::Cut { .. },
                    ..
                }))
            ),
            "{turn:?}"
        );
        // Its arms full again, its hands put their food in the hollow
        // without a draw; a gathering then stops with more than its arms
        // hold: its problem relieved, the trial kept.
        loaded(&mut world, &rules, id, "berries", arms);
        let HandsTurn::Decide(Some(decision)) = again(&mut world, &rules, &mut brains, id) else {
            panic!("a put-in");
        };
        assert!(
            matches!(decision.intention, Intention::PutIn { .. }),
            "{decision:?}"
        );
        done(&mut world, &rules, &mut brains, id, &decision);
        let _ = again(&mut world, &rules, &mut brains, id);
        loaded(&mut world, &rules, id, "berries", 100);
        let body = world.body(id).unwrap();
        assert!(body.food_g() > arms && body.stowed_g() > 0);
        stopped_full(&world, &rules, &mut brains, id);
        let mind = &brains.minds[&id];
        assert!(mind.trial.is_none(), "judged");
        let mut chain = Deeds::default();
        for deed in [strike, cut, fold, put] {
            chain.add(deed);
        }
        assert_eq!(mind.ways.hands, chain, "the whole chain, and only it");
        assert_eq!(mind.handwork.kept.len(), 4, "its own");
        assert!(mind.handwork.seen.is_empty(), "kept, no longer only seen");
        assert!(mind.handwork.served(flint_m), "the tool of a chain kept");
        assert!(
            mind.judged
                .is_some_and(|j| j.kept && j.after.hands == chain)
        );
        let mut news = Vec::new();
        let mind = brains.minds.get_mut(&id).unwrap();
        let _ = hands_turn(&world, &rules, (id, here), mind, (7, &mut news));
        let said = news
            .iter()
            .filter(|n| matches!(n, News::DeedKept { own: true, .. }))
            .count();
        assert_eq!(said, 4, "{news:?}");
    }

    #[test]
    fn a_trial_of_the_hands_cut_short_by_a_move_followed_ends_by_its_one_verdict() {
        // Defect of the audit of 28 November 2026: following a move put
        // the hands back as before the trial (a hollow that had served
        // was lost) and left the deeds heard as its aim with no trial
        // (done again without end, twelve bark cuts in eleven days for
        // one person of the run of seed 40). One verdict, the same day:
        // cut short, the trial whose chain served is kept, a success
        // learnt and told; the one that served nothing ends unjudged.
        let (rules, world, mut brains, id, birch) = by_a_birch();
        let flint = material(&rules, "flint");
        let hollow_m = material(&rules, "birch_bark") | HOLLOW;
        let put = Deed {
            gesture: Gesture::PutIn,
            on: material(&rules, "berries"),
            with: hollow_m,
        };
        let cut_oak = Deed {
            gesture: Gesture::Cut,
            on: u16::try_from(rules.species.iter().position(|s| s.id == "oak").unwrap()).unwrap(),
            with: flint,
        };
        let caller = BodyId(u32::MAX);
        let mind = brains.minds.get_mut(&id).unwrap();
        in_a_trial(mind, &[cut_oak], 0);
        let hollow = mind
            .handwork
            .threaded(bark_deeds(&rules, birch).iter().chain([put]), 900);
        let to = Pos::new(mind.ways.home.x + 40, mind.ways.home.y);
        let said: Said = [(id, (Act::Joined, None, Some(caller), 0))].into();
        let moves: BTreeMap<BodyId, Pos> = [(caller, to)].into();
        let followed: BTreeMap<BodyId, BodyId> = [(id, caller)].into();
        // Nothing served: unjudged, nothing told, its will as it was,
        // its hands as before, what it was after dropped.
        let mut idle = brains.clone();
        let will = |m: &Mind| (m.will, m.will_rest, m.will_failures);
        let before = will(&idle.minds[&id]);
        idle.start_move(&world, &rules, 3, &said, (&moves, &followed));
        let mind = &idle.minds[&id];
        assert!(mind.trial.is_some_and(|t| t.way == Way::Home), "it follows");
        assert!(mind.ways.hands.is_empty() && mind.judged.is_none());
        assert_eq!(will(mind), before);
        assert!(mind.handwork.aim.is_empty(), "what it was after is dropped");
        assert!(!idle.news().iter().any(|n| matches!(
            n,
            News::Tried {
                way: Way::Hands,
                ..
            }
        )));
        // The hollow held food beyond its arms: kept, the chain its own,
        // its will taught a success, the tale a success.
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.handwork.served_things.push(hollow);
        let mut taught = mind.clone();
        taught.learn_will(&rules, true);
        brains.start_move(&world, &rules, 3, &said, (&moves, &followed));
        let mind = &brains.minds[&id];
        assert!(mind.trial.is_some_and(|t| t.way == Way::Home), "it follows");
        let mut served = bark_deeds(&rules, birch);
        served.add(put);
        assert_eq!(mind.ways.hands, served, "what served is kept");
        assert_eq!(mind.handwork.kept.len(), 4, "its own");
        assert_eq!(will(mind), will(&taught), "a success learnt");
        assert!(
            mind.judged
                .is_some_and(|j| j.kept && j.way == Way::Hands && j.after.hands == served),
            "told as a success"
        );
        assert!(mind.handwork.aim.is_empty(), "what it was after is dropped");
        assert_eq!(mind.handwork.trial_seen, []);
        // With no trial and no aim, the cut it heard is not among what
        // its hands do again.
        assert!(!wanted(&rules, mind, false).contains(&cut_oak));
        assert!(brains.news().iter().any(|n| matches!(
            n,
            News::Tried {
                way: Way::Hands,
                kept: true,
                ..
            }
        )));
    }

    #[test]
    fn the_deeds_seen_push_a_trial_s_judgment_once_never_past_its_time_and_those_days() {
        // Defect of the audit of 28 November 2026: each deed learnt
        // pushed the judgment by the trial's days again, and a trial of
        // a season of 16 days lasted 33 in the run of seed 41. Here a
        // trial whose time is one day sees a new deed every day.
        let (rules, mut world, mut brains, id, _) = by_a_birch();
        let days = u64::from(rules.needs.trials.trial_days);
        let mind = brains.minds.get_mut(&id).unwrap();
        in_a_trial_of_full_arms(&rules, mind, 0, 1);
        let until = |brains: &Brains| brains.minds[&id].trial.unwrap().problem.unwrap().until;
        assert_eq!(until(&brains), 1);
        for n in 1..=3_u16 {
            world.advance(1440, &rules, &mut Vec::new());
            assert_eq!(today(&world), u64::from(n));
            let mind = brains.minds.get_mut(&id).unwrap();
            let deed = Deed {
                gesture: Gesture::Fold,
                on: n,
                with: NOTHING,
            };
            mind.handwork.pending = Some((deed, ItemId(1)));
            mind.handwork.done = Some(true);
            let mut news = Vec::new();
            resolve(&world, &rules, id, mind, &mut news);
            assert!(matches!(news[..], [News::DeedSeen { .. }]), "{news:?}");
            assert_eq!(
                until(&brains),
                1 + days,
                "day {n}: its time and those days, once (before: day {n} + {days})"
            );
        }
        // A deed seen early pushes nothing: its time is not out.
        let mind = brains.minds.get_mut(&id).unwrap();
        in_a_trial_of_full_arms(&rules, mind, 3, 16);
        mind.handwork.pending = Some((
            Deed {
                gesture: Gesture::Fold,
                on: 9,
                with: NOTHING,
            },
            ItemId(1),
        ));
        mind.handwork.done = Some(true);
        resolve(&world, &rules, id, mind, &mut Vec::new());
        assert_eq!(until(&brains), 3 + 16);
    }

    /// A world's days lived by its brains, tick after tick, as the server
    /// and the vivarium run them: the news of them.
    fn lived(world: &mut World, rules: &Content, brains: &mut Brains, ticks: u32) -> Vec<News> {
        let mut news = Vec::new();
        for _ in 0..ticks {
            let mut events = Vec::new();
            world.advance(1, rules, &mut events);
            for (_, decision) in brains.think(world, rules, &events) {
                world.queue(decision.intention);
            }
            news.extend(brains.news());
        }
        news
    }

    fn count(news: &[News], of: impl Fn(&News) -> bool) -> usize {
        news.iter().filter(|n| of(n)).count()
    }

    #[test]
    fn a_body_lays_its_things_down_for_a_kept_deed_and_has_them_again_afterwards() {
        // Free hands (28 November 2026, the project lead): "with full
        // arms the hands are not free; one cannot do an action. He lays
        // the thing down, does his trial, and takes it back afterwards."
        // Outside any trial too: a body that keeps the bark's deeds and
        // lacks a hollow, its arms holding nuts and berries, lays them
        // down, cuts, folds, and has everything again. Lived as the
        // brains live it, thought after thought, and the same twice.
        let (rules, mut world, mut brains, id, birch) = by_a_birch();
        let flint = picked(&mut world, &rules, id, "flint");
        let nuts = loaded(&mut world, &rules, id, "hazelnuts", 448);
        let berries = loaded(&mut world, &rules, id, "berries", 300);
        brains.minds.get_mut(&id).unwrap().ways.hands = bark_deeds(&rules, birch);
        let (mut twin_world, mut twin_brains) = (world.clone(), brains.clone());
        let news = lived(&mut world, &rules, &mut brains, 60);
        let body = world.body(id).unwrap();

        let hollow = carried(&world, &rules, id)
            .into_iter()
            .find(|(_, k)| kind_hollow(*k))
            .expect("a hollow made");
        assert!(body.has(hollow.0));
        assert!(
            body.has(flint) && body.has(nuts) && body.has(berries),
            "everything again"
        );
        assert!(
            body.food_g() >= 748,
            "its food again, and what it gathered since"
        );
        assert_eq!(brains.minds[&id].handwork.laid, []);
        // The nuts and the berries laid for the cut, the flint for the
        // fold: three layings, three takings up, nothing left.
        assert_eq!(
            count(&news, |n| matches!(n, News::Laid { .. })),
            3,
            "{news:?}"
        );
        assert_eq!(
            count(&news, |n| matches!(n, News::LaidBack { late: false, .. })),
            3
        );
        assert_eq!(
            count(&news, |n| matches!(
                n,
                News::LaidLeft { .. } | News::LaidLost { .. } | News::LaidLetBe { .. }
            )),
            0
        );
        // The same on every run.
        let again = lived(&mut twin_world, &rules, &mut twin_brains, 60);
        assert_eq!(news, again);
        // (The brains' own clocks of their work differ: not the minds.)
        assert_eq!(brains.minds, twin_brains.minds);
        assert_eq!(
            format!("{:?}", world.body(id)),
            format!("{:?}", twin_world.body(id))
        );
    }

    #[test]
    fn a_body_interrupted_leaves_what_it_laid_down_and_another_may_take_it() {
        // "Nothing forces the taking back": called elsewhere before its
        // gesture, it leaves its things where they lie, as any thing on
        // the ground. It does not walk back for them; back within its
        // hands' sight, it takes up what still lies there.
        let (rules, mut world, mut brains, id, birch) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let _flint = picked(&mut world, &rules, id, "flint");
        let nuts = loaded(&mut world, &rules, id, "hazelnuts", 448);
        let berries = loaded(&mut world, &rules, id, "berries", 300);
        brains.minds.get_mut(&id).unwrap().ways.hands = bark_deeds(&rules, birch);
        // Its nuts and its berries laid for the cut...
        let mut news = Vec::new();
        for _ in 0..2 {
            let mind = brains.minds.get_mut(&id).unwrap();
            let HandsTurn::Decide(Some(decision)) =
                hands_turn(&world, &rules, (id, here), mind, (7, &mut news))
            else {
                panic!("a laying");
            };
            assert!(matches!(decision.intention, Intention::Drop { .. }));
            world.queue(decision.intention);
            world.advance(1, &rules, &mut Vec::new());
        }
        assert_eq!(world.item(nuts).unwrap().pos(), Some(here));
        assert_eq!(world.body(id).unwrap().food_g(), 0);
        // ... and it is called away, farther than its hands see.
        let near = i32::try_from(rules.needs.hands.near_tiles).unwrap();
        let away = (near + 1..40)
            .map(|d| Pos::new(here.x + d, here.y))
            .chain((near + 1..40).map(|d| Pos::new(here.x, here.y + d)))
            .find(|&p| world.map().is_passable(p))
            .expect("open ground away");
        world.queue(Intention::Teleport {
            body: id,
            target: away,
        });
        world.advance(1, &rules, &mut Vec::new());
        let there = world.body(id).unwrap().pos();
        assert!(!in_sight(here, there, near));
        let mind = brains.minds.get_mut(&id).unwrap();
        let turn = hands_turn(&world, &rules, (id, there), mind, (7, &mut news));
        assert!(
            !matches!(
                turn,
                HandsTurn::Decide(Some(Decision {
                    intention: Intention::MoveTo { .. } | Intention::PickUp { .. },
                    ..
                }))
            ),
            "it does not go back for them: {turn:?}"
        );
        assert_eq!(count(&news, |n| matches!(n, News::LaidLeft { .. })), 2);
        assert_eq!(mind.handwork.laid.len(), 2, "remembered, where they lie");
        assert!(mind.handwork.clearing.is_none(), "the gesture given up");
        assert_eq!(world.item(berries).unwrap().pos(), Some(here));
        // Another takes the berries: its own are gone when it looks.
        let other = world.spawn_body(here);
        world.queue(Intention::PickUp {
            body: other,
            item: berries,
        });
        world.advance(1, &rules, &mut Vec::new());
        assert!(world.body(other).unwrap().has(berries));
        // Back beside them, its hands idle, it takes up the nuts.
        world.queue(Intention::Teleport {
            body: id,
            target: here,
        });
        world.advance(1, &rules, &mut Vec::new());
        // The cut it keeps comes first: the nuts lie where they are, no
        // longer carried, and its hands are free at once.
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.ways.hands = Deeds::default();
        mind.handwork.served.push(material(&rules, "flint"));
        let turn = hands_turn(&world, &rules, (id, here), mind, (7, &mut news));
        assert_eq!(
            turn,
            HandsTurn::Decide(Some(Decision {
                intention: Intention::PickUp {
                    body: id,
                    item: nuts
                },
                why: Why::Hands,
            }))
        );
        assert_eq!(
            count(
                &news,
                |n| matches!(n, News::LaidLost { by: Some(b), .. } if *b == other)
            ),
            1
        );
        world.queue(Intention::PickUp {
            body: id,
            item: nuts,
        });
        world.advance(1, &rules, &mut Vec::new());
        let mind = brains.minds.get_mut(&id).unwrap();
        let _ = hands_turn(&world, &rules, (id, here), mind, (7, &mut news));
        assert_eq!(
            count(&news, |n| matches!(n, News::LaidBack { late: true, .. })),
            1
        );
        assert_eq!(mind.handwork.laid, []);
    }

    /// Its hunger at `points`.
    fn hunger(world: &mut World, rules: &Content, id: BodyId, points: u32) {
        let b = world.bodies.iter_mut().find(|b| b.id == id).unwrap();
        b.condition.deficit_j =
            u64::from(points) * u64::from(rules.needs.kj_per_hunger_point) * 1000;
        b.condition.hunger = crate::Gauge::at(points);
    }

    /// The things its hands take up again, in order, till they are idle.
    fn taken_back(
        world: &mut World,
        rules: &Content,
        brains: &mut Brains,
        id: BodyId,
        news: &mut Vec<News>,
    ) -> Vec<ItemId> {
        let mut order = Vec::new();
        for _ in 0..256 {
            let here = world.body(id).unwrap().pos();
            let mind = brains.minds.get_mut(&id).unwrap();
            mind.handwork.tries = (today(world), u32::MAX);
            let turn = hands_turn(world, rules, (id, here), mind, (7, news));
            let HandsTurn::Decide(Some(Decision {
                intention: Intention::PickUp { item, .. },
                ..
            })) = turn
            else {
                return order;
            };
            assert_eq!(mind.handwork.taking, Some(item));
            order.push(item);
            world.queue(Intention::PickUp { body: id, item });
            world.advance(1, rules, &mut Vec::new());
        }
        panic!("its hands never done taking up");
    }

    #[test]
    fn what_it_takes_up_again_is_what_it_prefers_and_the_rest_is_let_be() {
        // "If there are several things he wants to take, it is for him
        // to choose which he prefers." By what the code already knows of
        // it: hungry, food first; a kind of thing that served it; a
        // thing a deed it keeps uses; food; in a trial, the rest.
        let (mut rules, mut world, mut brains, id, _) = by_a_birch();
        let here = world.body(id).unwrap().pos();
        let pebble = picked(&mut world, &rules, id, "pebble");
        let quartz = picked(&mut world, &rules, id, "quartz");
        let flint = picked(&mut world, &rules, id, "flint");
        let berries = loaded(&mut world, &rules, id, "berries", 400);
        let leaf = picked(&mut world, &rules, id, "butterbur_leaf");
        let day = today(&world);
        let mind = brains.minds.get_mut(&id).unwrap();
        in_a_trial(mind, &[], day);
        mind.handwork.served.push(material(&rules, "flint"));
        // A deed it keeps that uses quartz and asks for nothing here.
        mind.ways.hands.add(Deed {
            gesture: Gesture::Fold,
            on: material(&rules, "quartz"),
            with: NOTHING,
        });
        // A fold of the leaf: the four other things laid at its feet.
        let fold = Deed {
            gesture: Gesture::Fold,
            on: material(&rules, "butterbur_leaf"),
            with: NOTHING,
        };
        let mut news = Vec::new();
        let mut decision = go_do(
            &world,
            &rules,
            (id, here),
            (HandTarget::Item(leaf, here), fold),
            mind,
            &mut news,
        );
        for laid in 0.. {
            let Intention::Drop { .. } = decision.intention else {
                break;
            };
            assert!(laid < 64, "never done laying down");
            world.queue(decision.intention);
            world.advance(1, &rules, &mut Vec::new());
            let mind = brains.minds.get_mut(&id).unwrap();
            mind.handwork.tries = (day, u32::MAX);
            let HandsTurn::Decide(Some(next)) =
                hands_turn(&world, &rules, (id, here), mind, (7, &mut news))
            else {
                panic!("the fold");
            };
            decision = next;
        }
        assert_eq!(count(&news, |n| matches!(n, News::Laid { .. })), 4);
        assert!(matches!(decision.intention, Intention::Fold { .. }));
        world.queue(decision.intention);
        let mut events = Vec::new();
        world.advance(12, &rules, &mut events);
        brains.notice(&events, &world, &rules);
        // Hungry: its food first, then what served, what a kept deed
        // uses, the rest.
        let (mut hungry_world, mut hungry_brains) = (world.clone(), brains.clone());
        hunger(&mut hungry_world, &rules, id, super::super::HUNGRY);
        assert_eq!(
            taken_back(
                &mut hungry_world,
                &rules,
                &mut hungry_brains,
                id,
                &mut Vec::new()
            ),
            [berries, flint, quartz, pebble]
        );
        // Not hungry: what served, what a kept deed uses, its food, the
        // rest. Its load leaving no room for the last, that one is let
        // be: it lies there as any thing on the ground.
        let load = world.body(id).unwrap().load_g();
        let mass = |i: ItemId| world.item(i).unwrap().mass_g();
        rules.movement.carry_capacity_g = load + mass(flint) + mass(quartz) + mass(berries) + 1;
        let mut news = Vec::new();
        assert_eq!(
            taken_back(&mut world, &rules, &mut brains, id, &mut news),
            [flint, quartz, berries]
        );
        assert_eq!(world.item(pebble).unwrap().pos(), Some(here));
        assert!(brains.minds[&id].handwork.laid.is_empty(), "let be");
        assert_eq!(count(&news, |n| matches!(n, News::LaidLetBe { .. })), 1);
        assert_eq!(count(&news, |n| matches!(n, News::LaidBack { .. })), 3);
    }

    #[test]
    fn a_hollow_that_holds_its_food_is_one_thing_to_lay_down_and_take_up_again() {
        // No rule says a container is worth more: it follows from free
        // hands. Three kinds of food loose in its arms are three things
        // to lay down and three to take up again for one gesture; the
        // same food in a hollow is one.
        let layings = |stowed: bool| {
            let (rules, mut world, mut brains, id, birch) = by_a_birch();
            let here = world.body(id).unwrap().pos();
            let flint = picked(&mut world, &rules, id, "flint");
            let hollow = with_a_hollow(&mut world, &rules, id);
            for food in ["hazelnuts", "berries", "sloes"] {
                let item = loaded(&mut world, &rules, id, food, 150);
                if stowed {
                    world.lay_all_but(id, &[item, hollow], &rules);
                    assert_eq!(world.put_now(id, (item, hollow), &rules), Some(item));
                    world.lay_all_but(id, &[flint, hollow], &rules);
                }
            }
            if !stowed {
                world.queue(Intention::Drop {
                    body: id,
                    item: hollow,
                });
                world.advance(1, &rules, &mut Vec::new());
            }
            assert_eq!(world.body(id).unwrap().food_g(), 450);
            let cut = Deed {
                gesture: Gesture::Cut,
                on: birch,
                with: material(&rules, "flint"),
            };
            let (chunk, key, at, _) = trees_near(&world, here, 4, &brains.minds[&id])
                .into_iter()
                .find(|t| t.3 == usize::from(birch))
                .expect("the birch");
            let target = HandTarget::Tree {
                chunk,
                key,
                at,
                with: flint,
                part: Part::Trunk,
            };
            let day = today(&world);
            let mind = brains.minds.get_mut(&id).unwrap();
            in_a_trial(mind, &[], day);
            mind.handwork.tries = (day, u32::MAX);
            let mut news = Vec::new();
            let mut decision = go_do(&world, &rules, (id, here), (target, cut), mind, &mut news);
            for laid in 0.. {
                let Intention::Drop { .. } = decision.intention else {
                    break;
                };
                assert!(laid < 64, "never done laying down");
                world.queue(decision.intention);
                world.advance(1, &rules, &mut Vec::new());
                let mind = brains.minds.get_mut(&id).unwrap();
                let HandsTurn::Decide(Some(next)) =
                    hands_turn(&world, &rules, (id, here), mind, (7, &mut news))
                else {
                    panic!("the cut");
                };
                decision = next;
            }
            assert!(matches!(decision.intention, Intention::Cut { .. }));
            assert_eq!(world.body(id).unwrap().food_g(), 0, "its hands free");
            let mut events = Vec::new();
            world.queue(decision.intention);
            world.advance(12, &rules, &mut events);
            brains.notice(&events, &world, &rules);
            let back = taken_back(&mut world, &rules, &mut brains, id, &mut news);
            let _ = thinks(&mut world, &rules, &mut brains, id);
            let body = world.body(id).unwrap();
            assert_eq!(body.food_g(), 450, "its food again");
            assert_eq!(body.stowed_g(), if stowed { 450 } else { 0 });
            assert_eq!(brains.minds[&id].handwork.laid, []);
            let laid = count(&news, |n| matches!(n, News::Laid { .. }));
            assert_eq!(back.len(), laid, "as many takings up as layings");
            laid
        };
        assert_eq!(layings(false), 3);
        assert_eq!(layings(true), 1);
    }

    #[test]
    fn a_deed_that_did_not_give_what_was_told_is_remembered_of_that_thing_alone() {
        // 28 November 2026: "told a flake, here it gave no flake". A
        // blow heard told, nodule struck with a stone: on a nodule too
        // big for its stone nothing comes. That is remembered of that
        // nodule, which is not struck again; another nodule is.
        let (rules, mut world, mut brains, id, _) = by_a_birch();
        let hammer = hammer(&rules);
        let big = loaded(&mut world, &rules, id, "flint_nodule", 9000);
        let stone = loaded(&mut world, &rules, id, hammer, 300);
        let strike = Deed {
            gesture: Gesture::Strike,
            on: material(&rules, "flint_nodule"),
            with: material(&rules, hammer),
        };
        let day = today(&world);
        in_a_trial(brains.minds.get_mut(&id).unwrap(), &[strike], day);
        let HandsTurn::Decide(Some(decision)) = again(&mut world, &rules, &mut brains, id) else {
            panic!("the blow heard, tried");
        };
        assert_eq!(
            decision.intention,
            Intention::Strike {
                body: id,
                item: big,
                with: stone
            }
        );
        done(&mut world, &rules, &mut brains, id, &decision);
        let mind = &brains.minds[&id];
        assert!(mind.handwork.amiss(strike, Barred::Item(big)), "remembered");
        assert!(!mind.handwork.saw(strike));
        // Not again on that nodule, however often its hands think.
        for _ in 0..3 {
            assert_eq!(again(&mut world, &rules, &mut brains, id), HandsTurn::Free);
        }
        // Elsewhere it may be tried again: another nodule, and the flake
        // comes.
        let small = loaded(&mut world, &rules, id, "flint_nodule", 1200);
        let HandsTurn::Decide(Some(decision)) = again(&mut world, &rules, &mut brains, id) else {
            panic!("the blow, on the other nodule");
        };
        assert!(
            matches!(decision.intention, Intention::Strike { item, .. } if item == small),
            "{decision:?}"
        );
        done(&mut world, &rules, &mut brains, id, &decision);
        let mind = &brains.minds[&id];
        assert!(mind.handwork.saw(strike), "as told: seen");
        assert!(!mind.handwork.amiss(strike, Barred::Item(small)));
        // Bounded: the oldest forgotten.
        let mut hands = Handwork::default();
        for n in 0..40 {
            hands.not_as_told(strike, Barred::Item(ItemId(n)), 32);
        }
        assert_eq!(hands.amiss.len(), 32);
        assert!(!hands.amiss(strike, Barred::Item(ItemId(7))));
        assert!(hands.amiss(strike, Barred::Item(ItemId(8))));
    }

    #[test]
    fn a_cut_whose_chips_fall_at_its_feet_is_made_once_a_tree_for_a_deed_heard() {
        // The cause found in the run of seed 42 (person 56: the same
        // linden cut 17 times running, then an alder 24 times): it heard
        // of a put-in of wood chips in a bark hollow; a cut it had seen
        // gives chips, so the cut feeds the deed heard; chips always
        // fall at the cutter's feet, so it lacked them for ever and cut
        // again at every thought. The cut did not give what it was done
        // for into its hands: remembered of that tree, not done again on
        // it.
        let (rules, mut world, mut brains, id, hazel) =
            beside(|rules, s| rules.species[s].id == "hazel", 0);
        let here = world.body(id).unwrap().pos();
        let flint = picked(&mut world, &rules, id, "flint");
        with_a_hollow(&mut world, &rules, id);
        let cut = Deed {
            gesture: Gesture::Cut,
            on: hazel,
            with: material(&rules, "flint"),
        };
        let put_chips = Deed {
            gesture: Gesture::PutIn,
            on: material(&rules, "wood_chips"),
            with: material(&rules, "birch_bark") | HOLLOW,
        };
        assert_eq!(product(&rules, cut), Some(material(&rules, "wood_chips")));
        let day = today(&world);
        let mind = brains.minds.get_mut(&id).unwrap();
        in_a_trial(mind, &[put_chips], day);
        mind.handwork.see(cut, 32);
        assert!(
            wanted(&rules, mind, true).contains(&cut),
            "it feeds the deed heard"
        );
        let mut trees = Vec::new();
        let (mut took, mut put) = (0, 0);
        for _ in 0..12 {
            let HandsTurn::Decide(Some(decision)) = again(&mut world, &rules, &mut brains, id)
            else {
                break;
            };
            match decision.intention {
                Intention::Cut {
                    chunk, tree, with, ..
                } => {
                    assert_eq!(with, flint);
                    assert!(!trees.contains(&(chunk, tree)), "never the same tree twice");
                    trees.push((chunk, tree));
                    done(&mut world, &rules, &mut brains, id, &decision);
                }
                // The chips lie at its feet: the chain stops at what it
                // sees at hand (fourth round of 28 November 2026), and
                // it takes them up, as the hollow it laid for the cut.
                Intention::PickUp { .. } => {
                    took += 1;
                    done(&mut world, &rules, &mut brains, id, &decision);
                }
                // The deed heard, tried at last.
                Intention::PutIn { .. } => {
                    put += 1;
                    done(&mut world, &rules, &mut brains, id, &decision);
                }
                other => panic!("{other:?}"),
            }
        }
        assert_eq!((trees.len(), put), (1, 1), "one cut, then the deed heard");
        assert!(took >= 1, "the chips taken up");
        assert!(
            brains.minds[&id].handwork.saw(put_chips),
            "tried: its result known"
        );
        let mind = &brains.minds[&id];
        for &(chunk, tree) in &trees {
            assert!(mind.handwork.amiss(cut, Barred::Tree(chunk, tree)));
            assert_eq!(world.notch_mm2(chunk, tree, &rules), 1320, "one bite");
        }
        let _ = here;
    }
}
