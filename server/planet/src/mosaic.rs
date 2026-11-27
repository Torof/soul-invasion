//! The crust a supercontinent is made of: a mosaic. Old, cold, strong
//! cores of many sizes (cratons), welded by belts of younger crust with
//! a real width (old mountain belts, old troughs, old shear zones): what
//! older plates left when they met, began to part or slid. A belt's
//! crust is youngest along its axis and grades to its cores' age at its
//! edges. Nothing here says what a rift does: the crust's age is read by
//! the rifts as any crust's age is (`history/rifts.rs`, `weak`).

use crate::*;

/// The older plates over the whole sphere, whose pieces of crust are the
/// cores: a supercontinent of a quarter of the sphere holds seven to
/// eleven of them, whole or in part.
pub(crate) const CORES: (i64, i64) = (30, 46);
/// The warp of the cores' borders, `(cell, amplitude)` in [`ONE`] units:
/// cores are rounded, uneven shapes and the belts wind between them
/// (several hundred km either way over 1,500 km), not a Voronoi's lines.
pub(crate) const CORE_WARP: [(i64, i64); 4] = [
    (600_000, 200_000),
    (250_000, 140_000),
    (100_000, 50_000),
    (40_000, 12_000),
];
/// A belt's half-width, km, drawn for each pair of cores: belts of 200
/// to 900 km across.
pub(crate) const BELT_HALF_KM: (i64, i64) = (100, 450);
/// The age of a core's crust at the start, My, drawn for each core, and
/// of a belt's crust along its axis, drawn for each pair of cores.
pub(crate) const CORE_AGE_MY: (i64, i64) = (2_400, 3_600);
pub(crate) const BELT_AGE_MY: (i64, i64) = (500, 1_300);
/// The grain of the ages, My either way, and its cells, [`ONE`] units.
pub(crate) const AGE_GRAIN_MY: i64 = 150;
pub(crate) const AGE_GRAIN: [(i64, i64); 2] = [(500_000, 2), (200_000, 1)];

/// The mosaic, at each cell of the grid: read where the supercontinent's
/// crust stands at the start, and carried with it.
#[derive(Debug, Clone)]
pub struct Mosaic {
    /// The core (the older plate).
    pub core: Vec<u8>,
    /// On a belt's axis: what the older boundary was.
    pub edge: Vec<Option<OldKind>>,
    /// How far into a belt, ‰: 1000 on its axis, 0 in a core.
    pub belt: Vec<u16>,
    /// What the belt is (the kind of the nearest cell of its axis).
    pub kind: Vec<Option<OldKind>>,
    /// The belt's half-width, km (0 in a core).
    pub half_km: Vec<u16>,
    /// The crust's age at the start, My.
    pub age_my: Vec<u16>,
    /// When the belt's crust was last worked, My before the start (its
    /// axis' age; 0 in a core).
    pub worked_my: Vec<u16>,
}

impl Mosaic {
    /// The mosaic of the planet born from `seed`, and the older plates
    /// it comes from.
    pub(crate) fn draw(seed: u64, sphere: &Sphere) -> (Layout, Self) {
        let old = Layout::draw(seed, P_OLD, CORES, &CORE_WARP);
        let core: Vec<u8> = sphere
            .centre
            .iter()
            .map(|&p| old.plate_at(p) as u8)
            .collect();
        let all = vec![true; CELLS];
        let seam: Vec<Option<(usize, OldKind)>> = (0..CELLS)
            .map(|c| {
                across(sphere, &old, &core, &all, c).map(|(other, normal, _)| {
                    let (closing, slide) =
                        old.motion(core[c] as usize, other, sphere.centre[c], normal);
                    let kind = match classify(closing, slide, true, true, false) {
                        Kind::Collision => OldKind::Suture,
                        Kind::Rift => OldKind::FailedRift,
                        _ => OldKind::OldFault,
                    };
                    (other, kind)
                })
            })
            .collect();
        let axis: Vec<(usize, u32)> = (0..CELLS)
            .filter(|&c| seam[c].is_some())
            .map(|c| (c, c as u32))
            .collect();
        let (d, from) = sphere.spread(&axis, |_, _| true, BELT_HALF_KM.1 as u32);
        let grain = hash(&[seed, P_OLD, 5]);
        let mut m = Self {
            core,
            edge: seam.iter().map(|s| s.map(|(_, kind)| kind)).collect(),
            belt: vec![0; CELLS],
            kind: vec![None; CELLS],
            half_km: vec![0; CELLS],
            age_my: vec![0; CELLS],
            worked_my: vec![0; CELLS],
        };
        for c in 0..CELLS {
            let me = u64::from(m.core[c]);
            let mut age = draw(seed, P_OLD + 4, me, CORE_AGE_MY.0, CORE_AGE_MY.1);
            if d[c] != u32::MAX {
                let b = from[c] as usize;
                let (other, kind) = seam[b].expect("a cell of an axis");
                let (lo, hi) = (
                    u64::from(m.core[b]).min(other as u64),
                    u64::from(m.core[b]).max(other as u64),
                );
                let pair = lo * history::MAXP as u64 + hi;
                let half = draw(seed, P_OLD + 3, pair * 2, BELT_HALF_KM.0, BELT_HALF_KM.1);
                let grade = bump(i64::from(d[c]), half);
                if grade > 0 {
                    let worked = draw(seed, P_OLD + 3, pair * 2 + 1, BELT_AGE_MY.0, BELT_AGE_MY.1);
                    age = worked + (age - worked) * (1000 - grade) / 1000;
                    m.belt[c] = grade as u16;
                    m.kind[c] = Some(kind);
                    m.half_km[c] = half as u16;
                    m.worked_my[c] = worked as u16;
                }
            }
            let rough = (fbm3(grain, sphere.centre[c], &AGE_GRAIN) - 500) * AGE_GRAIN_MY / 500;
            m.age_my[c] = (age + rough * i64::from(1000 - m.belt[c]) / 1000) as u16;
        }
        (old, m)
    }
}
