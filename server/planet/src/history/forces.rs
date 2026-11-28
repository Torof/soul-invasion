//! The forces on the plates and their turns: slab pull, trench suction,
//! ridge push, the mantle's drag, the weight of a collision's pile; what the plates
//! do at their boundaries (`look`).

use super::*;

/// A slab younger than this, millions of years, is too light to pull.
pub const SLAB_YOUNG_MY: i64 = 10;
/// The most an old slab pulls, per mille of the pull at
/// [`Settings::slab_age_my`].
pub(super) const SLAB_MOST: i64 = 1_500;
/// A plate keeps this much of its last motion each step, per mille (the
/// rest comes from the forces of the step).
pub(super) const INERTIA: i64 = 400;
/// Convergence a trench needs to begin where no slab hangs yet, and
/// divergence a ridge needs, µm a year. A slab that hangs needs none: it
/// pulls by its weight, and is lost only where the plates part this fast.
pub(super) const FORCE_LEAST_UM: i64 = 2_000;

/// The section of a continent's crust, km² (its thickness times the
/// length of it gone under, for each km of front), whose buoyancy holds
/// a slab of full weight: crust of 2.8 in a mantle of 3.3 (the
/// densities of [`ISOSTASY_M`]) floats with 500 kg a m³; a section of
/// 14,000 km² of it on a slab dipping 45° pushes back 4.9 × 10¹³ N for
/// each m of trench, the pull of an old slab (Turcotte and Schubert,
/// *Geodynamics*, 6.21). That is 400 km of a whole crust, 700 km of a
/// margin stretched to 20 km: what the Earth's collisions swallowed
/// before their slabs broke off (India's thinned margin, some 10 to 25
/// My after the first contact; van Hunen and Allen 2011).
pub(super) const CRUST_FLOATS_KM2: i64 = 14_000;

/// How hard a slab of this age, millions of years, pulls, per mille of
/// the pull at `full_my`: the floor's weight in the mantle. A floor
/// younger than [`SLAB_YOUNG_MY`] is too light to pull.
pub(super) fn slab_weight(age: i64, full_my: i64) -> i64 {
    ((age - SLAB_YOUNG_MY) * 1000 / (full_my - SLAB_YOUNG_MY).max(1)).clamp(0, SLAB_MOST)
}

/// A cell where a force acts on its plate, the force's direction and its
/// weight, per mille.
pub(super) type Spot = (usize, u8, V3, i64);

/// A cell on a collision's front: its plate, the plate it meets, the
/// normal toward it, and how fast they close, µm a year.
pub(super) type Front = (usize, u8, u8, V3, i64);

/// A side of a cell on a collision's front, where two plates' continents
/// touch: the cell, its plate, the plate it meets, the normal toward it,
/// the weight of the pile there, ‰ ([`pile_weight`]), and whether the two
/// are in a collision (continents that had parted and met again: plates
/// a rift has cut and that press on each other again are held apart by
/// their pile, and stay two).
pub(super) type Pile = (usize, u8, u8, V3, i64, bool);

/// A cell on a suture, the front of a collision that has closed: the
/// cell, its plate, the plate it is joined to, the normal toward it, and
/// what the section of crust that joins them there holds of a pull
/// ([`holds`], the weaker of its two sides).
pub(super) type Suture = (usize, u8, u8, V3, i64);

/// How hard thickened crust pushes back on the plates that thicken it, ‰
/// of the push of a crust twice a continent's thickness: a column of
/// crust `thick` (tenths of a km) stands higher and reaches deeper than
/// the plain continent beside it, and its weight pushes outward with the
/// difference of the squares of their thicknesses (the weight of a
/// column grows with its height, and so does the face it pushes on). To
/// shorten the crust further the plates must push harder than that.
pub(super) fn pile_weight(thick: i64) -> i64 {
    let whole = i64::from(CONTINENT_THICK);
    let more = (thick - whole).max(0);
    more * (2 * whole + more) * 1000 / (3 * whole * whole)
}

/// The piles between two plates: the pair, the torque their push puts on
/// the second (and, reversed, on the first), each side's place and
/// normal from the first toward the second, and whether the two are in
/// a collision.
type PileFront = ((usize, usize), V3, Vec<(V3, V3)>, bool);

/// The suture between two plates: the pair, each of its cells' place and
/// normal from the first toward the second, and what its section holds.
type Seam = ((usize, usize), Vec<(V3, V3)>, i64);

/// How a pair of plates is held by the crust that still joins them: a
/// matrix on their relative turn.
type Hold = [[i128; 3]; 3];

/// Crust that still joins two plates a rift has cut (version 12): the
/// two plates, where it is, and how well it holds, ‰: what it would not
/// let through over what it would, shared among all the cells of its
/// trough (as the crust gives way along the trough, less and less holds).
/// It holds as well against closing: where two plates a rift has cut
/// press on each other, their crust meets as in any collision.
pub(super) type Bridge = (u8, u8, V3, i64);

/// The part of `v` tangent to the sphere at `x`.
pub(super) fn tangent(x: V3, v: V3) -> V3 {
    sub(v, scale(x, dot(v, x)))
}

