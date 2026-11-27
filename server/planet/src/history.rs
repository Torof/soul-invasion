//! The plates' history (version 4, the lead's 26 November: « la géologie,
//! c'est des plaques qui bougent »). The plates are what moves, through
//! the whole history; the continents ride on them.
//!
//! 1. **A supercontinent**: one warped cap carrying all the continental
//!    crust, its interior already old (cratons) and seamed with older
//!    sutures.
//! 2. **Plates.** The supercontinent lies on several plates (a warped,
//!    weighted Voronoi of its crust) and oceanic plates share the rest of
//!    the sphere. At every step each cell belongs to exactly one plate;
//!    each plate turns about its own Euler pole and carries all its crust,
//!    oceanic and continental, with the crust's record.
//! 3. **Forces, a balance on each plate at each step** (version 6). A
//!    plate's turn comes from the forces on its cells, summed as a
//!    torque: slab pull, the strongest, draws a plate toward every trench
//!    where it sinks, the more the older (heavier) its floor there; the
//!    same slab draws the plate riding over it toward the trench (trench
//!    suction: the pull a plate all continent has); ridges push, a
//!    little; the mantle carries the continents down the slope of its
//!    heat; a collision's front resists. Against them the mantle's drag,
//!    which grows with speed and is stronger under continental keels.
//!    **The mantle's heat** is one layer on the grid: warm under a large
//!    continent (a lid), cold where slabs sink, with the deep mantle's
//!    own provinces; it lingers and spreads. The heat under the
//!    supercontinent is what breaks it (no scripted upwelling), and
//!    plumes rise where it is hottest.
//! 4. **Divergent boundaries** leave gaps: new oceanic crust of age 0
//!    fills them from both sides, so the ocean floor carries symmetric age
//!    stripes.
//! 5. **Convergent boundaries.** Where two plates claim a cell, oceanic
//!    crust sinks and is destroyed: always under a continent, and the
//!    older, denser one under the younger; the riding edge of an ocean
//!    grows an island arc. Continental crust never sinks: continent
//!    against continent is a collision, a process (version 5): the plates
//!    stay apart, the front resists and slows them, the crust run under
//!    piles up (thicker, higher, spreading and wearing down), a block of
//!    the riding plate may escape sideways, and the two weld only once
//!    they have stopped closing. A terrane or an arc against a continent
//!    is accreted to its plate.
//! 6. **The break-up only opens.** Plates whose continents touch and have
//!    not parted move as one while the forces press them together: a
//!    boundary inside a continent exists only once it opens.
//! 7. **Rifts** (version 11, the lead's 27 November: « de vrais
//!    rifts »). The world starts with few plates, the supercontinent on
//!    one: plates are born of rifts, and a rift is a process. Where the
//!    forces on a plate's two sides would pull them apart and the
//!    lithosphere is weak (a warm mantle under it, an old suture, an old
//!    failed rift), a trough begins: the crust along it stretches, thins
//!    and sinks step after step, slowly at first, faster as it necks;
//!    its tips grow toward the weakest crust ahead. Once the trough has
//!    crossed its continent and its crust has given way, the plate is
//!    cut in two along it, each side keeping its lattice (the two shores
//!    fit), and the forces alone part them. A trough the pull has left
//!    is abandoned: a failed rift, its thinned crust a basin. No size
//!    threshold, no drawn date, no push added. Where a plume stands near
//!    a boundary that opens, the continent is flooded with basalt.
//! 8. **Hotspots** mark the crust passing over them, a mark a million
//!    years, carried with the crust: their chains bend as the plates
//!    turned, and are as old as the history.
//! 9. **Relief** (version 7, the lead's 27 November). A continent's
//!    height is carried by its crust, like its thickness, and changes
//!    only when something happens to that crust: a collision piles it up
//!    and it floats higher; a rift stretches it and it sinks, then sinks
//!    as much again as it cools (a passive margin's shelf); an arc, a
//!    docking terrane or a plume raise it where they act; high ground
//!    wears down slowly and its sediment fills the low ground. The
//!    supercontinent starts with a relief of its own, once. Nothing is
//!    recomputed from where the crust happens to lie.
//!
//! Moving the crust. The ocean floor (version 5) is a set of bits at
//! their exact places; each step every bit turns with its plate, and the
//! grid only draws them: each cell shows one bit (the winner where
//! several meet), a bit shows in one cell at most, and nothing is copied.
//!
//! The continents (version 10, the lead's 27 November: « de vraies
//! pièces rigides ») are rigid pieces. Every cell of continental crust
//! lives for good in its piece's own frame (the grid's lattice as it lay
//! when the piece was born) with the turn that places the frame on the
//! planet today. Moving a piece changes its turn only: nothing in it is
//! resampled, ever. A piece changes only by named events applied in its
//! frame: a collision's front (its cells run under another piece pile on
//! it), a rift (the piece is cut in two, each half keeping the lattice,
//! so the two shores fit for good), an arc or a docking, wear, silt. The
//! planet's grid is a view of the pieces (each cell shows the crust over
//! its centre), drawn for the forces and for the neighbourhood queries
//! (who touches whom): no state of a continent lives on it. Today's
//! plates are the plates of the last step.

use crate::{
    CELLS, Kind, ONE, P_RELIEF, RADIUS_KM, Sphere, TAU, V3, add, bump, classify, cos, cross, dot,
    draw, east_at, fbm3, hash, km_between, latitude, norm, normalize, north_at, quantile,
    random_unit, scale, sin, step, sub, warp,
};

/// A world's character, the model's input: a supercontinent of this
/// size and shape on so many plates, oceanic plates around it, forces of
/// this strength, so long a history. Read from a RON file by the example
/// (`monde-defaut.ron` beside the page lists them, each with what it
/// changes); missing fields take the defaults.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The planet's radius, km. Fixed at the Earth's for now: the grid and
    /// the zoom are measured on it.
    pub radius_km: i64,
    /// Share left dry by the sea today, per mille (130: an ice-free planet,
    /// its seas high; the lead's 27 November).
    pub land_per_mille: i64,
    /// Share of the surface carried by continental crust, per mille: the
    /// supercontinent's size (the land, plus the shelves).
    pub crust_per_mille: i64,
    /// How far the supercontinent's outline wanders from a round cap, per
    /// mille of the default warp (0: a round cap).
    pub supercontinent_warp: i64,
    /// How ragged its coast is, per mille of the default fray.
    pub supercontinent_fray: i64,
    /// How much the supercontinent is squeezed toward the equator, per
    /// mille (1000: a round cap; more: a band, less land at the poles).
    pub supercontinent_flatten: i64,
    /// The supercontinent's middle stays within this latitude, tenths of a
    /// degree (low: it straddles the equator, as Pangaea did).
    pub supercontinent_latitude: i64,
    /// Plates the supercontinent lies on at the start. One since version
    /// 11: plates are born of rifts. With more, it also breaks along
    /// their borders.
    pub continent_plates: (i64, i64),
    /// How unequal those plates are, [`ONE`] units of the largest seed
    /// weight (0: all alike).
    pub plate_inequality: i64,
    /// Oceanic plates at the start, around the supercontinent.
    pub ocean_plates: (i64, i64),
    /// A plate younger than twice this many steps does not weld.
    pub rift_steps: usize,
    /// Steps of the history.
    pub steps: usize,
    /// Each step's length, millions of years.
    pub step_my: i64,
    /// Plate speeds, mm a year: the least a plate is given today (for its
    /// drawn motion), and a safety ceiling only tiny plates touch (the
    /// mantle's drag, [`Settings::mantle_drag_mm`], is what slows them).
    pub speed_mm: (i64, i64),
    /// The mantle's drag grows with speed: at this speed, mm a year, it is
    /// twice a plain drag (the fast plates slow, no ceiling).
    pub mantle_drag_mm: i64,
    /// A collision's front grips the two plates: each step it takes away
    /// a share of their relative motion, ‰, of this times the front's
    /// length (cell sides) times a thousand over the smaller continent's
    /// cells (at most nine tenths): they slow as the front grows. Low on
    /// purpose: a strong grip jams the breakup's pieces against each other
    /// and they never spread (27 November).
    pub collision_resist: i64,
    /// Two colliding plates weld once they have moved against each other
    /// slower than this, mm a year, for four steps.
    pub weld_stop_mm: i64,
    /// The mantle's heat moves the continents: mm a year down a slope of
    /// the heat field as steep as under a supercontinent's edge (hot
    /// under a large continent, cold where slabs sink).
    pub heat_push: i64,
    /// Slab pull, the strongest force: mm a year per trench cell, shared
    /// by all the plate's cells (a plate's speed is about this times its
    /// trench cells over its cells), for a floor of
    /// [`Settings::slab_age_my`].
    pub slab_pull: i64,
    /// The floor's age, millions of years, at which a slab pulls at full
    /// strength: an older one pulls more (up to half as much again), a
    /// younger one less, one under [`SLAB_YOUNG_MY`] not at all.
    pub slab_age_my: i64,
    /// Trench suction: the sinking slab draws the plate riding over it
    /// toward the trench, mm a year per trench cell (as `slab_pull`). It
    /// is the pull a plate all continent has.
    pub slab_suction: i64,
    /// Ridge push, mm a year per ridge cell, shared the same way.
    pub ridge_push: i64,
    /// How much a continental keel slows its plate, per mille of drag
    /// added for a plate all continent.
    pub continent_drag: i64,
    /// A push on continents toward the equator, mm a year at 45° (the
    /// pole-fleeing force, « Polflucht »). 0 since version 8: the real
    /// force is far too weak to move a plate, and nothing else sorts
    /// continents by latitude.
    pub pole_flight: i64,
    /// Steps of subduction under an ocean's edge before its island arc
    /// rises (a third of its cells becomes arc crust).
    pub arc_steps: i64,
    /// Terranes waiting in the ocean to be accreted.
    pub terranes: (i64, i64),
    /// A terrane's length, cells of ~100 km.
    pub terrane_cells: (i64, i64),
    /// Hotspots: plumes rise where the mantle is hottest.
    pub hotspots: (i64, i64),
    /// A range loses this much of its height above a plain continent each
    /// step, per mille (its silt builds the coasts out).
    pub mountain_wear: i64,
    /// The sea follows the ocean floor's age: of each metre the floor
    /// stood higher than today's on average (a younger floor), the sea
    /// stood this much higher, per mille.
    pub sea_follows_floor: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            radius_km: RADIUS_KM,
            land_per_mille: 130,
            crust_per_mille: 240,
            supercontinent_warp: 1000,
            supercontinent_fray: 1000,
            supercontinent_latitude: 150,
            supercontinent_flatten: 2200,
            continent_plates: (1, 1),
            plate_inequality: 100_000,
            ocean_plates: (3, 5),
            rift_steps: 4,
            steps: 40,
            step_my: 5,
            speed_mm: (10, 250),
            mantle_drag_mm: 150,
            collision_resist: 3,
            weld_stop_mm: 5,
            heat_push: 150,
            slab_pull: 7_500,
            slab_age_my: 80,
            slab_suction: 2_500,
            ridge_push: 300,
            continent_drag: 800,
            pole_flight: 0,
            arc_steps: 4,
            terranes: (18, 28),
            terrane_cells: (1, 5),
            hotspots: (14, 20),
            mountain_wear: 100,
            sea_follows_floor: 150,
        }
    }
}

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

/// A cell of continental crust in its piece, at a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    /// Its cell of the piece's own lattice (the grid's, turned): the
    /// same for good.
    pub at: u32,
    /// Its name, for good.
    pub id: u32,
    /// Its plate (as in [`History::plate`]).
    pub plate: u8,
    /// Its own plate's id (plates held together share the one above).
    pub own: u8,
    /// Why it changed plate in the step (a `SWITCH_*`), 0 if it did not.
    pub switch: u8,
    /// [`CONTINENT`] or [`TERRANE`].
    pub class: u8,
    /// Whether it lies under another piece's crust (a collision).
    pub under: bool,
    /// The margin flags it has gathered.
    pub margins: u8,
    /// Its thickness, tenths of a km.
    pub thick: u16,
    /// The height it carries, m (as [`History::high_at`]).
    pub high: i16,
    /// Where it was at the start, or was born: its relief's grain.
    pub home: u32,
    /// The area it stands for, km².
    pub area_km2: u32,
    /// What changed its height since the step before, m, by cause.
    pub why: [i16; CAUSES],
    /// Its precedence where pieces overlap (the higher shows).
    pub rank: (i64, i64, i64),
}

/// A rigid piece of continent at a step (version 10): cells of its own
/// lattice, and the turn that places the lattice on the planet. Moving
/// it changes the turn only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    /// Its frame's name: a piece cut in two gives two frames.
    pub frame: u32,
    /// The turn from its lattice to the planet.
    pub turn: [[i64; 3]; 3],
    /// Its cells.
    pub tiles: Vec<Tile>,
}

impl Piece {
    /// Where one of its cells is on the planet.
    #[must_use]
    pub fn place(&self, sphere: &Sphere, tile: usize) -> V3 {
        normalize(apply(
            &self.turn,
            sphere.centre[self.tiles[tile].at as usize],
        ))
    }
}

/// The continents at a step, read at any point of the planet: the cell
/// of a piece's lattice that holds the point, exactly (the grid's view
/// reads only its cells' centres).
pub struct Pieces {
    back: Vec<M3>,
    slot: Slots,
    near: Vec<Vec<u32>>,
}

impl Pieces {
    /// The index of a step's pieces.
    #[must_use]
    pub fn new(sphere: &Sphere, pieces: &[Piece]) -> Self {
        let mut slot = Slots::default();
        let mut near: Vec<Vec<u32>> = vec![Vec::new(); CELLS];
        for (k, piece) in pieces.iter().enumerate() {
            let k = k as u32;
            for (i, tile) in piece.tiles.iter().enumerate() {
                slot.insert(slot_key(k, tile.at), i as u32);
                let q = sphere.cell_of(piece.place(sphere, i));
                for m in std::iter::once(q).chain(sphere.links[q].iter().map(|&(m, _)| m as usize))
                {
                    if !near[m].contains(&k) {
                        near[m].push(k);
                    }
                }
            }
        }
        Self {
            back: pieces.iter().map(|p| back_of(&p.turn)).collect(),
            slot,
            near,
        }
    }

    /// The cell of a piece at a place of its lattice, if it holds one.
    #[must_use]
    pub fn tile(&self, piece: usize, at: u32) -> Option<usize> {
        self.slot
            .get(&slot_key(piece as u32, at))
            .map(|&i| i as usize)
    }

