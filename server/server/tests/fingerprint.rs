//! The world a seed makes, pinned: a fingerprint of every tile (terrain,
//! water depth, current, density of trees, habitat), of every item lying
//! on the ground and of every tree, chunk by chunk. A change meant to make generation
//! faster must leave these untouched; a change meant to change the world
//! updates them on purpose, and says so.

use sim::{Calendar, Content, Map, Pos, World};

/// The repository's content: its species and flora plant the trees.
fn content() -> Content {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    server::content::load(&dir).expect("repository content")
}

/// FNV-1a, 64 bits: stable across Rust versions, unlike `DefaultHasher`.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn bytes(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

fn map_print(map: &Map) -> u64 {
    let mut h = Fnv::new();
    for y in 0..map.height() {
        for x in 0..map.width() {
            let pos = Pos::new(i32::try_from(x).unwrap(), i32::try_from(y).unwrap());
            h.bytes(format!("{:?}", map.get(pos)).as_bytes());
            h.bytes(&[map.depth(pos), map.trees(pos)]);
            h.bytes(format!("{:?}{:?}", map.habitat(pos), map.current(pos)).as_bytes());
        }
    }
    h.0
}

/// An item as the prints read it: which, of what and where, written as
/// items were before they carried their own grams and date (D35, 13
/// November 2026), so a heavier pile moved no item.
#[derive(Debug)]
#[allow(dead_code, reason = "read by Debug only")]
struct Item {
    id: sim::ItemId,
    material: String,
    place: sim::Place,
}

/// A tree as the prints read it: where and of what kind, written as trees
/// were before they carried their key (D2), so a key added moved no tree.
#[derive(Debug)]
#[allow(dead_code, reason = "read by Debug only")]
struct Tree {
    at: sim::FinePos,
    species: usize,
}

fn world_print(world: &World) -> (u64, u64, u64) {
    // The ground's items, made per chunk (and any stored), chunk by chunk.
    let mut items = Fnv::new();
    let chunks = world.map().width().div_ceil(sim::CHUNK_SIZE);
    let chunks = i32::try_from(chunks).expect("few chunks");
    for y in 0..chunks {
        for x in 0..chunks {
            let placed: Vec<Item> = world
                .items_in(sim::ChunkPos::new(x, y))
                .iter()
                .map(|i| Item {
                    id: i.id(),
                    material: i.material().to_owned(),
                    place: i.place(),
                })
                .collect();
            items.bytes(format!("{placed:?}").as_bytes());
        }
    }
    // The trees, placed per chunk, chunk by chunk: where and of what kind
    // ([`Tree`]).
    let mut trees = Fnv::new();
    for y in 0..chunks {
        for x in 0..chunks {
            let placed: Vec<Tree> = world
                .trees_in(sim::ChunkPos::new(x, y))
                .iter()
                .map(|t| Tree {
                    at: t.at,
                    species: t.species,
                })
                .collect();
            trees.bytes(format!("{placed:?}").as_bytes());
        }
    }
    (map_print(world.map()), items.0, trees.0)
}

#[test]
fn the_768_valley_is_pinned() {
    let world = World::with_valley(Calendar::default(), &content(), 768);
    let print = world_print(&world);
    assert_eq!(
        print,
        (
            0x69d7_839f_7a2f_8794,
            0x1699_064a_28c9_c440,
            0x506e_7e6f_4c5a_bab4
        ),
        "{print:#x?}"
    );
}

#[test]
fn a_1536_valley_of_another_seed_is_pinned() {
    let print = map_print(&Map::valley(1536, 1536, 7));
    assert_eq!(print, 0xf1b8_d67f_e900_9b4f, "{print:#x}");
}

/// The game's own valley; slow in a debug build, run by hand
/// (`cargo test --release -p server --test fingerprint -- --ignored`).
#[test]
#[ignore = "the game's 3072 valley: run by hand in release"]
fn the_game_valley_is_pinned() {
    let world = World::new(Calendar::default(), &content());
    let print = world_print(&world);
    assert_eq!(
        print,
        (
            0xf87f_9a29_456e_a107,
            0xe198_ab58_61fb_919b,
            0x3fb9_b8ad_f369_c17e
        ),
        "{print:#x?}"
    );
}
