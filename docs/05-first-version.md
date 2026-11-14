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
   - **1b. The scale** (added 5 November 2026, D31 and D32): the same walk in a valley of 256×256 one-metre tiles, cut in chunks sent around the body, eight-direction paths, a camera on the client. Decided before milestone 2 so that nothing is built on tiles whose meaning would change.
2. **Surviving alone**: gathering, crafting, fire, cold, hunger, spoilage, hunting, predator, death.
3. **The living tribe**: NPCs with needs, Willpower and a routine around the fire.
4. **The bonds**: opinions, prestige, intentions, imitation, banishment, strangeness.
5. **The life cycle**: couples, births, childhood, aging, choice of body, rebirth, soul points.

## Order of construction and decisions (5 November 2026)

Some things rest on others, so neither the building nor the deciding can
happen in any order. This section is the order we follow and amend; each
step names the decisions it settles, which are discussed and recorded in
`04` **before** the code that depends on them.

**Already settled**: time (D24), network (D25, D26), persistence (D28),
spatial scale (D31), movement (D32), content as data (D15, D27), views by
chunk (D08).

**Decisions still to take, in the order they block**

1. **The state of a body** — vitality, needs (hunger, cold, fatigue), wounds
   by limb. Everything rests on it: needs, combat, animals, aging.
2. **Combat** (D33) — tempo, gestures and guards, simultaneous resolution,
   wounds, the engagement window, composable commands without
   conditionals. Converged on 5 November 2026; written with step 3.
3. **Death and provisional reattachment** — the soul takes the first free
   body until milestone 5 brings the choice of body and rebirth.
4. **Objects and carrying** — what an item is in hand, carried, laid down;
   a tool made of parts (D15). Settles the weapon model for combat and the
   crafting model.
5. **Perception** (D18) — line of sight, light, sound, awareness. Founds
   vigilance in combat, assassination, witnesses, fleeing animals.
6. **The interface of intentions** — the context menu on a tile or item;
   the engagement window. With 4 and 2.
7. **The NPC brain** (D12) — needs to decisions, the same grammar of
   intentions as players. Before the tribe.
8. Groups (D19), prestige and witnesses — milestone 4. Soul points and
   reincarnation — milestone 5. Retention of the journal — before long
   games.

**Milestone 2, pull request by pull request**

| # | Pull request | Settles |
|---|---|---|
| 1 | Content in data: materials, spawn rules, items on the ground — **done** (#16, #17) | — |
| 2 | Fine movement, terrain costs — **done** (#17) | D32 amended |
| 3 | The state of a body and its needs; night; eating off the ground; sleeping; death and reattachment | 1, 2 (D33 written), 3 |
| 4 | Pick up, carry, lay down, eat; inventory; context menu; names on hover | 4, 6 |
| 5 | Crafting by assembly: parts, processes, computed qualities | 4 (continued) |
| 6 | Fire and shelter: warmth, cooking, smoking; felling, building | — |
| 7 | Perception: line of sight, light, awareness | 5 |
| 8 | Animals and the core of combat: tempo, gestures, wounds, the engagement window; hunting, the predator | 2 (code), 6 (window) |
| 9 | Audit of milestone 2 | — |

Perception comes before the animals because too much rests on it to come
after; combat comes with the animals, not after them. Each pull request
ends with a run of the real client; the milestone ends with an audit.

**Milestone 2 with the vivarium (13 November 2026)**

The project lead decided on 12 and 13 November that the rest of milestone 2
goes through the vivarium (D41): NPCs alone, from the Stone Age, under
glass. It replaces the order of the table above where they differ. It also
brings the year and its seasons into this version (D09 amended): "No
seasons" above, and seasons out of scope, no longer hold.

1. **The documents**: this record of the decisions (D41 and the amendments
   of 13 November).
2. **Eating and drinking**: the item model in grams, seasons and
   temperature, thirst, the dated flora that regrows, fruit years, the fat
   reserve, carrying without a container; what the plants and animals give;
   measuring what a larger map costs.
3. **The simple fire**: lit, kept by adding wood, warming and cooking, with
   the structures on tiles and dead wood that returns.
4. **Vivarium V1**: one band of about forty, strength and kinship, a
   minimal perception, the brain and its memory of places, the rock shelter,
   the camp, **a small deer herd**, the program and its report.
5. **Assembly, containers, built shelters.**
6. **Perception**, complete.
7. **Hunting** (V2): the deer herd that feeds, flees and breeds; the spear,
   alone or together.
8. **Knowledge and discovery** (V3), after the design page and the full
   model of processes.
9. **Society** (V4): opinions, prestige, a recognized chief, strong
   personalities and awakenings.
10. **Two tribes** (V5), the meeting, and the judgement on forty seeds and a
    second map.

Each step ends with something the project lead can read in the report. The
body rules hold for the game too, so the client's share of each step is
counted with it. Images of rocks and caves come only once their shape is
validated with placeholders.

Beside the vivarium, **the mountain** (decided 13 and 14 November 2026)
is built in three pull requests: the heights in the chunks (D43), the
slopes drawn by the client from them (D44), then the relief in the
generator.

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
