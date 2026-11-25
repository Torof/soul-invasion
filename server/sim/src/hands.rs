//! The gestures of the hands (D57; the project lead, 24 November 2026:
//! "C'est parfait"): picking up (the act of D35), cutting a tree with what
//! is in hand, folding what is carried. No recipe: what comes of a
//! gesture is what the materials are (`sharp`, `folds_into`, a tree's
//! `bark`, `content/`), and a gesture that meets nothing to work on still
//! takes its time and gives nothing.

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
    /// Pick up a thing lying beside (D35's act).
    #[default]
    Take,
    /// Cut a tree with what is in hand.
    Cut,
    /// Fold a thing carried.
    Fold,
}

impl Gesture {
    /// Every gesture, in a fixed order.
    pub const ALL: [Gesture; 3] = [Gesture::Take, Gesture::Cut, Gesture::Fold];

    /// Its name, as the vivarium writes it.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Gesture::Take => "take",
            Gesture::Cut => "cut",
            Gesture::Fold => "fold",
        }
    }
}

/// A gesture under way (D57): the body is busy at it until `until`; a
/// walk, sleep or any other intention ends it with nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Making {
    /// The gesture.
    pub gesture: Gesture,
    /// The tool it cuts with, or the thing it folds.
    pub item: ItemId,
    /// The tree it cuts, when it cuts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<(ChunkPos, TreeKey)>,
    /// When it is done.
    pub until: Tick,
}

impl World {
    /// The things lying within `tiles` of `pos` on each axis, stored and
    /// natural, in no order (D57): what a body sees around it to try a
    /// gesture on. The natural ones are found tile by tile, not chunk by
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
                tree: None,
                until,
            },
            events,
        );
    }

    /// Moves every gesture on (D57): one whose body walks or sleeps ends
    /// with nothing; one whose time is up ends with what the materials
    /// give. A cut with a sharp thing in hand at a tree with a bark gives
    /// a piece of that bark, on the body (on the ground when too heavy for
    /// it); a fold of a thing that folds turns it into what it folds
    /// into, made now. Anything else gives nothing.
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
            let gave = match making.gesture {
                Gesture::Cut => self.cut_gives(index, &making, rules, events),
                Gesture::Fold => self.fold_gives(index, making.item, rules, events),
                Gesture::Take => None,
            };
            events.push(Event::Gestured {
                body: self.bodies[index].id,
                gesture: making.gesture,
                item: making.item,
                gave,
            });
        }
    }

    /// What a cut gives: the tree's bark, when the thing in hand is sharp.
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
        let tool = self.item(making.item)?;
        if !rules.material(&tool.material).is_some_and(|m| m.sharp) {
            return None;
        }
        let tree = self
            .trees_in(chunk)
            .iter()
            .find(|t| t.key == key)
            .copied()?;
        let bark = self
            .species
            .get(tree.species)
            .and_then(|s| s.bark.clone())?;
        let mass_g = rules.material(&bark)?.mass_g().max(1);
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
            },
        );
        self.bodies[index].carried.push(item);
        self.inventory_changed(index, rules, events);
        Some(item)
    }

    /// What a fold gives: the thing itself, turned into what it folds
    /// into, made now; nothing when it does not fold.
    fn fold_gives(
        &mut self,
        index: usize,
        item: ItemId,
        rules: &Content,
        events: &mut Vec<Event>,
    ) -> Option<ItemId> {
        if !self.carries(index, item) {
            return None;
        }
        let into = rules
            .material(&self.item(item)?.material)?
            .folds_into
            .clone()?;
        let now = self.tick;
        let folded = self.items.get_mut(&item)?;
        folded.material = into;
        folded.made = now;
        self.inventory_changed(index, rules, events);
        Some(item)
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
        let mut world = World::with_valley(Calendar::new(1, 1440).unwrap(), rules, 768);
        let birch = rules.species.iter().position(|s| s.id == "birch").unwrap();
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
        panic!("a birch with room beside it");
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
        let mut events = Vec::new();
        for _ in 0..30 {
            events.clear();
            world.advance(1, rules, &mut events);
            for event in &events {
                if let Event::Gestured { body, gave, .. } = event
                    && *body == id
                {
                    return *gave;
                }
            }
        }
        panic!("no gesture ended");
    }

    #[test]
    fn a_flint_cuts_a_birchs_bark_and_the_bark_folds_into_a_container() {
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
        let arms = body(&world, id).food_capacity_g(&rules.movement);
        assert_eq!(arms, 1000);
        world.queue(Intention::Fold {
            body: id,
            item: bark,
        });
        let folded = gesture_ends(&mut world, &rules, id).expect("the bark folded");
        assert_eq!(folded, bark, "the same thing, folded");
        assert_eq!(world.item(bark).unwrap().material(), "folded_birch_bark");
        assert_eq!(
            body(&world, id).food_capacity_g(&rules.movement),
            1000 + 4000,
            "4 kg of loose food more"
        );
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
            .find(|m| m.id == "folded_birch_bark")
            .unwrap();
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
        assert_eq!(body(&world, id).food_capacity_g(&rules.movement), 5000);
        let mut events = Vec::new();
        world.advance(1440 * 2, &rules, &mut events);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Spoiled { item, material, holder: Some(b), .. }
                if *item == bark && material == "folded_birch_bark" && *b == id
        )));
        assert!(world.item(bark).is_none());
        assert_eq!(body(&world, id).food_capacity_g(&rules.movement), 1000);
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
}
