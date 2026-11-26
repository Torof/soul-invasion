//! The soil's water (generator version 5; lot 5, step 1, 26 November
//! 2026): how wet the ground of a tile is, in thousandths, from 0 (dust)
//! to 1000 (in the water). What the plants read to grow and to bear
//! ([`crate::flora`], [`crate::crops`]); what the animals, the dead wood
//! and the fibres will read later.
//!
//! Worked out at the tile, the same every time, from three things:
//!
//! - **the ground's own water**, from the atlas: the yearly rain and the
//!   water table's depth of the continent's cells around the tile,
//!   interpolated between their centres, 2 km apart
//!   ([`crate::plan::Window::atlas_water`]). Rain wets the soil from
//!   [`RAIN_DRY_MM`] to [`RAIN_WET_MM`]; a water table near the surface
//!   wets it from below, as much as rain at the surface, nothing past
//!   [`TABLE_ROOTS_CM`]. Either is enough: the two add as chances do (a
//!   dry country over a shallow table is wet ground);
//! - **the water beside it**: within [`REACH`] tiles of a river, a ford or
//!   a brook, the soil is as wet as today's flora had it, falling 110
//!   thousandths a tile from the water;
//! - **the shade of the wood**, which keeps the soil moist, and a small
//!   smooth noise, so the ground is not the same from one tile to the
//!   next.
//!
//! Only the atlas's cells around the tile are read, never an array over
//! the map: the cost follows the tiles asked, never the world's size. On
//! maps of older generators there is no soil water ([`soil_water`] is
//! `None`) and the flora keeps its noise, so their worlds keep their
//! plants and their food tile for tile.
//!
//! Every number here is the generator's, not content: a world is born
//! with its soil. What the plants make of it is content (`flora.ron`'s
//! moisture ranges, `crops.ron`'s `soil_yield_permille`). All are
//! placeholders.

use crate::r#gen::noise;
use crate::r#gen::scatter::{Frame, purpose};
use crate::map::{CHUNK_SIZE, ChunkPos, Map, Pos, Terrain};

/// How far the water beside a tile wets it, in tiles.
pub const REACH: i32 = 9;
/// The yearly rain under which the soil is dry, and over which it is as
/// wet as rain alone makes it, in millimetres.
pub const RAIN_DRY_MM: i64 = 250;
/// See [`RAIN_DRY_MM`].
pub const RAIN_WET_MM: i64 = 1250;
/// The water table's depth from which it no longer wets the soil, in
/// centimetres: past the deepest roots of the trees here.
pub const TABLE_ROOTS_CM: i64 = 800;
/// The soil on the driest ground, and how much the ground's own water
/// adds to it at most, in thousandths.
const SOIL_FLOOR: i64 = 50;
const SOIL_SPAN: i64 = 800;
/// How far the noise moves it either way, in thousandths, and over how
/// many tiles it changes.
const TEXTURE: i64 = 60;
const TEXTURE_CELL: u32 = 32;

/// The ground's own water from the atlas's rain (`rain_mm`, a year) and
/// water table (`table_cm` deep), in thousandths: 0 on dry ground over a
/// deep table, 1000 under heavy rain or over a table at the surface.
#[must_use]
pub fn ground_water(rain_mm: i64, table_cm: i64) -> i64 {
    let rain = ((rain_mm - RAIN_DRY_MM) * 1000 / (RAIN_WET_MM - RAIN_DRY_MM)).clamp(0, 1000);
    let table = (1000 - table_cm * 1000 / TABLE_ROOTS_CM).clamp(0, 1000);
    1000 - (1000 - rain) * (1000 - table) / 1000
}

/// The soil's water of a tile, in thousandths: its ground's own water
/// (`ground`, [`ground_water`]), `distance` tiles from the nearest water
/// (0 in it, past [`REACH`] for none near), under a wood of density
/// `trees` (0 to 100), with `texture` the noise there (0 to 1000).
#[must_use]
pub fn of(ground: i64, distance: i32, trees: u8, texture: u32) -> u16 {
    let own = SOIL_FLOOR
        + ground.clamp(0, 1000) * SOIL_SPAN / 1000
        + (i64::from(texture) - 500) * TEXTURE / 500
        + i64::from(trees) * 10 / 6;
    let beside = if distance <= 0 {
        1000
    } else {
        960 - i64::from(distance) * 110
    };
    u16::try_from(own.max(beside).clamp(0, 1000)).expect("at most a thousand")
}

/// The noise of the soil's water at the map's tile `(x, y)`.
fn texture(map: &Map, seed: u64, x: i32, y: i32) -> u32 {
    let (wx, wy) = Frame::at(map.origin()).tile(x, y);
    noise(seed ^ purpose::WETNESS, TEXTURE_CELL, wx, wy)
}

/// Is `pos` water that wets the soil around it: a river, a lake, a ford,
/// a brook.
fn wet(map: &Map, pos: Pos) -> bool {
    matches!(
        map.get(pos),
        Some(Terrain::Water | Terrain::Ford | Terrain::Brook)
    )
}

