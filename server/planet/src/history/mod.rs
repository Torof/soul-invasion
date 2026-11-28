//! The plates' history (version 4, the lead's 26 November: « la géologie,
//! c'est des plaques qui bougent »). The plates are what moves, through
//! the whole history; the continents ride on them.
//!
//! 1. **A supercontinent**: one warped cap carrying all the continental
//!    crust, a mosaic (version 13): old cores (cratons) welded by wide
//!    belts of younger, weaker crust.
//! 2. **Plates.** The supercontinent lies on its own plates (a warped,
//!    weighted Voronoi of its crust) and oceanic plates share the rest of
//!    the sphere, up to its rim (version 14): the old ocean around a
//!    supercontinent sinks under it, and a slab hangs all along the rim
//!    from the start. At every step each cell belongs to exactly one plate;
//!    each plate turns about its own Euler pole and carries all its crust,
//!    oceanic and continental, with the crust's record.
//! 3. **Forces, a balance on each plate at each step** (version 6). A
//!    plate's turn comes from the forces on its cells, summed as a
//!    torque: slab pull, the strongest, draws a plate toward every trench
//!    where it sinks, the more the older (heavier) its floor there; the
//!    same slab draws the plate riding over it toward the trench (trench
//!    suction: the pull a plate all continent has); ridges push, a
//!    little; the mantle carries the continents down the slope of its
//!    heat; a collision's front resists. Against them the mantle's drag,
//!    which grows with speed and is stronger under continental keels.
//!    **The mantle's heat** is one layer on the grid: warm under a large
//!    continent (a lid), cold where slabs sink, with the deep mantle's
//!    own provinces; it lingers and spreads. The heat under the
//!    supercontinent is what breaks it (no scripted upwelling), and
//!    plumes rise where it is hottest.
//! 4. **Divergent boundaries** leave gaps: new oceanic crust of age 0
//!    fills them from both sides, so the ocean floor carries symmetric age
//!    stripes.
//! 5. **Convergent boundaries.** Where two plates claim a cell, oceanic
//!    crust sinks and is destroyed: always under a continent, and the
//!    older, denser one under the younger; the riding edge of an ocean
//!    grows an island arc. Continental crust never sinks: continent
//!    against continent is a collision, a process: the crust run under
//!    piles up, thicker and higher, and its weight pushes back on the two
//!    plates, harder as it thickens (version 15); they slow, and once the
//!    pile holds all the forces press them with they turn as one and are
//!    one plate. No grip on their speed, no count of quiet steps, no
//!    block pushed out sideways. A pile flows out at its base only where
//!    it is very thick, and wears from the top slowly: a range keeps its
//!    root for hundreds of millions of years. A terrane or an arc against
//!    a continent is accreted to its plate.
//!    **A trench is a state** (version 14), not only a motion: where a
//!    slab hangs it pulls by its weight however slowly the plates close,
//!    and is lost only where they part. **And a trench is born, for a
//!    cause** (version 17): a passive margin gives way where the plates
//!    push its floor on its continent harder than the margin's section
//!    holds, two steps running; that floor is then a plate of its own
//!    and begins to sink (`trenches::margins_give_way`). A quiet margin
//!    holds however old it is. An ocean that opened can close.
//! 6. **The break-up only opens.** Plates whose continents touch and have
//!    not parted move as one while the forces press them together: a
//!    boundary inside a continent exists only once it opens.
//! 7. **Rifts** (version 11, the lead's 27 November: « de vrais
//!    rifts »). The world starts with few plates, the supercontinent on
//!    one: plates are born of rifts, and a rift is a process. Where the
//!    forces on a plate's two sides would pull them apart and the
//!    lithosphere is weak (a warm mantle under it, an old suture, an old
//!    failed rift), a trough begins: the crust along it stretches, thins
//!    and sinks step after step, slowly at first, faster as it necks;
//!    its tips grow toward the weakest crust ahead. Once the trough has
//!    crossed its continent and its crust has given way, the plate is
//!    cut in two along it, each side keeping its lattice (the two shores
//!    fit), and the forces alone part them. A trough the pull has left
//!    is abandoned: a failed rift, its thinned crust a basin. No size
//!    threshold, no drawn date, no push added. Where a plume stands near
//!    a boundary that opens, the continent is flooded with basalt.
//! 8. **Hotspots** mark the crust passing over them, a mark a million
//!    years, carried with the crust: their chains bend as the plates
//!    turned, and are as old as the history.
//! 9. **Relief** (version 7, the lead's 27 November). A continent's
//!    height is carried by its crust, like its thickness, and changes
//!    only when something happens to that crust: a collision piles it up
//!    and it floats higher; a rift stretches it and it sinks, then sinks
//!    as much again as it cools (a passive margin's shelf); an arc, a
//!    docking terrane or a plume raise it where they act; high ground
//!    wears down slowly and its sediment fills the low ground. The
//!    supercontinent starts with a relief of its own, once. Nothing is
//!    recomputed from where the crust happens to lie.
//!
//! Moving the crust. The ocean floor (version 5) is a set of bits at
//! their exact places; each step every bit turns with its plate, and the
//! grid only draws them: each cell shows one bit (the winner where
//! several meet), a bit shows in one cell at most, and nothing is copied.
//!
//! The continents (version 10, the lead's 27 November: « de vraies
//! pièces rigides ») are rigid pieces. Every cell of continental crust
//! lives for good in its piece's own frame (the grid's lattice as it lay
//! when the piece was born) with the turn that places the frame on the
//! planet today. Moving a piece changes its turn only: nothing in it is
//! resampled, ever. A piece changes only by named events applied in its
//! frame: a collision's front (its cells run under another piece pile on
//! it), a rift (the piece is cut in two, each half keeping the lattice,
//! so the two shores fit for good), an arc or a docking, wear, silt. The
//! planet's grid is a view of the pieces (each cell shows the crust over
//! its centre), drawn for the forces and for the neighbourhood queries
//! (who touches whom): no state of a continent lives on it. Today's
//! plates are the plates of the last step.

