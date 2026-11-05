//! World content: what things are made of, and where they lie (D15, D27).
//!
//! Content is data, never code: materials with properties, and spawn
//! rules that scatter them over the ground. Qualities of objects will be
//! computed from these properties, never from hard-coded recipes. The
//! server reads the RON files and hands their text to [`Content::from_ron`];
//! this crate never touches a file.
//!
//! Values in the public repository are placeholders, tuned by play; hidden
//! rules live in a private repository (D30).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::map::Terrain;

/// What family a material belongs to. Guides what can be done with it;
/// the properties decide how well.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialKind {
    /// Stone: knapped, hammered, ground.
    Stone,
    /// Wood: cut, split, burnt.
    Wood,
    /// Plant matter: fibres, leaves, berries.
    Plant,
    /// From an animal: hide, bone, meat.
    Animal,
}

/// What eating a material gives, when it is food.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Food {
    /// Energy of one piece, in kilojoules.
    pub energy_kj: u32,
    /// Days before it spoils, untreated (D16: computed on demand).
    pub keeps_days: u32,
}

/// How a material looks, for the client. Placeholder graphics: a colour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Look {
    /// `#rrggbb`.
    pub colour: String,
}

/// A material: the stuff pieces are made of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    /// Content id, `snake_case`, unique.
    pub id: String,
    /// Name shown to players.
    pub name: String,
    /// Family.
    pub kind: MaterialKind,
    /// Mass of one piece, in kilograms.
    pub mass_kg: f32,
    /// Hardness, 0 (soft) to 10 (hardest).
    pub hardness: u8,
    /// Does it burn?
    pub flammable: bool,
    /// Can it be eaten?
    pub food: Option<Food>,
    /// How it looks.
    pub look: Look,
}

/// Where a material lies on the ground when the world is created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spawn {
    /// The material.
    pub material: String,
    /// On which terrain.
    pub terrain: Terrain,
    /// Pieces per thousand tiles of that terrain.
    pub per_thousand: u32,
}

/// Everything loaded from the content files, validated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Content {
    /// Materials, in file order.
    pub materials: Vec<Material>,
    /// Spawn rules, in file order.
    pub spawns: Vec<Spawn>,
}

/// Why content was refused.
#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    /// A file does not parse.
    #[error("{file}: {source}")]
    Parse {
        /// Which file.
        file: &'static str,
        /// The parser's error, with its position.
        #[source]
        source: ron::error::SpannedError,
    },
    /// Two materials share an id.
    #[error("material `{0}` is defined twice")]
    DuplicateMaterial(String),
    /// A rule names a material that does not exist.
    #[error("spawn rule {index} names unknown material `{material}`")]
    UnknownMaterial {
        /// Position of the rule in the file, from 0.
        index: usize,
        /// The id it used.
        material: String,
    },
    /// A value is outside what makes sense.
    #[error("material `{id}`: {what}")]
    InvalidValue {
        /// Which material.
        id: String,
        /// What is wrong.
        what: &'static str,
    },
    /// A spawn density above one piece per tile.
    #[error("spawn rule {index}: per_thousand {per_thousand} is above 1000")]
    TooDense {
        /// Position of the rule in the file, from 0.
        index: usize,
        /// The value.
        per_thousand: u32,
    },
}

impl Content {
    /// Parses the materials and spawn files and validates them together.
    pub fn from_ron(materials: &str, spawns: &str) -> Result<Self, ContentError> {
        let materials = ron::from_str(materials).map_err(|source| ContentError::Parse {
            file: "materials.ron",
            source,
        })?;
        let spawns = ron::from_str(spawns).map_err(|source| ContentError::Parse {
            file: "spawns.ron",
            source,
        })?;
        let content = Self { materials, spawns };
        content.validate()?;
        Ok(content)
    }

    /// Checks ids are unique, references resolve and values make sense.
    pub fn validate(&self) -> Result<(), ContentError> {
        let mut ids = BTreeSet::new();
        for material in &self.materials {
            if !ids.insert(material.id.as_str()) {
                return Err(ContentError::DuplicateMaterial(material.id.clone()));
            }
            let invalid = |what| ContentError::InvalidValue {
                id: material.id.clone(),
                what,
            };
            if material.id.is_empty()
                || !material
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                return Err(invalid("id must be snake_case ASCII"));
            }
            if material.name.trim().is_empty() {
                return Err(invalid("name is empty"));
            }
            if !(material.mass_kg.is_finite() && material.mass_kg > 0.0) {
                return Err(invalid("mass_kg must be positive"));
            }
            if material.hardness > 10 {
                return Err(invalid("hardness is 0 to 10"));
            }
            if !is_hex_colour(&material.look.colour) {
                return Err(invalid("look.colour must be #rrggbb"));
            }
        }
        for (index, spawn) in self.spawns.iter().enumerate() {
            if !ids.contains(spawn.material.as_str()) {
                return Err(ContentError::UnknownMaterial {
                    index,
                    material: spawn.material.clone(),
                });
            }
            if spawn.per_thousand > 1000 {
                return Err(ContentError::TooDense {
                    index,
                    per_thousand: spawn.per_thousand,
                });
            }
        }
        Ok(())
    }

    /// A material by id.
    pub fn material(&self, id: &str) -> Option<&Material> {
        self.materials.iter().find(|m| m.id == id)
    }

    /// A small, valid content for tests: a stone, a branch and a berry.
    /// The real valley's content lives in the `content/` directory.
    pub fn sample() -> Self {
        Self::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS).expect("sample content is valid")
    }
}