/// The soil's water at `pos` on `map` of `seed`, in thousandths; `None`
/// on a map without it ([`Map::has_soil_water`]) and off the map. Looks
/// for the nearest water ring by ring, up to [`REACH`] tiles: who asks
/// for many tiles together asks [`soil_in`].
#[must_use]
pub fn soil_water(map: &Map, seed: u64, pos: Pos) -> Option<u16> {
    let (rain, table) = map.atlas_water(pos)?;
    let distance = (0..=REACH)
        .find(|&d| {
            (-d..=d).any(|i| {
                [
                    Pos::new(pos.x + i, pos.y - d),
                    Pos::new(pos.x + i, pos.y + d),
                    Pos::new(pos.x - d, pos.y + i),
                    Pos::new(pos.x + d, pos.y + i),
                ]
                .into_iter()
                .any(|p| wet(map, p))
            })
        })
        .unwrap_or(REACH + 1);
    Some(of(
        ground_water(rain, table),
        distance,
        map.trees(pos),
        texture(map, seed, pos.x, pos.y),
    ))
}

/// The soil's water of every tile of `chunk`, row by row, in
/// thousandths ([`soil_water`] for each, the water looked for once for
/// the whole chunk); `None` on a map without it.
#[must_use]
pub fn soil_in(map: &Map, seed: u64, chunk: ChunkPos) -> Option<Vec<u16>> {
    let size = i32::try_from(CHUNK_SIZE).expect("small chunk");
    let lo = Pos::new(chunk.x * size, chunk.y * size);
    map.atlas_water(lo)?;
    let distances = distances(map, lo, size);
    Some(
        (0..size * size)
            .map(|i| {
                let pos = Pos::new(lo.x + i % size, lo.y + i / size);
                let (rain, table) = map.atlas_water(pos).unwrap_or((0, TABLE_ROOTS_CM));
                of(
                    ground_water(rain, table),
                    distances[usize::try_from(i).expect("inside")],
                    map.trees(pos),
                    texture(map, seed, pos.x, pos.y),
                )
            })
            .collect(),
    )
}

/// The soil's water of a tile of a square whose water is known
/// ([`distances`]): what the flora reads for the tiles it works over.
pub(crate) fn of_tile(map: &Map, seed: u64, pos: Pos, distance: i32) -> Option<u16> {
    let (rain, table) = map.atlas_water(pos)?;
    Some(of(
        ground_water(rain, table),
        distance,
        map.trees(pos),
        texture(map, seed, pos.x, pos.y),
    ))
}

/// The distance to the nearest water (a river, a lake, a ford, a brook),
/// in tiles (the larger of the two offsets), of every tile of the `span`
/// × `span` square from `lo`, up to `REACH + 1`: two sweeps over the
/// square and a margin of `REACH` around it, instead of a search per
/// tile.
pub(crate) fn distances(map: &Map, lo: Pos, span: i32) -> Vec<i32> {
    let far = REACH + 1;
    let wide = span + 2 * REACH;
    let at = |x: i32, y: i32| usize::try_from(y * wide + x).expect("inside");
    let mut d: Vec<i32> = (0..wide * wide)
        .map(|i| {
            let p = Pos::new(lo.x - REACH + i % wide, lo.y - REACH + i / wide);
            if wet(map, p) { 0 } else { far }
        })
        .collect();
    for y in 0..wide {
        for x in 0..wide {
            let mut v = d[at(x, y)];
            for (dx, dy) in [(-1, 0), (-1, -1), (0, -1), (1, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if (0..wide).contains(&nx) && (0..wide).contains(&ny) {
                    v = v.min(d[at(nx, ny)] + 1);
                }
            }
            d[at(x, y)] = v;
        }
    }
    for y in (0..wide).rev() {
        for x in (0..wide).rev() {
            let mut v = d[at(x, y)];
            for (dx, dy) in [(1, 0), (1, 1), (0, 1), (-1, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if (0..wide).contains(&nx) && (0..wide).contains(&ny) {
                    v = v.min(d[at(nx, ny)] + 1);
                }
            }
            d[at(x, y)] = v;
        }
    }
    (0..span * span)
        .map(|i| d[at(i % span + REACH, i / span + REACH)].min(far))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rain_and_a_shallow_table_wet_the_ground() {
        // Dry: little rain over a deep table.
        assert_eq!(ground_water(RAIN_DRY_MM, TABLE_ROOTS_CM), 0);
        // Either rain or a table at the surface is enough.
        assert_eq!(ground_water(RAIN_WET_MM, 3000), 1000);
        assert_eq!(ground_water(0, 0), 1000);
        // More rain, a shallower table: never drier.
        let mut last = 0;
        for rain in (0..1600).step_by(100) {
            let g = ground_water(rain, 600);
            assert!(g >= last, "rain {rain}: {g} after {last}");
            last = g;
        }
        assert!(ground_water(500, 200) > ground_water(500, 600));
    }

    #[test]
    fn the_water_beside_wets_the_soil_as_the_flora_had_it() {
        // In the water, and falling away from it.
        assert_eq!(of(0, 0, 0, 500), 1000);
        assert_eq!(of(0, 1, 0, 500), 850);
        assert!(of(0, 5, 0, 500) > of(0, 6, 0, 500));
        // Past the reach: the ground's own water alone.
        assert_eq!(of(0, REACH + 1, 0, 500), 50);
        assert_eq!(of(1000, REACH + 1, 0, 500), 850);
        // The wood's shade and the noise, a little either way.
        assert!(of(500, REACH + 1, 60, 500) > of(500, REACH + 1, 0, 500));
        assert_eq!(of(500, REACH + 1, 0, 0), 390);
        assert_eq!(of(500, REACH + 1, 0, 1000), 510);
    }

    #[test]
    fn no_soil_water_on_the_older_generators() {
        let map = Map::valley(256, 256, 7);
        assert_eq!(soil_water(&map, 7, Pos::new(10, 10)), None);
        assert_eq!(soil_in(&map, 7, ChunkPos::new(0, 0)), None);
    }
}
