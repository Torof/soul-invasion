//! The world as its history starts: the supercontinent, the first plates,
//! the ocean floor, the relief and the records of the crust, the mantle's
//! heat and its plumes.

use super::*;

/// The supercontinent's default warp and fray: an irregular cap, not a
/// disc (scaled by [`Settings::supercontinent_warp`] and `_fray`).
pub(super) const SUPER_WARP: [(i64, i64); 2] = [(700_000, 260_000), (250_000, 80_000)];
pub(super) const SUPER_FRAY: [(i64, i64); 3] = [(600_000, 4), (250_000, 2), (100_000, 1)];
/// How much the default fray moves the coast, the cap's cosine units (‰).
pub(super) const FRAY: i64 = 600;
/// The plates' borders' warp: wandering edges, not a Voronoi's lines.
pub(super) const RIFT_WARP: [(i64, i64); 3] =
    [(400_000, 140_000), (150_000, 45_000), (60_000, 15_000)];

/// Oceanic plates' seeds stay this far apart and from the
/// supercontinent's plates' seeds, km.
pub(super) const OCEAN_SEED_APART_KM: i64 = 3_000;
/// Draws tried for a point that must meet a condition.
pub(super) const TRIES: u64 = 100_000;
/// Terranes and oceanic seeds stay this far off the supercontinent's
/// crust, in its cosine units (‰).
pub(super) const TERRANE_OFF: i64 = 150;
/// Terranes' ages, My at the start (the supercontinent's crust has the
/// ages of its mosaic).
pub(super) const TERRANE_MY: (i64, i64) = (20, 250);
/// The ocean at the start: its youngest floor, and how fast it spread
/// from the plates' borders, km per million years on each side.
pub(super) const OCEAN_START_MY: i64 = 5;
pub(super) const OCEAN_START_KM_MY: i64 = 25;

/// The starting supercontinent's relief comes from its crust (version
/// 16; until then a ramp from its edge to its heart, and belts at set
/// heights): its whole, cold crust stands on the platform
/// ([`PLATFORM_M`]) everywhere, and a belt stands by its thickness. An
/// old failed rift's crust was thinned to this on its axis, tenths of a
/// km, and lies as low as that thinning gives once cooled (a seventh
/// thinner: 750 m). An old mountain belt had a root this much thicker
/// than a continent when its building ended (as thick again), of which
/// the model's own wear ([`Settings::mountain_wear`]) has left what its
/// age says: 3 km under a belt 500 My old, nothing under one of 1,000.
pub(super) const OLD_RIFT_THICK: u16 = 300;
pub(super) const OLD_ROOT: i64 = 350;

