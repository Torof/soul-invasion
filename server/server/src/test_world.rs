//! Tiny worlds for visual tests (`--world`): a flat square of grass, the
//! steps test's ridge and the mountain test's flank. Ephemeral and never for play. Both
//! carry heights (the flat one all zero): the client knows a test world by
//! them and draws it with no fog.

use sim::Calendar;
use sim::map::{Map, Terrain};
use sim::{Content, World};

use crate::cli::TestWorld;

/// The steps test (eleventh version, 13 November 2026): one ridge whose
/// crest bends at the map's middle [`MARCHES_BEND`], (14.5, 14.5) in tiles.
/// From the screen's right corner, the map's (29, 0), to the middle it
/// runs straight across the screen along `x + y` = 29, as the eighth
/// version did; from the middle it turns down and to the left on screen,
/// along `x` = 14.5, to the middle of the diamond's lower left side, the
/// map's (14.5, 29). The screen's horizontal is the map's `x - y` and its
/// vertical `x + y` (`Iso`). A tile's height falls evenly with its
/// distance to that bent line, from the crest's [`MARCHES_CREST`]
/// decimetres to flat ground at [`MARCHES_FALLOFF`] (in the eighth
/// version's measure, a step of `x + y`, so the straight half is exactly
/// the eighth version's; only the inside of the bend, below the middle,
/// where the two arms' flanks meet, stands higher). A height that is not
/// a whole number of metres is a slope's tile (the client draws it flat,
/// with the slope's grass inclined the way it rises), so a slope's tile
/// that would be one is a decimetre lower. The camp lies on the flat
/// ground near the screen's bottom corner.
///
/// Distances are reckoned in half tiles (the bend and the end lie between
/// tiles) and in tenths of a step of `x + y` (a step is 1/√2 of a tile
/// across the straight half), all in integers.
const MARCHES_BEND: (i32, i32) = (29, 29);

/// The crest's two ends, in half tiles: the screen's right corner, and
/// the middle of the diamond's lower left side.
const MARCHES_ENDS: [(i32, i32); 2] = [(58, 0), (29, 58)];

/// The steps test's crest, in decimetres (before the decimetre off).
const MARCHES_CREST: i32 = 30;

/// Where the ground is flat again on either side, in steps of `x + y`
/// from the crest.
const MARCHES_FALLOFF: i32 = 11;

/// The steps test's side, in tiles.
pub const MARCHES_SIDE: u32 = 30;

/// The steps test's camp: on the flat ground near the screen's bottom
/// corner (`x + y` = 54, 25 steps from the straight half, 17.7 from the
/// bent one).
pub const MARCHES_CAMP: (u32, u32) = (27, 27);

/// The squared distance, in half tiles, from `p` to the segment `a`–`b`
/// (all in half tiles), as a fraction `num / den` (integers).
fn squared_to_segment(p: (i32, i32), a: (i32, i32), b: (i32, i32)) -> (i64, i64) {
    let (dx, dy) = (i64::from(b.0 - a.0), i64::from(b.1 - a.1));
    let (px, py) = (i64::from(p.0 - a.0), i64::from(p.1 - a.1));
    let len = dx * dx + dy * dy;
    let t = (px * dx + py * dy).clamp(0, len);
    // p - a - (t / len)(b - a), times len.
    let (ex, ey) = (px * len - t * dx, py * len - t * dy);
    (ex * ex + ey * ey, len * len)
}

/// A tile's distance to the bent crest, in tenths of a step of `x + y`.
fn marches_away(x: i32, y: i32) -> i32 {
    let p = (2 * x, 2 * y);
    let tenths = MARCHES_ENDS
        .iter()
        .map(|&end| {
            let (num, den) = squared_to_segment(p, MARCHES_BEND, end);
            // A step is 1/√2 of a tile, √2 half tiles: (10 d / √2)² = 50 d².
            (50 * num / den).isqrt()
        })
        .min()
        .expect("two arms");
    i32::try_from(tenths).expect("small")
}

/// The heights of the steps test's tiles, in decimetres, row by row.
#[must_use]
pub fn marches_heights() -> Vec<i16> {
    let side = MARCHES_SIDE as usize;
    let mut out = vec![0; side * side];
    for (i, h) in out.iter_mut().enumerate() {
        let x = i32::try_from(i % side).expect("small");
        let y = i32::try_from(i / side).expect("small");
        let falloff = 10 * MARCHES_FALLOFF;
        let dm = (MARCHES_CREST * (falloff - marches_away(x, y)) / falloff).max(0);
        let dm = if dm > 0 && dm % 10 == 0 { dm - 1 } else { dm };
        *h = i16::try_from(dm).expect("a few metres");
    }
    out
}

/// The mountain test (13 November 2026, version G): a window of
/// [`MOUNTAIN_SIDE`] tiles cut at (320, 310) from the relief the project
/// lead chose on images (`G-D-textures-scenario`): relief D, a crest that
/// winds, ravines running down its flanks along the fall line, with flat
/// floors that widen and deepen downhill and unequal sides, and benches
/// every 3 m of height; its shapes stretched 7.5 times across and 2.5
/// times in height (a third of the first version's heights). The map's
/// `x` and `y` are the image's `u` and `v`; a tile's height is the
/// relief's at its middle, (320 + x + 0.5, 310 + y + 0.5), rounded to
/// decimetres.
///
/// D's ravines follow traced fall lines, too heavy to port: the heights
/// are baked into [`MOUNTAIN_HEIGHTS`] (`mountain_heights.bin`, i16
/// decimetres little endian, row by row) by `bake-montagne-g.py` over
/// `montagne-scenario.py` (the project lead's local reserve,
/// `content/ai-reserve/notes/montagne-g/`). Test only.
pub const MOUNTAIN_SIDE: u32 = 192;

