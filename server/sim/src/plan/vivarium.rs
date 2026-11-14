//! The vivarium's world (D41, lot 0 of the vivarium plan): a zone of the
//! atlas cut out and refined at one metre, the way `plan:<seed>` cuts
//! its look-test window ([`Window`]), but at a place chosen once and
//! for all by the project lead (14 November 2026): the continent of seed
//! 14, its **zone 2**, scouted by `vivarium_zones` over the continent's
//! wadeable river crossings. The zone was picked for what a band needs:
//! a river one can cross, a forest edge and a hill within reach of the
//! crossing, and two camp sites on opposite banks, out of each other's
//! sight. The numbers below are the scouting's, in the region's cells of
//! 64 m (`content/ai-reserve/review/vivarium/zones/mesures.tsv`, zone 2).
//!
//! The map is the atlas's at the tile: the heights (the hill), the biomes,
//! the river at the width its flow gives it, the brooks and the woods all
//! come from the continent and its region, so the vivarium walks on the
//! same ground `plan:14` would show around this place. Only the camp is
//! set, at camp A, where the first body is born (D34).

use super::region::REGION_CELL_M;
use super::{Continent, Region, Window};

/// The continent the vivarium lives on: seed 14, the lead's choice among
/// the scouted seeds (a warm continent, 38 % broadleaf forest, ten great
/// rivers).
pub const VIVARIUM_SEED: u64 = 14;

/// Continent cells a side of the region the zone is cut from: the zones'
/// scouting refined ten cells of 2 km around each crossing, and the zone's
/// cells are counted in that region.
pub const REGION_CELLS: usize = 10;

/// A region cell's side in tiles, the unit the zone is counted in.
const CELL: u32 = 64;
const _: () = assert!(REGION_CELL_M == 64, "the zone is counted in cells of 64 m");

/// A zone of the atlas: where a vivarium is cut, in the cells of the
/// region around its crossing (64 m a cell).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zone {
    /// The continent cell of the river crossing the region is centred on.
    pub cell: (i32, i32),
    /// The zone's north-west corner, in region cells.
    pub corner: (usize, usize),
    /// Its side, in region cells.
    pub side_cells: u32,
    /// The river crossing the scouting found wadeable (the atlas's reach).
    pub crossing: (usize, usize),
    /// The forest edge nearest the crossing.
    pub forest_edge: (usize, usize),
    /// The hill's top.
    pub hill: (usize, usize),
    /// Camp A, where the first body is born.
    pub camp_a: (usize, usize),
    /// Camp B, on the other bank.
    pub camp_b: (usize, usize),
}

/// Zone 2 of seed 14: atlas cell (383, 521), region origin (378, 516);
/// 34 cells of 64 m a side, 2,176 m (4.73 km²); a river of 57 m³/s (13 at
/// low water) crossing at region cell (176, 176); the forest edge 230 m
/// from the crossing, the hill 1,584 m away, 144 m of relief, its top at
/// 217 m; the camps 2,443 m apart on opposite banks; 16.9 °C, 1,006 mm of
/// rain a year; broadleaf forest 604 cells, grassland 390.
pub const ZONE_2: Zone = Zone {
    cell: (383, 521),
    corner: (156, 164),
    side_cells: 34,
    crossing: (176, 176),
    forest_edge: (178, 173),
    hill: (159, 194),
    camp_a: (186, 194),
    camp_b: (159, 167),
};

impl Zone {
    /// The zone's side in tiles (metres): a whole number of chunks of 32.
    #[must_use]
    pub const fn side(&self) -> u32 {
        self.side_cells * CELL
    }

    /// The tile at the centre of region cell `cell`, counted from the
    /// zone's north-west corner.
    ///
    /// # Panics
    ///
    /// When the cell lies outside the zone.
    #[must_use]
    pub fn tile(&self, cell: (usize, usize)) -> (u32, u32) {
        let at = |c: usize, corner: usize| {
            assert!(c >= corner, "a cell inside the zone");
            let k = u32::try_from(c - corner).expect("a small zone");
            assert!(k < self.side_cells, "a cell inside the zone");
            k * CELL + CELL / 2
        };
        (at(cell.0, self.corner.0), at(cell.1, self.corner.1))
    }

