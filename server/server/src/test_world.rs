//! Tiny worlds for visual tests (`--world`): a flat square of grass, the
//! steps test's ridge, the mountain test's flank and the shapes test.
//! Ephemeral and never for play. All
//! carry heights (the flat one all zero): the client knows a test world by
//! them and draws it with no fog.

use sim::Calendar;
use sim::map::{Map, Terrain};
use sim::{Content, World};

use crate::cli::TestWorld;

/// The steps test (eleventh version, 13 November 2026): one ridge whose
/// crest bends at the map's middle [`MARCHES_BEND`], (14.5, 14.5) in tiles.
/// From the screen's right corner, the map's (29, 0), to the middle it
/// runs straight across the screen along `x + y` = 29, as the eighth
/// version did; from the middle it turns down and to the left on screen,
/// along `x` = 14.5, to the middle of the diamond's lower left side, the
/// map's (14.5, 29). The screen's horizontal is the map's `x - y` and its
/// vertical `x + y` (`Iso`). A tile's height falls evenly with its
/// distance to that bent line, from the crest's [`MARCHES_CREST`]
/// decimetres to flat ground at [`MARCHES_FALLOFF`] (in the eighth
/// version's measure, a step of `x + y`, so the straight half is exactly
/// the eighth version's; only the inside of the bend, below the middle,
/// where the two arms' flanks meet, stands higher). A height that is not
/// a whole number of metres is a slope's tile (the client draws it flat,
/// with the slope's grass inclined the way it rises), so a slope's tile
/// that would be one is a decimetre lower. The camp lies on the flat
/// ground near the screen's bottom corner.
///
/// Distances are reckoned in half tiles (the bend and the end lie between
/// tiles) and in tenths of a step of `x + y` (a step is 1/√2 of a tile
/// across the straight half), all in integers.
const MARCHES_BEND: (i32, i32) = (29, 29);

/// The crest's two ends, in half tiles: the screen's right corner, and
/// the middle of the diamond's lower left side.
const MARCHES_ENDS: [(i32, i32); 2] = [(58, 0), (29, 58)];

/// The steps test's crest, in decimetres (before the decimetre off).
const MARCHES_CREST: i32 = 30;

/// Where the ground is flat again on either side, in steps of `x + y`
/// from the crest.
const MARCHES_FALLOFF: i32 = 11;

/// The steps test's side, in tiles.
pub const MARCHES_SIDE: u32 = 30;

/// The steps test's camp: on the flat ground near the screen's bottom
/// corner (`x + y` = 54, 25 steps from the straight half, 17.7 from the
/// bent one).
pub const MARCHES_CAMP: (u32, u32) = (27, 27);

/// The squared distance, in half tiles, from `p` to the segment `a`–`b`
/// (all in half tiles), as a fraction `num / den` (integers).
fn squared_to_segment(p: (i32, i32), a: (i32, i32), b: (i32, i32)) -> (i64, i64) {
    let (dx, dy) = (i64::from(b.0 - a.0), i64::from(b.1 - a.1));
    let (px, py) = (i64::from(p.0 - a.0), i64::from(p.1 - a.1));
    let len = dx * dx + dy * dy;
    let t = (px * dx + py * dy).clamp(0, len);
    // p - a - (t / len)(b - a), times len.
    let (ex, ey) = (px * len - t * dx, py * len - t * dy);
    (ex * ex + ey * ey, len * len)
}

/// A tile's distance to the bent crest, in tenths of a step of `x + y`.
fn marches_away(x: i32, y: i32) -> i32 {
    let p = (2 * x, 2 * y);
    let tenths = MARCHES_ENDS
        .iter()
        .map(|&end| {
            let (num, den) = squared_to_segment(p, MARCHES_BEND, end);
            // A step is 1/√2 of a tile, √2 half tiles: (10 d / √2)² = 50 d².
            (50 * num / den).isqrt()
        })
        .min()
        .expect("two arms");
    i32::try_from(tenths).expect("small")
}

