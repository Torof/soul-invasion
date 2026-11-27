//! Trenches and arcs: the ocean floor sinking under a continent (active
//! margin) or under a younger floor, island arcs, terranes and arcs
//! docking; the grid's view of the floor; a passive margin whose old
//! floor gives way (a trench is born).

use super::*;

/// Share of an arc's riding edge that becomes arc crust, per mille.
pub(super) const ARC_SHARE: u64 = 200;

/// An arc builds volcanoes on the plate riding over a sinking floor:
/// toward this height, m, at their line ([`ARC_LINE_KM`] from the coast,
/// this half-width) and on the coast range, by [`ARC_RATE`] ‰ of what is
/// missing each step, as far as [`ARC_KM`].
pub(super) const ARC_TOP_M: i64 = 2_200;
pub(super) const ARC_COAST_M: i64 = 700;
pub(super) const ARC_RATE: i64 = 100;
pub(super) const ARC_LINE_KM: (i64, i64) = (180, 150);
pub(super) const ARC_KM: u32 = 340;
/// A terrane docking raises the crust it meets, m; an island arc is born
/// this high; a plume lifts the continent over it, m a step.
pub(super) const DOCK_M: i64 = 500;
pub(super) const ARC_ISLAND_M: i16 = 200;

/// A passive margin's floor bends down under its own weight (version 14):
/// by this much, m per million years, for a floor as heavy as a slab at
/// full pull ([`slab_weight`]: nothing while the floor is too young to
/// sink), when nothing presses it on its continent or pulls it off.
pub(super) const SAG_M_MY: i64 = 10;
/// Bent this far, m, the floor gives way: it tears from its continent
/// and begins to sink under it.
pub(super) const SAG_BREAK_M: i64 = 1_500;

