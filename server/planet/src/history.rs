//! The plates' history (version 3, the lead's 26 November): continents
//! are not placed, they come out of a Wilson cycle run on the grid.
//!
//! 1. **A supercontinent**: one warped cap carrying all the continental
//!    crust, its interior already old (cratons) and seamed with older
//!    sutures.
//! 2. **Break-up**: a mantle upwelling under it opens rifts along a
//!    warped, weighted Voronoi of its crust: fragments whose facing
//!    margins match, as Africa's and South America's do.
//! 3. **Drift**: each fragment turns about its own pole, step by step.
//!    Most are pushed away from the rift axis (a great circle through the
//!    upwelling, near north–south, as the Atlantic opened), faster the
//!    farther they are from it so every rift widens, slowing as the push
//!    fades; a few on each downwelling, chosen apart, are drawn toward it,
//!    as cold sinking mantle gathers continents. Pieces weld only once
//!    they have parted. A fragment's leading edge is an active margin
//!    (subduction, arcs, coastal ranges), its trailing edge a passive one
//!    (wide shelves, plains).
//! 4. **Collisions**: fragments that meet weld, with a suture range where
//!    they overlap, and from then on move as one, slower.
//! 5. **Accretion**: small terranes (island arcs, oceanic plateaus) wait
//!    in the ocean; a continent running into one welds it to its margin.
//!
//! Each piece of crust carries its record with it: its fragment, its
//! age, the margins it has been, its last mountain building. Moving the
//! crust on a fixed grid: each cell asks every fragment, through the
//! fragment's rotation run backward, which cell it came from.

use crate::{
    CELLS, ONE, RADIUS_KM, Sphere, TAU, V3, cos, cross, dot, draw, fbm3, hash, km_between,
    latitude, normalize, quantile, random_unit, sin, step, warp,
};

/// A world's character, the model's input: a supercontinent of this
/// size and shape, broken into so many fragments, drifting so long and so
/// fast, welding and gathering terranes this way. Read from a RON file by
/// the example (`monde-defaut.ron` beside the page lists them, each with
/// what it changes); missing fields take the defaults.
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
    /// Fragments the supercontinent breaks into.
    pub fragments: (i64, i64),
    /// How far the rift axis may turn from north–south, tenths of a degree.
    pub rift_turn: i64,
    /// Steps of rifting: no weld happens before.
    pub rift_steps: usize,
    /// How unequal the fragments are, [`ONE`] units of the largest seed
    /// weight (0: all alike).
    pub fragment_inequality: i64,
    /// Drift steps.
    pub steps: usize,
    /// Each step's length, millions of years.
    pub step_my: i64,
    /// Drift speeds, mm a year (km per million years).
    pub speed_mm: (i64, i64),
    /// How much a fragment turns on itself as it drifts, per mille.
    pub spin: i64,
    /// A dispersing group keeps this much of its speed per step, per mille.
    pub disperse_keep: i64,
    /// Downwellings that gather continents.
    pub downwellings: (i64, i64),
    /// Fragments each downwelling draws.
    pub gathered: (i64, i64),
    /// A gathered group stops this near its downwelling, km.
    pub arrived_km: i64,
    /// A weld keeps this much of the group's speed, per mille.
    pub weld_keep: i64,
    /// Terranes waiting in the ocean to be accreted.
    pub terranes: (i64, i64),
    /// A terrane's length, cells of ~100 km.
    pub terrane_cells: (i64, i64),
    /// Hotspots.
    pub hotspots: (i64, i64),
    /// Plates of today, at least (oceanic ones fill up to this).
    pub plates: (i64, i64),
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
            fragments: (22, 30),
            rift_turn: 300,
            rift_steps: 4,
            fragment_inequality: 50_000,
            steps: 40,
            step_my: 5,
            speed_mm: (20, 55),
            spin: 150,
            disperse_keep: 970,
            downwellings: (1, 2),
            gathered: (2, 3),
            arrived_km: 700,
            weld_keep: 700,
            terranes: (18, 28),
            terrane_cells: (1, 5),
            hotspots: (14, 20),
            plates: (10, 15),
            mantle_latitude: 350,
        }
    }
}

