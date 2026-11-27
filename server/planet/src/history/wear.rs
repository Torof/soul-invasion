//! The sea and wear: piles spreading and wearing down, silt building the
//! margins out, the sea level, the relief levelled and filled (`weather`).

use super::*;

/// Each step, a pile spreads this much toward its neighbours, ‰ (slowly:
/// a range stays a range, version 6).
pub(super) const THICK_SPREAD: i64 = 120;
/// Each step a continent loses [`Settings::mountain_wear`] of its
/// thickness above a continent's (wear); the silt builds its plate's
/// margins out, a shore cell at a time once there is enough for
/// [`SPILL_THICK`] there (no continental crust is ever lost, 27 November,
/// version 5.1).
pub(super) const SPILL_THICK: i64 = 200;

/// Wear: each step the height above a plain's (m) loses this much (‰,
/// slowly: an old range is still a range), slopes level by this much
/// (‰), and the sediment fills its plate's low ground, at most this share
/// (‰) of what lies below a shelf's depth (m).
pub(super) const WEAR: i64 = 20;
pub(super) const PLAIN_M: i64 = 300;
pub(super) const LEVELLING: i64 = 30;
pub(super) const FILLING: i64 = 300;
pub(super) const SHELF_M: i64 = -150;

/// What happens to the continents' relief in a step, apart from the
/// collisions' piles (version 7): arcs build (volcanoes on the plates
/// riding over a sinking floor, behind their active margins), stretched
/// margins cool and sink, high ground wears down and its sediment fills
/// the low ground. All of it in the pieces' own lattices (version 10):
/// `margin` is each cell of crust's margin.
pub(super) fn weather(sphere: &Sphere, land: &mut Land, margin: &[u8], grain: &[i16], sea: i64) {
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

/// Step 9. Piles spread and wear down; their silt builds the plate's
/// margins out.
pub(super) fn piles_and_silt(world: &mut World) {
    let seed = world.seed;
    let s = world.s;
    let sphere = world.sphere;
    let grain = &mut world.grain;
    let silt = &mut world.silt;
    let land = &mut world.land;
    let disp = &mut world.disp;
    let top = &mut world.top;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let next_bit = &mut world.next_bit;
    let t = world.step.t;
    let built = &mut world.step.built;
    let (weight, floor) = (world.step.weight, world.step.floor);
    let key = move |p: u8, class: u8| precedence(&weight, &floor, p, class);
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
        c.rock.thick = (thick[i] + spread[i]).clamp(i64::from(OCEAN_THICK), THICK_MAX) as u16;
        if c.rock.class != CONTINENT {
            continue;
        }
        let worn = (i64::from(c.rock.thick) - i64::from(CONTINENT_THICK)) * s.mountain_wear / 1000;
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
            if rock[m].class != OCEAN || land.cover(sphere, (y, m), &key, NO_BIT).is_some() {
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
    *built = 0;
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
        *built += full;
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
        let j = land.add(sphere, (frame, k), from.plate, made_of, *next_bit);
        *next_bit += 1;
        // The view: the cells whose centre the new crust covers.
        for q in std::iter::once(m).chain(sphere.links[m].iter().map(|&(a, _)| a as usize)) {
            if top[q] == NO_BIT
                && land.cover(sphere, (sphere.centre[q], q), &key, NO_BIT) == Some(j)
            {
                top[q] = j as u32;
                disp[q] = NO_BIT;
            }
        }
    }
    paint(&*land, &*top, &mut *plate, &mut *rock);
}
/// Step 12 (the sea). The sea level, the base level of wear and of
/// filling; the relief wears down and fills.
pub(super) fn sea_and_weather(world: &mut World) {
    let s = world.s;
    let sphere = world.sphere;
    let grain = &mut world.grain;
    let land = &mut world.land;
    let bits = &mut world.bits;
    let disp = &mut world.disp;
    let top = &mut world.top;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let t = world.step.t;
    let margin_of = &mut world.step.margin_of;
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
        weather(sphere, &mut *land, &*margin_of, &*grain, sea);
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
    paint(&*land, &*top, &mut *plate, &mut *rock);
    sync(&mut *bits, &*plate, &*rock, &*disp);
}
