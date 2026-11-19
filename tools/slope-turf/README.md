# slope-turf — the mountain grass, drawn by our own renderer

`slope_turf.py` draws the slope turf the ground shader lays on sloping
ground (D44, `client/world/slope/mountain_turf.md`): a calm green with
wave crests of blades climbing a diagonal, the lead's mountain grass
reproduced as a function rather than bought from an image model, so a
hundred trials cost nothing. It is a drawing tool, run by hand; the game
never runs it and depends on nothing here.

Requires Python 3 with **Pillow** and **NumPy** (both are in Debian and
Ubuntu as `python3-pil` and `python3-numpy`; no other package).

## The recipe the game ships (v17 grass, v18 angle rows, 16 November 2026)

Twenty tiles: four crest angles (20, 32, 43 and 55 degrees above the
horizontal) by five seeds, one shared border band a row (`--family 7`),
drawn at 1024-scale with ×4 supersampling and delivered at 1536 px, the
`--game` copy divided by the shader's `slope_tint` so the tint lays on
it as on the single tile before:

```sh
for A in 20 32 43 55; do for S in 1 2 3 4 5; do
  python3 tools/slope-turf/slope_turf.py out/tile-$A-$S.png \
    --game client/world/slope/mountain_turf_${A}_$S.webp \
    --angle $A --seed $S --out-side 1536 --family 7 --margin 32 --blend 96 \
    --cream-none 1.0 --cream-a 0 --masses 0 --fur 0 --fat 0 \
    --thin 1.5 --dark-w 1.15 --wedge-l 0.85 --shade-w 0 --shade-a 0 \
    --fringes 0.2 --bed-w 8 --bed-a 0.5 --ss 4 --spike 40 --cream-len 135 \
    --band-jitter 12 --band-drift 10 --field-scale 420 --band-len 100
done; done
```

About 20 s a tile on one core (the twenty in two minutes on sixteen).
The dials, in the lead's words: no cream patches, no fur masses, no fat
flames (`--cream-none 1.0 --cream-a 0 --masses 0 --fur 0 --fat 0`);
blades 1.5× longer and wider than the first family (`--thin 1.5`), their
dark upper edge 1.15× wider (`--dark-w`), their lit lower edge 15 %
darker (`--wedge-l 0.85`); the crests jittered and drifted by a slow
random field so no line repeats across the tile (`--band-jitter`,
`--band-drift`, `--field-scale`); `--angle` the crests' climb, the
blades themselves upright whatever the row.

## What the picture is made of

Three values laid in order (the study is in the art reserve,
`content/ai-reserve/review/pentes/`, not in the repository): a striated
calm green; the crest lines, continuous diagonals of blades rising from
them like foam on a wave; a blade is two edges, a dark upper edge and a
pale lower edge with the body between, never a filled or outlined shape,
and never whole: bundles of curved partial strokes. Everything is drawn
at ×SS on a torus, so every tile is seamless by construction, and
downscaled with Lanczos; nothing but the cream (off in the recipe) is
blurred. In family mode the elements touching the border band come from
the family's seed and the tile's own fields yield to the family's over
`--blend` px, so any tile of a row lays beside any other without a seam.

The rules learnt from the lead while building it are the art reserve's
`montagne-algo/REGLES-APPRISES.md` (§I and §J); the ones the game relies
on: tips always to the upper half of the screen, up the slope; it is the
line of blades that climbs, not the blade that bends; a crest is one
continuous diagonal line.

Licensed like the code (`LICENSE`); the tiles it draws are content
(`docs/LICENSE`).
