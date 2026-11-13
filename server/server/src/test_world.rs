//! Tiny worlds for visual tests (`--world`): a flat square of grass, and
//! the steps test's hill. Ephemeral and never for play. Both carry
//! heights (the flat one all zero): the client knows a test world by them
//! and draws it with no fog.

use sim::Calendar;
use sim::map::{Map, Terrain};
use sim::{Content, World};

use crate::cli::TestWorld;

/// The steps test's side, in tiles.
pub const MARCHES_SIDE: u32 = 32;

/// The steps test's hill (second version, 13 November 2026): a flat top
/// 3 m high (columns 7 to 11, rows 7 to 10) whose sides fall gently, 3 m
/// over some eight tiles; east of it a shoulder 2 m high (columns 13 to
/// 19, same rows) that ends in the one cliff, a 2 m drop between columns
/// 19 and 20, and falls to the south by a short steep ramp (2 m over three
/// tiles). The camp (16, 17) lies at the foot, south of the ramp.
/// Corners of the rectangles, inclusive: (west, north, east, south).
const TOP: (i32, i32, i32, i32) = (7, 7, 11, 10);
const TOP_DM: f64 = 30.0;
const SHOULDER: (i32, i32, i32, i32) = (13, 7, 19, 10);
const SHOULDER_DM: f64 = 20.0;
/// The gentle slopes and the ramp, decimetres lost a tile.
const SLOPE_DM: f64 = 4.0;
const RAMP_DM: f64 = 7.0;

/// The heights of the steps test's tiles, in decimetres, row by row.
#[must_use]
pub fn marches_heights() -> Vec<i16> {
    let side = MARCHES_SIDE.cast_signed();
    let mut out = Vec::with_capacity(MARCHES_SIDE as usize * MARCHES_SIDE as usize);
    for y in 0..side {
        for x in 0..side {
            let (w, n, e, s) = TOP;
            let dx = f64::from((w - x).max(x - e).max(0));
            let dy = f64::from((n - y).max(y - s).max(0));
            let top = TOP_DM - SLOPE_DM * dx.hypot(dy);
            // The shoulder: cut sheer on its east side, the ramp south.
            let (w, n, e, s) = SHOULDER;
            let shoulder = if x > e || x < w {
                0.0
            } else if y < n {
                SHOULDER_DM - SLOPE_DM * f64::from(n - y)
            } else if y > s {
                SHOULDER_DM - RAMP_DM * f64::from(y - s)
            } else {
                SHOULDER_DM
            };
            #[allow(clippy::cast_possible_truncation)]
            out.push(top.max(shoulder).max(0.0).round() as i16);
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
