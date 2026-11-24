//! The crops (D45): what the trees and plants bear, when, and how much.
//!
//! A crop is worked out from the date, never stored, as the trees and the
//! plants are placed again from the seed: on any day a plant bears what
//! its kind gives in that season of that year ([`Crop`],
//! `content/crops.ron`), less what men picked from it, which is the only
//! thing kept (the chunk's changes, [`crate::changes::ChunkChanges`]).
//! Ground no man touched keeps nothing, whatever it bears.
//!
//! - **When**: a crop is on the plant from a point of the year to
//!   another, each a season and how far into it. It runs into the next
//!   year when it ends before it starts (rose hips stay on the branch
//!   through the winter); from the first day of spring to the end of
//!   winter, it is there all year. What is not picked by its end is gone: fallen,
//!   eaten by the animals.
//! - **How much**: what a plant of its kind bears in a lean, a usual and
//!   a rich year, times a share of its own (three quarters to five
//!   quarters), so two hazels side by side do not bear the same.
//! - **Which year**: lean or rich comes by chance, the same for a kind
//!   over a whole region of the world ([`MAST_CELL`]), as the oaks of a
//!   country mast together. A lean year is a lean year for the whole
//!   valley.
//!
//! The draws are keyed on the world's position (`Frame`), as the plants
//! are, so a crop is the same seen through any map.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::changes::Picked;
use crate::r#gen::scatter::Frame;
use crate::r#gen::{hash, hash_of};
use crate::map::{CHUNK_SIZE, ChunkPos, FinePos, MM_PER_TILE};
use crate::time::{Calendar, Season, Tick};
use crate::trees::TreeKey;

/// The side of a region of the world, in tiles, over which a kind has the
/// same year (lean, usual or rich): a kilometre.
pub const MAST_CELL: i32 = 1024;

/// What a kind of tree or plant bears, when, and how much (one entry of
/// `content/crops.ron`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crop {
    /// The kind bearing it: a tree's id (`species.ron`) or a plant's
    /// (`flora.ron`).
    pub on: String,
    /// The material it gives (`materials.ron`).
    pub gives: String,
    /// When it comes on the plant: a season, and how far into it in
    /// hundredths (0 its first day, 100 its end).
    pub from: (Season, u8),
    /// When it is gone, the same way. Before `from`: in the next year.
    pub until: (Season, u8),
    /// What one plant bears, in grams, in a lean, a usual and a rich year.
    pub grams: (u32, u32, u32),
    /// How often its years are lean or rich.
    pub years: Years,
    /// What one person gathers of it by hand in an hour of work, in
    /// grams, while there is some on the plant.
    pub per_hour_g: u32,
    /// How it ripens and falls over its span (D48): absent, [`Ripen`]'s
    /// default.
    #[serde(default)]
    pub ripen: Ripen,
}

/// How much of a crop is on the plant over its span (D48): none at its
/// start, rising evenly to the whole over the first `rise` hundredths of
/// the span, the whole until the last `fall` hundredths, then falling
/// evenly to none at its end (fallen, eaten by the animals). The same
/// curve, by default, for a fruit window with no crop in season.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ripen {
    /// Hundredths of the span over which it ripens.
    pub rise: u8,
    /// Hundredths of the span over which it falls.
    pub fall: u8,
}

impl Default for Ripen {
    /// A fifth of the span to ripen, three tenths to fall.
    fn default() -> Self {
        Self { rise: 20, fall: 30 }
    }
}

/// The whole of a crop, in [`share_on`]'s millionths.
const WHOLE: u64 = 1_000_000;

/// The levels of the fruit a tree or a plant shows (D48): 0 none to
/// `LEVELS` the load of its rich year at its peak.
pub const LEVELS: u8 = 8;

/// The chances in a hundred of a lean year and of a rich one; the other
/// years are usual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Years {
    /// In a hundred years, how many lean.
    pub lean: u8,
    /// In a hundred years, how many rich.
    pub rich: u8,
}

/// What kind of year a crop has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Year {
    /// It bears little.
    Lean,
    /// It bears what it usually does.
    Usual,
    /// It bears much (a mast year).
    Rich,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CropsFile {
    crops: Vec<Crop>,
}

/// The crops of `text` (the text of `crops.ron`).
pub fn parse(text: &str) -> Result<Vec<Crop>, ron::error::SpannedError> {
    ron::from_str::<CropsFile>(text).map(|file| file.crops)
}

/// What is wrong with `crop` on its own, if anything (that its kind and
/// its material exist is the content's check).
#[must_use]
pub fn check(crop: &Crop) -> Option<&'static str> {
    let (lean, usual, rich) = crop.grams;
    if crop.from.1 > 100 || crop.until.1 > 100 {
        Some("a point of the year is 0 to 100 into its season")
    } else if point(crop.from) == point(crop.until) {
        Some("from and until are the same point of the year")
    } else if !(lean <= usual && usual <= rich) || rich == 0 {
        Some("grams are lean, usual, rich, rising, and bear something")
    } else if rich > 1_000_000 {
        Some("grams: a tonne a plant at most")
    } else if u32::from(crop.years.lean) + u32::from(crop.years.rich) > 100 {
        Some("years: lean and rich make a hundred at most")
    } else if !(1..=100_000).contains(&crop.per_hour_g) {
        Some("per_hour_g is 1 to 100,000")
    } else if u32::from(crop.ripen.rise) + u32::from(crop.ripen.fall) > 100 {
        Some("ripen: rise and fall make a hundred at most")
    } else {
        None
    }
}

/// A point of the year in four hundredths of it (a season is 100).
fn point((season, into): (Season, u8)) -> u64 {
    let index = Season::ALL
        .iter()
        .position(|s| *s == season)
        .expect("one of the four");
    index as u64 * 100 + u64::from(into)
}

/// The year whose crop is on the plant at `tick`, counted as the
/// calendar counts them (the world is born in year 1): this year's, last
/// year's if it runs into this one, or `None` out of its season.
#[must_use]
pub fn in_season(crop: &Crop, calendar: &Calendar, tick: Tick) -> Option<u64> {
    span_year(crop.from, crop.until, calendar, tick)
}

/// The year whose span from `from` to `until` runs at `tick` (this
/// year's, or last year's if it runs into this one), as [`in_season`]
/// counts it; `None` out of it.
fn span_year(
    from: (Season, u8),
    until: (Season, u8),
    calendar: &Calendar,
    tick: Tick,
) -> Option<u64> {
    span_at(from, until, calendar, tick).map(|span| span.year)
}

/// Where `tick` stands in a span of the year that runs at it.
#[derive(Debug, Clone, Copy)]
struct Span {
    /// The year it began in, as [`in_season`] counts it.
    year: u64,
    /// How much of it has passed, in `length`'s unit.
    done: u128,
    /// Its length.
    length: u128,
}

/// The span from `from` to `until` running at `tick` ([`span_year`]),
/// and how far into it `tick` is; `None` out of it.
fn span_at(
    from: (Season, u8),
    until: (Season, u8),
    calendar: &Calendar,
    tick: Tick,
) -> Option<Span> {
    let per_year = calendar.ticks_per_day() * u64::from(calendar.days_per_cycle());
    let year = tick.0 / per_year + 1;
    // Compared in four hundredths of a year, without rounding.
    let at = u128::from(tick.0 % per_year) * 400;
    let mark = |p: u64| u128::from(p.min(400)) * u128::from(per_year);
    let whole = mark(400);
    let (from, until) = (mark(point(from)), mark(point(until)));
    let span = |year, done, length| Span { year, done, length };
    if from < until {
        (from <= at && at < until).then(|| span(year, at - from, until - from))
    } else if at >= from {
        Some(span(year, at - from, whole - from + until))
    } else if at < until {
        (year > 1).then(|| span(year - 1, whole - from + at, whole - from + until))
    } else {
        None
    }
}

/// How much of its crop is on the plant at `span`, following `ripen`,
/// in millionths of the whole ([`WHOLE`]).
fn share_on(ripen: Ripen, span: Span) -> u64 {
    let whole = u128::from(WHOLE);
    let (rise, fall) = (u128::from(ripen.rise), u128::from(ripen.fall));
    let (done, length) = (span.done * 100, span.length.max(1));
    let share = if done < length * rise {
        done * whole / (length * rise)
    } else if done >= length * (100 - fall) && fall > 0 {
        (length * 100 - done) * whole / (length * fall)
    } else {
        whole
    };
    u64::try_from(share.min(whole)).expect("at most the whole")
}

