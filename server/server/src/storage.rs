//! Persistence (D28): world snapshots, behind the [`Storage`] interface so
//! the engine (SQLite today) can be replaced without touching the rest of
//! the server. There is no world journal any more (D17, 10 November 2026):
//! nothing read it back; a soul's deeds will have their own store (D14).
//!
//! What men changed in the world is not in the snapshot but beside it,
//! chunk by chunk (`chunk_changes`, lightness plan D2): a row a chunk men
//! touched, with its version, written again only when that version moved
//! since the last checkpoint.
//!
//! Whether the last run stopped cleanly is stored too (`run_state`): a
//! run that crashed lost what men changed since the last checkpoint, so
//! a chunk's version can go back to a number a client already holds with
//! other content; the next run then starts a new epoch, folded into the
//! world's identity (D4), and clients drop what they hold. The world's
//! birth, drawn at random when its database is made, is folded in too: a
//! world made anew (`--reset`) never shares an identity with the old one.
//!
//! Formats are versioned from day one (D21): the database schema through
//! SQLite's `user_version`, each snapshot through its own format version,
//! each chunk's changes through theirs.

use std::collections::BTreeMap;
use std::sync::Arc;

use rusqlite::{Connection, OptionalExtension, params};
use sim::changes::{ChunkChanges, Construction, Planted, Stage};
use sim::ground::Version;
use sim::trees::TreeKey;
use sim::{CHUNK_SIZE, ChunkPos, Content, Pos, World};

/// Version of the serialized `World` inside a snapshot. Bump on any change
/// to `World`'s shape, and add a migration in `decode_world`.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 9;

/// Version of a chunk's changes as stored (`encode_changes`). Bump on any
/// change to their bytes, and read the old ones in `decode_changes`.
pub const CHUNK_CHANGES_FORMAT_VERSION: u32 = 1;

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
    /// A chunk's stored changes are damaged, or of a format this build
    /// does not know.
    #[error("the changes of chunk ({x}, {y}) are damaged or of an unknown format (v{format})")]
    ChunkChanges {
        /// The chunk.
        x: i32,
        /// The chunk.
        y: i32,
        /// Their format version.
        format: u32,
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

    /// Starts a run: reads whether the last one stopped cleanly, clears
    /// that mark, and returns the world's lineage, its epoch one more
    /// than the last run's if it did not (a crash).
    fn begin_run(&mut self) -> Result<Lineage, StorageError>;

    /// Marks the run as stopped cleanly, after its final snapshot.
    fn mark_clean_stop(&mut self) -> Result<(), StorageError>;
}

/// What makes a stored world another world to a client (D28): its birth,
/// a random number drawn when its database was made, and its epoch, one
/// more after each crash. Both fold into the world's identity (D4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lineage {
    /// Drawn once, when the database is made.
    pub birth: u64,
    /// How many runs ended in a crash.
    pub epoch: u64,
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
    // v3: what men changed, chunk by chunk (lightness plan D2, 11 November
    // 2026), beside the snapshots: a row a chunk men touched, written
    // again only when its version moved. `tick` is the checkpoint that
    // wrote it. The natural items taken, which v8 snapshots held, move in
    // at the next checkpoint (the world reads them from the snapshot).
    "CREATE TABLE chunk_changes (
         x              INTEGER NOT NULL,
         y              INTEGER NOT NULL,
         version        INTEGER NOT NULL,
         format_version INTEGER NOT NULL,
         tick           INTEGER NOT NULL,
         data           BLOB    NOT NULL,
         PRIMARY KEY (x, y)
     ) WITHOUT ROWID;",
    // v4: whether the last run stopped cleanly, the world's epoch, one
    // more after each crash, and its birth, drawn here at random (SQLite's
    // `random()`, from the system's entropy) (11 November 2026). A
    // database that already holds a world does not know how its last run
    // ended: it counts as a crash, once.
    "CREATE TABLE run_state (
         id    INTEGER PRIMARY KEY CHECK (id = 0),
         birth INTEGER NOT NULL,
         epoch INTEGER NOT NULL,
         clean INTEGER NOT NULL
     );
     INSERT INTO run_state (id, birth, epoch, clean)
         SELECT 0, random(), 0, NOT EXISTS (SELECT 1 FROM snapshots);",
];

