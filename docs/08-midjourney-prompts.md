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

For every validated image, record beside it in `content/art/`: the file, the exact prompt, the date (see `content/references/style-ref-01.md`). This is what will later allow training a model on our style.

---

## Batch 1 — what the game shows now (6 November 2026)

The format contract is in `09-visual-production.md` §4. Each image has its file name under `content/art/`, its footprint, and its prompt ready to paste. Generate at twice the game size (the game uses 64 px per metre at scale 1); upscale the chosen variation and download the upscale.

### What the first generation taught

- **The ground is empty.** Everything that is an object in the game — a bush, a tree, a flower, a rock, a tuft worth picking — has its own image and its own states (full, picked, felled) and is drawn by the engine on top of the ground. Painted into a texture, it would be a thing one cannot touch, repeated every four metres.
- **Nothing is lit.** Light, shadows, night and the fire are the engine's. A vignette or a cast shadow in an image repeats and fights the engine.
- **Nothing is around.** Elements float on pure white with no ground, grass, soil or scenery, so they can be cut out and placed anywhere.
- **Calm beats rich.** Low contrast, few details on the ground; the detail comes from overlays and objects, which do not repeat.
- **Say it twice.** The subject says what we want; the `--no` list says what we refuse — Midjourney obeys the negative better.
- **Keep the dullest variation** for grounds, the cleanest for elements. Generate two to four per prompt.

### Grounds (top-down, seamless, one image covers 4 × 4 m → 512 × 512 px in game)

**Risk**: objects painted into the ground (bushes, trees, flowers) and a vignette of light that repeats every four metres. **Rule**: empty, flat-lit, low contrast; keep the dullest variation. If still busy: add `, plain` and `--stylize 30`.

What the grounds of 6 November taught, in the order it was learnt:

- **The first validated ground is the style reference of all the others.** Attach `content/art/ground/ground_grass_01_summer.png` in the Midjourney app (Use → Style) with `--sw 100`; the old `--sref` link of the scene is no longer used for grounds. A ground made without it comes out in another style and is lost.
- **One image per structure, sisters for variety, the shader for tone.** A second image is the wrong tool to vary the *tone* of a ground: it brings another structure (leaning tufts) and a colour jump. Tone is a tint in the shader (`dry_tint`, `base_tint`). Variety comes from **sisters**: the same prompt re-run with the first image as style reference, sorted by tone (mean RGB within a few points of the first) and drawn differently. The nine sisters of the grass were all usable; the dry grass was not.
- **Say the dose, not only the ingredient.** "A few leaves" covers the surface; "moss" becomes cushions that read as shrubs. Write the amount in words ("about a tenth of the surface", "two or three small faint patches") and lower `--s` to 50. And **a bare surface does not exist** for the generator: "plain soil" came out as cobbles, furrows and lumps, twice. A ground needs a continuous matter (grass, water); earth, leaves, moss come later as sisters at a small dose.
- **No dominant direction.** Fine blades all parallel look calm on the sheet and become diagonal stripes once projected in iso. Tufts must be radial. Test before anything: lay the image next to itself turned by a quarter turn; if the eye sees a change of direction, reject it.
- **Never turn a ground texture in the shader.** The drawings have a slight three-quarter reading; turned, the grass reads upside down, like a roof. The shader shifts, never turns.
- **The forest floor is grass in the shade.** Real woods keep green grass, less bright and sparser. The woods use the sparse grass and the worn grass tinted darker and cooler (`base_tint`), with more bare earth; no forest-floor image exists or is wanted.

The grass family today (all 1024 × 1024, 4 × 4 m, notes beside each file in `content/art/ground/`):

| File | What | Where |
|---|---|---|
| `ground_grass_01_summer` | the meadow grass, the style reference | meadow base |
| `ground_grass_02_summer` | worn grass with bare spots | meadow spots; woods base |
| `ground_grass_03_summer` | sister of 01, closest in tone | meadow sister (half) |
| `ground_grass_04_summer` | open sister, patches of shorter grass | meadow patches |
| `ground_grass_05_summer` | the least tufted sister | removed 10 November 2026 (never used) |
| `ground_grass_06_summer` | sparse forest grass, low radial tufts | woods sister (a quarter) |

