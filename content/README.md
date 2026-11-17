# World content

Data files describing the world: what things are made of, what grows
where and what it bears, what a body needs and how it moves. Written in [RON](https://github.com/ron-rs/ron),
read and validated by the server at startup (architecture decision D27).
The client never reads these files; it receives what it needs over the protocol.

Hidden game rules (karma, the Emperor's tree, titles) are content too, but their
real values live in a private repository (D30). Files here hold placeholders.

## Files

- `materials.ron` — what things are made of: family, mass, hardness,
  flammability, food value, provisional colour.
- `spawns.ron` — what lies on the ground when a valley is created, per
  terrain, in pieces per thousand tiles.
- `movement.ron` — walking speed, the cost of each terrain (which paths
  weigh and bodies slow down by), what a body can carry and how much a
  full load slows it.
- `needs.ron` — how fast a body hungers, freezes and tires, how it heals,
  and what kills it.
- `geology.ron` — the rock regions under the ground (granite, limestone,
  chalk, sandstone): their size, how their borders wander, how often
  each rock comes. Drawn on worlds of the generator's version 2 on.
- `species.ron` — the kinds of tree: height, the habitats each grows in,
  how far its seeds fall (D37, D40). The client finds a tree's images by
  its id.
- `flora.ron` — the shrubs, plants and mushrooms, and where each grows
  (D37).
- `crops.ron` — what the trees and plants bear to whoever gathers it,
  and when in the year (D45).
- `open_ground.ron` — the trees standing alone and in groves on the open
  biomes of the atlas (grassland, wooded steppe, steppe, marsh): how many
  of each a square kilometre, how large a grove. Worlds cut from the
  atlas only (versions 3 on); the valley's meadows have their own.

The server looks for this directory as `content`, `../content` or
`../../content` from where it runs, or where `--content <DIR>` points. An
invalid file prevents the server from starting; the CI loads every file on
every commit.

Licensed under CC BY-NC-SA 4.0 (see `docs/LICENSE`).
