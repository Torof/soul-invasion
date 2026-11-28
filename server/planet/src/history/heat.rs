//! The mantle's heat (warm under a lid of continent, cold where slabs
//! sink), its plumes, the hotspots' marks and the lava floods.

use super::*;

/// The mantle's heat, one layer on the grid (0: the mantle's mean). A
/// continent is a lid: the mantle under it warms toward a thousand at
/// [`HEAT_LID_KM`] from the sea, and on at that slope up to
/// [`HEAT_LID_REACH_KM`] (a supercontinent's heart is the hottest place
/// of the planet).
pub(super) const HEAT_LID_KM: i64 = 2_000;
pub(super) const HEAT_LID_REACH_KM: i64 = 4_000;
/// A sinking slab cools the mantle by this at its trench, fading to
/// nothing this far from it, km.
pub(super) const HEAT_SLAB: i64 = 600;
pub(super) const HEAT_SLAB_KM: i64 = 800;
/// The deep mantle's own warm and cold provinces, at most (they stay
/// where they are: plumes rise from the warm ones).
pub(super) const HEAT_DEEP: i64 = 300;
pub(super) const HEAT_DEEP_GRAIN: [(i64, i64); 2] = [(900_000, 2), (400_000, 1)];
/// Each step the mantle goes this much of the way to what the crust
/// above makes of it, per mille (heat lingers where a continent stood).
pub(super) const HEAT_RATE: i64 = 150;
/// And spreads to its neighbours, this many times.
pub(super) const HEAT_SWEEPS: usize = 2;
/// Plumes rise where the mantle is hottest, give or take this much heat
/// drawn for each cell, and stand at least this far apart, km.
pub(super) const PLUME_LUCK: u64 = 2_000;
pub(super) const PLUME_APART_KM: i64 = 1_800;

/// Flood basalts: a plume this near a boundary that opens floods the
/// continent this far around it, km.
pub(super) const LIP_REACH_KM: i64 = 800;
pub(super) const LIP_KM: i64 = 400;

pub(super) const PLUME_UP_M: i64 = 60;

/// The mantle's heat a step later: it goes part of the way to what the
/// crust above makes of it (warm under a continent, the more the farther
/// from the sea: a lid; cold around the slabs that sink; the deep
/// mantle's own provinces), and spreads.
pub(super) fn warm(
    sphere: &Sphere,
    rock: &[Rock],
    slab: &[Spot],
    deep: &[i32],
    heat: &mut [i32],
    rate: i64,
) {
    let sea: Vec<(usize, u32)> = (0..CELLS)
        .filter(|&c| rock[c].class == OCEAN)
        .map(|c| (c, 0))
        .collect();
    let (inland, _) = sphere.spread(
        &sea,
        |_, b| rock[b].class != OCEAN,
        HEAT_LID_REACH_KM as u32,
    );
    let trench: Vec<(usize, u32)> = slab.iter().map(|s| (s.0, 0)).collect();
    let (sinking, _) = sphere.spread(&trench, |_, _| true, HEAT_SLAB_KM as u32);
    for c in 0..CELLS {
        let mut goal = i64::from(deep[c]);
        if rock[c].class != OCEAN {
            let d = i64::from(inland[c]).min(HEAT_LID_REACH_KM);
            goal += d * 1000 / HEAT_LID_KM;
        }
        if sinking[c] != u32::MAX {
            goal -= HEAT_SLAB * (HEAT_SLAB_KM - i64::from(sinking[c])) / HEAT_SLAB_KM;
        }
        let now = i64::from(heat[c]);
        heat[c] = (now + (goal - now) * rate / 1000) as i32;
    }
    for _ in 0..HEAT_SWEEPS {
        let before = heat.to_vec();
        for c in 0..CELLS {
            let around: i64 = sphere.links[c]
                .iter()
                .map(|&(n, _)| i64::from(before[n as usize]))
                .sum();
            let mean = around / sphere.links[c].len().max(1) as i64;
            heat[c] = i64::midpoint(i64::from(before[c]), mean) as i32;
        }
    }
}