**Sister of a validated ground** (variety without a new prompt): the prompt of the ground as written in its note, the ground attached as style reference, `--sw 100`. Keep the ones whose mean RGB is within six points of the original and whose tufts are radial.

**`ground_water_01_summer.png`** — 4×4 m, not generated yet
```
top-down seamless ground texture, calm clear river water, soft blue-green, a few faint ripples only, uniform and calm, seen exactly from above, no perspective, flat even lighting with no vignette and no shadows, very low contrast, thin subtle outlines only, drawn in the same flat cel-shaded style as the reference, game texture --ar 1:1 --tile --sw 100 --s 50 --no photo, realistic, waves, foam, fish, rocks, pebbles, plants, reflections, sparkle, sun, shadows, vignette, text
```

**`ground_ford_01_summer.png`** — 4×4 m, not generated yet
```
top-down seamless ground texture, shallow clear water over small rounded pebbles, a river ford, the pebbles small and even, the water a faint blue-green veil, uniform and calm, seen exactly from above, no perspective, flat even lighting with no vignette and no shadows, very low contrast, thin subtle outlines only, drawn in the same flat cel-shaded style as the reference, game texture --ar 1:1 --tile --sw 100 --s 50 --no photo, realistic, large rocks, boulders, waves, foam, fish, plants, reflections, sparkle, shadows, vignette, text
```

**`ground_rock_01_summer.png`** — 4×4 m, not generated yet
```
top-down seamless ground texture, grey-green bare rock with a few thin cracks, mostly plain, a little moss in two or three cracks only, uniform and calm, seen exactly from above, no perspective, flat even lighting with no vignette and no shadows, very low contrast, thin subtle outlines only, drawn in the same flat cel-shaded style as the reference, game texture --ar 1:1 --tile --sw 100 --s 50 --no photo, realistic, boulders, pebbles, gravel, plants, grass, shadows, vignette, text
```

### Overlays (top-down, five pieces per sheet, cut out one by one and scattered on cells)

**Risk**: a carpet instead of separate pieces, a ground painted under them, a shadow. **Rule**: five pieces, far apart, on pure white; we cut each one out. Reject any variation where pieces touch or where ground shows.

**`overlay_flowers_summer.png`** — cell
```
2D game asset sheet, small wildflower clumps in yellow and white with one or two leaves, five small separate pieces widely spaced apart, each one isolated, seen exactly from above, thick dark outlines, flat cel shading, flat even lighting, no shadows, floating on a plain pure white background with nothing else --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no ground, grass, soil, shadow, scenery, background, frame, text, watermark
```

**`overlay_dead_leaves_summer.png`** — cell
```
2D game asset sheet, small clusters of fallen brown and ochre leaves, five small separate pieces widely spaced apart, each one isolated, seen exactly from above, thick dark outlines, flat cel shading, flat even lighting, no shadows, floating on a plain pure white background with nothing else --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no ground, grass, soil, shadow, scenery, background, frame, text, watermark
```

**`overlay_pebbles_summer.png`** — cell
```
2D game asset sheet, small groups of grey river pebbles, five small separate pieces widely spaced apart, each one isolated, seen exactly from above, thick dark outlines, flat cel shading, flat even lighting, no shadows, floating on a plain pure white background with nothing else --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no ground, grass, soil, shadow, scenery, background, frame, text, watermark
```

**`overlay_moss_summer.png`** — cell
```
2D game asset sheet, small patches of acid-green moss, five small separate pieces widely spaced apart, each one isolated, seen exactly from above, thick dark outlines, flat cel shading, flat even lighting, no shadows, floating on a plain pure white background with nothing else --ar 1:1 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no ground, grass, soil, shadow, scenery, background, frame, text, watermark
```

### Items on the ground (two sheets of five; each item cut out to lie on a cell; anchor: centre)

**Risk**: a wrong count, wildly different scales, a ground or shadow under the items, items touching. **Rule**: two sheets of five, one row, same scale (a log is much bigger than a flint), pure white. Name each item plainly; Midjourney keeps a row of five better than a grid of ten.

**`items_materials_01.png`** — sheet of 5
```
2D game asset sheet, five small prehistoric items: a dark grey flint nodule, a smooth round grey pebble, a dead wooden branch, a short thick log, a bundle of pale plant fibres, in a single row, evenly spaced, each one isolated and at the same scale, high-angle three-quarter view, thick dark outlines, flat cel shading, lit from the front left, no cast shadows, floating on a plain pure white background with nothing else --ar 16:9 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no ground, grass, soil, shadow, scenery, background, frame, text, watermark
```

