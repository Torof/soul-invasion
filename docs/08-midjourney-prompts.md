# 08 — Midjourney Prompt Library

*To be used together with the style bible (`07-style-bible.md`).*

## How to use

Every prompt already contains the style parameter (`--sref` followed by the reference image link). They can be pasted as-is into Midjourney. This is the library; **what is produced, and when, is decided in `09-visual-production.md`** — an image is generated only when the pull request that shows it is in sight.

If the reference image ever changes, simply replace that link throughout this document.

- The reference handles the style: the text mostly describes **the subject**.
- Style too weak: add `--sw 200`. Too strong (the subject is drowned): `--sw 60`.
- For variants of the same result: add `--chaos 15`.
- If your Midjourney version rejects a parameter, remove it.

Base formula for an isolated element:

```
single 2D game asset, [SUBJECT], high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background, no ground shadow --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

---

## 1. Trees and vegetation

**Main broadleaf tree**
```
single 2D game asset, a large broadleaf tree with a thick twisted trunk and dense rounded foliage clumps, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

**Small young tree**
```
single 2D game asset, a young slender broadleaf tree with light green foliage, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

**Conifer**
```
single 2D game asset, a tall dark green conifer tree with layered branches, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 2:3 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

**Dead tree**
```
single 2D game asset, a dead gnarled tree with bare twisted branches and grey bark, slightly ominous, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

**Cut stump** (after felling)
```
single 2D game asset, a freshly cut tree stump with visible rings and wood chips around it, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

**Berry bush** (with and without berries)
```
single 2D game asset, a round berry bush covered with small red berries, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```
Then the same with `a round green bush with no berries, recently picked`.

**Tall grass and ferns**
```
2D game asset sheet, several clumps of tall grass, ferns and small wildflowers, each isolated and evenly spaced, high-angle three-quarter view, thick dark outlines, flat cel shading, plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text
```

---

## 2. Rocks and relief

**Boulders**
```
2D game asset sheet, five mossy grey-green boulders of different sizes, each isolated and evenly spaced, high-angle three-quarter view, thick dark outlines, flat cel shading, plain white background --ar 16:9 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text
```

**Flint outcrop** (resource)
```
single 2D game asset, a small rocky outcrop with dark shiny flint nodules visible in pale stone, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

**Cliff**
```
single 2D game asset, a section of tall grey-green rocky cliff with blocky stone faces and moss on top, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

---

## 3. Ground

Ground textures are seen **from above** and meant to be repeated. They will need touch-ups to tile properly.

**Grass**
```
top-down seamless ground texture, lush green grass with small tufts and tiny flowers, flat cel shading, thick dark outlines on details, game tile --ar 1:1 --tile --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png
```

**Packed dirt and sand of the clearing**
```
top-down seamless ground texture, packed golden sandy dirt with small pebbles, flat cel shading, game tile --ar 1:1 --tile --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png
```

**Dark forest floor**
```
top-down seamless ground texture, dark forest floor with fallen leaves, roots and moss, flat cel shading, game tile --ar 1:1 --tile --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png
```

**Mud and riverbank**
```
top-down seamless ground texture, wet muddy riverbank with small stones, flat cel shading, game tile --ar 1:1 --tile --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png
```

---

## 4. Water

**Water texture**
```
top-down seamless water texture, clear turquoise river water with soft ripples and light reflections, flat cel shading, game tile --ar 1:1 --tile --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png
```

**River section** (for the overall look)
```
2D game environment piece, a short curved section of turquoise river with sandy banks, rocks and reeds, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 16:9 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text
```

**Riverbank elements**
```
2D game asset sheet, river reeds, lily pads, wet stones and a small fallen log, each isolated and evenly spaced, high-angle three-quarter view, thick dark outlines, flat cel shading, plain white background --ar 16:9 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text
```

---

## 5. Animals

For each animal: first a **concept sheet**, then the **in-game view**. Animals of our world may differ slightly from ours: add `slightly unusual, otherworldly features` to explore.

