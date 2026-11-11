//! Interest by place (D5): what the simulation publishes reaches only the
//! connections it concerns, looked up by the chunk it happens in.
//!
//! Each connection tells the [`Router`] the chunks it holds (its view)
//! and the chunks within the creatures' range around its body
//! ([`BODY_RANGE_M`]). An item event goes to the connections holding its
//! chunk; a body's event to those whose range covers its chunk, and to
//! the connection piloting that body wherever it is; the day and the night
//! to everyone. What a connection is handed follows what stands around it,
//! never the world's population.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use sim::{CHUNK_SIZE, ChunkPos, Map};
use tokio::sync::mpsc;

use crate::net::Published;

/// How far a body sees other bodies (creatures, men), in metres, measured
/// between the centres of the chunks they stand in: the project lead's
/// call (11 November 2026). Less than the land's view (608 m across, D08):
/// a body is seen within about 150 m, never past 6 chunks (192 m).
pub const BODY_RANGE_M: i32 = 150;

/// Is `chunk` within the creatures' range of a body standing in
/// `centre`? Within [`BODY_RANGE_M`] between the chunks' centres: a disc
/// of chunks, 4 either way along the axes.
pub fn in_body_range(centre: ChunkPos, chunk: ChunkPos) -> bool {
    let side = i64::from(CHUNK_SIZE);
    let (dx, dy) = (
        i64::from(chunk.x - centre.x) * side,
        i64::from(chunk.y - centre.y) * side,
    );
    dx * dx + dy * dy <= i64::from(BODY_RANGE_M) * i64::from(BODY_RANGE_M)
}

/// The chunks within the creatures' range of `centre`, clipped to the map.
pub fn body_range(map: &Map, centre: ChunkPos) -> BTreeSet<ChunkPos> {
    let reach = BODY_RANGE_M / i32::try_from(CHUNK_SIZE).expect("a small chunk");
    let mut chunks = BTreeSet::new();
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let chunk = ChunkPos::new(centre.x + dx, centre.y + dy);
            if in_body_range(centre, chunk) && map.has_chunk(chunk) {
                chunks.insert(chunk);
            }
        }
    }
    chunks
}

/// A connection's end of the router: what was published for it, and how
/// many messages it missed because it fell behind.
#[derive(Debug)]
pub struct Inbox {
    /// The connection, as the router knows it.
    pub id: u64,
    /// What was published for it, in order.
    pub events: mpsc::Receiver<Published>,
    /// Messages dropped because `events` was full: the connection is told
    /// its view again ([`crate::net`]'s resync).
    pub lagged: Arc<AtomicU64>,
}

/// One connection as the router knows it.
#[derive(Debug)]
struct Entry {
    sender: mpsc::Sender<Published>,
    lagged: Arc<AtomicU64>,
    view: BTreeSet<ChunkPos>,
    range: BTreeSet<ChunkPos>,
}

/// The connections by the chunks they hold and by the chunks within
/// their range: publishing an event costs the connections it concerns,
/// not every connection.
#[derive(Debug, Default)]
pub struct Router {
    next: u64,
    links: BTreeMap<u64, Entry>,
    /// Connections by the chunks they hold.
    view: BTreeMap<ChunkPos, BTreeSet<u64>>,
    /// Connections by the chunks within their creatures' range.
    range: BTreeMap<ChunkPos, BTreeSet<u64>>,
    /// The connection piloting each body.
    pilots: BTreeMap<u32, u64>,
}

impl Router {
    /// A new connection, holding nothing yet: it is handed the day and
    /// the night only, until it says what it holds. At most `capacity`
    /// messages wait for it; past that it misses some.
    pub fn subscribe(&mut self, capacity: usize) -> Inbox {
        let id = self.next;
        self.next += 1;
        let (sender, events) = mpsc::channel(capacity);
        let lagged = Arc::new(AtomicU64::new(0));
        self.links.insert(
            id,
            Entry {
                sender,
                lagged: Arc::clone(&lagged),
                view: BTreeSet::new(),
                range: BTreeSet::new(),
            },
        );
        Inbox { id, events, lagged }
    }

    /// Forgets a connection.
    pub fn unsubscribe(&mut self, id: u64) {
        let Some(entry) = self.links.remove(&id) else {
            return;
        };
        unindex(&mut self.view, &entry.view, id);
        unindex(&mut self.range, &entry.range, id);
        self.pilots.retain(|_, link| *link != id);
    }

    /// Sets the chunks a connection holds and those within its range.
    pub fn set_interest(&mut self, id: u64, view: &BTreeSet<ChunkPos>, range: &BTreeSet<ChunkPos>) {
        let Some(entry) = self.links.get_mut(&id) else {
            return;
        };
        reindex(&mut self.view, &mut entry.view, view, id);
        reindex(&mut self.range, &mut entry.range, range, id);
    }

    /// Sets the body a connection pilots, if any.
    pub fn set_pilot(&mut self, id: u64, body: Option<u32>) {
        self.pilots.retain(|_, link| *link != id);
        if let Some(body) = body
            && self.links.contains_key(&id)
        {
            self.pilots.insert(body, id);
        }
    }

