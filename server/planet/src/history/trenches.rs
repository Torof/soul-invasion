//! Trenches and arcs: the ocean floor sinking under a continent (active
//! margin) or under a younger floor, island arcs, terranes and arcs
//! docking; the grid's view of the floor; a passive margin whose old
//! floor gives way (a trench is born).

use super::*;

/// An arc is crust added over a sinking floor (version 18; until then
/// a continent's arc was a height aimed at, 2,200 m, with no crust
/// under it, and an island arc was born whole, 25 km thick, on one cell
/// in five drawn by lot). Each km² of floor swallowed adds this many m
/// of crust times a km² to the plate riding over it: the Earth's arcs
/// add some 3 km³ of crust a year (Stern and Scholl 2010; 30 to 130
/// km³ a km of arc and a My, Reymer and Schubert 1984, Jicha and
/// Jagoutz 2015) for some 3 km² of floor swallowed a year (Bird 2003).
/// Under a continent it is added around the arc's line, where the slab
/// is deep enough to melt the mantle above it ([`ARC_LINE_KM`] from the
/// edge, this half-width, no farther than [`ARC_KM`]); the height
/// follows from the crust ([`Rock::high`]), and wear takes it back.
/// Under an ocean's riding edge it thickens the floor itself, which
/// rises under the sea as any crust floats.
pub(super) const ARC_ADD_M: i64 = 1_000;
pub(super) const ARC_LINE_KM: (i64, i64) = (180, 150);
pub(super) const ARC_KM: u32 = 340;
/// A floor an arc has thickened to this, tenths of a km, is crust that
/// sinks no more (too light: Cloos 1993, 15 to 18 km for an arc or an
/// ocean plateau): an island arc's crust from then on, carried and
/// docked as a terrane is, and growing still while a floor sinks under
/// it. It is the thickness at which a rift's stretched crust gives way
/// to an ocean ([`BREAK_THICK`]).
pub(super) const ARC_CRUST_THICK: u16 = BREAK_THICK;

/// A margin gives way once the push across it has been more than its
/// section holds for this step and the one before (`Rock::pressed`): a
/// push of one step breaks nothing. Forced initiation takes some 100 to
/// 150 km of convergence before the slab sinks by itself (Gurnis, Hall
/// and Lavier 2004), a few million years at the plates' speeds: a step.
/// The measure is kept per mille of the hold, up to this.
pub(super) const PRESSED_MOST: i64 = 30_000;

/// No ocean: the cell is not ocean floor.
const NO_OCEAN: u32 = u32::MAX;

/// A shore cell of a passive margin and the ocean it faces: the way
/// from that floor to the shore, the oldest floor there, what the
/// shore's section holds.
struct Shore {
    q: usize,
    ocean: u32,
    normal: V3,
    age: i64,
    holds: i64,
}

/// How far a force of the step before is looked for when its cell is no
/// longer its plate's (the plates have turned since), cells.
const EDGE_REACH: usize = 3;

