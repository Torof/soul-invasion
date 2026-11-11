# Working rules for AI sessions on Soul Invasion

This file is read by every AI-assisted session. The project lead (Torof) is the
conductor: vision, decisions, practices, audits, security. The AI writes the code.

## Before anything else

1. Read `docs/README.md`, then only the documents the task touches: they
   are the reference, not a reading list. `docs/04-architecture.md` holds
   every architecture decision (D01…) with its reason; read the decisions
   the brief cites or the change concerns, and do not contradict one
   silently. `docs/08-midjourney-prompts.md` is for art sessions only.
2. Check `docs/06-open-questions.md` before inventing an answer to something
   that is still open: ask instead.
3. Context is the cost: keep tool output short (`--stat`, `head`, `grep`),
   batch commands, read images as contact sheets. An agent waits for its
   own background work with a blocking call and reports once, with a
   result; it never ends its turn "still waiting".

## Rules of the project

- **Nothing is built, coded or changed without explicit validation** from the
  project lead. Discuss first, decide, then build.
- A changed decision is updated in `docs/04-architecture.md` with its new
  reason, in the same pull request as the code that depends on it.
- **Everything is in English**: documents, code, comments, identifiers, commit
  messages. Conversation with the project lead may be in any language.
- `main` is protected. All work goes through a branch and a pull request.
  The CI must be green. Protected areas (see `.github/CODEOWNERS`) need the
  project lead's approval, given by adding the `lead-approved` label to the
  pull request; everything else merges on its own.
- **The AI never adds the `lead-approved` label, never approves a pull
  request, and never merges one by hand.** The AI works from the project
  lead's GitHub account, so this is a matter of trust, not of mechanics:
  the label is the project lead's decision and theirs alone.
- Never add a dependency, change the protocol, change a storage schema or
  touch `.github/` without saying so explicitly in the pull request.
- Hidden game rules (karma, the Emperor's tree, titles) are **content**, never
  code, and their real values live in a private repository (D30). This
  repository only ever holds placeholder values.
- No secrets, tokens or credentials in the repository, ever.

## Architecture in one breath

- `server/sim`: pure simulation, no I/O, no clock, no network, deterministic
  fixed ticks (D24). Most of the tests live here.
- `server/protocol`: the client/server wire contract, JSON over WebSocket
  (D25, D26). Mirrored by `client/net/protocol.gd`.
- `server/server`: the only crate that talks to the outside world. One
  simulation thread, async network beside it (D29). World snapshots in
  SQLite behind a `Storage` interface, written by a storage thread (D28).
- `client/`: Godot 4, GDScript. Displays and transmits intentions. Decides
  nothing (D03, D05).
- `content/`: RON data files read by the server only (D27).

## Conventions

- Commits: Conventional Commits (`feat:`, `fix:`, `docs:`, `ci:`, `refactor:`,
  `test:`, `chore:`), imperative mood, English.
- Rust: `cargo fmt` and `cargo clippy --all-targets -- -D warnings` must pass
  locally before opening a pull request, with the tests of what the change
  touches (its crate, or its test file: `cargo test -p sim`, `cargo test -p
  server --test net`, ...). The CI runs the whole suite on every pull request
  that touches the server, and nothing merges without it: running it all
  twice only made the project lead wait. Pedantic clippy is on. `unsafe` is
  forbidden.
- GDScript: typed, static analysis warnings fixed, tests in `client/tests/`.
- Tests: every rule of the simulation has a test; every protocol message has
  a round-trip test; scenario tests run the world accelerated and check
  invariants.
- Each milestone of `docs/05-first-version.md` ends with an audit: security
  review, performance measurement, dependency review, documents updated.

## Local toolchain

- Rust stable via `rustup` (`~/.cargo/bin`).
- Godot 4.7 (`godot` on the PATH). Client tests:
  `godot --headless --path client --import && godot --headless --path client -s tests/run_tests.gd`