/// The speed a plate reaches against a mantle whose drag grows with
/// speed: `u` the speed the forces alone would give against a plain drag,
/// `v0` the speed at which the drag has doubled, both µm a year.
pub(super) fn dragged(u: i64, v0: i64) -> i64 {
    if v0 <= 0 || u <= 0 {
        return u.max(0);
    }
    let root = (i128::from(v0) * i128::from(v0) + 4 * i128::from(u) * i128::from(v0))
        .cast_unsigned()
        .isqrt() as i64;
    (root - v0) / 2
}

/// The plates' turns at a step, and which plates move as one.
pub(super) struct Drive {
    /// Each plate's turn, µm a year about its pole (a vector along it).
    pub(super) omega: Vec<V3>,
    /// Each plate's group, named by its least id: plates whose continents
    /// have not parted move as one while the forces press them together.
    pub(super) group: [u8; MAXP],
    /// The pairs of plates a collision has made one: its pile holds all
    /// the forces press them together with, or (the third: true) its
    /// suture holds all they would part with; they turn as one.
    pub(super) locked: Vec<(u8, u8, bool)>,
}

/// The forces at the plates' edges, read at the last step: the slabs
/// (on the plate that sinks), their suction (on the plate riding over
/// them), the ridges, and the collisions' piles.
#[derive(Clone, Copy)]
pub(super) struct Edges<'a> {
    pub(super) slab: &'a [Spot],
    pub(super) suction: &'a [Spot],
    pub(super) ridge: &'a [Spot],
    pub(super) piles: &'a [Pile],
    pub(super) sutures: &'a [Suture],
    pub(super) bridges: &'a [Bridge],
}

/// The plates' turns from the forces on them. Plates whose continents
/// touch and have not parted (`held`) are held together while the forces
/// press them together: a boundary inside a continent exists only once
/// it opens (the supercontinent breaks only by opening).
pub(super) fn drive(
    sphere: &Sphere,
    s: &Settings,
    plate: &[u8],
    rock: &[Rock],
    heat: &[i32],
    edges: Edges,
    prev: Option<&[V3]>,
    held: &[[bool; MAXP]],
) -> Drive {
    let mut torque = vec![[0_i64; 3]; MAXP];
    let mut cells = [0_i64; MAXP];
    let mut cont = [0_i64; MAXP];
    let (cell_tq, edge_tq) = torques(sphere, s, rock, heat, edges);
    for c in 0..CELLS {
        let p = plate[c] as usize;
        cells[p] += 1;
        if rock[c].class != OCEAN {
            cont[p] += 1;
        }
        torque[p] = add(torque[p], cell_tq[c]);
    }
    for &(_, p, tq) in &edge_tq {
        torque[p as usize] = add(torque[p as usize], tq);
    }
    drive_from(
        sphere,
        s,
        plate,
        rock,
        edges,
        prev,
        held,
        (torque, cells, cont),
    )
}

/// The forces on the plates, as torques: each cell's, on its own plate
/// (the mantle's heat under a continent), and the edges' (a cell, the
/// plate it acts on, the torque): slabs, their suction, ridges.
pub(super) fn torques(
    sphere: &Sphere,
    s: &Settings,
    rock: &[Rock],
    heat: &[i32],
    edges: Edges,
) -> (Vec<V3>, Vec<(u32, u8, V3)>) {
    let mut cell_tq = vec![[0_i64; 3]; CELLS];
    let mut edge_tq = Vec::new();
    for c in 0..CELLS {
        let x = sphere.centre[c];
        let mut f = [0_i64; 3];
        if rock[c].class != OCEAN && s.pole_flight != 0 {
            let z = x[2];
            let across = (ONE * ONE - z * z).max(0).isqrt();
            let m = s.pole_flight * 1000 * 2 * z / ONE * across / ONE;
            let dir = normalize(tangent(x, [0, 0, -ONE]));
            f = add(f, scale(dir, m));
        }
        // The mantle carries a continent down the slope of its heat: away
        // from where it is warm (under a large continent), toward where
        // it is cold (where slabs sink).
        if rock[c].class != OCEAN && s.heat_push != 0 {
            let mut slope = [0_i64; 3];
            for &(n, km) in &sphere.links[c] {
                let d = i64::from(heat[n as usize]) - i64::from(heat[c]);
                if d != 0 {
                    let dir = normalize(tangent(x, sub(sphere.centre[n as usize], x)));
                    slope = add(slope, scale(dir, d * 1000 / i64::from(km).max(1)));
                }
            }
            let n = sphere.links[c].len() as i64 * 1000;
            f = add(f, slope.map(|v| -v * 2 * HEAT_LID_KM * s.heat_push / n));
        }
        cell_tq[c] = cross(x, f);
    }
    for (spots, strength) in [
        (edges.slab, s.slab_pull),
        (edges.suction, s.slab_suction),
        (edges.ridge, s.ridge_push),
    ] {
        for &(c, p, dir, weight) in spots {
            let f = scale(dir, strength * weight);
            edge_tq.push((c as u32, p, cross(sphere.centre[c], f)));
        }
    }
    (cell_tq, edge_tq)
}

/// A load on a plate: where, its torque, the drag it adds, and whether
/// it is a cell of continent.
pub(super) type Load = (V3, V3, i64, bool);

