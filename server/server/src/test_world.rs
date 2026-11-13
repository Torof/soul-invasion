//! Tiny worlds for visual tests (`--world`): a flat square of grass, and
//! the steps test's ridge. Ephemeral and never for play. Both
//! carry heights (the flat one all zero): the client knows a test world by
//! them and draws it with no fog.

use sim::Calendar;
use sim::map::{Map, Terrain};
use sim::{Content, World};

use crate::cli::TestWorld;

/// The steps test (eighth version, 13 November 2026): one ridge whose
/// crest runs straight across the screen, from the map's (0, 29), the
/// screen's left corner, to (29, 0), its right corner: the tiles where
/// `x + y` is [`MARCHES_CREST_LINE`]. The screen's horizontal is the map's
/// `x - y` and its vertical `x + y` (`Iso`), so the ridge's flanks rise
/// straight up the screen (its lower half) and straight down it (its upper
/// half). A tile's height falls evenly with its distance to the crest
/// line, from the crest's [`MARCHES_CREST`] decimetres to flat ground at
/// [`MARCHES_FALLOFF`] tiles on either side; flat grass remains near the
/// screen's top and bottom corners. A height that is not a whole number
/// of metres is a slope's tile (the client draws it flat, with the slope's
/// grass inclined the way it rises), so a slope's tile that would be one
/// is a decimetre lower. The camp lies on the flat ground near the
/// screen's bottom corner.
const MARCHES_CREST_LINE: i32 = 29;

/// The steps test's crest, in decimetres (before the decimetre off).
const MARCHES_CREST: i32 = 30;

/// Where the ground is flat again on either side, in tiles (of `x + y`)
/// from the crest line.
const MARCHES_FALLOFF: i32 = 11;

/// The steps test's side, in tiles.
pub const MARCHES_SIDE: u32 = 30;

/// The steps test's camp: on the flat ground near the screen's bottom
/// corner (`x + y` = 54, 25 from the crest line).
pub const MARCHES_CAMP: (u32, u32) = (27, 27);

/// The heights of the steps test's tiles, in decimetres, row by row.
#[must_use]
pub fn marches_heights() -> Vec<i16> {
    let side = MARCHES_SIDE as usize;
    let mut out = vec![0; side * side];
    for (i, h) in out.iter_mut().enumerate() {
        let x = i32::try_from(i % side).expect("small");
        let y = i32::try_from(i / side).expect("small");
        let away = (x + y - MARCHES_CREST_LINE).abs();
        let dm = (MARCHES_CREST * (MARCHES_FALLOFF - away) / MARCHES_FALLOFF).max(0);
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
