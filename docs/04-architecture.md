# 04 — Architecture Decisions

Each decision is recorded with its **reason** and its **consequences**. These are the foundations: getting them wrong would cost rewrites.

---

### D01 — Platform: PC first
- **Reason**: rich interface (chats, maps, management), online community, simple distribution (Steam, early access), freedom to update. Consoles: access barrier (dev kits, certifications); mobile: screen too small, wrong audience.
- **Consequences**: also playable with a controller from the start (Steam Deck, a Switch port possible one day).

### D02 — Visual style: isometric
- **Reason**: readability of towns, roads, constructions and armies; a natural grid for building.
- **Consequences**: choice between pure 2D and light 3D with a fixed camera and painted rendering, to be settled according to the graphics solution (probably AI). Visual consistency is the main challenge.
- **Settled on 6 November 2026: pure 2D isometric**, a 2:1 diamond grid of one-metre cells (64 × 32 px at scale 1), the ground from top-down textures projected and blended by the engine, elements as flat images standing on it, relief later as whole levels. A relief ground in 3D was weighed and declined: it buys continuous hills and costs the grid everything else rests on. The format every image obeys is in `09-visual-production.md`; the style in `07-style-bible.md`.

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
- **Status** (5 November 2026, step 1b): built at chunk granularity. Each connection holds a view of 5×5 chunks (160 m across) around its body; it receives those chunks with the bodies in them, then only the moves inside them; as the body walks, chunks entering the view are sent and chunks leaving it are hidden, and a body walking out is reported as left. Perception by line of sight, hearing and witnesses (D18) will refine this inside the view.

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
- **Status** (5 November 2026, milestone 2): first content files, `content/materials.ron` (family, mass, hardness, flammability, food value, provisional colour) and `content/spawns.ron` (what lies on which terrain, in pieces per thousand tiles). The valley is scattered from them when a world is created; items are world state, in the snapshot (format v4). Parts, processes and computed qualities come with crafting.

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
- **Consequences**: encoding is confined to a single module on each side, so we can switch to binary the day it matters without touching the rest. Every message carries a type and a protocol version. The Rust types in `protocol` are the reference; the client mirrors them. **Shape of the dialogue** (D05, D08, D20): client → server, intentions only; server → client, an initial state of what the body perceives, then changes. The client never receives the whole world: the ground travels in chunks around the body (D08, D31).

### D27 — Content in RON, read by the server only
- **Reason**: content (D15) is made of nested structures and categories (materials, parts, processes). RON is Rust's native data format: it expresses them directly, accepts comments and is checked at load time. The client does not need to read it: it receives the names and visual identifiers from the server at connection.
- **Consequences**: hidden rules stay on the server effortlessly (D05). Content is validated at server startup: an inconsistent file prevents launch; a CI test loads all the content at every commit.
- **Status** (5 November 2026, milestone 2): built. `sim::content` parses and validates (unique `snake_case` ids, resolved references, sane values); `server::content` is the only place files are read, from `--content <DIR>` or the first of `content`, `../content`, `../../content`. The server prints what it loaded at startup.

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

### D31 — Spatial scale: one tile is about one metre, on a single continuous grid
- **Reason**: cities must be able to grow from one hut to walled towns built by players, and the player must enter a house and see its rooms. That needs the tile to be the smallest thing that takes room: a body, a door, a wall segment, a bed, a fire. A hut is then 3×3 tiles, a house with rooms 6×8 to 10×12, a rampart one tile thick and as long as its builders' patience. Interiors stay on the same grid rather than opening a separate map: a house is walls on tiles, rooms are what walls enclose, the roof is hidden when a watched body is inside. This keeps everything physical: fire spreads, sound carries, witnesses see through a door (D18), a stone rampart is as many demolition sites as it has segments, and no system needs a special case for "door". Separate maps cost the same storage and bandwidth (the tiles exist either way); they save rendering complexity and lose coherence. A region (D07) is what a separate map is for: caves, other worlds — not houses.
- **Consequences**: the first valley is 256×256 tiles (about three minutes of walking across). A walking step takes about 0.7 s. The grid is cut into **chunks of 32×32 tiles**, the unit of storage, of transfer to clients and of interest (D08): a client receives the chunks around its body. Terrain is **generated from parameters** and never stored tile by tile; what changes the world is objects placed on tiles, through the journal (D17). One level only for the first version; floors are an open question (`06`). Rendering draws only the visible chunks; the client needs a camera (zoom, pan). Path searches on a 256×256 grid are a few milliseconds at worst; hierarchical paths come with many NPCs (D10, D11).