**Deer** (peaceful prey)
```
2D game creature design, a wild deer-like animal with branching antlers and a warm brown coat, calm and alert posture, strong readable silhouette, full body, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

**Doe and fawn** (for prey reproduction)
```
2D game creature design, a female deer-like animal with a small spotted fawn, gentle posture, full body, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

**Wolf** (predator)
```
2D game creature design, a lean wild wolf with dark grey fur, lowered head and menacing stalking posture, sharp angular silhouette, full body, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

**Pack** (to judge readability in a group)
```
2D game scene, a pack of three dark grey wolves emerging from the edge of a dark forest at dusk, menacing, high-angle three-quarter view, thick dark outlines, flat cel shading --ar 16:9 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text
```

**Wild boar** (dangerous when attacked)
```
2D game creature design, a stocky wild boar with bristly dark brown fur, curved tusks and a heavy head, stubborn defensive posture, strong readable silhouette, full body, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

**Scale lineup** (check relative sizes)
```
2D game lineup, a prehistoric human hunter standing next to a deer, a wolf and a wild boar, all to scale, side by side, high-angle three-quarter view, thick dark outlines, flat cel shading, plain white background --ar 16:9 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text
```

---

## 6. First camp elements

**Hide tent**
```
single 2D game asset, a conical prehistoric hide tent with wooden poles sticking out of the top, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

**Branch shelter**
```
single 2D game asset, a primitive lean-to shelter made of stacked branches and leaves, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

**Campfire** (unlit, embers, burning)
```
2D game asset sheet, the same stone-ringed campfire in three states: unlit with logs, glowing embers, and burning brightly, evenly spaced, high-angle three-quarter view, thick dark outlines, flat cel shading, plain white background --ar 16:9 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text
```