    /// The window of the zone on the continent of `seed`, its camp at
    /// camp A: the region of [`REGION_CELLS`] continent cells around the
    /// zone's cell, refined ([`Region::around`]), and the zone cut from it
    /// ([`Window::cut`]).
    #[must_use]
    pub fn window(&self, seed: u64) -> Window {
        let continent = Continent::new(seed);
        let region = Region::around(
            &continent,
            self.cell,
            REGION_CELLS * usize::try_from(super::region::PER_CELL).expect("positive"),
        );
        Window::cut(&continent, &region, self.corner, self.side()).with_camp(self.tile(self.camp_a))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use super::*;
    use crate::map::{CHUNK_SIZE, ChunkPos, Map, Pos, Terrain, VIVARIUM_GENERATOR};

    /// The zone's window, born once for the tests below: the continent of
    /// seed 14 takes some six seconds, the region 60 ms, the cut 4 ms
    /// (release, 14 November 2026).
    fn window() -> &'static Window {
        static WINDOW: OnceLock<Window> = OnceLock::new();
        WINDOW.get_or_init(|| ZONE_2.window(VIVARIUM_SEED))
    }

    /// The vivarium's map on the shared window.
    fn map() -> Map {
        Map::on_window_by(window().clone(), VIVARIUM_SEED, VIVARIUM_GENERATOR)
    }

    fn pos((x, y): (u32, u32)) -> Pos {
        Pos::new(
            i32::try_from(x).expect("small"),
            i32::try_from(y).expect("small"),
        )
    }

    /// The content's walking costs (`content/movement.ron`).
    fn walk(terrain: Terrain) -> Option<u32> {
        match terrain {
            Terrain::Grass => Some(10),
            Terrain::Forest | Terrain::Brook => Some(15),
            Terrain::Ford => Some(20),
            Terrain::Water | Terrain::Rock => None,
        }
    }

    #[test]
    fn zone_2_is_whole_chunks_and_cells() {
        assert_eq!(ZONE_2.side(), 2176);
        assert!(ZONE_2.side().is_multiple_of(CHUNK_SIZE));
        assert_eq!(ZONE_2.tile(ZONE_2.corner), (32, 32));
        assert_eq!(ZONE_2.tile(ZONE_2.camp_a), (30 * 64 + 32, 30 * 64 + 32));
        assert_eq!(ZONE_2.tile(ZONE_2.camp_b), (3 * 64 + 32, 3 * 64 + 32));
    }

    #[test]
    fn the_landmarks_lie_inside_the_zone() {
        for cell in [
            ZONE_2.crossing,
            ZONE_2.forest_edge,
            ZONE_2.hill,
            ZONE_2.camp_a,
            ZONE_2.camp_b,
        ] {
            let (x, y) = ZONE_2.tile(cell);
            assert!(x < ZONE_2.side() && y < ZONE_2.side());
        }
    }

    /// Born twice, the same bytes: the camp, and the terrain and heights
    /// of every fifth chunk each way, from a fresh birth
    /// ([`Map::vivarium`], the generator's own road) and from the shared
    /// window.
    #[test]
    fn born_twice_the_same_bytes() {
        let fresh = Map::vivarium();
        let again = map();
        assert_eq!(fresh.camp(), again.camp());
        assert_eq!(fresh.spec(), again.spec());
        let chunks = i32::try_from(ZONE_2.side() / CHUNK_SIZE).expect("small");
        for cy in (0..chunks).step_by(5) {
            for cx in (0..chunks).step_by(5) {
                let chunk = ChunkPos::new(cx, cy);
                assert_eq!(fresh.chunk(chunk), again.chunk(chunk), "chunk {chunk:?}");
                let heights = fresh.chunk_heights(chunk);
                assert!(heights.is_some(), "a height per tile");
                assert_eq!(heights, again.chunk_heights(chunk), "chunk {chunk:?}");
            }
        }
    }

