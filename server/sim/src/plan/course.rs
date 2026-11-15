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
//! 1. **rounded** by corner cutting (Chaikin, [`COARSE_SMOOTH`] passes):
//!    the right angle where one cell's course met the next becomes a
//!    bend about a river's width in radius;
//! 2. **refined** below the cell by more midpoint displacement
//!    ([`FINE_DEPTH`] halvings, [`FINE_PER_MILLE`]), keyed by each
//!    segment's two ends, and rounded once more so the detail never
//!    kinks: no turn sharper than [`SHARPEST_TURN_COS_MILLI`] between two
//!    consecutive segments;
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

/// The course below the region's 64 m: halvings of midpoint displacement
/// once the cell's course is rounded (one: its pieces of 8 to 20 m down
/// to 4 to 10 m), and the largest sideways shift of a midpoint, per
/// mille of its segment's length. Smaller than the region's 280: at a
/// few metres a river's line is nearly smooth, and larger shifts drew a
/// jagged bank.
const FINE_DEPTH: u32 = 1;
const FINE_PER_MILLE: i64 = 150;

/// Corner-cutting passes (Chaikin) over the course at the cell's scale
/// before the fine detail, and over the fine course after it. Each pass
/// replaces a corner by two of about half its turn, a quarter of the
/// segments in from it, so the right angle where one cell's course met
/// the next (and the hairpin a jittered node sometimes makes) becomes a
/// bend of about a river's width in radius, turning by a few degrees a
/// piece; the one pass after keeps the fine detail from kinking.
const COARSE_SMOOTH: u32 = 3;
const FINE_SMOOTH: u32 = 1;

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

/// A point the course steps to and nearly straight back from is a
/// spike, dropped before the rounding: the turn's cosine × 1000 at or
/// under this (about 155°). Corner cutting cannot open a corner that
/// has no width, and the region's course makes one now and then at a
/// cell's jittered node.
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

/// One corner-cutting pass (Chaikin): every segment keeps its quarter
/// and three-quarter points, the ends stay.
fn cut_corners(nodes: &[Node]) -> Vec<Node> {
    if nodes.len() < 3 {
        return nodes.to_vec();
    }
    let mut out = Vec::with_capacity(2 * nodes.len());
    out.push(nodes[0]);
    for pair in nodes.windows(2) {
        out.push(between(pair[0], pair[1], 1, 4));
        out.push(between(pair[0], pair[1], 3, 4));
    }
    out.push(nodes[nodes.len() - 1]);
    out
}

/// The course steps to `b` from `a` and nearly straight back towards
/// `c`: a spike ([`SPIKE_COS_MILLI`]).
fn spikes(a: (i64, i64), b: (i64, i64), c: (i64, i64)) -> bool {
    let (u, v) = ((b.0 - a.0, b.1 - a.1), (c.0 - b.0, c.1 - b.1));
    let dot = i128::from(u.0) * i128::from(v.0) + i128::from(u.1) * i128::from(v.1);
    let uu = i128::from(u.0).pow(2) + i128::from(u.1).pow(2);
    let vv = i128::from(v.0).pow(2) + i128::from(v.1).pow(2);
    // cos ≤ c < 0  ⇔  dot < 0 and dot² × 10⁶ ≥ c² × uu × vv.
    dot < 0 && dot * dot * 1_000_000 >= i128::from(SPIKE_COS_MILLI).pow(2) * uu * vv
}

