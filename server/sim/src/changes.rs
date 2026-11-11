//! What men changed in a chunk (lightness plan D2, 11 November 2026;
//! D28): everything else in it is placed again from the map and the seed,
//! the same every time, and never stored. A chunk no man touched has no
//! changes at all, so what is kept grows with what men do, never with the
//! land.
//!
//! Each chunk's changes carry its **version**: how many times men changed
//! it. The world's checkpoint writes only the chunks whose version moved
//! since the last one, and a client will compare it (D4) to know whether
//! the chunk it holds is still the one the server has.
//!
//! Today only the natural items taken off the ground are changes the
//! simulation makes. The shape is ready for felled trees, planted trees
//! and constructions: they are stored and read back, but nothing in the
//! simulation makes them yet.

use std::collections::{BTreeMap, BTreeSet};

use crate::ground::Version;
use crate::map::Pos;
use crate::trees::TreeKey;

/// The changes of one chunk. See the module's doc.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChunkChanges {
    /// How many times men changed the chunk.
    pub version: Version,
    /// The natural items taken off its ground, by their slot in the chunk
    /// ([`crate::items::slot_of`]): they do not come back.
    pub taken: BTreeSet<u16>,
    /// The trees of the seed felled, by their placement cell: their
    /// stumps stand where they stood. A felled forest never grows back by
    /// itself.
    pub felled: BTreeSet<TreeKey>,
    /// The trees men planted, by the placement cell they were planted on.
    pub planted: BTreeMap<TreeKey, Planted>,
    /// What men built in the chunk.
    pub built: Vec<Construction>,
}

/// A tree men planted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planted {
    /// Its kind: the content's id of the species (`oak`), so a change in
    /// the order of the content's kinds changes nothing.
    pub species: String,
    /// How far it has grown.
    pub stage: Stage,
    /// The game day it was planted.
    pub day: u64,
}

/// How far a planted tree has grown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// Just out of the ground.
    Seedling,
    /// A young tree.
    Young,
    /// Half grown.
    Medium,
    /// Grown.
    Adult,
}

impl Stage {
    /// Every stage, from the first.
    pub const ALL: [Self; 4] = [Self::Seedling, Self::Young, Self::Medium, Self::Adult];
}

/// Something men built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Construction {
    /// Its kind: a content id.
    pub kind: String,
    /// The tile it stands on, in the chunk.
    pub at: Pos,
    /// The game day it was built.
    pub day: u64,
}
