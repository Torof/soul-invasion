# 04 — Architecture Decisions

Each decision is recorded with its **reason** and its **consequences**. These are the foundations: getting them wrong would cost rewrites.

---

### D01 — Platform: PC first
- **Reason**: rich interface (chats, maps, management), online community, simple distribution (Steam, early access), freedom to update. Consoles: access barrier (dev kits, certifications); mobile: screen too small, wrong audience.
- **Consequences**: also playable with a controller from the start (Steam Deck, a Switch port possible one day).

### D02 — Visual style: isometric
- **Reason**: readability of towns, roads, constructions and armies; a natural grid for building.
- **Consequences**: choice between pure 2D and light 3D with a fixed camera and painted rendering, to be settled according to the graphics solution (probably AI). Visual consistency is the main challenge.

### D03 — Client: Godot (GDScript)
- **Reason**: free under the MIT license with no revenue condition, suited to 2D and stylized 3D, mature, readable text files (efficient with AI).
- **Consequences**: the client only **displays** and **transmits the player's intentions**.

### D04 — Simulation server: Rust, custom-built
- **Reason**: performance for a continuous simulation of tens of thousands of entities; a strict compiler that acts as a guardrail for AI-written code; total control (accelerated time, granularity). SpacetimeDB ruled out: non-free license, designed for queries rather than a heavy continuous simulation.
- **Consequences**: two languages at the heart of the project. Side services (accounts, website, tools) possibly in TypeScript.

