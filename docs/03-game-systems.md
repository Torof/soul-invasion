# 03 — Game systems

*Validated principles. Numbers and detailed rules will come with the versions.*

## 1. The soul

- **The player is a soul**, not a character.
- The soul has: soul points, karma, powers, titles, a history of all its lives.
- It **lives above the worlds**: it can migrate from one world to another.
- Any inhabitant can become a soul's body. Players and NPCs are the same kind of inhabitant; only the pilot changes.

## 2. Reincarnation

- **Soul points** are gained or lost according to how one lives and dies: a fulfilled life, martyrdom, influence, wealth, foundations = bonus; abandonment, hasty reincarnation = penalty. A victim must not be punished for their death (to be refined).
- **Incarnation cost** = Willpower of the targeted body. A beginner is reborn as a slave, a fisher or a baker; a powerful soul can aim for a noble child, or even higher.
- **Heirs**: you can be reborn into your lineage if you have prepared an heir; their value depends on what you invested (education, businesses, contacts). Educating an heir raises their Willpower, which protects them from other souls.
- **Lineage theft** is possible by other souls. Counters: a hidden heir, hidden treasures known only to you, a will in favor of an organization, powers.
- **The choice of rebirth is the choice of difficulty**: an isolated village, a large protected but rigid city, a wild frontier.
- **Information**: the more powerful the soul, the better it perceives the risks of a place of rebirth.
- **Past lives are secret**, discoverable through investigation (in-game and out-of-game).
- **Reincarnating incognito** is possible; the village forgets, karma does not.

## 3. Soul powers and titles

- **Powers**: rare, slow to develop, each one closes other doors. They only act at death and at rebirth.
  - *Bloodline Bond*: nobody else can incarnate into your lineage; trade-offs to be defined.
  - *Vampire*: can seize the heirs of others.
  - *Lucky*: random rebirth, from the worst to the best.
- **Passive titles** earned through one's deeds, not chosen. A dark example: *Chain of Hell*, for gratuitous relentlessness (harming without purpose, endlessly preying on the weak): forced rebirth into hard lives. The villain who plays their role is not punished; excess is. The exact rules deliberately remain mysterious.
- **Paths of elevation** (inner alchemy): the soul grows stronger through the way one lives (builder, sovereign, sage, conqueror, merchant…), including a dark path. Soul strength, knowledge, skills and powers form an alchemy unique to each player.

## 4. The summits

- **Demon Lords (8 seats)**: tied to mortal life. Longer life, permanent bonus to the soul. Dark rituals possible to prolong life, leaving traces. A race against rivals and against time; preparing one's succession and one's return is the strategy.
- **World Emperor (1 seat)**: mortal, bearer of the Aura of Destiny. Major advantages to the soul and to reincarnation, according to a **hidden tree of possibilities**.
- The more powerful a soul, the more **exposed** it is.

## 5. NPCs

- Every NPC has: **needs** (hunger, cold, fatigue, safety), **character**, **Willpower**, **opinions** about the people they know, **memories**.
- **Decisions**: every NPC weighs their options and chooses what makes sense to them. Extreme acts (assassinating a tyrant) emerge from character, memories and opportunity, without a script.
- **Reactions to power**: a bad leader provokes flight, conspiracy, exile, coups, assassination. NPCs vote with their feet.
- **Groups**: families, tribes, villages have a collective mood and make collective decisions (submit, go to war, flee, ally).
- **Progress**: mostly by imitating players and neighbors, with slight autonomous progress.
- NPCs can **found their own villages** and hold any position, including positions of power.
- **Romantic relationships**, couples, births, aging, death.
- **Attachment**: you grow attached to NPCs through shared history.

## 5b. Combat

- Slow real time by intentions: an engaged body commits one gesture per tempo (about a second); between tempos the player reads and chooses. No reflexes, no hit points, no battle mode.
- Gestures (strike, feint, parry, dodge, shove, break off) with directions and target limbs, three guards; simultaneous resolution, moves hidden until they land, wind-ups readable according to skill.
- Everything computed from content: weapon parts, protection by limb, skill, condition, facing, measure, numbers.
- Wounds by limb that bleed, hamper, heal and scar; death from a mortal wound or exhausted vitality; a corpse is an item.
- Assassination is combat against someone unaware. War is the same system at a hundred bodies, with orders, morale and fatigue.
- On the client, an engagement window that is a view of the same world; commands composable by the player, never conditional. Architecture decision D33.

## 6. Interacting with NPCs

- Every interaction goes through an **intention**: offer, help, ask, threaten, court, convince, lie…
- **Evolution**:
  1. Stone Age: actions and gestures (consistent with the lore).
  2. Later: NPCs who express themselves in their own words (generative AI).
  3. Eventually: free conversation, translated into intentions.
- **The AI never decides** on the NPC's behalf; it only helps them express themselves.

## 7. Prestige, witnesses, suspicion

- **Prestige** = the sum of NPC opinions. Feeding the tribe, hunting well, inventing raise it; killing a member destroys it. Banishment is possible, with hostility upon return.
- **Witnesses**: every action has witnesses, every witness keeps a memory of it. An act without a witness does not exist for the world. Witnesses choose: stay silent, profit, denounce.
- **Suspicion**: personal, born from strange behavior. Its consequences depend on the local culture.

## 8. Groups

The fundamental building block of all society.

- A group has: **members, roles, powers per role, rules** (entry, exit, sanctions), **succession rule**.
- Groups can contain groups (empire > provinces) and have **relations** (alliance, war, vassalage, trade).
- **Public or secret**.
- Covers: tribes, families, guilds, brotherhoods of assassins, merchant congresses, religions, armies, empires, systems invented by players.
- **A role's powers are not magic locks**: they exist because members and NPCs recognize them. A general without the loyalty of their soldiers has nothing but a title.

## 9. Political power

- In the tribe: the chief is chosen by **strength and prestige**. Strength alone = quick but illegitimate and unstable power; prestige alone = legitimate but slow power; both = the most solid.
- Power is not tied to sex; players create their own norms.

## 10. Economy and logistics

- **Everything is physical**: transporting 500 loaves requires carts, horses, drivers, escorts.
- **Local markets.** No global auction house.
- **Spoilage**: food has a freshness that decreases; preservation (smoking, drying, salting…) is an advance. Famine, reserves and resupply follow from it.
- Banditry, escorts, roads, inns: the economy creates trades.

## 11. Knowledge and technologies

- **Starting point**: advanced hunter-gatherers (fire, knapped stone, wood, ropes, spears, axes, hides, shelters).
- **No imposed tech tree.** Everything is possible if you gather the chain: knowledge, resources, labor, time.
- **Invention by assembly**: objects and buildings are made of parts; their qualities derive from the materials and the assembly.
- **Discovery** through experimentation and observation, emerging from the properties of the world (e.g. smoked meat keeps). Later, **intuitions** from skilled characters.
- Processes exist without being displayed: the space of possibilities is vast and hidden.
- In the long term, possibly AI to judge free-form inventions.

## 12. World and end of the world

- World time: days of about 2 real hours, seasons of a few real days, childhood of a few weeks, a life of a few months (orders of magnitude to be adjusted).
- **A world can die.** Nihilists could destroy humanity; it is extremely hard, and that is what gives meaning to the orders that fight them. Souls survive and migrate.
- **Core loop**: making the population prosper = more births = more bodies to be reborn into.
