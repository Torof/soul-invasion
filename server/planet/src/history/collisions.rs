//! Collisions: pieces that meet and pile up, collisions in progress
//! (contact, closing, the pile's push back on the plates), and their end:
//! the plates part again, or the pile stops them and they are one plate.

use super::*;

/// Crust run under another piece thickens the thinnest crust within this
/// many cells of the crust above it (some 370 km: a range is a few
/// hundred km wide, not one cell of the grid, and the squeeze of five
/// million years reaches that far behind a front), on whatever piece of
/// that plate it lies.
pub(super) const PILE_REACH: usize = 4;

/// Step 2. A collision the pile has stopped: the forces press its two
/// plates together no harder than the thickened crust pushes back
/// (`forces::drive_from`), so they turn as one. Two plates with one turn
/// and one crust across their front are one plate: the record says so.
/// No count of steps, no speed under which they "weld": the balance of
/// the forces decides, and a later pull finds the suture as weak crust
/// (a rift may open it again).
pub(super) fn stopped(world: &mut World) {
    let journal = &mut world.journal;
    let parted = &mut world.parted;
    let broken = &mut world.broken;
    let alias = &mut world.alias;
    let welds = &mut world.welds;
    let collisions = &mut world.collisions;
    let contact = &mut world.contact;
    let record_of = &mut world.record_of;
    let land = &mut world.land;
    let bits = &mut world.bits;
    let plate = &mut world.plate;
    let omega = &mut world.omega;
    let group = &mut world.group;
    let now_my = world.step.now_my;
    let cont_of = &mut world.step.cont_of;
    for (a, b) in std::mem::take(&mut world.locked) {
        let (a, b) = (a as usize, b as usize);
        if contact[a][b] == 0 || alias[a] != a as u8 || alias[b] != b as u8 || omega[a] != omega[b]
        {
            continue;
        }
        let (kept, absorbed) = if (cont_of[a], -(a as i64)) >= (cont_of[b], -(b as i64)) {
            (a, b)
        } else {
            (b, a)
        };
        let mut at = [0; 3];
        if record_of[a][b] != usize::MAX {
            let r = &mut collisions[record_of[a][b]];
            r.welded_my = now_my;
            r.ended_my = now_my;
            r.stopped = true;
            at = r.at;
        }
        journal.push(Event::plates(
            What::EndedWelded,
            (absorbed, kept),
            at,
            cont_of[absorbed],
        ));
        welds.push(Weld {
            my: now_my,
            kept: kept as u8,
            absorbed: absorbed as u8,
            kept_km2: cont_of[kept],
            absorbed_km2: cont_of[absorbed],
            parted: true,
        });
        absorb(
            (kept, absorbed, SWITCH_WELD),
            &mut *plate,
            (&mut *bits, &mut *land),
            &mut *alias,
            &mut *parted,
            &mut *broken,
            &mut *contact,
        );
        // The plates that moved as one with the plate that is gone go
        // on under the least of their own names.
        let gone = group[absorbed];
        group[absorbed] = absorbed as u8;
        if let Some(least) = (0..MAXP).find(|&p| group[p] == gone && p != absorbed) {
            for p in 0..MAXP {
                if group[p] == gone && p != absorbed {
                    group[p] = least as u8;
                }
            }
        }
        omega[absorbed] = [0; 3];
        cont_of[kept] += cont_of[absorbed];
        cont_of[absorbed] = 0;
    }
}
/// Step 5. Pieces that meet: continental crust never sinks, it piles up.
pub(super) fn meet(world: &mut World) {
    let sphere = world.sphere;
    let land = &mut world.land;
    let now_my = world.step.now_my;
    let pressed = &mut world.step.pressed;
    let accrete = &mut world.step.accrete;
    let shortened = &mut world.step.shortened;
    let (weight, floor) = (world.step.weight, world.step.floor);
    let key = move |p: u8, class: u8| precedence(&weight, &floor, p, class);
    // 5. Pieces that meet: continental crust never sinks. A cell
    // of crust whose place lies under another piece's crust (the
    // heavier continent rides) is run under it. A terrane or an
    // arc docks, once: it joins the plate it meets, and is part
    // of that continent from then on. Under a collision's
    // front the crust above is a suture. The crust run under
    // piles on the crust above (thicker: the area shrinks, the
    // volume stays); what a full pile cannot take waits under it.
    for i in 0..land.cells.len() {
        land.cells[i].under = false;
        let me = land.cells[i];
        let q = me.cell as usize;
        if land.near[q].len() < 2 {
            continue;
        }
        let Some(j) = land.cover(sphere, (me.pos, q), &key, me.frame) else {
            continue;
        };
        let above = land.cells[j];
        if (
            key(above.plate, above.rock.class),
            std::cmp::Reverse(above.frame),
        ) < (key(me.plate, me.rock.class), std::cmp::Reverse(me.frame))
        {
            continue;
        }
        land.cells[i].under = true;
        if above.plate != me.plate {
            let up = &mut land.cells[j];
            if me.rock.class == TERRANE && me.rock.margins & ACCRETED == 0 {
                if up.rock.margins & ACCRETED == 0 {
                    up.rock.rise(DOCK_M, BY_DOCK);
                }
                up.rock.margins |= ACCRETED;
                up.rock.orogeny = now_my;
                accrete.push((i, above.plate));
            } else if pressed[q] {
                up.rock.orogeny = now_my;
                up.rock.margins |= SUTURE;
                up.event = Margin::Suture as u8;
            }
        }
        // The crust run under thickens the crust above, or the thinnest
        // crust of that plate within [`PILE_REACH`] cells of it: where
        // the pile already stands high it is less work to thicken the
        // plain next to it, so a range that goes on being squeezed
        // widens (a plateau), and no ceiling is set on its thickness.
        // The volume is kept: the pile rises by the crust's volume over
        // the area the taker stands for.
        let fits = |k: usize| {
            let o = &land.cells[k];
            !o.dead && !o.under && o.plate == above.plate
        };
        // The crust above and, where other pieces of its plate lie
        // against it (a docked terrane, a welded continent), their crust
        // at the same place and around it.
        let mut near = vec![j];
        let mut seen = std::collections::HashSet::from([j]);
        let around = above.cell as usize;
        for &frame in &land.near[around] {
            if frame == above.frame {
                continue;
            }
            for m in
                std::iter::once(around).chain(sphere.links[around].iter().map(|l| l.0 as usize))
            {
                let at = sphere.cell_of(normalize(apply(
                    &land.back[frame as usize],
                    sphere.centre[m],
                )));
                if let Some(k) = land.get(frame, at as u32)
                    && fits(k)
                    && seen.insert(k)
                {
                    near.push(k);
                }
            }
        }
        let mut ring = 0..near.len();
        for _ in 0..PILE_REACH {
            for a in ring.clone() {
                let from = near[a];
                for (k, _) in land.kin(sphere, from) {
                    if fits(k) && seen.insert(k) {
                        near.push(k);
                    }
                }
            }
            ring = ring.end..near.len();
        }
        let to = near
            .into_iter()
            .min_by_key(|&k| (land.cells[k].rock.thick, k != j, land.cells[k].id))
            .unwrap_or(j);
        let vol = i64::from(me.rock.thick) * i64::from(me.area);
        let full = i64::from(land.cells[to].area).max(1);
        let up = &mut land.cells[to].rock;
        up.thick = (i64::from(up.thick) + vol / full).min(i64::from(u16::MAX)) as u16;
        if above.plate != me.plate {
            up.orogeny = now_my;
        }
        *shortened += i64::from(me.area);
        land.kill(i);
    }
}
/// Step 19. Collisions begin, last and end; the piles at their fronts,
/// whose weight pushes back on the plates at the next step.
pub(super) fn progress(world: &mut World) {
    let journal = &mut world.journal;
    let piles = &mut world.step.piles;
    let s = world.s;
    let sphere = world.sphere;
    let parted = &mut world.parted;
    let collisions = &mut world.collisions;
    let contact = &mut world.contact;
    let record_of = &mut world.record_of;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let omega = &mut world.omega;
    let group = &mut world.group;
    let t = world.step.t;
    let now_my = world.step.now_my;
    let seen = &mut world.step.seen;
    // Collisions in progress: a collision begins where a front closes
    // between parted plates, and lasts while their continents touch;
    // how long their contact is, how fast it still closes, where it is,
    // which way it squeezes, and how thick its pile has grown.
    let mut sum = vec![[0_i64; MAXP]; MAXP];
    let mut cnt = vec![[0_i64; MAXP]; MAXP];
    let mut hit = vec![[false; MAXP]; MAXP];
    let mut mid = vec![[[0_i64; 3]; MAXP]; MAXP];
    let mut way = vec![[[0_i64; 3]; MAXP]; MAXP];
    let mut most = vec![[0_u16; MAXP]; MAXP];
    for &(_, p, o, _, _) in &seen.front {
        hit[p.min(o) as usize][p.max(o) as usize] = true;
    }
    // The pile at a cell of a front: the thickest crust of the two
    // plates there or a cell away (a range's crest stands a little
    // behind its front).
    let crest = |c: usize, pair: (usize, usize)| {
        std::iter::once(c)
            .chain(sphere.links[c].iter().map(|&(n, _)| n as usize))
            .filter(|&n| {
                rock[n].class != OCEAN
                    && (plate[n] as usize == pair.0 || plate[n] as usize == pair.1)
            })
            .map(|n| rock[n].thick)
            .max()
            .unwrap_or(0)
    };
    // A continent, or what has docked to one: a terrane or an island arc
    // that meets a continent docks (`trenches::sink_and_dock`), which is
    // no collision of continents.
    let solid =
        |r: &Rock| r.class == CONTINENT || (r.class == TERRANE && r.margins & ACCRETED != 0);
    let mut sides: Vec<Pile> = Vec::new();
    for c in 0..CELLS {
        if !solid(&rock[c]) {
            continue;
        }
        let x = sphere.centre[c];
        let pc = plate[c] as usize;
        for &(m, _) in &sphere.links[c] {
            let m = m as usize;
            let pm = plate[m] as usize;
            if pm <= pc || !solid(&rock[m]) || group[pc] == group[pm] {
                continue;
            }
            let normal = normalize(tangent(x, sub(sphere.centre[m], x)));
            let rel = sub(cross(omega[pc], x), cross(omega[pm], x));
            let thick = crest(c, (pc, pm)).max(crest(m, (pc, pm)));
            sum[pc][pm] += dot(rel, normal);
            cnt[pc][pm] += 1;
            mid[pc][pm] = add(mid[pc][pm], x);
            way[pc][pm] = add(way[pc][pm], normal);
            most[pc][pm] = most[pc][pm].max(thick);
            sides.push((
                c,
                pc as u8,
                pm as u8,
                normal,
                pile_weight(i64::from(thick)),
                false,
            ));
        }
    }
    for a in 0..MAXP {
        for b in a + 1..MAXP {
            // A collision ends where the continents touch no more, or
            // part as fast as any boundary that opens.
            let on = if contact[a][b] == 0 {
                hit[a][b] && parted[a][b] && cnt[a][b] > 0
            } else {
                cnt[a][b] > 0 && sum[a][b] > -FORCE_LEAST_UM * cnt[a][b]
            };
            if on {
                let mean = sum[a][b] / cnt[a][b];
                let at = normalize(mid[a][b]);
                if contact[a][b] == 0 {
                    contact[a][b] = t + 1;
                    record_of[a][b] = collisions.len();
                    journal.push(Event::plates(What::CollisionBegun, (a, b), at, 0));
                    collisions.push(Collision {
                        a: a as u8,
                        b: b as u8,
                        km2: (world.step.cont_of[a], world.step.cont_of[b]),
                        from_my: now_my,
                        welded_my: NEVER,
                        ended_my: NEVER,
                        stopped: false,
                        at,
                        squeeze: [0; 3],
                        thick: 0,
                        closed_km: 0,
                        front_cells: 0,
                        steps: 0,
                    });
                }
                let r = &mut collisions[record_of[a][b]];
                r.closed_km += mean.max(0) * s.step_my / 1000;
                r.front_cells = r.front_cells.max(cnt[a][b]);
                r.steps += 1;
                r.at = at;
                r.squeeze = normalize(tangent(at, way[a][b]));
                r.thick = r.thick.max(most[a][b]);
            } else if contact[a][b] != 0 {
                if record_of[a][b] != usize::MAX {
                    collisions[record_of[a][b]].ended_my = now_my;
                }
                contact[a][b] = 0;
                record_of[a][b] = usize::MAX;
            }
        }
    }
    // A collision whose plate was taken by another (a crumb captured, a
    // plate welded elsewhere) is over.
    for (k, r) in collisions.iter_mut().enumerate() {
        if r.ended_my == NEVER && record_of[r.a as usize][r.b as usize] != k {
            r.ended_my = now_my;
        }
    }
    // A pile pushes back wherever two plates' continents touch, a
    // collision on record or not (two plates a rift has cut that press
    // on each other again pile up as well).
    sides.retain(|f| f.4 > 0);
    for f in &mut sides {
        f.5 = contact[f.1 as usize][f.2 as usize] != 0;
    }
    *piles = sides;
}
