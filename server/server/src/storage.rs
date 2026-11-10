//! Persistence (D28): world snapshots, behind the [`Storage`] interface so
//! the engine (SQLite today) can be replaced without touching the rest of
//! the server. There is no world journal any more (D17, 10 November 2026):
//! nothing read it back; a soul's deeds will have their own store (D14).
//!
//! Formats are versioned from day one (D21): the database schema through
//! SQLite's `user_version`, each snapshot through its own format version.

use std::sync::Arc;

use rusqlite::{Connection, OptionalExtension, params};
use sim::{Content, World};

/// Version of the serialized `World` inside a snapshot. Bump on any change
/// to `World`'s shape, and add a migration in `decode_world`.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 7;

/// Snapshots kept: the latest and two before it. Every one used to be
/// kept, a full copy of the world each game day (18 MB on the valley).
pub const KEPT_SNAPSHOTS: u32 = 3;

/// Errors from the storage layer.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The database engine failed.
    #[error("database: {0}")]
    Database(#[from] rusqlite::Error),
    /// A snapshot could not be (de)serialized.
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

/// Where the world's state lives.
pub trait Storage: Send {
    /// The most recent snapshot, if any.
    fn latest_world(&mut self) -> Result<Option<World>, StorageError>;

    /// Records a snapshot of the world at its current tick.
    fn save_world(&mut self, world: &World) -> Result<(), StorageError>;
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
    // v2: no world journal (D17, 10 November 2026): it held every step and
    // gauge, nothing read it back, and a crash already dropped what came
    // after the last snapshot.
    "DROP TABLE events;",
];

/// SQLite-backed storage: one file, or memory for ephemeral worlds.
#[derive(Debug)]
pub struct Sqlite {
    conn: Connection,
    /// Needed to rebuild what old snapshot formats did not hold.
    content: Arc<Content>,
}

impl Sqlite {
    /// Opens (or creates) the database file and brings its schema up to date.
    pub fn open(path: &std::path::Path, content: Arc<Content>) -> Result<Self, StorageError> {
        Self::from_connection(Connection::open(path)?, content)
    }

    /// An in-memory database: nothing survives the process.
    pub fn in_memory(content: Arc<Content>) -> Result<Self, StorageError> {
        Self::from_connection(Connection::open_in_memory()?, content)
    }

    fn from_connection(conn: Connection, content: Arc<Content>) -> Result<Self, StorageError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let mut storage = Self { conn, content };
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
        row.map(|(version, data)| decode_world(version, &data, &self.content))
            .transpose()
    }

    fn save_world(&mut self, world: &World) -> Result<(), StorageError> {
        let data = serde_json::to_string(world)?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO snapshots (tick, format_version, data) VALUES (?1, ?2, ?3)",
            params![db_int(world.tick().0), SNAPSHOT_FORMAT_VERSION, data],
        )?;
        // Only the latest few are kept: a world is resumed from the last
        // one, the others are a margin should it be damaged.
        tx.execute(
            "DELETE FROM snapshots WHERE rowid NOT IN \
             (SELECT rowid FROM snapshots ORDER BY tick DESC, rowid DESC LIMIT ?1)",
            [KEPT_SNAPSHOTS],
        )?;
        tx.commit()?;
        Ok(())
    }
}

/// SQLite integers are signed 64-bit; ticks and sequence numbers never
/// get anywhere near the sign bit.
fn db_int(value: u64) -> i64 {
    i64::try_from(value).expect("tick or sequence number fits in i64")
}

