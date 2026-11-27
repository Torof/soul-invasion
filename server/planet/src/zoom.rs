//! Our continent, shaped across its margin: trench, coast range, forearc
//! basin, volcanic arc, back-arc; its lines, settings and numbers.

use crate::*;

/// Pixels a side of the zoom on our continent.
pub const ZOOM_SIDE: usize = 640;
/// A zoom pixel, km.
pub const ZOOM_PX_KM: i64 = 2;

/// The zoom's frame: a tangent plane at our continent's middle.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    /// The middle.
    pub centre: V3,
    /// East at the middle.
    pub east: V3,
    /// North at the middle.
    pub north: V3,
}

impl Frame {
    /// The point of a pixel's centre.
    pub fn point(&self, i: usize) -> V3 {
        let (x, y) = Self::xy(i);
        normalize(add(
            add(self.centre, scale(self.east, x * ONE / RADIUS_KM)),
            scale(self.north, y * ONE / RADIUS_KM),
        ))
    }

    /// A pixel's position east and north of the middle, km.
    pub fn xy(i: usize) -> (i64, i64) {
        let half = ZOOM_SIDE as i64 / 2;
        let (px, py) = ((i % ZOOM_SIDE) as i64, (i / ZOOM_SIDE) as i64);
        ((px - half) * ZOOM_PX_KM + 1, (half - py) * ZOOM_PX_KM - 1)
    }

    /// The pixel holding a point, if the frame holds it.
    pub fn pixel_of(&self, p: V3) -> Option<usize> {
        let dn = dot(p, self.centre);
        if dn <= 0 {
            return None;
        }
        let x = dot(p, self.east) * RADIUS_KM / dn;
        let y = dot(p, self.north) * RADIUS_KM / dn;
        let half = ZOOM_SIDE as i64 / 2;
        let px = x.div_euclid(ZOOM_PX_KM) + half;
        let py = half - 1 - y.div_euclid(ZOOM_PX_KM);
        let side = ZOOM_SIDE as i64;
        (0..side).contains(&px).then_some(())?;
        (0..side).contains(&py).then_some(())?;
        Some(py as usize * ZOOM_SIDE + px as usize)
    }
}

/// A line drawn on the zoom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    /// Nothing.
    None,
    /// Our trench.
    Trench,
    /// Another convergent boundary.
    Convergent,
    /// A ridge or a rift of today.
    Divergent,
    /// A transform fault of today.
    Transform,
    /// An old suture.
    Suture,
    /// An old failed rift.
    FailedRift,
}

/// The geological setting of a zoom pixel: what the rock layer will read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// Open ocean floor (basalt under mud).
    Ocean,
    /// The trench and the sea floor sinking into it.
    Trench,
    /// The outer-arc high, the coast range: scraped-off sea sediments,
    /// sandstone and mudstone, slices of ocean floor.
    CoastRange,
    /// The forearc basin between the coast range and the volcanoes:
    /// sandstone and silt, lowlands, an inland sea where it is drowned.
    ForearcBasin,
    /// The volcanic arc: andesite, basalt, tuff; granite in its roots.
    Arc,
    /// Behind the arc, the uplands: lavas, folded older rock.
    BackArc,
    /// A basin behind the arc: limestone, chalk with flint, clay.
    BackArcBasin,
    /// The old core of the continent: granite and gneiss.
    OldCore,
    /// An old rift's trough: sandstone, salt, long lakes.
    FailedRift,
    /// An old suture: worn hills of granite and schist.
    Suture,
    /// A low coastal plain on the quiet east coast: deltas and marshes.
    CoastalPlain,
    /// A rift of today crossing the land: basalt, salt, hot springs.
    Rift,
    /// A range raised where two plates of today meet across the land.
    Range,
    /// A hotspot's lavas.
    Hotspot,
}

