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
//! 3. its **banks breathe**, each on its own (D36's rule for the valley):
//!    the half-width of each bank wanders ±[`BREATH_PER_MILLE`] over
//!    waves of [`BREATH_WAVE_WIDTHS`] river widths, from a value noise
//!    keyed by the cell and the bank, calm at both ends of a cell's
//!    course so two cells meet without a step; the depth takes what the
//!    width leaves, so the flow is kept;
//! 4. the **ford** at an atlas crossing (`measure::passages` at
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
const SHARPEST_TURN_COS_MILLI: i64 = 500;

/// Each bank breathes this much either way, per mille of its half-width:
/// a fifth, the project lead's figure, so the river's width stays within
/// ±20 % of what its flow gives it.
pub(crate) const BREATH_PER_MILLE: i64 = 200;
/// The breathing's wave, in river widths: a natural river alternates
/// pools and riffles every five to seven widths (Leopold, Wolman and
/// Miller 1964).
const BREATH_WAVE_WIDTHS: i64 = 6;
/// Most waves over one cell's course: the course has 32 points, and a
/// wave needs two.
const BREATH_MOST_WAVES: i64 = 16;
/// The banks hold their lines over this share (per mille) of a cell's
/// course at each end, so the banks of two cells meet without a step and
/// a junction stays calm (the valley's rule).
const BREATH_CALM_PER_MILLE: i64 = 100;

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

/// A tributary's end slides onto the stem it joins (the rounding moved
/// that stem off the node they shared), the slide spread over the
/// tributary's last this many metres so its course does not kink.
const JOIN_BLEND_M: i64 = 50;

/// The ford at an atlas crossing: the bar's length along the course, per
/// mille of the river's width (two to three widths), and how much wider
/// the river spreads over it at the middle, per mille: a ford is a lens,
/// wider than the river around it (D36).
pub(crate) const FORD_BAR_WIDTHS_PER_MILLE: i64 = 2500;
const FORD_LENS_PER_MILLE: i64 = 300;

/// A point of a river's course, in absolute millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Node {
    /// Where, in millimetres from the region grid's origin.
    pub(crate) p: (i64, i64),
    /// The water's level (cm).
    pub(crate) level: i64,
    /// The half-width on the left bank and on the right bank of the
    /// current, once the banks breathe (mm).
    pub(crate) half: (i64, i64),
    /// The width the flow gives the river here, before breathing (mm).
    pub(crate) width: i64,
    /// The mean depth the flow gives it here (cm).
    pub(crate) depth: i64,
    /// On the ford's bar.
    pub(crate) ford: bool,
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
fn course_turns_within(nodes: &[Node], cos_milli: i64) -> bool {
    nodes.windows(3).all(|w| {
        turns_within(
            (w[1].p.0 - w[0].p.0, w[1].p.1 - w[0].p.1),
            (w[2].p.0 - w[1].p.0, w[2].p.1 - w[1].p.1),
            cos_milli,
        )
    })
}

/// A value in -500..=500 along one cell's course for one bank: `waves`
/// lattice points over the course, smoothly interpolated at `f` per
/// mille of the way along it.
fn wave(seed: u64, cell: (i32, i32), bank: u64, waves: i64, f: i64) -> i64 {
    let at = |k: i64| {
        i64::try_from(
            roll(
                seed,
                purpose::PLAN_BANK,
                &[key(i64::from(cell.0)), key(i64::from(cell.1)), bank, key(k)],
            ) % 1001,
        )
        .expect("small")
            - 500
    };
    let u = f.clamp(0, 1000) * waves;
    let (k, t) = (u.div_euclid(1000), u.rem_euclid(1000));
    let s = smooth(t);
    at(k) + (at(k + 1) - at(k)) * s / 1000
}

/// How far along a cell's course the banks breathe freely, per mille:
/// up from nothing over the first [`BREATH_CALM_PER_MILLE`] of the way,
/// down again over the last.
fn breathing_room(f: i64) -> i64 {
    let calm = BREATH_CALM_PER_MILLE;
    if f < calm {
        smooth(f * 1000 / calm)
    } else if f > 1000 - calm {
        smooth((1000 - f) * 1000 / calm)
    } else {
        1000
    }
}

