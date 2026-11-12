# 06 — Open Questions

What remains to be decided, with the moment it will need to be settled.

## To be settled soon (stages 3 and 4)

- ~~Project practices~~: decided on 5 November 2026 (public GitHub repository, AGPL-3.0 code, CC BY-NC-SA 4.0 lore and documents, CI with protected areas; see D30 and `CONTRIBUTING.md`).
- ~~Technical skeleton~~: decided on 5 November 2026 (D23 to D29).

## To be settled during the first version

- Exact rhythms of time (day, childhood, lifetime) after trials.
- **The length of the year** (13 November 2026): about 50 game days is the provisional value (D09 amended), which the project lead likes but still weighs, having first wanted it a little faster. It must be fixed **before a world opens**. The pace of discovery is decided with it. A measure proposed for it: attachment (an NPC friend of thirty lives about four more real months, enough to grow attached, watch them age and grieve them).
- **The vivarium's map**: 6 or 12 km a side, decided after measuring what a larger map costs the generator (D41).
- **The starting fauna** in its details (beyond the first deer herd of the vivarium): after the NPCs and their stories. Also to check: that an unseen valley takes account of what happened around it.
- **Caves, rock and stone** (13 November 2026): a direction, not a design. Rock shelters first, then true caves as roofed areas in the same map, like the inside of a house; a geology layer by region driving caves (limestone only), flint (chalk and limestone outcrops, river gravel), boulders and clay. Needs its own design page: how the layer is generated, how a roofed area is drawn and entered, what it changes for light, cold and shelter.
- ~~Size of the valley~~: 256×256 one-metre tiles to start (D31, 5 November 2026), 3072×3072 since 8 November 2026 (768×768 then 1536×1536 earlier that day), meant to grow much further; neighbouring valleys as regions later. Exact number of NPCs still open.
- **Floors** (D31): the first version builds on one level. Upper floors and cellars need a third coordinate in the grid, the rendering and the paths; decide when the first player wants a staircase.
- Simple rules for soul points.
- Form of the interface for tool assembly and intentions. First form settled with D35 (6 November 2026): hover names, right-click context menu, inventory panel; assembly still open.
- ~~Visual rendering~~: pure 2D isometric, images generated with Midjourney under a style reference (6 November 2026, D02 settled, `07` and `09`). Still open: the animation tool for bodies, and whether ground overlays are content placed by the server or decoration scattered by the client.
- ~~**Retention** of the event journal and snapshots (D28)~~ — decided 10 November 2026: no world journal (D17); the last three snapshots are kept, written every five minutes on the storage thread; a soul's deeds are kept until they unlock a title, power or skill.

## To be settled later

- **Detailed reincarnation**: point scale, penalties, distinction between abandonment and death suffered.
- **Soul powers**: list, counterparts, counters, progression. Each power has its own moment (12 November 2026), still to set for each. A final name for the *Dethroner*. Proposed on 12 November, not decided: being reborn into the heirs of a player one has unmasked (a soul hunter's power); **the Vampire**, a body that sleeps while its player is offline, hidden in a cave, which the world, if it finds it, burns, imprisons, watches or worships, and which wakes decades later into a changed world and a legend about it (open: **does the sleeping body age?**); powers that lengthen or strengthen plans.
- **Plans** (proposed 12 November 2026): about 2 game days at first, up to about 4 (a real night) with mastery after a few lives, a ceiling around 12 for the greatest souls, never a week; fading gently. To be revised.
- **Conditional plans** ("if it rains, stay at camp"): to be discussed again. Combat already forbids conditional commands, and a conditional plan quickly looks like a robot playing for you.
- **Mastery in two parts** (proposed): the soul's own, gained slowly from life to life, and **the bond with the body**, which grows with the time spent in it (an old warrior long inhabited obeys better than one taken yesterday).
- **The screen form** of the plan, the journal, the book of the soul and the ghost.
- **Karma for acts done while the soul is absent**: when the body follows its plan or its own life, offline or after the plan faded, do its acts weigh on the soul's karma, and how much?
- **Rebirth only into a body without a soul** (proposed): another player's king stays theirs.
- **No in-game space where souls talk** (proposed): it would unmask the players. A forum outside the game is no problem.
- **What the player must see of who knows what** (proposed): the source in every bubble; asking "who told you?"; one's own rumour coming back changed; behaviours that change.
- **How far NPC-only regions advance**: should a region without players reach villages within a century, or stay close to the Stone Age?
- **The pieces of a society** (proposed, welcomed on 12 November 2026): recognition ("he is chief") is a belief; obedience is each one's choice (fear, loyalty, interest, habit, the wish to leave); a group's rules are data (who proposes, who approves, at what threshold), never applied automatically; property exists because others recognise it, with no lock; the freedom to leave, and the **cost of leaving**, which separates a kingdom from a free band; different lands; NPCs imitating people of prestige, so local customs drift apart.
- **Towards the kingdom** (the kingdom study, 12 November 2026): the institution as an object distinct from its founder, which outlives them (the greatest risk); three brakes from the start (distance, the consent of the counted people, death and a moving density); the polity sheet above communities; a finite list of civil project types the rules can read; pruning the dead. Open as well: a kingdom of about 3,000 people has the shape of a kingdom, not its population, unless peoples are larger; whether to build a mock-up chronicle first.
- **What the soul carries: karma, Willpower, prestige** (raised 5 November 2026). All three are consequences of acts, at different levels and with different effects. Karma belongs to the soul: hidden, it judges what was done across lives and shapes powers, titles and rebirth (D30). Prestige belongs to the body: what the world thinks of this inhabitant (D7), dying with it or passed on in part by heirs. Willpower is the body's (D12) — and may also be the soul's: the strength with which it holds its body, resists, persists; a strong soul in a weak body, or the reverse. Effects to settle: karma → what the soul *may* (powers, limits, price); the soul's Willpower → what it *holds*; prestige → what the world *allows* it (to follow, obey, banish). Strengths and weaknesses from their imbalance. To decide with soul points and reincarnation.
- **Bloodline Bond**: counterparts, limits (distance, extermination, bond-breaker).
- **Karma**: what is measured, without allowing it to be gamed.
- **Seats**: conditions for becoming a Demon Lord or the Emperor; the Emperor's hidden tree; extended lifespan.
- **Starting state of the world**: number of tribes, relations, beliefs.
- ~~**Seasons** and winter~~: the year and its seasons decided on 13 November 2026 (D09 amended, D41); the length of the year above.
- **Seizure of power** in the tribe (strength and prestige).
- **Intuitions** of skilled characters.
- **AI-generated speech** for NPCs, then free conversation.
- **Multiplayer**, hosting, costs, moderation. Before exposing the server beyond `127.0.0.1`: TLS, an idle timeout or server-side ping for clients that stop reading, rate-limited logging (milestone 1 audit).
- **End of the world** and nihilists: how hard it should be.
- **Final name** of the game (check the availability of "Soul Invasion") and name of the world.
- **Origin of the souls** (the great mystery).
- **Age rating** and treatment of dark themes.
- Participatory **business model**.
