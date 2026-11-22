//! The trees and plants bearing a crop, placed **ahead** of the bodies on
//! helper threads (D51, amended): a chunk's bearers cost a few
//! milliseconds to place ([`flora::plants_in`] over the trees of nine
//! chunks), once a chunk, and the brains asked for them on the simulation
//! thread the moment a body looked at new land. The placing is a pure
//! function of the map, the seed and the content, which never change under
//! a world ([`Placing`]); its result is memoised in the world's shared
//! [`Ground`], where the server's threads already read it (D29). So a
//! helper thread that is told where the bodies stand and where they walk
//! ([`Placer::follow`]) can place the chunks around them first, and the
//! simulation thread, asking the same cache, finds them placed: **the same
//! bearers, placed ahead or placed now**, by construction. A helper that
//! is late changes nothing but the time the thought takes; the simulation
//! thread places the chunk itself, as before.
//!
//! The helpers are workers of the one simulation thread (D29), not a second
//! simulation: they decide nothing, write nothing a rule reads, and the
//! simulation never waits for them. Plain `std::thread`s, a channel of
//! hints the simulation never blocks on, no dependency. The ground's caches
//! are the caller's to size ([`Ground::widen`]): the helpers fill them with
//! the land around the bodies, more than four views' worth on a long run.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::JoinHandle;

use crate::content::{Content, Species};
use crate::crops::{self, Bearer, Crop};
use crate::flora;
use crate::ground::Ground;
use crate::map::{CHUNK_SIZE, ChunkPos, FinePos, Map, Pos};
use crate::open_ground::OpenGround;
use crate::trees::Tree;
use crate::{Body, World};

/// How many chunks beyond a body's sight the helpers place: a chunk's
/// bearers cost a few milliseconds, the time a body walks a hundred tiles
/// at the vivarium's cadence, so the helpers must work two chunks ahead
/// of a body's sight to be done when it looks.
pub const LEAD_CHUNKS: i32 = 2;

/// Hints held in the channel at most: the helpers read the newest and
/// drop the rest; a full channel drops the hint, never blocks.
const HINTS_HELD: usize = 4;

/// What placing a chunk's bearers reads: the map, the seed, the content's
/// species, open biomes, flora and crops, and the ground where the trees
/// are held. All immutable under a world (the content is taken once,
/// [`World::take_content`]; the map never changes), so the same placing
/// from any thread. Owned, so a helper thread holds its own (`Parts`
/// borrows the same from a world, for the simulation thread).
#[derive(Debug, Clone)]
pub struct Placing {
    map: Arc<Map>,
    species: Vec<Species>,
    open: OpenGround,
    flora: Vec<flora::Kind>,
    crops: Vec<Crop>,
    seed: u64,
    ground: Arc<Ground>,
}

impl Placing {
    /// The placing of `world`'s bearers: its map, seed, content and
    /// ground, shared or copied once.
    #[must_use]
    pub fn of(world: &World) -> Self {
        Self {
            map: world.shared_map(),
            species: world.species.clone(),
            open: world.open_ground.clone(),
            flora: world.flora.clone(),
            crops: world.crops.clone(),
            seed: world.seed(),
            ground: world.shared_ground(),
        }
    }

    /// The same, borrowed.
    fn parts(&self) -> Parts<'_> {
        Parts {
            map: &self.map,
            species: &self.species,
            open: &self.open,
            flora: &self.flora,
            crops: &self.crops,
            seed: self.seed,
            ground: &self.ground,
        }
    }

    /// Places `chunk`'s bearers into the ground if they are not held yet:
    /// what a helper does. `true` if it placed them.
    fn place_ahead(&self, chunk: ChunkPos) -> bool {
        if !self.map.has_chunk(chunk) || self.ground.holds_bearers(chunk) {
            return false;
        }
        let parts = self.parts();
        self.ground.bearers_in(chunk, || parts.place(chunk));
        true
    }
}

/// What placing a chunk's bearers reads, borrowed from a world
/// ([`World::placing`]) or from a helper's [`Placing`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Parts<'a> {
    pub(crate) map: &'a Map,
    pub(crate) species: &'a [Species],
    pub(crate) open: &'a OpenGround,
    pub(crate) flora: &'a [flora::Kind],
    pub(crate) crops: &'a [Crop],
    pub(crate) seed: u64,
    pub(crate) ground: &'a Ground,
}

