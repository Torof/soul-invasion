//! The model's tests: the grid, the determinism of a seed, and what a
//! history keeps (crust, rigid pieces, plates in one piece, named causes of
//! height).

use super::*;

/// The planet of a seed, made once for all the tests that read it (a
/// planet takes seconds, and thirteen tests read seven of them).
fn made(seed: u64) -> &'static Planet {
    static MADE: [std::sync::OnceLock<Planet>; 32] = [const { std::sync::OnceLock::new() }; 32];
    MADE[seed as usize].get_or_init(|| Planet::new(seed))
}

#[test]
fn the_grid_finds_its_own_cells_and_covers_the_sphere() {
    let s = Sphere::new();
    for c in 0..CELLS {
        assert_eq!(s.cell_of(s.centre[c]), c);
        assert!(
            s.links[c].len() >= 6,
            "cell {c} has {} neighbours",
            s.links[c].len()
        );
    }
    // A direction belongs to the cell whose centre is nearest.
    for k in 0..400 {
        let p = random_unit(7, 1, k);
        let best = (0..CELLS)
            .max_by_key(|&c| (dot(s.centre[c], p), std::cmp::Reverse(c)))
            .expect("cells");
        assert_eq!(dot(s.centre[s.cell_of(p)], p), dot(s.centre[best], p));
    }
    let total: i64 = s.area_km2.iter().map(|&a| i64::from(a)).sum();
    let earth = 510_064_472_i64;
    assert!((total - earth).abs() < earth / 100, "{total}");
}

#[test]
fn integer_trigonometry_holds() {
    assert_eq!(sin(0), 0);
    assert!((sin(HALF_PI) - ONE).abs() < 3);
    assert!((cos(PI) + ONE).abs() < 3);
    assert!(latitude([0, 0, ONE]) >= 899);
    assert!((latitude(from_lat_lon(450, 300)) - 450).abs() <= 1);
    assert!((longitude(from_lat_lon(-300, -1200)) + 1200).abs() <= 1);
}

#[test]
fn a_seed_gives_the_same_planet() {
    let a = made(3);
    let b = &Planet::new(3);
    assert_eq!(a.elevation, b.elevation);
    assert_eq!(a.plate, b.plate);
    assert_eq!(a.history.plate, b.history.plate);
    assert_eq!(a.history.age, b.history.age);
    assert_eq!(a.history.orogeny_my, b.history.orogeny_my);
    assert_eq!(a.history.sea_at, b.history.sea_at);
    assert_eq!(a.history.silt_at, b.history.silt_at);
}

#[test]
fn lands_come_from_the_history() {
    let seed = 14;
    {
        let world = made(seed);
        let large = world
            .lands
            .iter()
            .filter(|l| l.area_km2 > 1_000_000)
            .count();
        assert!(large >= 4, "seed {seed}: {large} lands over 1 M km²");
        assert!(
            world.history.collisions.iter().any(|c| c.front_cells >= 40),
            "seed {seed}: no collision"
        );
    }
}

#[test]
fn continental_crust_is_kept_through_the_history() {
    // Collisions shorten and thicken it, rifts stretch and thin it,
    // wear builds the margins out: its volume holds within a tenth
    // of the start at every step (its area does not: a rift's
    // thinned crust stands for a wider one, version 11).
    for seed in [1, 9] {
        let world = made(seed);
        let volume: Vec<i64> = world
            .history
            .pieces_at
            .iter()
            .map(|pieces| {
                pieces
                    .iter()
                    .flat_map(|p| &p.tiles)
                    .map(|x| i64::from(x.thick) * i64::from(x.area_km2))
                    .sum()
            })
            .collect();
        for (t, &k) in volume.iter().enumerate() {
            assert!(
                (k - volume[0]).abs() * 10 <= volume[0],
                "seed {seed} step {t}: {k} of {}",
                volume[0]
            );
        }
    }
}