/// SQLite-backed storage: one file, or memory for ephemeral worlds.
#[derive(Debug)]
pub struct Sqlite {
    conn: Connection,
    /// Needed to rebuild what old snapshot formats did not hold.
    content: Arc<Content>,
    /// The version of each chunk's changes in the database, as last read
    /// or written: a checkpoint writes only the chunks whose version
    /// differs. Grows with what men change, never with the land.
    written: BTreeMap<ChunkPos, Version>,
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
        let mut storage = Self {
            conn,
            content,
            written: BTreeMap::new(),
        };
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
        let Some((version, data)) = row else {
            return Ok(None);
        };
        let mut world = decode_world(version, &data, &self.content)?;
        let mut rows = self
            .conn
            .prepare("SELECT x, y, version, format_version, data FROM chunk_changes")?;
        let rows = rows.query_map([], |row| {
            Ok((
                ChunkPos::new(row.get(0)?, row.get(1)?),
                row.get::<_, Version>(2)?,
                row.get::<_, u32>(3)?,
                row.get::<_, Vec<u8>>(4)?,
            ))
        })?;
        self.written.clear();
        for row in rows {
            let (chunk, version, format, data) = row?;
            let changes = decode_changes(chunk, version, format, &data).ok_or(
                StorageError::ChunkChanges {
                    x: chunk.x,
                    y: chunk.y,
                    format,
                },
            )?;
            world.restore_changes(chunk, changes);
            self.written.insert(chunk, version);
        }
        Ok(Some(world))
    }

    fn save_world(&mut self, world: &World) -> Result<(), StorageError> {
        let data = serde_json::to_string(world)?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO snapshots (tick, format_version, data) VALUES (?1, ?2, ?3)",
            params![db_int(world.tick().0), SNAPSHOT_FORMAT_VERSION, data],
        )?;
        // The chunks men changed since the last checkpoint, alone.
        let moved: Vec<(ChunkPos, Version)> = world
            .chunk_changes()
            .filter(|(chunk, changes)| self.written.get(chunk) != Some(&changes.version))
            .map(|(chunk, changes)| (chunk, changes.version))
            .collect();
        for &(chunk, version) in &moved {
            let changes = world.changes_in(chunk).expect("listed above");
            tx.execute(
                "INSERT OR REPLACE INTO chunk_changes (x, y, version, format_version, tick, data) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    chunk.x,
                    chunk.y,
                    version,
                    CHUNK_CHANGES_FORMAT_VERSION,
                    db_int(world.tick().0),
                    encode_changes(chunk, changes),
                ],
            )?;
        }
        // Only the latest few are kept: a world is resumed from the last
        // one, the others are a margin should it be damaged.
        tx.execute(
            "DELETE FROM snapshots WHERE rowid NOT IN \
             (SELECT rowid FROM snapshots ORDER BY tick DESC, rowid DESC LIMIT ?1)",
            [KEPT_SNAPSHOTS],
        )?;
        tx.commit()?;
        self.written.extend(moved);
        Ok(())
    }

    fn begin_run(&mut self) -> Result<Lineage, StorageError> {
        // Synchronous, so that a power cut cannot bring the mark back:
        // a crash after it would be taken for a clean stop.
        self.conn.pragma_update(None, "synchronous", "FULL")?;
        let tx = self.conn.transaction()?;
        let (birth, epoch, clean): (i64, i64, bool) =
            tx.query_row("SELECT birth, epoch, clean FROM run_state", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
        let epoch = if clean { epoch } else { epoch + 1 };
        tx.execute("UPDATE run_state SET epoch = ?1, clean = 0", [epoch])?;
        tx.commit()?;
        self.conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(Lineage {
            birth: birth.cast_unsigned(),
            epoch: u64::try_from(epoch).expect("a positive epoch"),
        })
    }

    fn mark_clean_stop(&mut self) -> Result<(), StorageError> {
        self.conn.execute("UPDATE run_state SET clean = 1", [])?;
        Ok(())
    }
}

