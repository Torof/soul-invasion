//! The gestures of the hands (D57; the project lead, 24 November 2026:
//! "C'est parfait"): picking up (the act of D35), cutting a tree with what
//! is in hand, folding what is carried, putting a thing carried in
//! another (amended 27 November 2026: nothing holds by itself; a hollow
//! holds what was put in it, a thing with no hollow holds nothing). No
//! recipe: what comes of a
//! gesture is computed from what the materials are on the scales they all
//! share (hardness, edge, suppleness, toughness, form, `content/`; amended
//! 27 November 2026), never from a result written for them. A gesture
//! that meets nothing to work on still takes its time and gives nothing,
//! and a fold of a stiff sheet breaks it.

use crate::content::Folded;

use serde::{Deserialize, Serialize};

use crate::content::Content;
use crate::map::{ChunkPos, Pos};
use crate::trees::TreeKey;
use crate::{BodyId, Event, GATHER_REACH_MM, Item, ItemId, Place, Refusal, Tick, World, items};

/// A gesture of the hands (D57).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Gesture {
    /// Pick up a thing lying beside (D35's act), never from a store: a
    /// heap of a hearth's reserve is no thing lying about (D58, amended
    /// 26 November 2026; taking from the common reserve is the hearth's
    /// own act). Named « take » before.
    #[default]
    PickUp,
    /// Cut a tree with what is in hand.
    Cut,
    /// Fold a thing carried.
    Fold,
    /// Put a thing carried in another thing carried.
    PutIn,
    /// Take a thing out of the thing carried it lies in (27 November
    /// 2026): the put-in undone.
    TakeOut,
    /// Strike a thing carried with another thing carried (27 November
    /// 2026).
    Strike,
}

/// A part of a standing tree a cut is made on (D57, amended 27 November
/// 2026): what the eye sees of a tree, never what to do with it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    /// Its trunk: the bark, and the living wood under it.
    #[default]
    Trunk,
    /// One of its living branches.
    Branch,
}

/// The trunk of a standing tree as it is on a journée (D57, amended 28
/// November 2026): what it is by the seed, and what men's marks have left
/// of it, healing counted ([`World::trunk_now`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trunk {
    /// How thick it is, in millimetres.
    pub trunk_mm: u32,
    /// Its whole section, in square millimetres.
    pub section_mm2: u32,
    /// The pieces of bark it wears where the hands reach, untouched: 0
    /// for a kind whose bark does not come away.
    pub plates: u32,
    /// The section cut through today.
    pub notch_mm2: u32,
    /// The pieces of bark missing today.
    pub barked: u32,
}

/// What is left of a mark of `left` on a whole of `whole` after `days`
/// journées, the whole healing in `over` journées.
fn healed(left: u32, whole: u32, days: u64, over: u64) -> u32 {
    let back = u64::from(whole).saturating_mul(days) / over.max(1);
    u32::try_from(u64::from(left).saturating_sub(back)).unwrap_or(u32::MAX)
}

/// The journées after which nothing is left of a mark of `left` on a
/// whole of `whole` that heals in `over` journées: the first for which
/// [`healed`] gives nothing.
fn heals_in(left: u32, whole: u32, over: u64) -> u64 {
    (u64::from(left).saturating_mul(over.max(1))).div_ceil(u64::from(whole.max(1)))
}

impl Part {
    /// Both parts, in a fixed order.
    pub const ALL: [Part; 2] = [Part::Trunk, Part::Branch];

    /// Its name, as the vivarium writes it.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Part::Trunk => "trunk",
            Part::Branch => "branch",
        }
    }

    #[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip_serializing_if
    fn is_trunk(&self) -> bool {
        *self == Part::Trunk
    }
}

/// A striker breaks a thing up to this many times its own mass (D57,
/// amended 27 November 2026): a pebble breaks a branch, not a log.
const STRIKE_MASS: u64 = 10;

impl Gesture {
    /// Every gesture, in a fixed order.
    pub const ALL: [Gesture; 6] = [
        Gesture::PickUp,
        Gesture::Cut,
        Gesture::Fold,
        Gesture::PutIn,
        Gesture::TakeOut,
        Gesture::Strike,
    ];

    /// Its name, as the vivarium writes it.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Gesture::PickUp => "pick up",
            Gesture::Cut => "cut",
            Gesture::Fold => "fold",
            Gesture::PutIn => "put in",
            Gesture::TakeOut => "take out",
            Gesture::Strike => "strike",
        }
    }
}

/// A gesture under way (D57): the body is busy at it until `until`; a
/// walk, sleep or any other intention ends it with nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Making {
    /// The gesture.
    pub gesture: Gesture,
    /// The tool it cuts with, the thing it folds, or the thing it puts.
    pub item: ItemId,
    /// The thing it puts `item` in, when it puts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub into: Option<ItemId>,
    /// The tree it cuts, when it cuts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<(ChunkPos, TreeKey)>,
    /// The part of the tree it cuts, when it cuts (amended 27 November
    /// 2026): the trunk when not said, as every cut of before.
    #[serde(default, skip_serializing_if = "Part::is_trunk")]
    pub part: Part,
    /// When it is done.
    pub until: Tick,
}

impl World {
    /// The things lying within `tiles` of `pos` on each axis, stored and
    /// natural, in no order (D57): what a body sees around it to try a
    /// gesture on; never the heaps of a hearth's reserve (D58, amended 26
    /// November 2026). The natural ones are found tile by tile, not chunk by
    /// chunk: a few tiles cost a few tiles.
    #[must_use]
    pub fn items_near(&self, pos: Pos, tiles: i32) -> Vec<Item> {
        let near = |p: Pos| (p.x - pos.x).abs() <= tiles && (p.y - pos.y).abs() <= tiles;
        let (low, high) = (
            Pos::new(pos.x - tiles, pos.y - tiles).chunk(),
            Pos::new(pos.x + tiles, pos.y + tiles).chunk(),
        );
        let mut out = Vec::new();
        for y in low.y..=high.y {
            for x in low.x..=high.x {
                // Food kept in a store (D58) is no item lying about: the
                // hands never pick it up (amended 26 November 2026; taking
                // from the common reserve is the hearth's own act).
                out.extend(
                    self.laid_in(ChunkPos::new(x, y))
                        .filter(|i| i.pos().is_some_and(near))
                        .cloned(),
                );
            }
        }
        if !self.natural {
            return out;
        }
        for y in pos.y - tiles..=pos.y + tiles {
            for x in pos.x - tiles..=pos.x + tiles {
                let tile = Pos::new(x, y);
                let taken = self.changes.get(&tile.chunk()).map(|c| &c.taken);
                for rule in 0..self.spawns.len() {
                    let Some(id) = items::natural_id(tile, rule) else {
                        continue;
                    };
                    if taken.is_some_and(|t| t.contains(&items::slot_of(id))) {
                        continue;
                    }
                    if let Some(item) =
                        items::natural_item(&self.map, &self.spawns, self.seed(), id)
                    {
                        out.push(item);
                    }
                }
            }
        }
        out
    }

    /// The things of [`World::items_near`] whose material `wanted`
    /// holds, in its order, each as its id, its tile and its material,
    /// nothing copied (D51, amended 26 November 2026): a natural thing of
    /// a rule whose material is not wanted is not even looked for on the
    /// ground. What a body's hands look at around it at a thought.
    pub(crate) fn things_near(
        &self,
        pos: Pos,
        tiles: i32,
        wanted: impl Fn(&str) -> bool,
    ) -> Vec<(ItemId, Pos, &str)> {
        let near = |p: Pos| (p.x - pos.x).abs() <= tiles && (p.y - pos.y).abs() <= tiles;
        let (low, high) = (
            Pos::new(pos.x - tiles, pos.y - tiles).chunk(),
            Pos::new(pos.x + tiles, pos.y + tiles).chunk(),
        );
        let mut out = Vec::new();
        for y in low.y..=high.y {
            for x in low.x..=high.x {
                let chunk = ChunkPos::new(x, y);
                // Not the food kept in a store (D58), as `items_near`.
                for i in self.laid_in(chunk) {
                    if let Some(p) = i.pos().filter(|&p| near(p))
                        && wanted(i.material())
                    {
                        out.push((i.id(), p, i.material()));
                    }
                }
            }
        }
        if !self.natural {
            return out;
        }
        let rules: Vec<usize> = (0..self.spawns.len())
            .filter(|&r| wanted(&self.spawns[r].material))
            .collect();
        if rules.is_empty() {
            return out;
        }
        for y in pos.y - tiles..=pos.y + tiles {
            for x in pos.x - tiles..=pos.x + tiles {
                let tile = Pos::new(x, y);
                let taken = self.changes.get(&tile.chunk()).map(|c| &c.taken);
                for &rule in &rules {
                    let Some(id) = items::natural_id(tile, rule) else {
                        continue;
                    };
                    if taken.is_some_and(|t| t.contains(&items::slot_of(id))) {
                        continue;
                    }
                    if let Some((at, spawn)) =
                        items::natural_lies(&self.map, &self.spawns, self.seed(), id)
                    {
                        out.push((id, at, spawn.material.as_str()));
                    }
                }
            }
        }
        out
    }

    /// Lays on the ground at `pos` a thing of `material` folded into the
    /// hollow a fold gives it, made now: for the tests.
    #[cfg(test)]
    pub(crate) fn spawn_hollow(
        &mut self,
        material: &str,
        mass_g: u32,
        pos: Pos,
        rules: &Content,
    ) -> ItemId {
        let Some(Folded::Hollow(ml)) = rules
            .material(material)
            .map(|m| m.folded(rules.needs.hands.fold_suppleness))
        else {
            panic!("{material} takes no hollow");
        };
        let id = self.spawn_item(material, mass_g, pos);
        self.items.get_mut(&id).expect("just laid").hollow_ml = ml;
        id
    }

    /// Puts `item` in `into`, both carried by `id`, at once, as a put-in
    /// ended would: for the tests.
    #[cfg(test)]
    pub(crate) fn put_now(
        &mut self,
        id: BodyId,
        (item, into): (ItemId, ItemId),
        rules: &Content,
    ) -> Option<ItemId> {
        let index = self.index_of(id)?;
        self.put_gives(index, (item, into), rules, &mut Vec::new())
    }

    /// The ticks of a gesture: the content's minutes.
    fn gesture_ticks(&self, rules: &Content) -> u64 {
        (self.calendar.ticks_per_day() * u64::from(rules.needs.hands.gesture_minutes) / (24 * 60))
            .max(1)
    }

    /// Whether the body at `index` carries `item`.
    fn carries(&self, index: usize, item: ItemId) -> bool {
        let id = self.bodies[index].id;
        self.item(item)
            .is_some_and(|i| i.place == Place::Carried { body: id })
    }

    /// Begins a gesture: the work it was at stops, its walk too.
    fn begin(&mut self, index: usize, making: Making, events: &mut Vec<Event>) {
        self.wake(index, events);
        let body = &mut self.bodies[index];
        body.forget_way();
        if !body.route.is_empty() {
            body.route.clear();
            events.push(Event::BodyStopped {
                body: body.id,
                at: body.at,
            });
        }
        body.making = Some(making);
    }

    /// `id` cuts `part` of `tree` with `with` (D57): it takes `with` in
    /// hand, the thing it held going on it, and is busy the content's
    /// minutes.
    pub(crate) fn cut(
        &mut self,
        id: BodyId,
        with: ItemId,
        (chunk, tree, part): (ChunkPos, TreeKey, Part),
        rules: &Content,
        events: &mut Vec<Event>,
    ) {
        let refuse = |events: &mut Vec<Event>, reason| {
            events.push(Event::GestureRefused {
                body: id,
                gesture: Gesture::Cut,
                reason,
            });
        };
        let Some(index) = self.index_of(id) else {
            return refuse(events, Refusal::Unknown);
        };
        if !self.carries(index, with) {
            return refuse(events, Refusal::NotCarried);
        }
        let Some(found) = self.trees_in(chunk).iter().find(|t| t.key == tree).copied() else {
            return refuse(events, Refusal::Unknown);
        };
        let at = self.bodies[index].at;
        let (dx, dy) = (i64::from(at.x - found.at.x), i64::from(at.y - found.at.y));
        if dx * dx + dy * dy > GATHER_REACH_MM * GATHER_REACH_MM {
            return refuse(events, Refusal::OutOfReach);
        }
        let body = &mut self.bodies[index];
        if body.held != Some(with) {
            body.carried.retain(|i| *i != with);
            if let Some(was) = body.held.replace(with) {
                body.carried.push(was);
            }
            events.push(Event::BodyHolds {
                body: id,
                item: Some(with),
            });
        }
        let until = Tick(self.tick.0 + self.gesture_ticks(rules));
        self.begin(
            index,
            Making {
                gesture: Gesture::Cut,
                item: with,
                into: None,
                tree: Some((chunk, tree)),
                part,
                until,
            },
            events,
        );
    }

