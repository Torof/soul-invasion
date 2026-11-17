# Soul Invasion

*Working title. Early development.*

An online game where every player is a **parasite soul** that takes over the
inhabitants of a living world, lives a whole life, dies, and is reborn
elsewhere, carrying its knowledge across the centuries of a civilisation the
players build themselves, from the stone age to wherever they take it.

Three pillars: **build** (everything by hand, slowly), **live in society**
(politics, economy, religions, all created by players and NPCs), and
**live, die, be reborn** (death is frequent; dying well and choosing one's
rebirth is a skill).

## Where things are

| Path | What |
|---|---|
| [`docs/`](docs/README.md) | Vision, lore, game systems, architecture decisions, first-version scope, open questions. **Start here.** |
| `server/` | Rust workspace: `sim` (pure simulation), `protocol` (wire contract), `server` (network, persistence, loop). |
| `client/` | Godot 4 client (GDScript). Displays the world, transmits intentions, decides nothing. |
| `content/` | World data (materials, trees and plants, needs…) in RON, read by the server, and the game's art. |

## Status

Milestone 2 of the first version, *The Tribe*: surviving, built through
the vivarium, a world of NPCs alone under glass (D41). Milestone 1, the
skeleton, was done on 5 November 2026. See
[`docs/05-first-version.md`](docs/05-first-version.md).

## Building

- Server: install Rust (`rustup`), then `cd server && cargo test && cargo run`.
- Client: install Godot 4.7, open `client/project.godot`, or run the headless
  tests with `godot --headless --path client -s tests/run_tests.gd`.

## Contributing

Ideas, lore, code and testing are all welcome. Read
[`CONTRIBUTING.md`](CONTRIBUTING.md) first.

## Licenses

- Code: [AGPL-3.0-or-later](LICENSE).
- Lore, design documents, world content and artwork: [CC BY-NC-SA 4.0](docs/LICENSE).
