//! The relief from a declared skeleton (D43, D44; the mountain plan,
//! `content/ai-reserve/review/montagne-algo/ALGORITHME.md`, §1 and §2):
//! a mountain is read by its skeleton, never by noise. A [`Ridge`] is a
//! polyline of [`Knot`]s (summits and cols), each with a crest height and
//! a flat shelf; between two knots the crest eases from one height to the
//! other. The flanks fall from the ridge to a declared valley floor with
//! one profile (plan §2.B): a **convex shoulder** under the shelf, where
//! the slope grows from nothing to the plane's, a **plane** middle whose
//! slope is a class held over the whole versant, and a **concave foot**
//! that eases onto the floor. The two versants of a ridge have different
//! lengths, the longer by [`LONG_FLANK_PCT`]. A ridge may carry a
//! **rim**: its back versant steps down [`RIM_DM`] at the crest, the edge
//! that occludes.
//!
//! Integer only (D24): positions in [`SUB`]-ths of a tile, heights in
//! decimetres, slopes in hundredths of a decimetre a tile. Today the
//! module holds only what the plan's first three tiny tests need
//! ([`trials`]): no spurs, no ravines, no strata, no noise, and the whole
//! patch is worked out at once (a patch is at most a few hundred tiles;
//! the chunk-by-chunk form of D39 comes with the generator). The
//! invariants of the plan's step E are measured by [`check`].

/// Sub-tile units a tile: positions and distances are reckoned in
/// sixteenths of a tile, fine enough for a crest to pass between tile
/// centres and coarse enough that every square below fits an `i64` with
/// room to spare.
pub const SUB: i64 = 16;

/// The share of a flank's length taken by the convex shoulder under the
/// shelf (plan §2.B: "convexe sur l'épaule, 15 % du flanc").
pub const SHOULDER_PCT: i64 = 15;

/// The share of a flank's length taken by the concave foot (plan §2.B:
/// "concave au pied, 10 %").
pub const FOOT_PCT: i64 = 10;

/// What a flank drops, in thousandths of "its plane's slope times its
/// length": the shoulder and the foot each give away half of their share
/// (their slope runs from nothing to the plane's, so their mean is half).
const DROP_SHARE_PER_MILLE: i64 = 1000 - SHOULDER_PCT * 5 - FOOT_PCT * 5;

/// The longer versant of a ridge, as a percentage of the shorter (plan
/// §2.B: "deux versants d'une même crête ont des longueurs différentes,
/// 0,7–1,3"). Near the range's end for a legible dissymmetry, short of
/// it so the first trials fit a 256-tile map with their 25-tile flanks.
pub const LONG_FLANK_PCT: i64 = 125;

/// The rim's step at the crest, in decimetres: a metre, enough to read as
/// an edge over the flank's own drop (half a metre a tile), under the
/// plan's ceiling of 1.7 m a tile (step E).
pub const RIM_DM: i32 = 10;

/// A slope class (plan §2.B: "les pentes sont par classes, tenues sur des
/// bandes d'au moins 6 cases"). A flank's plane holds one class over its
/// whole length. A flat flank would never reach the floor, so flat is not
/// a class here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slope {
    /// 0.25 m a metre, a spur's or a meadow's.
    Gentle,
    /// 0.5 m a metre, the plan's flank for the first tests.
    Medium,
    /// 0.7 m a metre, just under step E's ceiling of 0.75 on turf.
    Steep,
    /// 1 m a metre, past the ceiling: declared rock only.
    Rock,
}

impl Slope {
    /// The class's rise, in hundredths of a decimetre a tile (a tile is a
    /// metre, so 500 is 0.5 m a metre).
    #[must_use]
    pub const fn per_cent_dm(self) -> i64 {
        match self {
            Self::Gentle => 250,
            Self::Medium => 500,
            Self::Steep => 700,
            Self::Rock => 1000,
        }
    }
}

/// One of a ridge's two versants, named by the sign of the cross product
/// of the ridge's direction with the vector from the ridge to the tile:
/// `Plus` where `dir.x * dy - dir.y * dx > 0`. For a ridge running along
/// the screen's horizontal from the map's upper right to its lower left
/// (direction `(1, -1)`), `Plus` is the side of larger `x + y`: the near
/// one, toward the screen's bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The cross product is positive (or the tile lies on the line).
    Plus,
    /// The cross product is negative.
    Minus,
}

/// A point of a ridge's crest: a summit or a col.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Knot {
    /// The tile whose centre the crest passes through.
    pub at: (i32, i32),
    /// The crest's height there, in decimetres above the sea.
    pub crest_dm: i32,
    /// The flat shelf at the knot, in tiles: a disc of half this radius
    /// around the knot stays at the crest's height, the summit's plateau
    /// (plan §2.A: "chaque sommet porte un replat de 3–8 cases"); between
    /// two knots the crest carries a band as wide as the narrower shelf,
    /// so two summits of the same shelf make a flat-topped ridge. Zero
    /// for a bare crest.
    pub shelf_tiles: u32,
}

