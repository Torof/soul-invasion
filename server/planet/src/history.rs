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
//!    against continent is a collision (a suture range, crust lost to
//!    shortening) and the two plates weld into one; a terrane or an arc
//!    against a continent is accreted to its plate.
//! 6. **Rifts.** Later, the plate carrying the most continent may split
//!    along a warped line into two; if they weld again before they have
//!    parted, the rift is aborted (a failed rift stays in the crust).
//!
//! Moving the crust on a fixed grid: each cell asks the plates that may
//! reach it (found by a forward pass) which cell it came from, through
//! each plate's turn run backward. Today's plates are the plates of the
//! last step.

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
    /// Steps a rift has to part: two halves that meet again sooner do not
    /// weld (they part or press), later they weld (an aborted rift).
    pub rift_steps: usize,
    /// Steps of the history.
    pub steps: usize,
    /// Each step's length, millions of years.
    pub step_my: i64,
    /// Plate speeds, mm a year: the least a plate is given today (for the
    /// hotspots' chains), and the most any plate reaches.
    pub speed_mm: (i64, i64),
    /// The upwelling's push under the supercontinent, mm a year at
    /// [`EXPANSION_KM`] from the rift axis; later rifts push as hard.
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
            crust_per_mille: 220,
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
            speed_mm: (10, 90),
            upwelling_push: 60,
            push_keep: 960,
            downwellings: (2, 3),
            downwelling_draw: 12,
            slab_pull: 5_000,
            ridge_push: 300,
            continent_drag: 800,
            pole_flight: 10,
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
/// Plates that may claim one cell.
const CAND: usize = 6;

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

fn apply_back(m: &M3, v: V3) -> V3 {
    let r = |j: usize| {
        ((0..3)
            .map(|k| i128::from(m[k][j]) * i128::from(v[k]))
            .sum::<i128>()
            / i128::from(ONE)) as i64
    };
    [r(0), r(1), r(2)]
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
}

/// A push from the mantle: away from a great circle (`normal`), growing
/// with the distance from it up to `expansion_km`, within `reach_km` of
/// `centre`; on two plates only, for a later rift.
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

