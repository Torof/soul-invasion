//! The planet: its history, today's plates, boundaries, hotspots, heights
//! and sea, its landmasses; the planet as it was at a step of its history
//! (`at_step`).

use crate::*;

/// A hotspot and its chain.
#[derive(Debug, Clone)]
pub struct Hotspot {
    /// The plume, fixed under the plates.
    pub at: V3,
    /// The chain it left, from the plume on, with each point's age in
    /// millions of years.
    pub track: Vec<(V3, i64)>,
}

/// A piece of land: a connected set of dry cells.
#[derive(Debug, Clone)]
pub struct Landmass {
    /// Its cells.
    pub cells: Vec<u32>,
    /// Its area, km².
    pub area_km2: i64,
    /// Its middle.
    pub centre: V3,
    /// Twice its farthest cell from the middle, km: a rough size.
    pub span_km: i64,
}

/// The planet.
pub struct Planet {
    /// Its seed.
    pub seed: u64,
    /// The grid.
    pub sphere: Sphere,
    /// Today's plates.
    pub plates: Layout,
    /// The older plates.
    pub old: Layout,
    /// Each cell's plate.
    pub plate: Vec<u8>,
    /// Each cell's older plate.
    pub old_plate: Vec<u8>,
    /// Each cell's crust value (the supercontinent's, read at its source).
    pub crust: Vec<i64>,
    /// The threshold of continental crust.
    pub crust_cut: i64,
    /// Whether a cell's crust is continental.
    pub continental: Vec<bool>,
    /// Boundary cells of today.
    pub edge: Vec<Option<Edge>>,
    /// Boundary cells of the older layout, carried with the crust.
    pub old_edge: Vec<Option<OldKind>>,
    /// Each cell's distance to the nearest boundary of its own plate, km,
    /// and that boundary's cell.
    pub near: Vec<(u32, u32)>,
    /// The hotspots.
    pub hotspots: Vec<Hotspot>,
    /// Each cell's height above the sea, m.
    pub elevation: Vec<i32>,
    /// The sea's level at this step, m above the heights' datum: the
    /// history's own ([`History::sea_at`]), the level at which the
    /// planet's water fills the basins of the relief (version 16).
    pub sea: i64,
    /// How much higher than today's the sea stands at this step, m.
    pub sea_rise_m: i64,
    /// The hollows: ground below the sea's level that no path of such
    /// ground links to the ocean (version 11). They are land, not sea: a
    /// basin that may hold a lake (a later layer decides), as the
    /// Caspian's depression is.
    pub basin: Vec<bool>,
    /// The same for each cell of each piece of continent.
    basin_tile: Vec<Vec<bool>>,
    /// The pieces of land, largest first.
    pub lands: Vec<Landmass>,
    /// Cells reshaped for our continent (none until [`Planet::shape`]).
    pub ours: Vec<bool>,
    /// The world's settings.
    pub settings: Settings,
    /// The plates' history that made the continents (shared with the
    /// planet at each of its steps).
    pub history: std::sync::Arc<History>,
    /// The step this planet shows (the history's last: today).
    pub step: usize,
    /// The index of this step's pieces of continent, to read them at any
    /// point.
    index: history::Pieces,
    /// The mosaic of cores and belts, at each cell where the
    /// supercontinent's crust stood.
    pub mosaic: std::sync::Arc<Mosaic>,
}

/// A cell of crust's height above a sea standing at `sea`, m, with no
/// water on it: what it carries, its pile's lift, its flood basalts
/// ([`history::stands`]) and its home's grain.
pub(crate) fn tile_height(h: &History, tile: &history::Tile, sea: i64) -> i32 {
    let home = tile.home as usize;
    (history::stands(tile.high, tile.thick, tile.margins)
        + i64::from(h.rough[home])
        + i64::from(h.broad[home]) / 2
        - sea)
        .clamp(-11_000, 9_000) as i32
}