/// A ridge: a polyline of knots and the two versants falling from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ridge {
    /// At least two knots, in order along the crest.
    pub knots: Vec<Knot>,
    /// The plane's slope on the shorter versant; the longer one is
    /// gentler by [`LONG_FLANK_PCT`] so that both reach the same floor.
    pub slope: Slope,
    /// Which versant is the longer.
    pub long_side: Side,
    /// A rim, if any: the versant that steps down [`RIM_DM`] at the crest.
    pub rim: Option<Side>,
}

/// A patch of relief: a flat valley floor and the ridges standing on it.
/// Where ridges overlap the higher one shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skeleton {
    /// The valley floor, in decimetres above the sea.
    pub floor_dm: i32,
    /// The ridges.
    pub ridges: Vec<Ridge>,
}

/// A tile's centre in sub-tile units.
const fn centre(tile: (i32, i32)) -> (i64, i64) {
    (tile.0 as i64 * SUB + SUB / 2, tile.1 as i64 * SUB + SUB / 2)
}

/// Where a point stands against one segment of a ridge: how far across
/// (sub-tiles), how far along from the first knot (sub-tiles, clamped to
/// the segment), the segment's length, the side, and how much of a side
/// it is: in thousandths, 1,000 beside the segment, falling to 0 on the
/// segment's own line past either end. Past the ends the flanks of the
/// two sides meet around the knot, and whatever differs between them
/// (their length, the rim) is blended by this weight, so the cap has no
/// seam along the crest's extension.
struct Standing {
    across: i64,
    along: i64,
    len: i64,
    side: Side,
    sidedness: i64,
}

fn stand(point: (i64, i64), from: (i64, i64), to: (i64, i64)) -> Standing {
    let dir = (to.0 - from.0, to.1 - from.1);
    let off = (point.0 - from.0, point.1 - from.1);
    let len2 = dir.0 * dir.0 + dir.1 * dir.1;
    let len = len2.isqrt().max(1);
    let dot = off.0 * dir.0 + off.1 * dir.1;
    let cross = dir.0 * off.1 - dir.1 * off.0;
    let (nearest, along, sidedness) = if dot <= 0 {
        (from, 0, sidedness(cross, off, len))
    } else if dot >= len2 {
        let past = (point.0 - to.0, point.1 - to.1);
        (to, len, sidedness(cross, past, len))
    } else {
        (
            (from.0 + dir.0 * dot / len2, from.1 + dir.1 * dot / len2),
            dot / len,
            1000,
        )
    };
    let gap = (point.0 - nearest.0, point.1 - nearest.1);
    let across = (gap.0 * gap.0 + gap.1 * gap.1).isqrt();
    let side = if cross >= 0 { Side::Plus } else { Side::Minus };
    Standing {
        across,
        along,
        len,
        side,
        sidedness,
    }
}

/// How much a point past a knot belongs to its side, in thousandths: the
/// sine of its angle to the crest's line, `|cross| / (|from the knot| ·
/// len)`.
fn sidedness(cross: i64, from_knot: (i64, i64), len: i64) -> i64 {
    let dist = (from_knot.0 * from_knot.0 + from_knot.1 * from_knot.1).isqrt();
    if dist == 0 {
        return 1000;
    }
    (cross.abs() * 1000 / (dist * len)).min(1000)
}

/// The crest between two knots at `along` sub-tiles from the first: its
/// height in decimetres and the band's half-width across, in sub-tiles.
/// Each knot's crest holds over half its shelf along the ridge (its
/// plateau's reach); between the plateaus the height eases from one to
/// the other (smoothstep, so a col is flat at its bottom and a summit at
/// its top). The band is the narrower knot's shelf: a summit's plateau
/// ends on the arête with a lip of a few decimetres (what the flanks
/// drop within the plateau's radius), the edge that tells the plateau
/// from the slope. A shelf that narrowed gradually along the ridge was
/// tried and dropped: its narrowing is itself a slope along the ridge,
/// as steep as the flanks four tiles out, and the crest no longer read
/// as one (step E's 120° test).
fn crest_at(k0: &Knot, k1: &Knot, along: i64, len: i64) -> (i32, i64) {
    let (h0, h1) = (
        i64::from(k0.shelf_tiles) * SUB / 2,
        i64::from(k1.shelf_tiles) * SUB / 2,
    );
    let band = h0.min(h1);
    if along <= h0 {
        return (k0.crest_dm, band);
    }
    if along >= len - h1 {
        return (k1.crest_dm, band);
    }
    let span = (len - h0 - h1).max(1);
    let t = along - h0;
    let rise = i64::from(k1.crest_dm - k0.crest_dm);
    let eased = (rise * t * t * (3 * span - 2 * t)).div_euclid(span * span * span);
    let crest = i32::try_from(i64::from(k0.crest_dm) + eased).expect("a height in decimetres");
    (crest, band)
}

