//! The state a history is made of: everything a step reads and changes.
//! `start::begin` builds it, `step` (in `mod.rs`) runs the mechanisms on
//! it in order, `record::finish` turns it into a [`History`].

use super::*;

/// The planet while its history runs.
pub(crate) struct World<'a> {
    pub(super) seed: u64,
    pub(super) s: &'a Settings,
    pub(super) sphere: &'a Sphere,
    // The supercontinent's shape, kept for the record.
    pub(super) field: Vec<i64>,
    pub(super) crust_cut: i64,
    /// How far into a belt of the mosaic each cell of the supercontinent
    /// lies at the start, ‰ (read at a crust's home cell).
    pub(super) belt: Vec<u16>,
    pub(super) upwelling: V3,
    pub(super) plates_at_start: (usize, usize),
    pub(super) placed: u64,
    pub(super) rough: Vec<i16>,
    pub(super) broad: Vec<i16>,
    /// The relief's grain, read at a crust's home cell.
    pub(super) grain: Vec<i16>,
    // The mantle.
    pub(super) deep: Vec<i32>,
    pub(super) heat: Vec<i32>,
    /// The cells under which a slab of ocean floor hangs in the mantle:
    /// the trenches as they stood at the last step (`forces::look`).
    pub(super) hung: Vec<bool>,
    pub(super) hotspots: Vec<V3>,
    pub(super) plumes: Vec<V3>,
    pub(super) spent: Vec<bool>,
    // The plates: their turns, who moves with whom, who has parted.
    pub(super) omega: Vec<V3>,
    pub(super) group: [u8; MAXP],
    pub(super) parted: [[bool; MAXP]; MAXP],
    pub(super) broken: [[bool; MAXP]; MAXP],
    pub(super) alias: [u8; MAXP],
    pub(super) born_t: [usize; MAXP],
    pub(super) next_id: usize,
    /// Names of plates that are gone, free to be given again once no
    /// new name is left ([`World::free_names`]); the rifts whose
    /// record names such a plate (their ocean is not followed any
    /// more); and the plates that could not be born for lack of a name.
    pub(super) free: Vec<u8>,
    pub(super) renamed: Vec<usize>,
    pub(super) unnamed: u32,
    pub(super) edges_last: (Vec<Spot>, Vec<Spot>, Vec<Spot>),
    // The crust: the continents' pieces, the bits of ocean floor, the
    // hotspots' marks, and the grid's view of them.
    pub(super) land: Land,
    pub(super) bits: Vec<Bit>,
    pub(super) next_bit: u32,
    pub(super) marks: Vec<Mark>,
    pub(super) disp: Vec<u32>,
    pub(super) top: Vec<u32>,
    pub(super) plate: Vec<u8>,
    pub(super) rock: Vec<Rock>,
    pub(super) event: Vec<u8>,
    pub(super) last_kind: Vec<u8>,
    pub(super) shown_now: Vec<u8>,
    pub(super) hidden_now: [u32; 3],
    // Collisions in progress.
    pub(super) contact: Vec<[usize; MAXP]>,
    pub(super) record_of: Vec<[usize; MAXP]>,
    /// The pairs of plates a collision's pile has stopped, from the
    /// forces of the last step (`forces::drive_from`).
    pub(super) locked: Vec<(u8, u8)>,
    // Rifts in progress, arcs, silt.
    pub(super) troughs: Vec<Trough>,
    /// The tension across each plate's most pulled line at the last step,
    /// ‰ of what its section holds (a trough begins where it has lasted).
    pub(super) pulled: [i64; MAXP],
    pub(super) arc_steps: u8,
    pub(super) silt: [i64; MAXP],
    // The record: events, then the layers of each step.
    pub(super) rifts: Vec<Rift>,
    pub(super) welds: Vec<Weld>,
    pub(super) accretions: Vec<Accretion>,
    pub(super) collisions: Vec<Collision>,
    pub(super) captures: Vec<Capture>,
    pub(super) cut_off: usize,
    pub(super) built_km2: Vec<i64>,
    pub(super) lips: Vec<Lip>,
    pub(super) out_plate: Vec<Vec<u8>>,
    pub(super) out_class: Vec<Vec<u8>>,
    pub(super) out_age: Vec<Vec<u16>>,
    pub(super) out_kind: Vec<Vec<u8>>,
    pub(super) out_margin: Vec<Vec<u8>>,
    pub(super) out_motion: Vec<Vec<Motion>>,
    pub(super) out_home: Vec<Vec<u32>>,
    pub(super) out_margins: Vec<Vec<u8>>,
    pub(super) out_orogeny: Vec<Vec<u16>>,
    pub(super) out_thick: Vec<Vec<u16>>,
    pub(super) out_high: Vec<Vec<i16>>,
    pub(super) out_cause: Vec<Vec<[i16; CAUSES]>>,
    pub(super) out_shown: Vec<Vec<u8>>,
    pub(super) out_id: Vec<Vec<u32>>,
    pub(super) out_hidden: Vec<[u32; 3]>,
    pub(super) out_marks: Vec<Vec<(V3, u8, u16)>>,
    pub(super) out_heat: Vec<Vec<i16>>,
    pub(super) out_pieces: Vec<Vec<Piece>>,
    pub(super) out_turned: Vec<Vec<V3>>,
    pub(super) out_rifts: Vec<Vec<(V3, u16, bool)>>,
    pub(super) continental_km2: Vec<i64>,
    pub(super) shortened_km2: Vec<i64>,
    pub(super) made_km2: Vec<i64>,
    /// The journal: the events since the last step was written, the
    /// journal of each step, and each plate's group when the last step
    /// was written (`NONE`: it had no cell).
    pub(super) journal: Vec<Event>,
    pub(super) out_journal: Vec<Vec<Event>>,
    pub(super) was: [u8; MAXP],
    /// Turns a written scene imposes on plates (the bench); none in a
    /// planet's history.
    pub(super) imposed: Vec<(u8, V3)>,
    /// What the mechanisms of one step hand to one another.
    pub(super) step: Step,
}