/// The world as its history starts: the supercontinent, the plates
/// under it and around it, the ocean floor, the relief and the records
/// the crust carries, the mantle's heat and its plumes, and the first
/// forces.
pub(crate) fn begin<'a>(
    seed: u64,
    s: &'a Settings,
    sphere: &'a Sphere,
    mosaic: &Mosaic,
    scene: Option<&Scene>,
) -> World<'a> {
    assert_eq!(s.radius_km, RADIUS_KM, "only the Earth's radius for now");
    let total_my = s.steps as i64 * s.step_my;
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
    let plate: Vec<u8> = (0..CELLS)
        .map(|c| {
            let q = warp(border_seed, sphere.centre[c], &RIFT_WARP);
            // The ocean around the supercontinent lies on the ocean's own
            // plates (version 14): the supercontinent's plates carry no
            // skirt of floor, their rim is a plate boundary.
            let among = if inside[c] {
                0..kc
            } else if seeds.len() > kc {
                kc..seeds.len()
            } else {
                0..seeds.len()
            };
            let mut best = (i64::MIN, 0);
            for (k, &(at, w)) in seeds.iter().enumerate().take(among.end).skip(among.start) {
                let score = dot(q, at) + w;
                if score > best.0 {
                    best = (score, k);
                }
            }
            best.1 as u8
        })
        .collect();
    // A written scene (the bench) lays its own continents and plates.
    let (inside, plate, kc, plates) = match scene {
        Some(scene) => (
            scene.continent.clone(),
            scene.plate.clone(),
            scene.continent_plates,
            scene.plates,
        ),
        None => (inside, plate, kc, seeds.len()),
    };
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
    // The crust's thickness at the start, and how far below the
    // platform its thinning lays it (isostasy, as a rift's stretched
    // crust once cooled).
    let whole = i64::from(CONTINENT_THICK);
    let thick0: Vec<u16> = (0..CELLS)
        .map(|c| {
            let belt = i64::from(mosaic.belt[c]);
            (match mosaic.kind[c] {
                Some(OldKind::Suture) => {
                    let mut root = OLD_ROOT * 1_000_000;
                    for _ in 0..i64::from(mosaic.worked_my[c]) / s.step_my.max(1) {
                        root = root * (1000 - s.mountain_wear) / 1000;
                    }
                    whole + root / 1_000_000 * belt / 1000
                }
                Some(OldKind::FailedRift) => {
                    whole - (whole - i64::from(OLD_RIFT_THICK)) * belt / 1000
                }
                _ => whole,
            }) as u16
        })
        .collect();
    let thinned = |c: usize| (whole - i64::from(thick0[c])).max(0) * ISOSTASY_M / 10;

    // Records at the start.
    let mut rock: Vec<Rock> = (0..CELLS)
        .map(|c| {
            if inside[c] {
                let mut margins = 0;
                let mut orogeny = NEVER;
                if mosaic.kind[c] == Some(OldKind::Suture) {
                    // An old mountain belt, over its whole width.
                    orogeny = (total_my + i64::from(mosaic.worked_my[c])) as u16;
                }
                match mosaic.edge[c] {
                    Some(OldKind::Suture) => margins |= OLD_SUTURE,
                    Some(OldKind::FailedRift) => margins |= FAILED_RIFT,
                    _ => {}
                }
                Rock {
                    class: CONTINENT,
                    piece: plate[c],
                    margins,
                    arc: 0,
                    rift: 0,
                    born_my: (total_my + i64::from(mosaic.age_my[c])) as u16,
                    orogeny,
                    home: c as u32,
                    thick: thick0[c],
                    high: lift(0, PLATFORM_M - thinned(c)),
                    cool: 0,
                    sunk: lift(0, thinned(c)),
                    sag: 0,
                    silt: 0,
                    laid: NEVER,
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
                    sag: 0,
                    silt: 0,
                    laid: NEVER,
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
    // The trenches at the start (version 14): a supercontinent was
    // gathered by the oceans that closed between its parts, and the ocean
    // left around it has gone on sinking under its rim ever since. Where
    // the floor of another plate meets the supercontinent a slab hangs.
    let hung: Vec<bool> = (0..CELLS)
        .map(|c| {
            rock[c].class == OCEAN
                && sphere.links[c].iter().any(|&(n, _)| {
                    rock[n as usize].class == CONTINENT && plate[n as usize] != plate[c]
                })
        })
        .collect();
    let ring: Vec<Spot> = (0..CELLS)
        .filter(|&c| hung[c])
        .map(|c| (c, plate[c], [0; 3], 0))
        .collect();
    let mut heat = vec![0_i32; CELLS];
    warm(sphere, &rock, &ring, &deep, &mut heat, 1000);
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
    let born_t = [0_usize; MAXP];
    let next_id = plates;
    let rifts: Vec<Rift> = Vec::new();
    let welds: Vec<Weld> = Vec::new();
    let accretions: Vec<Accretion> = Vec::new();
    let collisions: Vec<Collision> = Vec::new();
    let captures: Vec<Capture> = Vec::new();
    let cut_off = 0;
    let built_km2 = vec![0];
    let mut lips: Vec<Lip> = Vec::new();
    // Collisions in progress: the step of first contact (+1), the mean
    // closing seen last, µm a year, and the front's cells.
    let contact = vec![[0_usize; MAXP]; MAXP];
    // Steps each collision has stood still.
    // The rifts in progress and the abandoned ones, and the forces at
    // the plates' edges read at the last step.
    let troughs: Vec<Trough> = Vec::new();
    let edges_last: (Vec<Spot>, Vec<Spot>, Vec<Spot>) = (Vec::new(), Vec::new(), Vec::new());
    let record_of = vec![[usize::MAX; MAXP]; MAXP];
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
    let marks: Vec<Mark> = Vec::new();
    // Step 0.
    let out_plate = Vec::with_capacity(s.steps + 1);
    let out_class = Vec::with_capacity(s.steps + 1);
    let out_age = Vec::with_capacity(s.steps + 1);
    let out_kind = Vec::with_capacity(s.steps + 1);
    let out_margin = Vec::with_capacity(s.steps + 1);
    let out_motion = Vec::with_capacity(s.steps + 1);
    let out_home = Vec::with_capacity(s.steps + 1);
    let out_margins = Vec::with_capacity(s.steps + 1);
    let out_orogeny = Vec::with_capacity(s.steps + 1);
    let out_thick = Vec::with_capacity(s.steps + 1);
    let out_high = Vec::with_capacity(s.steps + 1);
    let out_cause = Vec::with_capacity(s.steps + 1);
    let out_shown = Vec::with_capacity(s.steps + 1);
    let out_id = Vec::with_capacity(s.steps + 1);
    let out_hidden = Vec::with_capacity(s.steps + 1);
    let out_marks = Vec::with_capacity(s.steps + 1);
    let out_heat = Vec::with_capacity(s.steps + 1);
    let out_pieces = Vec::with_capacity(s.steps + 1);
    let out_turned = vec![vec![[0_i64; 3]; MAXP]];
    let out_rifts = Vec::with_capacity(s.steps + 1);
    let continental_km2 = Vec::with_capacity(s.steps + 1);
    let shortened_km2 = vec![0];
    let made_km2 = vec![0];
    let first = drive(
        sphere,
        s,
        &plate,
        &rock,
        &heat,
        Edges {
            slab: &[],
            suction: &[],
            ridge: &[],
            piles: &[],
            bridges: &[],
        },
        None,
        &held(&parted, &[[false; MAXP]; MAXP]),
    );
    // The slabs that hang at the start pull from the start.
    let seen = look(
        sphere,
        &plate,
        &first.group,
        &mut rock.clone(),
        &first.omega,
        &vec![Margin::Inland as u8; CELLS],
        &hung,
        (total_my as u16, s.slab_age_my),
    );
    let first = drive(
        sphere,
        s,
        &plate,
        &rock,
        &heat,
        Edges {
            slab: &seen.slab,
            suction: &seen.suction,
            ridge: &seen.ridge,
            piles: &[],
            bridges: &[],
        },
        None,
        &held(&parted, &[[false; MAXP]; MAXP]),
    );
    let omega = first.omega;
    let group = first.group;
    let opened = opening(sphere, &plate, &rock, &parted, &group, &mut broken);
    let mut journal = Vec::new();
    for &pair in &opened {
        for (at, km2) in flood(sphere, &mut land, &top, pair, &plumes, &mut spent) {
            journal.push(Event::plates(
                What::Lava,
                (pair.0 as usize, pair.1 as usize),
                at,
                km2,
            ));
            lips.push(Lip {
                my: total_my as u16,
                at,
                km2,
            });
        }
    }
    let event = vec![Margin::Inland as u8; CELLS];
    let last_kind = vec![0_u8; CELLS];
    let next_bit = CELLS as u32;
    let shown_now = vec![0_u8; CELLS];
    let hidden_now = [0_u32; 3];
    World {
        seed,
        s,
        sphere,
        belt: (0..CELLS)
            .map(|c| if inside[c] { mosaic.belt[c] } else { 0 })
            .collect(),
        field,
        crust_cut,
        upwelling,
        plates_at_start: (kc, plates),
        imposed: scene.map_or_else(Vec::new, |scene| scene.turns.clone()),
        placed,
        rough,
        broad,
        grain,
        deep,
        heat,
        hung,
        hotspots: hotspots.to_vec(),
        plumes,
        spent,
        parted,
        broken,
        alias,
        born_t,
        next_id,
        free: Vec::new(),
        renamed: Vec::new(),
        unnamed: 0,
        rifts,
        welds,
        accretions,
        collisions,
        captures,
        cut_off,
        built_km2,
        lips,
        sea: 0,
        wet: vec![false; CELLS],
        dust: vec![0; CELLS],
        out_silt: Vec::with_capacity(s.steps + 1),
        out_laid: Vec::with_capacity(s.steps + 1),
        out_sea: Vec::with_capacity(s.steps + 1),
        out_budget: Vec::with_capacity(s.steps + 1),
        contact,
        troughs,
        pulled: [0; MAXP],
        edges_last,
        record_of,
        locked: Vec::new(),
        arc_steps,
        land,
        bits,
        disp,
        top,
        marks,
        plate,
        rock,
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
        omega,
        group,
        event,
        last_kind,
        next_bit,
        shown_now,
        hidden_now,
        journal,
        out_journal: Vec::with_capacity(s.steps + 1),
        was: [NONE; MAXP],
        step: Step::new(0, 0),
    }
}