/// A versant's drop under the crest at `d` sub-tiles beyond the shelf's
/// edge, in hundredths of a decimetre, for a flank of `relief`
/// decimetres whose plane rises `slope` hundredths of a decimetre a tile
/// (plan §2.B's profile). Never decreasing with `d`; the whole relief
/// from the flank's end on. Worked out a hundred times finer than the
/// heights and rounded once by the caller, so that the foot's length
/// moves smoothly with the relief along a ridge instead of jumping with
/// each decimetre of it.
fn drop(relief: i64, slope: i64, d: i64) -> i64 {
    // The flank's length: what the plane would take to drop the relief,
    // stretched for what the shoulder and the foot give away.
    let len = (relief * 100 * 1000 * SUB / (DROP_SHARE_PER_MILLE * slope)).max(1);
    let shoulder = (len * SHOULDER_PCT / 100).max(1);
    let plane_end = len - len * FOOT_PCT / 100;
    // A length in sub-tiles times a slope in hundredths of a decimetre a
    // tile, divided by `SUB`, is in hundredths of a decimetre.
    if d <= shoulder {
        return slope * d * d / (2 * shoulder * SUB);
    }
    let shoulder_drop = slope * shoulder / (2 * SUB);
    if d <= plane_end {
        return shoulder_drop + slope * (d - shoulder) / SUB;
    }
    // The foot: a parabola that starts at the plane's slope and lands
    // flat on the floor. Its length is whatever drops the rest of the
    // relief at that start, so the roundings above are absorbed by a
    // foot a little longer or shorter, never by a step at the floor.
    let plane_drop = shoulder_drop + slope * (plane_end - shoulder) / SUB;
    let rest = (relief * 100 - plane_drop).max(0);
    let foot = (2 * rest * SUB / slope).max(1);
    let left = plane_end + foot - d;
    if left <= 0 {
        return relief * 100;
    }
    relief * 100 - slope * left * left / (2 * foot * SUB)
}

/// The flank's length in tiles for a relief and a slope: where the floor
/// is reached, measured from the shelf's edge.
#[must_use]
pub fn flank_tiles(relief_dm: i32, slope: Slope) -> i64 {
    i64::from(relief_dm) * 100 * 1000 / (DROP_SHARE_PER_MILLE * slope.per_cent_dm())
}

impl Ridge {
    /// The plane's slope where a point stands, in hundredths of a
    /// decimetre a tile: the side's beside the ridge, blended toward the
    /// two sides' mean past a knot (see [`Standing`]).
    fn slope_at(&self, standing: &Standing) -> i64 {
        let class = self.slope.per_cent_dm();
        let long = class * 100 / LONG_FLANK_PCT;
        let (own, other) = if standing.side == self.long_side {
            (long, class)
        } else {
            (class, long)
        };
        let mean = i64::midpoint(own, other);
        mean + (own - mean) * standing.sidedness / 1000
    }

    /// The rim's step where a point stands, in decimetres: the whole of
    /// [`RIM_DM`] beside the ridge on the rim's side, fading past a knot.
    fn rim_at(&self, standing: &Standing) -> i64 {
        if self.rim == Some(standing.side) {
            i64::from(RIM_DM) * standing.sidedness / 1000
        } else {
            0
        }
    }

    /// The nearest segment's standing for a point, with the crest's
    /// height and the band's half-width there.
    fn nearest(&self, point: (i64, i64)) -> (Standing, i32, i64) {
        let mut best: Option<(Standing, i32, i64)> = None;
        for pair in self.knots.windows(2) {
            let standing = stand(point, centre(pair[0].at), centre(pair[1].at));
            if best
                .as_ref()
                .is_none_or(|(held, _, _)| standing.across < held.across)
            {
                let (crest, band) = crest_at(&pair[0], &pair[1], standing.along, standing.len);
                best = Some((standing, crest, band));
            }
        }
        best.expect("a ridge has at least two knots")
    }

    /// The ridge's height at a tile's centre, over `floor_dm`.
    fn height_at(&self, floor_dm: i32, point: (i64, i64)) -> i32 {
        let (standing, crest, band) = self.nearest(point);
        let relief = i64::from(crest - floor_dm);
        if relief <= 0 {
            return floor_dm;
        }
        let beyond = standing.across - band;
        let mut height = if beyond <= 0 {
            i64::from(crest)
        } else {
            let dropped = (drop(relief, self.slope_at(&standing), beyond) + 50) / 100;
            (i64::from(crest) - dropped).max(i64::from(floor_dm))
        };
        if beyond > 0 {
            height = (height - self.rim_at(&standing)).max(i64::from(floor_dm));
        }
        // The plateaus: a disc at each knot with a shelf.
        for knot in &self.knots {
            let radius = i64::from(knot.shelf_tiles) * SUB / 2;
            let middle = centre(knot.at);
            let gap = (point.0 - middle.0, point.1 - middle.1);
            if radius > 0 && gap.0 * gap.0 + gap.1 * gap.1 <= radius * radius {
                height = height.max(i64::from(knot.crest_dm));
            }
        }
        i32::try_from(height).expect("a height in decimetres")
    }
}

