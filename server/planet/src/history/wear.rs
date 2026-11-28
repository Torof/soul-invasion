//! The sea and wear: piles spreading and wearing down, silt building the
//! margins out, the sea level, the relief levelled and filled (`weather`).

use super::*;

/// Each step, a pile as thick as two continents (70 km) spreads the
/// whole of its difference toward its neighbours, ‰; a thinner one far
/// less and a thicker one more, as the sixth power of what it has above
/// a continent's thickness (50 km: 6 ‰; 55 km: 35; 60 km: 130; 65 km:
/// 400): crust flows out from under a range only where it is thick
/// enough to be hot and weak at its base, and then fast (a plateau is
/// flat, and widens instead of rising), so a range keeps its root and
/// nothing sets a ceiling on a pile (version 15; until then every pile
/// spread 120 ‰ a step and stopped at 70 km). The flow is worked out in
/// [`SPREAD_PARTS`] parts of a step, each moving at most the whole
/// difference ([`SPREAD_MOST`]: beyond it the sum would overshoot):
/// limits of the arithmetic, not of the crust.
pub(super) const THICK_SPREAD: i64 = 1000;
pub(super) const SPREAD_PARTS: i64 = 8;
pub(super) const SPREAD_MOST: i64 = 1000;
/// Each step a continent loses [`Settings::mountain_wear`] of its
/// thickness above a continent's (wear): rock taken off the range, which
/// goes down the slopes with the rest of the sediment ([`settle`]).
///
/// Wear of the relief the crust carries: each step the height above a
/// plain's (m above the sea) loses this much (‰, slowly: an old range is
/// still a range), and slopes level by this much (‰).
pub(super) const WEAR: i64 = 20;
pub(super) const PLAIN_M: i64 = 300;
pub(super) const LEVELLING: i64 = 30;
/// A river carries its load on only down a slope: a cell keeps what
/// raises it to this much above the lowest water or ground beside it,
/// m a cell (0.1 m a km: a lowland river's slope), and passes the rest
/// on. So a pit fills to its rim, a sea's cell to a delta's plain, and
/// what is left goes on down.
pub(super) const GRADE_M: i64 = 10;
/// Sediment that fills a pit over its rim goes on from there in a later
/// pass of the same step; after this many passes what is still on its
/// way waits for the next step ([`World::dust`]): a limit of the
/// arithmetic.
pub(super) const SETTLE_PASSES: usize = 8;

/// What happens to the continents' relief in a step, apart from the
/// collisions' piles (version 7): arcs build (volcanoes on the plates
/// riding over a sinking floor, behind their active margins), stretched
/// margins cool and sink, slopes level, high ground and piles wear down.
/// All of it in the pieces' own lattices (version 10): `margin` is each
/// cell of crust's margin. Gives what each cell of crust shed, m of rock
/// times km².
pub(super) fn weather(
    sphere: &Sphere,
    land: &mut Land,
    margin: &[u8],
    grain: &[i16],
    (sea, mountain_wear): (i64, i64),
) -> Vec<i64> {
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
    // Wear stops at its base level, the sea: ground is not worn down
    // under it (version 11). What is worn is rock on its way down
    // (version 16): its volume, for each cell of crust.
    let mut shed = vec![0_i64; land.cells.len()];
    for (i, c) in land.cells.iter_mut().enumerate() {
        c.rock.rise(flow[i], BY_LEVEL);
        if c.dead {
            continue;
        }
        let r = &mut c.rock;
        if !c.under {
            let above = stands(high[i] as i16, r.thick, r.margins)
                + i64::from(grain[r.home as usize])
                - sea;
            let worn = ((high[i] - sea - PLAIN_M) * WEAR / 1000).clamp(0, above.max(0));
            r.rise(-worn, BY_WEAR);
            r.silt -= worn.min(i64::from(r.silt)) as u16;
            shed[i] = worn * i64::from(c.area);
        }
        // A pile wears from the top and floats back up: it loses
        // thickness.
        if r.class == CONTINENT {
            let worn = (i64::from(r.thick) - i64::from(CONTINENT_THICK)) * mountain_wear / 1000;
            if worn > 0 {
                r.thick -= worn as u16;
                shed[i] += worn * 100 * i64::from(c.area);
            }
        }
    }
    shed
}