/// What a zoom measures, for the lead's width question.
#[derive(Debug, Clone, Default)]
pub struct ZoomNumbers {
    /// Land of our continent, km².
    pub land_km2: i64,
    /// Land outside it (islands of the arc, hotspots), km².
    pub islands_km2: i64,
    /// Its land's extent along the chain, km.
    pub length_km: i64,
    /// Its land's mean width (area over length), km.
    pub mean_width_km: i64,
    /// The farthest a piece of its land lies from the sea, km.
    pub farthest_sea_km: i64,
    /// Shares of its land farther than 50 and 100 km from the sea, per mille.
    pub far_50: i64,
    /// See `far_50`.
    pub far_100: i64,
    /// Mean width of its land east of the volcanic crest (the lee), km.
    pub lee_km: i64,
    /// Its land east of the crest's lee slope (more than 60 km past the
    /// crest) and more than 40 km from any sea: room for a dry interior, km².
    pub interior_km2: i64,
    /// Volcanoes standing on land.
    pub volcanoes: i64,
    /// Sea enclosed by its land (not touching the frame), km².
    pub enclosed_sea_km2: i64,
    /// Land over sedimentary basins (forearc, back-arc, rift, coastal
    /// plain), km².
    pub basin_km2: i64,
    /// Land where the chalk and limestone platform can lie (back-arc
    /// basins and coastal plain), km².
    pub platform_km2: i64,
    /// Land over the old core, km².
    pub old_core_km2: i64,
    /// Low flat land where marshes will come, km².
    pub marsh_km2: i64,
    /// Highest land, m.
    pub highest_m: i64,
}

/// The zoom on our continent at one width.
pub struct Zoom {
    /// The frame.
    pub frame: Frame,
    /// Height above the sea, m, per pixel.
    pub elevation: Vec<i32>,
    /// Setting per pixel.
    pub setting: Vec<Setting>,
    /// Lines per pixel.
    pub line: Vec<Line>,
    /// Whether a pixel is our continent's crust.
    pub ours: Vec<bool>,
    /// Marshes to come, per pixel.
    pub marsh: Vec<bool>,
    /// Latitude per pixel, tenths of a degree.
    pub latitude: Vec<i16>,
    /// Volcanoes, as pixels.
    pub volcanoes: Vec<usize>,
    /// Hotspot chain points inside the frame, as pixels, with their age.
    pub hotspot_points: Vec<(usize, i64)>,
    /// The trench to volcanic arc distance, km.
    pub arc_km: i64,
    /// The trench to west coast distance, km.
    pub coast_km: i64,
    /// Closing speed at the trench, mm a year.
    pub rate: i64,
    /// The numbers.
    pub numbers: ZoomNumbers,
}

/// The coast's wander at the zoom's scale, `(cell, amplitude)` in
/// [`ONE`] units (~90, 32, 10 and 3 km): the grid's 100-km cells read as
/// a natural coast, not a staircase.
pub(crate) const COAST_WARP: [(i64, i64); 4] = [
    (120_000, 14_000),
    (40_000, 5_000),
    (12_000, 1_500),
    (4_000, 500),
];
/// A grid cell votes for land or sea within this, km.
pub(crate) const COAST_VOTE_KM: i64 = 150;
/// Trench to west coast, km (Cascadia: ~100; Chile: 60–130).
pub(crate) const COAST_KM: (i64, i64) = (55, 90);
/// Trench to volcanic arc, km: 100 km of depth to the sinking slab over
/// the slab's dip (25° → 215 km, 45° → 100 km); Cascadia ~250, Chile ~250.
pub(crate) const ARC_KM: (i64, i64) = (150, 230);
/// Between two volcanoes of the arc, km (Cascades: 40–100).
pub(crate) const VOLCANO_SPACING_KM: (i64, i64) = (40, 85);