### D32 — Walking in eight directions, with weighted steps
- **Reason**: with only four directions every oblique walk is a staircase, visible at one metre per tile and worse once animals flee and hunters chase. The movement model is a foundation for routines, hunting and carrying: changing it later means retesting all of them.
- **Consequences**: paths use A* over eight neighbours; a diagonal step costs √2 straight ones (10 and 14 in path units) and is never allowed to cut a corner between two impassable tiles. Ties break deterministically.
- **Amended on 5 November 2026 — fine positions and taut paths**: eight directions still make every oblique walk a staircase, and finer grids only make finer staircases. So a body stands at a **fine position** (millimetres) inside the one-metre grid, which stays the unit of everything else (items, buildings, occupation, perception, the eight-direction facing of combat). A walk is planned on the grid, **weighed by the content's cost of each terrain** (`content/movement.ron`: a road is cheap, a forest dear, water impossible), then **pulled taut** into straight segments that cross only tiles no dearer than those they replace — so a road is taken when it is worth the detour and never short-cut through the field beside it. The body advances along the segments every tick at the terrain's speed; it crosses tiles and stops exactly on the target's centre. Clients receive the route once (`body_walks`) and interpolate, instead of one message per metre. Paths cost the same as before; hierarchical paths still come with many NPCs.

### D33 — Combat: slow real time by intentions, exchanges at a tempo, wounds instead of hit points
- **Reason**: this is a game of lives and consequences, not a fighting game. Combat must be deliberate and rare, quick and lethal, heavy with consequences (witnesses, opinions, karma), and the same system for everything that hurts: a predator, a fall, an assassin. It must also be *played* between two players who think and react, and scale to a battle of a hundred soldiers.
- **Consequences**:
  - **Tempo**: an engaged body commits one **gesture** per tempo (about a game second, slowed by fatigue and wounds). Between tempos the player chooses the next gesture; with no choice, the body follows its **guard**. Players are not asked for reflexes but for reading and choice; network latency vanishes behind a second.
  - **Gestures and guards**: strike, feint, parry, dodge, shove, break off — each with a direction and, for strikes, a target limb; three guards (offensive, closed, evasive). They answer each other like a game of reading (a feint beats a parry, a shove beats a parry, a dodge beats a shove), with uncertainty from skill, condition and equipment.
  - **Simultaneous resolution, moves hidden**: the gestures of every engaged body resolve together at the tempo. One sees the opponent's guard and, during the tempo, the **wind-up** of the gesture — late and ambiguous from a skilled fighter, plain from a novice; a feint shows a false wind-up. All of it is computed in the simulation from the content (D15): the weapon's reach, mass, edge and balance from its parts; the protection worn by limb; the body's skill with that family of weapon, learnt by practice; its condition (fatigue, wounds by limb); its facing (eight directions, D32), the measure (reach strikes first as distance closes), the ground; and the number of attackers, each beyond the first taking from the defence.
  - **No hit points**: a hit gives a wound to a limb (light, grave, mortal) that bleeds, hampers, heals over days and scars. Vitality (D34) is what hunger, cold, blood loss and fatigue drain. Death comes from a mortal wound or from vitality at zero. A dead body is a corpse: an item on the ground, like an animal's.
  - **Assassination is combat against someone who does not know**: asleep, from behind, in the dark, or trusting — no parry, a first blow almost always mortal. Perception (D18) and awareness decide it.
  - **War is the same system**: a hundred engaged bodies are a hundred resolutions per game second. Lines, flanking and breaches emerge from positions and facing; orders are group intentions (D19); morale (Willpower, fear, seeing neighbours fall) makes bodies break and run; fatigue wears a line down. There is no battle mode and no arena instance: an arena is a place.
  - **The engagement window** on the client is a *view* of the same world, not an instance: the camera zooms on the engaged bodies and a panel shows both silhouettes — the opponent's is the attack command (click where you strike), one's own the defence command — while the map stays visible around it, so one sees who comes from behind and can break off and flee across it. Commands are **composable by the player** from the full grammar the server accepts (a default panel of a few, more granular ones to add, keys to assign), but **never conditional**: one intention per tempo, no "if he strikes left, parry left" — that is an NPC brain, and a player who has one is a bot. NPCs use exactly the same grammar.
  - Built with the animals (milestone 2), tried against the predator and in the hunt; witnesses, morale and orders with milestone 4. Tempo length, the gesture set and every number are content.

