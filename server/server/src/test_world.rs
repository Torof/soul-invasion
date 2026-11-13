//! Tiny worlds for visual tests (`--world`): a flat square of grass, and
//! the steps test's ridge. Ephemeral and never for play. Both
//! carry heights (the flat one all zero): the client knows a test world by
//! them and draws it with no fog.

use sim::Calendar;
use sim::map::{Map, Terrain};
use sim::{Content, World};

use crate::cli::TestWorld;

/// The steps test (sixth version, 13 November 2026): one ridge across the
/// whole square, its crest slanted and off-centre. The screen's horizontal
/// is the map's `x - y` and its vertical `x + y` (`Iso`). Flat grass lies
/// on the outer [`MARCHES_FOOT`]` - 1` diagonals near the screen's left
/// corner (0, 29) and right corner (29, 0) (`|x - y| >= MARCHES_FOOT`).
/// The crest runs from `x - y` = [`MARCHES_CREST_TOP`] at the screen's top
/// corner (`x + y` = 0) to `x - y` = [`MARCHES_CREST_BOTTOM`] at its bottom
/// one (`x + y` = 58), so it leans, and lower down the climbing flank is
/// much wider than the falling one. A tile rises evenly from the left foot
/// to the crest's [`MARCHES_CREST`] decimetres and falls evenly from it to
/// the right foot, so the falling flank is shorter and steeper. A height
/// that is not a whole number of metres is a slope's tile (the client draws
/// it flat, with the slope's grass inclined the way it rises), so a slope's
/// tile that would be one is a decimetre lower. The camp lies on the flat
/// ground near the screen's left corner.
const MARCHES_FOOT: i32 = 25;

/// The steps test's crest, in decimetres (before the decimetre off).
const MARCHES_CREST: i32 = 30;

/// The crest's `x - y` at the screen's top corner.
const MARCHES_CREST_TOP: i32 = 4;

/// The crest's `x - y` at the screen's bottom corner.
const MARCHES_CREST_BOTTOM: i32 = 14;

/// The steps test's side, in tiles.
pub const MARCHES_SIDE: u32 = 30;

/// The steps test's camp: on the flat ground near the screen's left
/// corner (`x - y` = -27).
pub const MARCHES_CAMP: (u32, u32) = (1, 28);

/// The heights of the steps test's tiles, in decimetres, row by row.
#[must_use]
pub fn marches_heights() -> Vec<i16> {
    let side = MARCHES_SIDE as usize;
    let span = 2 * (i32::try_from(side).expect("small") - 1);
    let mut out = vec![0; side * side];
    for (i, h) in out.iter_mut().enumerate() {
        let x = i32::try_from(i % side).expect("small");
        let y = i32::try_from(i / side).expect("small");
        // Everything below is scaled by `span` to keep the crest's slant exact.
        let across = (x - y) * span;
        let crest = MARCHES_CREST_TOP * span + (MARCHES_CREST_BOTTOM - MARCHES_CREST_TOP) * (x + y);
        let foot = MARCHES_FOOT * span;
        let dm = if across <= crest {
            MARCHES_CREST * (across + foot) / (crest + foot)
        } else {
            MARCHES_CREST * (foot - across) / (foot - crest)
        }
        .max(0);
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
