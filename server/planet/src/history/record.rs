//! What a history leaves: the constants and records of its events (rifts,
//! welds, collisions, ...), the layers of each step, and the `History`
//! itself.

use super::*;

/// No piece (oceanic crust), no plate.
pub const NONE: u8 = u8::MAX;
/// Why a cell of crust changed plate in a step: its plate welded to
/// another after a collision.
pub const SWITCH_WELD: u8 = 1;
/// A block pushed out sideways by a collision.
pub const SWITCH_ESCAPE: u8 = 2;
/// A terrane or an arc docked.
pub const SWITCH_DOCK: u8 = 3;
/// A lone cell of a plate inside another, on the grid's view.
pub const SWITCH_SPECK: u8 = 4;
/// A large part cut off from its plate: a plate of its own (it moves on
/// as before).
pub const SWITCH_CUT_OFF: u8 = 5;
/// A small part cut off from its plate, taken by the plate beside it.
pub const SWITCH_CAPTURE: u8 = 6;
/// A crumb of a plate, or a small plate enclosed in another, taken whole.
pub const SWITCH_CRUMB: u8 = 7;
/// A rift cut its plate in two.
pub const SWITCH_RIFT: u8 = 8;
/// No mountain building on record.
pub const NEVER: u16 = u16::MAX;
/// Plate ids are below this.
pub const MAXP: usize = 64;
/// Pieces: the supercontinent's plates first, terranes from here, island
/// arcs from [`ARC_BASE`] (plus the plate that raised them).
pub const TERRANE_BASE: u8 = 100;
/// See [`TERRANE_BASE`].
pub const ARC_BASE: u8 = 180;

/// Crust classes.
pub const OCEAN: u8 = 0;
/// See [`OCEAN`].
pub const CONTINENT: u8 = 1;
/// See [`OCEAN`]: a terrane or an island arc, light crust that does not
/// sink but is small.
pub const TERRANE: u8 = 2;

/// Margin flags a piece of crust gathers over time.
pub const RIFT: u8 = 1;
/// See [`RIFT`].
pub const PASSIVE: u8 = 2;
/// See [`RIFT`].
pub const ACTIVE: u8 = 4;
/// See [`RIFT`].
pub const SUTURE: u8 = 8;
/// See [`RIFT`].
pub const ACCRETED: u8 = 16;
/// See [`RIFT`]: an older suture, inside the supercontinent already.
pub const OLD_SUTURE: u8 = 32;
/// See [`RIFT`]: a rift that stopped before the ocean opened.
pub const FAILED_RIFT: u8 = 64;
/// See [`RIFT`]: flood basalts, poured where a plume stood as the
/// continent broke.
pub const LIP: u8 = 128;

/// What happened, in the journal of a step ([`History::journal`]): the
/// plates born and ended with the rule that made or ended them, the
/// rifts, the collisions, the terranes docked, the islands risen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum What {
    /// A trough begins on `plate` (rift `rift`).
    RiftBegun,
    /// The trough of rift `rift` is abandoned: the pull has left it.
    RiftAbandoned,
    /// `plate` is born: rift `rift` has cut plate `other` in two.
    BornOfRift,
    /// `plate` is born: a block of `other` escaping a collision sideways.
    BornEscaping,
    /// `plate` is born: a large part of `other` cut off from the rest of
    /// it on the grid is a plate of its own, with the same motion.
    BornCutOff,
    /// `plate` has ended: welded to `other` after their collision stood
    /// still.
    EndedWelded,
    /// `plate` has ended: under 100,000 km², taken by `other`, the plate
    /// it touches most.
    EndedCrumb,
    /// `plate` has ended: a small plate (under 5 M km²) enclosed in
    /// `other` alone, taken by it.
    EndedEnclosed,
    /// `plate` has ended with no rule naming it: no cell of the grid is
    /// left to it (its floor sank, its last cells were taken one by one).
    EndedNoCell,
    /// A small part of `plate`, cut off from the rest of it, is taken by
    /// `other`.
    PartTaken,
    /// `plate` moves on its own again: the forces no longer press it
    /// against the plates it moved as one with (`other`: that group).
    MovesAlone,
    /// `plate` moves as one with `other` again: their continents still
    /// touch (they have not parted) and the forces press them together.
    /// On the maps they are one plate.
    MovesWith,
    /// The continents of `plate` and `other` meet: a collision begins.
    CollisionBegun,
    /// Terranes or island arcs dock on `plate` (they rode `other`); the
    /// events of a step are added like the islands'.
    Docked,
    /// Islands rise on the edge of `plate`, over the floor of `other`
    /// sinking under it: an island arc (the events of a step are added:
    /// `km2` is their sum, `at` the first).
    IslandsRisen,
    /// A boundary opens inside a continent, between `plate` and `other`.
    Opened,
    /// A plume floods the continent with basalt where a boundary opens.
    Lava,
}

