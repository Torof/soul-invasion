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
/// Wear (version 18: one law, and it takes crust). Running water cuts
/// as the water it gathers times the slope it runs down (the stream
/// power law of river incision; Howard 1994; Whipple and Tucker 1999).
/// At the grid's scale: ground `x` m above its base level, standing `r`
/// m above the lowest ground or water beside it (its relief), loses
/// `x · r / WEAR_M_MY` m of rock from its top a million years. One
/// number, read off the denudation measured on the Earth: a steep
/// range's front (5,000 m, 4,000 m of relief) loses some 3,000 m a My
/// (the Himalaya: 1 to 5 mm a year); an old range (800 m, 500 m) 70 (40
/// to 100); a shield (500 m, 50 to 100 m) 4 to 8 (2 to 10); and the
/// rate for each m of relief, `x / WEAR_M_MY`, is the 0.2 to 0.3 m a My
/// measured in mountains of middling height (Ahnert 1970; Pinet and
/// Souriau 1988; Montgomery and Brandon 2002). So high, steep ground
/// wears very fast, a plateau's flat top slowly, a plain hardly:
/// nothing sets a ceiling on a range but this and the flow of its root.
/// The rock taken is crust taken ([`Rock::wear`]): the crust floats
/// back up by all but [`ISOSTASY_M`] of it, so a range has lost several
/// times its height in rock before it is a plain, and all of that rock
/// is sediment ([`settle`]). The base level is the sea, and inland a
/// river's own slope up from it ([`GRADE_M`] a cell: a river at grade
/// carries, it does not cut); until version 18 it was "300 m above the
/// sea" everywhere, and wear took height with no crust. A cell is not
/// cut below the ground beside it in one step: the rate is worked out
/// on the relief left ([`worn`]).
///
/// Slopes level by [`LEVELLING`] ‰ of a difference of height a step:
/// rock slides and washes to the lower cell beside it (crust, like the
/// rest).
pub(super) const WEAR_M_MY: i64 = 6_000;
pub(super) const LEVELLING: i64 = 30;

/// How far `my` million years of wear lower ground `x` m above its base
/// level with `r` m of relief, m: the law above and the crust floating
/// back, `dr/dt = −ISOSTASY · x · r / WEAR`, worked out on the relief
/// at the end of the step.
pub(super) fn worn(x: i64, r: i64, my: i64) -> i64 {
    if x <= 0 || r <= 0 {
        return 0;
    }
    let (k, a) = (1000 * WEAR_M_MY, x * my * ISOSTASY_M);
    r.min(x) * a / (k + a)
}

/// What is left of a range `x` m above its base level after `my`
/// million years of wear with nothing building it, its relief being its
/// height (`dx/dt = −ISOSTASY · x² / WEAR`, exactly).
pub(super) fn worn_to(x: i64, my: i64) -> i64 {
    let k = 1000 * WEAR_M_MY;
    if x <= 0 {
        x
    } else {
        x * k / (k + x * my * ISOSTASY_M)
    }
}

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