/// The heights of the steps test's tiles, in decimetres, row by row.
#[must_use]
pub fn marches_heights() -> Vec<i16> {
    let side = MARCHES_SIDE as usize;
    let mut out = vec![0; side * side];
    for (i, h) in out.iter_mut().enumerate() {
        let x = i32::try_from(i % side).expect("small");
        let y = i32::try_from(i / side).expect("small");
        let falloff = 10 * MARCHES_FALLOFF;
        let dm = (MARCHES_CREST * (falloff - marches_away(x, y)) / falloff).max(0);
        let dm = if dm > 0 && dm % 10 == 0 { dm - 1 } else { dm };
        *h = i16::try_from(dm).expect("a few metres");
    }
    out
}

/// The mountain test (13 November 2026): a window of [`MOUNTAIN_SIDE`]
/// tiles cut at [`MOUNTAIN_ORIGIN`] from the relief the project lead
/// validated on images (`montagne-v3-etiree-b`): a crest that winds, its
/// flanks crossed by winding ravines with flat floors and unequal sides,
/// and benches every 3 m of height, the whole stretched
/// [`MOUNTAIN_STRETCH`] times. The map's `x` and `y` are the image's `u`
/// and `v`. Test only: floats are fine here, the heights are rounded to
/// decimetres once.
pub const MOUNTAIN_SIDE: u32 = 192;

/// Where the mountain test's window starts in the relief, in tiles.
const MOUNTAIN_ORIGIN: (f64, f64) = (320.0, 310.0);

/// How many times the relief's shapes are stretched.
const MOUNTAIN_STRETCH: f64 = 7.5;

