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
}

impl Gesture {
    /// Every gesture, in a fixed order.
    pub const ALL: [Gesture; 4] = [Gesture::PickUp, Gesture::Cut, Gesture::Fold, Gesture::PutIn];

    /// Its name, as the vivarium writes it.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Gesture::PickUp => "pick up",
            Gesture::Cut => "cut",
            Gesture::Fold => "fold",
            Gesture::PutIn => "put in",
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

    /// `id` cuts `tree` with `with` (D57): it takes `with` in hand, the
    /// thing it held going on it, and is busy the content's minutes.
    pub(crate) fn cut(
        &mut self,
        id: BodyId,
        with: ItemId,
        (chunk, tree): (ChunkPos, TreeKey),
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
                until,
            },
            events,
        );
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
    /// give. A cut gives a piece of the tree's bark, on the body (on the
    /// ground when too heavy for it), when the thing in hand works it
    /// loose; a fold gives the thing itself a hollow, made now, breaks
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
                Gesture::Fold | Gesture::PickUp | Gesture::PutIn => {
                    self.item(making.item).map(|i| i.material.clone())
                }
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
                Gesture::PickUp => (None, false),
            };
            events.push(Event::Gestured {
                body: self.bodies[index].id,
                gesture: making.gesture,
                item: making.item,
                on,
                into: making.into,
                gave,
                broke,
            });
        }
    }

    /// What a cut gives: the tree's bark, when the thing in hand has an
    /// edge keen enough for it and is the harder ([`Content::cut_gives`]).
    fn cut_gives(
        &mut self,
        index: usize,
        making: &Making,
        rules: &Content,
        events: &mut Vec<Event>,
    ) -> Option<ItemId> {
        let (chunk, key) = making.tree?;
        let body = &self.bodies[index];
        if body.held != Some(making.item) {
            return None;
        }
        let tool = rules.material(&self.item(making.item)?.material)?;
        let tree = self
            .trees_in(chunk)
            .iter()
            .find(|t| t.key == key)
            .copied()?;
        let bark = rules.cut_gives(tool, self.species.get(tree.species)?)?;
        let mass_g = bark.mass_g().max(1);
        let bark = bark.id.clone();
        let (id, pos) = (body.id, body.pos());
        if body.load_g.saturating_add(mass_g) > body.capacity_g(&rules.movement) {
            return Some(self.spawn_item(&bark, mass_g, pos));
        }
        let item = ItemId(self.next_item);
        self.next_item += 1;
        self.items.insert(
            item,
            Item {
                id: item,
                material: bark,
                mass_g,
                made: self.tick,
                place: Place::Carried { body: id },
                hollow_ml: 0,
                within: None,
            },
        );
        self.bodies[index].carried.push(item);
        self.inventory_changed(index, rules, events);
        Some(item)
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
}
