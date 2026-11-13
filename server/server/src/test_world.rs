//! Tiny worlds for visual tests (`--world`): a flat square of grass, and
//! the steps test's hill. Ephemeral and never for play.

use sim::Calendar;
use sim::map::{Map, Terrain};
use sim::{Content, World};

use crate::cli::TestWorld;

/// The steps test's hill, row by row from the north-west corner (32 × 32
/// tiles): `.` level 0, `1` to `3` that many metres up, `a` and `b` a
/// ramp's tiles (3 and 7 decimetres) where levels 0 and 1 meet without a
/// step. A 2 m drop on its east side (rows 8 to 11), the ramp to the
/// south, the camp (16, 17) at its foot.
const MARCHES: [&str; 17] = [
    "................................",
    "................................",
    "..........11....................",
    "........111111..1...............",
    ".......11112111111..............",
    "......1111222221111.............",
    ".....111222222221111............",
    ".....11122233222211.............",
    "....1112223333222...............",
    "....11122233332222..............",
    "...1111222333322222.............",
    "...11112222333222...............",
    "....111122222221111.............",
    ".....11111222211111.............",
    "......111b11111111..............",
    ".......1bbb1.111................",
    "........aaa.....................",
];

/// The steps test's side, in tiles.
pub const MARCHES_SIDE: u32 = 32;

/// The heights of the steps test's tiles, in decimetres, row by row.
#[must_use]
pub fn marches_heights() -> Vec<i16> {
    let side = MARCHES_SIDE as usize;
    let mut out = vec![0; side * side];
    for (y, row) in MARCHES.iter().enumerate() {
        for (x, c) in row.bytes().enumerate() {
            out[y * side + x] = match c {
                b'1'..=b'9' => i16::from(c - b'0') * 10,
                b'a' => 3,
                b'b' => 7,
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
        TestWorld::Flat(n) => Map::filled(n, n, Terrain::Grass),
        TestWorld::Marches => {
            Map::filled(MARCHES_SIDE, MARCHES_SIDE, Terrain::Grass).with_heights(marches_heights())
        }
    };
    World::on_valley(calendar, content, map)
}
