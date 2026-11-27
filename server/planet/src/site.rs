//! Our continent's site: a subduction margin at 40-50° north with the ocean
//! to the west.

use crate::*;

/// Other land within this of our continent's middle is counted, km.
pub const CLEAR_KM: i64 = 1_100;

/// The plate across a boundary cell (the one most of its foreign
/// neighbours belong to, ties to the lower), the normal toward it, and
/// whether its crust there is continental.
pub(crate) fn across(
    sphere: &Sphere,
    layout: &Layout,
    plate: &[u8],
    continental: &[bool],
    c: usize,
) -> Option<(usize, V3, bool)> {
    let mine = plate[c];
    let mut counts = [0_u8; history::MAXP];
    for &(n, _) in &sphere.links[c] {
        let o = plate[n as usize];
        if o != mine {
            counts[o as usize] += 1;
        }
    }
    let (other, &most) = counts
        .iter()
        .enumerate()
        .max_by_key(|&(k, &n)| (n, Reverse(k)))?;
    if most == 0 {
        return None;
    }
    let p = sphere.centre[c];
    let mut dir = [0_i64; 3];
    let mut cont = 0;
    for &(n, _) in &sphere.links[c] {
        if plate[n as usize] as usize == other {
            dir = add(dir, sub(sphere.centre[n as usize], p));
            cont += i32::from(continental[n as usize]);
        }
    }
    // The normal from the plates themselves, looked at all around the
    // cell: the grid's eight neighbours alone give only a few directions,
    // and those favour some cells over others.
    let east = east_at(p);
    let north = north_at(p, east);
    let mut around = [0_i64; 3];
    for k in 0..NORMAL_LOOKS {
        let a = tenths(k * 3600 / NORMAL_LOOKS);
        let d = add(scale(east, cos(a)), scale(north, sin(a)));
        if layout.plate_at(step(p, d, NORMAL_REACH_KM)) == other {
            around = add(around, d);
        }
    }
    let normal = if around == [0, 0, 0] {
        // Keep the normal tangent: remove its part along `p`.
        let radial = dot(dir, p);
        normalize(sub(dir, scale(p, radial)))
    } else {
        normalize(around)
    };
    Some((other, normal, 2 * cont >= i32::from(most)))
}

/// Directions looked at around a boundary cell for its normal, and how
/// far, km (a little more than a cell).
pub(crate) const NORMAL_LOOKS: i64 = 24;
pub(crate) const NORMAL_REACH_KM: i64 = 130;

/// Our continent's length along its chain, km (the lead's ~550).
pub const LENGTH_KM: i64 = 550;
/// Its middle's latitudes north, tenths of a degree (the lead's 40–50°
/// N, north the cold side since 26 November).
pub(crate) const SITE_LATITUDE: (i64, i64) = (400, 500);
/// Our continent's area, km²: the lead's ~210,000, and what is accepted
/// (a piece the history left, not drawn: 26 November, version 4.1).
pub(crate) const PIECE_GOAL_KM2: i64 = 210_000;
pub(crate) const PIECE_KM2: std::ops::RangeInclusive<i64> = 150_000..=300_000;
/// A strait cut to part our continent from a larger land, m.
pub(crate) const STRAIT_M: i32 = -60;
/// Its trench lies at most this far west of its land, km.
pub(crate) const TRENCH_REACH_KM: i64 = 500;
/// How far the margin may turn from north–south: the cosine between the
/// trench's normal and the east, per mille (45°).
pub(crate) const SITE_FACING: i64 = 707;
/// Land a site may sink around it, km² (more, and it is a last resort).
/// How far west the ocean is looked at, km: the westerlies' fetch.
pub(crate) const FETCH_KM: i64 = 3_000;

/// Where our continent stands: a piece of land the history left at
/// 40–50° north, with a trench to its west (the ocean sinking under it).
#[derive(Debug, Clone)]
pub struct Site {
    /// The piece: its index in [`Planet::lands`].
    pub land: usize,
    /// Its area, km².
    pub area_km2: i64,
    /// From its trench to its nearest land, km.
    pub trench_km: i64,
    /// The trench cell.
    pub cell: usize,
    /// The sinking plate (west).
    pub sinking: usize,
    /// The riding plate (east), ours.
    pub riding: usize,
    /// The trench point.
    pub origin: V3,
    /// Latitude and longitude of the trench point, tenths of a degree.
    pub lat_lon: (i64, i64),
    /// Closing speed, mm a year.
    pub rate: i64,
    /// Cosine between the trench's normal and the east, per mille.
    pub facing: i64,
    /// Share of ocean along the fetch to the west, per mille.
    pub ocean_west: i64,
    /// Land within [`CLEAR_KM`] that has to be sunk, km².
    pub land_near_km2: i64,
    /// Old sutures and failed rifts within 300 km of the middle, cells.
    pub old_near: (u32, u32),
    /// Other boundaries (transforms, ridges) within 500 km, cells.
    pub ends_near: u32,
    /// A hotspot or its chain within 700 km.
    pub hotspot_near: bool,
    /// Its score.
    pub score: i64,
}

