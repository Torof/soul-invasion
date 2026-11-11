# Contributing to Soul Invasion

Soul Invasion is a passion project, built in the open and happy to welcome
people who want to help: ideas, lore, code, art, testing, all of it.

## How the project is run

- **Torof is the project lead** and has the final say on what goes into the
  game: vision, design decisions, practices. This keeps the project coherent.
  Everything else is open to discussion.
- The reference documents live in [`docs/`](docs/README.md). Read them first:
  they explain what the game is, what it is not, and why each technical
  decision was made.
- We discuss first, decide, then build. A new idea starts as a conversation,
  not as a pull request.

## Proposing an idea

Open a GitHub issue or a discussion. Say what problem it solves or what it
adds to the three pillars (build, live in society, live-die-be-reborn), and
how it fits the design philosophy in `docs/01-vision.md`. Lore proposals are
as welcome as technical ones.

## Proposing code

1. Agree on the change first (issue or discussion), unless it is a small fix.
2. Fork, branch, and keep the pull request focused on one thing.
3. Everything is in **English**: code, comments, identifiers, commits, docs.
4. Commits follow [Conventional Commits](https://www.conventionalcommits.org/)
   (`feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `ci:`, `chore:`).
5. Before opening the pull request, make sure it passes locally:
   - Server: `cd server && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
   - Client: `godot --headless --path client --import && godot --headless --path client -s tests/run_tests.gd`
   - A change to what the client ships (art, scenes, the export preset): `client/tools/export_size.sh` prints what the game weighs, to put in the pull request (D39).
6. The CI must be green. Pull requests that touch a **protected area**
   (`docs/`, the protocol, dependencies, storage schema, `.github/`, licenses)
   need the project lead's approval, given by the `lead-approved` label; the
   rest merges automatically once the CI passes.

Architecture rules that every change must respect are summarised in
[`CLAUDE.md`](CLAUDE.md) (written for AI-assisted sessions, but valid for
humans too).

## Licenses

By contributing you agree that your contribution is released under the
project's licenses:

- **Code** (`server/`, `client/`, `.github/`, build files): [AGPL-3.0-or-later](LICENSE).
  Anyone may use it, even commercially, but anyone who runs a modified
  version as a service must publish their modifications.
- **Lore, design documents, world content and artwork** (`docs/`, `content/`,
  and all art assets): [CC BY-NC-SA 4.0](docs/LICENSE). Non-commercial use
  only, with attribution and share-alike. The name *Soul Invasion* and the
  identity of the game belong to the project lead.

The split is deliberate: the engine is free for all; the world is ours to
share, not to sell.
