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
//! 3. **Forces, not scripts.** A plate's turn at each step comes from the
//!    forces on its cells, summed as a torque and slowed by continental
//!    keels: the upwelling under the supercontinent pushes its plates
//!    away from the rift axis (fading); later rifts push their two halves
//!    apart; downwellings draw; ridges push; and slab pull, the strongest,
//!    draws a plate toward every trench where it sinks, so plates with
//!    long subducting edges move fastest.
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
//! 7. **Rifts.** Later, the plate carrying the most continent may split,
//!    along its old seams where it has them, its halves parting at once
//!    at a few cm a year; if they are held together again before they
//!    have parted, the rift is aborted (a failed rift stays in the crust).
//!    Where a plume stands near a boundary that opens, the continent is
//!    flooded with basalt.
//! 8. **Hotspots** mark the crust passing over them, a mark a million
//!    years, carried with the crust: their chains bend as the plates
//!    turned, and are as old as the history.
//!
//! Moving the crust (version 5): the crust is a set of bits at their exact
//! places; each step every bit turns with its plate, so the turns add up
//! and a slow plate drifts as far as its speed says. The grid only draws
//! them: each cell shows one bit (the winner where several meet), a bit
//! shows in one cell at most, and nothing is copied. Today's plates are
//! the plates of the last step.

use crate::{
    CELLS, Kind, ONE, RADIUS_KM, Sphere, TAU, V3, add, classify, cos, cross, dot, draw, east_at,
    fbm3, hash, km_between, latitude, norm, normalize, north_at, quantile, random_unit, scale, sin,
    step, sub, warp,
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
    /// Share left dry by the sea, per mille (the lead's 15–20 %).
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
    /// Plates the supercontinent lies on at the start: it breaks along
    /// their borders.
    pub continent_plates: (i64, i64),
    /// How unequal those plates are, [`ONE`] units of the largest seed
    /// weight (0: all alike).
    pub plate_inequality: i64,
    /// Oceanic plates at the start, around the supercontinent.
    pub ocean_plates: (i64, i64),
    /// How far the first rift axis may turn from north–south, tenths of a
    /// degree.
    pub rift_turn: i64,
    /// Steps a later rift waits after the start; a plate younger than
    /// twice this does not weld.
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
    /// A later rift's halves part at this speed, mm a year.
    pub rift_open_mm: i64,
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
    /// The upwelling's push under the supercontinent, mm a year at
    /// [`EXPANSION_KM`] from the rift axis (later rifts push at
    /// `rift_open_mm`).
    pub upwelling_push: i64,
    /// A push keeps this much of its strength per step, per mille.
    pub push_keep: i64,
    /// Downwellings, where cold mantle sinks and draws the plates.
    pub downwellings: (i64, i64),
    /// A downwelling's draw on continents, mm a year near it (fading to 0
    /// at [`DOWN_REACH_KM`]): cold sinking mantle gathers them.
    pub downwelling_draw: i64,
    /// Slab pull, the strongest force: mm a year per trench cell, shared
    /// by all the plate's cells (a plate's speed is about this times its
    /// trench cells over its cells).
    pub slab_pull: i64,
    /// Ridge push, mm a year per ridge cell, shared the same way.
    pub ridge_push: i64,
    /// How much a continental keel slows its plate, per mille of drag
    /// added for a plate all continent.
    pub continent_drag: i64,
    /// The pole-fleeing force (Eötvös, « Polflucht ») on continents, mm a
    /// year at 45°: the spinning Earth's bulge nudges them toward the
    /// equator.
    pub pole_flight: i64,
    /// Later rifts: the plate carrying the most continent splits.
    pub later_rifts: (i64, i64),
    /// A plate splits only if it carries this much continent, km².
    pub rift_min_km2: i64,
    /// Steps of subduction under an ocean's edge before its island arc
    /// rises (a third of its cells becomes arc crust).
    pub arc_steps: i64,
    /// Terranes waiting in the ocean to be accreted.
    pub terranes: (i64, i64),
    /// A terrane's length, cells of ~100 km.
    pub terrane_cells: (i64, i64),
    /// Hotspots.
    pub hotspots: (i64, i64),
    /// The upwelling and downwellings stay within this latitude, tenths of
    /// a degree.
    pub mantle_latitude: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            radius_km: RADIUS_KM,
            land_per_mille: 175,
            crust_per_mille: 340,
            supercontinent_warp: 1000,
            supercontinent_fray: 1000,
            supercontinent_latitude: 150,
            supercontinent_flatten: 2200,
            continent_plates: (14, 18),
            plate_inequality: 100_000,
            ocean_plates: (5, 8),
            rift_turn: 300,
            rift_steps: 4,
            steps: 40,
            step_my: 5,
            speed_mm: (10, 250),
            mantle_drag_mm: 150,
            rift_open_mm: 30,
            collision_resist: 3,
            weld_stop_mm: 5,
            upwelling_push: 150,
            push_keep: 960,
            downwellings: (2, 3),
            downwelling_draw: 12,
            slab_pull: 7_500,
            ridge_push: 300,
            continent_drag: 800,
            pole_flight: 35,
            later_rifts: (2, 4),
            rift_min_km2: 12_000_000,
            arc_steps: 4,
            terranes: (18, 28),
            terrane_cells: (1, 5),
            hotspots: (14, 20),
            mantle_latitude: 350,
        }
    }
}

