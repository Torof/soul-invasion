# shore_pieces

The bank pieces, one image per straight heading (`straight_00` … `straight_15`)
and per one-step bend (`bend_XX_toland`, `bend_XX_towater`): 48 pieces, one or
two variants each (`name_1.png`, `name_2.png`, picked by the piece's position),
512 × 512 px drawn at 128 × 128 at scale 1 (four times the screen, like the
ground textures, so they stay sharp up to zoom 4), centred on the piece's middle sample
(`ShorePieces.middle`). On the water side the image is opaque for about 12 px and fades over 5;
on the land side it is opaque for 4 px past the lip, then fades smoothly over
13 px, and its grass is recoloured to the game's own grass (mean and spread of
each channel, away from the lip). It also fades just past the piece's two ends,
so neighbouring pieces blend. The client fills the ground up to the bank with its
own textures underneath (`ChunkView._draw_shore_pieces`).

- **Made**: 7 November 2026, on Scenario, with the project's style model
  `soul-invasion-ground-style-v1` (a Z-Image LoRA trained on 8 in-game views
  and 4 Midjourney bank references, epoch 6), image-to-image from the piece
  schematics of `client/dev/piece_schematics.gd` (strength 0.35, LoRA 0.8,
  28 steps, guidance 5, seed 11), two outputs per piece; the one whose bank
  drifts least from the schematic is `_1`, the other `_2` unless it drifts more
  than 9 % (three pieces have one variant).
- **Prompts**, by the face the bank shows at that heading:
  - earth face (headings 4–8): "a river bank seen from above at an angle,
    short meadow grass ending in a clearly visible lip of bare brown earth, a
    band of dark earth with small stones and a few roots along the whole edge,
    calm water below";
  - profile (1–3, 9–11): "a river edge seen from above at an angle, short
    meadow grass meeting calm water along a thin low edge of bare earth";
  - grass overhang (0, 12–15): "a river edge seen from above at an angle,
    short meadow grass overhanging calm water, no earth visible".
- **Sources**: the 1024 px outputs, both choices per piece, stay outside the
  repository (`content/ai-reserve/`).

Licensed under CC BY-NC-SA 4.0 like all content (`docs/LICENSE`).