/// Step 11 (the margins). A trench is born (version 14). The floor of a
/// passive margin is the oldest of its ocean: cold, heavier than the
/// mantle under it, held up only by the plate it is part of. It bends
/// down a little more at each step: the faster the heavier it is
/// (older), faster where the floor pushes its continent along (the
/// margin is pressed), slower or not at all where the continent drags
/// its floor behind it (the margin is pulled). The push is read from the
/// forces: what the mantle's drag on the continent takes to move it as
/// it moves, less what the forces on the continent itself give, is what
/// its floor gives it through the margin; a margin pressed as hard as a
/// full slab pulls bends twice as fast. Bent far enough it gives way:
/// the floor of that ocean, as far as it is one plate with the
/// continent, is a plate of its own from then on, and a slab hangs
/// where it gave way. Nothing else is decided here: the slab pulls like
/// any slab, the floor sinks under the continent like any floor, and
/// the tear runs on along the margin as the floor moves (`forces::look`).
pub(super) fn margins_give_way(world: &mut World) {
    let s = world.s;
    let sphere = world.sphere;
    let edges_last = &world.edges_last;
    let land = &mut world.land;
    let top = &world.top;
    let bits = &mut world.bits;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let hung = &mut world.hung;
    let (t, now_my) = (world.step.t, world.step.now_my);
    paint(&*land, top, &mut *plate, &mut *rock);
    // The margins: a continent's shore with floor of its own plate
    // beside it; the way from the floor to the shore, the oldest floor
    // there.
    let shore = |r: &Rock| r.class == CONTINENT || r.class == TERRANE && r.margins & ACCRETED != 0;
    let mut margin: Vec<(usize, V3, i64)> = Vec::new();
    let mut shores = [0_i64; MAXP];
    for q in 0..CELLS {
        if top[q] == NO_BIT || !shore(&rock[q]) {
            continue;
        }
        let x = sphere.centre[q];
        let mut from = [0_i64; 3];
        let mut age = 0;
        for &(m, _) in &sphere.links[q] {
            let m = m as usize;
            if plate[m] == plate[q] && rock[m].class == OCEAN {
                from = add(from, sub(x, sphere.centre[m]));
                age = age.max(i64::from(rock[m].born_my.saturating_sub(now_my)));
            }
        }
        if from != [0; 3] {
            margin.push((q, normalize(tangent(x, from)), age));
            shores[plate[q] as usize] += 1;
        }
    }
    if margin.is_empty() {
        return;
    }
    // What each plate's floor gives its crust through the margin, as a
    // torque: the drag on the crust moving as it moves, less the forces
    // on the crust itself.
    let (cell_tq, edge_tq) = torques(
        sphere,
        s,
        &*plate,
        &*rock,
        &world.pushes,
        &world.alias,
        &world.heat,
        Edges {
            slab: &edges_last.0,
            suction: &edges_last.1,
            ridge: &edges_last.2,
            grips: &[],
            bridges: &[],
        },
    );
    let mut own = vec![[0_i64; 3]; MAXP];
    let mut drag = [0_i64; MAXP];
    for c in 0..CELLS {
        if rock[c].class != OCEAN {
            let p = plate[c] as usize;
            own[p] = add(own[p], cell_tq[c]);
            drag[p] += 1000 + s.continent_drag;
        }
    }
    for &(c, p, torque) in &edge_tq {
        // A force at a plate's edge acts on the crust it finds there
        // (the edges are those of the step before: where the plate has
        // left the cell, the force is the floor's).
        let c = c as usize;
        let on_crust = if plate[c] == p {
            rock[c].class != OCEAN
        } else {
            sphere.links[c]
                .iter()
                .find(|&&(m, _)| plate[m as usize] == p)
                .is_some_and(|&(m, _)| rock[m as usize].class != OCEAN)
        };
        if on_crust {
            own[p as usize] = add(own[p as usize], torque);
        }
    }
    let v0 = s.mantle_drag_mm * 1000;
    let given: Vec<V3> = (0..MAXP)
        .map(|p| {
            let w = world.omega[p];
            let harder = if v0 > 0 { v0 + norm(w) } else { 1 };
            sub(w.map(|v| v * drag[p] / 1000 * harder / v0.max(1)), own[p])
        })
        .collect();
    // The floor bends, and gives way where it is bent far enough.
    let full = s.slab_pull * 1000;
    let mut gave: Vec<usize> = Vec::new();
    for &(q, normal, age) in &margin {
        let p = plate[q] as usize;
        let push = dot(cross(given[p], sphere.centre[q]), normal) / shores[p];
        let weight = slab_weight(age, s.slab_age_my);
        let more = SAG_M_MY * s.step_my * weight / 1000 * (full + push).max(0) / full.max(1);
        let r = &mut land.cells[top[q] as usize].rock;
        r.sag = (i64::from(r.sag) + more).min(SAG_BREAK_M) as i16;
        if more > 0 && i64::from(r.sag) >= SAG_BREAK_M {
            gave.extend(
                sphere.links[q]
                    .iter()
                    .map(|&(m, _)| m as usize)
                    .filter(|&m| plate[m] as usize == p && rock[m].class == OCEAN),
            );
        }
    }
    // Where it has given way, its floor is a plate of its own.
    let km2 = |c: usize| i64::from(sphere.area_km2[c]);
    let mut seen = vec![false; CELLS];
    for &q0 in &gave {
        if seen[q0] {
            continue;
        }
        let old = plate[q0];
        let mut cells = vec![q0];
        let mut around = [false; MAXP];
        seen[q0] = true;
        let mut k = 0;
        while k < cells.len() {
            let c = cells[k];
            k += 1;
            for &(m, _) in &sphere.links[c] {
                let m = m as usize;
                if plate[m] != old {
                    around[plate[m] as usize] = true;
                } else if rock[m].class == OCEAN && !seen[m] {
                    seen[m] = true;
                    cells.push(m);
                }
            }
        }
        let mut part = vec![false; CELLS];
        for &c in &cells {
            part[c] = true;
        }
        // (The plates' bookkeeping decides what is a plate of its own,
        // `plates::one_piece`: not a part under `PART_KM2`, not a small
        // plate shut in one other. The same holds here, for the floor
        // that leaves and for what it leaves: a pond of old floor, or
        // the floor around a crumb of continent, stays as it is.)
        let floor: i64 = cells.iter().map(|&c| km2(c)).sum();
        let rest: i64 = (0..CELLS)
            .filter(|&c| plate[c] == old && !part[c])
            .map(km2)
            .sum();
        let shut = (0..CELLS)
            .filter(|&c| plate[c] == old && !part[c])
            .all(|c| {
                sphere.links[c]
                    .iter()
                    .all(|&(m, _)| plate[m as usize] == old)
            });
        if floor < PART_KM2
            || rest < PART_KM2
            || floor < ENCLOSED_KM2 && !around.contains(&true)
            || rest < ENCLOSED_KM2 && shut
        {
            continue;
        }
        let Some(new) = name(&mut world.next_id, &mut world.free, &mut world.unnamed) else {
            continue;
        };
        for &c in &cells {
            plate[c] = new;
        }
        for b in bits.iter_mut() {
            if b.plate == old && part[sphere.cell_of(b.pos)] {
                b.plate = new;
            }
        }
        let torn: Vec<usize> = gave.iter().copied().filter(|&q| part[q]).collect();
        for &q in &torn {
            hung[q] = true;
        }
        let (o, w) = (old as usize, new as usize);
        for p in 0..MAXP {
            world.parted[w][p] = world.parted[o][p];
            world.parted[p][w] = world.parted[p][o];
            world.broken[w][p] = world.broken[o][p];
            world.broken[p][w] = world.broken[p][o];
        }
        world.parted[o][w] = true;
        world.parted[w][o] = true;
        world.born_t[w] = t;
        world.group[w] = new;
        world.omega[w] = world.omega[o];
        world.journal.push(Event::plates(
            What::BornOfTrench,
            (w, o),
            normalize(torn.iter().fold([0; 3], |acc, &q| {
                add(acc, sphere.centre[q].map(|v| v / 1000))
            })),
            floor,
        ));
    }
}

