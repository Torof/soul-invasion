//! Tiny worlds for visual tests (`--world`): a flat square of grass, and
//! the steps test's blocks and ridge. Ephemeral and never for play. Both
//! carry heights (the flat one all zero): the client knows a test world by
//! them and draws it with no fog.

use sim::Calendar;
use sim::map::{Map, Terrain};
use sim::{Content, World};

use crate::cli::TestWorld;

/// The steps test (third version, 13 November 2026), row by row from the
/// north-west corner: `.` the ground; `1` and `2` that many metres up,
/// stacked like cubes in the north-east corner (the screen's right), a
/// 2 m block on a 1 m one, every face a wall; `a` to `d` a small ridge's
/// tiles (2, 4, 6 and 8 decimetres) running down the screen left of the
/// camp, rising towards the screen's right and falling past its crest. A
/// height that is not a whole number of metres is a slope's tile: the
/// client draws it flat, with the slope's grass inclined the way it rises.
/// The camp (6, 6) lies on the ground between the ridge and the blocks.
const MARCHES: [&str; 12] = [
    "............",
    "a......1221.",
    "ba.....1221.",
    "cba....1111.",
    "dcba...1111.",
    "ddcba.......",
    "cddcba......",
    "bcddcba.....",
    "abcddcba....",
    ".abcddcba...",
    "..abcddcba..",
    "...abcddcba.",
];

/// The steps test's side, in tiles.
pub const MARCHES_SIDE: u32 = 12;

/// The heights of the steps test's tiles, in decimetres, row by row.
#[must_use]
pub fn marches_heights() -> Vec<i16> {
    let side = MARCHES_SIDE as usize;
    let mut out = vec![0; side * side];
    for (y, row) in MARCHES.iter().enumerate() {
        for (x, c) in row.bytes().enumerate() {
            out[y * side + x] = match c {
                b'1' | b'2' => i16::from(c - b'0') * 10,
                b'a'..=b'd' => i16::from(c - b'a' + 1) * 2,
                _ => 0,
            };
        }
    }
    out
}

/// A test world at tick zero, with nobody in it yet.
#[must_use]
pub fn make(world: TestWorld, calendar: Calendar, content: &Content) -> World {
    let map = match world {
        TestWorld::Flat(n) => {
            Map::filled(n, n, Terrain::Grass).with_heights(vec![0; (n * n) as usize])
        }
        TestWorld::Marches => {
            Map::filled(MARCHES_SIDE, MARCHES_SIDE, Terrain::Grass).with_heights(marches_heights())
        }
    };
    World::on_valley(calendar, content, map)
}
