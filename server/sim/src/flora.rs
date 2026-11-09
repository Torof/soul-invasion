//! The flora: shrubs, plants and mushrooms (D37, D39, `content/flora.ron`).
//!
//! Placed chunk by chunk when a chunk is asked for, from the map, the
//! trees and a seed: the same every time, nothing stored, so the cost
//! follows the chunks in use, never the map's size. What is picked will
//! be stored as a change when gathering comes.
//!
//! Each kind has a **niche**: the places it grows (the trees' habitats,
//! plus `bank`, `stream`, `shallows`), the moisture of the soil and the
//! light it needs, and for some mushrooms a host tree. Where its niche
//! holds, patches of it start (a single plant, a clump, a carpet, a fairy
//! ring) with a chance set by its density, times:
//!
//! - its **stand**: a smooth noise per kind, so a kind comes in places
//!   and fades out, kinds mixing where their stands meet;
//! - the **breath** of the ground: a smooth noise for all, so the
//!   undergrowth is thick here and sparse a little further.
//!
//! Plants with volume (half a metre and taller) keep their room: one does
//! not start where another already stands, nor at the foot of a tree.

// Places in metres and millimetres, radii and rolls: the casts between
// them are bounded by the content's checks (radius at most 8 m, heights
// at most 40 m) and by the chunk's size.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
// Generation is integer-only (invariant R7): no float arithmetic here,
// and no float maths functions (`sim/clippy.toml`). Known exceptions,
// each allowed where it stands, go when the generator is rewritten
// (stage 1): the patch shapes of `lay_patch` and a host's reach in metres.
#![warn(clippy::float_arithmetic)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::r#gen::{hash, noise, unit};
use crate::map::{CHUNK_SIZE, ChunkPos, FinePos, Map, Pos, Terrain};
use crate::woods::Habitat;

/// What a kind of flora is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    /// A tree still to come (placed with the trees once it has images).
    Tree,
    /// A shrub.
    Shrub,
    /// A plant.
    Plant,
    /// A mushroom.
    Mushroom,
}

/// How a kind comes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Grouping {
    /// One plant.
    #[default]
    Single,
    /// A clump of plants within this radius, in metres.
    Clump(f32),
    /// A carpet, thicker than a clump, within this radius.
    Carpet(f32),
    /// A fairy ring of this radius.
    Ring(f32),
}

/// Where a kind grows on its ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GrowsOn {
    /// On the ground.
    #[default]
    Ground,
    /// On the trunk of a standing tree of its host kinds.
    Trunk,
}

/// The trees a mushroom is tied to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Host {
    /// The kinds of tree (ids of `species.ron`).
    pub trees: Vec<String>,
    /// How near one must stand, in metres.
    pub within_m: f32,
}

/// A kind of shrub, plant or mushroom (`content/flora.ron`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Kind {
    /// Content id, `snake_case`, unique.
    pub id: String,
    /// Name shown to players.
    pub name: String,
    /// What it is (set from the file's section).
    #[serde(skip, default = "default_family")]
    pub family: Family,
    /// Grown height, in metres.
    pub height_m: f32,
    /// How much it grows in each place, against the others there.
    pub habitats: BTreeMap<String, u32>,
    /// Soil moisture it needs, 0 (dry) to 100 (in the water): least, most.
    pub moisture: (u8, u8),
    /// Light it needs, 0 (heart of a wood) to 100 (open): least, most.
    #[serde(default = "full_range")]
    pub light: (u8, u8),
    /// The trees it is tied to, if any.
    #[serde(default)]
    pub host: Option<Host>,
    /// On the ground, or on its host's trunk.
    #[serde(default)]
    pub on: GrowsOn,
    /// How it comes.
    #[serde(default)]
    pub grouping: Grouping,
    /// Patches a thousand tiles where its niche holds fully (on a trunk:
    /// a thousand host trees).
    #[serde(default)]
    pub per_thousand: u32,
    /// What gathering it yields (material ids, most still to come).
    #[serde(default)]
    pub gives: Vec<String>,
    /// When it would be in season, for later.
    #[serde(default)]
    pub season: String,
}