impl Parts<'_> {
    /// The trees standing in `chunk`, placed from the map and the seed
    /// the first time they are asked for and held ([`Ground::trees_in`]).
    fn trees_in(&self, chunk: ChunkPos) -> Arc<[Tree]> {
        self.ground
            .trees_in(self.map, self.species, self.open, self.seed, chunk)
    }

    /// The trees and plants of `chunk` bearing a crop, placed now: its
    /// trees, then its plants, in the order they are placed, the same
    /// every time. Each kind's name is made once and shared by its
    /// bearers; the trees around are read where the ground holds them.
    pub(crate) fn place(&self, chunk: ChunkPos) -> Vec<Bearer> {
        let bears = |id: &str| self.crops.iter().any(|c| c.on == id);
        // A kind's shared name, `None` for one bearing nothing.
        let named = |id: &str| bears(id).then(|| Arc::<str>::from(id));
        let species: Vec<Option<Arc<str>>> =
            self.species.iter().map(|kind| named(&kind.id)).collect();
        let mut out: Vec<Bearer> = self
            .trees_in(chunk)
            .iter()
            .filter_map(|tree| {
                Some(Bearer {
                    source: crops::Source::Tree(tree.key),
                    at: tree.at,
                    kind: Arc::clone(species[tree.species].as_ref()?),
                })
            })
            .collect();
        let flora: Vec<Option<Arc<str>>> = self.flora.iter().map(|kind| named(&kind.id)).collect();
        if flora.iter().all(Option::is_none) {
            return out;
        }
        let around: Vec<Arc<[Tree]>> = (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| ChunkPos::new(chunk.x + dx, chunk.y + dy)))
            .map(|near| self.trees_in(near))
            .collect();
        let near: Vec<flora::TreeAt<'_>> = around
            .iter()
            .flat_map(|trees| trees.iter())
            .map(|tree| flora::TreeAt {
                at: tree.at,
                species: self.species[tree.species].id.as_str(),
            })
            .collect();
        for plant in flora::plants_in(self.map, self.flora, &near, self.seed, chunk) {
            let Some(kind) = &flora[usize::from(plant.kind)] else {
                continue;
            };
            if let Some(key) = crops::PlantKey::of(chunk, plant.at) {
                out.push(Bearer {
                    source: crops::Source::Plant(key),
                    at: plant.at,
                    kind: Arc::clone(kind),
                });
            }
        }
        out
    }
}

/// Where a body stands and goes, as a hint to the helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    /// The tile the body stands on.
    pub at: Pos,
    /// Where its walk ends, if it walks.
    pub to: Option<Pos>,
    /// How far it sees from there, in tiles: what its next look reads.
    pub sight: i32,
}

impl Step {
    /// A body's step: where it stands, where its route ends, how far it
    /// sees plants from its tile: the terrain's sight, no farther than
    /// `plant_sight_tiles` (D53), as a look reads them.
    fn of(world: &World, rules: &Content, body: &Body) -> Self {
        let at = body.pos();
        let sight = world
            .map()
            .get(at)
            .map_or(1, |terrain| rules.movement.sight_tiles(terrain))
            .min(rules.needs.thought.plant_sight_tiles);
        Self {
            at,
            to: body.route().last().map(FinePos::tile),
            sight: i32::try_from(sight).unwrap_or(1),
        }
    }
}

/// The chunks the bodies of `steps` may look at soon, nearest to a body
/// first (then by row and column), each once, on the map or not: the
/// square of each body's sight widened by [`LEAD_CHUNKS`] chunks, and the
/// square of its sight around where it walks to. Pure, so a test reads
/// the order.
#[must_use]
pub fn ahead_of(steps: &[Step]) -> Vec<ChunkPos> {
    let side = i32::try_from(CHUNK_SIZE).expect("small chunk side");
    let mut nearest: BTreeMap<ChunkPos, i32> = BTreeMap::new();
    let mut note = |low: Pos, high: Pos, from: ChunkPos| {
        let (low, high) = (low.chunk(), high.chunk());
        for cy in low.y..=high.y {
            for cx in low.x..=high.x {
                let chunk = ChunkPos::new(cx, cy);
                let far = (cx - from.x).abs().max((cy - from.y).abs());
                nearest
                    .entry(chunk)
                    .and_modify(|d| *d = (*d).min(far))
                    .or_insert(far);
            }
        }
    };
    for step in steps {
        let reach = step.sight + LEAD_CHUNKS * side;
        let own = step.at.chunk();
        note(
            Pos::new(step.at.x - reach, step.at.y - reach),
            Pos::new(step.at.x + reach, step.at.y + reach),
            own,
        );
        if let Some(to) = step.to {
            note(
                Pos::new(to.x - step.sight, to.y - step.sight),
                Pos::new(to.x + step.sight, to.y + step.sight),
                own,
            );
        }
    }
    let mut chunks: Vec<(i32, ChunkPos)> = nearest.into_iter().map(|(c, d)| (d, c)).collect();
    chunks.sort_unstable_by_key(|&(far, c)| (far, c.y, c.x));
    chunks.into_iter().map(|(_, c)| c).collect()
}

