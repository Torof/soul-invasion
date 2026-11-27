//! Trenches and arcs: the ocean floor sinking under a continent (active
//! margin) or under a younger floor, island arcs, terranes and arcs
//! docking; the grid's view of the floor.

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
