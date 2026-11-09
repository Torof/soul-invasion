//! The world a seed makes, pinned: a fingerprint of every tile (terrain,
//! water depth, current, density of trees, habitat), of every item lying
//! on the ground and of every tree. A change meant to make generation
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

fn world_print(world: &World) -> (u64, u64, u64) {
    let mut items = Fnv::new();
    items.bytes(format!("{:?}", world.items()).as_bytes());
    let mut trees = Fnv::new();
    trees.bytes(format!("{:?}", world.trees()).as_bytes());
    (map_print(world.map()), items.0, trees.0)
}

#[test]
fn the_768_valley_is_pinned() {
    let world = World::with_valley(Calendar::default(), &content(), 768);
    let print = world_print(&world);
    assert_eq!(
        print,
        (
            0x14dd_73b4_8a19_0833,
            0x7833_1ef2_f0f2_ba3a,
            0x8d4b_87ab_cea5_950f
        ),
        "{print:#x?}"
    );
}

#[test]
fn a_1536_valley_of_another_seed_is_pinned() {
    let print = map_print(&Map::valley(1536, 1536, 7));
    assert_eq!(print, 0xd2fd_c8ae_7554_c77a, "{print:#x}");
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
            0xaef7_eec3_3a88_fb48,
            0x5f74_0f93_9913_d95b,
            0x366b_9cd3_34a9_e906
        ),
        "{print:#x?}"
    );
}