/// The level ([`LEVELS`]) of a share of the whole: rounded to the
/// nearest.
fn level_of_share(share: u64) -> u8 {
    let level = (share * u64::from(LEVELS) * 2 + WHOLE) / (2 * WHOLE);
    u8::try_from(level.min(u64::from(LEVELS))).expect("a level")
}

/// The crop `crop` on its plant at `tick`, at the calendar's tick: the
/// year whose crop it is and how much of it is on the plant then, out of
/// what the plant bears that year (`share_on`); `None` out of its
/// season.
#[must_use]
pub fn on_plant(crop: &Crop, calendar: &Calendar, tick: Tick) -> Option<(u64, u64)> {
    let span = span_at(crop.from, crop.until, calendar, tick)?;
    Some((span.year, share_on(crop.ripen, span)))
}

/// The whole days left before `crop` is gone from its plant at `tick`
/// (its `until`), `None` out of its season: what news of it can outlive
/// (D55, amended by lot 5a).
#[must_use]
pub fn days_left(crop: &Crop, calendar: &Calendar, tick: Tick) -> Option<u64> {
    let span = span_at(crop.from, crop.until, calendar, tick)?;
    // `span` counts in four hundredths of a tick.
    let ticks = u64::try_from((span.length - span.done) / 400).unwrap_or(u64::MAX);
    Some(ticks / calendar.ticks_per_day().max(1))
}

/// Is `crop` still ripening at `tick` ([`Ripen::rise`])? What is picked
/// bare then is not bare for long: more comes until it is ripe.
#[must_use]
pub fn ripening(crop: &Crop, calendar: &Calendar, tick: Tick) -> bool {
    span_at(crop.from, crop.until, calendar, tick)
        .is_some_and(|span| span.done * 100 < span.length * u128::from(crop.ripen.rise))
}

/// What a plant of `crop`'s kind standing at `at` offers at `tick`, before
/// anything is picked (D45, D48): what it bears that year times how much
/// of it is on the plant then ([`Ripen`]); the year whose crop it is.
/// `None` out of its season.
pub(crate) fn there(
    crop: &Crop,
    calendar: &Calendar,
    tick: Tick,
    seed: u64,
    frame: Frame,
    at: FinePos,
) -> Option<(u64, u32)> {
    there_keyed(crop, key_of(crop), calendar, tick, seed, frame, at)
}

/// [`there`], the crop's kind's key ([`key_of`]) given rather than hashed
/// again: the brains ask it for every plant they know at every choice.
pub(crate) fn there_keyed(
    crop: &Crop,
    on_key: u64,
    calendar: &Calendar,
    tick: Tick,
    seed: u64,
    frame: Frame,
    at: FinePos,
) -> Option<(u64, u32)> {
    let (year, share) = on_plant(crop, calendar, tick)?;
    let grams = u64::from(bears_keyed(crop, on_key, seed, frame, at, year)) * share / WHOLE;
    Some((year, u32::try_from(grams).expect("at most what it bears")))
}

/// The number standing for `crop`'s kind in its draws (a hash of its
/// id), the same on every machine; worked out once a crop by those who
/// ask often.
#[must_use]
pub fn key_of(crop: &Crop) -> u64 {
    text_key(&crop.on)
}

/// A number for a text (an id), the same on every machine: the hash of
/// its bytes, read as they come (the brains ask it millions of times a
/// run; collecting them first cost an allocation each).
fn text_key(text: &str) -> u64 {
    hash_of(text.bytes().map(u64::from))
}

/// A whole number as a hash key.
fn key(value: i64) -> u64 {
    u64::from_le_bytes(value.to_le_bytes())
}

/// What year `crop`'s kind has in `year` around the world's tile `tile`.
#[cfg(test)]
pub(crate) fn year_of(crop: &Crop, seed: u64, tile: (i32, i32), year: u64) -> Year {
    year_of_keyed(crop, key_of(crop), seed, tile, year)
}

/// What year `crop`'s kind has in `year` around the world's tile `tile`;
/// the kind's key ([`key_of`]) given.
fn year_of_keyed(crop: &Crop, on_key: u64, seed: u64, tile: (i32, i32), year: u64) -> Year {
    const PURPOSE: u64 = 0x4d41_5354; // "MAST"
    let (rx, ry) = (tile.0.div_euclid(MAST_CELL), tile.1.div_euclid(MAST_CELL));
    let roll = hash(&[
        PURPOSE,
        seed,
        on_key,
        year,
        key(i64::from(rx)),
        key(i64::from(ry)),
    ]) % 100;
    if roll < u64::from(crop.years.lean) {
        Year::Lean
    } else if roll >= 100 - u64::from(crop.years.rich) {
        Year::Rich
    } else {
        Year::Usual
    }
}

/// What a plant of `crop`'s kind standing at `at` bears in `year`, in
/// grams, before anything is picked.
#[cfg(test)]
pub(crate) fn bears(crop: &Crop, seed: u64, frame: Frame, at: FinePos, year: u64) -> u32 {
    bears_keyed(crop, key_of(crop), seed, frame, at, year)
}

/// What a plant of `crop`'s kind standing at `at` bears in `year`, in
/// grams, before anything is picked; the kind's key ([`key_of`]) given.
fn bears_keyed(crop: &Crop, on_key: u64, seed: u64, frame: Frame, at: FinePos, year: u64) -> u32 {
    let tile = at.tile();
    let base = match year_of_keyed(crop, on_key, seed, frame.tile(tile.x, tile.y), year) {
        Year::Lean => crop.grams.0,
        Year::Usual => crop.grams.1,
        Year::Rich => crop.grams.2,
    };
    u32::try_from(u64::from(base) * share_keyed(on_key, seed, frame, at) / 1000)
        .expect("under five quarters of a tonne")
}

/// The share of its kind's crop a plant of `crop`'s kind standing at
/// `at` bears, in thousandths: 750 to 1250.
fn share(crop: &Crop, seed: u64, frame: Frame, at: FinePos) -> u64 {
    share_keyed(key_of(crop), seed, frame, at)
}

/// [`share`], the kind's key given.
fn share_keyed(on_key: u64, seed: u64, frame: Frame, at: FinePos) -> u64 {
    const PURPOSE: u64 = 0x5348_4152; // "SHAR"
    let (x, y) = frame.mm(at.x, at.y);
    750 + hash(&[PURPOSE, seed, on_key, key(x), key(y)]) % 501
}

/// What a plant of `crop`'s kind standing at `at` bears in a rich year,
/// in grams: its share of the kind's rich crop, the load it shows full.
fn rich_on(crop: &Crop, seed: u64, frame: Frame, at: FinePos) -> u32 {
    u32::try_from(u64::from(crop.grams.2) * share(crop, seed, frame, at) / 1000)
        .expect("under five quarters of a tonne")
}

/// When a kind of tree or shrub shows its fruit (D48, `species.ron`'s
/// and `flora.ron`'s `fruit`): from a point of the year to another, as a
/// crop's ([`Crop::from`], [`Crop::until`]), running into the next year
/// when it ends before it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FruitWindow {
    /// When the fruit comes on the tree.
    pub from: (Season, u8),
    /// When it is gone.
    pub until: (Season, u8),
}

/// What is wrong with a fruit window, if anything.
#[must_use]
pub fn check_window(window: &FruitWindow) -> Option<&'static str> {
    if window.from.1 > 100 || window.until.1 > 100 {
        Some("fruit: a point of the year is 0 to 100 into its season")
    } else if point(window.from) == point(window.until) {
        Some("fruit: from and until are the same point of the year")
    } else {
        None
    }
}

/// The fruit `tree` (of `species`, its crops among `crops`) shows at
/// `tick` (D48), on a map whose corner is `origin` in the world
/// ([`crate::map::Map::origin`]), with `picked` its chunk's picks: a
/// level, 0 to [`LEVELS`] (`fruit_of`).
#[allow(clippy::too_many_arguments, reason = "the rule's inputs, each its own")]
#[must_use]
pub fn fruit_on(
    species: &crate::content::Species,
    crops: &[Crop],
    calendar: &Calendar,
    tick: Tick,
    seed: u64,
    origin: (i32, i32),
    tree: &crate::trees::Tree,
    picked: Option<&BTreeMap<(Source, String), Picked>>,
) -> u8 {
    fruit_of(
        species.fruit.as_ref(),
        &species.id,
        Source::Tree(tree.key),
        tree.at,
        crops,
        calendar,
        tick,
        seed,
        origin,
        picked,
    )
}

