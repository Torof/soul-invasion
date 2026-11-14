//! The relief look test's window of the atlas (`sim::plan::window`,
//! generator version 3): heights made the same from the same seed, with
//! no seam between chunks, and water lying flat in the low ground. On a
//! small continent and a window of 1 km, so the tests stay quick.

// A test window is 1,024 tiles a side: its coordinates fit every type.
#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::similar_names
)]

use sim::map::{CHUNK_SIZE, ChunkPos, Map, Pos};
use sim::plan::{Continent, Region, Window, region};

/// Tiles a side of the test windows: 1,024 m.
const SIDE: u32 = 1024;

fn window(seed: u64) -> Window {
    let continent = Continent::with(seed, 128, None);
    let site = region::site(&continent);
    let region = Region::around(&continent, site, 256);
    let corner = Window::choose(&region, SIDE);
    Window::cut(&continent, &region, corner, SIDE)
}

fn map(seed: u64) -> Map {
    Map::on_window(window(seed), seed)
}

/// Every height of the map, row by row, as its chunks carry them.
fn heights(map: &Map) -> Vec<i16> {
    let side = map.width() as usize;
    let c = CHUNK_SIZE as usize;
    let mut out = vec![0; side * side];
    for cy in 0..map.chunks_high() {
        for cx in 0..map.chunks_wide() {
            let chunk = ChunkPos::new(cx as i32, cy as i32);
            let h = map.chunk_heights(chunk).expect("a map with relief");
            for (k, v) in h.into_iter().enumerate() {
                out[(cy as usize * c + k / c) * side + cx as usize * c + k % c] = v;
            }
        }
    }
    out
}

#[test]
fn the_same_seed_gives_the_same_heights_and_waters() {
    let (a, b) = (window(3), window(3));
    assert_eq!(a.origin(), b.origin());
    assert_eq!(a.camp(), b.camp());
    assert_eq!(a.waters(), b.waters());
    for (x, y) in [(0, 0), (517, 3), (1023, 1023), (300, 777)] {
        assert_eq!(a.height_cm(x, y), b.height_cm(x, y));
    }
    let other = window(4);
    assert_ne!(
        (0..64)
            .map(|i| a.height_cm(i * 16, i * 16))
            .collect::<Vec<_>>(),
        (0..64)
            .map(|i| other.height_cm(i * 16, i * 16))
            .collect::<Vec<_>>(),
        "another seed, another ground"
    );
}

#[test]
fn heights_are_continuous_across_chunk_borders() {
    let map = map(3);
    let h = heights(&map);
    let side = map.width() as usize;
    let c = CHUNK_SIZE as usize;
    // The steepest step between two neighbours, inside chunks and across
    // their borders.
    let (mut inside, mut across) = (0, 0);
    for y in 0..side {
        for x in 0..side {
            for (dx, dy) in [(1, 0), (0, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx >= side || ny >= side {
                    continue;
                }
                let step = (i32::from(h[ny * side + nx]) - i32::from(h[y * side + x])).abs();
                if nx % c == 0 && dx == 1 || ny % c == 0 && dy == 1 {
                    across = across.max(step);
                } else {
                    inside = inside.max(step);
                }
            }
        }
    }
    assert!(
        across <= inside.max(2),
        "no seam: {across} dm across chunk borders, {inside} dm inside"
    );
    // And a chunk's heights are the tiles' own, whichever way read.
    for (x, y) in [(31, 31), (32, 32), (500, 64), (1023, 0)] {
        let pos = Pos::new(x, y);
        let chunk = pos.chunk();
        let k = (y.rem_euclid(32) * 32 + x.rem_euclid(32)) as usize;
        assert_eq!(
            map.chunk_heights(chunk).unwrap()[k],
            map.elevation(pos).unwrap()
        );
    }
}

/// Water lies flat in the low ground: along a water, the level falls at
/// most this much from a tile to the next (dm), and no dry tile beside
/// water lies more than this below it, so no water hangs on a slope's
/// side.
const FLAT_DM: i32 = 1;

#[test]
fn water_lies_flat_in_the_low_ground() {
    let map = map(3);
    let h = heights(&map);
    let side = map.width() as i32;
    let at = |x: i32, y: i32| i32::from(h[(y * side + x) as usize]);
    let wet = |x: i32, y: i32| map.depth(Pos::new(x, y)) > 0;
    let (mut water, mut steep, mut perched) = (0, Vec::new(), Vec::new());
    for y in 1..side - 1 {
        for x in 1..side - 1 {
            if !wet(x, y) {
                continue;
            }
            water += 1;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                let step = at(nx, ny) - at(x, y);
                if wet(nx, ny) && step.abs() > FLAT_DM {
                    steep.push((x, y, step));
                }
                if !wet(nx, ny) && step < -FLAT_DM {
                    perched.push((x, y, step));
                }
            }
        }
    }
    assert!(water > 1000, "the test window has water: {water} tiles");
    assert!(
        steep.is_empty(),
        "water on a slope: {:?}",
        &steep[..steep.len().min(10)]
    );
    assert!(
        perched.is_empty(),
        "water above its banks: {:?}",
        &perched[..perched.len().min(10)]
    );
}

#[test]
fn the_camp_is_dry_and_near_water_and_a_valley_has_no_relief() {
    let map = map(3);
    let camp = map.camp();
    assert_eq!(map.depth(camp), 0);
    assert!(map.is_passable(camp));
    let near =
        (-40..=40).any(|dy| (-40..=40).any(|dx| map.depth(Pos::new(camp.x + dx, camp.y + dy)) > 0));
    assert!(near, "water within 40 m of the camp {camp:?}");
    let valley = Map::valley(256, 256, 1);
    assert_eq!(valley.chunk_heights(ChunkPos::new(0, 0)), None);
    assert_eq!(valley.elevation(Pos::new(3, 3)), None);
}
