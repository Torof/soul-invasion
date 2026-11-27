//! Collisions: pieces that meet and pile up, collisions in progress
//! (contact, closing, standing still), the weld that ends them, the block
//! that escapes sideways.

use super::*;

/// Two colliding plates weld once they have stood still this many steps.
pub(super) const WELD_QUIET: usize = 4;

/// An escape: after this many steps of collision still closing at least
/// this fast (µm a year) on this many front cells, a block of the riding
/// plate this wide (km) is pushed out sideways at this speed (mm a year).
pub(super) const ESCAPE_AFTER: usize = 3;
pub(super) const ESCAPE_MIN_UM: i64 = 20_000;
pub(super) const ESCAPE_CELLS: i64 = 40;
/// Only a riding plate carrying this much continent sheds a block, km².
pub(super) const ESCAPE_FROM_KM2: i64 = 4_000_000;
pub(super) const ESCAPE_KM: i64 = 900;
pub(super) const ESCAPE_MM: i64 = 25;

/// Step 3. Collisions in progress: they weld once they have stopped
/// closing; while they still close hard, a block of the riding plate may
/// escape sideways.
pub(super) fn weld_and_escape(world: &mut World) {
    let journal = &mut world.journal;
    let seed = world.seed;
    let sphere = world.sphere;
    let pushes = &mut world.pushes;
    let parted = &mut world.parted;
    let broken = &mut world.broken;
    let alias = &mut world.alias;
    let born_t = &mut world.born_t;
    let next_id = &mut world.next_id;
    let welds = &mut world.welds;
    let collisions = &mut world.collisions;
    let escapes = &mut world.escapes;
    let contact = &mut world.contact;
    let closing_of = &mut world.closing_of;
    let quiet = &mut world.quiet;
    let front_of = &mut world.front_of;
    let record_of = &mut world.record_of;
    let escaped = &mut world.escaped;
    let shed = &mut world.shed;
    let young = world.young;
    let land = &mut world.land;
    let bits = &mut world.bits;
    let disp = &mut world.disp;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let omega = &mut world.omega;
    let group = &mut world.group;
    let fronts = &mut world.fronts;
    let t = world.step.t;
    let now_my = world.step.now_my;
    let cont_of = &mut world.step.cont_of;
    let area = |c: usize| i64::from(sphere.area_km2[c]);
    let mut cells_of = [0_i64; MAXP];
    for c in 0..CELLS {
        cells_of[plate[c] as usize] += 1;
    }
    // 3. Collisions in progress: they weld once they have stopped
    // closing; while they still close hard, a block of the riding
    // plate may escape sideways.
    for a in 0..MAXP {
        for b in a + 1..MAXP {
            if contact[a][b] == 0 || alias[a] != a as u8 || alias[b] != b as u8 {
                continue;
            }
            let since = contact[a][b] - 1;
            let closing = closing_of[a][b];
            if parted[a][b]
                && t - since >= 2
                && quiet[a][b] >= WELD_QUIET
                && t - born_t[a] >= young
                && t - born_t[b] >= young
            {
                let (kept, absorbed) = if (cont_of[a], -(a as i64)) >= (cont_of[b], -(b as i64)) {
                    (a, b)
                } else {
                    (b, a)
                };
                let front = fronts
                    .iter()
                    .filter(|f| {
                        (f.1 as usize, f.2 as usize) == (a, b)
                            || (f.1 as usize, f.2 as usize) == (b, a)
                    })
                    .fold([0; 3], |acc, f| add(acc, sphere.centre[f.0]));
                journal.push(Event::plates(
                    What::EndedWelded,
                    (absorbed, kept),
                    normalize(front),
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
                if record_of[a][b] != usize::MAX {
                    collisions[record_of[a][b]].welded_my = now_my;
                }
                absorb(
                    (kept, absorbed, SWITCH_WELD),
                    &mut *plate,
                    (&mut *bits, &mut *land),
                    &mut *alias,
                    &mut *parted,
                    &mut *broken,
                    &mut *contact,
                );
                let (nk, nx) = (cells_of[kept].max(1), cells_of[absorbed]);
                omega[kept] =
                    [0, 1, 2].map(|i| (omega[kept][i] * nk + omega[absorbed][i] * nx) / (nk + nx));
                omega[absorbed] = [0; 3];
                cells_of[kept] += cells_of[absorbed];
                cont_of[kept] += cont_of[absorbed];
                cells_of[absorbed] = 0;
                cont_of[absorbed] = 0;
            } else if t - since >= ESCAPE_AFTER
                && closing >= ESCAPE_MIN_UM
                && front_of[a][b] >= ESCAPE_CELLS
                && !escaped[a][b]
                && *next_id < MAXP
            {
                escaped[a][b] = true;
                let (over, ind) = if cont_of[a] >= cont_of[b] {
                    (a, b)
                } else {
                    (b, a)
                };
                if cont_of[over] < ESCAPE_FROM_KM2 || t - born_t[over] < young || shed[over] {
                    continue;
                }
                shed[over] = true;
                let line: Vec<(V3, V3)> = fronts
                    .iter()
                    .filter(|f| f.1 as usize == over && f.2 as usize == ind)
                    .map(|f| (sphere.centre[f.0], f.3))
                    .collect();
                if line.len() < 2 {
                    continue;
                }
                let mid = normalize(line.iter().fold([0; 3], |acc, l| add(acc, l.0)));
                let toward = normalize(line.iter().fold([0; 3], |acc, l| add(acc, l.1)));
                let along = normalize(tangent(mid, cross(mid, toward)));
                let sign = if hash(&[seed, P_ESCAPE, a as u64, b as u64]).is_multiple_of(2) {
                    1
                } else {
                    -1
                };
                let dir = along.map(|v| v * sign);
                let end = line
                    .iter()
                    .map(|l| l.0)
                    .max_by_key(|&x| dot(x, dir))
                    .expect("a front");
                let block_at = step(end, dir, ESCAPE_KM / 2);
                let block: Vec<bool> = (0..CELLS)
                    .map(|c| {
                        plate[c] as usize == over
                            && rock[c].class != OCEAN
                            && km_between(sphere.centre[c], block_at) < ESCAPE_KM
                    })
                    .collect();
                let km2: i64 = (0..CELLS).filter(|&c| block[c]).map(area).sum();
                // A block escapes toward a free face: another
                // plate's ocean ahead of it, not its own plate's
                // interior.
                let free = (0..CELLS).any(|c| {
                    block[c]
                        && dot(sub(sphere.centre[c], block_at), dir) >= 0
                        && sphere.links[c].iter().any(|&(m, _)| {
                            plate[m as usize] as usize != over && rock[m as usize].class == OCEAN
                        })
                });
                if km2 < 200_000 || 3 * km2 > cont_of[over] || !free {
                    continue;
                }
                let new = *next_id;
                *next_id += 1;
                for c in 0..CELLS {
                    if block[c] {
                        plate[c] = new as u8;
                    }
                }
                // The block's crust leaves its piece: a piece of its
                // own from now on, in the same lattice.
                for c in &mut land.cells {
                    if c.plate as usize == over && km_between(c.pos, block_at) < ESCAPE_KM {
                        c.plate = new as u8;
                        c.switch = SWITCH_ESCAPE;
                    }
                }
                for c in 0..CELLS {
                    if disp[c] != NO_BIT {
                        bits[disp[c] as usize].plate = plate[c];
                    }
                }
                for p in 0..MAXP {
                    parted[new][p] = parted[over][p];
                    parted[p][new] = parted[p][over];
                }
                parted[over][new] = true;
                parted[new][over] = true;
                born_t[new] = t;
                group[new] = new as u8;
                omega[new] = add(omega[over], lateral(end, dir, ESCAPE_MM * 1000));
                pushes.push(Push {
                    centre: end,
                    normal: dir,
                    reach_km: 2 * ESCAPE_KM,
                    expansion_km: ESCAPE_KM / 2,
                    strength: ESCAPE_MM * 1000,
                    plates: (over as u8, new as u8),
                });
                journal.push(Event::plates(
                    What::BornEscaping,
                    (new, over),
                    block_at,
                    km2,
                ));
                escapes.push(Escape {
                    my: now_my,
                    from: over as u8,
                    block: new as u8,
                    km2,
                });
            }
        }
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
        let full = i64::from(sphere.area_km2[above.at as usize]);
        let room = (THICK_MAX - i64::from(land.cells[j].rock.thick)) * full;
        let vol = i64::from(me.rock.thick) * i64::from(me.area);
        let take = room.min(vol);
        if take <= 0 {
            continue;
        }
        land.cells[j].rock.thick += (take / full.max(1)) as u16;
        let gone = take / i64::from(me.rock.thick).max(1);
        *shortened += gone;
        if take == vol {
            land.kill(i);
        } else {
            land.cells[i].area -= gone as u32;
        }
    }
}
/// Step 12 (collisions). Collisions begin, last and end; their fronts.
pub(super) fn progress(world: &mut World) {
    let journal = &mut world.journal;
    let grips = &mut world.step.grips;
    let s = world.s;
    let sphere = world.sphere;
    let parted = &mut world.parted;
    let collisions = &mut world.collisions;
    let contact = &mut world.contact;
    let closing_of = &mut world.closing_of;
    let quiet = &mut world.quiet;
    let front_of = &mut world.front_of;
    let record_of = &mut world.record_of;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let omega = &mut world.omega;
    let group = &mut world.group;
    let fronts = &mut world.fronts;
    let t = world.step.t;
    let now_my = world.step.now_my;
    let seen = &mut world.step.seen;
    // Collisions in progress: a collision begins where a front closes
    // between parted plates, and lasts while their continents touch;
    // how long their contact is, and how fast it still closes.
    let mut sum = vec![[0_i64; MAXP]; MAXP];
    let mut speed = vec![[0_i64; MAXP]; MAXP];
    let mut cnt = vec![[0_i64; MAXP]; MAXP];
    let mut hit = vec![[false; MAXP]; MAXP];
    for &(_, p, o, _, _) in &seen.front {
        hit[p.min(o) as usize][p.max(o) as usize] = true;
    }
    for c in 0..CELLS {
        if rock[c].class == OCEAN {
            continue;
        }
        let x = sphere.centre[c];
        let pc = plate[c] as usize;
        for &(m, _) in &sphere.links[c] {
            let m = m as usize;
            let pm = plate[m] as usize;
            if pm <= pc || rock[m].class == OCEAN || group[pc] == group[pm] {
                continue;
            }
            let normal = normalize(tangent(x, sub(sphere.centre[m], x)));
            let rel = sub(cross(omega[pc], x), cross(omega[pm], x));
            sum[pc][pm] += dot(rel, normal);
            speed[pc][pm] += norm(rel);
            cnt[pc][pm] += 1;
        }
    }
    for a in 0..MAXP {
        for b in a + 1..MAXP {
            // A collision ends where the continents part again.
            let on = if contact[a][b] == 0 {
                hit[a][b] && parted[a][b]
            } else {
                cnt[a][b] > 0 && sum[a][b] >= -s.weld_stop_mm * 1000 * cnt[a][b]
            };
            if on {
                let mean = sum[a][b] / cnt[a][b].max(1);
                if contact[a][b] == 0 {
                    contact[a][b] = t + 1;
                    record_of[a][b] = collisions.len();
                    let at = seen
                        .front
                        .iter()
                        .find(|f| (f.1.min(f.2) as usize, f.1.max(f.2) as usize) == (a, b))
                        .map_or([0; 3], |f| sphere.centre[f.0]);
                    journal.push(Event::plates(What::CollisionBegun, (a, b), at, 0));
                    collisions.push(Collision {
                        a: a as u8,
                        b: b as u8,
                        from_my: now_my,
                        welded_my: NEVER,
                        closed_km: 0,
                        front_cells: 0,
                        steps: 0,
                    });
                }
                let r = &mut collisions[record_of[a][b]];
                r.closed_km += mean.max(0) * s.step_my / 1000;
                r.front_cells = r.front_cells.max(cnt[a][b]);
                r.steps += 1;
                closing_of[a][b] = mean;
                // Stood still: neither closing nor sliding.
                if speed[a][b] <= s.weld_stop_mm * 1000 * cnt[a][b] {
                    quiet[a][b] += 1;
                } else {
                    quiet[a][b] = 0;
                }
                front_of[a][b] = cnt[a][b];
            } else if contact[a][b] != 0 {
                contact[a][b] = 0;
                quiet[a][b] = 0;
                record_of[a][b] = usize::MAX;
            }
        }
    }
    *grips = (0..MAXP)
        .flat_map(|a| (a + 1..MAXP).map(move |b| (a, b)))
        .filter(|&(a, b)| contact[a][b] != 0)
        .map(|(a, b)| (a as u8, b as u8, front_of[a][b]))
        .collect();
    fronts.clone_from(&seen.front);
}