/// The mountain test's heights, baked: decimetres, i16 little endian,
/// [`MOUNTAIN_SIDE`] rows of [`MOUNTAIN_SIDE`].
const MOUNTAIN_HEIGHTS: &[u8] = include_bytes!("mountain_heights.bin");

/// Where the camp should stand, the screen's middle left; it stands on
/// the nearest gentle tile to it ([`mountain_camp`]).
const MOUNTAIN_CAMP_NEAR: (i32, i32) = (61, 136);

/// A gentle tile's largest rise, squared: the rise to its neighbours at
/// two tiles on either side, in decimetres over four tiles (0.12 m a
/// tile, where the slope's gentle grass starts to give way, is 4.8 dm;
/// 23 is under 4.8²).
const MOUNTAIN_GENTLE: i32 = 23;

/// The heights of the mountain test's tiles, in decimetres, row by row.
#[must_use]
pub fn mountain_heights() -> Vec<i16> {
    let side = MOUNTAIN_SIDE as usize;
    assert_eq!(MOUNTAIN_HEIGHTS.len(), 2 * side * side, "a whole window");
    MOUNTAIN_HEIGHTS
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&b| i16::from_le_bytes(b))
        .collect()
}

/// Where the mountain test's camp stands: the gentle tile (its rise to
/// its four neighbours at two tiles at most [`MOUNTAIN_GENTLE`]) nearest
/// [`MOUNTAIN_CAMP_NEAR`], the first row by row among equals.
#[must_use]
pub fn mountain_camp(heights: &[i16]) -> (u32, u32) {
    let side = i32::try_from(MOUNTAIN_SIDE).expect("small");
    let at =
        |x: i32, y: i32| i32::from(heights[usize::try_from(y * side + x).expect("on the map")]);
    let (cx, cy) = MOUNTAIN_CAMP_NEAR;
    let mut best = (i32::MAX, MOUNTAIN_CAMP_NEAR);
    for y in 2..side - 2 {
        for x in 2..side - 2 {
            let du = at(x + 2, y) - at(x - 2, y);
            let dv = at(x, y + 2) - at(x, y - 2);
            let away = (x - cx).pow(2) + (y - cy).pow(2);
            if du * du + dv * dv <= MOUNTAIN_GENTLE && away < best.0 {
                best = (away, (x, y));
            }
        }
    }
    let (x, y) = best.1;
    (
        u32::try_from(x).expect("on the map"),
        u32::try_from(y).expect("on the map"),
    )
}

/// A test world at tick zero, with nobody in it yet.
#[must_use]
pub fn make(world: TestWorld, calendar: Calendar, content: &Content) -> World {
    let map = match world {
        TestWorld::Flat(n) => {
            Map::filled(n, n, Terrain::Grass).with_heights(vec![0; (n * n) as usize])
        }
        TestWorld::Marches => {
            let (x, y) = MARCHES_CAMP;
            Map::filled(MARCHES_SIDE, MARCHES_SIDE, Terrain::Grass)
                .with_heights(marches_heights())
                .with_camp(x, y)
        }
        TestWorld::Mountain => {
            let heights = mountain_heights();
            let (x, y) = mountain_camp(&heights);
            Map::filled(MOUNTAIN_SIDE, MOUNTAIN_SIDE, Terrain::Grass)
                .with_heights(heights)
                .with_camp(x, y)
        }
    };
    World::on_valley(calendar, content, map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32) -> i16 {
        let side = i32::try_from(MARCHES_SIDE).expect("small");
        marches_heights()[usize::try_from(y * side + x).expect("on the map")]
    }

    #[test]
    fn the_straight_half_is_the_eighth_versions_and_the_camp_is_flat() {
        for x in 0..30_i32 {
            for y in 0..30_i32 {
                // Above the bend's level, right of the screen's middle.
                if x > y && x + y <= 29 {
                    let away = (x + y - 29).abs();
                    let dm = (30 * (11 - away) / 11).max(0);
                    let dm = if dm > 0 && dm % 10 == 0 { dm - 1 } else { dm };
                    assert_eq!(i32::from(at(x, y)), dm, "({x}, {y})");
                }
            }
        }
        let (x, y) = MARCHES_CAMP;
        assert_eq!((x, y), (27, 27));
        assert_eq!(at(27, 27), 0);
    }

    #[test]
    fn the_crest_bends_down_to_the_lower_left_sides_middle() {
        // Along x = 14.5, both sides of it stand near the crest.
        assert!(at(14, 28) >= 27 && at(15, 28) >= 27);
        // The left corner is flat now.
        assert_eq!(at(0, 29), 0);
    }

    #[test]
    fn the_mountain_is_baked_whole_and_its_camp_is_gentle() {
        let heights = mountain_heights();
        let lowest = heights.iter().min().copied();
        let highest = heights.iter().max().copied();
        assert_eq!((lowest, highest), (Some(208), Some(802)), "relief D at 2.5");
        let (x, y) = mountain_camp(&heights);
        assert_eq!((x, y), (71, 145));
        let side = MOUNTAIN_SIDE as usize;
        let at = |x: usize, y: usize| i32::from(heights[y * side + x]);
        let (x, y) = (x as usize, y as usize);
        let du = at(x + 2, y) - at(x - 2, y);
        let dv = at(x, y + 2) - at(x, y - 2);
        assert!(du * du + dv * dv <= MOUNTAIN_GENTLE);
    }
}