**`items_materials_02.png`** — sheet of 5
```
2D game asset sheet, five small prehistoric items: a handful of red berries, a folded animal hide, a piece of raw red meat, a long pale bone, and a prehistoric human corpse lying curled on its side, in a single row, evenly spaced, each one isolated and at the same scale, high-angle three-quarter view, thick dark outlines, flat cel shading, lit from the front left, no cast shadows, floating on a plain pure white background with nothing else --ar 16:9 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no ground, grass, soil, shadow, scenery, background, frame, text, watermark
```

### Structures (three-quarter on white; footprint and anchor at the base of its centre)

**Risk**: a patch of grass or earth painted under the structure, a cast shadow, scenery around it. **Rule**: floating on pure white, nothing else; we add the ground and the shadow in the engine.

**`fire_states_01.png`** — 1×1, three states
```
2D game asset sheet, the same small stone-ringed campfire three times side by side, identical ring of stones, in three states: unlit with stacked logs, glowing orange embers, burning brightly with tall flames, in a single row, evenly spaced, each one isolated and at the same scale, high-angle three-quarter view, thick dark outlines, flat cel shading, lit from the front left, no cast shadows, floating on a plain pure white background with nothing else --ar 16:9 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no ground, grass, soil, shadow, scenery, background, frame, text, watermark
```

**`shelter_branches_01_summer.png`** — 1×2
```
single 2D game asset, a primitive lean-to shelter made of stacked branches and leaves, open on one side, about two metres long, alone, high-angle three-quarter view, thick dark outlines, flat cel shading, lit from the front left, no cast shadow, floating on a plain pure white background with nothing else --ar 4:3 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no ground, grass, soil, shadow, scenery, background, frame, text, watermark
```

**`rack_drying_01.png`** — 1×2
```
single 2D game asset, a primitive wooden rack of lashed poles with strips of meat hanging to dry, about two metres long, alone, high-angle three-quarter view, thick dark outlines, flat cel shading, lit from the front left, no cast shadow, floating on a plain pure white background with nothing else --ar 4:3 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no ground, grass, soil, shadow, scenery, background, frame, text, watermark
```

### Trees (six species, adult, two variants each; ages later)

**Made on 8 November 2026** with the prompt below, one per species and variant (the descriptions are in `content/art/trees/README.md`), the style reference **and** a clean oak on pure white as a second reference (`::2`); the chosen image is then edited in Midjourney to remove what is wrong, keeping every colour.
```
a single isolated <tree description>, nothing else in the image, high-angle three-quarter view, the whole tree in frame, on an empty plain pure white background, thick dark outlines, flat cel shading, lit from the front left --ar 2:3 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png <clean oak url>::2 --no ground, grass, grass tufts, soil, roots, rocks, stones, bushes, flowers, mushrooms, logs, path, fence, cast shadow, scenery, text, watermark --style raw
```

The first prompts, kept for the record:

**Risk**: grass and roots painted at the foot, a shadow, foliage hiding the trunk (bodies walk behind it). **Rule**: trunk clear on the lower third, floating on pure white.

**`oak_adult_01_summer.png`** — 1×1, 4 m tall
```
single 2D game asset, a large adult oak tree, thick trunk clearly visible on its lower third with no grass at its foot, dense rounded foliage clumps above, alone, high-angle three-quarter view, thick dark outlines, flat cel shading, lit from the front left, no cast shadow, floating on a plain pure white background with nothing else --ar 2:3 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no ground, grass, soil, shadow, scenery, background, frame, text, watermark
```

**`oak_adult_02_summer.png`** — 1×1, 4 m tall
```
single 2D game asset, a large adult oak tree with a different silhouette, one lower branch reaching out, thick trunk clearly visible on its lower third with no grass at its foot, dense rounded foliage above, alone, high-angle three-quarter view, thick dark outlines, flat cel shading, lit from the front left, no cast shadow, floating on a plain pure white background with nothing else --ar 2:3 --sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png --no ground, grass, soil, shadow, scenery, background, frame, text, watermark
```
