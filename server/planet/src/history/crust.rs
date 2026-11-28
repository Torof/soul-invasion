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
    /// The later rift it lies along (index + 1), 0 none.
    pub(super) rift: u8,
    /// When it formed, millions of years ago.
    pub(super) born_my: u16,
    pub(super) orogeny: u16,
    pub(super) home: u32,
    /// Its thickness, tenths of a km. **The one ledger of its height**
    /// (version 18): a crust stands as its thickness floats it
    /// ([`ISOSTASY_M`], for any thickness), plus its heat (`warm`) and
    /// the sediment on it (`silt`). Nothing else raises or lowers it:
    /// what builds a mountain adds crust, what wears it takes crust away.
    pub(super) thick: u16,
    /// The rest of its thickness, m, beyond `thick` (−50 to 49): wear
    /// takes a few metres a step, less than a tenth of a km.
    pub(super) fine: i8,
    /// What its heat adds to its height, m: crust just stretched, the
    /// shoulders of a rift, a plume under it, a young floor under an arc
    /// stand higher than their thickness alone gives, and sink as they
    /// cool ([`rifts::COOLING`] of it a step). Below zero (crust built
    /// on old, cold, dense floor) it stays.
    pub(super) warm: i16,
    /// A continent's shore at a passive margin: how hard the floor beside
    /// it was pushed on it at the last step, per mille of what the
    /// margin's section holds (`trenches::margins_give_way`; 0: no
    /// margin, or not pushed). Written anew at each step.
    pub(super) pressed: i16,
    /// The sediment lying on it, m of thickness (version 16): what
    /// erosion took from higher ground and laid here. It raises the
    /// ground by [`SILT_LIFTS`] of its thickness, on a continent
    /// ([`Rock::high`]) as on the ocean floor ([`floor_bed`]).
    pub(super) silt: u16,
    /// When that sediment was laid, millions of years ago (the mean of
    /// its layers by thickness); [`NEVER`] with none.
    pub(super) laid: u16,
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
/// terranes and arcs.
pub const CONTINENT_THICK: u16 = 350;
pub(super) const OCEAN_THICK: u16 = 70;
pub(super) const TERRANE_THICK: u16 = 250;

/// Height a km of extra crust raises a continent, m (Airy isostasy:
/// crust 2.8 floating on a mantle 3.3 stands above it by 0.5 / 3.3 of
/// its thickness; 100 until version 14, a third too little).
pub const ISOSTASY_M: i64 = 150;

/// Flood basalts stand this high on the crust they covered, m.
pub const LIP_M: i64 = 350;
/// A ridge's crest lies this far below the heights' datum, m. The datum
/// is where a continent's whole, cold crust would stand less
/// [`PLATFORM_M`].
pub const RIDGE_M: i64 = -2_500;
/// Ocean floor older than this, millions of years, sinks no more.
pub const FLAT_FLOOR_MY: i64 = 120;
/// A continent's whole crust ([`CONTINENT_THICK`]), old and cold, with
/// nothing on it, stands this high above the datum, m: 3,000 m above a
/// ridge's crest (the Earth: a ridge's crest lies some 2,600 m under the
/// sea and its shields stand 200 to 500 m above it).
pub const PLATFORM_M: i64 = 300;
/// Sediment raises the ground it is laid on by this much of its
/// thickness, ‰: its weight pushes its crust down (sediment 2.5 on a
/// mantle 3.3: 0.8 / 3.3).
pub const SILT_LIFTS: i64 = 240;
/// Water weighs on what it covers: ground under the sea is pushed down
/// by this much of the water's depth, ‰ (water 1.0 on a mantle 3.3).
/// Every height the history keeps is the ground's as it would stand
/// with no water on it; where the sea stands `d` m above such ground
/// the water is [`depth`]`(d)` deep. So when the ocean's basins shrink
/// the sea rises over the continents by seven tenths of what it would
/// over a rigid floor.
pub const WATER_SINKS: i64 = 303;

/// The water's depth where the sea stands `over` m above the ground as
/// it would be with no water on it ([`WATER_SINKS`]).
#[must_use]
pub fn depth(over: i64) -> i64 {
    over * 1000 / (1000 - WATER_SINKS)
}

/// How far the ocean floor has sunk at an age, millions of years, m: it
/// cools, ~350 m per square root of a million years (Parsons and
/// Sclater), flat past [`FLAT_FLOOR_MY`].
#[must_use]
pub fn sinking(age: i64) -> i64 {
    350 * (age.min(FLAT_FLOOR_MY) * 1_000_000).isqrt() / 1000
}