### D34 — The state of a body: vitality, needs, wounds by limb
- **Reason**: needs, combat, animals, illness and aging all act on the same body and must agree on what hurts and what kills, before any of them is built.
- **Consequences**: a body has a **vitality** (0–100), three **needs** (hunger, cold, fatigue, 0–100), a flag for sleep, and **wounds by limb** (head, torso, arms, legs: none, light, grave, mortal; bleeding or not). Needs rise with time and fall with what relieves them (food, warmth, sleep); at their maximum they drain vitality; a full fatigue forces sleep. Vitality regains slowly when nothing drains it. Death comes at zero vitality or from a mortal wound; the body becomes a corpse item and its pilot is told. Rates and thresholds are content (`content/needs.ron`). Wounds (D33) reduce what a limb can do; until combat exists they are carried empty. Clients see only their own body's gauges.
- **Death and reattachment, provisionally**: until milestone 5 brings the choice of body and rebirth, a pilot whose body dies takes the first free body, or watches if none is free.

### D35 — Objects and carrying: one place at a time, a hand and a load in kilograms
- **Reason**: picking up, carrying, laying down, eating, crafting, fighting and trading all move objects between bodies and the ground; they must agree on where an object is and what a body can hold, before any of them is built. Tools and weapons are *held*: what is in a body's hand is seen by all.
- **Consequences**: an item is in **exactly one place** at a time — on a tile, carried by a body, or (later) in a container or a structure; never in two, never nowhere. A body carries in two ways: **in hand**, one item, visible to all and the tool or weapon of combat (D33); and **on it**, the rest, a shapeless bag for now. Capacity is in **kilograms** from the materials' masses (`content/movement.ron`: 20 kg to start), not in slots; the walk slows with the load. Acts are intentions within a tile's reach: pick up (into the empty hand, else carried), hold, stow, drop on one's tile, eat from the ground or from what is carried; every act wakes. The **server judges, the client proposes**: the client composes its menu from what it knows, the server refuses what cannot be done and says why. A dead body drops everything. Containers, loads carried by two, objects laid on a table or a stall fit the one-place rule without change.
- **Interface, first form** (decision 6): names on hover; a context menu on right-click (without dragging) on a tile within reach; an inventory panel listing the hand first, with masses and the load. Keys remain as shortcuts.