fn default_family() -> Family {
    Family::Plant
}

fn full_range() -> (u8, u8) {
    (0, 100)
}

/// The file `content/flora.ron`, by section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct FloraFile {
    trees: Vec<Kind>,
    shrubs: Vec<Kind>,
    plants: Vec<Kind>,
    mushrooms: Vec<Kind>,
}

/// The places a kind may name: the trees' habitats and three more.
pub const PLACES: [&str; 13] = [
    "meadow",
    "edge",
    "wood",
    "heart",
    "clearing",
    "camp",
    "copse",
    "lone",
    "riverside",
    "cliff_foot",
    "bank",
    "stream",
    "shallows",
];

/// Parses `content/flora.ron` into one list, trees first, then shrubs,
/// plants and mushrooms, each with its family.
///
/// # Errors
/// The parser's error when the text does not parse.
pub fn parse(text: &str) -> Result<Vec<Kind>, ron::error::SpannedError> {
    let file: FloraFile = ron::from_str(text)?;
    let mut kinds = Vec::new();
    for (family, list) in [
        (Family::Tree, file.trees),
        (Family::Shrub, file.shrubs),
        (Family::Plant, file.plants),
        (Family::Mushroom, file.mushrooms),
    ] {
        kinds.extend(list.into_iter().map(|mut k| {
            k.family = family;
            k
        }));
    }
    Ok(kinds)
}

/// What is wrong with a kind, if anything: an id, a name, a height, a
/// place it grows, places and ranges that exist, hosts among `trees`.
#[must_use]
pub fn check(kind: &Kind, trees: &[&str]) -> Option<&'static str> {
    if kind.id.is_empty()
        || !kind
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Some("id must be snake_case ASCII");
    }
    if kind.name.trim().is_empty() {
        return Some("name is empty");
    }
    if !(kind.height_m.is_finite() && (0.01..=40.0).contains(&kind.height_m)) {
        return Some("height_m is 0.01 to 40");
    }
    if kind.habitats.values().all(|w| *w == 0) {
        return Some("grows nowhere");
    }
    if kind.habitats.keys().any(|p| !PLACES.contains(&p.as_str())) {
        return Some("names a place that does not exist");
    }
    let range = |(lo, hi): (u8, u8)| lo <= hi && hi <= 100;
    if !range(kind.moisture) || !range(kind.light) {
        return Some("a range is not least to most within 0 to 100");
    }
    if kind.per_thousand > 1000 {
        return Some("per_thousand is above 1000");
    }
    let radius = match kind.grouping {
        Grouping::Single => 0.0,
        Grouping::Clump(r) | Grouping::Carpet(r) | Grouping::Ring(r) => r,
    };
    if !(radius.is_finite() && (0.0..=MAX_RADIUS_M).contains(&radius)) {
        return Some("a group's radius is 0 to 8 metres");
    }
    if let Some(host) = &kind.host {
        if host.trees.is_empty() || host.trees.iter().any(|t| !trees.contains(&t.as_str())) {
            return Some("a host names no kind of tree, or an unknown one");
        }
        if !(host.within_m.is_finite() && (0.0..=20.0).contains(&host.within_m)) {
            return Some("a host's within_m is 0 to 20");
        }
    } else if kind.on == GrowsOn::Trunk {
        return Some("grows on a trunk but has no host");
    }
    None
}

/// A plant standing on the ground (or on a trunk).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plant {
    /// Where it stands.
    pub at: FinePos,
    /// Its kind: an index into the content's flora.
    pub kind: u16,
}

/// A tree as the flora sees it: where it stands, and its kind's id.
#[derive(Debug, Clone, Copy)]
pub struct TreeAt<'a> {
    /// Its foot.
    pub at: FinePos,
    /// Its kind (an id of `species.ron`).
    pub species: &'a str,
}