/// No piece (oceanic crust), no plate.
pub const NONE: u8 = u8::MAX;
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
}

/// A later rift.
#[derive(Debug, Clone, Copy)]
pub struct Rift {
    /// When, millions of years ago.
    pub my: u16,
    /// The plate that split, and the new one.
    pub plate: u8,
    /// See `plate`.
    pub new: u8,
    /// The continent's middle where it split.
    pub centre: V3,
    /// The continent the plate carried, km².
    pub area_km2: i64,
    /// The halves welded again before parting.
    pub aborted: bool,
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
    /// The upwelling that broke the supercontinent.
    pub upwelling: V3,
    /// The normal of its rift axis (a great circle through the upwelling).
    pub rift_normal: V3,
    /// The downwellings that draw.
    pub downwellings: Vec<V3>,
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
    /// The hotspots, fixed under the plates.
    pub hotspots: Vec<V3>,
    /// Per step, the hotspots' marks on the crust: where, which hotspot,
    /// and when it was made, millions of years ago.
    pub marks_at: Vec<Vec<(V3, u8, u16)>>,
    /// Per step, each plate id's motion (ids without cells turn by 0).
    pub motion: Vec<Vec<Motion>>,
    /// Per step, the continental crust's area, km².
    pub continental_km2: Vec<i64>,
    /// Per step, continental crust lost to collisions since the step
    /// before, km² (shortening).
    pub shortened_km2: Vec<i64>,
    /// Per step, island-arc crust made since the step before, km².
    pub made_km2: Vec<i64>,
    /// The later rifts.
    pub rifts: Vec<Rift>,
    /// The welds, in order.
    pub welds: Vec<Weld>,
    /// The accretions, in order.
    pub accretions: Vec<Accretion>,
    /// The collisions, in order of first contact.
    pub collisions: Vec<Collision>,
    /// The blocks escaped sideways.
    pub escapes: Vec<Escape>,
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
const P_DOWN: u64 = 0x0c50_0104;
const P_MOVE: u64 = 0x0c50_0105;
const P_TERRANE: u64 = 0x0c50_0106;
const P_AGE: u64 = 0x0c50_0107;
const P_OCEAN: u64 = 0x0c50_0108;
const P_RIFT: u64 = 0x0c50_0109;
const P_ARC: u64 = 0x0c50_010a;
const P_SINK: u64 = 0x0c50_010c;
const P_ESCAPE: u64 = 0x0c50_010b;

/// The supercontinent's default warp and fray: an irregular cap, not a
/// disc (scaled by [`Settings::supercontinent_warp`] and `_fray`).
const SUPER_WARP: [(i64, i64); 2] = [(700_000, 260_000), (250_000, 80_000)];
const SUPER_FRAY: [(i64, i64); 3] = [(600_000, 4), (250_000, 2), (100_000, 1)];
/// How much the default fray moves the coast, the cap's cosine units (‰).
const FRAY: i64 = 600;
/// The plates' borders' warp: wandering edges, not a Voronoi's lines.
const RIFT_WARP: [(i64, i64); 3] = [(400_000, 140_000), (150_000, 45_000), (60_000, 15_000)];
/// A downwelling is at least this far from the upwelling, km.
const DOWN_FROM_UP_KM: i64 = 10_000;
/// The upwelling's push grows with the distance from the rift axis up to
/// this, km: the opening is an expansion, every rift widens.
pub const EXPANSION_KM: i64 = 5_000;
/// The upwelling pushes this far from it (straight line), km.
const UP_REACH_KM: i64 = 8_000;
/// A later rift's push: its expansion and reach, km.
const RIFT_EXPANSION_KM: i64 = 1_500;
const RIFT_REACH_KM: i64 = 3_500;
/// A downwelling draws this far (straight line), km, and less within
/// [`DOWN_NEAR_KM`] (the plates converge there, they do not orbit it).
pub const DOWN_REACH_KM: i64 = 9_000;
const DOWN_NEAR_KM: i64 = 1_000;
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
const OCEAN_START_KM_MY: i64 = 50;
/// The older sutures' last mountain building, My ago.
const OLD_OROGENY_MY: (i64, i64) = (300, 700);
/// A plate keeps this much of its last motion each step, per mille (the
/// rest comes from the forces of the step).
const INERTIA: i64 = 400;
/// Convergence or divergence a slab or a ridge needs, µm a year.
const FORCE_LEAST_UM: i64 = 2_000;
/// Share of an arc's riding edge that becomes arc crust, per mille.
const ARC_SHARE: u64 = 200;
/// An upwelling's push outward from its centre, per mille of its push
/// away from its axis.
const RADIAL: i64 = 500;
/// The radial push's north–south part, per mille: the supercontinent
/// opens mostly east–west, as the Atlantic did.
const RADIAL_NORTH: i64 = 300;

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
}

/// A bit of crust where it really is. Each step it turns with its plate,
/// exactly, so a slow plate drifts as far as its speed says; the grid only
/// draws the bits (a cell shows one bit, a bit shows in one cell at most).
#[derive(Debug, Clone, Copy)]
struct Bit {
    pos: V3,
    plate: u8,
    /// The area it stands for, km² (its cell's at birth).
    area: u32,
    rock: Rock,
}

