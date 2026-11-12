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

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::map::Terrain;
use crate::woods::Habitat;

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

impl Material {
    /// Mass of one piece, in grams.
    // Not generation: content gives masses in kilograms, read once.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::float_arithmetic
    )]
    pub fn mass_g(&self) -> u32 {
        (self.mass_kg * 1000.0).round().clamp(0.0, 1e9) as u32
    }
}

/// A material: the stuff pieces are made of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    /// Content id, `snake_case`, unique.
    pub id: String,
    /// Name shown to players.
    pub name: String,
    /// One sentence shown to players, what it is and what it is for.
    pub description: String,
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

/// A closeness condition: within so many tiles of some terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Near {
    /// The terrain to be close to.
    pub terrain: Terrain,
    /// How many tiles away at most (eight-way steps).
    pub within: u32,
}

/// Where a material lies on the ground when the world is created: on a
/// terrain, at a density, and perhaps only near another terrain — flint
/// at the foot of the cliff, berries at the forest's edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spawn {
    /// The material.
    pub material: String,
    /// On which terrain.
    pub terrain: Terrain,
    /// Pieces per thousand tiles of that terrain (that satisfy `near`).
    pub per_thousand: u32,
    /// Only near this terrain, if given.
    #[serde(default)]
    pub near: Option<Near>,
}

/// How bodies move over the ground (D32).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Movement {
    /// Walking speed on the reference terrain (cost 10), in centimetres
    /// per game second.
    pub walk_speed_cm_per_s: u32,
    /// Cost of crossing one tile of each terrain, 10 being the reference;
    /// `None` cannot be walked. Paths weigh tiles by it and bodies slow
    /// down by it.
    pub terrain_costs: BTreeMap<Terrain, Option<u32>>,
    /// What a body can carry, in grams (D35).
    pub carry_capacity_g: u32,
    /// Walking speed at full load, in percent of the unloaded speed.
    pub speed_percent_at_full_load: u32,
    /// How far a body sees, by the terrain it stands on, in tiles on each
    /// axis: a path is searched within that square and no farther (D32).
    pub sight_tiles: BTreeMap<Terrain, u32>,
    /// Tiles a path search may expand at most; past it, the body walks
    /// straight (D32).
    pub path_budget_tiles: u32,
    /// Tiles the path searches of one tick may expand in all; the rest
    /// wait for the next tick, in their lane, in the order they came. The
    /// search at the head of a lane runs even past it, the next tick
    /// repaying the excess (D32).
    pub search_work_per_tick: u32,
    /// The share of that work kept for bodies with a pilot (players), in
    /// percent; the rest goes to the others (NPCs, animals). A lane's
    /// unused share goes to the other (D32).
    pub player_share_percent: u32,
    /// Tiles a body skirting an obstacle whose end it does not see may
    /// expand in one look for the best point on the edge of its sight
    /// (D32).
    pub skirt_budget_tiles: u32,
    /// Looks a body may take skirting obstacles for one order before it
    /// gives up and says it is blocked (D32).
    pub skirt_looks: u32,
}

impl Movement {
    /// Cost of a tile of this terrain, `None` when it cannot be walked.
    pub fn cost(&self, terrain: Terrain) -> Option<u32> {
        self.terrain_costs.get(&terrain).copied().flatten()
    }

    /// Walking speed on this terrain, in centimetres per game second;
    /// zero when it cannot be walked.
    pub fn speed_cm_per_s(&self, terrain: Terrain) -> u32 {
        self.cost(terrain)
            .map_or(0, |cost| self.walk_speed_cm_per_s * 10 / cost)
    }

    /// How far a body standing on this terrain sees, in tiles on each
    /// axis; the shortest sight there is for a terrain the content does
    /// not name.
    pub fn sight_tiles(&self, terrain: Terrain) -> u32 {
        self.sight_tiles
            .get(&terrain)
            .copied()
            .unwrap_or_else(|| self.sight_tiles.values().copied().min().unwrap_or(1))
    }

    /// The slowest walking speed on any terrain that can be walked, in
    /// centimetres per game second.
    pub fn slowest_speed_cm_per_s(&self) -> u32 {
        Terrain::ALL
            .iter()
            .map(|&t| self.speed_cm_per_s(t))
            .filter(|&speed| speed > 0)
            .min()
            .unwrap_or(self.walk_speed_cm_per_s)
    }
}