/// How fast the two sides of a line across a plate would part if nothing
/// held them together, µm a year (negative: pressed together): each
/// side turns as the forces on it alone would turn it, against the
/// mantle's drag and no faster than a plate may go (`top`), as a plate
/// does. The line is the great circle through
/// `x` whose pole is `n`. None where a side holds no continent beyond
/// the stretched zone: a rift tears a continent, not its shore. With it,
/// the drag the two sides share (the product of their drags over their
/// sum): the tension across the line is the speed times that drag, the
/// force that must cross the line for the two sides to go on as one. A
/// small side is carried along for little: the tension across a crumb
/// is small however fast its two sides would part.
pub(super) fn parting(loads: &[Load], x: V3, n: V3, (v0, top): (i64, i64)) -> Option<(i64, i64)> {
    let band = sin(i64::from(STRETCH_KM) * ONE / RADIUS_KM);
    let mut tq = [[0_i64; 3]; 2];
    let mut rs = [0_i64; 2];
    let mut far = [false; 2];
    for &(at, t, r, cont) in loads {
        let side = dot(at, n);
        let k = usize::from(side > 0);
        tq[k] = add(tq[k], t);
        rs[k] += r;
        far[k] |= cont && side.abs() > band;
    }
    if !far[0] || !far[1] {
        return None;
    }
    let w = [0, 1].map(|k| {
        let w = tq[k].map(|v| v * 1000 / rs[k].max(1));
        let u = norm(w);
        if u > 0 {
            w.map(|v| v * dragged(u, v0).min(top) / u)
        } else {
            w
        }
    });
    let shared =
        (i128::from(rs[0]) * i128::from(rs[1]) / i128::from((rs[0] + rs[1]).max(1))) as i64;
    Some((dot(sub(cross(w[1], x), cross(w[0], x)), n), shared))
}