/// The fruit a plant of `kind` (a shrub of `flora.ron`), standing at `at`
/// in `chunk`, shows at `tick` (D48), as a tree's ([`fruit_on`]): its
/// picks are its place's ([`PlantKey`]). None off its chunk.
#[allow(clippy::too_many_arguments, reason = "the rule's inputs, each its own")]
#[must_use]
pub fn plant_fruit_on(
    kind: &crate::flora::Kind,
    crops: &[Crop],
    calendar: &Calendar,
    tick: Tick,
    seed: u64,
    origin: (i32, i32),
    chunk: ChunkPos,
    at: FinePos,
    picked: Option<&BTreeMap<(Source, String), Picked>>,
) -> u8 {
    let Some(key) = PlantKey::of(chunk, at) else {
        return 0;
    };
    fruit_of(
        kind.fruit.as_ref(),
        &kind.id,
        Source::Plant(key),
        at,
        crops,
        calendar,
        tick,
        seed,
        origin,
        picked,
    )
}

/// The fruit `source`, a tree or a plant of the kind `kind` standing at
/// `at`, shows at `tick` (D48), its kind's fruit window `window`: a level
/// from 0 (none) to [`LEVELS`] (the load of its rich year at its peak):
///
/// - out of its window, or without one: 0;
/// - in it, when a crop of its kind is in season: what is on it now, what
///   can be picked ([`there`], less this year's picks), against what it
///   bears in a rich year, rounded to the nearest level, at least 1 while
///   anything is left (the best of its crops in season, should it have
///   two);
/// - in it with no crop in season (acorns, cones: fruit no one gathers):
///   the default curve ([`Ripen`]) over its window, rounded to a level.
#[allow(clippy::too_many_arguments, reason = "the rule's inputs, each its own")]
fn fruit_of(
    window: Option<&FruitWindow>,
    kind: &str,
    source: Source,
    at: FinePos,
    crops: &[Crop],
    calendar: &Calendar,
    tick: Tick,
    seed: u64,
    origin: (i32, i32),
    picked: Option<&BTreeMap<(Source, String), Picked>>,
) -> u8 {
    let frame = Frame::at(origin);
    let Some(window) = window else {
        return 0;
    };
    let Some(span) = span_at(window.from, window.until, calendar, tick) else {
        return 0;
    };
    crops
        .iter()
        .filter(|crop| crop.on == kind)
        .filter_map(|crop| {
            let (year, grams) = there(crop, calendar, tick, seed, frame, at)?;
            let taken = picked
                .and_then(|p| p.get(&(source, crop.gives.clone())))
                .filter(|p| p.year == year)
                .map_or(0, |p| p.grams);
            let left = u64::from(grams.saturating_sub(taken));
            let rich = u64::from(rich_on(crop, seed, frame, at)).max(1);
            Some(if left == 0 {
                0
            } else {
                level_of_share((left * WHOLE / rich).min(WHOLE)).max(1)
            })
        })
        .max()
        .unwrap_or_else(|| level_of_share(share_on(Ripen::default(), span)))
}

/// Which fruit the trees (`species`) and the shrubs (`flora`) show at
/// `tick`, as one number (D48): for each kind with a fruit window, whether
/// it is open and, then, each of its crops in season (its year and its
/// curve as a level, [`LEVELS`]), or, with none in season, the window's
/// own curve as a level. The fruit every tree and shrub shows follows
/// from it, the seed and the picks: it changes only when a level of a
/// curve moves, a window or a crop opens or shuts, or a crop's year
/// turns, and the server sends a chunk again when it does.
#[must_use]
pub fn fruit_phase(
    species: &[crate::content::Species],
    flora: &[crate::flora::Kind],
    crops: &[Crop],
    calendar: &Calendar,
    tick: Tick,
) -> u64 {
    const PURPOSE: u64 = 0x4652_5554; // "FRUT"
    let mut values = vec![PURPOSE];
    let windows = species
        .iter()
        .map(|s| (s.id.as_str(), s.fruit.as_ref()))
        .chain(flora.iter().map(|k| (k.id.as_str(), k.fruit.as_ref())));
    for (id, window) in windows {
        let Some(window) = window else {
            continue;
        };
        let Some(span) = span_at(window.from, window.until, calendar, tick) else {
            values.push(0);
            continue;
        };
        values.push(1);
        let mut any = false;
        for crop in crops.iter().filter(|c| c.on == id) {
            let on = on_plant(crop, calendar, tick);
            any |= on.is_some();
            values.push(on.map_or(0, |(year, share)| {
                (year + 1) * 16 + u64::from(level_of_share(share))
            }));
        }
        if !any {
            values.push(u64::from(level_of_share(share_on(Ripen::default(), span))));
        }
    }
    hash(&values)
}

/// A plant's key in its chunk: its place from the chunk's corner, in
/// millimetres. Plants keep their room, so two never share a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlantKey {
    /// Millimetres east of the chunk's corner.
    pub x_mm: u16,
    /// Millimetres south of the chunk's corner.
    pub y_mm: u16,
}

impl PlantKey {
    /// The key of a plant standing at `at` in `chunk`; `None` if it
    /// stands outside it.
    #[must_use]
    pub fn of(chunk: ChunkPos, at: FinePos) -> Option<Self> {
        let corner = chunk.origin();
        let side = i32::try_from(CHUNK_SIZE).expect("small chunk") * MM_PER_TILE;
        let (x, y) = (at.x - corner.x * MM_PER_TILE, at.y - corner.y * MM_PER_TILE);
        ((0..side).contains(&x) && (0..side).contains(&y)).then(|| Self {
            x_mm: u16::try_from(x).expect("within a chunk"),
            y_mm: u16::try_from(y).expect("within a chunk"),
        })
    }
}

/// What bears a crop, for the whole life of the world: a tree by its key,
/// a plant by its place. Stable as the trees' keys are: the same every
/// time its chunk is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// A tree of the chunk.
    Tree(TreeKey),
    /// A plant of the chunk.
    Plant(PlantKey),
}

/// A tree or a plant bearing crops in some season, as the world places
/// it ([`crate::World::bearers_in`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bearer {
    /// Who it is.
    pub source: Source,
    /// Where it stands.
    pub at: FinePos,
    /// Its kind's id (`hazel`, `bramble`), shared by the chunk's bearers
    /// of that kind: a meadow holds a thousand, each once named.
    /// Written and read as the text itself (a snapshot's JSON is as
    /// before, D28).
    #[serde(with = "shared_text")]
    pub kind: Arc<str>,
}

/// A shared text written and read as a plain string.
pub(crate) mod shared_text {
    use std::sync::Arc;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(text: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(text)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Arc<str>, D::Error> {
        String::deserialize(deserializer).map(Arc::from)
    }
}

/// A crop on its plant now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ripe {
    /// The material it gives.
    pub gives: String,
    /// The year whose crop it is.
    pub year: u64,
    /// What is left of it on the plant, in grams.
    pub grams: u32,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn hazel() -> Crop {
        Crop {
            on: "hazel".to_owned(),
            gives: "hazelnuts".to_owned(),
            from: (Season::Autumn, 0),
            until: (Season::Winter, 0),
            grams: (1000, 5000, 10_000),
            years: Years { lean: 25, rich: 25 },
            per_hour_g: 2000,
            ripen: Ripen::default(),
        }
    }

    pub(crate) fn calendar() -> Calendar {
        Calendar::new(1, 100).unwrap()
    }

    /// The tick at `day` and a half.
    fn midday(day: u64) -> Tick {
        Tick(day * 100 + 50)
    }

