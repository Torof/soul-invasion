//! Tiny worlds for visual tests (`--world`): a flat square of grass, and
//! the steps test's ridge. Ephemeral and never for play. Both
//! carry heights (the flat one all zero): the client knows a test world by
//! them and draws it with no fog.

use sim::Calendar;
use sim::map::{Map, Terrain};
use sim::{Content, World};

use crate::cli::TestWorld;

/// The steps test (fifth version, 13 November 2026): one ridge across the
/// whole square. The screen's horizontal is the map's `x - y` (`Iso`), so
/// a tile's height depends on it alone: flat grass for the outer
/// [`MARCHES_FOOT`]`- 1` diagonals near the screen's left corner (0, 29)
/// and right corner (29, 0), then rising evenly to a crest of about
/// [`MARCHES_CREST`] decimetres on the diagonal `x = y`, which runs from
/// the screen's top corner to its bottom one, and falling the same way
/// past it. A height that is not a whole number of metres is a slope's
/// tile (the client draws it flat, with the slope's grass inclined the way
/// it rises), so a slope's tile that would be one is a decimetre lower.
/// The camp lies on the flat ground near the screen's left corner.
const MARCHES_FOOT: i32 = 25;

/// The steps test's crest, in decimetres (before the decimetre off).
const MARCHES_CREST: i32 = 30;

/// The steps test's side, in tiles.
pub const MARCHES_SIDE: u32 = 30;

/// The steps test's camp: on the flat ground near the screen's left
/// corner (`x - y` = -27).
pub const MARCHES_CAMP: (u32, u32) = (1, 28);

/// The heights of the steps test's tiles, in decimetres, row by row.
#[must_use]
pub fn marches_heights() -> Vec<i16> {
    let side = MARCHES_SIDE as usize;
    let mut out = vec![0; side * side];
    for (i, h) in out.iter_mut().enumerate() {
        let (x, y) = (i % side, i / side);
        let across = (i32::try_from(x).expect("small") - i32::try_from(y).expect("small")).abs();
        let dm = (MARCHES_CREST * (MARCHES_FOOT - across) / MARCHES_FOOT).max(0);
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
