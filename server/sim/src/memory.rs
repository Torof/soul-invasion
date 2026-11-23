//! The well (D55): one memory for everybody, records written once.
//!
//! Every thing known is a **record** written once in the world's store,
//! numbered, dated, signed by the first who saw it, never changed. A body
//! carries **accesses**, numbers with who told it and when, never copies:
//! "person 14 says: I know memory 227", and whoever heard it holds 227
//! too. A record is freed when its last access is dropped; forgetting is
//! dropping an access. The store is indexed by what a record says, so
//! that two looks at the same thing the same day find the same number,
//! and a body only ever reads its own accesses: nothing scans the store.
//!
//! The listener's **credit** in what it heard is a perception, judged
//! once the night it heard it: a function of the seed, the listener, the
//! teller and the days, never a datum (only an exception, a liar or a
//! misreading, would be stored; lot 4 has none).

use std::collections::BTreeMap;

use crate::BodyId;
use crate::brain::{Lately, Way, Ways};
use crate::r#gen::scatter::{self, purpose};
use crate::map::{ChunkPos, Pos};

/// A record's number in the store.
pub type RecordId = u32;

/// Where a place is: its square's coordinates (`brain::PlaceKey`).
pub type PlaceKey = (i32, i32);

/// A kind's state at a place, as told: 0 bare, 1 some, 2 full
/// (`content/needs.ron`, `fire.place_buckets`).
pub type Bucket = u8;

/// How a present body fared on the day the fire closes, as it tells it:
/// what the relative judge (D50, amended) listens to.
#[allow(
    clippy::struct_excessive_bools,
    reason = "four marks of one day told, each read by its name"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fared {
    /// Gathered over the day, in per cent of its own need (a child
    /// gathers less, D47).
    pub kj_percent: u32,
    /// Thin at nightfall.
    pub thin: bool,
    /// Thirsty at nightfall.
    pub thirsty: bool,
    /// Cold the night before.
    pub cold: bool,
    /// It changed its ways these days: a trial under way, a way copied or
    /// kept within `trial_days`. The judge leaves those out.
    pub changed: bool,
}

/// A record of the well: written once, dated, signed by its first witness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    /// A place looked at: what kinds stood there and in what state.
    Place {
        /// The place.
        place: PlaceKey,
        /// The day of the look.
        day: u32,
        /// Who first saw it so.
        witness: BodyId,
        /// Each kind seen there and its state, the kinds rising.
        states: Vec<(u16, Bucket)>,
    },
    /// A place to drink found.
    Water {
        /// Its chunk.
        chunk: ChunkPos,
        /// The day it was found.
        day: u32,
        /// Who found it.
        witness: BodyId,
        /// Where to stand.
        stand: Pos,
        /// The water.
        water: Pos,
    },
    /// A way judged: kept or given up, and how its body lived before and
    /// during.
    Trial {
        /// Whose way.
        body: BodyId,
        /// The day it was judged.
        day: u32,
        /// The way moved.
        way: Way,
        /// Its ways as tried.
        after: Ways,
        /// The direction of the step: wider or narrower, more store or
        /// less, out or home; a home moved is always up.
        up: bool,
        /// Kept.
        kept: bool,
        /// Its will when judged (D50): the prestige a listener weighs.
        will: u8,
        /// How it lived the days before.
        before: Lately,
        /// How it lived during the trial.
        during: Lately,
    },
    /// A night at a camp's fire: who was there, what was told by whom,
    /// how each fared that day.
    Night {
        /// The camp.
        camp: Pos,
        /// The day that closed.
        day: u32,
        /// What was told, with its teller, in the order told.
        told: Vec<(RecordId, BodyId)>,
        /// Who was there, rising.
        present: Vec<BodyId>,
        /// How each present fared that day.
        fared: Vec<(BodyId, Fared)>,
    },
}

impl Record {
    /// The day it is dated.
    #[must_use]
    pub fn day(&self) -> u32 {
        match self {
            Record::Place { day, .. }
            | Record::Water { day, .. }
            | Record::Trial { day, .. }
            | Record::Night { day, .. } => *day,
        }
    }