    /// The crust at a point: its piece and its cell in the piece.
    #[must_use]
    pub fn at(&self, sphere: &Sphere, pieces: &[Piece], p: V3) -> Option<(usize, usize)> {
        let mut best: Option<(Rank, std::cmp::Reverse<u32>, (usize, usize))> = None;
        for &k in &self.near[sphere.cell_of(p)] {
            let at = sphere.cell_of(normalize(apply(&self.back[k as usize], p))) as u32;
            if let Some(&i) = self.slot.get(&slot_key(k, at)) {
                let piece = &pieces[k as usize];
                let key = (
                    piece.tiles[i as usize].rank,
                    std::cmp::Reverse(piece.frame),
                    (k as usize, i as usize),
                );
                if best.is_none_or(|b| key > b) {
                    best = Some(key);
                }
            }
        }
        best.map(|b| b.2)
    }
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

const P_UP: u64 = 0x0c50_0101;
const P_SUPER: u64 = 0x0c50_0102;
const P_FRAG: u64 = 0x0c50_0103;
const P_HEAT: u64 = 0x0c50_0104;
const P_HOT: u64 = 0x0c50_010d;
const P_TERRANE: u64 = 0x0c50_0106;
const P_AGE: u64 = 0x0c50_0107;
const P_OCEAN: u64 = 0x0c50_0108;
const P_ARC: u64 = 0x0c50_010a;
const P_SILT: u64 = 0x0c50_010c;
const P_ESCAPE: u64 = 0x0c50_010b;

/// The supercontinent's default warp and fray: an irregular cap, not a
/// disc (scaled by [`Settings::supercontinent_warp`] and `_fray`).
const SUPER_WARP: [(i64, i64); 2] = [(700_000, 260_000), (250_000, 80_000)];
const SUPER_FRAY: [(i64, i64); 3] = [(600_000, 4), (250_000, 2), (100_000, 1)];
/// How much the default fray moves the coast, the cap's cosine units (‰).
const FRAY: i64 = 600;
/// The plates' borders' warp: wandering edges, not a Voronoi's lines.
const RIFT_WARP: [(i64, i64); 3] = [(400_000, 140_000), (150_000, 45_000), (60_000, 15_000)];
/// The mantle's heat, one layer on the grid (0: the mantle's mean). A
/// continent is a lid: the mantle under it warms toward a thousand at
/// [`HEAT_LID_KM`] from the sea, and on at that slope up to
/// [`HEAT_LID_REACH_KM`] (a supercontinent's heart is the hottest place
/// of the planet).
const HEAT_LID_KM: i64 = 2_000;
const HEAT_LID_REACH_KM: i64 = 4_000;
/// A sinking slab cools the mantle by this at its trench, fading to
/// nothing this far from it, km.
const HEAT_SLAB: i64 = 600;
const HEAT_SLAB_KM: i64 = 800;
/// The deep mantle's own warm and cold provinces, at most (they stay
/// where they are: plumes rise from the warm ones).
const HEAT_DEEP: i64 = 300;
const HEAT_DEEP_GRAIN: [(i64, i64); 2] = [(900_000, 2), (400_000, 1)];
/// Each step the mantle goes this much of the way to what the crust
/// above makes of it, per mille (heat lingers where a continent stood).
const HEAT_RATE: i64 = 150;
/// And spreads to its neighbours, this many times.
const HEAT_SWEEPS: usize = 2;
/// Plumes rise where the mantle is hottest, give or take this much heat
/// drawn for each cell, and stand at least this far apart, km.
const PLUME_LUCK: u64 = 2_000;
const PLUME_APART_KM: i64 = 1_800;
/// A slab younger than this, millions of years, is too light to pull.
pub const SLAB_YOUNG_MY: i64 = 10;
/// The most an old slab pulls, per mille of the pull at
/// [`Settings::slab_age_my`].
const SLAB_MOST: i64 = 1_500;
/// An escape's push keeps this much of its strength per step,
/// per mille.
const PUSH_KEEP: i64 = 960;
/// Oceanic plates' seeds stay this far apart and from the
/// supercontinent's plates' seeds, km.
const OCEAN_SEED_APART_KM: i64 = 3_000;
/// Draws tried for a point that must meet a condition.
const TRIES: u64 = 100_000;
/// Terranes and oceanic seeds stay this far off the supercontinent's
/// crust, in its cosine units (‰).
const TERRANE_OFF: i64 = 150;
/// Crust ages: the supercontinent's interior (cratons), and terranes, My
/// at the start.
const CRATON_MY: (i64, i64) = (700, 3_000);
const TERRANE_MY: (i64, i64) = (20, 250);
/// The ocean at the start: its youngest floor, and how fast it spread
/// from the plates' borders, km per million years on each side.
const OCEAN_START_MY: i64 = 5;
const OCEAN_START_KM_MY: i64 = 25;
/// The older sutures' last mountain building, My ago.
const OLD_OROGENY_MY: (i64, i64) = (300, 700);
/// A plate keeps this much of its last motion each step, per mille (the
/// rest comes from the forces of the step).
const INERTIA: i64 = 400;
/// Convergence or divergence a slab or a ridge needs, µm a year.
const FORCE_LEAST_UM: i64 = 2_000;
/// Share of an arc's riding edge that becomes arc crust, per mille.
const ARC_SHARE: u64 = 200;

type M3 = [[i64; 3]; 3];

const IDENTITY: M3 = [[ONE, 0, 0], [0, ONE, 0], [0, 0, ONE]];

fn apply(m: &M3, v: V3) -> V3 {
    let r = |row: &[i64; 3]| {
        ((0..3)
            .map(|k| i128::from(row[k]) * i128::from(v[k]))
            .sum::<i128>()
            / i128::from(ONE)) as i64
    };
    [r(&m[0]), r(&m[1]), r(&m[2])]
}

/// A turn of `angle` (millionths of a radian) about the unit `axis`
/// (Rodrigues): points move along `axis × p`.
fn rotation(axis: V3, angle: i64) -> M3 {
    let (c, s) = (cos(angle), sin(angle));
    let t = ONE - c;
    let k = axis;
    let kx = [[0, -k[2], k[1]], [k[2], 0, -k[0]], [-k[1], k[0], 0]];
    let mut m = [[0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let outer = (i128::from(t) * i128::from(k[i]) * i128::from(k[j])
                / i128::from(ONE)
                / i128::from(ONE)) as i64;
            m[i][j] = if i == j { c } else { 0 } + s * kx[i][j] / ONE + outer;
        }
    }
    m
}

/// A point of a piece's lattice placed on the planet by the piece's turn
/// (not normalised).
#[must_use]
pub fn placed(turn: &[[i64; 3]; 3], p: V3) -> V3 {
    apply(turn, p)
}

/// Where a point carried by a plate turning by `omega` (µm a year, see
/// [`Motion::omega`]) is after a step of `step_my` million years.
#[must_use]
pub fn carried(omega: V3, step_my: i64, p: V3) -> V3 {
    let speed = norm(omega);
    if speed < 50 {
        return p;
    }
    let turn = rotation(normalize(omega), speed * step_my * ONE / (1000 * RADIUS_KM));
    normalize(apply(&turn, p))
}

/// A piece of crust and its record, carried with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rock {
    class: u8,
    piece: u8,
    margins: u8,
    /// Steps it has ridden over a sinking ocean (an island arc's growth).
    arc: u8,
    /// The later rift it lies along (index + 1), 0 none.
    rift: u8,
    /// When it formed, millions of years ago.
    born_my: u16,
    orogeny: u16,
    home: u32,
    /// Its thickness, tenths of a km.
    thick: u16,
    /// Its height, m above the sea of the start, without its grain
    /// ([`History::rough`], [`History::broad`]) and without what a pile
    /// lifts ([`ISOSTASY_M`]). The crust carries it (version 7): only what
    /// happens to the crust changes it.
    high: i16,
    /// How far it has still to sink as it cools, m (a stretched margin).
    cool: i16,
    /// How far stretching has lowered it in all, m.
    sunk: i16,
    /// What changed its height since the step before, m, by cause (see
    /// [`CAUSES`]).
    why: [i16; CAUSES],
}

/// A bit of ocean floor where it really is. Each step it turns with its
/// plate, exactly, so a slow plate drifts as far as its speed says; the
/// grid only draws the bits (a cell shows one bit, a bit shows in one
/// cell at most). The floor is short-lived and under water: it stays on
/// the grid (version 10).
#[derive(Debug, Clone, Copy)]
struct Bit {
    pos: V3,
    plate: u8,
    rock: Rock,
    /// Whether a cell showed it at the step before.
    seen: bool,
    /// Its name, for good.
    id: u32,
}

/// Where a hotspot stood under the crust, carried with what it marked: a
/// bit of ocean floor, or (`land`) a cell of a continent's piece.
#[derive(Debug, Clone, Copy)]
struct Mark {
    land: bool,
    bit: u32,
    pos: V3,
    spot: u8,
    my: u16,
}

/// A cell of continental crust (version 10, the lead's 27 November: « de
/// vraies pièces rigides »). It is for good one cell (`at`) of its
/// piece's own lattice (`frame`): moving the piece changes the frame's
/// turn and nothing here, and nothing here is ever resampled. Only named
/// events change it: a collision's front, a rift, an arc, a docking,
/// wear, silt.
#[derive(Debug, Clone, Copy)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "four plain facts about a cell of crust, not a state machine"
)]
struct Crust {
    frame: u32,
    at: u32,
    plate: u8,
    /// The area it stands for, km² (a collision shortens it).
    area: u32,
    rock: Rock,
    /// Its name, for good.
    id: u32,
    /// What happened to it in the step (a [`Margin`]).
    event: u8,
    /// Whether it lies under another piece's crust.
    under: bool,
    /// Whether a rift had torn it before the step.
    rifted: bool,
    /// Why it changed plate in the step (a `SWITCH_*`).
    switch: u8,
    /// Whether the planet's grid drew it at the step before.
    shown: bool,
    dead: bool,
    /// Where it is on the planet now and the grid's cell there, read from
    /// its frame's turn at each step.
    pos: V3,
    cell: u32,
}

/// `a` after `b`.
fn times(a: &M3, b: &M3) -> M3 {
    let mut m = [[0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = ((0..3)
                .map(|k| i128::from(a[i][k]) * i128::from(b[k][j]))
                .sum::<i128>()
                / i128::from(ONE)) as i64;
        }
    }
    m
}

/// The turn back.
fn back_of(m: &M3) -> M3 {
    std::array::from_fn(|i| std::array::from_fn(|j| m[j][i]))
}

/// A turn made a turn again after a product: its rows of length one and
/// square to each other (the rounding of forty products does not add up).
fn steady(m: &M3) -> M3 {
    let a = normalize(m[0]);
    let b = normalize(sub(m[1], scale(a, dot(m[1], a))));
    [a, b, normalize(cross(a, b))]
}

/// A plain mixing of a lattice cell's key (the std hasher is slow for
/// millions of small lookups; the map is only ever looked up, never
/// walked, so the order of its keys decides nothing).
#[derive(Default)]
struct Mix(u64);

impl std::hash::Hasher for Mix {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(u64::from(b));
        }
    }

    fn write_u64(&mut self, v: u64) {
        let x = (self.0 ^ v).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        self.0 = x ^ (x >> 32);
    }
}

type Slots = std::collections::HashMap<u64, u32, std::hash::BuildHasherDefault<Mix>>;

fn slot_key(frame: u32, at: u32) -> u64 {
    (u64::from(frame) << 32) | u64::from(at)
}

/// The precedence of a cell of crust where several lie over a point: its
/// class, its plate's weight.
type Rank = (i64, i64, i64);

/// The continents (version 10): cells of crust in the frames of their
/// pieces. A frame is the grid's own lattice with a turn that places it
/// on the planet today; the cells of a frame on plates that turn alike
/// are one rigid piece, and a frame whose cells come to turn differently
/// (a rift, a block escaping) splits into frames that keep the same
/// lattice, so the two shores of the new ocean fit for good. Pieces
/// welded together keep their frames and turn together.
struct Land {
    /// Each frame's turn, and the turn back.
    rot: Vec<M3>,
    back: Vec<M3>,
    cells: Vec<Crust>,
    /// The cell of crust at each held place of each frame's lattice.
    slot: Slots,
    /// For neighbourhood queries: the frames with crust in or around
    /// each cell of the planet's grid.
    near: Vec<Vec<u32>>,
}

impl Land {
    fn new() -> Self {
        Self {
            rot: Vec::new(),
            back: Vec::new(),
            cells: Vec::new(),
            slot: Slots::default(),
            near: vec![Vec::new(); CELLS],
        }
    }

    /// A new frame, turned by `rot`.
    fn frame(&mut self, rot: M3) -> u32 {
        self.rot.push(rot);
        self.back.push(back_of(&rot));
        self.rot.len() as u32 - 1
    }

    /// The cell of crust at a place of a frame's lattice.
    fn get(&self, frame: u32, at: u32) -> Option<usize> {
        self.slot.get(&slot_key(frame, at)).map(|&i| i as usize)
    }

    /// Where a cell of crust is on the planet now.
    fn place(&mut self, sphere: &Sphere, i: usize) {
        let c = &mut self.cells[i];
        c.pos = normalize(apply(
            &self.rot[c.frame as usize],
            sphere.centre[c.at as usize],
        ));
        c.cell = sphere.cell_of(c.pos) as u32;
        let (frame, q) = (c.frame, c.cell as usize);
        if !self.near[q].contains(&frame) {
            self.near[q].push(frame);
        }
        for &(m, _) in &sphere.links[q] {
            let list = &mut self.near[m as usize];
            if !list.contains(&frame) {
                list.push(frame);
            }
        }
    }

    /// New crust at a free place of a frame's lattice.
    fn add(
        &mut self,
        sphere: &Sphere,
        (frame, at): (u32, u32),
        plate: u8,
        rock: Rock,
        id: u32,
    ) -> usize {
        let i = self.cells.len();
        self.cells.push(Crust {
            frame,
            at,
            plate,
            area: sphere.area_km2[at as usize],
            rock,
            id,
            event: Margin::Inland as u8,
            under: false,
            rifted: false,
            switch: 0,
            shown: false,
            dead: false,
            pos: [0; 3],
            cell: 0,
        });
        self.slot.insert(slot_key(frame, at), i as u32);
        self.place(sphere, i);
        i
    }

    /// A cell of crust gone (run under another and piled on it).
    fn kill(&mut self, i: usize) {
        let c = &mut self.cells[i];
        c.dead = true;
        self.slot.remove(&slot_key(c.frame, c.at));
    }

    /// The neighbours of a cell of crust in its own lattice, and the
    /// distance to them, km: the crust it has always touched.
    fn kin<'a>(&'a self, sphere: &'a Sphere, i: usize) -> impl Iterator<Item = (usize, u32)> + 'a {
        let c = &self.cells[i];
        sphere.links[c.at as usize]
            .iter()
            .filter_map(move |&(n, km)| self.get(c.frame, n).map(|j| (j, km)))
    }

    /// The crust over the point `y` of the grid's cell `q`, the first in
    /// precedence, leaving out the frame `but`: the lattice cell of each
    /// frame around that holds the point, exactly.
    fn cover(
        &self,
        sphere: &Sphere,
        (y, q): (V3, usize),
        rank: &dyn Fn(u8, u8) -> Rank,
        but: u32,
    ) -> Option<usize> {
        let mut best: Option<(Rank, std::cmp::Reverse<u32>, usize)> = None;
        for &f in &self.near[q] {
            if f == but {
                continue;
            }
            let at = sphere.cell_of(normalize(apply(&self.back[f as usize], y)));
            if let Some(j) = self.get(f, at as u32) {
                let c = &self.cells[j];
                let k = (rank(c.plate, c.rock.class), std::cmp::Reverse(f), j);
                if best.is_none_or(|b| k > b) {
                    best = Some(k);
                }
            }
        }
        best.map(|b| b.2)
    }

    /// The plates turn, each frame with its cells' plate. A frame whose
    /// cells turn differently splits: each part keeps the lattice and
    /// the turn so far, and takes its own turn from now on. No cell
    /// changes.
    fn turn(&mut self, sphere: &Sphere, turn: &[M3]) {
        let same: Vec<usize> = (0..turn.len())
            .map(|p| (0..=p).find(|&q| turn[q] == turn[p]).expect("itself"))
            .collect();
        let old = self.rot.len();
        let mut keeps = vec![usize::MAX; old];
        let mut forks: Vec<(u32, usize, u32)> = Vec::new();
        for i in 0..self.cells.len() {
            let c = self.cells[i];
            let (f, k) = (c.frame as usize, same[c.plate as usize]);
            if keeps[f] == usize::MAX {
                keeps[f] = k;
            }
            if keeps[f] == k {
                continue;
            }
            let to = if let Some(x) = forks.iter().find(|x| x.0 == c.frame && x.1 == k) {
                x.2
            } else {
                let to = self.frame(self.rot[f]);
                forks.push((c.frame, k, to));
                to
            };
            self.slot.remove(&slot_key(c.frame, c.at));
            self.slot.insert(slot_key(to, c.at), i as u32);
            self.cells[i].frame = to;
        }
        let turned = (0..old)
            .map(|f| (f, keeps[f]))
            .chain(forks.iter().map(|x| (x.2 as usize, x.1)));
        for (f, k) in turned {
            if k != usize::MAX && turn[k] != IDENTITY {
                self.rot[f] = steady(&times(&turn[k], &self.rot[f]));
                self.back[f] = back_of(&self.rot[f]);
            }
        }
        for list in &mut self.near {
            list.clear();
        }
        for i in 0..self.cells.len() {
            self.place(sphere, i);
        }
    }

    /// The cells gone leave the record: each cell's new index.
    fn compact(&mut self) -> Vec<u32> {
        let mut new_ix = vec![NO_BIT; self.cells.len()];
        let mut kept = Vec::with_capacity(self.cells.len());
        for (i, c) in self.cells.iter().enumerate() {
            if !c.dead {
                new_ix[i] = kept.len() as u32;
                kept.push(*c);
            }
        }
        self.cells = kept;
        self.slot.clear();
        for (i, c) in self.cells.iter().enumerate() {
            self.slot.insert(slot_key(c.frame, c.at), i as u32);
        }
        new_ix
    }

    /// A many-source shortest walk through the crust, from cell to cell
    /// of a piece's own lattice, up to `reach` km, only into cells that
    /// `pass`: each cell's distance, km.
    fn spread(
        &self,
        sphere: &Sphere,
        sources: &[usize],
        pass: impl Fn(usize) -> bool,
        reach: u32,
    ) -> Vec<u32> {
        let mut dist = vec![u32::MAX; self.cells.len()];
        let mut heap = std::collections::BinaryHeap::new();
        for &i in sources {
            if dist[i] == u32::MAX {
                dist[i] = 0;
                heap.push(std::cmp::Reverse((0_u32, i as u32)));
            }
        }
        while let Some(std::cmp::Reverse((d, i))) = heap.pop() {
            let i = i as usize;
            if d > dist[i] {
                continue;
            }
            for (j, km) in self.kin(sphere, i) {
                let nd = d + km;
                if pass(j) && nd <= reach && nd < dist[j] {
                    dist[j] = nd;
                    heap.push(std::cmp::Reverse((nd, j as u32)));
                }
            }
        }
        dist
    }

    /// The other plates whose crust a cell touches: its neighbours in its
    /// own lattice, and the other pieces the grid draws around it (a
    /// neighbourhood query: who touches whom).
    fn facing(&self, sphere: &Sphere, top: &[u32], i: usize) -> Vec<u8> {
        let c = &self.cells[i];
        let mut out: Vec<u8> = Vec::new();
        for (j, _) in self.kin(sphere, i) {
            let p = self.cells[j].plate;
            if p != c.plate && !out.contains(&p) {
                out.push(p);
            }
        }
        let q = c.cell as usize;
        for m in std::iter::once(q).chain(sphere.links[q].iter().map(|&(m, _)| m as usize)) {
            if top[m] == NO_BIT {
                continue;
            }
            let o = &self.cells[top[m] as usize];
            if o.frame != c.frame && o.plate != c.plate && !out.contains(&o.plate) {
                out.push(o.plate);
            }
        }
        out
    }

    /// The pieces as the history keeps them at a step (`floor`: each
    /// plate's mean floor birth, for the precedence).
    fn pieces(&self, group: &[u8; MAXP], floor: &[i64; MAXP]) -> Vec<Piece> {
        let weight = self.weight();
        let mut index = vec![usize::MAX; self.rot.len()];
        let mut out: Vec<Piece> = Vec::new();
        for c in &self.cells {
            let f = c.frame as usize;
            if index[f] == usize::MAX {
                index[f] = out.len();
                out.push(Piece {
                    frame: c.frame,
                    turn: self.rot[f],
                    tiles: Vec::new(),
                });
            }
            out[index[f]].tiles.push(Tile {
                at: c.at,
                id: c.id,
                plate: group[c.plate as usize],
                own: c.plate,
                switch: c.switch,
                class: c.rock.class,
                under: c.under,
                margins: c.rock.margins,
                thick: c.rock.thick,
                high: c.rock.high,
                home: c.rock.home,
                area_km2: c.area,
                why: c.rock.why,
                rank: precedence(&weight, floor, c.plate, c.rock.class),
            });
        }
        out
    }

    /// Each plate's continental crust, km² (what no other piece covers).
    fn weight(&self) -> [i64; MAXP] {
        let mut out = [0_i64; MAXP];
        for c in &self.cells {
            if !c.dead && !c.under {
                out[c.plate as usize] += i64::from(c.area);
            }
        }
        out
    }
}