/// The sea's level, m above the datum: the level at which the planet's
/// water (`water_m` over the whole sphere) fills the basins of the
/// relief as it stands (`bed`: each cell's ground). The sea rises from
/// the deepest floor and enters a cell from a cell it already covers:
/// ground below the level that it does not reach holds none of it (a
/// closed basin). With the level, the cells the sea covers.
pub(super) fn level(sphere: &Sphere, bed: &[i64], water_m: i64) -> (i64, Vec<bool>) {
    use std::{cmp::Reverse, collections::BinaryHeap};
    let area = |c: usize| i64::from(sphere.area_km2[c]);
    // (The beds are the ground's with no water on it: the water is
    // deeper than the level stands above them, `depth`.)
    let volume = water_m * (0..CELLS).map(area).sum::<i64>() * (1000 - WATER_SINKS) / 1000;
    let start = (0..CELLS).min_by_key(|&c| (bed[c], c)).expect("a cell");
    let mut queued = vec![false; CELLS];
    let mut heap = BinaryHeap::from([Reverse((bed[start], start))]);
    queued[start] = true;
    // The cells the sea has reached, in the order it reaches them as it
    // rises; their area, and the volume of ground under the datum's
    // plane there.
    let mut reached: Vec<usize> = Vec::new();
    let (mut km2, mut ground, mut sill) = (0_i64, 0_i64, i64::MIN);
    let mut sea = None;
    while let Some(Reverse((rim, c))) = heap.pop() {
        if km2 > 0 {
            // The level the water stands at over the cells reached so
            // far: below the next cell's rim, it goes no farther.
            let here = (volume + ground).div_euclid(km2).max(sill);
            if here <= rim {
                sea = Some(here);
                break;
            }
        }
        km2 += area(c);
        ground += area(c) * bed[c];
        sill = rim;
        reached.push(c);
        for &(n, _) in &sphere.links[c] {
            let n = n as usize;
            if !queued[n] {
                queued[n] = true;
                heap.push(Reverse((rim.max(bed[n]), n)));
            }
        }
    }
    let sea = sea.unwrap_or_else(|| (volume + ground).div_euclid(km2).max(sill));
    let mut wet = vec![false; CELLS];
    for c in reached {
        wet[c] = bed[c] < sea;
    }
    (sea, wet)
}

/// Each cell of the grid's ground, m above the datum: a continent's
/// crust as it stands with its grain, the ocean floor by its age with
/// its sediment.
pub(super) fn beds(rock: &[Rock], grain: &[i16], rough: &[i16], now_my: u16) -> Vec<i64> {
    (0..CELLS)
        .map(|c| {
            let r = &rock[c];
            if r.class == OCEAN {
                floor_bed(
                    i64::from(r.born_my.saturating_sub(now_my)),
                    i64::from(r.silt),
                    i64::from(rough[c]),
                )
            } else {
                stands(r.high, r.thick, r.margins) + i64::from(grain[r.home as usize])
            }
        })
        .collect()
}