/// The nearest point of the polyline `nodes` to `p`: the point and its
/// distance.
fn nearest_on(nodes: &[Node], p: (i64, i64)) -> Option<((i64, i64), i64)> {
    let mut best: Option<((i64, i64), i64)> = None;
    for pair in nodes.windows(2) {
        let (a, b) = (pair[0].p, pair[1].p);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len2 = dx * dx + dy * dy;
        let t = if len2 == 0 {
            0
        } else {
            ((p.0 - a.0) * dx + (p.1 - a.1) * dy).clamp(0, len2)
        };
        let q = if len2 == 0 {
            a
        } else {
            (a.0 + dx * t / len2, a.1 + dy * t / len2)
        };
        let d = distance(p, q);
        if best.is_none_or(|(_, bd)| d < bd) {
            best = Some((q, d));
        }
    }
    best
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

/// The river stems of `c` whose cells lie in `x` × `y` (continent
/// cells): see the module.
#[allow(clippy::too_many_lines)]
pub(crate) fn stems(c: &Continent, x: std::ops::Range<i32>, y: std::ops::Range<i32>) -> Vec<Stem> {
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

    // Each chain's course, cell by cell, with its levels and widths, the
    // banks breathing within each cell's course.
    let mut out: Vec<Stem> = Vec::with_capacity(chains.len());
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
            let length: i64 = path
                .windows(2)
                .map(|w| distance(mm(w[0]), mm(w[1])))
                .sum::<i64>()
                .max(1);
            let waves = (length / (BREATH_WAVE_WIDTHS * width)).clamp(1, BREATH_MOST_WAVES);
            let steps = i64::try_from(path.len() - 1).expect("small").max(1);
            for (j, &g) in path.iter().enumerate() {
                if j == 0 && k > 0 {
                    continue;
                }
                let j = i64::try_from(j).expect("small");
                let f = j * 1000 / steps;
                let room = breathing_room(f);
                let bank = |side: u64| {
                    let w = wave(seed, (ix, iy), side, waves, f);
                    width / 2 + width / 2 * (w * BREATH_PER_MILLE / 500) * room / 1_000_000
                };
                nodes.push(Node {
                    p: mm(g),
                    level: top - (top - bottom) * j / steps,
                    half: (bank(0), bank(1)),
                    width,
                    depth,
                    ford: false,
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

    // A tributary's end slides onto the stem it joins.
    for k in 0..out.len() {
        let Some(main) = out[k].joins else {
            continue;
        };
        let last = out[k].nodes[out[k].nodes.len() - 1].p;
        let Some((target, _)) = nearest_on(&out[main].nodes, last) else {
            continue;
        };
        let shift = (target.0 - last.0, target.1 - last.1);
        let nodes = &mut out[k].nodes;
        // How far back along the course each node lies, before any moves.
        let mut back = vec![0; nodes.len()];
        for j in (0..nodes.len() - 1).rev() {
            back[j] = back[j + 1] + distance(nodes[j].p, nodes[j + 1].p);
        }
        for (node, back) in nodes.iter_mut().zip(back) {
            let w = smooth(1000 - back * 1000 / (JOIN_BLEND_M * 1000));
            node.p = (node.p.0 + shift.0 * w / 1000, node.p.1 + shift.1 * w / 1000);
        }
    }

    // The fords at the atlas's crossings.
    for stem in &mut out {
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
                spread_ford(&mut stem.nodes, at);
            }
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
        stems(c, 0..side, 0..side)
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
    fn the_width_breathes_within_a_fifth_and_never_holds_still() {
        let c = continent(1);
        let mut moved = false;
        for s in all_stems(&c) {
            for n in &s.nodes {
                if n.ford {
                    continue;
                }
                let width = n.half.0 + n.half.1;
                let (lo, hi) = (
                    n.width * (1000 - BREATH_PER_MILLE) / 1000,
                    n.width * (1000 + BREATH_PER_MILLE) / 1000,
                );
                assert!((lo..=hi).contains(&width), "{width} for {}", n.width);
                if (width - n.width).abs() > n.width / 20 {
                    moved = true;
                }
            }
        }
        assert!(moved, "some bank stands a twentieth off its line");
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
        let part = stems(&c, 32..96, 32..96);
        let per = i64::from(region::PER_CELL);
        let inner = |p: (i64, i64)| {
            let cell = |v: i64| v / (CELL_MM * per);
            (36..92).contains(&cell(p.0)) && (36..92).contains(&cell(p.1))
        };
        let known: std::collections::BTreeSet<(i64, i64)> = whole
            .iter()
            .flat_map(|w| w.nodes.iter().map(|n| n.p))
            .collect();
        let mut checked = 0;
        for s in &part {
            for n in s.nodes.iter().filter(|n| inner(n.p)) {
                assert!(
                    known.contains(&n.p),
                    "a point of a cut stem is a point of the whole one: {n:?}"
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
                let (_, d) = nearest_on(&stems[main].nodes, last.p).expect("a course");
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
