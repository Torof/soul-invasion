//! Natural items: what the ground itself drops (dead wood under the trees,
//! pebbles by the water), made per chunk from the seed and the content's
//! spawn rules, never stored (D31). Only what a body touches becomes a
//! stored item; the natural ones taken are remembered, so they do not come
//! back. A chunk's natural items depend on its tiles and their
//! neighbourhood alone: any chunk, in any order, gives the same items.

use crate::content::Spawn;
use crate::r#gen::hash;
use crate::map::{CHUNK_SIZE, ChunkPos, Map, Pos};
use crate::{Item, ItemId, Place};

/// The bit that marks a natural item's id; below it, the chunk (two
/// fields of `CHUNK_BITS`) and the slot (`SLOT_BITS`). All of it under
/// 2^53, so a client reading JSON numbers as floats keeps ids exact.
pub const NATURAL: u64 = 1 << 52;
/// Bits per chunk coordinate: chunks 0 to 262,143 on each axis, 8,388 km.
const CHUNK_BITS: u32 = 18;
/// Bits of a slot: the tile in its chunk (10 bits) and the spawn rule (6).
const SLOT_BITS: u32 = 16;
const RULE_BITS: u32 = 6;
/// Spawn rules a natural item's id can tell apart.
pub const MOST_RULES: usize = 1 << RULE_BITS;

/// The id of the natural item of `rule` on `tile`, or `None` outside the
/// chunks ids can name.
#[must_use]
pub fn natural_id(tile: Pos, rule: usize) -> Option<ItemId> {
    let chunk = tile.chunk();
    let side = i32::try_from(CHUNK_SIZE).expect("small chunk");
    let limit = 1_i64 << CHUNK_BITS;
    if !(0..limit).contains(&i64::from(chunk.x))
        || !(0..limit).contains(&i64::from(chunk.y))
        || rule >= MOST_RULES
    {
        return None;
    }
    let origin = chunk.origin();
    let index =
        u64::try_from((tile.y - origin.y) * side + (tile.x - origin.x)).expect("in its chunk");
    let slot = index << RULE_BITS | u64::try_from(rule).expect("few rules");
    let cx = u64::try_from(chunk.x).expect("checked");
    let cy = u64::try_from(chunk.y).expect("checked");
    Some(ItemId(
        NATURAL | cx << (CHUNK_BITS + SLOT_BITS) | cy << SLOT_BITS | slot,
    ))
}

/// Whether `id` names a natural item.
#[must_use]
pub fn is_natural(id: ItemId) -> bool {
    id.0 & NATURAL != 0 && id.0 < NATURAL << 1
}

/// The chunk a natural item's id names (where it lies until taken).
#[must_use]
pub fn chunk_of(id: ItemId) -> ChunkPos {
    let mask = (1_u64 << CHUNK_BITS) - 1;
    let x = (id.0 >> (CHUNK_BITS + SLOT_BITS)) & mask;
    let y = (id.0 >> SLOT_BITS) & mask;
    ChunkPos::new(
        i32::try_from(x).expect("18 bits"),
        i32::try_from(y).expect("18 bits"),
    )
}

/// The natural items of `chunk` on `map`, under the `spawns` rules and the
/// world's `seed`, in slot order: on each tile, for each rule of its
/// terrain whose `near` holds, a piece with the rule's chance.
#[must_use]
pub fn natural_items(map: &Map, spawns: &[Spawn], seed: u64, chunk: ChunkPos) -> Vec<Item> {
    let mut out = Vec::new();
    let origin = chunk.origin();
    let side = i32::try_from(CHUNK_SIZE).expect("small chunk");
    for dy in 0..side {
        for dx in 0..side {
            let tile = Pos::new(origin.x + dx, origin.y + dy);
            let Some(terrain) = map.get(tile) else {
                continue;
            };
            for (rule, spawn) in spawns.iter().enumerate() {
                if spawn.terrain != terrain || !near_enough(map, tile, spawn) {
                    continue;
                }
                #[allow(clippy::cast_sign_loss)]
                let roll = hash(&[
                    seed ^ 0x5eed_17e5,
                    tile.x as u64,
                    tile.y as u64,
                    rule as u64,
                ]) % 1000;
                if roll >= u64::from(spawn.per_thousand) {
                    continue;
                }
                if let Some(id) = natural_id(tile, rule) {
                    out.push(Item {
                        id,
                        material: spawn.material.clone(),
                        place: Place::Ground { pos: tile },
                    });
                }
            }
        }
    }
    out
}