/// The hollows (version 11, the lead's 27 November: « pourquoi une mer à
/// l'intérieur d'un continent grandirait ? »): the ground below the
/// sea's level that the ocean does not reach. The ocean is the largest
/// sheet of water over the ocean's floor; it enters a continent from
/// its coast, from one cell of crust below the sea's level to the next,
/// read in the pieces' own lattices (a piece that only turns keeps its
/// seas and its hollows) and from a piece to the one lying against it.
/// What it does not reach is not sea, whatever its depth: each cell of
/// the grid, and each cell of each piece.
pub(crate) fn hollows(
    sphere: &Sphere,
    h: &History,
    index: &history::Pieces,
    pieces: &[history::Piece],
    (continental, elevation): (&[bool], &[i32]),
    sea: i64,
) -> (Vec<bool>, Vec<Vec<bool>>) {
    let floor: Vec<bool> = (0..CELLS)
        .map(|c| elevation[c] <= 0 && !continental[c])
        .collect();
    // The ocean: the largest sheet of water over the floor.
    let mut sheet = vec![u32::MAX; CELLS];
    let mut best = (0_i64, u32::MAX);
    for start in 0..CELLS {
        if !floor[start] || sheet[start] != u32::MAX {
            continue;
        }
        let (mut stack, mut km2) = (vec![start], 0_i64);
        sheet[start] = start as u32;
        while let Some(c) = stack.pop() {
            km2 += i64::from(sphere.area_km2[c]);
            for &(n, _) in &sphere.links[c] {
                let n = n as usize;
                if floor[n] && sheet[n] == u32::MAX {
                    sheet[n] = start as u32;
                    stack.push(n);
                }
            }
        }
        if km2 > best.0 {
            best = (km2, start as u32);
        }
    }
    let low: Vec<Vec<bool>> = pieces
        .iter()
        .map(|p| {
            p.tiles
                .iter()
                .map(|x| !x.under && tile_height(h, x, sea) <= 0)
                .collect()
        })
        .collect();
    // What each cell of crust touches beyond its own piece: the floor's
    // cells, and the cells of the pieces lying against it.
    let mut reached: Vec<Vec<bool>> = low.iter().map(|l| vec![false; l.len()]).collect();
    let mut wet_floor: Vec<bool> = sheet.iter().map(|&s| s == best.1).collect();
    let mut beside_floor: Vec<Vec<(usize, usize)>> = vec![Vec::new(); CELLS];
    let mut beside: Vec<Vec<Vec<(usize, usize)>>> =
        low.iter().map(|l| vec![Vec::new(); l.len()]).collect();
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for (k, piece) in pieces.iter().enumerate() {
        for (i, x) in piece.tiles.iter().enumerate() {
            if !low[k][i] {
                continue;
            }
            for &(m, _) in &sphere.links[x.at as usize] {
                if index.tile(k, m).is_some() {
                    continue;
                }
                let y = normalize(history::placed(&piece.turn, sphere.centre[m as usize]));
                if let Some((k2, i2)) = index.at(sphere, pieces, y) {
                    if low[k2][i2] {
                        beside[k][i].push((k2, i2));
                        beside[k2][i2].push((k, i));
                    }
                } else {
                    let c = sphere.cell_of(y);
                    if wet_floor[c] {
                        if !reached[k][i] {
                            reached[k][i] = true;
                            stack.push((k, i));
                        }
                    } else if floor[c] {
                        beside_floor[c].push((k, i));
                    }
                }
            }
        }
    }
    while let Some((k, i)) = stack.pop() {
        let mut next: Vec<(usize, usize)> = beside[k][i].clone();
        let at = pieces[k].tiles[i].at;
        for &(m, _) in &sphere.links[at as usize] {
            if let Some(j) = index.tile(k, m) {
                next.push((k, j));
            } else {
                // A sheet of water over floor shut in the continent is
                // reached through the crust.
                let y = normalize(history::placed(&pieces[k].turn, sphere.centre[m as usize]));
                let c = sphere.cell_of(y);
                if floor[c] && !wet_floor[c] && index.at(sphere, pieces, y).is_none() {
                    let mut cells = vec![c];
                    wet_floor[c] = true;
                    while let Some(c) = cells.pop() {
                        next.append(&mut beside_floor[c]);
                        for &(n, _) in &sphere.links[c] {
                            let n = n as usize;
                            if floor[n] && !wet_floor[n] {
                                wet_floor[n] = true;
                                cells.push(n);
                            }
                        }
                    }
                }
            }
        }
        for (k2, i2) in next {
            if low[k2][i2] && !reached[k2][i2] {
                reached[k2][i2] = true;
                stack.push((k2, i2));
            }
        }
    }
    let basin_tile: Vec<Vec<bool>> = low
        .iter()
        .zip(&reached)
        .map(|(l, r)| l.iter().zip(r).map(|(&l, &r)| l && !r).collect())
        .collect();
    let basin: Vec<bool> = (0..CELLS)
        .map(|c| match index.at(sphere, pieces, sphere.centre[c]) {
            Some((k, i)) => basin_tile[k][i],
            None => floor[c] && !wet_floor[c],
        })
        .collect();
    (basin, basin_tile)
}

