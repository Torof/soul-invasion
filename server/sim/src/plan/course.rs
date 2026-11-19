//! The natural course of a continent river at the tile scale (the project
//! lead's verdict of 15 November 2026 on the vivarium: "the river is
//! straight with perfectly parallel banks, and one bend is a right
//! angle").
//!
//! A river of the continent is followed cell to cell as a **stem**: from
//! a source, or from where the span looked at begins, down its receivers
//! while it is the main water of the cell it enters; where it is not, it
//! is a tributary and ends on the stem it joins. Its course is the
//! region's ([`region::course`], midpoint displacement at the 64 m cell)
//! concatenated over the cells, then:
//!
//! 1. **thinned to the river's scale** ([`thin`]): each cell's course
//!    keeps a point every [`PIECE_WIDTHS`] widths (a region cell at
//!    least), so a great river no longer follows the 64 m zigzag of the
//!    region's grid (banks kinked in parallel every cell, a canal with
//!    facets: the marsh sheet of 15 November 2026);
//! 2. **meandered** ([`meander`]): each piece is bent to one side, the
//!    pieces alternating or bending away from the thinned course's
//!    corners, by [`MEANDER_PER_MILLE`] of its length, so no piece stays
//!    straight (the region's displacement, drawn uniformly and truncated
//!    to whole cells, left a straight reach every few cells: 26 windows
//!    of 108), every shift keyed by its segment's two ends and no vertex
//!    turning past [`SHARPEST_VERTEX_DEG`]; then **rounded** by corner
//!    cutting (Chaikin, as many passes as bring the pieces under
//!    [`SMOOTH_TO_MM`], [`smooth_passes`], each cell with its own count
//!    over its own run, [`round_cells`]) so the course never kinks: no
//!    turn sharper than [`SHARPEST_TURN_COS_MILLI`] between two
//!    consecutive segments, and no bend tighter than the river's width;
//! 3. its **level settles on the ground** ([`settle`]): never above the
//!    ground beside it, never rising downstream;
//! 4. a **tributary joins** the stem it drains to where its course first
//!    comes within [`JOIN_REACH_WIDTHS`] of that stem's widths of the
//!    stem's water ([`join`]): the rest of its course is dropped and its
//!    end slides onto the stem, its last reach falling to the stem's level
//!    there ([`regrade_end`]), the stem growing to its downstream width
//!    from there (the lead's rule of 15 November 2026: at the east
//!    confluence of the vivarium the tributary ran alongside the stem for
//!    300 m before the atlas's node, two rivers side by side);
//! 5. its **banks walk** as the valley's do ([`walk_banks`], the rule of
//!    `map::Rivers::breathe`, in widths rather than tiles): over each
//!    cell's course, each bank on its own holds its line for a few steps,
//!    then leaves it at a slope of 5.6° for a dozen to forty steps, out
//!    or in, half the moves once off the line going back towards it,
//!    within its reach; a step is a fifth of the river's nominal width
//!    (the valley's tile for its river of five), so a 5 m brook, a 30 m
//!    river and a 200 m river look alike at their scale. Both banks hold
//!    their lines over the first and last [`CALM_STEPS`] of a cell's
//!    course (a cell is computed without its neighbours, D39), around a
//!    ford's bar and around a junction. The depth takes what the width
//!    leaves, so the flow is kept (the window's rule);
//! 6. the **ford** at an atlas crossing (`measure::passages` at
//!    [`measure::DRY_CM`]): where the atlas finds the cell's river
//!    crossed dry-shod, a bar [`FORD_BAR_WIDTHS_PER_MILLE`] river widths
//!    long lies across the stem at the point nearest the cell's centre,
//!    the river spreading over it as a lens ([`FORD_LENS_PER_MILLE`]).
//!
//! Everything is worked out from the seed, the continent and absolute
//! coordinates (millimetres from the region grid's origin), with every
//! choice keyed by the cell or the segment it belongs to, so two windows
//! that overlap draw the same river where they overlap, whatever span
//! each looked at: a stem cut short at a span's edge differs from the
//! whole one only within a few cells of that edge, and a window keeps a
//! margin of two continent cells.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use super::region::{self, REGION_CELL_M, RIVER_DISCHARGE};
use super::{Continent, isqrt, measure};
use crate::r#gen::Lcg;
use crate::r#gen::scatter::{key, purpose, roll};

/// Millimetres a region cell.
const CELL_MM: i64 = REGION_CELL_M * 1000;

/// A cell's course is thinned to pieces of this many of the river's
/// widths ([`PIECE_LEAST_CELLS`] region cells at least) before its
/// meanders are drawn: half a meander's wavelength, about ten widths on
/// a river, so the bends of a river are the size of the river. Below it
/// the region's 64 m points are dropped: on a river 170 m wide they drew
/// a canal kinked every cell, and on any river the region's drainage
/// zigzags from cell to cell (south, south-west, south...), a staircase
/// two cells of thinning straighten.
const PIECE_WIDTHS: i64 = 5;
const PIECE_LEAST_CELLS: i64 = 2;
/// No piece shorter than this share of a piece (three quarters: the
/// legs of every corner are then at least 1.9 widths, what
/// [`SHARPEST_VERTEX_DEG`] counts on) nor longer than twice it (a long
/// piece's straight legs, a dropped hairpin's for one, read as a
/// straight reach): the thinning keeps the first, the meanders split
/// what is over the second.
const LEAST_PIECE_SHARE: (i64, i64) = (3, 4);
const LONGEST_PIECE_SHARE: (i64, i64) = (3, 2);
/// The meander of a piece: its midpoint is shifted sideways by this much
/// of its length, least and most (a bend of 44° to 53° at the midpoint),
/// so no piece stays straight and the course winds at the river's
/// scale. The pieces of a cell's course bend to alternate sides, except
/// next to a corner of the thinned course sharper than
/// [`CORNER_COS_MILLI`] (30°): a piece leaving or reaching one bends
/// away from it, and a piece between two that turn opposite ways bends
/// away from the sharper. A bend into a corner stacks its turn on the
/// corner's: the shift is capped so that the two together stay under
/// [`SHARPEST_VERTEX_DEG`] ([`shift_cap_per_mille`]).
const MEANDER_PER_MILLE: (i64, i64) = (200, 250);
const CORNER_COS_MILLI: i64 = 866;
/// The sharpest a vertex of the meandered course may turn, a corner of
/// the thinned course and the bend stacked on it together: 70°
/// ([`shift_cap_per_mille`], [`HAIRPIN_COS_MILLI`]). With legs of half
/// a piece (1.9 widths at least, [`LEAST_PIECE_SHARE`]) the rounding
/// then bends the course with a radius over the river's width (1.07
/// widths at 70°), so the bank on the inside of a bend turns by 20° a
/// step at most and never folds. Before 15 November 2026 the region's
/// course bent a 114 m river with a radius of 75 m and its inner bank
/// folded; halvings below the pieces (the fine detail of the first
/// draft) shortened the legs and stacked their turns, and were dropped
/// for the same reason.
const SHARPEST_VERTEX_DEG: i64 = 70;
/// Corner-cutting passes (Chaikin) over the meandered course. Each pass
/// replaces a corner by two of about half its turn, a quarter of the
/// segments in from it, so a corner of the course becomes a bend turning
/// by a few degrees a piece. As many passes as bring a cell's pieces
/// under this length (mm), at least [`LEAST_SMOOTH`] and at most
/// [`MOST_SMOOTH`]: a small river's 128 m pieces take five, a great
/// river's 855 m pieces eight, and both end with nodes 2 to 4 m apart,
/// what the banks' walk and the window's stretches want.
const SMOOTH_TO_MM: i64 = 4_000;
const LEAST_SMOOTH: u32 = 3;
const MOST_SMOOTH: u32 = 8;

/// No turn sharper than this between two consecutive segments of a
/// finished course, as the cosine of the turn × 1000: 60°, so the angle
/// between the segments is 120° at least. What the tests hold the
/// rounding to.
#[cfg(test)]
pub(crate) const SHARPEST_TURN_COS_MILLI: i64 = 500;

/// The walker's step along the course, per mille of the river's nominal
/// width: a fifth, the valley's tile for its river of five tiles (the
/// valley walks its banks a tile at a time), so every figure below, the
/// valley's in tiles, holds in widths.
const WALK_STEP_PER_MILLE: i64 = 200;
/// How fast a bank leaves its line, per mille of a step per step: the
/// tangent of 5.6°, the valley's `BANK_SLOPE`, so a river widens or
/// narrows slowly, never with a tooth.
const BANK_SLOPE_PER_MILLE: i64 = 98;
/// Steps a bank holds its line between two moves, at least and at most
/// (the valley's `BANK_HOLD`).
const BANK_HOLD_STEPS: (u32, u32) = (4, 12);
/// Steps a move of a bank lasts, at least and at most (the valley's
/// `BANK_SHIFT`): a move changes the bank by a quarter to four fifths of
/// the width, so a widening arrives anywhere, lasts, and blends.
const BANK_SHIFT_STEPS: (u32, u32) = (12, 40);
/// How far a bank may stand inside its line and outside it, in steps of
/// the slope (the valley's `BANK_REACH`): a fifth of the width in, 1.2
/// widths out.
const BANK_REACH_STEPS: (i64, i64) = (-10, 61);
/// Steps of hold kept in hand before the way back to the line, so a bank
/// never has to turn twice at once (the valley's).
const BANK_SPARE_STEPS: usize = 6;
/// Tries at the banks of one free stretch before leaving it straight.
const BANK_TRIES: u32 = 30;
/// Steps over which the banks hold their lines at both ends of a cell's
/// course, around a ford's bar and around a junction: 1.2 widths, the
/// valley's `CLEARANCE`. A cell's course is walked without its
/// neighbours, so its banks must meet theirs at the nominal width.
const CALM_STEPS: usize = 6;
/// The width a river may come to as its banks walk, per mille of the
/// width its flow gives it, narrowest and widest: three fifths (both
/// banks a fifth in) and three times (the valley's pool of 18 tiles on a
/// river of five, the widening the lead liked).
pub(crate) const NARROWEST_PER_MILLE: i64 = 600;
pub(crate) const WIDEST_PER_MILLE: i64 = 3000;

