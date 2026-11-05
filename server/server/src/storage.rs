//! Persistence (D28): an event journal (D17) plus world snapshots, behind
//! the [`Storage`] interface so the engine (SQLite today) can be replaced
//! without touching the rest of the server.
//!
//! Formats are versioned from day one (D21): the database schema through
//! SQLite's `user_version`, each snapshot through its own format version.

use rusqlite::{Connection, OptionalExtension, params};
use sim::{Event, Tick, World};

/// Version of the serialized `World` inside a snapshot. Bump on any change
/// to `World`'s shape, and add a migration in `decode_world`.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 2;

/// Errors from the storage layer.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The database engine failed.
    #[error("database: {0}")]
    Database(#[from] rusqlite::Error),
    /// A snapshot or event could not be (de)serialized.
    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),
    /// A snapshot was written by a format this build does not know.
    #[error("snapshot format v{found} is not supported (this build reads v{supported})")]
    UnsupportedSnapshot {
        /// Version found in the database.
        found: u32,
        /// Version this build reads.
        supported: u32,
    },
    /// The database schema is newer than this build.
    #[error("database schema v{found} is newer than this build (v{supported})")]
    SchemaTooNew {
        /// Version found in the database.
        found: u32,
        /// Version this build knows.
        supported: u32,
    },
}

/// Where the world's history and state live.
pub trait Storage: Send {
    /// The most recent snapshot, if any.
    fn latest_world(&mut self) -> Result<Option<World>, StorageError>;

    /// Records a snapshot of the world at its current tick.
    fn save_world(&mut self, world: &World) -> Result<(), StorageError>;

    /// Appends the events of one tick to the journal.
    fn append_events(&mut self, tick: Tick, events: &[Event]) -> Result<(), StorageError>;

    /// Events strictly after `tick`, in order.
    fn events_after(&mut self, tick: Tick) -> Result<Vec<(Tick, Event)>, StorageError>;
}

/// Schema migrations, applied in order; the index + 1 is the version
/// recorded in SQLite's `user_version`.
const MIGRATIONS: &[&str] = &[
    // v1: snapshots and the event journal.
    "CREATE TABLE snapshots (
         tick           INTEGER NOT NULL,
         format_version INTEGER NOT NULL,
         data           TEXT    NOT NULL,
         saved_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
     );
     CREATE INDEX snapshots_by_tick ON snapshots (tick);
     CREATE TABLE events (
         tick INTEGER NOT NULL,
         seq  INTEGER NOT NULL,
         data TEXT    NOT NULL,
         PRIMARY KEY (tick, seq)
     );",
];

/// SQLite-backed storage: one file, or memory for ephemeral worlds.
#[derive(Debug)]
pub struct Sqlite {
    conn: Connection,
}

impl Sqlite {
    /// Opens (or creates) the database file and brings its schema up to date.
    pub fn open(path: &std::path::Path) -> Result<Self, StorageError> {
        Self::from_connection(Connection::open(path)?)
    }

    /// An in-memory database: nothing survives the process.
    pub fn in_memory() -> Result<Self, StorageError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self, StorageError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let mut storage = Self { conn };
        storage.migrate()?;
        Ok(storage)
    }

    /// Current schema version (0 for a fresh database).
    pub fn schema_version(&self) -> Result<u32, StorageError> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    fn migrate(&mut self) -> Result<(), StorageError> {
        let found = self.schema_version()?;
        let supported = u32::try_from(MIGRATIONS.len()).expect("few migrations");
        if found > supported {
            return Err(StorageError::SchemaTooNew { found, supported });
        }
        for (index, sql) in MIGRATIONS.iter().enumerate().skip(found as usize) {
            let version = u32::try_from(index + 1).expect("few migrations");
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", version)?;
            tx.commit()?;
        }
        Ok(())
    }
}