/// A hotspot's island or seamount sinks as it ages: gone after this,
/// millions of years.
pub(crate) const TRACK_MYR: i64 = 120;

impl Planet {
    /// The planet born from `seed`, with the default settings.
    pub fn new(seed: u64) -> Self {
        Self::with(seed, &Settings::default())
    }

    /// The planet born from `seed` and `settings`.
    pub fn with(seed: u64, settings: &Settings) -> Self {
        let sphere = Sphere::new();
        // 1. The mosaic: the cores and the belts inside the supercontinent,
        // read where its crust stood before it broke, and carried with it.
        let (old, mosaic) = Mosaic::draw(seed, &sphere);
        let mosaic = std::sync::Arc::new(mosaic);
        // 2. The history: the supercontinent, its break-up, the drift.
        let history = std::sync::Arc::new(history::run(seed, settings, &sphere, &mosaic));
        let last = history.last();
        Self::build(seed, settings, sphere, old, mosaic, history, last)
    }

    /// The planet as it stood at step `t` of its history, drawn as today's
    /// is: its plates, their boundaries, its relief and its sea.
    #[must_use]
    pub fn at_step(&self, t: usize) -> Self {
        Self::build(
            self.seed,
            &self.settings,
            self.sphere.clone(),
            self.old.clone(),
            std::sync::Arc::clone(&self.mosaic),
            std::sync::Arc::clone(&self.history),
            t.min(self.history.last()),
        )
    }