/// An event of the journal: what, to which plates, where, how large.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    /// What happened.
    pub what: What,
    /// The plate it happened to (the one born, the one ended).
    pub plate: u8,
    /// The other plate ([`NONE`] if none): see [`What`].
    pub other: u8,
    /// Where (a zero vector: nowhere in particular).
    pub at: V3,
    /// Its area, km² (0: not an area).
    pub km2: i64,
    /// The rift, an index into [`History::rifts`] (`u16::MAX`: none).
    pub rift: u16,
}

impl Event {
    pub(super) fn plates(what: What, (plate, other): (usize, usize), at: V3, km2: i64) -> Self {
        Self {
            what,
            plate: plate as u8,
            other: other as u8,
            at,
            km2,
            rift: u16::MAX,
        }
    }

    pub(super) fn rift(what: What, plate: u8, at: V3, rift: usize) -> Self {
        Self {
            what,
            plate,
            other: NONE,
            at,
            km2: 0,
            rift: rift as u16,
        }
    }
}

/// The middle of the cells that pass (a zero vector if none does).
pub(super) fn middle(sphere: &Sphere, pass: impl Fn(usize) -> bool) -> V3 {
    normalize((0..CELLS).filter(|&c| pass(c)).fold([0; 3], |acc, c| {
        add(acc, sphere.centre[c].map(|v| v / 1000))
    }))
}

/// What a cell of continental crust is at one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Margin {
    /// Inside a continent, or the ocean.
    Inland = 0,
    /// A rift: the continent pulled apart, new ocean beside it.
    Rift = 1,
    /// A coast that trails: its ocean moves with it.
    Passive = 2,
    /// A coast over a sinking ocean.
    Active = 3,
    /// Where two continents collide.
    Suture = 4,
}

/// How a plate turns at a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Motion {
    /// The Euler pole it turns about.
    pub pole: V3,
    /// Its speed at 90° from the pole, mm a year.
    pub speed_mm: i64,
    /// The turn itself, a vector along the pole, µm a year: what moves
    /// the plate's crust in the next step.
    pub omega: V3,
}

/// A rift (version 11): a process over several steps. It starts as a
/// trough where a plate is pulled apart and weak, grows along its
/// length, and either cuts the plate in two (an ocean opens between two
/// shores that fit) or is abandoned (a failed rift: its thinned crust
/// stays, a basin).
#[derive(Debug, Clone, Copy)]
pub struct Rift {
    /// When its trough began, millions of years ago.
    pub my: u16,
    /// The plate it tears, and the plate born of it ([`NONE`] until the
    /// plate is cut).
    pub plate: u8,
    /// See `plate`.
    pub new: u8,
    /// Where it began.
    pub centre: V3,
    /// The continent its plate carried when it began, km².
    pub area_km2: i64,
    /// Abandoned: a failed rift.
    pub aborted: bool,
    /// When the crust gave way and the plate was cut in two, millions of
    /// years ago ([`NEVER`]: not yet, or never).
    pub cut_my: u16,
    /// When its two shores no longer touched anywhere, millions of years
    /// ago ([`NEVER`]: not yet, or never).
    pub parted_my: u16,
    /// When it was abandoned, millions of years ago ([`NEVER`]: not).
    pub failed_my: u16,
    /// Its trough's greatest length, cells of crust.
    pub cells: u32,
    /// The trough's length along its cells when the plate was cut, and
    /// the straight distance between its two ends, km (0 until then).
    pub along_km: u32,
    /// See `along_km`.
    pub straight_km: u32,
    /// How far the trough strays from the straight line through its two
    /// ends when the plate is cut, at most, km.
    pub bow_km: u32,
    /// The cells of crust along the trough when the plate was cut, and
    /// those of them that lie in a belt of the mosaic (0 until then).
    pub cut_cells: u32,
    /// See `cut_cells`.
    pub belt_cells: u32,
}

