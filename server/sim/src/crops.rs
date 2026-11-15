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

use serde::{Deserialize, Serialize};

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

/// The year whose crop is on the plant at `tick`: this year's, last
/// year's if it runs into this one, or `None` out of its season.
#[must_use]
pub fn in_season(crop: &Crop, calendar: &Calendar, tick: Tick) -> Option<u64> {
    let per_year = calendar.ticks_per_day() * u64::from(calendar.days_per_year());
    let year = tick.0 / per_year;
    // Compared in four hundredths of a year, without rounding.
    let at = u128::from(tick.0 % per_year) * 400;
    let mark = |p: u64| u128::from(p.min(400)) * u128::from(per_year);
    let (from, until) = (mark(point(crop.from)), mark(point(crop.until)));
    if from < until {
        (from <= at && at < until).then_some(year)
    } else if at >= from {
        Some(year)
    } else if at < until {
        year.checked_sub(1)
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
    const PURPOSE: u64 = 0x5348_4152; // "SHAR"
    let tile = at.tile();
    let base = match year_of(crop, seed, frame.tile(tile.x, tile.y), year) {
        Year::Lean => crop.grams.0,
        Year::Usual => crop.grams.1,
        Year::Rich => crop.grams.2,
    };
    let (x, y) = frame.mm(at.x, at.y);
    let share = 750 + hash(&[PURPOSE, seed, text_key(&crop.on), key(x), key(y)]) % 501;
    u32::try_from(u64::from(base) * share / 1000).expect("under five quarters of a tonne")
}

/// A plant's key in its chunk: its place from the chunk's corner, in
/// millimetres. Plants keep their room, so two never share a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// A tree of the chunk.
    Tree(TreeKey),
    /// A plant of the chunk.
    Plant(PlantKey),
}

/// A tree or a plant bearing crops in some season, as the world places
/// it ([`crate::World::bearers_in`]).
#[derive(Debug, Clone, PartialEq, Eq)]
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
mod tests {
    use super::*;

    fn hazel() -> Crop {
        Crop {
            on: "hazel".to_owned(),
            gives: "hazelnuts".to_owned(),
            from: (Season::Autumn, 0),
            until: (Season::Winter, 0),
            grams: (1000, 5000, 10_000),
            years: Years { lean: 25, rich: 25 },
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
        assert_eq!(in_season(&hazel, &calendar, midday(30)), Some(0));
        assert_eq!(in_season(&hazel, &calendar, midday(80)), Some(1));
        // The first tick of autumn bears; the first of winter does not.
        assert_eq!(in_season(&hazel, &calendar, Tick(2500)), Some(0));
        assert_eq!(in_season(&hazel, &calendar, Tick(3749)), Some(0));
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
        assert_eq!(in_season(&hips, &calendar, midday(32)), Some(0));
        assert_eq!(in_season(&hips, &calendar, midday(45)), Some(0));
        assert_eq!(in_season(&hips, &calendar, midday(51)), Some(0));
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
                Some(day / 50)
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
    fn content() -> crate::Content {
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
    fn valley_with(kind: &str) -> (crate::World, crate::Content, ChunkPos, Bearer) {
        let content = content();
        let world = crate::World::with_valley(calendar(), &content, 256);
        let side = 256 / i32::try_from(CHUNK_SIZE).unwrap();
        for y in 0..side {
            for x in 0..side {
                let chunk = ChunkPos::new(x, y);
                if let Some(bearer) = world.bearers_in(chunk).into_iter().find(|b| b.kind == kind) {
                    return (world, content, chunk, bearer);
                }
            }
        }
        panic!("no {kind} in the valley");
    }

    /// Runs `world` to the middle of `day`.
    fn run_to(world: &mut crate::World, content: &crate::Content, day: u64) {
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
        let full = bears(crop, world.seed(), frame, hazel.at, 0);
        let ripe = world.ripe_on(chunk, &hazel);
        assert_eq!(
            ripe,
            [Ripe {
                gives: "hazelnuts".to_owned(),
                year: 0,
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
        let next = bears(crop, world.seed(), frame, hazel.at, 1);
        assert_eq!(world.ripe_on(chunk, &hazel)[0].grams, next);
        assert_eq!(world.ripe_on(chunk, &hazel)[0].year, 1);
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
        assert_eq!((taken.year, taken.grams), (0, 100));
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
}