impl World<'_> {
    /// Once every new name has been given, the names of the plates that
    /// are gone (no cell of the grid, no floor, no crust) are free
    /// again: what the world remembers of each is forgotten, or passed
    /// to the plate that took it in.
    pub(super) fn free_names(&mut self) {
        if self.next_id < MAXP {
            return;
        }
        let mut alive = [false; MAXP];
        for &p in &self.plate {
            alive[p as usize] = true;
        }
        for b in &self.bits {
            alive[b.plate as usize] = true;
        }
        for c in self.land.cells.iter().filter(|c| !c.dead) {
            alive[c.plate as usize] = true;
        }
        for p in 0..MAXP {
            if alive[p] || self.free.contains(&(p as u8)) {
                continue;
            }
            let heir = alias_of(&self.alias, p as u8);
            let gone = heir as usize == p;
            for (k, r) in self.rifts.iter_mut().enumerate() {
                for name in [&mut r.plate, &mut r.new] {
                    if *name as usize == p && !gone {
                        *name = heir;
                    } else if *name as usize == p && !self.renamed.contains(&k) {
                        self.renamed.push(k);
                    }
                }
            }
            for tr in &mut self.troughs {
                if tr.plate as usize == p {
                    tr.plate = heir;
                }
            }
            for q in 0..MAXP {
                if self.alias[q] as usize == p {
                    self.alias[q] = if gone { q as u8 } else { heir };
                }
                for (a, b) in [(p, q), (q, p)] {
                    self.parted[a][b] = true;
                    self.broken[a][b] = false;
                    self.contact[a][b] = 0;
                    self.record_of[a][b] = usize::MAX;
                }
            }
            self.alias[p] = p as u8;
            self.pulled[p] = 0;
            self.silt[p] = 0;
            self.born_t[p] = 0;
            self.omega[p] = [0; 3];
            self.group[p] = p as u8;
            self.free.push(p as u8);
        }
    }

    /// A scene's imposed turns replace the forces' (nothing to do in a
    /// planet's history).
    pub(super) fn impose(&mut self) {
        for &(p, turn) in &self.imposed {
            self.omega[p as usize] = turn;
        }
    }
}

/// A name for a plate about to be born: a new one while any is left, then
/// the name of a plate that is gone ([`World::free_names`]). None when
/// every name is borne by a living plate: the plate is not born, and the
/// history counts it (`History::unnamed`), never in silence.
pub(super) fn name(next_id: &mut usize, free: &mut Vec<u8>, unnamed: &mut u32) -> Option<u8> {
    if *next_id < MAXP {
        *next_id += 1;
        return Some((*next_id - 1) as u8);
    }
    if free.is_empty() {
        *unnamed += 1;
        return None;
    }
    Some(free.remove(0))
}