/// A tributary joins the stem it drains to where its course first comes
/// within this many of the stem's widths of the stem's water: one, so two
/// waters a width apart are one.
const JOIN_REACH_WIDTHS: i64 = 1;
/// A tributary's end slides onto the stem it joins, the slide spread over
/// the tributary's last [`JOIN_BLEND_M`] metres at least and
/// [`JOIN_BLEND_PER_SHIFT`] times the slide at least, so its way in bends
/// by about 30° at most and the course does not kink.
const JOIN_BLEND_M: i64 = 50;
const JOIN_BLEND_PER_SHIFT: i64 = 2;
/// The stem's width grows to its downstream width from a junction over
/// this many of the downstream widths (two) or [`JOIN_BLEND_M`], the
/// longer: the water of both is there from the junction on.
const JOIN_WIDEN_WIDTHS: i64 = 2;
/// A tributary's course is looked at for its first contact with the stem
/// over its last this many continent cells: the atlas sends the tributary
/// to the stem's node, so the two run close only near it.
const JOIN_LOOK_CELLS: usize = 2;

/// A piece of a finished course is at least this long (mm): the
/// rounding of a hairpin leaves specks a few millimetres apart, whose
/// turns mean nothing.
const LEAST_PIECE_MM: i64 = 500;

/// A point the course steps to and turns back from is a spike, dropped:
/// the turn's cosine × 1000 under this. On the thinned course, before
/// its meanders, any corner over 70° ([`HAIRPIN_COS_MILLI`]): the
/// region's jittered nodes zigzag now and then within a cell (a turn of
/// 90° then one of 117° on seed 1), a river does not turn back on
/// itself within a few of its widths, and a sharper corner would bend
/// the course tighter than the river's width ([`SHARPEST_VERTEX_DEG`]).
/// On the rounded course, the specks the rounding leaves (about 155°,
/// [`SPIKE_COS_MILLI`]).
const HAIRPIN_COS_MILLI: i64 = 342;
const SPIKE_COS_MILLI: i64 = -900;

/// The ford at an atlas crossing: the bar's length along the course, per
/// mille of the river's width (two to three widths), and how much wider
/// the river spreads over it at the middle, per mille: a ford is a lens,
/// wider than the river around it (D36).
pub(crate) const FORD_BAR_WIDTHS_PER_MILLE: i64 = 2500;
const FORD_LENS_PER_MILLE: i64 = 300;

/// The steepest a river's surface falls, per mille (0.15 m a metre, a
/// torrent): where its level dives to the ground ([`settle`]), the dive
/// is spread upstream at this slope. Steeper, the water and the banks
/// stepped over the net's 1.7 m a tile across a bend: the surface folds
/// around it, and two tiles side by side lie on points of the course
/// metres apart along it. No cliff layer exists yet, so no waterfall.
const STEEPEST_FALL_PER_MILLE: i64 = 150;

/// A point of a river's course, in absolute millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Node {
    /// Where, in millimetres from the region grid's origin.
    pub(crate) p: (i64, i64),
    /// The water's level (cm).
    pub(crate) level: i64,
    /// The half-width on the left bank and on the right bank of the
    /// current, once the banks walk (mm).
    pub(crate) half: (i64, i64),
    /// The width the flow gives the river here, before its banks walk
    /// (mm).
    pub(crate) width: i64,
    /// The mean depth the flow gives it here (cm).
    pub(crate) depth: i64,
    /// On the ford's bar.
    pub(crate) ford: bool,
    /// The continent cell whose course this point belongs to: what the
    /// banks' walk is keyed by, so it is the same whatever the span.
    pub(crate) cell: usize,
}

/// One river of the continent followed cell to cell, at the tile scale.
#[derive(Debug, Clone)]
pub(crate) struct Stem {
    /// Its course, upstream end first.
    pub(crate) nodes: Vec<Node>,
    /// The continent cells it runs through, upstream first.
    pub(crate) cells: Vec<usize>,
    /// The stem it ends on, if it is a tributary of one in the span.
    pub(crate) joins: Option<usize>,
}

/// A smoothstep over 0..1000, per mille.
fn smooth(t: i64) -> i64 {
    let t = t.clamp(0, 1000);
    t * t * (3000 - 2 * t) / 1_000_000
}

/// The distance between two points (mm).
fn distance(a: (i64, i64), b: (i64, i64)) -> i64 {
    let d2 = (a.0 - b.0).pow(2) + (a.1 - b.1).pow(2);
    i64::try_from(isqrt(u64::try_from(d2).expect("a square"))).expect("small")
}

/// `a + (b - a) × num / den`, for a node's every number.
fn between(a: Node, b: Node, num: i64, den: i64) -> Node {
    let mix = |x: i64, y: i64| x + (y - x) * num / den;
    Node {
        p: (mix(a.p.0, b.p.0), mix(a.p.1, b.p.1)),
        level: mix(a.level, b.level),
        half: (mix(a.half.0, b.half.0), mix(a.half.1, b.half.1)),
        width: mix(a.width, b.width),
        depth: mix(a.depth, b.depth),
        ford: a.ford,
        // Across two cells' courses the point keeps the nearer cell.
        cell: if 2 * num < den { a.cell } else { b.cell },
    }
}

/// The course steps to `b` from `a` and turns back towards `c` by more
/// than `cos_milli` allows (the cosine of the turn × 1000, negative): a
/// spike.
fn spikes(a: (i64, i64), b: (i64, i64), c: (i64, i64), cos_milli: i64) -> bool {
    let (u, v) = ((b.0 - a.0, b.1 - a.1), (c.0 - b.0, c.1 - b.1));
    if (u.0, u.1) == (0, 0) || (v.0, v.1) == (0, 0) {
        return false;
    }
    !turns_within(u, v, cos_milli)
}

/// The course without its spikes ([`spikes`], turns over `cos_milli`)
/// and without pieces shorter than `least` (mm), the ends kept.
fn tidy(nodes: &[Node], least: i64, cos_milli: i64) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::with_capacity(nodes.len());
    for &n in nodes {
        while out.len() >= 2 && spikes(out[out.len() - 2].p, out[out.len() - 1].p, n.p, cos_milli) {
            out.pop();
        }
        if out.last().is_some_and(|last| distance(last.p, n.p) < least) {
            continue;
        }
        out.push(n);
    }
    if let (Some(&last), Some(end)) = (nodes.last(), out.last_mut())
        && end.p != last.p
    {
        if out.len() >= 2 && distance(out[out.len() - 2].p, last.p) >= least {
            *out.last_mut().expect("two") = last;
        } else {
            out.push(last);
        }
    }
    out
}

/// The nodes of one cell's course thinned to pieces of about `piece`
/// millimetres: both ends kept, and between them every node at least
/// [`LEAST_PIECE_SHARE`] of a piece along from the one kept before and
/// from the end, so no piece is shorter than that share (a short piece
/// between two corners bent the course tighter than the river's width)
/// nor longer than about twice a piece. A course shorter than a piece
/// keeps its two ends only.
fn thin(nodes: &[Node], piece: i64) -> Vec<Node> {
    if nodes.len() < 3 {
        return nodes.to_vec();
    }
    let along = along(nodes);
    let length = along[nodes.len() - 1];
    let least = piece * LEAST_PIECE_SHARE.0 / LEAST_PIECE_SHARE.1;
    let mut out = vec![nodes[0]];
    let mut kept_at = 0;
    for j in 1..nodes.len() - 1 {
        if along[j] - kept_at >= least
            && length - along[j] >= least
            && nodes[j].p != out[out.len() - 1].p
        {
            out.push(nodes[j]);
            kept_at = along[j];
        }
    }
    out.push(nodes[nodes.len() - 1]);
    out
}

/// A segment's midpoint shifted sideways by `per_mille` of its length
/// (positive to the left of its way, seen with y downwards).
fn shifted(a: Node, b: Node, per_mille: i64) -> Node {
    let (dx, dy) = (b.p.0 - a.p.0, b.p.1 - a.p.1);
    let mut mid = between(a, b, 1, 2);
    mid.p = (
        mid.p.0 - dy * per_mille / 1000,
        mid.p.1 + dx * per_mille / 1000,
    );
    mid
}

/// The roll of a segment, keyed by its two ends.
fn segment_roll(seed: u64, purpose: u64, a: (i64, i64), b: (i64, i64)) -> u64 {
    roll(seed, purpose, &[key(a.0), key(a.1), key(b.0), key(b.1)])
}

/// The corner at `v` (reached from `p`, left towards `n`): the cosine of
/// its turn × 1000, and the sign of a shift ([`shifted`]) that bends a
/// piece either side of it into it (a positive shift of either piece
/// bends it towards the side the corner turns to).
fn corner(p: (i64, i64), v: (i64, i64), n: (i64, i64)) -> (i64, i64) {
    let (u, w) = ((v.0 - p.0, v.1 - p.1), (n.0 - v.0, n.1 - v.1));
    let dot = i128::from(u.0) * i128::from(w.0) + i128::from(u.1) * i128::from(w.1);
    let uu = i128::from(u.0).pow(2) + i128::from(u.1).pow(2);
    let vv = i128::from(w.0).pow(2) + i128::from(w.1).pow(2);
    let cos_milli = if dot <= 0 || uu == 0 || vv == 0 {
        0
    } else {
        i64::try_from(isqrt(
            u64::try_from(dot * dot * 1_000_000 / (uu * vv)).expect("a cosine"),
        ))
        .expect("small")
    };
    let cross = i128::from(u.0) * i128::from(w.1) - i128::from(u.1) * i128::from(w.0);
    (cos_milli.min(1000), if cross > 0 { 1 } else { -1 })
}

/// The most a piece may be shifted, per mille of its length, when its
/// bend goes into a corner whose turn's cosine × 1000 is `cos_milli`:
/// the corner's turn and the bend's (twice the arc tangent of twice the
/// shift) together stay under [`SHARPEST_VERTEX_DEG`]; by bands of five
/// degrees of the corner, from none to 70° (a sharper corner is a
/// hairpin, dropped before).
fn shift_cap_per_mille(cos_milli: i64) -> i64 {
    // (cos × 1000 at the band's upper bound, the cap for that band):
    // tan((70° − corner) / 2) / 2 at the band's sharpest corner.
    const BANDS: [(i64, i64); 15] = [
        (996, 315),
        (985, 289),
        (966, 260),
        (940, 233),
        (906, 208),
        (866, 184),
        (819, 161),
        (766, 139),
        (707, 117),
        (643, 96),
        (574, 66),
        (500, 44),
        (423, 22),
        (342, 0),
        (0, 0),
    ];
    // The last band with a cap is the sharpest vertex less five degrees.
    debug_assert_eq!(
        BANDS[BANDS.len() - 3].0,
        423,
        "{SHARPEST_VERTEX_DEG}° less 5°"
    );
    BANDS
        .iter()
        .find(|&&(at_most, _)| cos_milli >= at_most)
        .map_or(0, |&(_, cap)| cap)
}

