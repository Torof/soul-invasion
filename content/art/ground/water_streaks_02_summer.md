# water_streaks_02_summer

The current's streaks over the river's water: a grey light map (mid-grey changes nothing, lighter and darker streaks along the texture's rows), read by the water meshes' shaders with its streaks turned along the current and scrolled at its pace, in two phases like the water; only where the water runs fast (`streak_slow` to `streak_fast` metres a second), so pools stay calm. The project lead's choice of 11 November 2026 among four Scenario trials (D36).

- **File**: `water_streaks_02_summer.png`, 512 × 512, greyscale, seamless both ways; 4 m a texture (`streak_metres`). Imported like the ground (BC7, mipmaps), read at an integer mipmap level a zoom step.
- **Made**: 11 November 2026 on Scenario with "Scenario Texture" (tileable, seam erasing on), from the water `ground_water_07_summer.png` as the style reference, asked as a light map of thin soft streaks of current, no colour.
- **Source**: `content/ai-reserve/sources/water/scenario-2026-11-11/streaks_02_lightmap.png`.

Licensed under CC BY-NC-SA 4.0 like all content (`docs/LICENSE`).