    /// `id` folds `item` it carries (D57): busy the content's minutes.
    pub(crate) fn fold(
        &mut self,
        id: BodyId,
        item: ItemId,
        rules: &Content,
        events: &mut Vec<Event>,
    ) {
        let Some(index) = self.index_of(id) else {
            return events.push(Event::GestureRefused {
                body: id,
                gesture: Gesture::Fold,
                reason: Refusal::Unknown,
            });
        };
        if !self.carries(index, item) {
            return events.push(Event::GestureRefused {
                body: id,
                gesture: Gesture::Fold,
                reason: Refusal::NotCarried,
            });
        }
        let until = Tick(self.tick.0 + self.gesture_ticks(rules));
        self.begin(
            index,
            Making {
                gesture: Gesture::Fold,
                item,
                into: None,
                tree: None,
                part: Part::Trunk,
                until,
            },
            events,
        );
    }

    /// `id` puts `item` in `into`, both carried (D57, amended 27 November
    /// 2026): busy the content's minutes, whatever comes of it.
    pub(crate) fn put_in(
        &mut self,
        id: BodyId,
        (item, into): (ItemId, ItemId),
        rules: &Content,
        events: &mut Vec<Event>,
    ) {
        let refuse = |events: &mut Vec<Event>, reason| {
            events.push(Event::GestureRefused {
                body: id,
                gesture: Gesture::PutIn,
                reason,
            });
        };
        let Some(index) = self.index_of(id) else {
            return refuse(events, Refusal::Unknown);
        };
        if !self.carries(index, item) || !self.carries(index, into) {
            return refuse(events, Refusal::NotCarried);
        }
        let until = Tick(self.tick.0 + self.gesture_ticks(rules));
        self.begin(
            index,
            Making {
                gesture: Gesture::PutIn,
                item,
                into: Some(into),
                tree: None,
                part: Part::Trunk,
                until,
            },
            events,
        );
    }

    /// The gestures on things carried alone: put in, take out, strike.
    pub(crate) fn on_carried(
        &mut self,
        intention: &crate::Intention,
        rules: &Content,
        events: &mut Vec<Event>,
    ) {
        match *intention {
            crate::Intention::PutIn { body, item, into } => {
                self.put_in(body, (item, into), rules, events);
            }
            crate::Intention::TakeOut { body, item } => self.take_out(body, item, rules, events),
            crate::Intention::Strike { body, item, with } => {
                self.strike(body, (item, with), rules, events);
            }
            _ => {}
        }
    }

    /// `id` strikes `item` with `with`, both carried (D57, amended 27
    /// November 2026): busy the content's minutes, whatever comes of it.
    pub(crate) fn strike(
        &mut self,
        id: BodyId,
        (item, with): (ItemId, ItemId),
        rules: &Content,
        events: &mut Vec<Event>,
    ) {
        let refuse = |events: &mut Vec<Event>, reason| {
            events.push(Event::GestureRefused {
                body: id,
                gesture: Gesture::Strike,
                reason,
            });
        };
        let Some(index) = self.index_of(id) else {
            return refuse(events, Refusal::Unknown);
        };
        if !self.carries(index, item) || !self.carries(index, with) {
            return refuse(events, Refusal::NotCarried);
        }
        let until = Tick(self.tick.0 + self.gesture_ticks(rules));
        self.begin(
            index,
            Making {
                gesture: Gesture::Strike,
                item,
                into: Some(with),
                tree: None,
                part: Part::Trunk,
                until,
            },
            events,
        );
    }

    /// What a blow gives ([`Gesture::Strike`]), by what the two things
    /// are ([`Material::struck_with`]): the less tough of the two breaks,
    /// the struck or the striker, unless the other is too light to break
    /// it (under a [`STRIKE_MASS`]th of its mass). A piece comes off: of
    /// what the material breaks into when it names it, a piece of that
    /// material's mass; else half of the thing. The piece, carried.
    fn strike_gives(
        &mut self,
        index: usize,
        (item, with): (ItemId, ItemId),
        rules: &Content,
        events: &mut Vec<Event>,
    ) -> Option<ItemId> {
        if item == with || !self.carries(index, item) || !self.carries(index, with) {
            return None;
        }
        let (a, b) = (self.item(item)?, self.item(with)?);
        if a.hollow_ml > 0 || b.hollow_ml > 0 || a.within.is_some() || b.within.is_some() {
            return None;
        }
        let (on, by) = (rules.material(&a.material)?, rules.material(&b.material)?);
        let mine = on.struck_with(by, rules.needs.hands.fold_suppleness)?;
        let (broken, breaker, stuff) = if mine {
            (item, b.mass_g, on)
        } else {
            (with, a.mass_g, by)
        };
        let mass = self.item(broken)?.mass_g;
        if u64::from(breaker) * STRIKE_MASS < u64::from(mass) {
            return None;
        }
        let into = stuff
            .breaks_into
            .as_deref()
            .and_then(|id| rules.material(id));
        let off = into.map_or(mass / 2, crate::content::Material::mass_g);
        let piece = if off == 0 {
            return None;
        } else if off >= mass {
            // Too small to break further: what is left is the piece.
            let material = into?.id.clone();
            self.items.get_mut(&broken)?.material = material;
            broken
        } else {
            let part = ItemId(self.next_item);
            let mut taken = self.items.get_mut(&broken)?.split(part, off)?;
            self.next_item += 1;
            if let Some(into) = into {
                taken.material.clone_from(&into.id);
            }
            self.items.insert(part, taken);
            self.bodies[index].carried.push(part);
            part
        };
        self.inventory_changed(index, rules, events);
        Some(piece)
    }

    /// `id` takes `item` out of the thing carried it lies in (D57, amended
    /// 27 November 2026): busy the content's minutes, whatever comes of
    /// it.
    pub(crate) fn take_out(
        &mut self,
        id: BodyId,
        item: ItemId,
        rules: &Content,
        events: &mut Vec<Event>,
    ) {
        let refuse = |events: &mut Vec<Event>, reason| {
            events.push(Event::GestureRefused {
                body: id,
                gesture: Gesture::TakeOut,
                reason,
            });
        };
        let Some(index) = self.index_of(id) else {
            return refuse(events, Refusal::Unknown);
        };
        if !self.carries(index, item) {
            return refuse(events, Refusal::NotCarried);
        }
        let until = Tick(self.tick.0 + self.gesture_ticks(rules));
        let into = self.item(item).and_then(|i| i.within);
        self.begin(
            index,
            Making {
                gesture: Gesture::TakeOut,
                item,
                into,
                tree: None,
                part: Part::Trunk,
                until,
            },
            events,
        );
    }

    /// What a take-out gives ([`Gesture::TakeOut`]): nothing when the
    /// thing lies in no hollow, or when it is loose food its arms have no
    /// room for; else the thing, out of the hollow and carried as any
    /// other.
    fn take_gives(
        &mut self,
        index: usize,
        item: ItemId,
        rules: &Content,
        events: &mut Vec<Event>,
    ) -> Option<ItemId> {
        if !self.carries(index, item) {
            return None;
        }
        let thing = self.item(item)?;
        thing.within?;
        let food = rules
            .material(&thing.material)
            .is_some_and(|m| m.food.is_some());
        let body = &self.bodies[index];
        let room = body
            .arms_g(&rules.movement)
            .saturating_sub(body.arms_food_g());
        if food && thing.mass_g > room {
            return None;
        }
        self.items.get_mut(&item)?.within = None;
        self.inventory_changed(index, rules, events);
        Some(item)
    }

    /// Whether the hollow `hollow` shows it has no room for `thing` (D57,
    /// amended 27 November 2026): something lies in it already and
    /// no room is left, or `thing` is a whole thing heavier than the room
    /// left. A body sees that and does not try; whether a thing fits an
    /// empty hollow it finds by trying.
    #[must_use]
    pub fn hollow_shows_no_room(&self, hollow: ItemId, thing: ItemId, rules: &Content) -> bool {
        let Some(h) = self.item(hollow).filter(|h| h.hollow_ml > 0) else {
            return false;
        };
        let room = self.hollow_room_g(hollow, rules);
        if room >= rules.needs.hands.holds_g(h.hollow_ml) {
            return false;
        }
        // A loose thing goes in as far as the room allows; a whole thing
        // needs room for all of itself.
        self.item(thing).is_some_and(|t| {
            let loose = rules
                .material(&t.material)
                .is_some_and(crate::content::Material::loose);
            room == 0 || (!loose && t.mass_g > room)
        })
    }

    /// The grams of loose things the hollow `hollow` can still take: what
    /// its volume keeps together (the content's grams a litre) less what
    /// lies in it. 0 for a thing with no hollow.
    #[must_use]
    pub fn hollow_room_g(&self, hollow: ItemId, rules: &Content) -> u32 {
        let Some(thing) = self.item(hollow).filter(|h| h.hollow_ml > 0) else {
            return 0;
        };
        let Place::Carried { body } = thing.place else {
            return 0;
        };
        let inside: u32 = self.body(body).map_or(0, |b| {
            b.inventory()
                .filter_map(|i| self.item(i))
                .filter(|i| i.within == Some(hollow))
                .fold(0, |sum, i| sum.saturating_add(i.mass_g))
        });
        rules
            .needs
            .hands
            .holds_g(thing.hollow_ml)
            .saturating_sub(inside)
    }

    /// What a put-in gives ([`Gesture::PutIn`]), by what the two things
    /// are: nothing when the second has no hollow, when the first is
    /// itself folded into a hollow or lies in one already, or when no
    /// room is left; a loose thing (food, bits) goes in as far as the
    /// room allows, the rest staying where it was; a whole thing goes in
    /// whole or not at all. What is in the hollow now.
    fn put_gives(
        &mut self,
        index: usize,
        (item, into): (ItemId, ItemId),
        rules: &Content,
        events: &mut Vec<Event>,
    ) -> Option<ItemId> {
        if item == into || !self.carries(index, item) || !self.carries(index, into) {
            return None;
        }
        let room = self.hollow_room_g(into, rules);
        let thing = self.item(item)?;
        if thing.hollow_ml > 0 || thing.within.is_some() {
            return None;
        }
        let loose = rules
            .material(&thing.material)
            .is_some_and(crate::content::Material::loose);
        let mass = thing.mass_g;
        let goes = if loose { mass.min(room) } else { mass };
        if goes == 0 || goes > room {
            return None;
        }
        let id = self.bodies[index].id;
        let put = if goes < mass {
            // The part that fits goes in, a pile of its own.
            let part = ItemId(self.next_item);
            let mut taken = self.items.get_mut(&item)?.split(part, goes)?;
            self.next_item += 1;
            taken.within = Some(into);
            self.items.insert(part, taken);
            self.bodies[index].carried.push(part);
            part
        } else {
            self.items.get_mut(&item)?.within = Some(into);
            // What lies in a hollow is no longer in hand.
            let body = &mut self.bodies[index];
            if body.held == Some(item) {
                body.held = None;
                body.carried.push(item);
                events.push(Event::BodyHolds {
                    body: id,
                    item: None,
                });
            }
            item
        };
        self.inventory_changed(index, rules, events);
        Some(put)
    }

    /// What the hollow `hollow` carried by the body at `index` held falls
    /// at its feet (D57, amended 27 November 2026): the hollow is let go
    /// or torn, and nothing holds by itself. The caller weighs the load
    /// again.
    pub(crate) fn spill(&mut self, index: usize, hollow: ItemId, events: &mut Vec<Event>) {
        let at = self.bodies[index].pos();
        let inside: Vec<ItemId> = self.bodies[index]
            .inventory()
            .filter(|i| self.item(*i).is_some_and(|i| i.within == Some(hollow)))
            .collect();
        for item in inside {
            self.bodies[index].let_go(item);
            let Some(fallen) = self.items.get_mut(&item) else {
                continue;
            };
            fallen.within = None;
            fallen.place = Place::Ground { pos: at };
            let (material, mass_g) = (fallen.material.clone(), fallen.mass_g);
            self.lay(item, at.chunk());
            events.push(Event::ItemAdded {
                item,
                material,
                mass_g,
                at,
                version: self.chunk_version(at.chunk()),
            });
        }
    }

