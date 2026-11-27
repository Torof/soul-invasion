//! The forces on the plates and their turns: slab pull, trench suction,
//! ridge push, the mantle's drag, the grip of a collision; what the plates
//! do at their boundaries (`look`).

use super::*;

/// A slab younger than this, millions of years, is too light to pull.
pub const SLAB_YOUNG_MY: i64 = 10;
/// The most an old slab pulls, per mille of the pull at
/// [`Settings::slab_age_my`].
pub(super) const SLAB_MOST: i64 = 1_500;
/// An escape's push keeps this much of its strength per step,
/// per mille.
pub(super) const PUSH_KEEP: i64 = 960;

/// A plate keeps this much of its last motion each step, per mille (the
/// rest comes from the forces of the step).
pub(super) const INERTIA: i64 = 400;
/// Convergence or divergence a slab or a ridge needs, µm a year.
pub(super) const FORCE_LEAST_UM: i64 = 2_000;

/// A push on two plates, an escaping block and
/// the plate it leaves: away from a great circle (`normal`), growing
/// with the distance from it up to `expansion_km`, within `reach_km` of
/// `centre`.
#[derive(Debug, Clone, Copy)]
pub(super) struct Push {
    pub(super) centre: V3,
    pub(super) normal: V3,
    pub(super) reach_km: i64,
    pub(super) expansion_km: i64,
    /// µm a year.
    pub(super) strength: i64,
    pub(super) plates: (u8, u8),
}

/// A cell where a force acts on its plate, the force's direction and its
/// weight, per mille.
pub(super) type Spot = (usize, u8, V3, i64);

/// A cell on a collision's front: its plate, the plate it meets, the
/// normal toward it, and how fast they close, µm a year.
pub(super) type Front = (usize, u8, u8, V3, i64);

/// Two plates in a collision and its front's length, cells.
pub(super) type Grip = (u8, u8, i64);

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

/// The most of two colliding plates' relative motion a front takes away
/// in a step, ‰.
pub(super) const GRIP_MOST: i64 = 900;

/// The part of `v` tangent to the sphere at `x`.
pub(super) fn tangent(x: V3, v: V3) -> V3 {
    sub(v, scale(x, dot(v, x)))
}

/// The turn that moves the point `at` along the unit `dir` at `um` µm a
/// year (a vector along its pole).
pub(super) fn lateral(at: V3, dir: V3, um: i64) -> V3 {
    scale(normalize(cross(at, dir)), um)
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
}