/// The meanders of one cell's run of a course: `run`, from the cell
/// before's last node to the cell's last, thinned to its pieces of
/// about `piece`, with the nodes `before` and `after` it, where there
/// are any, telling where the corners at its ends are. A piece over
/// [`LONGEST_PIECE_SHARE`] of a piece is first split evenly; then every
/// piece is bent to one side by [`MEANDER_PER_MILLE`] of its length, the
/// pieces to alternate sides unless a corner says otherwise, the bend
/// capped where it goes into a corner ([`shift_cap_per_mille`]), every
/// shift keyed by its segment's two ends (see the module). The run's
/// two ends included.
fn meander(
    seed: u64,
    before: Option<Node>,
    run: &[Node],
    after: Option<Node>,
    piece: i64,
) -> Vec<Node> {
    // The pieces, none over the share.
    let longest = piece * LONGEST_PIECE_SHARE.0 / LONGEST_PIECE_SHARE.1;
    let mut nodes: Vec<Node> = Vec::with_capacity(run.len() * 2);
    nodes.extend(before);
    nodes.extend(run.first().copied());
    for pair in run.windows(2) {
        let parts = (distance(pair[0].p, pair[1].p) + longest.max(1) - 1) / longest.max(1);
        let parts = parts.max(1);
        for j in 1..=parts {
            nodes.push(between(pair[0], pair[1], j, parts));
        }
    }
    nodes.extend(after);
    let (lo, hi) = (
        usize::from(before.is_some()),
        nodes.len() - 1 - usize::from(after.is_some()),
    );
    let (least, most) = MEANDER_PER_MILLE;
    let mut out = Vec::with_capacity((hi - lo) * 2 + 1);
    out.push(nodes[lo]);
    for k in lo..hi {
        let (a, b) = (nodes[k], nodes[k + 1]);
        let r = segment_roll(seed, purpose::PLAN_FINE, a.p, b.p);
        let magnitude = least + i64::try_from(r % 1000).expect("small") * (most - least) / 999;
        // The corners at both ends: (cosine × 1000, the sign bending
        // into it), where there is a node beyond.
        let at_start = k.checked_sub(1).map(|j| corner(nodes[j].p, a.p, b.p));
        let at_end = nodes.get(k + 2).map(|n| corner(a.p, b.p, n.p));
        let sharp = |c: Option<(i64, i64)>| c.filter(|&(cos, _)| cos < CORNER_COS_MILLI);
        let sign = match (sharp(at_start), sharp(at_end)) {
            // Away from the sharper of two corners (or the one), and
            // from both when they turn the same way.
            (Some((start_cos, start_into)), Some((end_cos, end_into))) => {
                if start_cos <= end_cos {
                    -start_into
                } else {
                    -end_into
                }
            }
            (Some((_, into)), None) | (None, Some((_, into))) => -into,
            (None, None) => {
                if (k - lo) % 2 == 0 {
                    1
                } else {
                    -1
                }
            }
        };
        let cap = [at_start, at_end]
            .into_iter()
            .flatten()
            .filter(|&(_, into)| into == sign)
            .map(|(cos, _)| shift_cap_per_mille(cos))
            .min()
            .unwrap_or(most);
        out.push(shifted(a, b, sign * magnitude.min(cap)));
        out.push(b);
    }
    out
}

/// How many corner-cutting passes bring pieces of `piece` under
/// [`SMOOTH_TO_MM`] (see the constants): set by the river's width,
/// never by what a span measured, so every span rounds a cell alike.
fn smooth_passes(piece: i64) -> u32 {
    let mut passes = 0;
    let mut piece = piece;
    while piece > SMOOTH_TO_MM && passes < MOST_SMOOTH {
        piece /= 2;
        passes += 1;
    }
    passes.max(LEAST_SMOOTH)
}

/// The meanders of a line from `a` to `b` (mm), for the window's brooks:
/// `depth` halvings of midpoint displacement, the first shifting the
/// midpoint by `per_mille.0` to `per_mille.1` of the line's length to
/// either side, the ones after by up to `per_mille.1` either way at the
/// second and half as much at each after, each shift drawn from `seed`
/// and `purpose` with `keys` and the segment's place in the line, so the
/// same line gives the same meanders whichever window asks. Both ends
/// included, `2^depth + 1` points.
pub(crate) fn meander_line(
    seed: u64,
    purpose: u64,
    keys: &[u64],
    a: (i64, i64),
    b: (i64, i64),
    depth: u32,
    per_mille: (i64, i64),
) -> Vec<(i64, i64)> {
    #[allow(clippy::too_many_arguments)]
    fn split(
        seed: u64,
        purpose: u64,
        keys: &[u64],
        a: (i64, i64),
        b: (i64, i64),
        depth: u32,
        per_mille: (i64, i64),
        index: u64,
        out: &mut Vec<(i64, i64)>,
    ) {
        if depth == 0 {
            out.push(b);
            return;
        }
        let mut k = keys.to_vec();
        k.push(index);
        let r = roll(seed, purpose, &k);
        let shift = if index == 1 {
            let magnitude = per_mille.0
                + i64::try_from(r % 1000).expect("small") * (per_mille.1 - per_mille.0) / 999;
            if (r >> 20) & 1 == 0 {
                magnitude
            } else {
                -magnitude
            }
        } else {
            // The halving's rank below the first: 1 at the second.
            let rank = (u64::BITS - index.leading_zeros()).saturating_sub(2);
            (i64::try_from(r % 2001).expect("small") - 1000) * per_mille.1 / 1000 / (1 << rank)
        };
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let mid = (
            i64::midpoint(a.0, b.0) - dy * shift / 1000,
            i64::midpoint(a.1, b.1) + dx * shift / 1000,
        );
        split(
            seed,
            purpose,
            keys,
            a,
            mid,
            depth - 1,
            per_mille,
            2 * index,
            out,
        );
        split(
            seed,
            purpose,
            keys,
            mid,
            b,
            depth - 1,
            per_mille,
            2 * index + 1,
            out,
        );
    }
    let mut out = vec![a];
    split(seed, purpose, keys, a, b, depth, per_mille, 1, &mut out);
    out
}

/// The runs of nodes of one cell along a course: `(from, to)`.
fn cell_runs(nodes: &[Node]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut from = 0;
    while from < nodes.len() {
        let cell = nodes[from].cell;
        let to = nodes[from..]
            .iter()
            .position(|n| n.cell != cell)
            .map_or(nodes.len(), |len| from + len);
        out.push((from, to));
        from = to;
    }
    out
}

/// The course rounded cell by cell: each cell's run of nodes, with two
/// nodes of the cells either side, is cut by corners its own number of
/// times (`passes` of its cell, [`smooth_passes`]), and what lies
/// between the run's first node and its last is kept, by the control
/// polygon's parameter (a node's index, in quarters at every pass). Two
/// cells rounded by different counts meet on one curve, since the
/// curve near a node depends on that node and its neighbours alone, and
/// a span that holds a cell but not the next rounds it the same way (D39).
fn round_cells(nodes: &[Node], passes: impl Fn(usize) -> u32) -> Vec<Node> {
    let mut out = Vec::with_capacity(nodes.len() * 8);
    for (from, to) in cell_runs(nodes) {
        let (lo, hi) = (from.saturating_sub(2), (to + 2).min(nodes.len()));
        let mut poly: Vec<(Node, i64)> = nodes[lo..hi]
            .iter()
            .enumerate()
            .map(|(j, &n)| (n, i64::try_from(lo + j).expect("small")))
            .collect();
        let mut scale = 1;
        for _ in 0..passes(nodes[from].cell) {
            if poly.len() < 3 {
                break;
            }
            let mut next = Vec::with_capacity(2 * poly.len());
            next.push((poly[0].0, poly[0].1 * 4));
            for pair in poly.windows(2) {
                let ((a, pa), (b, pb)) = (pair[0], pair[1]);
                next.push((between(a, b, 1, 4), 4 * pa + (pb - pa)));
                next.push((between(a, b, 3, 4), 4 * pa + 3 * (pb - pa)));
            }
            next.push((poly[poly.len() - 1].0, poly[poly.len() - 1].1 * 4));
            poly = next;
            scale *= 4;
        }
        let (first, last) = (
            i64::try_from(from).expect("small") * scale,
            i64::try_from(to).expect("small") * scale,
        );
        // The run's own cell, whatever the neighbours' nodes mixed in.
        let cell = nodes[from].cell;
        out.extend(
            poly.into_iter()
                .filter(|&(_, p)| (first..last).contains(&p))
                .map(|(mut n, _)| {
                    n.cell = cell;
                    n
                }),
        );
    }
    out
}

/// Two consecutive segments turn no more than `cos_milli` allows (the
/// cosine of the turn, × 1000).
fn turns_within(u: (i64, i64), v: (i64, i64), cos_milli: i64) -> bool {
    let dot = i128::from(u.0) * i128::from(v.0) + i128::from(u.1) * i128::from(v.1);
    let uu = i128::from(u.0).pow(2) + i128::from(u.1).pow(2);
    let vv = i128::from(v.0).pow(2) + i128::from(v.1).pow(2);
    if uu == 0 || vv == 0 {
        return true;
    }
    // cos = dot / √(uu vv) ≥ c: for c ≥ 0, dot ≥ 0 and dot² × 10⁶ ≥
    // c² × uu × vv; for c < 0, dot ≥ 0 or dot² × 10⁶ ≤ c² × uu × vv.
    let c2 = i128::from(cos_milli).pow(2) * uu * vv;
    if cos_milli >= 0 {
        dot >= 0 && dot * dot * 1_000_000 >= c2
    } else {
        dot >= 0 || dot * dot * 1_000_000 <= c2
    }
}

/// The sharpest turn along `nodes` stays within `cos_milli`.
#[cfg(test)]
pub(crate) fn course_turns_within(nodes: &[Node], cos_milli: i64) -> bool {
    nodes.windows(3).all(|w| {
        turns_within(
            (w[1].p.0 - w[0].p.0, w[1].p.1 - w[0].p.1),
            (w[2].p.0 - w[1].p.0, w[2].p.1 - w[1].p.1),
            cos_milli,
        )
    })
}

/// The nearest point to `p` of the segment from `a` to `b`, and its
/// distance.
fn to_segment(a: (i64, i64), b: (i64, i64), p: (i64, i64)) -> ((i64, i64), i64) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let q = if len2 == 0 {
        a
    } else {
        let t = ((p.0 - a.0) * dx + (p.1 - a.1) * dy).clamp(0, len2);
        (a.0 + dx * t / len2, a.1 + dy * t / len2)
    };
    (q, distance(p, q))
}

