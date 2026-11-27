//! What the crust is (a rock's record, a bit of ocean floor, a hotspot's
//! mark, the named causes of a change of height) and its upkeep at each
//! step: the turn of the plates, the bits sorted by cell, the dead removed.

use super::*;

/// A piece of crust and its record, carried with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Rock {
    pub(super) class: u8,
    pub(super) piece: u8,
    pub(super) margins: u8,
    /// Steps it has ridden over a sinking ocean (an island arc's growth).
    pub(super) arc: u8,
    /// The later rift it lies along (index + 1), 0 none.
    pub(super) rift: u8,
    /// When it formed, millions of years ago.
    pub(super) born_my: u16,
    pub(super) orogeny: u16,
    pub(super) home: u32,
    /// Its thickness, tenths of a km.
    pub(super) thick: u16,
    /// Its height, m above the sea of the start, without its grain
    /// ([`History::rough`], [`History::broad`]) and without what a pile
    /// lifts ([`ISOSTASY_M`]). The crust carries it (version 7): only what
    /// happens to the crust changes it.
    pub(super) high: i16,
    /// How far it has still to sink as it cools, m (a stretched margin).
    pub(super) cool: i16,
    /// How far stretching has lowered it in all, m.
    pub(super) sunk: i16,
    /// What changed its height since the step before, m, by cause (see
    /// [`CAUSES`]).
    pub(super) why: [i16; CAUSES],
}

/// A bit of ocean floor where it really is. Each step it turns with its
/// plate, exactly, so a slow plate drifts as far as its speed says; the
/// grid only draws the bits (a cell shows one bit, a bit shows in one
/// cell at most). The floor is short-lived and under water: it stays on
/// the grid (version 10).
#[derive(Debug, Clone, Copy)]
pub(super) struct Bit {
    pub(super) pos: V3,
    pub(super) plate: u8,
    pub(super) rock: Rock,
    /// Whether a cell showed it at the step before.
    pub(super) seen: bool,
    /// Its name, for good.
    pub(super) id: u32,
}

/// Where a hotspot stood under the crust, carried with what it marked: a
/// bit of ocean floor, or (`land`) a cell of a continent's piece.
#[derive(Debug, Clone, Copy)]
pub(super) struct Mark {
    pub(super) land: bool,
    pub(super) bit: u32,
    pub(super) pos: V3,
    pub(super) spot: u8,
    pub(super) my: u16,
}

/// No bit shown.
pub(super) const NO_BIT: u32 = u32::MAX;
/// Crust thickness, tenths of a km: continents, the ocean floor,
/// terranes and arcs; the most a pile reaches.
pub const CONTINENT_THICK: u16 = 350;
pub(super) const OCEAN_THICK: u16 = 70;
pub(super) const TERRANE_THICK: u16 = 250;
pub(super) const THICK_MAX: i64 = 700;

/// Height a km of extra crust raises a continent, m (Airy isostasy:
/// crust 2.8, mantle 3.3).
pub const ISOSTASY_M: i64 = 100;

/// A carried height changed by `by` m.
pub(super) fn lift(high: i16, by: i64) -> i16 {
    (i64::from(high) + by).clamp(-6_000, 8_000) as i16
}

/// The causes that change a crust's carried height, in the order of
/// [`History::cause_at`]: a rift stretched it, it cooled, an arc
/// built on it, a terrane docked, a plume lifted it, it wore down, it
/// levelled with its neighbours, sediment filled it.
pub const CAUSES: usize = 8;
pub(super) const BY_STRETCH: usize = 0;
pub(super) const BY_COOLING: usize = 1;
pub(super) const BY_ARC: usize = 2;
pub(super) const BY_DOCK: usize = 3;
pub(super) const BY_PLUME: usize = 4;
pub(super) const BY_WEAR: usize = 5;
pub(super) const BY_LEVEL: usize = 6;
pub(super) const BY_FILL: usize = 7;

impl Rock {
    /// Its carried height changes by `by` m, for a cause.
    pub(super) fn rise(&mut self, by: i64, cause: usize) {
        let was = self.high;
        self.high = lift(was, by);
        self.why[cause] = lift(self.why[cause], i64::from(self.high) - i64::from(was));
    }
}