/// The forces at the plates' edges, read at the last step: the slabs
/// (on the plate that sinks), their suction (on the plate riding over
/// them), the ridges, and the collisions' fronts.
#[derive(Clone, Copy)]
pub(super) struct Edges<'a> {
    pub(super) slab: &'a [Spot],
    pub(super) suction: &'a [Spot],
    pub(super) ridge: &'a [Spot],
    pub(super) grips: &'a [Grip],
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
    pushes: &[Push],
    alias: &[u8; MAXP],
    heat: &[i32],
    edges: Edges,
    prev: Option<&[V3]>,
    held: &[[bool; MAXP]],
) -> Drive {
    let mut torque = vec![[0_i64; 3]; MAXP];
    let mut cells = [0_i64; MAXP];
    let mut cont = [0_i64; MAXP];
    let (cell_tq, edge_tq) = torques(sphere, s, plate, rock, pushes, alias, heat, edges);
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
/// (an escape's push, the mantle's heat under a continent),
/// and the edges' (a cell, the plate it acts on, the torque): slabs,
/// their suction, ridges.
#[allow(
    clippy::too_many_arguments,
    reason = "the forces' inputs, each one named"
)]
pub(super) fn torques(
    sphere: &Sphere,
    s: &Settings,
    plate: &[u8],
    rock: &[Rock],
    pushes: &[Push],
    alias: &[u8; MAXP],
    heat: &[i32],
    edges: Edges,
) -> (Vec<V3>, Vec<(u32, u8, V3)>) {
    let mut cell_tq = vec![[0_i64; 3]; CELLS];
    let mut edge_tq = Vec::new();
    let pushes: Vec<Push> = pushes
        .iter()
        .filter(|p| p.strength > 0)
        .filter_map(|p| {
            let (a, b) = (alias_of(alias, p.plates.0), alias_of(alias, p.plates.1));
            (a != b).then_some(Push {
                plates: (a, b),
                ..*p
            })
        })
        .collect();
    for c in 0..CELLS {
        let p = plate[c];
        let x = sphere.centre[c];
        let mut f = [0_i64; 3];
        for push in &pushes {
            if p != push.plates.0 && p != push.plates.1 {
                continue;
            }
            let d = km_between(x, push.centre);
            if d >= push.reach_km {
                continue;
            }
            let side = dot(x, push.normal);
            let away = (side * RADIUS_KM / ONE * 1000 / push.expansion_km).clamp(-1000, 1000);
            let fade = ((push.reach_km - d) * 2000 / push.reach_km).min(1000);
            let dir = normalize(tangent(x, push.normal));
            f = add(f, scale(dir, push.strength * away / 1000 * fade / 1000));
        }
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
/// the stretched zone: a rift tears a continent, not its shore.
pub(super) fn parting(loads: &[Load], x: V3, n: V3, (v0, top): (i64, i64)) -> Option<i64> {
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
    Some(dot(sub(cross(w[1], x), cross(w[0], x)), n))
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
    let mut w = vec![[0_i64; 3]; MAXP];
    loop {
        let mut tq = vec![[0_i64; 3]; MAXP];
        let mut rs = [0_i64; MAXP];
        for p in 0..MAXP {
            let r = root(&up, p as u8) as usize;
            tq[r] = add(tq[r], torque[p]);
            rs[r] += resist[p];
        }
        for r in 0..MAXP {
            w[r] = if rs[r] > 0 {
                tq[r].map(|v| v * 1000 / rs[r])
            } else {
                [0; 3]
            };
        }
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
    }
    // A collision's front grips the two plates: it takes away part of
    // their relative motion, more as the front grows against the smaller
    // one (they slow, and stop once the front is long).
    let mut grip = vec![0_i64; MAXP * MAXP];
    for &(p, o, cells) in edges.grips {
        let (ra, rb) = (root(&up, p) as usize, root(&up, o) as usize);
        if ra != rb {
            grip[ra.min(rb) * MAXP + ra.max(rb)] += cells;
        }
    }
    let mut rs = [0_i64; MAXP];
    let mut cs = [0_i64; MAXP];
    for p in 0..MAXP {
        let r = root(&up, p as u8) as usize;
        rs[r] += resist[p];
        cs[r] += cont[p];
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
    for a in 0..MAXP {
        for b in a + 1..MAXP {
            let fc = grip[a * MAXP + b];
            if fc == 0 {
                continue;
            }
            let small = cs[a].min(cs[b]).max(1);
            let f = (fc * s.collision_resist * 1000 / small).min(GRIP_MOST);
            let rel = sub(w[a], w[b]);
            let total = (rs[a] + rs[b]).max(1);
            w[a] = sub(w[a], rel.map(|v| v * f / 1000 * rs[b] / total));
            w[b] = add(w[b], rel.map(|v| v * f / 1000 * rs[a] / total));
        }
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
    let (v0, top) = (s.mantle_drag_mm * 1000, s.speed_mm.1 * 1000);
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
    Drive { omega, group }
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
}

pub(super) fn look(
    sphere: &Sphere,
    plate: &[u8],
    group: &[u8; MAXP],
    rock: &mut [Rock],
    omega: &[V3],
    event: &[u8],
    (now_my, full_my): (u16, i64),
) -> Look {
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
            if !mine_cont
                && closing >= FORCE_LEAST_UM
                && 2 * closing >= slide
                && (theirs_cont || !mine_up)
            {
                // An old slab is heavy: it pulls its plate, and draws the
                // plate riding over it toward the trench.
                let age = i64::from(rock[c].born_my.saturating_sub(now_my));
                let weight = ((age - SLAB_YOUNG_MY) * 1000 / (full_my - SLAB_YOUNG_MY).max(1))
                    .clamp(0, SLAB_MOST);
                if weight > 0 {
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
        (now_my, s.slab_age_my),
    );
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
    let pushes = &mut world.pushes;
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
    let grips = &mut world.step.grips;
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
            &*pushes,
            &*alias,
            &*heat,
            Edges {
                slab: &seen.slab,
                suction: &seen.suction,
                ridge: &seen.ridge,
                grips: &*grips,
                bridges: &bridges,
            },
            Some(&*omega),
            &held(&*parted, &*broken),
        );
        *omega = next.omega;
        *group = next.group;
        // A rift's ocean is open once its two shores touch nowhere.
        for r in &mut *rifts {
            if r.cut_my != NEVER && r.parted_my == NEVER {
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