#[test]
fn a_continent_carries_its_relief() {
    // Followed by its home from step to step, a crust keeps its height
    // unless something happens to it: little of it crosses the level
    // of the start's sea in a step (a height recomputed at each step
    // sent a tenth of it across).
    let world = made(9);
    let h = &world.history;
    let height = |t: usize, c: usize| {
        let home = h.home_at[t][c] as usize;
        i64::from(h.high_at[t][c])
            + i64::from(h.rough[home])
            + i64::from(h.broad[home]) / 2
            + (i64::from(h.thick_at[t][c]) - i64::from(history::CONTINENT_THICK)).max(0)
                * history::ISOSTASY_M
                / 10
    };
    let (mut followed, mut crossed) = (0_i64, 0_i64);
    for t in 1..=h.last() {
        let mut was = vec![None; CELLS];
        for c in 0..CELLS {
            if h.class[t - 1][c] != history::OCEAN {
                was[h.home_at[t - 1][c] as usize] = Some(height(t - 1, c) > 0);
            }
        }
        for c in 0..CELLS {
            if h.class[t][c] == history::OCEAN {
                continue;
            }
            if let Some(dry) = was[h.home_at[t][c] as usize] {
                followed += 1;
                crossed += i64::from(dry != (height(t, c) > 0));
            }
        }
    }
    assert!(crossed * 100 <= followed * 4, "{crossed} of {followed}");
}

#[test]
fn a_crust_changes_height_only_by_a_named_cause() {
    // Every cell of continental crust is followed by its name in its
    // piece (version 10: all of them, none is hidden by a grid); from
    // a step to the next its carried height moves by what its causes
    // say (a rift's stretch, cooling, an arc, a docking, a plume,
    // wear, levelling, infill), and it stays the same cell of the
    // same lattice.
    let planet = made(14);
    let h = &planet.history;
    let (mut followed, mut crust) = (0_u64, 0_u64);
    for t in 1..=h.last() {
        let mut was = std::collections::HashMap::new();
        for piece in &h.pieces_at[t - 1] {
            for tile in &piece.tiles {
                was.insert(tile.id, (tile.high, tile.at));
            }
        }
        for piece in &h.pieces_at[t] {
            for tile in &piece.tiles {
                crust += 1;
                if let Some(&(before, at)) = was.get(&tile.id) {
                    let by: i64 = tile.why.iter().map(|&m| i64::from(m)).sum();
                    assert_eq!(
                        i64::from(tile.high) - i64::from(before),
                        by,
                        "step {t}, crust {}",
                        tile.id
                    );
                    assert_eq!(tile.at, at, "step {t}, crust {}", tile.id);
                    followed += 1;
                }
            }
        }
    }
    assert!(followed * 100 > crust * 97, "{followed} of {crust}");
}

#[test]
fn a_piece_of_continent_is_rigid() {
    // Version 10: a piece of continent is never redrawn. Over twenty
    // steps, every cell a piece keeps is the same cell of the piece's
    // lattice, lies at the same place once both ends are turned back,
    // and stays as far from the piece's other cells. A piece nothing
    // happened to (the same cells at both ends) has the same outline
    // at both ends.
    let planet = made(14);
    let h = &planet.history;
    let sphere = &planet.sphere;
    let back = |p: &history::Piece, x: V3| -> V3 {
        let m = p.turn;
        normalize([0, 1, 2].map(|j| dot([m[0][j], m[1][j], m[2][j]], x)))
    };
    let outline = |p: &history::Piece| -> Vec<(u32, u32)> {
        let mut cells: Vec<(u32, u32)> = p.tiles.iter().map(|x| (x.id, x.at)).collect();
        cells.sort_unstable();
        cells
    };
    let (mut kept, mut untouched) = (0, 0);
    for (span, every) in [(20, 4), (5, 1)] {
        for t in (0..=h.last() - span).step_by(every) {
            for a in h.pieces_at[t].iter().filter(|a| a.tiles.len() >= 30) {
                let Some(b) = h.pieces_at[t + span].iter().find(|b| b.frame == a.frame) else {
                    continue;
                };
                if outline(a) == outline(b) {
                    untouched += 1;
                }
                if span != 20 {
                    continue;
                }
                let then: std::collections::HashMap<u32, usize> =
                    a.tiles.iter().enumerate().map(|(i, x)| (x.id, i)).collect();
                let mut last: Option<(usize, usize)> = None;
                for (j, x) in b.tiles.iter().enumerate() {
                    let Some(&i) = then.get(&x.id) else {
                        continue;
                    };
                    assert_eq!(a.tiles[i].at, x.at, "step {t}: crust {} moved", x.id);
                    let (pa, pb) = (a.place(sphere, i), b.place(sphere, j));
                    let off = km_between(back(a, pa), back(b, pb));
                    assert!(off <= 1, "step {t}: {off} km off its place in the piece");
                    if let Some((i0, j0)) = last {
                        let da = km_between(pa, a.place(sphere, i0));
                        let db = km_between(pb, b.place(sphere, j0));
                        assert!((da - db).abs() <= 1, "step {t}: {da} km became {db}");
                    }
                    last = Some((i, j));
                    kept += 1;
                }
            }
        }
    }
    assert!(
        kept >= 5_000 && untouched >= 3,
        "{kept} cells, {untouched} pieces"
    );
}