/// A chunk's changes as stored, format 1: four lists, each its length
/// then its entries, every number a LEB128 varint (a byte under 128).
/// The taken slots and the felled trees' keys ([`TreeKey::bits`]) in
/// order, each as its distance from the one before: about a byte an
/// entry. A planted tree: its key, its species' id (length, then UTF-8),
/// its stage (0 to 3), its day. A construction: its kind's id, its tile
/// in the chunk (`y × 32 + x`), its day.
fn encode_changes(chunk: ChunkPos, changes: &ChunkChanges) -> Vec<u8> {
    fn varint(out: &mut Vec<u8>, mut value: u64) {
        while value >= 0x80 {
            out.push(u8::try_from(value & 0x7f).expect("seven bits") | 0x80);
            value >>= 7;
        }
        out.push(u8::try_from(value).expect("under 128"));
    }
    fn ascending(out: &mut Vec<u8>, values: impl ExactSizeIterator<Item = u16>) {
        varint(out, values.len() as u64);
        let mut last = 0;
        for value in values {
            varint(out, u64::from(value - last));
            last = value;
        }
    }
    fn text(out: &mut Vec<u8>, text: &str) {
        varint(out, text.len() as u64);
        out.extend_from_slice(text.as_bytes());
    }
    let mut out = Vec::new();
    ascending(&mut out, changes.taken.iter().copied());
    let mut felled: Vec<u16> = changes.felled.iter().map(|k| k.bits()).collect();
    felled.sort_unstable();
    ascending(&mut out, felled.into_iter());
    varint(&mut out, changes.planted.len() as u64);
    for (key, planted) in &changes.planted {
        varint(&mut out, u64::from(key.bits()));
        text(&mut out, &planted.species);
        let stage = Stage::ALL.iter().position(|s| *s == planted.stage);
        varint(&mut out, stage.expect("a stage") as u64);
        varint(&mut out, planted.day);
    }
    varint(&mut out, changes.built.len() as u64);
    let origin = chunk.origin();
    let side = i32::try_from(CHUNK_SIZE).expect("small chunk");
    for built in &changes.built {
        text(&mut out, &built.kind);
        let (dx, dy) = (built.at.x - origin.x, built.at.y - origin.y);
        assert!(
            (0..side).contains(&dx) && (0..side).contains(&dy),
            "a construction stands in its chunk"
        );
        varint(
            &mut out,
            u64::try_from(dy * side + dx).expect("in the chunk"),
        );
        varint(&mut out, built.day);
    }
    out
}