/// The heights of a `width × height` patch of the skeleton, in
/// decimetres, row by row: the floor where no ridge stands.
#[must_use]
pub fn heights(skeleton: &Skeleton, width: u32, height: u32) -> Vec<i16> {
    let mut out = Vec::with_capacity(width as usize * height as usize);
    for row in 0..height {
        for col in 0..width {
            let point = centre((
                i32::try_from(col).expect("small"),
                i32::try_from(row).expect("small"),
            ));
            let tile = skeleton
                .ridges
                .iter()
                .map(|ridge| ridge.height_at(skeleton.floor_dm, point))
                .max()
                .unwrap_or(skeleton.floor_dm);
            out.push(i16::try_from(tile).expect("a height in decimetres"));
        }
    }
    out
}

/// What a tile is in a skeleton, for the checks: on a declared crest, or
/// on a rim's step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mark {
    /// The tile's centre is within half a tile of a crest line; the
    /// vector across the crest, one tile long, in sub-tiles.
    pub crest: Option<(i64, i64)>,
    /// The tile stands within one tile of a rim's step, on the stepped
    /// side: its slope is the declared edge's, not the turf's.
    pub rim: bool,
}

/// The marks of a `width × height` patch, row by row.
#[must_use]
pub fn marks(skeleton: &Skeleton, width: u32, height: u32) -> Vec<Mark> {
    let mut out = vec![Mark::default(); width as usize * height as usize];
    for row in 0..height {
        for col in 0..width {
            let point = centre((
                i32::try_from(col).expect("small"),
                i32::try_from(row).expect("small"),
            ));
            let mark = &mut out[row as usize * width as usize + col as usize];
            for ridge in &skeleton.ridges {
                for pair in ridge.knots.windows(2) {
                    let (from, to) = (centre(pair[0].at), centre(pair[1].at));
                    let standing = stand(point, from, to);
                    if standing.across <= SUB / 2 {
                        let dir = (to.0 - from.0, to.1 - from.1);
                        mark.crest =
                            Some((-dir.1 * SUB / standing.len, dir.0 * SUB / standing.len));
                    }
                }
                let (standing, crest, band) = ridge.nearest(point);
                let beyond = standing.across - band;
                if crest > skeleton.floor_dm
                    && ridge.rim_at(&standing) > 0
                    && beyond > 0
                    && beyond <= SUB
                {
                    mark.rim = true;
                }
            }
        }
    }
    out
}

/// The measures of the plan's step E on a generated patch, so a test
/// asserts them and a report quotes them.
pub mod check {
    use super::{Mark, SUB};

    /// The ceiling on a slope off declared rock, in hundredths of a
    /// decimetre a tile: 0.75 m a metre (plan, step E).
    pub const MAX_SLOPE_PER_CENT_DM: i64 = 750;

    /// The ceiling on a step between two neighbouring tiles off a cliff,
    /// in decimetres: 1.7 m (plan, step E).
    pub const MAX_STEP_DM: i32 = 17;

    /// How far across a crest the 120° test probes, in tiles.
    pub const CREST_PROBE_TILES: i64 = 4;

    /// The cosine of 120°, in thousandths: the two up directions probed
    /// must make a wider angle, so their cosine must be below this.
    pub const CREST_COS_PER_MILLE: i64 = -500;

    /// The least a slope must rise, in decimetres over the probe's four
    /// tiles, for the probe to have a direction: under that, with heights
    /// in whole decimetres, the direction is noise.
    pub const PROBE_MIN_RISE_DM: i64 = 3;

    /// The least a flank may measure from the crest to the floor, in
    /// tiles (plan, step E: "au moins 25 cases de la crête au fond").
    pub const MIN_FLANK_TILES: i64 = 25;

    fn at(heights: &[i16], w: usize, x: usize, y: usize) -> i64 {
        i64::from(heights[y * w + x])
    }

    /// A tile index as a coordinate.
    fn tile_i64(index: usize) -> i64 {
        i64::try_from(index).expect("a patch is small")
    }

    /// The steepest step between two neighbouring tiles, in decimetres.
    #[must_use]
    pub fn max_step_dm(heights: &[i16], width: u32) -> i64 {
        let w = width as usize;
        let h = heights.len() / w;
        let mut worst = 0;
        for y in 0..h {
            for x in 0..w {
                let here = at(heights, w, x, y);
                if x + 1 < w {
                    worst = worst.max((at(heights, w, x + 1, y) - here).abs());
                }
                if y + 1 < h {
                    worst = worst.max((at(heights, w, x, y + 1) - here).abs());
                }
            }
        }
        worst
    }