    /// The zone at the tile is the one scouted on the atlas: the camps on
    /// dry grass at their cells, camp A the birthplace; the hill's top
    /// about 217 m and the crossing about 63 m above the sea (the scouting
    /// read the region's 64 m cells, the tile adds the hills and banks);
    /// the river at the crossing some 30 m wide, the width its 57 m³/s
    /// give it (`measure::width_cm`), read as the shortest wet run on the
    /// row and the column through the crossing (up to √2 wider than the
    /// river when it runs diagonally).
    #[test]
    fn the_zone_reads_as_scouted() {
        let map = map();
        let camp_a = ZONE_2.tile(ZONE_2.camp_a);
        assert_eq!(map.camp(), pos(camp_a), "born at camp A");
        for camp in [camp_a, ZONE_2.tile(ZONE_2.camp_b)] {
            assert_eq!(map.get(pos(camp)), Some(Terrain::Grass), "camp {camp:?}");
            assert_eq!(map.depth(pos(camp)), 0);
        }
        let metres =
            |cell| i64::from(map.elevation(pos(ZONE_2.tile(cell))).expect("a height")) / 10;
        assert!(
            (212..=222).contains(&metres(ZONE_2.hill)),
            "hill {} m",
            metres(ZONE_2.hill)
        );
        assert!(
            (58..=68).contains(&metres(ZONE_2.crossing)),
            "crossing {} m",
            metres(ZONE_2.crossing)
        );
        let (cx, cy) = ZONE_2.tile(ZONE_2.crossing);
        let river = |p: Pos| matches!(map.get(p), Some(Terrain::Water | Terrain::Ford));
        assert!(river(pos((cx, cy))), "the crossing is on the river");
        let run = |step: (i32, i32)| {
            let mut n = 1;
            for sign in [1, -1] {
                let mut p = pos((cx, cy));
                loop {
                    p = Pos::new(p.x + sign * step.0, p.y + sign * step.1);
                    if !river(p) {
                        break;
                    }
                    n += 1;
                }
            }
            n
        };
        let width = run((1, 0)).min(run((0, 1)));
        assert!(
            (28..=48).contains(&width),
            "the river is {width} tiles wide at the crossing"
        );
    }