impl Storage for Sqlite {
    fn latest_world(&mut self) -> Result<Option<World>, StorageError> {
        let row: Option<(u32, String)> = self
            .conn
            .query_row(
                "SELECT format_version, data FROM snapshots ORDER BY tick DESC, rowid DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        row.map(|(version, data)| decode_world(version, &data))
            .transpose()
    }

    fn save_world(&mut self, world: &World) -> Result<(), StorageError> {
        let data = serde_json::to_string(world)?;
        self.conn.execute(
            "INSERT INTO snapshots (tick, format_version, data) VALUES (?1, ?2, ?3)",
            params![db_int(world.tick().0), SNAPSHOT_FORMAT_VERSION, data],
        )?;
        Ok(())
    }

    fn append_events(&mut self, tick: Tick, events: &[Event]) -> Result<(), StorageError> {
        if events.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        {
            let mut insert =
                tx.prepare_cached("INSERT INTO events (tick, seq, data) VALUES (?1, ?2, ?3)")?;
            for (seq, event) in events.iter().enumerate() {
                let seq = u64::try_from(seq).expect("fewer events than u64::MAX");
                insert.execute(params![
                    db_int(tick.0),
                    db_int(seq),
                    serde_json::to_string(event)?
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn events_after(&mut self, tick: Tick) -> Result<Vec<(Tick, Event)>, StorageError> {
        let mut query = self
            .conn
            .prepare_cached("SELECT tick, data FROM events WHERE tick > ?1 ORDER BY tick, seq")?;
        let rows = query.query_map([db_int(tick.0)], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (tick, data) = row?;
            Ok((Tick(tick.unsigned_abs()), serde_json::from_str(&data)?))
        })
        .collect()
    }
}

/// SQLite integers are signed 64-bit; ticks and sequence numbers never
/// get anywhere near the sign bit.
fn db_int(value: u64) -> i64 {
    i64::try_from(value).expect("tick or sequence number fits in i64")
}

/// Turns stored snapshot data into a `World`, migrating old formats.
fn decode_world(format_version: u32, data: &str) -> Result<World, StorageError> {
    let mut version = format_version;
    let mut data = data.to_owned();
    while version < SNAPSHOT_FORMAT_VERSION {
        data = match version {
            1 => migrate_v1_to_v2(&data)?,
            _ => unreachable!("every version below the current one has a migration"),
        };
        version += 1;
    }
    if version == SNAPSHOT_FORMAT_VERSION {
        Ok(serde_json::from_str(&data)?)
    } else {
        Err(StorageError::UnsupportedSnapshot {
            found: version,
            supported: SNAPSHOT_FORMAT_VERSION,
        })
    }
}

/// v1 held only the calendar and the tick; v2 adds the map and the
/// bodies. A v1 world gets the standard valley and inhabitants, at its
/// old tick.
fn migrate_v1_to_v2(data: &str) -> Result<String, StorageError> {
    let old: serde_json::Value = serde_json::from_str(data)?;
    let calendar: sim::Calendar = serde_json::from_value(old["calendar"].clone())?;
    let mut new = serde_json::to_value(World::new(calendar))?;
    new["tick"] = old["tick"].clone();
    Ok(new.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::Calendar;

    fn world_at(ticks: u64) -> World {
        let mut world = World::new(Calendar::new(2, 3).unwrap());
        world.advance(ticks, &mut Vec::new());
        world
    }

    #[test]
    fn fresh_database_has_no_world() {
        let mut storage = Sqlite::in_memory().unwrap();
        assert_eq!(storage.schema_version().unwrap(), 1);
        assert!(storage.latest_world().unwrap().is_none());
    }

    #[test]
    fn snapshot_round_trips_and_latest_wins() {
        let mut storage = Sqlite::in_memory().unwrap();
        storage.save_world(&world_at(6)).unwrap();
        storage.save_world(&world_at(12)).unwrap();
        let world = storage.latest_world().unwrap().unwrap();
        assert_eq!(world.tick(), Tick(12));
        assert_eq!(world.calendar(), &Calendar::new(2, 3).unwrap());
    }

    #[test]
    fn events_are_journaled_in_order() {
        let mut storage = Sqlite::in_memory().unwrap();
        storage
            .append_events(Tick(6), &[Event::DayStarted { day: 2 }])
            .unwrap();
        storage.append_events(Tick(7), &[]).unwrap();
        storage
            .append_events(Tick(12), &[Event::DayStarted { day: 3 }])
            .unwrap();
        assert_eq!(
            storage.events_after(Tick(0)).unwrap(),
            vec![
                (Tick(6), Event::DayStarted { day: 2 }),
                (Tick(12), Event::DayStarted { day: 3 }),
            ]
        );
        assert_eq!(
            storage.events_after(Tick(6)).unwrap(),
            vec![(Tick(12), Event::DayStarted { day: 3 })]
        );
    }

    #[test]
    fn migrations_are_idempotent_across_reopens() {
        let dir = std::env::temp_dir().join(format!("soul-invasion-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("reopen.sqlite");
        {
            let mut storage = Sqlite::open(&path).unwrap();
            storage.save_world(&world_at(3)).unwrap();
        }
        let mut storage = Sqlite::open(&path).unwrap();
        assert_eq!(storage.schema_version().unwrap(), 1);
        assert_eq!(storage.latest_world().unwrap().unwrap().tick(), Tick(3));
        drop(storage);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn v1_snapshots_are_migrated() {
        let mut storage = Sqlite::in_memory().unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (0, 1, ?1)",
                [r#"{"calendar":{"ticks_per_second":2,"seconds_per_day":3},"tick":12}"#],
            )
            .unwrap();
        let world = storage.latest_world().unwrap().unwrap();
        assert_eq!(world.tick(), Tick(12));
        assert_eq!(world.calendar(), &Calendar::new(2, 3).unwrap());
        assert_eq!(
            world.bodies().len(),
            World::new(Calendar::default()).bodies().len()
        );
    }

    #[test]
    fn unknown_snapshot_format_is_refused() {
        let mut storage = Sqlite::in_memory().unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (0, 99, '{}')",
                [],
            )
            .unwrap();
        let err = storage.latest_world().unwrap_err();
        assert!(matches!(
            err,
            StorageError::UnsupportedSnapshot {
                found: 99,
                supported: SNAPSHOT_FORMAT_VERSION
            }
        ));
    }

    #[test]
    fn newer_schema_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 42).unwrap();
        let err = Sqlite::from_connection(conn).unwrap_err();
        assert!(matches!(
            err,
            StorageError::SchemaTooNew {
                found: 42,
                supported: 1
            }
        ));
    }
}
