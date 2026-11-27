//! Rifts as a process: where a plate is weak, troughs that begin, stretch
//! and thin the crust, grow, cut the plate in two or are abandoned;
//! boundaries that open; the sinking of a rifted margin.

use super::*;

/// A rift (version 11). Along a line across a plate, the forces on the
/// plate's two sides would part them at some speed if nothing held them
/// (the same balance that turns the plates). The lithosphere holds: it
/// lets through only a share of that speed, its weakness, ‰: little
/// where it is cold and whole, more over a warm mantle (up to
/// [`WEAK_WARM`] at [`WEAK_HOT`]), more along an old suture or a failed
/// rift. As the crust thins the line weakens (necking): the share goes
/// to the whole as the square of the stretching done.
pub(super) const WEAK_COLD: i64 = 10;
pub(super) const WEAK_WARM: i64 = 50;
pub(super) const WEAK_HOT: i64 = 2_500;
pub(super) const WEAK_SEAM: i64 = 40;
/// The stretched zone's width at the start, km (a narrow rift), and the
/// crust's thickness at which it gives way, tenths of a km: the plate is
/// cut there and the ocean opens.
pub(super) const RIFT_WIDTH_KM: i64 = 200;
pub(super) const BREAK_THICK: u16 = 150;
/// A trough exists once its line stretches this fast, µm a year; below
/// it for this many steps it is abandoned (a failed rift).
pub(super) const RIFT_LEAST_UM: i64 = 1_000;
pub(super) const RIFT_IDLE: usize = 4;
/// Each tip of a trough grows as fast as the trough stretches: this
/// many km for each m of opening (the East African rift: ~4 mm a year
/// and ~60 km a million years; once the crust necks the stretching
/// races, and the tips with it: the rift unzips). It grows toward the
/// weakest crust ahead: within a cone about the line across the pull, a
/// km off the line costing [`RIFT_OFF`] ‰ of weakness per hundred km.
pub(super) const RIFT_GROW: i64 = 10;
pub(super) const RIFT_OFF: i64 = 10;
/// Where a plate may begin to tear: at the weakest of this many spots of
/// its continent, this far apart, km, along the best of twelve lines.
pub(super) const RIFT_SPOTS: usize = 6;
pub(super) const RIFT_SPOT_APART_KM: i64 = 800;

/// A rift stretches and thins the crust this far from its line, km: it
/// sinks at once by this much at the line, m, and as much again as it
/// cools, [`COOLING`] ‰ of what is left each step (a passive margin's
/// shelf).
pub(super) const STRETCH_KM: u32 = 230;
pub(super) const STRETCH_M: i64 = 400;
pub(super) const COOLING: i64 = 100;

/// How weak the lithosphere is under a cell of crust, ‰ (see
/// [`WEAK_COLD`]).
pub(super) fn weak(heat: i32, r: &Rock) -> i64 {
    let warm = i64::from(heat).clamp(0, WEAK_HOT) * WEAK_WARM / WEAK_HOT;
    let seam = if r.margins & (SUTURE | OLD_SUTURE | FAILED_RIFT) != 0 {
        WEAK_SEAM
    } else {
        0
    };
    WEAK_COLD + warm + seam
}

/// A rift in progress (version 11): a trough along cells of crust of one
/// plate. It is carried with its plate; each step the pull across it
/// stretches and thins the crust along it, and its tips grow. Once it
/// has crossed its continent and its crust has given way, the plate is
/// cut along it. A trough the pull has left is abandoned: it stays.
pub(super) struct Trough {
    /// Its record, an index into the rifts.
    pub(super) record: usize,
    pub(super) plate: u8,
    /// The cells of crust along it, in order from one tip to the other.
    pub(super) path: Vec<u32>,
    /// The pole of the great circle it follows: the pull's direction.
    pub(super) normal: V3,
    /// How far it has opened, m.
    pub(super) ext_m: i64,
    /// Whether each tip has reached its continent's edge.
    pub(super) done: [bool; 2],
    /// Steps without stretching.
    pub(super) idle: usize,
    pub(super) failed: bool,
}

