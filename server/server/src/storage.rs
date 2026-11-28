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
//! Apart from the world, in a file of its own ([`ContinentStore`], 19
//! November 2026): the continents of the atlas, born once and read back
//! at every later start, so a window is cut from its continent again
//! with no birth. Not the world's state: the same for every world of a
//! seed, kept across `--reset` and by ephemeral worlds.
//!
//! Formats are versioned from day one (D21): the database schema through
//! SQLite's `user_version`, each snapshot through its own format version,
//! each chunk's changes through theirs.

use std::collections::BTreeMap;
use std::sync::Arc;

use rusqlite::{Connection, OptionalExtension, params};
use sim::changes::{ChunkChanges, Construction, Picked, Planted, Scar, Stage};
use sim::crops::{PlantKey, Source};
use sim::ground::Version;
use sim::plan::Continent;
use sim::trees::TreeKey;
use sim::{CHUNK_SIZE, ChunkPos, Content, Pos, World};

/// Version of the serialized `World` inside a snapshot. Bump on any change
/// to `World`'s shape, and add a migration in `decode_world`.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 21;

/// Version of a chunk's changes as stored (`encode_changes`). Bump on any
/// change to their bytes, and read the old ones in `decode_changes`.
/// 2 (16 November 2026): the crops picked (D45), a fifth list.
/// 3 (27 November 2026, reshaped on the 28th before any world was stored
/// with it): the marks on the trunks (D57), a sixth: each tree's notch,
/// its bark lifted, the journée of the last mark and how long they take
/// to heal.
pub const CHUNK_CHANGES_FORMAT_VERSION: u32 = 3;

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
    /// The file, or what it holds, is not a save this server can use:
    /// a foreign SQLite file, a crafted value out of range. Refused with
    /// its reason, never adopted or guessed at.
    #[error("refused: {0}")]
    Refused(String),
    /// The storage thread panicked: a bug in the save path, reported as
    /// a failed save instead of silencing every later one.
    #[error("the storage thread panicked: {0}")]
    WriterPanicked(String),
    /// The file could not be created.
    #[error("cannot create {path}: {source}")]
    Create {
        /// The file.
        path: std::path::PathBuf,
        /// The error.
        #[source]
        source: std::io::Error,
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
    /// Whether the world was read from an older snapshot because the
    /// latest was damaged: the run then counts as a crash (D28, the
    /// project lead's decision of 20 November 2026).
    fell_back: bool,
    /// After a fallback, how many chunks of the land a checkpoint newer
    /// than the resumed snapshot wrote: their changes (what men took,
    /// felled or built) stay as the lost run left them, while what they
    /// gave is in nobody's hands (the second security audit of 20
    /// November 2026, C1; said, not undone: the lead's call of that day).
    land_lost: usize,
}

impl Sqlite {
    /// After the last read, how many chunks of the land a checkpoint newer
    /// than the resumed snapshot had written: zero unless the read fell
    /// back to an older snapshot (the second security audit of 20
    /// November 2026, C1).
    #[must_use]
    pub fn land_changes_lost(&self) -> usize {
        self.land_lost
    }

    /// Opens (or creates) the database file and brings its schema up to
    /// date. A new file is the owner's alone (mode 0600); a file that is
    /// not a Soul Invasion save is refused.
    pub fn open(path: &std::path::Path, content: Arc<Content>) -> Result<Self, StorageError> {
        create_private(path)?;
        Self::from_connection(
            Connection::open(path)?,
            content,
            &path.display().to_string(),
        )
    }

    /// An in-memory database: nothing survives the process.
    pub fn in_memory(content: Arc<Content>) -> Result<Self, StorageError> {
        Self::from_connection(Connection::open_in_memory()?, content, "memory")
    }

    fn from_connection(
        conn: Connection,
        content: Arc<Content>,
        label: &str,
    ) -> Result<Self, StorageError> {
        refuse_foreign(&conn, "snapshots", label)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let mut storage = Self {
            conn,
            content,
            written: BTreeMap::new(),
            fell_back: false,
            land_lost: 0,
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
    /// The latest snapshot; when it is damaged, the one before it (the
    /// kept ones are a margin for this, D28), the damaged rows dropped
    /// and the run counted as a crash. A snapshot of a newer format is
    /// not damage: it is refused, so an older build never buries what a
    /// newer one wrote.
    fn latest_world(&mut self) -> Result<Option<World>, StorageError> {
        use rusqlite::types::Value;
        self.land_lost = 0;
        let mut kept = self.conn.prepare(
            "SELECT rowid, tick, format_version, data FROM snapshots \
             ORDER BY tick DESC, rowid DESC LIMIT ?1",
        )?;
        let rows: Vec<(i64, Value, Value, Value)> = kept
            .query_map([KEPT_SNAPSHOTS], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<Result<_, _>>()?;
        drop(kept);
        let mut damaged: Vec<i64> = Vec::new();
        let mut first_error = None;
        let mut found = None;
        for (rowid, tick, version, data) in rows {
            match decode_row(&version, &data, &self.content) {
                Ok(world) => {
                    found = Some(world);
                    break;
                }
                Err(e) if is_damage(&e) => {
                    eprintln!("note: the snapshot at tick {tick:?} is damaged: {e}");
                    damaged.push(rowid);
                    first_error.get_or_insert(e);
                }
                Err(e) => return Err(e),
            }
        }
        let Some(mut world) = found else {
            return match first_error {
                None => Ok(None),
                Some(e) => Err(StorageError::Refused(format!(
                    "every kept snapshot is damaged, the latest: {e}"
                ))),
            };
        };
        if !damaged.is_empty() {
            // The chunks a later checkpoint changed: their rows are the
            // newest the lost run wrote, past the snapshot resumed.
            let lost: i64 = self.conn.query_row(
                "SELECT count(*) FROM chunk_changes WHERE tick > ?1",
                [db_int(world.tick().0)?],
                |row| row.get(0),
            )?;
            self.land_lost = usize::try_from(lost).unwrap_or(usize::MAX);
            eprintln!(
                "note: falling back to the snapshot at tick {}: {} damaged snapshot(s) dropped, \
                 the run counts as a crash; {} change(s) of the land lost (chunks changed after \
                 that tick keep their changes, what they gave is in nobody's hands)",
                world.tick().0,
                damaged.len(),
                lost
            );
            for rowid in damaged {
                self.conn
                    .execute("DELETE FROM snapshots WHERE rowid = ?1", [rowid])?;
            }
            self.fell_back = true;
        }
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
            params![db_int(world.tick().0)?, SNAPSHOT_FORMAT_VERSION, data],
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
                    db_int(world.tick().0)?,
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
        // A world read from an older snapshot lost what came after it,
        // as a crash does (D28, 20 November 2026).
        let epoch = if clean && !self.fell_back {
            epoch
        } else {
            epoch.checked_add(1).ok_or_else(|| {
                StorageError::Refused("the world's epoch is at its maximum".to_owned())
            })?
        };
        let lineage = Lineage {
            birth: birth.cast_unsigned(),
            epoch: u64::try_from(epoch).map_err(|_| {
                StorageError::Refused(format!("the world's epoch is negative ({epoch})"))
            })?,
        };
        tx.execute("UPDATE run_state SET epoch = ?1, clean = 0", [epoch])?;
        tx.commit()?;
        self.conn.pragma_update(None, "synchronous", "NORMAL")?;
        self.fell_back = false;
        Ok(lineage)
    }

    fn mark_clean_stop(&mut self) -> Result<(), StorageError> {
        self.conn.execute("UPDATE run_state SET clean = 1", [])?;
        Ok(())
    }
}

/// A chunk's changes as stored, format 3: six lists, each its length
/// then its entries, every number a LEB128 varint (a byte under 128).
/// Format 1 was the first four, format 2 the first five.
/// The taken slots and the felled trees' keys ([`TreeKey::bits`]) in
/// order, each as its distance from the one before: about a byte an
/// entry. A planted tree: its key, its species' id (length, then UTF-8),
/// its stage (0 to 3), its day. A construction: its kind's id, its tile
/// in the chunk (`y × 32 + x`), its day. A crop picked: what bore it (0
/// and a tree's key, or 1 and a plant's place in millimetres, east then
/// south), the material's id, the crop's year, the grams taken. A marked
/// trunk (D57, amended 27 and 28 November 2026), in the keys' order: the
/// tree's key, the square millimetres of its trunk's section cut
/// through, the pieces of bark lifted, the journée of the last mark, and
/// the journées from it to the one by which the marks have healed.
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
    varint(&mut out, changes.picked.len() as u64);
    for ((source, gives), picked) in &changes.picked {
        match source {
            Source::Tree(key) => {
                varint(&mut out, 0);
                varint(&mut out, u64::from(key.bits()));
            }
            Source::Plant(key) => {
                varint(&mut out, 1);
                varint(&mut out, u64::from(key.x_mm));
                varint(&mut out, u64::from(key.y_mm));
            }
        }
        text(&mut out, gives);
        varint(&mut out, picked.year);
        varint(&mut out, u64::from(picked.grams));
    }
    varint(&mut out, changes.scars.len() as u64);
    for (key, scar) in &changes.scars {
        varint(&mut out, u64::from(key.bits()));
        varint(&mut out, u64::from(scar.notch_mm2));
        varint(&mut out, u64::from(scar.barked));
        varint(&mut out, scar.day);
        varint(&mut out, scar.whole_day.saturating_sub(scar.day));
    }
    out
}

/// The changes [`encode_changes`] wrote for `chunk` at `version`, in
/// `format` (1 to 3); `None` if the bytes are damaged or the format
/// unknown. An older format has no marked trunk: none is read.
#[allow(
    clippy::too_many_lines,
    reason = "one straight reading, top to bottom; splitting it would scatter the rule"
)]
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
    if !(1..=CHUNK_CHANGES_FORMAT_VERSION).contains(&format) {
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
    if format >= 2 {
        let side_mm = side * 1000;
        for _ in 0..bytes.count()? {
            let source = match bytes.varint()? {
                0 => Source::Tree(TreeKey::from_bits(u16::try_from(bytes.varint()?).ok()?)?),
                1 => {
                    let (x, y) = (bytes.varint()?, bytes.varint()?);
                    if x >= side_mm || y >= side_mm {
                        return None;
                    }
                    Source::Plant(PlantKey {
                        x_mm: u16::try_from(x).ok()?,
                        y_mm: u16::try_from(y).ok()?,
                    })
                }
                _ => return None,
            };
            let gives = bytes.text()?;
            let year = bytes.varint()?;
            let grams = u32::try_from(bytes.varint()?).ok()?;
            changes
                .picked
                .insert((source, gives), Picked { year, grams });
        }
    }
    if format >= 3 {
        for _ in 0..bytes.count()? {
            let key = TreeKey::from_bits(u16::try_from(bytes.varint()?).ok()?)?;
            let notch_mm2 = u32::try_from(bytes.varint()?).ok()?;
            let barked = u32::try_from(bytes.varint()?).ok()?;
            let day = bytes.varint()?;
            let whole_day = day.checked_add(bytes.varint()?)?;
            let scar = Scar {
                notch_mm2,
                barked,
                day,
                whole_day,
            };
            // A mark is of a tree standing, each tree once, and is of
            // something.
            if (notch_mm2 == 0 && barked == 0)
                || changes.felled.contains(&key)
                || changes.scars.insert(key, scar).is_some()
            {
                return None;
            }
        }
    }
    bytes.0.is_empty().then_some(changes)
}