#[test]
fn the_history_breaks_the_supercontinent_and_keeps_its_records() {
    let world = made(11);
    let h = &world.history;
    // At the start, one supercontinent of the settings' crust share.
    let first = &h.class[0];
    let land = (0..CELLS)
        .filter(|&c| first[c] == history::CONTINENT)
        .count();
    let share = land as i64 * 1000 / CELLS as i64;
    assert!(
        (share - Settings::default().crust_per_mille).abs() < 60,
        "{share}"
    );
    // Today, every piece of crust carries a record.
    for c in 0..CELLS {
        if world.continental[c] {
            assert!((h.home[c] as usize) < CELLS);
        }
        // The supercontinent's crust is old (an arc may be born today).
        if h.piece[c] < history::TERRANE_BASE {
            assert!(h.age_my[c] >= 700, "{}", h.age_my[c]);
        }
    }
    // Leading edges today are young mountains.
    let last = h.last();
    assert!(
        (0..CELLS)
            .any(|c| h.margin[last][c] == history::Margin::Active as u8 && h.orogeny_my[c] == 0)
    );
    // The ocean floor is young near the ridges and old far from them:
    // new floor was made, old floor sank.
    let ocean: Vec<u16> = (0..CELLS)
        .filter(|&c| h.class[last][c] == history::OCEAN)
        .map(|c| h.age[last][c])
        .collect();
    let young = ocean.iter().filter(|&&a| a <= 20).count();
    assert!(young * 20 > ocean.len(), "{young} of {}", ocean.len());
}

#[test]
fn the_mantle_is_warm_under_the_supercontinent_and_the_sea_is_anchored_today() {
    let world = made(3);
    let h = &world.history;
    // At the start the supercontinent is a lid: the mantle under it is
    // warmer than under the ocean, and most plumes rise under it.
    let mean = |land: bool| {
        let cells: Vec<i64> = (0..CELLS)
            .filter(|&c| (h.class[0][c] != history::OCEAN) == land)
            .map(|c| i64::from(h.heat_at[0][c]))
            .collect();
        cells.iter().sum::<i64>() / cells.len() as i64
    };
    assert!(
        mean(true) > mean(false) + 300,
        "{} {}",
        mean(true),
        mean(false)
    );
    assert!(h.hotspots.len() >= 14);
    // Slabs have cooled it somewhere since.
    assert!(h.heat_at[h.last()].iter().any(|&v| v < -200));
    // The sea is today's at the last step, and moved in the past.
    assert_eq!(world.sea_rise_m, 0);
    assert!((0..h.last()).any(|t| world.at_step(t).sea_rise_m != 0));
}