/// How a body's needs and vitality evolve (D34). Every rate is in points
/// of a 0–100 gauge per game day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Needs {
    /// Where night begins, in percent of the day; the day starts at dawn.
    pub night_from_percent: u32,
    /// Hunger rises this much per day.
    pub hunger_per_day: u32,
    /// Cold rises this much per day of night, without warmth.
    pub cold_per_day_at_night: u32,
    /// Cold falls this much per day of daylight.
    pub cold_relief_per_day: u32,
    /// Fatigue rises this much per day awake.
    pub fatigue_per_day_awake: u32,
    /// Fatigue falls this much per day asleep.
    pub rest_per_day_asleep: u32,
    /// Vitality lost per day while hunger is at its maximum.
    pub starving_per_day: u32,
    /// Vitality lost per day while cold is at its maximum.
    pub freezing_per_day: u32,
    /// Vitality regained per day when fed, warm enough and rested.
    pub vitality_regain_per_day: u32,
    /// Kilojoules of food that relieve one point of hunger.
    pub kj_per_hunger_point: u32,
}

/// Everything loaded from the content files, validated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Content {
    /// Materials, in file order.
    pub materials: Vec<Material>,
    /// Spawn rules, in file order.
    pub spawns: Vec<Spawn>,
    /// How bodies move.
    pub movement: Movement,
    /// How bodies hunger, freeze, tire and die.
    pub needs: Needs,
    /// The kinds of tree and where each grows (D37), in file order.
    pub species: Vec<Species>,
    /// The shrubs, plants and mushrooms, and the trees still to come
    /// (`flora.ron`), in file order by section.
    #[serde(default)]
    pub flora: Vec<crate::flora::Kind>,
}

/// A kind of tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Species {
    /// Content id, `snake_case`, unique.
    pub id: String,
    /// Name shown to players.
    pub name: String,
    /// Height of a grown tree, in metres.
    pub height_m: f32,
    /// How much this species grows in each habitat, against the others
    /// there; habitats not listed have none of it.
    pub habitats: BTreeMap<Habitat, u32>,
    /// A big tree, standing on the woods' wide grid (8 m) with nothing
    /// small right under its crown; the others stand on the narrow one
    /// (4 m), between the big ones (D40).
    #[serde(default)]
    pub big: bool,
    /// How far its seeds fall from the tree that bears them, in metres
    /// (4 to [`MOST_SPREAD_M`]): heavy seeds (acorns, hazelnuts) close by,
    /// in tight groups; light ones on the wind (birch, pine) farther, in
    /// loose ones (D37).
    #[serde(default = "Species::default_spread")]
    pub spread_m: u32,
    /// A pioneer: the first to grow back in the openings of the woods.
    #[serde(default)]
    pub pioneer: bool,
}

/// Farthest a species' seeds may fall ([`Species::spread_m`]): the trees
/// look for the trees that seeded them this far around, no farther.
pub const MOST_SPREAD_M: u32 = 36;