    /// Who first wrote it.
    #[must_use]
    pub fn witness(&self) -> Option<BodyId> {
        match self {
            Record::Place { witness, .. } | Record::Water { witness, .. } => Some(*witness),
            Record::Trial { body, .. } => Some(*body),
            Record::Night { .. } => None,
        }
    }

    /// What makes it the same record as another: a place, a day and its
    /// states, whoever looked; a water, a day and where; a body's way on a
    /// day; a camp's night.
    fn key(&self) -> Key {
        match self {
            Record::Place {
                place, day, states, ..
            } => Key::Place(*place, *day, states.clone()),
            Record::Water {
                chunk, day, stand, ..
            } => Key::Water(*chunk, *day, *stand),
            Record::Trial { body, day, way, .. } => Key::Trial(*body, *day, *way),
            Record::Night { camp, day, .. } => Key::Night(*camp, *day),
        }
    }
}

/// The index's key: what a record says ([`Record::key`]).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Place(PlaceKey, u32, Vec<(u16, Bucket)>),
    Water(ChunkPos, u32, Pos),
    Trial(BodyId, u32, Way),
    Night(Pos, u32),
}

/// An access a body holds to a record: the number, who it has it from
/// (itself when it saw, the teller when it heard) and the day it got it.
/// Twelve bytes; its credit is a function ([`credit`]), not a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Access {
    /// The record.
    pub record: RecordId,
    /// Who it has it from.
    pub by: BodyId,
    /// The day it got it.
    pub day: u32,
}

impl Access {
    /// Seen by the body itself, not heard.
    #[must_use]
    pub fn is_own(self, body: BodyId) -> bool {
        self.by == body
    }
}

/// A record held, with how many accesses name it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Slot {
    record: Record,
    refs: u32,
}

/// The world's store of records: one memory for all.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Store {
    /// Each record by its number; `None` where one was freed.
    slots: Vec<Option<Slot>>,
    /// The numbers freed, taken again first.
    free: Vec<RecordId>,
    /// Each live record by what it says: a look finds the record a
    /// same look made.
    index: BTreeMap<Key, RecordId>,
    /// Records ever written.
    written: u64,
    /// Records freed at their last access.
    freed: u64,
}

impl Store {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Finds the record saying the same, or writes `record` once: its
    /// number, and whether it was found. The caller holds one access to
    /// it from here on ([`Store::release`] to drop it).
    pub fn intern(&mut self, record: Record) -> (RecordId, bool) {
        let key = record.key();
        if let Some(&id) = self.index.get(&key) {
            self.hold(id);
            return (id, true);
        }
        let slot = Some(Slot { record, refs: 1 });
        let id = if let Some(id) = self.free.pop() {
            self.slots[id as usize] = slot;
            id
        } else {
            let id = RecordId::try_from(self.slots.len()).expect("fewer than four billion records");
            self.slots.push(slot);
            id
        };
        self.index.insert(key, id);
        self.written += 1;
        (id, false)
    }

    /// One more access to `id`.
    pub fn hold(&mut self, id: RecordId) {
        if let Some(slot) = self.slots.get_mut(id as usize).and_then(Option::as_mut) {
            slot.refs += 1;
        }
    }

    /// One access to `id` fewer; at none the record is freed, and a night
    /// freed releases what was told at it.
    pub fn release(&mut self, id: RecordId) {
        let Some(slot) = self.slots.get_mut(id as usize) else {
            return;
        };
        let Some(held) = slot.as_mut() else {
            return;
        };
        held.refs = held.refs.saturating_sub(1);
        if held.refs > 0 {
            return;
        }
        let Slot { record, .. } = slot.take().expect("held");
        self.index.remove(&record.key());
        self.free.push(id);
        self.freed += 1;
        if let Record::Night { told, .. } = record {
            for (told, _) in told {
                self.release(told);
            }
        }
    }