impl Zoom {
    /// Our continent as the history left it: the land of `site`'s piece
    /// (and any other land in the frame), read across its margin.
    pub fn new(planet: &Planet, site: &Site) -> Self {
        let seed = hash(&[planet.seed, P_ZOOM]);
        let centre = planet.lands[site.land].centre;
        let east = east_at(centre);
        let frame = Frame {
            centre,
            east,
            north: north_at(centre, east),
        };
        let n = ZOOM_SIDE * ZOOM_SIDE;
        let points: Vec<V3> = (0..n).map(|i| frame.point(i)).collect();
        let cur: Vec<u8> = points
            .iter()
            .map(|&p| planet.plates.plate_at(p) as u8)
            .collect();
        let old: Vec<u8> = points
            .iter()
            .map(|&p| planet.old.plate_at(p) as u8)
            .collect();
        let latitude: Vec<i16> = points.iter().map(|&p| latitude(p) as i16).collect();
        // The land the history left, cell by cell, read at each pixel: the
        // cells around vote, nearer ones more, at a gently wandering point.
        let mut piece = vec![false; CELLS];
        for &c in &planet.lands[site.land].cells {
            piece[c as usize] = true;
        }
        let floor = cos(COAST_VOTE_KM * ONE / RADIUS_KM);
        let coast_seed = hash(&[seed, 0xc0a5]);
        let (land_px, ours_px): (Vec<bool>, Vec<bool>) = points
            .iter()
            .map(|&p| {
                let q = warp(coast_seed, p, &COAST_WARP);
                let c = planet.sphere.cell_of(q);
                let (mut mine, mut other, mut sea) = (0, 0, 0);
                let around = std::iter::once(c)
                    .chain(planet.sphere.links[c].iter().map(|&(n, _)| n as usize));
                for k in around {
                    let w = (dot(q, planet.sphere.centre[k]) - floor).max(0);
                    if piece[k] {
                        mine += w;
                    } else if planet.elevation[k] > 0 {
                        other += w;
                    } else {
                        sea += w;
                    }
                }
                let land = mine + other > sea;
                (land, land && mine >= other)
            })
            .unzip();
        let sea_px: Vec<bool> = land_px.iter().map(|&l| !l).collect();
        let d_in = chamfer(&sea_px);
        let d_out = chamfer(&land_px);
        let (sink, ride) = (site.sinking as u8, site.riding as u8);
        // Boundaries of today and of old, per pixel.
        let mut line = vec![Line::None; n];
        let mut trench = vec![false; n];
        let mut trench_sink = vec![false; n];
        let mut sutures = vec![false; n];
        let mut rifts = vec![false; n];
        for i in 0..n {
            let (px, py) = (i % ZOOM_SIDE, i / ZOOM_SIDE);
            for (j, dir) in [
                (i + 1, frame.east),
                (i + ZOOM_SIDE, frame.north.map(|v| -v)),
            ] {
                if (j == i + 1 && px + 1 >= ZOOM_SIDE)
                    || (j == i + ZOOM_SIDE && py + 1 >= ZOOM_SIDE)
                {
                    continue;
                }
                let (a, b) = (cur[i], cur[j]);
                if a != b {
                    if (a, b) == (ride, sink) || (a, b) == (sink, ride) {
                        let (r, s) = if a == ride { (i, j) } else { (j, i) };
                        trench[r] = true;
                        trench_sink[s] = true;
                        line[i] = Line::Trench;
                    } else {
                        let (closing, slide) =
                            planet.plates.motion(a as usize, b as usize, points[i], dir);
                        line[i] = match classify(closing, slide, false, false, true) {
                            Kind::Ridge | Kind::Rift => Line::Divergent,
                            Kind::Transform => Line::Transform,
                            _ => Line::Convergent,
                        };
                    }
                }
                let (a, b) = (old[i], old[j]);
                if a != b && line[i] == Line::None {
                    let (closing, slide) =
                        planet.old.motion(a as usize, b as usize, points[i], dir);
                    match classify(closing, slide, true, true, false) {
                        Kind::Collision => {
                            sutures[i] = true;
                            line[i] = Line::Suture;
                        }
                        Kind::Rift => {
                            rifts[i] = true;
                            line[i] = Line::FailedRift;
                        }
                        _ => {}
                    }
                }
            }
        }
        let d_ride = chamfer(&trench);
        let d_sink = chamfer(&trench_sink);
        let d_suture = chamfer(&sutures);
        let d_rift = chamfer(&rifts);
        let of_line = |l: Line| chamfer(&line.iter().map(|&x| x == l).collect::<Vec<_>>());
        let (d_parting, d_meeting, d_sliding) = (
            of_line(Line::Divergent),
            of_line(Line::Convergent),
            of_line(Line::Transform),
        );
        // The chain's direction: the trench's main axis near the middle.
        let (mut sx, mut sy, mut sxx, mut syy, mut sxy, mut count) =
            (0_i64, 0_i64, 0_i64, 0_i64, 0_i64, 0_i64);
        for i in 0..n {
            if trench[i] {
                let (x, y) = Frame::xy(i);
                if y.abs() <= 400 {
                    sx += x;
                    sy += y;
                    sxx += x * x;
                    syy += y * y;
                    sxy += x * y;
                    count += 1;
                }
            }
        }
        let (ax, ay) = if count > 10 {
            let (mx, my) = (sx / count, sy / count);
            let cxx = sxx / count - mx * mx;
            let cyy = syy / count - my * my;
            let cxy = sxy / count - mx * my;
            let theta = angle_of(cxx - cyy, 2 * cxy) / 2;
            let (c, s) = (cos(tenths(theta)) / 1000, sin(tenths(theta)) / 1000);
            if s < 0 { (-c, -s) } else { (c, s) }
        } else {
            (0, 1000)
        };
        // The cross-section's numbers for this margin.
        let coast_km = draw(seed, 1, 0, COAST_KM.0, COAST_KM.1);
        let arc_km = draw(seed, 2, 0, ARC_KM.0, ARC_KM.1);
        let rate = site.rate;
        let squeezed = rate >= 45;
        let half_len = LENGTH_KM / 2;
        // Volcanoes along the arc, in (along, across) km.
        let mut cones: Vec<(i64, i64)> = Vec::new();
        let mut s = -half_len - 400 + draw(seed, 3, 0, 0, 40);
        let mut k = 0;
        while s < half_len + 400 {
            cones.push((s, arc_km + draw(seed, 4, k, -14, 14)));
            s += draw(seed, 5, k, VOLCANO_SPACING_KM.0, VOLCANO_SPACING_KM.1);
            k += 1;
        }
        let hotspot_points: Vec<(usize, i64)> = planet
            .hotspots
            .iter()
            .flat_map(|h| {
                h.track
                    .iter()
                    .filter_map(|&(x, age)| frame.pixel_of(x).map(|i| (i, age)))
            })
            .collect();
        let sig = |v: i64| v - 500;
        let mut elevation = vec![0_i32; n];
        let mut setting = vec![Setting::Ocean; n];
        let mut ours = vec![false; n];
        let mut along_of = vec![0_i64; n];
        let mut across_of = vec![0_i64; n];
        for i in 0..n {
            let (x, y) = Frame::xy(i);
            let along = (x * ax + y * ay) / 1000;
            let p = points[i];
            let pl = cur[i];
            // Across the margin, km: positive on the riding side.
            // A third plate's ground counts as the riding side: our
            // continent may straddle its border, which then crosses it.
            let d = if pl == sink {
                -(i64::from(d_sink[i]) / 1000)
            } else {
                i64::from(d_ride[i]) / 1000
            };
            along_of[i] = along;
            across_of[i] = d;
            let n1 = |purpose: u64, cell: i64| {
                sig(noise3(
                    hash(&[seed, purpose]),
                    [along * 1000, 0, 0],
                    cell * 1000,
                ))
            };
            let n2 = |purpose: u64, cell: i64| {
                sig(noise3(hash(&[seed, purpose]), p, cell * ONE / RADIUS_KM))
            };
            let inner = (n2(26, 50) * 60 + n2(27, 15) * 25) / 1000;
            let db = d + inner;
            let inside = land_px[i];
            // Into the land from its coast, and out to sea from it, km.
            let (inland, offshore) = (i64::from(d_in[i]) / 1000, i64::from(d_out[i]) / 1000);
            // Ocean floor, the trench and the arc's ridge, everywhere.
            let mut e: i64 = if d <= -1000 {
                -4_200
            } else if d < 0 {
                -4_600 - 2_800 * bump(d, 70) / 1000
            } else {
                -3_400 - 3_400 * bump(d, 70) / 1000
            };
            let rough = (n2(20, 18) * 2 + n2(21, 6)) / 6;
            if d > -1000 {
                e += 2_300 * bump(d - arc_km, 50) / 1000;
                e += 1_900 * bump(d - (coast_km + 25), 38) / 1000;
                for &(cs, cd) in &cones {
                    if (along - cs).abs() < 30 {
                        let r = ((along - cs).pow(2) + (d - cd).pow(2)).isqrt();
                        e += 1_900 * bump(r, 16) / 1000;
                    }
                }
            }
            let mut set = if d <= -1000 {
                Setting::Ocean
            } else if d < 80 {
                Setting::Trench
            } else {
                Setting::Ocean
            };
            if inside {
                let edge = inland;
                let gap = n1(16, 70);
                let mut base: i64 = 260 + rough;
                set = Setting::BackArc;
                let fore_lo = coast_km + 55;
                let fore_hi = arc_km - 38;
                // Each band fades into the next over ~15–40 km: no step.
                let ramp = |v: i64, over: i64| (v * 1000 / over).clamp(0, 1000);
                if db < fore_lo {
                    set = Setting::CoastRange;
                    // Where the coast range sags, the sea comes through.
                    if gap > 330 {
                        base -= 900 + 2_000 * bump(d - (coast_km + 25), 38) / 1000;
                    }
                }
                let fore = ramp(db - fore_lo, 15) * ramp(fore_hi - db, 15) / 1000;
                if fore > 0 {
                    let floor = -110 + n1(17, 90) * 800 / 1000 + n2(18, 26) * 3 / 4 + n2(31, 9) / 4;
                    base = (base * (1000 - fore) + floor * fore) / 1000;
                    if fore > 500 {
                        set = Setting::ForearcBasin;
                    }
                }
                if db >= fore_hi && db < arc_km + 45 {
                    set = Setting::Arc;
                }
                // Behind the arc: an upland rising off the volcanoes' back
                // and sinking toward the east coast.
                let behind = db - arc_km - 45;
                let plateau = if squeezed { 950 } else { 450 };
                base += plateau * ramp(db - arc_km, 45) / 1000 * (1000 - ramp(behind, 200)) / 1000;
                if behind >= 0 {
                    let basin = n2(19, 160) + n2(32, 50) / 3;
                    let sag = ramp(-60 - basin, 160);
                    if sag > 0 {
                        // A flat floor; only the deepest sag is drowned.
                        let floor = 40 + (basin + 330) * 2 / 3 + rough / 8;
                        base = (base * (1000 - sag) + floor * sag) / 1000;
                        if sag > 300 {
                            set = Setting::BackArcBasin;
                        }
                    }
                    let core = n2(22, 130) + n2(33, 45) / 3;
                    let rise = ramp(core - 80, 200) * ramp(behind - 40, 60) / 1000;
                    if rise > 0 {
                        base += 420 * rise / 1000;
                        if rise > 200 {
                            set = Setting::OldCore;
                        }
                    }
                    if inland < 45 && base < 350 {
                        set = Setting::CoastalPlain;
                        base = base.min(4 + inland * 2 + rough / 12);
                    }
                }
                let sut = i64::from(d_suture[i]) / 1000;
                if sut < 40 {
                    base += 320 * bump(sut, 40) / 1000;
                    if !matches!(set, Setting::Arc | Setting::CoastRange) {
                        set = Setting::Suture;
                    }
                }
                let rift = i64::from(d_rift[i]) / 1000;
                if rift < 45 {
                    base += -900 * bump(rift, 24) / 1000 + 260 * bump(rift - 32, 14) / 1000;
                    if rift < 24 && set != Setting::Arc {
                        set = Setting::FailedRift;
                    }
                }
                // Today's other borders, where they cross our land: a rift
                // opens a trough the sea may enter, a meeting raises a
                // range, a transform cuts a straight valley.
                let parting = i64::from(d_parting[i]) / 1000;
                if parting < 70 {
                    base += -1_400 * bump(parting, 34) / 1000 + 380 * bump(parting - 48, 20) / 1000;
                    if parting < 34 {
                        set = Setting::Rift;
                    }
                }
                let meeting = i64::from(d_meeting[i]) / 1000;
                if meeting < 60 {
                    base += 1_500 * bump(meeting, 60) / 1000;
                    set = Setting::Range;
                }
                let sliding = i64::from(d_sliding[i]) / 1000;
                if sliding < 12 {
                    base -= 260 * bump(sliding, 12) / 1000;
                }
                // The shore: a shelf just off the coast, low ground along it.
                let shore = (edge * 1000 / 12).clamp(0, 1000);
                let land = base + (e + 3_400).max(0) * 1000 / 1000;
                e = (land * shore + (-80) * (1000 - shore)) / 1000;
                ours[i] = ours_px[i];
            } else if offshore < 200 {
                // Just off the coasts, the shelf falls to the deep.
                e = e.max(-80 - offshore * 70);
            }
            // Hotspots: islands and seamounts in the ocean, lavas on land.
            for &(hp, age) in &hotspot_points {
                let (hx, hy) = Frame::xy(hp);
                let r = ((x - hx).pow(2) + (y - hy).pow(2)).isqrt();
                if inside {
                    if r < 70 {
                        e += 700 * bump(r, 70) / 1000 / (1 + age / 15);
                        set = Setting::Hotspot;
                    }
                } else if r < 25 {
                    e += 4_600 * bump(r, 25) / 1000 * (TRACK_MYR - age).max(0) / TRACK_MYR;
                }
            }
            elevation[i] = e.clamp(-9_000, 7_000) as i32;
            setting[i] = set;
        }
        // Volcanoes standing above the sea, at their summit pixel.
        let mut volcanoes = Vec::new();
        for &(cs, cd) in &cones {
            let mut best: Option<(i32, usize)> = None;
            for i in 0..n {
                if (along_of[i] - cs).abs() <= 6
                    && (across_of[i] - cd).abs() <= 6
                    && elevation[i] > 0
                {
                    let e = elevation[i];
                    if best.is_none_or(|(b, _)| e > b) {
                        best = Some((e, i));
                    }
                }
            }
            if let Some((_, i)) = best {
                volcanoes.push(i);
            }
        }
        // Marshes to come: low flat land near water or in a basin.
        let sea: Vec<bool> = elevation.iter().map(|&e| e <= 0).collect();
        let d_water = chamfer(&sea);
        let d_sea = chamfer(&open_sea(&elevation));
        let mut marsh = vec![false; n];
        for i in 0..n {
            if elevation[i] <= 0 || elevation[i] > 60 {
                continue;
            }
            let near_sea = d_water[i] <= 30_000;
            let basin = matches!(
                setting[i],
                Setting::ForearcBasin
                    | Setting::BackArcBasin
                    | Setting::CoastalPlain
                    | Setting::FailedRift
            );
            if near_sea || basin {
                let (px, py) = (i % ZOOM_SIDE, i / ZOOM_SIDE);
                let mut flat = true;
                for (dx, dy) in [(-2_i64, 0_i64), (2, 0), (0, -2), (0, 2)] {
                    let qx = px as i64 + dx;
                    let qy = py as i64 + dy;
                    if (0..ZOOM_SIDE as i64).contains(&qx) && (0..ZOOM_SIDE as i64).contains(&qy) {
                        let j = qy as usize * ZOOM_SIDE + qx as usize;
                        if (elevation[j] - elevation[i]).abs() > 30 {
                            flat = false;
                        }
                    }
                }
                marsh[i] = flat;
            }
        }
        let mut zoom = Self {
            frame,
            elevation,
            setting,
            line,
            ours,
            marsh,
            latitude,
            volcanoes,
            hotspot_points,
            arc_km,
            coast_km,
            rate,
            numbers: ZoomNumbers::default(),
        };
        zoom.numbers = zoom.measure(&along_of, &across_of, &d_sea);
        zoom
    }

