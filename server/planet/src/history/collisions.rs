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

/// Crust piled on a plate: `vol` (m of thickness times km²) thickens the
/// crust of the plate of the cell of crust `j` within [`PILE_REACH`]
/// cells of it, `j` itself or not (`on_it`), the thinnest first: it is
/// shared so that the thinnest cells there rise to one thickness, as
/// water fills the low ground first. Where the pile already stands high
/// it is less work to thicken the plain next to it, so a range that goes
/// on being squeezed widens (a plateau), and no ceiling is set on its
/// thickness. (Until version 19 the whole of a cell's crust went to the
/// one thinnest cell, 35 km at once: a range was a scatter of towers
/// among plains, each with its full height as relief, and wear took a
/// third of a pile in its first step.) The volume is kept: each cell
/// rises by its share of the crust's volume over the area it stands
/// for. Gives the cells that took it; none where no crust of that plate
/// lies within reach.
pub(super) fn pile(
    land: &mut Land,
    sphere: &Sphere,
    j: usize,
    on_it: bool,
    vol: i64,
) -> Vec<usize> {
    let plate = land.cells[j].plate;
    let fits = |k: usize| {
        let o = &land.cells[k];
        !o.dead && !o.under && o.plate == plate
    };
    // The crust there and, where other pieces of its plate lie against
    // it (a docked terrane, a welded continent), their crust at the same
    // place and around it.
    let mut near = vec![j];
    let mut seen = std::collections::HashSet::from([j]);
    for k in land.beside(sphere, j) {
        if fits(k) && seen.insert(k) {
            near.push(k);
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
    near.retain(|&k| on_it || k != j);
    near.sort_unstable_by_key(|&k| (land.cells[k].rock.crust_m(), land.cells[k].id));
    // The thickness the thinnest of them rise to.
    let (mut km2, mut held, mut level, mut takers) = (0_i64, 0_i64, 0_i64, 0);
    for (n, &k) in near.iter().enumerate() {
        let c = &land.cells[k];
        if n > 0 && level <= c.rock.crust_m() {
            break;
        }
        km2 += i64::from(c.area).max(1);
        held += i64::from(c.area).max(1) * c.rock.crust_m();
        level = (vol + held) / km2;
        takers = n + 1;
    }
    near.truncate(takers);
    for &k in &near {
        let r = &mut land.cells[k].rock;
        r.thicken(level - r.crust_m(), BY_DOCK);
    }
    near
}

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
    for (a, b, held) in std::mem::take(&mut world.locked) {
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
            r.held = held;
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
        // crust of that plate near it ([`pile`]). (The sediment on it is
        // crust like the rest once piled.)
        let vol = (me.rock.crust_m() + i64::from(me.rock.silt)) * i64::from(me.area);
        let takers = pile(land, sphere, j, true, vol);
        if above.plate != me.plate {
            for to in takers {
                land.cells[to].rock.orogeny = now_my;
            }
        }
        *shortened += i64::from(me.area);
        land.kill(i);
    }
}
/// Step 19. Collisions begin, last and end; the piles at their fronts,
/// whose weight pushes back on the plates at the next step, and their
/// sutures, which hold the two plates together against a pull
/// (`forces::drive_from`).
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
    let mut seams: Vec<Suture> = Vec::new();
    let heat = &world.heat;
    let hold = |c: usize| holds(heat[c], &rock[c], now_my, s.slab_age_my);
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
            // The section that joins the two continents there holds as
            // the weaker of its two sides.
            match seams.last_mut() {
                Some(f) if f.0 == c && f.2 == pm as u8 => {
                    f.3 = add(f.3, normal);
                    f.4 = f.4.min(hold(m));
                }
                _ => seams.push((c, pc as u8, pm as u8, normal, hold(c).min(hold(m)))),
            }
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
                        held: false,
                        end: 0,
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
                    let r = &mut collisions[record_of[a][b]];
                    r.ended_my = now_my;
                    r.end = if cnt[a][b] > 0 {
                        ENDED_PARTED
                    } else {
                        ENDED_APART
                    };
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
            r.end = ENDED_TAKEN;
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
    // A suture: the front of a collision that has closed. The crust of
    // one continent has run under the other's there and is piled on it:
    // one crust joins the two plates across it.
    seams.retain(|f| {
        let k = record_of[f.1 as usize][f.2 as usize];
        contact[f.1 as usize][f.2 as usize] != 0 && k != usize::MAX && collisions[k].closed_km > 0
    });
    for f in &mut seams {
        f.3 = normalize(f.3);
    }
    world.step.sutures = seams;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piled_crust_fills_the_thinnest_crust_first_and_keeps_its_volume() {
        let sphere = Sphere::new();
        // A cell of crust and its six neighbours, one of them stretched
        // to 20 km.
        let mut land = Land::new();
        let frame = land.frame(IDENTITY);
        let middle = 1_000;
        let mut at = vec![middle];
        at.extend(sphere.links[middle as usize].iter().map(|&(n, _)| n));
        for (k, &a) in at.iter().enumerate() {
            let thick = if k == 1 { 200 } else { CONTINENT_THICK };
            land.add(
                &sphere,
                (frame, a),
                0,
                Rock::plain(CONTINENT, thick),
                k as u32,
            );
        }
        let volume = |land: &Land| -> i64 {
            land.cells
                .iter()
                .map(|c| c.rock.crust_m() * i64::from(c.area))
                .sum()
        };
        let (was, km2) = (volume(&land), i64::from(land.cells[0].area));
        // A cell's worth of crust, 35 km of it, piled there: the thin
        // cell rises first, then all of them together; no tower.
        let vol = 35_000 * km2;
        let takers = pile(&mut land, &sphere, 0, true, vol);
        assert_eq!(takers.len(), land.cells.len());
        let thick: Vec<i64> = land.cells.iter().map(|c| c.rock.crust_m()).collect();
        let (least, most) = (*thick.iter().min().unwrap(), *thick.iter().max().unwrap());
        assert!(most - least <= 1, "{thick:?}");
        assert!((37_000..39_000).contains(&most), "{thick:?}");
        let all: i64 = land.cells.iter().map(|c| i64::from(c.area)).sum();
        assert!(
            (volume(&land) - was - vol).abs() <= all,
            "the volume is kept to the metre"
        );
        // A little crust goes to the thinnest only; and not to the cell
        // it comes from when that cell is the one folded.
        land.cells[2].rock.thicken(-5_000, BY_STRETCH);
        assert_eq!(pile(&mut land, &sphere, 0, true, 1_000 * km2), vec![2]);
        land.cells[0].rock.thicken(-9_000, BY_STRETCH);
        assert!(!pile(&mut land, &sphere, 0, false, 1_000 * km2).contains(&0));
    }
}