/// SQLite integers are signed 64-bit; ticks never get anywhere near the
/// sign bit (29 billion years at ten a second). A crafted one is refused.
fn db_int(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value)
        .map_err(|_| StorageError::Refused(format!("tick {value} is beyond the database's range")))
}

/// A stored snapshot's row as a world, whatever its columns hold: a
/// column of the wrong type is damage, like unreadable data.
fn decode_row(
    version: &rusqlite::types::Value,
    data: &rusqlite::types::Value,
    content: &Content,
) -> Result<World, StorageError> {
    use rusqlite::types::Value;
    let (Value::Integer(version), Value::Text(data)) = (version, data) else {
        return Err(StorageError::Refused(
            "the snapshot's columns are not an integer and a text".to_owned(),
        ));
    };
    let version = u32::try_from(*version).map_err(|_| StorageError::UnsupportedSnapshot {
        found: 0,
        supported: SNAPSHOT_FORMAT_VERSION,
    })?;
    let world = decode_world(version, data, content)?;
    // Never written by this server: a tick it could not save again.
    db_int(world.tick().0)?;
    Ok(world)
}

/// Whether a snapshot's error means the row is damaged (unreadable,
/// crafted, of no format any build wrote), so the one before it may be
/// read instead; a format newer than this build is not damage.
fn is_damage(e: &StorageError) -> bool {
    match e {
        StorageError::Serialization(_) | StorageError::Refused(_) => true,
        StorageError::UnsupportedSnapshot { found, .. } => *found == 0,
        _ => false,
    }
}

/// Refuses a file that is not this server's: a SQLite file of another
/// program (tables of its own and no schema version), or one that claims
/// a version without the `table` every version has. Checked before the
/// file is touched (no journal mode set, no table added).
fn refuse_foreign(conn: &Connection, table: &str, label: &str) -> Result<(), StorageError> {
    let version: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let tables: u32 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table'",
        [],
        |row| row.get(0),
    )?;
    let ours: bool = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get::<_, u32>(0).map(|n| n > 0),
    )?;
    if tables > 0 && (version == 0 || !ours) {
        return Err(StorageError::Refused(format!(
            "{label} is not a Soul Invasion save (schema version {version}, {tables} table(s))"
        )));
    }
    Ok(())
}