    fn measure(&self, along: &[i64], across: &[i64], d_sea: &[u32]) -> ZoomNumbers {
        let n = ZOOM_SIDE * ZOOM_SIDE;
        let px = ZOOM_PX_KM * ZOOM_PX_KM;
        let mut m = ZoomNumbers::default();
        let (mut lo, mut hi) = (i64::MAX, i64::MIN);
        let mut lee = 0;
        let (mut far50, mut far100) = (0, 0);
        let mut land_px = 0;
        for i in 0..n {
            let e = i64::from(self.elevation[i]);
            if e <= 0 {
                continue;
            }
            if !self.ours[i] {
                m.islands_km2 += px;
                continue;
            }
            land_px += 1;
            m.land_km2 += px;
            m.highest_m = m.highest_m.max(e);
            lo = lo.min(along[i]);
            hi = hi.max(along[i]);
            let sea_km = i64::from(d_sea[i]) / 1000;
            m.farthest_sea_km = m.farthest_sea_km.max(sea_km);
            far50 += i64::from(sea_km > 50);
            far100 += i64::from(sea_km > 100);
            if across[i] > self.arc_km {
                lee += 1;
            }
            if across[i] > self.arc_km + 60 && sea_km > 40 {
                m.interior_km2 += px;
            }
            match self.setting[i] {
                Setting::ForearcBasin | Setting::FailedRift | Setting::Rift => m.basin_km2 += px,
                Setting::BackArcBasin | Setting::CoastalPlain => {
                    m.basin_km2 += px;
                    m.platform_km2 += px;
                }
                Setting::OldCore => m.old_core_km2 += px,
                _ => {}
            }
            if self.marsh[i] {
                m.marsh_km2 += px;
            }
        }
        if land_px > 0 {
            m.length_km = hi - lo;
            m.mean_width_km = m.land_km2 / m.length_km.max(1);
            m.lee_km = lee * px / m.length_km.max(1);
            m.far_50 = far50 * 1000 / land_px;
            m.far_100 = far100 * 1000 / land_px;
        }
        m.volcanoes = self.volcanoes.iter().filter(|&&i| self.ours[i]).count() as i64;
        let seen = open_sea(&self.elevation);
        m.enclosed_sea_km2 = (0..n)
            .filter(|&i| self.elevation[i] <= 0 && !seen[i])
            .count() as i64
            * px;
        m
    }
}