#[test]
fn a_plate_is_one_piece_and_no_small_plate_sits_inside_another() {
    // Parts of a plate cut off from the rest and small plates enclosed
    // in one other plate, counted over the whole history (a rift of
    // the step may leave one until the next step). A part the grid's
    // view shows apart is not cut off if its continent holds to the
    // plate's in its piece's own lattice (version 11).
    let mut strays = 0;
    for seed in [14, 17] {
        let world = made(seed);
        let h = &world.history;
        for t in 1..=h.last() {
            let plate = &h.plate[t];
            let mut seen = vec![false; CELLS];
            let mut parts: Vec<(u8, i64, usize)> = Vec::new();
            let mut part_of = vec![0_usize; CELLS];
            for start in 0..CELLS {
                if seen[start] {
                    continue;
                }
                seen[start] = true;
                part_of[start] = parts.len();
                let (mut km2, mut around, mut stack) = (0, Vec::new(), vec![start]);
                while let Some(c) = stack.pop() {
                    km2 += i64::from(world.sphere.area_km2[c]);
                    for &(n, _) in &world.sphere.links[c] {
                        let n = n as usize;
                        if plate[n] != plate[start] {
                            if !around.contains(&plate[n]) {
                                around.push(plate[n]);
                            }
                        } else if !seen[n] {
                            seen[n] = true;
                            part_of[n] = parts.len();
                            stack.push(n);
                        }
                    }
                }
                parts.push((plate[start], km2, around.len()));
            }
            let mut held = vec![false; parts.len()];
            for piece in &h.pieces_at[t] {
                let place: std::collections::HashMap<u32, usize> = piece
                    .tiles
                    .iter()
                    .enumerate()
                    .map(|(i, x)| (x.at, i))
                    .collect();
                let cell: Vec<usize> = (0..piece.tiles.len())
                    .map(|i| world.sphere.cell_of(piece.place(&world.sphere, i)))
                    .collect();
                for (i, x) in piece.tiles.iter().enumerate() {
                    let a = part_of[cell[i]];
                    if parts[a].0 != x.plate {
                        continue;
                    }
                    for &(m, _) in &world.sphere.links[x.at as usize] {
                        if let Some(&j) = place.get(&m) {
                            let b = part_of[cell[j]];
                            if a != b && piece.tiles[j].plate == x.plate && parts[b].0 == x.plate {
                                held[a] = true;
                                held[b] = true;
                            }
                        }
                    }
                }
            }
            for (k, &(id, km2, around)) in parts.iter().enumerate() {
                let largest = parts.iter().filter(|p| p.0 == id).map(|p| p.1).max();
                if km2 < 2_000_000 && !held[k] && (around <= 1 || Some(km2) < largest) {
                    strays += 1;
                }
            }
        }
    }
    assert!(strays <= 12, "{strays} stray parts of plates");
}

#[test]
fn every_cell_is_on_exactly_one_plate_at_every_step() {
    for seed in [11, 17] {
        let world = made(seed);
        let h = &world.history;
        for t in 0..=h.last() {
            // One plate per cell, a plate id (never "none"), whose
            // motion at that step is on record.
            let mut cells = vec![0_usize; history::MAXP];
            for &p in &h.plate[t] {
                assert!(
                    (p as usize) < h.motion[t].len(),
                    "seed {seed} step {t}: plate {p}"
                );
                cells[p as usize] += 1;
            }
            assert_eq!(cells.iter().sum::<usize>(), CELLS);
        }
    }
}