    fn build(
        seed: u64,
        settings: &Settings,
        sphere: Sphere,
        old: Layout,
        mosaic: std::sync::Arc<Mosaic>,
        history: std::sync::Arc<History>,
        t: usize,
    ) -> Self {
        let h = &*history;
        let now_my = ((h.last() - t) as i64 * settings.step_my) as u16;
        let continental: Vec<bool> = h.class[t].iter().map(|&k| k != history::OCEAN).collect();
        let home = &h.home_at[t];
        let crust_cut = h.crust_cut;
        let crust: Vec<i64> = (0..CELLS)
            .map(|c| {
                if continental[c] {
                    h.field[home[c] as usize].max(crust_cut)
                } else {
                    crust_cut - 400
                }
            })
            .collect();
        let mut old_plate = vec![0_u8; CELLS];
        let mut old_edge = vec![None; CELLS];
        for c in 0..CELLS {
            if continental[c] {
                let home = home[c] as usize;
                old_plate[c] = mosaic.core[home];
                old_edge[c] = mosaic.edge[home];
            } else {
                old_plate[c] = old.plate_at(sphere.centre[c]) as u8;
            }
        }
        // 3. The plates at this step (today's: the history's last).
        let (plates, plate) = Layout::of_history(seed, settings, &sphere, h, t);
        // 4. Boundaries.
        let mut edge = vec![None; CELLS];
        for c in 0..CELLS {
            if let Some((other, normal, theirs_cont)) =
                across(&sphere, &plates, &plate, &continental, c)
            {
                let mine = plate[c] as usize;
                let p = sphere.centre[c];
                let (closing, slide) = plates.motion(mine, other, p, normal);
                let up = plates.plates[mine].buoyancy > plates.plates[other].buoyancy;
                let kind = classify(closing, slide, continental[c], theirs_cont, up);
                let rate = if matches!(kind, Kind::Transform) {
                    slide
                } else {
                    closing.abs()
                };
                edge[c] = Some(Edge {
                    kind,
                    other: other as u8,
                    rate: rate as i32,
                    normal,
                });
            }
        }
        // Distances.
        let sources: Vec<(usize, u32)> = (0..CELLS)
            .filter(|&c| edge[c].is_some())
            .map(|c| (c, c as u32))
            .collect();
        let (near_d, near_l) = sphere.spread(&sources, |a, b| plate[a] == plate[b], 1_500);
        let near: Vec<(u32, u32)> = near_d.into_iter().zip(near_l).collect();
        // 5. Hotspots: their chains are the marks the history carried, the
        // youngest first.
        let hotspots: Vec<Hotspot> = h
            .hotspots
            .iter()
            .enumerate()
            .map(|(k, &at)| {
                let mut track: Vec<(V3, i64)> = h.marks_at[t]
                    .iter()
                    .filter(|m| usize::from(m.1) == k)
                    .map(|m| (m.0, i64::from(m.2.saturating_sub(now_my))))
                    .collect();
                track.sort_by_key(|m| m.1);
                if track.first().is_none_or(|m| m.1 > 0) {
                    track.insert(0, (at, 0));
                }
                Hotspot { at, track }
            })
            .collect();
        // Each cell's nearest hotspot mark within reach, and its age.
        let mut lava: Vec<Option<(i64, i64)>> = vec![None; CELLS];
        for hs in &hotspots {
            for &(x, age) in &hs.track {
                let c = sphere.cell_of(x);
                let mut ring = vec![c];
                for &(n, _) in &sphere.links[c] {
                    ring.push(n as usize);
                    ring.extend(sphere.links[n as usize].iter().map(|&(m, _)| m as usize));
                }
                for k in ring {
                    let d = km_between(sphere.centre[k], x);
                    if d < 250 && lava[k].is_none_or(|(bd, ba)| (d, age) < (bd, ba)) {
                        lava[k] = Some((d, age));
                    }
                }
            }
        }
        // 6. Height. A continent's is carried by its crust (version 7,
        // 27 November 2026): the history changed it only where something
        // happened to the crust, and the grain is its home's. The ocean
        // floor's follows its age, with its trenches, its island arcs and
        // its hotspots' chains.
        let mut elevation: Vec<i64> = (0..CELLS)
            .map(|c| {
                if continental[c] {
                    let from = home[c] as usize;
                    // A pile of crust floats higher (isostasy), flood
                    // basalts stand as plateaus.
                    return history::stands(h.high_at[t][c], h.thick_at[t][c], h.margins_at[t][c])
                        + i64::from(h.rough[from])
                        + i64::from(h.broad[from]) / 2;
                }
                // The floor sinks as it cools and rises with the
                // sediment laid on it.
                let mut e = i64::from(h.rough[c]);
                let (d, src) = near[c];
                if d != u32::MAX {
                    let d = i64::from(d);
                    // (An island arc is no longer drawn on the map: it is
                    // crust the history adds, version 18.)
                    if edge[src as usize].expect("a source is an edge").kind == Kind::Trench {
                        e -= 2_600 * bump(d, 90) / 1000;
                    }
                }
                if let Some((d, age)) = lava[c] {
                    e += 3_800 * bump(d, 130) / 1000 * (TRACK_MYR - age).max(0) / TRACK_MYR;
                }
                history::floor_bed(
                    i64::from(h.age[t][c]),
                    i64::from(h.silt_at[t][c]),
                    h.thick_at[t][c],
                    e,
                )
            })
            .collect();
        // The sea is the history's: the level at which the planet's
        // water fills the basins of the relief at this step (version 16;
        // the trenches' deeps and the hotspots' chains drawn above are
        // the map's only, and hold none of the count).
        let sea = i64::from(h.sea_at[t]);
        let sea_rise_m = sea - i64::from(h.sea_at[h.last()]);
        for e in &mut elevation {
            *e -= sea;
        }
        let mut elevation: Vec<i32> = elevation
            .iter()
            .map(|&e| e.clamp(-11_000, 9_000) as i32)
            .collect();
        let index = history::Pieces::new(&sphere, &history.pieces_at[t]);
        let (basin, basin_tile) = hollows(
            &sphere,
            h,
            &index,
            &history.pieces_at[t],
            (&continental, &elevation),
            sea,
        );
        // Under the sea the water's weight pushes the ground down: the
        // depth drawn is the water's. A closed basin holds no sea.
        for c in 0..CELLS {
            if elevation[c] < 0 && !basin[c] {
                elevation[c] = (-history::depth(-i64::from(elevation[c]))).max(-11_000) as i32;
            }
        }
        let mut planet = Self {
            seed,
            sphere,
            plates,
            old,
            plate,
            old_plate,
            crust,
            crust_cut,
            continental,
            edge,
            old_edge,
            near,
            hotspots,
            elevation,
            sea,
            sea_rise_m,
            basin,
            basin_tile,
            lands: Vec::new(),
            ours: vec![false; CELLS],
            settings: *settings,
            index,
            history,
            step: t,
            mosaic,
        };
        planet.lands = planet.landmasses();
        planet
    }

