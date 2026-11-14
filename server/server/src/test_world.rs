//! Tiny worlds for visual tests (`--world`): a flat square of grass, the
//! steps test's ridge, the mountain test's flank, the massif test that
//! holds the whole mountain, the shapes test, the sense test, and the
//! relief test that gathers the flank and the last two on one map
//! (`test:relief`, the one the sandbox uses). Ephemeral and never for
//! play. All carry heights (the flat one all zero): the client knows a
//! test world by them and draws it with no fog.

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

/// The relief sampled on a `side × side` window starting at `origin`,
/// in decimetres, row by row: the relief at each tile's middle.
fn mountain_window(origin: (f64, f64), side: u32) -> Vec<i16> {
    let (u0, v0) = origin;
    let side = side as usize;
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

/// The heights of the mountain test's tiles, in decimetres, row by row:
/// the relief at each tile's middle.
#[must_use]
pub fn mountain_heights() -> Vec<i16> {
    mountain_window(MOUNTAIN_ORIGIN, MOUNTAIN_SIDE)
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

/// The massif test (14 November 2026): the whole mountain the mountain
/// test cuts its window from, on [`MASSIF_SIDE`] tiles of grass. The
/// relief has no end along its crest: it is a ridge about 45 m high
/// running along the map's diagonal (`x - y` within about ±100 of the
/// crest's line, `x - y` ≈ 75 at the middle), 700 tiles wide at its foot
/// (where its height falls to the plain's 0 m), that swells to a 270 m
/// summit around `x + y` = 600 of the relief and falls back to the ridge
/// some 700 tiles away on either side. The map is therefore cut to
/// 1024 × 1024 tiles centred on the summit, the relief's (356, 282),
/// the map's (511, 511): the window starts at [`MASSIF_ORIGIN`]. Its
/// outer [`MASSIF_MARGIN`] tiles are flat at the plain's 0 m, and the
/// mountain fades to the plain over the [`MASSIF_FADE`] tiles inside
/// them (the ridge leaves by the map's upper left and lower right
/// corners, about 50 m high there). The ravines dig below the plain near
/// the foot: the lowest tile is about −26 m.
///
/// Along the crest the summit has a col on each side, about 200 m high
/// and 95 tiles away: the nearer, toward the next summit (232 m, at the
/// map's `x + y` = 839), is [`MASSIF_COL`]. The camp stands on the
/// gentlest tile within [`MASSIF_CAMP_REACH`] tiles of that col
/// ([`massif_camp`]): (439, 449), 202.7 m, level with its neighbours.
/// Test only: floats are fine here, the heights are rounded to
/// decimetres once.
pub const MASSIF_SIDE: u32 = 1024;

/// Where the massif test's window starts in the relief, in tiles: the
/// summit at the map's middle.
const MASSIF_ORIGIN: (f64, f64) = (-155.0, -229.0);

/// How many tiles at the massif test's edge are flat at the plain.
pub const MASSIF_MARGIN: u32 = 16;

/// Over how many tiles inside the margin the mountain fades to the plain.
pub const MASSIF_FADE: u32 = 8;

/// The col beside the massif's summit, on the map: the lowest point of
/// the crest between the summit and the next one, found by sampling.
pub const MASSIF_COL: (u32, u32) = (440, 449);

/// How far from the col, in tiles on each axis, the camp may stand.
pub const MASSIF_CAMP_REACH: i32 = 20;

/// The steepest rise the camp's tile may have between its neighbours two
/// tiles apart on an axis, in decimetres: 0.15 m a tile.
pub const MASSIF_CAMP_RISE_DM: i32 = 3;

/// The heights of the massif test's tiles, in decimetres, row by row:
/// the relief at each tile's middle, faded to the plain inside the flat
/// margin.
#[must_use]
pub fn massif_heights() -> Vec<i16> {
    let side = MASSIF_SIDE as usize;
    let last = side - 1;
    let (margin, fade) = (f64::from(MASSIF_MARGIN), f64::from(MASSIF_FADE));
    mountain_window(MASSIF_ORIGIN, MASSIF_SIDE)
        .into_iter()
        .enumerate()
        .map(|(i, h)| {
            let (x, y) = (i % side, i / side);
            let edge = x.min(y).min(last - x).min(last - y);
            let edge = f64::from(u32::try_from(edge).expect("small"));
            let weight = smooth(margin, margin + fade, edge);
            #[allow(clippy::cast_possible_truncation)]
            let dm = (weight * f64::from(h)).round() as i16;
            dm
        })
        .collect()
}

/// Where the massif test's camp stands: the gentlest tile (the smallest
/// rise between its neighbours one tile away on each axis, the nearest
/// to the col among equals) within [`MASSIF_CAMP_REACH`] tiles of
/// [`MASSIF_COL`].
#[must_use]
pub fn massif_camp(heights: &[i16]) -> (u32, u32) {
    let side = i32::try_from(MASSIF_SIDE).expect("small");
    let at =
        |x: i32, y: i32| i32::from(heights[usize::try_from(y * side + x).expect("on the map")]);
    let (cx, cy) = (
        i32::try_from(MASSIF_COL.0).expect("small"),
        i32::try_from(MASSIF_COL.1).expect("small"),
    );
    let reach = MASSIF_CAMP_REACH;
    let mut best = ((i32::MAX, i32::MAX), (cx, cy));
    for y in cy - reach..=cy + reach {
        for x in cx - reach..=cx + reach {
            let du = at(x + 1, y) - at(x - 1, y);
            let dv = at(x, y + 1) - at(x, y - 1);
            let rise = du * du + dv * dv;
            let away = (x - cx) * (x - cx) + (y - cy) * (y - cy);
            if (rise, away) < best.0 {
                best = ((rise, away), (x, y));
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

/// The sense test (14 November 2026): what makes a slope read as going
/// up rather than as a hole (the shapes test's 6 read as a vortex). Six
/// shapes on their own meadows, on [`SENS_SIDE`] tiles of grass flat at
/// [`SENS_BASE_M`], every crest running across the screen (along the
/// map's (1, −1)) so that one flank faces the camera (the front, rising
/// toward the screen's top, the map's (−1, −1)) and the other faces away.
/// The heights are smooth (no step above 12 dm between neighbours), the
/// crests sharp unless a flat top is said. Where each stands, in map
/// tiles (`x` range × `y` range of the tiles that are not flat, both
/// inclusive; the crest's tiles are those with the `x + y` given):
///
/// 1. The ridge with a shoulder: a crest 30 tiles long centred on
///    (50, 126), `x + y` = 175, a flat top 5 tiles wide, both flanks 25
///    tiles at [`SENS_SLOPE`] with a convex shoulder (the upper 4 tiles
///    ease from 0.5 to 0.125 a tile at the top) and a concave foot (the
///    lowest 3 tiles ease from 0.5 to 0 onto the meadow), 11 m high; the
///    ends rounded as half cones; x 13–86 × y 89–162.
/// 2. The same with a sharp top, to compare: plain planes at 0.5 meeting
///    at a line 12.5 m high, centred on (44, 44), `x + y` = 87; x 8–79 ×
///    y 8–79.
/// 3. Dissymmetric ridge A: the front flank short and steep (15 tiles at
///    0.8), the back long and gentle (35 tiles at 12/35, about 0.34),
///    sharp crest 12 m high centred on (96, 186), `x + y` = 281; x 55–126
///    × y 145–216.
/// 4. Dissymmetric ridge B: A turned 180 degrees (the front long and
///    gentle, the back short and steep), centred on (171, 82), `x + y` =
///    252; x 140–211 × y 51–122.
/// 5. The col: shape 1's profile on a crest 60 tiles long centred on
///    (193, 193), `x + y` = 385, with two summits 8 m over the crest's
///    line at the tiles (182, 203) and (203, 182) (15 tiles from the
///    crest's middle along it: a climb of 1 m a tile along the crest, the
///    top rounded over 3 tiles), the col between them at the crest's base
///    height; the flat top 5 wide on the summits only, narrowing to a
///    sharp line at the col; x 147–238 × y 147–238.
/// 6. The valley with a flat floor: shape 1's mirror dug into the meadow:
///    a floor 3 tiles wide 11 m below it (`x + y` = 175, centred on
///    (126, 50)), the flanks 25 tiles at 0.5 with a concave foot onto the
///    floor and a convex lip at the meadow; the ends rounded; x 90–161 ×
///    y 14–85.
///
/// No two shapes stand closer than 14 tiles, none closer than 8 to the
/// map's edge. The camp, [`SENS_CAMP`], is a flat tile at the map's
/// middle with 24 tiles of meadow around it. Test only: floats are fine
/// here, the heights are rounded to decimetres once.
pub const SENS_SIDE: u32 = 256;

/// The meadows' height, in metres: high enough for the valley to be dug.
pub const SENS_BASE_M: f64 = 50.0;

/// The flanks' slope, in metres a tile, unless said otherwise.
pub const SENS_SLOPE: f64 = 0.5;

/// A flank's length, in tiles, from the crest's flat top to the meadow.
const SENS_FLANK: f64 = 25.0;

/// The convex shoulder: over its tiles below the flat top the slope eases
/// from [`SENS_SHOULDER_TOP`] at the top to [`SENS_SLOPE`].
const SENS_SHOULDER: f64 = 4.0;
const SENS_SHOULDER_TOP: f64 = 0.125;

/// The concave foot: over its tiles the slope eases from [`SENS_SLOPE`]
/// to flat on the meadow.
const SENS_FOOT: f64 = 3.0;

/// What the shoulder and the foot drop, in metres (the mean slope over
/// their tiles).
const SENS_SHOULDER_DROP: f64 = SENS_SHOULDER * (SENS_SHOULDER_TOP + SENS_SLOPE) / 2.0;
const SENS_FOOT_RISE: f64 = SENS_SLOPE * SENS_FOOT / 2.0;

/// The shouldered ridge's height over the meadow, in metres: 11.
const SENS_RIDGE_M: f64 =
    SENS_SHOULDER_DROP + SENS_SLOPE * (SENS_FLANK - SENS_SHOULDER - SENS_FOOT) + SENS_FOOT_RISE;

/// Half the flat top's width, in tiles.
const SENS_TOP_HALF: f64 = 2.5;

/// The sharp ridge's height: plain planes at [`SENS_SLOPE`] over
/// [`SENS_FLANK`] tiles.
const SENS_SHARP_M: f64 = SENS_SLOPE * SENS_FLANK;

/// The dissymmetric ridges: the steep flank's slope and length, the
/// gentle flank's length; the crest is as high as the steep flank climbs.
const SENS_STEEP: f64 = 0.8;
const SENS_STEEP_LEN: f64 = 15.0;
const SENS_GENTLE_LEN: f64 = 35.0;
const SENS_DISSYMMETRIC_M: f64 = SENS_STEEP * SENS_STEEP_LEN;

/// The col's summits: how much they rise over the crest's line, in
/// metres, their climb along the crest in metres a tile, how many tiles
/// of their top are rounded (a parabola), and how far from the crest's
/// middle they stand along it (21 tiles of `x - y`, so that each summit
/// is a tile's middle).
const SENS_SUMMIT_M: f64 = 8.0;
const SENS_SUMMIT_SLOPE: f64 = 1.0;
const SENS_SUMMIT_TOP: f64 = 3.0;
const SENS_SUMMIT_ALONG: f64 = 21.0 * std::f64::consts::FRAC_1_SQRT_2;

/// Half the valley's floor's width, in tiles.
const SENS_FLOOR_HALF: f64 = 1.5;

/// Half a crest's length, in tiles, and half the col's.
const SENS_HALF: f64 = 15.0;
const SENS_COL_HALF: f64 = 30.0;

/// The sense test's camp.
pub const SENS_CAMP: (u32, u32) = (128, 128);

/// Where the shapes' crests are centred, in map coordinates, in the
/// order of the doc comment.
const SENS_CENTRES: [(f64, f64); 6] = [
    (50.0, 126.0),
    (44.0, 44.0),
    (96.0, 186.0),
    (171.0, 82.0),
    (193.0, 193.0),
    (126.0, 50.0),
];

/// What a shouldered flank has dropped `f` tiles below the flat top's
/// edge, in metres: the shoulder's ease, then [`SENS_SLOPE`].
fn sens_drop(f: f64) -> f64 {
    if f <= SENS_SHOULDER {
        SENS_SHOULDER_TOP * f + (SENS_SLOPE - SENS_SHOULDER_TOP) * f * f / (2.0 * SENS_SHOULDER)
    } else {
        SENS_SHOULDER_DROP + SENS_SLOPE * (f - SENS_SHOULDER)
    }
}

/// The concave foot: the height `h` a plain flank at [`SENS_SLOPE`] would
/// have, eased onto the meadow over the foot's tiles.
fn sens_foot(h: f64) -> f64 {
    if h >= SENS_FOOT_RISE {
        return h;
    }
    let g = ((SENS_FOOT_RISE - h) / SENS_SLOPE).min(SENS_FOOT);
    SENS_FOOT_RISE - SENS_SLOPE * g + SENS_SLOPE * g * g / (2.0 * SENS_FOOT)
}

/// Where `p` stands against a crest across the screen centred on
/// `centre`, of half length `half`: how far across it (positive toward
/// the screen's bottom, the map's (1, 1): the front), how far beyond its
/// ends along it, and where along it (clamped to the crest).
fn sens_frame(p: (f64, f64), centre: (f64, f64), half: f64) -> (f64, f64, f64) {
    let r = std::f64::consts::FRAC_1_SQRT_2;
    let across = (p.0 - centre.0 + p.1 - centre.1) * r;
    let along = (p.0 - centre.0 - p.1 + centre.1) * r;
    (
        across,
        (along.abs() - half).max(0.0),
        along.clamp(-half, half),
    )
}

/// Shapes 1 and 5: the shouldered profile, `crest` metres high with a
/// flat top of half width `top`, at `across` and `beyond` the crest.
fn sens_shouldered(across: f64, beyond: f64, top: f64, crest: f64) -> f64 {
    let d = beyond.hypot((across.abs() - top).max(0.0));
    sens_foot(crest - sens_drop(d))
}

/// Shape 1: the ridge with a shoulder.
fn sens_ridge(p: (f64, f64)) -> f64 {
    let (across, beyond, _) = sens_frame(p, SENS_CENTRES[0], SENS_HALF);
    sens_shouldered(across, beyond, SENS_TOP_HALF, SENS_RIDGE_M)
}

/// Shape 2: the sharp ridge.
fn sens_sharp(p: (f64, f64)) -> f64 {
    let (across, beyond, _) = sens_frame(p, SENS_CENTRES[1], SENS_HALF);
    (SENS_SHARP_M - SENS_SLOPE * beyond.hypot(across)).max(0.0)
}

/// Shapes 3 and 4: a dissymmetric ridge, its steep flank in front (shape
/// 3) or behind (shape 4); the ends fall at [`SENS_SLOPE`].
fn sens_dissymmetric(p: (f64, f64), centre: (f64, f64), steep_front: bool) -> f64 {
    let (across, beyond, _) = sens_frame(p, centre, SENS_HALF);
    let slope = if (across > 0.0) == steep_front {
        SENS_STEEP
    } else {
        SENS_DISSYMMETRIC_M / SENS_GENTLE_LEN
    };
    (SENS_DISSYMMETRIC_M - (slope * across).hypot(SENS_SLOPE * beyond)).max(0.0)
}

/// A summit's rise over the crest's line, `a` tiles from it along the
/// crest.
fn sens_summit(a: f64) -> f64 {
    let d = a.abs();
    if d < SENS_SUMMIT_TOP {
        SENS_SUMMIT_M - SENS_SUMMIT_SLOPE * d * d / (2.0 * SENS_SUMMIT_TOP)
    } else {
        let below_top = SENS_SUMMIT_M - SENS_SUMMIT_SLOPE * SENS_SUMMIT_TOP / 2.0;
        (below_top - SENS_SUMMIT_SLOPE * (d - SENS_SUMMIT_TOP)).max(0.0)
    }
}

/// Shape 5: the col, two summits on the shouldered profile; the flat top
/// narrows with the summit's rise, to nothing at the col.
fn sens_col(p: (f64, f64)) -> f64 {
    let (across, beyond, along) = sens_frame(p, SENS_CENTRES[4], SENS_COL_HALF);
    let rise = sens_summit(along - SENS_SUMMIT_ALONG) + sens_summit(along + SENS_SUMMIT_ALONG);
    let top = SENS_TOP_HALF * rise / SENS_SUMMIT_M;
    sens_shouldered(across, beyond, top, SENS_RIDGE_M + rise)
}

/// Shape 6: the valley with a flat floor, shape 1's mirror.
fn sens_valley(p: (f64, f64)) -> f64 {
    let (across, beyond, _) = sens_frame(p, SENS_CENTRES[5], SENS_HALF);
    -sens_shouldered(across, beyond, SENS_FLOOR_HALF, SENS_RIDGE_M)
}

/// Each shape's rise above the meadow at map point `p`, in metres (the
/// valley's below zero), in the doc comment's order.
fn sens_rises(p: (f64, f64)) -> [f64; 6] {
    [
        sens_ridge(p),
        sens_sharp(p),
        sens_dissymmetric(p, SENS_CENTRES[2], true),
        sens_dissymmetric(p, SENS_CENTRES[3], false),
        sens_col(p),
        sens_valley(p),
    ]
}

/// The heights of the sense test's tiles, in decimetres, row by row: the
/// shapes at each tile's middle (they never overlap).
#[must_use]
pub fn sens_heights() -> Vec<i16> {
    let side = SENS_SIDE as usize;
    (0..side * side)
        .map(|i| {
            let (x, y) = (
                f64::from(u32::try_from(i % side).expect("small")),
                f64::from(u32::try_from(i / side).expect("small")),
            );
            let rise: f64 = sens_rises((x + 0.5, y + 0.5)).iter().sum();
            let dm = ((SENS_BASE_M + rise) * 10.0).round();
            #[allow(clippy::cast_possible_truncation)]
            let dm = dm.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
            dm
        })
        .collect()
}

/// The relief test (14 November 2026): the shapes test, the sense test
/// and the mountain test gathered on one map of [`RELIEF_SIDE`] tiles of
/// grass, flat at [`RELIEF_BASE_DM`] everywhere else, so the project lead
/// has a single sandbox. This is the world the sandbox uses; the three
/// older worlds stay for the tests. Where each part lies, in map tiles
/// (`x` range × `y` range, the ends excluded):
///
/// - the shapes test, its heights as they are: x 0–256 × y 0–256;
/// - the sense test, its heights as they are: x 256–512 × y 0–256;
/// - the mountain test, in the window [`RELIEF_MOUNTAIN`]: x 32–224 ×
///   y 288–480. Its heights are shifted so that the mean of its window's
///   outer ring sits at the base (if a tile then fell below
///   [`RELIEF_FLOOR_DM`], the whole band is raised instead until none
///   does: its relief is kept either way), then faded to the base over
///   the [`RELIEF_FADE`] tiles inside its window's edge, so the window's
///   border is no cliff: its outer ring is the base.
///
/// The camp is the shapes test's, [`SHAPES_CAMP`].
pub const RELIEF_SIDE: u32 = 512;

/// The relief test's base height, in decimetres: the shapes and sense
/// tests' meadows, [`SHAPES_BASE_M`] and [`SENS_BASE_M`].
pub const RELIEF_BASE_DM: i16 = 500;

/// Below this height, in decimetres, no tile of the mountain band falls.
pub const RELIEF_FLOOR_DM: i16 = 200;

/// Where the mountain test's window starts on the relief test's map.
pub const RELIEF_MOUNTAIN: (u32, u32) = (32, 288);

/// Over how many tiles inside its window's edge the mountain fades to
/// the base.
pub const RELIEF_FADE: u32 = 8;

/// Copies `part`, a `side × side` block of heights, onto `heights` (a
/// `RELIEF_SIDE` wide map) at `origin`.
fn relief_lay(heights: &mut [i16], part: &[i16], side: u32, origin: (u32, u32)) {
    let (map, side) = (RELIEF_SIDE as usize, side as usize);
    let (x0, y0) = (origin.0 as usize, origin.1 as usize);
    for (row, tiles) in part.chunks_exact(side).enumerate() {
        let start = (y0 + row) * map + x0;
        heights[start..start + side].copy_from_slice(tiles);
    }
}

/// The mountain test's heights as the relief test lays them: shifted so
/// the mean of the outer ring sits at the base (raised further if any
/// tile fell below the floor), then faded to the base over the window's
/// edge. Row by row, `MOUNTAIN_SIDE` on a side.
#[must_use]
pub fn relief_mountain_heights() -> Vec<i16> {
    let heights = mountain_heights();
    let side = MOUNTAIN_SIDE as usize;
    let last = side - 1;
    let on_ring = |i: usize| {
        let (x, y) = (i % side, i / side);
        x == 0 || y == 0 || x == last || y == last
    };
    let ring: Vec<f64> = heights
        .iter()
        .enumerate()
        .filter(|&(i, _)| on_ring(i))
        .map(|(_, &h)| f64::from(h))
        .collect();
    let ring_mean = ring.iter().sum::<f64>() / f64::from(u32::try_from(ring.len()).expect("small"));
    let lowest = heights.iter().copied().min().unwrap_or(RELIEF_BASE_DM);
    let base = f64::from(RELIEF_BASE_DM);
    let shift = (base - ring_mean).max(f64::from(RELIEF_FLOOR_DM) - f64::from(lowest));
    let fade = f64::from(RELIEF_FADE);
    heights
        .iter()
        .enumerate()
        .map(|(i, &h)| {
            let (x, y) = (i % side, i / side);
            let edge = x.min(y).min(last - x).min(last - y);
            let edge = f64::from(u32::try_from(edge).expect("small"));
            let weight = smooth(0.0, fade, edge);
            let dm = (base + weight * (f64::from(h) + shift - base)).round();
            #[allow(clippy::cast_possible_truncation)]
            let dm = dm.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
            dm
        })
        .collect()
}

/// The heights of the relief test's tiles, in decimetres, row by row:
/// the three parts at their places, the base everywhere else.
#[must_use]
pub fn relief_heights() -> Vec<i16> {
    let side = RELIEF_SIDE as usize;
    let mut heights = vec![RELIEF_BASE_DM; side * side];
    relief_lay(&mut heights, &shapes_heights(), SHAPES_SIDE, (0, 0));
    relief_lay(&mut heights, &sens_heights(), SENS_SIDE, (SHAPES_SIDE, 0));
    relief_lay(
        &mut heights,
        &relief_mountain_heights(),
        MOUNTAIN_SIDE,
        RELIEF_MOUNTAIN,
    );
    heights
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
        TestWorld::Massif => {
            let heights = massif_heights();
            let (x, y) = massif_camp(&heights);
            Map::filled(MASSIF_SIDE, MASSIF_SIDE, Terrain::Grass)
                .with_heights(heights)
                .with_camp(x, y)
        }
        TestWorld::Shapes => {
            let (x, y) = SHAPES_CAMP;
            Map::filled(SHAPES_SIDE, SHAPES_SIDE, Terrain::Grass)
                .with_heights(shapes_heights())
                .with_camp(x, y)
        }
        TestWorld::Sense => {
            let (x, y) = SENS_CAMP;
            Map::filled(SENS_SIDE, SENS_SIDE, Terrain::Grass)
                .with_heights(sens_heights())
                .with_camp(x, y)
        }
        TestWorld::Relief => {
            let (x, y) = SHAPES_CAMP;
            Map::filled(RELIEF_SIDE, RELIEF_SIDE, Terrain::Grass)
                .with_heights(relief_heights())
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
    fn the_massif_holds_the_whole_mountain_on_a_flat_margin_and_the_camp_is_gentle() {
        let heights = massif_heights();
        let side = MASSIF_SIDE as usize;
        assert_eq!(heights.len(), side * side);
        let at = |x: usize, y: usize| i32::from(heights[y * side + x]);
        // The summit, about 270 m, stands at the map's middle; the ravines
        // dig a little below the plain at the foot.
        let (top, i) = heights
            .iter()
            .enumerate()
            .map(|(i, &h)| (h, i))
            .max()
            .expect("tiles");
        assert!((2650..=2750).contains(&top), "summit {top} dm");
        let (sx, sy) = (i % side, i / side);
        assert!(
            sx.abs_diff(side / 2) <= 2 && sy.abs_diff(side / 2) <= 2,
            "summit at ({sx}, {sy})"
        );
        let low = heights.iter().copied().min().expect("tiles");
        assert!((-300..0).contains(&low), "lowest {low} dm");
        // The margin is flat at the plain, and the ridge still crosses the
        // corners inside the fade.
        let margin = MASSIF_MARGIN as usize;
        let last = side - 1;
        for (i, &h) in heights.iter().enumerate() {
            let (x, y) = (i % side, i / side);
            if x.min(y).min(last - x).min(last - y) < margin {
                assert_eq!(h, 0, "margin tile ({x}, {y})");
            }
        }
        let inside = margin + MASSIF_FADE as usize;
        let ridge = |sum: usize| {
            (inside..side - inside)
                .filter_map(|x| sum.checked_sub(x).filter(|&y| y < side).map(|y| at(x, y)))
                .max()
                .expect("tiles")
        };
        assert!(ridge(2 * (inside + 20)) > 300);
        assert!(ridge(2 * (last - inside - 20)) > 300);
        // The col is well below the summit, and the camp near it lies level.
        let (cx, cy) = (MASSIF_COL.0 as usize, MASSIF_COL.1 as usize);
        assert!((1950..=2100).contains(&at(cx, cy)), "col {} dm", at(cx, cy));
        let (x, y) = massif_camp(&heights);
        let (x, y) = (x as usize, y as usize);
        assert!(
            x.abs_diff(cx) <= 20 && y.abs_diff(cy) <= 20,
            "camp ({x}, {y})"
        );
        let du = (at(x + 1, y) - at(x - 1, y)).abs();
        let dv = (at(x, y + 1) - at(x, y - 1)).abs();
        assert!(
            du <= MASSIF_CAMP_RISE_DM && dv <= MASSIF_CAMP_RISE_DM,
            "camp rises {du}, {dv}"
        );
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

    #[test]
    fn the_sense_test_spans_the_valleys_floor_to_the_summits_and_the_camp_is_flat() {
        let heights = sens_heights();
        let side = SENS_SIDE as usize;
        let at = |x: usize, y: usize| heights[y * side + x];
        // The valley's floor 11 m below the meadow, the col's summits 19 m
        // above it.
        assert_eq!(heights.iter().min(), Some(&390));
        assert_eq!(heights.iter().max(), Some(&690));
        // Shape 1's flat top: 5 tiles wide across the crest's line.
        for (x, y) in [
            (50, 125),
            (49, 126),
            (51, 126),
            (50, 127),
            (52, 126),
            (46, 126),
        ] {
            assert_eq!(at(x, y), 610, "({x}, {y})");
        }
        // Shape 2's sharp line.
        assert_eq!(at(44, 43), 625);
        assert!(at(44, 44) < 625 && at(43, 43) < 625);
        // Shapes 3 and 4: the steep flank in front, then behind.
        assert_eq!(at(96, 185), 620);
        assert!(at(100, 189) < at(92, 181), "A: the front is the steep one");
        assert_eq!(at(171, 81), 620);
        assert!(at(175, 85) > at(167, 77), "B: the back is the steep one");
        // The col: the summits' flat tops, the col at the crest's base height.
        for (x, y) in [(182, 203), (203, 182), (183, 204), (181, 202), (204, 183)] {
            assert_eq!(at(x, y), 690, "({x}, {y})");
        }
        assert_eq!(at(193, 192), 610);
        // The valley's floor: 3 tiles wide.
        for (x, y) in [(126, 49), (125, 50), (127, 50)] {
            assert_eq!(at(x, y), 390, "({x}, {y})");
        }
        assert!(at(128, 50) > 390);
        // No step above 12 dm between neighbours.
        for y in 0..side {
            for x in 1..side {
                assert!((at(x, y) - at(x - 1, y)).abs() <= 12, "({x}, {y})");
                assert!((at(y, x) - at(y, x - 1)).abs() <= 12, "({y}, {x})");
            }
        }
        // The camp and the 20 tiles around it are meadow.
        let (cx, cy) = (SENS_CAMP.0 as usize, SENS_CAMP.1 as usize);
        for y in cy - 20..=cy + 20 {
            for x in cx - 20..=cx + 20 {
                if x.abs_diff(cx).pow(2) + y.abs_diff(cy).pow(2) <= 400 {
                    assert_eq!(at(x, y), 500, "({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn the_sense_tests_shapes_keep_12_tiles_apart_and_4_from_the_edges() {
        let side = i32::try_from(SENS_SIDE).expect("small");
        let shaped = |x: i32, y: i32, shape: usize| {
            (0..side).contains(&x)
                && (0..side).contains(&y)
                && sens_rises((f64::from(x) + 0.5, f64::from(y) + 0.5))[shape].abs() > 0.04
        };
        // Each shape's tiles that are not flat and touch a flat one.
        let edges: Vec<Vec<(i32, i32)>> = (0..6)
            .map(|shape| {
                let mut edge = Vec::new();
                for y in 0..side {
                    for x in 0..side {
                        if shaped(x, y, shape) {
                            assert!((4..side - 4).contains(&x) && (4..side - 4).contains(&y));
                            if [(1, 0), (-1, 0), (0, 1), (0, -1)]
                                .iter()
                                .any(|(dx, dy)| !shaped(x + dx, y + dy, shape))
                            {
                                edge.push((x, y));
                            }
                        }
                    }
                }
                edge
            })
            .collect();
        for (i, a) in edges.iter().enumerate() {
            for b in &edges[i + 1..] {
                let nearest = a
                    .iter()
                    .flat_map(|p| {
                        b.iter()
                            .map(move |q| (p.0 - q.0).pow(2) + (p.1 - q.1).pow(2))
                    })
                    .min()
                    .expect("tiles");
                assert!(nearest >= 144, "shapes too close: {nearest}");
            }
        }
    }
    #[test]
    fn the_relief_test_lays_the_three_parts_on_one_flat_map_and_the_camp_is_flat() {
        let heights = relief_heights();
        let side = RELIEF_SIDE as usize;
        assert_eq!(heights.len(), side * side);
        let at = |x: usize, y: usize| heights[y * side + x];
        // Nothing below the floor; the highest tile is the mountain's.
        let laid = relief_mountain_heights();
        let (low, high) = (
            *heights.iter().min().expect("tiles"),
            *heights.iter().max().expect("tiles"),
        );
        assert!(low >= RELIEF_FLOOR_DM, "lowest {low}");
        assert_eq!(Some(&high), laid.iter().max(), "highest {high}");
        // The shapes test at (0, 0): its valley's floor and a crest.
        assert_eq!(at(35, 130), 200, "the shapes' valley floor");
        assert_eq!(at(135, 49), 800, "the shapes' crest across the screen");
        // The sense test at (256, 0): its col's floor and a summit.
        let sens = sens_heights();
        assert_eq!(
            at(256 + 193, 192),
            sens[192 * 256 + 193],
            "the sense test's col"
        );
        assert_eq!(at(256 + 193, 192), 610);
        assert_eq!(at(256 + 128, 50), sens[50 * 256 + 128]);
        assert!(at(256 + 128, 50) > 390);
        // The mountain at (32, 288): its relief, shifted and faded, at
        // its middle; its window's outer ring at the base.
        let (mx, my) = (RELIEF_MOUNTAIN.0 as usize, RELIEF_MOUNTAIN.1 as usize);
        let m = MOUNTAIN_SIDE as usize;
        assert_eq!(at(mx + 96, my + 96), laid[96 * m + 96]);
        let mountain = mountain_heights();
        let inner: Vec<i32> = (0..m * m)
            .filter(|i| {
                let (x, y) = (i % m, i / m);
                let edge = x.min(y).min(m - 1 - x).min(m - 1 - y);
                edge >= RELIEF_FADE as usize
            })
            .map(|i| i32::from(laid[i]) - i32::from(mountain[i]))
            .collect();
        let shift = inner[0];
        assert!(
            inner.iter().all(|&d| (d - shift).abs() <= 1),
            "the relief is kept"
        );
        assert!(
            (0..m * m).any(|i| laid[i] != RELIEF_BASE_DM),
            "the mountain is not flat"
        );
        for i in 0..m {
            for (x, y) in [(i, 0), (i, m - 1), (0, i), (m - 1, i)] {
                let h = at(mx + x, my + y);
                assert!((h - RELIEF_BASE_DM).abs() <= 3, "ring ({x}, {y}): {h}");
            }
        }
        // Outside the parts, the base.
        assert_eq!(at(300, 300), RELIEF_BASE_DM);
        assert_eq!(at(10, 270), RELIEF_BASE_DM);
        assert_eq!(at(10, 500), RELIEF_BASE_DM);
        // The camp (the shapes test's) and the 20 tiles around it are meadow.
        let (cx, cy) = (SHAPES_CAMP.0 as usize, SHAPES_CAMP.1 as usize);
        for y in cy - 20..=cy + 20 {
            for x in cx - 20..=cx + 20 {
                if x.abs_diff(cx).pow(2) + y.abs_diff(cy).pow(2) <= 400 {
                    assert_eq!(at(x, y), RELIEF_BASE_DM, "({x}, {y})");
                }
            }
        }
    }
}