fn smooth(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The crest's line wanders across the screen.
fn crest_offset(u: f64, v: f64) -> f64 {
    10.0 * ((u + v) / 34.0).sin() + 4.0 * ((u + v) / 11.0).sin()
}

/// Bumps, small crests and the ravines.
fn detail(u: f64, v: f64) -> f64 {
    let bumps = 0.9 * (u * 0.31 + v * 0.17).sin()
        + 0.7 * (u * 0.12 - v * 0.29 + 1.3).sin()
        + 0.5 * (u * 0.53 + v * 0.47 + 2.1).sin()
        + 0.35 * (u * 0.81 - v * 0.66 + 0.4).sin();
    // Ravines that wind, with a flat floor a few tiles wide and two
    // unequal sides.
    let phase =
        (u - v) * 0.21 + 0.8 + 1.3 * ((u + v) * 0.075).sin() + 0.6 * ((u + v) * 0.19 + 1.1).sin();
    let x = phase.sin();
    let floor = 1.0 - smooth(0.10, 0.18, x.abs());
    let side = if x > 0.0 {
        smooth(0.10, 0.55, x)
    } else {
        smooth(0.10, 0.30, -x)
    };
    let ravine = -1.4 * (floor + (1.0 - floor) * (1.0 - side)) * smooth(20.0, 45.0, u + v);
    bumps + ravine
}

/// The relief before the stretch, in metres.
fn mountain_base(u: f64, v: f64) -> f64 {
    let across = (u - v) - crest_offset(u, v);
    let along = u + v;
    let top = 26.0 * (-((along - 80.0) / 70.0).powi(2)).exp() + 6.0;
    let h = top * (1.0 - across.abs() / 52.0).clamp(0.0, 1.0).powf(1.25);
    let h = h + 2.2 * detail(u, v) * (h / 4.0).clamp(0.0, 1.0);
    // Benches every 3 m of height.
    let period = 3.0;
    let amplitude = 0.62 * period / std::f64::consts::TAU;
    h - amplitude * (std::f64::consts::TAU * h / period).sin()
}

/// The relief's height at map point (`u`, `v`), in metres.
fn mountain_at(u: f64, v: f64) -> f64 {
    MOUNTAIN_STRETCH * mountain_base(u / MOUNTAIN_STRETCH, v / MOUNTAIN_STRETCH)
}

/// The heights of the mountain test's tiles, in decimetres, row by row:
/// the relief at each tile's middle.
#[must_use]
pub fn mountain_heights() -> Vec<i16> {
    let side = MOUNTAIN_SIDE as usize;
    let (u0, v0) = MOUNTAIN_ORIGIN;
    (0..side * side)
        .map(|i| {
            let (x, y) = (
                f64::from(u32::try_from(i % side).expect("small")),
                f64::from(u32::try_from(i / side).expect("small")),
            );
            let dm = (mountain_at(u0 + x + 0.5, v0 + y + 0.5) * 10.0).round();
            #[allow(clippy::cast_possible_truncation)]
            let dm = dm.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
            dm
        })
        .collect()
}

/// Where the mountain test's camp stands: the gentlest tile (the smallest
/// rise to its four neighbours at two tiles) within 16 tiles of the
/// screen's middle left, the map's (48, 144).
#[must_use]
pub fn mountain_camp(heights: &[i16]) -> (u32, u32) {
    let side = i32::try_from(MOUNTAIN_SIDE).expect("small");
    let at =
        |x: i32, y: i32| i32::from(heights[usize::try_from(y * side + x).expect("on the map")]);
    let mut best = (i32::MAX, (48, 144));
    for y in 128..=160 {
        for x in 32..=64 {
            let du = at(x + 2, y) - at(x - 2, y);
            let dv = at(x, y + 2) - at(x, y - 2);
            let rise = du * du + dv * dv;
            if rise < best.0 {
                best = (rise, (x, y));
            }
        }
    }
    let (x, y) = best.1;
    (
        u32::try_from(x).expect("on the map"),
        u32::try_from(y).expect("on the map"),
    )
}

/// The shapes test (14 November 2026): simple shapes the slope rendering
/// is judged on, each on its own meadow, on [`SHAPES_SIDE`] tiles of grass
/// flat at [`SHAPES_BASE_M`]. The screen's horizontal is the map's
/// `x - y` and its vertical `x + y` (`Iso`). Every side is a plane rising
/// [`SHAPES_RISE_M`] over 25 tiles ([`SHAPES_SLOPE`] metres a tile) unless
/// said otherwise, and crests are left sharp (the client decides how they
/// look). Where each stands, in map tiles (`x` range × `y` range of the
/// tiles that are not flat, both inclusive):
///
/// 1. A crest across the screen (along the map's (1, −1)): 30 tiles of
///    straight crest centred on (135, 50), its tiles `x + y` = 184, the
///    ends rounded as half cones; x 99–170 × y 14–85.
/// 2. A crest up the screen (along the map's (1, 1)): centred on
///    (114, 160), its tiles `x - y` = −46; x 78–149 × y 124–195.
/// 3. A crest along a screen diagonal (the map's `x` axis): row `y` = 36,
///    from `x` 35 to 65; x 10–89 × y 12–60.
/// 4. A valley along the map's `y` axis, dug into the meadow: a floor of
///    two tiles (`x` 35 and 36) at 20 m, from `y` 115 to 145, two sides
///    rising 30 m to the meadow, the ends rounded; x 10–61 × y 89–170.
/// 5. A cone 30 m high, slope 1.1, its top rounded over 3 tiles, centred
///    on the tile (220, 40); x 192–248 × y 12–68.
/// 6. A plane rising toward the screen's upper right (the map's −`y`):
///    0.4 a tile for 20 tiles from its foot at `y` = 161, then 1.2 for 20
///    tiles, to 31.4 m at `y` = 122, 12 tiles wide at the top; its back
///    and ends fall back at 1.2; x 163–228 × y 95–161. Beside it below, a
///    crest bent by 60 degrees, its bend rounded (radius 6.6 tiles) and
///    pointing at the screen's bottom: on its outer side the rising
///    direction turns 60 degrees across 20 tiles halfway up, 7 at the
///    top and 33 at the foot (its inner side folds); x 160–236 × y
///    175–251. The two are 13 tiles apart, the least between any two
///    shapes.
///
/// The camp, [`SHAPES_CAMP`], is a flat tile near the map's middle with
/// 26 tiles of meadow around it. The map is larger than the mountain
/// test's: seven shapes 50 to 77 tiles across, 12 tiles of meadow apart,
/// do not fit in 192. Test only: floats are fine here, the
/// heights are rounded to decimetres once.
pub const SHAPES_SIDE: u32 = 256;

/// The meadows' height, in metres: high enough for the valley to be dug.
pub const SHAPES_BASE_M: f64 = 50.0;

/// How much the crests, the cone and the bent crest rise, and the valley
/// sinks, in metres.
pub const SHAPES_RISE_M: f64 = 30.0;

/// The sides' slope, in metres a tile.
pub const SHAPES_SLOPE: f64 = 1.2;

/// Half a straight crest's (and the valley's floor's) length, in tiles.
const SHAPES_HALF: f64 = 15.0;

/// The shapes test's camp.
pub const SHAPES_CAMP: (u32, u32) = (136, 110);

/// The distance from `p` to the segment of half length `half` centred on
/// `centre` along the unit vector `dir`.
fn to_segment(p: (f64, f64), centre: (f64, f64), dir: (f64, f64), half: f64) -> f64 {
    let (px, py) = (p.0 - centre.0, p.1 - centre.1);
    let t = (px * dir.0 + py * dir.1).clamp(-half, half);
    (px - t * dir.0).hypot(py - t * dir.1)
}

/// A roof's rise at `d` tiles from its crest, in metres.
fn roof(d: f64) -> f64 {
    (SHAPES_RISE_M - SHAPES_SLOPE * d).max(0.0)
}

/// Shape 5: the cone, slope 1.1, its top a paraboloid over 3 tiles.
fn shapes_cone(p: (f64, f64)) -> f64 {
    let (slope, top) = (1.1, 3.0);
    let d = (p.0 - 220.5).hypot(p.1 - 40.5);
    let h = if d < top {
        SHAPES_RISE_M - slope * d * d / (2.0 * top)
    } else {
        SHAPES_RISE_M - slope * top / 2.0 - slope * (d - top)
    };
    h.max(0.0)
}

/// Shape 6, the plane whose slope changes: rising toward −`y` from its
/// foot, the least of the two-slope plane, its back and its two ends.
fn shapes_steepening(p: (f64, f64)) -> f64 {
    let t = 162.0 - p.1;
    if t <= 0.0 {
        return 0.0;
    }
    let front = if t < 20.0 {
        0.4 * t
    } else {
        8.0 + SHAPES_SLOPE * (t - 20.0)
    };
    let top = 32.0;
    let back = SHAPES_SLOPE * (40.0 - t) + top;
    let ends = SHAPES_SLOPE * (6.0 - (p.0 - 196.0).abs()) + top;
    front.min(back).min(ends).max(0.0)
}

/// Shape 6, the plane that turns: a crest bent by 60 degrees, the bend an
/// arc of radius 6.6 tiles around (201, 216) facing the screen's bottom
/// (the map's (1, 1)), each arm 18 tiles long.
fn shapes_turning(p: (f64, f64)) -> f64 {
    let (ox, oy) = (201.0, 216.0);
    let radius = 6.6;
    let arm = 18.0;
    let (px, py) = (p.0 - ox, p.1 - oy);
    // The arc runs from 15 to 75 degrees (the map's angles, `y` down).
    let angle = py.atan2(px).to_degrees();
    let mut d = f64::INFINITY;
    if (15.0..=75.0).contains(&angle) {
        d = (px.hypot(py) - radius).abs();
    }
    for end in [15.0_f64, 75.0] {
        let (c, s) = (end.to_radians().cos(), end.to_radians().sin());
        // Each arm leaves the arc's end along its tangent, up the screen.
        let dir = if end < 45.0 { (s, -c) } else { (-s, c) };
        let start = (ox + radius * c, oy + radius * s);
        let mid = (start.0 + dir.0 * arm / 2.0, start.1 + dir.1 * arm / 2.0);
        d = d.min(to_segment(p, mid, dir, arm / 2.0));
    }
    roof(d)
}

/// The shapes test's rise above the meadow at map point `p`, in metres
/// (the valley's is below zero). The shapes never overlap.
fn shapes_rise(p: (f64, f64)) -> f64 {
    let r = std::f64::consts::FRAC_1_SQRT_2;
    let across = roof(to_segment(p, (135.0, 50.0), (r, -r), SHAPES_HALF));
    let up = roof(to_segment(p, (114.0, 160.0), (r, r), SHAPES_HALF));
    let diagonal = roof(to_segment(p, (50.0, 36.5), (1.0, 0.0), SHAPES_HALF));
    let valley = -roof((to_segment(p, (36.0, 130.0), (0.0, 1.0), SHAPES_HALF) - 1.0).max(0.0));
    across + up + diagonal + valley + shapes_cone(p) + shapes_steepening(p) + shapes_turning(p)
}

/// The heights of the shapes test's tiles, in decimetres, row by row: the
/// shapes at each tile's middle.
#[must_use]
pub fn shapes_heights() -> Vec<i16> {
    let side = SHAPES_SIDE as usize;
    (0..side * side)
        .map(|i| {
            let (x, y) = (
                f64::from(u32::try_from(i % side).expect("small")),
                f64::from(u32::try_from(i / side).expect("small")),
            );
            let dm = ((SHAPES_BASE_M + shapes_rise((x + 0.5, y + 0.5))) * 10.0).round();
            #[allow(clippy::cast_possible_truncation)]
            let dm = dm.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
            dm
        })
        .collect()
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
        TestWorld::Mountain => {
            let heights = mountain_heights();
            let (x, y) = mountain_camp(&heights);
            Map::filled(MOUNTAIN_SIDE, MOUNTAIN_SIDE, Terrain::Grass)
                .with_heights(heights)
                .with_camp(x, y)
        }
        TestWorld::Shapes => {
            let (x, y) = SHAPES_CAMP;
            Map::filled(SHAPES_SIDE, SHAPES_SIDE, Terrain::Grass)
                .with_heights(shapes_heights())
                .with_camp(x, y)
        }
    };
    World::on_valley(calendar, content, map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32) -> i16 {
        let side = i32::try_from(MARCHES_SIDE).expect("small");
        marches_heights()[usize::try_from(y * side + x).expect("on the map")]
    }

    #[test]
    fn the_straight_half_is_the_eighth_versions_and_the_camp_is_flat() {
        for x in 0..30_i32 {
            for y in 0..30_i32 {
                // Above the bend's level, right of the screen's middle.
                if x > y && x + y <= 29 {
                    let away = (x + y - 29).abs();
                    let dm = (30 * (11 - away) / 11).max(0);
                    let dm = if dm > 0 && dm % 10 == 0 { dm - 1 } else { dm };
                    assert_eq!(i32::from(at(x, y)), dm, "({x}, {y})");
                }
            }
        }
        let (x, y) = MARCHES_CAMP;
        assert_eq!((x, y), (27, 27));
        assert_eq!(at(27, 27), 0);
    }

    #[test]
    fn the_crest_bends_down_to_the_lower_left_sides_middle() {
        // Along x = 14.5, both sides of it stand near the crest.
        assert!(at(14, 28) >= 27 && at(15, 28) >= 27);
        // The left corner is flat now.
        assert_eq!(at(0, 29), 0);
    }

    #[test]
    fn the_shapes_span_the_valley_to_the_crests_and_the_camp_is_flat() {
        let heights = shapes_heights();
        let side = SHAPES_SIDE as usize;
        let at = |x: usize, y: usize| heights[y * side + x];
        // The valley's floor at 20 m, the steepening plane's top at 31.4 m
        // above the meadow, the crests and the cone at 30.
        assert_eq!(heights.iter().min(), Some(&200));
        assert_eq!(heights.iter().max(), Some(&814));
        assert_eq!((at(35, 130), at(36, 130)), (200, 200));
        assert_eq!(at(135, 49), 800, "crest across the screen");
        assert_eq!(at(114, 160), 800, "crest up the screen");
        assert_eq!(at(50, 36), 800, "crest along the map's x");
        assert_eq!(at(220, 40), 800, "the cone's top");
        // No side steeper than 1.2 m a tile.
        for y in 0..side {
            for x in 1..side {
                assert!((at(x, y) - at(x - 1, y)).abs() <= 12, "({x}, {y})");
                assert!((at(y, x) - at(y, x - 1)).abs() <= 12, "({y}, {x})");
            }
        }
        // The camp and the 20 tiles around it are meadow.
        let (cx, cy) = (SHAPES_CAMP.0 as usize, SHAPES_CAMP.1 as usize);
        for y in cy - 20..=cy + 20 {
            for x in cx - 20..=cx + 20 {
                if x.abs_diff(cx).pow(2) + y.abs_diff(cy).pow(2) <= 400 {
                    assert_eq!(at(x, y), 500, "({x}, {y})");
                }
            }
        }
    }
}
