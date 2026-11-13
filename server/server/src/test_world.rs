//! Tiny worlds for visual tests (`--world`): a flat square of grass, and
//! the steps test's ridge. Ephemeral and never for play. Both
//! carry heights (the flat one all zero): the client knows a test world by
//! them and draws it with no fog.

use sim::Calendar;
use sim::map::{Map, Terrain};
use sim::{Content, World};

use crate::cli::TestWorld;

/// The steps test (seventh version, 13 November 2026): one ridge whose
/// crest is a diagonal on screen, from the map's (0, 0), the screen's top
/// corner, to [`MARCHES_CREST_END`] on the screen's lower-right side
/// (`x` = 29), two thirds of the way to its right corner (29, 0). The
/// screen's horizontal is the map's `x - y` and its vertical `x + y`
/// (`Iso`). A tile's height falls evenly with its distance to the crest
/// line, from the crest's [`MARCHES_CREST`] decimetres to flat ground at
/// [`MARCHES_FALLOFF_LEFT`] tiles on the side of the screen's left and
/// bottom corners and at [`MARCHES_FALLOFF_RIGHT`] tiles on the narrower
/// side of its right corner, which is so steeper and shorter; flat grass
/// remains near both corners. A height that is not a whole number of
/// metres is a slope's tile (the client draws it flat, with the slope's
/// grass inclined the way it rises), so a slope's tile that would be one
/// is a decimetre lower. The camp lies on the flat ground near the
/// screen's left corner.
const MARCHES_CREST_END: (i32, i32) = (29, 10);

/// The steps test's crest, in decimetres (before the decimetre off).
const MARCHES_CREST: i32 = 30;

/// Where the ground is flat again on the side of the screen's left and
/// bottom corners, in tiles from the crest line.
const MARCHES_FALLOFF_LEFT: i32 = 18;

/// Where the ground is flat again on the side of the screen's right
/// corner, in tiles from the crest line.
const MARCHES_FALLOFF_RIGHT: i32 = 6;

/// The crest line's length in tiles, about the square root of
/// `29² + 10²` (30.7), rounded so distances stay whole numbers.
const MARCHES_CREST_LENGTH: i32 = 31;

/// The steps test's side, in tiles.
pub const MARCHES_SIDE: u32 = 30;

/// The steps test's camp: on the flat ground near the screen's left
/// corner (about 26 tiles from the crest line).
pub const MARCHES_CAMP: (u32, u32) = (1, 28);

/// The heights of the steps test's tiles, in decimetres, row by row.
#[must_use]
pub fn marches_heights() -> Vec<i16> {
    let side = MARCHES_SIDE as usize;
    let (ex, ey) = MARCHES_CREST_END;
    let mut out = vec![0; side * side];
    for (i, h) in out.iter_mut().enumerate() {
        let x = i32::try_from(i % side).expect("small");
        let y = i32::try_from(i / side).expect("small");
        // The signed distance to the crest line, times its length:
        // positive on the side of the screen's right corner.
        let across = ey * x - ex * y;
        let falloff = if across > 0 {
            MARCHES_FALLOFF_RIGHT
        } else {
            MARCHES_FALLOFF_LEFT
        } * MARCHES_CREST_LENGTH;
        let dm = (MARCHES_CREST * (falloff - across.abs()) / falloff).max(0);
        let dm = if dm > 0 && dm % 10 == 0 { dm - 1 } else { dm };
        *h = i16::try_from(dm).expect("a few metres");
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
            let (x, y) = MARCHES_CAMP;
            Map::filled(MARCHES_SIDE, MARCHES_SIDE, Terrain::Grass)
                .with_heights(marches_heights())
                .with_camp(x, y)
        }
    };
    World::on_valley(calendar, content, map)
}