/// The sediment of the step goes down and is laid (version 16; until
/// then at most 30 % of it was shared over its plate's low ground and
/// the rest vanished). What each cell of crust shed (`shed`, m times
/// km²) goes down the steepest slope of the grid's ground, cell to cell.
/// A cell on its way keeps what raises it to [`GRADE_M`] above the
/// lowest water or ground beside it and passes the rest on: so a pit
/// fills to its rim (a closed hollow that is fed fills), the first cells
/// of a sea fill to the sea's level (a shelf builds up, then out), and
/// what the continent's low ground has no room for is laid on the ocean
/// floor at its foot. Nothing is lost: what is too thin to lay (under a
/// metre over its cell) waits for the next step.
fn settle(world: &mut World, shed: &[i64], bed: &mut [i64]) {
    use std::collections::BinaryHeap;
    let sphere = world.sphere;
    let (sea, now_my) = (world.sea, world.step.now_my);
    let wet = &world.wet;
    let land = &mut world.land;
    let top = &world.top;
    let rock = &mut world.rock;
    let budget = &mut world.step.budget;
    let mut load = std::mem::replace(&mut world.dust, vec![0; CELLS]);
    for (i, &q) in seats(land, top).iter().enumerate() {
        load[q as usize] += shed[i];
    }
    budget.worn = shed.iter().sum();
    for _ in 0..SETTLE_PASSES {
        // The cells that carry a load, the highest first; 1: waiting
        // its turn, 2: done in this pass.
        let mut state = vec![0_u8; CELLS];
        let mut heap: BinaryHeap<(i64, usize)> = BinaryHeap::new();
        for c in 0..CELLS {
            if load[c] > 0 {
                state[c] = 1;
                heap.push((bed[c], c));
            }
        }
        if heap.is_empty() {
            break;
        }
        let mut later = vec![0_i64; CELLS];
        while let Some((_, c)) = heap.pop() {
            let mut left = std::mem::take(&mut load[c]);
            state[c] = 2;
            // The lowest water or ground beside it, and the way down.
            let (mut low, mut down) = (i64::MAX, (i64::MAX, c));
            for &(n, _) in &sphere.links[c] {
                let n = n as usize;
                low = low.min(if wet[n] { bed[n].max(sea) } else { bed[n] });
                down = down.min((bed[n], n));
            }
            let crust = top[c] != NO_BIT;
            let km2 = if crust {
                i64::from(land.cells[top[c] as usize].area)
            } else {
                i64::from(sphere.area_km2[c])
            }
            .max(1);
            let lifts = SILT_LIFTS;
            let room = low + GRADE_M - bed[c];
            let thick = (left / km2).min((room.max(0) * 1000 + lifts - 1) / lifts);
            if thick > 0 {
                let r = if crust {
                    &mut land.cells[top[c] as usize].rock
                } else {
                    &mut rock[c]
                };
                let up = thick * lifts / 1000;
                r.lay(thick, now_my);
                if r.class != OCEAN {
                    r.rise(up, BY_FILL);
                }
                bed[c] += up;
                left -= thick * km2;
                *(if r.class == OCEAN {
                    &mut budget.on_floor
                } else if wet[c] {
                    &mut budget.on_shelf
                } else {
                    &mut budget.on_land
                }) += thick * km2;
            }
            if left == 0 {
                continue;
            }
            if down.0 >= bed[c] {
                // Under a metre over a pit: it waits there.
                world.dust[c] += left;
            } else if state[down.1] == 2 {
                later[down.1] += left;
            } else {
                load[down.1] += left;
                if state[down.1] == 0 {
                    state[down.1] = 1;
                    heap.push((bed[down.1], down.1));
                }
            }
        }
        load = later;
    }
    for c in 0..CELLS {
        world.dust[c] += load[c];
    }
    budget.waiting = world.dust.iter().sum();
}

