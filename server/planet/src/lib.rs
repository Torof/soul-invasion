//! Step 0 of the new world (probe, 26 November 2026): the planet's plates
//! and the outlines of its continents, on a coarse grid of fixed size,
//! and our continent shaped at a subduction margin. Nothing reads it yet:
//! it is drawn by `examples/planet.rs` for the project lead to judge.
//!
//! The planet has the Earth's radius. Its grid is a cube projected on the
//! sphere, every cell the same area (six faces of [`FACE`]² cells of
//! about 92 km): a fixed size, whatever detail the layers above it get
//! later.
//!
//! What it tells, in order, each step reading only the ones before:
//!
//! 1. **The plates' history** ([`history`], version 4, the lead's 26
//!    November): plates move through the whole history, driven by the
//!    mantle and by slab pull, making ocean floor at their ridges and
//!    sinking it at their trenches; the supercontinent on them breaks,
//!    its pieces collide and weld, terranes and arcs accrete. The
//!    continents of today are what is left, each piece of crust carrying
//!    its record (piece, age, margins, last mountain building). A world's
//!    character is an input: [`Settings`].
//! 2. **The mosaic** the supercontinent is made of (version 13): old
//!    strong cores welded by wide belts of younger crust (old mountain
//!    belts, failed rifts, shear zones), carried with its crust.
//! 3. **Today's plates**: the plates of the history's last step, turning
//!    as they last turned.
//! 4. **Boundaries.** Each boundary cell is classified by the relative
//!    motion of the two plates: convergent (collision, continental arc,
//!    island arc, or the trench of the plate that sinks), divergent
//!    (ridge in the ocean, rift in a continent), or transform.
//! 5. **Hotspots.** Fixed plumes under moving plates leave chains, older
//!    the farther they run in the plate's direction of motion.
//! 6. **Height and the sea.** A continent's height is carried by its
//!    crust through the history (version 7) and changes only where
//!    something happens to it; the ocean floor's follows its age. The sea
//!    level leaves the settings' share of the surface dry.
//! 7. **Our continent** ([`Site`], [`Zoom`]): a subduction margin at
//!    40–50° north with the ocean to the west is chosen, and a
//!    continent of [`LENGTH_KM`] is shaped on its overriding side, read
//!    across the margin: trench, coast range, forearc basin, volcanic
//!    arc, back-arc, as on the real margins of Cascadia and Chile.
//!
//! Integers only (D24): angles in millionths of a radian, vectors on the
//! unit sphere at length [`ONE`], sines from a Taylor series.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::many_single_char_names,
    clippy::needless_range_loop,
    clippy::cast_possible_wrap,
    reason = "a probe's geometry: values bounded by the grid and the planet; long functions read as one story; short names follow the maths (the lints `sim::plan` allows, inherited when it lived there)"
)]
#![allow(
    clippy::wildcard_imports,
    reason = "the model is one story cut into modules for reading, one mechanism each: they share its names"
)]

use std::cmp::Reverse;
use std::collections::BinaryHeap;

mod hash;
pub mod history;
mod math;
mod mosaic;
mod noise;
mod planet;
mod site;
mod sphere;
mod today;
mod zoom;

pub use self::{math::*, mosaic::*, noise::*, planet::*, site::*, sphere::*, today::*, zoom::*};
use hash::hash;
pub use history::{History, Settings};

/// Draw purposes.
pub(crate) const P_PLATES: u64 = 0x0c50_0010;
pub(crate) const P_OLD: u64 = 0x0c50_0020;
pub(crate) const P_RELIEF: u64 = 0x0c50_0040;
pub(crate) const P_ZOOM: u64 = 0x0c50_0050;

#[cfg(test)]
mod tests;
