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

1. **The skeleton**: a character walking in the valley; client and server connected; adjustable time.
2. **Surviving alone**: gathering, crafting, fire, cold, hunger, spoilage, hunting, predator, death.
3. **The living tribe**: NPCs with needs, Willpower and a routine around the fire.
4. **The bonds**: opinions, prestige, intentions, imitation, banishment, strangeness.
5. **The life cycle**: couples, births, childhood, aging, choice of body, rebirth, soul points.

## Feasibility

- Feasible: server, connection, physical systems, animals.
- Hard: **NPC credibility**, **game feel**, **interface**. These require play and human adjustments.
- Order of magnitude: the first milestones in weeks, the full version in months, depending on the time available to test and decide.