/// Pairs of plates whose continents touch, have not parted, and now move
/// apart: their boundary opens.
pub(super) fn opening(
    sphere: &Sphere,
    plate: &[u8],
    rock: &[Rock],
    parted: &[[bool; MAXP]; MAXP],
    group: &[u8; MAXP],
    broken: &mut [[bool; MAXP]; MAXP],
) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    for c in 0..CELLS {
        if rock[c].class != CONTINENT {
            continue;
        }
        let a = plate[c];
        for &(n, _) in &sphere.links[c] {
            let n = n as usize;
            let b = plate[n];
            let (i, j) = (a as usize, b as usize);
            if b != a
                && rock[n].class == CONTINENT
                && !parted[i][j]
                && group[i] != group[j]
                && !broken[i][j]
            {
                broken[i][j] = true;
                broken[j][i] = true;
                out.push((a.min(b), a.max(b)));
            }
        }
    }
    out
}

/// Step 3b. The rifts: troughs begin, stretch the crust, grow and cut
/// their plate in two.
pub(super) fn troughs(world: &mut World) {
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
    let born_t = &mut world.born_t;
    let next_id = &mut world.next_id;
    let rifts = &mut world.rifts;
    let lips = &mut world.lips;
    let troughs = &mut world.troughs;
    let edges_last = &mut world.edges_last;
    let land = &mut world.land;
    let bits = &mut world.bits;
    let disp = &mut world.disp;
    let top = &mut world.top;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let omega = &mut world.omega;
    let group = &mut world.group;
    let t = world.step.t;
    let now_my = world.step.now_my;
    let cont_of = &mut world.step.cont_of;
    // 3b. Rifts (version 11, the lead's 27 November: « de vrais
    // rifts »). A plate tears where it is pulled apart and weak:
    // a trough begins, stretches and thins the crust along it
    // step after step, grows at its tips toward the weakest
    // crust ahead, and cuts the plate in two once it has crossed
    // the continent and the crust has given way. A trough the
    // pull has left is abandoned and stays.
    {
        let (cell_tq, edge_tq) = torques(
            sphere,
            s,
            &*plate,
            &*rock,
            &*pushes,
            &*alias,
            &*heat,
            Edges {
                slab: &edges_last.0,
                suction: &edges_last.1,
                ridge: &edges_last.2,
                grips: &[],
            },
        );
        let mut loads: Vec<Vec<Load>> = vec![Vec::new(); MAXP];
        for c in 0..CELLS {
            let p = plate[c] as usize;
            if cont_of[p] > 0 {
                let land_here = rock[c].class != OCEAN;
                loads[p].push((
                    sphere.centre[c],
                    cell_tq[c],
                    1000 + if land_here { s.continent_drag } else { 0 },
                    rock[c].class == CONTINENT,
                ));
            }
        }
        for &(c, p, tq) in &edge_tq {
            if cont_of[p as usize] > 0 {
                loads[p as usize].push((sphere.centre[c as usize], tq, 0, false));
            }
        }
        let v0 = s.mantle_drag_mm * 1000;
        let break_m = RIFT_WIDTH_KM * 1000 * i64::from(CONTINENT_THICK - BREAK_THICK)
            / i64::from(BREAK_THICK);
        let mut busy = [false; MAXP];
        let mut on_path = vec![false; land.cells.len()];
        for tr in &*troughs {
            for &i in &tr.path {
                on_path[i as usize] = true;
            }
        }
        let mut cuts: Vec<usize> = Vec::new();
        for k in 0..troughs.len() {
            if troughs[k].failed {
                continue;
            }
            let pl = alias_of(&*alias, troughs[k].plate);
            let mut path = std::mem::take(&mut troughs[k].path);
            path.retain(|&i| {
                let c = &land.cells[i as usize];
                !c.dead && c.plate == pl
            });
            let record = troughs[k].record;
            let give_up = |land: &mut Land, rifts: &mut Vec<Rift>| {
                rifts[record].aborted = true;
                rifts[record].failed_my = now_my;
                for c in &mut land.cells {
                    if usize::from(c.rock.rift) == record + 1 {
                        c.rock.margins |= FAILED_RIFT;
                    }
                }
            };
            if path.is_empty() || busy[pl as usize] {
                troughs[k].failed = true;
                troughs[k].path = path;
                give_up(&mut *land, &mut *rifts);
                journal.push(Event::rift(
                    What::RiftAbandoned,
                    pl,
                    rifts[record].centre,
                    record,
                ));
                continue;
            }
            busy[pl as usize] = true;
            troughs[k].plate = pl;
            let at = |i: u32| land.cells[i as usize].pos;
            let mid = at(path[path.len() / 2]);
            let mut n = if path.len() >= 5 {
                normalize(cross(mid, sub(at(path[path.len() - 1]), at(path[0]))))
            } else {
                normalize(tangent(mid, troughs[k].normal))
            };
            if dot(n, troughs[k].normal) < 0 {
                n = n.map(|v| -v);
            }
            troughs[k].normal = n;
            let pull = parting(&loads[pl as usize], mid, n, v0).unwrap_or(0);
            let w0 = path
                .iter()
                .map(|&i| {
                    let c = &land.cells[i as usize];
                    weak(heat[c.cell as usize], &c.rock)
                })
                .sum::<i64>()
                / path.len() as i64;
            let done_share = (troughs[k].ext_m * 1000 / break_m).min(1000);
            let w = w0 + (1000 - w0) * done_share / 1000 * done_share / 1000;
            let rate = pull * w / 1000;
            if rate < RIFT_LEAST_UM {
                troughs[k].idle += 1;
                if troughs[k].idle >= RIFT_IDLE {
                    troughs[k].failed = true;
                    give_up(&mut *land, &mut *rifts);
                    journal.push(Event::rift(
                        What::RiftAbandoned,
                        pl,
                        rifts[record].centre,
                        record,
                    ));
                }
                troughs[k].path = path;
                continue;
            }
            troughs[k].idle = 0;
            // Its tips grow toward the weakest crust ahead.
            for e in 0..2 {
                if troughs[k].done[e] {
                    continue;
                }
                let mut budget = rate * s.step_my * RIFT_GROW / 1000;
                while budget > 0 {
                    let i = if e == 0 {
                        path[0]
                    } else {
                        path[path.len() - 1]
                    } as usize;
                    let me = land.cells[i];
                    let line = normalize(cross(n, me.pos));
                    let dir = if e == 0 { line.map(|v| -v) } else { line };
                    let mut around: Vec<usize> = land.kin(sphere, i).map(|(j, _)| j).collect();
                    let q = me.cell as usize;
                    for m in
                        std::iter::once(q).chain(sphere.links[q].iter().map(|&(m, _)| m as usize))
                    {
                        if top[m] != NO_BIT && land.cells[top[m] as usize].frame != me.frame {
                            around.push(top[m] as usize);
                        }
                    }
                    let mut best: Option<(i64, std::cmp::Reverse<usize>, i64)> = None;
                    for cone in [ONE / 2, ONE / 10] {
                        for &j in &around {
                            let o = &land.cells[j];
                            if o.dead || o.under || o.plate != pl || on_path[j] {
                                continue;
                            }
                            let km = km_between(o.pos, me.pos);
                            if km == 0 {
                                continue;
                            }
                            let along = dot(normalize(tangent(me.pos, sub(o.pos, me.pos))), dir);
                            if along < cone {
                                continue;
                            }
                            let off = dot(o.pos, n).abs() * RADIUS_KM / ONE;
                            let score = weak(heat[o.cell as usize], &o.rock) - off * RIFT_OFF / 100;
                            let key = (score, std::cmp::Reverse(j), km);
                            if best.is_none_or(|b| key > b) {
                                best = Some(key);
                            }
                        }
                        if best.is_some() {
                            break;
                        }
                    }
                    let Some((_, std::cmp::Reverse(j), km)) = best else {
                        troughs[k].done[e] = true;
                        break;
                    };
                    on_path[j] = true;
                    if e == 0 {
                        path.insert(0, j as u32);
                    } else {
                        path.push(j as u32);
                    }
                    budget -= km;
                }
            }
            // The pull stretches and thins the crust along it: it
            // sinks, and will sink as much again as it cools. The
            // crust's volume is kept: a thinned cell stands for a
            // wider crust.
            let more = rate * s.step_my;
            let from: Vec<usize> = path.iter().map(|&i| i as usize).collect();
            let d = land.spread(sphere, &from, |_| true, STRETCH_KM);
            for (i, c) in land.cells.iter_mut().enumerate() {
                if d[i] == u32::MAX || c.dead {
                    continue;
                }
                // Each cell thins from where it is (a cell the
                // trough has just reached has all to do), down to
                // the thickness at which the crust gives way on
                // the axis, less toward the zone's edge.
                let reach = bump(i64::from(d[i]), i64::from(STRETCH_KM));
                let least = i64::from(BREAK_THICK)
                    + i64::from(CONTINENT_THICK - BREAK_THICK) * (1000 - reach) / 1000;
                let r = &mut c.rock;
                let was = i64::from(r.thick);
                r.rift = (record + 1).min(255) as u8;
                let thin = (more * was / (RIFT_WIDTH_KM * 1000) * was / i64::from(CONTINENT_THICK)
                    * reach
                    / 1000)
                    .min(was - least);
                if thin <= 0 {
                    continue;
                }
                r.thick = (was - thin) as u16;
                c.area = (i64::from(c.area) * was / (was - thin)) as u32;
                let drop = thin * STRETCH_M / i64::from(CONTINENT_THICK - BREAK_THICK);
                r.rise(-drop, BY_STRETCH);
                r.cool = lift(r.cool, drop);
                r.sunk = lift(r.sunk, drop);
            }
            for &i in &path {
                let c = &mut land.cells[i as usize];
                if c.event != Margin::Suture as u8 {
                    c.event = Margin::Rift as u8;
                }
            }
            troughs[k].ext_m = (troughs[k].ext_m + more).min(break_m);
            rifts[record].cells = rifts[record].cells.max(path.len() as u32);
            if troughs[k].done == [true; 2] && troughs[k].ext_m >= break_m {
                cuts.push(k);
            }
            troughs[k].path = path;
        }
        // The crust has given way along a trough that crosses its
        // continent: the plate is cut in two along it, and on
        // through its floor from both ends. Each side keeps its
        // lattice: the two shores fit for good.
        let mut cut_done = vec![false; troughs.len()];
        for &k in &cuts {
            if *next_id >= MAXP {
                continue;
            }
            let old = troughs[k].plate;
            let path = &troughs[k].path;
            let seat = seats(&*land, &*top);
            let mut on = vec![false; CELLS];
            for &i in path {
                on[seat[i as usize] as usize] = true;
            }
            let mut sources: Vec<(usize, u32)> = Vec::new();
            for (j, &i) in path.iter().enumerate() {
                let x = land.cells[i as usize].pos;
                let a = land.cells[path[j.saturating_sub(1)] as usize].pos;
                let b = land.cells[path[(j + 1).min(path.len() - 1)] as usize].pos;
                let left = cross(x, sub(b, a));
                let q = seat[i as usize] as usize;
                for &(m, _) in &sphere.links[q] {
                    let m = m as usize;
                    if plate[m] == old && !on[m] {
                        let side = dot(sub(sphere.centre[m], sphere.centre[q]), left) > 0;
                        sources.push((m, u32::from(side)));
                    }
                }
            }
            let (_, label) = sphere.spread(&sources, |_, b| plate[b] == old && !on[b], 30_000);
            let mut count = [0_usize; 2];
            for c in 0..CELLS {
                if plate[c] == old && label[c] < 2 {
                    count[label[c] as usize] += 1;
                }
            }
            if count[0] == 0 || count[1] == 0 {
                continue;
            }
            let away = u32::from(count[1] <= count[0]);
            let new = *next_id as u8;
            *next_id += 1;
            let side_new: Vec<bool> = (0..CELLS)
                .map(|c| plate[c] == old && label[c] == away)
                .collect();
            for c in 0..CELLS {
                if side_new[c] {
                    plate[c] = new;
                    if disp[c] != NO_BIT {
                        bits[disp[c] as usize].plate = new;
                    }
                }
            }
            for (i, c) in land.cells.iter_mut().enumerate() {
                if c.plate == old && side_new[seat[i] as usize] {
                    c.plate = new;
                    c.switch = SWITCH_RIFT;
                }
            }
            for (at, km2) in flood(sphere, &mut *land, &*top, (old, new), &*plumes, &mut *spent) {
                lips.push(Lip {
                    my: now_my,
                    at,
                    km2,
                });
            }
            let (o, w) = (old as usize, new as usize);
            for p in 0..MAXP {
                parted[w][p] = parted[o][p];
                parted[p][w] = parted[p][o];
            }
            parted[o][w] = false;
            parted[w][o] = false;
            broken[o][w] = true;
            broken[w][o] = true;
            born_t[w] = t;
            group[w] = new;
            omega[w] = omega[o];
            let r = &mut rifts[troughs[k].record];
            r.new = new;
            r.cut_my = now_my;
            cut_done[k] = true;
            journal.push(Event {
                other: old,
                ..Event::rift(
                    What::BornOfRift,
                    new,
                    land.cells[path[path.len() / 2] as usize].pos,
                    troughs[k].record,
                )
            });
        }
        let mut k = 0;
        troughs.retain(|_| {
            k += 1;
            !cut_done[k - 1]
        });
        if cut_done.iter().any(|&c| c) {
            *cont_of = land.weight();
        }
        // A plate with no trough begins to tear where the pull
        // across its weakest crust stretches it fast enough.
        for p in 0..MAXP {
            if busy[p] || cont_of[p] == 0 || alias[p] != p as u8 || loads[p].is_empty() {
                continue;
            }
            let mut spots: Vec<(i64, u32, usize)> = land
                .cells
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    !c.dead && !c.under && c.plate as usize == p && c.rock.class == CONTINENT
                })
                .map(|(i, c)| (weak(heat[c.cell as usize], &c.rock), c.id, i))
                .collect();
            spots.sort_unstable_by_key(|&(w, id, _)| (std::cmp::Reverse(w), id));
            let mut picked: Vec<(i64, usize)> = Vec::new();
            for &(w, _, i) in &spots {
                if picked.len() == RIFT_SPOTS {
                    break;
                }
                let x = land.cells[i].pos;
                if picked
                    .iter()
                    .all(|&(_, j)| km_between(x, land.cells[j].pos) >= RIFT_SPOT_APART_KM)
                {
                    picked.push((w, i));
                }
            }
            let mut best: Option<(i64, std::cmp::Reverse<usize>, usize, V3)> = None;
            for (order, &(w, i)) in picked.iter().enumerate() {
                let x = land.cells[i].pos;
                let (e, nn) = (east_at(x), north_at(x, east_at(x)));
                for a in 0..12 {
                    let angle = a * TAU / 24;
                    let n = normalize(add(scale(e, cos(angle)), scale(nn, sin(angle))));
                    if let Some(pull) = parting(&loads[p], x, n, v0) {
                        let key = (
                            pull * w / 1000,
                            std::cmp::Reverse(order * 12 + a as usize),
                            i,
                            n,
                        );
                        if best.is_none_or(|b| (key.0, key.1) > (b.0, b.1)) {
                            best = Some(key);
                        }
                    }
                }
            }
            if let Some((rate, _, i, n)) = best
                && rate >= RIFT_LEAST_UM
            {
                troughs.push(Trough {
                    record: rifts.len(),
                    plate: p as u8,
                    path: vec![i as u32],
                    normal: n,
                    ext_m: 0,
                    done: [false; 2],
                    idle: 0,
                    failed: false,
                });
                rifts.push(Rift {
                    my: now_my,
                    plate: p as u8,
                    new: NONE,
                    centre: land.cells[i].pos,
                    area_km2: cont_of[p],
                    aborted: false,
                    cut_my: NEVER,
                    parted_my: NEVER,
                    failed_my: NEVER,
                    cells: 1,
                });
                journal.push(Event::rift(
                    What::RiftBegun,
                    p as u8,
                    land.cells[i].pos,
                    rifts.len() - 1,
                ));
            }
        }
    }
}
/// Step 11 (the rifts). The crust beside a new ocean floor is a rift's
/// margin: it sinks.
pub(super) fn margins_sink(world: &mut World) {
    let sphere = world.sphere;
    let land = &mut world.land;
    // A rift stretches and thins the crust along it: it sinks at
    // once, and as much again while it cools.
    let torn: Vec<usize> = (0..land.cells.len())
        .filter(|&i| {
            let c = &land.cells[i];
            c.rock.margins & RIFT != 0 && !c.rifted
        })
        .collect();
    if !torn.is_empty() {
        let d = land.spread(sphere, &torn, |_| true, STRETCH_KM);
        for (i, c) in land.cells.iter_mut().enumerate() {
            if d[i] == u32::MAX {
                continue;
            }
            let r = &mut c.rock;
            let due = STRETCH_M * bump(i64::from(d[i]), i64::from(STRETCH_KM)) / 1000;
            let more = due - i64::from(r.sunk);
            if more > 0 {
                r.rise(-more, BY_STRETCH);
                r.cool = lift(r.cool, more);
                r.sunk = lift(r.sunk, more);
            }
        }
    }
}