/// Each plate's mean ocean floor birth, millions of years ago (older
/// floor is denser: of two oceanic plates, the older one sinks).
pub(super) fn floor_born(plate: &[u8], rock: &[Rock]) -> [i64; MAXP] {
    let mut sum = [0_i64; MAXP];
    let mut n = [0_i64; MAXP];
    for (c, r) in rock.iter().enumerate() {
        if r.class == OCEAN {
            sum[plate[c] as usize] += i64::from(r.born_my);
            n[plate[c] as usize] += 1;
        }
    }
    let mut out = [0_i64; MAXP];
    for p in 0..MAXP {
        out[p] = if n[p] > 0 { sum[p] / n[p] } else { 0 };
    }
    out
}

/// The shown floor and its plates written back to their bits (the
/// continents are not on the grid: nothing is written back to them).
pub(super) fn sync(bits: &mut [Bit], plate: &[u8], rock: &[Rock], disp: &[u32]) {
    for c in 0..CELLS {
        if disp[c] != NO_BIT {
            let b = &mut bits[disp[c] as usize];
            b.plate = plate[c];
            b.rock = rock[c];
        }
    }
}

/// A step begins: what the last step left on each cell of crust is cleared.
pub(super) fn begin_step(world: &mut World) {
    let land = &mut world.land;
    let bits = &mut world.bits;
    let disp = &mut world.disp;
    let cont_of = &mut world.step.cont_of;
    for b in &mut *bits {
        b.seen = false;
    }
    for &d in &*disp {
        if d != NO_BIT {
            bits[d as usize].seen = true;
        }
    }
    for c in &mut land.cells {
        c.rock.why = [0; CAUSES];
        c.event = Margin::Inland as u8;
        c.switch = 0;
        c.rifted = c.rock.margins & RIFT != 0;
    }
    *cont_of = land.weight();
}
/// Step 4. The plates turn, carrying their crust; the bits of floor are
/// sorted by the cell they now lie in.
pub(super) fn turn(world: &mut World) {
    let pressed = &mut world.step.pressed;
    let spare = &mut world.step.spare;
    let accrete = &mut world.step.accrete;
    let s = world.s;
    let sphere = world.sphere;
    let troughs = &mut world.troughs;
    let land = &mut world.land;
    let bits = &mut world.bits;
    let marks = &mut world.marks;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let out_turned = &mut world.out_turned;
    let omega = &mut world.omega;
    let last_kind = &mut world.last_kind;
    let cont_of = &mut world.step.cont_of;
    let moved = &mut world.step.moved;
    let at = &mut world.step.at;
    let start = &mut world.step.start;
    let order = &mut world.step.order;
    let old_plate = &mut world.step.old_plate;
    let old_rock = &mut world.step.old_rock;
    let dead = &mut world.step.dead;
    let pending = &mut world.step.pending;
    let shortened = &mut world.step.shortened;
    let made = &mut world.step.made;
    // 4. The plates turn, carrying their crust. A piece of
    // continent only changes its turn; every bit of floor moves
    // exactly, from where it was, not from a cell.
    moved.clone_from(omega);
    out_turned.push(moved.clone());
    let turn: Vec<M3> = omega
        .iter()
        .map(|&w| {
            let speed = norm(w);
            if speed < 50 {
                IDENTITY
            } else {
                rotation(normalize(w), speed * s.step_my * ONE / (1000 * RADIUS_KM))
            }
        })
        .collect();
    for m in &mut *marks {
        let p = if m.land {
            land.cells[m.bit as usize].plate
        } else {
            bits[m.bit as usize].plate
        };
        m.pos = normalize(apply(&turn[p as usize], m.pos));
    }
    for b in &mut *bits {
        b.pos = normalize(apply(&turn[b.plate as usize], b.pos));
    }
    for tr in &mut *troughs {
        tr.normal = normalize(apply(&turn[tr.plate as usize], tr.normal));
    }
    land.turn(sphere, &turn);
    let n = bits.len();
    world.step.n = n;
    *at = bits.iter().map(|b| sphere.cell_of(b.pos) as u32).collect();
    *start = vec![0_u32; CELLS + 1];
    for &c in &*at {
        start[c as usize + 1] += 1;
    }
    for c in 0..CELLS {
        start[c + 1] += start[c];
    }
    *order = vec![0_u32; n];
    let mut fill = start.clone();
    for (i, &c) in at.iter().enumerate() {
        order[fill[c as usize] as usize] = i as u32;
        fill[c as usize] += 1;
    }
    old_plate.clone_from(plate);
    old_rock.clone_from(rock);
    world.step.floor = floor_born(&*old_plate, &*old_rock);
    world.step.weight = *cont_of;
    *dead = vec![false; n];
    // Where continents collide (a collision's front last step, or
    // next to it), a continent run under the other is a suture.
    let collision = Kind::Collision as u8 + 1;
    *pressed = (0..CELLS)
        .map(|q| {
            last_kind[q] == collision
                || sphere.links[q]
                    .iter()
                    .any(|&(m, _)| last_kind[m as usize] == collision)
        })
        .collect();
    *spare = vec![Vec::new(); CELLS];
    *pending = vec![false; CELLS];
    // The cells of crust that dock (a terrane or an arc), and the
    // plate they join.
    *accrete = Vec::new();
    *shortened = 0;
    *made = 0;
}
/// Step 11. The floor sunk and the crust piled leave the record; what
/// the view hides is counted.
pub(super) fn compact(world: &mut World) {
    let pushes = &mut world.pushes;
    let troughs = &mut world.troughs;
    let land = &mut world.land;
    let bits = &mut world.bits;
    let disp = &mut world.disp;
    let top = &mut world.top;
    let marks = &mut world.marks;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let shortened_km2 = &mut world.shortened_km2;
    let made_km2 = &mut world.made_km2;
    let shown_now = &mut world.shown_now;
    let hidden_now = &mut world.hidden_now;
    let dead = &mut world.step.dead;
    let shortened = &mut world.step.shortened;
    let made = &mut world.step.made;
    sync(&mut *bits, &*plate, &*rock, &*disp);
    // The floor sunk and the crust piled leave the record.
    let mut new_ix = vec![NO_BIT; bits.len()];
    let mut kept = Vec::with_capacity(bits.len());
    for (i, b) in bits.iter().enumerate() {
        if !dead[i] {
            new_ix[i] = kept.len() as u32;
            kept.push(*b);
        }
    }
    *bits = kept;
    for d in &mut *disp {
        if *d != NO_BIT {
            *d = new_ix[*d as usize];
        }
    }
    let land_ix = land.compact();
    for tr in &mut *troughs {
        tr.path = tr
            .path
            .iter()
            .map(|&i| land_ix[i as usize])
            .filter(|&i| i != NO_BIT)
            .collect();
    }
    for d in &mut *top {
        if *d != NO_BIT {
            *d = land_ix[*d as usize];
        }
    }
    marks.retain_mut(|m| {
        let k = if m.land {
            land_ix[m.bit as usize]
        } else {
            new_ix[m.bit as usize]
        };
        m.bit = k;
        k != NO_BIT
    });
    // What the grid's view leaves out of the pieces (a view of
    // the same fineness as the crust cannot show every cell of a
    // turned lattice): cells of crust it does not draw, those of
    // them it drew at the step before; and the cells it draws
    // from crust it did not draw before.
    let mut is_shown = vec![false; land.cells.len()];
    for &d in &*top {
        if d != NO_BIT {
            is_shown[d as usize] = true;
        }
    }
    let mut hidden = [0_u32; 3];
    for c in 0..CELLS {
        shown_now[c] = if top[c] == NO_BIT {
            0
        } else {
            u8::from(!land.cells[top[c] as usize].shown)
        };
    }
    for (i, c) in land.cells.iter_mut().enumerate() {
        if !is_shown[i] && !c.under {
            hidden[0] += 1;
            hidden[1] += u32::from(c.shown);
        }
        c.shown = is_shown[i];
    }
    *hidden_now = hidden;
    shortened_km2.push(*shortened);
    made_km2.push(*made);
    for push in &mut *pushes {
        push.strength = push.strength * PUSH_KEEP / 1000;
    }
}