/// The water joined to the frame's edge: the open ocean, not the lakes
/// and closed seas.
pub fn open_sea(elevation: &[i32]) -> Vec<bool> {
    let n = ZOOM_SIDE * ZOOM_SIDE;
    let mut seen = vec![false; n];
    let mut queue: Vec<usize> = (0..n)
        .filter(|&i| {
            let (x, y) = (i % ZOOM_SIDE, i / ZOOM_SIDE);
            (x == 0 || y == 0 || x + 1 == ZOOM_SIDE || y + 1 == ZOOM_SIDE) && elevation[i] <= 0
        })
        .collect();
    for &i in &queue {
        seen[i] = true;
    }
    let mut k = 0;
    while k < queue.len() {
        let i = queue[k];
        k += 1;
        let (x, y) = (i % ZOOM_SIDE, i / ZOOM_SIDE);
        let mut next = Vec::with_capacity(4);
        if x > 0 {
            next.push(i - 1);
        }
        if x + 1 < ZOOM_SIDE {
            next.push(i + 1);
        }
        if y > 0 {
            next.push(i - ZOOM_SIDE);
        }
        if y + 1 < ZOOM_SIDE {
            next.push(i + ZOOM_SIDE);
        }
        for j in next {
            if !seen[j] && elevation[j] <= 0 {
                seen[j] = true;
                queue.push(j);
            }
        }
    }
    seen
}