/// No fragment.
pub const NONE: u8 = u8::MAX;
/// No mountain building on record.
pub const NEVER: u16 = u16::MAX;

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

/// What a cell of crust is at one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Margin {
    /// Inside a continent, or the ocean.
    Inland = 0,
    /// A rift between two fragments (the first step).
    Rift = 1,
    /// A trailing edge.
    Passive = 2,
    /// A leading edge.
    Active = 3,
    /// Where two pieces weld.
    Suture = 4,
}

/// A fragment of the supercontinent, or a terrane.
#[derive(Debug, Clone)]
pub struct Fragment {
    /// Its middle at the start.
    pub centre: V3,
    /// Its farthest cell from the middle, km.
    pub reach_km: i64,
    /// Its area, km².
    pub area_km2: i64,
    /// A terrane (a small piece from the ocean), not a fragment.
    pub terrane: bool,
}

/// A group of welded fragments and how it moves.
#[derive(Debug, Clone, Copy)]
pub struct Motion {
    /// The pole it turns about.
    pub pole: V3,
    /// Its turn per step, millionths of a radian.
    pub angle: i64,
    /// Where it is drawn, if a downwelling draws it.
    pub target: Option<V3>,
}

/// The whole history.
pub struct History {
    /// The upwelling that broke the supercontinent.
    pub upwelling: V3,
    /// The downwellings that gather.
    pub downwellings: Vec<V3>,
    /// The fragments, then the terranes.
    pub fragments: Vec<Fragment>,
    /// Each source cell's piece at the start ([`NONE`]: ocean).
    pub origin: Vec<u8>,
    /// Per step, each cell's piece.
    pub owner: Vec<Vec<u8>>,
    /// Per step, each cell's [`Margin`].
    pub margin: Vec<Vec<u8>>,
    /// Per piece, its group at the end (welded pieces share one).
    pub group: Vec<u8>,
    /// Per piece, how its group moves at the end.
    pub motion: Vec<Motion>,
    /// Today: the source cell each cell's crust came from.
    pub source: Vec<u32>,
    /// Today: each cell's piece ([`NONE`]: ocean).
    pub piece: Vec<u8>,
    /// Today: each cell's crust age, millions of years (0 at sea).
    pub age_my: Vec<u16>,
    /// Today: the margin flags the crust gathered.
    pub margins: Vec<u8>,
    /// Today: the last mountain building, millions of years ago.
    pub orogeny_my: Vec<u16>,
    /// The welds, in order: `(millions of years ago, larger group's
    /// piece, smaller group's piece, a terrane's accretion)`.
    pub welds: Vec<(u16, u8, u8, bool)>,
    /// The crust's value at its source (the supercontinent's shape).
    pub crust: Vec<i64>,
    /// The threshold of the supercontinent's crust.
    pub crust_cut: i64,
}

const P_UP: u64 = 0x0c50_0101;
const P_SUPER: u64 = 0x0c50_0102;
const P_FRAG: u64 = 0x0c50_0103;
const P_DOWN: u64 = 0x0c50_0104;
const P_MOVE: u64 = 0x0c50_0105;
const P_TERRANE: u64 = 0x0c50_0106;
const P_AGE: u64 = 0x0c50_0107;

/// The supercontinent's default warp and fray: an irregular cap, not a
/// disc (scaled by [`Settings::supercontinent_warp`] and `_fray`).
const SUPER_WARP: [(i64, i64); 2] = [(700_000, 260_000), (250_000, 80_000)];
const SUPER_FRAY: [(i64, i64); 3] = [(600_000, 4), (250_000, 2), (100_000, 1)];
/// How much the default fray moves the coast, the cap's cosine units (‰).
const FRAY: i64 = 600;
/// The rifts' warp: fragments with wandering edges.
const RIFT_WARP: [(i64, i64); 3] = [(400_000, 140_000), (150_000, 45_000), (60_000, 15_000)];
/// A downwelling is at least this far from the upwelling, km.
const DOWN_FROM_UP_KM: i64 = 10_000;
/// A dispersing fragment this far from the rift axis moves at the
/// settings' highest speed, km.
const EXPANSION_KM: i64 = 5_000;
/// Draws tried for a point that must meet a condition (always far more
/// than needed: each draw meets it with odds of a fifth or better).
const TRIES: u64 = 100_000;
/// Terranes stay this far off the supercontinent's crust, in its cosine
/// units (‰).
const TERRANE_OFF: i64 = 150;
/// Crust ages: the supercontinent's interior (cratons), and terranes, My.
const CRATON_MY: (i64, i64) = (700, 3_000);
const TERRANE_MY: (i64, i64) = (20, 250);
/// The older sutures' last mountain building, My ago.
const OLD_OROGENY_MY: (i64, i64) = (300, 700);

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

