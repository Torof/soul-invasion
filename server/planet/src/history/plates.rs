//! The plates as sets of cells: a plate is one piece (parts cut off become
//! plates or are captured, crumbs and specks absorbed), plates merged
//! (`absorb`), plates parted.

use super::*;

/// A part of a plate cut off from the rest is taken by the plate it
/// touches most if it is smaller than this, km²; larger, it is a plate of
/// its own. A plate is one piece.
pub(super) const PART_KM2: i64 = 2_000_000;
/// A plate smaller than this, km², is a crumb the grid cannot carry (a
/// dozen cells): the plate it touches most takes it.
pub(super) const CRUMB_KM2: i64 = 100_000;
/// A plate enclosed in one other plate and smaller than this, km², is
/// taken by it: a small plate lives wedged between plates, at a trench or
/// a ridge, not inside one.
pub(super) const ENCLOSED_KM2: i64 = 5_000_000;

pub(super) fn alias_of(alias: &[u8; MAXP], mut p: u8) -> u8 {
    while alias[p as usize] != p {
        p = alias[p as usize];
    }
    p
}

pub(super) fn root(up: &[u8; MAXP], mut p: u8) -> u8 {
    while up[p as usize] != p {
        p = up[p as usize];
    }
    p
}

/// The plate `absorbed` joins the plate `kept`: its cells, its floor, its
/// crust and its record with the other plates.
pub(super) fn absorb(
    (kept, absorbed, why): (usize, usize, u8),
    plate: &mut [u8],
    (bits, land): (&mut [Bit], &mut Land),
    alias: &mut [u8; MAXP],
    parted: &mut [[bool; MAXP]; MAXP],
    broken: &mut [[bool; MAXP]; MAXP],
    contact: &mut [[usize; MAXP]],
) {
    for p in plate.iter_mut() {
        if *p as usize == absorbed {
            *p = kept as u8;
        }
    }
    for bit in bits.iter_mut() {
        if bit.plate as usize == absorbed {
            bit.plate = kept as u8;
        }
    }
    for c in &mut land.cells {
        if c.plate as usize == absorbed {
            c.plate = kept as u8;
            c.switch = why;
        }
    }
    alias[absorbed] = kept as u8;
    for o in 0..MAXP {
        parted[kept][o] &= parted[absorbed][o];
        parted[o][kept] = parted[kept][o];
        broken[kept][o] |= broken[absorbed][o];
        broken[o][kept] = broken[kept][o];
        contact[absorbed][o] = 0;
        contact[o][absorbed] = 0;
    }
}

/// A connected part of a plate: its plate, its area in km², and the cells
/// along its edge by the plate across.
pub(super) type Part = (u8, i64, [u32; MAXP]);

/// The parts of the plates: each cell's part, and the parts.
pub(super) fn parts(sphere: &Sphere, plate: &[u8]) -> (Vec<u32>, Vec<Part>) {
    let mut part = vec![u32::MAX; CELLS];
    let mut out: Vec<Part> = Vec::new();
    let mut stack = Vec::new();
    for start in 0..CELLS {
        if part[start] != u32::MAX {
            continue;
        }
        let k = out.len() as u32;
        let id = plate[start];
        let (mut km2, mut edge) = (0, [0_u32; MAXP]);
        part[start] = k;
        stack.push(start);
        while let Some(c) = stack.pop() {
            km2 += i64::from(sphere.area_km2[c]);
            for &(n, _) in &sphere.links[c] {
                let n = n as usize;
                if plate[n] != id {
                    edge[plate[n] as usize] += 1;
                } else if part[n] == u32::MAX {
                    part[n] = k;
                    stack.push(n);
                }
            }
        }
        out.push((id, km2, edge));
    }
    (part, out)
}