/// The grid's view of the continents: each cell shows the crust over its
/// centre. A view only: every change is made in the pieces.
fn paint(land: &Land, top: &[u32], plate: &mut [u8], rock: &mut [Rock]) {
    for q in 0..CELLS {
        if top[q] != NO_BIT {
            let c = &land.cells[top[q] as usize];
            plate[q] = c.plate;
            rock[q] = c.rock;
        }
    }
}

/// Where the plates' bookkeeping finds each cell of crust on the grid: a
/// cell of the grid that draws it, else the cell it lies in.
fn seats(land: &Land, top: &[u32]) -> Vec<u32> {
    let mut seat: Vec<u32> = land.cells.iter().map(|c| c.cell).collect();
    for q in (0..CELLS).rev() {
        if top[q] != NO_BIT {
            seat[top[q] as usize] = q as u32;
        }
    }
    seat
}

/// The precedence of crust of a class on a plate: continental crust never
/// sinks, the heavier continent rides; of two ocean floors the younger.
fn precedence(cont_of: &[i64; MAXP], floor: &[i64; MAXP], p: u8, class: u8) -> Rank {
    let q = i64::from(p);
    match class {
        CONTINENT => (3, cont_of[p as usize], -q),
        TERRANE => (2, cont_of[p as usize], -q),
        _ => (1, -floor[p as usize], -q),
    }
}

/// A push on two plates, an escaping block and
/// the plate it leaves: away from a great circle (`normal`), growing
/// with the distance from it up to `expansion_km`, within `reach_km` of
/// `centre`.
#[derive(Debug, Clone, Copy)]
struct Push {
    centre: V3,
    normal: V3,
    reach_km: i64,
    expansion_km: i64,
    /// µm a year.
    strength: i64,
    plates: (u8, u8),
}

/// A cell where a force acts on its plate, the force's direction and its
/// weight, per mille.
type Spot = (usize, u8, V3, i64);

/// A cell on a collision's front: its plate, the plate it meets, the
/// normal toward it, and how fast they close, µm a year.
type Front = (usize, u8, u8, V3, i64);

/// No bit shown.
const NO_BIT: u32 = u32::MAX;
/// Crust thickness, tenths of a km: continents, the ocean floor,
/// terranes and arcs; the most a pile reaches.
pub const CONTINENT_THICK: u16 = 350;
const OCEAN_THICK: u16 = 70;
const TERRANE_THICK: u16 = 250;
const THICK_MAX: i64 = 700;
/// Two plates in a collision and its front's length, cells.
type Grip = (u8, u8, i64);

/// Each step, a pile spreads this much toward its neighbours, ‰ (slowly:
/// a range stays a range, version 6).
const THICK_SPREAD: i64 = 120;
/// Each step a continent loses [`Settings::mountain_wear`] of its
/// thickness above a continent's (wear); the silt builds its plate's
/// margins out, a shore cell at a time once there is enough for
/// [`SPILL_THICK`] there (no continental crust is ever lost, 27 November,
/// version 5.1).
const SPILL_THICK: i64 = 200;
/// Two colliding plates weld once they have stood still this many steps.
const WELD_QUIET: usize = 4;
/// The most of two colliding plates' relative motion a front takes away
/// in a step, ‰.
const GRIP_MOST: i64 = 900;
/// A cell left empty by the grid's sampling takes a bit of its plate not
/// shown nearby, within this, km (first its neighbours, then two rings).
const HOLE_KM: [i64; 2] = [120, 190];
/// A cell still empty with at least this many neighbours shown, all of
/// one plate, is a hole of the sampling.
const HOLE_AROUND: usize = 6;
/// Such a cell between two plates is a gap (new ocean floor) only if they
/// part there at least this fast, µm a year; else it is a hole of the
/// sampling too: no floor is born in a trench or in a range.
const GAP_LEAST_UM: i64 = 1_000;
/// A part of a plate cut off from the rest is taken by the plate it
/// touches most if it is smaller than this, km²; larger, it is a plate of
/// its own. A plate is one piece.
const PART_KM2: i64 = 2_000_000;
/// A plate smaller than this, km², is a crumb the grid cannot carry (a
/// dozen cells): the plate it touches most takes it.
const CRUMB_KM2: i64 = 100_000;
/// A plate enclosed in one other plate and smaller than this, km², is
/// taken by it: a small plate lives wedged between plates, at a trench or
/// a ridge, not inside one.
const ENCLOSED_KM2: i64 = 5_000_000;
/// An escape: after this many steps of collision still closing at least
/// this fast (µm a year) on this many front cells, a block of the riding
/// plate this wide (km) is pushed out sideways at this speed (mm a year).
const ESCAPE_AFTER: usize = 3;
const ESCAPE_MIN_UM: i64 = 20_000;
const ESCAPE_CELLS: i64 = 40;
/// Only a riding plate carrying this much continent sheds a block, km².
const ESCAPE_FROM_KM2: i64 = 4_000_000;
const ESCAPE_KM: i64 = 900;
const ESCAPE_MM: i64 = 25;
/// Flood basalts: a plume this near a boundary that opens floods the
/// continent this far around it, km.
const LIP_REACH_KM: i64 = 800;
const LIP_KM: i64 = 400;
/// A rift (version 11). Along a line across a plate, the forces on the
/// plate's two sides would part them at some speed if nothing held them
/// (the same balance that turns the plates). The lithosphere holds: it
/// lets through only a share of that speed, its weakness, ‰: little
/// where it is cold and whole, more over a warm mantle (up to
/// [`WEAK_WARM`] at [`WEAK_HOT`]), more along an old suture or a failed
/// rift. As the crust thins the line weakens (necking): the share goes
/// to the whole as the square of the stretching done.
const WEAK_COLD: i64 = 10;
const WEAK_WARM: i64 = 50;
const WEAK_HOT: i64 = 2_500;
const WEAK_SEAM: i64 = 40;
/// The stretched zone's width at the start, km (a narrow rift), and the
/// crust's thickness at which it gives way, tenths of a km: the plate is
/// cut there and the ocean opens.
const RIFT_WIDTH_KM: i64 = 200;
const BREAK_THICK: u16 = 150;
/// A trough exists once its line stretches this fast, µm a year; below
/// it for this many steps it is abandoned (a failed rift).
const RIFT_LEAST_UM: i64 = 1_000;
const RIFT_IDLE: usize = 4;
/// Each tip of a trough grows as fast as the trough stretches: this
/// many km for each m of opening (the East African rift: ~4 mm a year
/// and ~60 km a million years; once the crust necks the stretching
/// races, and the tips with it: the rift unzips). It grows toward the
/// weakest crust ahead: within a cone about the line across the pull, a
/// km off the line costing [`RIFT_OFF`] ‰ of weakness per hundred km.
const RIFT_GROW: i64 = 10;
const RIFT_OFF: i64 = 10;
/// Where a plate may begin to tear: at the weakest of this many spots of
/// its continent, this far apart, km, along the best of twelve lines.
const RIFT_SPOTS: usize = 6;
const RIFT_SPOT_APART_KM: i64 = 800;

/// Height a km of extra crust raises a continent, m (Airy isostasy:
/// crust 2.8, mantle 3.3).
pub const ISOSTASY_M: i64 = 100;
/// The starting supercontinent's relief, m: its shore, how high its
/// heart stands above it and how far inland it gets there (km); an old
/// range along an old suture and an old failed rift's trough, with their
/// half-widths (km).
const SHORE_M: i64 = -300;
const HEART_M: i64 = 1_800;
const HEART_KM: i64 = 2_000;
const OLD_RANGE: (i64, i64) = (450, 220);
const OLD_TROUGH: (i64, i64) = (-650, 110);
/// A rift stretches and thins the crust this far from its line, km: it
/// sinks at once by this much at the line, m, and as much again as it
/// cools, [`COOLING`] ‰ of what is left each step (a passive margin's
/// shelf).
const STRETCH_KM: u32 = 230;
const STRETCH_M: i64 = 400;
const COOLING: i64 = 100;
/// An arc builds volcanoes on the plate riding over a sinking floor:
/// toward this height, m, at their line ([`ARC_LINE_KM`] from the coast,
/// this half-width) and on the coast range, by [`ARC_RATE`] ‰ of what is
/// missing each step, as far as [`ARC_KM`].
const ARC_TOP_M: i64 = 2_200;
const ARC_COAST_M: i64 = 700;
const ARC_RATE: i64 = 100;
const ARC_LINE_KM: (i64, i64) = (180, 150);
const ARC_KM: u32 = 340;
/// A terrane docking raises the crust it meets, m; an island arc is born
/// this high; a plume lifts the continent over it, m a step.
const DOCK_M: i64 = 500;
const ARC_ISLAND_M: i16 = 200;
const PLUME_UP_M: i64 = 60;
/// Wear: each step the height above a plain's (m) loses this much (‰,
/// slowly: an old range is still a range), slopes level by this much
/// (‰), and the sediment fills its plate's low ground, at most this share
/// (‰) of what lies below a shelf's depth (m).
const WEAR: i64 = 20;
const PLAIN_M: i64 = 300;
const LEVELLING: i64 = 30;
const FILLING: i64 = 300;
const SHELF_M: i64 = -150;

/// A carried height changed by `by` m.
fn lift(high: i16, by: i64) -> i16 {
    (i64::from(high) + by).clamp(-6_000, 8_000) as i16
}

/// The causes that change a crust's carried height, in the order of
/// [`History::cause_at`]: a rift stretched it, it cooled, an arc
/// built on it, a terrane docked, a plume lifted it, it wore down, it
/// levelled with its neighbours, sediment filled it.
pub const CAUSES: usize = 8;
const BY_STRETCH: usize = 0;
const BY_COOLING: usize = 1;
const BY_ARC: usize = 2;
const BY_DOCK: usize = 3;
const BY_PLUME: usize = 4;
const BY_WEAR: usize = 5;
const BY_LEVEL: usize = 6;
const BY_FILL: usize = 7;

impl Rock {
    /// Its carried height changes by `by` m, for a cause.
    fn rise(&mut self, by: i64, cause: usize) {
        let was = self.high;
        self.high = lift(was, by);
        self.why[cause] = lift(self.why[cause], i64::from(self.high) - i64::from(was));
    }
}

/// What happens to the continents' relief in a step, apart from the
/// collisions' piles (version 7): arcs build (volcanoes on the plates
/// riding over a sinking floor, behind their active margins), stretched
/// margins cool and sink, high ground wears down and its sediment fills
/// the low ground. All of it in the pieces' own lattices (version 10):
/// `margin` is each cell of crust's margin.
fn weather(sphere: &Sphere, land: &mut Land, margin: &[u8], grain: &[i16], sea: i64) {
    let active: Vec<usize> = (0..land.cells.len())
        .filter(|&i| margin[i] == Margin::Active as u8)
        .collect();
    let d = land.spread(sphere, &active, |_| true, ARC_KM);
    for (i, c) in land.cells.iter_mut().enumerate() {
        let r = &mut c.rock;
        if d[i] != u32::MAX {
            let d = i64::from(d[i]);
            let top = ARC_TOP_M * bump(d - ARC_LINE_KM.0, ARC_LINE_KM.1) / 1000
                + ARC_COAST_M * bump(d, 90) / 1000;
            let up = (top - i64::from(r.high)).max(0) * ARC_RATE / 1000;
            r.rise(up, BY_ARC);
        }
        if r.cool > 0 {
            let sunk = (i64::from(r.cool) * COOLING / 1000).max(1);
            r.rise(-sunk, BY_COOLING);
            r.cool -= sunk as i16;
        }
    }
    let high: Vec<i64> = land.cells.iter().map(|c| i64::from(c.rock.high)).collect();
    let flow: Vec<i64> = (0..land.cells.len())
        .map(|i| {
            land.kin(sphere, i)
                .map(|(j, _)| (high[j] - high[i]) * LEVELLING / 1000 / 8)
                .sum()
        })
        .collect();
    let mut silt = [0_i64; MAXP];
    let mut room = [0_i64; MAXP];
    let below = |r: &Rock| {
        sea + SHELF_M
            - i64::from(r.high)
            - i64::from(grain[r.home as usize])
            - (i64::from(r.thick) - i64::from(CONTINENT_THICK)).max(0) * ISOSTASY_M / 10
    };
    for (i, c) in land.cells.iter_mut().enumerate() {
        // Wear stops at its base level, the sea: ground is not worn down
        // under it (version 11).
        let above = high[i] - sea
            + i64::from(grain[c.rock.home as usize])
            + (i64::from(c.rock.thick) - i64::from(CONTINENT_THICK)).max(0) * ISOSTASY_M / 10;
        let worn = ((high[i] - PLAIN_M) * WEAR / 1000).clamp(0, above.max(0));
        let area = i64::from(c.area);
        c.rock.rise(flow[i], BY_LEVEL);
        c.rock.rise(-worn, BY_WEAR);
        silt[c.plate as usize] += worn * area;
        room[c.plate as usize] += below(&c.rock).max(0) * area;
    }
    for c in &mut land.cells {
        let p = c.plate as usize;
        let deep = below(&c.rock);
        if deep <= 0 || silt[p] == 0 {
            continue;
        }
        let fill = deep * silt[p].min(room[p]) / room[p].max(1) * FILLING / 1000;
        c.rock.rise(fill, BY_FILL);
    }
}

/// The part of `v` tangent to the sphere at `x`.
fn tangent(x: V3, v: V3) -> V3 {
    sub(v, scale(x, dot(v, x)))
}

/// The turn that moves the point `at` along the unit `dir` at `um` µm a
/// year (a vector along its pole).
fn lateral(at: V3, dir: V3, um: i64) -> V3 {
    scale(normalize(cross(at, dir)), um)
}

/// The speed a plate reaches against a mantle whose drag grows with
/// speed: `u` the speed the forces alone would give against a plain drag,
/// `v0` the speed at which the drag has doubled, both µm a year.
fn dragged(u: i64, v0: i64) -> i64 {
    if v0 <= 0 || u <= 0 {
        return u.max(0);
    }
    let root = (i128::from(v0) * i128::from(v0) + 4 * i128::from(u) * i128::from(v0))
        .cast_unsigned()
        .isqrt() as i64;
    (root - v0) / 2
}

/// Each plate's mean ocean floor birth, millions of years ago (older
/// floor is denser: of two oceanic plates, the older one sinks).
fn floor_born(plate: &[u8], rock: &[Rock]) -> [i64; MAXP] {
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

fn alias_of(alias: &[u8; MAXP], mut p: u8) -> u8 {
    while alias[p as usize] != p {
        p = alias[p as usize];
    }
    p
}

fn root(up: &[u8; MAXP], mut p: u8) -> u8 {
    while up[p as usize] != p {
        p = up[p as usize];
    }
    p
}

/// The plates' turns at a step, and which plates move as one.
struct Drive {
    /// Each plate's turn, µm a year about its pole (a vector along it).
    omega: Vec<V3>,
    /// Each plate's group, named by its least id: plates whose continents
    /// have not parted move as one while the forces press them together.
    group: [u8; MAXP],
}

/// The forces at the plates' edges, read at the last step: the slabs
/// (on the plate that sinks), their suction (on the plate riding over
/// them), the ridges, and the collisions' fronts.
#[derive(Clone, Copy)]
struct Edges<'a> {
    slab: &'a [Spot],
    suction: &'a [Spot],
    ridge: &'a [Spot],
    grips: &'a [Grip],
}

/// The mantle's heat a step later: it goes part of the way to what the
/// crust above makes of it (warm under a continent, the more the farther
/// from the sea: a lid; cold around the slabs that sink; the deep
/// mantle's own provinces), and spreads.
fn warm(sphere: &Sphere, rock: &[Rock], slab: &[Spot], deep: &[i32], heat: &mut [i32], rate: i64) {
    let sea: Vec<(usize, u32)> = (0..CELLS)
        .filter(|&c| rock[c].class == OCEAN)
        .map(|c| (c, 0))
        .collect();
    let (inland, _) = sphere.spread(
        &sea,
        |_, b| rock[b].class != OCEAN,
        HEAT_LID_REACH_KM as u32,
    );
    let trench: Vec<(usize, u32)> = slab.iter().map(|s| (s.0, 0)).collect();
    let (sinking, _) = sphere.spread(&trench, |_, _| true, HEAT_SLAB_KM as u32);
    for c in 0..CELLS {
        let mut goal = i64::from(deep[c]);
        if rock[c].class != OCEAN {
            let d = i64::from(inland[c]).min(HEAT_LID_REACH_KM);
            goal += d * 1000 / HEAT_LID_KM;
        }
        if sinking[c] != u32::MAX {
            goal -= HEAT_SLAB * (HEAT_SLAB_KM - i64::from(sinking[c])) / HEAT_SLAB_KM;
        }
        let now = i64::from(heat[c]);
        heat[c] = (now + (goal - now) * rate / 1000) as i32;
    }
    for _ in 0..HEAT_SWEEPS {
        let before = heat.to_vec();
        for c in 0..CELLS {
            let around: i64 = sphere.links[c]
                .iter()
                .map(|&(n, _)| i64::from(before[n as usize]))
                .sum();
            let mean = around / sphere.links[c].len().max(1) as i64;
            heat[c] = i64::midpoint(i64::from(before[c]), mean) as i32;
        }
    }
}

/// The plates' turns from the forces on them. Plates whose continents
/// touch and have not parted (`held`) are held together while the forces
/// press them together: a boundary inside a continent exists only once
/// it opens (the supercontinent breaks only by opening).
fn drive(
    sphere: &Sphere,
    s: &Settings,
    plate: &[u8],
    rock: &[Rock],
    pushes: &[Push],
    alias: &[u8; MAXP],
    heat: &[i32],
    edges: Edges,
    prev: Option<&[V3]>,
    held: &[[bool; MAXP]],
) -> Drive {
    let mut torque = vec![[0_i64; 3]; MAXP];
    let mut cells = [0_i64; MAXP];
    let mut cont = [0_i64; MAXP];
    let (cell_tq, edge_tq) = torques(sphere, s, plate, rock, pushes, alias, heat, edges);
    for c in 0..CELLS {
        let p = plate[c] as usize;
        cells[p] += 1;
        if rock[c].class != OCEAN {
            cont[p] += 1;
        }
        torque[p] = add(torque[p], cell_tq[c]);
    }
    for &(_, p, tq) in &edge_tq {
        torque[p as usize] = add(torque[p as usize], tq);
    }
    drive_from(
        sphere,
        s,
        plate,
        rock,
        edges,
        prev,
        held,
        (torque, cells, cont),
    )
}