    /// The continents as they are kept at this step: rigid pieces, each
    /// in its own lattice (version 10).
    #[must_use]
    pub fn pieces(&self) -> &[history::Piece] {
        &self.history.pieces_at[self.step]
    }

    /// The continental crust at a point, read in the pieces themselves:
    /// its piece and its cell in the piece.
    #[must_use]
    pub fn tile_at(&self, p: V3) -> Option<(usize, usize)> {
        self.index.at(&self.sphere, self.pieces(), p)
    }

    /// A cell of crust's height above this step's sea, m: what it
    /// carries, its home's grain, its pile's lift, its flood basalts.
    #[must_use]
    pub fn tile_height(&self, tile: &history::Tile) -> i32 {
        tile_height(&self.history, tile, self.sea)
    }

    /// The water's depth, m, over ground the sea covers `under` m below
    /// its level as it would stand dry.
    fn sunk(under: i32) -> i32 {
        (-history::depth(-i64::from(under))).max(-11_000) as i32
    }

    /// Whether a cell of a piece of continent is dry: above the sea's
    /// level, or in a hollow the ocean does not reach.
    #[must_use]
    pub fn tile_dry(&self, piece: usize, tile: usize) -> bool {
        let x = &self.pieces()[piece].tiles[tile];
        !x.under && (self.tile_height(x) > 0 || self.basin_tile[piece][tile])
    }

    /// Whether a point lies in a hollow: below the sea's level, with no
    /// path to the ocean. Land, not sea.
    #[must_use]
    pub fn basin_at(&self, p: V3) -> bool {
        match self.tile_at(p) {
            Some((k, i)) => self.basin_tile[k][i],
            None => self.basin[self.sphere.cell_of(p)],
        }
    }

    /// Whether a cell of the grid is land: above the sea, or a hollow.
    #[must_use]
    pub fn dry(&self, c: usize) -> bool {
        self.elevation[c] > 0 || self.basin[c]
    }

    /// What a point of the planet shows: its height above the sea, m, and
    /// whether continental crust lies there. The continents are read in
    /// their pieces, finer than the grid (whose cells show only what lies
    /// at their centre); the ocean floor on the grid.
    #[must_use]
    pub fn surface(&self, p: V3) -> (i32, bool) {
        if let Some((k, i)) = self.tile_at(p) {
            let e = self.tile_height(&self.pieces()[k].tiles[i]);
            let wet = e < 0 && !self.basin_tile[k][i];
            return (if wet { Self::sunk(e) } else { e }, true);
        }
        let c = self.sphere.cell_of(p);
        if !self.continental[c] {
            return (self.elevation[c], false);
        }
        // Floor between a piece's edge and the grid cell that draws it.
        let beside = self.sphere.links[c]
            .iter()
            .filter(|&&(m, _)| !self.continental[m as usize])
            .map(|&(m, km)| (km, self.elevation[m as usize]))
            .min();
        (beside.map_or(-200, |b| b.1), false)
    }

    /// The pieces of land, largest first (ties by their first cell).
    pub fn landmasses(&self) -> Vec<Landmass> {
        let mut seen = vec![false; CELLS];
        let mut lands = Vec::new();
        for start in 0..CELLS {
            if seen[start] || !self.dry(start) {
                continue;
            }
            seen[start] = true;
            let mut cells = vec![start as u32];
            let mut k = 0;
            while k < cells.len() {
                let c = cells[k] as usize;
                k += 1;
                for &(n, _) in &self.sphere.links[c] {
                    let n = n as usize;
                    if !seen[n] && self.dry(n) {
                        seen[n] = true;
                        cells.push(n as u32);
                    }
                }
            }
            let area_km2 = cells
                .iter()
                .map(|&c| i64::from(self.sphere.area_km2[c as usize]))
                .sum();
            let mut sum = [0_i64; 3];
            for &c in &cells {
                sum = add(sum, self.sphere.centre[c as usize]);
            }
            let centre = normalize(sum);
            let span_km = 2 * cells
                .iter()
                .map(|&c| km_between(centre, self.sphere.centre[c as usize]))
                .max()
                .unwrap_or(0);
            lands.push(Landmass {
                cells,
                area_km2,
                centre,
                span_km,
            });
        }
        lands.sort_by_key(|l| (Reverse(l.area_km2), l.cells[0]));
        lands
    }