impl Species {
    fn default_spread() -> u32 {
        20
    }
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
    /// A kind of tree is defined twice or makes no sense.
    #[error("species `{id}`: {what}")]
    Species {
        /// Which species.
        id: String,
        /// What is wrong.
        what: &'static str,
    },
    /// The movement rules are incomplete or senseless.
    #[error("movement: {0}")]
    Movement(&'static str),
    /// The needs rules are senseless.
    #[error("needs: {0}")]
    Needs(&'static str),
    /// A spawn rule is senseless.
    #[error("spawn rule {index}: {what}")]
    Spawn {
        /// Position of the rule in the file, from 0.
        index: usize,
        /// What is wrong.
        what: &'static str,
    },
}

impl Content {
    /// Parses the content files and validates them together.
    pub fn from_ron(
        materials: &str,
        spawns: &str,
        movement: &str,
        needs: &str,
    ) -> Result<Self, ContentError> {
        let materials = ron::from_str(materials).map_err(|source| ContentError::Parse {
            file: "materials.ron",
            source,
        })?;
        let spawns = ron::from_str(spawns).map_err(|source| ContentError::Parse {
            file: "spawns.ron",
            source,
        })?;
        let movement = ron::from_str(movement).map_err(|source| ContentError::Parse {
            file: "movement.ron",
            source,
        })?;
        let needs = ron::from_str(needs).map_err(|source| ContentError::Parse {
            file: "needs.ron",
            source,
        })?;
        let content = Self {
            materials,
            spawns,
            movement,
            needs,
            species: Vec::new(),
            flora: Vec::new(),
        };
        content.validate()?;
        Ok(content)
    }

    /// The same content with the kinds of tree of `species` (the text of
    /// `species.ron`).
    pub fn with_species(mut self, species: &str) -> Result<Self, ContentError> {
        self.species = ron::from_str(species).map_err(|source| ContentError::Parse {
            file: "species.ron",
            source,
        })?;
        self.validate()?;
        Ok(self)
    }

    /// The same content with the flora of `flora` (the text of
    /// `flora.ron`).
    pub fn with_flora(mut self, flora: &str) -> Result<Self, ContentError> {
        self.flora = crate::flora::parse(flora).map_err(|source| ContentError::Parse {
            file: "flora.ron",
            source,
        })?;
        self.validate()?;
        Ok(self)
    }

    /// Checks the flora: unique ids (among the trees too), and each kind
    /// sensible (`flora::check`).
    fn validate_flora(&self) -> Result<(), ContentError> {
        // A host may be a tree of the game or one still to come.
        let trees: Vec<&str> = self
            .species
            .iter()
            .map(|s| s.id.as_str())
            .chain(
                self.flora
                    .iter()
                    .filter(|k| k.family == crate::flora::Family::Tree)
                    .map(|k| k.id.as_str()),
            )
            .collect();
        let mut ids: BTreeSet<&str> = self.species.iter().map(|s| s.id.as_str()).collect();
        for kind in &self.flora {
            let invalid = |what| ContentError::Species {
                id: kind.id.clone(),
                what,
            };
            if !ids.insert(kind.id.as_str()) {
                return Err(invalid("defined twice (in flora.ron or species.ron)"));
            }
            if let Some(what) = crate::flora::check(kind, &trees) {
                return Err(invalid(what));
            }
        }
        Ok(())
    }

    /// Checks the kinds of tree: unique ids, a name, a sensible height,
    /// and somewhere to grow.
    fn validate_species(&self) -> Result<(), ContentError> {
        let mut species_ids = BTreeSet::new();
        for species in &self.species {
            let invalid = |what| ContentError::Species {
                id: species.id.clone(),
                what,
            };
            if !species_ids.insert(species.id.as_str()) {
                return Err(invalid("defined twice"));
            }
            if species.id.is_empty()
                || !species
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                return Err(invalid("id must be snake_case ASCII"));
            }
            if species.name.trim().is_empty() {
                return Err(invalid("name is empty"));
            }
            if !(species.height_m.is_finite() && (0.5..=40.0).contains(&species.height_m)) {
                return Err(invalid("height_m is 0.5 to 40"));
            }
            if species.habitats.values().all(|w| *w == 0) {
                return Err(invalid("grows nowhere"));
            }
            if !(4..=MOST_SPREAD_M).contains(&species.spread_m) {
                return Err(invalid("spread_m is 4 to 36"));
            }
        }
        Ok(())
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
            if material.description.trim().is_empty() {
                return Err(invalid("description is empty"));
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
        self.validate_species()?;
        self.validate_flora()?;
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
            if spawn.near.is_some_and(|near| near.within == 0) {
                return Err(ContentError::Spawn {
                    index,
                    what: "near.within is 1 or more",
                });
            }
            // A natural item's id holds its rule's index (crate::items).
            if index >= crate::items::MOST_RULES {
                return Err(ContentError::Spawn {
                    index,
                    what: "at most 64 spawn rules",
                });
            }
            if spawn.near.is_some_and(|near| near.within > 16) {
                return Err(ContentError::Spawn {
                    index,
                    what: "near.within is 16 at most (a chunk looks that far round)",
                });
            }
        }
        self.validate_movement()?;
        if !(1..100).contains(&self.needs.night_from_percent) {
            return Err(ContentError::Needs("night_from_percent is 1 to 99"));
        }
        if self.needs.kj_per_hunger_point == 0 {
            return Err(ContentError::Needs("kj_per_hunger_point must be positive"));
        }
        Ok(())
    }

    /// Checks the movement rules (D32).
    fn validate_movement(&self) -> Result<(), ContentError> {
        if self.movement.walk_speed_cm_per_s == 0 {
            return Err(ContentError::Movement(
                "walk_speed_cm_per_s must be positive",
            ));
        }
        for terrain in Terrain::ALL {
            match self.movement.terrain_costs.get(&terrain) {
                None => return Err(ContentError::Movement("every terrain needs a cost")),
                Some(Some(0)) => return Err(ContentError::Movement("a cost is 1 or more")),
                Some(_) => {}
            }
        }
        if self.movement.carry_capacity_g == 0 {
            return Err(ContentError::Movement("carry_capacity_g must be positive"));
        }
        if !(1..=100).contains(&self.movement.speed_percent_at_full_load) {
            return Err(ContentError::Movement(
                "speed_percent_at_full_load is 1 to 100",
            ));
        }
        for terrain in Terrain::ALL {
            match self.movement.sight_tiles.get(&terrain) {
                None => return Err(ContentError::Movement("every terrain needs a sight")),
                Some(0) => return Err(ContentError::Movement("a sight is 1 tile or more")),
                Some(_) => {}
            }
        }
        if self.movement.path_budget_tiles == 0 {
            return Err(ContentError::Movement("path_budget_tiles must be positive"));
        }
        if self.movement.search_work_per_tick == 0 {
            return Err(ContentError::Movement(
                "search_work_per_tick must be positive",
            ));
        }
        if self.movement.player_share_percent > 100 {
            return Err(ContentError::Movement("player_share_percent is 0 to 100"));
        }
        if self.movement.skirt_budget_tiles == 0 {
            return Err(ContentError::Movement(
                "skirt_budget_tiles must be positive",
            ));
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
        Self::from_ron(
            SAMPLE_MATERIALS,
            SAMPLE_SPAWNS,
            SAMPLE_MOVEMENT,
            SAMPLE_NEEDS,
        )
        .expect("sample content is valid")
    }
}

fn is_hex_colour(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Materials of [`Content::sample`].
pub const SAMPLE_MATERIALS: &str = r##"[
    (id: "flint", name: "Flint", description: "A hard stone that flakes to a sharp edge.", kind: stone, mass_kg: 0.4, hardness: 7, flammable: false, food: None, look: (colour: "#6e6a62")),
    (id: "branch", name: "Branch", description: "Dead wood, dry enough to burn.", kind: wood, mass_kg: 1.5, hardness: 3, flammable: true, food: None, look: (colour: "#7a5a3a")),
    (id: "berries", name: "Berries", description: "A handful of ripe berries.", kind: plant, mass_kg: 0.1, hardness: 0, flammable: false, food: Some((energy_kj: 250, keeps_days: 3)), look: (colour: "#8a2a5a")),
]"##;

/// Movement rules of [`Content::sample`]: the valley's own.
pub const SAMPLE_MOVEMENT: &str = r"(
    walk_speed_cm_per_s: 143,
    terrain_costs: { grass: Some(10), forest: Some(15), ford: Some(20), brook: Some(15), water: None, rock: None },
    carry_capacity_g: 20000,
    speed_percent_at_full_load: 60,
    sight_tiles: { grass: 60, forest: 25, ford: 60, brook: 60, water: 60, rock: 60 },
    path_budget_tiles: 1500,
    search_work_per_tick: 60000,
    player_share_percent: 50,
    skirt_budget_tiles: 15000,
    skirt_looks: 24,
)";

/// Needs rules of [`Content::sample`]: the valley's own.
pub const SAMPLE_NEEDS: &str = r"(
    night_from_percent: 58,
    hunger_per_day: 33,
    cold_per_day_at_night: 300,
    cold_relief_per_day: 600,
    fatigue_per_day_awake: 120,
    rest_per_day_asleep: 300,
    starving_per_day: 100,
    freezing_per_day: 400,
    vitality_regain_per_day: 50,
    kj_per_hunger_point: 25,
)";

