# 05 — First Version: "The Tribe"

## Single objective

Find out whether **living, crafting and dying among a tribe of NPCs is fun**.

Success criterion: after one hour of play, you want to keep going.

## Content

**The place**
- A small, dense valley: forest, river, rocky cliff, the camp clearing.

**Time**
- Day and night (about 2 real hours per day). The night is cold, dark, dangerous.
- Adjustable time, accelerated mode for testing.
- No seasons.

**The tribe**
- About fifteen NPCs: name, age, character, Willpower, family ties.
- Needs: hunger, cold, fatigue.
- Routine: gather, hunt, tend the fire, sleep by the fire.
- Opinions and memories.
- A chief and elders (the tribe is a group). **No seizure of power** by the player in this version.
- **Life cycle**: couples, pregnancy, birth, childhood, aging, natural death. Food limits births.

**Incarnation**
- The player chooses **any body** in the tribe. Each body has its history, its abilities, its situation (and therefore its natural goals).
- They inherit a few memories (names, ties, places).

**Actions**
- Walk, pick up, carry.
- Gather, collect branches, stones, fibers.
- Knap stone, make cordage, assemble a tool.
- Fell a tree, make and tend a fire, build a shelter.
- Hunt, butcher, eat, sleep, store.
- The body's skills improve with practice.

**Materials and objects**
- Materials: flint, pebble, branch, log, fibers, hide, meat, berries, bone.
- Objects: sharp flake, cordage, spear, stone axe, shelter, fire.
- Tools **made of parts**; computed qualities.
- **Freshness and spoilage** of food, perceptible by the player.

**Discoveries** (through experimentation and observation)
- Smoking meat (preservation).
- Sewing hide clothing with a bone needle (cold).
- Setting traps (small game).

**Animals**
- One prey and one predator, species specific to the world.

**Relationships**
- Intentions toward NPCs: offer, help, ask, threaten.
- **Prestige** = sum of opinions; banishment possible.
- **Imitation**: NPCs adopt the techniques they see.
- Minimal **strangeness**: NPCs find sudden changes odd (without the concept of the Changed).

**Death and rebirth**
- Death by hunger, cold, injuries, predator.
- Rebirth in another body of the tribe, with simple soul points.
- The tribe remembers who you were.
- **If the tribe disappears, the test world is over.**

**Technical**
- Godot client + local Rust server, placeholder graphics (readable, with a minimum of atmosphere).
- Content as data, event journal, adjustable time.

## Out of scope

Multiplayer, other tribes, seasons, agriculture, trade, religion, soul powers, heirs, seizure of power, politics beyond the chief.

## The five milestones

Each milestone is **playable** and **judged** before moving on to the next.

1. **The skeleton**: a character walking in the valley; client and server connected; adjustable time. — **Done and audited on 5 November 2026** (see below).
2. **Surviving alone**: gathering, crafting, fire, cold, hunger, spoilage, hunting, predator, death.
3. **The living tribe**: NPCs with needs, Willpower and a routine around the fire.
4. **The bonds**: opinions, prestige, intentions, imitation, banishment, strangeness.
5. **The life cycle**: couples, births, childhood, aging, choice of body, rebirth, soul points.

## Milestone 1 audit (5 November 2026)

What the skeleton is: a Rust server (`sim`, `protocol`, `server`), a Godot client, one valley of 32×32 tiles with a cliff, a river and a ford, three bodies, the first free one piloted by each connection. Time runs in fixed ticks (10 per game second, 7 200 game seconds per day), at any pace or headless. Snapshots and an event journal in SQLite. 59 server tests, 17 client checks, an end-to-end run with the real client.

**Performance** (release build, Intel i7-1260P, one core for the simulation):

| Scenario | Measured |
|---|---|
| Headless, nobody walking, 100 game days (7.2 M ticks) | 54 ms, 6.5 MB resident |
| Same with journal and daily snapshots in SQLite | 56 ms; 0.9 MB file (8.5 KB per snapshot) |
| 3 bodies walking without pause, one game day | 217 game days per real second |
| 30 bodies | 18 days/s |
| 300 bodies | 1.6 days/s |
| 3 000 bodies | 104 µs per tick, of a 100 ms budget |
| Path across the river, corner to corner (56 steps) | 5.5 µs; impassable or off-map target: immediate |
| Paced server idle | 6.5 MB resident, 0 % CPU, 4 threads |

The first version wants about fifteen NPCs: the budget is not a concern for a long while. These numbers are the baseline to compare against at the next audits.

**Dependencies**: 8 direct, 72 crates in the normal tree, no duplicate versions, licences and advisories checked by `cargo deny` on every commit. Most of the tree is tokio and tungstenite: the price of WebSocket.

**Security review of the listener** — found and fixed in the audit: silent TCP connections were kept forever (the handshake deadline only started after the WebSocket upgrade); no limit on message size (64 MiB by default); no limit on the number of connections; a client could flood intentions, each refusal being broadcast to everyone. Now: deadline on the upgrade too, 4 KiB messages, `--max-connections` (default 64), one pending intention per body per tick, refusals to the pilot alone. Each has a test.

**Left for later**, recorded in `06`: a client that stops reading holds its connection until TCP gives up (idle timeout or server-side ping); log lines per connection and per bad message can be flooded (needs a real logging layer); TLS and anything beyond `127.0.0.1` when multiplayer comes; retention of journal and snapshots.

## Feasibility

- Feasible: server, connection, physical systems, animals.
- Hard: **NPC credibility**, **game feel**, **interface**. These require play and human adjustments.
- Order of magnitude: the first milestones in weeks, the full version in months, depending on the time available to test and decide.