/// Turns stored snapshot data into a `World`, migrating old formats.
fn decode_world(format_version: u32, data: &str, content: &Content) -> Result<World, StorageError> {
    let mut version = format_version;
    let mut data = data.to_owned();
    while version < SNAPSHOT_FORMAT_VERSION {
        data = match version {
            1 => migrate_v1_to_v2(&data, content)?,
            2 => migrate_v2_to_v3(&data, content)?,
            3 => migrate_v3_to_v4(&data, content)?,
            4 => migrate_v4_to_v5(&data)?,
            5 => migrate_v5_to_v6(&data)?,
            6 => migrate_v6_to_v7(&data)?,
            _ => unreachable!("every version below the current one has a migration"),
        };
        version += 1;
    }
    if version == SNAPSHOT_FORMAT_VERSION {
        // The trees are not stored: planted again from the map.
        let mut world: World = serde_json::from_str(&data)?;
        world.plant(content);
        Ok(world)
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
fn migrate_v1_to_v2(data: &str, content: &Content) -> Result<String, StorageError> {
    standard_world_at_old_tick(data, content)
}

/// v2 stored the 32×32 map tile by tile; v3 stores how the map is
/// generated, and the valley is 256×256 at one metre per tile (D31).
/// Positions of the old valley mean nothing in the new one: a v2 world
/// gets the standard valley and inhabitants, at its old tick.
fn migrate_v2_to_v3(data: &str, content: &Content) -> Result<String, StorageError> {
    standard_world_at_old_tick(data, content)
}

/// v4 adds the items lying on the ground. A v3 world keeps its bodies and
/// gets the items of a fresh valley of its own size, as if they had always
/// been there.
fn migrate_v3_to_v4(data: &str, content: &Content) -> Result<String, StorageError> {
    let mut old: serde_json::Value = serde_json::from_str(data)?;
    let calendar: sim::Calendar = serde_json::from_value(old["calendar"].clone())?;
    let size = old["map"]["width"]
        .as_u64()
        .and_then(|w| u32::try_from(w).ok())
        .unwrap_or(sim::VALLEY_SIZE);
    let fresh = serde_json::to_value(World::with_valley(calendar, content, size))?;
    old["items"] = fresh["items"].clone();
    old["next_item"] = fresh["next_item"].clone();
    Ok(old.to_string())
}

/// v5 places bodies finely (D32): `pos`, `path` and `next_step` become
/// `at` (the tile's centre, in millimetres) and an empty `route`.
fn migrate_v4_to_v5(data: &str) -> Result<String, StorageError> {
    let mut old: serde_json::Value = serde_json::from_str(data)?;
    if let Some(bodies) = old["bodies"].as_array_mut() {
        for body in bodies {
            if body.get("pos").is_none() {
                continue; // already fine: rebuilt by an earlier migration
            }
            let pos: sim::Pos = serde_json::from_value(body["pos"].clone())?;
            let at = pos.centre();
            body["at"] = serde_json::json!({ "x": at.x, "y": at.y });
            body["route"] = serde_json::json!([]);
            if let Some(fields) = body.as_object_mut() {
                fields.remove("pos");
                fields.remove("path");
                fields.remove("next_step");
            }
        }
    }
    Ok(old.to_string())
}

/// v6 gives bodies a condition (D34): older bodies wake up healthy.
fn migrate_v5_to_v6(data: &str) -> Result<String, StorageError> {
    let mut old: serde_json::Value = serde_json::from_str(data)?;
    let healthy = serde_json::to_value(sim::Condition::healthy())?;
    if let Some(bodies) = old["bodies"].as_array_mut() {
        for body in bodies {
            if body.get("condition").is_none() {
                body["condition"] = healthy.clone();
            }
        }
    }
    Ok(old.to_string())
}

/// v7 gives items a place (D35): `pos` becomes `place: ground`, and
/// bodies gain empty hands.
fn migrate_v6_to_v7(data: &str) -> Result<String, StorageError> {
    let mut old: serde_json::Value = serde_json::from_str(data)?;
    if let Some(items) = old["items"].as_array_mut() {
        for item in items {
            if let Some(pos) = item.get("pos").cloned() {
                item["place"] = serde_json::json!({ "type": "ground", "pos": pos });
                if let Some(fields) = item.as_object_mut() {
                    fields.remove("pos");
                }
            }
        }
    }
    if let Some(bodies) = old["bodies"].as_array_mut() {
        for body in bodies {
            if body.get("held").is_none() {
                body["held"] = serde_json::Value::Null;
                body["carried"] = serde_json::json!([]);
            }
        }
    }
    Ok(old.to_string())
}

fn standard_world_at_old_tick(data: &str, content: &Content) -> Result<String, StorageError> {
    let old: serde_json::Value = serde_json::from_str(data)?;
    let calendar: sim::Calendar = serde_json::from_value(old["calendar"].clone())?;
    let mut new = serde_json::to_value(World::new(calendar, content))?;
    new["tick"] = old["tick"].clone();
    Ok(new.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{Calendar, Tick};

    fn content() -> Arc<Content> {
        Arc::new(Content::sample())
    }

    fn world_at(ticks: u64) -> World {
        // A small valley: saved and restored like the game's, in a
        // fraction of the time.
        let mut world = World::with_valley(Calendar::new(2, 3).unwrap(), &content(), 256);
        world.advance(ticks, &content(), &mut Vec::new());
        world
    }

    #[test]
    fn fresh_database_has_no_world() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
        assert_eq!(storage.schema_version().unwrap(), 2);
        assert!(storage.latest_world().unwrap().is_none());
    }

    #[test]
    fn snapshot_round_trips_and_latest_wins() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
        storage.save_world(&world_at(6)).unwrap();
        storage.save_world(&world_at(12)).unwrap();
        let world = storage.latest_world().unwrap().unwrap();
        assert_eq!(world.tick(), Tick(12));
        assert_eq!(world.calendar(), &Calendar::new(2, 3).unwrap());
    }

    #[test]
    fn only_the_latest_snapshots_are_kept() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
        for ticks in [6, 12, 18, 24, 30] {
            storage.save_world(&world_at(ticks)).unwrap();
        }
        let kept: Vec<i64> = storage
            .conn
            .prepare("SELECT tick FROM snapshots ORDER BY tick")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(kept, vec![18, 24, 30]);
        assert_eq!(storage.latest_world().unwrap().unwrap().tick(), Tick(30));
    }

    #[test]
    fn migrations_are_idempotent_across_reopens() {
        let dir = std::env::temp_dir().join(format!("soul-invasion-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("reopen.sqlite");
        {
            let mut storage = Sqlite::open(&path, content()).unwrap();
            storage.save_world(&world_at(3)).unwrap();
        }
        let mut storage = Sqlite::open(&path, content()).unwrap();
        assert_eq!(storage.schema_version().unwrap(), 2);
        assert_eq!(storage.latest_world().unwrap().unwrap().tick(), Tick(3));
        drop(storage);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_v1_database_loses_its_journal_and_keeps_its_world() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO events (tick, seq, data) VALUES (5, 0, '{}')",
            [],
        )
        .unwrap();
        let mut storage = Sqlite::from_connection(conn, content()).unwrap();
        storage.save_world(&world_at(4)).unwrap();
        assert_eq!(storage.schema_version().unwrap(), 2);
        let journal: i64 = storage
            .conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'events'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(journal, 0, "no journal table");
        assert_eq!(storage.latest_world().unwrap().unwrap().tick(), Tick(4));
    }

    #[test]
    fn v1_snapshots_are_migrated() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
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
        assert_eq!(world.bodies().len(), 3, "the standard valley's inhabitants");
    }

    #[test]
    fn v2_snapshots_are_migrated() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
        // A v2 world: tiles stored one by one, bodies on the old 32×32 valley.
        let v2 = r#"{"calendar":{"ticks_per_second":2,"seconds_per_day":3},"tick":40,
            "map":{"width":2,"height":1,"tiles":["grass","water"]},
            "bodies":[{"id":1,"pos":{"x":0,"y":0},"path":[],"next_step":40}],"next_body":2}"#;
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (40, 2, ?1)",
                [v2],
            )
            .unwrap();
        let world = storage.latest_world().unwrap().unwrap();
        assert_eq!(world.tick(), Tick(40));
        assert_eq!(world.calendar(), &Calendar::new(2, 3).unwrap());
        assert_eq!(world.map().width(), sim::VALLEY_SIZE);
        assert_eq!(world.bodies().len(), 3);
    }

    #[test]
    fn v3_snapshots_keep_their_bodies_and_gain_items() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
        // A v3 world: map as spec, one body somewhere unusual, no items.
        let v3 = r#"{"calendar":{"ticks_per_second":2,"seconds_per_day":3},"tick":50,
            "map":{"kind":"valley","width":256,"height":256,"seed":1},
            "bodies":[{"id":1,"pos":{"x":200,"y":200},"path":[],"next_step":50}],"next_body":2}"#;
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (50, 3, ?1)",
                [v3],
            )
            .unwrap();
        let world = storage.latest_world().unwrap().unwrap();
        assert_eq!(world.tick(), Tick(50));
        assert_eq!(world.bodies().len(), 1);
        assert_eq!(world.bodies()[0].pos(), sim::Pos::new(200, 200));
        assert_eq!(world.bodies()[0].at(), sim::Pos::new(200, 200).centre());
        assert!(!world.items().is_empty(), "the valley's items appeared");
    }

    #[test]
    fn v4_snapshots_place_their_bodies_finely() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
        // A v4 world: bodies on tiles with a tile path, items present.
        let v4 = r#"{"calendar":{"ticks_per_second":2,"seconds_per_day":3},"tick":7,
            "map":{"kind":"valley","width":256,"height":256,"seed":1},
            "bodies":[{"id":1,"pos":{"x":10,"y":20},"path":[{"x":11,"y":20}],"next_step":9}],"next_body":2,
            "items":[{"id":1,"material":"berries","pos":{"x":3,"y":3}}],"next_item":2}"#;
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (7, 4, ?1)",
                [v4],
            )
            .unwrap();
        let world = storage.latest_world().unwrap().unwrap();
        let body = &world.bodies()[0];
        assert_eq!(body.at(), sim::FinePos::new(10_500, 20_500));
        assert!(!body.is_walking(), "the old tile path is dropped");
        assert_eq!(body.condition(), &sim::Condition::healthy(), "v6: healthy");
        assert_eq!(body.held(), None, "v7: empty hands");
        assert_eq!(world.items().len(), 1);
        assert_eq!(
            world.items()[0].pos(),
            Some(sim::Pos::new(3, 3)),
            "v7: on the ground"
        );
    }

    #[test]
    fn unknown_snapshot_format_is_refused() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
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
        let err = Sqlite::from_connection(conn, content()).unwrap_err();
        assert!(matches!(
            err,
            StorageError::SchemaTooNew {
                found: 42,
                supported: 2
            }
        ));
    }
}