/// Step 10. Piles spread; the shelves build out.
pub(super) fn piles_and_silt(world: &mut World) {
    let sphere = world.sphere;
    let sea = world.sea;
    let (grain, rough) = (&world.grain, &world.rough);
    let land = &mut world.land;
    let bits = &mut world.bits;
    let disp = &mut world.disp;
    let top = &mut world.top;
    let plate = &mut world.plate;
    let rock = &mut world.rock;
    let next_bit = &mut world.next_bit;
    let now_my = world.step.now_my;
    let built = &mut world.step.built;
    let to_shelf = &mut world.step.budget.to_shelf;
    let (weight, floor) = (world.step.weight, world.step.floor);
    let key = move |p: u8, class: u8| precedence(&weight, &floor, p, class);
    // Piles spread, in their pieces: what stands above a continent's
    // thickness (a rift's thinned crust is not filled again from its
    // sides, version 11). They wear with the rest of the relief
    // (`weather`).
    let whole = i64::from(CONTINENT_THICK);
    let mut thick: Vec<i64> = land.cells.iter().map(|c| i64::from(c.rock.thick)).collect();
    for _ in 0..SPREAD_PARTS {
        // Only a pile flows: each pair of neighbours of which one at
        // least stands above a continent's thickness, once.
        let mut spread = vec![0_i64; thick.len()];
        for i in 0..thick.len() {
            if thick[i] <= whole {
                continue;
            }
            for (j, _) in land.kin(sphere, i) {
                if land.cells[j].plate != land.cells[i].plate || (thick[j] > whole && j < i) {
                    continue;
                }
                let (mine, theirs) = (thick[i], thick[j].max(whole));
                let part = (mine.max(theirs) - whole) * 1000 / whole;
                let cube = part * part / 1000 * part / 1000;
                let rate =
                    (THICK_SPREAD * cube / 1000 * cube / 1000 / SPREAD_PARTS).min(SPREAD_MOST);
                let flow = (theirs - mine) * rate / 1000 / 8;
                spread[i] += flow;
                spread[j] -= flow;
            }
        }
        for i in 0..thick.len() {
            thick[i] += spread[i];
        }
    }
    for (i, c) in land.cells.iter_mut().enumerate() {
        if !c.dead {
            c.rock.thick = thick[i].max(i64::from(OCEAN_THICK)) as u16;
        }
    }
    // A shelf builds out (version 16): floor beside a continent that
    // sediment has buried up to the sea's level is that continent's
    // crust from now on, a cell of its own lattice, made of the floor
    // and of what lies on it. (Until version 16 the silt of a plate's
    // piles built 20 km of crust at once on a coast of the plate drawn
    // by lot.)
    let mut shore: Vec<(u32, u32, usize)> = Vec::new();
    for i in 0..land.cells.len() {
        let c = land.cells[i];
        if c.dead || c.under || c.rock.class != CONTINENT {
            continue;
        }
        for &(k, _) in &sphere.links[c.at as usize] {
            if land.get(c.frame, k).is_none() {
                shore.push((c.frame, k, i));
            }
        }
    }
    shore.sort_unstable();
    shore.dedup_by_key(|x| (x.0, x.1));
    *built = 0;
    for &(frame, k, i) in &shore {
        let y = normalize(apply(&land.rot[frame as usize], sphere.centre[k as usize]));
        let m = sphere.cell_of(y);
        let under = rock[m];
        let bed = floor_bed(
            i64::from(under.born_my.saturating_sub(now_my)),
            i64::from(under.silt),
            i64::from(rough[m]),
        );
        if under.class != OCEAN
            || under.silt == 0
            || bed < sea
            || land.cover(sphere, (y, m), &key, NO_BIT).is_some()
        {
            continue;
        }
        let from = land.cells[i];
        let full = i64::from(sphere.area_km2[k as usize]);
        let silt = (i64::from(under.silt) * i64::from(sphere.area_km2[m]) / full.max(1))
            .min(i64::from(u16::MAX));
        *built += full;
        *to_shelf += silt * full;
        let made_of = Rock {
            thick: OCEAN_THICK + (silt / 100) as u16,
            high: lift(0, bed - i64::from(grain[m])),
            silt: silt as u16,
            laid: under.laid,
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
        rock[m].silt = 0;
        rock[m].laid = NEVER;
        if disp[m] != NO_BIT {
            bits[disp[m] as usize].rock = rock[m];
        }
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
/// Step 18 (the sea). The sea's level: the level at which the planet's
/// water fills the basins of the relief as it stands (version 16; until
/// then "the level that leaves 13 % of the sphere dry", and another one
/// on the maps). It is the base level of wear and of filling, and the
/// level the maps draw. Then the relief wears down and its sediment is
/// laid.
pub(super) fn sea_and_weather(world: &mut World) {
    let (t, now_my) = (world.step.t, world.step.now_my);
    let mut bed = beds(&world.rock, &world.grain, &world.rough, now_my);
    (world.sea, world.wet) = level(world.sphere, &bed, world.s.water_m);
    if t > 0 {
        let shed = weather(
            world.sphere,
            &mut world.land,
            &world.step.margin_of,
            &world.grain,
            (world.sea, world.s.mountain_wear),
        );
        paint(&world.land, &world.top, &mut world.plate, &mut world.rock);
        bed = beds(&world.rock, &world.grain, &world.rough, now_my);
        settle(world, &shed, &mut bed);
    }
    paint(&world.land, &world.top, &mut world.plate, &mut world.rock);
    sync(&mut world.bits, &world.plate, &world.rock, &world.disp);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sea_fills_the_basins_it_reaches_and_no_other() {
        let sphere = Sphere::new();
        let pole = [0, 0, ONE];
        // A floor 4,000 m down, a continent 500 m up within 3,000 km of
        // the pole, and in its middle a hollow 1,000 m below the datum.
        let bed: Vec<i64> = (0..CELLS)
            .map(|c| match km_between(sphere.centre[c], pole) {
                d if d < 500 => -1_000,
                d if d < 3_000 => 500,
                _ => -4_000,
            })
            .collect();
        let km2 = |keep: &dyn Fn(usize) -> bool| -> i64 {
            (0..CELLS)
                .filter(|&c| keep(c))
                .map(|c| i64::from(sphere.area_km2[c]))
                .sum()
        };
        let (whole, floor) = (km2(&|_| true), km2(&|c| bed[c] == -4_000));
        // Water for 3,000 m over the floor, as deep as it lies under its
        // own weight.
        let water = 3_000 * floor / whole * 1000 / (1000 - WATER_SINKS);
        let (sea, wet) = level(&sphere, &bed, water);
        assert!((sea + 1_000).abs() < 20, "{sea}");
        assert!((0..CELLS).all(|c| wet[c] == (bed[c] == -4_000)));
        // The hollow lies below that sea and holds none of it; with
        // enough water to pass the continent it is sea.
        let (sea, wet) = level(&sphere, &bed, 2 * water);
        assert!(sea > 500, "{sea}");
        assert!(wet.iter().all(|&w| w));
    }
}
