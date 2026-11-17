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

use serde::{Deserialize, Serialize};

use crate::changes::Picked;
use crate::r#gen::hash;
use crate::r#gen::scatter::Frame;
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
}

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
    let per_year = calendar.ticks_per_day() * u64::from(calendar.days_per_year());
    let year = tick.0 / per_year + 1;
    // Compared in four hundredths of a year, without rounding.
    let at = u128::from(tick.0 % per_year) * 400;
    let mark = |p: u64| u128::from(p.min(400)) * u128::from(per_year);
    let (from, until) = (mark(point(from)), mark(point(until)));
    if from < until {
        (from <= at && at < until).then_some(year)
    } else if at >= from {
        Some(year)
    } else if at < until {
        (year > 1).then(|| year - 1)
    } else {
        None
    }
}

/// A number for a text (an id), the same on every machine.
fn text_key(text: &str) -> u64 {
    hash(&text.bytes().map(u64::from).collect::<Vec<_>>())
}

/// A whole number as a hash key.
fn key(value: i64) -> u64 {
    u64::from_le_bytes(value.to_le_bytes())
}

/// What year `crop`'s kind has in `year` around the world's tile `tile`.
pub(crate) fn year_of(crop: &Crop, seed: u64, tile: (i32, i32), year: u64) -> Year {
    const PURPOSE: u64 = 0x4d41_5354; // "MAST"
    let (rx, ry) = (tile.0.div_euclid(MAST_CELL), tile.1.div_euclid(MAST_CELL));
    let roll = hash(&[
        PURPOSE,
        seed,
        text_key(&crop.on),
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
pub(crate) fn bears(crop: &Crop, seed: u64, frame: Frame, at: FinePos, year: u64) -> u32 {
    let tile = at.tile();
    let base = match year_of(crop, seed, frame.tile(tile.x, tile.y), year) {
        Year::Lean => crop.grams.0,
        Year::Usual => crop.grams.1,
        Year::Rich => crop.grams.2,
    };
    u32::try_from(u64::from(base) * share(crop, seed, frame, at) / 1000)
        .expect("under five quarters of a tonne")
}

/// The share of its kind's crop a plant of `crop`'s kind standing at
/// `at` bears, in thousandths: 750 to 1250.
fn share(crop: &Crop, seed: u64, frame: Frame, at: FinePos) -> u64 {
    const PURPOSE: u64 = 0x5348_4152; // "SHAR"
    let (x, y) = frame.mm(at.x, at.y);
    750 + hash(&[PURPOSE, seed, text_key(&crop.on), key(x), key(y)]) % 501
}

/// What a plant of `crop`'s kind standing at `at` bears in a usual year,
/// in grams: its share of the kind's usual crop.
fn usual_on(crop: &Crop, seed: u64, frame: Frame, at: FinePos) -> u32 {
    u32::try_from(u64::from(crop.grams.1) * share(crop, seed, frame, at) / 1000)
        .expect("under five quarters of a tonne")
}

/// When a kind of tree shows its fruit (D48, `species.ron`'s `fruit`):
/// from a point of the year to another, as a crop's ([`Crop::from`],
/// [`Crop::until`]), running into the next year when it ends before it
/// starts.
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

/// Is `window` open at `tick`? As a crop, a window running into the next
/// year is shut in the world's first spring: there was no autumn before.
#[must_use]
pub fn window_open(window: &FruitWindow, calendar: &Calendar, tick: Tick) -> bool {
    span_year(window.from, window.until, calendar, tick).is_some()
}

/// How much fruit a tree shows (D48): the client draws its image of that
/// state. On the wire, 0, 1 and 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Fruit {
    /// No fruit: out of its window, or picked bare.
    Empty = 0,
    /// Some fruit: a lean year, or picked.
    Third = 1,
    /// Laden: two thirds of a usual year's crop or more on it, or fruit no
    /// one gathers in its window.
    Full = 2,
}

/// The fruit `tree` (of `species`, its crops among `crops`) shows at
/// `tick` (D48), on a map whose corner is `origin` in the world
/// ([`crate::map::Map::origin`]), with `picked` its chunk's picks:
///
/// - out of its species' fruit window, or without one: [`Fruit::Empty`];
/// - in it, when a crop of its species is in season: what is left on the
///   tree of this year's crop against what it bears in a usual year: two
///   thirds of it or more [`Fruit::Full`], less but something
///   [`Fruit::Third`], nothing [`Fruit::Empty`] (the best of its crops in
///   season, should it have two);
/// - in it with no crop in season (acorns, cones: fruit no one gathers):
///   [`Fruit::Full`].
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
) -> Fruit {
    let frame = Frame::at(origin);
    let Some(window) = &species.fruit else {
        return Fruit::Empty;
    };
    if !window_open(window, calendar, tick) {
        return Fruit::Empty;
    }
    let source = Source::Tree(tree.key);
    crops
        .iter()
        .filter(|crop| crop.on == species.id)
        .filter_map(|crop| {
            let year = in_season(crop, calendar, tick)?;
            let grams = bears(crop, seed, frame, tree.at, year);
            let taken = picked
                .and_then(|p| p.get(&(source, crop.gives.clone())))
                .filter(|p| p.year == year)
                .map_or(0, |p| p.grams);
            let left = u64::from(grams.saturating_sub(taken));
            let usual = u64::from(usual_on(crop, seed, frame, tree.at));
            Some(if left == 0 {
                Fruit::Empty
            } else if left * 3 >= usual * 2 {
                Fruit::Full
            } else {
                Fruit::Third
            })
        })
        .max()
        .unwrap_or(Fruit::Full)
}

/// Which fruit windows and tree crops are open at `tick`, as one number
/// (D48): the fruit every tree shows follows from it, the seed and the
/// picks. It changes only when a window or a crop of a tree with a window
/// opens or shuts, or a crop's year turns: the server sends a chunk again
/// when it does.
#[must_use]
pub fn fruit_phase(
    species: &[crate::content::Species],
    crops: &[Crop],
    calendar: &Calendar,
    tick: Tick,
) -> u64 {
    const PURPOSE: u64 = 0x4652_5554; // "FRUT"
    let mut values = vec![PURPOSE];
    for kind in species {
        let Some(window) = &kind.fruit else {
            continue;
        };
        values.push(u64::from(window_open(window, calendar, tick)));
        for crop in crops.iter().filter(|c| c.on == kind.id) {
            values.push(in_season(crop, calendar, tick).map_or(0, |year| year + 1));
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
    /// Its kind's id (`hazel`, `bramble`).
    pub kind: String,
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
        }
    }

    fn calendar() -> Calendar {
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
                    .find(|b| b.kind == kind)
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
    /// `day`, with `picked`.
    fn fruit_at(
        species: &crate::content::Species,
        crops: &[Crop],
        day: u64,
        tree: &crate::trees::Tree,
        picked: Option<&BTreeMap<(Source, String), Picked>>,
    ) -> Fruit {
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

    #[test]
    fn a_tree_shows_no_fruit_out_of_its_window_and_without_one() {
        // 50 days a year: autumn is days 25 to 37.
        let oak = fruiting("oak", (Season::Autumn, 0), (Season::Winter, 0));
        let tree = tree_at(FinePos::new(10_500, 10_500));
        for day in [0, 10, 24, 37, 45, 74] {
            assert_eq!(
                fruit_at(&oak, &[], day, &tree, None),
                Fruit::Empty,
                "day {day}"
            );
        }
        let bare = crate::content::Species { fruit: None, ..oak };
        assert_eq!(fruit_at(&bare, &[], 30, &tree, None), Fruit::Empty);
    }

    #[test]
    fn a_tree_whose_fruit_no_one_gathers_is_laden_through_its_window() {
        // Acorns: no crop in this repository (D30, D45).
        let oak = fruiting("oak", (Season::Autumn, 0), (Season::Winter, 0));
        let tree = tree_at(FinePos::new(10_500, 10_500));
        for day in [25, 30, 36, 75, 86] {
            assert_eq!(
                fruit_at(&oak, &[], day, &tree, None),
                Fruit::Full,
                "day {day}"
            );
        }
        // A crop of its kind out of season does not count: in the window's
        // first days, before the crop comes, it is laden.
        let late = Crop {
            on: "oak".to_owned(),
            from: (Season::Autumn, 50),
            ..hazel()
        };
        assert_eq!(in_season(&late, &calendar(), midday(26)), None);
        assert_eq!(fruit_at(&oak, &[late], 26, &tree, None), Fruit::Full);
    }

    #[test]
    fn a_crop_in_season_shows_as_much_as_is_left_of_a_usual_year() {
        let hazel_tree = fruiting("hazel", (Season::Autumn, 0), (Season::Winter, 0));
        let tree = tree_at(FinePos::new(10_500, 10_500));
        let year = |lean, rich| Crop {
            years: Years { lean, rich },
            ..hazel()
        };
        let (usual, lean, rich) = (year(0, 0), year(100, 0), year(0, 100));
        assert_eq!(
            fruit_at(&hazel_tree, std::slice::from_ref(&usual), 30, &tree, None),
            Fruit::Full
        );
        assert_eq!(
            fruit_at(&hazel_tree, &[lean], 30, &tree, None),
            Fruit::Third,
            "a lean year shows a third from the start"
        );
        assert_eq!(fruit_at(&hazel_tree, &[rich], 30, &tree, None), Fruit::Full);
        // Picked: two thirds left or more, laden; less, a third; none, bare.
        let grams = bears(&usual, 7, Frame::ORIGIN, tree.at, 1);
        let picked = |year, grams| {
            BTreeMap::from([(
                (Source::Tree(tree.key), "hazelnuts".to_owned()),
                Picked { year, grams },
            )])
        };
        let crops = [usual];
        let at = |p: &BTreeMap<_, _>| fruit_at(&hazel_tree, &crops, 30, &tree, Some(p));
        assert_eq!(at(&picked(1, grams / 4)), Fruit::Full);
        assert_eq!(at(&picked(1, grams / 2)), Fruit::Third);
        assert_eq!(at(&picked(1, grams - 1)), Fruit::Third);
        assert_eq!(at(&picked(1, grams)), Fruit::Empty, "picked bare");
        // Another tree's picks, or last year's, take nothing off this one.
        assert_eq!(at(&picked(0, grams)), Fruit::Full);
        let mut other = picked(1, grams);
        let (key, value) = other.pop_first().unwrap();
        let elsewhere = Source::Plant(PlantKey { x_mm: 1, y_mm: 2 });
        other.insert((elsewhere, key.1), value);
        assert_eq!(at(&other), Fruit::Full);
    }

    #[test]
    fn a_window_may_run_into_the_next_year() {
        // From the middle of winter to the middle of spring; winter is
        // days 37.5 to 50 of each year of 50 days.
        let kind = fruiting("holly", (Season::Winter, 50), (Season::Spring, 50));
        let tree = tree_at(FinePos::new(10_500, 10_500));
        let shows = |day| fruit_at(&kind, &[], day, &tree, None);
        assert_eq!(
            shows(3),
            Fruit::Empty,
            "the world's first spring had no winter before"
        );
        assert_eq!(shows(40), Fruit::Empty);
        assert_eq!(shows(45), Fruit::Full);
        assert_eq!(shows(52), Fruit::Full, "into the next year's spring");
        assert_eq!(shows(57), Fruit::Empty);
        assert_eq!(shows(70), Fruit::Empty);
        assert_eq!(shows(96), Fruit::Full);
        assert_eq!(shows(103), Fruit::Full);
    }

    #[test]
    fn the_fruit_phase_moves_only_when_a_window_or_a_crop_does() {
        let calendar = calendar();
        let kinds = [
            fruiting("hazel", (Season::Autumn, 0), (Season::Winter, 0)),
            fruiting("oak", (Season::Autumn, 0), (Season::Winter, 50)),
        ];
        let crops = [hazel()];
        let phase = |day| fruit_phase(&kinds, &crops, &calendar, midday(day));
        // Spring and summer: nothing open, one phase.
        assert_eq!(phase(1), phase(20));
        // Autumn opens both windows and the crop.
        assert_ne!(phase(20), phase(26));
        assert_eq!(phase(26), phase(36));
        // Winter shuts the hazel's, not yet the oak's.
        assert_ne!(phase(36), phase(38));
        assert_ne!(phase(38), phase(45));
        // The next autumn's crop is another year's.
        assert_ne!(phase(30), phase(80));
        assert_eq!(phase(1), phase(51), "a closed year looks the same");
        // A kind without a window, or a crop on it, changes nothing.
        let windowless = [crate::content::Species {
            fruit: None,
            ..fruiting("hazel", (Season::Autumn, 0), (Season::Winter, 0))
        }];
        let flat = |day| fruit_phase(&windowless, &crops, &calendar, midday(day));
        assert_eq!(flat(1), flat(30));
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
        assert_eq!(
            world.fruit_on(chunk, &tree),
            Fruit::Empty,
            "nothing in spring"
        );
        assert_eq!(world.fruit_on(oak.0, &oak.1), Fruit::Empty);
        run_to(&mut world, &content, 30);
        assert_eq!(world.fruit_on(oak.0, &oak.1), Fruit::Full, "acorns");
        let crop = &content.crops[0];
        let frame = Frame::at(world.map().origin());
        let full = bears(crop, world.seed(), frame, hazel.at, 1);
        let usual = usual_on(crop, world.seed(), frame, hazel.at);
        let first = world.fruit_on(chunk, &tree);
        assert_eq!(
            first,
            if full * 3 >= usual * 2 {
                Fruit::Full
            } else {
                Fruit::Third
            }
        );
        world.pick(chunk, hazel.source, "hazelnuts", full - 1);
        assert_eq!(world.fruit_on(chunk, &tree), Fruit::Third);
        world.pick(chunk, hazel.source, "hazelnuts", 1);
        assert_eq!(world.fruit_on(chunk, &tree), Fruit::Empty, "picked bare");
    }
}