/// The helper threads placing bearers ahead of the bodies. Made once a
/// run ([`Placer::spawn`]), told where the bodies are now and then
/// ([`Placer::follow`]), joined when dropped. Each helper places every
/// `n`th chunk of the hint's order, so `n` helpers share the work without
/// a queue between them.
#[derive(Debug)]
pub struct Placer {
    /// One channel a helper, the same hint sent to each; `None` once
    /// dropped, which ends the helpers.
    hints: Vec<SyncSender<Arc<[Step]>>>,
    threads: Vec<JoinHandle<()>>,
    /// Chunks the helpers placed, in all.
    placed: Arc<AtomicU64>,
}

impl Placer {
    /// `helpers` threads placing `world`'s bearers ahead, none for 0.
    #[must_use]
    pub fn spawn(world: &World, helpers: usize) -> Option<Self> {
        if helpers == 0 {
            return None;
        }
        let placing = Placing::of(world);
        let placed = Arc::new(AtomicU64::new(0));
        let (hints, threads): (Vec<_>, Vec<_>) = (0..helpers)
            .map(|k| {
                let (tx, rx) = mpsc::sync_channel::<Arc<[Step]>>(HINTS_HELD);
                let placing = placing.clone();
                let placed = Arc::clone(&placed);
                let thread = std::thread::Builder::new()
                    .name(format!("bearers-ahead-{k}"))
                    .spawn(move || helper(&placing, &rx, (k, helpers), &placed))
                    .expect("a helper thread");
                (tx, thread)
            })
            .unzip();
        Some(Self {
            hints,
            threads,
            placed,
        })
    }

    /// Tells the helpers where `world`'s bodies stand and walk to, so
    /// they place the chunks around them. Never blocks: a hint the
    /// helpers have not read yet is dropped, the next will tell more.
    pub fn follow(&self, world: &World, rules: &Content) {
        let steps: Arc<[Step]> = world
            .bodies()
            .iter()
            .map(|body| Step::of(world, rules, body))
            .collect();
        for hint in &self.hints {
            match hint.try_send(Arc::clone(&steps)) {
                Ok(()) | Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
            }
        }
    }

    /// Chunks the helpers placed so far.
    #[must_use]
    pub fn placed(&self) -> u64 {
        self.placed.load(Ordering::Relaxed)
    }
}

impl Drop for Placer {
    fn drop(&mut self) {
        // Closing the channels ends the helpers after the chunk in hand.
        self.hints.clear();
        for thread in self.threads.drain(..) {
            // A helper that panicked has nothing to tell: its cache
            // entries are whole (one insert each), and the simulation
            // thread places what is missing itself.
            let _ = thread.join();
        }
    }
}