    /// Moves every gesture on (D57): one whose body walks or sleeps ends
    /// with nothing; one whose time is up ends with what the materials
    /// give. A cut gives what the part of the tree cut gives to the thing
    /// in hand ([`World::cut_gives`]), on the body (on the ground when
    /// too heavy for it); a fold gives the thing itself a hollow, made now, breaks
    /// it, or does nothing, by what it is. Anything else gives nothing.
    pub(crate) fn make(&mut self, rules: &Content, events: &mut Vec<Event>) {
        for index in 0..self.bodies.len() {
            let body = &self.bodies[index];
            let Some(making) = &body.making else {
                continue;
            };
            if !body.route.is_empty() || body.condition.asleep {
                self.bodies[index].making = None;
                continue;
            }
            if self.tick < making.until {
                continue;
            }
            let making = self.bodies[index].making.take().expect("a gesture");
            let on = match making.gesture {
                Gesture::Cut => making.tree.and_then(|(chunk, key)| {
                    let species = self.trees_in(chunk).iter().find(|t| t.key == key)?.species;
                    Some(self.species.get(species)?.id.clone())
                }),
                Gesture::Fold
                | Gesture::PickUp
                | Gesture::PutIn
                | Gesture::TakeOut
                | Gesture::Strike => self.item(making.item).map(|i| i.material.clone()),
            }
            .unwrap_or_default();
            let (gave, broke) = match making.gesture {
                Gesture::Cut => (self.cut_gives(index, &making, rules, events), false),
                Gesture::Fold => self.fold_gives(index, making.item, rules, events),
                Gesture::PutIn => (
                    making
                        .into
                        .and_then(|into| self.put_gives(index, (making.item, into), rules, events)),
                    false,
                ),
                Gesture::TakeOut => (self.take_gives(index, making.item, rules, events), false),
                Gesture::Strike => (
                    making.into.and_then(|with| {
                        self.strike_gives(index, (making.item, with), rules, events)
                    }),
                    false,
                ),
                Gesture::PickUp => (None, false),
            };
            events.push(Event::Gestured {
                body: self.bodies[index].id,
                gesture: making.gesture,
                item: making.item,
                on,
                into: making.into,
                part: (making.gesture == Gesture::Cut).then_some(making.part),
                gave,
                broke,
            });
        }
    }

    /// What a cut gives, by the part of the tree it is made on and what
    /// the thing in hand is ([`Content::cut_gives`]; amended 27 November
    /// 2026). On the trunk: a piece of the tree's bark, when the thing
    /// works it loose, while the trunk wears bark where the hands reach
    /// ([`World::trunk_now`]); lifting bark never deepens the notch
    /// (amended 28 November 2026). Where the bark is gone, or on a tree
    /// whose bark does not come away, a thing that works the living wood
    /// bites ([`World::bite`]): chips fall and the tree's notch deepens,
    /// until the tree falls. On a branch: the branch, when the thing
    /// works living wood.
    fn cut_gives(
        &mut self,
        index: usize,
        making: &Making,
        rules: &Content,
        events: &mut Vec<Event>,
    ) -> Option<ItemId> {
        let (chunk, key) = making.tree?;
        if self.bodies[index].held != Some(making.item) {
            return None;
        }
        let tool = rules.material(&self.item(making.item)?.material)?;
        let tree = self
            .trees_in(chunk)
            .iter()
            .find(|t| t.key == key)
            .copied()?;
        let species = rules.species.get(tree.species)?;
        let piece = |m: &crate::content::Material| (m.id.clone(), m.mass_g().max(1));
        match making.part {
            Part::Branch => {
                let (wood, mass_g) = rules.cut_gives(tool, species, Part::Branch).map(piece)?;
                Some(self.gain(index, &wood, mass_g, rules, events))
            }
            Part::Trunk => {
                let bark = species
                    .bark
                    .as_deref()
                    .and_then(|bark| rules.material(bark))
                    .filter(|bark| tool.works(bark))
                    .map(piece);
                let Some(trunk) = self.trunk_now(chunk, &tree, rules) else {
                    // Trees without parts: the bark, as before them.
                    let (bark, mass_g) = bark?;
                    return Some(self.gain(index, &bark, mass_g, rules, events));
                };
                if trunk.barked < trunk.plates {
                    // The bark is on: a piece comes away, or nothing
                    // does; the wood under it is not reached.
                    let (bark, mass_g) = bark?;
                    let lifted = Trunk {
                        barked: trunk.barked + 1,
                        ..trunk
                    };
                    self.mark(chunk, tree.key, lifted, rules);
                    events.push(Event::BarkLifted {
                        body: self.bodies[index].id,
                        chunk,
                        tree: tree.key,
                        left: lifted.plates - lifted.barked,
                        version: self.chunk_version(chunk),
                    });
                    return Some(self.gain(index, &bark, mass_g, rules, events));
                }
                self.bite(index, (chunk, tree), trunk, tool, rules, events)
            }
        }
    }

    /// The body at `index` gains a piece of `mass_g` grams of `material`,
    /// made now: carried, or lying at its feet when too heavy for it. The
    /// thing.
    fn gain(
        &mut self,
        index: usize,
        material: &str,
        mass_g: u32,
        rules: &Content,
        events: &mut Vec<Event>,
    ) -> ItemId {
        let body = &self.bodies[index];
        let (id, pos) = (body.id, body.pos());
        if body.load_g.saturating_add(mass_g) > body.capacity_g(&rules.movement) {
            let item = self.spawn_item(material, mass_g, pos);
            events.push(Event::ItemAdded {
                item,
                material: material.to_owned(),
                mass_g,
                at: pos,
                version: self.chunk_version(pos.chunk()),
            });
            return item;
        }
        let item = ItemId(self.next_item);
        self.next_item += 1;
        self.items.insert(
            item,
            Item {
                id: item,
                material: material.to_owned(),
                mass_g,
                made: self.tick,
                place: Place::Carried { body: id },
                hollow_ml: 0,
                within: None,
            },
        );
        self.bodies[index].carried.push(item);
        self.inventory_changed(index, rules, events);
        item
    }

    /// `mass_g` grams of the loose `material` fall on the ground at
    /// `pos` (D57, amended 27 November 2026): they join the pile of it
    /// lying there, told as the pile gone and there again, heavier; else
    /// they are a new pile. The pile.
    fn falls(&mut self, material: &str, mass_g: u32, pos: Pos, events: &mut Vec<Event>) -> ItemId {
        let pile = self
            .laid_in(pos.chunk())
            .find(|i| i.pos() == Some(pos) && i.material == material && i.hollow_ml == 0)
            .map(|i| i.id);
        let item = match pile {
            Some(pile) => {
                self.change(pos.chunk(), |_| {});
                events.push(Event::ItemRemoved {
                    item: pile,
                    at: pos,
                    version: self.chunk_version(pos.chunk()),
                });
                if let Some(thing) = self.items.get_mut(&pile) {
                    thing.mass_g = thing.mass_g.saturating_add(mass_g);
                }
                self.change(pos.chunk(), |_| {});
                pile
            }
            None => self.spawn_item(material, mass_g, pos),
        };
        events.push(Event::ItemAdded {
            item,
            material: material.to_owned(),
            mass_g: self.item(item).map_or(mass_g, |i| i.mass_g),
            at: pos,
            version: self.chunk_version(pos.chunk()),
        });
        item
    }