/// Widest a group may be, in metres.
const MAX_RADIUS_M: f32 = 8.0;
/// Tiles read around a chunk: patches starting there may reach into it.
const MARGIN: i32 = 9;
/// How far the soil's moisture looks for water, in tiles.
const WATER_REACH: i32 = 9;
/// Width of a kind's stands, and of the ground's breath, in tiles.
const STAND_CELL: u32 = 14;
const BREATH_CELL: u32 = 22;
/// The valley's background wetness (the noise the brooks rise from).
const WET_CELL: u32 = 32;
/// Soft edge of a niche's ranges, in points of 0 to 100.
const NICHE_SOFT: i32 = 12;
/// Plants a square metre in a clump, in a carpet.
const CLUMP_DENSITY: f32 = 0.9;
const CARPET_DENSITY: f32 = 2.2;
/// Most plants in one patch.
const MOST_IN_PATCH: u32 = 60;
/// Room a plant with volume keeps, in millimetres; and how near a tree's
/// foot nothing grows (the trunk is there).
const ROOM_MM: i32 = 450;
const TRUNK_MM: i32 = 600;
/// Height from which a plant keeps its room, in metres.
const VOLUME_M: f32 = 0.5;

/// How well `value` lies within `(lo, hi)`, in thousandths: 1000 inside,
/// easing to nothing `NICHE_SOFT` points outside.
fn within(value: i32, (lo, hi): (u8, u8)) -> u32 {
    let out = (i32::from(lo) - value).max(value - i32::from(hi)).max(0);
    u32::try_from((1000 - out * 1000 / NICHE_SOFT).max(0)).expect("non-negative")
}

/// What the ground is like at a tile, for the flora.
struct Ground {
    /// The places it counts as (a habitat, and maybe bank, stream...).
    places: Vec<&'static str>,
    /// Soil moisture, 0 to 100.
    moisture: i32,
    /// Light, 0 to 100.
    light: i32,
    /// Can something take root on it at all.
    rooted: bool,
}

fn habitat_name(h: Habitat) -> Option<&'static str> {
    Some(match h {
        Habitat::Meadow => "meadow",
        Habitat::Edge => "edge",
        Habitat::Wood => "wood",
        Habitat::Heart => "heart",
        Habitat::Clearing => "clearing",
        Habitat::Camp => "camp",
        Habitat::Copse => "copse",
        Habitat::Lone => "lone",
        Habitat::Riverside => "riverside",
        Habitat::CliffFoot => "cliff_foot",
        Habitat::None => return None,
    })
}

/// The ground at `pos`, `distance` tiles from the nearest water (0 in it,
/// `WATER_REACH + 1` for none near).
fn ground_at(map: &Map, seed: u64, pos: Pos, distance: i32) -> Ground {
    let terrain = map.get(pos);
    let mut places = Vec::new();
    match terrain {
        Some(Terrain::Brook) => places.push("stream"),
        Some(Terrain::Ford) => places.push("shallows"),
        _ => {}
    }
    if let Some(name) = habitat_name(map.habitat(pos)) {
        places.push(name);
    }
    let rooted = matches!(
        terrain,
        Some(Terrain::Grass | Terrain::Forest | Terrain::Brook | Terrain::Ford)
    );
    if rooted && distance > 0 && distance <= 2 {
        places.push("bank");
    }
    let trees = i32::from(map.trees(pos));
    let background = 20
        + i32::try_from(noise(seed ^ 0x77e7, WET_CELL, pos.x, pos.y)).expect("small") / 20
        + trees / 6;
    let near_water = if distance == 0 {
        100
    } else {
        96 - distance * 11
    };
    Ground {
        places,
        moisture: near_water.max(background).clamp(0, 100),
        light: 100 - trees,
        rooted,
    }
}

