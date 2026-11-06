# 09 — Visual Production

*Version 0.2 — 6 November 2026. Supersedes the 0.1 handover of the same day, which was written without the engineering documents in view. This document rules the visuals; `04-architecture.md` and `05-first-version.md` rule the code.*

## 1. Why we produce visuals now

The charter says: do little, but very well; ship early; depth before size. Milestone 1 and half of milestone 2 are playable with provisional shapes of colour. The visual identity is fixed (`07-style-bible.md`); real images are produced **when the pull request that shows them is in sight, and not before**, so that no image is made without a cell to stand on.

## 2. How we work

- **Partnership.** Nothing is produced, coded or modified without the project lead's explicit approval. We list, validate, then produce.
- **Generation tool**: Midjourney. The lead generates; the AI writes the prompts (`08-midjourney-prompts.md`), analyses, cuts out, integrates.
- **Style reference** mandatory in every prompt: `--sref https://cdn.midjourney.com/0fb23b62-fb63-43e8-8f0d-ccb4fdcf6751/0_3.png` (`content/art/references/style-ref-01.png`).
- Every validated image lives in `content/art/` (content, CC BY-NC-SA 4.0) **at the size the game uses**, with a note beside it giving its prompt and date. Midjourney sources stay out of the repository.
- File names say what and when: `oak_adult_summer.png`, `ground_grass_01_summer.png`. One season for now; the others will come as `_autumn`, `_winter`, `_spring`.

## 3. The rendering: 2D isometric, confirmed (6 November 2026)

Three approaches were weighed: diamond tiles drawn one by one, a fully painted scenery, and a relief ground in 3D with flat elements facing the camera. The relief ground buys continuous hills; it costs exactly where this game lives — walls one cell thick, rooms, roofs hidden from inside, cities built cell by cell (D31), perception by line of sight (D18), combat facing eight ways (D33), thousands of items streamed by chunk (D08) — and it risks a 3D ground clashing with drawn elements.

**Decision: the world stays 2D isometric (D02), as Dofus, with our own identity.**

- The ground is a **2:1 diamond grid of one-metre cells**, drawn from **top-down textures projected onto the grid and blended by the engine** — no diamond tile has to come out of the AI.
- Elements (trees, rocks, shelters, items, bodies) are **flat images standing on the ground**, drawn in order of depth.
- Water, wind in the grass and trees, day and night, the fire as the only light at night, cast shadows: shaders and 2D lights in Godot, no image needed.
- **No map loading, ever**: the world arrives by chunks around the body and is drawn continuously.
- **Open roofs**: a building is walls on cells and a roof drawn above; when a watched body is inside, the roof is not drawn. A layer, not a problem.
- **Relief** comes later as **whole levels** (a cliff, a terrace), never as a continuous slope.
- The ground changes **by cell** (a worn path, a ploughed field, a dug pit) — the natural model of a grid.

## 4. The format contract

Every image obeys it, so that it has a place, a scale and an anchor the day it is made.

| | Rule |
|---|---|
| **Cell** | 1 m × 1 m. On screen, a diamond of **64 × 32 px** at scale 1 (zoom ×1 to ×4). **64 px = 1 m**, horizontally and vertically. |
| **Angle** | One angle for every element: the high-angle three-quarter of the reference. The 2:1 ground and elements drawn slightly more from the front sit together, as in Dofus. |
| **Grounds** | Seen **exactly from above**, no perspective, no outline, **no shadow**, calm and low in detail (details come from overlays), seamless. One texture covers **4 × 4 m**: 512 × 512 px (128 px per metre, enough for zoom ×2). |
| **Overlays** | Same as grounds, on a transparent background, sparse and well separated. |
| **Elements** | High-angle three-quarter, **plain white background** to be cut out, **no ground shadow**, light from the left and slightly in front. Each has a **footprint** in cells (1×1, 1×2, 2×2, 3×3) and is **anchored at the base of the centre of its footprint**. Height in metres sets the pixels: 64 px per metre at scale 1, generated at twice that. A 4 m tree is about 260 px tall at scale 1. |
| **Bodies and animals** | Cut-out puppets: **four facings mirrored into eight** (combat reads the facing, D33), a few states (standing, walking, lying asleep, dead). Midjourney gives the model and the turnaround sheet; animation is Godot's or a sprite-sheet tool's. The hardest class: last, one character first. |
| **Buildings** | For the first version, **whole structures with fixed footprints** (shelter 1×2, tent 2×2, hut 3×3, fire 1×1, drying rack 1×2, palisade by segment). Modular walls (segment on each diagonal, corner, door, roof pieces) come with cities and will need human touch-ups: Midjourney paints a hut, not a wall kit. |
| **Items on the ground** | One sheet per batch of materials, each item sized to lie on a cell, cut out individually. |