    /// The record `id`, if live.
    #[must_use]
    pub fn get(&self, id: RecordId) -> Option<&Record> {
        self.slots
            .get(id as usize)
            .and_then(Option::as_ref)
            .map(|slot| &slot.record)
    }

    /// How many accesses name `id`; 0 when freed or never written.
    #[must_use]
    pub fn refs(&self, id: RecordId) -> u32 {
        self.slots
            .get(id as usize)
            .and_then(Option::as_ref)
            .map_or(0, |slot| slot.refs)
    }

    /// Every live record, by number, with its accesses.
    pub fn records(&self) -> impl Iterator<Item = (RecordId, &Record, u32)> {
        self.slots.iter().enumerate().filter_map(|(i, slot)| {
            let slot = slot.as_ref()?;
            Some((
                RecordId::try_from(i).expect("indexed by RecordId"),
                &slot.record,
                slot.refs,
            ))
        })
    }

    /// Live records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    /// No live record.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Records ever written, and freed.
    #[must_use]
    pub fn counts(&self) -> (u64, u64) {
        (self.written, self.freed)
    }
}

/// A listener's credit in what a teller told it, 1 to 100, judged once
/// the night it heard it and never again: a perception, a function of
/// the seed, the listener (its will: a strong will believes others
/// less), the teller (its will: prestige, D50), the record's age that
/// night and the night itself (a jitter of twenty). Never stored: an
/// access gives the same credit each time it is asked.
#[must_use]
pub fn credit(
    seed: u64,
    listener: (BodyId, u8),
    teller: (BodyId, u8),
    record_day: u32,
    heard_day: u32,
) -> u8 {
    credit_lasting(
        seed,
        listener,
        teller,
        (record_day, heard_day, heard_day),
        TRUST_DAYS,
    )
}

/// How many days of its age a record told takes to lose the 50 points of
/// age in a listener's credit, by default: a way judged, a water found
/// (five points a day, lot 4's pace).
pub const TRUST_DAYS: u32 = 10;