    /// The steepest slope, in hundredths of a decimetre a tile: the norm
    /// of each tile's forward differences along `x` and `y`, skipping the
    /// tiles a rim marks (a declared edge) and their neighbours.
    #[must_use]
    pub fn max_slope_per_cent_dm(heights: &[i16], marks: &[Mark], width: u32) -> i64 {
        let w = width as usize;
        let h = heights.len() / w;
        let mut worst = 0;
        for y in 0..h.saturating_sub(1) {
            for x in 0..w.saturating_sub(1) {
                if marks[y * w + x].rim || marks[y * w + x + 1].rim || marks[(y + 1) * w + x].rim {
                    continue;
                }
                let here = at(heights, w, x, y);
                let dx = at(heights, w, x + 1, y) - here;
                let dy = at(heights, w, x, y + 1) - here;
                worst = worst.max(dx * dx + dy * dy);
            }
        }
        (worst * 10_000).isqrt()
    }

    /// How many tiles stand strictly below all eight of their neighbours:
    /// the basins, which a mountain made of lines never has.
    #[must_use]
    pub fn basins(heights: &[i16], width: u32) -> usize {
        let w = width as usize;
        let h = heights.len() / w;
        let mut count = 0;
        for y in 1..h.saturating_sub(1) {
            for x in 1..w.saturating_sub(1) {
                let here = at(heights, w, x, y);
                let lowest = (-1..=1).all(|dy: isize| {
                    (-1..=1).all(|dx: isize| {
                        (dx == 0 && dy == 0)
                            || at(
                                heights,
                                w,
                                x.wrapping_add_signed(dx),
                                y.wrapping_add_signed(dy),
                            ) > here
                    })
                });
                if lowest {
                    count += 1;
                }
            }
        }
        count
    }

    /// The up direction at a tile: the rise from two tiles before to two
    /// tiles after along `x` and `y`, in decimetres, the client's own read
    /// of a slope (D44: "the slope is read over two tiles"). `None` at
    /// the patch's edge.
    fn up(heights: &[i16], w: usize, h: usize, x: i64, y: i64) -> Option<(i64, i64)> {
        let (x, y) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?);
        if x < 2 || y < 2 || x + 2 >= w || y + 2 >= h {
            return None;
        }
        Some((
            at(heights, w, x + 2, y) - at(heights, w, x - 2, y),
            at(heights, w, x, y + 2) - at(heights, w, x, y - 2),
        ))
    }

    /// The 120° test on every marked crest tile: the up directions
    /// [`CREST_PROBE_TILES`] out on either side must each rise at least
    /// [`PROBE_MIN_RISE_DM`] and face away from each other by more than
    /// 120°. Returns the worst cosine met, in thousandths (1,000 when a
    /// probe had no direction or fell off the patch), and how many crest
    /// tiles were probed.
    #[must_use]
    pub fn crest_worst_cos_per_mille(heights: &[i16], marks: &[Mark], width: u32) -> (i64, usize) {
        let w = width as usize;
        let h = heights.len() / w;
        let mut worst = i64::MIN;
        let mut probed = 0;
        for (i, mark) in marks.iter().enumerate() {
            let Some((ax, ay)) = mark.crest else {
                continue;
            };
            probed += 1;
            let (cx, cy) = (
                tile_i64(i % w) * SUB + SUB / 2,
                tile_i64(i / w) * SUB + SUB / 2,
            );
            let probe = |sign: i64| {
                let px = (cx + sign * CREST_PROBE_TILES * ax).div_euclid(SUB);
                let py = (cy + sign * CREST_PROBE_TILES * ay).div_euclid(SUB);
                up(heights, w, h, px, py)
            };
            let cos = match (probe(1), probe(-1)) {
                (Some(u), Some(v)) => {
                    let (nu, nv) = (u.0 * u.0 + u.1 * u.1, v.0 * v.0 + v.1 * v.1);
                    if nu < PROBE_MIN_RISE_DM * PROBE_MIN_RISE_DM
                        || nv < PROBE_MIN_RISE_DM * PROBE_MIN_RISE_DM
                    {
                        1000
                    } else {
                        let dot = u.0 * v.0 + u.1 * v.1;
                        dot * 1000 / (nu * nv).isqrt().max(1)
                    }
                }
                _ => 1000,
            };
            worst = worst.max(cos);
        }
        (worst, probed)
    }

    /// The shortest way from a marked crest tile to a tile at the floor,
    /// in hundredths of a tile; `i64::MAX` with no crest or no floor.
    #[must_use]
    pub fn min_crest_to_floor_per_cent_tile(
        heights: &[i16],
        marks: &[Mark],
        width: u32,
        floor_dm: i16,
    ) -> i64 {
        let w = width as usize;
        let floor: Vec<(i64, i64)> = heights
            .iter()
            .enumerate()
            .filter(|&(_, &h)| h == floor_dm)
            .map(|(i, _)| (tile_i64(i % w), tile_i64(i / w)))
            .collect();
        let mut best = i64::MAX;
        for (i, mark) in marks.iter().enumerate() {
            if mark.crest.is_none() {
                continue;
            }
            let (cx, cy) = (tile_i64(i % w), tile_i64(i / w));
            let nearest = floor
                .iter()
                .map(|&(x, y)| (x - cx) * (x - cx) + (y - cy) * (y - cy))
                .min()
                .unwrap_or(i64::MAX / 10_000);
            best = best.min(nearest);
        }
        if best == i64::MAX {
            best
        } else {
            (best * 10_000).isqrt()
        }
    }
}