use crate::{
    CELLS, Kind, Mosaic, ONE, OldKind, P_RELIEF, RADIUS_KM, Sphere, TAU, V3, add, bump, classify,
    cos, cross, dot, draw, east_at, fbm3, hash, km_between, latitude, norm, normalize, north_at,
    quantile, random_unit, scale, sin, step, sub, warp,
};

mod collisions;
mod crust;
mod floor;
mod forces;
mod heat;
mod pieces;
mod plates;
mod record;
mod rifts;
mod settings;
mod start;
mod state;
mod trenches;
mod wear;

pub use self::{crust::*, forces::*, pieces::*, record::*, settings::*, state::Scene};
use self::{heat::*, plates::*, rifts::*, start::*, state::*, trenches::*};

// The draws' purposes.
pub(super) const P_UP: u64 = 0x0c50_0101;
pub(super) const P_SUPER: u64 = 0x0c50_0102;
pub(super) const P_FRAG: u64 = 0x0c50_0103;
pub(super) const P_HEAT: u64 = 0x0c50_0104;
pub(super) const P_HOT: u64 = 0x0c50_010d;
pub(super) const P_TERRANE: u64 = 0x0c50_0106;
pub(super) const P_AGE: u64 = 0x0c50_0107;
pub(super) const P_OCEAN: u64 = 0x0c50_0108;
pub(super) const P_ARC: u64 = 0x0c50_010a;

/// The history of the planet born from `seed`.
pub fn run(seed: u64, s: &Settings, sphere: &Sphere, mosaic: &Mosaic) -> History {
    let mut world = begin(seed, s, sphere, mosaic, None);
    for t in 0..=s.steps {
        advance(&mut world, t);
    }
    finish(world)
}

/// The history of a written scene (the bench): the model's own steps on
/// continents and plates laid by hand.
pub fn run_scene(seed: u64, s: &Settings, sphere: &Sphere, scene: &Scene) -> History {
    let (_, mosaic) = Mosaic::draw(seed, sphere);
    let mut world = begin(seed, s, sphere, &mosaic, Some(scene));
    world.impose();
    for t in 0..=s.steps {
        advance(&mut world, t);
        if t + 1 < scene.free_from {
            world.impose();
        }
    }
    finish(world)
}

/// One step of the history: the mechanisms, in the order they run. Step
/// 0 only reads the world as it starts.
pub(crate) fn advance(world: &mut World, t: usize) {
    let s = world.s;
    world.step = Step::new(t, ((s.steps - t) as i64 * s.step_my) as u16);
    if t > 0 {
        world.free_names();
        crust::begin_step(world);
        collisions::stopped(world);
        rifts::troughs(world);
        crust::turn(world);
        collisions::meet(world);
        trenches::sink_and_dock(world);
        trenches::draw_floor(world);
        floor::holes_and_gaps(world);
        plates::specks(world);
        wear::piles_and_silt(world);
        trenches::margins_give_way(world);
        plates::one_piece(world);
        heat::hotspots_mark(world);
        crust::compact(world);
        rifts::margins_sink(world);
    } else {
        for c in &mut world.land.cells {
            c.shown = true;
        }
    }
    forces::look_at(world);
    heat::warm_step(world);
    wear::sea_and_weather(world);
    collisions::progress(world);
    record::write(world);
    forces::next(world);
}