    #[test]
    fn the_repository_crops_parse_and_check() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/crops.ron"
        ))
        .unwrap();
        let crops = parse(&text).unwrap();
        assert_ne!(crops, []);
        for crop in &crops {
            assert_eq!(check(crop), None, "{crop:?}");
        }
    }

    #[test]
    fn a_hazel_bears_in_autumn_and_nothing_the_rest_of_the_year() {
        let calendar = calendar(); // 50 days a year: autumn is days 25 to 37
        let hazel = hazel();
        let on: Vec<u64> = (0..100)
            .filter(|&day| in_season(&hazel, &calendar, midday(day)).is_some())
            .collect();
        let autumns: Vec<u64> = (25..37).chain(75..87).collect();
        assert_eq!(on, autumns);
        assert_eq!(in_season(&hazel, &calendar, midday(30)), Some(1));
        assert_eq!(in_season(&hazel, &calendar, midday(80)), Some(2));
        // The first tick of autumn bears; the first of winter does not.
        assert_eq!(in_season(&hazel, &calendar, Tick(2500)), Some(1));
        assert_eq!(in_season(&hazel, &calendar, Tick(3749)), Some(1));
        assert_eq!(in_season(&hazel, &calendar, Tick(3750)), None);
    }

    #[test]
    fn a_crop_through_the_winter_belongs_to_the_year_it_came() {
        let calendar = calendar();
        let hips = Crop {
            from: (Season::Autumn, 50),
            until: (Season::Spring, 25),
            ..hazel()
        };
        assert_eq!(in_season(&hips, &calendar, midday(30)), None);
        assert_eq!(in_season(&hips, &calendar, midday(32)), Some(1));
        assert_eq!(in_season(&hips, &calendar, midday(45)), Some(1));
        assert_eq!(in_season(&hips, &calendar, midday(51)), Some(1));
        assert_eq!(in_season(&hips, &calendar, midday(54)), None);
        // The first spring of the world bears no crop of a year before it.
        assert_eq!(in_season(&hips, &calendar, midday(1)), None);
    }

    #[test]
    fn lean_and_rich_years_come_as_often_as_the_content_says_over_a_whole_region() {
        let hazel = hazel();
        let years: Vec<Year> = (0..400).map(|y| year_of(&hazel, 7, (10, 10), y)).collect();
        let lean = years.iter().filter(|y| **y == Year::Lean).count();
        let rich = years.iter().filter(|y| **y == Year::Rich).count();
        assert!((70..130).contains(&lean), "{lean} lean in 400");
        assert!((70..130).contains(&rich), "{rich} rich in 400");
        // The same year all over the region, another one elsewhere.
        for year in 0..20 {
            let here = year_of(&hazel, 7, (10, 10), year);
            assert_eq!(year_of(&hazel, 7, (1000, 700), year), here);
        }
        let elsewhere = (0..40)
            .filter(|&y| year_of(&hazel, 7, (5000, 10), y) != year_of(&hazel, 7, (10, 10), y))
            .count();
        assert!(
            elsewhere > 5,
            "a region a few kilometres off has its own years"
        );
    }

    #[test]
    fn two_plants_side_by_side_bear_a_share_of_their_own() {
        let hazel = hazel();
        let a = bears(&hazel, 7, Frame::ORIGIN, FinePos::new(10_500, 10_500), 3);
        let b = bears(&hazel, 7, Frame::ORIGIN, FinePos::new(12_500, 10_500), 3);
        assert_ne!(a, b);
        for at in [FinePos::new(10_500, 10_500), FinePos::new(12_500, 10_500)] {
            let grams = bears(&hazel, 7, Frame::ORIGIN, at, 3);
            let base = match year_of(&hazel, 7, (at.tile().x, at.tile().y), 3) {
                Year::Lean => 1000,
                Year::Usual => 5000,
                Year::Rich => 10_000,
            };
            assert!(
                grams * 4 >= base * 3 && grams * 4 <= base * 5,
                "{grams} of {base}"
            );
        }
        // Seen through a map whose corner is elsewhere in the world, the
        // same plant of the world bears the same.
        let shifted = Frame::at((-3, -4));
        assert_eq!(
            bears(&hazel, 7, shifted, FinePos::new(13_500, 14_500), 3),
            a
        );
    }

    #[test]
    fn bad_crops_are_refused() {
        let ok = hazel();
        assert_eq!(check(&ok), None);
        let same = Crop {
            until: (Season::Autumn, 0),
            ..hazel()
        };
        assert!(check(&same).is_some());
        let year_round = Crop {
            from: (Season::Spring, 0),
            until: (Season::Winter, 100),
            ..hazel()
        };
        assert_eq!(check(&year_round), None, "the whole year (cattail roots)");
        let calendar = calendar();
        for day in [0, 13, 49, 50, 99] {
            assert_eq!(
                in_season(&year_round, &calendar, midday(day)),
                Some(day / 50 + 1)
            );
        }
        assert!(
            check(&Crop {
                from: (Season::Spring, 101),
                ..hazel()
            })
            .is_some()
        );
        assert!(
            check(&Crop {
                grams: (5, 3, 9),
                ..hazel()
            })
            .is_some()
        );
        assert!(
            check(&Crop {
                grams: (0, 0, 0),
                ..hazel()
            })
            .is_some()
        );
        let crowded = Crop {
            years: Years { lean: 60, rich: 50 },
            ..hazel()
        };
        assert!(check(&crowded).is_some());
    }

    #[test]
    fn a_plant_key_is_its_place_in_its_chunk() {
        let chunk = ChunkPos::new(2, -1);
        let at = FinePos::new(64_000 + 1234, -32_000 + 31_999);
        let key = PlantKey::of(chunk, at).unwrap();
        assert_eq!((key.x_mm, key.y_mm), (1234, 31_999));
        assert_eq!(PlantKey::of(chunk, FinePos::new(63_999, -10)), None);
    }

    /// The repository's content, crops included.
    pub(crate) fn content() -> crate::Content {
        crate::Content::from_ron(
            include_str!("../../../content/materials.ron"),
            include_str!("../../../content/spawns.ron"),
            include_str!("../../../content/movement.ron"),
            include_str!("../../../content/needs.ron"),
        )
        .and_then(|c| c.with_species(include_str!("../../../content/species.ron")))
        .and_then(|c| c.with_flora(include_str!("../../../content/flora.ron")))
        .and_then(|c| c.with_crops(include_str!("../../../content/crops.ron")))
        .unwrap()
    }

    /// The valley on a short calendar (a day of 100 ticks, 50 days a
    /// year), and the first bearer of `kind` in it, with its chunk.
    pub(crate) fn valley_with(kind: &str) -> (crate::World, crate::Content, ChunkPos, Bearer) {
        let content = content();
        let world = crate::World::with_valley(calendar(), &content, 256);
        let side = 256 / i32::try_from(CHUNK_SIZE).unwrap();
        for y in 0..side {
            for x in 0..side {
                let chunk = ChunkPos::new(x, y);
                if let Some(bearer) = world
                    .bearers_in(chunk)
                    .iter()
                    .find(|b| *b.kind == *kind)
                    .cloned()
                {
                    return (world, content, chunk, bearer);
                }
            }
        }
        panic!("no {kind} in the valley");
    }

    /// Runs `world` to the middle of `day`.
    pub(crate) fn run_to(world: &mut crate::World, content: &crate::Content, day: u64) {
        let target = midday(day).0;
        let mut events = Vec::new();
        world.advance(target - world.tick().0, content, &mut events);
    }

    #[test]
    fn a_hazel_of_the_valley_is_picked_in_autumn_and_bears_again_the_next() {
        let (mut world, content, chunk, hazel) = valley_with("hazel");
        assert!(matches!(hazel.source, Source::Tree(_)));
        assert_eq!(world.ripe_on(chunk, &hazel), [], "nothing in spring");
        run_to(&mut world, &content, 30);
        let crop = &content.crops[0];
        let frame = Frame::at(world.map().origin());
        let full = bears(crop, world.seed(), frame, hazel.at, 1);
        let ripe = world.ripe_on(chunk, &hazel);
        assert_eq!(
            ripe,
            [Ripe {
                gives: "hazelnuts".to_owned(),
                year: 1,
                grams: full
            }]
        );
        // Nothing is kept until men pick.
        assert_eq!(world.chunk_version(chunk), 0);
        assert_eq!(world.pick(chunk, hazel.source, "hazelnuts", 400), 400);
        assert_eq!(world.chunk_version(chunk), 1);
        assert_eq!(world.ripe_on(chunk, &hazel)[0].grams, full - 400);
        // At most what is left; then nothing.
        assert_eq!(
            world.pick(chunk, hazel.source, "hazelnuts", u32::MAX),
            full - 400
        );
        assert_eq!(world.ripe_on(chunk, &hazel), []);
        assert_eq!(world.pick(chunk, hazel.source, "hazelnuts", 1), 0);
        // What it does not bear, or no tree there: nothing, no change.
        assert_eq!(world.pick(chunk, hazel.source, "blackberries", 1), 0);
        let elsewhere = Source::Plant(PlantKey { x_mm: 1, y_mm: 1 });
        assert_eq!(world.pick(chunk, elsewhere, "hazelnuts", 1), 0);
        assert_eq!(world.chunk_version(chunk), 2);
        // Winter: none. The next autumn: the next year's crop, whole.
        run_to(&mut world, &content, 40);
        assert_eq!(world.ripe_on(chunk, &hazel), []);
        run_to(&mut world, &content, 80);
        let next = bears(crop, world.seed(), frame, hazel.at, 2);
        assert_eq!(world.ripe_on(chunk, &hazel)[0].grams, next);
        assert_eq!(world.ripe_on(chunk, &hazel)[0].year, 2);
    }

    #[test]
    fn a_bramble_is_a_plant_known_by_its_place_and_picked_the_same_way() {
        let (mut world, content, chunk, bramble) = valley_with("bramble");
        let Source::Plant(key) = bramble.source else {
            panic!("a bramble is a plant");
        };
        assert_eq!(PlantKey::of(chunk, bramble.at), Some(key));
        run_to(&mut world, &content, 25); // late summer
        let ripe = world.ripe_on(chunk, &bramble);
        assert_eq!(ripe.len(), 1);
        assert_eq!(ripe[0].gives, "blackberries");
        assert_eq!(world.pick(chunk, bramble.source, "blackberries", 100), 100);
        let picked = &world.changes_in(chunk).unwrap().picked;
        let taken = picked[&(bramble.source, "blackberries".to_owned())];
        assert_eq!((taken.year, taken.grams), (1, 100));
        // The same bearers, placed again.
        assert!(world.bearers_in(chunk).contains(&bramble));
    }

    #[test]
    fn over_years_a_hazel_has_lean_and_usual_or_rich_autumns_and_bare_winters() {
        let (mut world, content, chunk, hazel) = valley_with("hazel");
        let crop = content.crops[0].clone();
        let tile = hazel.at.tile();
        let frame = Frame::at(world.map().origin());
        let kinds: Vec<Year> = (0..12)
            .map(|y| year_of(&crop, world.seed(), frame.tile(tile.x, tile.y), y))
            .collect();
        assert!(kinds.contains(&Year::Lean), "{kinds:?}");
        assert!(kinds.iter().any(|y| *y != Year::Lean), "{kinds:?}");
        // Two years, day by day: autumn bears, the other seasons do not.
        for day in 0..100 {
            run_to(&mut world, &content, day);
            let autumn = (25..37).contains(&(day % 50));
            assert_eq!(
                !world.ripe_on(chunk, &hazel).is_empty(),
                autumn,
                "day {day}"
            );
        }
    }

    /// A body born on the free tile nearest `bearer`.
    pub(crate) fn body_by(world: &mut crate::World, bearer: &Bearer) -> crate::BodyId {
        let t = bearer.at.tile();
        let near: Vec<crate::Pos> = {
            let trunks = world.trunks();
            let mut near: Vec<crate::Pos> = (-2..=2)
                .flat_map(|dy| (-2..=2).map(move |dx| crate::Pos::new(t.x + dx, t.y + dy)))
                .filter(|&p| world.map().is_passable(p) && !trunks.blocks(p))
                .collect();
            near.sort_by_key(|p| (p.x - t.x).pow(2) + (p.y - t.y).pow(2));
            near
        };
        let body = world.spawn_body(near[0]);
        let at = world.bodies().iter().find(|b| b.id() == body).unwrap().at();
        let (dx, dy) = (i64::from(at.x - bearer.at.x), i64::from(at.y - bearer.at.y));
        assert!(
            dx * dx + dy * dy <= crate::GATHER_REACH_MM.pow(2),
            "within reach"
        );
        body
    }

    fn gather(
        body: crate::BodyId,
        chunk: ChunkPos,
        bearer: &Bearer,
        gives: &str,
    ) -> crate::Intention {
        crate::Intention::Gather {
            body,
            chunk,
            source: bearer.source,
            gives: gives.to_owned(),
        }
    }

    fn mass_of(world: &crate::World, item: crate::ItemId) -> u32 {
        world.item(item).map_or(0, crate::Item::mass_g)
    }

    #[test]
    fn gathering_a_hazel_takes_its_rate_an_hour_into_the_hand_until_it_is_bare() {
        let (mut world, content, chunk, hazel) = valley_with("hazel");
        run_to(&mut world, &content, 30);
        let full = world.ripe_on(chunk, &hazel)[0].grams;
        let body = body_by(&mut world, &hazel);
        let mut events = Vec::new();
        world.queue(gather(body, chunk, &hazel, "hazelnuts"));
        world.advance(1, &content, &mut events);
        assert!(events.contains(&crate::Event::WorkStarted {
            body,
            gives: "hazelnuts".to_owned()
        }));
        // An hour is 4 ticks of this short day: 2 kg an hour, 500 g a tick.
        world.advance(2, &content, &mut events);
        let held = world
            .bodies()
            .iter()
            .find(|b| b.id() == body)
            .unwrap()
            .held();
        let item = held.expect("the nuts in hand");
        assert_eq!(mass_of(&world, item), full.min(1000));
        assert_eq!(world.ripe_on(chunk, &hazel)[0].grams, full - full.min(1000));
        // Each pinch is told, with the chunk's version after it (D48).
        assert!(events.contains(&crate::Event::CropPicked {
            chunk,
            version: world.chunk_version(chunk)
        }));
        // On until the hazel is bare: all of it in the hand, told once.
        events.clear();
        world.advance(40, &content, &mut events);
        let stopped: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, crate::Event::WorkStopped { .. }))
            .collect();
        assert_eq!(
            stopped,
            [&crate::Event::WorkStopped {
                body,
                gives: "hazelnuts".to_owned(),
                grams: full,
                why: crate::WorkEnd::Bare,
            }]
        );
        assert_eq!(mass_of(&world, item), full);
        assert_eq!(world.ripe_on(chunk, &hazel), []);
        let body_now = world.bodies().iter().find(|b| b.id() == body).unwrap();
        assert!(body_now.work().is_none());
    }

    #[test]
    fn a_full_load_or_another_intention_stops_the_work() {
        let (mut world, mut content, chunk, hazel) = valley_with("hazel");
        run_to(&mut world, &content, 30);
        let body = body_by(&mut world, &hazel);
        let mut events = Vec::new();
        // Room for 300 g: full after a pinch or two.
        content.movement.carry_capacity_g = 300;
        world.queue(gather(body, chunk, &hazel, "hazelnuts"));
        world.advance(6, &content, &mut events);
        assert!(events.contains(&crate::Event::WorkStopped {
            body,
            gives: "hazelnuts".to_owned(),
            grams: 300,
            why: crate::WorkEnd::Full,
        }));
        // Refused with nothing more to carry.
        events.clear();
        world.queue(gather(body, chunk, &hazel, "hazelnuts"));
        world.advance(1, &content, &mut events);
        assert!(events.iter().any(|e| matches!(
            e,
            crate::Event::GatherRefused {
                reason: crate::Refusal::TooHeavy,
                ..
            }
        )));
        // Room again; another intention stops it, the nuts kept.
        content.movement.carry_capacity_g = 20_000;
        world.queue(gather(body, chunk, &hazel, "hazelnuts"));
        world.advance(3, &content, &mut events);
        events.clear();
        world.queue(crate::Intention::Sleep { body });
        world.advance(1, &content, &mut events);
        assert!(events.iter().any(|e| matches!(
            e,
            crate::Event::WorkStopped { why: crate::WorkEnd::Interrupted, grams, .. } if *grams > 0
        )));
        let load = world
            .bodies()
            .iter()
            .find(|b| b.id() == body)
            .unwrap()
            .load_g();
        assert!(load > 300, "{load}");
    }

    #[test]
    fn gathering_is_refused_out_of_season_out_of_reach_and_where_nothing_stands() {
        let (mut world, content, chunk, hazel) = valley_with("hazel");
        let body = body_by(&mut world, &hazel);
        let mut events = Vec::new();
        let refused = |events: &[crate::Event]| {
            events.iter().find_map(|e| match e {
                crate::Event::GatherRefused { reason, .. } => Some(*reason),
                _ => None,
            })
        };
        world.queue(gather(body, chunk, &hazel, "hazelnuts"));
        world.advance(1, &content, &mut events);
        assert_eq!(
            refused(&events),
            Some(crate::Refusal::NothingRipe),
            "spring"
        );
        run_to(&mut world, &content, 30);
        events.clear();
        let nowhere = Bearer {
            source: Source::Plant(PlantKey { x_mm: 3, y_mm: 3 }),
            ..hazel.clone()
        };
        world.queue(gather(body, chunk, &nowhere, "hazelnuts"));
        world.advance(1, &content, &mut events);
        assert_eq!(refused(&events), Some(crate::Refusal::Unknown));
        let far = world.spawn_at_camp().unwrap();
        events.clear();
        world.queue(gather(far, chunk, &hazel, "hazelnuts"));
        world.advance(1, &content, &mut events);
        assert_eq!(refused(&events), Some(crate::Refusal::OutOfReach));
    }

    #[test]
    fn food_spoils_after_its_days_is_refused_then_taken_away() {
        let (mut world, mut content, _, _) = valley_with("hazel");
        // Four days without water would kill the eater first.
        content.needs.thirst_per_day = 0;
        let body = world.spawn_at_camp().unwrap();
        let at = world
            .bodies()
            .iter()
            .find(|b| b.id() == body)
            .unwrap()
            .pos();
        run_to(&mut world, &content, 1);
        let berries = world.spawn_item("blackberries", 100, at);
        let branch = world.spawn_item("branch", 1500, at);
        let mut events = Vec::new();
        // Three days keep: at the middle of day 4 it has spoiled, and the
        // day has not started again to take it.
        run_to(&mut world, &content, 4);
        world.queue(crate::Intention::Eat {
            body,
            item: berries,
        });
        world.advance(1, &content, &mut events);
        assert!(events.iter().any(|e| matches!(
            e,
            crate::Event::ActRefused {
                reason: crate::Refusal::Spoiled,
                ..
            }
        )));
        run_to(&mut world, &content, 5);
        // Gone at the next dawn.
        assert!(world.item(berries).is_none(), "spoiled berries are gone");
        assert!(world.item(branch).is_some(), "wood does not spoil");
    }

    #[test]
    fn a_spoiled_heap_is_told_when_the_day_starts() {
        let (mut world, content, _, _) = valley_with("hazel");
        let at = world.map().camp();
        let berries = world.spawn_item("blackberries", 100, at);
        let mut events = Vec::new();
        world.advance(299, &content, &mut events); // to the last tick of day 2
        assert!(world.item(berries).is_some());
        world.advance(1, &content, &mut events); // dawn of day 3
        assert!(events.contains(&crate::Event::Spoiled {
            item: berries,
            material: "blackberries".to_owned(),
            grams: 100,
            holder: None,
        }));
        assert!(world.item(berries).is_none());
    }

    #[test]
    fn thirst_comes_faster_in_the_heat_and_at_work() {
        let needs = content().needs;
        let per_day = 100;
        let rest_spring = crate::thirst_raise(&needs, Season::Spring, false, per_day);
        let rest_summer = crate::thirst_raise(&needs, Season::Summer, false, per_day);
        let work_summer = crate::thirst_raise(&needs, Season::Summer, true, per_day);
        let rest_winter = crate::thirst_raise(&needs, Season::Winter, false, per_day);
        assert_eq!(rest_summer * 100, rest_spring * 140);
        assert_eq!(work_summer * 100, rest_summer * 150);
        assert_eq!(rest_winter * 100, rest_spring * 80);
        // In a world: a body born in spring and one born in summer, the
        // same time at rest.
        let (mut world, content, _, _) = valley_with("hazel");
        let thirst = |world: &crate::World, id| {
            world
                .bodies()
                .iter()
                .find(|b| b.id() == id)
                .unwrap()
                .condition()
                .thirst
                .points()
        };
        let spring = world.spawn_at_camp().unwrap();
        let mut events = Vec::new();
        world.advance(20, &content, &mut events);
        assert_eq!(thirst(&world, spring), 20, "a fifth of a spring day");
        run_to(&mut world, &content, 15);
        let summer = world.spawn_near(world.map().camp()).unwrap();
        world.advance(20, &content, &mut events);
        assert_eq!(thirst(&world, summer), 28, "a fifth of a summer day");
    }

    /// A kind of tree showing its fruit from `from` to `until`.
    fn fruiting(id: &str, from: (Season, u8), until: (Season, u8)) -> crate::content::Species {
        crate::content::Species {
            id: id.to_owned(),
            name: id.to_owned(),
            height_m: 5.0,
            habitats: std::collections::BTreeMap::new(),
            big: false,
            spread_m: 10,
            pioneer: false,
            fruit: Some(FruitWindow { from, until }),
        }
    }

    /// A tree of the first kind at `at`.
    fn tree_at(at: FinePos) -> crate::trees::Tree {
        crate::trees::Tree {
            at,
            species: 0,
            key: crate::trees::TreeKey {
                grid: crate::trees::TreeGrid::Narrow,
                cell: (2, 3),
                member: 0,
            },
        }
    }

    /// The fruit of `tree` of `species` bearing `crops` at the middle of
    /// `day`, with `picked`: its level.
    fn fruit_at(
        species: &crate::content::Species,
        crops: &[Crop],
        day: u64,
        tree: &crate::trees::Tree,
        picked: Option<&BTreeMap<(Source, String), Picked>>,
    ) -> u8 {
        fruit_on(
            species,
            crops,
            &calendar(),
            midday(day),
            7,
            (0, 0),
            tree,
            picked,
        )
    }

    /// A span of 100 into which `done` has passed.
    fn into(done: u128) -> Span {
        Span {
            year: 1,
            done,
            length: 100,
        }
    }

    #[test]
    fn the_season_curve_rises_holds_and_falls() {
        let ripen = Ripen::default();
        let at = |done| share_on(ripen, into(done));
        // Rises over the first fifth, evenly.
        assert_eq!(at(0), 0);
        assert_eq!(at(5), WHOLE / 4);
        assert_eq!(at(10), WHOLE / 2);
        assert_eq!(at(19), WHOLE * 19 / 20);
        // The whole, on the plateau.
        assert_eq!(at(20), WHOLE);
        assert_eq!(at(50), WHOLE);
        assert_eq!(at(70), WHOLE);
        // Falls over the last three tenths, to none at the end.
        assert_eq!(at(85), WHOLE / 2);
        assert_eq!(at(97), WHOLE / 10);
        // Without rise or fall, the whole span through.
        let flat = Ripen { rise: 0, fall: 0 };
        assert!(
            [0, 50, 99]
                .iter()
                .all(|&d| share_on(flat, into(d)) == WHOLE)
        );
        // Levels, rounded to the nearest eighth.
        assert_eq!(level_of_share(0), 0);
        assert_eq!(level_of_share(WHOLE / 16 - 1), 0);
        assert_eq!(level_of_share(WHOLE / 16), 1);
        assert_eq!(level_of_share(WHOLE * 3 / 4), 6);
        assert_eq!(level_of_share(WHOLE), LEVELS);
        // A crop's own curve, on the calendar: the hazel's autumn is days
        // 25 to 37.5; out of it, none.
        let hazel = hazel();
        let share = |day| on_plant(&hazel, &calendar(), midday(day)).map(|(_, s)| s);
        assert_eq!(share(20), None);
        assert_eq!(
            share(25),
            Some(WHOLE / 5),
            "half a day into its 2.5 to ripen"
        );
        assert_eq!(share(30), Some(WHOLE));
        assert_eq!(
            share(36),
            Some(WHOLE * 8 / 30),
            "falling: a day and a half left"
        );
        assert_eq!(share(37), None, "its end");
        // Rise and fall make a hundred at most.
        let steep = Crop {
            ripen: Ripen { rise: 60, fall: 41 },
            ..hazel
        };
        assert!(check(&steep).is_some());
    }

    #[test]
    fn a_tree_shows_no_fruit_out_of_its_window_and_without_one() {
        // 50 days a year: autumn is days 25 to 37.
        let oak = fruiting("oak", (Season::Autumn, 0), (Season::Winter, 0));
        let tree = tree_at(FinePos::new(10_500, 10_500));
        for day in [0, 10, 24, 37, 45, 74] {
            assert_eq!(fruit_at(&oak, &[], day, &tree, None), 0, "day {day}");
        }
        let bare = crate::content::Species { fruit: None, ..oak };
        assert_eq!(fruit_at(&bare, &[], 30, &tree, None), 0);
    }

    #[test]
    fn a_tree_whose_fruit_no_one_gathers_follows_the_curve_of_its_window() {
        // Acorns: no crop in this repository (D30, D45). Its window, days
        // 25 to 37.5, on the default curve.
        let oak = fruiting("oak", (Season::Autumn, 0), (Season::Winter, 0));
        let tree = tree_at(FinePos::new(10_500, 10_500));
        let shows = |day| fruit_at(&oak, &[], day, &tree, None);
        assert_eq!(shows(25), 2, "ripening: a fifth of the whole");
        assert_eq!(shows(30), LEVELS, "full on its plateau");
        assert_eq!(shows(33), LEVELS);
        assert_eq!(shows(36), 2, "falling");
        assert_eq!(shows(80), LEVELS, "the next year's");
        // A crop of its kind out of season does not count: in the window's
        // first days, before the crop comes, its window's curve shows.
        let late = Crop {
            on: "oak".to_owned(),
            from: (Season::Autumn, 50),
            ..hazel()
        };
        assert_eq!(in_season(&late, &calendar(), midday(26)), None);
        assert_eq!(fruit_at(&oak, &[late], 26, &tree, None), shows(26));
    }

    #[test]
    fn a_tree_s_load_is_what_is_on_it_against_its_rich_year() {
        let hazel_tree = fruiting("hazel", (Season::Autumn, 0), (Season::Winter, 0));
        let tree = tree_at(FinePos::new(10_500, 10_500));
        // Rich, usual and lean: 8, 6 and 1 tonne-eighths.
        let year = |lean, rich| Crop {
            grams: (1000, 6000, 8000),
            years: Years { lean, rich },
            ..hazel()
        };
        let (usual, lean, rich) = (year(0, 0), year(100, 0), year(0, 100));
        let peak = |crop: &Crop| fruit_at(&hazel_tree, std::slice::from_ref(crop), 30, &tree, None);
        assert_eq!(peak(&rich), LEVELS, "a rich year at its peak");
        assert_eq!(peak(&usual), 6, "a usual year, about three quarters");
        assert_eq!(peak(&lean), 1);
        // Ripening, a rich year shows less.
        let ripening = fruit_at(&hazel_tree, std::slice::from_ref(&rich), 25, &tree, None);
        assert_eq!(ripening, 2);
        // Picked: what is left of the rich year's whole.
        let grams = bears(&rich, 7, Frame::ORIGIN, tree.at, 1);
        let picked = |year, grams| {
            BTreeMap::from([(
                (Source::Tree(tree.key), "hazelnuts".to_owned()),
                Picked { year, grams },
            )])
        };
        let crops = [rich];
        let at = |p: &BTreeMap<_, _>| fruit_at(&hazel_tree, &crops, 30, &tree, Some(p));
        assert_eq!(at(&picked(1, grams / 2)), 4, "picked half, about half");
        assert_eq!(at(&picked(1, grams - 1)), 1, "a gram left still shows");
        assert_eq!(at(&picked(1, grams)), 0, "picked bare");
        // Another tree's picks, or last year's, take nothing off this one.
        assert_eq!(at(&picked(0, grams)), LEVELS);
        let mut other = picked(1, grams);
        let (key, value) = other.pop_first().unwrap();
        let elsewhere = Source::Plant(PlantKey { x_mm: 1, y_mm: 2 });
        other.insert((elsewhere, key.1), value);
        assert_eq!(at(&other), LEVELS);
    }

    #[test]
    fn a_window_may_run_into_the_next_year() {
        // From the middle of winter to the middle of spring, days 43.75 to
        // 56.25: winter is days 37.5 to 50 of each year of 50 days.
        let kind = fruiting("holly", (Season::Winter, 50), (Season::Spring, 50));
        let tree = tree_at(FinePos::new(10_500, 10_500));
        let shows = |day| fruit_at(&kind, &[], day, &tree, None);
        assert_eq!(shows(3), 0, "the world's first spring had no winter before");
        assert_eq!(shows(40), 0);
        assert_eq!(shows(45), 6, "ripening");
        assert_eq!(shows(52), LEVELS, "into the next year's spring");
        assert_eq!(shows(57), 0);
        assert_eq!(shows(70), 0);
        assert_eq!(shows(96), LEVELS);
        assert_eq!(shows(103), 6);
    }

    #[test]
    fn the_fruit_phase_moves_only_when_a_level_a_window_or_a_crop_does() {
        let calendar = calendar();
        let kinds = [
            fruiting("hazel", (Season::Autumn, 0), (Season::Winter, 0)),
            fruiting("oak", (Season::Autumn, 0), (Season::Winter, 50)),
        ];
        let crops = [hazel()];
        let phase = |tick| fruit_phase(&kinds, &[], &crops, &calendar, tick);
        let day = |d| phase(midday(d));
        // Spring and summer: nothing open, one phase.
        assert_eq!(day(1), day(20));
        // Autumn opens both windows and the crop, which ripen: a level
        // every few hours.
        assert_ne!(day(20), day(25));
        assert_ne!(phase(Tick(2_510)), phase(Tick(2_540)));
        // On the hazel's plateau, the oak's too (days 31 to 38): nothing
        // moves.
        assert_eq!(day(31), day(33));
        // The hazel falls, the oak's window holds.
        assert_ne!(day(33), day(36));
        // The next autumn's crop is another year's.
        assert_ne!(day(30), day(80));
        assert_eq!(day(1), day(51), "a closed year looks the same");
        // A kind without a window, or a crop on it, changes nothing.
        let windowless = [crate::content::Species {
            fruit: None,
            ..fruiting("hazel", (Season::Autumn, 0), (Season::Winter, 0))
        }];
        let flat = |d| fruit_phase(&windowless, &[], &crops, &calendar, midday(d));
        assert_eq!(flat(1), flat(30));
    }

    #[test]
    fn early_in_the_season_less_is_there_to_pick() {
        let (mut world, content, chunk, hazel) = valley_with("hazel");
        let crop = content
            .crops
            .iter()
            .find(|c| c.on == "hazel")
            .unwrap()
            .clone();
        let frame = Frame::at(world.map().origin());
        let full = bears(&crop, world.seed(), frame, hazel.at, 1);
        // Half a day into its ripening (2.5 days): a fifth of it.
        run_to(&mut world, &content, 25);
        let early = full / 5;
        assert_eq!(world.ripe_on(chunk, &hazel)[0].grams, early);
        assert_eq!(
            world.pick(chunk, hazel.source, "hazelnuts", u32::MAX),
            early
        );
        assert_eq!(world.ripe_on(chunk, &hazel), [], "nothing more today");
        assert!(
            ripening(&crop, world.calendar(), world.tick()),
            "more to come"
        );
        // Ripe: the rest of it.
        run_to(&mut world, &content, 30);
        assert_eq!(world.ripe_on(chunk, &hazel)[0].grams, full - early);
        assert!(!ripening(&crop, world.calendar(), world.tick()));
    }

    #[test]
    fn the_valley_s_hazels_and_oaks_show_their_fruit() {
        let (mut world, content, chunk, hazel) = valley_with("hazel");
        let tree = world
            .trees_in(chunk)
            .iter()
            .find(|t| Source::Tree(t.key) == hazel.source)
            .copied()
            .unwrap();
        let oak = (0..8)
            .flat_map(|y| (0..8).map(move |x| ChunkPos::new(x, y)))
            .find_map(|c| {
                world
                    .trees_in(c)
                    .iter()
                    .find(|t| content.species[t.species].id == "oak")
                    .map(|t| (c, *t))
            })
            .expect("oaks in the valley");
        assert_eq!(world.fruit_on(chunk, &tree), 0, "nothing in spring");
        assert_eq!(world.fruit_on(oak.0, &oak.1), 0);
        run_to(&mut world, &content, 30);
        assert_eq!(world.fruit_on(oak.0, &oak.1), LEVELS, "acorns");
        let crop = content.crops.iter().find(|c| c.on == "hazel").unwrap();
        let frame = Frame::at(world.map().origin());
        let full = bears(crop, world.seed(), frame, hazel.at, 1);
        let rich = rich_on(crop, world.seed(), frame, hazel.at);
        let level = u8::try_from((u64::from(full) * 16 + u64::from(rich)) / (2 * u64::from(rich)))
            .unwrap()
            .max(1);
        assert_eq!(world.fruit_on(chunk, &tree), level);
        world.pick(chunk, hazel.source, "hazelnuts", full - 1);
        assert_eq!(world.fruit_on(chunk, &tree), 1);
        world.pick(chunk, hazel.source, "hazelnuts", 1);
        assert_eq!(world.fruit_on(chunk, &tree), 0, "picked bare");
    }

    #[test]
    fn the_fruit_phase_moves_on_few_dawns_a_year() {
        // The repository's content on its calendar: on how many dawns of a
        // year the fruit shown may have moved, so its chunks go again.
        let content = content();
        let calendar = Calendar::default();
        let day = calendar.ticks_per_day();
        let days = u64::from(calendar.days_per_cycle());
        let phase = |d: u64| {
            fruit_phase(
                &content.species,
                &content.flora,
                &content.crops,
                &calendar,
                Tick(d * day),
            )
        };
        let moves = (days + 1..=2 * days)
            .filter(|&d| phase(d) != phase(d - 1))
            .count();
        eprintln!("the fruit phase moves on {moves} dawns of {days}");
        assert!(moves > 0 && moves <= usize::try_from(days).unwrap());
    }

    /// The repository's kind of shrub `id`, and its crops.
    fn shrub(id: &str) -> (crate::flora::Kind, Vec<Crop>) {
        let content = content();
        let kind = content.flora.iter().find(|k| k.id == id).unwrap().clone();
        let crops = content.crops.into_iter().filter(|c| c.on == id).collect();
        (kind, crops)
    }

    /// The fruit of a plant of `kind` bearing `crops`, standing at `at` in
    /// chunk (0, 0), at the middle of `day`, with `picked`.
    fn plant_fruit_at(
        kind: &crate::flora::Kind,
        crops: &[Crop],
        day: u64,
        at: FinePos,
        picked: Option<&BTreeMap<(Source, String), Picked>>,
    ) -> u8 {
        let chunk = ChunkPos::new(0, 0);
        plant_fruit_on(
            kind,
            crops,
            &calendar(),
            midday(day),
            7,
            (0, 0),
            chunk,
            at,
            picked,
        )
    }

    #[test]
    fn the_berry_shrubs_show_their_fruit_while_their_crop_is_on() {
        let content = content();
        let fruiting: Vec<&str> = content
            .flora
            .iter()
            .filter(|k| k.fruit.is_some())
            .map(|k| k.id.as_str())
            .collect();
        assert_eq!(fruiting.len(), 14, "{fruiting:?}");
        for kind in content.flora.iter().filter(|k| k.fruit.is_some()) {
            let window = kind.fruit.unwrap();
            for crop in content.crops.iter().filter(|c| c.on == kind.id) {
                assert_eq!(
                    (window.from, window.until),
                    (crop.from, crop.until),
                    "{}",
                    kind.id
                );
            }
        }
    }

    #[test]
    fn a_shrub_shows_no_berries_out_of_its_window_and_without_one() {
        // Blackberries: late summer to the autumn's middle, days 20 to 32.5.
        let (bramble, crops) = shrub("bramble");
        let at = FinePos::new(10_500, 10_500);
        for day in [0, 10, 19, 33, 45] {
            assert_eq!(
                plant_fruit_at(&bramble, &crops, day, at, None),
                0,
                "day {day}"
            );
        }
        let bare = crate::flora::Kind {
            fruit: None,
            ..bramble
        };
        assert_eq!(plant_fruit_at(&bare, &crops, 25, at, None), 0);
        // Off its chunk, no plant is there.
        let (bramble, crops) = shrub("bramble");
        let off = FinePos::new(-500, 10_500);
        assert_eq!(plant_fruit_at(&bramble, &crops, 25, off, None), 0);
    }

    #[test]
    fn a_shrub_in_season_shows_its_load_and_less_once_picked() {
        let (bramble, crops) = shrub("bramble");
        let rich: Vec<Crop> = crops
            .iter()
            .map(|c| Crop {
                years: Years { lean: 0, rich: 100 },
                ..c.clone()
            })
            .collect();
        let at = FinePos::new(10_500, 10_500);
        assert_eq!(plant_fruit_at(&bramble, &rich, 25, at, None), LEVELS);
        let grams = bears(&rich[0], 7, Frame::ORIGIN, at, 1);
        let key = PlantKey::of(ChunkPos::new(0, 0), at).unwrap();
        let picked = |grams| {
            BTreeMap::from([(
                (Source::Plant(key), "blackberries".to_owned()),
                Picked { year: 1, grams },
            )])
        };
        let shows = |p: &BTreeMap<_, _>| plant_fruit_at(&bramble, &rich, 25, at, Some(p));
        assert_eq!(shows(&picked(grams / 4)), 6);
        assert_eq!(shows(&picked(grams / 2)), 4, "picked half");
        assert_eq!(shows(&picked(grams)), 0, "picked bare");
        // Another plant's picks take nothing off this one.
        let next = FinePos::new(10_600, 10_500);
        let elsewhere = plant_fruit_at(&bramble, &rich, 25, next, Some(&picked(grams)));
        assert_eq!(elsewhere, LEVELS);
        // The elder bears nothing here (D30): its window's curve, days 20
        // to 30.
        let (elder, none) = shrub("elder");
        assert_eq!(none, []);
        assert_eq!(plant_fruit_at(&elder, &none, 25, at, None), LEVELS);
        assert_eq!(plant_fruit_at(&elder, &none, 35, at, None), 0);
    }

    #[test]
    fn the_fruit_phase_moves_with_the_shrubs_windows_and_crops() {
        let calendar = calendar();
        let (bramble, crops) = shrub("bramble");
        let phase = |kinds: &[crate::flora::Kind], day| {
            fruit_phase(&[], kinds, &crops, &calendar, midday(day))
        };
        // Days 20 to 32.5: ripe from day 22.5, falling from 28.75.
        let kinds = [bramble.clone()];
        assert_eq!(phase(&kinds, 1), phase(&kinds, 19));
        assert_ne!(
            phase(&kinds, 19),
            phase(&kinds, 21),
            "the blackberries come"
        );
        assert_eq!(phase(&kinds, 23), phase(&kinds, 27));
        assert_ne!(phase(&kinds, 27), phase(&kinds, 31), "and go");
        let windowless = [crate::flora::Kind {
            fruit: None,
            ..bramble
        }];
        assert_eq!(phase(&windowless, 1), phase(&windowless, 25));
    }

    #[test]
    fn the_valley_s_brambles_show_their_berries() {
        let (mut world, content, chunk, bramble) = valley_with("bramble");
        let plant = crate::flora::Plant {
            at: bramble.at,
            kind: u16::try_from(
                content
                    .flora
                    .iter()
                    .position(|k| k.id == "bramble")
                    .unwrap(),
            )
            .unwrap(),
        };
        assert_eq!(world.plant_fruit_on(chunk, &plant), 0, "nothing in spring");
        run_to(&mut world, &content, 25); // late summer
        let crop = content.crops.iter().find(|c| c.on == "bramble").unwrap();
        let frame = Frame::at(world.map().origin());
        let full = bears(crop, world.seed(), frame, bramble.at, 1);
        let before = world.plant_fruit_on(chunk, &plant);
        assert!(before > 0);
        world.pick(chunk, bramble.source, "blackberries", full / 2);
        assert!(
            world.plant_fruit_on(chunk, &plant) < before,
            "picked: lower"
        );
        world.pick(chunk, bramble.source, "blackberries", full);
        assert_eq!(world.plant_fruit_on(chunk, &plant), 0, "picked bare");
    }
}