fn is_hex_colour(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Materials of [`Content::sample`].
pub const SAMPLE_MATERIALS: &str = r##"[
    (id: "flint", name: "Flint", kind: stone, mass_kg: 0.4, hardness: 7, flammable: false, food: None, look: (colour: "#6e6a62")),
    (id: "branch", name: "Branch", kind: wood, mass_kg: 1.5, hardness: 3, flammable: true, food: None, look: (colour: "#7a5a3a")),
    (id: "berries", name: "Berries", kind: plant, mass_kg: 0.1, hardness: 0, flammable: false, food: Some((energy_kj: 250, keeps_days: 3)), look: (colour: "#8a2a5a")),
]"##;

/// Spawn rules of [`Content::sample`].
pub const SAMPLE_SPAWNS: &str = r#"[
    (material: "branch", terrain: forest, per_thousand: 60),
    (material: "berries", terrain: grass, per_thousand: 10),
]"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_content_parses_and_validates() {
        let content = Content::sample();
        assert_eq!(content.materials.len(), 3);
        assert_eq!(content.spawns.len(), 2);
        let berries = content.material("berries").unwrap();
        assert_eq!(berries.kind, MaterialKind::Plant);
        assert_eq!(berries.food.map(|f| f.keeps_days), Some(3));
        assert!(content.material("gold").is_none());
    }

    #[test]
    fn a_parse_error_names_the_file() {
        let err = Content::from_ron("[(id: oops", SAMPLE_SPAWNS).unwrap_err();
        assert!(matches!(
            err,
            ContentError::Parse {
                file: "materials.ron",
                ..
            }
        ));
        let err = Content::from_ron(SAMPLE_MATERIALS, "nope").unwrap_err();
        assert!(matches!(
            err,
            ContentError::Parse {
                file: "spawns.ron",
                ..
            }
        ));
    }

    #[test]
    fn duplicate_and_unknown_ids_are_refused() {
        let twice = format!(
            "[{0}, {0}]",
            r##"(id: "flint", name: "Flint", kind: stone, mass_kg: 1, hardness: 7, flammable: false, food: None, look: (colour: "#000000"))"##
        );
        assert!(matches!(
            Content::from_ron(&twice, "[]").unwrap_err(),
            ContentError::DuplicateMaterial(id) if id == "flint"
        ));
        let err = Content::from_ron(
            SAMPLE_MATERIALS,
            r#"[(material: "gold", terrain: grass, per_thousand: 1)]"#,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContentError::UnknownMaterial { index: 0, material } if material == "gold"
        ));
    }

    #[test]
    fn values_outside_sense_are_refused() {
        let bad = |field: &str| {
            let text = SAMPLE_MATERIALS.replacen(field, "", 1);
            Content::from_ron(&text, "[]").unwrap_err()
        };
        // Removing "flammable: false, " makes the struct incomplete: a parse error.
        assert!(matches!(
            bad("flammable: false, "),
            ContentError::Parse { .. }
        ));

        let heavy = SAMPLE_MATERIALS.replacen("mass_kg: 0.4", "mass_kg: 0", 1);
        assert!(matches!(
            Content::from_ron(&heavy, "[]").unwrap_err(),
            ContentError::InvalidValue {
                what: "mass_kg must be positive",
                ..
            }
        ));
        let hard = SAMPLE_MATERIALS.replacen("hardness: 7", "hardness: 11", 1);
        assert!(matches!(
            Content::from_ron(&hard, "[]").unwrap_err(),
            ContentError::InvalidValue {
                what: "hardness is 0 to 10",
                ..
            }
        ));
        let colour = SAMPLE_MATERIALS.replacen("#6e6a62", "grey", 1);
        assert!(matches!(
            Content::from_ron(&colour, "[]").unwrap_err(),
            ContentError::InvalidValue {
                what: "look.colour must be #rrggbb",
                ..
            }
        ));
        let shouting = SAMPLE_MATERIALS.replacen("id: \"flint\"", "id: \"Flint\"", 1);
        assert!(matches!(
            Content::from_ron(&shouting, "[]").unwrap_err(),
            ContentError::InvalidValue {
                what: "id must be snake_case ASCII",
                ..
            }
        ));
        let dense = SAMPLE_SPAWNS.replacen("per_thousand: 60", "per_thousand: 1001", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, &dense).unwrap_err(),
            ContentError::TooDense {
                index: 0,
                per_thousand: 1001
            }
        ));
    }
}
