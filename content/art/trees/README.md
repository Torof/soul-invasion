# trees

The trees standing in the woods (D37): adult, one image a species (two for
the willow, see below) (`<species>_adult_<nn>_summer.webp`; the client takes `_01`, `_02`, … up to the first missing number), drawn by the client on
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
mapped quantile by quantile, and on the foliage only (yellow-green to
teal hues) hue turned and chroma scaled to the old means, so the bark,
the apples and the flowers keep their own colours; per pixel, the outlines
and the white background left untouched.
The uncorrected sources are kept in
`content/ai-reserve/sources/trees/originals/regeneration-2026-11-10/before-colour/`.

**10 November 2026, feet made whole**: in the game, a good number of the
redrawn trees showed holes, a semi-transparent band, stray strokes or
grass at the foot of their trunk. Causes and fixes:
- `flora_art.py` cleared every green pixel in the lowest 7 % of a tree,
  as grass. On the redrawn trees that took the dark green-black outline
  at the foot (the ash, the oak 02: a trunk with no outline, the outline
  above it running on alone), the grey-green bark (the alder 01, the
  willow 02's roots, the birch 01, the linden 02's moss: holes) and the
  fill of grass tufts while leaving their outlines (stray strokes). The
  rule is gone: the tool keeps the drawing as it is.
- Its rule for slivers of background between two outlines also took pale
  highlights on the birch's bark (small holes): a sliver is now cleared
  only when outlines or the background hem it in.
- Seven originals had something under the foot: grass (alder 01 and 02,
  ash 02, yew 02), a patch of white ground (yew 01), a grey ring (ash 02),
  or a torn foot without an outline (linden 01, wild apple 01).
  `content/tools/clean_tree_feet.py` cleans them, by recipes written in
  it, from untouched copies kept beside them
  (`<species>_<nn>_before-feet.png`): grass and ground cleared, the trunk
  closed with the bark around it, the edge of the foot rounded and given
  the tree's own outline.
- The client checks that every tree's foot is whole: no hole, no speck
  apart from it.

**17 November 2026, one image a species**: the project lead's decision ("une espèce, une image et le masque des fruits"), the variety now coming from the many species. Kept: version 01 of oak, pine, birch, hazel, wild apple, ash, linden and yew; version 02 of the alder, renamed `_01` (fuller, a double trunk); both willows, since the willow lines every bank and one image repeated there would show. The other images left the game for the reserve (`content/ai-reserve/sources/trees/retired-2026-11-17/`).

**17 November 2026, fruit in three states (D48)**: the oak, the hazel, the
yew and the pine have three images each: `<id>_adult_01_summer.webp`
bare, `_fruit1` with a third of their fruit, `_fruit2` laden, all three the
same size and frame. The project lead added the fruit to the bare tree with
a Midjourney edit and chose one result per tree (oak 5 with two more
clusters, hazel 1, yew 1, pine 0; `content/ai-reserve/review/fruits-arbres/`).
The bare and the third are that edit with whole clusters of fruit taken
away and the original tree put back in their place, so the three differ
only where the fruit is. The edit had darkened the foliage: each state was
given its old version's colours with `match_tree_colours.py` (one mapping
for the three), cut and sized as `flora_art.py --trees` does, at the bare
image's frame, and the white left inside the crown where fruit was taken
out filled with the leaves around it.

**17 November 2026, four fruit trees of their own**: the mulberry, the
sweet chestnut, the walnut and the wild pear stop being drawn with a
stand-in's images (`STAND_INS` in `client/world/trees.gd`). The project
lead made each tree laden in Midjourney (mulberry 2, chestnut 3 of the
first batch, walnut 1, pear 3), then the same tree without its fruit
with a Midjourney edit (mulberry 0, chestnut 1, walnut 1, pear 0). Here the
fruitless edit is the base: the bare state is that edit, the laden one the
edit with every fruit of the laden original pasted on it (the original
aligned on the base's crown, the fruit picked by colour, kept only where
it lies on the base's tree), the third the same with a third of the
clusters, spread out. The edit's foliage was lighter or darker than the
laden original's (the walnut much darker): one `match_tree_colours.py`
mapping, from the laden state to the original, was given to the three.
Then cut and sized at the bare state's frame as above. The walnut's and
the chestnut's sources came out shorter than their shipped size (2,203 and
1,881 px against 2,660 and 2,816): enlarged to it (Lanczos) before the
sharpening, so the card reads them at the same zooms as the other trees. Their fruit shows
while their crop is in season (`content/species.ron`). The fig waits for
its own redo.

**18 November 2026, fruit as sprites (D48)**: the three images a tree of
17 November (bare, `_fruit1`, `_fruit2`) were too heavy and are gone; every
tree keeps its bare image only. The fruit is drawn over it as sprites
(`fruit/<id>.webp`, `fruit/fruit.json`, made with
`content/tools/tree_fruit.py`). The wild pear is the first: a new tree,
less heavily outlined than the first one (first sheet of 18 November,
variant 2, with a Midjourney edit taking away its grass and shadow), and
four pears made whole on white in Midjourney (two single, two pairs),
brought to the colours of the first tree's pears. The other kinds with a
fruit window are drawn bare until their fruit is made.
The walnut and the sweet chestnut are back to the size their sources give
(2,203 and 1,881 px high, short of 2,660 and 2,816): nothing is enlarged.
They are read whole from a farther zoom until they are made again bigger.

