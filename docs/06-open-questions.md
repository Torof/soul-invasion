# 06 — Open Questions

What remains to be decided, with the moment it will need to be settled.

## To be settled during the first version

- Exact rhythms of time (day, childhood, lifetime) after trials.
- **The length of the year** (13 November 2026): about 50 game days is the provisional value (D09 amended), which the project lead likes but still weighs, having first wanted it a little faster. It must be fixed **before a world opens**. The pace of discovery is decided with it. A measure proposed for it: attachment (an NPC friend of thirty lives about four more real months, enough to grow attached, watch them age and grieve them).
- **The starting fauna** in its details (beyond the first deer herd of the vivarium): after the NPCs and their stories. Also to check: that an unseen valley takes account of what happened around it.
- **Caves, rock and stone** (13 November 2026): a direction, not a design. Rock shelters first, then true caves as roofed areas in the same map, like the inside of a house; a geology layer by region driving caves (limestone only), flint (chalk and limestone outcrops, river gravel), boulders and clay. Needs its own design page: how the layer is generated, how a roofed area is drawn and entered, what it changes for light, cold and shelter.
- **The exact number of NPCs** in a valley (the valley's size is settled: D31, 3072×3072 tiles since 8 November 2026, meant to grow much further).
- **Floors** (D31): the first version builds on one level. Upper floors and cellars need a third coordinate in the grid, the rendering and the paths; decide when the first player wants a staircase.
- Simple rules for soul points.
- Form of the interface for tool assembly and intentions. First form settled with D35 (6 November 2026): hover names, right-click context menu, inventory panel; assembly still open.
- **The animation tool for bodies** (the rendering itself is settled: pure 2D isometric, D02, `07` and `09`).

## Generator blind spots (15 November 2026)

What the atlas → region → window generator does not do yet, found while building the continent's safety net (D41, `plan::net`, `windows_sheet`). Each line says what would reveal the gap; none is decided.

- **Jungle and steppe have no species of their own**: every forest biome is drawn with the valley's kinds (`species.ron`: oak, pine, birch...), the jungle included; the open biomes' trees are the lone trees' kinds of the region's stands. Revealed by the jungle sheet beside the broadleaf one (the same dots), and by any species list per biome.
- **Mountains**: the heights and the slopes are drawn (D43, D44), but nothing changes with altitude: no alpine meadow, no conifers above 1,500 m, no cliff, no scree; an alpine cell is a meadow on the region's slopes. Revealed by a window on a cell over 1,500 m (the net's hills stratum; seed 14 has under fifty alpine cells and no tundra, so the net cannot judge the cold biomes on it: run it on a cold seed).
- **Coasts and the sea**: the window draws no sea. A coastal cell's sea is land below the sea's level, grass with trees, and the brooks that drain to the sea are dropped a cell short. Revealed by the net's coast stratum (`SeaAsLand`, dry brook ends) and the sheets of coastal cells.
- **Deserts**: a meadow with the meadow's plants and no trees (the content asks none), no sand, no rock, no wadi. Revealed by the desert sheet.
- **Lakes' shores**: a lake is a flat level with the ground drawn down to it over a wandering shore; no beach, no reed belt, no marsh; a brook ends at the lake cell's centre. Revealed by a lake cell among the sampled windows.
- **Great rivers over 500 m³/s**: drawn at the width their flow gives them (`measure::width_cm`), no island, no arm; the atlas finds no dry-shod crossing on them, so a great river cuts the land for good (the atlas's detours measure it). Revealed by the net's great-river stratum: the ford check has nothing to check there.
- **Marsh**: an open biome with the open ground's trees, no wet ground, no reeds, no standing water. Revealed by the marsh sheet.
- **Rivers of 1 to 40 m³/s between the atlas's crossings**: the atlas's crossings are the only fords; a river whose mean depth is over 50 cm is water from bank to bank wherever the atlas found no crossing, so a small river cuts the land between its crossings (one ≤ 50 cm deep is a ford all along). Revealed by `measure::longest_dry_km` on the atlas and by a net invariant still to write: from any bank, a way across within a few kilometres.
- **Tributaries' approach** (the join itself is fixed, #239): on seed 14 the sheets show the junctions, and the net has no invariant yet for two distinct courses running side by side. Revealed by an invariant on the distance between two stems' courses (never under two widths over 100 m unless they join).
- **Seasons absent from the atlas**: the coldest season is estimated from the mean (`coldest_tenths`), the low water from a share of the discharge; the fords are drawn at the mean discharge. Revealed by a winter window: the river at low water, its fords elsewhere and wider.
- **The continent born at every world birth**: seed 14 takes about 6 s (release) before a region or a window can be cut, paid by every server start and every test that reads the atlas. Revealed by the server's start time; a continent cached per seed, or a smaller atlas, would settle it.

What the first sampling found (108 windows of seed 14, 54 of them breaking an invariant), left as findings:

- **The woods are the same drawing in every window**: the valley's woods are keyed by the window's own tile coordinates (`Woods::new(side, side, ...)`), not the continent's, so every forest window of the atlas shows the same massifs, the same clearings and the same openings at the same places (the broadleaf, jungle, conifer and mixed sheets, tile for tile), and two windows that overlap do not agree. Revealed by any two sheets side by side; the fix is an origin for the woods' noises.
- **Pits**: a tile lower than its eight neighbours off the water in every window (the rounding of the hills' noise to the decimetre, up to a thousand a window, 1 dm deep) and, ~~in 16 windows, pits of 2 to 6 dm~~ (mended: 16 → 1 with the valleys, and the last, five pits of 2 to 3 dm on a steppe lake's shore at (390, 476), on 16 November 2026: a lake's shore wandered as fast as the forest's border, 20 m on noises of 40 and 11 tiles, and the point a tile read its lake share at ran back on itself, folding the shore's slope; it wanders on noises of 120 and 40 tiles now and never runs back along an axis, `window::SHORE_WANDER_CELLS`; the shore's 11 m wiggles are gone, a look for the lead's eye. `Pits` 1 window → 0).
- **Lakes' shores in 64 m squares** (the steppe, desert and wooded-steppe sheets): the shore wanders 20 m, not enough to hide the cell.
- **Biome borders in blocks** on the alpine sheet: the forest patches inside the alpine cells keep the 64 m cells' corners (the border wanders 28 m).
- Seed 14 holds 3 alpine cells, 3 mixed-forest, 6 conifer and no tundra among the sampled: the cold biomes are barely measured on it.

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
- **Seizure of power** in the tribe (strength and prestige).
- **Intuitions** of skilled characters.
- **AI-generated speech** for NPCs, then free conversation.
- **Multiplayer**, hosting, costs, moderation. Before exposing the server beyond `127.0.0.1`: TLS, an idle timeout or server-side ping for clients that stop reading, rate-limited logging (milestone 1 audit).
- **End of the world** and nihilists: how hard it should be.
- **Final name** of the game (check the availability of "Soul Invasion") and name of the world.
- **Origin of the souls** (the great mystery).
- **Age rating** and treatment of dark themes.
- Participatory **business model**.