/// The forces on the plates, as torques: each cell's, on its own plate
/// (an escape's push, the mantle's heat under a continent),
/// and the edges' (a cell, the plate it acts on, the torque): slabs,
/// their suction, ridges.
#[allow(
    clippy::too_many_arguments,
    reason = "the forces' inputs, each one named"
)]
fn torques(
    sphere: &Sphere,
    s: &Settings,
    plate: &[u8],
    rock: &[Rock],
    pushes: &[Push],
    alias: &[u8; MAXP],
    heat: &[i32],
    edges: Edges,
) -> (Vec<V3>, Vec<(u32, u8, V3)>) {
    let mut cell_tq = vec![[0_i64; 3]; CELLS];
    let mut edge_tq = Vec::new();
    let pushes: Vec<Push> = pushes
        .iter()
        .filter(|p| p.strength > 0)
        .filter_map(|p| {
            let (a, b) = (alias_of(alias, p.plates.0), alias_of(alias, p.plates.1));
            (a != b).then_some(Push {
                plates: (a, b),
                ..*p
            })
        })
        .collect();
    for c in 0..CELLS {
        let p = plate[c];
        let x = sphere.centre[c];
        let mut f = [0_i64; 3];
        for push in &pushes {
            if p != push.plates.0 && p != push.plates.1 {
                continue;
            }
            let d = km_between(x, push.centre);
            if d >= push.reach_km {
                continue;
            }
            let side = dot(x, push.normal);
            let away = (side * RADIUS_KM / ONE * 1000 / push.expansion_km).clamp(-1000, 1000);
            let fade = ((push.reach_km - d) * 2000 / push.reach_km).min(1000);
            let dir = normalize(tangent(x, push.normal));
            f = add(f, scale(dir, push.strength * away / 1000 * fade / 1000));
        }
        if rock[c].class != OCEAN && s.pole_flight != 0 {
            let z = x[2];
            let across = (ONE * ONE - z * z).max(0).isqrt();
            let m = s.pole_flight * 1000 * 2 * z / ONE * across / ONE;
            let dir = normalize(tangent(x, [0, 0, -ONE]));
            f = add(f, scale(dir, m));
        }
        // The mantle carries a continent down the slope of its heat: away
        // from where it is warm (under a large continent), toward where
        // it is cold (where slabs sink).
        if rock[c].class != OCEAN && s.heat_push != 0 {
            let mut slope = [0_i64; 3];
            for &(n, km) in &sphere.links[c] {
                let d = i64::from(heat[n as usize]) - i64::from(heat[c]);
                if d != 0 {
                    let dir = normalize(tangent(x, sub(sphere.centre[n as usize], x)));
                    slope = add(slope, scale(dir, d * 1000 / i64::from(km).max(1)));
                }
            }
            let n = sphere.links[c].len() as i64 * 1000;
            f = add(f, slope.map(|v| -v * 2 * HEAT_LID_KM * s.heat_push / n));
        }
        cell_tq[c] = cross(x, f);
    }
    for (spots, strength) in [
        (edges.slab, s.slab_pull),
        (edges.suction, s.slab_suction),
        (edges.ridge, s.ridge_push),
    ] {
        for &(c, p, dir, weight) in spots {
            let f = scale(dir, strength * weight);
            edge_tq.push((c as u32, p, cross(sphere.centre[c], f)));
        }
    }
    (cell_tq, edge_tq)
}

/// A load on a plate: where, its torque, the drag it adds, and whether
/// it is a cell of continent.
type Load = (V3, V3, i64, bool);

/// How fast the two sides of a line across a plate would part if nothing
/// held them together, µm a year (negative: pressed together): each
/// side turns as the forces on it alone would turn it, against the
/// mantle's drag, as a plate does. The line is the great circle through
/// `x` whose pole is `n`. None where a side holds no continent beyond
/// the stretched zone: a rift tears a continent, not its shore.
fn parting(loads: &[Load], x: V3, n: V3, v0: i64) -> Option<i64> {
    let band = sin(i64::from(STRETCH_KM) * ONE / RADIUS_KM);
    let mut tq = [[0_i64; 3]; 2];
    let mut rs = [0_i64; 2];
    let mut far = [false; 2];
    for &(at, t, r, cont) in loads {
        let side = dot(at, n);
        let k = usize::from(side > 0);
        tq[k] = add(tq[k], t);
        rs[k] += r;
        far[k] |= cont && side.abs() > band;
    }
    if !far[0] || !far[1] {
        return None;
    }
    let w = [0, 1].map(|k| {
        let w = tq[k].map(|v| v * 1000 / rs[k].max(1));
        let u = norm(w);
        if u > 0 {
            w.map(|v| v * dragged(u, v0) / u)
        } else {
            w
        }
    });
    Some(dot(sub(cross(w[1], x), cross(w[0], x)), n))
}

/// How weak the lithosphere is under a cell of crust, ‰ (see
/// [`WEAK_COLD`]).
fn weak(heat: i32, r: &Rock) -> i64 {
    let warm = i64::from(heat).clamp(0, WEAK_HOT) * WEAK_WARM / WEAK_HOT;
    let seam = if r.margins & (SUTURE | OLD_SUTURE | FAILED_RIFT) != 0 {
        WEAK_SEAM
    } else {
        0
    };
    WEAK_COLD + warm + seam
}

/// A rift in progress (version 11): a trough along cells of crust of one
/// plate. It is carried with its plate; each step the pull across it
/// stretches and thins the crust along it, and its tips grow. Once it
/// has crossed its continent and its crust has given way, the plate is
/// cut along it. A trough the pull has left is abandoned: it stays.
struct Trough {
    /// Its record, an index into the rifts.
    record: usize,
    plate: u8,
    /// The cells of crust along it, in order from one tip to the other.
    path: Vec<u32>,
    /// The pole of the great circle it follows: the pull's direction.
    normal: V3,
    /// How far it has opened, m.
    ext_m: i64,
    /// Whether each tip has reached its continent's edge.
    done: [bool; 2],
    /// Steps without stretching.
    idle: usize,
    failed: bool,
}

/// The plates' turns from the torques on them (see [`drive`]).
fn drive_from(
    sphere: &Sphere,
    s: &Settings,
    plate: &[u8],
    rock: &[Rock],
    edges: Edges,
    prev: Option<&[V3]>,
    held: &[[bool; MAXP]],
    (torque, cells, cont): (Vec<V3>, [i64; MAXP], [i64; MAXP]),
) -> Drive {
    let resist: Vec<i64> = (0..MAXP)
        .map(|p| cells[p] * 1000 + s.continent_drag * cont[p])
        .collect();
    let mut samples: Vec<(usize, usize)> = Vec::new();
    for c in 0..CELLS {
        if rock[c].class != CONTINENT {
            continue;
        }
        let a = plate[c] as usize;
        for &(n, _) in &sphere.links[c] {
            let n = n as usize;
            let b = plate[n] as usize;
            if b != a && rock[n].class == CONTINENT && held[a][b] {
                samples.push((c, n));
            }
        }
    }
    let mut up: [u8; MAXP] = std::array::from_fn(|k| k as u8);
    let mut w = vec![[0_i64; 3]; MAXP];
    loop {
        let mut tq = vec![[0_i64; 3]; MAXP];
        let mut rs = [0_i64; MAXP];
        for p in 0..MAXP {
            let r = root(&up, p as u8) as usize;
            tq[r] = add(tq[r], torque[p]);
            rs[r] += resist[p];
        }
        for r in 0..MAXP {
            w[r] = if rs[r] > 0 {
                tq[r].map(|v| v * 1000 / rs[r])
            } else {
                [0; 3]
            };
        }
        let mut close = vec![0_i64; MAXP * MAXP];
        for &(c, n) in &samples {
            let (ra, rb) = (root(&up, plate[c]) as usize, root(&up, plate[n]) as usize);
            if ra == rb {
                continue;
            }
            let x = sphere.centre[c];
            let rel = sub(cross(w[ra], x), cross(w[rb], x));
            close[ra * MAXP + rb] += dot(rel, normalize(tangent(x, sub(sphere.centre[n], x))));
        }
        let mut merged = false;
        for a in 0..MAXP {
            for b in a + 1..MAXP {
                if close[a * MAXP + b] + close[b * MAXP + a] > 0
                    && root(&up, a as u8) == a as u8
                    && root(&up, b as u8) == b as u8
                {
                    up[b] = a as u8;
                    merged = true;
                }
            }
        }
        if !merged {
            break;
        }
    }
    // A collision's front grips the two plates: it takes away part of
    // their relative motion, more as the front grows against the smaller
    // one (they slow, and stop once the front is long).
    let mut grip = vec![0_i64; MAXP * MAXP];
    for &(p, o, cells) in edges.grips {
        let (ra, rb) = (root(&up, p) as usize, root(&up, o) as usize);
        if ra != rb {
            grip[ra.min(rb) * MAXP + ra.max(rb)] += cells;
        }
    }
    let mut rs = [0_i64; MAXP];
    let mut cs = [0_i64; MAXP];
    for p in 0..MAXP {
        let r = root(&up, p as u8) as usize;
        rs[r] += resist[p];
        cs[r] += cont[p];
    }
    for a in 0..MAXP {
        for b in a + 1..MAXP {
            let fc = grip[a * MAXP + b];
            if fc == 0 {
                continue;
            }
            let small = cs[a].min(cs[b]).max(1);
            let f = (fc * s.collision_resist * 1000 / small).min(GRIP_MOST);
            let rel = sub(w[a], w[b]);
            let total = (rs[a] + rs[b]).max(1);
            w[a] = sub(w[a], rel.map(|v| v * f / 1000 * rs[b] / total));
            w[b] = add(w[b], rel.map(|v| v * f / 1000 * rs[a] / total));
        }
    }
    let mut least = [u8::MAX; MAXP];
    for p in 0..MAXP {
        let r = root(&up, p as u8) as usize;
        least[r] = least[r].min(p as u8);
    }
    let group: [u8; MAXP] = std::array::from_fn(|p| least[root(&up, p as u8) as usize]);
    let mut pv = vec![[0_i64; 3]; MAXP];
    let mut pr = [0_i64; MAXP];
    if let Some(prev) = prev {
        for p in 0..MAXP {
            let r = root(&up, p as u8) as usize;
            pv[r] = add(pv[r], prev[p].map(|v| v * resist[p] / 1000));
            pr[r] += resist[p];
        }
    }
    let (v0, top) = (s.mantle_drag_mm * 1000, s.speed_mm.1 * 1000);
    let omega = (0..MAXP)
        .map(|p| {
            if cells[p] == 0 {
                return [0; 3];
            }
            let r = root(&up, p as u8) as usize;
            let u = norm(w[r]);
            let mut x = if u > 0 {
                w[r].map(|v| v * dragged(u, v0) / u)
            } else {
                w[r]
            };
            if prev.is_some() && pr[r] > 0 {
                let q = pv[r].map(|v| v * 1000 / pr[r]);
                x = [0, 1, 2].map(|k| (q[k] * INERTIA + x[k] * (1000 - INERTIA)) / 1000);
            }
            let speed = norm(x);
            if speed > top {
                x = x.map(|v| v * top / speed);
            }
            x
        })
        .collect();
    Drive { omega, group }
}

/// What the plates do at a step, read from their relative motion: each
/// cell's boundary kind and margin, the slabs, the ridges and the
/// collisions' fronts (whose forces drive the next step). Marks the
/// margins the crust takes.
struct Look {
    kind: Vec<u8>,
    margin: Vec<u8>,
    slab: Vec<Spot>,
    suction: Vec<Spot>,
    ridge: Vec<Spot>,
    front: Vec<Front>,
}

fn look(
    sphere: &Sphere,
    plate: &[u8],
    group: &[u8; MAXP],
    rock: &mut [Rock],
    omega: &[V3],
    event: &[u8],
    (now_my, full_my): (u16, i64),
) -> Look {
    let floor = floor_born(plate, rock);
    let mut kind = vec![0_u8; CELLS];
    let mut margin = vec![Margin::Inland as u8; CELLS];
    let mut slab = Vec::new();
    let mut suction = Vec::new();
    let mut ridge = Vec::new();
    let mut front = Vec::new();
    let mut active = vec![false; CELLS];
    for c in 0..CELLS {
        let mine = plate[c];
        let x = sphere.centre[c];
        let mut tally: [(u8, u8); 8] = [(NONE, 0); 8];
        let mut ocean_near = false;
        for &(n, _) in &sphere.links[c] {
            let n = n as usize;
            ocean_near |= rock[n].class == OCEAN;
            let o = plate[n];
            if group[o as usize] == group[mine as usize] {
                continue;
            }
            if let Some(t) = tally.iter_mut().find(|t| t.0 == o || t.0 == NONE) {
                t.0 = o;
                t.1 += 1;
            }
        }
        let best = tally
            .iter()
            .filter(|t| t.0 != NONE)
            .max_by_key(|t| (t.1, std::cmp::Reverse(t.0)));
        let mine_cont = rock[c].class != OCEAN;
        let mut arc = false;
        if let Some(&(other, count)) = best {
            let mut dir = [0_i64; 3];
            let mut cont = 0_i64;
            for &(n, _) in &sphere.links[c] {
                let n = n as usize;
                if plate[n] == other {
                    dir = add(dir, sub(sphere.centre[n], x));
                    cont += i64::from(rock[n].class != OCEAN);
                }
            }
            let count = i64::from(count);
            let normal = normalize(tangent(x, dir));
            let rel = sub(
                cross(omega[mine as usize], x),
                cross(omega[other as usize], x),
            );
            let closing = dot(rel, normal);
            let slide = norm(sub(rel, scale(normal, closing)));
            let theirs_cont = 2 * cont >= count;
            let mine_up = (floor[mine as usize], mine) < (floor[other as usize], other);
            let k = classify(
                closing / 1000,
                slide / 1000,
                mine_cont,
                theirs_cont,
                mine_up,
            );
            kind[c] = k as u8 + 1;
            arc = k == Kind::Arc;
            if k == Kind::Collision {
                front.push((c, mine, other, normal, closing));
            }
            if !mine_cont
                && closing >= FORCE_LEAST_UM
                && 2 * closing >= slide
                && (theirs_cont || !mine_up)
            {
                // An old slab is heavy: it pulls its plate, and draws the
                // plate riding over it toward the trench.
                let age = i64::from(rock[c].born_my.saturating_sub(now_my));
                let weight = ((age - SLAB_YOUNG_MY) * 1000 / (full_my - SLAB_YOUNG_MY).max(1))
                    .clamp(0, SLAB_MOST);
                if weight > 0 {
                    slab.push((c, mine, normal, weight));
                    suction.push((c, other, normal.map(|v| -v), weight));
                }
            }
            if !mine_cont && closing <= -FORCE_LEAST_UM && -2 * closing >= slide {
                ridge.push((c, mine, normal.map(|v| -v), 1000));
            }
        }
        if !mine_cont {
            continue;
        }
        margin[c] = if event[c] != Margin::Inland as u8 {
            event[c]
        } else if arc {
            active[c] = true;
            Margin::Active as u8
        } else if ocean_near {
            rock[c].margins |= PASSIVE;
            Margin::Passive as u8
        } else {
            Margin::Inland as u8
        };
    }
    for c in 0..CELLS {
        if active[c] {
            rock[c].margins |= ACTIVE;
        }
    }
    Look {
        kind,
        margin,
        slab,
        suction,
        ridge,
        front,
    }
}

/// The plate `absorbed` joins the plate `kept`: its cells, its floor, its
/// crust and its record with the other plates.
fn absorb(
    (kept, absorbed, why): (usize, usize, u8),
    plate: &mut [u8],
    (bits, land): (&mut [Bit], &mut Land),
    alias: &mut [u8; MAXP],
    parted: &mut [[bool; MAXP]; MAXP],
    broken: &mut [[bool; MAXP]; MAXP],
    contact: &mut [[usize; MAXP]],
) {
    for p in plate.iter_mut() {
        if *p as usize == absorbed {
            *p = kept as u8;
        }
    }
    for bit in bits.iter_mut() {
        if bit.plate as usize == absorbed {
            bit.plate = kept as u8;
        }
    }
    for c in &mut land.cells {
        if c.plate as usize == absorbed {
            c.plate = kept as u8;
            c.switch = why;
        }
    }
    alias[absorbed] = kept as u8;
    for o in 0..MAXP {
        parted[kept][o] &= parted[absorbed][o];
        parted[o][kept] = parted[kept][o];
        broken[kept][o] |= broken[absorbed][o];
        broken[o][kept] = broken[kept][o];
        contact[absorbed][o] = 0;
        contact[o][absorbed] = 0;
    }
}

/// A connected part of a plate: its plate, its area in km², and the cells
/// along its edge by the plate across.
type Part = (u8, i64, [u32; MAXP]);

/// The parts of the plates: each cell's part, and the parts.
fn parts(sphere: &Sphere, plate: &[u8]) -> (Vec<u32>, Vec<Part>) {
    let mut part = vec![u32::MAX; CELLS];
    let mut out: Vec<Part> = Vec::new();
    let mut stack = Vec::new();
    for start in 0..CELLS {
        if part[start] != u32::MAX {
            continue;
        }
        let k = out.len() as u32;
        let id = plate[start];
        let (mut km2, mut edge) = (0, [0_u32; MAXP]);
        part[start] = k;
        stack.push(start);
        while let Some(c) = stack.pop() {
            km2 += i64::from(sphere.area_km2[c]);
            for &(n, _) in &sphere.links[c] {
                let n = n as usize;
                if plate[n] != id {
                    edge[plate[n] as usize] += 1;
                } else if part[n] == u32::MAX {
                    part[n] = k;
                    stack.push(n);
                }
            }
        }
        out.push((id, km2, edge));
    }
    (part, out)
}