/// Makes the file if it is missing, readable by its owner alone (0600):
/// a world, and later the souls, are nobody else's on a shared host.
/// SQLite gives the file 0644 when it makes it, and its WAL and shared
/// memory files take the main file's mode.
fn create_private(path: &std::path::Path) -> Result<(), StorageError> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(source) => Err(StorageError::Create {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Schema of the continents' file, as [`MIGRATIONS`] for the world's:
/// the index + 1 is the version in its `user_version`.
const CONTINENT_MIGRATIONS: &[&str] = &[
    // v1 (19 November 2026): a continent a seed and side. `data` is
    // `Continent::to_bytes` (its own layout version and checksum inside),
    // `fingerprint` `Continent::fingerprint`, checked on reading.
    "CREATE TABLE continents (
         seed        INTEGER NOT NULL,
         side        INTEGER NOT NULL,
         fingerprint INTEGER NOT NULL,
         data        BLOB    NOT NULL,
         saved_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
         PRIMARY KEY (seed, side)
     );",
];

/// The continents of the atlas born once and kept (D28, 19 November
/// 2026), in a SQLite file of their own: the vivarium's continent (1,024
/// cells a side, about 60 MB) takes 6 to 13 s and 140 MB at its peak to
/// be born, and is the same at every start of a world of its seed.
#[derive(Debug)]
pub struct ContinentStore {
    conn: Connection,
}

impl ContinentStore {
    /// Opens (or creates) the continents' file, the owner's alone when
    /// made here; a file that is not one of these is refused.
    pub fn open(path: &std::path::Path) -> Result<Self, StorageError> {
        create_private(path)?;
        Self::from_connection(Connection::open(path)?, &path.display().to_string())
    }

    /// In memory, for the tests.
    pub fn in_memory() -> Result<Self, StorageError> {
        Self::from_connection(Connection::open_in_memory()?, "memory")
    }

    fn from_connection(mut conn: Connection, label: &str) -> Result<Self, StorageError> {
        refuse_foreign(&conn, "continents", label)?;
        let found: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let supported = u32::try_from(CONTINENT_MIGRATIONS.len()).expect("few migrations");
        if found > supported {
            return Err(StorageError::SchemaTooNew { found, supported });
        }
        for (index, sql) in CONTINENT_MIGRATIONS.iter().enumerate().skip(found as usize) {
            let version = u32::try_from(index + 1).expect("few migrations");
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", version)?;
            tx.commit()?;
        }
        Ok(Self { conn })
    }

    /// The continent of `seed`, `side` cells a side, kept whole and
    /// undamaged; `None` when there is none, or when it is damaged (it is
    /// then born again, and kept in its place).
    pub fn load(&self, seed: u64, side: usize) -> Result<Option<Continent>, StorageError> {
        let side = i64::try_from(side).expect("a small side");
        let row: Option<(i64, Vec<u8>)> = self
            .conn
            .query_row(
                "SELECT fingerprint, data FROM continents WHERE seed = ?1 AND side = ?2",
                params![db_bits(seed), side],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((fingerprint, data)) = row else {
            return Ok(None);
        };
        let whole = Continent::from_bytes(&data).filter(|c| {
            c.seed == seed
                && i64::try_from(c.grid.side) == Ok(side)
                && db_bits(c.fingerprint()) == fingerprint
        });
        if whole.is_none() {
            println!("note: the kept continent of seed {seed} is damaged: born again");
        }
        Ok(whole)
    }

    /// Keeps `continent`, in place of any kept of its seed and side.
    pub fn keep(&self, continent: &Continent) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO continents (seed, side, fingerprint, data)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                db_bits(continent.seed),
                i64::try_from(continent.grid.side).expect("a small side"),
                db_bits(continent.fingerprint()),
                continent.to_bytes(),
            ],
        )?;
        Ok(())
    }
}

/// A seed or a fingerprint as SQLite's signed integer, bit for bit.
fn db_bits(value: u64) -> i64 {
    i64::from_le_bytes(value.to_le_bytes())
}

/// Turns stored snapshot data into a `World`, migrating old formats. A
/// format no build wrote (0) or newer than this one is refused.
fn decode_world(format_version: u32, data: &str, content: &Content) -> Result<World, StorageError> {
    let unsupported = || StorageError::UnsupportedSnapshot {
        found: format_version,
        supported: SNAPSHOT_FORMAT_VERSION,
    };
    if format_version == 0 {
        return Err(unsupported());
    }
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
            9 => migrate_v9_to_v10(&data)?,
            10 => migrate_v10_to_v11(&data, content)?,
            11 => migrate_v11_to_v12(&data)?,
            12 => migrate_v12_to_v13(&data)?,
            13 => migrate_v13_to_v14(&data)?,
            14 => migrate_v14_to_v15(&data)?,
            15 => migrate_v15_to_v16(data),
            16 => migrate_v16_to_v17(data),
            17 => migrate_v17_to_v18(&data)?,
            18 => migrate_v18_to_v19(data),
            19 => migrate_v19_to_v20(&data, content)?,
            20 => migrate_v20_to_v21(data),
            _ => return Err(unsupported()),
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
        Err(unsupported())
    }
}

/// v1 held only the calendar and the tick; v2 adds the map and the
/// bodies. A v1 world gets the standard valley, nobody in it yet, at its
/// old tick.
fn migrate_v1_to_v2(data: &str, content: &Content) -> Result<String, StorageError> {
    standard_world_at_old_tick(data, content)
}

/// v2 stored the 32×32 map tile by tile; v3 stores how the map is
/// generated, and the valley is 256×256 at one metre per tile (D31).
/// Positions of the old valley mean nothing in the new one: a v2 world
/// gets the standard valley, nobody in it yet, at its old tick.
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

/// v10: a valley stores the version of the generator that made it (D24,
/// amended 13 November 2026). A valley saved without one was made by
/// the first, and keeps it for life: no rock under its ground.
fn migrate_v9_to_v10(data: &str) -> Result<String, StorageError> {
    let mut old: serde_json::Value = serde_json::from_str(data)?;
    if let Some(map) = old["map"].as_object_mut()
        && map.get("kind").and_then(serde_json::Value::as_str) == Some("valley")
        && !map.contains_key("generator")
    {
        map.insert("generator".into(), sim::map::FIRST_GENERATOR.into());
    }
    Ok(old.to_string())
}

/// v11: an item weighs its own whole grams and keeps the tick it was made
/// (D35, amended 13 November 2026). An item saved before weighs its
/// material's piece, as it did, and is as old as the world (tick 0).
fn migrate_v10_to_v11(data: &str, content: &Content) -> Result<String, StorageError> {
    let mut old: serde_json::Value = serde_json::from_str(data)?;
    if let Some(items) = old["items"].as_array_mut() {
        for item in items
            .iter_mut()
            .filter_map(serde_json::Value::as_object_mut)
        {
            if !item.contains_key("mass_g") {
                let grams = item
                    .get("material")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|m| content.material(m))
                    .map_or(1, sim::Material::mass_g);
                item.insert("mass_g".into(), grams.into());
            }
            item.entry("made").or_insert_with(|| 0.into());
        }
    }
    Ok(old.to_string())
}

/// v12: the calendar has a year (D09, amended 13 November 2026) and
/// bodies thirst (D34, amended the same day). A world saved before gets
/// the provisional year of 50 days, pinned (never the default, which
/// may change), and its bodies have just drunk.
fn migrate_v11_to_v12(data: &str) -> Result<String, StorageError> {
    let mut old: serde_json::Value = serde_json::from_str(data)?;
    if let Some(calendar) = old["calendar"].as_object_mut() {
        calendar
            .entry("days_per_year")
            .or_insert_with(|| sim::Calendar::LEGACY_DAYS_PER_CYCLE.into());
    }
    let quenched = serde_json::to_value(sim::Condition::healthy().thirst)?;
    if let Some(bodies) = old["bodies"].as_array_mut() {
        for condition in bodies
            .iter_mut()
            .filter_map(|b| b.get_mut("condition"))
            .filter_map(serde_json::Value::as_object_mut)
        {
            condition
                .entry("thirst")
                .or_insert_with(|| quenched.clone());
        }
    }
    Ok(old.to_string())
}

/// v13: a body may be busy at work that lasts (D45, gathering). A world
/// saved before has every body idle.
fn migrate_v12_to_v13(data: &str) -> Result<String, StorageError> {
    let mut old: serde_json::Value = serde_json::from_str(data)?;
    if let Some(bodies) = old["bodies"].as_array_mut() {
        for body in bodies
            .iter_mut()
            .filter_map(serde_json::Value::as_object_mut)
        {
            body.entry("work").or_insert(serde_json::Value::Null);
        }
    }
    Ok(old.to_string())
}

/// v14: a body has a fat reserve (lot 3), what it misses and what it
/// ate today. A world saved before has every reserve full.
fn migrate_v13_to_v14(data: &str) -> Result<String, StorageError> {
    let mut old: serde_json::Value = serde_json::from_str(data)?;
    if let Some(bodies) = old["bodies"].as_array_mut() {
        for condition in bodies
            .iter_mut()
            .filter_map(|body| body["condition"].as_object_mut())
        {
            condition.entry("deficit_j").or_insert(0.into());
            condition.entry("eaten_today_j").or_insert(0.into());
        }
    }
    Ok(old.to_string())
}