/// What the mechanisms of one step hand to one another; made anew at
/// each step.
pub(super) struct Step {
    pub(super) t: usize,
    pub(super) now_my: u16,
    /// Each plate's continental crust, km².
    pub(super) cont_of: [i64; MAXP],
    /// The plates' turns used at this step.
    pub(super) moved: Vec<V3>,
    // The bits of floor after the turn: their number, each one's cell,
    // and the bits of each cell (`order[start[q]..start[q + 1]]`).
    pub(super) n: usize,
    pub(super) at: Vec<u32>,
    pub(super) start: Vec<u32>,
    pub(super) order: Vec<u32>,
    // The grid's view before the turn.
    pub(super) old_plate: Vec<u8>,
    pub(super) old_rock: Vec<Rock>,
    // Who rides over whom (`precedence`).
    pub(super) floor: [i64; MAXP],
    pub(super) weight: [i64; MAXP],
    /// Bits of floor gone at this step.
    pub(super) dead: Vec<bool>,
    /// Cells at or beside a collision's front of the last step.
    pub(super) pressed: Vec<bool>,
    /// Each cell's bits of floor not shown.
    pub(super) spare: Vec<Vec<u32>>,
    /// Cells with no crust to show yet.
    pub(super) pending: Vec<bool>,
    /// The cells of crust that dock, and the plate they join.
    pub(super) accrete: Vec<(usize, u8)>,
    pub(super) shortened: i64,
    pub(super) made: i64,
    /// Where each bit of floor is drawn.
    pub(super) drawn: Vec<u32>,
    pub(super) built: i64,
    /// What the plates do now (`forces::look`).
    pub(super) seen: Look,
    pub(super) margin_of: Vec<u8>,
    /// The collisions' piles (`collisions::progress`).
    pub(super) piles: Vec<Pile>,
}

impl Step {
    pub(super) fn new(t: usize, now_my: u16) -> Self {
        Self {
            t,
            now_my,
            cont_of: [0; MAXP],
            moved: Vec::new(),
            n: 0,
            at: Vec::new(),
            start: Vec::new(),
            order: Vec::new(),
            old_plate: Vec::new(),
            old_rock: Vec::new(),
            floor: [0; MAXP],
            weight: [0; MAXP],
            dead: Vec::new(),
            pressed: Vec::new(),
            spare: Vec::new(),
            pending: Vec::new(),
            accrete: Vec::new(),
            shortened: 0,
            made: 0,
            drawn: Vec::new(),
            built: 0,
            seen: Look {
                kind: Vec::new(),
                margin: Vec::new(),
                slab: Vec::new(),
                suction: Vec::new(),
                ridge: Vec::new(),
                front: Vec::new(),
                hung: Vec::new(),
            },
            margin_of: Vec::new(),
            piles: Vec::new(),
        }
    }
}

/// A written scene for the bench (`examples/banc.rs`): continents and
/// plates laid by hand on the grid, in place of the supercontinent and
/// the plates a seed draws, and turns imposed on some plates in place
/// of the turns the forces give them. Everything else is the model.
#[derive(Debug, Clone)]
pub struct Scene {
    /// Each cell: continental crust or ocean floor.
    pub continent: Vec<bool>,
    /// Each cell's plate; the plates under continents come first.
    pub plate: Vec<u8>,
    /// How many plates carry continent, and how many plates in all.
    pub continent_plates: usize,
    /// How many plates in all.
    pub plates: usize,
    /// Plates whose turn is imposed at every step: the turn's axis
    /// scaled by its speed in µm a year (0 holds the plate still).
    pub turns: Vec<(u8, V3)>,
    /// The step from which the turns are imposed no more and the forces
    /// alone turn the plates (`usize::MAX`: never).
    pub free_from: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_new_then_given_again_and_running_out_is_counted() {
        let (mut next_id, mut free, mut unnamed) = (MAXP - 1, vec![7_u8, 3], 0_u32);
        assert_eq!(name(&mut next_id, &mut free, &mut unnamed), Some(63));
        assert_eq!(name(&mut next_id, &mut free, &mut unnamed), Some(7));
        assert_eq!(name(&mut next_id, &mut free, &mut unnamed), Some(3));
        assert_eq!(unnamed, 0);
        assert_eq!(name(&mut next_id, &mut free, &mut unnamed), None);
        assert_eq!(unnamed, 1);
    }
}