## 5. What is produced, and when

The library of prompts is `08-midjourney-prompts.md`; only what a milestone needs is taken from it.

**Batch 1 — now (milestone 2, pull requests 4 to 6: what the game already shows)** — about 22 images
- Grounds: grass ×2, forest floor ×1, ford / shallow water ×1, water ×1, cliff rock ×1 — the five terrains of the valley.
- Overlays: flowers, dead leaves, pebbles, moss.
- The **ten materials** as items on the ground: flint, pebble, branch, log, fibres, berries, hide, meat, bone, corpse — one sheet.
- Fire in three states, branch shelter, meat drying rack (pull request 6).
- One tree only, oak adult, two variants: the forest is a ground for now; trees become objects with pull request 6 (felling) and `species.ron`, and the six species in three ages come **then**.

**Batch 2 — pull requests 5 and 8**: tools (sharp flake, cordage, spear, stone axe); the **hunter** (model and four views); **one predator and one prey** (four views, walking, attacking, dead); the scale lineup of `08`.

**Batch 3 — milestones 3 to 5**: the tribe's variety (men, women, children, elders, by palette variants of one model), hut 3×3, tent, everyday objects.

**Later**: modular walls, roads, fields, seasons, the full set of tree species.

## 6. The tree species and their role

Kept from version 0.1, to be used when trees become objects. **Everything that grows is useful for something.** Names may be adapted to our world later.

| Species | Where | Role in game | Silhouette |
|---|---|---|---|
| Oak | Meadow, open forest | Abundant hardwood, acorns | Wide, stocky, massive trunk, cloud-like foliage |
| Birch | Forest edge | Useful bark, light wood | Thin, straight, white trunk with black marks, light foliage |
| Pine | Plateau, cliff top | Resin, straight timber | Tall, bare trunk, dark parasol at the top |
| Willow | Riverside | Supple branches for ties and basketry | Leaning, branches hanging to the ground |
| Hazel | Forest edge, undergrowth | Hazelnuts, straight shoots (spears) | Several stems from the ground, tall bush |
| Wild apple | Meadow | Fruit | Small, round, visible red apples |

Ages: young (thin, light foliage), adult, old (wider, gnarled, darker, sometimes a dead branch), giant (rare). Heights relative to a human: young 2, adult 4, old 5, giant 8. The older the tree, the more wood it yields. The trunk stays clear on the lower third so bodies walk behind it.

## 7. Settled and open

**Settled**: 1 cell = 1 m; 2D isometric at 64 × 32 px per cell; grounds top-down and seamless over 4 × 4 m, without outline or shadow; elements three-quarter on white, anchored at the base of their footprint; four facings mirrored into eight for bodies; whole structures first, modular walls later; summer only; production by batch, when needed.

**Open**: the pixel size of the first real cut-outs after the first batch is seen on screen (64 px per metre may become 80 or 96); how ground overlays are placed (per cell by the server as content, or scattered by the client as decoration — the first keeps D03 pure, the second is cheaper); the animation tool for bodies.

## 8. Next steps

1. The client goes to 64 × 32 px per cell and draws the ground from top-down textures through a shader (small pull request).
2. Batch 1 prompts, each with its file name, footprint and anchor, from `08`.
3. Generation by the lead; joint selection; the chosen images into `content/art/` with their notes.
4. Integration: ground textures, item sheet, fire and shelters as pull request 6 lands.
5. Judge the result on screen; fix the open points of section 7; then batch 2.