/// The ocean floor's bed above the datum, m, as it would stand with no
/// water on it: a ridge's crest, sunk as its age says, with its grain
/// (`more`: and what a map draws on it), all of them depths under the
/// sea standing at the datum, so less the water's push
/// ([`WATER_SINKS`]); raised by the sediment on it, and by the crust an
/// arc has added to its 7 km (`thick`, tenths of a km: an island arc
/// growing under the sea, version 18), as any crust floats
/// ([`ISOSTASY_M`]).
#[must_use]
pub fn floor_bed(age_my: i64, silt_m: i64, thick: u16, more: i64) -> i64 {
    (RIDGE_M - sinking(age_my) + more) * (1000 - WATER_SINKS) / 1000
        + silt_m * SILT_LIFTS / 1000
        + (i64::from(thick) - i64::from(OCEAN_THICK)) * ISOSTASY_M / 10
}

/// A continent's crust above the datum, m, without its grain and with
/// no water on it: what its crust up to a continent's thickness, its
/// heat and its sediment give (`high`, [`Rock::high`]), what its crust
/// beyond that thickness lifts (a pile), its flood basalts. The
/// history's sea and the maps read this one height.
#[must_use]
pub fn stands(high: i16, thick: u16, margins: u8) -> i64 {
    i64::from(high)
        + (i64::from(thick) - i64::from(CONTINENT_THICK)).max(0) * ISOSTASY_M / 10
        + if margins & LIP != 0 { LIP_M } else { 0 }
}

/// A height changed by `by` m (the record's width: a limit of the
/// arithmetic).
pub(super) fn lift(high: i16, by: i64) -> i16 {
    (i64::from(high) + by).clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
}

/// The causes that change a crust's height ([`Rock::high`]), in the
/// order of [`History::cause_at`]: a rift stretched it, it cooled, an
/// arc added crust to it, crust was piled on it below a continent's
/// thickness (a docking, a collision), a plume warmed it, it wore down,
/// crust flowed or slid to or from its neighbours, sediment was laid on
/// it.
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
    /// Its crust's thickness, m.
    pub(super) fn crust_m(&self) -> i64 {
        i64::from(self.thick) * 100 + i64::from(self.fine)
    }

    /// Its height above the datum, m, without its grain, its flood
    /// basalts and what its crust beyond a continent's thickness lifts
    /// ([`stands`] adds those): its crust floating, its heat, its
    /// sediment. Worked out, never kept.
    pub(super) fn high(&self) -> i16 {
        let whole = i64::from(CONTINENT_THICK) * 100;
        lift(
            0,
            PLATFORM_M
                + ((self.crust_m().min(whole) - whole) * ISOSTASY_M).div_euclid(1000)
                + i64::from(self.warm)
                + i64::from(self.silt) * SILT_LIFTS / 1000,
        )
    }

    /// Something happens to it (`what`), for a cause: the change of its
    /// height is written to that cause.
    pub(super) fn change(&mut self, cause: usize, what: impl FnOnce(&mut Rock)) {
        let was = i64::from(self.high());
        what(self);
        // (The ocean floor's height is its age's: it keeps no causes.)
        if self.class != OCEAN {
            self.why[cause] = lift(self.why[cause], i64::from(self.high()) - was);
        }
    }

    /// Its crust gains `m` metres of thickness (loses, below zero), for
    /// a cause. (Between nothing and 6,500 km: the record's width.)
    pub(super) fn thicken(&mut self, m: i64, cause: usize) {
        self.change(cause, |r| {
            let all = (r.crust_m() + m).clamp(0, 6_553_500);
            r.thick = ((all + 50) / 100) as u16;
            r.fine = (all - i64::from(r.thick) * 100) as i8;
        });
    }

    /// Its heat raises it by `m` metres more (less, below zero).
    pub(super) fn heat(&mut self, m: i64, cause: usize) {
        self.change(cause, |r| r.warm = lift(r.warm, m));
    }

    /// Sediment laid on it, `m` of thickness, now: its thickness and the
    /// mean date of its layers. (A thickness beyond what the record
    /// holds, 65 km, is not counted: a limit of the arithmetic.)
    pub(super) fn lay(&mut self, m: i64, now_my: u16) {
        self.change(BY_FILL, |r| {
            let had = i64::from(r.silt);
            let all = (had + m).min(i64::from(u16::MAX));
            if all > 0 {
                let then = if had > 0 { i64::from(r.laid) } else { 0 };
                r.laid = ((then * had + i64::from(now_my) * (all - had)) / all) as u16;
            }
            r.silt = all as u16;
        });
    }

    /// A rift stretches it: `thin` tenths of a km thinner than the
    /// `was` it had. It floats lower by what it lost, half of it at
    /// once and half as it cools (stretched crust is hot under: the
    /// warm mantle has risen where the cold lithosphere thinned;
    /// `McKenzie` 1978), and its sediment stretches with it.
    pub(super) fn thinned(&mut self, thin: i64, was: i64) {
        self.thicken(-thin * 100, BY_STRETCH);
        self.heat(thin * ISOSTASY_M / 10 / 2, BY_STRETCH);
        self.change(BY_STRETCH, |r| {
            r.silt = (i64::from(r.silt) * (was - thin) / was.max(1)) as u16;
        });
    }

    /// Its heat is set so that it stands `m` metres above the datum
    /// (without its grain) as it is: crust made of a floor stands where
    /// that floor stood.
    pub(super) fn stand_at(&mut self, m: i64) {
        self.warm = 0;
        self.warm = lift(0, m - i64::from(self.high()));
    }

    /// `m` metres of rock worn off its top: its sediment first, then
    /// its crust. Gives what was taken of each, m.
    pub(super) fn wear(&mut self, m: i64) -> (i64, i64) {
        let loose = m.min(i64::from(self.silt));
        let hard = (m - loose).min(self.crust_m());
        self.change(BY_WEAR, |r| r.silt -= loose as u16);
        self.thicken(-hard, BY_WEAR);
        (loose, hard)
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
    let troughs = &mut world.troughs;
    let land = &mut world.land;
    let bits = &mut world.bits;
    let disp = &mut world.disp;
    let top = &mut world.top;
    let marks = &mut world.marks;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let shortened_km2 = &mut world.shortened_km2;
    let folded_km2 = &mut world.folded_km2;
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
        for j in 0..tr.path.len() {
            tr.path[j] = land_ix[tr.path[j] as usize];
            if tr.twin[j] != NO_BIT {
                tr.twin[j] = land_ix[tr.twin[j] as usize];
            }
        }
        tr.keep(|i| i != NO_BIT);
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
    folded_km2.push(world.step.folded);
    made_km2.push(*made);
}

