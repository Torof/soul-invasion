# World content

Data files describing the world: materials and their properties, parts,
processes, species, names, calendars. Written in [RON](https://github.com/ron-rs/ron),
read and validated by the server at startup (architecture decision D27).
The client never reads these files; it receives what it needs over the protocol.

Hidden game rules (karma, the Emperor's tree, titles) are content too, but their
real values live in a private repository (D30). Files here hold placeholders.

Licensed under CC BY-NC-SA 4.0 (see `docs/LICENSE`).