/// Step 8. A lone cell of a plate inside another joins the plate around
/// it.
pub(super) fn specks(world: &mut World) {
    let sphere = world.sphere;
    let land = &mut world.land;
    let bits = &mut world.bits;
    let disp = &mut world.disp;
    let top = &mut world.top;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let at = &mut world.step.at;
    let drawn = &mut world.step.drawn;
    // Where each bit of floor is drawn: its cell, or the cell it
    // fills.
    *drawn = std::mem::take(at);
    drawn.resize(bits.len(), 0);
    for c in 0..CELLS {
        if disp[c] != NO_BIT {
            drawn[disp[c] as usize] = c as u32;
        }
    }
    paint(&*land, &*top, &mut *plate, &mut *rock);
    // A lone cell of a plate inside another (a speck left by a
    // transfer or a gap) joins the plate around it.
    let mut relabel: Vec<(usize, u8, u8)> = Vec::new();
    for c in 0..CELLS {
        let same = sphere.links[c]
            .iter()
            .filter(|&&(m, _)| plate[m as usize] == plate[c])
            .count();
        if same > usize::from(rock[c].class == OCEAN) {
            continue;
        }
        // Not a speck if its crust holds to its plate's in its own
        // lattice (version 11).
        if top[c] != NO_BIT
            && land
                .kin(sphere, top[c] as usize)
                .any(|(j, _)| land.cells[j].plate == plate[c])
        {
            continue;
        }
        let mut tally: [(u8, u8); 8] = [(NONE, 0); 8];
        for &(m, _) in &sphere.links[c] {
            let o = plate[m as usize];
            if o == plate[c] {
                continue;
            }
            if let Some(t) = tally.iter_mut().find(|t| t.0 == o || t.0 == NONE) {
                t.0 = o;
                t.1 += 1;
            }
        }
        if let Some(&(o, _)) = tally
            .iter()
            .filter(|t| t.0 != NONE)
            .max_by_key(|t| (t.1, std::cmp::Reverse(t.0)))
        {
            relabel.push((c, plate[c], o));
            plate[c] = o;
        }
    }
    if !relabel.is_empty() {
        let mut to = vec![(NONE, NONE); CELLS];
        for &(c, from, o) in &relabel {
            to[c] = (from, o);
        }
        for (i, b) in bits.iter_mut().enumerate() {
            let (from, o) = to[drawn[i] as usize];
            if from != NONE && b.plate == from {
                b.plate = o;
            }
        }
        let seat = seats(&*land, &*top);
        for (i, c) in land.cells.iter_mut().enumerate() {
            let (from, o) = to[seat[i] as usize];
            if from != NONE && c.plate == from {
                c.plate = o;
                c.switch = SWITCH_SPECK;
                c.rock.margins |= ACCRETED;
            }
        }
    }
}
/// Step 9 (the plates). A plate is one piece: parts cut off become plates
/// or are captured, crumbs are absorbed; plates whose continents no
/// longer touch have parted.
pub(super) fn one_piece(world: &mut World) {
    let journal = &mut world.journal;
    let sphere = world.sphere;
    let parted = &mut world.parted;
    let broken = &mut world.broken;
    let alias = &mut world.alias;
    let born_t = &mut world.born_t;
    let next_id = &mut world.next_id;
    let captures = &mut world.captures;
    let cut_off = &mut world.cut_off;
    let built_km2 = &mut world.built_km2;
    let contact = &mut world.contact;
    let land = &mut world.land;
    let bits = &mut world.bits;
    let disp = &mut world.disp;
    let top = &mut world.top;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let omega = &mut world.omega;
    let group = &mut world.group;
    let now_my = world.step.now_my;
    let cont_of = &mut world.step.cont_of;
    let drawn = &mut world.step.drawn;
    let built = &mut world.step.built;
    // A plate is one piece. A small part cut off from the rest of
    // its plate is taken by the plate it touches most; a large one
    // is a plate of its own from now on.
    let (part, list) = parts(sphere, &*plate);
    // Parts the grid's view shows apart but whose continent is
    // one in its own lattice are one part (version 11): a
    // continent is not cut from its plate by the way the view
    // draws a neck of it.
    let mut one: Vec<u32> = (0..list.len() as u32).collect();
    let find = |one: &mut Vec<u32>, mut k: u32| {
        while one[k as usize] != k {
            one[k as usize] = one[one[k as usize] as usize];
            k = one[k as usize];
        }
        k
    };
    let seat = seats(&*land, &*top);
    for i in 0..land.cells.len() {
        let c = &land.cells[i];
        let a = part[seat[i] as usize];
        if c.dead || list[a as usize].0 != c.plate {
            continue;
        }
        for (j, _) in land.kin(sphere, i) {
            let b = part[seat[j] as usize];
            if a != b && land.cells[j].plate == c.plate && list[b as usize].0 == c.plate {
                let (ra, rb) = (find(&mut one, a), find(&mut one, b));
                one[ra.max(rb) as usize] = ra.min(rb);
            }
        }
    }
    let mut whole = vec![0_i64; list.len()];
    for k in 0..list.len() {
        let r = find(&mut one, k as u32) as usize;
        whole[r] += list[k].1;
    }
    let mut main = [(0_i64, u32::MAX); MAXP];
    for (k, &(id, _, _)) in list.iter().enumerate() {
        if one[k] == k as u32 && whole[k] > main[id as usize].0 {
            main[id as usize] = (whole[k], k as u32);
        }
    }
    let most = |edge: &[u32; MAXP]| {
        (0..MAXP)
            .filter(|&p| edge[p] > 0)
            .max_by_key(|&p| (edge[p], std::cmp::Reverse(p)))
    };
    let mut to = vec![NONE; list.len()];
    let mut fresh_plate = [false; MAXP];
    for (k, &(id, km2, edge)) in list.iter().enumerate() {
        if main[id as usize].1 == find(&mut one, k as u32) {
            continue;
        }
        let old = id as usize;
        let enclosed = edge.iter().filter(|&&n| n > 0).count() == 1 && km2 < ENCLOSED_KM2;
        if km2 >= PART_KM2 && !enclosed && *next_id < MAXP {
            let new = *next_id;
            *next_id += 1;
            to[k] = new as u8;
            fresh_plate[new] = true;
            for p in 0..MAXP {
                parted[new][p] = parted[old][p];
                parted[p][new] = parted[p][old];
                broken[new][p] = broken[old][p];
                broken[p][new] = broken[p][old];
            }
            parted[old][new] = true;
            parted[new][old] = true;
            born_t[new] = born_t[old];
            group[new] = group[old];
            omega[new] = omega[old];
            *cut_off += 1;
            journal.push(Event::plates(
                What::BornCutOff,
                (new, old),
                middle(sphere, |c| part[c] as usize == k),
                km2,
            ));
        } else if let Some(by) = most(&edge) {
            to[k] = by as u8;
            journal.push(Event::plates(
                What::PartTaken,
                (old, by),
                middle(sphere, |c| part[c] as usize == k),
                km2,
            ));
            captures.push(Capture {
                my: now_my,
                plate: id,
                by: by as u8,
                km2,
                whole: false,
            });
        }
    }
    if to.iter().any(|&p| p != NONE) {
        for c in 0..CELLS {
            if to[part[c] as usize] != NONE {
                plate[c] = to[part[c] as usize];
            }
        }
        for (i, b) in bits.iter_mut().enumerate() {
            let k = part[drawn[i] as usize] as usize;
            if to[k] != NONE && b.plate == list[k].0 {
                b.plate = to[k];
            }
        }
        let seat = seats(&*land, &*top);
        for (i, c) in land.cells.iter_mut().enumerate() {
            let k = part[seat[i] as usize] as usize;
            if to[k] != NONE && c.plate == list[k].0 {
                c.plate = to[k];
                c.switch = if fresh_plate[to[k] as usize] {
                    SWITCH_CUT_OFF
                } else {
                    SWITCH_CAPTURE
                };
            }
        }
    }
    // A crumb of a plate is taken by the plate it touches most, and
    // a small plate enclosed in one other plate by that plate.
    // (A plate is counted whole: its parts joined through their
    // continent's own lattice are one.)
    let (_, list) = parts(sphere, &*plate);
    let mut whole: Vec<Part> = Vec::new();
    for &(id, km2, edge) in &list {
        if let Some(w) = whole.iter_mut().find(|w| w.0 == id) {
            w.1 += km2;
            for p in 0..MAXP {
                w.2[p] += edge[p];
            }
        } else {
            whole.push((id, km2, edge));
        }
    }
    let mut gone = [false; MAXP];
    for &(id, km2, edge) in &whole {
        let around = (0..MAXP).filter(|&p| edge[p] > 0).count();
        let Some(by) = most(&edge) else {
            continue;
        };
        let by = alias_of(&*alias, by as u8) as usize;
        let enclosed = around == 1 && km2 < ENCLOSED_KM2;
        if (km2 >= CRUMB_KM2 && !enclosed) || gone[by] || by == id as usize {
            continue;
        }
        gone[id as usize] = true;
        journal.push(Event::plates(
            if enclosed {
                What::EndedEnclosed
            } else {
                What::EndedCrumb
            },
            (id as usize, by),
            middle(sphere, |c| plate[c] == id),
            km2,
        ));
        absorb(
            (by, id as usize, SWITCH_CRUMB),
            &mut *plate,
            (&mut *bits, &mut *land),
            &mut *alias,
            &mut *parted,
            &mut *broken,
            &mut *contact,
        );
        omega[id as usize] = [0; 3];
        captures.push(Capture {
            my: now_my,
            plate: id,
            by: by as u8,
            km2,
            whole: true,
        });
    }
    built_km2.push(*built);
    sync(&mut *bits, &*plate, &*rock, &*disp);
    paint(&*land, &*top, &mut *plate, &mut *rock);
    let mut cells_of = [0_i64; MAXP];
    for c in 0..CELLS {
        cells_of[plate[c] as usize] += 1;
    }
    *cont_of = land.weight();
    // Plates whose continents no longer touch have parted.
    let mut touching = vec![[false; MAXP]; MAXP];
    for c in 0..CELLS {
        if rock[c].class == OCEAN {
            continue;
        }
        for &(m, _) in &sphere.links[c] {
            let m = m as usize;
            if rock[m].class != OCEAN && plate[m] != plate[c] {
                touching[plate[c] as usize][plate[m] as usize] = true;
            }
        }
    }
    for a in 0..MAXP {
        for b in 0..MAXP {
            if !touching[a][b] && !touching[b][a] {
                parted[a][b] = true;
            }
        }
    }
}