### D36 — Water as a volume: a river carries a flow that sets its width and depth
- **Reason**: a tile is about a metre (D31), so a tile of water half a metre deep is a half-cube, and a river is a number of half-cubes passing: its **flow**. One number then explains everything the banks need: two waters that meet become one, wider or deeper, never two rivers crossing; a river that parts around an island shares its flow between the arms; a ford is where a river spreads until it is half a metre deep right across, which only a small flow can do; a brook is the same water with a tiny flow. Width, depth, fords, islands, brooks and junctions stop being separate rules with separate dials, and later systems (the current, wading, drowning, boats, fishing, the sea at the end of the network) will read the same depth. It costs almost nothing: the flow is computed once at generation and the depth stored once per tile; nothing flows at every tick.
- **Consequences**:
  - The map keeps a **depth per tile in half-metres** (0 on land, 1 on a ford, up to 8 in a river), regenerated with the terrain, not stored. Half a metre is crossed easily; a metre can be waded, slowly and without a load, when movement in water is built (a content cost per depth, not code); deeper, one swims or drowns.
  - The valley's waters form a **tree**: a main river from the cliff's foot to the south edge; tributaries from the edges; **brooks** that rise where the ground is wet (a noise of wetness, like the groves; its average is a dial of the generator now and becomes a datum of each region once regions exist, D07) and run to the nearest water, river or brook. Every water ends where it meets another; later, the main river reaches a sea.
  - The **section follows the flow**: `width × depth` covers the flow, a stretch being either *wide* (width about √(2 × flow)) or *deep* (about √flow), the choice drawn once per stretch between junctions. Water **one or two tiles wide is a brook** (`Terrain::Brook`, a metre deep at most, walked through); from three wide it is a river, within 3 to 12 tiles wide and up to 4 m deep; past that it only widens.
  - A **ford** is a lens where a river spreads to half a metre right across, so it is always wider than the river around it; only a flow of 12 half-cubes or less can do it, upstream of the junctions that make a river too strong. A river that could have a ford has one **by chance** (a dial, one in two to start), never by right: a ford is a place the terrain gives, rare enough to matter, and a bank without one is a reason for a bridge or a boat. Nothing is forced; a part of the map may wait for them. Only the starting ground is checked to be large enough.
  - A **divergence** splits a river in two arms around an island, from a rock to a large island; the smaller arm, when shallow enough, shoals into a ford, which is how one reaches the island.
  - **The banks breathe** (7 November 2026): a river with both banks parallel for tens of tiles looked drawn with a ruler, and a real bank is rarely straight for long: the two banks drift apart or together, or one bends alone and the river widens there. So each bank of a river, on its own, holds its line for 4 to 12 steps, then turns a quarter of a step of heading (about 5.6 degrees) out or in for 12 to 40 steps and comes back, so a river widens and narrows slowly (a whole step, 22.5 degrees, then half a step, made the river look misshapen: the lead's verdicts the same day); it stays between about a tile inside and six tiles outside its line (three until the lead asked for wider pools, the same day). Both out is a pool, both in a narrows, one out and one in the river sliding sideways, one alone a bend on one side. The width stays from 3 (never a brook) up to 18 for a **pool** (past the 12 of a river's own section) and the **depth takes what the width leaves**, so the flow is kept: a narrows is deeper, a pool shallower, but **never less than a metre**. A pool wider than its flow can fill a metre deep is slower water holding more than its flow, as in a real basin, rather than a shoal: it never becomes a ford by accident (a ford stays a place chosen as such). The banks hold their lines near junctions, fords, divergences, the map's edges and on brooks, and a bank never bends within a step of a bend of the centre path, so a bank never turns by more than one step at once. The centre path also bends more often: a heading is held 4 to 10 steps (was 6 to 14).
  - The rules of the shore stay: straight runs in sixteen headings, a bend is a piece, a bank never cuts through a tile. A breathing bank at a quarter of a step is laid by the client as straight pieces with a one-tile jog now and then, so the bank pieces already made cover it. Brooks have no bank pieces: the client draws them as a line on the ground.
  - The client receives `Water`, `Ford` and `Brook` (protocol v9); depth reaches it when something is drawn from it. Every number here is a dial of the generator, to be tuned by looking at maps; the mechanism is the decision.