/// The plan's first tiny tests (§4: 1, 2 and 6), as skeletons on a floor
/// of [`trials::FLOOR_DM`]. Each is a patch of its own size; the server's
/// test world `test:squelette` lays them side by side.
pub mod trials {
    use super::{Knot, RIM_DM, Ridge, Side, Skeleton, Slope};

    /// The trials' valley floor, in decimetres: 50 m, the relief test's
    /// meadow.
    pub const FLOOR_DM: i32 = 500;

    /// The humpback's crest over the floor, in decimetres: 11.5 m, what
    /// a flank of about 25 tiles drops at 0.5 with the profile's
    /// shoulder and foot (plan §4.1: "flancs de 25 cases à 0,5"), and a
    /// little more so the shorter flank measures 25 tiles to the floor
    /// after rounding to the decimetre.
    pub const HUMPBACK_RELIEF_DM: i32 = 115;

    /// The humpback's and the col's summits' shelf, in tiles (plan §4.1:
    /// "replat de 5 cases").
    pub const SHELF_TILES: u32 = 5;

    /// The col's two summits over the floor, in decimetres, and the col
    /// between them. The col stands 12 % under the first summit and 14 %
    /// under the second (plan §2.A: "cols à 10–30 % du relief local"),
    /// as low as the col's own flanks allow while keeping 25 tiles to the
    /// floor at the plan's slope of 0.5 (step E).
    pub const COL_SUMMITS_DM: [i32; 2] = [136, 140];
    /// The col's height over the floor, in decimetres.
    pub const COL_DM: i32 = 120;