/// The changes [`encode_changes`] wrote for `chunk` at `version`, in
/// `format`; `None` if the bytes are damaged or the format unknown.
fn decode_changes(
    chunk: ChunkPos,
    version: Version,
    format: u32,
    data: &[u8],
) -> Option<ChunkChanges> {
    struct Bytes<'a>(&'a [u8]);
    impl Bytes<'_> {
        fn varint(&mut self) -> Option<u64> {
            let mut value = 0_u64;
            for shift in (0..64).step_by(7) {
                let (&byte, rest) = self.0.split_first()?;
                self.0 = rest;
                value |= u64::from(byte & 0x7f).checked_shl(shift)?;
                if byte & 0x80 == 0 {
                    return Some(value);
                }
            }
            None
        }
        fn count(&mut self) -> Option<usize> {
            // Never more entries than bytes left: damaged bytes cannot ask
            // for a huge allocation.
            usize::try_from(self.varint()?)
                .ok()
                .filter(|n| *n <= self.0.len())
        }
        fn ascending(&mut self) -> Option<Vec<u16>> {
            let count = self.count()?;
            let mut values = Vec::with_capacity(count);
            let mut last = 0_u16;
            for n in 0..count {
                let step = u16::try_from(self.varint()?).ok()?;
                if n > 0 && step == 0 {
                    return None;
                }
                last = last.checked_add(step)?;
                values.push(last);
            }
            Some(values)
        }
        fn text(&mut self) -> Option<String> {
            let len = usize::try_from(self.varint()?).ok()?;
            if len > self.0.len() {
                return None;
            }
            let (text, rest) = self.0.split_at(len);
            self.0 = rest;
            String::from_utf8(text.to_vec()).ok()
        }
    }
    if format != CHUNK_CHANGES_FORMAT_VERSION {
        return None;
    }
    let mut bytes = Bytes(data);
    let mut changes = ChunkChanges {
        version,
        taken: bytes.ascending()?.into_iter().collect(),
        ..ChunkChanges::default()
    };
    for bits in bytes.ascending()? {
        changes.felled.insert(TreeKey::from_bits(bits)?);
    }
    for _ in 0..bytes.count()? {
        let key = TreeKey::from_bits(u16::try_from(bytes.varint()?).ok()?)?;
        let species = bytes.text()?;
        let stage = *Stage::ALL.get(usize::try_from(bytes.varint()?).ok()?)?;
        let day = bytes.varint()?;
        changes.planted.insert(
            key,
            Planted {
                species,
                stage,
                day,
            },
        );
    }
    let origin = chunk.origin();
    let side = u64::from(CHUNK_SIZE);
    for _ in 0..bytes.count()? {
        let kind = bytes.text()?;
        let tile = bytes.varint()?;
        if tile >= side * side {
            return None;
        }
        let at = Pos::new(
            origin.x + i32::try_from(tile % side).ok()?,
            origin.y + i32::try_from(tile / side).ok()?,
        );
        let day = bytes.varint()?;
        changes.built.push(Construction { kind, at, day });
    }
    bytes.0.is_empty().then_some(changes)
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
            7 => migrate_v7_to_v8(&data)?,
            8 => migrate_v8_to_v9(data),
            _ => unreachable!("every version below the current one has a migration"),
        };
        version += 1;
    }
    if version == SNAPSHOT_FORMAT_VERSION {
        // The trees are not stored: placed again from the map, chunk by
        // chunk, with the content's kinds.
        let mut world: World = serde_json::from_str(&data)?;
        world.take_content(content);
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
    // Today's fresh valley drops natural items instead (v8).
    old["natural"] = fresh["natural"].clone();
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

/// v8: the ground drops natural items, made per chunk and never stored,
/// with the list of those taken. A world made before keeps the items it
/// scattered, all stored, and makes no natural ones.
fn migrate_v7_to_v8(data: &str) -> Result<String, StorageError> {
    let mut old: serde_json::Value = serde_json::from_str(data)?;
    if old.get("natural").is_none() {
        old["natural"] = serde_json::Value::Bool(false);
        old["taken"] = serde_json::json!([]);
    }
    Ok(old.to_string())
}

/// v9: the natural items taken leave the snapshot for their chunks'
/// changes, stored beside it (schema v3). A v8 snapshot's `taken` list
/// is read as it is, each item becoming its chunk's change
/// (`World`'s reading); the next checkpoint writes them in their chunks.
fn migrate_v8_to_v9(data: String) -> String {
    data
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
        assert_eq!(storage.schema_version().unwrap(), 4);
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
        assert_eq!(storage.schema_version().unwrap(), 4);
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
        assert_eq!(storage.schema_version().unwrap(), 4);
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
        assert!(
            (0..8).any(|y| (0..8).any(|x| !world.items_in(sim::ChunkPos::new(x, y)).is_empty())),
            "the valley's items appeared"
        );
    }

    #[test]
    fn v7_worlds_keep_their_scattered_items_and_make_no_natural_ones() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
        // A v7 world: today's, as it was written before natural items.
        let mut v7 = serde_json::to_value(World::empty(
            Calendar::new(2, 3).unwrap(),
            sim::Map::valley(256, 256, 1),
        ))
        .unwrap();
        for field in ["natural", "taken"] {
            v7.as_object_mut().unwrap().remove(field);
        }
        v7["items"] = serde_json::json!([
            {"id": 1, "material": "branch", "place": {"type": "ground", "pos": {"x": 10, "y": 40}}}
        ]);
        v7["next_item"] = serde_json::json!(2);
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (0, 7, ?1)",
                [v7.to_string()],
            )
            .unwrap();
        let world = storage.latest_world().unwrap().unwrap();
        assert!(!world.has_natural_items());
        assert_eq!(world.items().count(), 1);
        assert_eq!(world.items_in(sim::Pos::new(10, 40).chunk()).len(), 1);
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
        assert_eq!(world.items().count(), 1);
        assert_eq!(
            world.items().next().expect("one item").pos(),
            Some(sim::Pos::new(3, 3)),
            "v7: on the ground"
        );
    }

    /// A database file of its own, in a fresh directory removed by the
    /// caller.
    fn temp_db(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("soul-invasion-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("world.sqlite");
        (dir, path)
    }

    /// Changes of every kind, in a chunk east of the map and one west of
    /// it, at the far end of the cells keys name.
    fn every_kind_of_change() -> Vec<(ChunkPos, ChunkChanges)> {
        let key = |bits| TreeKey::from_bits(bits).expect("a key");
        let east = ChunkPos::new(5, 3);
        let west = ChunkPos::new(-1, 2);
        let mut a = ChunkChanges {
            version: 4,
            taken: [3, 70, 4000, u16::MAX].into(),
            felled: [key(0), key(0x0888), key(1 << 11 | 0x7f8)].into(),
            ..ChunkChanges::default()
        };
        for (n, stage) in Stage::ALL.into_iter().enumerate() {
            a.planted.insert(
                key(2 << 11 | u16::try_from(n).unwrap()),
                Planted {
                    species: ["oak", "birch", "wild_apple", "pine"][n].to_owned(),
                    stage,
                    day: 1 + 400 * n as u64,
                },
            );
        }
        a.built.push(Construction {
            kind: "hut".to_owned(),
            at: Pos::new(east.origin().x + 31, east.origin().y + 31),
            day: u64::MAX,
        });
        let b = ChunkChanges {
            version: 1,
            built: vec![Construction {
                kind: "cairn".to_owned(),
                at: west.origin(),
                day: 0,
            }],
            ..ChunkChanges::default()
        };
        vec![(east, a), (west, b)]
    }

    #[test]
    fn chunk_changes_round_trip_through_sqlite() {
        // The bytes alone, every kind of change, and nothing at all.
        for (chunk, changes) in every_kind_of_change() {
            let bytes = encode_changes(chunk, &changes);
            let back = decode_changes(chunk, changes.version, CHUNK_CHANGES_FORMAT_VERSION, &bytes);
            assert_eq!(back.as_ref(), Some(&changes));
            // Damaged bytes are refused, never misread.
            for cut in 0..bytes.len() {
                assert_eq!(
                    decode_changes(chunk, 1, 1, &bytes[..cut]),
                    None,
                    "cut at {cut}"
                );
            }
            let mut longer = bytes.clone();
            longer.push(0);
            assert_eq!(decode_changes(chunk, 1, 1, &longer), None);
            assert_eq!(decode_changes(chunk, 1, 2, &bytes), None, "unknown format");
        }
        let none = ChunkChanges::default();
        assert_eq!(encode_changes(ChunkPos::new(0, 0), &none), [0, 0, 0, 0]);
        // Through a database file, closed and opened again.
        let (dir, path) = temp_db("round-trip");
        let mut world = world_at(6);
        for (chunk, changes) in every_kind_of_change() {
            world.restore_changes(chunk, changes);
        }
        Sqlite::open(&path, content())
            .unwrap()
            .save_world(&world)
            .unwrap();
        let mut storage = Sqlite::open(&path, content()).unwrap();
        let back = storage.latest_world().unwrap().unwrap();
        for (chunk, changes) in every_kind_of_change() {
            assert_eq!(back.changes_in(chunk), Some(&changes));
            assert_eq!(back.chunk_version(chunk), changes.version);
        }
        assert_eq!(back.chunk_changes().count(), 2);
        drop(storage);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_v2_database_of_today_keeps_its_taken_items() {
        // A snapshot written by the code of 11 November 2026 (format v8,
        // schema v2): a small valley, four natural items picked up in
        // three chunks, their ids in the snapshot's `taken` list.
        const FIXTURE: &str = include_str!("../tests/fixtures/snapshot-v8.json");
        let taken: Vec<sim::ItemId> = serde_json::from_value(
            serde_json::from_str::<serde_json::Value>(FIXTURE).unwrap()["taken"].clone(),
        )
        .unwrap();
        assert_eq!(taken.len(), 4);
        let (dir, path) = temp_db("v2");
        {
            // The database as v2 made it: its first two migrations.
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.execute_batch(MIGRATIONS[1]).unwrap();
            conn.pragma_update(None, "user_version", 2).unwrap();
            conn.execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (4, 8, ?1)",
                [FIXTURE],
            )
            .unwrap();
        }
        let check = |world: &World| {
            let mut found: Vec<_> = world.taken().collect();
            found.sort_unstable();
            assert_eq!(found, taken);
            for &id in &taken {
                assert!(world.is_taken(id));
                let chunk = sim::items::chunk_of(id);
                assert!(
                    world
                        .items_in(chunk)
                        .iter()
                        .all(|i| i.id() != id || world.item(id).is_some())
                );
            }
            assert_eq!(world.chunk_changes().count(), 3, "three chunks");
            assert_eq!(
                world.chunk_changes().map(|(_, c)| c.version).sum::<u32>(),
                4,
                "a version an item taken"
            );
        };
        let mut storage = Sqlite::open(&path, content()).unwrap();
        assert_eq!(storage.schema_version().unwrap(), 4);
        let world = storage.latest_world().unwrap().unwrap();
        assert_eq!(world.tick(), Tick(4));
        check(&world);
        // The next checkpoint moves them into their chunks' rows, and the
        // snapshot no longer holds them.
        storage.save_world(&world).unwrap();
        let rows: i64 = storage
            .conn
            .query_row("SELECT count(*) FROM chunk_changes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 3);
        let latest: String = storage
            .conn
            .query_row(
                "SELECT data FROM snapshots WHERE format_version = ?1",
                [SNAPSHOT_FORMAT_VERSION],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!latest.contains("\"taken\""));
        drop(storage);
        let mut storage = Sqlite::open(&path, content()).unwrap();
        check(&storage.latest_world().unwrap().unwrap());
        drop(storage);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_checkpoint_writes_only_the_chunks_that_changed() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
        let written = |storage: &Sqlite| -> Vec<(i32, i32, u32, i64)> {
            storage
                .conn
                .prepare("SELECT x, y, version, tick FROM chunk_changes ORDER BY x, y")
                .unwrap()
                .query_map([], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let (a, b) = (ChunkPos::new(1, 1), ChunkPos::new(2, 1));
        let felled = |version| ChunkChanges {
            version,
            felled: [TreeKey::from_bits(u16::try_from(version).unwrap() << 3).unwrap()].into(),
            ..ChunkChanges::default()
        };
        let mut world = world_at(6);
        world.restore_changes(a, felled(1));
        world.restore_changes(b, felled(1));
        storage.save_world(&world).unwrap();
        assert_eq!(written(&storage), [(1, 1, 1, 6), (2, 1, 1, 6)]);
        // Only b changes: only b is written again.
        world.advance(6, &content(), &mut Vec::new());
        world.restore_changes(b, felled(2));
        storage.save_world(&world).unwrap();
        assert_eq!(written(&storage), [(1, 1, 1, 6), (2, 1, 2, 12)]);
        // Nothing men changed in them: they are not written again. (The
        // starving bodies die meanwhile: the items they lay change their
        // chunk, D4, which is written then.)
        world.advance(6, &content(), &mut Vec::new());
        storage.save_world(&world).unwrap();
        let rows = written(&storage);
        assert_eq!(
            rows.iter()
                .filter(|r| [(1, 1), (2, 1)].contains(&(r.0, r.1)))
                .collect::<Vec<_>>(),
            [&(1, 1, 1, 6), &(2, 1, 2, 12)]
        );
        assert!(
            rows.iter()
                .filter(|r| ![(1, 1), (2, 1)].contains(&(r.0, r.1)))
                .all(|r| r.2 == world.chunk_version(ChunkPos::new(r.0, r.1)) && r.3 == 18),
            "{rows:?}"
        );
        assert_eq!(storage.latest_world().unwrap().unwrap().tick(), Tick(18));
        // A chunk changed in the simulation itself (an item picked up) is
        // written at the next checkpoint.
        let mut world = storage.latest_world().unwrap().unwrap();
        let item = (0..8)
            .flat_map(|y| (0..8).map(move |x| ChunkPos::new(x, y)))
            .flat_map(|c| world.items_in(c))
            .find(|i| sim::items::is_natural(i.id()) && world.map().is_passable(i.pos().unwrap()))
            .expect("a natural item on walkable ground");
        let body = world.spawn_body(item.pos().unwrap());
        world.queue(sim::Intention::PickUp {
            body,
            item: item.id(),
        });
        world.step(&content(), &mut Vec::new());
        let chunk = item.pos().unwrap().chunk();
        storage.save_world(&world).unwrap();
        let rows = written(&storage);
        assert!(
            rows.contains(&(
                chunk.x,
                chunk.y,
                world.chunk_version(chunk),
                db_int(world.tick().0)
            )),
            "{rows:?}"
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r.3 == db_int(world.tick().0))
                .count(),
            1,
            "{rows:?}"
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
                supported: 4
            }
        ));
    }
}