    /// Hands `event` to the connections it concerns.
    pub fn publish(&mut self, event: &Published) {
        let mut to = BTreeSet::new();
        let near = |index: &BTreeMap<ChunkPos, BTreeSet<u64>>, to: &mut BTreeSet<u64>, chunk| {
            if let Some(ids) = index.get(&chunk) {
                to.extend(ids);
            }
        };
        let body = match *event {
            Published::DayStarted { .. } | Published::NightFell { .. } => {
                to.extend(self.links.keys());
                None
            }
            Published::ItemAdded { ref item, .. } => {
                near(&self.view, &mut to, sim::Pos::new(item.x, item.y).chunk());
                None
            }
            Published::ItemRemoved { at, .. } => {
                near(&self.view, &mut to, at.chunk());
                None
            }
            Published::BodyWalks { body, from: at, .. }
            | Published::BodyStopped { body, at }
            | Published::BodySleeps { body, at, .. }
            | Published::BodyDied { body, at, .. }
            | Published::BodyHolds { body, at, .. } => {
                near(&self.range, &mut to, at.chunk());
                Some(body)
            }
            Published::BodyMoved {
                body, from, to: at, ..
            } => {
                near(&self.range, &mut to, from.chunk());
                near(&self.range, &mut to, at.chunk());
                Some(body)
            }
        };
        if let Some(pilot) = body.and_then(|b| self.pilots.get(&b)) {
            to.insert(*pilot);
        }
        let mut gone = Vec::new();
        for id in to {
            let Some(entry) = self.links.get(&id) else {
                continue;
            };
            match entry.sender.try_send(event.clone()) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    entry.lagged.fetch_add(1, Ordering::Relaxed);
                }
                Err(mpsc::error::TrySendError::Closed(_)) => gone.push(id),
            }
        }
        for id in gone {
            self.unsubscribe(id);
        }
    }

    /// Connections currently subscribed.
    pub fn len(&self) -> usize {
        self.links.len()
    }

    /// No connection subscribed?
    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }
}

/// Removes `id` from the index at each of `chunks`.
fn unindex(index: &mut BTreeMap<ChunkPos, BTreeSet<u64>>, chunks: &BTreeSet<ChunkPos>, id: u64) {
    for chunk in chunks {
        if let Some(ids) = index.get_mut(chunk) {
            ids.remove(&id);
            if ids.is_empty() {
                index.remove(chunk);
            }
        }
    }
}

/// Moves `id` in the index from the chunks of `held` to those of `next`.
fn reindex(
    index: &mut BTreeMap<ChunkPos, BTreeSet<u64>>,
    held: &mut BTreeSet<ChunkPos>,
    next: &BTreeSet<ChunkPos>,
    id: u64,
) {
    let left: BTreeSet<ChunkPos> = held.difference(next).copied().collect();
    unindex(index, &left, id);
    for chunk in next.difference(held) {
        index.entry(*chunk).or_default().insert(id);
    }
    held.clone_from(next);
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::Pos;

    fn walks(body: u32, at: Pos) -> Published {
        Published::BodyWalks {
            body,
            from: at,
            tick: 1,
        }
    }

    #[test]
    fn the_range_is_a_disc_of_150_m_between_chunk_centres() {
        let c = ChunkPos::new(10, 10);
        assert!(in_body_range(c, ChunkPos::new(14, 10)));
        assert!(!in_body_range(c, ChunkPos::new(15, 10)));
        assert!(in_body_range(c, ChunkPos::new(13, 13)));
        assert!(!in_body_range(c, ChunkPos::new(14, 13)));
    }

    #[test]
    fn an_event_reaches_the_connections_of_its_chunk_and_the_pilot_only() {
        let mut router = Router::default();
        let mut near = router.subscribe(8);
        let mut far = router.subscribe(8);
        let mut pilot = router.subscribe(8);
        let here = ChunkPos::new(2, 2);
        let there = ChunkPos::new(20, 2);
        let set = |c: ChunkPos| BTreeSet::from([c]);
        router.set_interest(near.id, &set(here), &set(here));
        router.set_interest(far.id, &set(there), &set(there));
        router.set_interest(pilot.id, &set(there), &set(there));
        router.set_pilot(pilot.id, Some(7));

        router.publish(&walks(7, Pos::new(70, 70)));
        router.publish(&walks(8, Pos::new(70, 70)));
        router.publish(&Published::NightFell { day: 1 });
        let got = |inbox: &mut Inbox| {
            std::iter::from_fn(|| inbox.events.try_recv().ok()).collect::<Vec<_>>()
        };
        assert_eq!(got(&mut near).len(), 3);
        assert_eq!(got(&mut far), vec![Published::NightFell { day: 1 }]);
        assert_eq!(
            got(&mut pilot).len(),
            2,
            "its own body's walk and the night"
        );

        // A connection whose queue is full misses the rest and knows it.
        for _ in 0..10 {
            router.publish(&walks(8, Pos::new(70, 70)));
        }
        assert_eq!(near.lagged.load(Ordering::Relaxed), 2);
        // A connection gone is forgotten.
        drop(far);
        router.publish(&Published::NightFell { day: 2 });
        assert_eq!(router.len(), 2);
    }
}