/// A rule with `near` holds within that many tiles (eight-way) of its
/// terrain; one without always does.
fn near_enough(map: &Map, tile: Pos, spawn: &Spawn) -> bool {
    let Some(near) = &spawn.near else {
        return true;
    };
    let r = i32::try_from(near.within).expect("a short reach");
    (-r..=r).any(|dy| {
        (-r..=r).any(|dx| map.get(Pos::new(tile.x + dx, tile.y + dy)) == Some(near.terrain))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{Near, Spawn};
    use crate::map::Terrain;

    fn rules() -> Vec<Spawn> {
        vec![
            Spawn {
                material: "branch".to_owned(),
                terrain: Terrain::Forest,
                per_thousand: 60,
                near: None,
            },
            Spawn {
                material: "pebble".to_owned(),
                terrain: Terrain::Grass,
                per_thousand: 90,
                near: Some(Near {
                    terrain: Terrain::Water,
                    within: 4,
                }),
            },
        ]
    }

    #[test]
    fn natural_ids_name_their_chunk_and_stay_under_two_to_the_53() {
        for (tile, rule) in [
            (Pos::new(0, 0), 0),
            (Pos::new(3071, 2048), 1),
            (Pos::new(8_388_000, 5), 63),
        ] {
            let id = natural_id(tile, rule).expect("in range");
            assert!(is_natural(id));
            assert!(id.0 < 1 << 53, "{id:?}");
            assert_eq!(chunk_of(id), tile.chunk());
        }
        assert_eq!(natural_id(Pos::new(-1, 0), 0), None);
        assert_eq!(natural_id(Pos::new(0, 0), MOST_RULES), None);
        assert!(!is_natural(ItemId(12)));
        // Every slot of a chunk is a different id.
        let ids: std::collections::BTreeSet<_> = (0..32)
            .flat_map(|y| {
                (0..32).flat_map(move |x| (0..2).map(move |r| natural_id(Pos::new(x, y), r)))
            })
            .collect();
        assert_eq!(ids.len(), 32 * 32 * 2);
    }

    #[test]
    fn a_chunk_gives_the_same_items_alone_or_among_its_neighbours() {
        let map = Map::valley(256, 256, 4);
        let spawns = rules();
        let mut all = Vec::new();
        for cy in 0..8 {
            for cx in 0..8 {
                all.extend(natural_items(&map, &spawns, 4, ChunkPos::new(cx, cy)));
            }
        }
        // In reverse order, each chunk alone: the same items.
        let mut again = Vec::new();
        for cy in (0..8).rev() {
            for cx in (0..8).rev() {
                let mut chunk = natural_items(&map, &spawns, 4, ChunkPos::new(cx, cy));
                chunk.extend(again);
                again = chunk;
            }
        }
        assert_eq!(all.len(), again.len());
        let key = |i: &Item| i.id;
        all.sort_by_key(key);
        again.sort_by_key(key);
        assert_eq!(all, again);
        assert!(all.len() > 100, "{} items", all.len());
        // Where the rules say: branches in the forest, pebbles near water.
        for item in &all {
            let pos = item.pos().expect("on the ground");
            match item.material() {
                "branch" => assert_eq!(map.get(pos), Some(Terrain::Forest)),
                "pebble" => assert_eq!(map.get(pos), Some(Terrain::Grass)),
                other => panic!("{other}"),
            }
            assert_eq!(chunk_of(item.id), pos.chunk());
        }
    }
}