/// Two plates welded by a collision.
#[derive(Debug, Clone, Copy)]
pub struct Weld {
    /// When, millions of years ago.
    pub my: u16,
    /// The plate kept (the more continental), and the one it absorbed.
    pub kept: u8,
    /// See `kept`.
    pub absorbed: u8,
    /// Each one's continent, km².
    pub kept_km2: i64,
    /// See `kept_km2`.
    pub absorbed_km2: i64,
    /// Their continents had parted since they last touched (false: an
    /// aborted rift closing).
    pub parted: bool,
}

/// A terrane or an arc accreted to a continent.
#[derive(Debug, Clone, Copy)]
pub struct Accretion {
    /// When, millions of years ago.
    pub my: u16,
    /// The plate it joined.
    pub plate: u8,
    /// Its piece.
    pub piece: u8,
    /// Its area, km².
    pub km2: i64,
}

/// A collision between two continents, from their first contact.
#[derive(Debug, Clone, Copy)]
pub struct Collision {
    /// The two plates.
    pub a: u8,
    /// See `a`.
    pub b: u8,
    /// First contact, millions of years ago.
    pub from_my: u16,
    /// When they welded, millions of years ago ([`NEVER`]: not welded).
    pub welded_my: u16,
    /// How far they closed after the contact, km.
    pub closed_km: i64,
    /// The longest front, cells of ~100 km.
    pub front_cells: i64,
    /// Steps of contact.
    pub steps: usize,
}

/// A plate, or a part of one cut off from the rest, taken by the plate
/// beside it.
#[derive(Debug, Clone, Copy)]
pub struct Capture {
    /// When, millions of years ago.
    pub my: u16,
    /// The plate it was, and the plate that took it.
    pub plate: u8,
    /// See `plate`.
    pub by: u8,
    /// Its area, km².
    pub km2: i64,
    /// The whole plate (false: a part cut off from the rest).
    pub whole: bool,
}

/// A block of a riding plate pushed out sideways by a collision.
#[derive(Debug, Clone, Copy)]
pub struct Escape {
    /// When, millions of years ago.
    pub my: u16,
    /// The plate it left, and its own.
    pub from: u8,
    /// See `from`.
    pub block: u8,
    /// Its continent, km².
    pub km2: i64,
}

/// Flood basalts.
#[derive(Debug, Clone, Copy)]
pub struct Lip {
    /// When, millions of years ago.
    pub my: u16,
    /// The plume.
    pub at: V3,
    /// The continent flooded, km².
    pub km2: i64,
}