### D05 — The server is the only source of truth
- **Reason**: anti-cheat, a world that lives without connected players.
- **Consequences**: the client decides nothing. The hidden rules (the Emperor's tree, karma) exist **only on the server**, never in the installed game files.

### D06 — Simulation separated from display, from day one
- **Reason**: going online must never require a rewrite.
- **Consequences**: even the first version runs as a Godot client connected to a local Rust server.

### D07 — A single persistent world, divided into regions
- **Reason**: wars, convoys and politics require a continuous world; locality (like EVE's solar systems) allows the load to be spread.
- **Consequences**: at first, everything fits on one machine; the region structure allows spreading later. If the population demands it, we open **another world** rather than expanding indefinitely.

### D08 — Interest management
- **Reason**: each player only receives what happens around them (performance, network).
- **Consequences**: reinforces the principle that information travels physically.
- **Status** (milestone 1 audit, 5 November 2026): not implemented yet. `world_state` sends the whole valley once at connection (32×32 tiles, every body), and every move is published to every client. Fine for one valley and a tribe; to be built before several regions (D07) or many players.

### D09 — Adjustable time and a calendar specific to each world
- **Reason**: the world has its own calendar and its own biology; testing requires massive acceleration.
- **Consequences**: durations (day, season, childhood, lifetime) are adjustable independently and defined as world data. An accelerated mode without players is **mandatory** from the start.

### D10 — Levels of detail in the simulation
- **Reason**: simulating everything in fine detail everywhere is impossible.
- **Consequences**: full detail near players, summarized simulation elsewhere, bulk simulation far away, with a return to the individual level when a player arrives. States (memories, opinions) are never lost.

### D11 — Target: several tens of thousands of NPCs
- **Reason**: a living, populated world.
- **Consequences**: data organized for performance; thinking **spread over time**; optimized movement (precomputed paths, no pathfinding for distant NPCs); **relationships limited to known people** (~150); old memories summarized.

### D12 — Willpower sets the richness of NPC thought
- **Reason**: lore, reincarnation cost and performance brought together in a single notion.
- **Consequences**: Willpower sets **how far** an NPC thinks; proximity to players sets **how often**.

### D13 — A single type of inhabitant; the soul is a pilot
- **Reason**: reincarnation must feel natural.
- **Consequences**: no code difference between player and NPC at the body level. A body is piloted either by the AI or by a soul.

### D14 — Souls live above the worlds
- **Reason**: soul migrations, multiple worlds, the possible end of a world.
- **Consequences**: soul data is stored in a separate service, independent of any world.

### D15 — All content as data
- **Reason**: technologies without an imposed tree, invention by assembly, evolution without recoding.
- **Consequences**: materials with properties, parts, processes, composite objects whose qualities are **computed**. Never a recipe hard-coded in the code.

### D16 — Object states computed on demand
- **Reason**: spoilage, wear, preservation for millions of objects.
- **Consequences**: the state is computed at the moment it is consulted, from the date and the history (smoking, cold…), not updated continuously.

### D17 — Permanent event journal
- **Reason**: the world's memory, historical merit, investigations, karma, NPC memories.
- **Consequences**: significant events kept forever; fine details kept temporarily. Volume to be monitored.

### D18 — Witnesses and perception
- **Reason**: crimes, investigations, rumors, suspicion, justice.
- **Consequences**: every action has potential witnesses; every witness keeps a memory of it. No omniscient information.

### D19 — Generic group system
- **Reason**: all of society (tribe, guild, religion, empire) rests on it.
- **Consequences**: members, roles, powers, rules, succession, hierarchy, relations, visibility. The tribe of the first version is already a group. Role powers are **social recognitions**, not locks.

### D20 — Interactions through intentions
- **Reason**: to allow generated speech and free conversation later without rebuilding anything.
- **Consequences**: menus, gestures or free sentences all translate into intentions handled the same way by the NPC's brain.

### D21 — Versioned data
- **Reason**: to evolve the world and migrate souls without breaking everything.
- **Consequences**: every save format is versioned and migratable from the start.

### D22 — Evolving security and balance
- **Reason**: like an ecosystem, balance is built through iterations.
- **Consequences**: no large initial anti-cheat system beyond server authority; we observe, we adjust.

---

*Technical skeleton decisions, approved on 5 November 2026.*

### D23 — A single repository (monorepo)
- **Reason**: client, server, content and documents evolve together; one version, one history, one CI.
- **Consequences**: structure `docs/` (this corpus), `server/` (Rust), `client/` (Godot), `content/` (world data).

### D24 — The simulation is a pure library, with no network and no clock
- **Reason**: it is the most important piece (NPCs, Willpower, needs) and the hardest to get right. Isolated, it is tested in milliseconds, runs in accelerated mode without a client (D09) and depends on no network technology.
- **Consequences**: three Rust crates from the start. `sim`: the world, the NPCs, the rules; no input/output; receives intentions and a number of ticks, returns events. `protocol`: the client ↔ server message types, in a single place. `server`: network, persistence, time loop; the only crate that talks to the outside world. **Fixed tick** (order of magnitude: 10 ticks per second of game time); acceleration only changes the tick rate, never the rules. Same inputs, same result: essential for tests and for replaying a bug.

### D25 — Transport: WebSocket
- **Reason**: native in Godot 4 (`WebSocketPeer`), mature in Rust (`tokio-tungstenite`). Goes through everywhere (firewalls, proxies), trivial TLS later, opens the door to a web client and to administration tools in a browser. The game is slow: no need for UDP.
- **Consequences**: no high-level Godot multiplayer API. The client has a single connection, a single network module.

### D26 — Message encoding: JSON, behind a single door
- **Reason**: readable in logs, parsed natively by Godot, trivial to debug. The volumes of the first version are small.
- **Consequences**: encoding is confined to a single module on each side, so we can switch to binary the day it matters without touching the rest. Every message carries a type and a protocol version. The Rust types in `protocol` are the reference; the client mirrors them. **Shape of the dialogue** (D05, D08, D20): client → server, intentions only; server → client, an initial state of what the body perceives, then changes. The client never receives the whole world — except, until D08 is built, the single valley of the first version (see D08's status).

### D27 — Content in RON, read by the server only
- **Reason**: content (D15) is made of nested structures and categories (materials, parts, processes). RON is Rust's native data format: it expresses them directly, accepts comments and is checked at load time. The client does not need to read it: it receives the names and visual identifiers from the server at connection.
- **Consequences**: hidden rules stay on the server effortlessly (D05). Content is validated at server startup: an inconsistent file prevents launch; a CI test loads all the content at every commit.

### D28 — Persistence: event journal + snapshots, SQLite first
- **Reason**: the simulation lives in memory; we persist the event journal (D17) and periodic snapshots. SQLite is a single file, with no server to install, sufficient for a tribe and a valley.
- **Consequences**: storage goes through a single interface (`Storage`); replacing it with PostgreSQL only touches that module. Schema and snapshots versioned with migrations from day one (D21). Soul data lives in a separate file, behind its own interface (D14).
- **Status** (milestone 1 audit, 5 November 2026): schema v1, snapshot format v2 (calendar, tick, map, bodies), one snapshot per game day plus the journal. Nothing is pruned yet; the retention policy is an open question (`06`).

### D29 — One server, one simulation thread
- **Reason**: the simulation is deterministic and sequential, the network is asynchronous; mixing them creates the hardest bugs there are.
- **Consequences**: the simulation loop runs on a single thread and receives intentions through a queue; the network (tokio) runs alongside and only translates. The split into regions (D07) will come later with several loops of this kind.
- **Precision** (milestone 1 audit, 5 November 2026): the queue holds **one pending intention per body**, the latest wins, taken before each tick. A client flooding `move_to` costs one path search per tick and nothing to anyone else. Refusals go back to the pilot alone; moves are published to all. The listener also bounds what the outside can cost: a handshake deadline, a maximum message size, a maximum number of connections (`--max-connections`).

### D30 — Public repository on GitHub (`Torof`), code under AGPL-3.0, lore and documents under CC BY-NC-SA 4.0
- **Reason**: a participatory project, open to ideas and new people. Two protections are wanted: nobody must be able to appropriate the project to turn it into a closed commercial product, nor take it "their own way" under the same name. The AGPL forces anyone who runs the code (even as an online service) to publish their modifications; CC BY-NC-SA forbids commercial use of the world, the lore and the documents, and requires share-alike. The game's name and its identity remain the property of the project lead.
- **Consequences**: the hidden rules (karma, the Emperor's tree, titles) are **content**, in a second **private** repository loaded by the production server; the public repository contains dummy values so that everyone can compile and play. This private repository is created the day the first hidden rule appears. The project lead keeps the final say on what goes into the project (`CONTRIBUTING.md`). Contributions are accepted under the same licenses.