**18 November 2026, oak, alder and linden drawn anew**: the project lead
found them hard to tell apart by their leaves. Each was first made as a
realistic photograph in Midjourney, then redrawn in the game's style with
that photograph as the image prompt and our hazel, yew and sweet chestnut
as style references, then cleared of its grass and shadow by a Midjourney
edit. The oak: a broad, twisted, knotted trunk, lobed leaves in the lit
clusters. The alder: narrow, conical and dark, two trunks, round leaves.
The linden: tall and regular, heart-shaped leaves (the other versions
drew maple leaves). They keep their own colours (no `match_tree_colours`:
they are new identities). The oak and the alder were made on a square
frame and are short of their size (1,850 and 1,886 px high, for 2,496 and
2,257): nothing is enlarged; a tall frame (`--ar 1:2` or `2:3`) is the
way to make a tree taller in pixels. The linden is 2,504 px, near its
2,660. The old sources are kept in
`content/ai-reserve/sources/trees/identite-2026-11-18/anciennes-sources/`.
`flora_art.py` now clears the pale, colourless pixels left of the white
background inside a tree (`clear_pale`, the client test's own measure),
except on the birch, the ash and the linden, pale by nature.

**18 November 2026, the ink line**: every tree has a dark outline at least
9 px wide at the shipped size (`INK_PX` in `flora_art.py`, about the
walnut's own line), the project lead's wish ("un contour un peu dessiné,
fort et épais… ça fait vraiment ressortir l'arbre"). It lies half outside
the silhouette and half inside, so the leaves of the edge stay seen; the
silhouette is first closed over small notches and holes and slightly
rounded, so the line runs on like a drawn one, and the bottom of a notch
it closes takes the leaves' colours. A line already darker is left as it
is: the trees drawn with a thick outline barely change. The canvas keeps
its size (the line is cut at the image's edges), so the foot, the
outlines and the fruit points do not move. Made with each tree from now
on; the fifteen images already shipped were given it once with
`flora_art.py --ink` (run again, it would draw a second line). Weight:
about +1 % on disk, nothing in video memory.

**18 November 2026, the wild cherry of its own**: the wild cherry stops
being drawn with the wild apple's images. The project lead made it in
Midjourney with the new identity's style board (image 4 of the second
grid): a slim red-brown trunk banded across, the bark a cherry is known
by, and an oval crown of toothed leaves. It came clean (no grass, no
shadow) and was made alone with `flora_art.py --trees` from a sources
folder holding only it, so it has the ink line and the other trees were
left as they were. It carries no fruit yet (the content gives it none).
The wild fig is the last tree still drawn with a stand-in's (the hazel's).

**18 November 2026, the yew back to its source**: the yew shipped was a
Midjourney edit made for the fruit states of 17 November, darker, with
small red arils left in its crown. It is made again from its own clean
source (`yew_01.png`), alone with `flora_art.py --trees`, so it has the
ink line and no red speck; the other trees were left as they were.

**18 November 2026, the wild fig and a new wild apple**: the last stand-in
goes (`STAND_INS` is empty: every kind has its own image). The project lead
made both in Midjourney with the style board; the fig is an edit of his
first fig, its leaves made bigger, the apple image 2 of its grid: a
leaning trunk in an S, an open crown in clumps. Both are drawn without
fruit, for the sprites (D48). The old wild apple's sources went to
`content/ai-reserve/sources/trees/identite-2026-11-18/anciennes-sources/`.
- The fig is a tree of many stems. The thick ink ringed the narrow gaps
  between its trunks with black, as slits; `INK_KEEP_HOLES` in
  `flora_art.py` names the kinds whose holes (clear parts not joined to the
  image's border) keep their own drawn line, only the outer edge inked.
- The apple's foot had a few thin blades of grass. Clearing the grass by
  its colour took the olive bark with it, so its recipe in
  `clean_tree_feet.py` only rounds the foot, which takes the blades.

**18 November 2026, every fruit tree's fruit**: the oak, hazel, pine, yew,
walnut, sweet chestnut, mulberry, wild cherry, wild apple and wild fig
get their fruit sprites, as the pear did (D48). The project lead made each
fruit in Midjourney as a sheet of four whole fruit on white, the tree as
style reference, and picked one sheet of four per kind (the sheets in
`content/ai-reserve/sources/trees/fruit-2026-11-18/<id>.png`). Made with:

    tree_fruit.py sprites <id> - <sheet>:0:0 <sheet>:0:1 <sheet>:1:0 <sheet>:1:1 --px N
    tree_fruit.py points <id>

- `-` as reference: the fruit keep their own colours, and their green (a
  husk, a burr); only the pear's sheet, edited from a laden tree, is
  recoloured.
- A size each, against the pear's 130 px: acorns 90, hazelnuts 100, pine
  cones 110 (drawn stalk down, turned with `:180`), yew arils 70,
  walnuts 105, chestnut burrs 110, mulberries 75, cherries 95, crab apples
  105, figs 100.
- The project lead wants trees well laden ("des arbres bien chargés"):
  up to 30 fruit a tree (COUNT), 140 px apart (SPACING), where there were
  16 and 175. The fig and the apple, outlined in thin brown lines, had
  almost no dark leaf edges to hang fruit from: places anywhere in the
  foliage now follow the edges, and only the branches (brown wider than a
  line) are kept clear. The pear's points were found again the same way.
- The white between a pair of cherries' stalks is enclosed by the fruit;
  such patches wider than ENCLOSED_WHITE are background too.