/// What happens to the continents' relief in a step, apart from what
/// adds crust (collisions, arcs): warm crust cools and sinks, slopes
/// level, ground above its base level wears. All of it in the pieces'
/// own lattices (version 10). `base`: each grid cell's base level, and
/// the lowest ground or water beside it, m above the datum. Gives what
/// each cell of crust shed, m of rock times km² (crust and sediment
/// taken off it, to the metre), and how much of it all was crust.
pub(super) fn weather(
    sphere: &Sphere,
    land: &mut Land,
    base: &[(i64, i64)],
    grain: &[i16],
    step_my: i64,
) -> (Vec<i64>, i64) {
    for c in &mut land.cells {
        let r = &mut c.rock;
        if r.warm > 0 {
            let sunk = (i64::from(r.warm) * COOLING / 1000).max(1);
            r.heat(-sunk, BY_COOLING);
        }
    }
    // Slopes level: rock goes to the lower of two cells of a piece, as
    // much as changes their heights by a share of their difference
    // (the volume is kept: by the smaller of the two cells).
    let high: Vec<i64> = land
        .cells
        .iter()
        .map(|c| stands(c.rock.high(), c.rock.thick, c.rock.margins))
        .collect();
    let mut flow = vec![0_i64; land.cells.len()];
    for i in 0..land.cells.len() {
        let mine = i64::from(land.cells[i].area);
        flow[i] = land
            .kin(sphere, i)
            .map(|(j, _)| {
                (high[j] - high[i]) * LEVELLING / 8 / ISOSTASY_M
                    * mine.min(i64::from(land.cells[j].area))
            })
            .sum();
    }
    // Wear stops at its base level: ground is not worn down under the
    // sea (version 11).
    let mut shed = vec![0_i64; land.cells.len()];
    let mut crust = 0;
    for (i, c) in land.cells.iter_mut().enumerate() {
        let km2 = i64::from(c.area).max(1);
        c.rock.thicken(flow[i] / km2, BY_LEVEL);
        if c.dead || c.under {
            continue;
        }
        let r = &mut c.rock;
        let stands = stands(r.high(), r.thick, r.margins) + i64::from(grain[r.home as usize]);
        let (base, low) = base[c.cell as usize];
        let lower = worn(stands - base, stands - low, step_my);
        if lower > 0 {
            let (loose, hard) = r.wear(lower * 1000 / ISOSTASY_M);
            shed[i] = (loose + hard) * km2;
            crust += hard * km2;
        }
    }
    (shed, crust)
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
                    r.thick,
                    i64::from(rough[c]),
                )
            } else {
                stands(r.high(), r.thick, r.margins) + i64::from(grain[r.home as usize])
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
                let was = i64::from(r.high());
                r.lay(thick, now_my);
                bed[c] += if r.class == OCEAN {
                    thick * lifts / 1000
                } else {
                    i64::from(r.high()) - was
                };
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
    // Piles spread: what stands above a continent's thickness flows
    // toward any thinner crust that touches it, of its piece or of
    // another lying against it (version 18; in its own piece only until
    // then, so an island arc docked on another, a piece of one cell,
    // could not flow and stood 100 km thick and more). A rift's thinned
    // crust is not filled again from its sides (version 11). The volume
    // is kept: by the smaller of the two cells. They wear with the rest
    // of the relief (`weather`).
    let whole = i64::from(CONTINENT_THICK) * 100;
    let mut thick: Vec<i64> = land.cells.iter().map(|c| c.rock.crust_m()).collect();
    let km2 = |i: usize| i64::from(land.cells[i].area).max(1);
    let takes = |j: usize| !land.cells[j].dead && !land.cells[j].under;
    // (The crust that touches a cell, found once a step.)
    let mut touches: Vec<Option<Vec<usize>>> = vec![None; thick.len()];
    for _ in 0..SPREAD_PARTS {
        // Only a pile flows: each pair of neighbours of which one at
        // least stands above a continent's thickness, once.
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for i in 0..thick.len() {
            if thick[i] <= whole || land.cells[i].dead {
                continue;
            }
            let near = touches[i].get_or_insert_with(|| {
                let kin = land.kin(sphere, i).map(|(j, _)| j);
                kin.chain(land.beside(sphere, i))
                    .filter(|&j| takes(j))
                    .collect()
            });
            pairs.extend(near.iter().map(|&j| (i.min(j), i.max(j))));
        }
        pairs.sort_unstable();
        pairs.dedup();
        let mut spread = vec![0_i64; thick.len()];
        for &(i, j) in &pairs {
            let (mine, theirs) = (thick[i].max(whole), thick[j].max(whole));
            let part = (mine.max(theirs) - whole) * 1000 / whole;
            let cube = part * part / 1000 * part / 1000;
            let rate = (THICK_SPREAD * cube / 1000 * cube / 1000 / SPREAD_PARTS).min(SPREAD_MOST);
            let flow = (theirs - mine) * rate / 1000 / 8 * km2(i).min(km2(j));
            spread[i] += flow;
            spread[j] -= flow;
        }
        for i in 0..thick.len() {
            thick[i] += spread[i] / km2(i);
        }
    }
    for (i, c) in land.cells.iter_mut().enumerate() {
        if !c.dead {
            c.rock.thicken(thick[i] - c.rock.crust_m(), BY_LEVEL);
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
            under.thick,
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
        let mut made_of = Rock {
            thick: under.thick,
            fine: 0,
            silt: silt as u16,
            laid: under.laid,
            warm: 0,
            why: [0; CAUSES],
            home: m as u32,
            margins: 0,
            rift: 0,
            orogeny: NEVER,
            ..from.rock
        };
        // It stands where the buried floor stood.
        made_of.stand_at(bed - i64::from(grain[m]));
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
        // The base level of wear: the sea, and inland a river's slope
        // up from the nearest shore.
        let shore: Vec<(usize, u32)> = (0..CELLS)
            .filter(|&c| world.wet[c])
            .map(|c| (c, 0))
            .collect();
        let (inland, _) = world.sphere.spread(&shore, |_, _| true, u32::MAX);
        let base: Vec<(i64, i64)> = (0..CELLS)
            .map(|c| {
                let low = world.sphere.links[c]
                    .iter()
                    .map(|&(n, _)| {
                        let n = n as usize;
                        if world.wet[n] {
                            bed[n].max(world.sea)
                        } else {
                            bed[n]
                        }
                    })
                    .min()
                    .unwrap_or(bed[c]);
                let up = i64::from(inland[c].min(40_000)) * GRADE_M / CELL_KM;
                (world.sea + up, low)
            })
            .collect();
        let (shed, crust) = weather(
            world.sphere,
            &mut world.land,
            &base,
            &world.grain,
            world.s.step_my,
        );
        paint(&world.land, &world.top, &mut world.plate, &mut world.rock);
        bed = beds(&world.rock, &world.grain, &world.rough, now_my);
        settle(world, &shed, &mut bed);
        world.step.budget.crust = crust;
    }
    paint(&world.land, &world.top, &mut world.plate, &mut world.rock);
    sync(&mut world.bits, &world.plate, &world.rock, &world.disp);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_steep_ground_wears_fast_a_plateau_slowly_and_none_below_its_neighbour() {
        // A range's front against a plain as high above its base level.
        assert!(worn(4_000, 3_000, 5) > 20 * worn(500, 100, 5));
        // A plateau's flat top hardly wears, its edge does.
        assert!(worn(5_000, 100, 5) * 10 < worn(5_000, 4_000, 5));
        // Never below the ground beside it, nor below its base level,
        // however long the step.
        assert!(worn(8_000, 3_000, 1_000) <= 3_000);
        assert!(worn(1_000, 3_000, 1_000) <= 1_000);
        assert_eq!(worn(-10, 500, 5), 0);
        assert_eq!(worn(500, -10, 5), 0);
        // A range left alone: half its height in some 10 My at 4,000 m,
        // and still a few hundred metres after 300 My.
        assert!((1_500..2_500).contains(&worn_to(4_000, 10)));
        assert!((50..400).contains(&worn_to(4_000, 300)));
    }

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
