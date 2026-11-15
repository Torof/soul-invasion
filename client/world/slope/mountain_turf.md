# The mountain turf — a family of five tiles

The grass the slopes are drawn with (D44, `ground.gdshader`, `slope_lit`): the lead's mountain grass, drawn by our own renderer, as a **family of five interchangeable tiles** of the same grass. Each tile has its own pattern of blades; all five share one border band, so any tile lays beside any other, in any order, without a seam. The ground shader lays one tile a cell of the turf's repetition, chosen by a hash of the cell (`slope_turf_variant`), so the slopes read as one uniform grass, not as a picture repeated.

- **Files**: `mountain_turf_1.png` … `mountain_turf_5.png`, 1024 × 1024 each, RGB. 1024 px of tile is 384 px on the screen (`SLOPE_LOOK["slope_turf_px"]`). The look's tint inverse is already baked into the pixels, so the shader's `slope_tint` lays on them as it did on the single tile before.
- **How they are read**: `ChunkView.slope_turf_set` builds one `Texture2DArray` from the five at load, one layer a tile, with mipmaps; the array is laid out as `angles × variants` (one row of angles today) so a second index, the slope's steepness, can choose the row later without the layout changing. The developer overlay's key G lays tile 1 alone, to see what the family brings against one tile repeated.
- **Made**: 16 November 2026 by our renderer `cretes-vagues.py` (in the art reserve `content/ai-reserve/review/pentes/`, git-ignored; to be moved into the repository later), the five with one dial set and seeds 1 to 5: `--cream-none 1.0 --cream-a 0 --masses 0 --fur 0 --fat 0 --thin 0.6 --shade-w 0 --shade-a 0 --fringes 0.2 --bed-w 8 --bed-a 0.5 --ss 4 --spike 31 --cream-len 135 --family 7 --seed 1..5`.
- **Status**: validated by the project lead on 16 November 2026 as a first version ("on y est presque, fine-tuning après": nearly there, the fine-tuning comes after).
- **Source**: `content/ai-reserve/review/pentes/v13-famille/for-game/`.

Licensed under CC BY-NC-SA 4.0 like all content (`docs/LICENSE`).
