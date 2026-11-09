//! A bounded cache that forgets the least recently used entry first.
//!
//! Made for what the server can always compute again from the seed (the
//! flora of a chunk, later the chunks themselves, D39): a cache only
//! memoises, so forgetting an entry at any time changes nothing but the
//! time the next request takes. Its memory follows its capacity, never
//! the map's size nor the land visited.
//!
//! Both indexes are `BTreeMap`s: every operation is a few comparisons for
//! the few thousand entries such a cache holds, and nothing depends on a
//! hasher's order.

use std::collections::BTreeMap;

/// A cache of at most `capacity` entries; inserting into a full cache
/// first drops the entry read or written longest ago.
#[derive(Debug)]
pub struct Lru<K, V> {
    /// Entries held, at most.
    capacity: usize,
    /// Each entry with the stamp of its last use.
    entries: BTreeMap<K, (V, u64)>,
    /// The key of each entry by the stamp of its last use: the first is
    /// the one used longest ago.
    recency: BTreeMap<u64, K>,
    /// The next stamp: it only grows (a `u64` does not run out).
    clock: u64,
}

impl<K: Ord + Copy, V> Lru<K, V> {
    /// An empty cache holding at most `capacity` entries.
    ///
    /// # Panics
    ///
    /// If `capacity` is 0: such a cache would hold nothing.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "a cache holds at least one entry");
        Self {
            capacity,
            entries: BTreeMap::new(),
            recency: BTreeMap::new(),
            clock: 0,
        }
    }

    /// Entries held, at most.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Entries held now.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Is the cache empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Is `key` held? Does not count as a use.
    #[must_use]
    pub fn contains(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    /// The value held for `key`, which becomes the most recently used.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let stamp = self.next_stamp();
        let (value, used) = self.entries.get_mut(key)?;
        self.recency.remove(used);
        *used = stamp;
        self.recency.insert(stamp, *key);
        Some(value)
    }

    /// Holds `value` for `key`, the most recently used; replaces what was
    /// held for `key`, or else, in a full cache, drops the least recently
    /// used entry.
    pub fn insert(&mut self, key: K, value: V) {
        let stamp = self.next_stamp();
        if let Some((_, used)) = self.entries.insert(key, (value, stamp)) {
            self.recency.remove(&used);
        } else if self.entries.len() > self.capacity
            && let Some((_, oldest)) = self.recency.pop_first()
        {
            self.entries.remove(&oldest);
        }
        self.recency.insert(stamp, key);
    }

    fn next_stamp(&mut self) -> u64 {
        let stamp = self.clock;
        self.clock += 1;
        stamp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_least_recently_used_goes_first() {
        let mut cache = Lru::new(3);
        for key in 1..=3 {
            cache.insert(key, key * 10);
        }
        assert_eq!(cache.get(&1), Some(&10), "1 is now the latest used");
        cache.insert(4, 40);
        assert!(!cache.contains(&2), "2, used longest ago, went");
        assert!(cache.contains(&1) && cache.contains(&3) && cache.contains(&4));
        cache.insert(5, 50);
        assert!(!cache.contains(&3));
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn it_never_holds_more_than_its_capacity() {
        let mut cache = Lru::new(7);
        for key in 0..1000 {
            cache.insert(key, ());
            if key % 3 == 0 {
                let _ = cache.get(&(key / 2));
            }
            assert!(cache.len() <= cache.capacity());
            assert_eq!(cache.entries.len(), cache.recency.len(), "indexes agree");
        }
        assert_eq!(cache.len(), 7);
        assert!((993..1000).all(|key| cache.contains(&key)));
    }

    #[test]
    fn inserting_a_held_key_replaces_it_and_drops_nothing() {
        let mut cache = Lru::new(2);
        cache.insert('a', 1);
        cache.insert('b', 2);
        cache.insert('a', 3);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.recency.len(), 2);
        cache.insert('c', 4);
        assert!(!cache.contains(&'b'), "'a' was written after 'b'");
        assert_eq!(cache.get(&'a'), Some(&3));
    }

    #[test]
    fn a_miss_changes_nothing() {
        let mut cache: Lru<u8, u8> = Lru::new(1);
        assert!(cache.is_empty());
        assert_eq!(cache.get(&1), None);
        assert!(cache.is_empty() && cache.recency.is_empty());
    }

    #[test]
    #[should_panic(expected = "at least one entry")]
    fn a_cache_holds_something() {
        let _ = Lru::<u8, u8>::new(0);
    }
}