/// Step 5 (the floor). The ocean floor sinks under the continents (active
/// margins); terranes and arcs dock.
pub(super) fn sink_and_dock(world: &mut World) {
    let journal = &mut world.journal;
    let n = world.step.n;
    let sphere = world.sphere;
    let accretions = &mut world.accretions;
    let land = &mut world.land;
    let bits = &mut world.bits;
    let now_my = world.step.now_my;
    let at = &mut world.step.at;
    let dead = &mut world.step.dead;
    let accrete = &mut world.step.accrete;
    let (weight, floor) = (world.step.weight, world.step.floor);
    let key = move |p: u8, class: u8| precedence(&weight, &floor, p, class);
    let ride = |p: u8| (-floor[p as usize], -i64::from(p));
    // The ocean floor under a continent: a bit of floor whose
    // place a piece of another plate covers has sunk under it (an
    // active margin); a terrane carried onto a floor that rides
    // higher docks on that floor's plate.
    for i in 0..n {
        let b = bits[i];
        let q = at[i] as usize;
        if land.near[q].is_empty() {
            continue;
        }
        let Some(j) = land.cover(sphere, (b.pos, q), &key, NO_BIT) else {
            continue;
        };
        dead[i] = true;
        let c = &mut land.cells[j];
        if c.plate == b.plate {
            // New crust was built over it (silt, an arc).
            continue;
        }
        if c.rock.class == TERRANE
            && c.rock.margins & ACCRETED == 0
            && ride(b.plate) > ride(c.plate)
        {
            accrete.push((j, b.plate));
            continue;
        }
        c.rock.margins |= ACTIVE;
        c.rock.orogeny = now_my;
        if c.event != Margin::Suture as u8 {
            c.event = Margin::Active as u8;
        }
    }
    // Accretion: the whole terrane or arc joins the plate it met,
    // followed through its own lattice.
    for &(i0, pw) in &*accrete {
        let (pl, piece) = (land.cells[i0].plate, land.cells[i0].rock.piece);
        if pl == pw || land.cells[i0].dead {
            continue;
        }
        let mut stack = vec![i0];
        let mut km2 = 0;
        while let Some(i) = stack.pop() {
            let c = &mut land.cells[i];
            if c.plate != pl
                || c.rock.piece != piece
                || c.rock.class != TERRANE
                || c.rock.margins & ACCRETED != 0
            {
                continue;
            }
            c.plate = pw;
            c.switch = SWITCH_DOCK;
            c.rock.rise(DOCK_M, BY_DOCK);
            c.rock.margins |= ACCRETED;
            c.rock.orogeny = now_my;
            km2 += i64::from(c.area);
            stack.extend(land.kin(sphere, i).map(|(j, _)| j));
        }
        if km2 > 0 || !accretions.iter().any(|a| a.piece == piece) {
            journal.push(Event::plates(
                What::Docked,
                (pw as usize, pl as usize),
                land.cells[i0].pos,
                km2,
            ));
            accretions.push(Accretion {
                my: now_my,
                plate: pw,
                piece,
                km2,
            });
        }
    }
}
/// Step 5 (the view). The grid's view of the crust; each cell of open
/// ocean shows its winning bit of floor, the older sinks under the
/// younger, and the riding edge grows an island arc.
pub(super) fn draw_floor(world: &mut World) {
    let journal = &mut world.journal;
    let seed = world.seed;
    let sphere = world.sphere;
    let arc_steps = world.arc_steps;
    let land = &mut world.land;
    let bits = &mut world.bits;
    let disp = &mut world.disp;
    let top = &mut world.top;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let next_bit = &mut world.next_bit;
    let now_my = world.step.now_my;
    let start = &mut world.step.start;
    let order = &mut world.step.order;
    let dead = &mut world.step.dead;
    let spare = &mut world.step.spare;
    let pending = &mut world.step.pending;
    let made = &mut world.step.made;
    let area = |c: usize| i64::from(sphere.area_km2[c]);
    let (weight, floor) = (world.step.weight, world.step.floor);
    let key = move |p: u8, class: u8| precedence(&weight, &floor, p, class);
    // The view: each cell of the grid shows the crust over its
    // centre, if any.
    for q in 0..CELLS {
        top[q] = if land.near[q].is_empty() {
            NO_BIT
        } else {
            land.cover(sphere, (sphere.centre[q], q), &key, NO_BIT)
                .map_or(NO_BIT, |j| j as u32)
        };
    }
    // Elsewhere each cell shows its winning bit of floor: the
    // older ocean sinks under the younger, and a sunk bit is gone.
    // The riding edge of an ocean grows an island arc: new crust,
    // in the lattice of the arc beside it if there is one.
    let mut birth: [u32; MAXP] = [NO_BIT; MAXP];
    for q in 0..CELLS {
        let here: Vec<u32> = order[start[q] as usize..start[q + 1] as usize]
            .iter()
            .copied()
            .filter(|&i| !dead[i as usize])
            .collect();
        if top[q] != NO_BIT {
            // Floor beside a coast, not under it: kept for the
            // holes of the floor's sampling.
            disp[q] = NO_BIT;
            spare[q] = here;
            continue;
        }
        if here.is_empty() {
            pending[q] = true;
            disp[q] = NO_BIT;
            continue;
        }
        let x = sphere.centre[q];
        let pw = here
            .iter()
            .map(|&i| bits[i as usize].plate)
            .max_by_key(|&p| key(p, OCEAN))
            .expect("a bit");
        let iw = here
            .iter()
            .copied()
            .filter(|&i| bits[i as usize].plate == pw)
            .max_by_key(|&i| {
                let b = &bits[i as usize];
                (b.seen, dot(b.pos, x), std::cmp::Reverse(i))
            })
            .expect("the winner's bit");
        let mut r = bits[iw as usize].rock;
        let mut over_ocean = false;
        for &j in &here {
            if j == iw {
                continue;
            }
            if bits[j as usize].plate == pw {
                spare[q].push(j);
            } else {
                dead[j as usize] = true;
                over_ocean = true;
            }
        }
        plate[q] = pw;
        if over_ocean {
            r.arc = r.arc.saturating_add(1);
            if r.arc >= arc_steps && hash(&[seed, P_ARC, u64::from(r.home)]) % 1000 < ARC_SHARE {
                let sinking = here
                    .iter()
                    .map(|&j| bits[j as usize].plate)
                    .find(|&p| p != pw)
                    .unwrap_or(NONE);
                journal.push(Event::plates(
                    What::IslandsRisen,
                    (pw as usize, sinking as usize),
                    x,
                    area(q),
                ));
                r.class = TERRANE;
                r.piece = ARC_BASE + pw;
                r.born_my = now_my;
                r.margins |= ACTIVE;
                r.orogeny = now_my;
                r.thick = TERRANE_THICK;
                r.high = ARC_ISLAND_M;
                r.cool = 0;
                r.sunk = 0;
                r.sag = 0;
                r.why = [0; CAUSES];
                *made += area(q);
                // The floor there is crust now.
                dead[iw as usize] = true;
                let beside = sphere.links[q]
                    .iter()
                    .filter(|&&(m, _)| top[m as usize] != NO_BIT)
                    .map(|&(m, _)| land.cells[top[m as usize] as usize])
                    .filter(|c| c.plate == pw)
                    .map(|c| {
                        let back = &land.back[c.frame as usize];
                        (c.frame, sphere.cell_of(normalize(apply(back, x))) as u32)
                    })
                    .find(|&(f, a)| land.get(f, a).is_none());
                let place = beside.unwrap_or_else(|| {
                    if birth[pw as usize] == NO_BIT {
                        birth[pw as usize] = land.frame(IDENTITY);
                    }
                    (birth[pw as usize], q as u32)
                });
                top[q] = land.add(sphere, place, pw, r, *next_bit) as u32;
                *next_bit += 1;
                rock[q] = r;
                disp[q] = NO_BIT;
                continue;
            }
        }
        rock[q] = r;
        disp[q] = iw;
        bits[iw as usize].plate = pw;
        bits[iw as usize].rock = r;
    }
    paint(&*land, &*top, &mut *plate, &mut *rock);
}