/// v15: a body has a birth day and a sex (lot 3 B). A body saved before
/// is a grown one, twenty to thirty years old today, a woman or a man,
/// both drawn from its id so the same world migrates the same.
fn migrate_v14_to_v15(data: &str) -> Result<String, StorageError> {
    let mut old: serde_json::Value = serde_json::from_str(data)?;
    let calendar: sim::Calendar = serde_json::from_value(old["calendar"].clone())?;
    let tick = old["tick"].as_u64().unwrap_or(0);
    let today = i64::try_from(tick / calendar.ticks_per_day().max(1)).unwrap_or(0);
    let per_year = i64::from(calendar.days_per_age_year());
    if let Some(bodies) = old["bodies"].as_array_mut() {
        for body in bodies.iter_mut().filter_map(|b| b.as_object_mut()) {
            let id = body
                .get("id")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            // A plain mix of the id (splitmix64's finaliser).
            let mut h = id.wrapping_add(0x9e37_79b9_7f4a_7c15);
            h = (h ^ (h >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            h = (h ^ (h >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            h ^= h >> 31;
            let days =
                (20 * per_year) + i64::try_from(h % 1000).unwrap_or(0) * 10 * per_year / 1000;
            body.entry("born_day").or_insert((today - days).into());
            let sex = if (h >> 32) & 1 == 0 { "Female" } else { "Male" };
            body.entry("sex").or_insert(sex.into());
        }
    }
    Ok(old.to_string())
}

/// v16: a body has a mother, a father, a spouse and the child it carries
/// unborn, and the world whether the life rule plays (lot 3 C, D49). A
/// world saved before has none of them and the rule off, which is what
/// the fields read as when absent: the data is the same.
fn migrate_v15_to_v16(data: String) -> String {
    data
}

/// v17: a body counts the dawns in a row its spouse was too far, and
/// the couple parts at the content's count (D49, amended 24 November
/// 2026). A world saved before counts none, which is what the field reads
/// as when absent: the data is the same.
fn migrate_v16_to_v17(data: String) -> String {
    data
}

/// v18: the calendar counts its age-years in a cycle of seasons (D09,
/// amended 24 November 2026: three clocks). A world saved before ages one
/// age-year a cycle, as it always did: `age_years_per_cycle` is written
/// as the legacy value, pinned, never read from a default that may change.
fn migrate_v17_to_v18(data: &str) -> Result<String, StorageError> {
    let mut old: serde_json::Value = serde_json::from_str(data)?;
    if let Some(calendar) = old["calendar"].as_object_mut() {
        calendar
            .entry("age_years_per_cycle")
            .or_insert_with(|| sim::Calendar::LEGACY_AGE_YEARS_PER_CYCLE.into());
    }
    Ok(old.to_string())
}

/// v19: illness as episodes (D49, amended 26 November 2026): a body's
/// episode and its immunities, the world's fouled places. A world saved
/// before has every body healthy, never ill, and every place clean, which
/// is what the fields read as when absent: the data is the same.
fn migrate_v18_to_v19(data: String) -> String {
    data
}

/// v21: "put in" is a gesture and nothing holds by itself (D57, amended
/// 27 November 2026): an item may lie `within` a hollow thing carried by
/// the same body, and a put-in under way names what it puts `into`. A
/// world saved before has nothing put in anything, which is what the
/// fields read as when absent: the data is the same. Since 28 November
/// 2026 an item may also lie `within` a hollow laid on its own tile: the
/// same field, no new version. A body that carried
/// food beyond its arms beside a hollow holds it all in its arms again,
/// and takes no more till they have room.
fn migrate_v20_to_v21(data: String) -> String {
    data
}

/// The hollow the repository's birch bark took when folded, in
/// millilitres, when v20 came: for a content that holds no birch bark.
const LEGACY_FOLDED_BARK_ML: u32 = 4740;

/// v20: a fold gives a thing a hollow, a form of the thing, instead of
/// turning it into another material (D57, amended 27 November 2026): an
/// item may carry `hollow_ml`. A world saved before holds its folded
/// barks as items of the material `folded_birch_bark`, which is gone:
/// each becomes a birch bark with the hollow a fold gives it today.
fn migrate_v19_to_v20(data: &str, content: &Content) -> Result<String, StorageError> {
    fn walk(value: &mut serde_json::Value, hollow_ml: u32) {
        match value {
            serde_json::Value::Object(map) => {
                if map.get("material").and_then(serde_json::Value::as_str)
                    == Some("folded_birch_bark")
                {
                    map.insert("material".to_owned(), "birch_bark".into());
                    map.insert("hollow_ml".to_owned(), hollow_ml.into());
                }
                for inner in map.values_mut() {
                    walk(inner, hollow_ml);
                }
            }
            serde_json::Value::Array(list) => {
                for inner in list {
                    walk(inner, hollow_ml);
                }
            }
            _ => {}
        }
    }
    let hollow_ml = match content
        .material("birch_bark")
        .map(|m| m.folded(content.needs.hands.fold_suppleness))
    {
        Some(sim::content::Folded::Hollow(ml)) => ml,
        _ => LEGACY_FOLDED_BARK_ML,
    };
    let mut old: serde_json::Value = serde_json::from_str(data)?;
    walk(&mut old, hollow_ml);
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
        let mut storage = Sqlite::from_connection(conn, content(), "test").unwrap();
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
        assert_eq!(
            world.calendar(),
            &Calendar::new(2, 3).unwrap().with_cycle(50, 1).unwrap(),
            "the legacy cycle, pinned"
        );
        assert!(
            world.bodies().is_empty(),
            "the standard valley, nobody in it yet"
        );
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
        assert_eq!(
            world.calendar(),
            &Calendar::new(2, 3).unwrap().with_cycle(50, 1).unwrap(),
            "the legacy cycle, pinned"
        );
        assert_eq!(world.map().width(), sim::VALLEY_SIZE);
        assert!(
            world.bodies().is_empty(),
            "the standard valley, nobody in it yet"
        );
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
    #[test]
    fn a_continent_is_kept_whole_and_a_damaged_one_left_out() {
        let store = ContinentStore::in_memory().unwrap();
        assert!(store.load(1, 64).unwrap().is_none());
        let (a, b) = (Continent::with(1, 64, None), Continent::with(2, 64, None));
        store.keep(&a).unwrap();
        store.keep(&b).unwrap();
        // Kept again: replaced, not doubled.
        store.keep(&a).unwrap();
        let count: i64 = store
            .conn
            .query_row("SELECT count(*) FROM continents", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(store.load(1, 64).unwrap().unwrap().to_bytes(), a.to_bytes());
        assert_eq!(store.load(2, 64).unwrap().unwrap().to_bytes(), b.to_bytes());
        assert!(store.load(1, 128).unwrap().is_none(), "another side");
        // A wrong fingerprint, or damaged bytes: left out, to be born again.
        store
            .conn
            .execute("UPDATE continents SET fingerprint = 7 WHERE seed = 1", [])
            .unwrap();
        store
            .conn
            .execute("UPDATE continents SET data = x'00' WHERE seed = 2", [])
            .unwrap();
        assert!(store.load(1, 64).unwrap().is_none());
        assert!(store.load(2, 64).unwrap().is_none());
        // A newer file is refused.
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 9).unwrap();
        assert!(matches!(
            ContinentStore::from_connection(conn, "test").unwrap_err(),
            StorageError::SchemaTooNew {
                found: 9,
                supported: 1
            }
        ));
    }

    /// The vivarium's world made again on its continent kept in the
    /// continents' file is the world its birth gave, tile for tile, and
    /// nothing is born.
    #[test]
    fn a_kept_continent_gives_back_the_world_its_birth_gave() {
        use crate::cli::TestWorld;
        use crate::test_world::make;
        use sim::plan::{CONTINENT_SIDE, VIVARIUM_SEED, born_within};
        let (dir, path) = temp_db("continent");
        let calendar = Calendar::new(2, 3).unwrap();
        let (world, born) = born_within(Vec::new(), || {
            make(TestWorld::Vivarium, calendar, &content())
        });
        assert_eq!(born.len(), 1, "the vivarium's continent born");
        ContinentStore::open(&path).unwrap().keep(&born[0]).unwrap();
        let kept = ContinentStore::open(&path)
            .unwrap()
            .load(VIVARIUM_SEED, CONTINENT_SIDE)
            .unwrap()
            .expect("kept");
        assert_eq!(kept.to_bytes(), born[0].to_bytes());
        drop(born);
        let (again, born) = born_within(vec![kept], || {
            make(TestWorld::Vivarium, calendar, &content())
        });
        assert!(born.is_empty(), "nothing born again");
        assert_eq!(again.map().spec(), world.map().spec());
        // Every fourth chunk each way, tile for tile.
        let (a, b) = (world.map(), again.map());
        for cy in (0..a.chunks_high()).step_by(4) {
            for cx in (0..a.chunks_wide()).step_by(4) {
                let chunk = ChunkPos::new(i32::try_from(cx).unwrap(), i32::try_from(cy).unwrap());
                assert_eq!(a.chunk_heights(chunk), b.chunk_heights(chunk));
                for k in 0..CHUNK_SIZE * CHUNK_SIZE {
                    let pos = Pos::new(
                        chunk.x * CHUNK_SIZE.cast_signed() + (k % CHUNK_SIZE).cast_signed(),
                        chunk.y * CHUNK_SIZE.cast_signed() + (k / CHUNK_SIZE).cast_signed(),
                    );
                    assert_eq!(a.get(pos), b.get(pos), "{pos:?}");
                    assert_eq!(a.depth(pos), b.depth(pos), "{pos:?}");
                    assert_eq!(a.current(pos), b.current(pos), "{pos:?}");
                    assert_eq!(a.trees(pos), b.trees(pos), "{pos:?}");
                }
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

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
        a.scars.insert(
            key(8),
            Scar {
                notch_mm2: 1,
                barked: 0,
                day: 0,
                whole_day: 4000,
            },
        );
        a.scars.insert(
            key(0x0890),
            Scar {
                notch_mm2: u32::MAX,
                barked: u32::MAX,
                day: 70_000,
                whole_day: 70_128,
            },
        );
        a.scars.insert(
            key(2 << 11 | 0x0105),
            Scar {
                notch_mm2: 0,
                barked: 3,
                day: 12,
                whole_day: 108,
            },
        );
        a.picked.insert(
            (Source::Tree(key(0x0888)), "hazelnuts".to_owned()),
            Picked {
                year: 3,
                grams: 4200,
            },
        );
        a.picked.insert(
            (
                Source::Plant(PlantKey {
                    x_mm: 31_999,
                    y_mm: 0,
                }),
                "blackberries".to_owned(),
            ),
            Picked {
                year: u64::MAX,
                grams: u32::MAX,
            },
        );
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
    fn chunk_changes_of_format_1_are_read_with_nothing_picked() {
        // Format 1 was format 2 without its fifth list.
        for (chunk, mut changes) in every_kind_of_change() {
            changes.picked.clear();
            changes.scars.clear();
            let mut bytes = encode_changes(chunk, &changes);
            assert_eq!(bytes.pop(), Some(0), "an empty sixth list");
            assert_eq!(bytes.pop(), Some(0), "an empty fifth list");
            let back = decode_changes(chunk, changes.version, 1, &bytes);
            assert_eq!(back.as_ref(), Some(&changes));
        }
        // A plant's place outside its chunk is damage.
        let mut far = ChunkChanges::default();
        far.picked.insert(
            (
                Source::Plant(PlantKey {
                    x_mm: 32_000,
                    y_mm: 0,
                }),
                "x".to_owned(),
            ),
            Picked { year: 0, grams: 1 },
        );
        let mut bytes = encode_changes(ChunkPos::new(0, 0), &far);
        assert_eq!(bytes.pop(), Some(0), "an empty sixth list");
        assert_eq!(decode_changes(ChunkPos::new(0, 0), 1, 2, &bytes), None);
    }

    #[test]
    fn chunk_changes_of_format_2_are_read_with_no_notch() {
        // Format 2 was format 3 without its sixth list (D57, amended 27
        // November 2026): a world stored before the notches is read as it
        // was, no tree cut into.
        for (chunk, mut changes) in every_kind_of_change() {
            assert!(
                chunk.x < 0 || !changes.scars.is_empty(),
                "a marked trunk among the changes"
            );
            let with = encode_changes(chunk, &changes);
            changes.scars.clear();
            let mut bytes = encode_changes(chunk, &changes);
            assert_eq!(bytes.pop(), Some(0), "an empty sixth list");
            let back = decode_changes(chunk, changes.version, 2, &bytes);
            assert_eq!(back.as_ref(), Some(&changes));
            // The sixth list is not of format 2.
            if with.len() > bytes.len() + 1 {
                assert_eq!(decode_changes(chunk, changes.version, 2, &with), None);
            }
        }
        // A mark on a felled tree, two on one tree, or a mark of nothing
        // are damage.
        let key = TreeKey::from_bits(0x0888).unwrap();
        let mut felled = ChunkChanges::default();
        felled.felled.insert(key);
        let mut bytes = encode_changes(ChunkPos::new(0, 0), &felled);
        assert_eq!(bytes.pop(), Some(0));
        let (lo, hi) = (0x08_u8 | 0x80, 0x11_u8);
        bytes.extend([1, lo, hi, 5, 0, 3, 9]);
        assert_eq!(decode_changes(ChunkPos::new(0, 0), 1, 3, &bytes), None);
        let mut twice = encode_changes(ChunkPos::new(0, 0), &ChunkChanges::default());
        assert_eq!(twice.pop(), Some(0));
        let empty = twice.clone();
        twice.extend([2, lo, hi, 5, 0, 3, 9, lo, hi, 6, 0, 3, 9]);
        assert_eq!(decode_changes(ChunkPos::new(0, 0), 1, 3, &twice), None);
        let mut nothing = empty.clone();
        nothing.extend([1, lo, hi, 0, 0, 3, 9]);
        assert_eq!(decode_changes(ChunkPos::new(0, 0), 1, 3, &nothing), None);
        // The same tree marked once is read: its notch, its bark, the
        // journée, and the journée it is whole again.
        let mut marked = empty;
        marked.extend([1, lo, hi, 5, 2, 3, 9]);
        let read = decode_changes(ChunkPos::new(0, 0), 1, 3, &marked).expect("read");
        assert_eq!(
            read.scars[&key],
            Scar {
                notch_mm2: 5,
                barked: 2,
                day: 3,
                whole_day: 12
            }
        );
    }

    #[test]
    fn a_tree_healed_since_is_forgotten_when_the_world_is_loaded() {
        // D57, amended 28 November 2026: a marked tree is kept until the
        // journée it is whole again; loaded on or after it, it is a tree
        // among the others, and its chunk keeps its version.
        let (dir, path) = temp_db("healed");
        let mut world = world_at(6);
        let today = u64::try_from(world.today()).unwrap();
        let chunk = ChunkPos::new(1, 1);
        let key = |bits: u16| TreeKey::from_bits(bits).unwrap();
        let scar = |whole_day| Scar {
            notch_mm2: 1320,
            barked: 2,
            day: 0,
            whole_day,
        };
        let mut changes = ChunkChanges {
            version: 5,
            ..ChunkChanges::default()
        };
        changes.scars.insert(key(8), scar(today + 1));
        let kept = changes.clone();
        world.restore_changes(chunk, changes.clone());
        assert_eq!(world.changes_in(chunk), Some(&kept));
        // Stored with a tree whose marks have healed by today.
        changes.scars.insert(key(0x0890), scar(today));
        let bytes = encode_changes(chunk, &changes);
        let read = decode_changes(chunk, 5, CHUNK_CHANGES_FORMAT_VERSION, &bytes).unwrap();
        assert_eq!(read.scars.len(), 2, "the bytes keep both");
        world.restore_changes(chunk, read);
        assert_eq!(world.changes_in(chunk), Some(&kept));
        // Through the database: the same.
        Sqlite::open(&path, content())
            .unwrap()
            .save_world(&world)
            .unwrap();
        let back = Sqlite::open(&path, content())
            .unwrap()
            .latest_world()
            .unwrap()
            .unwrap();
        assert_eq!(back.changes_in(chunk), Some(&kept));
        assert_eq!(back.chunk_version(chunk), 5);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn chunk_changes_round_trip_through_sqlite() {
        // The bytes alone, every kind of change, and nothing at all.
        for (chunk, changes) in every_kind_of_change() {
            let bytes = encode_changes(chunk, &changes);
            let back = decode_changes(chunk, changes.version, CHUNK_CHANGES_FORMAT_VERSION, &bytes);
            assert_eq!(back.as_ref(), Some(&changes));
            // Damaged bytes are refused, never misread.
            let format = CHUNK_CHANGES_FORMAT_VERSION;
            for cut in 0..bytes.len() {
                assert_eq!(
                    decode_changes(chunk, 1, format, &bytes[..cut]),
                    None,
                    "cut at {cut}"
                );
            }
            let mut longer = bytes.clone();
            longer.push(0);
            assert_eq!(decode_changes(chunk, 1, format, &longer), None);
            assert_eq!(decode_changes(chunk, 1, 4, &bytes), None, "unknown format");
        }
        let none = ChunkChanges::default();
        assert_eq!(
            encode_changes(ChunkPos::new(0, 0), &none),
            [0, 0, 0, 0, 0, 0]
        );
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
    fn items_saved_before_their_own_grams_weigh_their_material() {
        // A v10 snapshot: its items carry neither grams nor a date.
        let mut world = world_at(3);
        let flint = world.spawn_item("flint", 123, Pos::new(1, 1));
        let mut old: serde_json::Value = serde_json::to_value(&world).unwrap();
        for item in old["items"].as_array_mut().unwrap() {
            let item = item.as_object_mut().unwrap();
            assert!(item.remove("mass_g").is_some() && item.remove("made").is_some());
        }
        let mut storage = Sqlite::in_memory(content()).unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (3, 10, ?1)",
                [old.to_string()],
            )
            .unwrap();
        let back = storage.latest_world().unwrap().unwrap();
        let item = back.item(flint).expect("the flint");
        assert_eq!(item.mass_g(), 400, "the material's piece");
        assert_eq!(item.made(), Tick(0));
        // Saved again in today's format: its grams and date kept.
        storage.save_world(&back).unwrap();
        let again = storage.latest_world().unwrap().unwrap();
        assert_eq!(again.item(flint), back.item(flint));
        // Today's items keep their own grams and date.
        let mut storage = Sqlite::in_memory(content()).unwrap();
        storage.save_world(&world).unwrap();
        let today = storage.latest_world().unwrap().unwrap();
        let item = today.item(flint).expect("the flint");
        assert_eq!((item.mass_g(), item.made()), (123, Tick(3)));
    }

    #[test]
    fn a_world_saved_before_the_year_gets_fifty_days_and_bodies_quenched() {
        // A v11 snapshot: its calendar has no year, its bodies no thirst.
        let mut world = World::with_valley(
            Calendar::new(2, 3).unwrap().with_cycle(12, 1).unwrap(),
            &content(),
            256,
        );
        let body = world.spawn_at_camp().expect("a body at the camp");
        let mut events = Vec::new();
        world.advance(4, &content(), &mut events); // thirsty by now
        assert!(world.body(body).unwrap().condition().thirst.points() > 0);
        let mut old: serde_json::Value = serde_json::to_value(&world).unwrap();
        assert!(
            old["calendar"]
                .as_object_mut()
                .unwrap()
                .remove("days_per_year")
                .is_some()
        );
        for body in old["bodies"].as_array_mut().unwrap() {
            let condition = body["condition"].as_object_mut().unwrap();
            assert!(condition.remove("thirst").is_some());
        }
        let mut storage = Sqlite::in_memory(content()).unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (4, 11, ?1)",
                [old.to_string()],
            )
            .unwrap();
        let back = storage.latest_world().unwrap().unwrap();
        assert_eq!(back.calendar().days_per_cycle(), 50);
        assert_eq!(back.calendar().ticks_per_day(), 6);
        assert_eq!(back.body(body).unwrap().condition().thirst.points(), 0);
        // Today's worlds keep their own year and thirst.
        let mut storage = Sqlite::in_memory(content()).unwrap();
        storage.save_world(&world).unwrap();
        let today = storage.latest_world().unwrap().unwrap();
        assert_eq!(today.calendar().days_per_cycle(), 12);
        assert_eq!(
            today.body(body).unwrap().condition(),
            world.body(body).unwrap().condition()
        );
    }

    #[test]
    fn a_v12_world_has_its_bodies_idle() {
        let content = content();
        let mut world = World::with_valley(Calendar::new(2, 3).unwrap(), &content, 256);
        let body = world.spawn_at_camp().expect("a body at the camp");
        let mut old: serde_json::Value = serde_json::to_value(&world).unwrap();
        for body in old["bodies"].as_array_mut().unwrap() {
            assert!(body.as_object_mut().unwrap().remove("work").is_some());
        }
        let mut storage = Sqlite::in_memory(content.clone()).unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (0, 12, ?1)",
                [old.to_string()],
            )
            .unwrap();
        let back = storage.latest_world().unwrap().unwrap();
        assert!(back.body(body).unwrap().work().is_none());
        // Today's worlds write the field, and read it back.
        let json = serde_json::to_value(&world).unwrap();
        assert_eq!(json["bodies"][0]["work"], serde_json::Value::Null);
        let mut storage = Sqlite::in_memory(content).unwrap();
        storage.save_world(&world).unwrap();
        let today = storage.latest_world().unwrap().unwrap();
        assert_eq!(
            today.body(body).unwrap().work(),
            world.body(body).unwrap().work()
        );
    }

    #[test]
    fn a_v13_world_has_its_reserves_full() {
        let content = content();
        let mut world = World::with_valley(Calendar::new(2, 3).unwrap(), &content, 256);
        let body = world.spawn_at_camp().expect("a body at the camp");
        let mut old: serde_json::Value = serde_json::to_value(&world).unwrap();
        for body in old["bodies"].as_array_mut().unwrap() {
            let condition = body["condition"].as_object_mut().unwrap();
            assert!(condition.remove("deficit_j").is_some());
            assert!(condition.remove("eaten_today_j").is_some());
        }
        let mut storage = Sqlite::in_memory(content.clone()).unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (0, 13, ?1)",
                [old.to_string()],
            )
            .unwrap();
        let back = storage.latest_world().unwrap().unwrap();
        let condition = back.body(body).unwrap().condition();
        assert_eq!((condition.deficit_j, condition.eaten_today_j), (0, 0));
    }

    #[test]
    fn a_v14_world_has_grown_bodies_of_both_sexes() {
        let content = content();
        let mut world = World::with_valley(Calendar::default(), &content, 256);
        let bodies: Vec<_> = (0..12)
            .map(|_| world.spawn_at_camp().expect("a body at the camp"))
            .collect();
        let mut old: serde_json::Value = serde_json::to_value(&world).unwrap();
        for body in old["bodies"].as_array_mut().unwrap() {
            let body = body.as_object_mut().unwrap();
            assert!(body.remove("born_day").is_some());
            assert!(body.remove("sex").is_some());
        }
        let mut storage = Sqlite::in_memory(content.clone()).unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (0, 14, ?1)",
                [old.to_string()],
            )
            .unwrap();
        let back = storage.latest_world().unwrap().unwrap();
        let mut sexes = std::collections::BTreeSet::new();
        for id in bodies {
            let body = back.body(id).unwrap();
            let years = back.age_milli(body) / 1000;
            assert!((20..30).contains(&years), "{years} years");
            sexes.insert(body.sex());
        }
        assert_eq!(sexes.len(), 2, "women and men");
    }

    #[test]
    fn a_v15_world_has_no_kin_no_pregnancy_and_the_life_rule_off() {
        let content = content();
        let mut world = World::with_valley(Calendar::default(), &content, 256);
        let bodies: Vec<_> = (0..4)
            .map(|_| world.spawn_at_camp().expect("a body at the camp"))
            .collect();
        let mut old: serde_json::Value = serde_json::to_value(&world).unwrap();
        assert!(old.as_object_mut().unwrap().remove("life").is_some());
        for body in old["bodies"].as_array_mut().unwrap() {
            let body = body.as_object_mut().unwrap();
            for field in ["mother", "father", "spouse", "pregnant"] {
                assert!(body.remove(field).is_none(), "{field}: absent when none");
            }
        }
        let mut storage = Sqlite::in_memory(content.clone()).unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (0, 15, ?1)",
                [old.to_string()],
            )
            .unwrap();
        let back = storage.latest_world().unwrap().unwrap();
        assert!(!back.life(), "the rule is off in a world saved before it");
        for id in bodies {
            let body = back.body(id).unwrap();
            assert_eq!(body.mother(), None);
            assert_eq!(body.father(), None);
            assert_eq!(body.spouse(), None);
            assert_eq!(body.pregnant(), None);
        }
        // Saved again, the world is v16 and keeps the rule's switch.
        let mut on = back;
        on.set_life(true);
        storage.save_world(&on).unwrap();
        let version: u32 = storage
            .conn
            .query_row(
                "SELECT format_version FROM snapshots ORDER BY tick DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, SNAPSHOT_FORMAT_VERSION);
        assert!(storage.latest_world().unwrap().unwrap().life());
    }

    #[test]
    fn a_v17_world_ages_one_age_year_a_cycle_and_a_v18_keeps_its_own() {
        let content = content();
        let calendar = Calendar::new(2, 3).unwrap().with_cycle(16, 1).unwrap();
        let mut world = World::with_valley(calendar, &content, 256);
        let body = world.spawn_at_camp().expect("a body at the camp");
        let mut events = Vec::new();
        world.advance(4, &content, &mut events);
        assert!(world.body(body).is_some());
        let mut old: serde_json::Value = serde_json::to_value(&world).unwrap();
        assert!(
            old["calendar"]
                .as_object_mut()
                .unwrap()
                .remove("age_years_per_cycle")
                .is_some()
        );
        let mut storage = Sqlite::in_memory(content.clone()).unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (4, 17, ?1)",
                [old.to_string()],
            )
            .unwrap();
        let back = storage.latest_world().unwrap().unwrap();
        assert_eq!(back.calendar().age_years_per_cycle(), 1);
        assert_eq!(back.calendar().days_per_age_year(), 16);
        assert_eq!(back.calendar().days_per_cycle(), 16);
        assert_eq!(
            back.age_milli(back.body(body).unwrap()),
            world.age_milli(world.body(body).unwrap()),
            "the same age, in age-years of the cycle"
        );
        // A v18 world keeps its age-years: four a cycle, of four days.
        let quick = Calendar::new(2, 3).unwrap().with_cycle(16, 4).unwrap();
        let world = World::with_valley(quick, &content, 256);
        let mut storage = Sqlite::in_memory(content).unwrap();
        storage.save_world(&world).unwrap();
        let today = storage.latest_world().unwrap().unwrap();
        assert_eq!(today.calendar(), &quick);
        assert_eq!(today.calendar().days_per_age_year(), 4);
    }

    #[test]
    fn a_v19_folded_bark_becomes_a_birch_bark_with_a_hollow() {
        let content = content();
        let mut world = World::with_valley(Calendar::new(2, 3).unwrap(), &content, 256);
        let material = content.materials[0].id.clone();
        let item = world.spawn_item(&material, 200, sim::Pos::new(3, 3));
        let old = serde_json::to_string(&world).unwrap().replace(
            &format!("\"material\":\"{material}\""),
            "\"material\":\"folded_birch_bark\"",
        );
        assert!(old.contains("folded_birch_bark"));
        let mut storage = Sqlite::in_memory(content).unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (0, 19, ?1)",
                [old],
            )
            .unwrap();
        let back = storage.latest_world().unwrap().unwrap();
        let folded = back.item(item).expect("the item kept");
        assert_eq!(folded.material(), "birch_bark");
        assert!(folded.hollow_ml() > 0, "folded: it holds");
        assert_eq!(folded.mass_g(), 200);
    }

    #[test]
    fn a_v20_world_has_nothing_put_in_anything() {
        let content = content();
        let mut world = World::with_valley(Calendar::new(2, 3).unwrap(), &content, 256);
        let material = content.materials[0].id.clone();
        let item = world.spawn_item(&material, 200, sim::Pos::new(3, 3));
        let old = serde_json::to_string(&world).unwrap();
        assert!(!old.contains("within"), "absent when nothing is put in");
        let mut storage = Sqlite::in_memory(content).unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (0, 20, ?1)",
                [old],
            )
            .unwrap();
        let back = storage.latest_world().unwrap().unwrap();
        assert_eq!(back.item(item).expect("the item kept").within(), None);
    }

    #[test]
    fn a_world_saved_before_generator_versions_keeps_the_first() {
        // A v9 snapshot (the v8 fixture reads the same): its valley has no
        // generator, so it was made by the first and keeps it for life.
        const FIXTURE: &str = include_str!("../tests/fixtures/snapshot-v8.json");
        assert!(!FIXTURE.contains("generator"));
        let mut storage = Sqlite::in_memory(content()).unwrap();
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (4, 9, ?1)",
                [FIXTURE],
            )
            .unwrap();
        let old = storage.latest_world().unwrap().unwrap();
        assert_eq!(old.map().generator(), sim::map::FIRST_GENERATOR);
        assert!(old.rocks_in(sim::ChunkPos::new(0, 0)).is_none());
        storage.save_world(&old).unwrap();
        let again = storage.latest_world().unwrap().unwrap();
        assert_eq!(again.map().generator(), sim::map::FIRST_GENERATOR);
        // A new world is made by today's generator, and keeps it.
        let mut storage = Sqlite::in_memory(content()).unwrap();
        storage.save_world(&world_at(0)).unwrap();
        let new = storage.latest_world().unwrap().unwrap();
        assert_eq!(new.map().generator(), sim::map::GENERATOR);
        assert!(new.rocks_in(sim::ChunkPos::new(0, 0)).is_some());
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
                db_int(world.tick().0).unwrap()
            )),
            "{rows:?}"
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r.3 == db_int(world.tick().0).unwrap())
                .count(),
            1,
            "{rows:?}"
        );
    }

    /// Writes a snapshot row of today's format from `data`, as a hand
    /// (or a disk fault) would.
    fn insert_snapshot(storage: &Sqlite, tick: i64, format: u32, data: &str) {
        storage
            .conn
            .execute(
                "INSERT INTO snapshots (tick, format_version, data) VALUES (?1, ?2, ?3)",
                params![tick, format, data],
            )
            .unwrap();
    }

    fn snapshots(storage: &Sqlite) -> Vec<i64> {
        let mut rows = storage
            .conn
            .prepare("SELECT tick FROM snapshots ORDER BY tick")
            .unwrap();
        rows.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn a_damaged_latest_snapshot_falls_back_to_the_one_before_as_a_crash() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
        for ticks in [10, 20, 30] {
            storage.save_world(&world_at(ticks)).unwrap();
        }
        storage
            .conn
            .execute("UPDATE snapshots SET data = '[1' WHERE tick = 30", [])
            .unwrap();

        let world = storage.latest_world().unwrap().expect("the one before");
        assert_eq!(world.tick(), Tick(20));
        assert_eq!(storage.land_changes_lost(), 0, "nothing of the land moved");
        // The damaged row is gone, so the next checkpoint is the newest.
        assert_eq!(snapshots(&storage), vec![10, 20]);
        // A clean stop, but the fallback lost play: a crash to clients.
        assert_eq!(storage.begin_run().unwrap().epoch, 1);
        storage.save_world(&world_at(25)).unwrap();
        assert_eq!(storage.latest_world().unwrap().unwrap().tick(), Tick(25));
        // Read again, nothing is damaged: no second crash.
        storage.mark_clean_stop().unwrap();
        assert_eq!(storage.begin_run().unwrap().epoch, 1);
    }

    /// The second security audit of 20 November 2026, C1: a fallback keeps
    /// the land as the lost run left it; it says how many chunks.
    #[test]
    fn a_fallback_says_how_many_chunks_of_the_land_it_loses() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
        storage.save_world(&world_at(10)).unwrap();
        // The land changed in two chunks before the snapshot at 20: kept
        // with it, nothing lost.
        let mut world = world_at(20);
        for (chunk, changes) in every_kind_of_change() {
            world.restore_changes(chunk, changes);
        }
        storage.save_world(&world).unwrap();
        // One more chunk changed by the lost run, at 30.
        let mut world = world_at(30);
        for (chunk, changes) in every_kind_of_change() {
            world.restore_changes(chunk, changes);
        }
        let later = ChunkChanges {
            version: 1,
            taken: [7].into(),
            ..ChunkChanges::default()
        };
        world.restore_changes(ChunkPos::new(5, 5), later);
        storage.save_world(&world).unwrap();
        storage
            .conn
            .execute("UPDATE snapshots SET data = '[1' WHERE tick = 30", [])
            .unwrap();

        let world = storage.latest_world().unwrap().expect("the one before");
        assert_eq!(world.tick(), Tick(20));
        assert_eq!(storage.land_changes_lost(), 1, "the chunk changed at 30");
        assert_eq!(
            world.chunk_changes().count(),
            3,
            "the land stays as the lost run left it"
        );
        // A fallback is said once: read again, nothing is lost.
        assert_eq!(storage.begin_run().unwrap().epoch, 1);
        storage.latest_world().unwrap().expect("sound");
        assert_eq!(storage.land_changes_lost(), 0);
    }

    #[test]
    fn a_snapshot_of_format_zero_is_refused_not_a_panic() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
        insert_snapshot(&storage, 7, 0, "{}");
        let err = storage.latest_world().unwrap_err();
        assert!(
            matches!(err, StorageError::Refused(ref what) if what.contains("every kept snapshot is damaged")),
            "{err}"
        );
    }

    #[test]
    fn a_newer_snapshot_in_front_is_refused_not_skipped() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
        storage.save_world(&world_at(10)).unwrap();
        insert_snapshot(&storage, 20, 99, "{}");
        let err = storage.latest_world().unwrap_err();
        assert!(
            matches!(err, StorageError::UnsupportedSnapshot { found: 99, .. }),
            "{err}"
        );
        assert_eq!(snapshots(&storage), vec![10, 20], "nothing dropped");
    }

    type Craft = Box<dyn Fn(&mut serde_json::Value)>;

    #[test]
    fn crafted_snapshots_are_refused_not_panics() {
        let sound = serde_json::to_value(world_at(4)).unwrap();
        let crafted: [(&str, Craft); 6] = [
            (
                "calendar.ticks_per_second 0",
                Box::new(|v| {
                    v["calendar"]["ticks_per_second"] = 0.into();
                }),
            ),
            (
                "calendar.days_per_year 0",
                Box::new(|v| {
                    v["calendar"]["days_per_year"] = 0.into();
                }),
            ),
            (
                "a map of 4,000,000 tiles a side",
                Box::new(|v| {
                    v["map"]["width"] = 4_000_000.into();
                    v["map"]["height"] = 4_000_000.into();
                }),
            ),
            (
                "generator 99",
                Box::new(|v| v["map"]["generator"] = 99.into()),
            ),
            (
                "the vivarium's generator on another size",
                Box::new(|v| {
                    v["map"]["generator"] = 4.into();
                }),
            ),
            ("tick past i64", Box::new(|v| v["tick"] = u64::MAX.into())),
        ];
        for (what, craft) in crafted {
            let mut data = sound.clone();
            craft(&mut data);
            let mut storage = Sqlite::in_memory(content()).unwrap();
            insert_snapshot(&storage, 4, SNAPSHOT_FORMAT_VERSION, &data.to_string());
            match storage.latest_world() {
                Ok(Some(world)) => {
                    let _ = world.time();
                    panic!("{what}: accepted");
                }
                Ok(None) => panic!("{what}: not seen"),
                Err(e) => assert!(matches!(e, StorageError::Refused(_)), "{what}: {e}"),
            }
        }
    }

    #[test]
    fn an_epoch_at_its_maximum_is_refused() {
        let mut storage = Sqlite::in_memory(content()).unwrap();
        storage
            .conn
            .execute("UPDATE run_state SET epoch = ?1, clean = 0", [i64::MAX])
            .unwrap();
        let err = storage.begin_run().unwrap_err();
        assert!(matches!(err, StorageError::Refused(_)), "{err}");
    }

    #[test]
    fn a_foreign_sqlite_file_is_refused_not_adopted() {
        let (dir, path) = temp_db("foreign");
        let foreign = Connection::open(&path).unwrap();
        foreign
            .execute_batch("CREATE TABLE notes (id INTEGER PRIMARY KEY, text TEXT);")
            .unwrap();
        drop(foreign);
        let err = Sqlite::open(&path, content()).unwrap_err();
        assert!(
            matches!(err, StorageError::Refused(ref what) if what.contains("not a Soul Invasion save")),
            "{err}"
        );
        // Untouched: its table alone, no schema version, no WAL.
        let foreign = Connection::open(&path).unwrap();
        let version: u32 = foreign
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, 0);
        assert!(!dir.join("world.sqlite-wal").exists());
        let err = ContinentStore::open(&path).unwrap_err();
        assert!(matches!(err, StorageError::Refused(_)), "{err}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn the_world_file_is_made_for_its_owner_alone() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, path) = temp_db("private");
        let mut storage = Sqlite::open(&path, content()).unwrap();
        storage.save_world(&world_at(4)).unwrap();
        let mode = |name: &str| {
            std::fs::metadata(dir.join(name)).map_or_else(
                |_| panic!("{name} exists"),
                |m| m.permissions().mode() & 0o777,
            )
        };
        assert_eq!(mode("world.sqlite"), 0o600);
        assert_eq!(mode("world.sqlite-wal"), 0o600);
        drop(storage);
        let continents = dir.join("continents.sqlite");
        ContinentStore::open(&continents).unwrap();
        assert_eq!(mode("continents.sqlite"), 0o600);
        let _ = std::fs::remove_dir_all(dir);
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
        let err = Sqlite::from_connection(conn, content(), "test").unwrap_err();
        assert!(matches!(
            err,
            StorageError::SchemaTooNew {
                found: 42,
                supported: 4
            }
        ));
    }
}
