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

**Decisions still to take, in the order they block** (17 November 2026: 1 to 4 are settled, the rest stand)

1. **The state of a body** — vitality, needs (hunger, cold, fatigue), wounds
   by limb. Everything rests on it: needs, combat, animals, aging.
   — **Settled** (D34, #21).
2. **Combat** (D33) — tempo, gestures and guards, simultaneous resolution,
   wounds, the engagement window, composable commands without
   conditionals. Converged on 5 November 2026; written with step 3.
   — **Settled** (D33); the code comes with the animals.
3. **Death and provisional reattachment** — the soul takes the first free
   body until milestone 5 brings the choice of body and rebirth.
   — **Settled** (D34, #21; amended in #225: a body is born at the camp).
4. **Objects and carrying** — what an item is in hand, carried, laid down;
   a tool made of parts (D15). Settles the weapon model for combat and the
   crafting model. — **Settled** (D35, #30, #227); the parts and
   processes come with crafting.
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
| 3 | The state of a body and its needs; night; eating off the ground; sleeping; death and reattachment — **done** (#21, #22) | 1, 2 (D33 written), 3 |
| 4 | Pick up, carry, lay down, eat; inventory; context menu; names on hover — **done** (#29, #30) | 4, 6 |
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

**Lot 0, the vivarium's world (14 November 2026).** The vivarium is born on
the continent of seed 14, its **zone 2** (`sim::plan::vivarium`, `--world
vivarium`, generator version 4): atlas cell (383, 521), region origin
(378, 516), the zone's corner at region cell (156, 164), 34 cells of 64 m a
side, 2,176 m (4.73 km²), cut from the atlas like `plan:<seed>` so its
heights, biomes, woods, river and brooks are the atlas's. In it: a river of
57 m³/s (13 at low water) crossing at region cell (176, 176), some 30 m
wide at the tile, the width its flow gives it; the forest edge at
(178, 173), 230 m from the crossing; a hill at (159, 194), 144 m of relief,
its top at 217 m, 1,584 m from the crossing, scenery only (heights, no
walking cost); camp A at (186, 194) and camp B at (159, 167) on opposite
banks, 2,443 m apart; the first body is born at camp A. Measured: the zone
itself is cut in about 60 ms, but the continent it is cut from takes some
6 s in release, so the world is born in about 6 s, not under the second
the plan asked; the continent's cost is the thing to bring down or to keep,
not the zone's. The atlas finds the crossing wadeable (a bar reach, its
riffle about 33 cm deep) but the window drew a river at its mean depth,
1.5 m here, and no ford. Since the river and net mendings of 15 and 16
November (D36, the net's `FordPath`) the window draws it: 1,636 ford tiles
around (1312, 800), the one way between the camps.

**Lot 1, the program (16 November 2026).** The `vivarium` crate
(`server/vivarium`, depending on `sim` alone) bears a world as the server
would (the content read from the same files), bears its people at the camp,
runs the ticks with no clock and no network, and writes one HTML file: the
factors named (the cadence, the world, the year, the brain, the food, the
content's rates), the map with the camp, the living and the dead, four
curves sampled 24 times a day (the living, mean hunger, thirst and
fatigue), the chronicle (days, seasons, births, meals, drinks, deaths with
their cause and place) and a page per person with its gauges; the speed in
game years a minute on its own line, so the rest of the page is the same
byte for byte for the same settings (tested). The cadence is one tick a
game second, the plan's slow one. The needs and the gathering are worked
out by the game day and hour, so they do not depend on it; but a thought
becomes an intention on the next tick and a refused act costs a whole
tick, so the outcome does move with the cadence (audit of 20 November
2026, F2: the D47 trial below loses two in thirty days at cadence 1 and
nobody at cadence 10; the audit's sixty-eight people over fourteen days,
53 dead against 38, 3,765 against 4,827 kJ a person-day). Whether the
vivarium judges at the game's cadence, about 1.3× the computing time, is
the lead's call; until then the vivarium's test
`the_cadence_does_not_change_who_lives_nor_what_they_gather` is kept
`#[ignore]`d and fails. Without a brain the people stand where
they were born and die of thirst, or of the nights' cold on the way there,
which the chronicle says. The brain is the lot's second pull request, the
two tribes the third, the point zero the fourth.

**Lot 1, the point zero (16 November 2026).** The brain (`sim::brain`)
pilots each body as a player would and thinks only when something changes
for it: born, a gauge past a threshold, a stop, a refusal, a drink or a
meal, nightfall, dawn. It looks as far as the ground lets it see and
remembers the drinking places (one a chunk) and the food it saw; then,
in order, thirsty (50, or 80 asleep) it goes to drink, hungry (50) to eat,
at night home to sleep within 6 tiles of its camp, by day home; knowing no
place it goes looking in rings out of its camp. Born at its camp it knows
where the camp drinks (the tribe's knowledge of its land, until knowledge
passes between people in V3). The two tribes are born at camp A
(1952, 1952) and camp B (224, 224), 2,443 m apart, the river and its ford
between them. Measured on the lead's laptop (release), the world born in
14 s:

| Run | Deaths | Run time | Game years a minute |
|---|---|---|---|
| 2 × 5 people, 10 days | none | 5.0 s | 5.5 |
| 2 × 20 people, 2 years (100 days) | none | 20 s | 6.0 |
| 2 × 150 people, 2 years | none | 152 s | 0.8 |

The same settings give the same report twice (the timing line aside).
Nobody dies: the world lays no food yet and hunger is paused, water is
always near the camp, a night outside costs less vitality than the day
gives back. They live too easily, as the plan expected: this is the point
zero the next lots are measured against. Two things learnt: a thought cost
4 ms when it tried every tile in sight as a place to drink and 40 ms when
it made the ground's dead wood and pebbles to look for food among them
(now it looks for water from the water, only from a new place, and for
food among the items bodies laid: 0.4 ms); and every body of a tribe does
the same thing at the same time (one camp, one rule), which the lot 2
needs and the people's own traits will break. With 300 people the report
weighs 38 MB (every thought on every page): to be cut when it matters.

**Lot 2, the brain gathers** (16 November 2026, two tribes of five, two
years, release): nobody dies, the run takes about 60 s after the world's
birth. Each tribe gathers 0.8 to 1.5 GJ a year, eats nearly all of it and
loses 1 to 3 % to spoilage; that is about half of what five people need
(10 MJ a day each), and they spend about half of their days at full
hunger, harmless while hunger is a gauge. They gather from the
blackberries' ripening (day 20 of the year) to the hazelnuts' end, and go
hungry through winter and spring: with two crops only, a store does not
outlast the season. Two things learnt: a tribe whose water is far from
its camp roams far from it, so the plants it remembers must be the nearest
to where it is, not to its camp; and bushes seen across a river are tried
one after the other unless the whole patch is left for the day.

**Lot 2, the valley's food** (16 November 2026, the same trial with
nineteen crops): nobody dies, the run takes about 115 s. Each tribe
gathers about 2.1 GJ a year, near nine tenths of its need, and spends
about 40 person-days a year at full hunger instead of 130. The winter is
carried by haws and rose hips on the branch, the summer by grain and peas,
the spring stays the lean season (dandelion leaves, cattail roots).
Nothing spoils: hungry people eat what they pick. No hazelnut is gathered:
the brain remembers the 256 plants nearest to it, so in a meadow the
hazels are crowded out, and it does not yet prefer the richer food. Three
things learnt: a gathering asked which plant it was by placing the
chunk's flora again (now held per chunk); looking for water at every new
tile cost three quarters of a thought by a lake shore (now every 8
tiles); and a walk to the water stopped short far from the camp made a
tribe forget its only water and die of cold lost (now it goes back to the
camp first). With every thought on every page, the report of this run
weighs 63 MB.

**Lot 3, the fat reserve** (16 November 2026, D46). Five people on the
50 × 50 patch, nothing to eat in spring: thin from the third day, weak
near the fifth, all dead on the seventh, of hunger. Two tribes of five,
two years, with the spring of lot 2: nine of ten died of hunger, in
spring or the summer after (leaves gave some 1,000 kJ an hour of
gathering against a need of 10,000 a day). With the richer spring the
project lead asked for: three die, all of tribe A at the start of the
second summer, between the spring's leaves and the grain; tribe B is
never thin. The gap is a gap of species: more fruits and vegetables come
next.

**More of the valley** (16 November 2026): 8 shrubs (red currant,
blackcurrant, gooseberry, bilberry, cornelian cherry, wild grape,
strawberry tree, blackthorn) and 11 plants (wild asparagus, hop, sorrel,
lamb's lettuce, chickweed, purslane, fat hen, wild onion, silverweed,
arrowhead, water chestnut), each with its crop, without images yet (the
client draws a kind it has no image of as nothing). Burdock is left out:
its root is mistaken for deadly nightshade's. Two tribes of five, two
years: nobody dies, nobody is ever thin, each tribe gathers some
2.5 GJ a year.

**The fruit and nut trees** (16 November 2026): wild cherry, mulberry,
wild pear, walnut, sweet chestnut and wild fig in `species.ron`, with
their crops, drawn with a stand-in's images until their own (D39). Two tribes, two years: nobody dies, tribe B a day and a
half thin in all in its first year; the tribes gather cherries and
walnuts among the rest.

**Lot 3 B, ages and sexes** (17 November 2026, D47). A woman alone with
her infant on the patch, nothing to eat: she dies of hunger on the
eighth day, and her infant, left with nobody to carry it, dies the next
night. Two tribes of five adults, three children (3, 7 and 11) and an
infant each, two years: at first the children died of hunger beside fed
adults (hunger counted in a grown man's points, a child of three was
seldom hungry enough to eat; seven of eighteen died); with hunger in
points of each body's own need, all eighteen live, the two infants are
weaned on day 76 and walk, tribe A is thin some fifty person-days in
two years, tribe B never.

*The trial no longer holds (audit of 20 November 2026, F4).* Run again
that day for thirty days at cadence 1: **two die**, a woman of tribe B of
hunger on day 19 and the infant she carried of thirst on day 21; at
cadence 10 all eighteen live (F2). The text above predates D48 (18
November) and the brain's changes; the band's hunger is the sim phase's
matter (F1: the tribe forages as one body and never leaves its first 300
m). The vivarium's test
`the_d47_trial_two_tribes_with_children_and_infants_live_thirty_days`
(`#[ignore]`d, some 30 s in release) runs the trial, prints every death
with its day and cause, and fails until the trial holds again.

**The point zero regained** (21 November 2026, the sim phase's first
step, the project lead's go). The D47 trial run again: nobody dies, the
run takes 15 s on the wall instead of 34 (2.5 game years a minute at
cadence 1). The cause of the two deaths was the whole tribe's: it
gathered some 4,100 kJ a person-day against 8,000 of need and lived on
its reserve, because the brain walked from crumb to crumb (a plant ranked
by what an hour of a full one gives, while each held a trickle) and a
hungry body never slept (hunger came before the night). D41 amended: a
plant is worth what is left on it, none under ten minutes of work, and
a hungry body sleeps at night unless its reserve is weak. Thin
person-days 58 and 30 → 0 and 26, thoughts a person down fifteenfold,
food gathered up a tenth, the people ranging up to 1,350 tiles from
their camps by day and home at night. The cadence trial (F2) still
differs at cadence 10 in food alone (2.33 and 2.30 GJ against 2.34 and
2.20), nobody dying at either; the plan's small trial, two tribes of
five for ten days, holds again too.

**Lot 3 C, births, deaths and families** (21 and 22 November 2026, D49,
D41 amended). The life rule in `sim` (#306): conception once a season,
three seasons of pregnancy, birth where the mother stands, a death curve
by age drawn daily, illness under sixty and old age from it, a spouse
taken among the unwed who are no kin; off in the game. The vivarium
turns it on, pairs the grown at the departure and gives the children
parents, keeps the registry of kinship and deaths in its report and
writes **one page per family** with counters per tribe and per year.
*Trial 1, a couple of twenty alone on the 50 × 50 patch, food without
limit (hunger a day 0), twenty years, cadence 1, release, seed 1*:
**6 births**, 2.8 to 3.7 years apart (138 to 187 days; mean 3.2 years),
all six weaned at two to the day and walking after; **one death**, the
mother, of illness at 39 in the last year; nobody of want; 7 alive at the
end; the eldest leaves the family at 16 in year 18 and pairs with nobody
(his sisters are his kin and under sixteen); no fertile woman without a
possible spouse while the mother lives. The run took 46 s on the wall
(26 game years a minute), its page 0.5 MB. The test
`one_couple_alone_and_fed_twenty_years` (`#[ignore]`d) runs it and holds
those invariants. The second trial, two tribes of 125 over ten seeds, is
PR 3.

**D50, the first trials** (22 November 2026, D50; D46 amended: the fat
reserve is 300 MJ). The brain's constants became ways on each mind (how
far a plant is worth going, sleeping out or at home, the store, the home),
with a will per body; a body in distress for three days may try one way
moved by one step, the world judges after five days, and at night at the
camp a body may copy the way of the camp-mate who gathered most. Every
number is `needs.ron`'s `trials` section. *Tiny first, one band of 25 (15
grown, 7 children, 3 infants) at camp A of the vivarium's world, five
years, cadence 1, seed 1, release*: **with trials 14 alive and 17 dead**
(49 trials, 27 kept, 114 ways copied, 6 homes moved; 11 of the dead of
thirst, 3 of cold); **without, 19 alive and 12 dead** (8 of thirst, 3 of
cold). Deaths by year with trials: 5, 4, 3, 4, 2; without: 1, 1, 2, 6, 7
(the control dies later, of the same thirst by camp A). Births: 3, 1, 1
with; 4, 0, 1 without. *Then the two tribes of 25 (camps A and B), twenty
years*: **with trials 5 alive and 53 dead; without, 20 alive and 50
dead.** Tribe A with trials, year by year (deaths / births): 5/0, 4/3,
3/1, 4/0, 2/1, 3/0, 1/0, 0/0, 3/0, 0/1, then 0 to 1 a year and no birth,
3 alive at twenty years (121 trials, 57 kept, 187 copied, 10 homes
moved; at the end 2 of 3 sleep out, worth halved at 136 tiles, store
60,000 kJ); without trials 1/0, 1/4, 2/0, 6/1, 7/0, 3/1, 2/1, 1/0, 3/0,
0/2, 2/0, then 0 to 3 a year, 2 alive. Tribe B with trials: 3/0, 2/1,
3/0, 2/1, 3/0, 2/0, 2/0, 5/0, 0/0, 1/0, 1/0, 0, 1, then nothing, **2
alive** (235 trials, 104 kept, 144 copied, 44 homes moved: its camp went
from (224, 224) to the map's west edge, (0, 224), (0, 724), (0, 2000),
(0, 1500); at the end nobody sleeps out, worth halved at 24 tiles, store
435,000 kJ); without trials 1/0, 0/2, 1/1, 0/0, 0/0, 0/3, 1/2, 2/1, 1/0,
2/0, 4/0, then 0 to 2 a year and 1 birth, **18 alive**. The curve the
project lead expects ("au début plus de morts, après moins de morts… et
on grandit") does not turn within twenty years on this world: the deaths
with trials come earlier and the births fewer, and tribe B, which lives
without them, dwindles with them. What the pages show, not tuned (the
lead's rule): most deaths are of thirst in both runs; sleeping out costs
deaths of cold; a trial of the home towards a seeded direction stops at
the map's edge on this 2,176-tile world, and the band follows. The pages
`2026-11-22-essais-bande-25.html`, `…-temoin-bande-25.html`,
`…-essais-deux-tribus-25.html` and `…-temoin-deux-tribus-25.html` are
in the review folder; the `#[ignore]`d test
`the_d50_band_of_25_five_years_with_and_without_trials` runs the band's
pair and holds: a trial kept, a way copied, the band not extinct, the
same trials from the same seed (5 min in release).

**Lightening of the simulation, PR 1 (22 November 2026).** The lead
asked for runs "beaucoup plus rapides", measured first, with the same
results before and after (D24). The profile of the two tribes of 25 over
40 days read, with call stacks: the brain was 38 % of the process (the
plants placed as the minds learn them, 19 %), the world's step 37 % (the
walks planned, 30 %), the birth 17 %; and a quarter of the whole was the
**reading of a tile's terrain** (`Land::terrain`), through a thread's
list of four recent chunks searched and reordered at every read and the
chunk cloned out, the land's lock and cache behind it. The terrain was
already made once a chunk and held (D51); the reading is now a thread's
direct table of 16 × 16 chunk slots, read in place, and the trunks'
lookup of a walk's planning keeps the chunk read last. The vivarium's
pair runs in **−22 to −27 %** of its time (run 58.9 → 46.1 s and 67.9 →
49.8 s, alternated on a loaded machine), the page's hash without its
timings unchanged on both runs. What was found and not touched, for PR 2:
the plants placed a chunk (~6 ms, 1,360 chunks a run, already held),
`crops::text_key` hashing a name's bytes through a fresh `Vec` at every
call, the known plants' map, and the birth, whose continent the server
keeps in SQLite but the `vivarium` binary bears anew (10 s).

**Lightening of the simulation, PR 2 (22 November 2026).** The brain,
nothing visible changing (D51 amended). The brain audit of the day read it
at 65 % of the loop once the terrain was cheap: placing the plants of a
chunk as the minds learn them (39 %), choosing the plant to gather by
re-reading every known plant in the world by name (30 %), learning and
re-ranking at every look a thousand plants a full memory could not keep
(20 %). Six steps, one commit each, the pair of 25 a camp hashing the same
after each (page without its timings, JSON without `meta.timing`): what is
ripe asked by the crop's number and the name hashed without a vector; a
look keeping only what outranks the plant its memory holds last; the plant
to gather chosen once a thought; a chunk's bearers held once with the
world's, their kind's name shared; only the minds concerned swept; the
bearers' and shores' caches widened to the world's chunks. Measured on a quiet machine, reference and new alternated: the
pair's CPU **31.9 → 26.5 s** with trials, 31.7 → 26.2 s without (−17 %,
the birth's 8 s unchanged), the brain **15.2 → 11.4 s (−25 %)** and 63 →
59 % of the loop, the plant's choice halved, the plants ranked divided by
six, peak memory 146 → 140 MB; 250 days 313 → 200 s under load, pages the
same. Placing a chunk's plants (6.1 s of the 11.4) is now the brain's
cost, intrinsic to `flora::plants_in`.
Left for the lead, as rules: the audit's S7–S13 (think before looking,
choose once an hour, remember patches, a hunger hysteresis, a shorter
plant sight, the worth's distance).

**Lightening of the simulation, PR 4 (22 November 2026).** The world's
step, nothing visible changing (D51 amended). The step audit read it at
43.5 % of the loop after lightening 2, the routes 76 % of it: the A*
spending its whole budget of 1,500 tiles to say "no way in sight" before
a river was 85 % of the tiles expanded. The lead: "à part S12, on peut
absolument toutes les faire". Seven steps, one commit each, the pair of
25 a camp hashing the same after each, on 250 days and with infants too:
a connectivity pre-test by chunk before every A* (each chunk's walkable
pieces, joined across the sight square: a target nothing joins is refused
with no tile expanded, the plan of `foule/chemins.md` §2 never coded); the
A* arena and the skirt's five windows reused, stamped, never zeroed; a
straight walk's sidesteps answered by a flood before the small A*; the
bodies grown once a day and the carriers read only when someone is
carried; the gatherer asking by its crop's number, borrowing, keeping its
key; the vivarium's continent kept on disk between runs (a cache of the
tool, `--no-continent-cache` to bear it anew). Measured on the loaded machine, reference and new alternated: the pair's process CPU **37.8 → 20.0 s** (−47 %), the world's step by the probe **12.1 → 5.6 s (−53 %)** and 44 → 32 % of the loop (the pre-test alone −34 %), the birth **5.9 → 0.3 s** (the continent read, not born), peak memory 132 → 122 MB; 250 days 429 → 175 s under load, pages the same. Left to PR 5 (D54), the rules of the route the lead chose: a lower path budget, a shorter sight in the open, and looking again toward the window's best edge after a skirt instead of giving up.

**D50 b, the judge counts every distress** (22 November 2026, D50
amended; after #308's deaths of thirst and cold). The day a mind closes at
nightfall records every distress (gathered kJ, thin, thirsty, cold the
night before, hungry, bare); a trial is kept only if nothing worsened and
one thing improved (gathered a day, thin days, thirsty days, cold days);
the mate copied is the one with the fewest days of distress, gathered kJ
breaking ties; a home trial goes towards the plants known bearing or
towards the nearest water known, half and half (the lead: a tribe's
decision, not a rule of ours), and never past the map's edge (a margin of
half the step). Nothing else tuned; same pairs as #308, release, cadence
1, seed 1 (the four runs at once on 16 cores, so the wall times are not
#308's). *One band of 25 at camp A, five years*: **with trials 11 alive
and 18 dead** (62 trials, 30 kept, 49 copied, no home moved; 13 of the
dead of thirst, 3 of cold, 1 of hunger; #308: 13 alive and 17 dead);
**without, 19 alive and 12 dead**, the same run to the tick as #308's
control. Deaths by year with trials 2, 6, 9, 1, 0 (#308: 5, 4, 3, 0, 5);
without 1, 1, 2, 3, 5. What the band found: sleep out tried 22, kept 9,
copied 12 (and "sleep at home" copied 14 back); search wider 12 / 8 / 5,
narrower 8 / 4 / 7; store more 8 / 5 / 6, less 11 / 4 / 5; at five years 7
of 11 sleep out. *Two tribes of 25, twenty years*: **with trials 38 alive
and 41 dead** (#308: 5 and 53); **without, 20 alive and 50 dead** (as
#308). Tribe A with trials: 9 alive (#308: 3; control 2), 23 dead (13 of
thirst, 4 of cold, 6 of illness), 182 trials, 74 kept, 251 copied, no
home moved (the quarter step of 500 tiles from (1952, 1952) leaves only
the north-west inside the margin, and no trial drew it); found: sleep out
27 / 13 / 34, sleep at home 31 / 10 / 30, wider 34 / 11 / 48, narrower 25
/ 9 / 59, store more 33 / 21 / 37, less 32 / 10 / 43; at the end 2 of 9
sleep out, worth and store back at the medians 32 tiles and 30,000 kJ.
Tribe B with trials: **29 alive** (#308: 2; control 18), 18 dead (12 of
illness, 3 of thirst, 3 of cold), 22 births, 664 trials, 311 kept, 561
copied, 17 homes moved (8 kept) and the camp still at (224, 224) at the
median; found: sleep out 137 / 65 / 57, sleep at home 71 / 27 / 81, wider
107 / 50 / 71, narrower 105 / 55 / 65, store more 131 / 71 / 132, less 96
/ 35 / 155; at the end 10 of 29 sleep out. The curve turns within twenty
years where #308's did not: the deaths of thirst fall from 14 and 13 to
13 and 3, the camp no longer drifts to the edge, and tribe B grows. The
band of five years does worse than #308's (its first years are bolder:
sleep out tried 22 times), measured, not tuned. Wall: band 171 s with
trials and 228 s without; two tribes 3,442 s with trials (38 alive at the
end) and 903 s without. The day costs more with trials because the world
does more, not the judge: over the same 100 days, two tribes, #308's
binary 169 s (40 alive), this one 238 s (49 alive), its control 150 s
(53 alive); a probe over 30 days gives `World::step` 26.9 s with trials
against 15.9 s without, `Brains::think` 20.6 s against 20.3 s. The
vivarium prints one line a year on stderr (alive, dead so far, seconds)
so nobody waits blind.

**The laboratory** (22 November 2026, D52). The simulation phase works by
series of runs, so the series get a tool: `tools/lab/`, a local page on the
standard library alone that creates a series from a form (the question,
the world, the people, the days, the seeds, with and without trials, the
cadence), launches it and follows it live, with a stop button; it absorbs
the local launcher `soul-runs` (same plan format, same output layout), so
the plans of lot 3 C keep working. Its first pull request builds the
"Nouvelle série" and "En cours" screens; "Lire / Comparer" (reports,
tables, curves, comparisons) comes once the vivarium writes its report as
JSON beside the page.

**The laboratory, PR 2** (22 November 2026). The vivarium writes its data
as JSON beside each page (D41 amended), and the lab reads it, never the
page: a run (the cards, four curves, the map with the camps' trail, the
presence heatmap and the deaths, the discoveries told as a story of
techniques per tribe, the yearly table, the people), a series (one row per
run, the mean and spread of the survivors with and without trials, every
population curve overlaid) and a comparison (two versions of the same plan
run by run, or two runs face to face, differences highlighted). The lead's
correction after PR 1 shapes it: the AI session launches, the lead reads
and compares. So every series records its version (the binary's git commit
and branch, one sentence of what changed), a command line `lab.py run
PLAN --change "…"` launches a series detached from any server, three
standard plans live in `tools/lab/plans/` (a band of 25 over five years,
two tribes of 25 over twenty years, the same over ten seeds), and the first
comparison offered on a series is "the same plan under another version".

**Three defects of the brain, and the squared distance** (22 November
2026; D45 and D50 b amended; the probe `soif-tribu-A-2026-11-22.md`, the
audit and the brief in `content/ai-reserve/review/vivarium/allegement/`).
Tribe A died of thirst with its water 354 tiles from the camp, for two
reasons that were not the distance: a body walking straight beyond its
sight stops 30 cm short of each trunk to look again (D32), and the brain
read that pause as a stop, "stopped short far from the camp: back to the
camp first", 241 times in 25 days, every one with a goal still set; and a
parched body does not sleep, so at full fatigue the world laid it down,
the brain's next order woke it, it walked a tick and fell again, 8,000 to
11,000 decisions in a day and a half, dead standing. Both corrected on the
brain's side, the protocol unchanged: a pause (`Body::is_pausing`) is
neither a thought nor a stop; a sleeper at `COLLAPSED` (90) or more is
asked nothing until it has slept a tenth of its gauge off. Third, the
home step refused any direction whose far end left the map, so on the
2,176-tile world no adult of will ever tried its home: the step now
shortens to the edge less the margin, along the axes it moves on (the
lead: the edge is physics, one walks until the land ends). And a plant's
worth was halved at the square of 32 tiles, not at 32 (audit P7). The
red `a_piloted_body_on_the_valley…` (thirst 100, fatigue full at tick 83)
is green. *Measured, the same pair as D50 b over 250 days, seed 1,
cadence 1, `origin/main`'s binary against this one, four runs at once*:
**with trials, 47 alive and 20 dead before, 65 alive and 5 dead after**;
tribe A before 18 alive, 14 dead of thirst, 3 of cold, 1 of illness, 1 in
childbirth, 2 home trials (2 kept); after 33 alive, none of thirst or
cold, 2 of illness, 1 of hunger, 1 in childbirth, 172 trials (70 before),
20 of the home (11 kept); tribe B before 29 alive, 1 dead, 7 home trials;
after 32 alive, 1 dead, 38 home trials (21 kept), its camp at (724, 250)
at the median against (224, 224). **Without trials, 50 alive and 19 dead
before, 74 alive and 5 dead after** (tribe A's 9 of thirst and 7 of cold
gone). Wall 277 to 467 s a run, more people walking after than before.

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

**Dependencies**: 8 direct, 72 crates in the normal tree, no duplicate versions, licences and advisories checked by `cargo deny` on every push to `main` and on every pull request that touches the dependencies or the CI (20 November 2026). Most of the tree is tokio and tungstenite: the price of WebSocket.

**Security review of the listener** — found and fixed in the audit: silent TCP connections were kept forever (the handshake deadline only started after the WebSocket upgrade); no limit on message size (64 MiB by default); no limit on the number of connections; a client could flood intentions, each refusal being broadcast to everyone. Now: deadline on the upgrade too, 4 KiB messages, `--max-connections` (default 64), one pending intention per body per tick, refusals to the pilot alone. Each has a test.

**Left for later**, recorded in `06`: a client that stops reading holds its connection until TCP gives up (idle timeout or server-side ping); log lines per connection and per bad message can be flooded (needs a real logging layer); TLS and anything beyond `127.0.0.1` when multiplayer comes; retention of journal and snapshots.

## Feasibility

- Feasible: server, connection, physical systems, animals.
- Hard: **NPC credibility**, **game feel**, **interface**. These require play and human adjustments.
- Order of magnitude: the first milestones in weeks, the full version in months, depending on the time available to test and decide.
