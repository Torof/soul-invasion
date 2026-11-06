# World content

Data files describing the world: materials and their properties, parts,
processes, species, names, calendars. Written in [RON](https://github.com/ron-rs/ron),
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

The server looks for this directory as `content`, `../content` or
`../../content` from where it runs, or where `--content <DIR>` points. An
invalid file prevents the server from starting; the CI loads every file on
every commit.

Licensed under CC BY-NC-SA 4.0 (see `docs/LICENSE`).