#[test]
fn the_journal_names_the_rule_behind_every_plate_that_appears_or_disappears() {
    use history::What;
    for seed in [9, 14] {
        let world = made(seed);
        let h = &world.history;
        let drawn = |t: usize| {
            let mut on = [false; history::MAXP];
            for &p in &h.plate[t] {
                on[p as usize] = true;
            }
            on
        };
        assert_eq!(h.journal.len(), h.last() + 1);
        for t in 1..=h.last() {
            let (before, after) = (drawn(t - 1), drawn(t));
            for p in 0..history::MAXP {
                let told = |whats: &[What]| {
                    h.journal[t]
                        .iter()
                        .any(|e| e.plate as usize == p && whats.contains(&e.what))
                };
                // A plate also shows under a new number when the plate
                // whose number its group bore has left the group.
                let renamed = h.journal[t]
                    .iter()
                    .any(|e| e.what == What::MovesWith && e.other as usize == p);
                if after[p] && !before[p] {
                    assert!(
                        renamed
                            || told(&[
                                What::BornOfRift,
                                What::BornOfTrench,
                                What::BornCutOff,
                                What::MovesAlone
                            ]),
                        "seed {seed}, step {t}: plate {p} appears, the journal does not say why"
                    );
                }
                if before[p] && !after[p] {
                    assert!(
                        told(&[
                            What::EndedWelded,
                            What::EndedCrumb,
                            What::EndedEnclosed,
                            What::EndedNoCell,
                            What::MovesWith
                        ]),
                        "seed {seed}, step {t}: plate {p} disappears, the journal does not say why"
                    );
                }
            }
        }
    }
    // The lead's case (27 November): on seed 9 a fifth of the planet was
    // cut out of the largest plate at step 9 and was back in it at step
    // 12. A plate born of a rift is never given back (version 12): it
    // never moves as one with the plate it was cut from.
    for seed in [9, 14] {
        let journal = &made(seed).history.journal;
        let mut cut: Vec<(u8, u8)> = Vec::new();
        for (t, events) in journal.iter().enumerate() {
            for e in events {
                if e.what == What::BornOfRift {
                    cut.push((e.plate, e.other));
                }
                assert!(
                    e.what != What::MovesWith
                        || !(cut.contains(&(e.plate, e.other))
                            || cut.contains(&(e.other, e.plate))),
                    "seed {seed}, step {t}: plate {} is given back to plate {}",
                    e.plate,
                    e.other
                );
            }
        }
        assert!(!cut.is_empty(), "seed {seed}: no plate born of a rift");
    }
}

#[test]
fn the_supercontinent_is_ringed_by_trenches_and_the_plates_move_from_the_start() {
    for seed in [9, 14] {
        let world = made(seed);
        let h = &world.history;
        // Its plates carry no floor: the old ocean around it is other
        // plates', and sinks under its rim.
        let supercontinent = h.plates_at_start.0;
        assert!((0..CELLS).all(|c| {
            (h.class[0][c] == history::CONTINENT) == ((h.plate[0][c] as usize) < supercontinent)
                || h.class[0][c] == history::TERRANE
        }));
        let trench = Kind::Trench as u8 + 1;
        let rim = (0..CELLS)
            .filter(|&c| {
                h.class[0][c] == history::OCEAN
                    && world.sphere.links[c]
                        .iter()
                        .any(|&(n, _)| h.class[0][n as usize] == history::CONTINENT)
            })
            .count();
        let sinking = (0..CELLS).filter(|&c| h.kind[0][c] == trench).count();
        assert!(
            2 * sinking >= rim,
            "seed {seed}: {sinking} cells of trench at the start for a rim of {rim}"
        );
        // The slabs pull from the start: no still world.
        let mut cells = [0_i64; history::MAXP];
        for &p in &h.plate[0] {
            cells[p as usize] += 1;
        }
        let speed = (0..history::MAXP)
            .map(|p| cells[p] * h.motion[0][p].speed_mm)
            .sum::<i64>()
            / CELLS as i64;
        assert!(speed >= 10, "seed {seed}: {speed} mm a year at the start");
    }
}

#[test]
fn an_old_passive_margin_bends_step_after_step_then_gives_way() {
    use history::What;
    // A continent alone on its plate with the old floor around it, two
    // ocean plates beyond, the forces free (the bench's `margin`).
    let sphere = Sphere::new();
    let at = |lon: i64| from_lat_lon(0, lon * 10);
    let seeds = [at(0), at(120), at(-120)];
    let scene = history::Scene {
        continent: sphere
            .centre
            .iter()
            .map(|&x| km_between(x, seeds[0]) < 1400)
            .collect(),
        plate: sphere
            .centre
            .iter()
            .map(|&x| {
                (0..3)
                    .min_by_key(|&k| (km_between(x, seeds[k]), k))
                    .expect("a plate") as u8
            })
            .collect(),
        continent_plates: 1,
        plates: 3,
        free_from: usize::MAX,
        turns: Vec::new(),
    };
    let settings = Settings {
        steps: 30,
        terranes: (0, 0),
        hotspots: (0, 0),
        ..Settings::default()
    };
    let h = history::run_scene(1, &settings, &sphere, &scene);
    let born: Vec<(usize, history::Event)> = h
        .journal
        .iter()
        .enumerate()
        .flat_map(|(t, events)| events.iter().map(move |&e| (t, e)))
        .filter(|(_, e)| e.what == What::BornOfTrench)
        .collect();
    let &(t, first) = born.first().expect("a trench is born");
    // Not at once: the floor is at its heaviest and takes a hundred
    // million years to bend that far.
    assert!(t >= 15, "the margin gave way at step {t}");
    // Before, no floor sinks along the continent; after, the floor that
    // left is a plate, and it sinks.
    let trench = Kind::Trench as u8 + 1;
    let sinking = |t: usize| (0..CELLS).filter(|&c| h.kind[t][c] == trench).count();
    assert_eq!(sinking(t - 1), 0);
    assert!(h.plate[t].contains(&first.plate));
    assert!(!h.plate[t - 1].contains(&first.plate));
    assert!(sinking(t) > 0);
    // The tear runs on along the margin.
    assert!(sinking((t + 3).min(h.last())) > 2 * sinking(t));
}

