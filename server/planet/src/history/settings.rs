//! A world's character, the model's input: every setting with what it
//! changes and its default.

use super::*;

/// A world's character, the model's input: a supercontinent of this
/// size and shape on so many plates, oceanic plates around it, forces of
/// this strength, so long a history. Read from a RON file by the example
/// (`monde-defaut.ron` beside the page lists them, each with what it
/// changes); missing fields take the defaults.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The planet's radius, km. Fixed at the Earth's for now: the grid and
    /// the zoom are measured on it.
    pub radius_km: i64,
    /// The planet's water: the depth it would have spread over the whole
    /// sphere, m (version 16; the Earth's: about 2,600, with two fifths
    /// of its sphere in continental crust against a quarter here). It
    /// never changes: at each step the sea stands at the level at which
    /// this volume fills the basins of the relief as it is
    /// (`wear::level`), and the share of land is what comes out. 3,650:
    /// the supercontinent of the start stands whole out of a sea some
    /// 800 m below its plains; as its pieces part the ocean's floor
    /// grows younger and shallower and the sea rises some 500 m, to
    /// stand today 200 to 250 m below the platform, with about a quarter
    /// of the continents' crust under it (the Earth: a quarter to 30 %)
    /// and a fifth of the sphere dry. 150 m of water more or less moves
    /// the sea by some 130 m.
    pub water_m: i64,
    /// Share of the surface carried by continental crust, per mille: the
    /// supercontinent's size (the land, plus the shelves).
    pub crust_per_mille: i64,
    /// How far the supercontinent's outline wanders from a round cap, per
    /// mille of the default warp (0: a round cap).
    pub supercontinent_warp: i64,
    /// How ragged its coast is, per mille of the default fray.
    pub supercontinent_fray: i64,
    /// How much the supercontinent is squeezed toward the equator, per
    /// mille (1000: a round cap; more: a band, less land at the poles).
    pub supercontinent_flatten: i64,
    /// The supercontinent's middle stays within this latitude, tenths of a
    /// degree (low: it straddles the equator, as Pangaea did).
    pub supercontinent_latitude: i64,
    /// Plates the supercontinent lies on at the start. One since version
    /// 11: plates are born of rifts. With more, it also breaks along
    /// their borders.
    pub continent_plates: (i64, i64),
    /// How unequal those plates are, [`ONE`] units of the largest seed
    /// weight (0: all alike).
    pub plate_inequality: i64,
    /// Oceanic plates at the start, around the supercontinent: they
    /// carry all the ocean's floor, up to the supercontinent's rim.
    pub ocean_plates: (i64, i64),
    /// A plate younger than twice this many steps does not weld.
    pub rift_steps: usize,
    /// Steps of the history.
    pub steps: usize,
    /// Each step's length, millions of years.
    pub step_my: i64,
    /// Plate speeds, mm a year: the least a plate is given today (for its
    /// drawn motion), and a safety ceiling only tiny plates touch (the
    /// mantle's drag, [`Settings::mantle_drag_mm`], is what slows them).
    pub speed_mm: (i64, i64),
    /// The mantle's drag grows with speed: at this speed, mm a year, it is
    /// twice a plain drag (the fast plates slow, no ceiling).
    pub mantle_drag_mm: i64,
    /// A collision's pile pushes back on the two plates that thicken it:
    /// mm a year per side of a cell on its front (a cell of front has
    /// about three), shared by all the plate's cells as
    /// [`Settings::slab_pull`] is, for a pile twice a continent's
    /// thickness (70 km); it grows with the pile
    /// ([`pile_weight`](super::pile_weight)). At 4,000 a front as long
    /// as a trench holds, at 70 km, about one and a half times what the
    /// trench's slab pulls with.
    pub pile_push: i64,
    /// The strength of the lithosphere: the tension a cell of the
    /// strongest crust (old, cold, a continent's thickness; or old ocean
    /// floor) holds across a plate's section, in the forces' units (mm a
    /// year of parting times the drag of a thousand cells of ocean
    /// floor). A rift begins where the tension across a plate is more
    /// than its section holds (`rifts::holds`); weaker crust holds less
    /// (a warm young belt a tenth of this).
    pub crust_holds: i64,
    /// The mantle's heat moves the continents: mm a year down a slope of
    /// the heat field as steep as under a supercontinent's edge (hot
    /// under a large continent, cold where slabs sink).
    pub heat_push: i64,
    /// Slab pull, the strongest force: mm a year per trench cell, shared
    /// by all the plate's cells (a plate's speed is about this times its
    /// trench cells over its cells), for a floor of
    /// [`Settings::slab_age_my`].
    pub slab_pull: i64,
    /// The floor's age, millions of years, at which a slab pulls at full
    /// strength: an older one pulls more (up to half as much again), a
    /// younger one less, one under [`SLAB_YOUNG_MY`] not at all.
    pub slab_age_my: i64,
    /// Trench suction: the sinking slab draws the plate riding over it
    /// toward the trench, mm a year per trench cell (as `slab_pull`). It
    /// is the pull a plate all continent has.
    pub slab_suction: i64,
    /// Ridge push, mm a year per ridge cell, shared the same way.
    pub ridge_push: i64,
    /// How much a continental keel slows its plate, per mille of drag
    /// added for a plate all continent.
    pub continent_drag: i64,
    /// A push on continents toward the equator, mm a year at 45° (the
    /// pole-fleeing force, « Polflucht »). 0 since version 8: the real
    /// force is far too weak to move a plate, and nothing else sorts
    /// continents by latitude.
    pub pole_flight: i64,
    /// Terranes waiting in the ocean to be accreted.
    pub terranes: (i64, i64),
    /// A terrane's length, cells of ~100 km.
    pub terrane_cells: (i64, i64),
    /// Hotspots: plumes rise where the mantle is hottest.
    pub hotspots: (i64, i64),
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            radius_km: RADIUS_KM,
            water_m: 3_650,
            crust_per_mille: 240,
            supercontinent_warp: 1000,
            supercontinent_fray: 1000,
            supercontinent_latitude: 150,
            supercontinent_flatten: 2200,
            continent_plates: (1, 1),
            plate_inequality: 100_000,
            ocean_plates: (3, 5),
            rift_steps: 4,
            steps: 40,
            step_my: 5,
            speed_mm: (10, 250),
            mantle_drag_mm: 150,
            pile_push: 4_000,
            crust_holds: 300,
            heat_push: 150,
            slab_pull: 7_500,
            slab_age_my: 80,
            slab_suction: 2_500,
            ridge_push: 300,
            continent_drag: 800,
            pole_flight: 0,
            terranes: (18, 28),
            terrane_cells: (1, 5),
            hotspots: (14, 20),
        }
    }
}