/// Spawn rules of [`Content::sample`].
pub const SAMPLE_SPAWNS: &str = r#"[
    (material: "branch", terrain: forest, per_thousand: 60),
    (material: "berries", terrain: grass, per_thousand: 40, near: Some((terrain: forest, within: 2))),
]"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closeness_condition_needs_a_reach() {
        let zero = SAMPLE_SPAWNS.replacen("within: 2", "within: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, &zero, SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Spawn {
                index: 1,
                what: "near.within is 1 or more"
            }
        ));
        let content = Content::sample();
        assert_eq!(content.spawns[0].near, None, "near is optional");
        assert_eq!(
            content.spawns[1].near,
            Some(Near {
                terrain: Terrain::Forest,
                within: 2
            })
        );
    }

    #[test]
    fn movement_and_carrying_rules_are_checked() {
        let movement = Content::sample().movement;
        assert_eq!(movement.cost(Terrain::Grass), Some(10));
        assert_eq!(movement.cost(Terrain::Water), None);
        assert_eq!(movement.speed_cm_per_s(Terrain::Forest), 95);
        assert_eq!(movement.speed_cm_per_s(Terrain::Rock), 0);
        let no_rock = SAMPLE_MOVEMENT.replacen(", rock: None", "", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &no_rock, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("every terrain needs a cost")
        ));
        let weak = SAMPLE_MOVEMENT.replacen("carry_capacity_g: 20000", "carry_capacity_g: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &weak, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("carry_capacity_g must be positive")
        ));
        let frozen = SAMPLE_MOVEMENT.replacen(
            "speed_percent_at_full_load: 60",
            "speed_percent_at_full_load: 0",
            1,
        );
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &frozen, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("speed_percent_at_full_load is 1 to 100")
        ));
        assert_eq!(Content::sample().material("flint").unwrap().mass_g(), 400);
    }

    #[test]
    fn sight_budget_and_cap_are_checked() {
        let movement = Content::sample().movement;
        assert_eq!(movement.sight_tiles(Terrain::Grass), 60);
        assert_eq!(movement.sight_tiles(Terrain::Forest), 25);
        assert_eq!(movement.path_budget_tiles, 1500);
        assert_eq!(movement.search_work_per_tick, 60_000);
        assert_eq!(movement.player_share_percent, 50);
        let blind = SAMPLE_MOVEMENT.replacen("forest: 25, ", "", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &blind, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("every terrain needs a sight")
        ));
        let dark = SAMPLE_MOVEMENT.replacen("forest: 25", "forest: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &dark, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("a sight is 1 tile or more")
        ));
        let idle = SAMPLE_MOVEMENT.replacen("path_budget_tiles: 1500", "path_budget_tiles: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &idle, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("path_budget_tiles must be positive")
        ));
        let stuck =
            SAMPLE_MOVEMENT.replacen("search_work_per_tick: 60000", "search_work_per_tick: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &stuck, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("search_work_per_tick must be positive")
        ));
        let greedy =
            SAMPLE_MOVEMENT.replacen("player_share_percent: 50", "player_share_percent: 101", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &greedy, SAMPLE_NEEDS).unwrap_err(),
            ContentError::Movement("player_share_percent is 0 to 100")
        ));
        let blindfold =
            SAMPLE_MOVEMENT.replacen("skirt_budget_tiles: 15000", "skirt_budget_tiles: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, &blindfold, SAMPLE_NEEDS)
                .unwrap_err(),
            ContentError::Movement("skirt_budget_tiles must be positive")
        ));
    }

    #[test]
    fn needs_rules_are_checked() {
        let late = SAMPLE_NEEDS.replacen("night_from_percent: 58", "night_from_percent: 100", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, SAMPLE_MOVEMENT, &late).unwrap_err(),
            ContentError::Needs("night_from_percent is 1 to 99")
        ));
        let free = SAMPLE_NEEDS.replacen("kj_per_hunger_point: 25", "kj_per_hunger_point: 0", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, SAMPLE_MOVEMENT, &free).unwrap_err(),
            ContentError::Needs("kj_per_hunger_point must be positive")
        ));
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, SAMPLE_SPAWNS, SAMPLE_MOVEMENT, "nope")
                .unwrap_err(),
            ContentError::Parse {
                file: "needs.ron",
                ..
            }
        ));
    }

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
        let err = Content::from_ron("[(id: oops", SAMPLE_SPAWNS, SAMPLE_MOVEMENT, SAMPLE_NEEDS)
            .unwrap_err();
        assert!(matches!(
            err,
            ContentError::Parse {
                file: "materials.ron",
                ..
            }
        ));
        let err =
            Content::from_ron(SAMPLE_MATERIALS, "nope", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err();
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
            r##"(id: "flint", name: "Flint", description: "A stone.", kind: stone, mass_kg: 1, hardness: 7, flammable: false, food: None, look: (colour: "#000000"))"##
        );
        assert!(matches!(
            Content::from_ron(&twice, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::DuplicateMaterial(id) if id == "flint"
        ));
        let err = Content::from_ron(
            SAMPLE_MATERIALS,
            r#"[(material: "gold", terrain: grass, per_thousand: 1)]"#,
            SAMPLE_MOVEMENT,
            SAMPLE_NEEDS,
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
            Content::from_ron(&text, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err()
        };
        // Removing "flammable: false, " makes the struct incomplete: a parse error.
        assert!(matches!(
            bad("flammable: false, "),
            ContentError::Parse { .. }
        ));

        let heavy = SAMPLE_MATERIALS.replacen("mass_kg: 0.4", "mass_kg: 0", 1);
        assert!(matches!(
            Content::from_ron(&heavy, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::InvalidValue {
                what: "mass_kg must be positive",
                ..
            }
        ));
        let hard = SAMPLE_MATERIALS.replacen("hardness: 7", "hardness: 11", 1);
        assert!(matches!(
            Content::from_ron(&hard, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::InvalidValue {
                what: "hardness is 0 to 10",
                ..
            }
        ));
        let colour = SAMPLE_MATERIALS.replacen("#6e6a62", "grey", 1);
        assert!(matches!(
            Content::from_ron(&colour, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::InvalidValue {
                what: "look.colour must be #rrggbb",
                ..
            }
        ));
        let mute = SAMPLE_MATERIALS.replacen(
            "description: \"A hard stone that flakes to a sharp edge.\"",
            "description: \" \"",
            1,
        );
        assert!(matches!(
            Content::from_ron(&mute, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::InvalidValue {
                what: "description is empty",
                ..
            }
        ));
        let shouting = SAMPLE_MATERIALS.replacen("id: \"flint\"", "id: \"Flint\"", 1);
        assert!(matches!(
            Content::from_ron(&shouting, "[]", SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::InvalidValue {
                what: "id must be snake_case ASCII",
                ..
            }
        ));
        let dense = SAMPLE_SPAWNS.replacen("per_thousand: 60", "per_thousand: 1001", 1);
        assert!(matches!(
            Content::from_ron(SAMPLE_MATERIALS, &dense, SAMPLE_MOVEMENT, SAMPLE_NEEDS).unwrap_err(),
            ContentError::TooDense {
                index: 0,
                per_thousand: 1001
            }
        ));
    }

    #[test]
    fn species_are_checked() {
        let base = || Content::sample();
        let ok = r#"[(id: "oak", name: "Oak", height_m: 4.0, habitats: { heart: 50, lone: 60 })]"#;
        let content = base().with_species(ok).unwrap();
        assert_eq!(content.species[0].habitats[&Habitat::Lone], 60);
        let twice = r#"[(id: "oak", name: "Oak", height_m: 4.0, habitats: { heart: 1 }),
                        (id: "oak", name: "Oak", height_m: 4.0, habitats: { heart: 1 })]"#;
        assert!(matches!(
            base().with_species(twice).unwrap_err(),
            ContentError::Species { .. }
        ));
        let nowhere = r#"[(id: "oak", name: "Oak", height_m: 4.0, habitats: { heart: 0 })]"#;
        assert!(matches!(
            base().with_species(nowhere).unwrap_err(),
            ContentError::Species { .. }
        ));
        let far =
            r#"[(id: "oak", name: "Oak", height_m: 4.0, habitats: { heart: 1 }, spread_m: 80)]"#;
        assert!(matches!(
            base().with_species(far).unwrap_err(),
            ContentError::Species { .. }
        ));
        let giant = r#"[(id: "oak", name: "Oak", height_m: 400.0, habitats: { heart: 1 })]"#;
        assert!(matches!(
            base().with_species(giant).unwrap_err(),
            ContentError::Species { .. }
        ));
        assert!(matches!(
            base().with_species("nope").unwrap_err(),
            ContentError::Parse {
                file: "species.ron",
                ..
            }
        ));
        // The repository's own file is valid.
        base()
            .with_species(include_str!("../../../content/species.ron"))
            .unwrap();
    }
}