    /// Camp A and camp B stand on opposite banks, and a path joins them:
    /// the atlas finds the river crossed dry-shod at the zone's cell (a
    /// bar reach spread four times its width, its riffle about 33 cm deep
    /// at 57 m³/s: `measure::passages`), and the window draws that
    /// crossing as a ford across the river, near the cell's centre, the
    /// one place the path can wade. The ford's tiles lie within a cell of
    /// the crossing's and the path steps on one of them.
    #[test]
    fn a_path_joins_the_camps_over_the_ford_at_the_crossing() {
        let map = map();
        let (a, b) = (map.camp(), pos(ZONE_2.tile(ZONE_2.camp_b)));
        let reach = i32::try_from(ZONE_2.side()).expect("small");
        let path = map
            .path_within(a, b, reach, u32::MAX, walk)
            .expect("a path from camp A to camp B");
        assert!(
            path.iter().any(|p| map.get(*p) == Some(Terrain::Ford)),
            "the path wades the ford"
        );
        let side = i64::from(ZONE_2.side());
        let fords: Vec<(i64, i64)> = (0..side)
            .flat_map(|y| (0..side).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                window()
                    .wet(x, y)
                    .is_some_and(|w| w.terrain == Terrain::Ford)
            })
            .collect();
        let (cx, cy) = ZONE_2.tile(ZONE_2.crossing);
        assert!(!fords.is_empty(), "a ford is drawn at the crossing");
        for &(x, y) in &fords {
            let away = (x - i64::from(cx)).abs().max((y - i64::from(cy)).abs());
            assert!(away <= 64, "a ford tile {away} tiles from the crossing");
        }
    }

    /// A camp asked for on the water moves to the nearest dry tile.
    #[test]
    fn a_camp_on_the_water_moves_to_the_bank() {
        let (cx, cy) = ZONE_2.tile(ZONE_2.crossing);
        let window = window().clone().with_camp((cx, cy));
        let (x, y) = window.camp();
        assert!(window.wet(i64::from(x), i64::from(y)).is_none());
        let away = (i64::from(x) - i64::from(cx))
            .abs()
            .max((i64::from(y) - i64::from(cy)).abs());
        assert!((1..=30).contains(&away), "{away} tiles from the crossing");
    }

    /// Writes the zone as an image (a binary PPM) for a look: the terrain,
    /// the water by depth, the ford, the camps (red), the crossing's cell
    /// (yellow) and the path from camp A to camp B (orange) when one
    /// exists. Run by hand: `SOUL_INVASION_DUMP=zone.ppm cargo test -p sim
    /// dump_vivarium -- --ignored`.
    #[test]
    #[ignore = "writes the vivarium's zone as an image for a look; run by hand"]
    fn dump_vivarium() {
        let path = std::env::var("SOUL_INVASION_DUMP").unwrap_or_else(|_| "vivarium.ppm".into());
        let map = map();
        let side = ZONE_2.side();
        let n = usize::try_from(side * side).expect("small");
        let mut rgb = vec![0_u8; 3 * n];
        for y in 0..side {
            for x in 0..side {
                let p = pos((x, y));
                let depth = u32::from(map.depth(p));
                let c: [u8; 3] = match map.get(p) {
                    Some(Terrain::Grass) => [190, 205, 140],
                    Some(Terrain::Forest) => [60, 110, 50],
                    Some(Terrain::Water) => [
                        30,
                        u8::try_from(110_u32.saturating_sub(depth * 12)).expect("small"),
                        u8::try_from(200_u32.saturating_sub(depth * 10)).expect("small"),
                    ],
                    Some(Terrain::Ford) => [140, 215, 225],
                    Some(Terrain::Brook) => [90, 150, 210],
                    Some(Terrain::Rock) | None => [120, 120, 120],
                };
                let i = 3 * usize::try_from(y * side + x).expect("small");
                rgb[i..i + 3].copy_from_slice(&c);
            }
        }
        let mut mark = |(cx, cy): (u32, u32), c: [u8; 3], r: u32| {
            for y in cy.saturating_sub(r)..(cy + r + 1).min(side) {
                for x in cx.saturating_sub(r)..(cx + r + 1).min(side) {
                    let i = 3 * usize::try_from(y * side + x).expect("small");
                    rgb[i..i + 3].copy_from_slice(&c);
                }
            }
        };
        let (a, b) = (map.camp(), pos(ZONE_2.tile(ZONE_2.camp_b)));
        let reach = i32::try_from(side).expect("small");
        let fords: Vec<(i64, i64)> = (0..side)
            .flat_map(|y| (0..side).map(move |x| (x, y)))
            .filter(|&(x, y)| map.get(pos((x, y))) == Some(Terrain::Ford))
            .map(|(x, y)| (i64::from(x), i64::from(y)))
            .collect();
        if !fords.is_empty() {
            let n = i64::try_from(fords.len()).expect("small");
            let (sx, sy) = fords.iter().fold((0, 0), |s, f| (s.0 + f.0, s.1 + f.1));
            let (lo_x, hi_x) = fords
                .iter()
                .fold((i64::MAX, i64::MIN), |r, f| (r.0.min(f.0), r.1.max(f.0)));
            let (lo_y, hi_y) = fords
                .iter()
                .fold((i64::MAX, i64::MIN), |r, f| (r.0.min(f.1), r.1.max(f.1)));
            eprintln!(
                "ford: {n} tiles, centre ({}, {}), x {lo_x}..={hi_x}, y {lo_y}..={hi_y}; camp A ({}, {})",
                sx / n,
                sy / n,
                a.x,
                a.y
            );
        }
        if let Some(path) = map.path_within(a, b, reach, u32::MAX, walk) {
            eprintln!("path from camp A to camp B: {} steps", path.len());
            for p in path {
                mark(
                    (
                        u32::try_from(p.x).expect("inside"),
                        u32::try_from(p.y).expect("inside"),
                    ),
                    [240, 130, 20],
                    1,
                );
            }
        }
        mark(ZONE_2.tile(ZONE_2.crossing), [250, 230, 40], 5);
        mark(ZONE_2.tile(ZONE_2.camp_a), [220, 30, 30], 5);
        mark(ZONE_2.tile(ZONE_2.camp_b), [220, 30, 30], 5);
        let mut out = format!("P6\n{side} {side}\n255\n").into_bytes();
        out.extend_from_slice(&rgb);
        std::fs::write(path, out).expect("the image");
    }
}