/// The pairs that may be held together: continents that have not parted.
fn held(parted: &[[bool; MAXP]; MAXP]) -> Vec<[bool; MAXP]> {
    parted.iter().map(|row| row.map(|p| !p)).collect()
}

/// The shown floor and its plates written back to their bits (the
/// continents are not on the grid: nothing is written back to them).
fn sync(bits: &mut [Bit], plate: &[u8], rock: &[Rock], disp: &[u32]) {
    for c in 0..CELLS {
        if disp[c] != NO_BIT {
            let b = &mut bits[disp[c] as usize];
            b.plate = plate[c];
            b.rock = rock[c];
        }
    }
}

/// Pairs of plates whose continents touch, have not parted, and now move
/// apart: their boundary opens.
fn opening(
    sphere: &Sphere,
    plate: &[u8],
    rock: &[Rock],
    parted: &[[bool; MAXP]; MAXP],
    group: &[u8; MAXP],
    broken: &mut [[bool; MAXP]; MAXP],
) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    for c in 0..CELLS {
        if rock[c].class != CONTINENT {
            continue;
        }
        let a = plate[c];
        for &(n, _) in &sphere.links[c] {
            let n = n as usize;
            let b = plate[n];
            let (i, j) = (a as usize, b as usize);
            if b != a
                && rock[n].class == CONTINENT
                && !parted[i][j]
                && group[i] != group[j]
                && !broken[i][j]
            {
                broken[i][j] = true;
                broken[j][i] = true;
                out.push((a.min(b), a.max(b)));
            }
        }
    }
    out
}

/// Flood basalts where a plume stands near a boundary that opens between
/// `pair`: the continent around takes the [`LIP`] flag, in its pieces.
/// Each plume that floods (once in the history), and the area flooded,
/// km².
fn flood(
    sphere: &Sphere,
    land: &mut Land,
    top: &[u32],
    pair: (u8, u8),
    plumes: &[V3],
    spent: &mut [bool],
) -> Vec<(V3, i64)> {
    let ok: Vec<bool> = land
        .cells
        .iter()
        .map(|c| !c.dead && (c.plate == pair.0 || c.plate == pair.1))
        .collect();
    let edge: Vec<usize> = (0..land.cells.len())
        .filter(|&i| {
            ok[i]
                && land
                    .facing(sphere, top, i)
                    .iter()
                    .any(|&o| o == pair.0 || o == pair.1)
        })
        .collect();
    let mut out = Vec::new();
    for (k, &plume) in plumes.iter().enumerate() {
        if spent[k] {
            continue;
        }
        let near: Vec<usize> = edge
            .iter()
            .copied()
            .filter(|&i| km_between(land.cells[i].pos, plume) < LIP_REACH_KM)
            .collect();
        if near.is_empty() {
            continue;
        }
        let d = land.spread(sphere, &near, |j| ok[j], LIP_KM as u32);
        let mut km2 = 0;
        for (i, c) in land.cells.iter_mut().enumerate() {
            if d[i] != u32::MAX {
                c.rock.margins |= LIP;
                km2 += i64::from(c.area);
            }
        }
        spent[k] = true;
        out.push((plume, km2));
    }
    out
}