/// Flood basalts where a plume stands near a boundary that opens between
/// `pair`: the continent around takes the [`LIP`] flag, in its pieces.
/// Each plume that floods (once in the history), and the area flooded,
/// km².
pub(super) fn flood(
    sphere: &Sphere,
    land: &mut Land,
    top: &[u32],
    pair: (u8, u8),
    plumes: &[V3],
    spent: &mut [bool],
) -> Vec<(V3, i64)> {
    let ok: Vec<bool> = land
        .cells
        .iter()
        .map(|c| !c.dead && (c.plate == pair.0 || c.plate == pair.1))
        .collect();
    let edge: Vec<usize> = (0..land.cells.len())
        .filter(|&i| {
            ok[i]
                && land
                    .facing(sphere, top, i)
                    .iter()
                    .any(|&o| o == pair.0 || o == pair.1)
        })
        .collect();
    let mut out = Vec::new();
    for (k, &plume) in plumes.iter().enumerate() {
        if spent[k] {
            continue;
        }
        let near: Vec<usize> = edge
            .iter()
            .copied()
            .filter(|&i| km_between(land.cells[i].pos, plume) < LIP_REACH_KM)
            .collect();
        if near.is_empty() {
            continue;
        }
        let d = land.spread(sphere, &near, |j| ok[j], LIP_KM as u32);
        let mut km2 = 0;
        for (i, c) in land.cells.iter_mut().enumerate() {
            if d[i] != u32::MAX {
                c.rock.margins |= LIP;
                km2 += i64::from(c.area);
            }
        }
        spent[k] = true;
        out.push((plume, km2));
    }
    out
}

/// Step 10. The hotspots mark the crust passing over them and lift it.
pub(super) fn hotspots_mark(world: &mut World) {
    let s = world.s;
    let sphere = world.sphere;
    let hotspots = &world.hotspots[..];
    let land = &mut world.land;
    let bits = &mut world.bits;
    let disp = &mut world.disp;
    let marks = &mut world.marks;
    let now_my = world.step.now_my;
    let moved = &mut world.step.moved;
    let (weight, floor) = (world.step.weight, world.step.floor);
    let key = move |p: u8, class: u8| precedence(&weight, &floor, p, class);
    // 10. The hotspots mark the crust passing over them, a mark a
    // million years, carried with it from then on.
    for (k, &hp) in hotspots.iter().enumerate() {
        let c = sphere.cell_of(hp);
        let above = land.cover(sphere, (hp, c), &key, NO_BIT);
        let first = match above {
            Some(j) => (true, j as u32, land.cells[j].plate),
            None if disp[c] != NO_BIT => (false, disp[c], bits[disp[c] as usize].plate),
            None => continue,
        };
        // A plume lifts the continent over it and floods it a little.
        if let Some(j) = above {
            let around: Vec<usize> = land.kin(sphere, j).map(|(i, _)| i).collect();
            land.cells[j].rock.heat(PLUME_UP_M, BY_PLUME);
            for i in around {
                land.cells[i].rock.heat(PLUME_UP_M / 3, BY_PLUME);
            }
        }
        let p = first.2;
        let w = moved[p as usize];
        let speed = norm(w);
        for j in 0..s.step_my {
            let pos = if speed < 50 || j == 0 {
                hp
            } else {
                normalize(apply(
                    &rotation(normalize(w), speed * j * ONE / (1000 * RADIUS_KM)),
                    hp,
                ))
            };
            let cj = sphere.cell_of(pos);
            let (on_land, bit) = match land.cover(sphere, (pos, cj), &key, NO_BIT) {
                Some(i) if land.cells[i].plate == p => (true, i as u32),
                _ if disp[cj] != NO_BIT && bits[disp[cj] as usize].plate == p => (false, disp[cj]),
                _ => (first.0, first.1),
            };
            marks.push(Mark {
                land: on_land,
                bit,
                pos,
                spot: k as u8,
                my: now_my + j as u16,
            });
        }
    }
}
/// Step 12 (the mantle). The heat under the crust as it now lies.
pub(super) fn warm_step(world: &mut World) {
    let sphere = world.sphere;
    let deep = &mut world.deep;
    let heat = &mut world.heat;
    let land = &mut world.land;
    let rock = &mut world.rock;
    let out_heat = &mut world.out_heat;
    let t = world.step.t;
    let now_my = world.step.now_my;
    let seen = &mut world.step.seen;
    let margin_of = &mut world.step.margin_of;
    // The mantle under the crust as it now lies.
    if t > 0 {
        warm(sphere, &*rock, &seen.slab, &*deep, &mut *heat, HEAT_RATE);
    }
    out_heat.push(
        heat.iter()
            .map(|&v| v.clamp(-30_000, 30_000) as i16)
            .collect::<Vec<i16>>(),
    );
    for (i, c) in land.cells.iter_mut().enumerate() {
        if margin_of[i] == Margin::Active as u8 {
            c.rock.orogeny = now_my;
        }
    }
}
