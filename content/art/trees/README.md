# trees

The trees standing in the woods (D37): six species, adult, two variants
each (`<species>_adult_<nn>_summer.png`; the client takes `_01`, `_02`, … up to the first missing number), drawn by the client on
the point where the server says the trunk meets the ground.

- **Format**: sized by D39's sharpness rule (10 November 2026): originals
  at 384 px a shown metre (sharp at zoom 2.5 on a 3072-px-wide screen),
  shipped at 307 px a shown metre by `content/tools/flora_art.py --trees`
  (an oak shown 8.1 m is 2,413–2,496 px tall). A source smaller than its
  original target is shipped at its own size, never enlarged; the client
  sets each tree's height on screen. The bottom edge is the trunk's foot
  and the image is centred on it. Transparent background.
- **Made**: 7–8 November 2026 on Midjourney by the project lead, with the
  style reference and the prompt of `docs/08-midjourney-prompts.md` (Trees),
  plus a clean oak on white as a second reference; the first oak edited in
  Midjourney to remove a patch of grass and low foliage behind the trunk,
  its colours then restored from the original where the drawing had not
  changed. Cut out of their plain background, with the shadow and ground at
  the foot removed, and scaled to their height.
- **Descriptions** (each prompt's start):
  - oak 01: a large adult oak tree, one lower branch reaching out to the side;
  - oak 02: wide and stocky, one thick branch leaning, bare massive trunk;
  - birch 01, 02: slender, thin straight white trunk with black marks, light airy crown;
  - pine 01: tall scots pine, bare reddish trunk, dark green parasol crown;
  - pine 02: a slightly bent reddish trunk, an uneven parasol crown; generated with a bush at its foot, removed by the project lead in Midjourney's edit mode (8 November 2026), the colours kept;
  - willow 01: branches hanging like a curtain, parted so the trunk shows;
  - willow 02: a gnarled leaning trunk under a curtain of branches; generated with a ring of grass and a dark outline round its foot, removed by the project lead in Midjourney's edit mode (8 November 2026), the colours kept;
  - hazel 01, 02: a tall bush of thin stems rising from one point, rounded leafy top;
  - wild apple 01, 02: small, round crown with a few red apples.
- **Sources**: the Midjourney outputs stay outside the repository
  (`content/ai-reserve/sources/trees/`, the cut images and their originals).

Licensed under CC BY-NC-SA 4.0 like all content (`docs/LICENSE`).

**8 November 2026, second cut**: every tree is now cut out by shape (the background reached from the border through its own colour, the trunk followed down and protected). The second hazel, the second wild apple and the second willow had grass and a shadow painted at their foot that could not be told from their bark: the project lead removed them in Midjourney's edit mode, and these edits are the images used.

**8 November 2026, regenerated**: the second hazel and the first oak, too soft at their source, were generated again by the project lead (`new_hazel.png`, `new_oak.png`) and replace the earlier ones. All trees are cut from their sources in one pass, sharpened once after the reduction, and checked by a client test (standing on their foot, no pale speck).

**9 November 2026, four more**: alder, ash, linden and yew, two variants
each, made on Midjourney by the project lead with the style reference and
the clean oak on white as a second reference, cut out by
`content/tools/flora_art.py` (the grass at their foot removed) and scaled
to their height (alder 4.5 m, ash and linden 5 m, yew 3.5 m).

**10 November 2026, made sharp**: all twenty trees were redrawn sharp at
the size of D39's rule. The project lead gave each one's existing image
(its cut on white) to Midjourney's edit with prompt B of the pine quality
test (`docs/08-midjourney-prompts.md`, Trees, "Made sharp"): the same
drawing, silhouette and colours, only crisp outlines and clean shapes. Out
of four results per tree he chose one. Then `flora_art.py --trees` cut and
shipped them. Two cut fixes:
- the tool now leaves a tree's foot alone when most of it is green: the
  trunk itself is green (the first willow's bark, the second pine's mossy
  foot);
- the first wild apple's source had a ring of grass and white flowers at
  its foot; it was painted out of the source before the cut (the original
  is kept beside it as `wild_apple_01_before-cleanup.png`).

The chosen images are the sources
(`content/ai-reserve/sources/flora/<species>_<nn>.png`); Midjourney's
sheets of four and the earlier sources are kept in
`content/ai-reserve/sources/trees/originals/`, outside the repository.
Midjourney's real detail stops at about 1,300–1,800 px tall whatever its
upscale: the tallest trees have about 55–80 % of the detail their shipped
size could hold.

**10 November 2026, colours brought back**: the edit had made the trees
darker and cooler than before. Each new source was given its old
version's colours with `content/tools/match_tree_colours.py`: lightness
mapped quantile by quantile, hue turned and chroma scaled to the old
means, per pixel, the outlines and the white background left untouched.
The uncorrected sources are kept in
`content/ai-reserve/sources/trees/originals/regeneration-2026-11-10/before-colour/`.