/// The history of the planet born from `seed`.
pub fn run(
    seed: u64,
    s: &Settings,
    sphere: &Sphere,
    (old_suture, old_rift): (&[bool], &[bool]),
) -> History {
    assert_eq!(s.radius_km, RADIUS_KM, "only the Earth's radius for now");
    let total_my = s.steps as i64 * s.step_my;
    let area = |c: usize| i64::from(sphere.area_km2[c]);
    let super_warp = SUPER_WARP.map(|(c, a)| (c, a * s.supercontinent_warp / 1000));
    // 1. The supercontinent.
    let upwelling = (0..TRIES)
        .map(|k| random_unit(seed, P_UP, k))
        .find(|&p| latitude(p).abs() <= s.supercontinent_latitude)
        .expect("a point at low latitude");
    let field_at = |p: V3| {
        let q = warp(hash(&[seed, P_SUPER]), p, &super_warp);
        let q = normalize([q[0], q[1], q[2] * s.supercontinent_flatten / 1000]);
        let u = normalize([
            upwelling[0],
            upwelling[1],
            upwelling[2] * s.supercontinent_flatten / 1000,
        ]);
        dot(q, u) * 1000 / ONE
            + (fbm3(hash(&[seed, P_SUPER, 1]), p, &SUPER_FRAY) - 500) * FRAY / 500
                * s.supercontinent_fray
                / 1000
    };
    let field: Vec<i64> = sphere.centre.iter().map(|&p| field_at(p)).collect();
    let crust_cut = quantile(sphere, &field, 1000 - s.crust_per_mille);
    let inside: Vec<bool> = field.iter().map(|&f| f >= crust_cut).collect();
    // 2. The plates at the start: the supercontinent's, then the ocean's.
    let kc = draw(seed, P_FRAG, 0, s.continent_plates.0, s.continent_plates.1).clamp(1, 40) as u64;
    let mut seeds: Vec<(V3, i64)> = (0..kc)
        .map(|k| {
            let at = (0..TRIES)
                .map(|t| random_unit(seed, P_FRAG + 1, k * 10_000 + t))
                .find(|&p| inside[sphere.cell_of(p)])
                .expect("a point on the supercontinent");
            let u = draw(seed, P_FRAG, 100 + k, 0, 1000);
            (at, u * u / 1000 * s.plate_inequality / 1000)
        })
        .collect();
    let ko = draw(seed, P_OCEAN, 0, s.ocean_plates.0, s.ocean_plates.1).clamp(0, 20) as u64;
    for k in 0..ko {
        let at = (0..TRIES / 10)
            .map(|t| random_unit(seed, P_OCEAN + 1, k * 10_000 + t))
            .find(|&p| {
                field_at(p) < crust_cut - TERRANE_OFF
                    && seeds
                        .iter()
                        .all(|&(q, _)| km_between(p, q) >= OCEAN_SEED_APART_KM)
            });
        if let Some(at) = at {
            let u = draw(seed, P_OCEAN, 100 + k, 0, 1000);
            seeds.push((at, u * u / 1000 * s.plate_inequality / 1000));
        }
    }
    let kc = kc as usize;
    let border_seed = hash(&[seed, P_FRAG, 7]);
    let mut plate: Vec<u8> = (0..CELLS)
        .map(|c| {
            let q = warp(border_seed, sphere.centre[c], &RIFT_WARP);
            let among = if inside[c] { kc } else { seeds.len() };
            let mut best = (i64::MIN, 0);
            for (k, &(at, w)) in seeds.iter().take(among).enumerate() {
                let score = dot(q, at) + w;
                if score > best.0 {
                    best = (score, k);
                }
            }
            best.1 as u8
        })
        .collect();
    // The ocean's floor at the start: as if spread from the plates'
    // borders, so it too carries stripes.
    let ridges: Vec<(usize, u32)> = (0..CELLS)
        .filter(|&c| {
            !inside[c]
                && sphere.links[c]
                    .iter()
                    .any(|&(n, _)| !inside[n as usize] && plate[n as usize] != plate[c])
        })
        .map(|c| (c, 0))
        .collect();
    let (from_ridge, _) = sphere.spread(&ridges, |a, b| !inside[b] && plate[a] == plate[b], 20_000);
    // The relief the crust carries (version 7). Its grain is read once at
    // each cell, and a crust keeps the grain of its home. The
    // supercontinent starts with a relief of its own: low shores, a higher
    // heart, old ranges along its old sutures, troughs along its failed
    // rifts.
    let relief_seed = hash(&[seed, P_RELIEF]);
    let rough: Vec<i16> = sphere
        .centre
        .iter()
        .map(|&p| (fbm3(relief_seed, p, &[(120_000, 2), (40_000, 1)]) - 500) as i16)
        .collect();
    let broad: Vec<i16> = sphere
        .centre
        .iter()
        .map(|&p| (fbm3(relief_seed ^ 0xb0, p, &[(300_000, 2), (120_000, 1)]) - 500) as i16)
        .collect();
    let grain: Vec<i16> = (0..CELLS).map(|c| rough[c] + broad[c] / 2).collect();
    let high0: Vec<i16> = {
        let around: Vec<(usize, u32)> =
            (0..CELLS).filter(|&c| !inside[c]).map(|c| (c, 0)).collect();
        let (inland, _) = sphere.spread(&around, |_, b| inside[b], 2_000);
        let near = |which: &[bool], reach: i64| {
            let from: Vec<(usize, u32)> = (0..CELLS)
                .filter(|&c| inside[c] && which[c])
                .map(|c| (c, 0))
                .collect();
            sphere.spread(&from, |_, b| inside[b], reach as u32).0
        };
        let (range, trough) = (near(old_suture, OLD_RANGE.1), near(old_rift, OLD_TROUGH.1));
        let old = |d: u32, (m, km): (i64, i64)| {
            if d == u32::MAX {
                0
            } else {
                m * bump(i64::from(d), km) / 1000
            }
        };
        let raw: Vec<i64> = (0..CELLS)
            .map(|c| {
                if inside[c] {
                    SHORE_M
                        + i64::from(inland[c]).min(HEART_KM) * HEART_M / HEART_KM
                        + old(range[c], OLD_RANGE)
                        + old(trough[c], OLD_TROUGH)
                } else {
                    0
                }
            })
            .collect();
        (0..CELLS)
            .map(|c| if inside[c] { lift(0, raw[c]) } else { 0 })
            .collect()
    };
    // Records at the start.
    let old_my = draw(seed, P_AGE, 1, OLD_OROGENY_MY.0, OLD_OROGENY_MY.1) as u16;
    let mut rock: Vec<Rock> = (0..CELLS)
        .map(|c| {
            if inside[c] {
                let n = fbm3(
                    hash(&[seed, P_AGE]),
                    sphere.centre[c],
                    &[(500_000, 2), (200_000, 1)],
                );
                let mut margins = 0;
                let mut orogeny = NEVER;
                if old_suture[c] {
                    margins |= OLD_SUTURE;
                    orogeny = old_my;
                }
                if old_rift[c] {
                    margins |= FAILED_RIFT;
                }
                Rock {
                    class: CONTINENT,
                    piece: plate[c],
                    margins,
                    arc: 0,
                    rift: 0,
                    born_my: (total_my
                        + CRATON_MY.0
                        + (CRATON_MY.1 - CRATON_MY.0) * n.clamp(0, 1000) / 1000)
                        as u16,
                    orogeny,
                    home: c as u32,
                    thick: CONTINENT_THICK,
                    high: high0[c],
                    cool: 0,
                    sunk: 0,
                    why: [0; CAUSES],
                }
            } else {
                let d = i64::from(from_ridge[c].min(20_000));
                let age = (OCEAN_START_MY + d / OCEAN_START_KM_MY).min(180);
                Rock {
                    class: OCEAN,
                    piece: NONE,
                    margins: 0,
                    arc: 0,
                    rift: 0,
                    born_my: (total_my + age) as u16,
                    orogeny: NEVER,
                    home: c as u32,
                    thick: OCEAN_THICK,
                    high: 0,
                    cool: 0,
                    sunk: 0,
                    why: [0; CAUSES],
                }
            }
        })
        .collect();
    // Terranes: short strings of cells in the open ocean.
    let terranes = draw(seed, P_TERRANE, 0, s.terranes.0, s.terranes.1).clamp(0, 70) as u64;
    let mut placed = 0_u64;
    for k in 0..terranes * 20 {
        if placed == terranes {
            break;
        }
        let at = random_unit(seed, P_TERRANE + 1, k);
        if field_at(at) > crust_cut - TERRANE_OFF || latitude(at).abs() > 700 {
            continue;
        }
        let a = draw(seed, P_TERRANE + 2, k, 0, TAU);
        let (east, north) = (east_at(at), north_at(at, east_at(at)));
        let dir = normalize(add(scale(east, cos(a)), scale(north, sin(a))));
        let n = draw(seed, P_TERRANE + 3, k, s.terrane_cells.0, s.terrane_cells.1);
        let cells: Vec<usize> = (0..n)
            .map(|i| sphere.cell_of(step(at, dir, i * 110)))
            .collect();
        if cells.iter().any(|&c| rock[c].class != OCEAN) {
            continue;
        }
        let born = draw(seed, P_AGE, 2 + k, TERRANE_MY.0, TERRANE_MY.1) + total_my;
        for c in cells {
            rock[c].class = TERRANE;
            rock[c].piece = TERRANE_BASE + placed as u8;
            rock[c].born_my = born as u16;
            rock[c].thick = TERRANE_THICK;
        }
        placed += 1;
    }
    // The mantle's heat at the start: the supercontinent has long been a
    // lid over it. Plumes rise where it is hottest, give or take.
    let deep: Vec<i32> = sphere
        .centre
        .iter()
        .map(|&x| {
            ((fbm3(hash(&[seed, P_HEAT]), x, &HEAT_DEEP_GRAIN) - 500) * HEAT_DEEP / 500) as i32
        })
        .collect();
    let mut heat = vec![0_i32; CELLS];
    warm(sphere, &rock, &[], &deep, &mut heat, 1000);
    let count = draw(seed, P_HOT, 0, s.hotspots.0, s.hotspots.1).max(0) as usize;
    let mut rising: Vec<(i64, usize)> = (0..CELLS)
        .map(|c| {
            let luck = hash(&[seed, P_HOT, 1, c as u64]) % PLUME_LUCK;
            (i64::from(heat[c]) + luck as i64, c)
        })
        .collect();
    rising.sort_unstable_by_key(|&(h, c)| (std::cmp::Reverse(h), c));
    let mut hotspots: Vec<V3> = Vec::with_capacity(count);
    for &(_, c) in &rising {
        if hotspots.len() == count {
            break;
        }
        let x = sphere.centre[c];
        if hotspots.iter().all(|&q| km_between(x, q) >= PLUME_APART_KM) {
            hotspots.push(x);
        }
    }
    let hotspots = &hotspots[..];
    let mut pushes: Vec<Push> = Vec::new();
    // The plumes may flood a breaking continent.
    let plumes: Vec<V3> = hotspots.to_vec();
    // A plume floods once.
    let mut spent = vec![false; plumes.len()];
    // Plates whose continents touch have not parted: they move as one
    // until the forces pull them apart (their boundary opens, `broken`);
    // only parted plates collide.
    let mut parted = [[true; MAXP]; MAXP];
    for c in 0..CELLS {
        if rock[c].class != CONTINENT {
            continue;
        }
        for &(n, _) in &sphere.links[c] {
            let n = n as usize;
            if rock[n].class == CONTINENT && plate[n] != plate[c] {
                parted[plate[c] as usize][plate[n] as usize] = false;
            }
        }
    }
    let mut broken = [[false; MAXP]; MAXP];
    let mut alias = [0_u8; MAXP];
    for (k, a) in alias.iter_mut().enumerate() {
        *a = k as u8;
    }
    let mut born_t = [0_usize; MAXP];
    let mut next_id = seeds.len();
    let mut rifts: Vec<Rift> = Vec::new();
    let mut welds: Vec<Weld> = Vec::new();
    let mut accretions: Vec<Accretion> = Vec::new();
    let mut collisions: Vec<Collision> = Vec::new();
    let mut escapes: Vec<Escape> = Vec::new();
    let mut captures: Vec<Capture> = Vec::new();
    let mut cut_off = 0;
    let mut built_km2 = vec![0];
    let mut lips: Vec<Lip> = Vec::new();
    let mut silt = [0_i64; MAXP];
    // Collisions in progress: the step of first contact (+1), the mean
    // closing seen last, µm a year, and the front's cells.
    let mut contact = vec![[0_usize; MAXP]; MAXP];
    let mut closing_of = vec![[0_i64; MAXP]; MAXP];
    // Steps each collision has stood still.
    let mut quiet = vec![[0_usize; MAXP]; MAXP];
    // The rifts in progress and the abandoned ones, and the forces at
    // the plates' edges read at the last step.
    let mut troughs: Vec<Trough> = Vec::new();
    let mut edges_last: (Vec<Spot>, Vec<Spot>, Vec<Spot>) = (Vec::new(), Vec::new(), Vec::new());
    let mut front_of = vec![[0_i64; MAXP]; MAXP];
    let mut record_of = vec![[usize::MAX; MAXP]; MAXP];
    let mut escaped = vec![[false; MAXP]; MAXP];
    let mut shed = [false; MAXP];
    let young = 2 * s.rift_steps;
    let arc_steps = s.arc_steps.clamp(1, 250) as u8;
    // The crust (version 10). The continents are rigid pieces: every
    // cell of continental crust lives in its piece's own lattice, placed
    // on the planet by the piece's turn, and is never resampled. The
    // ocean floor is a set of bits the grid draws. The grid (`plate`,
    // `rock`, `top`) is a view of the pieces, for the forces, the page
    // and the neighbourhood queries: no state of the continents lives
    // on it.
    let mut land = Land::new();
    let frame0 = land.frame(IDENTITY);
    let mut bits: Vec<Bit> = Vec::new();
    let mut disp = vec![NO_BIT; CELLS];
    let mut top = vec![NO_BIT; CELLS];
    for c in 0..CELLS {
        if rock[c].class == OCEAN {
            disp[c] = bits.len() as u32;
            bits.push(Bit {
                pos: sphere.centre[c],
                plate: plate[c],
                rock: rock[c],
                seen: true,
                id: c as u32,
            });
        } else {
            top[c] = land.add(sphere, (frame0, c as u32), plate[c], rock[c], c as u32) as u32;
        }
    }
    let mut marks: Vec<Mark> = Vec::new();
    // Step 0.
    let mut out_plate = Vec::with_capacity(s.steps + 1);
    let mut out_class = Vec::with_capacity(s.steps + 1);
    let mut out_age = Vec::with_capacity(s.steps + 1);
    let mut out_kind = Vec::with_capacity(s.steps + 1);
    let mut out_margin = Vec::with_capacity(s.steps + 1);
    let mut out_motion = Vec::with_capacity(s.steps + 1);
    let mut out_home = Vec::with_capacity(s.steps + 1);
    let mut out_margins = Vec::with_capacity(s.steps + 1);
    let mut out_orogeny = Vec::with_capacity(s.steps + 1);
    let mut out_thick = Vec::with_capacity(s.steps + 1);
    let mut out_high = Vec::with_capacity(s.steps + 1);
    let mut out_cause = Vec::with_capacity(s.steps + 1);
    let mut out_shown = Vec::with_capacity(s.steps + 1);
    let mut out_id = Vec::with_capacity(s.steps + 1);
    let mut out_hidden = Vec::with_capacity(s.steps + 1);
    let mut out_marks = Vec::with_capacity(s.steps + 1);
    let mut out_heat = Vec::with_capacity(s.steps + 1);
    let mut out_pieces = Vec::with_capacity(s.steps + 1);
    let mut out_turned = vec![vec![[0_i64; 3]; MAXP]];
    let mut out_rifts = Vec::with_capacity(s.steps + 1);
    let mut continental_km2 = Vec::with_capacity(s.steps + 1);
    let mut shortened_km2 = vec![0];
    let mut made_km2 = vec![0];
    let first = drive(
        sphere,
        s,
        &plate,
        &rock,
        &pushes,
        &alias,
        &heat,
        Edges {
            slab: &[],
            suction: &[],
            ridge: &[],
            grips: &[],
        },
        None,
        &held(&parted),
    );
    let mut omega = first.omega;
    let mut group = first.group;
    let mut opened = opening(sphere, &plate, &rock, &parted, &group, &mut broken);
    for &pair in &opened {
        for (at, km2) in flood(sphere, &mut land, &top, pair, &plumes, &mut spent) {
            lips.push(Lip {
                my: total_my as u16,
                at,
                km2,
            });
        }
    }
    let mut event = vec![Margin::Inland as u8; CELLS];
    let mut fronts: Vec<Front> = Vec::new();
    let mut last_kind = vec![0_u8; CELLS];
    let mut next_bit = CELLS as u32;
    let mut shown_now = vec![0_u8; CELLS];
    let mut hidden_now = [0_u32; 3];
    for t in 0..=s.steps {
        let now_my = ((s.steps - t) as i64 * s.step_my) as u16;
        if t > 0 {
            for b in &mut bits {
                b.seen = false;
            }
            for &d in &disp {
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
            let mut cont_of = land.weight();
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
                        let (kept, absorbed) =
                            if (cont_of[a], -(a as i64)) >= (cont_of[b], -(b as i64)) {
                                (a, b)
                            } else {
                                (b, a)
                            };
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
                            &mut plate,
                            (&mut bits, &mut land),
                            &mut alias,
                            &mut parted,
                            &mut broken,
                            &mut contact,
                        );
                        let (nk, nx) = (cells_of[kept].max(1), cells_of[absorbed]);
                        omega[kept] = [0, 1, 2]
                            .map(|i| (omega[kept][i] * nk + omega[absorbed][i] * nx) / (nk + nx));
                        omega[absorbed] = [0; 3];
                        cells_of[kept] += cells_of[absorbed];
                        cont_of[kept] += cont_of[absorbed];
                        cells_of[absorbed] = 0;
                        cont_of[absorbed] = 0;
                    } else if t - since >= ESCAPE_AFTER
                        && closing >= ESCAPE_MIN_UM
                        && front_of[a][b] >= ESCAPE_CELLS
                        && !escaped[a][b]
                        && next_id < MAXP
                    {
                        escaped[a][b] = true;
                        let (over, ind) = if cont_of[a] >= cont_of[b] {
                            (a, b)
                        } else {
                            (b, a)
                        };
                        if cont_of[over] < ESCAPE_FROM_KM2 || t - born_t[over] < young || shed[over]
                        {
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
                        let sign = if hash(&[seed, P_ESCAPE, a as u64, b as u64]).is_multiple_of(2)
                        {
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
                                    plate[m as usize] as usize != over
                                        && rock[m as usize].class == OCEAN
                                })
                        });
                        if km2 < 200_000 || 3 * km2 > cont_of[over] || !free {
                            continue;
                        }
                        let new = next_id;
                        next_id += 1;
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
                        escapes.push(Escape {
                            my: now_my,
                            from: over as u8,
                            block: new as u8,
                            km2,
                        });
                    }
                }
            }
            // 3b. Rifts (version 11, the lead's 27 November: « de vrais
            // rifts »). A plate tears where it is pulled apart and weak:
            // a trough begins, stretches and thins the crust along it
            // step after step, grows at its tips toward the weakest
            // crust ahead, and cuts the plate in two once it has crossed
            // the continent and the crust has given way. A trough the
            // pull has left is abandoned and stays.
            {
                let (cell_tq, edge_tq) = torques(
                    sphere,
                    s,
                    &plate,
                    &rock,
                    &pushes,
                    &alias,
                    &heat,
                    Edges {
                        slab: &edges_last.0,
                        suction: &edges_last.1,
                        ridge: &edges_last.2,
                        grips: &[],
                    },
                );
                let mut loads: Vec<Vec<Load>> = vec![Vec::new(); MAXP];
                for c in 0..CELLS {
                    let p = plate[c] as usize;
                    if cont_of[p] > 0 {
                        let land_here = rock[c].class != OCEAN;
                        loads[p].push((
                            sphere.centre[c],
                            cell_tq[c],
                            1000 + if land_here { s.continent_drag } else { 0 },
                            rock[c].class == CONTINENT,
                        ));
                    }
                }
                for &(c, p, tq) in &edge_tq {
                    if cont_of[p as usize] > 0 {
                        loads[p as usize].push((sphere.centre[c as usize], tq, 0, false));
                    }
                }
                let v0 = s.mantle_drag_mm * 1000;
                let break_m = RIFT_WIDTH_KM * 1000 * i64::from(CONTINENT_THICK - BREAK_THICK)
                    / i64::from(BREAK_THICK);
                let mut busy = [false; MAXP];
                let mut on_path = vec![false; land.cells.len()];
                for tr in &troughs {
                    for &i in &tr.path {
                        on_path[i as usize] = true;
                    }
                }
                let mut cuts: Vec<usize> = Vec::new();
                for k in 0..troughs.len() {
                    if troughs[k].failed {
                        continue;
                    }
                    let pl = alias_of(&alias, troughs[k].plate);
                    let mut path = std::mem::take(&mut troughs[k].path);
                    path.retain(|&i| {
                        let c = &land.cells[i as usize];
                        !c.dead && c.plate == pl
                    });
                    let record = troughs[k].record;
                    let give_up = |land: &mut Land, rifts: &mut Vec<Rift>| {
                        rifts[record].aborted = true;
                        rifts[record].failed_my = now_my;
                        for c in &mut land.cells {
                            if usize::from(c.rock.rift) == record + 1 {
                                c.rock.margins |= FAILED_RIFT;
                            }
                        }
                    };
                    if path.is_empty() || busy[pl as usize] {
                        troughs[k].failed = true;
                        troughs[k].path = path;
                        give_up(&mut land, &mut rifts);
                        continue;
                    }
                    busy[pl as usize] = true;
                    troughs[k].plate = pl;
                    let at = |i: u32| land.cells[i as usize].pos;
                    let mid = at(path[path.len() / 2]);
                    let mut n = if path.len() >= 5 {
                        normalize(cross(mid, sub(at(path[path.len() - 1]), at(path[0]))))
                    } else {
                        normalize(tangent(mid, troughs[k].normal))
                    };
                    if dot(n, troughs[k].normal) < 0 {
                        n = n.map(|v| -v);
                    }
                    troughs[k].normal = n;
                    let pull = parting(&loads[pl as usize], mid, n, v0).unwrap_or(0);
                    let w0 = path
                        .iter()
                        .map(|&i| {
                            let c = &land.cells[i as usize];
                            weak(heat[c.cell as usize], &c.rock)
                        })
                        .sum::<i64>()
                        / path.len() as i64;
                    let done_share = (troughs[k].ext_m * 1000 / break_m).min(1000);
                    let w = w0 + (1000 - w0) * done_share / 1000 * done_share / 1000;
                    let rate = pull * w / 1000;
                    if rate < RIFT_LEAST_UM {
                        troughs[k].idle += 1;
                        if troughs[k].idle >= RIFT_IDLE {
                            troughs[k].failed = true;
                            give_up(&mut land, &mut rifts);
                        }
                        troughs[k].path = path;
                        continue;
                    }
                    troughs[k].idle = 0;
                    // Its tips grow toward the weakest crust ahead.
                    for e in 0..2 {
                        if troughs[k].done[e] {
                            continue;
                        }
                        let mut budget = rate * s.step_my * RIFT_GROW / 1000;
                        while budget > 0 {
                            let i = if e == 0 {
                                path[0]
                            } else {
                                path[path.len() - 1]
                            } as usize;
                            let me = land.cells[i];
                            let line = normalize(cross(n, me.pos));
                            let dir = if e == 0 { line.map(|v| -v) } else { line };
                            let mut around: Vec<usize> =
                                land.kin(sphere, i).map(|(j, _)| j).collect();
                            let q = me.cell as usize;
                            for m in std::iter::once(q)
                                .chain(sphere.links[q].iter().map(|&(m, _)| m as usize))
                            {
                                if top[m] != NO_BIT && land.cells[top[m] as usize].frame != me.frame
                                {
                                    around.push(top[m] as usize);
                                }
                            }
                            let mut best: Option<(i64, std::cmp::Reverse<usize>, i64)> = None;
                            for cone in [ONE / 2, ONE / 10] {
                                for &j in &around {
                                    let o = &land.cells[j];
                                    if o.dead || o.under || o.plate != pl || on_path[j] {
                                        continue;
                                    }
                                    let km = km_between(o.pos, me.pos);
                                    if km == 0 {
                                        continue;
                                    }
                                    let along =
                                        dot(normalize(tangent(me.pos, sub(o.pos, me.pos))), dir);
                                    if along < cone {
                                        continue;
                                    }
                                    let off = dot(o.pos, n).abs() * RADIUS_KM / ONE;
                                    let score =
                                        weak(heat[o.cell as usize], &o.rock) - off * RIFT_OFF / 100;
                                    let key = (score, std::cmp::Reverse(j), km);
                                    if best.is_none_or(|b| key > b) {
                                        best = Some(key);
                                    }
                                }
                                if best.is_some() {
                                    break;
                                }
                            }
                            let Some((_, std::cmp::Reverse(j), km)) = best else {
                                troughs[k].done[e] = true;
                                break;
                            };
                            on_path[j] = true;
                            if e == 0 {
                                path.insert(0, j as u32);
                            } else {
                                path.push(j as u32);
                            }
                            budget -= km;
                        }
                    }
                    // The pull stretches and thins the crust along it: it
                    // sinks, and will sink as much again as it cools. The
                    // crust's volume is kept: a thinned cell stands for a
                    // wider crust.
                    let more = rate * s.step_my;
                    let from: Vec<usize> = path.iter().map(|&i| i as usize).collect();
                    let d = land.spread(sphere, &from, |_| true, STRETCH_KM);
                    for (i, c) in land.cells.iter_mut().enumerate() {
                        if d[i] == u32::MAX || c.dead {
                            continue;
                        }
                        // Each cell thins from where it is (a cell the
                        // trough has just reached has all to do), down to
                        // the thickness at which the crust gives way on
                        // the axis, less toward the zone's edge.
                        let reach = bump(i64::from(d[i]), i64::from(STRETCH_KM));
                        let least = i64::from(BREAK_THICK)
                            + i64::from(CONTINENT_THICK - BREAK_THICK) * (1000 - reach) / 1000;
                        let r = &mut c.rock;
                        let was = i64::from(r.thick);
                        r.rift = (record + 1).min(255) as u8;
                        let thin = (more * was / (RIFT_WIDTH_KM * 1000) * was
                            / i64::from(CONTINENT_THICK)
                            * reach
                            / 1000)
                            .min(was - least);
                        if thin <= 0 {
                            continue;
                        }
                        r.thick = (was - thin) as u16;
                        c.area = (i64::from(c.area) * was / (was - thin)) as u32;
                        let drop = thin * STRETCH_M / i64::from(CONTINENT_THICK - BREAK_THICK);
                        r.rise(-drop, BY_STRETCH);
                        r.cool = lift(r.cool, drop);
                        r.sunk = lift(r.sunk, drop);
                    }
                    for &i in &path {
                        let c = &mut land.cells[i as usize];
                        if c.event != Margin::Suture as u8 {
                            c.event = Margin::Rift as u8;
                        }
                    }
                    troughs[k].ext_m = (troughs[k].ext_m + more).min(break_m);
                    rifts[record].cells = rifts[record].cells.max(path.len() as u32);
                    if troughs[k].done == [true; 2] && troughs[k].ext_m >= break_m {
                        cuts.push(k);
                    }
                    troughs[k].path = path;
                }
                // The crust has given way along a trough that crosses its
                // continent: the plate is cut in two along it, and on
                // through its floor from both ends. Each side keeps its
                // lattice: the two shores fit for good.
                let mut cut_done = vec![false; troughs.len()];
                for &k in &cuts {
                    if next_id >= MAXP {
                        continue;
                    }
                    let old = troughs[k].plate;
                    let path = &troughs[k].path;
                    let seat = seats(&land, &top);
                    let mut on = vec![false; CELLS];
                    for &i in path {
                        on[seat[i as usize] as usize] = true;
                    }
                    let mut sources: Vec<(usize, u32)> = Vec::new();
                    for (j, &i) in path.iter().enumerate() {
                        let x = land.cells[i as usize].pos;
                        let a = land.cells[path[j.saturating_sub(1)] as usize].pos;
                        let b = land.cells[path[(j + 1).min(path.len() - 1)] as usize].pos;
                        let left = cross(x, sub(b, a));
                        let q = seat[i as usize] as usize;
                        for &(m, _) in &sphere.links[q] {
                            let m = m as usize;
                            if plate[m] == old && !on[m] {
                                let side = dot(sub(sphere.centre[m], sphere.centre[q]), left) > 0;
                                sources.push((m, u32::from(side)));
                            }
                        }
                    }
                    let (_, label) =
                        sphere.spread(&sources, |_, b| plate[b] == old && !on[b], 30_000);
                    let mut count = [0_usize; 2];
                    for c in 0..CELLS {
                        if plate[c] == old && label[c] < 2 {
                            count[label[c] as usize] += 1;
                        }
                    }
                    if count[0] == 0 || count[1] == 0 {
                        continue;
                    }
                    let away = u32::from(count[1] <= count[0]);
                    let new = next_id as u8;
                    next_id += 1;
                    let side_new: Vec<bool> = (0..CELLS)
                        .map(|c| plate[c] == old && label[c] == away)
                        .collect();
                    for c in 0..CELLS {
                        if side_new[c] {
                            plate[c] = new;
                            if disp[c] != NO_BIT {
                                bits[disp[c] as usize].plate = new;
                            }
                        }
                    }
                    for (i, c) in land.cells.iter_mut().enumerate() {
                        if c.plate == old && side_new[seat[i] as usize] {
                            c.plate = new;
                            c.switch = SWITCH_RIFT;
                        }
                    }
                    for (at, km2) in flood(sphere, &mut land, &top, (old, new), &plumes, &mut spent)
                    {
                        lips.push(Lip {
                            my: now_my,
                            at,
                            km2,
                        });
                    }
                    let (o, w) = (old as usize, new as usize);
                    for p in 0..MAXP {
                        parted[w][p] = parted[o][p];
                        parted[p][w] = parted[p][o];
                    }
                    parted[o][w] = false;
                    parted[w][o] = false;
                    broken[o][w] = true;
                    broken[w][o] = true;
                    born_t[w] = t;
                    group[w] = new;
                    omega[w] = omega[o];
                    let r = &mut rifts[troughs[k].record];
                    r.new = new;
                    r.cut_my = now_my;
                    cut_done[k] = true;
                }
                let mut k = 0;
                troughs.retain(|_| {
                    k += 1;
                    !cut_done[k - 1]
                });
                if cut_done.iter().any(|&c| c) {
                    cont_of = land.weight();
                }
                // A plate with no trough begins to tear where the pull
                // across its weakest crust stretches it fast enough.
                for p in 0..MAXP {
                    if busy[p] || cont_of[p] == 0 || alias[p] != p as u8 || loads[p].is_empty() {
                        continue;
                    }
                    let mut spots: Vec<(i64, u32, usize)> = land
                        .cells
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| {
                            !c.dead
                                && !c.under
                                && c.plate as usize == p
                                && c.rock.class == CONTINENT
                        })
                        .map(|(i, c)| (weak(heat[c.cell as usize], &c.rock), c.id, i))
                        .collect();
                    spots.sort_unstable_by_key(|&(w, id, _)| (std::cmp::Reverse(w), id));
                    let mut picked: Vec<(i64, usize)> = Vec::new();
                    for &(w, _, i) in &spots {
                        if picked.len() == RIFT_SPOTS {
                            break;
                        }
                        let x = land.cells[i].pos;
                        if picked
                            .iter()
                            .all(|&(_, j)| km_between(x, land.cells[j].pos) >= RIFT_SPOT_APART_KM)
                        {
                            picked.push((w, i));
                        }
                    }
                    let mut best: Option<(i64, std::cmp::Reverse<usize>, usize, V3)> = None;
                    for (order, &(w, i)) in picked.iter().enumerate() {
                        let x = land.cells[i].pos;
                        let (e, nn) = (east_at(x), north_at(x, east_at(x)));
                        for a in 0..12 {
                            let angle = a * TAU / 24;
                            let n = normalize(add(scale(e, cos(angle)), scale(nn, sin(angle))));
                            if let Some(pull) = parting(&loads[p], x, n, v0) {
                                let key = (
                                    pull * w / 1000,
                                    std::cmp::Reverse(order * 12 + a as usize),
                                    i,
                                    n,
                                );
                                if best.is_none_or(|b| (key.0, key.1) > (b.0, b.1)) {
                                    best = Some(key);
                                }
                            }
                        }
                    }
                    if let Some((rate, _, i, n)) = best
                        && rate >= RIFT_LEAST_UM
                    {
                        troughs.push(Trough {
                            record: rifts.len(),
                            plate: p as u8,
                            path: vec![i as u32],
                            normal: n,
                            ext_m: 0,
                            done: [false; 2],
                            idle: 0,
                            failed: false,
                        });
                        rifts.push(Rift {
                            my: now_my,
                            plate: p as u8,
                            new: NONE,
                            centre: land.cells[i].pos,
                            area_km2: cont_of[p],
                            aborted: false,
                            cut_my: NEVER,
                            parted_my: NEVER,
                            failed_my: NEVER,
                            cells: 1,
                        });
                    }
                }
            }
            // 4. The plates turn, carrying their crust. A piece of
            // continent only changes its turn; every bit of floor moves
            // exactly, from where it was, not from a cell.
            let moved = omega.clone();
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
            for m in &mut marks {
                let p = if m.land {
                    land.cells[m.bit as usize].plate
                } else {
                    bits[m.bit as usize].plate
                };
                m.pos = normalize(apply(&turn[p as usize], m.pos));
            }
            for b in &mut bits {
                b.pos = normalize(apply(&turn[b.plate as usize], b.pos));
            }
            for tr in &mut troughs {
                tr.normal = normalize(apply(&turn[tr.plate as usize], tr.normal));
            }
            land.turn(sphere, &turn);
            let n = bits.len();
            let at: Vec<u32> = bits.iter().map(|b| sphere.cell_of(b.pos) as u32).collect();
            let mut start = vec![0_u32; CELLS + 1];
            for &c in &at {
                start[c as usize + 1] += 1;
            }
            for c in 0..CELLS {
                start[c + 1] += start[c];
            }
            let mut order = vec![0_u32; n];
            let mut fill = start.clone();
            for (i, &c) in at.iter().enumerate() {
                order[fill[c as usize] as usize] = i as u32;
                fill[c as usize] += 1;
            }
            let old_plate = plate.clone();
            let old_rock = rock.clone();
            let floor = floor_born(&old_plate, &old_rock);
            let weight = cont_of;
            let key = move |p: u8, class: u8| precedence(&weight, &floor, p, class);
            let ride = |p: u8| (-floor[p as usize], -i64::from(p));
            let mut dead = vec![false; n];
            // Where continents collide (a collision's front last step, or
            // next to it), a continent run under the other is a suture.
            let collision = Kind::Collision as u8 + 1;
            let pressed: Vec<bool> = (0..CELLS)
                .map(|q| {
                    last_kind[q] == collision
                        || sphere.links[q]
                            .iter()
                            .any(|&(m, _)| last_kind[m as usize] == collision)
                })
                .collect();
            let mut spare: Vec<Vec<u32>> = vec![Vec::new(); CELLS];
            let mut pending = vec![false; CELLS];
            // The cells of crust that dock (a terrane or an arc), and the
            // plate they join.
            let mut accrete: Vec<(usize, u8)> = Vec::new();
            let mut shortened = 0;
            let mut made = 0;
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
                shortened += gone;
                if take == vol {
                    land.kill(i);
                } else {
                    land.cells[i].area -= gone as u32;
                }
            }
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
            for &(i0, pw) in &accrete {
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
                    accretions.push(Accretion {
                        my: now_my,
                        plate: pw,
                        piece,
                        km2,
                    });
                }
            }
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
                    if r.arc >= arc_steps
                        && hash(&[seed, P_ARC, u64::from(r.home)]) % 1000 < ARC_SHARE
                    {
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
                        made += area(q);
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
                        top[q] = land.add(sphere, place, pw, r, next_bit) as u32;
                        next_bit += 1;
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
            paint(&land, &top, &mut plate, &mut rock);
            // A lower plate's floor showing through a hole of the sampling
            // of the plates riding over it has sunk under them: the cell is
            // a hole of theirs.
            let over: Vec<(i64, i64, i64)> = (0..CELLS)
                .map(|q| {
                    if pending[q] {
                        (i64::MIN, 0, 0)
                    } else {
                        key(plate[q], rock[q].class)
                    }
                })
                .collect();
            let through: Vec<usize> = (0..CELLS)
                .filter(|&q| {
                    !pending[q]
                        && rock[q].class == OCEAN
                        && sphere.links[q]
                            .iter()
                            .filter(|&&(m, _)| {
                                let m = m as usize;
                                group[plate[m] as usize] != group[plate[q] as usize]
                                    && over[m] > over[q]
                            })
                            .count()
                            >= HOLE_AROUND
                })
                .collect();
            for q in through {
                for &i in &order[start[q] as usize..start[q + 1] as usize] {
                    dead[i as usize] = true;
                }
                spare[q].clear();
                pending[q] = true;
                disp[q] = NO_BIT;
            }
            // 6. A cell the floor's sampling left empty inside a plate
            // shows a bit of floor not shown nearby (none is copied).
            let mut used = vec![false; n];
            for (pass, reach) in HOLE_KM.iter().enumerate() {
                for q in 0..CELLS {
                    if !pending[q] {
                        continue;
                    }
                    let x = sphere.centre[q];
                    let mut ring: Vec<usize> =
                        sphere.links[q].iter().map(|&(m, _)| m as usize).collect();
                    if pass == 1 {
                        let first = ring.clone();
                        for m in first {
                            ring.extend(sphere.links[m].iter().map(|&(k, _)| k as usize));
                        }
                    }
                    let mut best: Option<(bool, i64, u32)> = None;
                    for &m in &ring {
                        for &j in &spare[m] {
                            if used[j as usize] || dead[j as usize] {
                                continue;
                            }
                            let d = km_between(bits[j as usize].pos, x);
                            let k = (!bits[j as usize].seen, d, j);
                            if d < *reach && best.is_none_or(|b| k < b) {
                                best = Some(k);
                            }
                        }
                    }
                    if let Some((_, _, j)) = best {
                        used[j as usize] = true;
                        pending[q] = false;
                        plate[q] = bits[j as usize].plate;
                        rock[q] = bits[j as usize].rock;
                        disp[q] = j;
                    }
                }
            }
            // A cell of floor still empty with its plate all around is a
            // hole of the floor's sampling, not a gap: it draws the floor
            // beside it again (the floor it showed, between two pieces of
            // continent with no floor beside), and no floor is made.
            // Empty cells walled in by plates that move as one are holes
            // of the sampling whatever their number (version 9): a rigid
            // plate does not open, and no ocean floor is born inside it.
            let ocean_rock = |q: usize| Rock {
                class: OCEAN,
                piece: NONE,
                margins: 0,
                arc: 0,
                rift: 0,
                born_my: now_my,
                orogeny: NEVER,
                home: q as u32,
                thick: OCEAN_THICK,
                high: 0,
                cool: 0,
                sunk: 0,
                why: [0; CAUSES],
            };
            let again = |q: usize, m: usize, rock: &[Rock], pending: &[bool]| {
                if rock[m].class == OCEAN {
                    rock[m]
                } else if let Some(&(_, k)) = sphere.links[q]
                    .iter()
                    .filter(|&&(k, _)| !pending[k as usize] && rock[k as usize].class == OCEAN)
                    .map(|&(k, km)| (km, k as usize))
                    .min()
                    .as_ref()
                {
                    rock[k]
                } else if old_rock[q].class == OCEAN {
                    old_rock[q]
                } else {
                    ocean_rock(q)
                }
            };
            let mut inner = vec![false; CELLS];
            let mut seen_hole = vec![false; CELLS];
            for q0 in 0..CELLS {
                if !pending[q0] || seen_hole[q0] {
                    continue;
                }
                let (mut stack, mut cells) = (vec![q0], Vec::new());
                seen_hole[q0] = true;
                let (mut wall, mut one) = (NONE, true);
                while let Some(q) = stack.pop() {
                    cells.push(q);
                    for &(m, _) in &sphere.links[q] {
                        let m = m as usize;
                        if pending[m] {
                            if !seen_hole[m] {
                                seen_hole[m] = true;
                                stack.push(m);
                            }
                        } else {
                            let g = group[plate[m] as usize];
                            one &= wall == NONE || wall == g;
                            wall = g;
                        }
                    }
                }
                if one && wall != NONE {
                    for q in cells {
                        inner[q] = true;
                    }
                }
            }
            loop {
                let mut filled = Vec::new();
                for q in 0..CELLS {
                    if !pending[q] || !inner[q] {
                        continue;
                    }
                    if let Some((_, m)) = sphere.links[q]
                        .iter()
                        .filter(|&&(m, _)| !pending[m as usize])
                        .map(|&(m, km)| (km, m as usize))
                        .min()
                    {
                        filled.push((q, plate[m], again(q, m, &rock, &pending)));
                    }
                }
                if filled.is_empty() {
                    break;
                }
                for (q, p, r) in filled {
                    plate[q] = p;
                    rock[q] = r;
                    disp[q] = NO_BIT;
                    pending[q] = false;
                }
            }
            let mut mended = Vec::new();
            for q in 0..CELLS {
                if !pending[q] {
                    continue;
                }
                let around: Vec<(u32, usize)> = sphere.links[q]
                    .iter()
                    .filter(|&&(m, _)| !pending[m as usize])
                    .map(|&(m, km)| (km, m as usize))
                    .collect();
                let g = |m: usize| group[plate[m] as usize];
                // Between two plates it is a gap only if they part there.
                let parting = || {
                    let &(_, first) = around.iter().min().expect("neighbours");
                    let (mut ca, mut cb, mut other) = ([0_i64; 3], [0_i64; 3], first);
                    for &(_, m) in &around {
                        if g(m) == g(first) {
                            ca = add(ca, sphere.centre[m]);
                        } else {
                            cb = add(cb, sphere.centre[m]);
                            other = m;
                        }
                    }
                    let x = sphere.centre[q];
                    let across = normalize(tangent(x, sub(normalize(cb), normalize(ca))));
                    let rel = sub(
                        cross(moved[plate[other] as usize], x),
                        cross(moved[plate[first] as usize], x),
                    );
                    dot(rel, across) > GAP_LEAST_UM
                };
                if around.len() >= HOLE_AROUND
                    && (around.iter().all(|&(_, m)| g(m) == g(around[0].1)) || !parting())
                {
                    let &(_, m) = around.iter().min().expect("neighbours");
                    mended.push((q, plate[m], again(q, m, &rock, &pending)));
                }
            }
            for (q, p, r) in mended {
                plate[q] = p;
                rock[q] = r;
                disp[q] = NO_BIT;
                pending[q] = false;
            }
            // 7. Gaps: new ocean floor, filled from both sides at once.
            let mut open: Vec<usize> = (0..CELLS).filter(|&q| pending[q]).collect();
            let born: Vec<usize> = open.clone();
            while !open.is_empty() {
                let mut set = Vec::new();
                let mut next = Vec::new();
                for &q in &open {
                    let mut tally: [(u8, u8); 8] = [(NONE, 0); 8];
                    for &(m, _) in &sphere.links[q] {
                        let m = m as usize;
                        if pending[m] {
                            continue;
                        }
                        if let Some(t) = tally.iter_mut().find(|t| t.0 == plate[m] || t.0 == NONE) {
                            t.0 = plate[m];
                            t.1 += 1;
                        }
                    }
                    match tally
                        .iter()
                        .filter(|t| t.0 != NONE)
                        .max_by_key(|t| (t.1, std::cmp::Reverse(t.0)))
                    {
                        Some(&(p, _)) => set.push((q, p)),
                        None => next.push(q),
                    }
                }
                if set.is_empty() {
                    break;
                }
                for (q, p) in set {
                    pending[q] = false;
                    plate[q] = p;
                    rock[q] = ocean_rock(q);
                }
                open = next;
            }
            let mut newborn = vec![false; CELLS];
            for &q in &born {
                if pending[q] {
                    plate[q] = old_plate[q];
                    rock[q] = ocean_rock(q);
                    pending[q] = false;
                }
                newborn[q] = true;
                disp[q] = bits.len() as u32;
                bits.push(Bit {
                    pos: sphere.centre[q],
                    plate: plate[q],
                    rock: rock[q],
                    seen: true,
                    id: next_bit,
                });
                next_bit += 1;
            }
            dead.resize(bits.len(), false);
            // The crust beside a floor just born is a rift's margin: the
            // crust the grid draws around the new floor, and the crust
            // whose free neighbour in its own lattice lies in it.
            if !born.is_empty() {
                let mut torn: Vec<usize> = Vec::new();
                for &q in &born {
                    for &(m, _) in &sphere.links[q] {
                        if top[m as usize] != NO_BIT {
                            torn.push(top[m as usize] as usize);
                        }
                    }
                }
                for i in 0..land.cells.len() {
                    let c = &land.cells[i];
                    let rot = &land.rot[c.frame as usize];
                    let beside = sphere.links[c.at as usize].iter().any(|&(k, _)| {
                        land.get(c.frame, k).is_none()
                            && newborn
                                [sphere.cell_of(normalize(apply(rot, sphere.centre[k as usize])))]
                    });
                    if beside {
                        torn.push(i);
                    }
                }
                for i in torn {
                    let c = &mut land.cells[i];
                    if c.event != Margin::Suture as u8 && !c.under {
                        c.rock.margins |= RIFT;
                        c.event = Margin::Rift as u8;
                    }
                }
            }
            // Where each bit of floor is drawn: its cell, or the cell it
            // fills.
            let mut drawn: Vec<u32> = at;
            drawn.resize(bits.len(), 0);
            for c in 0..CELLS {
                if disp[c] != NO_BIT {
                    drawn[disp[c] as usize] = c as u32;
                }
            }
            paint(&land, &top, &mut plate, &mut rock);
            // A lone cell of a plate inside another (a speck left by a
            // transfer or a gap) joins the plate around it.
            let mut relabel: Vec<(usize, u8, u8)> = Vec::new();
            for c in 0..CELLS {
                let same = sphere.links[c]
                    .iter()
                    .filter(|&&(m, _)| plate[m as usize] == plate[c])
                    .count();
                if same > usize::from(rock[c].class == OCEAN) {
                    continue;
                }
                // Not a speck if its crust holds to its plate's in its own
                // lattice (version 11).
                if top[c] != NO_BIT
                    && land
                        .kin(sphere, top[c] as usize)
                        .any(|(j, _)| land.cells[j].plate == plate[c])
                {
                    continue;
                }
                let mut tally: [(u8, u8); 8] = [(NONE, 0); 8];
                for &(m, _) in &sphere.links[c] {
                    let o = plate[m as usize];
                    if o == plate[c] {
                        continue;
                    }
                    if let Some(t) = tally.iter_mut().find(|t| t.0 == o || t.0 == NONE) {
                        t.0 = o;
                        t.1 += 1;
                    }
                }
                if let Some(&(o, _)) = tally
                    .iter()
                    .filter(|t| t.0 != NONE)
                    .max_by_key(|t| (t.1, std::cmp::Reverse(t.0)))
                {
                    relabel.push((c, plate[c], o));
                    plate[c] = o;
                }
            }
            if !relabel.is_empty() {
                let mut to = vec![(NONE, NONE); CELLS];
                for &(c, from, o) in &relabel {
                    to[c] = (from, o);
                }
                for (i, b) in bits.iter_mut().enumerate() {
                    let (from, o) = to[drawn[i] as usize];
                    if from != NONE && b.plate == from {
                        b.plate = o;
                    }
                }
                let seat = seats(&land, &top);
                for (i, c) in land.cells.iter_mut().enumerate() {
                    let (from, o) = to[seat[i] as usize];
                    if from != NONE && c.plate == from {
                        c.plate = o;
                        c.switch = SWITCH_SPECK;
                        c.rock.margins |= ACCRETED;
                    }
                }
            }
            // 9. Piles spread and wear down, in their pieces: what stands
            // above a continent's thickness (a rift's thinned crust is not
            // filled again from its sides, version 11).
            let thick: Vec<i64> = land.cells.iter().map(|c| i64::from(c.rock.thick)).collect();
            let pile = |i: usize| thick[i].max(i64::from(CONTINENT_THICK));
            let spread: Vec<i64> = (0..land.cells.len())
                .map(|i| {
                    land.kin(sphere, i)
                        .filter(|&(j, _)| land.cells[j].plate == land.cells[i].plate)
                        .map(|(j, _)| (pile(j) - pile(i)) * THICK_SPREAD / 1000 / 8)
                        .sum()
                })
                .collect();
            // The piles wear down: what a continent loses above its
            // thickness goes, as silt, to build its plate's margins out
            // (no continental crust is ever lost).
            for (i, c) in land.cells.iter_mut().enumerate() {
                if c.dead {
                    continue;
                }
                c.rock.thick =
                    (thick[i] + spread[i]).clamp(i64::from(OCEAN_THICK), THICK_MAX) as u16;
                if c.rock.class != CONTINENT {
                    continue;
                }
                let worn =
                    (i64::from(c.rock.thick) - i64::from(CONTINENT_THICK)) * s.mountain_wear / 1000;
                if worn > 0 {
                    c.rock.thick -= worn as u16;
                    silt[c.plate as usize] += worn * i64::from(c.area);
                }
            }
            // The silt builds a piece out, at a free place of its own
            // lattice beside its crust: on the open coast first (not in a
            // narrow sea between two plates' continents), then, for a
            // plate with none, on any coast: over its own plate's floor
            // first, else (another plate's floor, a trench) a wedge.
            let mut shore: Vec<(bool, bool, usize, u64, u32, u32, usize)> = Vec::new();
            for i in 0..land.cells.len() {
                let c = land.cells[i];
                if c.dead || c.under || c.rock.class != CONTINENT {
                    continue;
                }
                for &(k, _) in &sphere.links[c.at as usize] {
                    if land.get(c.frame, k).is_some() {
                        continue;
                    }
                    let y = normalize(apply(
                        &land.rot[c.frame as usize],
                        sphere.centre[k as usize],
                    ));
                    let m = sphere.cell_of(y);
                    if rock[m].class != OCEAN || land.cover(sphere, (y, m), &key, NO_BIT).is_some()
                    {
                        continue;
                    }
                    let p = c.plate;
                    let alone = sphere.links[m].iter().all(|&(a, _)| {
                        std::iter::once(a)
                            .chain(sphere.links[a as usize].iter().map(|&(b, _)| b))
                            .all(|b| rock[b as usize].class == OCEAN || plate[b as usize] == p)
                    });
                    // A bay fills before a cape grows: the coast evens out.
                    let open = sphere.links[k as usize]
                        .iter()
                        .filter(|&&(a, _)| land.get(c.frame, a).is_none())
                        .count();
                    shore.push((
                        !alone,
                        plate[m] != p,
                        open,
                        hash(&[seed, P_SILT, t as u64, m as u64]),
                        c.frame,
                        k,
                        i,
                    ));
                }
            }
            shore.sort_unstable();
            shore.dedup_by_key(|x| (x.4, x.5));
            let mut built = 0;
            let mut open_coast = [false; MAXP];
            for &(narrow, _, _, _, frame, k, i) in &shore {
                let from = land.cells[i];
                let full = i64::from(sphere.area_km2[k as usize]);
                let need = SPILL_THICK * full;
                let u = from.plate as usize;
                if !narrow {
                    open_coast[u] = true;
                }
                if silt[u] < need || (narrow && open_coast[u]) || land.get(frame, k).is_some() {
                    continue;
                }
                let y = normalize(apply(&land.rot[frame as usize], sphere.centre[k as usize]));
                let m = sphere.cell_of(y);
                if land.cover(sphere, (y, m), &key, NO_BIT).is_some() {
                    continue;
                }
                let v = silt[u].min(i64::from(CONTINENT_THICK) * full);
                silt[u] -= v;
                built += full;
                let made_of = Rock {
                    thick: (v / full.max(1)) as u16,
                    // Sediment builds a shelf out.
                    high: lift(0, SHELF_M - i64::from(grain[m])),
                    cool: 0,
                    sunk: 0,
                    why: [0; CAUSES],
                    home: m as u32,
                    margins: 0,
                    arc: 0,
                    rift: 0,
                    orogeny: NEVER,
                    ..from.rock
                };
                let j = land.add(sphere, (frame, k), from.plate, made_of, next_bit);
                next_bit += 1;
                // The view: the cells whose centre the new crust covers.
                for q in std::iter::once(m).chain(sphere.links[m].iter().map(|&(a, _)| a as usize))
                {
                    if top[q] == NO_BIT
                        && land.cover(sphere, (sphere.centre[q], q), &key, NO_BIT) == Some(j)
                    {
                        top[q] = j as u32;
                        disp[q] = NO_BIT;
                    }
                }
            }
            paint(&land, &top, &mut plate, &mut rock);
            // A plate is one piece. A small part cut off from the rest of
            // its plate is taken by the plate it touches most; a large one
            // is a plate of its own from now on.
            let (part, list) = parts(sphere, &plate);
            // Parts the grid's view shows apart but whose continent is
            // one in its own lattice are one part (version 11): a
            // continent is not cut from its plate by the way the view
            // draws a neck of it.
            let mut one: Vec<u32> = (0..list.len() as u32).collect();
            let find = |one: &mut Vec<u32>, mut k: u32| {
                while one[k as usize] != k {
                    one[k as usize] = one[one[k as usize] as usize];
                    k = one[k as usize];
                }
                k
            };
            let seat = seats(&land, &top);
            for i in 0..land.cells.len() {
                let c = &land.cells[i];
                let a = part[seat[i] as usize];
                if c.dead || list[a as usize].0 != c.plate {
                    continue;
                }
                for (j, _) in land.kin(sphere, i) {
                    let b = part[seat[j] as usize];
                    if a != b && land.cells[j].plate == c.plate && list[b as usize].0 == c.plate {
                        let (ra, rb) = (find(&mut one, a), find(&mut one, b));
                        one[ra.max(rb) as usize] = ra.min(rb);
                    }
                }
            }
            let mut whole = vec![0_i64; list.len()];
            for k in 0..list.len() {
                let r = find(&mut one, k as u32) as usize;
                whole[r] += list[k].1;
            }
            let mut main = [(0_i64, u32::MAX); MAXP];
            for (k, &(id, _, _)) in list.iter().enumerate() {
                if one[k] == k as u32 && whole[k] > main[id as usize].0 {
                    main[id as usize] = (whole[k], k as u32);
                }
            }
            let most = |edge: &[u32; MAXP]| {
                (0..MAXP)
                    .filter(|&p| edge[p] > 0)
                    .max_by_key(|&p| (edge[p], std::cmp::Reverse(p)))
            };
            let mut to = vec![NONE; list.len()];
            let mut fresh_plate = [false; MAXP];
            for (k, &(id, km2, edge)) in list.iter().enumerate() {
                if main[id as usize].1 == find(&mut one, k as u32) {
                    continue;
                }
                let old = id as usize;
                let enclosed = edge.iter().filter(|&&n| n > 0).count() == 1 && km2 < ENCLOSED_KM2;
                if km2 >= PART_KM2 && !enclosed && next_id < MAXP {
                    let new = next_id;
                    next_id += 1;
                    to[k] = new as u8;
                    fresh_plate[new] = true;
                    for p in 0..MAXP {
                        parted[new][p] = parted[old][p];
                        parted[p][new] = parted[p][old];
                        broken[new][p] = broken[old][p];
                        broken[p][new] = broken[p][old];
                    }
                    parted[old][new] = true;
                    parted[new][old] = true;
                    born_t[new] = born_t[old];
                    group[new] = group[old];
                    omega[new] = omega[old];
                    cut_off += 1;
                } else if let Some(by) = most(&edge) {
                    to[k] = by as u8;
                    captures.push(Capture {
                        my: now_my,
                        plate: id,
                        by: by as u8,
                        km2,
                        whole: false,
                    });
                }
            }
            if to.iter().any(|&p| p != NONE) {
                for c in 0..CELLS {
                    if to[part[c] as usize] != NONE {
                        plate[c] = to[part[c] as usize];
                    }
                }
                for (i, b) in bits.iter_mut().enumerate() {
                    let k = part[drawn[i] as usize] as usize;
                    if to[k] != NONE && b.plate == list[k].0 {
                        b.plate = to[k];
                    }
                }
                let seat = seats(&land, &top);
                for (i, c) in land.cells.iter_mut().enumerate() {
                    let k = part[seat[i] as usize] as usize;
                    if to[k] != NONE && c.plate == list[k].0 {
                        c.plate = to[k];
                        c.switch = if fresh_plate[to[k] as usize] {
                            SWITCH_CUT_OFF
                        } else {
                            SWITCH_CAPTURE
                        };
                    }
                }
            }
            // A crumb of a plate is taken by the plate it touches most, and
            // a small plate enclosed in one other plate by that plate.
            // (A plate is counted whole: its parts joined through their
            // continent's own lattice are one.)
            let (_, list) = parts(sphere, &plate);
            let mut whole: Vec<Part> = Vec::new();
            for &(id, km2, edge) in &list {
                if let Some(w) = whole.iter_mut().find(|w| w.0 == id) {
                    w.1 += km2;
                    for p in 0..MAXP {
                        w.2[p] += edge[p];
                    }
                } else {
                    whole.push((id, km2, edge));
                }
            }
            let mut gone = [false; MAXP];
            for &(id, km2, edge) in &whole {
                let around = (0..MAXP).filter(|&p| edge[p] > 0).count();
                let Some(by) = most(&edge) else {
                    continue;
                };
                let by = alias_of(&alias, by as u8) as usize;
                let enclosed = around == 1 && km2 < ENCLOSED_KM2;
                if (km2 >= CRUMB_KM2 && !enclosed) || gone[by] || by == id as usize {
                    continue;
                }
                gone[id as usize] = true;
                absorb(
                    (by, id as usize, SWITCH_CRUMB),
                    &mut plate,
                    (&mut bits, &mut land),
                    &mut alias,
                    &mut parted,
                    &mut broken,
                    &mut contact,
                );
                omega[id as usize] = [0; 3];
                captures.push(Capture {
                    my: now_my,
                    plate: id,
                    by: by as u8,
                    km2,
                    whole: true,
                });
            }
            built_km2.push(built);
            sync(&mut bits, &plate, &rock, &disp);
            paint(&land, &top, &mut plate, &mut rock);
            let mut cells_of = [0_i64; MAXP];
            for c in 0..CELLS {
                cells_of[plate[c] as usize] += 1;
            }
            cont_of = land.weight();
            // Plates whose continents no longer touch have parted.
            let mut touching = vec![[false; MAXP]; MAXP];
            for c in 0..CELLS {
                if rock[c].class == OCEAN {
                    continue;
                }
                for &(m, _) in &sphere.links[c] {
                    let m = m as usize;
                    if rock[m].class != OCEAN && plate[m] != plate[c] {
                        touching[plate[c] as usize][plate[m] as usize] = true;
                    }
                }
            }
            for a in 0..MAXP {
                for b in 0..MAXP {
                    if !touching[a][b] && !touching[b][a] {
                        parted[a][b] = true;
                    }
                }
            }
            // 10. The hotspots mark the crust passing over them, a mark a
            // million years, carried with it from then on.
            for (k, &hp) in hotspots.iter().enumerate() {
                let c = sphere.cell_of(hp);
                let above = land.cover(sphere, (hp, c), &key, NO_BIT);
                let first = match above {
                    Some(j) => (true, j as u32, land.cells[j].plate),
                    None if disp[c] != NO_BIT => (false, disp[c], bits[disp[c] as usize].plate),
                    None => continue,
                };
                // A plume lifts the continent over it and floods it a little.
                if let Some(j) = above {
                    let around: Vec<usize> = land.kin(sphere, j).map(|(i, _)| i).collect();
                    land.cells[j].rock.rise(PLUME_UP_M, BY_PLUME);
                    for i in around {
                        land.cells[i].rock.rise(PLUME_UP_M / 3, BY_PLUME);
                    }
                }
                let p = first.2;
                let w = moved[p as usize];
                let speed = norm(w);
                for j in 0..s.step_my {
                    let pos = if speed < 50 || j == 0 {
                        hp
                    } else {
                        normalize(apply(
                            &rotation(normalize(w), speed * j * ONE / (1000 * RADIUS_KM)),
                            hp,
                        ))
                    };
                    let cj = sphere.cell_of(pos);
                    let (on_land, bit) = match land.cover(sphere, (pos, cj), &key, NO_BIT) {
                        Some(i) if land.cells[i].plate == p => (true, i as u32),
                        _ if disp[cj] != NO_BIT && bits[disp[cj] as usize].plate == p => {
                            (false, disp[cj])
                        }
                        _ => (first.0, first.1),
                    };
                    marks.push(Mark {
                        land: on_land,
                        bit,
                        pos,
                        spot: k as u8,
                        my: now_my + j as u16,
                    });
                }
            }
            sync(&mut bits, &plate, &rock, &disp);
            // The floor sunk and the crust piled leave the record.
            let mut new_ix = vec![NO_BIT; bits.len()];
            let mut kept = Vec::with_capacity(bits.len());
            for (i, b) in bits.iter().enumerate() {
                if !dead[i] {
                    new_ix[i] = kept.len() as u32;
                    kept.push(*b);
                }
            }
            bits = kept;
            for d in &mut disp {
                if *d != NO_BIT {
                    *d = new_ix[*d as usize];
                }
            }
            let land_ix = land.compact();
            for tr in &mut troughs {
                tr.path = tr
                    .path
                    .iter()
                    .map(|&i| land_ix[i as usize])
                    .filter(|&i| i != NO_BIT)
                    .collect();
            }
            for d in &mut top {
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
            for &d in &top {
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
            hidden_now = hidden;
            shortened_km2.push(shortened);
            made_km2.push(made);
            for push in &mut pushes {
                push.strength = push.strength * PUSH_KEEP / 1000;
            }
            // A rift stretches and thins the crust along it: it sinks at
            // once, and as much again while it cools.
            let torn: Vec<usize> = (0..land.cells.len())
                .filter(|&i| {
                    let c = &land.cells[i];
                    c.rock.margins & RIFT != 0 && !c.rifted
                })
                .collect();
            if !torn.is_empty() {
                let d = land.spread(sphere, &torn, |_| true, STRETCH_KM);
                for (i, c) in land.cells.iter_mut().enumerate() {
                    if d[i] == u32::MAX {
                        continue;
                    }
                    let r = &mut c.rock;
                    let due = STRETCH_M * bump(i64::from(d[i]), i64::from(STRETCH_KM)) / 1000;
                    let more = due - i64::from(r.sunk);
                    if more > 0 {
                        r.rise(-more, BY_STRETCH);
                        r.cool = lift(r.cool, more);
                        r.sunk = lift(r.sunk, more);
                    }
                }
            }
        } else {
            for c in &mut land.cells {
                c.shown = true;
            }
        }
        // 12. What the plates do now, and the forces for the next step:
        // read on the grid's view of the pieces.
        paint(&land, &top, &mut plate, &mut rock);
        for q in 0..CELLS {
            event[q] = if top[q] == NO_BIT {
                Margin::Inland as u8
            } else {
                land.cells[top[q] as usize].event
            };
        }
        let seen = look(
            sphere,
            &plate,
            &group,
            &mut rock,
            &omega,
            &event,
            (now_my, s.slab_age_my),
        );
        // Each cell of crust's margin: what the view saw where it draws
        // it; a cell the view does not draw has its piece's margin there.
        let mut margin_of = vec![u8::MAX; land.cells.len()];
        for q in 0..CELLS {
            if top[q] == NO_BIT {
                continue;
            }
            let i = top[q] as usize;
            land.cells[i].rock.margins |= rock[q].margins & (PASSIVE | ACTIVE);
            if margin_of[i] == u8::MAX || seen.margin[q] == Margin::Active as u8 {
                margin_of[i] = seen.margin[q];
            }
        }
        for i in 0..land.cells.len() {
            if margin_of[i] != u8::MAX {
                continue;
            }
            let c = land.cells[i];
            let q = c.cell as usize;
            margin_of[i] = Margin::Inland as u8;
            if top[q] != NO_BIT && !c.under {
                let o = land.cells[top[q] as usize];
                if o.frame == c.frame && o.plate == c.plate {
                    margin_of[i] = seen.margin[q];
                    land.cells[i].rock.margins |= o.rock.margins & (PASSIVE | ACTIVE);
                }
            }
        }
        // The mantle under the crust as it now lies.
        if t > 0 {
            warm(sphere, &rock, &seen.slab, &deep, &mut heat, HEAT_RATE);
        }
        out_heat.push(
            heat.iter()
                .map(|&v| v.clamp(-30_000, 30_000) as i16)
                .collect::<Vec<i16>>(),
        );
        for (i, c) in land.cells.iter_mut().enumerate() {
            if margin_of[i] == Margin::Active as u8 {
                c.rock.orogeny = now_my;
            }
        }
        if t > 0 {
            // The sea, the base level of wear and of filling: the level
            // that leaves the settings' share of land dry now.
            let whole: Vec<i64> = (0..CELLS)
                .map(|c| {
                    if rock[c].class == OCEAN {
                        i64::from(i16::MIN)
                    } else {
                        i64::from(rock[c].high)
                            + i64::from(grain[rock[c].home as usize])
                            + (i64::from(rock[c].thick) - i64::from(CONTINENT_THICK)).max(0)
                                * ISOSTASY_M
                                / 10
                    }
                })
                .collect();
            let sea = quantile(sphere, &whole, 1000 - s.land_per_mille);
            weather(sphere, &mut land, &margin_of, &grain, sea);
        } else {
            // Heights are counted from the sea of the start, the one
            // that leaves the settings' share of land dry.
            let whole: Vec<i64> = (0..CELLS)
                .map(|c| {
                    if rock[c].class == CONTINENT {
                        i64::from(rock[c].high) + i64::from(grain[c])
                    } else {
                        i64::from(i16::MIN)
                    }
                })
                .collect();
            let sea = quantile(sphere, &whole, 1000 - s.land_per_mille);
            for c in &mut land.cells {
                if c.rock.class == CONTINENT {
                    c.rock.high = lift(c.rock.high, -sea);
                }
            }
        }
        paint(&land, &top, &mut plate, &mut rock);
        sync(&mut bits, &plate, &rock, &disp);
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
        let grips: Vec<Grip> = (0..MAXP)
            .flat_map(|a| (a + 1..MAXP).map(move |b| (a, b)))
            .filter(|&(a, b)| contact[a][b] != 0)
            .map(|(a, b)| (a as u8, b as u8, front_of[a][b]))
            .collect();
        fronts.clone_from(&seen.front);
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
        out_kind.push(seen.kind);
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
        out_pieces.push(land.pieces(&group, &floor_born(&plate, &rock)));
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
        out_hidden.push(hidden_now);
        out_marks.push(
            marks
                .iter()
                .map(|m| (m.pos, m.spot, m.my))
                .collect::<Vec<(V3, u8, u16)>>(),
        );
        out_margin.push(seen.margin);
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
        if t < s.steps {
            let next = drive(
                sphere,
                s,
                &plate,
                &rock,
                &pushes,
                &alias,
                &heat,
                Edges {
                    slab: &seen.slab,
                    suction: &seen.suction,
                    ridge: &seen.ridge,
                    grips: &grips,
                },
                Some(&omega),
                &held(&parted),
            );
            omega = next.omega;
            group = next.group;
            // A rift's ocean is open once its two shores touch nowhere.
            for r in &mut rifts {
                if r.cut_my != NEVER && r.parted_my == NEVER {
                    let (a, b) = (alias_of(&alias, r.plate), alias_of(&alias, r.new));
                    if a != b && parted[a as usize][b as usize] {
                        r.parted_my = now_my;
                    }
                }
            }
            opened = opening(sphere, &plate, &rock, &parted, &group, &mut broken);
            for &pair in &opened {
                for (at, km2) in flood(sphere, &mut land, &top, pair, &plumes, &mut spent) {
                    lips.push(Lip {
                        my: now_my,
                        at,
                        km2,
                    });
                }
            }
        }
        edges_last = (seen.slab, seen.suction, seen.ridge);
    }
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
        plates_at_start: (kc, seeds.len()),
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
        hotspots: hotspots.to_vec(),
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