/// Where a hotspot stood under the crust, carried with the bit it marked.
#[derive(Debug, Clone, Copy)]
struct Mark {
    bit: u32,
    pos: V3,
    spot: u8,
    my: u16,
}

/// A push from the mantle: away from a great circle (`normal`), growing
/// with the distance from it up to `expansion_km`, within `reach_km` of
/// `centre`; on two plates only, for a later rift or an escaping block.
#[derive(Debug, Clone, Copy)]
struct Push {
    centre: V3,
    normal: V3,
    reach_km: i64,
    expansion_km: i64,
    /// µm a year.
    strength: i64,
    plates: Option<(u8, u8)>,
}

/// A cell where a force acts on its plate, and the force's direction.
type Spot = (usize, u8, V3);

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

/// Of a continent's crust run under another, the share that sinks into
/// the mantle each step, ‰ (its area gone, its thickness on the pile);
/// the rest stays under the pile, its own, unseen, and comes back if the
/// two part. All of it sinking took a third of the continents in 200 My.
const STACK_SINK: u64 = 500;
/// Each step, a pile spreads this much toward its neighbours, and its
/// excess over a continent's thickness keeps this much (erosion), ‰.
const THICK_SPREAD: i64 = 400;
const THICK_KEEP: i64 = 970;
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
/// A later rift's line: seams this near count, within this of its middle;
/// its halves grow from this far on each side and stop at the seams, km.
const RIFT_SEAM_KM: i64 = 250;
const RIFT_SPAN_KM: i64 = 3_000;
const RIFT_SIDE_KM: i64 = 500;

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
    downwellings: &[V3],
    (slab, ridge, grips): (&[Spot], &[Spot], &[Grip]),
    prev: Option<&[V3]>,
    held: &[[bool; MAXP]],
) -> Drive {
    let mut torque = vec![[0_i64; 3]; MAXP];
    let mut cells = [0_i64; MAXP];
    let mut cont = [0_i64; MAXP];
    let pushes: Vec<Push> = pushes
        .iter()
        .filter(|p| p.strength > 0)
        .filter_map(|p| match p.plates {
            None => Some(*p),
            Some((a, b)) => {
                let (a, b) = (alias_of(alias, a), alias_of(alias, b));
                (a != b).then_some(Push {
                    plates: Some((a, b)),
                    ..*p
                })
            }
        })
        .collect();
    for c in 0..CELLS {
        let p = plate[c];
        let x = sphere.centre[c];
        cells[p as usize] += 1;
        if rock[c].class != OCEAN {
            cont[p as usize] += 1;
        }
        let mut f = [0_i64; 3];
        for push in &pushes {
            if push.plates.is_some_and(|(a, b)| p != a && p != b) {
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
            if push.plates.is_none() {
                // And outward from the upwelling itself: the pieces along
                // the axis part too.
                let out = tangent(x, push.centre).map(|v| -v);
                let out = normalize([out[0], out[1], out[2] * RADIAL_NORTH / 1000]);
                let radial = (d * 1000 / push.expansion_km).min(1000);
                f = add(
                    f,
                    scale(
                        out,
                        push.strength * RADIAL / 1000 * radial / 1000 * fade / 1000,
                    ),
                );
            }
        }
        if rock[c].class != OCEAN && s.pole_flight != 0 {
            let z = x[2];
            let across = (ONE * ONE - z * z).max(0).isqrt();
            let m = s.pole_flight * 1000 * 2 * z / ONE * across / ONE;
            let dir = normalize(tangent(x, [0, 0, -ONE]));
            f = add(f, scale(dir, m));
        }
        for &dw in downwellings {
            if rock[c].class == OCEAN {
                continue;
            }
            let d = km_between(x, dw);
            if d >= DOWN_REACH_KM || d == 0 {
                continue;
            }
            let near = (d * 1000 / DOWN_NEAR_KM).min(1000);
            let dir = normalize(tangent(x, dw));
            let m = s.downwelling_draw * 1000 * (DOWN_REACH_KM - d) / DOWN_REACH_KM * near / 1000;
            f = add(f, scale(dir, m));
        }
        torque[p as usize] = add(torque[p as usize], cross(x, f));
    }
    for (spots, strength) in [(slab, s.slab_pull), (ridge, s.ridge_push)] {
        for &(c, p, dir) in spots {
            let f = scale(dir, strength * 1000);
            torque[p as usize] = add(torque[p as usize], cross(sphere.centre[c], f));
        }
    }
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
    for &(p, o, cells) in grips {
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
) -> Look {
    let floor = floor_born(plate, rock);
    let mut kind = vec![0_u8; CELLS];
    let mut margin = vec![Margin::Inland as u8; CELLS];
    let mut slab = Vec::new();
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
                slab.push((c, mine, normal));
            }
            if !mine_cont && closing <= -FORCE_LEAST_UM && -2 * closing >= slide {
                ridge.push((c, mine, normal.map(|v| -v)));
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
        ridge,
        front,
    }
}

/// The pairs that may be held together: continents that have not parted,
/// but not a young rift's halves (a rift opens for a while).
fn held(
    parted: &[[bool; MAXP]; MAXP],
    fresh: &[(u8, u8, usize)],
    t: usize,
    young: usize,
) -> Vec<[bool; MAXP]> {
    let mut out: Vec<[bool; MAXP]> = parted.iter().map(|row| row.map(|p| !p)).collect();
    for &(a, b, at) in fresh {
        if t < at + young {
            out[a as usize][b as usize] = false;
            out[b as usize][a as usize] = false;
        }
    }
    out
}

/// The shown rocks and plates written back to their bits.
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
/// `pair`: the continent around takes the [`LIP`] flag. Each plume that
/// floods (once in the history), and the area flooded, km².
fn flood(
    sphere: &Sphere,
    plate: &[u8],
    rock: &mut [Rock],
    pair: (u8, u8),
    plumes: &[V3],
    spent: &mut [bool],
) -> Vec<(V3, i64)> {
    let ok: Vec<bool> = (0..CELLS)
        .map(|c| rock[c].class != OCEAN && (plate[c] == pair.0 || plate[c] == pair.1))
        .collect();
    let edge: Vec<usize> = (0..CELLS)
        .filter(|&c| {
            ok[c]
                && sphere.links[c].iter().any(|&(n, _)| {
                    let n = n as usize;
                    ok[n] && plate[n] != plate[c]
                })
        })
        .collect();
    let mut out = Vec::new();
    for (k, &plume) in plumes.iter().enumerate() {
        if spent[k] {
            continue;
        }
        let near: Vec<(usize, u32)> = edge
            .iter()
            .filter(|&&c| km_between(sphere.centre[c], plume) < LIP_REACH_KM)
            .map(|&c| (c, 0))
            .collect();
        if near.is_empty() {
            continue;
        }
        let (d, _) = sphere.spread(&near, |_, b| ok[b], LIP_KM as u32);
        let mut km2 = 0;
        for c in 0..CELLS {
            if d[c] != u32::MAX {
                rock[c].margins |= LIP;
                km2 += i64::from(sphere.area_km2[c]);
            }
        }
        spent[k] = true;
        out.push((plume, km2));
    }
    out
}

/// The history of the planet born from `seed`, with its hotspots.
pub fn run(
    seed: u64,
    s: &Settings,
    sphere: &Sphere,
    old_suture: &[bool],
    hotspots: &[V3],
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
    // The mantle: the upwelling's rift axis, the downwellings.
    let axis_turn = draw(seed, P_MOVE, 999, -s.rift_turn, s.rift_turn) * TAU / 3600;
    let (east, north) = (east_at(upwelling), north_at(upwelling, east_at(upwelling)));
    let rift_normal = normalize(add(
        scale(east, cos(axis_turn)),
        scale(north, sin(axis_turn)),
    ));
    let downs = draw(seed, P_DOWN, 0, s.downwellings.0, s.downwellings.1) as u64;
    let downwellings: Vec<V3> = (0..downs)
        .map(|k| {
            (0..TRIES)
                .map(|t| random_unit(seed, P_DOWN + 1, k * 10_000 + t))
                .find(|&p| {
                    latitude(p).abs() <= s.mantle_latitude
                        && km_between(p, upwelling) >= DOWN_FROM_UP_KM
                })
                .expect("a point far from the upwelling")
        })
        .collect();
    let mut pushes = vec![Push {
        centre: upwelling,
        normal: rift_normal,
        reach_km: UP_REACH_KM,
        expansion_km: EXPANSION_KM,
        strength: s.upwelling_push * 1000,
        plates: None,
    }];
    // The plumes that may flood a breaking continent: the hotspots and the
    // upwelling itself.
    let mut plumes: Vec<V3> = hotspots.to_vec();
    plumes.push(upwelling);
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
    let mut rift_when: Vec<usize> = {
        let n = draw(seed, P_RIFT, 0, s.later_rifts.0, s.later_rifts.1).max(0) as u64;
        let (lo, hi) = (s.rift_steps as i64 + 3, s.steps as i64 - 3);
        (0..n)
            .filter(|_| lo <= hi)
            .map(|k| draw(seed, P_RIFT, 1 + k, lo, hi) as usize)
            .collect()
    };
    rift_when.sort_unstable();
    let mut rifts: Vec<Rift> = Vec::new();
    let mut welds: Vec<Weld> = Vec::new();
    let mut accretions: Vec<Accretion> = Vec::new();
    let mut collisions: Vec<Collision> = Vec::new();
    let mut escapes: Vec<Escape> = Vec::new();
    let mut lips: Vec<Lip> = Vec::new();
    // Collisions in progress: the step of first contact (+1), the mean
    // closing seen last, µm a year, and the front's cells.
    let mut contact = vec![[0_usize; MAXP]; MAXP];
    let mut closing_of = vec![[0_i64; MAXP]; MAXP];
    // Steps each collision has stood still, and the young rifts.
    let mut quiet = vec![[0_usize; MAXP]; MAXP];
    let mut fresh: Vec<(u8, u8, usize)> = Vec::new();
    let mut front_of = vec![[0_i64; MAXP]; MAXP];
    let mut record_of = vec![[usize::MAX; MAXP]; MAXP];
    let mut escaped = vec![[false; MAXP]; MAXP];
    let mut shed = [false; MAXP];
    let young = 2 * s.rift_steps;
    let arc_steps = s.arc_steps.clamp(1, 250) as u8;
    // The crust, bit by bit, and which bit each cell shows.
    let mut bits: Vec<Bit> = (0..CELLS)
        .map(|c| Bit {
            pos: sphere.centre[c],
            plate: plate[c],
            area: sphere.area_km2[c],
            rock: rock[c],
        })
        .collect();
    let mut disp: Vec<u32> = (0..CELLS as u32).collect();
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
    let mut out_marks = Vec::with_capacity(s.steps + 1);
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
        &downwellings,
        (&[], &[], &[]),
        None,
        &held(&parted, &fresh, 0, young),
    );
    let mut omega = first.omega;
    let mut group = first.group;
    let mut opened = opening(sphere, &plate, &rock, &parted, &group, &mut broken);
    for &pair in &opened {
        for (at, km2) in flood(sphere, &plate, &mut rock, pair, &plumes, &mut spent) {
            lips.push(Lip {
                my: total_my as u16,
                at,
                km2,
            });
        }
    }
    sync(&mut bits, &plate, &rock, &disp);
    let mut event = vec![Margin::Inland as u8; CELLS];
    let mut fronts: Vec<Front> = Vec::new();
    let mut last_kind = vec![0_u8; CELLS];
    for t in 0..=s.steps {
        let now_my = ((s.steps - t) as i64 * s.step_my) as u16;
        if t > 0 {
            let mut cont_of = [0_i64; MAXP];
            let mut cells_of = [0_i64; MAXP];
            for c in 0..CELLS {
                cells_of[plate[c] as usize] += 1;
                if rock[c].class != OCEAN {
                    cont_of[plate[c] as usize] += area(c);
                }
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
                        for p in &mut plate {
                            if *p as usize == absorbed {
                                *p = kept as u8;
                            }
                        }
                        for bit in &mut bits {
                            if bit.plate as usize == absorbed {
                                bit.plate = kept as u8;
                            }
                        }
                        let (nk, nx) = (cells_of[kept].max(1), cells_of[absorbed]);
                        omega[kept] = [0, 1, 2]
                            .map(|i| (omega[kept][i] * nk + omega[absorbed][i] * nx) / (nk + nx));
                        omega[absorbed] = [0; 3];
                        cells_of[kept] += cells_of[absorbed];
                        cont_of[kept] += cont_of[absorbed];
                        cells_of[absorbed] = 0;
                        cont_of[absorbed] = 0;
                        alias[absorbed] = kept as u8;
                        for o in 0..MAXP {
                            parted[kept][o] &= parted[absorbed][o];
                            parted[o][kept] = parted[kept][o];
                            broken[kept][o] |= broken[absorbed][o];
                            broken[o][kept] = broken[kept][o];
                            contact[absorbed][o] = 0;
                            contact[o][absorbed] = 0;
                        }
                        contact[a][b] = 0;
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
                        if km2 < 200_000 || 3 * km2 > cont_of[over] {
                            continue;
                        }
                        let new = next_id;
                        next_id += 1;
                        for c in 0..CELLS {
                            if block[c] {
                                plate[c] = new as u8;
                            }
                        }
                        for bit in &mut bits {
                            if bit.plate as usize == over && block[sphere.cell_of(bit.pos)] {
                                bit.plate = new as u8;
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
                            plates: Some((over as u8, new as u8)),
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
            // 4. The plates turn, carrying their crust: every bit moves
            // exactly, from where it was, not from a cell.
            let moved = omega.clone();
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
                m.pos = normalize(apply(&turn[bits[m.bit as usize].plate as usize], m.pos));
            }
            for b in &mut bits {
                b.pos = normalize(apply(&turn[b.plate as usize], b.pos));
            }
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
            let rank = |class: u8| match class {
                CONTINENT => 3,
                TERRANE => 2,
                _ => 1,
            };
            let key = |p: u8, class: u8| {
                let q = i64::from(p);
                match class {
                    CONTINENT => (3, cont_of[p as usize], -q),
                    TERRANE => (2, cont_of[p as usize], -q),
                    _ => (1, -floor[p as usize], -q),
                }
            };
            let ride = |p: u8| (-floor[p as usize], -i64::from(p));
            let mut dead = vec![false; n];
            // Where continents collide (a collision's front last step, or
            // next to it), a continent's bit run under the other piles on it.
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
            event.fill(Margin::Inland as u8);
            let mut accrete: Vec<(usize, u8, u8, u8)> = Vec::new();
            let mut shortened = 0;
            let mut made = 0;
            // 5. Each cell shows its winning bit: continental crust never
            // sinks, the older ocean sinks under the younger. A sunk bit is
            // gone; a continent's bit run under another continent piles on
            // it (the crust thickens) and half of it sinks; a terrane docks.
            for q in 0..CELLS {
                let here = &order[start[q] as usize..start[q + 1] as usize];
                if here.is_empty() {
                    pending[q] = true;
                    disp[q] = NO_BIT;
                    continue;
                }
                let x = sphere.centre[q];
                let mut pw = NONE;
                let mut kw = (i64::MIN, 0, 0);
                for &i in here {
                    let b = &bits[i as usize];
                    let k = key(b.plate, b.rock.class);
                    if k > kw {
                        kw = k;
                        pw = b.plate;
                    }
                }
                let iw = here
                    .iter()
                    .copied()
                    .filter(|&i| bits[i as usize].plate == pw)
                    .max_by_key(|&i| {
                        let b = &bits[i as usize];
                        (rank(b.rock.class), dot(b.pos, x), std::cmp::Reverse(i))
                    })
                    .expect("the winner's bit");
                let mut r = bits[iw as usize].rock;
                if r.class == TERRANE
                    && let Some(rider) = here
                        .iter()
                        .map(|&j| &bits[j as usize])
                        .filter(|b| {
                            b.plate != pw && b.rock.class == OCEAN && ride(b.plate) > ride(pw)
                        })
                        .map(|b| b.plate)
                        .max_by_key(|&p| ride(p))
                {
                    accrete.push((q, rider, pw, r.piece));
                    r.margins |= ACCRETED;
                    r.orogeny = now_my;
                    pw = rider;
                }
                let mut over_ocean = false;
                for &j in here {
                    if j == iw {
                        continue;
                    }
                    let bj = bits[j as usize];
                    if bj.plate == pw {
                        spare[q].push(j);
                        continue;
                    }
                    match bj.rock.class {
                        OCEAN => {
                            dead[j as usize] = true;
                            if r.class == OCEAN {
                                over_ocean = true;
                            } else {
                                r.margins |= ACTIVE;
                                r.orogeny = now_my;
                                event[q] = Margin::Active as u8;
                            }
                        }
                        TERRANE => {
                            r.margins |= ACCRETED;
                            r.orogeny = now_my;
                            accrete.push((q, pw, bj.plate, bj.rock.piece));
                            let b = &mut bits[j as usize];
                            b.plate = pw;
                            b.rock.margins |= ACCRETED;
                            spare[q].push(j);
                        }
                        // Continents sliding past or parting: the other's
                        // bit stays its own, unseen here.
                        _ if !pressed[q] => {}
                        // A full pile takes no more: the bit stays under
                        // it, its own, unseen.
                        _ if i64::from(r.thick) >= THICK_MAX => {}
                        _ => {
                            if hash(&[seed, P_SINK, u64::from(j), t as u64]) % 1000 < STACK_SINK {
                                shortened += i64::from(bj.area);
                                dead[j as usize] = true;
                            }
                            let add =
                                i64::from(bj.rock.thick) * i64::from(bj.area) / area(q).max(1);
                            r.thick = (i64::from(r.thick) + add).min(THICK_MAX) as u16;
                            r.orogeny = now_my;
                            r.margins |= SUTURE;
                            event[q] = Margin::Suture as u8;
                        }
                    }
                }
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
                        made += area(q);
                    }
                }
                plate[q] = pw;
                rock[q] = r;
                disp[q] = iw;
                bits[iw as usize].plate = pw;
                bits[iw as usize].rock = r;
            }
            // 6. A cell the grid's sampling left empty inside a plate shows
            // a bit of that plate not shown nearby (none is copied).
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
                    let mut best: Option<(i64, u32)> = None;
                    for &m in &ring {
                        for &j in &spare[m] {
                            if used[j as usize] || dead[j as usize] {
                                continue;
                            }
                            let d = km_between(bits[j as usize].pos, x);
                            if d < *reach && best.is_none_or(|(bd, bj)| (d, j) < (bd, bj)) {
                                best = Some((d, j));
                            }
                        }
                    }
                    if let Some((_, j)) = best {
                        used[j as usize] = true;
                        pending[q] = false;
                        plate[q] = bits[j as usize].plate;
                        rock[q] = bits[j as usize].rock;
                        disp[q] = j;
                    }
                }
            }
            // A cell still empty with its plate all around is a hole of the
            // sampling, not a gap (the grid's cells are not all the same
            // size, a moving plate's bits are not either): it draws its
            // nearest neighbour again, and no crust is made or copied.
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
                if around.len() >= HOLE_AROUND
                    && around.iter().all(|&(_, m)| g(m) == g(around[0].1))
                {
                    let &(_, m) = around.iter().min().expect("neighbours");
                    plate[q] = plate[m];
                    rock[q] = rock[m];
                    disp[q] = NO_BIT;
                    pending[q] = false;
                }
            }
            // 7. Gaps: new ocean floor, filled from both sides at once.
            let mut open: Vec<usize> = (0..CELLS).filter(|&q| pending[q]).collect();
            let born: Vec<usize> = open.clone();
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
            };
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
            for &q in &born {
                if pending[q] {
                    plate[q] = old_plate[q];
                    rock[q] = ocean_rock(q);
                    pending[q] = false;
                }
                disp[q] = bits.len() as u32;
                bits.push(Bit {
                    pos: sphere.centre[q],
                    plate: plate[q],
                    area: sphere.area_km2[q],
                    rock: rock[q],
                });
                for &(m, _) in &sphere.links[q] {
                    let m = m as usize;
                    if rock[m].class != OCEAN && event[m] != Margin::Suture as u8 {
                        rock[m].margins |= RIFT;
                        event[m] = Margin::Rift as u8;
                    }
                }
            }
            dead.resize(bits.len(), false);
            // Where each bit is drawn: its cell, or the cell it fills.
            let mut drawn: Vec<u32> = at;
            drawn.resize(bits.len(), 0);
            for c in 0..CELLS {
                if disp[c] != NO_BIT {
                    drawn[disp[c] as usize] = c as u32;
                }
            }
            // 8. Accretion: the terrane or arc joins the continent's plate.
            for &(q, pw, pl, piece) in &accrete {
                let mut stack: Vec<usize> = Vec::new();
                for &(m, _) in &sphere.links[q] {
                    stack.push(m as usize);
                    for &(k, _) in &sphere.links[m as usize] {
                        stack.push(k as usize);
                    }
                }
                let mut km2 = 0;
                let mut moved_cells = Vec::new();
                while let Some(c) = stack.pop() {
                    if plate[c] != pl || rock[c].piece != piece || rock[c].class != TERRANE {
                        continue;
                    }
                    plate[c] = pw;
                    rock[c].margins |= ACCRETED;
                    rock[c].orogeny = now_my;
                    km2 += area(c);
                    moved_cells.push(c);
                    stack.extend(sphere.links[c].iter().map(|&(m, _)| m as usize));
                }
                if !moved_cells.is_empty() {
                    let mut hit = vec![false; CELLS];
                    for &c in &moved_cells {
                        hit[c] = true;
                    }
                    for (i, b) in bits.iter_mut().enumerate() {
                        if b.plate == pl
                            && b.rock.class == TERRANE
                            && b.rock.piece == piece
                            && hit[drawn[i] as usize]
                        {
                            b.plate = pw;
                        }
                    }
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
                    if rock[c].class != OCEAN {
                        rock[c].margins |= ACCRETED;
                    }
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
            }
            // 9. Piles spread and wear down.
            let thick: Vec<i64> = rock.iter().map(|r| i64::from(r.thick)).collect();
            for c in 0..CELLS {
                if rock[c].class == OCEAN {
                    continue;
                }
                let mut flow = 0;
                for &(m, _) in &sphere.links[c] {
                    let m = m as usize;
                    if rock[m].class != OCEAN && plate[m] == plate[c] {
                        flow += (thick[m] - thick[c]) * THICK_SPREAD / 1000 / 8;
                    }
                }
                let base = i64::from(if rock[c].class == CONTINENT {
                    CONTINENT_THICK
                } else {
                    TERRANE_THICK
                });
                let v = thick[c] + flow;
                let v = if v > base {
                    base + (v - base) * THICK_KEEP / 1000
                } else {
                    v
                };
                rock[c].thick = v.clamp(i64::from(OCEAN_THICK), THICK_MAX) as u16;
            }
            sync(&mut bits, &plate, &rock, &disp);
            let mut cells_of = [0_i64; MAXP];
            let mut cont_of = [0_i64; MAXP];
            for c in 0..CELLS {
                cells_of[plate[c] as usize] += 1;
                if rock[c].class != OCEAN {
                    cont_of[plate[c] as usize] += area(c);
                }
            }
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
                let i0 = disp[c];
                if i0 == NO_BIT {
                    continue;
                }
                let p = bits[i0 as usize].plate;
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
                    let bit = if disp[cj] != NO_BIT && bits[disp[cj] as usize].plate == p {
                        disp[cj]
                    } else {
                        i0
                    };
                    marks.push(Mark {
                        bit,
                        pos,
                        spot: k as u8,
                        my: now_my + j as u16,
                    });
                }
            }
            // 11. A later rift: the plate carrying the most continent
            // splits, along its old seams where it has them.
            if rift_when.contains(&t) && next_id < MAXP {
                let big = (0..MAXP)
                    .max_by_key(|&p| (cont_of[p], std::cmp::Reverse(p)))
                    .expect("plates");
                if cont_of[big] >= s.rift_min_km2 {
                    let mut sum = [0_i64; 3];
                    for c in 0..CELLS {
                        if plate[c] as usize == big && rock[c].class == CONTINENT {
                            sum = add(sum, sphere.centre[c]);
                        }
                    }
                    let centre = normalize(sum);
                    let seam: Vec<bool> = (0..CELLS)
                        .map(|c| {
                            plate[c] as usize == big
                                && rock[c].class == CONTINENT
                                && rock[c].margins & (OLD_SUTURE | SUTURE) != 0
                        })
                        .collect();
                    let a0 = draw(seed, P_RIFT, 100 + t as u64, 0, TAU);
                    let (e, nn) = (east_at(centre), north_at(centre, east_at(centre)));
                    let band = sin(RIFT_SEAM_KM * ONE / RADIUS_KM);
                    let normal_at = |k: i64| {
                        let a = a0 + k * TAU / 36;
                        normalize(add(scale(e, cos(a)), scale(nn, sin(a))))
                    };
                    let normal = normal_at(
                        (0..18)
                            .max_by_key(|&k| {
                                let nk = normal_at(k);
                                let score = (0..CELLS)
                                    .filter(|&c| {
                                        seam[c]
                                            && dot(sphere.centre[c], nk).abs() < band
                                            && km_between(sphere.centre[c], centre) < RIFT_SPAN_KM
                                    })
                                    .count();
                                (score, std::cmp::Reverse(k))
                            })
                            .expect("directions"),
                    );
                    let far = sin(RIFT_SIDE_KM * ONE / RADIUS_KM);
                    let sources: Vec<(usize, u32)> = (0..CELLS)
                        .filter(|&c| plate[c] as usize == big)
                        .filter_map(|c| {
                            let side = dot(sphere.centre[c], normal);
                            if side > far {
                                Some((c, 0))
                            } else if side < -far {
                                Some((c, 1))
                            } else {
                                None
                            }
                        })
                        .collect();
                    let (_, label) =
                        sphere.spread(&sources, |_, b| plate[b] as usize == big && !seam[b], 6_000);
                    let (old, new) = (big as u8, next_id as u8);
                    next_id += 1;
                    let side_new: Vec<bool> = (0..CELLS)
                        .map(|c| {
                            plate[c] == old
                                && match label[c] {
                                    0 => false,
                                    1 => true,
                                    _ => dot(sphere.centre[c], normal) < 0,
                                }
                        })
                        .collect();
                    for c in 0..CELLS {
                        if side_new[c] {
                            plate[c] = new;
                        }
                    }
                    for (i, b) in bits.iter_mut().enumerate() {
                        if b.plate == old && side_new[drawn[i] as usize] {
                            b.plate = new;
                        }
                    }
                    let index = rifts.len() as u8 + 1;
                    for c in 0..CELLS {
                        if rock[c].class == OCEAN || (plate[c] != old && plate[c] != new) {
                            continue;
                        }
                        if sphere.links[c].iter().any(|&(m, _)| {
                            let m = m as usize;
                            rock[m].class != OCEAN
                                && plate[m] != plate[c]
                                && (plate[m] == old || plate[m] == new)
                        }) {
                            rock[c].margins |= RIFT;
                            rock[c].rift = index;
                            event[c] = Margin::Rift as u8;
                        }
                    }
                    for (at, km2) in
                        flood(sphere, &plate, &mut rock, (old, new), &plumes, &mut spent)
                    {
                        lips.push(Lip {
                            my: now_my,
                            at,
                            km2,
                        });
                    }
                    sync(&mut bits, &plate, &rock, &disp);
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
                    fresh.push((old, new, t));
                    // The halves part at once, at the rift's speed.
                    let half = s.rift_open_mm * 1000 / 2;
                    let base = omega[o];
                    omega[o] = add(base, lateral(centre, normal, half));
                    omega[w] = add(base, lateral(centre, normal.map(|v| -v), half));
                    pushes.push(Push {
                        centre,
                        normal,
                        reach_km: RIFT_REACH_KM,
                        expansion_km: RIFT_EXPANSION_KM,
                        strength: 2 * half,
                        plates: Some((old, new)),
                    });
                    rifts.push(Rift {
                        my: now_my,
                        plate: old,
                        new,
                        centre,
                        area_km2: cont_of[o],
                        aborted: false,
                    });
                }
            }
            // The bits gone (sunk, piled) leave the record.
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
            marks.retain_mut(|m| {
                let k = new_ix[m.bit as usize];
                m.bit = k;
                k != NO_BIT
            });
            shortened_km2.push(shortened);
            made_km2.push(made);
            for push in &mut pushes {
                push.strength = push.strength * s.push_keep / 1000;
            }
        }
        // 12. What the plates do now, and the forces for the next step.
        let seen = look(sphere, &plate, &group, &mut rock, &omega, &event);
        for c in 0..CELLS {
            if seen.margin[c] == Margin::Active as u8 && rock[c].orogeny != now_my {
                rock[c].orogeny = now_my;
            }
        }
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
                })
                .collect::<Vec<Motion>>(),
        );
        continental_km2.push(
            (0..CELLS)
                .filter(|&c| rock[c].class != OCEAN)
                .map(area)
                .sum(),
        );
        if t < s.steps {
            let next = drive(
                sphere,
                s,
                &plate,
                &rock,
                &pushes,
                &alias,
                &downwellings,
                (&seen.slab, &seen.ridge, &grips),
                Some(&omega),
                &held(&parted, &fresh, t, young),
            );
            omega = next.omega;
            group = next.group;
            // A later rift whose halves are held together again before
            // they parted has aborted: a failed rift stays in the crust.
            for (i, r) in rifts.iter_mut().enumerate() {
                let (a, b) = (alias_of(&alias, r.plate), alias_of(&alias, r.new));
                if !r.aborted
                    && a != b
                    && !parted[a as usize][b as usize]
                    && group[a as usize] == group[b as usize]
                {
                    r.aborted = true;
                    for cell in &mut rock {
                        if usize::from(cell.rift) == i + 1 {
                            cell.margins |= FAILED_RIFT;
                        }
                    }
                    for bit in &mut bits {
                        if usize::from(bit.rock.rift) == i + 1 {
                            bit.rock.margins |= FAILED_RIFT;
                        }
                    }
                    for push in &mut pushes {
                        if push.plates == Some((r.plate, r.new)) {
                            push.strength = 0;
                        }
                    }
                }
            }
            opened = opening(sphere, &plate, &rock, &parted, &group, &mut broken);
            for &pair in &opened {
                for (at, km2) in flood(sphere, &plate, &mut rock, pair, &plumes, &mut spent) {
                    lips.push(Lip {
                        my: now_my,
                        at,
                        km2,
                    });
                }
            }
            sync(&mut bits, &plate, &rock, &disp);
        }
    }
    let mut crust = vec![crust_cut - 400; CELLS];
    for c in 0..CELLS {
        if rock[c].class != OCEAN {
            crust[c] = field[rock[c].home as usize].max(crust_cut);
        }
    }
    History {
        upwelling,
        rift_normal,
        downwellings,
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
        marks_at: out_marks,
        hotspots: hotspots.to_vec(),
        motion: out_motion,
        continental_km2,
        shortened_km2,
        made_km2,
        rifts,
        welds,
        accretions,
        collisions,
        escapes,
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