/// Step 11 (the margins). A trench is born, for a cause (version 17; a
/// timer until then: the floor bent 10 m a My under its own weight and
/// gave way at 1,500 m, so every margin of one age gave way together,
/// and again at once ever after). An old passive margin does not give
/// way by itself: its floor is heavy, but the plate it is part of is
/// strong, and grows stronger with age (Cloetingh, Wortel and Vlaar
/// 1989; Mueller and Phillips 1991; Stern 2004: no known case in 66
/// My). Two things begin a trench there (Stern 2004; Stern and Gerya
/// 2018; Crameri et al. 2020):
///
/// 1. **The plates push it** (induced initiation). The floor of an
///    ocean and the rest of its plate are two sides joined along the
///    margin. Each side would turn as the forces on it alone turn it,
///    against the mantle's drag (as `forces::parting` reads the two
///    sides of a rift's line); where they would close on each other,
///    the force that crosses the margin for them to go on as one is that
///    speed times the drag they share. The margin's section holds as a
///    plate's section holds a rift's tension (`rifts::holds`, the same
///    [`Settings::crust_holds`]): each shore cell as the weaker of its
///    crust and of the floor beside it (the floor by the square root of
///    its age: a young margin is weak, an old one strong). Where the
///    push is more than the section holds, at this step and the one
///    before, the margin gives way.
/// 2. **A neighbouring trench runs on along it** (lateral propagation).
///    Where a slab of this same floor already hangs a cell away (the
///    floor sinks under another plate there), its weight tears the floor
///    off its own continent's shore next to it, unless the two sides
///    would part there (the rule of `forces::look` for a trench that
///    goes on).
///
/// Either way the floor must be heavy enough to sink ([`slab_weight`]).
/// Then, as before: the floor of that ocean, as far as it is one plate
/// with the continent, is a plate of its own, and a slab hangs where it
/// gave way; the slab pulls like any slab, the floor sinks under the
/// continent like any floor, and the tear runs on along the margin as
/// the floor moves (`forces::look`).
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
    let heat = &world.heat;
    let (t, now_my) = (world.step.t, world.step.now_my);
    paint(&*land, top, &mut *plate, &mut *rock);
    // Each ocean: the floor of one plate in one piece.
    let mut ocean = vec![NO_OCEAN; CELLS];
    let mut oceans: Vec<u8> = Vec::new();
    for c0 in 0..CELLS {
        if rock[c0].class != OCEAN || ocean[c0] != NO_OCEAN {
            continue;
        }
        let id = oceans.len() as u32;
        oceans.push(plate[c0]);
        ocean[c0] = id;
        let mut stack = vec![c0];
        while let Some(c) = stack.pop() {
            for &(m, _) in &sphere.links[c] {
                let m = m as usize;
                if ocean[m] == NO_OCEAN && rock[m].class == OCEAN && plate[m] == plate[c] {
                    ocean[m] = id;
                    stack.push(m);
                }
            }
        }
    }
    // The margins: a continent's shore with floor of its own plate
    // beside it; for each ocean it faces, the way from that floor to
    // the shore, the oldest floor there and what the shore's section
    // holds.
    let shore = |r: &Rock| r.class == CONTINENT || r.class == TERRANE && r.margins & ACCRETED != 0;
    let mut margin: Vec<Shore> = Vec::new();
    for q in 0..CELLS {
        if top[q] == NO_BIT || !shore(&rock[q]) {
            continue;
        }
        let x = sphere.centre[q];
        let mut faced: Vec<u32> = sphere.links[q]
            .iter()
            .map(|&(m, _)| m as usize)
            .filter(|&m| plate[m] == plate[q] && rock[m].class == OCEAN)
            .map(|m| ocean[m])
            .collect();
        faced.sort_unstable();
        faced.dedup();
        for o in faced {
            let mut from = [0_i64; 3];
            let mut age = 0;
            let mut floor_holds = 0;
            for &(m, _) in &sphere.links[q] {
                let m = m as usize;
                if ocean[m] != o {
                    continue;
                }
                from = add(from, sub(x, sphere.centre[m]));
                age = age.max(i64::from(rock[m].born_my.saturating_sub(now_my)));
                floor_holds = floor_holds.max(holds(heat[m], &rock[m], now_my, s.slab_age_my));
            }
            margin.push(Shore {
                q,
                ocean: o,
                normal: normalize(tangent(x, from)),
                age,
                holds: floor_holds.min(holds(heat[q], &rock[q], now_my, s.slab_age_my)),
            });
        }
    }
    // The measure is the step's: a shore that is no margin now keeps none.
    let before: Vec<i16> = land.cells.iter().map(|c| c.rock.pressed).collect();
    for c in &mut land.cells {
        c.rock.pressed = 0;
    }
    let mut gave: Vec<usize> = Vec::new();
    if !margin.is_empty() {
        // The forces on each side, as torques, and the drag on it: an
        // ocean's floor on one side, the rest of its plate on the other.
        let (cell_tq, edge_tq) = torques(
            sphere,
            s,
            &*rock,
            heat,
            Edges {
                slab: &edges_last.0,
                suction: &edges_last.1,
                ridge: &edges_last.2,
                piles: &[],
                sutures: &[],
                bridges: &[],
            },
        );
        let mut whole_tq = vec![[0_i64; 3]; MAXP];
        let mut whole_drag = [0_i64; MAXP];
        let mut floor_tq = vec![[0_i64; 3]; oceans.len()];
        let mut floor_drag = vec![0_i64; oceans.len()];
        for c in 0..CELLS {
            let p = plate[c] as usize;
            whole_tq[p] = add(whole_tq[p], cell_tq[c]);
            if rock[c].class == OCEAN {
                whole_drag[p] += 1000;
                floor_drag[ocean[c] as usize] += 1000;
            } else {
                whole_drag[p] += 1000 + s.continent_drag;
            }
        }
        for &(c, p, torque) in &edge_tq {
            whole_tq[p as usize] = add(whole_tq[p as usize], torque);
            // A force at a plate's edge acts on what it finds there of
            // its plate (the edges are those of the step before: the
            // plate may have left the cell).
            let mut ring = vec![c as usize];
            let mut found = None;
            for _ in 0..=EDGE_REACH {
                found = ring.iter().copied().filter(|&m| plate[m] == p).min();
                if found.is_some() {
                    break;
                }
                let mut next: Vec<usize> = ring
                    .iter()
                    .flat_map(|&m| sphere.links[m].iter().map(|&(n, _)| n as usize))
                    .collect();
                next.sort_unstable();
                next.dedup();
                ring = next;
            }
            if let Some(m) = found
                && rock[m].class == OCEAN
            {
                let o = ocean[m] as usize;
                floor_tq[o] = add(floor_tq[o], torque);
            }
        }
        let speeds = (s.mantle_drag_mm * 1000, s.speed_mm.1 * 1000);
        let turn_of = |tq: V3, drag: i64| {
            let w = tq.map(|v| v * 1000 / drag.max(1));
            let u = norm(w);
            if u > 0 {
                w.map(|v| v * dragged(u, speeds.0).min(speeds.1) / u)
            } else {
                w
            }
        };
        // What each ocean's margin holds along its whole section.
        let mut section = vec![0_i64; oceans.len()];
        for m in &margin {
            section[m.ocean as usize] += m.holds;
        }
        for m in &margin {
            let o = m.ocean as usize;
            let p = oceans[o] as usize;
            let rest_drag = whole_drag[p] - floor_drag[o];
            let x = sphere.centre[m.q];
            let floor_turn = turn_of(floor_tq[o], floor_drag[o]);
            let rest_turn = turn_of(sub(whole_tq[p], floor_tq[o]), rest_drag);
            let closing = dot(sub(cross(floor_turn, x), cross(rest_turn, x)), m.normal);
            let shared = (i128::from(floor_drag[o]) * i128::from(rest_drag)
                / i128::from((floor_drag[o] + rest_drag).max(1))) as i64;
            let push = tension(closing, shared, section[o], s.crust_holds).min(PRESSED_MOST);
            let i = top[m.q] as usize;
            let r = &mut land.cells[i].rock;
            r.pressed = r.pressed.max(push as i16);
            if push >= 1000 && before[i] >= 1000 && slab_weight(m.age, s.slab_age_my) > 0 {
                gave.extend(
                    sphere.links[m.q]
                        .iter()
                        .map(|&(n, _)| n as usize)
                        .filter(|&n| ocean[n] == m.ocean),
                );
            }
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

/// The edge of a continent that rides over a trench folds (version 19;
/// until then a continent's edge above a trench only grew an arc, some
/// 1,500 m high, and no range stood along an ocean). A slab hangs in the
/// mantle and does not move aside freely: its hinge falls back by
/// itself, slowly, and a continent that advances on its trench faster
/// than that is squeezed against it; the advance left over is shared
/// between the hinge pushed back and the continent's margin shortened
/// (the slab is "partly anchored": Heuret and Lallemand 2005, the upper
/// plate's motion is the trench's plus the deformation behind it). The
/// two numbers are read off the models of the Andes (Sobolev and Babeyko
/// 2005; Sobolev et al. 2006): no compression under an advance of 10 to
/// 15 mm a year ([`ROLLBACK_UM`]); 180 km of shortening in 35 My for a
/// continent overriding at 20 mm a year, 350 km at 30 (5 and 10 mm a
/// year: half of what is beyond the threshold, [`FOLD_SHARE`]; the
/// Central Andes: 250 to 350 km since 35 to 45 Ma, at 6 to 14 mm a year,
/// Oncken et al. 2006, `McQuarrie` et al. 2005).
///
/// What squeezes is a push, not the trench's own pull. A slab's suction
/// draws the plate riding over it after the hinge as it falls back
/// (`Settings::slab_suction`): that advance closes on nothing. So the
/// advance counted is the one the other forces on the plate give it
/// (the mantle's heat under it, the ridges, its own slabs), each plate
/// turned by them alone against the mantle's drag, as `forces::parting`
/// and [`margins_give_way`] read the two sides of a line; and no more
/// than the plate's true advance over the mantle. It is read toward the
/// trench, where the last step saw a continent's edge ride over a
/// sinking floor. A rigid piece does not shrink: the
/// shortening owed is kept by the cell of crust at the edge
/// ([`Crust::squeezed`], km); once it is as wide as the cell, the cell's
/// crust is folded on the crust behind it, the thinnest first (`collisions::pile`,
/// as crust run under in a collision is: within some 370 km, so a
/// margin squeezed for long widens into a plateau), the cell is gone and
/// the floor sinks under the next one. The volume is kept; the height
/// follows from the thickness ([`Rock::high`]); the flow of thick crust
/// and wear are what limit it, as for any pile.
pub(super) const ROLLBACK_UM: i64 = 10_000;
pub(super) const FOLD_SHARE: i64 = 2;

/// Step 4 (the margins above a trench). See [`ROLLBACK_UM`].
pub(super) fn margins_fold(world: &mut World) {
    let sphere = world.sphere;
    let step_my = world.s.step_my;
    let land = &mut world.land;
    let (top, plate, rock) = (&world.top, &world.plate, &world.rock);
    let (omega, group, last_kind) = (&world.omega, &world.group, &world.last_kind);
    let now_my = world.step.now_my;
    let arc = Kind::Arc as u8 + 1;
    let solid = |r: &Rock| r.class == CONTINENT || r.class == TERRANE && r.margins & ACCRETED != 0;
    if !(0..CELLS).any(|q| last_kind[q] == arc) {
        return;
    }
    // Each plate's turn by the forces on it other than its trenches'
    // suction (the plates that move as one, as one).
    let s = world.s;
    let (cell_tq, edge_tq) = torques(
        sphere,
        s,
        rock,
        &world.heat,
        Edges {
            slab: &world.edges_last.0,
            suction: &[],
            ridge: &world.edges_last.2,
            piles: &[],
            sutures: &[],
            bridges: &[],
        },
    );
    let mut pushed = vec![[0_i64; 3]; MAXP];
    let mut drag = [0_i64; MAXP];
    for c in 0..CELLS {
        let g = group[plate[c] as usize] as usize;
        pushed[g] = add(pushed[g], cell_tq[c]);
        drag[g] += 1000
            + if rock[c].class == OCEAN {
                0
            } else {
                s.continent_drag
            };
    }
    for &(_, p, torque) in &edge_tq {
        let g = group[p as usize] as usize;
        pushed[g] = add(pushed[g], torque);
    }
    let (v0, most) = (s.mantle_drag_mm * 1000, s.speed_mm.1 * 1000);
    for g in 0..MAXP {
        let w = pushed[g].map(|v| v * 1000 / drag[g].max(1));
        let u = norm(w);
        pushed[g] = if u > 0 {
            w.map(|v| v * dragged(u, v0).min(most) / u)
        } else {
            w
        };
    }
    // (The grid's view is still the last step's: the edges it saw.)
    for q in 0..CELLS {
        if last_kind[q] != arc || top[q] == NO_BIT || !solid(&rock[q]) {
            continue;
        }
        let i = top[q] as usize;
        if land.cells[i].dead || land.cells[i].under {
            continue;
        }
        let x = sphere.centre[q];
        let mut toward = [0_i64; 3];
        for &(n, _) in &sphere.links[q] {
            let n = n as usize;
            if rock[n].class == OCEAN && group[plate[n] as usize] != group[plate[q] as usize] {
                toward = add(toward, sub(sphere.centre[n], x));
            }
        }
        if toward == [0; 3] {
            continue;
        }
        let normal = normalize(tangent(x, toward));
        let p = plate[q] as usize;
        let advance =
            dot(cross(omega[p], x), normal).min(dot(cross(pushed[group[p] as usize], x), normal));
        let owed = (advance - ROLLBACK_UM).max(0) / FOLD_SHARE * step_my / 1000;
        let all = i64::from(land.cells[i].squeezed) + owed;
        let wide = i64::from(land.cells[i].area).isqrt().max(1);
        if all < wide {
            land.cells[i].squeezed = all as u16;
            continue;
        }
        // Folded: its crust and the sediment on it go to the crust
        // behind it, and what is owed beyond its width to the cell of
        // its piece farthest from the trench beside it, the next edge.
        let me = land.cells[i];
        let vol = (me.rock.crust_m() + i64::from(me.rock.silt)) * i64::from(me.area);
        let takers = collisions::pile(land, sphere, i, false, vol);
        if takers.is_empty() {
            land.cells[i].squeezed = all.min(i64::from(u16::MAX)) as u16;
            continue;
        }
        for to in takers {
            let up = &mut land.cells[to].rock;
            up.orogeny = now_my;
            up.margins |= ACTIVE;
        }
        let behind = land
            .kin(sphere, i)
            .filter(|&(k, _)| !land.cells[k].dead && !land.cells[k].under)
            .min_by_key(|&(k, _)| (dot(sub(land.cells[k].pos, me.pos), normal), k))
            .map(|(k, _)| k);
        if let Some(k) = behind {
            let rest = i64::from(land.cells[k].squeezed) + all - wide;
            land.cells[k].squeezed = rest.min(i64::from(u16::MAX)) as u16;
        }
        world.step.folded += i64::from(me.area);
        land.kill(i);
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
    let mut fed: Vec<(usize, i64)> = Vec::new();
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
        fed.push((j, i64::from(sphere.area_km2[q])));
        if c.event != Margin::Suture as u8 {
            c.event = Margin::Active as u8;
        }
    }
    arcs_grow(sphere, land, fed);
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
/// The crust the arcs add in a step ([`ARC_ADD_M`]). `fed`: the cells
/// of crust a bit of floor sank under, and that floor's km². What a
/// cell was fed goes to the crust of its piece around the arc's line
/// behind it, by its place across that line; on a piece too small to
/// hold a line (an island arc) to the cell itself.
fn arcs_grow(sphere: &Sphere, land: &mut Land, mut fed: Vec<(usize, i64)>) {
    fed.sort_unstable();
    fed.dedup_by(|b, a| {
        let same = a.0 == b.0;
        if same {
            a.1 += b.1;
        }
        same
    });
    let sources: Vec<usize> = fed.iter().map(|f| f.0).collect();
    let (d, origin) = land.spread_from(sphere, &sources, ARC_KM);
    let share: Vec<i64> = (0..land.cells.len())
        .map(|i| {
            if d[i] == u32::MAX || land.cells[i].dead {
                0
            } else {
                bump(i64::from(d[i]) - ARC_LINE_KM.0, ARC_LINE_KM.1) * i64::from(land.cells[i].area)
            }
        })
        .collect();
    let mut all = vec![0_i64; fed.len()];
    for i in 0..land.cells.len() {
        if origin[i] != u32::MAX {
            all[origin[i] as usize] += share[i];
        }
    }
    for i in 0..land.cells.len() {
        if origin[i] == u32::MAX {
            continue;
        }
        let k = origin[i] as usize;
        let km2 = i64::from(land.cells[i].area).max(1);
        let vol = fed[k].1 * ARC_ADD_M;
        let add = if all[k] > 0 {
            vol * share[i] / all[k] / km2
        } else if i == fed[k].0 {
            vol / km2
        } else {
            0
        };
        land.cells[i].rock.thicken(add, BY_ARC);
    }
}
/// Step 5 (the view). The grid's view of the crust; each cell of open
/// ocean shows its winning bit of floor, the older sinks under the
/// younger, and the riding edge grows an island arc.
pub(super) fn draw_floor(world: &mut World) {
    let journal = &mut world.journal;
    let sphere = world.sphere;
    let (grain, rough) = (&world.grain, &world.rough);
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
        let mut over_ocean = 0_u16;
        for &j in &here {
            if j == iw {
                continue;
            }
            if bits[j as usize].plate == pw {
                spare[q].push(j);
            } else {
                dead[j as usize] = true;
                over_ocean += 1;
            }
        }
        plate[q] = pw;
        if over_ocean > 0 {
            // The floor swallowed here (a bit a cell) feeds an arc: the
            // riding floor thickens, and is an arc's crust once it is
            // too thick to sink.
            r.thick = r
                .thick
                .saturating_add(over_ocean * (ARC_ADD_M / 100) as u16);
            if r.thick >= ARC_CRUST_THICK {
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
                // It stands where the thickened floor stood, on the
                // heat of the floor it is made of.
                let bed = floor_bed(
                    i64::from(r.born_my.saturating_sub(now_my)),
                    i64::from(r.silt),
                    r.thick,
                    i64::from(rough[q]),
                );
                r.class = TERRANE;
                r.piece = ARC_BASE + pw;
                r.born_my = now_my;
                r.margins |= ACTIVE;
                r.orogeny = now_my;
                r.fine = 0;
                r.pressed = 0;
                r.stand_at(bed - i64::from(grain[r.home as usize]));
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