/// The nearest point to `p` of the polyline `nodes`: the point, its
/// distance and its segment (from that node to the next).
fn nearest_on(nodes: &[Node], p: (i64, i64)) -> Option<((i64, i64), i64, usize)> {
    nodes
        .windows(2)
        .enumerate()
        .map(|(s, pair)| {
            let (q, d) = to_segment(pair[0].p, pair[1].p, p);
            (q, d, s)
        })
        .min_by_key(|&(_, d, _)| d)
}

/// The segments of a course by the 64 m square each runs through, for a
/// quick look at what lies within reach of a point: a tributary's every
/// node asks what the stem is near it.
struct Near<'a> {
    nodes: &'a [Node],
    squares: BTreeMap<(i64, i64), Vec<usize>>,
}

impl<'a> Near<'a> {
    fn new(nodes: &'a [Node]) -> Self {
        let mut squares: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
        for (s, pair) in nodes.windows(2).enumerate() {
            let (a, b) = (pair[0].p, pair[1].p);
            for x in a.0.min(b.0).div_euclid(CELL_MM)..=a.0.max(b.0).div_euclid(CELL_MM) {
                for y in a.1.min(b.1).div_euclid(CELL_MM)..=a.1.max(b.1).div_euclid(CELL_MM) {
                    squares.entry((x, y)).or_default().push(s);
                }
            }
        }
        Self { nodes, squares }
    }

    /// The nearest point of the course within `reach` of `p`: the point,
    /// its distance and its segment.
    fn nearest(&self, p: (i64, i64), reach: i64) -> Option<((i64, i64), i64, usize)> {
        let r = reach.div_euclid(CELL_MM) + 1;
        let (cx, cy) = (p.0.div_euclid(CELL_MM), p.1.div_euclid(CELL_MM));
        let mut best: Option<((i64, i64), i64, usize)> = None;
        for x in cx - r..=cx + r {
            for y in cy - r..=cy + r {
                for &s in self.squares.get(&(x, y)).map_or(&[][..], Vec::as_slice) {
                    let (q, d) = to_segment(self.nodes[s].p, self.nodes[s + 1].p, p);
                    if d <= reach && best.is_none_or(|(_, bd, _)| d < bd) {
                        best = Some((q, d, s));
                    }
                }
            }
        }
        best
    }
}

/// How far along `nodes` each lies, from the first (mm).
fn along(nodes: &[Node]) -> Vec<i64> {
    let mut out = vec![0; nodes.len()];
    for j in 1..nodes.len() {
        out[j] = out[j - 1] + distance(nodes[j - 1].p, nodes[j].p);
    }
    out
}

/// The last `n` nodes of `nodes` slide by `shift`, the slide spread over
/// the course's last `blend` millimetres so it does not kink.
fn slide_end(nodes: &mut [Node], shift: (i64, i64), blend: i64) {
    let mut back = vec![0; nodes.len()];
    for j in (0..nodes.len().saturating_sub(1)).rev() {
        back[j] = back[j + 1] + distance(nodes[j].p, nodes[j + 1].p);
    }
    for (node, back) in nodes.iter_mut().zip(back) {
        let w = smooth(1000 - back * 1000 / blend.max(1));
        node.p = (node.p.0 + shift.0 * w / 1000, node.p.1 + shift.1 * w / 1000);
    }
}

/// One bank over the nodes `0..=n` of a free stretch, standing on its
/// line at both ends: how far out it stands at each, in steps of the
/// slope, or none if this draw breaks a rule. `fits` tells whether the
/// bank may stand `k` out at node `j`. The valley's `bank_profile`, less
/// its rule about the centre's bends: a course here is rounded already.
fn bank_profile(n: usize, fits: impl Fn(usize, i64) -> bool, rng: &mut Lcg) -> Option<Vec<i64>> {
    let between = |rng: &mut Lcg, (lo, hi): (u32, u32)| lo + rng.next_below(hi - lo + 1);
    let (inside, outside) = BANK_REACH_STEPS;
    let (shortest, longest) = BANK_SHIFT_STEPS;
    let spare = BANK_SPARE_STEPS;
    let mut k = vec![0; n + 1];
    let (mut j, mut out) = (0, 0_i64);
    let mut hold = between(rng, BANK_HOLD_STEPS);
    while j < n {
        let back = usize::try_from(out.unsigned_abs()).expect("small");
        if n - j < back {
            return None;
        }
        let must_return = out != 0 && n - j <= back + spare;
        if hold == 0 || must_return {
            // A move from node j: back towards the line half the time
            // once off it, else out or in; of a length that fits the
            // reach and the width at every step and leaves room to come
            // back.
            let dir = if must_return || (out != 0 && rng.next_below(2) == 0) {
                -out.signum()
            } else if rng.next_below(2) == 0 {
                1
            } else {
                -1
            };
            let span = (longest - shortest + 1) as usize;
            let first = rng.next_below(longest - shortest + 1) as usize;
            let lengths: Vec<usize> = if must_return {
                vec![back]
            } else {
                (0..span)
                    .map(|i| shortest as usize + (first + i) % span)
                    .collect()
            };
            let chosen = lengths.into_iter().find(|&steps| {
                let end = j + steps;
                let last = out + dir * i64::try_from(steps).expect("small");
                end <= n
                    && (inside..=outside).contains(&last)
                    && (must_return
                        || n - end >= usize::try_from(last.unsigned_abs()).expect("small") + spare)
                    && (1..=steps)
                        .all(|i| fits(j + i, out + dir * i64::try_from(i).expect("small")))
            });
            if let Some(steps) = chosen {
                for _ in 0..steps {
                    out += dir;
                    j += 1;
                    k[j] = out;
                }
                hold = between(rng, BANK_HOLD_STEPS);
                continue;
            }
        }
        hold = hold.saturating_sub(1);
        j += 1;
        k[j] = out;
    }
    (k[n] == 0).then_some(k)
}

/// The two banks over the nodes `0..=n` of a free stretch, in steps of
/// the slope, or none if no draw fits: first each bank on its own, the
/// second fitting around the first; then, if that keeps failing, the
/// first bank leaves the second its whole reach. `fits` tells whether the
/// two banks may stand `total` steps out together. The valley's.
fn banks_profile(
    n: usize,
    fits: impl Fn(i64) -> bool,
    rng: &mut Lcg,
) -> Option<(Vec<i64>, Vec<i64>)> {
    let (inside, outside) = BANK_REACH_STEPS;
    for attempt in 0..2 * BANK_TRIES {
        let first_fits = |k: i64| attempt < BANK_TRIES || (fits(k + inside) && fits(k + outside));
        let Some(one) = bank_profile(n, |_, k| first_fits(k), rng) else {
            continue;
        };
        let Some(other) = bank_profile(n, |j, k| fits(one[j] + k), rng) else {
            continue;
        };
        if (0..=n).all(|j| fits(one[j] + other[j])) {
            return Some(if rng.next_below(2) == 0 {
                (other, one)
            } else {
                (one, other)
            });
        }
    }
    None
}

/// The banks of one cell's course (`nodes`, the cell `xy` in grid
/// coordinates, the river `width` wide there, mm) walk as the valley's
/// do (see the module): drawn from a stream keyed by the cell, in steps
/// of a fifth of the width along the course, calm over [`CALM_STEPS`] at
/// both ends and around each of `quiet` (a node's index and a reach
/// either way along the course, mm: a ford's bar, a junction).
fn walk_banks(seed: u64, nodes: &mut [Node], xy: (i32, i32), width: i64, quiet: &[(usize, i64)]) {
    let along = along(nodes);
    let step = (width * WALK_STEP_PER_MILLE / 1000).max(1);
    let Some(&length) = along.last() else {
        return;
    };
    let n = usize::try_from(length / step).expect("small");
    let calm_reach = i64::try_from(CALM_STEPS).expect("small") * step;
    let mut calm: Vec<bool> = (0..=n)
        .map(|s| s < CALM_STEPS || s + CALM_STEPS > n)
        .collect();
    for &(at, reach) in quiet {
        let (lo, hi) = (
            along[at] - reach - calm_reach,
            along[at] + reach + calm_reach,
        );
        for (s, c) in calm.iter_mut().enumerate() {
            let here = i64::try_from(s).expect("small") * step;
            if (lo..=hi).contains(&here) {
                *c = true;
            }
        }
    }
    let mut rng = Lcg::new(roll(
        seed,
        purpose::PLAN_BANK,
        &[key(i64::from(xy.0)), key(i64::from(xy.1))],
    ));
    let narrowest = width * NARROWEST_PER_MILLE / 1000;
    let widest = width * WIDEST_PER_MILLE / 1000;
    let out_mm = |k: i64| k * BANK_SLOPE_PER_MILLE * step / 1000;
    let fits = |total: i64| (narrowest..=widest).contains(&(width + out_mm(total)));
    let mut banks = vec![(0, 0); n + 1];
    let mut s = 0;
    while s <= n {
        if calm[s] {
            s += 1;
            continue;
        }
        let first = s;
        while s <= n && !calm[s] {
            s += 1;
        }
        // Nodes first - 1 (calm) to s (calm), as j = 0..=stretch.
        let stretch = s - first + 1;
        if let Some((left, right)) = banks_profile(stretch, fits, &mut rng) {
            for j in 1..stretch {
                banks[first - 1 + j] = (left[j], right[j]);
            }
        }
    }
    // Each node's banks from the steps' offsets around it, a cubic
    // through them that never overshoots (Hermite, a step's tangent its
    // move's where the move goes on through it and flat where a move
    // begins, ends or turns): along a move the bank is the line the
    // walker drew, and where a move begins or ends its heading turns
    // over the step instead of at a corner, which showed on a wide
    // river, a step being a fifth of its width (the marsh sheet of
    // 15 November 2026). The offsets are in steps of the slope, so the
    // cubic is taken in thousandths of a step.
    let last = i64::try_from(n).expect("small");
    let at_step = |s: i64, bank: usize| -> i64 {
        let (left, right) = banks[usize::try_from(s.clamp(0, last)).expect("small")];
        if bank == 0 { left } else { right }
    };
    let tangent = |s: i64, bank: usize| -> i64 {
        let (before, after) = (
            at_step(s, bank) - at_step(s - 1, bank),
            at_step(s + 1, bank) - at_step(s, bank),
        );
        if before == after { before } else { 0 }
    };
    for (node, &at) in nodes.iter_mut().zip(&along) {
        let s = (at / step).min(last);
        let t = (at % step) * 1000 / step;
        let cubic = |bank: usize| {
            let (p1, p2) = (at_step(s, bank), at_step(s + 1, bank));
            let (m1, m2) = (tangent(s, bank), tangent(s + 1, bank));
            // Hermite with t in thousandths, the result × 1000.
            let (t2, t3) = (t * t / 1000, t * t * t / 1_000_000);
            let value = p1 * (2 * t3 - 3 * t2 + 1000)
                + m1 * (t3 - 2 * t2 + t)
                + p2 * (3 * t2 - 2 * t3)
                + m2 * (t3 - t2);
            value * BANK_SLOPE_PER_MILLE * step / 1_000_000
        };
        node.half = (node.width / 2 + cubic(0), node.width / 2 + cubic(1));
    }
}