impl Site {
    /// Every candidate, best first: each piece of land of
    /// [`PIECE_KM2`] whose middle is at [`SITE_LATITUDE`], with a trench
    /// within [`TRENCH_REACH_KM`] to its west, the ocean sinking east
    /// under it.
    pub fn candidates(planet: &Planet) -> Vec<Self> {
        let s = &planet.sphere;
        let mut out = Vec::new();
        for (k, land) in planet.lands.iter().enumerate() {
            let lat = latitude(land.centre);
            if !PIECE_KM2.contains(&land.area_km2) || lat < SITE_LATITUDE.0 || lat > SITE_LATITUDE.1
            {
                continue;
            }
            let near = cos((land.span_km / 2 + TRENCH_REACH_KM) * ONE / RADIUS_KM);
            // The trench cell nearest the piece, west of it, facing east.
            let mut best: Option<(i64, usize, Edge, i64)> = None;
            for c in 0..CELLS {
                let Some(e) = planet.edge[c] else { continue };
                let p = s.centre[c];
                if e.kind != Kind::Trench || dot(p, land.centre) < near {
                    continue;
                }
                let east = east_at(p);
                let facing = dot(e.normal, east) * 1000 / ONE;
                if facing < SITE_FACING || dot(sub(land.centre, p), east) <= 0 {
                    continue;
                }
                let d = land
                    .cells
                    .iter()
                    .map(|&l| km_between(p, s.centre[l as usize]))
                    .min()
                    .unwrap_or(i64::MAX);
                if d <= TRENCH_REACH_KM && best.is_none_or(|(b, bc, _, _)| (d, c) < (b, bc)) {
                    best = Some((d, c, e, facing));
                }
            }
            let Some((trench_km, c, e, facing)) = best else {
                continue;
            };
            let p = s.centre[c];
            let mut ocean = 0;
            let mut seen = 0;
            let mut x = p;
            for _ in 0..FETCH_KM / 100 {
                x = step(x, east_at(x).map(|v| -v), 100);
                seen += 1;
                ocean += i64::from(planet.elevation[s.cell_of(x)] < 0);
            }
            let ocean_west = ocean * 1000 / seen;
            let middle = land.centre;
            let mut land_near_km2 = 0;
            let mut old_near = (0, 0);
            let mut ends_near = 0;
            for q in 0..CELLS {
                let at = s.centre[q];
                if dot(at, middle) < cos(CLEAR_KM * ONE / RADIUS_KM) {
                    continue;
                }
                let d = km_between(at, middle);
                if planet.elevation[q] > 0 && !land.cells.contains(&(q as u32)) {
                    land_near_km2 += i64::from(s.area_km2[q]);
                }
                if d <= 300 {
                    match planet.old_edge[q] {
                        Some(OldKind::Suture) => old_near.0 += 1,
                        Some(OldKind::FailedRift) => old_near.1 += 1,
                        _ => {}
                    }
                }
                if d <= 500
                    && planet.edge[q].is_some_and(|b| {
                        matches!(b.kind, Kind::Transform | Kind::Ridge | Kind::Rift)
                    })
                {
                    ends_near += 1;
                }
            }
            let hotspot_near = planet
                .hotspots
                .iter()
                .any(|h| h.track.iter().any(|&(x, _)| km_between(x, middle) < 700));
            let rate = i64::from(e.rate);
            let score = ocean_west - (land.area_km2 - PIECE_GOAL_KM2).abs() / 500 - trench_km
                + facing / 4
                - (lat - 450).abs();
            out.push(Self {
                land: k,
                area_km2: land.area_km2,
                trench_km,
                cell: c,
                sinking: planet.plate[c] as usize,
                riding: e.other as usize,
                origin: p,
                lat_lon: (latitude(p), longitude(p)),
                rate,
                facing,
                ocean_west,
                land_near_km2,
                old_near,
                ends_near,
                hotspot_near,
                score,
            });
        }
        out.sort_by_key(|x| (Reverse(x.score), x.land));
        out
    }

    /// The best candidate, if the planet has one.
    pub fn find(planet: &Planet) -> Option<Self> {
        Self::candidates(planet).into_iter().next()
    }
}