    /// Boundary length by kind, km (each boundary counted on both sides).
    pub fn boundary_km(&self) -> Vec<(Kind, i64)> {
        let kinds = [
            Kind::Ridge,
            Kind::Rift,
            Kind::Transform,
            Kind::Trench,
            Kind::Arc,
            Kind::IslandArc,
            Kind::Collision,
        ];
        kinds
            .iter()
            .map(|&k| {
                let cells = self
                    .edge
                    .iter()
                    .filter(|e| e.is_some_and(|e| e.kind == k))
                    .count();
                (k, cells as i64 * 100)
            })
            .collect()
    }

    /// When no piece of land qualifies for our continent (see
    /// [`Site::candidates`]), one is cut from a larger land behind a
    /// trench at [`SITE_LATITUDE`]: the land nearest the trench, grown
    /// outward until it holds [`PIECE_KM2`], is parted from the rest by a
    /// strait flooded along its narrowest neck (the cut that floods the
    /// least land). Returns the land flooded, km² (0: nothing cut).
    pub fn carve(&mut self) -> i64 {
        let s = &self.sphere;
        let reach = cos(TRENCH_REACH_KM * ONE / RADIUS_KM);
        // (flooded km², distance from the goal area, the strait's cells)
        let mut best: Option<(i64, i64, Vec<usize>)> = None;
        for c in 0..CELLS {
            let Some(e) = self.edge[c] else { continue };
            let p = s.centre[c];
            let lat = latitude(p);
            if e.kind != Kind::Trench || lat < SITE_LATITUDE.0 || lat > SITE_LATITUDE.1 {
                continue;
            }
            let east = east_at(p);
            if dot(e.normal, east) * 1000 / ONE < SITE_FACING {
                continue;
            }
            // The land nearest the trench, east of it.
            let Some((_, start)) = (0..CELLS)
                .filter(|&l| {
                    self.elevation[l] > 0
                        && dot(s.centre[l], p) >= reach
                        && dot(sub(s.centre[l], p), east) > 0
                })
                .map(|l| (km_between(p, s.centre[l]), l))
                .min()
            else {
                continue;
            };
            let (dist, _) = s.spread(&[(start, 0)], |_, b| self.elevation[b] > 0, 900);
            let mut order: Vec<(u32, usize)> = (0..CELLS)
                .filter(|&l| dist[l] != u32::MAX)
                .map(|l| (dist[l], l))
                .collect();
            order.sort_unstable();
            let mut inside = vec![false; CELLS];
            let mut area = 0;
            let mut sum = [0_i64; 3];
            for (k, &(_, l)) in order.iter().enumerate() {
                inside[l] = true;
                area += i64::from(s.area_km2[l]);
                sum = add(sum, s.centre[l]);
                if area < *PIECE_KM2.start() {
                    continue;
                }
                if area > *PIECE_KM2.end() {
                    break;
                }
                let mid = latitude(normalize(sum));
                if mid < SITE_LATITUDE.0 || mid > SITE_LATITUDE.1 {
                    continue;
                }
                // The strait: land outside the piece touching it.
                let mut strait: Vec<usize> = order[..=k]
                    .iter()
                    .flat_map(|&(_, a)| s.links[a].iter().map(|&(n, _)| n as usize))
                    .filter(|&n| !inside[n] && self.elevation[n] > 0)
                    .collect();
                strait.sort_unstable();
                strait.dedup();
                let flooded: i64 = strait.iter().map(|&n| i64::from(s.area_km2[n])).sum();
                let key = (flooded, (area - PIECE_GOAL_KM2).abs());
                if best.as_ref().is_none_or(|b| key < (b.0, b.1)) {
                    best = Some((key.0, key.1, strait));
                }
            }
        }
        let Some((flooded, _, strait)) = best else {
            return 0;
        };
        for c in strait {
            self.elevation[c] = STRAIT_M;
        }
        self.lands = self.landmasses();
        flooded
    }

    /// Our continent marked on the grid: the cells of `site`'s piece
    /// (nothing is reshaped, nothing sunk).
    pub fn mark(&mut self, site: &Site) {
        self.ours = vec![false; CELLS];
        for &c in &self.lands[site.land].cells {
            self.ours[c as usize] = true;
        }
    }
}