/// The ford's bar over `nodes` around node `at`: every node within half
/// a bar along the course is on the ford, the river spreading over it
/// as a lens.
fn spread_ford(nodes: &mut [Node], at: usize) {
    let bar = nodes[at].width * FORD_BAR_WIDTHS_PER_MILLE / 1000;
    let half_bar = (bar / 2).max(1);
    let mut along = vec![0; nodes.len()];
    for j in (0..at).rev() {
        along[j] = along[j + 1] + distance(nodes[j].p, nodes[j + 1].p);
    }
    for j in at + 1..nodes.len() {
        along[j] = along[j - 1] + distance(nodes[j].p, nodes[j - 1].p);
    }
    for (node, &along) in nodes.iter_mut().zip(&along) {
        if along > half_bar {
            continue;
        }
        let lens = smooth(1000 - along * 1000 / half_bar);
        let f = 1000 + FORD_LENS_PER_MILLE * lens / 1000;
        node.half = (node.half.0 * f / 1000, node.half.1 * f / 1000);
        node.ford = true;
    }
}

/// The water never runs above the ground beside it, and never falls in a
/// step: along the course,
/// each node's level is the lower of the atlas's (falling evenly from
/// cell to cell) and the lowest ground met so far at any node (`ground`,
/// the region's before its valleys are cut, around the point), so the
/// level never rises downstream and the river dives into a dip of the
/// ground instead of crossing it in the air. Before 15 November 2026 the
/// level was the atlas's alone: in the mountains, where the region's
/// detail swings 100 m and more, the river ran up to 54 m above the
/// ground and the window raised the ground to its water, a dyke with a
/// 47 m step ((631, 624) of seed 14).
fn settle(nodes: &mut [Node], ground: &dyn Fn((i64, i64)) -> Option<i64>) {
    let mut low = i64::MAX;
    for node in nodes.iter_mut() {
        if let Some(g) = ground(node.p) {
            // Never under the sea's level: ground below it is flooded
            // to it (`Region`), and a river diving under it stepped
            // where it met that water at the coast.
            low = low.min(g.max(0));
        }
        node.level = node.level.min(low);
    }
    // No waterfall: where the water dives, it starts falling upstream of
    // the dip, at `STEEPEST_FALL_PER_MILLE` at most.
    for j in (1..nodes.len()).rev() {
        let fall = distance(nodes[j - 1].p, nodes[j].p) * STEEPEST_FALL_PER_MILLE / 10_000;
        nodes[j - 1].level = nodes[j - 1].level.min(nodes[j].level + fall);
    }
}

/// The water's level at `q`, a point of segment `s` of `nodes` (cm).
fn level_on(nodes: &[Node], s: usize, q: (i64, i64)) -> i64 {
    let (a, b) = (nodes[s], nodes[s + 1]);
    let len = distance(a.p, b.p);
    if len == 0 {
        return a.level;
    }
    a.level + (b.level - a.level) * distance(a.p, q) / len
}

/// A tributary cut short meets its stem at the stem's level `target`,
/// not at the level its own course had there: its levels from node
/// `from` on fall evenly to `target`, from the latest node at or above
/// it, and where its whole course lies lower (the stem is higher than
/// the ground the atlas gave the tributary), the water stands at the
/// stem's level, never flowing uphill. Before 15 November 2026 the cut
/// kept the tributary's own levels, so in the mountains it reached the
/// stem up to 50 m above it and the window raised the ground to its
/// water: a dyke beside the stem ((631, 624) of seed 14).
fn regrade_end(nodes: &mut [Node], from: usize, target: i64) {
    let Some(last) = nodes.len().checked_sub(1) else {
        return;
    };
    let mut m0 = from.min(last);
    while m0 > 0 && nodes[m0].level < target {
        m0 -= 1;
    }
    let along = along(nodes);
    let (l0, a0) = (nodes[m0].level, along[m0]);
    let span = (along[last] - a0).max(1);
    for m in m0..=last {
        nodes[m].level = l0 + (target - l0) * (along[m] - a0) / span;
    }
    for node in nodes.iter_mut() {
        node.level = node.level.max(target);
    }
}

/// The width and depth the flow gives a river over one cell's course.
#[derive(Debug, Clone, Copy)]
struct Course {
    /// The cell, in grid coordinates.
    xy: (i32, i32),
    /// The width the flow gives the river over it (mm).
    width: i64,
    /// The mean depth it gives it (cm).
    depth: i64,
}

/// Tributary `k` of `stems` joins stem `main` where its course first
/// comes within [`JOIN_REACH_WIDTHS`] of the stem's widths of the stem's
/// water; its course is cut there and its end slides onto the stem (see
/// the module). The stem grows to the width of `end`, the cell the atlas
/// sends the tributary to (`courses`), from the junction on. Returns the
/// stem's node the tributary joins at, or none if the two never come
/// close (the end then slides onto the stem's nearest point).
fn join(
    stems: &mut [Stem],
    k: usize,
    main: usize,
    end: usize,
    courses: &BTreeMap<usize, Course>,
) -> Option<usize> {
    let anchor = stems[k].nodes[stems[k].nodes.len() - 1].p;
    // The tributary's last cells.
    let cells = &stems[k].cells;
    let last_cells = &cells[cells.len().saturating_sub(JOIN_LOOK_CELLS)..];
    let start = stems[k]
        .nodes
        .iter()
        .position(|n| last_cells.contains(&n.cell))
        .unwrap_or(0);
    let reach = |w: i64| w / 2 + JOIN_REACH_WIDTHS * w;
    // The stem's water at the junction is both waters': the width it has
    // downstream of the junction, where the atlas sends the tributary.
    let downstream_width = courses.get(&end).map_or(0, |c| c.width);
    let widest = stems[main]
        .nodes
        .iter()
        .map(|n| n.width)
        .max()
        .unwrap_or(0)
        .max(downstream_width);
    let near = Near::new(&stems[main].nodes);
    let mut cut = None;
    for j in start..stems[k].nodes.len() {
        let p = stems[k].nodes[j].p;
        if let Some((q, d, s)) = near.nearest(p, reach(widest))
            && d <= reach(stems[main].nodes[s].width.max(downstream_width))
        {
            cut = Some((j, q, d, s));
            break;
        }
    }
    let Some((j, q, d, s)) = cut else {
        // Never close: the end slides onto the stem's nearest point.
        if let Some((q, d, s)) = nearest_on(&stems[main].nodes, anchor) {
            let shift = (q.0 - anchor.0, q.1 - anchor.1);
            let blend = (JOIN_BLEND_M * 1000).max(JOIN_BLEND_PER_SHIFT * d);
            slide_end(&mut stems[k].nodes, shift, blend);
            let target = level_on(&stems[main].nodes, s, q);
            regrade_end(&mut stems[k].nodes, start, target);
        }
        return None;
    };
    stems[k].nodes.truncate(j + 1);
    let p = stems[k].nodes[j].p;
    let shift = (q.0 - p.0, q.1 - p.1);
    let blend = (JOIN_BLEND_M * 1000).max(JOIN_BLEND_PER_SHIFT * d);
    slide_end(&mut stems[k].nodes, shift, blend);
    let target = level_on(&stems[main].nodes, s, q);
    regrade_end(&mut stems[k].nodes, start, target);
    // The stem carries both waters from the junction on: its width grows
    // to the downstream cell's over a short way.
    if let Some(&downstream) = courses.get(&end) {
        let nodes = &mut stems[main].nodes;
        // Up to where the course's own width reaches the downstream
        // cell's (the widths blend across the two cells' courses as
        // they are rounded), never over a node already as wide.
        let first = nodes
            .iter()
            .enumerate()
            .position(|(m, n)| m > s && n.width >= downstream.width)
            .unwrap_or(nodes.len());
        let along = along(nodes);
        let widen = (JOIN_BLEND_M * 1000).max(JOIN_WIDEN_WIDTHS * downstream.width);
        for m in s + 1..first {
            let t = smooth((along[m] - along[s]) * 1000 / widen);
            let node = &mut nodes[m];
            node.width += (downstream.width - node.width) * t / 1000;
            node.depth += (downstream.depth - node.depth) * t / 1000;
            node.half = (node.width / 2, node.width / 2);
        }
    }
    Some(s)
}