    /// A patch and its skeleton.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Trial {
        /// Its name, the plan's.
        pub name: &'static str,
        /// Its size in tiles.
        pub size: (u32, u32),
        /// Its skeleton.
        pub skeleton: Skeleton,
    }

    const fn knot(at: (i32, i32), over_floor_dm: i32, shelf_tiles: u32) -> Knot {
        Knot {
            at,
            crest_dm: FLOOR_DM + over_floor_dm,
            shelf_tiles,
        }
    }

    fn one(name: &'static str, size: (u32, u32), ridge: Ridge) -> Trial {
        Trial {
            name,
            size,
            skeleton: Skeleton {
                floor_dm: FLOOR_DM,
                ridges: vec![ridge],
            },
        }
    }

    /// Test 1, the humpback with a shoulder: one straight crest, oblique
    /// on screen (along the map's `(3, -1)`, 14° off the screen's
    /// horizontal: neither the screen's diagonal, where a crest
    /// vanishes, nor its vertical, where the flanks read flat, plan §3),
    /// a shelf of five tiles, flanks of 25 tiles at 0.5 (the longer 33),
    /// the longer on the near side.
    #[must_use]
    pub fn humpback() -> Trial {
        one(
            "humpback",
            (82, 76),
            Ridge {
                knots: vec![
                    knot((32, 40), HUMPBACK_RELIEF_DM, SHELF_TILES),
                    knot((47, 35), HUMPBACK_RELIEF_DM, SHELF_TILES),
                ],
                slope: Slope::Medium,
                long_side: Side::Plus,
                rim: None,
            },
        )
    }

    /// Test 2, the col: the same crest with two summits (shelves of five
    /// tiles, 19 and 22 tiles from the col: irregular on purpose) and a
    /// bare col between them.
    #[must_use]
    pub fn col() -> Trial {
        one(
            "col",
            (122, 90),
            Ridge {
                knots: vec![
                    knot((39, 48), COL_SUMMITS_DM[0], SHELF_TILES),
                    knot((57, 42), COL_DM, 0),
                    knot((78, 35), COL_SUMMITS_DM[1], SHELF_TILES),
                ],
                slope: Slope::Medium,
                long_side: Side::Plus,
                rim: None,
            },
        )
    }

    /// Test 6, the sense by edges: a crest along the screen's horizontal
    /// (the map's `(1, -1)`, ten tiles long), bare, then with a shelf of
    /// five tiles, then bare with a rim on the near side: the near
    /// versant stands [`RIM_DM`] higher, its edge is the crest, and the
    /// far versant starts a metre lower one tile behind (so both still
    /// measure 25 tiles to the floor); each also turned by 180° (the
    /// knots in the other order, so the long versant and the rim swap
    /// sides). Six patches, in that order: bare, shelf, rim, then the
    /// three turned.
    #[must_use]
    pub fn edges() -> Vec<Trial> {
        let size = (72, 72);
        // The long versant lies on the near side of the crest, so the
        // shape sits off the knots' centre toward the screen's bottom;
        // the turned crests are set the other way, each in its patch's
        // middle.
        let (a, b) = ((30, 37), (37, 30));
        let (a_turned, b_turned) = ((35, 42), (42, 35));
        let variants: [(&'static str, u32, Option<Side>, i32); 3] = [
            ("edge-bare", 0, None, HUMPBACK_RELIEF_DM),
            ("edge-shelf", SHELF_TILES, None, HUMPBACK_RELIEF_DM),
            (
                "edge-rim",
                0,
                Some(Side::Minus),
                HUMPBACK_RELIEF_DM + RIM_DM,
            ),
        ];
        let mut out = Vec::with_capacity(6);
        for turned in [false, true] {
            for (name, shelf, rim, relief) in variants {
                let (first, second) = if turned { (b_turned, a_turned) } else { (a, b) };
                out.push(one(
                    if turned {
                        match name {
                            "edge-bare" => "edge-bare-turned",
                            "edge-shelf" => "edge-shelf-turned",
                            _ => "edge-rim-turned",
                        }
                    } else {
                        name
                    },
                    size,
                    Ridge {
                        knots: vec![knot(first, relief, shelf), knot(second, relief, shelf)],
                        slope: Slope::Medium,
                        long_side: Side::Plus,
                        rim,
                    },
                ));
            }
        }
        out
    }

    /// All the trials, in the order the test world lays them.
    #[must_use]
    pub fn all() -> Vec<Trial> {
        let mut out = vec![humpback(), col()];
        out.extend(edges());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::check::{
        CREST_COS_PER_MILLE, MAX_SLOPE_PER_CENT_DM, MAX_STEP_DM, MIN_FLANK_TILES, basins,
        crest_worst_cos_per_mille, max_slope_per_cent_dm, max_step_dm,
        min_crest_to_floor_per_cent_tile,
    };
    use super::trials::{self, FLOOR_DM, HUMPBACK_RELIEF_DM, Trial};
    use super::*;

    fn bounds(heights: &[i16], width: u32) -> (usize, usize, usize, usize) {
        let w = width as usize;
        let mut b = (usize::MAX, usize::MAX, 0, 0);
        for (i, &h) in heights.iter().enumerate() {
            if i32::from(h) != FLOOR_DM {
                let (x, y) = (i % w, i / w);
                b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
            }
        }
        b
    }

    #[test]
    fn the_profile_drops_the_relief_over_the_flank_and_never_rises() {
        let (relief, slope) = (115, Slope::Medium.per_cent_dm());
        let len = flank_tiles(115, Slope::Medium) * SUB;
        assert_eq!(flank_tiles(115, Slope::Medium), 26, "about 25 tiles at 0.5");
        let mut last = 0;
        for d in 0..=len + 4 * SUB {
            let now = drop(relief, slope, d);
            assert!(now >= last, "d {d}: {now} < {last}");
            last = now;
        }
        assert_eq!(drop(relief, slope, len + 4 * SUB), relief * 100);
        assert!(
            drop(relief, slope, len - SUB) >= relief * 100 - 300,
            "the foot eases onto the floor"
        );
        // The shoulder is convex: its slope grows; the plane is one slope.
        let a = len * SHOULDER_PCT / 100;
        assert!(drop(relief, slope, a / 2) * 2 < drop(relief, slope, a));
        let plane = drop(relief, slope, a + 2 * SUB) - drop(relief, slope, a + SUB);
        assert_eq!(plane, 500, "half a metre a tile");
    }

    #[test]
    fn the_sides_are_named_by_the_cross_product() {
        let a = centre((10, 10));
        let b = centre((11, 9));
        assert_eq!(
            stand(centre((12, 12)), a, b).side,
            Side::Plus,
            "larger x + y"
        );
        assert_eq!(stand(centre((8, 8)), a, b).side, Side::Minus);
    }

    #[test]
    fn the_humpback_has_a_flat_shelf_a_longer_near_flank_and_the_floor_around() {
        let trial = trials::humpback();
        let (wide, tall) = trial.size;
        let heights = heights(&trial.skeleton, wide, tall);
        let at = |x: usize, y: usize| i32::from(heights[y * wide as usize + x]);
        assert_eq!(at(38, 38), FLOOR_DM + HUMPBACK_RELIEF_DM, "on the crest");
        assert_eq!(at(38, 39), FLOOR_DM + HUMPBACK_RELIEF_DM, "on the shelf");
        assert_eq!(at(0, 0), FLOOR_DM);
        assert_eq!(at(wide as usize - 1, tall as usize - 1), FLOOR_DM);
        // The near side (larger x + 3y) is the longer: the floor farther,
        // by about 1.25, counted in steps of (1, 3) across the crest.
        let steps = |toward_near: bool| {
            (1..12)
                .take_while(|&k| {
                    let (x, y) = if toward_near {
                        (38 + k, 38 + 3 * k)
                    } else {
                        (38 - k, 38 - 3 * k)
                    };
                    at(x, y) > FLOOR_DM
                })
                .count()
        };
        let (near, far) = (steps(true), steps(false));
        assert!(
            near * 10 >= far * 11 && near * 10 <= far * 14,
            "near {near}, far {far}"
        );
        let (x0, y0, x1, y1) = bounds(&heights, wide);
        assert!(x0 >= 1 && y0 >= 1 && x1 + 2 <= wide as usize && y1 + 2 <= tall as usize);
    }

    #[test]
    fn the_col_lies_between_two_summits_and_is_a_saddle() {
        let trial = trials::col();
        let (wide, tall) = trial.size;
        let heights = heights(&trial.skeleton, wide, tall);
        let at = |x: usize, y: usize| i32::from(heights[y * wide as usize + x]);
        assert_eq!(at(39, 48), FLOOR_DM + trials::COL_SUMMITS_DM[0]);
        assert_eq!(at(78, 35), FLOOR_DM + trials::COL_SUMMITS_DM[1]);
        assert_eq!(at(57, 42), FLOOR_DM + trials::COL_DM);
        // Lower than the ridge on either side, higher than the flanks.
        assert!(at(54, 43) > at(57, 42) && at(60, 41) > at(57, 42));
        assert!(at(57, 39) < at(57, 42) && at(57, 45) < at(57, 42));
        let (x0, y0, x1, y1) = bounds(&heights, wide);
        assert!(x0 >= 1 && y0 >= 1 && x1 + 2 <= wide as usize && y1 + 2 <= tall as usize);
    }

    #[test]
    fn the_rim_steps_down_a_metre_on_the_far_side_and_turns_with_the_crest() {
        let all = trials::edges();
        let rim = &all[2];
        let turned = &all[5];
        assert_eq!(rim.name, "edge-rim");
        assert_eq!(turned.name, "edge-rim-turned");
        let (wide, tall) = rim.size;
        let a = heights(&rim.skeleton, wide, tall);
        let b = heights(&turned.skeleton, wide, tall);
        let at = |hs: &[i16], x: usize, y: usize| i32::from(hs[y * wide as usize + x]);
        // The crest passes through (33, 34) and (34, 33); the far side is
        // the smaller x + y, the near side the larger. Turned, the crest
        // passes through (38, 39) and (39, 38).
        let rim_crest = FLOOR_DM + HUMPBACK_RELIEF_DM + RIM_DM;
        assert_eq!(at(&a, 33, 34), rim_crest);
        assert_eq!(at(&a, 33, 33), rim_crest - RIM_DM, "far side: stepped down");
        assert_eq!(at(&a, 34, 34), rim_crest, "near side: the crest's height");
        assert_eq!(
            at(&b, 39, 39),
            rim_crest - RIM_DM,
            "turned: near side stepped"
        );
        assert_eq!(at(&b, 38, 38), rim_crest);
        // Turned, the long flank is the far one.
        let (x0, y0, x1, y1) = bounds(&b, wide);
        assert!(x0 >= 1 && y0 >= 1 && x1 + 2 <= wide as usize && y1 + 2 <= tall as usize);
    }

    fn step_e(trial: &Trial) -> (i64, i64, usize, i64, usize, i64) {
        let (wide, tall) = trial.size;
        let heights = heights(&trial.skeleton, wide, tall);
        let marks = marks(&trial.skeleton, wide, tall);
        let floor = i16::try_from(trial.skeleton.floor_dm).expect("dm");
        let (cos, probed) = crest_worst_cos_per_mille(&heights, &marks, wide);
        (
            max_slope_per_cent_dm(&heights, &marks, wide),
            max_step_dm(&heights, wide),
            basins(&heights, wide),
            cos,
            probed,
            min_crest_to_floor_per_cent_tile(&heights, &marks, wide, floor),
        )
    }

    #[test]
    fn every_trial_holds_the_plans_step_e_invariants() {
        for trial in trials::all() {
            let (slope, step, basins, cos, probed, flank) = step_e(&trial);
            let name = trial.name;
            assert!(slope <= MAX_SLOPE_PER_CENT_DM, "{name}: slope {slope}");
            assert!(step <= i64::from(MAX_STEP_DM), "{name}: step {step} dm");
            assert_eq!(basins, 0, "{name}: basins");
            assert!(probed >= 8, "{name}: {probed} crest tiles probed");
            assert!(cos < CREST_COS_PER_MILLE, "{name}: worst cos {cos}");
            assert!(flank >= MIN_FLANK_TILES * 100, "{name}: flank {flank}");
        }
    }

    #[test]
    fn the_rim_is_the_one_step_past_the_turfs_ceiling_and_is_marked() {
        let rim = &trials::edges()[2];
        let (wide, tall) = rim.size;
        let heights = heights(&rim.skeleton, wide, tall);
        let marks = marks(&rim.skeleton, wide, tall);
        let none = vec![Mark::default(); marks.len()];
        assert!(max_slope_per_cent_dm(&heights, &none, wide) > MAX_SLOPE_PER_CENT_DM);
        assert!(max_slope_per_cent_dm(&heights, &marks, wide) <= MAX_SLOPE_PER_CENT_DM);
        assert!(marks.iter().filter(|m| m.rim).count() >= 6);
    }
}