/// The plates' turns from the torques on them (see [`drive`]).
pub(super) fn drive_from(
    sphere: &Sphere,
    s: &Settings,
    plate: &[u8],
    rock: &[Rock],
    edges: Edges,
    prev: Option<&[V3]>,
    held: &[[bool; MAXP]],
    (torque, cells, cont): (Vec<V3>, [i64; MAXP], [i64; MAXP]),
) -> Drive {
    let resist: Vec<i64> = (0..MAXP)
        .map(|p| cells[p] * 1000 + s.continent_drag * cont[p])
        .collect();
    let mut samples: Vec<(usize, usize)> = Vec::new();
    for c in 0..CELLS {
        if rock[c].class != CONTINENT {
            continue;
        }
        let a = plate[c] as usize;
        for &(n, _) in &sphere.links[c] {
            let n = n as usize;
            let b = plate[n] as usize;
            if b != a && rock[n].class == CONTINENT && held[a][b] {
                samples.push((c, n));
            }
        }
    }
    let mut up: [u8; MAXP] = std::array::from_fn(|k| k as u8);
    let turns = |up: &[u8; MAXP]| {
        let mut tq = vec![[0_i64; 3]; MAXP];
        let mut rs = [0_i64; MAXP];
        for p in 0..MAXP {
            let r = root(up, p as u8) as usize;
            tq[r] = add(tq[r], torque[p]);
            rs[r] += resist[p];
        }
        let w: Vec<V3> = (0..MAXP)
            .map(|r| {
                if rs[r] > 0 {
                    tq[r].map(|v| v * 1000 / rs[r])
                } else {
                    [0; 3]
                }
            })
            .collect();
        (w, rs)
    };
    let (mut w, mut rs) = turns(&up);
    loop {
        let mut close = vec![0_i64; MAXP * MAXP];
        for &(c, n) in &samples {
            let (ra, rb) = (root(&up, plate[c]) as usize, root(&up, plate[n]) as usize);
            if ra == rb {
                continue;
            }
            let x = sphere.centre[c];
            let rel = sub(cross(w[ra], x), cross(w[rb], x));
            close[ra * MAXP + rb] += dot(rel, normalize(tangent(x, sub(sphere.centre[n], x))));
        }
        let mut merged = false;
        for a in 0..MAXP {
            for b in a + 1..MAXP {
                if close[a * MAXP + b] + close[b * MAXP + a] > 0
                    && root(&up, a as u8) == a as u8
                    && root(&up, b as u8) == b as u8
                {
                    up[b] = a as u8;
                    merged = true;
                }
            }
        }
        if !merged {
            break;
        }
        (w, rs) = turns(&up);
    }
    // A collision's pile pushes back on the two plates that thicken it
    // (`pile_weight`), against their closing only: thick crust does not
    // pull. While the forces press harder than the pile pushes back, the
    // plates go on closing, slower, and the pile grows. Once the pile
    // holds all they press with, nothing moves the two plates against
    // each other across their front: one balance of forces turns them
    // both, as one. The closing is read on the speeds the plates reach
    // against the mantle's drag (`dragged`), as they will turn (version
    // 19; until then on the turns before the drag, which slows a fast
    // plate more than a slow one: a pile that held less than the forces
    // pressed with could send a small fast plate back from the large
    // slow one it was closing on, and the two parted. Most collisions of
    // two true continents ended so).
    let (v0, top) = (s.mantle_drag_mm * 1000, s.speed_mm.1 * 1000);
    let alone = |w: V3| {
        let u = norm(w);
        if u > 0 {
            w.map(|v| v * dragged(u, v0).min(top) / u)
        } else {
            w
        }
    };
    let before = up;
    let mut back: Vec<(usize, usize, V3, V3)> = Vec::new();
    loop {
        back.clear();
        let mut fronts: Vec<PileFront> = Vec::new();
        for &(c, p, o, normal, weight, met) in edges.piles {
            let (mut a, mut b) = (root(&up, p) as usize, root(&up, o) as usize);
            if a == b {
                continue;
            }
            let mut n = normal;
            if a > b {
                (a, b) = (b, a);
                n = n.map(|v| -v);
            }
            let x = sphere.centre[c];
            let at = fronts
                .iter()
                .position(|f| f.0 == (a, b))
                .unwrap_or_else(|| {
                    fronts.push(((a, b), [0; 3], Vec::new(), false));
                    fronts.len() - 1
                });
            fronts[at].1 = add(fronts[at].1, cross(x, scale(n, s.pile_push * weight)));
            fronts[at].2.push((x, n));
            fronts[at].3 |= met;
        }
        let mut merged = false;
        for ((a, b), tq, sides, met) in fronts {
            let closing = |wa: V3, wb: V3| -> i64 {
                sides
                    .iter()
                    .map(|&(x, n)| dot(sub(cross(wa, x), cross(wb, x)), n))
                    .sum()
            };
            let da = tq.map(|v| -v * 1000 / rs[a].max(1));
            let db = tq.map(|v| v * 1000 / rs[b].max(1));
            let pressed = closing(alone(w[a]), alone(w[b]));
            let left = closing(alone(add(w[a], da)), alone(add(w[b], db)));
            let held = pressed - left;
            if pressed <= 0 || held <= 0 {
                continue;
            }
            if left > 0 {
                back.push((a, b, da, db));
            } else if met {
                let (ra, rb) = (root(&up, a as u8), root(&up, b as u8));
                if ra != rb {
                    up[ra.max(rb) as usize] = ra.min(rb);
                }
                merged = true;
            } else {
                let part = |d: V3| {
                    d.map(|v| (i128::from(v) * i128::from(pressed) / i128::from(held)) as i64)
                };
                back.push((a, b, part(da), part(db)));
            }
        }
        if !merged {
            break;
        }
        (w, rs) = turns(&up);
    }
    // A suture holds (version 19). Where a collision has closed, the
    // crust of one continent lies under and on the other's: one crust
    // joins the two plates across their front, and they part again only
    // by tearing it, as a continent is torn by a rift. So the measure is
    // the rifts' own (`parting`, `tension`): each of the two turns as the
    // forces on it alone would turn it; where they would part, the force
    // that must cross the suture for them to go on as one is that speed
    // times the drag they share, and the suture holds what its section
    // holds (`holds`: young, warm, seamed crust, a tenth of a shield's,
    // by its thickness). While it holds they turn as one, and are one
    // plate: a long front holds, two continents that touch by a point
    // tear apart. Nothing holds them against closing: pressed, they go
    // on closing against their pile. Until version 19 nothing held two
    // continents that had met: they parted at the first pull, and no
    // continent was ever made of two.
    let piled = up;
    loop {
        let mut seams: Vec<Seam> = Vec::new();
        for &(c, p, o, normal, holds) in edges.sutures {
            let (mut a, mut b) = (root(&up, p) as usize, root(&up, o) as usize);
            if a == b {
                continue;
            }
            let mut n = normal;
            if a > b {
                (a, b) = (b, a);
                n = n.map(|v| -v);
            }
            let at = seams.iter().position(|f| f.0 == (a, b)).unwrap_or_else(|| {
                seams.push(((a, b), Vec::new(), 0));
                seams.len() - 1
            });
            seams[at].1.push((sphere.centre[c], n));
            seams[at].2 += holds;
        }
        let mut merged = false;
        for ((a, b), sides, section) in seams {
            let (wa, wb) = (alone(w[a]), alone(w[b]));
            let parting = -sides
                .iter()
                .map(|&(x, n)| dot(sub(cross(wa, x), cross(wb, x)), n))
                .sum::<i64>()
                / sides.len() as i64;
            let shared =
                (i128::from(rs[a]) * i128::from(rs[b]) / i128::from((rs[a] + rs[b]).max(1))) as i64;
            if parting > 0 && tension(parting, shared, section, s.crust_holds) < 1000 {
                let (ra, rb) = (root(&up, a as u8), root(&up, b as u8));
                if ra != rb {
                    up[ra.max(rb) as usize] = ra.min(rb);
                    merged = true;
                }
            }
        }
        if !merged {
            break;
        }
        (w, rs) = turns(&up);
    }
    back.retain(|&(a, b, ..)| root(&up, a as u8) != root(&up, b as u8));
    let now_one = |was: &[u8; MAXP], p: u8, o: u8| {
        root(&up, p) == root(&up, o) && root(was, p) != root(was, o)
    };
    let mut locked: Vec<(u8, u8, bool)> = edges
        .piles
        .iter()
        .filter(|f| now_one(&before, f.1, f.2))
        .map(|f| (f.1.min(f.2), f.1.max(f.2), false))
        .chain(
            edges
                .sutures
                .iter()
                .filter(|f| now_one(&piled, f.1, f.2))
                .map(|f| (f.1.min(f.2), f.1.max(f.2), true)),
        )
        .collect();
    locked.sort_unstable();
    locked.dedup_by_key(|x| (x.0, x.1));
    for &(a, b, da, db) in &back {
        let (a, b) = (root(&up, a as u8) as usize, root(&up, b as u8) as usize);
        w[a] = add(w[a], da);
        w[b] = add(w[b], db);
    }
    // The crust that still joins two plates a rift has cut holds them
    // there: it lets through its weakness' share of their parting at
    // each of its cells, and leaves them free to turn about it (the
    // ocean opens like scissors, away from the crust that holds).
    let mut hold: Vec<((usize, usize), Hold)> = Vec::new();
    for &(p, o, x, k) in edges.bridges {
        let (ra, rb) = (root(&up, p) as usize, root(&up, o) as usize);
        if ra == rb {
            continue;
        }
        let key = (ra.min(rb), ra.max(rb));
        let at = hold.iter().position(|h| h.0 == key).unwrap_or_else(|| {
            hold.push((key, [[0; 3]; 3]));
            hold.len() - 1
        });
        for i in 0..3 {
            for j in 0..3 {
                let same = if i == j { ONE } else { 0 };
                hold[at].1[i][j] += i128::from(k) * i128::from(same - x[i] * x[j] / ONE);
            }
        }
    }
    for &((a, b), m) in &hold {
        let m: Hold = std::array::from_fn(|i| {
            std::array::from_fn(|j| m[i][j] / i128::from(ONE) + if i == j { 1000 } else { 0 })
        });
        let det3 = |c: &Hold| {
            c[0][0] * (c[1][1] * c[2][2] - c[1][2] * c[2][1])
                - c[0][1] * (c[1][0] * c[2][2] - c[1][2] * c[2][0])
                + c[0][2] * (c[1][0] * c[2][1] - c[1][1] * c[2][0])
        };
        let det = det3(&m);
        if det == 0 {
            continue;
        }
        let rel = sub(w[a], w[b]);
        let through: V3 = std::array::from_fn(|k| {
            let mut c = m;
            for i in 0..3 {
                c[i][k] = i128::from(rel[i]) * 1000;
            }
            (det3(&c) / det) as i64
        });
        let held_back = sub(rel, through);
        let total = (rs[a] + rs[b]).max(1);
        w[a] = sub(w[a], held_back.map(|v| v * rs[b] / total));
        w[b] = add(w[b], held_back.map(|v| v * rs[a] / total));
    }
    let mut least = [u8::MAX; MAXP];
    for p in 0..MAXP {
        let r = root(&up, p as u8) as usize;
        least[r] = least[r].min(p as u8);
    }
    let group: [u8; MAXP] = std::array::from_fn(|p| least[root(&up, p as u8) as usize]);
    let mut pv = vec![[0_i64; 3]; MAXP];
    let mut pr = [0_i64; MAXP];
    if let Some(prev) = prev {
        for p in 0..MAXP {
            let r = root(&up, p as u8) as usize;
            pv[r] = add(pv[r], prev[p].map(|v| v * resist[p] / 1000));
            pr[r] += resist[p];
        }
    }
    let omega = (0..MAXP)
        .map(|p| {
            if cells[p] == 0 {
                return [0; 3];
            }
            let r = root(&up, p as u8) as usize;
            let u = norm(w[r]);
            let mut x = if u > 0 {
                w[r].map(|v| v * dragged(u, v0) / u)
            } else {
                w[r]
            };
            if prev.is_some() && pr[r] > 0 {
                let q = pv[r].map(|v| v * 1000 / pr[r]);
                x = [0, 1, 2].map(|k| (q[k] * INERTIA + x[k] * (1000 - INERTIA)) / 1000);
            }
            let speed = norm(x);
            if speed > top {
                x = x.map(|v| v * top / speed);
            }
            x
        })
        .collect();
    Drive {
        omega,
        group,
        locked,
    }
}