fn compose(a: &M3, b: &M3) -> M3 {
    let mut out = [[0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = ((0..3)
                .map(|k| i128::from(a[i][k]) * i128::from(b[k][j]))
                .sum::<i128>()
                / i128::from(ONE)) as i64;
        }
    }
    out
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

fn root(group: &mut [usize], mut g: usize) -> usize {
    while group[g] != g {
        group[g] = group[group[g]];
        g = group[g];
    }
    g
}

/// The history of the planet born from `seed`.
pub fn run(seed: u64, s: &Settings, sphere: &Sphere, old_suture: &[bool]) -> History {
    assert_eq!(s.radius_km, RADIUS_KM, "only the Earth's radius for now");
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
    // 2. Break-up: the fragments.
    let count = draw(seed, P_FRAG, 0, s.fragments.0, s.fragments.1) as u64;
    let seeds: Vec<(V3, i64)> = (0..count)
        .map(|k| {
            let at = (0..TRIES)
                .map(|t| random_unit(seed, P_FRAG + 1, k * 10_000 + t))
                .find(|&p| inside[sphere.cell_of(p)])
                .expect("a point on the supercontinent");
            let u = draw(seed, P_FRAG, 100 + k, 0, 1000);
            (at, u * u / 1000 * s.fragment_inequality / 1000)
        })
        .collect();
    let mut origin = vec![NONE; CELLS];
    for c in 0..CELLS {
        if !inside[c] {
            continue;
        }
        let q = warp(hash(&[seed, P_FRAG, 7]), sphere.centre[c], &RIFT_WARP);
        let mut best = (i64::MIN, 0);
        for (k, &(at, w)) in seeds.iter().enumerate() {
            let score = dot(q, at) + w;
            if score > best.0 {
                best = (score, k);
            }
        }
        origin[c] = best.1 as u8;
    }
    // Terranes: short strings of cells in the open ocean.
    let terranes = draw(seed, P_TERRANE, 0, s.terranes.0, s.terranes.1) as u64;
    let mut placed = 0_u64;
    for k in 0..terranes * 20 {
        if placed == terranes {
            break;
        }
        let at = random_unit(seed, P_TERRANE + 1, k);
        if field_at(at) > crust_cut - TERRANE_OFF || latitude(at).abs() > 700 {
            continue;
        }
        let id = (count + placed) as u8;
        let a = draw(seed, P_TERRANE + 2, k, 0, TAU);
        let (east, north) = (crate::east_at(at), crate::north_at(at, crate::east_at(at)));
        let dir = normalize(crate::add(
            crate::scale(east, cos(a)),
            crate::scale(north, sin(a)),
        ));
        let n = draw(seed, P_TERRANE + 3, k, s.terrane_cells.0, s.terrane_cells.1);
        let cells: Vec<usize> = (0..n)
            .map(|i| sphere.cell_of(step(at, dir, i * 110)))
            .collect();
        if cells.iter().any(|&c| origin[c] != NONE) {
            continue;
        }
        for c in cells {
            origin[c] = id;
        }
        placed += 1;
    }
    let pieces = (count + placed) as usize;
    let mut fragments: Vec<Fragment> = (0..pieces)
        .map(|f| {
            let mut sum = [0_i64; 3];
            let mut area = 0;
            for c in 0..CELLS {
                if origin[c] as usize == f {
                    sum = crate::add(sum, sphere.centre[c]);
                    area += i64::from(sphere.area_km2[c]);
                }
            }
            Fragment {
                centre: if area > 0 { normalize(sum) } else { seeds[0].0 },
                reach_km: 0,
                area_km2: area,
                terrane: f as u64 >= count,
            }
        })
        .collect();
    for c in 0..CELLS {
        if origin[c] != NONE {
            let f = &mut fragments[origin[c] as usize];
            f.reach_km = f.reach_km.max(km_between(f.centre, sphere.centre[c]));
        }
    }
    // Records at the source.
    let mut margins = vec![0_u8; CELLS];
    let mut orogeny = vec![NEVER; CELLS];
    let mut age = vec![0_u16; CELLS];
    let old_my = draw(seed, P_AGE, 1, OLD_OROGENY_MY.0, OLD_OROGENY_MY.1) as u16;
    for c in 0..CELLS {
        let f = origin[c];
        if f == NONE {
            continue;
        }
        if fragments[f as usize].terrane {
            age[c] = draw(seed, P_AGE, 2 + u64::from(f), TERRANE_MY.0, TERRANE_MY.1) as u16;
            continue;
        }
        let n = fbm3(
            hash(&[seed, P_AGE]),
            sphere.centre[c],
            &[(500_000, 2), (200_000, 1)],
        );
        age[c] = (CRATON_MY.0 + (CRATON_MY.1 - CRATON_MY.0) * n.clamp(0, 1000) / 1000) as u16;
        if old_suture[c] {
            margins[c] |= OLD_SUTURE;
            orogeny[c] = old_my;
        }
        if sphere.links[c]
            .iter()
            .any(|&(n, _)| origin[n as usize] != NONE && origin[n as usize] != f)
        {
            margins[c] |= RIFT;
        }
    }
    // 3. Motions.
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
    // Pieces touching at the start: they part as the rifts open, and only
    // pieces that have parted can weld again.
    let mut parted = vec![vec![true; pieces]; pieces];
    for c in 0..CELLS {
        let a = origin[c];
        if a == NONE {
            continue;
        }
        for &(n, _) in &sphere.links[c] {
            let b = origin[n as usize];
            if b != NONE && b != a {
                parted[a as usize][b as usize] = false;
            }
        }
    }
    let mut target: Vec<Option<V3>> = vec![None; pieces];
    for (k, &d) in downwellings.iter().enumerate() {
        let want = draw(seed, P_DOWN, 10 + k as u64, s.gathered.0, s.gathered.1) as usize;
        let mut near: Vec<(i64, usize)> = (0..count as usize)
            .filter(|&f| target[f].is_none() && fragments[f].area_km2 > 0)
            .map(|f| (km_between(fragments[f].centre, d), f))
            .collect();
        near.sort_unstable();
        // The nearest, then the next nearest touching none chosen: they
        // come from apart and meet.
        let mut chosen: Vec<usize> = Vec::new();
        for &(_, f) in &near {
            if chosen.len() == want {
                break;
            }
            if chosen.iter().all(|&g| parted[f][g]) {
                chosen.push(f);
            }
        }
        for f in chosen {
            target[f] = Some(d);
        }
    }
    // The rift axis: a great circle through the upwelling, at most
    // `rift_turn` from north–south (the Atlantic opened along such a line);
    // pieces are pushed away from it, so most drift east or west.
    let axis_turn = draw(seed, P_MOVE, 999, -s.rift_turn, s.rift_turn) * TAU / 3600;
    let (east, north) = (
        crate::east_at(upwelling),
        crate::north_at(upwelling, crate::east_at(upwelling)),
    );
    let rift_normal = normalize(crate::add(
        crate::scale(east, cos(axis_turn)),
        crate::scale(north, sin(axis_turn)),
    ));
    let rift_pole = normalize(cross(upwelling, rift_normal));
    let mut motion: Vec<Motion> = (0..pieces)
        .map(|f| {
            let fr = &fragments[f];
            if fr.terrane || fr.area_km2 == 0 {
                return Motion {
                    pole: [0, 0, ONE],
                    angle: 0,
                    target: None,
                };
            }
            // Drawn: toward the downwelling. Pushed: away from the rift
            // axis, on its own side.
            let side = dot(fr.centre, rift_normal);
            let pure = match target[f] {
                Some(d) => cross(fr.centre, d),
                None if side >= 0 => rift_pole,
                None => rift_pole.map(|v| -v),
            };
            let push = random_unit(seed, P_MOVE + 1, f as u64);
            let pure = normalize(pure);
            let pole = normalize([
                pure[0] + push[0] * s.spin / 1000,
                pure[1] + push[1] * s.spin / 1000,
                pure[2] + push[2] * s.spin / 1000,
            ]);
            // A pushed fragment moves faster the farther it is from the
            // rift axis: the opening is an expansion, every rift widens. A
            // drawn one moves at a speed of its own, in the upper half.
            let speed = if target[f].is_some() {
                draw(
                    seed,
                    P_MOVE + 2,
                    f as u64,
                    i64::midpoint(s.speed_mm.0, s.speed_mm.1),
                    s.speed_mm.1,
                )
            } else {
                let away = side.abs() * RADIUS_KM / ONE;
                (s.speed_mm.1 * away / EXPANSION_KM).clamp(s.speed_mm.0, s.speed_mm.1)
                    * draw(seed, P_MOVE + 2, f as u64, 900, 1100)
                    / 1000
            };
            Motion {
                pole,
                angle: speed * s.step_my * ONE / RADIUS_KM,
                target: target[f],
            }
        })
        .collect();
    // 4. Drift, collisions, accretion.
    let mut turn = vec![IDENTITY; pieces];
    let mut group: Vec<usize> = (0..pieces).collect();
    let mut area: Vec<i64> = fragments.iter().map(|f| f.area_km2).collect();
    let mut owners = Vec::with_capacity(s.steps + 1);
    let mut kinds = Vec::with_capacity(s.steps + 1);
    let mut source = vec![u32::MAX; CELLS];
    let mut welds = Vec::new();
    for t in 0..=s.steps {
        let now_my = ((s.steps - t) as i64 * s.step_my) as u16;
        if t > 0 {
            for f in 0..pieces {
                let g = root(&mut group, f);
                if motion[g].angle != 0 {
                    turn[f] = compose(&rotation(motion[g].pole, motion[g].angle), &turn[f]);
                }
            }
        }
        // Each cell asks the pieces, the larger groups first.
        let mut order: Vec<usize> = (0..pieces).filter(|&f| fragments[f].area_km2 > 0).collect();
        let mut by: Vec<(i64, usize)> = order
            .iter()
            .map(|&f| {
                let g = root(&mut group, f);
                (-area[g], f)
            })
            .collect();
        by.sort_unstable();
        order = by.into_iter().map(|(_, f)| f).collect();
        let mut owner = vec![NONE; CELLS];
        source.fill(u32::MAX);
        let mut contacts: Vec<(usize, usize, usize, usize)> = Vec::new();
        for &f in &order {
            let centre = apply(&turn[f], fragments[f].centre);
            let near = cos((fragments[f].reach_km + 150) * ONE / RADIUS_KM);
            let gf = root(&mut group, f);
            for c in 0..CELLS {
                let q = sphere.centre[c];
                if dot(q, centre) < near {
                    continue;
                }
                let from = sphere.cell_of(normalize(apply_back(&turn[f], q)));
                if origin[from] as usize != f {
                    continue;
                }
                if owner[c] == NONE {
                    owner[c] = f as u8;
                    source[c] = from as u32;
                } else if t > s.rift_steps
                    && parted[owner[c] as usize][f]
                    && root(&mut group, owner[c] as usize) != gf
                {
                    contacts.push((c, owner[c] as usize, f, from));
                }
            }
        }
        // Margins.
        let mut kind = vec![Margin::Inland as u8; CELLS];
        for c in 0..CELLS {
            if owner[c] == NONE {
                continue;
            }
            let src = source[c] as usize;
            if t == 0 {
                if margins[src] & RIFT != 0 {
                    kind[c] = Margin::Rift as u8;
                }
                continue;
            }
            let q = sphere.centre[c];
            let mut out = [0_i64; 3];
            for &(n, _) in &sphere.links[c] {
                if owner[n as usize] == NONE {
                    out = crate::add(out, crate::sub(sphere.centre[n as usize], q));
                }
            }
            if out == [0, 0, 0] {
                continue;
            }
            let m = motion[root(&mut group, owner[c] as usize)];
            let leading = m.angle > 0 && dot(cross(m.pole, q), out) > 0;
            if leading {
                kind[c] = Margin::Active as u8;
                margins[src] |= ACTIVE;
                orogeny[src] = now_my;
            } else {
                kind[c] = Margin::Passive as u8;
                margins[src] |= PASSIVE;
            }
        }
        // Pieces no longer touching have parted.
        let mut touching = vec![vec![false; pieces]; pieces];
        for c in 0..CELLS {
            let a = owner[c];
            if a == NONE {
                continue;
            }
            for &(n, _) in &sphere.links[c] {
                let b = owner[n as usize];
                if b != NONE && b != a {
                    touching[a as usize][b as usize] = true;
                }
            }
        }
        for (a, row) in touching.iter().enumerate() {
            for (b, &touch) in row.iter().enumerate() {
                if !touch && !touching[b][a] {
                    parted[a][b] = true;
                }
            }
        }
        // Collisions weld.
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for &(c, a, b, from) in &contacts {
            let flag = if fragments[a].terrane || fragments[b].terrane {
                ACCRETED
            } else {
                SUTURE
            };
            for src in [source[c] as usize, from] {
                margins[src] |= flag;
                orogeny[src] = now_my;
            }
            kind[c] = Margin::Suture as u8;
            let (ga, gb) = (root(&mut group, a), root(&mut group, b));
            pairs.push((ga.min(gb), ga.max(gb)));
        }
        pairs.sort_unstable();
        pairs.dedup();
        for (a, b) in pairs {
            let (ga, gb) = (root(&mut group, a), root(&mut group, b));
            if ga == gb {
                continue;
            }
            let (big, small) = if (area[ga], gb) >= (area[gb], ga) {
                (ga, gb)
            } else {
                (gb, ga)
            };
            let terrane = fragments[small].terrane && area[small] < 1_000_000;
            welds.push((now_my, big as u8, small as u8, terrane));
            group[small] = big;
            area[big] += area[small];
            if !terrane {
                motion[big].angle = motion[big].angle * s.weld_keep / 1000;
                motion[big].target = motion[big].target.or(motion[small].target);
            }
        }
        // Drawn groups stop at their downwelling; pushed ones slow.
        for f in 0..pieces {
            if group[f] != f {
                continue;
            }
            match motion[f].target {
                Some(d) => {
                    let centre = apply(&turn[f], fragments[f].centre);
                    if km_between(normalize(centre), d) < s.arrived_km {
                        motion[f].angle = 0;
                    }
                }
                None => motion[f].angle = motion[f].angle * s.disperse_keep / 1000,
            }
        }
        owners.push(owner);
        kinds.push(kind);
    }
    let last = owners.last().expect("at least one step");
    let piece = last.clone();
    let mut today_age = vec![0_u16; CELLS];
    let mut today_margins = vec![0_u8; CELLS];
    let mut today_orogeny = vec![NEVER; CELLS];
    let mut crust = vec![crust_cut - 400; CELLS];
    for c in 0..CELLS {
        if piece[c] == NONE {
            continue;
        }
        let src = source[c] as usize;
        today_age[c] = age[src];
        today_margins[c] = margins[src];
        today_orogeny[c] = orogeny[src];
        crust[c] = field[src].max(crust_cut);
    }
    let groups: Vec<u8> = (0..pieces).map(|f| root(&mut group, f) as u8).collect();
    let final_motion: Vec<Motion> = (0..pieces).map(|f| motion[root(&mut group, f)]).collect();
    History {
        upwelling,
        downwellings,
        fragments,
        origin,
        owner: owners,
        margin: kinds,
        group: groups,
        motion: final_motion,
        source,
        piece,
        age_my: today_age,
        margins: today_margins,
        orogeny_my: today_orogeny,
        welds,
        crust,
        crust_cut,
    }
}