/// Distance to the nearest set pixel, m, by a two-pass chamfer (2 km
/// straight, 2.83 km diagonal): within 5 % of the true distance.
pub fn chamfer(sources: &[bool]) -> Vec<u32> {
    let side = ZOOM_SIDE;
    let straight = (ZOOM_PX_KM * 1000) as u32;
    let diagonal = straight * 1414 / 1000;
    let far = u32::MAX / 2;
    let mut d: Vec<u32> = sources.iter().map(|&s| if s { 0 } else { far }).collect();
    for y in 0..side {
        for x in 0..side {
            let i = y * side + x;
            let mut v = d[i];
            if x > 0 {
                v = v.min(d[i - 1] + straight);
            }
            if y > 0 {
                v = v.min(d[i - side] + straight);
                if x > 0 {
                    v = v.min(d[i - side - 1] + diagonal);
                }
                if x + 1 < side {
                    v = v.min(d[i - side + 1] + diagonal);
                }
            }
            d[i] = v;
        }
    }
    for y in (0..side).rev() {
        for x in (0..side).rev() {
            let i = y * side + x;
            let mut v = d[i];
            if x + 1 < side {
                v = v.min(d[i + 1] + straight);
            }
            if y + 1 < side {
                v = v.min(d[i + side] + straight);
                if x + 1 < side {
                    v = v.min(d[i + side + 1] + diagonal);
                }
                if x > 0 {
                    v = v.min(d[i + side - 1] + diagonal);
                }
            }
            d[i] = v;
        }
    }
    d
}