#[test]
fn the_sea_holds_the_planets_water_and_more_water_leaves_less_land() {
    let world = made(3);
    let s = &world.sphere;
    let whole: i64 = s.area_km2.iter().map(|&a| i64::from(a)).sum();
    let held = |w: &Planet| {
        (0..CELLS)
            .filter(|&c| w.elevation[c] < 0 && !w.basin[c])
            .map(|c| -i64::from(w.elevation[c]) * i64::from(s.area_km2[c]))
            .sum::<i64>()
            / whole
    };
    let water = world.settings.water_m;
    // The maps hold the water the history counted (they draw the
    // trenches' deeps and the hotspots' chains on top of it).
    for t in [0, world.history.last() / 2, world.history.last()] {
        let then = world.at_step(t);
        assert_eq!(then.sea, i64::from(world.history.sea_at[t]));
        assert!(
            (held(&then) - water).abs() * 50 < water,
            "step {t}: {}",
            held(&then)
        );
    }
    // A hollow the sea does not reach is not sea.
    assert!((0..CELLS).all(|c| !world.basin[c] || world.dry(c)));
    let land = |w: &Planet| w.lands.iter().map(|l| l.area_km2).sum::<i64>();
    let wetter = Planet::with(
        3,
        &Settings {
            water_m: water + 300,
            ..Settings::default()
        },
    );
    assert!(wetter.history.sea_at[0] > world.history.sea_at[0] + 150);
    assert!(wetter.sea > world.sea);
    assert!(land(&wetter) < land(world));
}

#[test]
fn what_wears_is_laid_somewhere() {
    let h = &made(14).history;
    let mut waiting = 0;
    let mut laid = 0;
    for b in &h.budget {
        assert_eq!(
            b.worn + waiting,
            b.on_land + b.on_shelf + b.on_floor + b.waiting
        );
        waiting = b.waiting;
        laid += b.on_land + b.on_shelf + b.on_floor;
    }
    assert!(laid > 0);
    let last = h.last();
    // Sediment lies on the continents and on the floor, with its date.
    for class in [history::CONTINENT, history::OCEAN] {
        assert!((0..CELLS).any(|c| h.class[last][c] == class && h.silt_at[last][c] > 100));
    }
    assert!(
        (0..CELLS).all(|c| (h.silt_at[last][c] == 0) == (h.laid_at[last][c] == history::NEVER))
    );
}

#[test]
fn the_start_relief_is_the_crusts() {
    let world = made(14);
    let (h, m) = (&world.history, &world.mosaic);
    let whole = history::CONTINENT_THICK;
    let mut thinned = 0;
    for c in (0..CELLS).filter(|&c| h.class[0][c] == history::CONTINENT) {
        let (thick, high) = (h.thick_at[0][c], i64::from(h.high_at[0][c]));
        if m.kind[c] == Some(OldKind::FailedRift) {
            let low = i64::from(whole - thick) * history::ISOSTASY_M / 10;
            assert_eq!(high, history::PLATFORM_M - low);
            thinned += usize::from(low > 300);
        } else {
            assert!(thick >= whole);
            assert_eq!(high, history::PLATFORM_M);
        }
    }
    assert!(thinned > 100);
}