/// What the plates do at a step, read from their relative motion: each
/// cell's boundary kind and margin, the slabs, the ridges and the
/// collisions' fronts (whose forces drive the next step). Marks the
/// margins the crust takes.
pub(super) struct Look {
    pub(super) kind: Vec<u8>,
    pub(super) margin: Vec<u8>,
    pub(super) slab: Vec<Spot>,
    pub(super) suction: Vec<Spot>,
    pub(super) ridge: Vec<Spot>,
    pub(super) front: Vec<Front>,
    /// The cells of floor that sink now: a slab hangs under each.
    pub(super) hung: Vec<bool>,
    /// Whose slab hangs under each of them, and how hard it pulls, ‰.
    pub(super) slab_of: Vec<u8>,
    pub(super) slab_left: Vec<i16>,
}

/// `hung`: the cells under which a slab hung at the step before (version
/// 14). A trench is a state, not only a motion: where a slab hangs, or
/// hung a cell away (the trench moved, or the tear runs on along the
/// boundary), the floor goes on sinking and its slab pulls by its weight
/// however slowly the plates close; the slab is lost where they part.
///
/// **A slab does not let go when the continents meet** (version 18;
/// until then the pull ended with the last cell of floor, so nothing
/// pressed two continents on once they touched, and two collisions in
/// three lasted one step). The slab hangs from the edge of the continent
/// that followed its floor down, and goes on pulling that plate under
/// the other one. What ends it is the crust itself: a continent's crust
/// is light, and each km of it dragged down floats against the slab
/// ([`CRUST_FLOATS_KM2`]). The pull left is the slab's weight less the
/// buoyancy of the crust dragged under since the continents met; when
/// nothing is left the slab has broken off (Davies and von Blanckenburg
/// 1995), and only the other forces on the two plates press them on,
/// against their pile.
pub(super) fn look(
    sphere: &Sphere,
    plate: &[u8],
    group: &[u8; MAXP],
    rock: &mut [Rock],
    omega: &[V3],
    event: &[u8],
    (hung, slab_of, slab_left): (&[bool], &[u8], &[i16]),
    (now_my, full_my, step_my): (u16, i64, i64),
) -> Look {
    let mut hangs = vec![false; CELLS];
    let mut hangs_of = vec![NONE; CELLS];
    let mut left = vec![0_i16; CELLS];
    let floor = floor_born(plate, rock);
    let mut kind = vec![0_u8; CELLS];
    let mut margin = vec![Margin::Inland as u8; CELLS];
    let mut slab = Vec::new();
    let mut suction = Vec::new();
    let mut ridge = Vec::new();
    let mut front = Vec::new();
    let mut active = vec![false; CELLS];
    for c in 0..CELLS {
        let mine = plate[c];
        let x = sphere.centre[c];
        let mut tally: [(u8, u8); 8] = [(NONE, 0); 8];
        let mut ocean_near = false;
        for &(n, _) in &sphere.links[c] {
            let n = n as usize;
            ocean_near |= rock[n].class == OCEAN;
            let o = plate[n];
            if group[o as usize] == group[mine as usize] {
                continue;
            }
            if let Some(t) = tally.iter_mut().find(|t| t.0 == o || t.0 == NONE) {
                t.0 = o;
                t.1 += 1;
            }
        }
        let best = tally
            .iter()
            .filter(|t| t.0 != NONE)
            .max_by_key(|t| (t.1, std::cmp::Reverse(t.0)));
        let mine_cont = rock[c].class != OCEAN;
        let mut arc = false;
        if let Some(&(other, count)) = best {
            let mut dir = [0_i64; 3];
            let mut cont = 0_i64;
            for &(n, _) in &sphere.links[c] {
                let n = n as usize;
                if plate[n] == other {
                    dir = add(dir, sub(sphere.centre[n], x));
                    cont += i64::from(rock[n].class != OCEAN);
                }
            }
            let count = i64::from(count);
            let normal = normalize(tangent(x, dir));
            let rel = sub(
                cross(omega[mine as usize], x),
                cross(omega[other as usize], x),
            );
            let closing = dot(rel, normal);
            let slide = norm(sub(rel, scale(normal, closing)));
            let theirs_cont = 2 * cont >= count;
            let mine_up = (floor[mine as usize], mine) < (floor[other as usize], other);
            let k = classify(
                closing / 1000,
                slide / 1000,
                mine_cont,
                theirs_cont,
                mine_up,
            );
            kind[c] = k as u8 + 1;
            arc = k == Kind::Arc;
            if k == Kind::Collision {
                front.push((c, mine, other, normal, closing));
            }
            let begins = closing >= FORCE_LEAST_UM && 2 * closing >= slide;
            let goes_on = closing > -FORCE_LEAST_UM
                && (hung[c] || sphere.links[c].iter().any(|&(n, _)| hung[n as usize]));
            if !mine_cont && (begins || goes_on) && (theirs_cont || !mine_up) {
                hangs[c] = true;
                if k == Kind::Transform {
                    // A trench the plates hardly close on is a trench.
                    kind[c] = Kind::Trench as u8 + 1;
                }
                // An old slab is heavy: it pulls its plate, and draws the
                // plate riding over it toward the trench.
                let age = i64::from(rock[c].born_my.saturating_sub(now_my));
                let weight = slab_weight(age, full_my);
                hangs_of[c] = mine;
                left[c] = weight as i16;
                if weight > 0 {
                    slab.push((c, mine, normal, weight));
                    suction.push((c, other, normal.map(|v| -v), weight));
                }
            }
            if mine_cont && theirs_cont && closing > -FORCE_LEAST_UM {
                // The slab of this plate that hung here, or a cell
                // away, still hangs from this continent's edge: less the
                // crust it drags under in this step.
                let had = std::iter::once(c)
                    .chain(sphere.links[c].iter().map(|&(n, _)| n as usize))
                    .filter(|&n| hung[n] && slab_of[n] == mine)
                    .map(|n| i64::from(slab_left[n]))
                    .max();
                let under = closing.max(0) * step_my / 1000 * i64::from(rock[c].thick) / 10;
                let weight = had.map_or(0, |w| w - 1000 * under / CRUST_FLOATS_KM2);
                if weight > 0 {
                    hangs[c] = true;
                    hangs_of[c] = mine;
                    left[c] = weight as i16;
                    slab.push((c, mine, normal, weight));
                    suction.push((c, other, normal.map(|v| -v), weight));
                }
            }
            if !mine_cont && closing <= -FORCE_LEAST_UM && -2 * closing >= slide {
                ridge.push((c, mine, normal.map(|v| -v), 1000));
            }
        }
        if !mine_cont {
            continue;
        }
        margin[c] = if event[c] != Margin::Inland as u8 {
            event[c]
        } else if arc {
            active[c] = true;
            Margin::Active as u8
        } else if ocean_near {
            rock[c].margins |= PASSIVE;
            Margin::Passive as u8
        } else {
            Margin::Inland as u8
        };
    }
    for c in 0..CELLS {
        if active[c] {
            rock[c].margins |= ACTIVE;
        }
    }
    Look {
        kind,
        margin,
        slab,
        suction,
        ridge,
        front,
        hung: hangs,
        slab_of: hangs_of,
        slab_left: left,
    }
}

