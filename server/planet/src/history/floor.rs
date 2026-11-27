//! The ocean floor's holes and gaps: holes of the floor's sampling mended,
//! true gaps between parting plates filled with new floor (ridges), the
//! rift margins beside it.

use super::*;

/// A cell left empty by the grid's sampling takes a bit of its plate not
/// shown nearby, within this, km (first its neighbours, then two rings).
pub(super) const HOLE_KM: [i64; 2] = [120, 190];
/// A cell still empty with at least this many neighbours shown, all of
/// one plate, is a hole of the sampling.
pub(super) const HOLE_AROUND: usize = 6;
/// Such a cell between two plates is a gap (new ocean floor) only if they
/// part there at least this fast, µm a year; else it is a hole of the
/// sampling too: no floor is born in a trench or in a range.
pub(super) const GAP_LEAST_UM: i64 = 1_000;

/// Steps 6 and 7. Holes of the floor's sampling are mended; true gaps
/// between parting plates are filled with new ocean floor, and the crust
/// beside it is a rift's margin.
pub(super) fn holes_and_gaps(world: &mut World) {
    let sphere = world.sphere;
    let land = &mut world.land;
    let bits = &mut world.bits;
    let disp = &mut world.disp;
    let top = &mut world.top;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let group = &mut world.group;
    let next_bit = &mut world.next_bit;
    let now_my = world.step.now_my;
    let moved = &mut world.step.moved;
    let n = world.step.n;
    let start = &mut world.step.start;
    let order = &mut world.step.order;
    let old_plate = &mut world.step.old_plate;
    let old_rock = &mut world.step.old_rock;
    let dead = &mut world.step.dead;
    let spare = &mut world.step.spare;
    let pending = &mut world.step.pending;
    let (weight, floor) = (world.step.weight, world.step.floor);
    let key = move |p: u8, class: u8| precedence(&weight, &floor, p, class);
    // A lower plate's floor showing through a hole of the sampling
    // of the plates riding over it has sunk under them: the cell is
    // a hole of theirs.
    let over: Vec<(i64, i64, i64)> = (0..CELLS)
        .map(|q| {
            if pending[q] {
                (i64::MIN, 0, 0)
            } else {
                key(plate[q], rock[q].class)
            }
        })
        .collect();
    let through: Vec<usize> = (0..CELLS)
        .filter(|&q| {
            !pending[q]
                && rock[q].class == OCEAN
                && sphere.links[q]
                    .iter()
                    .filter(|&&(m, _)| {
                        let m = m as usize;
                        group[plate[m] as usize] != group[plate[q] as usize] && over[m] > over[q]
                    })
                    .count()
                    >= HOLE_AROUND
        })
        .collect();
    for q in through {
        for &i in &order[start[q] as usize..start[q + 1] as usize] {
            dead[i as usize] = true;
        }
        spare[q].clear();
        pending[q] = true;
        disp[q] = NO_BIT;
    }
    // 6. A cell the floor's sampling left empty inside a plate
    // shows a bit of floor not shown nearby (none is copied).
    let mut used = vec![false; n];
    for (pass, reach) in HOLE_KM.iter().enumerate() {
        for q in 0..CELLS {
            if !pending[q] {
                continue;
            }
            let x = sphere.centre[q];
            let mut ring: Vec<usize> = sphere.links[q].iter().map(|&(m, _)| m as usize).collect();
            if pass == 1 {
                let first = ring.clone();
                for m in first {
                    ring.extend(sphere.links[m].iter().map(|&(k, _)| k as usize));
                }
            }
            let mut best: Option<(bool, i64, u32)> = None;
            for &m in &ring {
                for &j in &spare[m] {
                    if used[j as usize] || dead[j as usize] {
                        continue;
                    }
                    let d = km_between(bits[j as usize].pos, x);
                    let k = (!bits[j as usize].seen, d, j);
                    if d < *reach && best.is_none_or(|b| k < b) {
                        best = Some(k);
                    }
                }
            }
            if let Some((_, _, j)) = best {
                used[j as usize] = true;
                pending[q] = false;
                plate[q] = bits[j as usize].plate;
                rock[q] = bits[j as usize].rock;
                disp[q] = j;
            }
        }
    }
    // A cell of floor still empty with its plate all around is a
    // hole of the floor's sampling, not a gap: it draws the floor
    // beside it again (the floor it showed, between two pieces of
    // continent with no floor beside), and no floor is made.
    // Empty cells walled in by plates that move as one are holes
    // of the sampling whatever their number (version 9): a rigid
    // plate does not open, and no ocean floor is born inside it.
    let ocean_rock = |q: usize| Rock {
        class: OCEAN,
        piece: NONE,
        margins: 0,
        arc: 0,
        rift: 0,
        born_my: now_my,
        orogeny: NEVER,
        home: q as u32,
        thick: OCEAN_THICK,
        high: 0,
        cool: 0,
        sunk: 0,
        sag: 0,
        why: [0; CAUSES],
    };
    let again = |q: usize, m: usize, rock: &[Rock], pending: &[bool]| {
        if rock[m].class == OCEAN {
            rock[m]
        } else if let Some(&(_, k)) = sphere.links[q]
            .iter()
            .filter(|&&(k, _)| !pending[k as usize] && rock[k as usize].class == OCEAN)
            .map(|&(k, km)| (km, k as usize))
            .min()
            .as_ref()
        {
            rock[k]
        } else if old_rock[q].class == OCEAN {
            old_rock[q]
        } else {
            ocean_rock(q)
        }
    };
    let mut inner = vec![false; CELLS];
    let mut seen_hole = vec![false; CELLS];
    for q0 in 0..CELLS {
        if !pending[q0] || seen_hole[q0] {
            continue;
        }
        let (mut stack, mut cells) = (vec![q0], Vec::new());
        seen_hole[q0] = true;
        let (mut wall, mut one) = (NONE, true);
        while let Some(q) = stack.pop() {
            cells.push(q);
            for &(m, _) in &sphere.links[q] {
                let m = m as usize;
                if pending[m] {
                    if !seen_hole[m] {
                        seen_hole[m] = true;
                        stack.push(m);
                    }
                } else {
                    let g = group[plate[m] as usize];
                    one &= wall == NONE || wall == g;
                    wall = g;
                }
            }
        }
        if one && wall != NONE {
            for q in cells {
                inner[q] = true;
            }
        }
    }
    loop {
        let mut filled = Vec::new();
        for q in 0..CELLS {
            if !pending[q] || !inner[q] {
                continue;
            }
            if let Some((_, m)) = sphere.links[q]
                .iter()
                .filter(|&&(m, _)| !pending[m as usize])
                .map(|&(m, km)| (km, m as usize))
                .min()
            {
                filled.push((q, plate[m], again(q, m, &*rock, &*pending)));
            }
        }
        if filled.is_empty() {
            break;
        }
        for (q, p, r) in filled {
            plate[q] = p;
            rock[q] = r;
            disp[q] = NO_BIT;
            pending[q] = false;
        }
    }
    // A gap beside crust a rift has stretched is no ocean yet (version
    // 12). Thinned crust stands for a wider crust than its place in its
    // lattice: that width now lies in the gap, as thin crust, while
    // there is any left around. Once there is none the crust has given
    // way, and the gap is ocean floor.
    for q in 0..CELLS {
        if !pending[q] {
            continue;
        }
        let spare = |c: &Crust| {
            (i64::from(c.area) - i64::from(sphere.area_km2[c.at as usize])).max(0)
                * i64::from(c.rock.thick)
        };
        let mut donors: Vec<usize> = Vec::new();
        for &(m, _) in &sphere.links[q] {
            if top[m as usize] == NO_BIT {
                continue;
            }
            let i = top[m as usize] as usize;
            for j in std::iter::once(i).chain(land.kin(sphere, i).map(|(j, _)| j)) {
                let c = &land.cells[j];
                if !c.dead && !c.under && c.rock.rift != 0 && spare(c) > 0 && !donors.contains(&j) {
                    donors.push(j);
                }
            }
        }
        if donors.is_empty() {
            continue;
        }
        donors.sort_unstable_by_key(|&j| {
            let c = &land.cells[j];
            (km_between(c.pos, sphere.centre[q]), c.id)
        });
        let first = land.cells[donors[0]];
        let place = sphere.cell_of(normalize(apply(
            &land.back[first.frame as usize],
            sphere.centre[q],
        ))) as u32;
        if land.get(first.frame, place).is_some() {
            continue;
        }
        // Its place must be free of other crust: no crust over its
        // centre, and no other piece's cell centred in it.
        let y = normalize(apply(
            &land.rot[first.frame as usize],
            sphere.centre[place as usize],
        ));
        let m = sphere.cell_of(y);
        let back = &land.back[first.frame as usize];
        let taken = std::iter::once(m)
            .chain(sphere.links[m].iter().map(|&(a, _)| a as usize))
            .filter(|&a| top[a] != NO_BIT)
            .any(|a| {
                let o = &land.cells[top[a] as usize];
                o.frame != first.frame
                    && sphere.cell_of(normalize(apply(back, o.pos))) as u32 == place
            });
        if taken || land.cover(sphere, (y, m), &key, NO_BIT).is_some() {
            continue;
        }
        let full = i64::from(sphere.area_km2[place as usize]);
        let have: i64 = donors.iter().map(|&j| spare(&land.cells[j])).sum();
        let take = have.min(full * i64::from(BREAK_THICK));
        if take < full * i64::from(OCEAN_THICK) {
            continue;
        }
        let mut left = take;
        for &j in &donors {
            let c = &mut land.cells[j];
            let v = spare(c).min(left);
            c.area -= (v / i64::from(c.rock.thick).max(1)) as u32;
            left -= v;
        }
        let made = Rock {
            thick: (take / full) as u16,
            why: [0; CAUSES],
            ..first.rock
        };
        let j = land.add(sphere, (first.frame, place), first.plate, made, *next_bit);
        *next_bit += 1;
        land.cells[j].event = Margin::Rift as u8;
        top[q] = j as u32;
        disp[q] = NO_BIT;
        plate[q] = first.plate;
        rock[q] = made;
        pending[q] = false;
    }
    let mut mended = Vec::new();
    for q in 0..CELLS {
        if !pending[q] {
            continue;
        }
        let around: Vec<(u32, usize)> = sphere.links[q]
            .iter()
            .filter(|&&(m, _)| !pending[m as usize])
            .map(|&(m, km)| (km, m as usize))
            .collect();
        let g = |m: usize| group[plate[m] as usize];
        // Between two plates it is a gap only if they part there.
        let parting = || {
            let &(_, first) = around.iter().min().expect("neighbours");
            let (mut ca, mut cb, mut other) = ([0_i64; 3], [0_i64; 3], first);
            for &(_, m) in &around {
                if g(m) == g(first) {
                    ca = add(ca, sphere.centre[m]);
                } else {
                    cb = add(cb, sphere.centre[m]);
                    other = m;
                }
            }
            let x = sphere.centre[q];
            let across = normalize(tangent(x, sub(normalize(cb), normalize(ca))));
            let rel = sub(
                cross(moved[plate[other] as usize], x),
                cross(moved[plate[first] as usize], x),
            );
            dot(rel, across) > GAP_LEAST_UM
        };
        if around.len() >= HOLE_AROUND
            && (around.iter().all(|&(_, m)| g(m) == g(around[0].1)) || !parting())
        {
            let &(_, m) = around.iter().min().expect("neighbours");
            mended.push((q, plate[m], again(q, m, &*rock, &*pending)));
        }
    }
    for (q, p, r) in mended {
        plate[q] = p;
        rock[q] = r;
        disp[q] = NO_BIT;
        pending[q] = false;
    }
    // 7. Gaps: new ocean floor, filled from both sides at once.
    let mut open: Vec<usize> = (0..CELLS).filter(|&q| pending[q]).collect();
    let born: Vec<usize> = open.clone();
    while !open.is_empty() {
        let mut set = Vec::new();
        let mut next = Vec::new();
        for &q in &open {
            let mut tally: [(u8, u8); 8] = [(NONE, 0); 8];
            for &(m, _) in &sphere.links[q] {
                let m = m as usize;
                if pending[m] {
                    continue;
                }
                if let Some(t) = tally.iter_mut().find(|t| t.0 == plate[m] || t.0 == NONE) {
                    t.0 = plate[m];
                    t.1 += 1;
                }
            }
            match tally
                .iter()
                .filter(|t| t.0 != NONE)
                .max_by_key(|t| (t.1, std::cmp::Reverse(t.0)))
            {
                Some(&(p, _)) => set.push((q, p)),
                None => next.push(q),
            }
        }
        if set.is_empty() {
            break;
        }
        for (q, p) in set {
            pending[q] = false;
            plate[q] = p;
            rock[q] = ocean_rock(q);
        }
        open = next;
    }
    let mut newborn = vec![false; CELLS];
    for &q in &born {
        if pending[q] {
            plate[q] = old_plate[q];
            rock[q] = ocean_rock(q);
            pending[q] = false;
        }
        newborn[q] = true;
        disp[q] = bits.len() as u32;
        bits.push(Bit {
            pos: sphere.centre[q],
            plate: plate[q],
            rock: rock[q],
            seen: true,
            id: *next_bit,
        });
        *next_bit += 1;
    }
    dead.resize(bits.len(), false);
    // The crust beside a floor just born is a rift's margin: the
    // crust the grid draws around the new floor, and the crust
    // whose free neighbour in its own lattice lies in it.
    if !born.is_empty() {
        let mut torn: Vec<usize> = Vec::new();
        for &q in &born {
            for &(m, _) in &sphere.links[q] {
                if top[m as usize] != NO_BIT {
                    torn.push(top[m as usize] as usize);
                }
            }
        }
        for i in 0..land.cells.len() {
            let c = &land.cells[i];
            let rot = &land.rot[c.frame as usize];
            let beside = sphere.links[c.at as usize].iter().any(|&(k, _)| {
                land.get(c.frame, k).is_none()
                    && newborn[sphere.cell_of(normalize(apply(rot, sphere.centre[k as usize])))]
            });
            if beside {
                torn.push(i);
            }
        }
        for i in torn {
            let c = &mut land.cells[i];
            if c.event != Margin::Suture as u8 && !c.under {
                c.rock.margins |= RIFT;
                c.event = Margin::Rift as u8;
            }
        }
    }
}