/// The distance to the nearest water (a river, a ford, a brook), in tiles
/// (the larger of the two offsets), of every tile of the `span` × `span`
/// square from `lo`, up to `WATER_REACH + 1`: two sweeps over the square
/// and a margin of `WATER_REACH` around it, instead of a search per tile.
fn water_distances(map: &Map, lo: Pos, span: i32) -> Vec<i32> {
    let far = WATER_REACH + 1;
    let wide = span + 2 * WATER_REACH;
    let at = |x: i32, y: i32| usize::try_from(y * wide + x).expect("inside");
    let mut d: Vec<i32> = (0..wide * wide)
        .map(|i| {
            let p = Pos::new(lo.x - WATER_REACH + i % wide, lo.y - WATER_REACH + i / wide);
            if matches!(
                map.get(p),
                Some(Terrain::Water | Terrain::Ford | Terrain::Brook)
            ) {
                0
            } else {
                far
            }
        })
        .collect();
    for y in 0..wide {
        for x in 0..wide {
            let mut v = d[at(x, y)];
            for (dx, dy) in [(-1, 0), (-1, -1), (0, -1), (1, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if (0..wide).contains(&nx) && (0..wide).contains(&ny) {
                    v = v.min(d[at(nx, ny)] + 1);
                }
            }
            d[at(x, y)] = v;
        }
    }
    for y in (0..wide).rev() {
        for x in (0..wide).rev() {
            let mut v = d[at(x, y)];
            for (dx, dy) in [(1, 0), (1, 1), (0, 1), (-1, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if (0..wide).contains(&nx) && (0..wide).contains(&ny) {
                    v = v.min(d[at(nx, ny)] + 1);
                }
            }
            d[at(x, y)] = v;
        }
    }
    (0..span * span)
        .map(|i| d[at(i % span + WATER_REACH, i / span + WATER_REACH)].min(far))
        .collect()
}

/// How well `kind` grows on `ground`, in thousandths: its best place's
/// weight against its best weight anywhere, times its two ranges.
fn fit(kind: &Kind, best: u32, ground: &Ground) -> u32 {
    if best == 0 || !ground.rooted {
        return 0;
    }
    let weight = ground
        .places
        .iter()
        .map(|p| kind.habitats.get(*p).copied().unwrap_or(0))
        .max()
        .unwrap_or(0);
    if weight == 0 {
        return 0;
    }
    let place = weight * 1000 / best;
    place * within(ground.moisture, kind.moisture) / 1000 * within(ground.light, kind.light) / 1000
}

/// The plants of chunk `chunk`, of the kinds `kinds` (the content's
/// flora; trees are left out), on `map`, around the trees `trees` (those
/// in the chunk and around it), for `seed`. Sorted by place, so the same
/// every time.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn plants_in(
    map: &Map,
    kinds: &[Kind],
    trees: &[TreeAt<'_>],
    seed: u64,
    chunk: ChunkPos,
) -> Vec<Plant> {
    let size = i32::try_from(CHUNK_SIZE).expect("small chunk");
    let origin = Pos::new(chunk.x * size, chunk.y * size);
    let inside = |at: FinePos| {
        let t = at.tile();
        (origin.x..origin.x + size).contains(&t.x) && (origin.y..origin.y + size).contains(&t.y)
    };
    let best: Vec<u32> = kinds
        .iter()
        .map(|k| k.habitats.values().copied().max().unwrap_or(0))
        .collect();
    // The ground of every tile read, worked out once.
    let span = size + 2 * MARGIN + 2 * (MAX_RADIUS_M as i32 + 1);
    let lo = Pos::new(origin.x - (span - size) / 2, origin.y - (span - size) / 2);
    let distances = water_distances(map, lo, span);
    let grounds: Vec<Ground> = (0..span * span)
        .map(|i| {
            let d = distances[usize::try_from(i).expect("inside")];
            ground_at(map, seed, Pos::new(lo.x + i % span, lo.y + i / span), d)
        })
        .collect();
    let ground = |pos: Pos| -> Option<&Ground> {
        let (x, y) = (pos.x - lo.x, pos.y - lo.y);
        ((0..span).contains(&x) && (0..span).contains(&y))
            .then(|| &grounds[usize::try_from(y * span + x).expect("inside")])
    };
    let mut plants = Vec::new();
    // Room kept: plants with volume, and the trunks.
    let mut taken: Vec<FinePos> = trees.iter().map(|t| t.at).collect();
    let free = |taken: &[FinePos], at: FinePos, room: i32| {
        taken
            .iter()
            .all(|o| (o.x - at.x).abs() >= room || (o.y - at.y).abs() >= room)
    };
    // Trunks first: the mushrooms on their host trees.
    for (k, kind) in kinds.iter().enumerate() {
        if kind.family == Family::Tree || kind.on != GrowsOn::Trunk {
            continue;
        }
        let Some(host) = &kind.host else { continue };
        for tree in trees {
            if !inside(tree.at) || !host.trees.iter().any(|h| h == tree.species) {
                continue;
            }
            let Some(ground) = ground(tree.at.tile()) else {
                continue;
            };
            let chance = kind.per_thousand * fit(kind, best[k], ground) / 1000;
            #[allow(clippy::cast_sign_loss)]
            if unit(&[seed, 0xf107a, k as u64, tree.at.x as u64, tree.at.y as u64]) >= chance {
                continue;
            }
            let at = FinePos::new(tree.at.x + 120, tree.at.y + 60);
            plants.push(Plant {
                at,
                kind: u16::try_from(k).expect("few kinds"),
            });
        }
    }
    // Then the patches, in a fixed order: tile by tile, kind by kind.
    for ty in origin.y - MARGIN..origin.y + size + MARGIN {
        for tx in origin.x - MARGIN..origin.x + size + MARGIN {
            let pos = Pos::new(tx, ty);
            let Some(here) = ground(pos) else { continue };
            if !here.rooted {
                continue;
            }
            let breath = 300 + noise(seed ^ 0xb4ea, BREATH_CELL, tx, ty) * 14 / 10;
            for (k, kind) in kinds.iter().enumerate() {
                if kind.family == Family::Tree
                    || kind.on == GrowsOn::Trunk
                    || kind.per_thousand == 0
                {
                    continue;
                }
                let niche = fit(kind, best[k], here);
                if niche == 0 {
                    continue;
                }
                // The stand squared, in thousandths, about 1000 on average.
                let stand =
                    u64::from(noise(seed ^ hash(&[0x57a4d, k as u64]), STAND_CELL, tx, ty)) + 50;
                let stand = stand * stand * 2860 / 1_102_500;
                // A chance in a million: the density, times the niche, the
                // stand and the breath (each in thousandths).
                let chance = u64::from(kind.per_thousand)
                    * 1000
                    * u64::from(niche)
                    * stand
                    * u64::from(breath)
                    / 1_000_000_000;
                #[allow(clippy::cast_sign_loss)]
                let roll = hash(&[seed, 0x9a7c, k as u64, tx as u64, ty as u64]) % 1_000_000;
                if roll >= chance {
                    continue;
                }
                if let Some(host) = &kind.host {
                    // R7 exception, to go in stage 1: metres to millimetres
                    // in floats (the content gives the reach in metres).
                    #[allow(clippy::float_arithmetic)]
                    let reach = (host.within_m * 1000.0) as i32;
                    let centre = pos.centre();
                    let near = trees.iter().any(|t| {
                        host.trees.iter().any(|h| h == t.species)
                            && (t.at.x - centre.x).abs() <= reach
                            && (t.at.y - centre.y).abs() <= reach
                    });
                    if !near {
                        continue;
                    }
                }
                // Each plant of the patch on ground where its kind grows: a
                // carpet stops where its place ends.
                let grows =
                    |at: FinePos| ground(at.tile()).is_some_and(|g| fit(kind, best[k], g) > 0);
                lay_patch(
                    kind,
                    k,
                    pos,
                    seed,
                    &mut taken,
                    &mut plants,
                    &inside,
                    &grows,
                    &free,
                );
            }
        }
    }
    plants.sort_by_key(|p| (p.at.y, p.at.x, p.kind));
    plants
}

/// The plants of one patch of `kind` started at `pos`, those that fall in
/// the chunk (`inside`), on ground where it grows (`grows`), and find
/// room.
#[allow(clippy::too_many_arguments)]
fn lay_patch(
    kind: &Kind,
    k: usize,
    pos: Pos,
    seed: u64,
    taken: &mut Vec<FinePos>,
    plants: &mut Vec<Plant>,
    inside: &dyn Fn(FinePos) -> bool,
    grows: &dyn Fn(FinePos) -> bool,
    free: &dyn Fn(&[FinePos], FinePos, i32) -> bool,
) {
    let centre = pos.centre();
    #[allow(clippy::cast_sign_loss)]
    let roll = |n: u32, salt: u64| {
        hash(&[
            seed,
            0x9a7c,
            k as u64,
            pos.x as u64,
            pos.y as u64,
            u64::from(n),
            salt,
        ])
    };
    let jitter = |n: u32, salt: u64, range: i32| {
        i32::try_from(roll(n, salt) % u64::try_from(2 * range + 1).expect("positive"))
            .expect("small")
            - range
    };
    let spots: Vec<FinePos> = match kind.grouping {
        Grouping::Single => vec![FinePos::new(
            centre.x + jitter(0, 1, 400),
            centre.y + jitter(0, 2, 400),
        )],
        Grouping::Clump(r) | Grouping::Carpet(r) => {
            let density = if matches!(kind.grouping, Grouping::Carpet(_)) {
                CARPET_DENSITY
            } else {
                CLUMP_DENSITY
            };
            // R7 exceptions, to go in stage 1: a clump's or a carpet's
            // count, reach and thinning are worked out in floats, with
            // `f64::hypot`.
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::float_arithmetic
            )]
            let n = ((std::f32::consts::PI * r * r * density) as u32).clamp(1, MOST_IN_PATCH);
            #[allow(clippy::cast_possible_truncation, clippy::float_arithmetic)]
            let reach = (r * 1000.0) as i32;
            (0..n)
                .filter_map(|i| {
                    let (dx, dy) = (jitter(i, 3, reach), jitter(i, 4, reach));
                    // Within the circle, and thinner towards its rim.
                    #[allow(clippy::float_arithmetic, clippy::disallowed_methods)]
                    let d = (f64::from(dx).hypot(f64::from(dy)) / f64::from(reach.max(1))) as f32;
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        clippy::float_arithmetic
                    )]
                    let keep = d <= 1.0 && (roll(i, 5) % 1000) as f32 >= d * d * 600.0;
                    keep.then(|| FinePos::new(centre.x + dx, centre.y + dy))
                })
                .collect()
        }
        Grouping::Ring(r) => {
            // R7 exceptions, to go in stage 1: a fairy ring's count, angles
            // and wobble are worked out in floats, with `f32::sin` and
            // `f32::cos`.
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::float_arithmetic
            )]
            let n = ((std::f32::consts::TAU * r * 1.6) as u32).clamp(5, MOST_IN_PATCH);
            (0..n)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss, clippy::float_arithmetic)]
                    let angle = std::f32::consts::TAU * i as f32 / n as f32;
                    #[allow(clippy::float_arithmetic)]
                    let wobble = 1.0 + (jitter(i, 6, 100) as f32) / 1000.0;
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::float_arithmetic,
                        clippy::disallowed_methods
                    )]
                    FinePos::new(
                        centre.x + (angle.cos() * r * wobble * 1000.0) as i32,
                        centre.y + (angle.sin() * r * wobble * 1000.0) as i32,
                    )
                })
                .collect()
        }
    };
    let volume = kind.height_m >= VOLUME_M;
    for at in spots {
        if !inside(at) {
            continue;
        }
        // On ground where its kind grows, and not on a trunk.
        if !grows(at) {
            continue;
        }
        if !free(taken, at, if volume { ROOM_MM } else { TRUNK_MM / 3 }) {
            continue;
        }
        if volume {
            taken.push(at);
        }
        plants.push(Plant {
            at,
            kind: u16::try_from(k).expect("few kinds"),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Species;

    const FLORA: &str = include_str!("../../../content/flora.ron");
    const SPECIES: &str = include_str!("../../../content/species.ron");

    fn kinds() -> Vec<Kind> {
        parse(FLORA).expect("flora.ron parses")
    }

    #[test]
    fn the_repository_flora_is_sensible() {
        let species: Vec<Species> = ron::from_str(SPECIES).expect("species.ron parses");
        let kinds = kinds();
        let trees: Vec<&str> = species
            .iter()
            .map(|s| s.id.as_str())
            .chain(
                kinds
                    .iter()
                    .filter(|k| k.family == Family::Tree)
                    .map(|k| k.id.as_str()),
            )
            .collect();
        assert_eq!(kinds.len(), 29, "6 shrubs, 14 plants, 9 mushrooms");
        for kind in &kinds {
            assert_eq!(check(kind, &trees), None, "{}", kind.id);
        }
        assert_eq!(
            kinds
                .iter()
                .filter(|k| k.family == Family::Mushroom)
                .count(),
            9
        );
    }

    /// The valley's chunks around the camp, with their plants.
    type Sample = (
        Map,
        Vec<Kind>,
        Vec<(ChunkPos, Vec<Plant>)>,
        Vec<(FinePos, String)>,
    );

    fn sample() -> Sample {
        let species: Vec<Species> = ron::from_str(SPECIES).expect("species.ron parses");
        let map = Map::valley(768, 768, 1);
        let trees: Vec<(FinePos, String)> = crate::trees::plant(&map, &species, 1)
            .into_iter()
            .map(|t| (t.at, species[t.species].id.clone()))
            .collect();
        let kinds = kinds();
        let camp = map.camp().chunk();
        let mut chunks = Vec::new();
        for dy in -3..=3 {
            for dx in -3..=3 {
                let chunk = ChunkPos::new(camp.x + dx, camp.y + dy);
                let near: Vec<TreeAt<'_>> = trees
                    .iter()
                    .filter(|(at, _)| {
                        let c = at.tile().chunk();
                        (c.x - chunk.x).abs() <= 1 && (c.y - chunk.y).abs() <= 1
                    })
                    .map(|(at, s)| TreeAt {
                        at: *at,
                        species: s,
                    })
                    .collect();
                chunks.push((chunk, plants_in(&map, &kinds, &near, 1, chunk)));
            }
        }
        (map, kinds, chunks, trees)
    }

    #[test]
    fn plants_grow_where_their_niche_holds() {
        let started = std::time::Instant::now();
        let (map, kinds, chunks, trees) = sample();
        let per_chunk = started.elapsed() / u32::try_from(chunks.len()).expect("few");
        let total: usize = chunks.iter().map(|(_, p)| p.len()).sum();
        let mut by_kind = vec![0_usize; kinds.len()];
        for (chunk, plants) in &chunks {
            for p in plants {
                assert_eq!(p.at.tile().chunk(), *chunk, "a plant lies in its own chunk");
                by_kind[usize::from(p.kind)] += 1;
                let kind = &kinds[usize::from(p.kind)];
                let terrain = map.get(p.at.tile());
                assert!(
                    matches!(
                        terrain,
                        Some(Terrain::Grass | Terrain::Forest | Terrain::Brook | Terrain::Ford)
                    ),
                    "{} on {terrain:?}",
                    kind.id
                );
                if kind
                    .habitats
                    .keys()
                    .all(|h| h == "stream" || h == "shallows")
                {
                    assert!(
                        matches!(terrain, Some(Terrain::Brook | Terrain::Ford)),
                        "{} out of the water",
                        kind.id
                    );
                }
                if let Some(host) = &kind.host {
                    let reach = (host.within_m * 1000.0) as i32 + 1500;
                    assert!(
                        trees.iter().any(|(at, s)| host.trees.contains(s)
                            && (at.x - p.at.x).abs() <= reach
                            && (at.y - p.at.y).abs() <= reach),
                        "{} far from its hosts",
                        kind.id
                    );
                }
            }
        }
        let names: Vec<String> = kinds
            .iter()
            .zip(&by_kind)
            .filter(|(k, _)| k.family != Family::Tree)
            .map(|(k, n)| format!("{} {n}", k.id))
            .collect();
        eprintln!(
            "{total} plants on {} chunks ({} a chunk), {per_chunk:?} a chunk: {}",
            chunks.len(),
            total / chunks.len(),
            names.join(", ")
        );
        assert!(total > 0);
        // The same every time.
        let (_, _, again, _) = sample();
        assert_eq!(chunks, again);
    }
}