#[cfg(test)]
impl Rock {
    /// A plain crust of a class and a thickness, for the tests.
    pub(super) fn plain(class: u8, thick: u16) -> Rock {
        Rock {
            class,
            piece: 0,
            margins: 0,
            rift: 0,
            born_my: 0,
            orogeny: NEVER,
            home: 0,
            thick,
            fine: 0,
            warm: 0,
            pressed: 0,
            silt: 0,
            laid: NEVER,
            why: [0; CAUSES],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crust(thick: u16) -> Rock {
        Rock::plain(CONTINENT, thick)
    }

    #[test]
    fn a_crust_stands_as_its_thickness_floats_it_and_wear_takes_crust() {
        let height = |r: &Rock| stands(r.high(), r.thick, r.margins);
        // A whole crust on the platform; thinner lower, thicker higher,
        // by the same 150 m a km.
        let mut r = crust(CONTINENT_THICK);
        assert_eq!(height(&r), PLATFORM_M);
        assert_eq!(height(&crust(250)), PLATFORM_M - 1_500);
        assert_eq!(height(&crust(600)), PLATFORM_M + 3_750);
        // What builds adds crust: 10 km of it, 1,500 m.
        r.thicken(10_000, BY_ARC);
        assert_eq!((r.thick, height(&r)), (450, PLATFORM_M + 1_500));
        // What wears takes crust, and the crust floats back: 2 km of
        // rock off the top lower it by 300 m.
        assert_eq!(r.wear(2_000), (0, 2_000));
        assert_eq!((r.thick, height(&r)), (430, PLATFORM_M + 1_200));
        // A few metres are kept, not lost to the record's tenth of a km.
        for _ in 0..10 {
            r.wear(30);
        }
        assert_eq!(r.crust_m(), 42_700);
        // Sediment is worn before the crust under it.
        r.lay(100, 0);
        assert_eq!(r.wear(150), (100, 50));
        // Heat holds it up, and goes.
        let cold = height(&r);
        r.heat(400, BY_PLUME);
        assert_eq!(height(&r), cold + 400);
    }
}