/// The part of `v` tangent to the sphere at `x`.
fn tangent(x: V3, v: V3) -> V3 {
    sub(v, scale(x, dot(v, x)))
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

/// The plates' turns from the forces on them, µm a year about each pole
/// (a vector along the pole).
fn drive(
    sphere: &Sphere,
    s: &Settings,
    plate: &[u8],
    rock: &[Rock],
    pushes: &[Push],
    alias: &[u8; MAXP],
    downwellings: &[V3],
    (slab, ridge): (&[Spot], &[Spot]),
    prev: Option<&[V3]>,
) -> Vec<V3> {
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
            // And outward from the upwelling itself: the pieces along the
            // axis part too.
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
    let top = s.speed_mm.1 * 1000;
    (0..MAXP)
        .map(|p| {
            if cells[p] == 0 {
                return [0; 3];
            }
            let resist = cells[p] * 1000 + s.continent_drag * cont[p];
            let mut w = torque[p].map(|v| v * 1000 / resist);
            if let Some(prev) = prev {
                w = [0, 1, 2].map(|k| (prev[p][k] * INERTIA + w[k] * (1000 - INERTIA)) / 1000);
            }
            let speed = norm(w);
            if speed > top {
                w = w.map(|v| v * top / speed);
            }
            w
        })
        .collect()
}

/// What the plates do at a step, read from their relative motion: each
/// cell's boundary kind and margin, the slabs and the ridges (whose
/// forces drive the next step). Marks the margins the crust takes.
struct Look {
    kind: Vec<u8>,
    margin: Vec<u8>,
    slab: Vec<Spot>,
    ridge: Vec<Spot>,
}

fn look(sphere: &Sphere, plate: &[u8], rock: &mut [Rock], omega: &[V3], event: &[u8]) -> Look {
    let floor = floor_born(plate, rock);
    let mut kind = vec![0_u8; CELLS];
    let mut margin = vec![Margin::Inland as u8; CELLS];
    let mut slab = Vec::new();
    let mut ridge = Vec::new();
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
            if o == mine {
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
    }
}

/// The history of the planet born from `seed`.
pub fn run(seed: u64, s: &Settings, sphere: &Sphere, old_suture: &[bool]) -> History {
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
                if sphere.links[c]
                    .iter()
                    .any(|&(n, _)| inside[n as usize] && plate[n as usize] != plate[c])
                {
                    margins |= RIFT;
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
    // Plates whose continents touch have not parted; only parted plates
    // weld, or a rift older than `rift_steps` (it aborted).
    let mut parted = [[true; MAXP]; MAXP];
    let mut split_at = vec![[0_usize; MAXP]; MAXP];
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
    let mut alias = [0_u8; MAXP];
    for (k, a) in alias.iter_mut().enumerate() {
        *a = k as u8;
    }
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
    let arc_steps = s.arc_steps.clamp(1, 250) as u8;
    // Step 0.
    let mut out_plate = Vec::with_capacity(s.steps + 1);
    let mut out_class = Vec::with_capacity(s.steps + 1);
    let mut out_age = Vec::with_capacity(s.steps + 1);
    let mut out_kind = Vec::with_capacity(s.steps + 1);
    let mut out_margin = Vec::with_capacity(s.steps + 1);
    let mut out_motion = Vec::with_capacity(s.steps + 1);
    let mut continental_km2 = Vec::with_capacity(s.steps + 1);
    let mut shortened_km2 = vec![0];
    let mut made_km2 = vec![0];
    let mut omega = drive(
        sphere,
        s,
        &plate,
        &rock,
        &pushes,
        &alias,
        &downwellings,
        (&[], &[]),
        None,
    );
    let mut event: Vec<u8> = (0..CELLS)
        .map(|c| {
            if rock[c].margins & RIFT != 0 {
                Margin::Rift as u8
            } else {
                Margin::Inland as u8
            }
        })
        .collect();
    for t in 0..=s.steps {
        let now_my = ((s.steps - t) as i64 * s.step_my) as u16;
        if t > 0 {
            // 3. The plates turn, carrying their crust.
            let turn: Vec<M3> = omega
                .iter()
                .map(|&w| {
                    let speed = norm(w);
                    if speed < 500 {
                        IDENTITY
                    } else {
                        rotation(normalize(w), speed * s.step_my * ONE / (1000 * RADIUS_KM))
                    }
                })
                .collect();
            let old_plate = plate.clone();
            let old_rock = rock.clone();
            let mut cont_km2 = [0_i64; MAXP];
            for c in 0..CELLS {
                if old_rock[c].class != OCEAN {
                    cont_km2[old_plate[c] as usize] += area(c);
                }
            }
            let mut cand = vec![[NONE; CAND]; CELLS];
            let put = |cand: &mut [[u8; CAND]], q: usize, p: u8| {
                for slot in &mut cand[q] {
                    if *slot == p {
                        return;
                    }
                    if *slot == NONE {
                        *slot = p;
                        return;
                    }
                }
            };
            for c in 0..CELLS {
                let p = old_plate[c];
                let q = sphere.cell_of(apply(&turn[p as usize], sphere.centre[c]));
                put(&mut cand, q, p);
                for &(n, _) in &sphere.links[q] {
                    put(&mut cand, n as usize, p);
                }
            }
            let floor = floor_born(&old_plate, &old_rock);
            let key = |(p, src): (u8, usize)| {
                let r = old_rock[src];
                let p = i64::from(p);
                match r.class {
                    CONTINENT => (3, cont_km2[p as usize], -p),
                    TERRANE => (2, cont_km2[p as usize], -p),
                    _ => (1, -floor[p as usize], -p),
                }
            };
            let mut pending = vec![false; CELLS];
            event.fill(Margin::Inland as u8);
            let mut collide: Vec<(u8, u8)> = Vec::new();
            let mut accrete: Vec<(usize, u8, u8, u8)> = Vec::new();
            let mut shortened = 0;
            let mut made = 0;
            for q in 0..CELLS {
                let x = sphere.centre[q];
                let mut claims: Vec<(u8, usize)> = Vec::with_capacity(CAND);
                for &p in &cand[q] {
                    if p == NONE {
                        break;
                    }
                    let src = sphere.cell_of(apply_back(&turn[p as usize], x));
                    if old_plate[src] == p {
                        claims.push((p, src));
                    }
                }
                let Some(&win) = claims.iter().max_by_key(|&&cl| key(cl)) else {
                    pending[q] = true;
                    continue;
                };
                let (mut pw, sw) = win;
                let mut r = old_rock[sw];
                // A terrane meeting the ocean that rides over its own
                // plate docks there: it is accreted to the riding plate
                // (it does not plough through it).
                let ride = |p: u8| (-floor[p as usize], -i64::from(p));
                if r.class == TERRANE
                    && let Some(&(rider, _)) = claims
                        .iter()
                        .filter(|&&(p, src)| {
                            p != pw && old_rock[src].class == OCEAN && ride(p) > ride(pw)
                        })
                        .max_by_key(|&&(p, _)| ride(p))
                {
                    accrete.push((q, rider, pw, r.piece));
                    r.margins |= ACCRETED;
                    r.orogeny = now_my;
                    pw = rider;
                }
                let mut over_ocean = false;
                for &(pl, sl) in &claims {
                    if (pl, sl) == win {
                        continue;
                    }
                    let lost = old_rock[sl];
                    if lost.class == OCEAN {
                        if r.class == OCEAN {
                            over_ocean = true;
                        } else {
                            r.margins |= ACTIVE;
                            r.orogeny = now_my;
                            event[q] = Margin::Active as u8;
                        }
                        continue;
                    }
                    shortened += area(q);
                    r.orogeny = now_my;
                    if lost.class == TERRANE {
                        r.margins |= ACCRETED;
                        accrete.push((q, pw, pl, lost.piece));
                    } else {
                        let (a, b) = (pw as usize, pl as usize);
                        if parted[a][b] || t - split_at[a][b] > s.rift_steps {
                            r.margins |= SUTURE;
                            event[q] = Margin::Suture as u8;
                            collide.push((pw.min(pl), pw.max(pl)));
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
                        made += area(q);
                    }
                }
                plate[q] = pw;
                rock[q] = r;
            }
            // 4. Gaps: new ocean floor, filled from both sides at once.
            let mut open: Vec<usize> = (0..CELLS).filter(|&q| pending[q]).collect();
            let born: Vec<usize> = open.clone();
            while !open.is_empty() {
                let mut set = Vec::new();
                let mut next = Vec::new();
                for &q in &open {
                    let mut tally: [(u8, u8); 8] = [(NONE, 0); 8];
                    for &(n, _) in &sphere.links[q] {
                        let n = n as usize;
                        if pending[n] {
                            continue;
                        }
                        if let Some(t) = tally.iter_mut().find(|t| t.0 == plate[n] || t.0 == NONE) {
                            t.0 = plate[n];
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
                    rock[q] = Rock {
                        class: OCEAN,
                        piece: NONE,
                        margins: 0,
                        arc: 0,
                        rift: 0,
                        born_my: now_my,
                        orogeny: NEVER,
                        home: q as u32,
                    };
                }
                open = next;
            }
            for &q in &born {
                for &(n, _) in &sphere.links[q] {
                    let n = n as usize;
                    if rock[n].class != OCEAN && event[n] != Margin::Suture as u8 {
                        rock[n].margins |= RIFT;
                        event[n] = Margin::Rift as u8;
                    }
                }
            }
            // 5. Accretion: the terrane or arc joins the continent's plate.
            for &(q, pw, pl, piece) in &accrete {
                let mut stack: Vec<usize> = Vec::new();
                for &(n, _) in &sphere.links[q] {
                    stack.push(n as usize);
                    for &(m, _) in &sphere.links[n as usize] {
                        stack.push(m as usize);
                    }
                }
                let mut km2 = 0;
                while let Some(c) = stack.pop() {
                    if plate[c] != pl || rock[c].piece != piece || rock[c].class != TERRANE {
                        continue;
                    }
                    plate[c] = pw;
                    rock[c].margins |= ACCRETED;
                    rock[c].orogeny = now_my;
                    km2 += area(c);
                    stack.extend(sphere.links[c].iter().map(|&(n, _)| n as usize));
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
            for c in 0..CELLS {
                let same = sphere.links[c]
                    .iter()
                    .filter(|&&(n, _)| plate[n as usize] == plate[c])
                    .count();
                if same > usize::from(rock[c].class == OCEAN) {
                    continue;
                }
                let mut tally: [(u8, u8); 8] = [(NONE, 0); 8];
                for &(n, _) in &sphere.links[c] {
                    let o = plate[n as usize];
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
                    plate[c] = o;
                    if rock[c].class != OCEAN {
                        rock[c].margins |= ACCRETED;
                    }
                }
            }
            // 6. Collisions weld the two plates into one.
            collide.sort_unstable();
            collide.dedup();
            let mut cells_of = [0_i64; MAXP];
            let mut cont_of = [0_i64; MAXP];
            for c in 0..CELLS {
                cells_of[plate[c] as usize] += 1;
                if rock[c].class != OCEAN {
                    cont_of[plate[c] as usize] += area(c);
                }
            }
            for (a, b) in collide {
                let (a, b) = (alias_of(&alias, a), alias_of(&alias, b));
                if a == b {
                    continue;
                }
                let (kept, absorbed) = if (cont_of[a as usize], -i64::from(a))
                    >= (cont_of[b as usize], -i64::from(b))
                {
                    (a, b)
                } else {
                    (b, a)
                };
                let (k, x) = (kept as usize, absorbed as usize);
                let was_parted = parted[k][x];
                if !was_parted {
                    for (i, r) in rifts.iter_mut().enumerate() {
                        let ends = (alias_of(&alias, r.plate), alias_of(&alias, r.new));
                        if !r.aborted && (ends == (kept, absorbed) || ends == (absorbed, kept)) {
                            r.aborted = true;
                            for cell in &mut rock {
                                if usize::from(cell.rift) == i + 1 {
                                    cell.margins |= FAILED_RIFT;
                                }
                            }
                            for push in &mut pushes {
                                if push.plates == Some((r.plate, r.new)) {
                                    push.strength = 0;
                                }
                            }
                        }
                    }
                }
                welds.push(Weld {
                    my: now_my,
                    kept,
                    absorbed,
                    kept_km2: cont_of[k],
                    absorbed_km2: cont_of[x],
                    parted: was_parted,
                });
                for p in &mut plate {
                    if *p == absorbed {
                        *p = kept;
                    }
                }
                let (nk, nx) = (cells_of[k].max(1), cells_of[x]);
                omega[k] = [0, 1, 2].map(|i| (omega[k][i] * nk + omega[x][i] * nx) / (nk + nx));
                omega[x] = [0; 3];
                cells_of[k] += cells_of[x];
                cont_of[k] += cont_of[x];
                cells_of[x] = 0;
                cont_of[x] = 0;
                alias[x] = kept;
                for o in 0..MAXP {
                    parted[k][o] &= parted[x][o];
                    parted[o][k] = parted[k][o];
                    split_at[k][o] = split_at[k][o].max(split_at[x][o]);
                    split_at[o][k] = split_at[k][o];
                }
            }
            // Plates whose continents no longer touch have parted.
            let mut touching = [[false; MAXP]; MAXP];
            for c in 0..CELLS {
                if rock[c].class == OCEAN {
                    continue;
                }
                for &(n, _) in &sphere.links[c] {
                    let n = n as usize;
                    if rock[n].class != OCEAN && plate[n] != plate[c] {
                        touching[plate[c] as usize][plate[n] as usize] = true;
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
            // 7. A later rift: the plate carrying the most continent splits.
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
                    let a = draw(seed, P_RIFT, 100 + t as u64, 0, TAU);
                    let (e, n) = (east_at(centre), north_at(centre, east_at(centre)));
                    let normal = normalize(add(scale(e, cos(a)), scale(n, sin(a))));
                    let warp_seed = hash(&[seed, P_RIFT, 200 + t as u64]);
                    let (old, new) = (big as u8, next_id as u8);
                    next_id += 1;
                    for c in 0..CELLS {
                        if plate[c] == old
                            && dot(warp(warp_seed, sphere.centre[c], &RIFT_WARP), normal) < 0
                        {
                            plate[c] = new;
                        }
                    }
                    let index = rifts.len() as u8 + 1;
                    for c in 0..CELLS {
                        if rock[c].class == OCEAN || (plate[c] != old && plate[c] != new) {
                            continue;
                        }
                        if sphere.links[c].iter().any(|&(n, _)| {
                            let n = n as usize;
                            rock[n].class != OCEAN
                                && plate[n] != plate[c]
                                && (plate[n] == old || plate[n] == new)
                        }) {
                            rock[c].margins |= RIFT;
                            rock[c].rift = index;
                            event[c] = Margin::Rift as u8;
                        }
                    }
                    let (o, w) = (old as usize, new as usize);
                    for p in 0..MAXP {
                        parted[w][p] = parted[o][p];
                        parted[p][w] = parted[p][o];
                        split_at[w][p] = split_at[o][p];
                        split_at[p][w] = split_at[p][o];
                    }
                    parted[o][w] = false;
                    parted[w][o] = false;
                    split_at[o][w] = t;
                    split_at[w][o] = t;
                    omega[w] = omega[o];
                    pushes.push(Push {
                        centre,
                        normal,
                        reach_km: RIFT_REACH_KM,
                        expansion_km: RIFT_EXPANSION_KM,
                        strength: s.upwelling_push * 1000,
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
            shortened_km2.push(shortened);
            made_km2.push(made);
            for push in &mut pushes {
                push.strength = push.strength * s.push_keep / 1000;
            }
        }
        // 8. What the plates do now, and the forces for the next step.
        let seen = look(sphere, &plate, &mut rock, &omega, &event);
        for c in 0..CELLS {
            if seen.margin[c] == Margin::Active as u8 && rock[c].orogeny != now_my {
                rock[c].orogeny = now_my;
            }
        }
        out_plate.push(plate.clone());
        out_class.push(rock.iter().map(|r| r.class).collect::<Vec<u8>>());
        out_age.push(
            rock.iter()
                .map(|r| r.born_my.saturating_sub(now_my))
                .collect::<Vec<u16>>(),
        );
        out_kind.push(seen.kind);
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
            omega = drive(
                sphere,
                s,
                &plate,
                &rock,
                &pushes,
                &alias,
                &downwellings,
                (&seen.slab, &seen.ridge),
                Some(&omega),
            );
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
        motion: out_motion,
        continental_km2,
        shortened_km2,
        made_km2,
        rifts,
        welds,
        accretions,
        home: rock.iter().map(|r| r.home).collect(),
        piece: rock
            .iter()
            .map(|r| if r.class == OCEAN { NONE } else { r.piece })
            .collect(),
        age_my: rock.iter().map(|r| r.born_my).collect(),
        margins: rock.iter().map(|r| r.margins).collect(),
        orogeny_my: rock.iter().map(|r| r.orogeny).collect(),
        crust,
        crust_cut,
    }
}