/// The course without its spikes ([`spikes`]) and without pieces
/// shorter than `least` (mm), the ends kept.
fn tidy(nodes: &[Node], least: i64) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::with_capacity(nodes.len());
    for &n in nodes {
        while out.len() >= 2 && spikes(out[out.len() - 2].p, out[out.len() - 1].p, n.p) {
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

/// Midpoint displacement of every segment, `depth` halvings, each
/// midpoint shifted sideways by up to `per_mille` of its segment, keyed
/// by the segment's two ends.
fn refine(seed: u64, nodes: &[Node], depth: u32, per_mille: i64) -> Vec<Node> {
    fn split(seed: u64, a: Node, b: Node, depth: u32, per_mille: i64, out: &mut Vec<Node>) {
        if depth == 0 {
            out.push(b);
            return;
        }
        let (dx, dy) = (b.p.0 - a.p.0, b.p.1 - a.p.1);
        let r = roll(
            seed,
            purpose::PLAN_FINE,
            &[key(a.p.0), key(a.p.1), key(b.p.0), key(b.p.1)],
        );
        let shift = (i64::try_from(r % 2001).expect("small") - 1000) * per_mille / 1000;
        let mut mid = between(a, b, 1, 2);
        mid.p = (mid.p.0 - dy * shift / 1000, mid.p.1 + dx * shift / 1000);
        split(seed, a, mid, depth - 1, per_mille, out);
        split(seed, mid, b, depth - 1, per_mille, out);
    }
    let mut out = Vec::with_capacity(nodes.len() << depth);
    out.push(nodes[0]);
    for pair in nodes.windows(2) {
        split(seed, pair[0], pair[1], depth, per_mille, &mut out);
    }
    out
}

/// Two consecutive segments turn no more than `cos_milli` allows (the
/// cosine of the turn, × 1000).
#[cfg(test)]
fn turns_within(u: (i64, i64), v: (i64, i64), cos_milli: i64) -> bool {
    let dot = i128::from(u.0) * i128::from(v.0) + i128::from(u.1) * i128::from(v.1);
    let uu = i128::from(u.0).pow(2) + i128::from(u.1).pow(2);
    let vv = i128::from(v.0).pow(2) + i128::from(v.1).pow(2);
    if uu == 0 || vv == 0 {
        return true;
    }
    // cos = dot / √(uu vv) ≥ c  ⇔  dot ≥ 0 and dot² × 10⁶ ≥ c² × uu × vv
    // (for c ≥ 0).
    dot >= 0 && dot * dot * 1_000_000 >= i128::from(cos_milli).pow(2) * uu * vv
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
    for (node, &at) in nodes.iter_mut().zip(&along) {
        let (s, t) = (
            usize::try_from(at / step).expect("small").min(n),
            (at % step) * 1000 / step,
        );
        let next = banks[(s + 1).min(n)];
        let mix =
            |a: i64, b: i64| (a * 1000 + (b - a) * t) * BANK_SLOPE_PER_MILLE * step / 1_000_000;
        node.half = (
            node.width / 2 + mix(banks[s].0, next.0),
            node.width / 2 + mix(banks[s].1, next.1),
        );
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
            low = low.min(g);
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
    let widest = stems[main].nodes.iter().map(|n| n.width).max().unwrap_or(0);
    let near = Near::new(&stems[main].nodes);
    let mut cut = None;
    for j in start..stems[k].nodes.len() {
        let p = stems[k].nodes[j].p;
        if let Some((q, d, s)) = near.nearest(p, reach(widest))
            && d <= reach(stems[main].nodes[s].width)
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
        let first = nodes
            .iter()
            .position(|n| n.cell == end)
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

    // Each chain's course, cell by cell, with its levels and widths.
    let mut out: Vec<Stem> = Vec::with_capacity(chains.len());
    let mut courses: BTreeMap<usize, Course> = BTreeMap::new();
    for (cells, end) in &chains {
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
            for (j, &g) in path.iter().enumerate() {
                if j == 0 && k > 0 {
                    continue;
                }
                let j = i64::try_from(j).expect("small");
                nodes.push(Node {
                    p: mm(g),
                    level: top - (top - bottom) * j / steps,
                    half: (width / 2, width / 2),
                    width,
                    depth,
                    ford: false,
                    cell: i,
                });
            }
        }
        nodes = tidy(&nodes, 1);
        for _ in 0..COARSE_SMOOTH {
            nodes = cut_corners(&nodes);
        }
        nodes = refine(seed, &nodes, FINE_DEPTH, FINE_PER_MILLE);
        for _ in 0..FINE_SMOOTH {
            nodes = cut_corners(&nodes);
        }
        nodes = tidy(&nodes, LEAST_PIECE_MM);
        if let Some(ground) = ground {
            settle(&mut nodes, ground);
        }
        let joins = if cells.contains(end) {
            None
        } else {
            owner.get(end).copied()
        };
        out.push(Stem {
            nodes,
            cells: cells.clone(),
            joins,
        });
    }

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
        let mut from = 0;
        while from < stem.nodes.len() {
            let cell = stem.nodes[from].cell;
            let to = stem.nodes[from..]
                .iter()
                .position(|n| n.cell != cell)
                .map_or(stem.nodes.len(), |len| from + len);
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
            from = to;
        }
        for &at in &fords {
            spread_ford(&mut stem.nodes, at);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small continent, as the relief look test's tests use.
    fn continent(seed: u64) -> Continent {
        Continent::with(seed, 128, None)
    }

    fn all_stems(c: &Continent) -> Vec<Stem> {
        let side = i32::try_from(c.grid.side).expect("small");
        stems(c, 0..side, 0..side, None)
    }

    /// The farthest a bank stands in and out of its line, per mille of
    /// the half-width the flow gives it, from the walker's reach.
    fn bank_room() -> (i64, i64) {
        let out = |k: i64| 1000 + k * BANK_SLOPE_PER_MILLE * WALK_STEP_PER_MILLE * 2 / 1000;
        (out(BANK_REACH_STEPS.0), out(BANK_REACH_STEPS.1))
    }

    #[test]
    fn no_turn_is_sharper_than_sixty_degrees() {
        for seed in 1..=3 {
            let c = continent(seed);
            let stems = all_stems(&c);
            assert!(!stems.is_empty(), "seed {seed} has rivers");
            for (k, s) in stems.iter().enumerate() {
                assert!(
                    course_turns_within(&s.nodes, SHARPEST_TURN_COS_MILLI),
                    "seed {seed}, stem {k}"
                );
            }
        }
    }

    #[test]
    fn each_bank_stays_within_the_walkers_reach_and_the_width_within_its_room() {
        let c = continent(1);
        let (least, most) = bank_room();
        for s in all_stems(&c) {
            for n in s.nodes.iter().filter(|n| !n.ford) {
                // A node's own width may differ a little from its cell's
                // (the blend across two cells, the growth past a
                // junction): a twentieth of slack.
                for half in [n.half.0, n.half.1] {
                    let (lo, hi) = (
                        n.width / 2 * (least - 50) / 1000,
                        n.width / 2 * (most + 50) / 1000,
                    );
                    assert!((lo..=hi).contains(&half), "{half} for {}", n.width);
                }
                let total = n.half.0 + n.half.1;
                assert!(
                    (n.width * (NARROWEST_PER_MILLE - 50) / 1000
                        ..=n.width * (WIDEST_PER_MILLE + 50) / 1000)
                        .contains(&total),
                    "{total} wide for {}",
                    n.width
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
        for s in all_stems(&c) {
            let widths = widths_along(&s);
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
        for s in all_stems(&c) {
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

    /// The runs of nodes of one cell along a stem: `(from, to)`.
    fn cell_runs(s: &Stem) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut from = 0;
        while from < s.nodes.len() {
            let cell = s.nodes[from].cell;
            let to = s.nodes[from..]
                .iter()
                .position(|n| n.cell != cell)
                .map_or(s.nodes.len(), |len| from + len);
            out.push((from, to));
            from = to;
        }
        out
    }

    #[test]
    fn the_banks_hold_their_lines_at_both_ends_of_a_cells_course() {
        let c = continent(1);
        let mut checked = 0;
        for s in all_stems(&c) {
            for (from, to) in cell_runs(&s) {
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
        for s in all_stems(&c) {
            for (from, to) in cell_runs(&s) {
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
                    let mut plateaus: Vec<i64> = Vec::new();
                    let mut run = 1;
                    for j in 1..nodes.len() {
                        let slope = (along[j] - along[j - 1]) * BANK_SLOPE_PER_MILLE / 2000;
                        if (offsets[j] - offsets[j - 1]).abs() < slope {
                            run += 1;
                        } else {
                            if run >= 3 {
                                plateaus.push(offsets[j - 1]);
                            }
                            run = 1;
                        }
                    }
                    for pair in plateaus.windows(2) {
                        assert!(
                            (pair[1] - pair[0]).abs() > width / 20,
                            "two sections in a row at the same width: {plateaus:?}"
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
        let whole = all_stems(&c);
        let again = all_stems(&c);
        assert_eq!(whole.len(), again.len());
        for (a, b) in whole.iter().zip(&again) {
            assert_eq!(a.nodes, b.nodes);
        }
        // A span of the middle: every point of its stems four cells in
        // from the span's edges is a point of a whole stem.
        let part = stems(&c, 32..96, 32..96, None);
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
        let stems = all_stems(&c);
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
        let stems = all_stems(&c);
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
        let passages = measure::passages(&c, measure::DRY_CM);
        let crossings = passages
            .iter()
            .filter(|&&p| p == measure::Passage::Crossing)
            .count();
        let mut whole_bars = 0;
        for s in all_stems(&c) {
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