/// The pairs that may be held together: continents that have not
/// parted, across a boundary that has never opened. A boundary that has
/// opened once, or that a rift has cut, is a plate boundary for good
/// (version 12): its two plates are never one again but by a weld.
pub(super) fn held(
    parted: &[[bool; MAXP]; MAXP],
    broken: &[[bool; MAXP]; MAXP],
) -> Vec<[bool; MAXP]> {
    (0..MAXP)
        .map(|a| std::array::from_fn(|b| !parted[a][b] && !broken[a][b]))
        .collect()
}

/// Step 12. What the plates do now, read on the grid's view of the
/// pieces: the kinds of boundaries, the margins.
pub(super) fn look_at(world: &mut World) {
    let s = world.s;
    let sphere = world.sphere;
    let land = &mut world.land;
    let top = &mut world.top;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let omega = &mut world.omega;
    let group = &mut world.group;
    let event = &mut world.event;
    let now_my = world.step.now_my;
    let seen = &mut world.step.seen;
    let margin_of = &mut world.step.margin_of;
    // 12. What the plates do now, and the forces for the next step:
    // read on the grid's view of the pieces.
    paint(&*land, &*top, &mut *plate, &mut *rock);
    for q in 0..CELLS {
        event[q] = if top[q] == NO_BIT {
            Margin::Inland as u8
        } else {
            land.cells[top[q] as usize].event
        };
    }
    *seen = look(
        sphere,
        &*plate,
        &*group,
        &mut *rock,
        &*omega,
        &*event,
        (&world.hung, &world.slab_of, &world.slab_left),
        (now_my, s.slab_age_my, s.step_my),
    );
    world.hung.clone_from(&seen.hung);
    world.slab_of.clone_from(&seen.slab_of);
    world.slab_left.clone_from(&seen.slab_left);
    // Each cell of crust's margin: what the view saw where it draws
    // it; a cell the view does not draw has its piece's margin there.
    *margin_of = vec![u8::MAX; land.cells.len()];
    for q in 0..CELLS {
        if top[q] == NO_BIT {
            continue;
        }
        let i = top[q] as usize;
        land.cells[i].rock.margins |= rock[q].margins & (PASSIVE | ACTIVE);
        if margin_of[i] == u8::MAX || seen.margin[q] == Margin::Active as u8 {
            margin_of[i] = seen.margin[q];
        }
    }
    for i in 0..land.cells.len() {
        if margin_of[i] != u8::MAX {
            continue;
        }
        let c = land.cells[i];
        let q = c.cell as usize;
        margin_of[i] = Margin::Inland as u8;
        if top[q] != NO_BIT && !c.under {
            let o = land.cells[top[q] as usize];
            if o.frame == c.frame && o.plate == c.plate {
                margin_of[i] = seen.margin[q];
                land.cells[i].rock.margins |= o.rock.margins & (PASSIVE | ACTIVE);
            }
        }
    }
}
/// Step 12 (the forces). The plates' turns for the next step.
pub(super) fn next(world: &mut World) {
    let journal = &mut world.journal;
    let s = world.s;
    let sphere = world.sphere;
    let heat = &mut world.heat;
    let plumes = &mut world.plumes;
    let spent = &mut world.spent;
    let parted = &mut world.parted;
    let broken = &mut world.broken;
    let alias = &mut world.alias;
    let rifts = &mut world.rifts;
    let lips = &mut world.lips;
    let edges_last = &mut world.edges_last;
    let land = &mut world.land;
    let top = &mut world.top;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let omega = &mut world.omega;
    let group = &mut world.group;
    let t = world.step.t;
    let now_my = world.step.now_my;
    let seen = &mut world.step.seen;
    let piles = &mut world.step.piles;
    // The crust that still joins the plates a rift has cut.
    let mut bridges: Vec<Bridge> = Vec::new();
    for tr in world.troughs.iter().filter(|tr| tr.cut) {
        let n = tr.path.len() as i64;
        for j in 0..tr.path.len() {
            if tr.ext[j] >= BREAK_M || tr.twin[j] == NO_BIT {
                continue;
            }
            let (c, o) = (
                &land.cells[tr.path[j] as usize],
                &land.cells[tr.twin[j] as usize],
            );
            if c.dead || o.dead || c.plate == o.plate {
                continue;
            }
            let w = necked(weak(heat[c.cell as usize], &c.rock, now_my), tr.ext[j]);
            bridges.push((c.plate, o.plate, c.pos, (1000 - w) * 1000 / (w * n)));
        }
    }
    if t < s.steps {
        let next = drive(
            sphere,
            s,
            &*plate,
            &*rock,
            &*heat,
            Edges {
                slab: &seen.slab,
                suction: &seen.suction,
                ridge: &seen.ridge,
                piles: &*piles,
                sutures: &world.step.sutures,
                bridges: &bridges,
            },
            Some(&*omega),
            &held(&*parted, &*broken),
        );
        *omega = next.omega;
        *group = next.group;
        world.locked = next.locked;
        // A rift's ocean is open once its two shores touch nowhere.
        for (k, r) in rifts.iter_mut().enumerate() {
            if r.cut_my != NEVER && r.parted_my == NEVER && !world.renamed.contains(&k) {
                let (a, b) = (alias_of(&*alias, r.plate), alias_of(&*alias, r.new));
                if a != b && parted[a as usize][b as usize] {
                    r.parted_my = now_my;
                }
            }
        }
        let opened = opening(sphere, &*plate, &*rock, &*parted, &*group, &mut *broken);
        for &pair in &opened {
            journal.push(Event::plates(
                What::Opened,
                (pair.0 as usize, pair.1 as usize),
                middle(sphere, |c| {
                    plate[c] == pair.0
                        && rock[c].class == CONTINENT
                        && sphere.links[c]
                            .iter()
                            .any(|&(n, _)| plate[n as usize] == pair.1)
                }),
                0,
            ));
            for (at, km2) in flood(sphere, &mut *land, &*top, pair, &*plumes, &mut *spent) {
                journal.push(Event::plates(
                    What::Lava,
                    (pair.0 as usize, pair.1 as usize),
                    at,
                    km2,
                ));
                lips.push(Lip {
                    my: now_my,
                    at,
                    km2,
                });
            }
        }
    }
    *edges_last = (
        std::mem::take(&mut seen.slab),
        std::mem::take(&mut seen.suction),
        std::mem::take(&mut seen.ridge),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two plates, each half the sphere, all ocean floor; the forces
    /// turn the second about the north pole, which parts it from the
    /// first around `(0, 1, 0)` and presses it on the first on the far
    /// side. A suture of `cells` cells of their boundary there, each
    /// holding `holds`: do they turn as one?
    fn joined(cells: usize, holds: i64, turn: i64, far_side: bool) -> (bool, Vec<(u8, u8, bool)>) {
        let sphere = Sphere::new();
        let s = Settings::default();
        let plate: Vec<u8> = (0..CELLS)
            .map(|c| u8::from(sphere.centre[c][0] < 0))
            .collect();
        let rock = vec![Rock::plain(OCEAN, OCEAN_THICK); CELLS];
        let place = [0, if far_side { -ONE } else { ONE }, 0];
        let mut edge: Vec<usize> = (0..CELLS)
            .filter(|&c| {
                plate[c] == 0 && sphere.links[c].iter().any(|&(n, _)| plate[n as usize] == 1)
            })
            .collect();
        edge.sort_by_key(|&c| (km_between(sphere.centre[c], place), c));
        let sutures: Vec<Suture> = edge[..cells]
            .iter()
            .map(|&c| (c, 0, 1, [-ONE, 0, 0], holds))
            .collect();
        let mut count = [0_i64; MAXP];
        for &p in &plate {
            count[p as usize] += 1;
        }
        let mut torque = vec![[0_i64; 3]; MAXP];
        torque[1] = [0, 0, turn * count[1]];
        let d = drive_from(
            &sphere,
            &s,
            &plate,
            &rock,
            Edges {
                slab: &[],
                suction: &[],
                ridge: &[],
                piles: &[],
                sutures: &sutures,
                bridges: &[],
            },
            None,
            &vec![[false; MAXP]; MAXP],
            (torque, count, [0; MAXP]),
        );
        (d.group[0] == d.group[1], d.locked)
    }

    #[test]
    fn a_suture_holds_a_pull_its_section_can_hold_and_tears_under_more() {
        // A pull of 20 mm a year across a suture of 40 cells of a
        // shield's strength: more than it holds, the plates part.
        assert_eq!(joined(40, 1_000_000, 20_000, false), (false, vec![]));
        // The same suture under a pull of 1 mm a year holds: the two
        // turn as one, and the record will make them one plate.
        assert_eq!(
            joined(40, 1_000_000, 1_000, false),
            (true, vec![(0, 1, true)])
        );
        // A long front holds what a point of contact does not.
        assert!(!joined(2, 1_000_000, 1_000, false).0);
        // Young, seamed crust holds a tenth of it.
        assert!(!joined(40, 100_000, 1_000, false).0);
        // Nothing holds two continents against closing: pressed on each
        // other they go on closing (against their pile).
        assert_eq!(joined(40, 1_000_000, 20_000, true), (false, vec![]));
    }

    #[test]
    fn a_pile_pushes_back_as_the_difference_of_the_squares_of_the_thicknesses() {
        let whole = i64::from(CONTINENT_THICK);
        assert_eq!(pile_weight(whole), 0);
        assert_eq!(pile_weight(whole - 100), 0);
        assert_eq!(pile_weight(2 * whole), 1000);
        // 50 km of crust: (50² - 35²) / (70² - 35²).
        assert_eq!(
            pile_weight(500),
            (500 * 500 - 350 * 350) * 1000 / (3 * 350 * 350)
        );
        assert!(pile_weight(3 * whole) > 2 * pile_weight(2 * whole));
    }
}
