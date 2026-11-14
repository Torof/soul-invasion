//! The open ground inhabited: how many trees stand alone and in groves on
//! the open biomes of the atlas (`content/open_ground.ron`, D27; D37, the
//! project lead's verdict of 15 November 2026 on the vivarium: "huge
//! plains have no plant and no tree").
//!
//! The atlas gives a whole 2 km cell one biome, and its grassland,
//! steppe or marsh is open ground: the woods of the valley grow only
//! where the biome is a forest. A real prairie is not bare: a few trees
//! stand alone on every hectare, and small groves of a handful of trees
//! dot it. Each open biome says how many of each a square kilometre, and
//! how large a grove is; the trees themselves are placed by the meadows'
//! grid ([`crate::trees`]), one group at most every 24 m, in the kinds of
//! the lone trees of the region's stands. A forest biome has no entry:
//! its meadows are the valley's own, with their copses and groups by the
//! woods. Placeholder values, to be tuned by looking at the zone.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::plan::Biome;

/// How an open biome is inhabited by trees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Open {
    /// Trees standing alone, a square kilometre.
    pub lone_per_km2: u32,
    /// Groves (small groups of trees), a square kilometre.
    pub groves_per_km2: u32,
    /// A grove's trees, at least and at most (1 to 8: a group's trees
    /// are told apart by a rank of three bits, [`crate::trees::TreeKey`]).
    pub grove: (u8, u8),
}

/// The open biomes' trees: `content/open_ground.ron` (D27). A biome with
/// no entry has no tree of its own on its open ground.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGround {
    /// Each open biome's trees, by the biome's name in `snake_case`
    /// (`grassland`, `wooded_steppe`, `steppe`, `marsh`, `desert`,
    /// `tundra`, `alpine`).
    pub biomes: BTreeMap<String, Open>,
}

/// The most trees and groves an open biome may hold together, a square
/// kilometre: the meadows' grid holds one group at most every 24 m
/// (1,736 cells a square kilometre), and the odds past this leave the
/// grid full.
pub const MOST_PER_KM2: u32 = 1_500;

/// The largest grove: a group's rank has three bits.
pub const LARGEST_GROVE: u8 = 8;

/// The placeholder values of `content/open_ground.ron`, for content made
/// without the file (the tests' sample).
impl Default for OpenGround {
    fn default() -> Self {
        let open = |lone_per_km2, groves_per_km2, grove| Open {
            lone_per_km2,
            groves_per_km2,
            grove,
        };
        Self {
            biomes: BTreeMap::from([
                ("grassland".to_owned(), open(200, 50, (3, 8))),
                ("wooded_steppe".to_owned(), open(150, 120, (4, 8))),
                ("steppe".to_owned(), open(30, 10, (3, 5))),
                ("marsh".to_owned(), open(100, 30, (3, 6))),
            ]),
        }
    }
}

impl OpenGround {
    /// What is wrong with these values, if anything.
    ///
    /// # Errors
    ///
    /// A name that is no biome's or a forest's, a grove of no tree or of
    /// more than [`LARGEST_GROVE`], a lower bound above the upper, or more
    /// than [`MOST_PER_KM2`] trees and groves together.
    pub fn check(&self) -> Result<(), &'static str> {
        for (name, open) in &self.biomes {
            if Self::biome(name).is_none() {
                return Err("a biome's name in snake_case (grassland, wooded_steppe, ...)");
            }
            if Self::biome(name).is_some_and(Biome::is_forest) {
                return Err("a forest biome has the valley's own meadows, no entry here");
            }
            let (lo, hi) = open.grove;
            if lo == 0 || lo > hi || hi > LARGEST_GROVE {
                return Err("grove must be (at least, at most) within 1 to 8");
            }
            if open.lone_per_km2.saturating_add(open.groves_per_km2) > MOST_PER_KM2 {
                return Err("lone_per_km2 and groves_per_km2 must sum to 1,500 at most");
            }
        }
        Ok(())
    }

    /// How `biome`'s open ground is inhabited; `None` for a biome with no
    /// entry (the forests, the sea, a lake, or one left bare).
    #[must_use]
    pub fn of(&self, biome: Biome) -> Option<Open> {
        self.biomes.get(Self::name(biome)).copied()
    }

    /// A biome's name in `snake_case`.
    fn name(biome: Biome) -> &'static str {
        match biome {
            Biome::Sea => "sea",
            Biome::Lake => "lake",
            Biome::Desert => "desert",
            Biome::Steppe => "steppe",
            Biome::WoodedSteppe => "wooded_steppe",
            Biome::Grassland => "grassland",
            Biome::Broadleaf => "broadleaf",
            Biome::Jungle => "jungle",
            Biome::MixedForest => "mixed_forest",
            Biome::Conifer => "conifer",
            Biome::Marsh => "marsh",
            Biome::Tundra => "tundra",
            Biome::Alpine => "alpine",
        }
    }

    /// The biome of that name, if any.
    fn biome(name: &str) -> Option<Biome> {
        [
            Biome::Sea,
            Biome::Lake,
            Biome::Desert,
            Biome::Steppe,
            Biome::WoodedSteppe,
            Biome::Grassland,
            Biome::Broadleaf,
            Biome::Jungle,
            Biome::MixedForest,
            Biome::Conifer,
            Biome::Marsh,
            Biome::Tundra,
            Biome::Alpine,
        ]
        .into_iter()
        .find(|b| Self::name(*b) == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_placeholders_are_valid_and_name_the_open_biomes() {
        let open = OpenGround::default();
        assert_eq!(open.check(), Ok(()));
        assert!(open.of(Biome::Grassland).is_some());
        assert!(open.of(Biome::Broadleaf).is_none());
        assert!(open.of(Biome::Desert).is_none());
        let grass = open.of(Biome::Grassland).unwrap();
        assert!(grass.lone_per_km2 > 0 && grass.groves_per_km2 > 0);
    }

    #[test]
    fn senseless_values_are_refused() {
        let with = |name: &str, open: Open| OpenGround {
            biomes: BTreeMap::from([(name.to_owned(), open)]),
        };
        let fine = Open {
            lone_per_km2: 10,
            groves_per_km2: 10,
            grove: (3, 8),
        };
        assert!(with("prairie", fine).check().is_err());
        assert!(with("broadleaf", fine).check().is_err());
        assert!(
            with(
                "steppe",
                Open {
                    grove: (0, 3),
                    ..fine
                }
            )
            .check()
            .is_err()
        );
        assert!(
            with(
                "steppe",
                Open {
                    grove: (5, 3),
                    ..fine
                }
            )
            .check()
            .is_err()
        );
        assert!(
            with(
                "steppe",
                Open {
                    grove: (3, 9),
                    ..fine
                }
            )
            .check()
            .is_err()
        );
        assert!(
            with(
                "steppe",
                Open {
                    lone_per_km2: 1_400,
                    groves_per_km2: 200,
                    ..fine
                }
            )
            .check()
            .is_err()
        );
        assert_eq!(with("tundra", fine).check(), Ok(()));
    }

    #[test]
    fn the_repository_file_parses_to_the_placeholders() {
        let file: OpenGround =
            ron::from_str(include_str!("../../../content/open_ground.ron")).expect("parses");
        assert_eq!(file.check(), Ok(()));
        assert_eq!(
            file,
            OpenGround::default(),
            "the file and the placeholders agree"
        );
    }
}