/// The whole history.
pub struct History {
    /// The supercontinent's middle.
    pub upwelling: V3,
    /// Per step, the mantle's heat under each cell (0: its mean; about a
    /// thousand under a large continent's heart, negative where slabs
    /// sink).
    pub heat_at: Vec<Vec<i16>>,
    /// Plates at the start: on the supercontinent, and in all.
    pub plates_at_start: (usize, usize),
    /// Terranes placed in the ocean at the start.
    pub terranes: usize,
    /// Per step, each cell's plate.
    pub plate: Vec<Vec<u8>>,
    /// Per step, each cell's crust class ([`OCEAN`], [`CONTINENT`],
    /// [`TERRANE`]).
    pub class: Vec<Vec<u8>>,
    /// Per step, each cell's crust age, millions of years (the ocean
    /// floor's age at sea).
    pub age: Vec<Vec<u16>>,
    /// Per step, each cell's boundary: 0 none, else `Kind as u8 + 1`, from
    /// the plates' relative motion at that step.
    pub kind: Vec<Vec<u8>>,
    /// Per step, each cell's [`Margin`].
    pub margin: Vec<Vec<u8>>,
    /// Per step, the cell each cell's crust was at the start, or was born
    /// at.
    pub home_at: Vec<Vec<u32>>,
    /// Per step, the margin flags each cell's crust has gathered.
    pub margins_at: Vec<Vec<u8>>,
    /// Per step, each cell's last mountain building, millions of years
    /// before today ([`NEVER`]: none).
    pub orogeny_at: Vec<Vec<u16>>,
    /// Per step, each cell's crust thickness, tenths of a km.
    pub thick_at: Vec<Vec<u16>>,
    /// Per step, the height each cell's continental crust carries, m
    /// above the sea of the start, without its grain and its pile's lift
    /// (version 7).
    pub high_at: Vec<Vec<i16>>,
    /// Per step, what changed each cell's carried height since the step
    /// before, m, cause by cause (see [`CAUSES`]).
    pub cause_at: Vec<Vec<[i16; CAUSES]>>,
    /// Per step, how the grid's view draws each cell of continental
    /// crust: 0 crust it drew at the step before or that is new, 1 crust
    /// of a piece it did not draw at the step before (the view only: the
    /// pieces themselves are never redrawn, version 10).
    pub shown_at: Vec<Vec<u8>>,
    /// Per step, the cells of the pieces the grid's view does not draw
    /// (and no other piece covers), those of them it drew at the step
    /// before, and 0 (no cell is a copy any more).
    pub hidden_at: Vec<[u32; 3]>,
    /// Per step, the name of the crust each cell draws (`u32::MAX`: floor
    /// drawn again in a hole of the floor's sampling): the same crust
    /// keeps its name for good.
    pub id_at: Vec<Vec<u32>>,
    /// Per step, the continents as they are kept (version 10): rigid
    /// pieces, each with its own lattice and the turn that places it on
    /// the planet. The grid's layers above are a view of them.
    pub pieces_at: Vec<Vec<Piece>>,
    /// The relief's grain at each cell, m: fine, and broad. A continent's
    /// crust keeps the grain of its home (fine plus half the broad one);
    /// the ocean floor has the fine one of where it lies.
    pub rough: Vec<i16>,
    /// The broad grain (see [`History::rough`]).
    pub broad: Vec<i16>,
    /// The hotspots, fixed under the plates: plumes risen where the mantle
    /// was hottest at the start.
    pub hotspots: Vec<V3>,
    /// Per step, the hotspots' marks on the crust: where, which hotspot,
    /// and when it was made, millions of years ago.
    pub marks_at: Vec<Vec<(V3, u8, u16)>>,
    /// Per step, each plate id's motion (ids without cells turn by 0).
    pub motion: Vec<Vec<Motion>>,
    /// Per step, the turn that carried each plate id's crust in that
    /// step (as [`Motion::omega`]; none at the start).
    pub turned_at: Vec<Vec<V3>>,
    /// Per step, the continental crust's area, km².
    pub continental_km2: Vec<i64>,
    /// Per step, continental crust lost to collisions since the step
    /// before, km² (shortening).
    pub shortened_km2: Vec<i64>,
    /// Per step, island-arc crust made since the step before, km².
    pub made_km2: Vec<i64>,
    /// The rifts, in order of their beginning.
    pub rifts: Vec<Rift>,
    /// Per step, the cells of crust along each rift's trough: where, the
    /// rift (an index into [`History::rifts`]), and whether it is
    /// abandoned.
    pub rifts_at: Vec<Vec<(V3, u16, bool)>>,
    /// The welds, in order.
    pub welds: Vec<Weld>,
    /// The accretions, in order.
    pub accretions: Vec<Accretion>,
    /// The collisions, in order of first contact.
    pub collisions: Vec<Collision>,
    /// The blocks escaped sideways.
    pub escapes: Vec<Escape>,
    /// The plates and cut-off parts taken by their neighbours, in order.
    pub captures: Vec<Capture>,
    /// Plates born of a large part cut off from its plate.
    pub cut_off: usize,
    /// Per step, the margins built out of silt since the step before, km².
    pub built_km2: Vec<i64>,
    /// The flood basalts.
    pub lips: Vec<Lip>,
    /// The journal: per step, what happened to the plates since the step
    /// before, each event with its rule and its place ([`What`]).
    pub journal: Vec<Vec<Event>>,
    /// Today: the cell each cell's crust was at the start, or was born at.
    pub home: Vec<u32>,
    /// Today: each cell's piece ([`NONE`]: ocean).
    pub piece: Vec<u8>,
    /// Today: each cell's crust age, millions of years.
    pub age_my: Vec<u16>,
    /// Today: the margin flags the crust gathered.
    pub margins: Vec<u8>,
    /// Today: the last mountain building, millions of years ago.
    pub orogeny_my: Vec<u16>,
    /// The supercontinent's field at each cell at the start (its crust
    /// where at least [`History::crust_cut`]).
    pub field: Vec<i64>,
    /// Today: the crust's value at its home (the supercontinent's shape).
    pub crust: Vec<i64>,
    /// The threshold of the supercontinent's crust.
    pub crust_cut: i64,
}

