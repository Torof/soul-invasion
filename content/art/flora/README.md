# flora

The shrubs, plants and mushrooms of `content/flora.ron`, in one atlas
(`atlas.webp`), with each image's rectangle in `atlas.json` (`<id>_<nn>`):
one texture for the whole flora, so the client draws it in a few draw
calls (D39). Two images a kind; the client picks one by the plant's place.

- **Format**: each image as tall as its kind is at the closest zoom, 2.5,
  on a 2560-px-wide screen (D39's sharpness rule, shipped size: shown
  height × 307 px a metre; 25 px for a chanterelle, 768 px for an elder).
  The originals, at 384 px a metre, stay outside the repository. The client sizes
  each plant on screen by its kind's height (`height_m`, D40). The image's
  bottom middle is the plant's foot. Images start on a 4-pixel block, 8
  pixels apart. Transparent background; the clear pixels around each plant
  carry its colour, so it does not darken far away.
- **Made**: 9 November 2026 on Midjourney by the project lead, one prompt a
  kind and variant (`content/ai-reserve/notes/flora-prompts.md`), the
  style reference in each; wild flax, woad and yarrow made again as plants
  standing on one foot. Chosen, cut out and packed by
  `content/tools/flora_art.py` (white background and cast shadows removed,
  stray pieces cleared, cropped).
- **Sources**: the Midjourney outputs stay outside the repository
  (`content/ai-reserve/sources/flora/`).

Licensed under CC BY-NC-SA 4.0 like all content (`docs/LICENSE`).