    /// How thick the trunk of `tree` is, in millimetres (D57, amended 27
    /// November 2026): from its kind's height and build and the tree's
    /// own share ([`crate::content::TreeParts::trunk_mm`]), the same for
    /// the whole life of the world. `None` where trees have no parts.
    #[must_use]
    pub fn trunk_mm(&self, tree: &crate::trees::Tree, rules: &Content) -> Option<u32> {
        let parts = rules.needs.hands.trees.as_ref()?;
        let species = rules.species.get(tree.species)?;
        let roll = crate::r#gen::scatter::roll(
            self.seed(),
            crate::r#gen::scatter::purpose::TRUNK,
            &[
                u64::from(tree.at.x.cast_unsigned()),
                u64::from(tree.at.y.cast_unsigned()),
            ],
        );
        Some(parts.trunk_mm(species, roll))
    }

    /// The trunk of `tree` of `chunk` as it stands today (D57, amended
    /// 28 November 2026): its thickness and section, the pieces of bark
    /// it wears where the hands reach, and what is left today of the
    /// marks men made on it, healed by the journées gone since the last
    /// one: a notch closes, the bark grows back, each in proportion to
    /// the content's seasons. Worked out here, when the tree is looked at
    /// or cut, never by a pass over the trees. `None` where trees have no
    /// parts.
    #[must_use]
    pub fn trunk_now(
        &self,
        chunk: ChunkPos,
        tree: &crate::trees::Tree,
        rules: &Content,
    ) -> Option<Trunk> {
        let parts = rules.needs.hands.trees.as_ref()?;
        let species = rules.species.get(tree.species)?;
        let trunk_mm = self.trunk_mm(tree, rules)?;
        let section_mm2 = crate::content::TreeParts::section_mm2(trunk_mm);
        let plates = species
            .bark
            .as_deref()
            .and_then(|bark| rules.material(bark))
            .map_or(0, |bark| parts.bark_plates(trunk_mm, bark));
        let today = self.day_now();
        let (notch_mm2, barked) = self
            .changes_in(chunk)
            .and_then(|c| c.scars.get(&tree.key))
            .filter(|scar| scar.whole_day > today)
            .map_or((0, 0), |scar| {
                let days = today.saturating_sub(scar.day);
                let (closes, grows) = parts.heal_days(self.calendar.days_per_cycle());
                (
                    healed(scar.notch_mm2.min(section_mm2), section_mm2, days, closes),
                    healed(scar.barked.min(plates), plates, days, grows),
                )
            });
        Some(Trunk {
            trunk_mm,
            section_mm2,
            plates,
            notch_mm2,
            barked,
        })
    }

    /// The tree of `chunk` with the key `key`, if it stands.
    fn standing(&self, chunk: ChunkPos, key: TreeKey) -> Option<crate::trees::Tree> {
        self.trees_in(chunk).iter().find(|t| t.key == key).copied()
    }

    /// The section of the trunk of the tree `tree` of `chunk` cut
    /// through as it is today, in square millimetres: 0 for a tree no
    /// edge bit, or whose notch has closed.
    #[must_use]
    pub fn notch_mm2(&self, chunk: ChunkPos, tree: TreeKey, rules: &Content) -> u32 {
        self.standing(chunk, tree)
            .and_then(|tree| self.trunk_now(chunk, &tree, rules))
            .map_or(0, |trunk| trunk.notch_mm2)
    }

    /// The pieces of bark the trunk of the tree `tree` of `chunk` wears
    /// today where the hands reach (D57, amended 28 November 2026): what
    /// the eye sees of a trunk, bare or not. `None` for a tree that
    /// stands no more, or where trees have no parts; 0 for a kind whose
    /// bark does not come away.
    #[must_use]
    pub fn bark_left(&self, chunk: ChunkPos, tree: TreeKey, rules: &Content) -> Option<u32> {
        let tree = self.standing(chunk, tree)?;
        let trunk = self.trunk_now(chunk, &tree, rules)?;
        Some(trunk.plates - trunk.barked)
    }

    /// Keeps the marks of `trunk`, the trunk of the tree `key` of
    /// `chunk`, as of today (D57, amended 28 November 2026), with the
    /// journée by which they will have healed if nobody adds to them.
    fn mark(&mut self, chunk: ChunkPos, key: TreeKey, trunk: Trunk, rules: &Content) {
        let Some(parts) = rules.needs.hands.trees.as_ref() else {
            return;
        };
        let day = self.day_now();
        let (closes, grows) = parts.heal_days(self.calendar.days_per_cycle());
        let heals = heals_in(trunk.notch_mm2, trunk.section_mm2, closes).max(heals_in(
            trunk.barked,
            trunk.plates,
            grows,
        ));
        self.change(chunk, |c| {
            c.scars.insert(
                key,
                crate::changes::Scar {
                    notch_mm2: trunk.notch_mm2,
                    barked: trunk.barked,
                    day,
                    whole_day: day.saturating_add(heals),
                },
            );
        });
    }

    /// The thing in hand bites the trunk of `tree` (D57, amended 27
    /// November 2026), when it works the living wood
    /// ([`Content::bite_mm2`]): it cuts through so much of what is left
    /// of the trunk's section, the chips of it fall at the cutter's feet
    /// ([`World::falls`]), and the notch, kept with the chunk's changes,
    /// deepens by whoever cuts. A trunk cut through falls
    /// ([`World::fell`]). The chips.
    fn bite(
        &mut self,
        index: usize,
        (chunk, tree): (ChunkPos, crate::trees::Tree),
        trunk: Trunk,
        tool: &crate::content::Material,
        rules: &Content,
        events: &mut Vec<Event>,
    ) -> Option<ItemId> {
        let parts = rules.needs.hands.trees.as_ref()?;
        let bite = rules.bite_mm2(tool)?;
        let (trunk_mm, section_mm2) = (trunk.trunk_mm, trunk.section_mm2);
        let before = trunk.notch_mm2.min(section_mm2);
        let cut = bite.min(section_mm2 - before);
        let notch_mm2 = before + cut;
        self.mark(chunk, tree.key, Trunk { notch_mm2, ..trunk }, rules);
        let chips_g = parts.chips_g(cut);
        let feet = self.bodies[index].pos();
        let chips = (chips_g > 0).then(|| self.falls(&parts.chips, chips_g, feet, events));
        events.push(Event::TreeBitten {
            body: self.bodies[index].id,
            chunk,
            tree: tree.key,
            chips_g,
            notch_mm2,
            section_mm2,
            version: self.chunk_version(chunk),
        });
        if notch_mm2 >= section_mm2 {
            self.fell(index, (chunk, tree), trunk_mm, rules, events);
        }
        chips
    }

    /// The tree `tree` of `chunk` falls, its trunk cut through (D57,
    /// amended 27 November 2026): felled for good in its chunk's changes
    /// (a felled forest never grows back by itself), it stands no more:
    /// its tile is free, it bears nothing, no cut finds it. Where it
    /// stood lie its trunk, a log of the trunk's mass, and the branches
    /// of its crown big enough to be things, of living wood.
    fn fell(
        &mut self,
        index: usize,
        (chunk, tree): (ChunkPos, crate::trees::Tree),
        trunk_mm: u32,
        rules: &Content,
        events: &mut Vec<Event>,
    ) {
        let (Some(parts), Some(species)) = (
            rules.needs.hands.trees.as_ref(),
            rules.species.get(tree.species),
        ) else {
            return;
        };
        self.change(chunk, |c| {
            c.scars.remove(&tree.key);
            c.felled.insert(tree.key);
        });
        if let Some(felled) = self.changes_in(chunk).map(|c| c.felled.clone()) {
            self.ground_cache.set_felled(chunk, &felled);
        }
        let at = tree.at.tile();
        let log_g = parts.trunk_g(species, trunk_mm);
        let branch_g = rules
            .material(&parts.wood)
            .map_or(0, crate::content::Material::mass_g);
        let crown_g = u64::from(log_g) * u64::from(parts.crown_percent) / 100;
        let branches = if branch_g == 0 || parts.branches_most == 0 {
            0
        } else {
            (crown_g / u64::from(branch_g)).clamp(1, u64::from(parts.branches_most))
        };
        let mut lying = vec![(parts.log.as_str(), log_g)];
        lying.extend((0..branches).map(|_| (parts.wood.as_str(), branch_g)));
        for (material, mass_g) in lying {
            let item = self.spawn_item(material, mass_g, at);
            events.push(Event::ItemAdded {
                item,
                material: material.to_owned(),
                mass_g,
                at,
                version: self.chunk_version(at.chunk()),
            });
        }
        events.push(Event::TreeFell {
            body: self.bodies[index].id,
            chunk,
            tree: tree.key,
            at: tree.at,
            species: species.id.clone(),
            trunk_mm,
            log_g,
            version: self.chunk_version(chunk),
        });
    }

    /// What a fold gives, and whether it broke the thing
    /// ([`crate::content::Material::folded`]): the thing itself with a
    /// hollow, made now; the thing broken, gone from the world; nothing
    /// when it is no sheet or is folded already.
    fn fold_gives(
        &mut self,
        index: usize,
        item: ItemId,
        rules: &Content,
        events: &mut Vec<Event>,
    ) -> (Option<ItemId>, bool) {
        if !self.carries(index, item) {
            return (None, false);
        }
        let Some(folded) = self
            .item(item)
            .filter(|i| i.hollow_ml == 0)
            .and_then(|i| rules.material(&i.material))
            .map(|m| m.folded(rules.needs.hands.fold_suppleness))
        else {
            return (None, false);
        };
        match folded {
            Folded::Nothing => (None, false),
            Folded::Breaks => {
                self.remove_item(item);
                let was_held = self.bodies[index].held == Some(item);
                self.bodies[index].let_go(item);
                self.inventory_changed(index, rules, events);
                if was_held {
                    events.push(Event::BodyHolds {
                        body: self.bodies[index].id,
                        item: None,
                    });
                }
                (None, true)
            }
            Folded::Hollow(ml) => {
                let now = self.tick;
                if let Some(thing) = self.items.get_mut(&item) {
                    thing.hollow_ml = ml;
                    thing.made = now;
                }
                self.inventory_changed(index, rules, events);
                (Some(item), false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{SAMPLE_MOVEMENT, SAMPLE_NEEDS, SAMPLE_SPAWNS};
    use crate::time::Calendar;
    use crate::{Body, Intention};

    /// The sample rules with the repository's materials and trees, the
    /// arms holding 1 kg (D57).
    fn rules() -> Content {
        let mut rules = Content::from_ron(
            include_str!("../../../content/materials.ron"),
            SAMPLE_SPAWNS,
            SAMPLE_MOVEMENT,
            SAMPLE_NEEDS,
        )
        .unwrap()
        .with_species(include_str!("../../../content/species.ron"))
        .unwrap();
        rules.movement.hands_g = 1000;
        rules
    }

    /// A valley of minute ticks, a body standing beside a birch: the
    /// world, the body, the birch's chunk and key.
    fn by_a_birch(rules: &Content) -> (World, BodyId, ChunkPos, TreeKey) {
        by_a(rules, "birch")
    }

    /// The same beside a tree of `species`.
    fn by_a(rules: &Content, species: &str) -> (World, BodyId, ChunkPos, TreeKey) {
        let mut world = World::with_valley(Calendar::new(1, 1440).unwrap(), rules, 768);
        let birch = rules.species.iter().position(|s| s.id == species).unwrap();
        let side = i32::try_from(world.map().chunks_wide()).unwrap();
        for y in 0..side {
            for x in 0..side {
                let chunk = ChunkPos::new(x, y);
                for tree in world.trees_in(chunk).iter().filter(|t| t.species == birch) {
                    let trunk = tree.at.tile();
                    let stand = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .into_iter()
                        .map(|(dx, dy)| Pos::new(trunk.x + dx, trunk.y + dy))
                        .find(|&p| {
                            world.map().is_passable(p)
                                && !world.trees_in(p.chunk()).iter().any(|t| t.at.tile() == p)
                        });
                    if let Some(stand) = stand {
                        let (chunk, key) = (chunk, tree.key);
                        let body = world.spawn_body(stand);
                        return (world, body, chunk, key);
                    }
                }
            }
        }
        panic!("a {species} with room beside it");
    }

    fn body(world: &World, id: BodyId) -> &Body {
        world.body(id).unwrap()
    }

    /// Lays `material` at the body's feet and has it picked up.
    fn picked(
        world: &mut World,
        rules: &Content,
        id: BodyId,
        material: &str,
        grams: u32,
    ) -> ItemId {
        let pos = body(world, id).pos();
        let item = world.spawn_item(material, grams, pos);
        let mut events = Vec::new();
        world.queue(Intention::PickUp { body: id, item });
        world.advance(1, rules, &mut events);
        item
    }

    /// Advances until `id` ends a gesture: what came of it.
    fn gesture_ends(world: &mut World, rules: &Content, id: BodyId) -> Option<ItemId> {
        gesture_outcome(world, rules, id).0
    }

    /// Advances until `id` ends a gesture: what came of it, and whether
    /// the thing broke.
    fn gesture_outcome(world: &mut World, rules: &Content, id: BodyId) -> (Option<ItemId>, bool) {
        let mut events = Vec::new();
        for _ in 0..30 {
            events.clear();
            world.advance(1, rules, &mut events);
            for event in &events {
                if let Event::Gestured {
                    body, gave, broke, ..
                } = event
                    && *body == id
                {
                    return (*gave, *broke);
                }
            }
        }
        panic!("no gesture ended");
    }

    /// What a cut of a tree of `species` gives with a piece of `tool`.
    fn cut(rules: &Content, species: &str, tool: &str) -> Option<String> {
        let (mut world, id, chunk, tree) = by_a(rules, species);
        let grams = rules.material(tool).unwrap().mass_g();
        let with = picked(&mut world, rules, id, tool, grams);
        world.queue(Intention::Cut {
            part: Part::Trunk,
            body: id,
            with,
            chunk,
            tree,
        });
        gesture_ends(&mut world, rules, id)
            .map(|bark| world.item(bark).unwrap().material().to_owned())
    }

    #[test]
    fn a_keen_stone_detaches_bark_a_blunt_one_does_not_each_bark_by_its_toughness() {
        let rules = rules();
        // Flint, the keenest: every bark that comes away.
        assert_eq!(cut(&rules, "birch", "flint").as_deref(), Some("birch_bark"));
        assert_eq!(cut(&rules, "oak", "flint").as_deref(), Some("oak_bark"));
        assert_eq!(
            cut(&rules, "pine", "flint").as_deref(),
            Some("pine_outer_bark")
        );
        // Quartz, hard but less keen: not the oak's tough bark.
        assert_eq!(
            cut(&rules, "birch", "quartz").as_deref(),
            Some("birch_bark")
        );
        assert_eq!(cut(&rules, "oak", "quartz"), None);
        // Slate has an edge, but a soft one: only the crumbly pine bark.
        assert_eq!(cut(&rules, "birch", "slate"), None);
        assert_eq!(cut(&rules, "oak", "slate"), None);
        assert_eq!(
            cut(&rules, "pine", "slate").as_deref(),
            Some("pine_outer_bark")
        );
        // No edge to speak of: nothing, however hard.
        for blunt in ["pebble", "sandstone", "branch"] {
            for tree in ["birch", "oak", "pine"] {
                assert_eq!(cut(&rules, tree, blunt), None, "{blunt} on {tree}");
            }
        }
        // A tree whose bark does not come away gives nothing to any edge.
        assert_eq!(cut(&rules, "hazel", "flint"), None);
    }

    #[test]
    fn a_stiff_bark_breaks_when_folded_and_is_lost() {
        let rules = rules();
        for stiff in ["oak_bark", "pine_outer_bark"] {
            let (mut world, id, ..) = by_a_birch(&rules);
            let bark = picked(&mut world, &rules, id, stiff, 400);
            let load = body(&world, id).load_g();
            world.queue(Intention::Fold {
                body: id,
                item: bark,
            });
            assert_eq!(
                gesture_outcome(&mut world, &rules, id),
                (None, true),
                "{stiff} breaks"
            );
            assert!(world.item(bark).is_none(), "the thing is gone");
            assert!(!body(&world, id).has(bark));
            assert_eq!(body(&world, id).held(), None);
            assert_eq!(body(&world, id).load_g(), load - 400);
            assert_eq!(body(&world, id).food_room_g(&rules.movement), 1000);
        }
    }

    #[test]
    fn a_big_leaf_folds_into_a_small_hollow_that_tears_within_a_day() {
        let mut rules = rules();
        rules.needs.freezing_per_day = 0;
        rules.needs.dehydration_per_day = 0;
        rules.needs.starving_per_day = 0;
        let (mut world, id, ..) = by_a_birch(&rules);
        let leaf = picked(&mut world, &rules, id, "butterbur_leaf", 50);
        world.queue(Intention::Fold {
            body: id,
            item: leaf,
        });
        assert_eq!(gesture_outcome(&mut world, &rules, id), (Some(leaf), false));
        // A square of 9 dm2: 2 litres, a kilogram of loose food.
        assert_eq!(world.item(leaf).unwrap().hollow_ml(), 2000);
        assert_eq!(world.hollow_room_g(leaf, &rules), 1000);
        // Carried, it holds nothing by itself (amended 27 November 2026).
        assert_eq!(body(&world, id).food_room_g(&rules.movement), 1000);
        let mut events = Vec::new();
        // The world looks at what tore once a day.
        world.advance(1440 * 2, &rules, &mut events);
        assert!(world.item(leaf).is_none(), "torn a day after the fold");
        // Unfolded, a leaf does not wear: only the folds tear.
        let flat = picked(&mut world, &rules, id, "butterbur_leaf", 50);
        world.advance(1440 * 3, &rules, &mut events);
        assert!(world.item(flat).is_some());
    }

    #[test]
    fn a_fold_is_computed_from_what_the_thing_is() {
        use crate::content::Folded;
        let rules = rules();
        let fold = |id: &str| {
            rules
                .material(id)
                .unwrap()
                .folded(rules.needs.hands.fold_suppleness)
        };
        assert_eq!(fold("birch_bark"), Folded::Hollow(4740));
        assert_eq!(fold("butterbur_leaf"), Folded::Hollow(2000));
        assert_eq!(fold("hide"), Folded::Hollow(74_074));
        assert_eq!(fold("oak_bark"), Folded::Breaks);
        assert_eq!(fold("pine_outer_bark"), Folded::Breaks);
        // No sheet, supple or not: nothing.
        for other in ["flint", "branch", "fibres", "nettle_leaves", "meat"] {
            assert_eq!(fold(other), Folded::Nothing, "{other}");
        }
        // Nothing in the content names what a fold gives or what holds.
        let ron = include_str!("../../../content/materials.ron");
        for word in ["folds_into", "holds_g", "sharp:", "folded_birch_bark"] {
            assert!(
                !ron.lines()
                    .any(|l| !l.trim_start().starts_with("//") && l.contains(word)),
                "{word}"
            );
        }
    }

    #[test]
    fn a_thing_folded_is_not_folded_again_nor_poured_into_another() {
        let rules = rules();
        let (mut world, id, ..) = by_a_birch(&rules);
        let bark = picked(&mut world, &rules, id, "birch_bark", 200);
        world.queue(Intention::Fold {
            body: id,
            item: bark,
        });
        gesture_ends(&mut world, &rules, id).expect("folded");
        world.queue(Intention::Fold {
            body: id,
            item: bark,
        });
        assert_eq!(gesture_outcome(&mut world, &rules, id), (None, false));
        assert_eq!(world.item(bark).unwrap().hollow_ml(), 4740);
        let mut flat = world.item(bark).unwrap().clone();
        flat.hollow_ml = 0;
        let hollow = world.item(bark).unwrap().clone();
        assert!(flat.clone().merge(hollow.clone()).is_err());
        assert!(hollow.clone().merge(flat).is_err());
    }

    #[test]
    fn a_flint_cuts_a_birchs_bark_and_the_bark_folds_into_a_hollow() {
        let rules = rules();
        let (mut world, id, chunk, tree) = by_a_birch(&rules);
        // Berries in hand first: the flint goes on the body, and the cut
        // takes it in hand.
        picked(&mut world, &rules, id, "berries", 500);
        let flint = picked(&mut world, &rules, id, "flint", 400);
        assert_ne!(body(&world, id).held(), Some(flint));
        world.queue(Intention::Cut {
            part: Part::Trunk,
            body: id,
            with: flint,
            chunk,
            tree,
        });
        let mut events = Vec::new();
        world.advance(1, &rules, &mut events);
        assert_eq!(body(&world, id).held(), Some(flint), "the tool in hand");
        assert!(body(&world, id).making().is_some());
        let bark = gesture_ends(&mut world, &rules, id).expect("the bark cut");
        assert_eq!(world.item(bark).unwrap().material(), "birch_bark");
        assert!(body(&world, id).has(bark));
        // Ten game minutes: ten ticks of this day.
        let arms = body(&world, id).arms_g(&rules.movement);
        assert_eq!(arms, 1000);
        world.queue(Intention::Fold {
            body: id,
            item: bark,
        });
        let folded = gesture_ends(&mut world, &rules, id).expect("the bark folded");
        assert_eq!(folded, bark, "the same thing, folded");
        assert_eq!(world.item(bark).unwrap().material(), "birch_bark");
        assert_eq!(world.item(bark).unwrap().hollow_ml(), 4740);
        assert_eq!(
            world.hollow_room_g(bark, &rules),
            2370,
            "4.74 litres: room for 2.37 kg of loose food"
        );
        assert_eq!(body(&world, id).stowed_g(), 0, "nothing in it yet");
    }

    #[test]
    fn a_cut_with_no_edge_and_a_fold_of_what_does_not_fold_give_nothing() {
        let rules = rules();
        let (mut world, id, chunk, tree) = by_a_birch(&rules);
        let pebble = picked(&mut world, &rules, id, "pebble", 600);
        world.queue(Intention::Cut {
            part: Part::Trunk,
            body: id,
            with: pebble,
            chunk,
            tree,
        });
        assert_eq!(gesture_ends(&mut world, &rules, id), None);
        world.queue(Intention::Fold {
            body: id,
            item: pebble,
        });
        assert_eq!(gesture_ends(&mut world, &rules, id), None);
        assert_eq!(world.item(pebble).unwrap().material(), "pebble");
        // A walk ends a gesture with nothing.
        world.queue(Intention::Fold {
            body: id,
            item: pebble,
        });
        let mut events = Vec::new();
        world.advance(1, &rules, &mut events);
        let here = body(&world, id).pos();
        world.queue(Intention::MoveTo {
            body: id,
            target: Pos::new(here.x + 3, here.y),
        });
        world.advance(1, &rules, &mut events);
        assert!(body(&world, id).making().is_none());
    }

    #[test]
    fn the_arms_hold_a_kilogram_of_loose_food_whole_things_the_whole_load() {
        let rules = rules();
        let (mut world, id, ..) = by_a_birch(&rules);
        let heavy = picked(&mut world, &rules, id, "berries", 1500);
        assert!(!body(&world, id).has(heavy), "1.5 kg of berries: too much");
        let berries = picked(&mut world, &rules, id, "berries", 800);
        assert!(body(&world, id).has(berries));
        assert_eq!(body(&world, id).food_room_g(&rules.movement), 200);
        // Whole things count against the load alone, as before.
        for _ in 0..4 {
            let branch = picked(&mut world, &rules, id, "branch", 1500);
            assert!(body(&world, id).has(branch));
        }
        assert_eq!(body(&world, id).load_g(), 800 + 6000);
    }

    #[test]
    fn a_container_tears_after_its_days() {
        let mut rules = rules();
        let folded = rules
            .materials
            .iter_mut()
            .find(|m| m.id == "birch_bark")
            .unwrap();
        folded.wears_seasons = 0;
        folded.wears_days = 1;
        // Two days alive whatever the cold, thirst and hunger.
        rules.needs.freezing_per_day = 0;
        rules.needs.dehydration_per_day = 0;
        rules.needs.starving_per_day = 0;
        let (mut world, id, ..) = by_a_birch(&rules);
        let bark = picked(&mut world, &rules, id, "birch_bark", 200);
        world.queue(Intention::Fold {
            body: id,
            item: bark,
        });
        gesture_ends(&mut world, &rules, id).expect("folded");
        assert_eq!(world.hollow_room_g(bark, &rules), 2370);
        // What it holds falls at the body's feet when it tears.
        let berries = picked(&mut world, &rules, id, "berries", 600);
        world.queue(Intention::PutIn {
            body: id,
            item: berries,
            into: bark,
        });
        assert_eq!(gesture_ends(&mut world, &rules, id), Some(berries));
        assert_eq!(body(&world, id).stowed_g(), 600);
        let mut events = Vec::new();
        world.advance(1440 * 2, &rules, &mut events);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Spoiled { item, material, holder: Some(b), .. }
                if *item == bark && material == "birch_bark" && *b == id
        )));
        assert!(world.item(bark).is_none());
        assert!(!body(&world, id).has(berries), "spilt with the tear");
        assert_eq!(body(&world, id).food_g(), 0);
    }

    #[test]
    fn a_thing_put_in_a_hollow_is_taken_out_again() {
        let rules = rules();
        let (mut world, id, ..) = by_a_birch(&rules);
        let bark = hollow(&mut world, &rules, id);
        let room = world.hollow_room_g(bark, &rules);
        let pebble = picked(&mut world, &rules, id, "pebble", 300);
        assert_eq!(put(&mut world, &rules, id, (pebble, bark)), Some(pebble));
        assert_eq!(world.hollow_room_g(bark, &rules), room - 300);
        world.queue(Intention::TakeOut {
            body: id,
            item: pebble,
        });
        assert_eq!(gesture_ends(&mut world, &rules, id), Some(pebble));
        assert!(world.item(pebble).unwrap().within().is_none());
        assert!(
            body(&world, id).has(pebble),
            "out of the hollow, still on it"
        );
        assert_eq!(world.hollow_room_g(bark, &rules), room);
        // What lies in nothing is taken out of nothing.
        world.queue(Intention::TakeOut {
            body: id,
            item: pebble,
        });
        assert_eq!(gesture_ends(&mut world, &rules, id), None);
    }

    #[test]
    fn the_less_tough_of_two_things_struck_breaks_and_a_nodule_gives_keen_flint() {
        let rules = rules();
        let (mut world, id, ..) = by_a_birch(&rules);
        let strike = |world: &mut World, item, with| {
            world.queue(Intention::Strike {
                body: id,
                item,
                with,
            });
            gesture_ends(world, &rules, id)
        };
        let nodule = picked(&mut world, &rules, id, "flint_nodule", 1200);
        let pebble = picked(&mut world, &rules, id, "pebble", 600);
        // The pebble is the tougher: a piece of keen flint comes off the
        // nodule, which keeps the rest.
        let flake = strike(&mut world, nodule, pebble).expect("a piece");
        let piece = world.item(flake).unwrap();
        assert_eq!((piece.material(), piece.mass_g()), ("flint", 400));
        assert_eq!(world.item(nodule).unwrap().mass_g(), 800);
        assert!(body(&world, id).has(flake));
        // The pebble struck with the nodule: the nodule breaks all the
        // same, it is the less tough whichever hand holds it.
        let second = strike(&mut world, pebble, nodule).expect("a piece");
        assert_eq!(world.item(second).unwrap().material(), "flint");
        assert_eq!(world.item(pebble).unwrap().mass_g(), 600);
        // What is left is a piece itself.
        assert_eq!(strike(&mut world, nodule, pebble), Some(nodule));
        assert_eq!(world.item(nodule).unwrap().material(), "flint");
        // A branch breaks in two under the pebble; a pebble does not break
        // a log, forty times its mass; a bark that folds gives under the
        // blow.
        let branch = picked(&mut world, &rules, id, "branch", 1500);
        let half = strike(&mut world, branch, pebble).expect("a half");
        assert_eq!(
            (
                world.item(half).unwrap().material(),
                world.item(half).unwrap().mass_g()
            ),
            ("branch", 750)
        );
        let bark = picked(&mut world, &rules, id, "birch_bark", 200);
        assert_eq!(strike(&mut world, bark, pebble), None);
        // As tough as each other: nothing.
        assert_eq!(strike(&mut world, flake, half), None);
    }

    /// Folds a birch bark of 200 g on the body: the hollow.
    fn hollow(world: &mut World, rules: &Content, id: BodyId) -> ItemId {
        let bark = picked(world, rules, id, "birch_bark", 200);
        world.queue(Intention::Fold {
            body: id,
            item: bark,
        });
        gesture_ends(world, rules, id).expect("folded")
    }

    /// `id` puts `item` in `into`: what is in it now.
    fn put(
        world: &mut World,
        rules: &Content,
        id: BodyId,
        (item, into): (ItemId, ItemId),
    ) -> Option<ItemId> {
        world.queue(Intention::PutIn {
            body: id,
            item,
            into,
        });
        gesture_ends(world, rules, id)
    }

    #[test]
    fn food_put_in_a_hollow_is_off_the_arms_as_far_as_its_room() {
        let rules = rules();
        let (mut world, id, ..) = by_a_birch(&rules);
        let bark = hollow(&mut world, &rules, id);
        // Its arms full: the hollow it carries takes nothing by itself.
        let first = picked(&mut world, &rules, id, "berries", 1000);
        assert_eq!(body(&world, id).food_room_g(&rules.movement), 0);
        let refused = world.spawn_item("hazelnuts", 100, body(&world, id).pos());
        let mut events = Vec::new();
        world.queue(Intention::PickUp {
            body: id,
            item: refused,
        });
        world.advance(1, &rules, &mut events);
        assert!(!body(&world, id).has(refused), "its arms are full");
        // Put in: its arms are free again, the food still carried.
        assert_eq!(put(&mut world, &rules, id, (first, bark)), Some(first));
        assert_eq!(world.item(first).unwrap().within(), Some(bark));
        let b = body(&world, id);
        assert_eq!((b.food_g(), b.stowed_g(), b.arms_food_g()), (1000, 1000, 0));
        assert_eq!(b.food_room_g(&rules.movement), 1000);
        assert_eq!(world.hollow_room_g(bark, &rules), 1370);
        // Part full: loose food still shows room; a whole thing heavier
        // than the room left does not (a branch weighs 1.5 kg).
        let branch = world.spawn_item("branch", 1500, body(&world, id).pos());
        assert!(!world.hollow_shows_no_room(bark, first, &rules));
        assert!(world.hollow_shows_no_room(bark, branch, &rules));
        // What lies in it is not put in again.
        assert_eq!(put(&mut world, &rules, id, (first, bark)), None);
        // A second kilogram, then a third of which only 370 g fit: the
        // part that fits goes in, the rest stays in its arms.
        let second = picked(&mut world, &rules, id, "hazelnuts", 1000);
        assert_eq!(put(&mut world, &rules, id, (second, bark)), Some(second));
        let third = picked(&mut world, &rules, id, "berries", 1000);
        let part = put(&mut world, &rules, id, (third, bark)).expect("a part");
        assert_ne!(part, third);
        assert_eq!(world.item(part).unwrap().mass_g(), 370);
        assert_eq!(world.item(third).unwrap().mass_g(), 630);
        let b = body(&world, id);
        assert_eq!(
            (b.food_g(), b.stowed_g(), b.arms_food_g()),
            (3000, 2370, 630)
        );
        assert_eq!(world.hollow_room_g(bark, &rules), 0);
        // Full: it shows no room, for anything.
        assert!(world.hollow_shows_no_room(bark, third, &rules));
        // Full: nothing more goes in.
        assert_eq!(put(&mut world, &rules, id, (third, bark)), None);
        // Saved and read back, what lies in the hollow lies in it still.
        let saved = serde_json::to_string(&world).unwrap();
        assert!(saved.contains("\"within\""));
        let read: World = serde_json::from_str(&saved).unwrap();
        assert_eq!(read.item(first).unwrap().within(), Some(bark));
        assert_eq!(read.item(third).unwrap().within(), None);
    }

    #[test]
    fn a_thing_with_no_hollow_holds_nothing_and_a_whole_thing_goes_in_whole_or_not() {
        let rules = rules();
        let (mut world, id, ..) = by_a_birch(&rules);
        let flat = picked(&mut world, &rules, id, "birch_bark", 200);
        let berries = picked(&mut world, &rules, id, "berries", 500);
        // A flat sheet, a stone: nothing stays in them.
        assert_eq!(put(&mut world, &rules, id, (berries, flat)), None);
        let flint = picked(&mut world, &rules, id, "flint", 300);
        assert_eq!(put(&mut world, &rules, id, (berries, flint)), None);
        assert_eq!(world.item(berries).unwrap().within(), None);
        assert_eq!(body(&world, id).stowed_g(), 0);
        // A whole thing goes in a hollow whole: it takes its room and
        // frees no arm, the arms holding loose food alone.
        let bark = hollow(&mut world, &rules, id);
        assert_eq!(put(&mut world, &rules, id, (flint, bark)), Some(flint));
        assert_eq!(world.hollow_room_g(bark, &rules), 2070);
        assert_eq!(body(&world, id).stowed_g(), 0);
        assert_eq!(body(&world, id).food_room_g(&rules.movement), 500);
        // Too big for the room left, it does not go in; nor does a
        // thing in itself, nor a hollow in a hollow.
        let log = picked(&mut world, &rules, id, "branch", 2500);
        assert_eq!(put(&mut world, &rules, id, (log, bark)), None);
        assert_eq!(put(&mut world, &rules, id, (bark, bark)), None);
        let leaf = picked(&mut world, &rules, id, "butterbur_leaf", 50);
        world.queue(Intention::Fold {
            body: id,
            item: leaf,
        });
        gesture_ends(&mut world, &rules, id).expect("folded");
        assert_eq!(put(&mut world, &rules, id, (leaf, bark)), None);
    }

    #[test]
    fn a_hollow_let_go_spills_what_it_held_and_a_thing_taken_in_hand_is_out_of_it() {
        let rules = rules();
        let (mut world, id, ..) = by_a_birch(&rules);
        let bark = hollow(&mut world, &rules, id);
        let berries = picked(&mut world, &rules, id, "berries", 800);
        let nuts = picked(&mut world, &rules, id, "hazelnuts", 100);
        assert_eq!(put(&mut world, &rules, id, (berries, bark)), Some(berries));
        assert_eq!(put(&mut world, &rules, id, (nuts, bark)), Some(nuts));
        // Taken in hand, the nuts are in its arms again.
        let mut events = Vec::new();
        world.queue(Intention::Hold {
            body: id,
            item: nuts,
        });
        world.advance(1, &rules, &mut events);
        assert_eq!(world.item(nuts).unwrap().within(), None);
        assert_eq!(body(&world, id).stowed_g(), 800);
        // The hollow dropped, what it held lies on the ground with it.
        world.queue(Intention::Drop {
            body: id,
            item: bark,
        });
        world.advance(1, &rules, &mut events);
        let here = body(&world, id).pos();
        assert_eq!(world.item(bark).unwrap().pos(), Some(here));
        assert_eq!(world.item(berries).unwrap().pos(), Some(here));
        assert_eq!(world.item(berries).unwrap().within(), None);
        let b = body(&world, id);
        assert_eq!((b.food_g(), b.stowed_g()), (100, 0));
        assert!(b.has(nuts) && !b.has(berries));
    }

    #[test]
    fn the_things_near_are_those_of_their_chunks_found_tile_by_tile() {
        let rules = rules();
        let (mut world, id, ..) = by_a_birch(&rules);
        let here = world.body(id).unwrap().pos();
        picked(&mut world, &rules, id, "flint", 400);
        let laid = world.spawn_item("pebble", 600, Pos::new(here.x + 2, here.y));
        let mut seen: Vec<ItemId> = world.items_near(here, 4).iter().map(Item::id).collect();
        seen.sort_unstable();
        let mut chunks: Vec<ChunkPos> = [(-4, -4), (4, 4), (-4, 4), (4, -4)]
            .iter()
            .map(|(dx, dy)| Pos::new(here.x + dx, here.y + dy).chunk())
            .collect();
        chunks.sort_unstable_by_key(|c| (c.x, c.y));
        chunks.dedup();
        let mut all: Vec<ItemId> = chunks
            .into_iter()
            .flat_map(|c| world.items_in(c))
            .filter(|i| {
                i.pos()
                    .is_some_and(|p| (p.x - here.x).abs() <= 4 && (p.y - here.y).abs() <= 4)
            })
            .map(|i| i.id())
            .collect();
        all.sort_unstable();
        assert_eq!(seen, all);
        assert!(seen.contains(&laid));
    }

    #[test]
    fn the_hands_see_the_things_of_a_material_as_all_the_things_near_hold_them() {
        let rules = rules();
        let (mut world, id, ..) = by_a_birch(&rules);
        let here = world.body(id).unwrap().pos();
        picked(&mut world, &rules, id, "flint", 400);
        world.spawn_item("pebble", 600, Pos::new(here.x + 2, here.y));
        world.spawn_item("flint", 300, Pos::new(here.x - 1, here.y + 3));
        let all: Vec<(ItemId, Pos, String)> = world
            .items_near(here, 4)
            .iter()
            .map(|i| (i.id(), i.pos().unwrap(), i.material().to_owned()))
            .collect();
        assert!(all.iter().any(|t| crate::items::is_natural(t.0)));
        assert!(all.iter().any(|t| !crate::items::is_natural(t.0)));
        for wanted in [None, Some("flint"), Some("pebble"), Some("bark")] {
            let seen: Vec<(ItemId, Pos, String)> = world
                .things_near(here, 4, |m| wanted.is_none_or(|w| w == m))
                .into_iter()
                .map(|(i, p, m)| (i, p, m.to_owned()))
                .collect();
            let expected: Vec<(ItemId, Pos, String)> = all
                .iter()
                .filter(|t| wanted.is_none_or(|w| w == t.2))
                .cloned()
                .collect();
            assert_eq!(seen, expected, "{wanted:?}");
        }
    }

    /// The rules of [`rules`] with the trees in parts (D57, amended 27
    /// November 2026), as `content/needs.ron` has them.
    fn tree_rules() -> Content {
        let mut rules = rules();
        rules.needs.hands.trees = Some(crate::content::TreeParts::placeholders());
        rules.validate().unwrap();
        rules
    }

    /// The tree `key` of `chunk`, while it stands.
    fn standing(world: &World, chunk: ChunkPos, key: TreeKey) -> Option<crate::trees::Tree> {
        world.trees_in(chunk).iter().find(|t| t.key == key).copied()
    }

    /// `id` cuts `part` of the tree with `with`: what came of it, as its
    /// material, and the events of the gesture's last tick.
    fn cut_part(
        world: &mut World,
        rules: &Content,
        (id, with): (BodyId, ItemId),
        (chunk, tree, part): (ChunkPos, TreeKey, Part),
    ) -> (Option<String>, Vec<Event>) {
        world.queue(Intention::Cut {
            body: id,
            with,
            chunk,
            tree,
            part,
        });
        let mut events = Vec::new();
        for _ in 0..30 {
            events.clear();
            world.advance(1, rules, &mut events);
            let ended = events.iter().find_map(|e| match e {
                Event::Gestured { body, gave, .. } if *body == id => Some(*gave),
                _ => None,
            });
            if let Some(gave) = ended {
                let gave = gave.map(|g| world.item(g).unwrap().material().to_owned());
                return (gave, events);
            }
        }
        panic!("no gesture ended");
    }

    #[test]
    fn a_trunk_is_as_thick_as_its_kind_is_tall_and_built_each_tree_by_its_own_share() {
        let rules = tree_rules();
        let parts = rules.needs.hands.trees.clone().unwrap();
        let kind = |id: &str| rules.species.iter().find(|s| s.id == id).unwrap();
        // The kind's own thickness, the middle of the spread: a height
        // over 80 for a small kind, over 40 for a big one.
        assert_eq!(parts.trunk_mm(kind("hazel"), 25), 62);
        assert_eq!(parts.trunk_mm(kind("birch"), 25), 225);
        assert_eq!(parts.trunk_mm(kind("oak"), 25), 550);
        // A quarter thinner, a quarter thicker at most.
        assert_eq!(parts.trunk_mm(kind("birch"), 0), 168);
        assert_eq!(parts.trunk_mm(kind("birch"), 50), 281);
        assert_eq!(parts.trunk_mm(kind("birch"), 51), 168);
        // A tree of the world has its own, the same every time.
        let (world, _, chunk, key) = by_a(&rules, "birch");
        let tree = standing(&world, chunk, key).unwrap();
        let mm = world.trunk_mm(&tree, &rules).unwrap();
        assert!((168..=281).contains(&mm), "{mm}");
        assert_eq!(world.trunk_mm(&tree, &rules), Some(mm));
        let birch = tree.species;
        let side = i32::try_from(world.map().chunks_wide()).unwrap();
        let all: std::collections::BTreeSet<u32> = (0..side * side)
            .flat_map(|n| world.trees_in(ChunkPos::new(n % side, n / side)).to_vec())
            .filter(|t| t.species == birch)
            .filter_map(|t| world.trunk_mm(&t, &rules))
            .collect();
        assert!(all.len() > 10, "the birches differ: {all:?}");
        assert!(all.iter().all(|mm| (168..=281).contains(mm)));
        // Where the content gives trees no parts, no thickness.
        assert_eq!(world.trunk_mm(&tree, &self::rules()), None);
        // The log of a trunk: its section, its height, the stem's form,
        // the living wood's density.
        assert_eq!(parts.trunk_g(kind("hazel"), 62), 6_113);
        assert_eq!(parts.trunk_g(kind("birch"), 225), 289_850);
        assert_eq!(parts.trunk_g(kind("oak"), 550), 2_116_855);
    }

    #[test]
    fn felling_with_a_flake_in_the_hand_takes_an_hour_for_a_thin_tree_and_days_for_a_big_oak() {
        use crate::content::TreeParts;
        let rules = tree_rules();
        let stuff = |id: &str| rules.material(id).unwrap();
        // Keen flint works living wood: 330 mm² for each point of its
        // edge (9) over the wood's toughness (6), plus one.
        let bite = rules.bite_mm2(stuff("flint")).unwrap();
        assert_eq!(bite, 1320);
        // Nothing else lying about does: an edge under the wood's
        // toughness, or none.
        for other in ["quartz", "slate", "pebble", "sandstone", "branch", "bone"] {
            assert_eq!(rules.bite_mm2(stuff(other)), None, "{other}");
        }
        // The gestures to cut a trunk through, ten minutes each: 20
        // minutes for 5 cm, an hour for 10 cm, 16 hours of work for an
        // oak of 40 cm, 30 for one of 55 cm.
        assert_eq!(rules.needs.hands.gesture_minutes, 10);
        let cuts = |mm: u32| TreeParts::section_mm2(mm).div_ceil(bite);
        assert_eq!(
            [50, 100, 225, 400, 550, 700].map(cuts),
            [2, 6, 31, 96, 180, 292]
        );
        // Without parts, nothing bites.
        assert_eq!(self::rules().bite_mm2(stuff("flint")), None);
    }

    #[test]
    fn a_cut_on_a_branch_takes_a_green_branch_off_with_an_edge_that_works_living_wood() {
        let rules = tree_rules();
        for (tool, gives) in [("flint", Some("green_branch")), ("quartz", None)] {
            let (mut world, id, chunk, tree) = by_a(&rules, "hazel");
            let with = picked(&mut world, &rules, id, tool, 400);
            let (gave, events) =
                cut_part(&mut world, &rules, (id, with), (chunk, tree, Part::Branch));
            assert_eq!(gave.as_deref(), gives, "{tool}");
            // The gesture says its part; the trunk is not touched.
            assert!(events.iter().any(|e| matches!(
                e,
                Event::Gestured { part: Some(Part::Branch), on, .. } if on == "hazel"
            )));
            assert_eq!(world.notch_mm2(chunk, tree, &rules), 0);
            assert!(!events.iter().any(|e| matches!(e, Event::TreeBitten { .. })));
            if gives.is_some() {
                let branch = body(&world, id)
                    .inventory()
                    .filter_map(|i| world.item(i))
                    .find(|i| i.material() == "green_branch")
                    .expect("carried");
                assert_eq!(branch.mass_g(), 2000);
            }
        }
        // Without parts, a tree has no branch to take.
        let plain = self::rules();
        let (mut world, id, chunk, tree) = by_a(&plain, "hazel");
        let with = picked(&mut world, &plain, id, "flint", 400);
        let (gave, _) = cut_part(&mut world, &plain, (id, with), (chunk, tree, Part::Branch));
        assert_eq!(gave, None);
    }

    #[test]
    fn an_edge_that_works_living_wood_bites_the_trunk_chips_fall_and_the_notch_lasts() {
        let rules = tree_rules();
        let (mut world, id, chunk, tree) = by_a(&rules, "birch");
        let section = crate::content::TreeParts::section_mm2(
            world
                .trunk_mm(&standing(&world, chunk, tree).unwrap(), &rules)
                .unwrap(),
        );
        // The bark gone where the hands reach (28 November 2026), flint
        // bites the wood: chips, and a notch of one bite.
        let flint = picked(&mut world, &rules, id, "flint", 400);
        strip(&mut world, &rules, (id, flint), (chunk, tree));
        let (gave, events) = cut_part(&mut world, &rules, (id, flint), (chunk, tree, Part::Trunk));
        assert_eq!(gave.as_deref(), Some("wood_chips"));
        assert!(events.iter().any(|e| matches!(
            e,
            Event::TreeBitten { body, chips_g: 66, notch_mm2: 1320, section_mm2, .. }
                if *body == id && *section_mm2 == section
        )));
        assert_eq!(world.notch_mm2(chunk, tree, &rules), 1320);
        // The chips fell at its feet; it carries none.
        let feet = body(&world, id).pos();
        let chips = |world: &World| -> Vec<u32> {
            world
                .laid_in(feet.chunk())
                .filter(|i| i.pos() == Some(feet) && i.material() == "wood_chips")
                .map(Item::mass_g)
                .collect()
        };
        assert_eq!(chips(&world), [66]);
        assert!(
            body(&world, id)
                .inventory()
                .filter_map(|i| world.item(i))
                .all(|i| i.material() != "wood_chips")
        );
        assert!(events.iter().any(|e| matches!(
            e,
            Event::ItemAdded { material, mass_g: 66, at, .. } if material == "wood_chips" && *at == feet
        )));
        // Again: the chips join the pile lying there, told gone and
        // there again, heavier; the notch deepens.
        let (_, events) = cut_part(&mut world, &rules, (id, flint), (chunk, tree, Part::Trunk));
        assert_eq!(chips(&world), [132]);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ItemRemoved { at, .. } if *at == feet))
        );
        assert!(events.iter().any(|e| matches!(
            e,
            Event::ItemAdded { material, mass_g: 132, .. } if material == "wood_chips"
        )));
        assert_eq!(world.notch_mm2(chunk, tree, &rules), 2640);
        // Another's cuts add to it: the notch is the tree's.
        let other = world.spawn_body(feet);
        let its = picked(&mut world, &rules, other, "flint", 400);
        cut_part(&mut world, &rules, (other, its), (chunk, tree, Part::Trunk));
        assert_eq!(world.notch_mm2(chunk, tree, &rules), 3960);
        // It is kept with the chunk's changes, and the tree still stands.
        assert_eq!(
            world.changes_in(chunk).unwrap().scars[&tree].notch_mm2,
            3960
        );
        assert!(standing(&world, chunk, tree).is_some());
        // A tree with no bark that comes away gives its chips.
        let (mut world, id, chunk, tree) = by_a(&rules, "hazel");
        let flint = picked(&mut world, &rules, id, "flint", 400);
        let (gave, _) = cut_part(&mut world, &rules, (id, flint), (chunk, tree, Part::Trunk));
        assert_eq!(gave.as_deref(), Some("wood_chips"));
    }

    #[test]
    fn a_tree_falls_when_its_notch_has_cut_its_trunk_through_not_before_and_for_good() {
        let rules = tree_rules();
        let parts = rules.needs.hands.trees.clone().unwrap();
        let (mut world, id, chunk, key) = by_a(&rules, "hazel");
        let tree = standing(&world, chunk, key).unwrap();
        let tile = tree.at.tile();
        let trunk_mm = world.trunk_mm(&tree, &rules).unwrap();
        let section = crate::content::TreeParts::section_mm2(trunk_mm);
        let cuts = section.div_ceil(1320);
        assert!((2..=4).contains(&cuts), "a hazel stem: {cuts} cuts");
        let placed = world.shared_ground().trees_placed();
        let flint = picked(&mut world, &rules, id, "flint", 400);
        let mut fell = Vec::new();
        for n in 1..=cuts {
            assert!(world.is_trunk(tile), "it stands before cut {n}");
            assert!(standing(&world, chunk, key).is_some());
            let (_, events) = cut_part(&mut world, &rules, (id, flint), (chunk, key, Part::Trunk));
            fell = events;
            let down = fell.iter().any(|e| matches!(e, Event::TreeFell { .. }));
            assert_eq!(down, n == cuts, "cut {n} of {cuts}");
        }
        // It fell: told, with who, what and how thick.
        let log_g = parts.trunk_g(&rules.species[tree.species], trunk_mm);
        assert!(fell.iter().any(|e| matches!(
            e,
            Event::TreeFell { body, chunk: c, tree: k, at, species, trunk_mm: mm, log_g: g, .. }
                if *body == id && *c == chunk && *k == key && *at == tree.at
                    && species == "hazel" && *mm == trunk_mm && *g == log_g
        )));
        // It stands no more: no tree, a free tile, felled in the changes,
        // its notch gone with it; nothing was placed anew.
        assert!(standing(&world, chunk, key).is_none());
        assert!(!world.is_trunk(tile));
        let changes = world.changes_in(chunk).unwrap();
        assert!(changes.felled.contains(&key) && !changes.scars.contains_key(&key));
        assert_eq!(world.shared_ground().trees_placed(), placed);
        // Where it stood lie its log, of the trunk's mass, and a branch
        // of its crown, of living wood, each told as an item added.
        let lying: Vec<(String, u32)> = world
            .laid_in(tile.chunk())
            .filter(|i| i.pos() == Some(tile))
            .map(|i| (i.material().to_owned(), i.mass_g()))
            .collect();
        assert_eq!(lying.len(), 2, "{lying:?}");
        assert!(lying.contains(&("log".to_owned(), log_g)));
        assert!(lying.contains(&("green_branch".to_owned(), 2000)));
        let added = fell
            .iter()
            .filter(|e| matches!(e, Event::ItemAdded { at, .. } if *at == tile))
            .count();
        assert_eq!(added, 2);
        // A cut of it finds no tree: refused, and nothing comes.
        let mut events = Vec::new();
        world.queue(Intention::Cut {
            body: id,
            with: flint,
            chunk,
            tree: key,
            part: Part::Trunk,
        });
        world.advance(1, &rules, &mut events);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::GestureRefused {
                gesture: Gesture::Cut,
                reason: Refusal::Unknown,
                ..
            }
        )));
        // A body may walk where it stood.
        assert!(world.map().is_passable(tile));
    }

    #[test]
    fn a_big_trunk_leaves_a_log_no_one_carries_and_the_branches_of_its_crown() {
        let rules = tree_rules();
        let (mut world, id, chunk, key) = by_a(&rules, "birch");
        let tree = standing(&world, chunk, key).unwrap();
        let trunk_mm = world.trunk_mm(&tree, &rules).unwrap();
        let section = crate::content::TreeParts::section_mm2(trunk_mm);
        // Its bark lifted and all but the last bite given already, by
        // whoever: the marks stored are the tree's, and the next cut ends
        // it.
        let mut changes = crate::changes::ChunkChanges {
            version: 1,
            ..Default::default()
        };
        changes.scars.insert(
            key,
            crate::changes::Scar {
                notch_mm2: section - 1,
                barked: world.bark_left(chunk, key, &rules).unwrap(),
                day: 0,
                whole_day: 128,
            },
        );
        world.restore_changes(chunk, changes);
        let flint = picked(&mut world, &rules, id, "flint", 400);
        let (_, events) = cut_part(&mut world, &rules, (id, flint), (chunk, key, Part::Trunk));
        // The last cut took what was left: a square millimetre, no chip
        // heavy enough to be a thing.
        assert!(events.iter().any(|e| matches!(
            e,
            Event::TreeBitten { chips_g: 0, notch_mm2, section_mm2, .. } if notch_mm2 == section_mm2
        )));
        let tile = tree.at.tile();
        let lying: Vec<(String, u32)> = world
            .laid_in(tile.chunk())
            .filter(|i| i.pos() == Some(tile))
            .map(|i| (i.material().to_owned(), i.mass_g()))
            .collect();
        let log = lying.iter().find(|(m, _)| m == "log").unwrap().1;
        assert!(
            (160_000..=460_000).contains(&log),
            "a birch's trunk: {log} g"
        );
        assert!(log > body(&world, id).capacity_g(&rules.movement) * 5);
        // Eight branches at most are things.
        assert_eq!(lying.iter().filter(|(m, _)| m == "green_branch").count(), 8);
        // The log is far too heavy to pick up.
        let item = world
            .laid_in(tile.chunk())
            .find(|i| i.material() == "log")
            .unwrap()
            .id();
        let mut events = Vec::new();
        world.queue(Intention::PickUp { body: id, item });
        world.advance(1, &rules, &mut events);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::ActRefused {
                reason: Refusal::TooHeavy,
                ..
            }
        )));
    }

    #[test]
    fn a_world_loaded_with_a_felled_tree_has_it_standing_nowhere() {
        let rules = tree_rules();
        let (mut world, _, chunk, key) = by_a(&rules, "birch");
        let tree = standing(&world, chunk, key).unwrap();
        let before = world.trees_in(chunk).len();
        let mut changes = crate::changes::ChunkChanges {
            version: 3,
            ..Default::default()
        };
        changes.felled.insert(key);
        world.restore_changes(chunk, changes);
        assert_eq!(world.trees_in(chunk).len(), before - 1);
        assert!(standing(&world, chunk, key).is_none());
        assert!(!world.is_trunk(tree.at.tile()));
    }

    /// `id` lifts with `with` all the bark the trunk of `tree` wears
    /// where the hands reach, laying each piece down; how many pieces.
    fn strip(
        world: &mut World,
        rules: &Content,
        (id, with): (BodyId, ItemId),
        (chunk, tree): (ChunkPos, TreeKey),
    ) -> u32 {
        let pieces = world.bark_left(chunk, tree, rules).unwrap();
        for left in (0..pieces).rev() {
            let (gave, events) = cut_part(world, rules, (id, with), (chunk, tree, Part::Trunk));
            assert!(gave.is_some_and(|g| g.ends_with("bark")));
            assert!(events.iter().any(|e| matches!(
                e,
                Event::BarkLifted { body, chunk: c, tree: k, left: l, .. }
                    if *body == id && *c == chunk && *k == tree && *l == left
            )));
            assert!(!events.iter().any(|e| matches!(e, Event::TreeBitten { .. })));
        }
        pieces
    }

    /// `days` journées go by, nothing done.
    fn days_pass(world: &mut World, days: u64) {
        world.tick.0 += days * world.calendar.ticks_per_day();
    }

    #[test]
    fn a_trunk_wears_so_many_pieces_of_bark_by_its_girth_and_lifting_them_never_notches_it() {
        let rules = tree_rules();
        let parts = rules.needs.hands.trees.clone().unwrap();
        let stuff = |id: &str| rules.material(id).unwrap();
        // Its girth times the metre the hands reach, over a piece's
        // area: 4 pieces on a birch of 22 cm, 10 on an oak of 55 cm, one
        // at least on the thinnest stem.
        assert_eq!(parts.bark_plates(220, stuff("birch_bark")), 4);
        assert_eq!(parts.bark_plates(550, stuff("oak_bark")), 10);
        assert_eq!(parts.bark_plates(10, stuff("oak_bark")), 1);
        // Keen flint, that bites wood, lifts every piece and bites
        // nothing: no chip, no notch, the tree standing.
        let (mut world, id, chunk, tree) = by_a(&rules, "birch");
        let trunk_mm = world
            .trunk_mm(&standing(&world, chunk, tree).unwrap(), &rules)
            .unwrap();
        let pieces = parts.bark_plates(trunk_mm, stuff("birch_bark"));
        assert!((3..=5).contains(&pieces), "a birch: {pieces} pieces");
        assert_eq!(world.bark_left(chunk, tree, &rules), Some(pieces));
        let flint = picked(&mut world, &rules, id, "flint", 400);
        assert_eq!(
            strip(&mut world, &rules, (id, flint), (chunk, tree)),
            pieces
        );
        assert_eq!(world.notch_mm2(chunk, tree, &rules), 0);
        assert_eq!(world.bark_left(chunk, tree, &rules), Some(0));
        let scar = world.changes_in(chunk).unwrap().scars[&tree];
        assert_eq!((scar.notch_mm2, scar.barked), (0, pieces));
        assert!(standing(&world, chunk, tree).is_some());
        let feet = body(&world, id).pos();
        assert!(
            world
                .laid_in(feet.chunk())
                .all(|i| i.material() != "wood_chips")
        );
        // A kind whose bark does not come away wears none.
        let (world, _, chunk, tree) = by_a(&rules, "hazel");
        assert_eq!(world.bark_left(chunk, tree, &rules), Some(0));
        // Without parts a trunk is not measured: bark without end.
        let plain = self::rules();
        let (mut world, id, chunk, tree) = by_a(&plain, "birch");
        assert_eq!(world.bark_left(chunk, tree, &plain), None);
        let flint = picked(&mut world, &plain, id, "flint", 400);
        for _ in 0..8 {
            let (gave, _) = cut_part(&mut world, &plain, (id, flint), (chunk, tree, Part::Trunk));
            assert_eq!(gave.as_deref(), Some("birch_bark"));
        }
        assert!(world.changes_in(chunk).is_none_or(|c| c.scars.is_empty()));
    }

    #[test]
    fn the_wood_is_cut_only_where_the_bark_is_gone() {
        let rules = tree_rules();
        let (mut world, id, chunk, tree) = by_a(&rules, "birch");
        // Quartz lifts the birch's bark, piece after piece.
        let quartz = picked(&mut world, &rules, id, "quartz", 400);
        let (gave, events) = cut_part(&mut world, &rules, (id, quartz), (chunk, tree, Part::Trunk));
        assert_eq!(gave.as_deref(), Some("birch_bark"));
        assert!(!events.iter().any(|e| matches!(e, Event::TreeBitten { .. })));
        // While a piece is left, flint too gives bark and no wood.
        let flint = picked(&mut world, &rules, id, "flint", 400);
        assert!(world.bark_left(chunk, tree, &rules).unwrap() >= 1);
        strip(&mut world, &rules, (id, flint), (chunk, tree));
        assert_eq!(world.notch_mm2(chunk, tree, &rules), 0);
        // The trunk bare, the same cut is answered by the wood: nothing
        // to quartz, too blunt for it; chips and a notch to flint.
        let (gave, events) = cut_part(&mut world, &rules, (id, quartz), (chunk, tree, Part::Trunk));
        assert_eq!(gave, None);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::TreeBitten { .. } | Event::BarkLifted { .. }))
        );
        let (gave, events) = cut_part(&mut world, &rules, (id, flint), (chunk, tree, Part::Trunk));
        assert_eq!(gave.as_deref(), Some("wood_chips"));
        assert!(events.iter().any(|e| matches!(e, Event::TreeBitten { .. })));
        assert!(!events.iter().any(|e| matches!(e, Event::BarkLifted { .. })));
        assert_eq!(world.notch_mm2(chunk, tree, &rules), 1320);
        // An oak's bark is too tough for quartz: under a bark that does
        // not come away to it, no edge reaches the wood.
        let (mut world, id, chunk, tree) = by_a(&rules, "oak");
        let quartz = picked(&mut world, &rules, id, "quartz", 400);
        let (gave, _) = cut_part(&mut world, &rules, (id, quartz), (chunk, tree, Part::Trunk));
        assert_eq!(gave, None);
        assert!(world.changes_in(chunk).is_none_or(|c| c.scars.is_empty()));
    }

    #[test]
    fn a_notch_nobody_deepens_closes_and_the_bark_grows_back_with_the_seasons() {
        let rules = tree_rules();
        let (mut world, id, chunk, tree) = by_a(&rules, "birch");
        let section = crate::content::TreeParts::section_mm2(
            world
                .trunk_mm(&standing(&world, chunk, tree).unwrap(), &rules)
                .unwrap(),
        );
        let flint = picked(&mut world, &rules, id, "flint", 400);
        let pieces = strip(&mut world, &rules, (id, flint), (chunk, tree));
        for _ in 0..6 {
            cut_part(&mut world, &rules, (id, flint), (chunk, tree, Part::Trunk));
        }
        assert_eq!(world.notch_mm2(chunk, tree, &rules), 6 * 1320);
        let stored = world.changes_in(chunk).unwrap().clone();
        // A night: the notch has lost the trunk's section over the 128
        // journées of eight seasons, a fifth of a bite or so; a felling
        // of several journées goes on from there.
        days_pass(&mut world, 1);
        let night = u32::try_from(u64::from(section) / 128).unwrap();
        assert!((150..=500).contains(&night), "{night} mm² a night");
        assert_eq!(world.notch_mm2(chunk, tree, &rules), 6 * 1320 - night);
        assert_eq!(world.bark_left(chunk, tree, &rules), Some(0));
        // Nothing was written by looking: what is kept is the marks as
        // they were made.
        assert_eq!(world.changes_in(chunk), Some(&stored));
        // The bark comes back piece by piece over eight seasons, the
        // notch closes in proportion to what was cut.
        days_pass(&mut world, 63);
        let half = u32::try_from(u64::from(section) * 64 / 128).unwrap();
        assert_eq!(
            world.notch_mm2(chunk, tree, &rules),
            (6 * 1320_u32).saturating_sub(half)
        );
        assert_eq!(world.bark_left(chunk, tree, &rules), Some(pieces / 2));
        days_pass(&mut world, 63);
        assert_eq!(world.bark_left(chunk, tree, &rules), Some(pieces - 1));
        assert_eq!(stored.scars[&tree].whole_day, 128);
        days_pass(&mut world, 1);
        assert_eq!(world.notch_mm2(chunk, tree, &rules), 0);
        assert_eq!(world.bark_left(chunk, tree, &rules), Some(pieces));
        // Whole again, the same cut gives bark again and bites nothing.
        let flint = picked(&mut world, &rules, id, "flint", 400);
        let (gave, events) = cut_part(&mut world, &rules, (id, flint), (chunk, tree, Part::Trunk));
        assert_eq!(gave.as_deref(), Some("birch_bark"));
        assert!(!events.iter().any(|e| matches!(e, Event::TreeBitten { .. })));
        let scar = world.changes_in(chunk).unwrap().scars[&tree];
        assert_eq!((scar.notch_mm2, scar.barked, scar.day), (0, 1, 128));
    }

    #[test]
    fn a_cut_dates_the_marks_from_what_healing_left_of_them() {
        let rules = tree_rules();
        let (mut world, id, chunk, tree) = by_a(&rules, "birch");
        let flint = picked(&mut world, &rules, id, "flint", 400);
        strip(&mut world, &rules, (id, flint), (chunk, tree));
        cut_part(&mut world, &rules, (id, flint), (chunk, tree, Part::Trunk));
        cut_part(&mut world, &rules, (id, flint), (chunk, tree, Part::Trunk));
        days_pass(&mut world, 2);
        let left = world.notch_mm2(chunk, tree, &rules);
        assert!(left > 0 && left < 2640);
        let flint = picked(&mut world, &rules, id, "flint", 400);
        cut_part(&mut world, &rules, (id, flint), (chunk, tree, Part::Trunk));
        let scar = world.changes_in(chunk).unwrap().scars[&tree];
        assert_eq!((scar.notch_mm2, scar.day), (left + 1320, 2));
        assert_eq!(world.notch_mm2(chunk, tree, &rules), left + 1320);
    }

    #[test]
    fn a_healed_tree_is_forgotten_when_its_chunk_changes_or_is_loaded() {
        let rules = tree_rules();
        let (mut world, id, chunk, tree) = by_a(&rules, "birch");
        let pieces = world.bark_left(chunk, tree, &rules).unwrap();
        let flint = picked(&mut world, &rules, id, "flint", 400);
        let (gave, _) = cut_part(&mut world, &rules, (id, flint), (chunk, tree, Part::Trunk));
        assert_eq!(gave.as_deref(), Some("birch_bark"));
        let stored = world.changes_in(chunk).unwrap().clone();
        let whole = stored.scars[&tree].whole_day;
        assert!(
            (16..=64).contains(&whole),
            "a piece of bark: {whole} journées"
        );
        // The journée before, it is still kept, loaded or not.
        days_pass(&mut world, whole - 1);
        world.restore_changes(chunk, stored.clone());
        assert_eq!(world.changes_in(chunk), Some(&stored));
        assert_eq!(world.bark_left(chunk, tree, &rules), Some(pieces - 1));
        // Healed, it is a tree among the others to whoever looks, though
        // nothing passed over the records...
        days_pass(&mut world, 1);
        assert_eq!(world.changes_in(chunk), Some(&stored));
        assert_eq!(world.bark_left(chunk, tree, &rules), Some(pieces));
        // ...and its record goes when the chunk is loaded, the version
        // kept...
        let mut loaded = world.clone();
        loaded.restore_changes(chunk, stored.clone());
        let back = loaded.changes_in(chunk).unwrap();
        assert!(back.scars.is_empty());
        assert_eq!(back.version, stored.version);
        // ...or when men next change the chunk: here another tree of it
        // marked.
        let other = world
            .trees_in(chunk)
            .iter()
            .map(|t| t.key)
            .find(|k| *k != tree)
            .expect("another tree in the chunk");
        world.mark(
            chunk,
            other,
            Trunk {
                trunk_mm: 100,
                section_mm2: 7853,
                plates: 0,
                notch_mm2: 1320,
                barked: 0,
            },
            &rules,
        );
        let scars = &world.changes_in(chunk).unwrap().scars;
        assert!(!scars.contains_key(&tree) && scars.contains_key(&other));
        // The changes forget by their own dates alone.
        let mut changes = stored;
        assert!(!changes.forget_healed(whole - 1));
        assert!(changes.forget_healed(whole) && changes.scars.is_empty());
    }

    #[test]
    fn a_felled_tree_stays_felled_whatever_the_seasons() {
        let rules = tree_rules();
        let (mut world, id, chunk, key) = by_a(&rules, "hazel");
        let flint = picked(&mut world, &rules, id, "flint", 400);
        for _ in 0..8 {
            if standing(&world, chunk, key).is_none() {
                break;
            }
            cut_part(&mut world, &rules, (id, flint), (chunk, key, Part::Trunk));
        }
        assert!(standing(&world, chunk, key).is_none());
        days_pass(&mut world, 64 * 50);
        assert!(standing(&world, chunk, key).is_none());
        assert_eq!(world.notch_mm2(chunk, key, &rules), 0);
        assert_eq!(world.bark_left(chunk, key, &rules), None);
        let stored = world.changes_in(chunk).unwrap().clone();
        world.restore_changes(chunk, stored);
        let changes = world.changes_in(chunk).unwrap();
        assert!(changes.felled.contains(&key) && changes.scars.is_empty());
        assert!(standing(&world, chunk, key).is_none());
    }

    #[test]
    fn the_marks_and_their_healing_are_the_same_on_every_run_looked_at_or_not() {
        let rules = tree_rules();
        let run = |look: bool| {
            let (mut world, id, chunk, tree) = by_a(&rules, "birch");
            let mut told = Vec::new();
            for day in 0..6 {
                let flint = picked(&mut world, &rules, id, "flint", 400);
                for _ in 0..3 {
                    let (gave, events) =
                        cut_part(&mut world, &rules, (id, flint), (chunk, tree, Part::Trunk));
                    told.push((day, gave, events));
                    if look {
                        let _ = world.notch_mm2(chunk, tree, &rules);
                        let _ = world.bark_left(chunk, tree, &rules);
                    }
                }
                days_pass(&mut world, 3);
            }
            let trunk = world.trunk_now(chunk, &standing(&world, chunk, tree).unwrap(), &rules);
            (world.changes_in(chunk).cloned(), trunk, told)
        };
        let first = run(false);
        assert!(first.1.is_some_and(|t| t.notch_mm2 > 0));
        assert_eq!(first, run(false));
        assert_eq!(first, run(true));
    }
}
