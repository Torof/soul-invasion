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
    pub(super) hotspots: Vec<V3>,
    pub(super) plumes: Vec<V3>,
    pub(super) spent: Vec<bool>,
    // The plates: their turns, who moves with whom, who has parted.
    pub(super) omega: Vec<V3>,
    pub(super) group: [u8; MAXP],
    pub(super) pushes: Vec<Push>,
    pub(super) parted: [[bool; MAXP]; MAXP],
    pub(super) broken: [[bool; MAXP]; MAXP],
    pub(super) alias: [u8; MAXP],
    pub(super) born_t: [usize; MAXP],
    pub(super) next_id: usize,
    pub(super) young: usize,
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
    pub(super) closing_of: Vec<[i64; MAXP]>,
    pub(super) quiet: Vec<[usize; MAXP]>,
    pub(super) front_of: Vec<[i64; MAXP]>,
    pub(super) record_of: Vec<[usize; MAXP]>,
    pub(super) escaped: Vec<[bool; MAXP]>,
    pub(super) shed: [bool; MAXP],
    pub(super) fronts: Vec<Front>,
    // Rifts in progress, arcs, silt.
    pub(super) troughs: Vec<Trough>,
    pub(super) arc_steps: u8,
    pub(super) silt: [i64; MAXP],
    // The record: events, then the layers of each step.
    pub(super) rifts: Vec<Rift>,
    pub(super) welds: Vec<Weld>,
    pub(super) accretions: Vec<Accretion>,
    pub(super) collisions: Vec<Collision>,
    pub(super) escapes: Vec<Escape>,
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
    /// What the mechanisms of one step hand to one another.
    pub(super) step: Step,
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
    pub(super) grips: Vec<Grip>,
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
            },
            margin_of: Vec::new(),
            grips: Vec::new(),
        }
    }
}