/// The credit ([`credit`]) on `today` in a record heard on `heard_day`
/// whose news lasts `lasting_days` (D55, amended by lot 5a): the jitter is
/// the night's it was heard, the record's age is counted to `today` and
/// takes its 50 points over `lasting_days` instead of [`TRUST_DAYS`]. A
/// place heard is believed less as it ages, as fast as what it bears goes:
/// leaves picked in minutes are old news by the next evening, nuts keep
/// for weeks (`content/needs.ron`, `fire.told_days_per_hour`). Still a
/// function, never stored: asked again the same day, the same.
#[must_use]
pub fn credit_lasting(
    seed: u64,
    (listener, listener_will): (BodyId, u8),
    (teller, teller_will): (BodyId, u8),
    (record_day, heard_day, today): (u32, u32, u32),
    lasting_days: u32,
) -> u8 {
    if listener == teller {
        return 100;
    }
    let age = u64::from(today.saturating_sub(record_day));
    let jitter = scatter::roll(
        seed,
        purpose::FIRE_CREDIT,
        &[
            u64::from(listener.0),
            u64::from(teller.0),
            u64::from(heard_day),
        ],
    ) % 21;
    let base = 50 + u64::from(teller_will) / 2 + jitter;
    let lasting = u64::from(lasting_days.max(1));
    let less = u64::from(listener_will) / 4 + age.min(lasting) * 50 / lasting;
    u8::try_from(base.saturating_sub(less).clamp(1, 100)).expect("under 101")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(day: u32, witness: u32, states: &[(u16, Bucket)]) -> Record {
        Record::Place {
            place: (3, 4),
            day,
            witness: BodyId(witness),
            states: states.to_vec(),
        }
    }

    #[test]
    fn a_record_is_written_once_and_a_same_look_finds_it() {
        let mut store = Store::new();
        let (a, found_a) = store.intern(place(12, 14, &[(1, 2)]));
        let (b, found_b) = store.intern(place(12, 9, &[(1, 2)]));
        assert_eq!((a, found_a, found_b), (b, false, true));
        assert_eq!(store.refs(a), 2);
        assert_eq!(store.get(a).and_then(Record::witness), Some(BodyId(14)));
        assert_eq!(store.len(), 1);
        // Another state, or another day: another record.
        let (c, _) = store.intern(place(12, 7, &[(1, 0)]));
        let (d, _) = store.intern(place(13, 7, &[(1, 2)]));
        assert!(c != a && d != a && c != d);
        assert_eq!(store.len(), 3);
    }

    #[test]
    fn the_last_access_dropped_frees_the_record_and_its_number_serves_again() {
        let mut store = Store::new();
        let (a, _) = store.intern(place(12, 14, &[(1, 2)]));
        store.hold(a);
        store.release(a);
        assert_eq!(store.get(a).map(Record::day), Some(12));
        store.release(a);
        assert_eq!(store.get(a), None);
        assert!(store.is_empty());
        assert_eq!(store.counts(), (1, 1));
        let (b, found) = store.intern(place(12, 14, &[(1, 2)]));
        assert_eq!(
            (b, found),
            (a, false),
            "the number serves again, as a new record"
        );
    }

    #[test]
    fn a_night_freed_releases_what_was_told_at_it() {
        let mut store = Store::new();
        let (a, _) = store.intern(place(12, 14, &[(1, 2)]));
        let (night, _) = store.intern(Record::Night {
            camp: Pos::new(5, 5),
            day: 12,
            told: vec![(a, BodyId(14))],
            present: vec![BodyId(7), BodyId(14)],
            fared: Vec::new(),
        });
        assert_eq!(store.refs(a), 1);
        store.release(night);
        assert_eq!(store.get(night), None);
        assert_eq!(store.get(a), None, "the night held the only access");
    }

    #[test]
    fn a_record_lasting_ten_days_ages_as_before_and_a_short_lived_one_is_old_news_in_a_day() {
        let me = (BodyId(7), 50);
        let teller = (BodyId(14), 60);
        for (rec, heard) in [(12, 12), (10, 12), (0, 40)] {
            assert_eq!(
                credit_lasting(1, me, teller, (rec, heard, heard), TRUST_DAYS),
                credit(1, me, teller, rec, heard),
                "ten days: lot 4's five points a day"
            );
        }
        let fresh = credit_lasting(1, me, teller, (12, 12, 12), 1);
        let day_old = credit_lasting(1, me, teller, (12, 12, 13), 1);
        assert_eq!(
            fresh - day_old,
            50,
            "a day-long news loses its age in a day"
        );
        assert_eq!(
            day_old,
            credit_lasting(1, me, teller, (12, 12, 40), 1),
            "and no more after"
        );
        assert_eq!(
            credit_lasting(1, me, teller, (12, 12, 13), 20),
            fresh - 2,
            "a twenty-day news loses two and a half points a day"
        );
        assert_eq!(
            credit_lasting(1, me, teller, (12, 12, 14), 20),
            fresh - 5,
            "the jitter is the night's it was heard: another day, only the age moves"
        );
    }

    #[test]
    fn the_credit_is_a_function_rising_with_the_tellers_will_and_falling_with_age() {
        let me = (BodyId(7), 50);
        let strong = (BodyId(14), 100);
        let weak = (BodyId(9), 0);
        let once = credit(1, me, strong, 12, 12);
        assert_eq!(
            once,
            credit(1, me, strong, 12, 12),
            "judged once, the same each time"
        );
        assert!(credit(1, me, strong, 12, 12) > credit(1, me, weak, 12, 12));
        assert!(credit(1, me, strong, 12, 12) > credit(1, me, strong, 2, 12));
        assert!(
            credit(1, (BodyId(7), 100), strong, 12, 12) < credit(1, (BodyId(7), 0), strong, 12, 12),
            "a strong will believes others less"
        );
        assert_eq!(credit(1, me, me, 12, 12), 100, "one's own look");
        for will in [0_u8, 50, 100] {
            let c = credit(1, (BodyId(7), will), (BodyId(14), will), 0, 40);
            assert!((1..=100).contains(&c));
        }
    }
}