impl History {
    /// The last step.
    pub fn last(&self) -> usize {
        self.plate.len() - 1
    }
}

/// Step 12 (the record). The crust as drawn, cell by cell, for the
/// history's layers.
pub(super) fn write(world: &mut World) {
    let sphere = world.sphere;
    let journal = &mut world.journal;
    let troughs = &mut world.troughs;
    let land = &mut world.land;
    let bits = &mut world.bits;
    let disp = &mut world.disp;
    let top = &mut world.top;
    let marks = &mut world.marks;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let out_plate = &mut world.out_plate;
    let out_class = &mut world.out_class;
    let out_age = &mut world.out_age;
    let out_kind = &mut world.out_kind;
    let out_margin = &mut world.out_margin;
    let out_motion = &mut world.out_motion;
    let out_home = &mut world.out_home;
    let out_margins = &mut world.out_margins;
    let out_orogeny = &mut world.out_orogeny;
    let out_thick = &mut world.out_thick;
    let out_high = &mut world.out_high;
    let out_cause = &mut world.out_cause;
    let out_shown = &mut world.out_shown;
    let out_id = &mut world.out_id;
    let out_hidden = &mut world.out_hidden;
    let out_marks = &mut world.out_marks;
    let out_pieces = &mut world.out_pieces;
    let out_rifts = &mut world.out_rifts;
    let continental_km2 = &mut world.continental_km2;
    let omega = &mut world.omega;
    let group = &mut world.group;
    let last_kind = &mut world.last_kind;
    let shown_now = &mut world.shown_now;
    let hidden_now = &mut world.hidden_now;
    let now_my = world.step.now_my;
    let seen = &mut world.step.seen;
    // The crust as drawn, cell by cell.
    out_plate.push(
        plate
            .iter()
            .map(|&p| group[p as usize])
            .collect::<Vec<u8>>(),
    );
    out_class.push(rock.iter().map(|r| r.class).collect::<Vec<u8>>());
    out_age.push(
        rock.iter()
            .map(|r| r.born_my.saturating_sub(now_my))
            .collect::<Vec<u16>>(),
    );
    last_kind.clone_from(&seen.kind);
    out_kind.push(std::mem::take(&mut seen.kind));
    out_home.push(rock.iter().map(|r| r.home).collect::<Vec<u32>>());
    out_margins.push(rock.iter().map(|r| r.margins).collect::<Vec<u8>>());
    out_orogeny.push(rock.iter().map(|r| r.orogeny).collect::<Vec<u16>>());
    out_thick.push(rock.iter().map(|r| r.thick).collect::<Vec<u16>>());
    out_high.push(rock.iter().map(|r| r.high).collect::<Vec<i16>>());
    out_cause.push(rock.iter().map(|r| r.why).collect::<Vec<[i16; CAUSES]>>());
    out_shown.push(shown_now.clone());
    out_id.push(
        (0..CELLS)
            .map(|q| {
                if top[q] != NO_BIT {
                    land.cells[top[q] as usize].id
                } else if disp[q] != NO_BIT {
                    bits[disp[q] as usize].id
                } else {
                    NO_BIT
                }
            })
            .collect::<Vec<u32>>(),
    );
    out_pieces.push(land.pieces(&*group, &floor_born(&*plate, &*rock)));
    out_rifts.push(
        troughs
            .iter()
            .flat_map(|tr| {
                tr.path
                    .iter()
                    .map(|&i| (land.cells[i as usize].pos, tr.record as u16, tr.failed))
            })
            .collect::<Vec<(V3, u16, bool)>>(),
    );
    out_hidden.push(*hidden_now);
    out_marks.push(
        marks
            .iter()
            .map(|m| (m.pos, m.spot, m.my))
            .collect::<Vec<(V3, u8, u16)>>(),
    );
    out_margin.push(std::mem::take(&mut seen.margin));
    out_motion.push(
        omega
            .iter()
            .map(|&w| Motion {
                pole: normalize(w),
                speed_mm: norm(w) / 1000,
                omega: w,
            })
            .collect::<Vec<Motion>>(),
    );
    continental_km2.push(land.weight().iter().sum());
    // The journal: the plates that began or stopped moving as one with
    // others since the last step was written, the plates left with no
    // cell, and the islands of the step added by trench.
    let mut now = [NONE; MAXP];
    for &p in plate.iter() {
        now[p as usize] = group[p as usize];
    }
    let ended = |journal: &[Event], p: usize| {
        journal.iter().any(|e| {
            e.plate as usize == p
                && matches!(
                    e.what,
                    What::EndedWelded | What::EndedCrumb | What::EndedEnclosed
                )
        })
    };
    for p in 0..MAXP {
        let (before, after) = (world.was[p], now[p]);
        if world.step.t == 0 || before == after {
            continue;
        }
        let of = |plates: &[u8]| middle(sphere, |c| plates[c] as usize == p);
        if after == NONE {
            if !ended(journal, p) {
                let last = &out_plate[out_plate.len() - 2];
                let at = middle(sphere, |c| last[c] == before && plate[c] != before);
                journal.push(Event::plates(What::EndedNoCell, (p, NONE as usize), at, 0));
            }
        } else if before != NONE && after == p as u8 {
            journal.push(Event::plates(
                What::MovesAlone,
                (p, before as usize),
                of(plate),
                0,
            ));
        } else if before != NONE {
            journal.push(Event::plates(
                What::MovesWith,
                (p, after as usize),
                of(plate),
                0,
            ));
        }
    }
    world.was = now;
    let mut told: Vec<Event> = Vec::with_capacity(journal.len());
    for e in journal.drain(..) {
        if matches!(e.what, What::IslandsRisen | What::Docked)
            && let Some(first) = told
                .iter_mut()
                .find(|o| (o.what, o.plate, o.other) == (e.what, e.plate, e.other))
        {
            first.km2 += e.km2;
        } else {
            told.push(e);
        }
    }
    world.out_journal.push(told);
}
/// The history, once its last step has run.
pub(crate) fn finish(world: World) -> History {
    let World {
        land,
        top,
        mut plate,
        mut rock,
        field,
        crust_cut,
        upwelling,
        plates_at_start,
        placed,
        rough,
        broad,
        hotspots,
        out_plate,
        out_class,
        out_age,
        out_kind,
        out_margin,
        out_motion,
        out_home,
        out_margins,
        out_orogeny,
        out_thick,
        out_high,
        out_cause,
        out_shown,
        out_id,
        out_hidden,
        out_marks,
        out_heat,
        out_pieces,
        out_turned,
        out_rifts,
        continental_km2,
        shortened_km2,
        made_km2,
        rifts,
        welds,
        accretions,
        collisions,
        escapes,
        captures,
        cut_off,
        built_km2,
        lips,
        out_journal,
        ..
    } = world;
    paint(&land, &top, &mut plate, &mut rock);
    let mut crust = vec![crust_cut - 400; CELLS];
    for c in 0..CELLS {
        if rock[c].class != OCEAN {
            crust[c] = field[rock[c].home as usize].max(crust_cut);
        }
    }
    History {
        upwelling,
        heat_at: out_heat,
        plates_at_start,
        terranes: placed as usize,
        plate: out_plate,
        class: out_class,
        age: out_age,
        kind: out_kind,
        margin: out_margin,
        home_at: out_home,
        margins_at: out_margins,
        orogeny_at: out_orogeny,
        thick_at: out_thick,
        high_at: out_high,
        cause_at: out_cause,
        shown_at: out_shown,
        id_at: out_id,
        pieces_at: out_pieces,
        hidden_at: out_hidden,
        rough,
        broad,
        marks_at: out_marks,
        hotspots,
        motion: out_motion,
        turned_at: out_turned,
        continental_km2,
        shortened_km2,
        made_km2,
        rifts,
        rifts_at: out_rifts,
        welds,
        accretions,
        collisions,
        escapes,
        captures,
        cut_off,
        built_km2,
        lips,
        journal: out_journal,
        home: rock.iter().map(|r| r.home).collect(),
        piece: rock
            .iter()
            .map(|r| if r.class == OCEAN { NONE } else { r.piece })
            .collect(),
        age_my: rock.iter().map(|r| r.born_my).collect(),
        margins: rock.iter().map(|r| r.margins).collect(),
        orogeny_my: rock.iter().map(|r| r.orogeny).collect(),
        crust,
        field,
        crust_cut,
    }
}