**Meat drying rack** (for the smoking discovery)
```
single 2D game asset, a primitive wooden rack with strips of meat hanging above a smoking fire, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

---

## 7. Ideas to go further

- **Night**: redo the reference scene at night, lit only by the fire, to check that the mood holds.
  ```
  the same prehistoric tribe camp at night, lit only by the central campfire, dark forest edges, glowing eyes in the trees --ar 16:9 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png
  ```
- **States**: every resource exists in several states (tree standing, felled, stump; bush full, empty; meat fresh, smoked, rotten). Remember to generate each state.
- **Readability**: paste a character and an animal onto the reference scene to see whether they stand out.
- **Variants**: for a given tree or rock, generate 3 or 4 variants so the forest does not look copy-pasted.

## Tracking

For every validated image, record beside it in `content/art/`: the file, the exact prompt, the date (see `content/art/references/style-ref-01.md`). This is what will later allow training a model on our style.

---

## Batch 1 — what the game shows now (6 November 2026)

The format contract is in `09-visual-production.md` §4. Each image has its file name under `content/art/`, its footprint, and its prompt ready to paste. Grounds are seen exactly from above and must tile; elements stand on white with no ground shadow, lit from the front left; anchors are at the base of the centre of the footprint. Generate at twice the game size (the game uses 64 px per metre at scale 1).

### Grounds (top-down, seamless, one image covers 4 × 4 m → 512 × 512 px in game)

**`ground_grass_01_summer.png`** — 4×4 m
```
top-down seamless ground texture, lush green meadow grass with a few small tufts, seen exactly from above, no perspective, flat cel shading, calm and low in detail, no outlines, no shadows, game texture --ar 1:1 --tile --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, objects, characters
```

**`ground_grass_02_summer.png`** — 4×4 m
```
top-down seamless ground texture, green meadow grass worn by footsteps, patches of bare dirt showing, seen exactly from above, no perspective, flat cel shading, calm and low in detail, no outlines, no shadows, game texture --ar 1:1 --tile --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, objects, characters
```

**`ground_forest_floor_01_summer.png`** — 4×4 m
```
top-down seamless ground texture, dark forest floor, fallen leaves, roots and moss, seen exactly from above, no perspective, flat cel shading, calm and low in detail, no outlines, no shadows, game texture --ar 1:1 --tile --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, objects, characters
```

**`ground_ford_01_summer.png`** — 4×4 m
```
top-down seamless ground texture, shallow clear turquoise water over pebbles, a river ford, seen exactly from above, no perspective, flat cel shading, calm and low in detail, no outlines, no shadows, game texture --ar 1:1 --tile --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, objects, characters
```

**`ground_water_01_summer.png`** — 4×4 m
```
top-down seamless ground texture, clear turquoise river water with soft ripples, seen exactly from above, no perspective, flat cel shading, calm and low in detail, no outlines, no shadows, game texture --ar 1:1 --tile --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, objects, characters
```

**`ground_rock_01_summer.png`** — 4×4 m
```
top-down seamless ground texture, grey-green cliff rock with cracks and acid-green moss in the cracks, seen exactly from above, no perspective, flat cel shading, calm and low in detail, no outlines, no shadows, game texture --ar 1:1 --tile --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, objects, characters
```

### Overlays (top-down, transparent after cut-out, scattered on cells)

**`overlay_flowers_summer.png`** — cell
```
top-down game overlay, a few small wildflowers in yellow and white with a leaf or two, sparse and well separated, seen exactly from above, thick dark outlines on each element, flat cel shading, plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, ground texture
```

**`overlay_dead_leaves_summer.png`** — cell
```
top-down game overlay, a few fallen brown and ochre leaves, sparse and well separated, seen exactly from above, thick dark outlines on each element, flat cel shading, plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, ground texture
```

**`overlay_pebbles_summer.png`** — cell
```
top-down game overlay, a few small grey river pebbles, sparse and well separated, seen exactly from above, thick dark outlines on each element, flat cel shading, plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, ground texture
```

**`overlay_moss_summer.png`** — cell
```
top-down game overlay, a few patches of acid-green moss, sparse and well separated, seen exactly from above, thick dark outlines on each element, flat cel shading, plain white background --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, ground texture
```

### Items on the ground (one sheet, each item cut out to lie on a cell; anchor: centre)

**`items_materials_01.png`** — sheet
```
2D game item sheet, ten small items evenly spaced on a grid, each isolated: a dark grey flint nodule, a smooth round pebble, a dead wooden branch, a short thick log, a bundle of pale plant fibres, a handful of red berries, a folded animal hide, a piece of raw red meat, a long white bone, and a prehistoric human corpse lying curled on its side, high-angle three-quarter view, thick dark outlines, flat cel shading, plain white background, no shadows --ar 16:9 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

### Structures (three-quarter on white; footprint and anchor at the base of its centre)

**`fire_states_01.png`** — 1×1, three states
```
2D game asset sheet, the same stone-ringed campfire in three states side by side: unlit with stacked logs, glowing orange embers, burning brightly with tall flames, evenly spaced, high-angle three-quarter view, thick dark outlines, flat cel shading, plain white background, no ground shadow --ar 16:9 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text
```

**`shelter_branches_01_summer.png`** — 1×2
```
single 2D game asset, a primitive lean-to shelter made of stacked branches and leaves, open on one side, about two metres long, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background, no ground shadow, lit from the front left --ar 4:3 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

**`rack_drying_01.png`** — 1×2
```
single 2D game asset, a primitive wooden rack of lashed poles with strips of meat hanging to dry, about two metres long, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background, no ground shadow, lit from the front left --ar 4:3 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

### Trees (one species now; the others with `species.ron`)

**`oak_adult_01_summer.png`** — 1×1, 4 m tall
```
single 2D game asset, a large adult oak tree with a thick trunk clear on its lower third and dense rounded foliage clumps, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background, no ground shadow, lit from the front left --ar 2:3 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```

**`oak_adult_02_summer.png`** — 1×1, 4 m tall
```
single 2D game asset, a large adult oak tree, a different silhouette, thick trunk clear on its lower third, dense rounded foliage, one lower branch reaching out, high-angle three-quarter view, thick dark outlines, flat cel shading, isolated on a plain white background, no ground shadow, lit from the front left --ar 2:3 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no text, scenery
```