/// The river stems of `c` whose cells lie in `x` × `y` (continent
/// cells): see the module.
#[allow(clippy::too_many_lines)]
pub(crate) fn stems(
    c: &Continent,
    x: std::ops::Range<i32>,
    y: std::ops::Range<i32>,
    ground: Option<&dyn Fn((i64, i64)) -> Option<i64>>,
) -> Vec<Stem> {
    let seed = c.seed;
    let is_river = |i: usize| !c.sea[i] && c.discharge[i] >= RIVER_DISCHARGE;
    let in_span = |i: usize| {
        let (cx, cy) = c.grid.xy(i);
        x.contains(&cx) && y.contains(&cy)
    };
    // The main water entering cell `i`: the river among its neighbours
    // that drains into it with the largest discharge.
    let main_upstream = |i: usize| -> Option<usize> {
        let (cx, cy) = c.grid.xy(i);
        let mut best: Option<(u64, usize)> = None;
        for dy in -1..=1 {
            for dx in -1..=1 {
                if (dx, dy) == (0, 0) || !c.grid.inside(cx + dx, cy + dy) {
                    continue;
                }
                let j = c.grid.index(cx + dx, cy + dy);
                if j != i
                    && is_river(j)
                    && c.receiver[j] as usize == i
                    && best.is_none_or(|(q, k)| (c.discharge[j], Reverse(j)) > (q, Reverse(k)))
                {
                    best = Some((c.discharge[j], j));
                }
            }
        }
        best.map(|(_, j)| j)
    };
    let passages = measure::passages(c, measure::DRY_CM);
    let mm = |g: (i64, i64)| (g.0 * CELL_MM + CELL_MM / 2, g.1 * CELL_MM + CELL_MM / 2);

    // The chains of cells.
    let mut chains: Vec<(Vec<usize>, usize)> = Vec::new();
    for cy in y.clone() {
        for cx in x.clone() {
            if !c.grid.inside(cx, cy) {
                continue;
            }
            let start = c.grid.index(cx, cy);
            if !is_river(start) || main_upstream(start).is_some_and(in_span) {
                continue;
            }
            let mut cells = vec![start];
            let mut cur = start;
            let end = loop {
                let r = c.receiver[cur] as usize;
                if r == cur {
                    break cur;
                }
                if !is_river(r) || !in_span(r) || main_upstream(r) != Some(cur) {
                    break r;
                }
                cells.push(r);
                cur = r;
            };
            chains.push((cells, end));
        }
    }
    let owner: BTreeMap<usize, usize> = chains
        .iter()
        .enumerate()
        .flat_map(|(k, (cells, _))| cells.iter().map(move |&i| (i, k)))
        .collect();

    // The stems before their tributaries: a tributary's course ends on
    // the rounded course of the stem it drains to, so it must be drawn
    // first. A chain is ready once the chain owning its end is drawn.
    let mut order: Vec<usize> = Vec::with_capacity(chains.len());
    let mut drawn = vec![false; chains.len()];
    while order.len() < chains.len() {
        let before = order.len();
        for (k, (cells, end)) in chains.iter().enumerate() {
            let ready = cells.contains(end) || owner.get(end).is_none_or(|&m| drawn[m]);
            if !drawn[k] && ready {
                drawn[k] = true;
                order.push(k);
            }
        }
        assert!(order.len() > before, "the waters form a tree");
    }

    // Each chain's course, cell by cell, with its levels and widths.
    let mut out: Vec<Option<Stem>> = vec![None; chains.len()];
    let mut courses: BTreeMap<usize, Course> = BTreeMap::new();
    for &chain in &order {
        let (cells, end) = &chains[chain];
        let mut nodes: Vec<Node> = Vec::new();
        for (k, &i) in cells.iter().enumerate() {
            let r = cells.get(k + 1).copied().unwrap_or(*end);
            let (ix, iy) = c.grid.xy(i);
            let (rx, ry) = c.grid.xy(r);
            let path = region::course(seed, region::node(seed, ix, iy), region::node(seed, rx, ry));
            let (top, bottom) = (i64::from(c.surface[i]), i64::from(c.surface[r]));
            let q = c.discharge[i];
            let width = (i64::try_from(measure::width_cm(q)).expect("small") * 10).max(2000);
            let depth = i64::try_from(measure::depth_cm(q)).expect("small");
            courses.insert(
                i,
                Course {
                    xy: (ix, iy),
                    width,
                    depth,
                },
            );
            let steps = i64::try_from(path.len() - 1).expect("small").max(1);
            let mut raw: Vec<Node> = path
                .iter()
                .enumerate()
                .map(|(j, &g)| {
                    let j = i64::try_from(j).expect("small");
                    Node {
                        p: mm(g),
                        level: top - (top - bottom) * j / steps,
                        half: (width / 2, width / 2),
                        width,
                        depth,
                        ford: false,
                        cell: i,
                    }
                })
                .collect();
            // A tributary's last cell ends on the stem's course as drawn
            // (rounded, it passes up to a piece from the atlas's node),
            // at the point of it nearest that node, so the two meet.
            if k + 1 == cells.len()
                && let Some(main) = owner.get(end).and_then(|&m| out[m].as_ref())
                && let Some((q, _, _)) = nearest_on(&main.nodes, mm(region::node(seed, rx, ry)))
            {
                // The stem's course may have cut the corner the atlas's
                // node made (a hairpin dropped, up to a kilometre): the
                // way there is paced like the region's path, a point a
                // cell, so it gets its pieces and its meanders too.
                let last = raw.pop().expect("a path");
                let before = raw.last().copied().unwrap_or(last);
                let steps = (distance(before.p, q) / CELL_MM).max(1);
                for j in 1..=steps {
                    let mut node = between(before, last, j, steps);
                    node.p = (
                        before.p.0 + (q.0 - before.p.0) * j / steps,
                        before.p.1 + (q.1 - before.p.1) * j / steps,
                    );
                    raw.push(node);
                }
            }
            // The cell's course thinned to the river's scale (see the
            // module); its first node is the cell before's last.
            let piece = (PIECE_WIDTHS * width).max(PIECE_LEAST_CELLS * CELL_MM);
            nodes.extend(thin(&raw, piece).into_iter().skip(usize::from(k > 0)));
        }
        // No hairpin; then each cell's meanders, each cell's rounding.
        nodes = tidy(&nodes, 1, HAIRPIN_COS_MILLI);
        let piece_of =
            |cell: usize| (PIECE_WIDTHS * courses[&cell].width).max(PIECE_LEAST_CELLS * CELL_MM);
        let mut meandered = Vec::with_capacity(nodes.len() * 2);
        for (from, to) in cell_runs(&nodes) {
            // A run's pieces start at the node before it, the cell
            // before's last, so the piece between the two cells gets
            // its meanders too.
            let own = meander(
                seed,
                from.checked_sub(2).map(|j| nodes[j]),
                &nodes[from.saturating_sub(1)..to],
                nodes.get(to).copied(),
                piece_of(nodes[from].cell),
            );
            meandered.extend(own.into_iter().skip(usize::from(from > 0)));
        }
        nodes = round_cells(&meandered, |cell| smooth_passes(piece_of(cell)));
        nodes = tidy(&nodes, LEAST_PIECE_MM, SPIKE_COS_MILLI);
        if let Some(ground) = ground {
            settle(&mut nodes, ground);
        }
        let joins = if cells.contains(end) {
            None
        } else {
            owner.get(end).copied()
        };
        out[chain] = Some(Stem {
            nodes,
            cells: cells.clone(),
            joins,
        });
    }
    let mut out: Vec<Stem> = out.into_iter().map(|s| s.expect("drawn")).collect();

    // The tributaries join at first contact; the junctions, per stem.
    let mut junctions: Vec<Vec<usize>> = vec![Vec::new(); out.len()];
    for k in 0..out.len() {
        let Some(main) = out[k].joins else {
            continue;
        };
        if let Some(at) = join(&mut out, k, main, chains[k].1, &courses) {
            junctions[main].push(at);
        }
    }

    // The fords at the atlas's crossings: their bars' middles; then the
    // banks' walk over each cell's course, calm around the bars and the
    // junctions; then the lens over each bar.
    for (stem, junctions) in out.iter_mut().zip(&junctions) {
        let mut fords = Vec::new();
        for &i in &stem.cells {
            if passages[i] != measure::Passage::Crossing {
                continue;
            }
            let (cx, cy) = c.grid.xy(i);
            let per = i64::from(region::PER_CELL);
            let centre = mm((i64::from(cx) * per + per / 2, i64::from(cy) * per + per / 2));
            if let Some(at) =
                (0..stem.nodes.len()).min_by_key(|&j| distance(stem.nodes[j].p, centre))
            {
                fords.push(at);
            }
        }
        for (from, to) in cell_runs(&stem.nodes) {
            let cell = stem.nodes[from].cell;
            if let Some(&course) = courses.get(&cell) {
                let quiet: Vec<(usize, i64)> = fords
                    .iter()
                    .filter(|&&at| (from..to).contains(&at))
                    .map(|&at| {
                        (
                            at - from,
                            stem.nodes[at].width * FORD_BAR_WIDTHS_PER_MILLE / 2000,
                        )
                    })
                    .chain(
                        junctions
                            .iter()
                            .filter(|&&at| (from..to).contains(&at))
                            .map(|&at| (at - from, 0)),
                    )
                    .collect();
                walk_banks(
                    seed,
                    &mut stem.nodes[from..to],
                    course.xy,
                    course.width,
                    &quiet,
                );
            }
        }
        for &at in &fords {
            spread_ford(&mut stem.nodes, at);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use super::*;

    /// The seeds of the small continents the tests read: 1 to 3.
    const SEEDS: usize = 3;

    /// A small continent, as the relief look test's tests use: born once
    /// for the module's tests, which only read it.
    fn continent(seed: u64) -> &'static Continent {
        static CONTINENTS: [OnceLock<Continent>; SEEDS] = [const { OnceLock::new() }; SEEDS];
        CONTINENTS[seed_index(seed)].get_or_init(|| Continent::with(seed, 128, None))
    }

    /// The whole stems of the continent of `seed`, drawn once for the
    /// module's tests, which only read them.
    fn shared_stems(seed: u64) -> &'static [Stem] {
        static STEMS: [OnceLock<Vec<Stem>>; SEEDS] = [const { OnceLock::new() }; SEEDS];
        STEMS[seed_index(seed)].get_or_init(|| all_stems(continent(seed)))
    }

    fn seed_index(seed: u64) -> usize {
        let i = usize::try_from(seed).expect("small") - 1;
        assert!(i < SEEDS, "seed {seed} is not shared");
        i
    }

    fn all_stems(c: &Continent) -> Vec<Stem> {
        let side = i32::try_from(c.grid.side).expect("small");
        stems(c, 0..side, 0..side, None)
    }

    #[test]
    fn no_turn_is_sharper_than_sixty_degrees() {
        for seed in 1..=3 {
            let c = continent(seed);
            let stems = shared_stems(c.seed);
            assert!(!stems.is_empty(), "seed {seed} has rivers");
            for (k, s) in stems.iter().enumerate() {
                if !course_turns_within(&s.nodes, SHARPEST_TURN_COS_MILLI) {
                    for w in s.nodes.windows(3) {
                        let (u, v) = (
                            (w[1].p.0 - w[0].p.0, w[1].p.1 - w[0].p.1),
                            (w[2].p.0 - w[1].p.0, w[2].p.1 - w[1].p.1),
                        );
                        if !turns_within(u, v, SHARPEST_TURN_COS_MILLI) {
                            let (cos_milli, _) = corner(w[0].p, w[1].p, w[2].p);
                            eprintln!(
                                "seed {seed} stem {k}: a turn of cosine {cos_milli} at {:?} (cells {} {} {}, widths {} {} {}, pieces {} {} mm)",
                                w[1].p,
                                w[0].cell,
                                w[1].cell,
                                w[2].cell,
                                w[0].width,
                                w[1].width,
                                w[2].width,
                                distance(w[0].p, w[1].p),
                                distance(w[1].p, w[2].p)
                            );
                        }
                    }
                }
                assert!(
                    course_turns_within(&s.nodes, SHARPEST_TURN_COS_MILLI),
                    "seed {seed}, stem {k}"
                );
            }
        }
    }

    /// The longest chord of `nodes` every node between whose ends lies
    /// within a quarter of the width of it (a metre at least), mm: the
    /// safety net's straight reach (`plan::net`).
    fn longest_straight(nodes: &[Node]) -> i64 {
        let off_line = |a: (i64, i64), b: (i64, i64), p: (i64, i64)| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            ((dx * (p.1 - a.1) - dy * (p.0 - a.0)) / distance(a, b).max(1)).abs()
        };
        let mut longest = 0;
        for i in 0..nodes.len() {
            let tolerance = (nodes[i].width / 4).max(1000);
            for j in i + 1..nodes.len() {
                if !(i + 1..j).all(|k| off_line(nodes[i].p, nodes[j].p, nodes[k].p) <= tolerance) {
                    break;
                }
                longest = longest.max(distance(nodes[i].p, nodes[j].p));
            }
        }
        longest
    }

    /// No reach of a river runs straight: within a quarter width of a
    /// chord over 300 m or ten widths, the longer (the lead's rule of
    /// 15 November 2026, the safety net's `StraightReach`; before, the
    /// region's displacement left one every few cells: 26 windows of
    /// 108, the vivarium's river straight for 675 m).
    #[test]
    fn no_reach_runs_straight() {
        let mut stems_judged = 0;
        for seed in 1..=3 {
            let c = continent(seed);
            for (k, s) in shared_stems(c.seed).iter().enumerate() {
                let width = s.nodes.iter().map(|n| n.width).max().unwrap_or(0);
                let allowed = 300_000.max(10 * width);
                let straight = longest_straight(&s.nodes);
                if straight > allowed {
                    // Where: the chord.
                    let off_line = |a: (i64, i64), b: (i64, i64), p: (i64, i64)| {
                        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                        ((dx * (p.1 - a.1) - dy * (p.0 - a.0)) / distance(a, b).max(1)).abs()
                    };
                    let nodes = &s.nodes;
                    let (mut bi, mut bj, mut best) = (0, 0, 0);
                    for i in 0..nodes.len() {
                        let tolerance = (nodes[i].width / 4).max(1000);
                        for j in i + 1..nodes.len() {
                            if !(i + 1..j)
                                .all(|k| off_line(nodes[i].p, nodes[j].p, nodes[k].p) <= tolerance)
                            {
                                break;
                            }
                            if distance(nodes[i].p, nodes[j].p) > best {
                                best = distance(nodes[i].p, nodes[j].p);
                                bi = i;
                                bj = j;
                            }
                        }
                    }
                    let along = along(nodes);
                    eprintln!(
                        "reach from {:?} to {:?}: nodes {bi}..{bj} of {} (along {} .. {} of {}), cells {:?}, joins {:?}, widths {} {}, fords {}",
                        nodes[bi].p,
                        nodes[bj].p,
                        nodes.len(),
                        along[bi],
                        along[bj],
                        along[nodes.len() - 1],
                        {
                            let mut v: Vec<usize> = nodes[bi..=bj].iter().map(|n| n.cell).collect();
                            v.dedup();
                            v
                        },
                        s.joins,
                        nodes[bi].width,
                        nodes[bj].width,
                        nodes[bi..=bj].iter().filter(|n| n.ford).count()
                    );
                }
                assert!(
                    straight <= allowed,
                    "seed {seed}, stem {k}: a straight reach of {} m on a river {} m wide, {} m allowed",
                    straight / 1000,
                    width / 1000,
                    allowed / 1000
                );
                stems_judged += 1;
            }
        }
        assert!(stems_judged > 30, "{stems_judged} stems judged");
    }

    /// The cosine × 1000 of every whole degree from 0 to 90.
    const COS_MILLI_BY_DEGREE: [i64; 91] = [
        1000, 1000, 999, 999, 998, 996, 995, 993, 990, 988, 985, 982, 978, 974, 970, 966, 961, 956,
        951, 946, 940, 934, 927, 921, 914, 906, 899, 891, 883, 875, 866, 857, 848, 839, 829, 819,
        809, 799, 788, 777, 766, 755, 743, 731, 719, 707, 695, 682, 669, 656, 643, 629, 616, 602,
        588, 574, 559, 545, 530, 515, 500, 485, 469, 454, 438, 423, 407, 391, 375, 358, 342, 326,
        309, 292, 276, 259, 242, 225, 208, 191, 174, 156, 139, 122, 105, 87, 70, 52, 35, 17, 0,
    ];

    /// Each bank, read at every node, turns by at most 20° a step of the
    /// walker (a fifth of the width; a node of a small river is farther
    /// than a step from the next, and is allowed that much more, up to
    /// the course's own 60°), on every river of every width: the banks
    /// of a great river stay smooth (the marsh sheet of 15 November 2026
    /// showed them stepping), and no bend of the course is tighter than
    /// its banks.
    #[test]
    fn the_banks_turn_gently_at_every_width() {
        let (mut turns_judged, mut widest) = (0, 0);
        for seed in 1..=3 {
            let c = continent(seed);
            for (k, s) in shared_stems(c.seed).iter().enumerate() {
                for (from, to) in cell_runs(&s.nodes) {
                    let nodes = &s.nodes[from..to];
                    let step = (nodes[0].width * WALK_STEP_PER_MILLE / 1000).max(1);
                    widest = widest.max(nodes[0].width);
                    for bank in 0..2 {
                        let points: Vec<(i64, i64)> = (from..to)
                            .filter(|&j| j + 1 < s.nodes.len())
                            .map(|j| {
                                let (n, ahead) = (s.nodes[j], s.nodes[j + 1].p);
                                let (dx, dy) = (ahead.0 - n.p.0, ahead.1 - n.p.1);
                                let len = distance(n.p, ahead).max(1);
                                // The bank at the river's own width: where
                                // the walker stands a bank out, a pool on
                                // the inside of a bend may still fold.
                                let (half, sign) = (n.width / 2, if bank == 0 { 1 } else { -1 });
                                (
                                    n.p.0 - sign * dy * half / len,
                                    n.p.1 + sign * dx * half / len,
                                )
                            })
                            .collect();
                        for (j, w) in points.windows(3).enumerate() {
                            let (u, v) = (
                                (w[1].0 - w[0].0, w[1].1 - w[0].1),
                                (w[2].0 - w[1].0, w[2].1 - w[1].1),
                            );
                            let shorter = distance(w[0], w[1]).min(distance(w[1], w[2]));
                            let allowed = (20 * shorter.max(step) / step).min(60);
                            let cos_milli =
                                COS_MILLI_BY_DEGREE[usize::try_from(allowed).expect("small")];
                            assert!(
                                turns_within(u, v, cos_milli.max(SHARPEST_TURN_COS_MILLI)),
                                "seed {seed}, stem {k}, bank {bank} turns over {allowed}° at node {j} of a {} mm river near {:?}: {:?}",
                                nodes[0].width,
                                w[1],
                                &s.nodes[from + j.saturating_sub(2)..(from + j + 4).min(to)]
                            );
                            turns_judged += 1;
                        }
                    }
                }
            }
        }
        assert!(
            turns_judged > 1000 && widest > 30_000,
            "{turns_judged} turns judged, widest river {widest} mm"
        );
    }

    #[test]
    fn each_bank_stays_within_the_walkers_reach_and_the_width_within_its_room() {
        let c = continent(1);
        for s in shared_stems(c.seed) {
            for n in s.nodes.iter().filter(|n| !n.ford) {
                // The walker steps in its cell's nominal width; a node's
                // own width blends across two cells' courses and grows
                // past a junction, so the banks are read as offsets from
                // the node's own half-width, against the cell's.
                let width = (i64::try_from(measure::width_cm(c.discharge[n.cell])).expect("small")
                    * 10)
                    .max(2000);
                let step = width * WALK_STEP_PER_MILLE / 1000;
                let out = |k: i64| k * BANK_SLOPE_PER_MILLE * step / 1000;
                for half in [n.half.0, n.half.1] {
                    let offset = half - n.width / 2;
                    let (lo, hi) = (out(BANK_REACH_STEPS.0) - 1, out(BANK_REACH_STEPS.1) + 1);
                    assert!(
                        (lo..=hi).contains(&offset),
                        "an offset of {offset} on a river of {width} (the node {} wide)",
                        n.width
                    );
                }
                // The two banks' cubics between two steps may sum to a
                // quarter of a step more than the walker's own line.
                let total = n.half.0 + n.half.1 - n.width + width;
                assert!(
                    (width * NARROWEST_PER_MILLE / 1000 - out(1)
                        ..=width * WIDEST_PER_MILLE / 1000 + out(1))
                        .contains(&total),
                    "{total} wide for {width}: {n:?}"
                );
            }
        }
    }

    /// The widths along a stem, off the fords, with how far along the
    /// course each lies (mm).
    fn widths_along(s: &Stem) -> Vec<(i64, i64)> {
        let mut along = 0;
        let mut out = Vec::with_capacity(s.nodes.len());
        for (j, n) in s.nodes.iter().enumerate() {
            if j > 0 {
                along += distance(s.nodes[j - 1].p, n.p);
            }
            if !n.ford {
                out.push((along, n.half.0 + n.half.1));
            }
        }
        out
    }

    #[test]
    fn the_width_nearly_doubles_within_five_hundred_metres_somewhere() {
        let c = continent(1);
        let reach = 500_000;
        let mut best = 0;
        for s in shared_stems(c.seed) {
            let widths = widths_along(s);
            for (i, &(from, _)) in widths.iter().enumerate() {
                let (mut lo, mut hi) = (i64::MAX, 0);
                for &(at, w) in &widths[i..] {
                    if at - from > reach {
                        break;
                    }
                    lo = lo.min(w);
                    hi = hi.max(w);
                }
                if lo < i64::MAX {
                    best = best.max(hi * 1000 / lo);
                }
            }
        }
        assert!(
            best >= 1800,
            "widest over narrowest within 500 m: {best} per mille"
        );
    }

    #[test]
    fn the_two_banks_walk_each_on_its_own() {
        let c = continent(1);
        let (mut apart, mut all) = (0, 0);
        for s in shared_stems(c.seed) {
            for n in s.nodes.iter().filter(|n| !n.ford) {
                all += 1;
                // The banks stand a tenth of the width apart from each
                // other's offset.
                if (n.half.0 - n.half.1).abs() > n.width / 10 {
                    apart += 1;
                }
            }
        }
        assert!(
            apart * 5 > all,
            "{apart} of {all} nodes have their banks a tenth apart"
        );
    }

    #[test]
    fn the_banks_hold_their_lines_at_both_ends_of_a_cells_course() {
        let c = continent(1);
        let mut checked = 0;
        for s in shared_stems(c.seed) {
            for (from, to) in cell_runs(&s.nodes) {
                for n in [s.nodes[from], s.nodes[to - 1]] {
                    if n.ford {
                        continue;
                    }
                    assert_eq!(n.half, (n.width / 2, n.width / 2), "{n:?}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 100);
    }

    #[test]
    fn each_bank_leaves_its_line_over_every_long_cell_course_and_never_the_same_way_twice() {
        let c = continent(1);
        let (mut long, mut levels) = (0, 0);
        for s in shared_stems(c.seed) {
            for (from, to) in cell_runs(&s.nodes) {
                let nodes = &s.nodes[from..to];
                let width = nodes[0].width;
                let along = along(nodes);
                let length = along[nodes.len() - 1];
                // Sixty steps of a fifth of a width: room for a hold, a
                // move and the way back, twice over; a river wide enough
                // for its nodes, 4 to 10 m apart, to resolve its steps.
                if nodes.iter().any(|n| n.ford) || length < 12 * width || width < 20_000 {
                    continue;
                }
                long += 1;
                for bank in 0..2 {
                    let offsets: Vec<i64> = nodes
                        .iter()
                        .map(|n| if bank == 0 { n.half.0 } else { n.half.1 } - n.width / 2)
                        .collect();
                    assert!(
                        offsets.iter().any(|&o| o.abs() > width / 20),
                        "a bank leaves its line over {length} mm of a {width} mm river"
                    );
                    // The sections where the bank holds its line (the
                    // offset changes by less than half the slope's over
                    // three nodes or more): no two in a row at the same
                    // height.
                    let mut plateaus: Vec<(i64, usize)> = Vec::new();
                    let mut run = 1;
                    for j in 1..nodes.len() {
                        let slope = (along[j] - along[j - 1]) * BANK_SLOPE_PER_MILLE / 2000;
                        if (offsets[j] - offsets[j - 1]).abs() < slope {
                            run += 1;
                        } else {
                            if run >= 3 {
                                plateaus.push((offsets[j - 1], j - 1));
                            }
                            run = 1;
                        }
                    }
                    // Two sections in a row stand at least a step of the
                    // slope apart (a move back to the line may be one
                    // step long).
                    let one_step =
                        BANK_SLOPE_PER_MILLE * (width * WALK_STEP_PER_MILLE / 1000) / 1000;
                    for pair in plateaus.windows(2) {
                        assert!(
                            (pair[1].0 - pair[0].0).abs() > one_step / 2,
                            "two sections in a row at the same width: {plateaus:?}, width {width}"
                        );
                    }
                    levels += plateaus.len();
                }
            }
        }
        assert!(
            long > 5 && levels > long,
            "{long} long courses, {levels} plateaus"
        );
    }

    fn flat_nodes(levels: &[i64]) -> Vec<Node> {
        levels
            .iter()
            .enumerate()
            .map(|(j, &level)| Node {
                p: (i64::try_from(j).expect("small") * 10_000, 0),
                level,
                half: (2000, 2000),
                width: 4000,
                depth: 50,
                ford: false,
                cell: 0,
            })
            .collect()
    }

    #[test]
    fn the_water_settles_on_the_ground_and_never_falls_in_a_step() {
        // The atlas's line falls 10 m over 100 m; the ground dips 30 m
        // under it at the sixth node.
        let mut nodes = flat_nodes(&(0..11).map(|j| 10_000 - j * 100).collect::<Vec<_>>());
        let ground = |p: (i64, i64)| {
            Some(if (50_000..=60_000).contains(&p.0) {
                6_500
            } else {
                20_000
            })
        };
        settle(&mut nodes, &ground);
        let levels: Vec<i64> = nodes.iter().map(|n| n.level).collect();
        assert!(
            levels.windows(2).all(|w| w[0] >= w[1]),
            "never rises: {levels:?}"
        );
        assert!(
            levels[5] <= 6_500 && levels[10] <= 6_500,
            "dives and stays: {levels:?}"
        );
        let fall = 10_000 * STEEPEST_FALL_PER_MILLE / 10_000;
        assert!(
            levels.windows(2).all(|w| w[0] - w[1] <= fall),
            "no step over {fall} cm in 10 m: {levels:?}"
        );
        assert!(levels[0] < 10_000, "the dive starts upstream of the dip");
        // No ground known: the atlas's line stands.
        let mut plain = flat_nodes(&[500, 400, 300]);
        settle(&mut plain, &|_| None);
        assert_eq!(
            plain.iter().map(|n| n.level).collect::<Vec<_>>(),
            [500, 400, 300]
        );
    }

    #[test]
    fn a_tributary_cut_short_meets_its_stem_at_the_stems_level() {
        // Above the stem: the last reach falls to it.
        let mut nodes = flat_nodes(&[900, 800, 700, 600, 500]);
        regrade_end(&mut nodes, 2, 100);
        let levels: Vec<i64> = nodes.iter().map(|n| n.level).collect();
        assert_eq!(levels[..2], [900, 800]);
        assert_eq!(levels[2], 700);
        assert_eq!(levels[4], 100);
        assert!(levels.windows(2).all(|w| w[0] >= w[1]), "{levels:?}");
        // The stem higher than the reach's start: the regrade begins
        // where the course was still above it.
        let mut nodes = flat_nodes(&[900, 800, 700, 600, 500]);
        regrade_end(&mut nodes, 3, 750);
        let levels: Vec<i64> = nodes.iter().map(|n| n.level).collect();
        assert_eq!(levels[..2], [900, 800]);
        assert_eq!(levels[4], 750);
        assert!(levels.windows(2).all(|w| w[0] >= w[1]), "{levels:?}");
        // The stem higher than the whole course: the water stands at its
        // level, never flowing uphill.
        let mut nodes = flat_nodes(&[400, 300, 200]);
        regrade_end(&mut nodes, 1, 1000);
        assert!(nodes.iter().all(|n| n.level == 1000));
    }

    #[test]
    fn the_same_continent_gives_the_same_stems_whatever_the_span() {
        let c = continent(2);
        let whole = shared_stems(2);
        let again = all_stems(c);
        assert_eq!(whole.len(), again.len());
        for (a, b) in whole.iter().zip(&again) {
            assert_eq!(a.nodes, b.nodes);
        }
        // A span of the middle: every point of its stems four cells in
        // from the span's edges is a point of a whole stem.
        let part = stems(c, 32..96, 32..96, None);
        let per = i64::from(region::PER_CELL);
        let inner = |p: (i64, i64)| {
            let cell = |v: i64| v / (CELL_MM * per);
            (36..92).contains(&cell(p.0)) && (36..92).contains(&cell(p.1))
        };
        // By place: a tributary's end and the stem it joins share one.
        let mut known: BTreeMap<(i64, i64), Vec<Node>> = BTreeMap::new();
        for n in whole.iter().flat_map(|w| w.nodes.iter()) {
            known.entry(n.p).or_default().push(*n);
        }
        let mut checked = 0;
        for s in &part {
            for n in s.nodes.iter().filter(|n| inner(n.p)) {
                assert!(
                    known.get(&n.p).is_some_and(|nodes| nodes.contains(n)),
                    "a point of a cut stem is a point of the whole one, banks and all: {n:?}"
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "the middle holds a river");
    }

    #[test]
    fn every_stem_reaches_the_sea_or_the_water_it_joins() {
        let c = continent(1);
        let stems = shared_stems(c.seed);
        for (k, s) in stems.iter().enumerate() {
            let last = s.nodes[s.nodes.len() - 1];
            if let Some(main) = s.joins {
                let (_, d, _) = nearest_on(&stems[main].nodes, last.p).expect("a course");
                assert!(d <= 1000, "stem {k} ends {d} mm off the stem it joins");
            } else {
                let end = s.cells[s.cells.len() - 1];
                let r = c.receiver[end] as usize;
                assert!(
                    c.sea[r] || r == end || c.discharge[r] < RIVER_DISCHARGE,
                    "stem {k} ends at a river cell it does not join"
                );
            }
        }
    }

    #[test]
    fn a_tributary_joins_at_first_contact_and_never_runs_alongside() {
        let c = continent(1);
        let stems = shared_stems(c.seed);
        let mut tributaries = 0;
        for (k, s) in stems.iter().enumerate() {
            let Some(main) = s.joins else {
                continue;
            };
            tributaries += 1;
            let main = &stems[main].nodes;
            let near = Near::new(main);
            let last = s.nodes[s.nodes.len() - 1].p;
            let (_, _, seg) = nearest_on(main, last).expect("a course");
            let width = main[seg].width;
            let widest = main.iter().map(|n| n.width).max().expect("nodes");
            // Within a width of the stem's water only over its way in:
            // the slide (twice up to one and a half widths) or 50 m.
            let way_in = (JOIN_BLEND_M * 1000).max(3 * width);
            let along = along(&s.nodes);
            let end = along[along.len() - 1];
            for (j, n) in s.nodes.iter().enumerate() {
                if end - along[j] <= way_in {
                    continue;
                }
                let Some((_, d, m)) = near.nearest(n.p, widest / 2 + JOIN_REACH_WIDTHS * widest)
                else {
                    continue;
                };
                let w = main[m].width;
                assert!(
                    d > w / 2 + JOIN_REACH_WIDTHS * w,
                    "stem {k} runs {d} mm from a {w} mm stem {} mm before its end",
                    end - along[j]
                );
            }
        }
        assert!(tributaries > 10, "{tributaries} tributaries");
    }

    #[test]
    fn a_crossing_bears_a_ford_of_two_to_three_widths() {
        let c = continent(1);
        let passages = measure::passages(c, measure::DRY_CM);
        let crossings = passages
            .iter()
            .filter(|&&p| p == measure::Passage::Crossing)
            .count();
        let mut whole_bars = 0;
        for s in shared_stems(c.seed) {
            let crossings_here = s
                .cells
                .iter()
                .filter(|&&i| passages[i] == measure::Passage::Crossing)
                .count();
            let (mut run, mut width, mut runs) = (0, 0, Vec::new());
            for (j, pair) in s.nodes.windows(2).enumerate() {
                if pair[0].ford {
                    run += distance(pair[0].p, pair[1].p);
                    width = width.max(pair[0].width);
                } else if run > 0 {
                    runs.push((run, width, j));
                    run = 0;
                }
            }
            let total: i64 = runs.iter().map(|r| r.0).sum();
            assert!(
                total <= 3 * width * i64::try_from(crossings_here).expect("small"),
                "the bars are at most three widths a crossing"
            );
            for (run, width, end) in runs {
                // A bar cut by the stem's end is shorter.
                if end + 1 < s.nodes.len() && run >= 2 * width {
                    whole_bars += 1;
                }
            }
        }
        assert!(
            crossings > 0 && whole_bars > 0,
            "{crossings} crossings, {whole_bars} whole bars"
        );
    }
}