/// A helper's life: wait for a hint, place the chunks it names that are
/// not held yet, nearest first, its own `k`th of every `n`; read the
/// newest hint between two chunks, so it follows the bodies rather than
/// finishing an old square. The chunks it found held or placed itself are
/// noted, so a hint costs no lock for the land already placed (the
/// ground's lock is the simulation thread's too). Ends when the channel
/// closes.
fn helper(
    placing: &Placing,
    hints: &Receiver<Arc<[Step]>>,
    (k, n): (usize, usize),
    placed: &AtomicU64,
) {
    let mut next: Option<Arc<[Step]>> = None;
    let mut known: BTreeSet<ChunkPos> = BTreeSet::new();
    loop {
        let Ok(mut steps) = next.take().map_or_else(|| hints.recv(), Ok) else {
            return;
        };
        // The newest hint, the older ones dropped.
        loop {
            match hints.try_recv() {
                Ok(newer) => steps = newer,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        for chunk in ahead_of(&steps).into_iter().skip(k).step_by(n) {
            if known.contains(&chunk) {
                continue;
            }
            match hints.try_recv() {
                Ok(newer) => {
                    next = Some(newer);
                    break;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => return,
            }
            if placing.place_ahead(chunk) {
                placed.fetch_add(1, Ordering::Relaxed);
            }
            known.insert(chunk);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crops::tests::{calendar, content};

    /// Placed ahead or placed now, the same bearers: a helper fills one
    /// world's ground around a body; another world, no helper, places
    /// the same chunks itself; both read the same bearers, and the first
    /// world's `bearers_in` copies nothing (a hit shares the helper's).
    #[test]
    fn placed_ahead_is_placed_the_same() {
        let rules = content();
        let ahead = World::with_valley(calendar(), &rules, 256);
        let alone = World::with_valley(calendar(), &rules, 256);
        let placer = Placer::spawn(&ahead, 2).expect("two helpers");
        let steps: Arc<[Step]> = Arc::from([Step {
            at: Pos::new(128, 128),
            to: Some(Pos::new(200, 100)),
            sight: 25,
        }]);
        for hint in &placer.hints {
            hint.send(Arc::clone(&steps)).unwrap();
        }
        let on_map: Vec<ChunkPos> = ahead_of(&steps)
            .into_iter()
            .filter(|c| ahead.map().has_chunk(*c))
            .collect();
        assert!(on_map.len() > 9, "the valley holds the chunks around");
        // The helpers end at once when dropped, so wait for them to be
        // done first (a few chunks each, well under a second).
        let ground = ahead.shared_ground();
        let patience = std::time::Instant::now();
        while !on_map.iter().all(|c| ground.holds_bearers(*c)) {
            assert!(
                patience.elapsed().as_secs() < 30,
                "the helpers never placed them all"
            );
            std::thread::yield_now();
        }
        assert_eq!(
            placer.placed(),
            u64::try_from(on_map.len()).unwrap(),
            "each chunk placed once, by one helper"
        );
        drop(placer);
        for chunk in &on_map {
            let before = ahead.bearers_in(*chunk);
            let again = ahead.bearers_in(*chunk);
            assert!(Arc::ptr_eq(&before, &again), "a hit shares the helper's");
            assert_eq!(
                *before,
                *alone.bearers_in(*chunk),
                "the same bearers at {chunk:?}"
            );
        }
    }

    #[test]
    fn no_helper_for_zero_and_a_hint_never_blocks() {
        let rules = content();
        let world = World::with_valley(calendar(), &rules, 64);
        assert!(Placer::spawn(&world, 0).is_none());
        let placer = Placer::spawn(&world, 1).unwrap();
        for _ in 0..(HINTS_HELD * 3) {
            placer.follow(&world, &rules);
        }
        drop(placer);
    }

    fn step(x: i32, y: i32, sight: i32) -> Step {
        Step {
            at: Pos::new(x, y),
            to: None,
            sight,
        }
    }

    #[test]
    fn the_chunks_ahead_come_nearest_first_each_once() {
        // A body at (100, 100), in chunk (3, 3), seeing 20 tiles: its
        // sight widened by two chunks reaches 84 tiles, 16..=184, chunks
        // 0..=5: 36 chunks.
        let chunks = ahead_of(&[step(100, 100, 20)]);
        assert_eq!(chunks.len(), 36);
        assert_eq!(chunks[0], ChunkPos::new(3, 3));
        let far = |c: &ChunkPos| (c.x - 3).abs().max((c.y - 3).abs());
        assert!(chunks.windows(2).all(|w| far(&w[0]) <= far(&w[1])));
        let mut once = chunks.clone();
        once.sort_unstable();
        once.dedup();
        assert_eq!(once.len(), chunks.len(), "each once");
    }

    #[test]
    fn the_walk_end_adds_its_sight_square_after_the_near_chunks() {
        let mut walking = step(100, 100, 20);
        walking.to = Some(Pos::new(100 + 32 * 10, 100));
        let chunks = ahead_of(&[walking]);
        // 36 near, plus the walk's end's sight: tiles 400..=440 by
        // 80..=120, chunks 12..=13 by 2..=3, four more, after the near.
        assert_eq!(chunks.len(), 36 + 4);
        assert!(chunks[36..].iter().all(|c| (12..=13).contains(&c.x)));
    }

    #[test]
    fn two_bodies_share_their_chunks_by_the_nearer() {
        let chunks = ahead_of(&[step(100, 100, 20), step(100 + 32, 100, 20)]);
        // Columns 0..=6, rows 0..=5: 42 chunks, the two bodies' own first.
        assert_eq!(chunks.len(), 42);
        assert!(chunks[..2].contains(&ChunkPos::new(3, 3)));
        assert!(chunks[..2].contains(&ChunkPos::new(4, 3)));
    }
}
