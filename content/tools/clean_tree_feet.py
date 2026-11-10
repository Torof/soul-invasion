#!/usr/bin/env python3
"""Cleans the foot of the trees whose original was drawn standing on
something: grass, a patch of ground, a grey halo, a ragged foot.

Development tool, run by hand; the game never runs it. Needs Pillow and
numpy. From the repository's root:

    python3 content/tools/clean_tree_feet.py <sources>

<sources> is `content/ai-reserve/sources/flora/`, the trees' originals on
white, as `flora_art.py --trees` reads them. Each tree of RECIPES is read
from its untouched copy `<id>_<nn>_before-feet.png` beside it (made from
`<id>_<nn>.png` the first time), cleaned, and written as `<id>_<nn>.png`:
running it again gives the same images. Then `flora_art.py --trees` ships
them. A tree must stand on nothing: the client draws its foot on the
ground, so its original ends in a closed trunk base with its outline.

What a recipe can do, all within the foot (the lowest `band` of the tree's
height), in this order:
- `halo`: clears a grey ring drawn around the outline (light, colourless
  pixels reached from the background);
- `grass`: clears the grass drawn at the foot (green pixels: hue, colour
  and lightness of grass, not of an outline or of a grey-green bark;
  `grass_saturation` lowers the colour wanted for a pale grass) and the
  dark strokes that outlined its blades;
- `ground`: clears a patch of white ground and its scribbles near what
  the grass clearing took (pale, nearly colourless pixels);
- `span`: clears what stands left or right of the foot (x from, x to, in
  the original's pixels: a patch of ground running off to the side);
- `smooth`: rounds the foot's outline (a closing then an opening, of that
  radius in pixels): the notches the grass left, a ragged edge;
then holes left inside the trunk are filled with the bark around them, what
is left of the foot is one piece, and its edge, where it changed, gets the
tree's own outline (its width and colour measured on the trunk just above).
"""

import sys
from collections import deque
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

sys.path.insert(0, str(Path(__file__).parent))
import flora_art  # noqa: E402

# id: what to do. Each line says why (what the original shows).
RECIPES = {
    # Two tufts of grass either side of the foot, the colour of the bark's
    # light green rim: cut off where they stand out of the trunk.
    "alder_01": {"band": 0.06, "span": (695, 880), "smooth": 4},
    # Grass all along the foot, on a dark strip of ground running off both
    # sides.
    "alder_02": {"band": 0.05, "grass": True, "span": (585, 1035), "smooth": 6},
    # Tufts of grass on both sides, some pale, and a grey ring around the
    # outline.
    "ash_02": {"band": 0.07, "halo": True, "grass": True, "grass_saturation": 0.15, "smooth": 5},
    # A torn, ragged foot with no outline (as Midjourney redrew it).
    "linden_01": {"band": 0.09, "smooth": 6},
    # A torn foot, left so when the grass and flowers were painted out.
    "wild_apple_01": {"band": 0.09, "smooth": 6},
    # A patch of white ground with grass strokes to the right of the foot.
    "yew_01": {"band": 0.07, "grass": True, "ground": True, "span": (370, 1058), "smooth": 7},
    # Tufts of dry grass in front of and between the roots.
    "yew_02": {"band": 0.16, "grass": True, "ground": True, "smooth": 8},
}

# Grass: hue (degrees), saturation and value (0..1) of the blades.
GRASS_HUE = (55.0, 170.0)
GRASS_SATURATION = 0.28
GRASS_VALUE = 0.33
# An outline pixel: its brightest channel under this.
DARK = 80
# A piece of bark in the foot smaller than this (px) is a speck.
SPECK = 400
# White ground: lighter than this, chroma under this.
GROUND_LIGHT = 130
GROUND_CHROMA = 40
# ... within this many pixels of what was cleared.
GROUND_REACH = 40
# The grey ring: colourless (chroma under this), lighter than an outline.
HALO_CHROMA = 26


def grow(mask, r):
    """`mask` grown by `r` pixels, with a round-ish brush (crosses and
    squares in turn: an octagon)."""
    m = mask.copy()
    for i in range(r):
        p = np.pad(m, 1)
        g = p[1:-1, 1:-1] | p[:-2, 1:-1] | p[2:, 1:-1] | p[1:-1, :-2] | p[1:-1, 2:]
        if i % 2:
            g |= p[:-2, :-2] | p[:-2, 2:] | p[2:, :-2] | p[2:, 2:]
        m = g
    return m


def shrink(mask, r):
    return ~grow(~mask, r)


def smooth(mask, sigma):
    """`mask` with its outline rounded: thin strands and narrow notches
    (under about `sigma` pixels) go, the shape stays."""
    blurred = Image.fromarray(mask.astype(np.uint8) * 255).filter(ImageFilter.GaussianBlur(sigma))
    return np.asarray(blurred) >= 128


def pieces(mask):
    """Labels of the 8-connected pieces of `mask`, and their sizes."""
    h, w = mask.shape
    labels = np.zeros((h, w), np.int32)
    sizes = [0]
    for y0, x0 in zip(*np.nonzero(mask)):
        if labels[y0, x0]:
            continue
        n = len(sizes)
        labels[y0, x0] = n
        q = deque([(y0, x0)])
        size = 0
        while q:
            y, x = q.popleft()
            size += 1
            for dy in (-1, 0, 1):
                for dx in (-1, 0, 1):
                    ny, nx = y + dy, x + dx
                    if 0 <= ny < h and 0 <= nx < w and mask[ny, nx] and not labels[ny, nx]:
                        labels[ny, nx] = n
                        q.append((ny, nx))
        sizes.append(size)
    return labels, sizes


def reached(free, seeds):
    """Pixels of `free` reached from the `seeds` mask (4-connected)."""
    h, w = free.shape
    seen = seeds & free
    q = deque(zip(*np.nonzero(seen)))
    while q:
        y, x = q.popleft()
        for ny, nx in ((y - 1, x), (y + 1, x), (y, x - 1), (y, x + 1)):
            if 0 <= ny < h and 0 <= nx < w and free[ny, nx] and not seen[ny, nx]:
                seen[ny, nx] = True
                q.append((ny, nx))
    return seen


def hsv(rgb):
    f = rgb.astype(np.float32) / 255.0
    mx, mn = f.max(axis=2), f.min(axis=2)
    d = mx - mn + 1e-6
    r, g, b = f[..., 0], f[..., 1], f[..., 2]
    hue = np.where(mx == r, ((g - b) / d) % 6, np.where(mx == g, (b - r) / d + 2, (r - g) / d + 4)) * 60
    sat = np.where(mx > 0, (mx - mn) / np.maximum(mx, 1e-6), 0)
    return hue, sat, mx


def outline_of(rgb, sil, rows):
    """The width (px) and colour of the tree's outline, measured on the
    trunk's two edges over `rows`."""
    dark = rgb.max(axis=2) < DARK
    widths, colours = [], []
    for y in rows:
        xs = np.nonzero(sil[y])[0]
        if len(xs) == 0:
            continue
        for start, step in ((xs[0], 1), (xs[-1], -1)):
            n, x = 0, start
            # Skip the soft edge (a pixel or two lighter than the outline).
            while 0 <= x < sil.shape[1] and sil[y, x] and not dark[y, x] and n < 3:
                x += step
                n += 1
            k = 0
            while 0 <= x < sil.shape[1] and dark[y, x]:
                colours.append(rgb[y, x])
                x += step
                k += 1
            if k:
                widths.append(k)
    width = int(round(np.median(widths))) if widths else 8
    colour = np.median(np.array(colours), axis=0) if colours else np.array([10, 14, 10])
    return max(3, min(width, 24)), colour


def inpaint(rgb, known, todo, passes=400):
    """Fills `todo` from the `known` pixels around it, ring by ring."""
    out = rgb.astype(np.float32).copy()
    have = known.copy()
    left = todo & ~have
    for _ in range(passes):
        if not left.any():
            break
        acc = np.zeros_like(out)
        cnt = np.zeros(have.shape, np.float32)
        for dy, dx in ((-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (-1, 1), (1, -1), (1, 1)):
            h = np.roll(have, (dy, dx), axis=(0, 1))
            acc += np.roll(out, (dy, dx), axis=(0, 1)) * h[..., None]
            cnt += h
        grow = left & (cnt > 0)
        out[grow] = acc[grow] / cnt[grow][:, None]
        have |= grow
        left &= ~grow
    return out


def clean(rgb, recipe):
    """The original `rgb` (int32, on white) with its foot cleaned."""
    out = rgb.copy()
    white = flora_art.border_colour(rgb)
    sil0 = ~flora_art.background(rgb, keep_white=False, tree=True)
    rows = np.nonzero(sil0.any(axis=1))[0]
    top, bottom = int(rows[0]), int(rows[-1])
    band_top = bottom - int((bottom - top) * recipe["band"])
    # Work on a window around the foot: the band, and as much above it.
    span = bottom - band_top
    y0, y1 = max(band_top - span, 0), min(bottom + 41, rgb.shape[0])
    cols = np.nonzero(sil0[band_top:bottom + 1].any(axis=0))[0]
    x0, x1 = max(int(cols[0]) - 40, 0), min(int(cols[-1]) + 41, rgb.shape[1])
    win = rgb[y0:y1, x0:x1]
    sil = sil0[y0:y1, x0:x1].copy()
    h, w = sil.shape
    band = np.zeros((h, w), bool)
    band[band_top - y0:] = True
    width, colour = outline_of(win, sil, range(max(band_top - y0 - span // 2, 0), band_top - y0))
    dark = win.max(axis=2) < DARK
    # The grey ring around the outline.
    if recipe.get("halo"):
        chroma = win.max(axis=2) - win.min(axis=2)
        grey = band & sil & (chroma < HALO_CHROMA) & ~dark
        sil &= ~(reached(grey | ~sil, ~sil) & sil)
    keep = sil.copy()
    if recipe.get("grass"):
        hue, sat, val = hsv(win)
        lo, hi = GRASS_HUE
        grass = (hue >= lo) & (hue <= hi) & (sat >= recipe.get("grass_saturation", GRASS_SATURATION)) & (val >= GRASS_VALUE)
        # With the pale, soft edge of each blade.
        keep &= ~(band & (grass | grow(grass, 1) & ~dark))
    if "span" in recipe:
        a, b = recipe["span"]
        xs = np.arange(w)[None, :] + x0
        keep &= ~(band & ((xs < a) | (xs > b)))
    if recipe.get("ground"):
        # Pale, nearly colourless pixels near what was cleared: the patch
        # of white ground and its scribbles.
        near = grow(band & sil & ~keep, max(3 * width, GROUND_REACH))
        chroma = win.max(axis=2) - win.min(axis=2)
        keep &= ~(band & near & (win.mean(axis=2) > GROUND_LIGHT) & (chroma < GROUND_CHROMA))
    # Where something was cleared, an outline is only kept along the bark:
    # the strokes that outlined the grass, or a strip of ground, go with
    # it. Specks of bark there (bits of grass not told apart) go first.
    near = grow(band & sil & ~keep, 2 * width)
    bark = keep & ~dark
    labels, sizes = pieces(bark & near)
    tiny = [i for i, s in enumerate(sizes) if i and s < max((3 * width) ** 2, SPECK)]
    bark &= ~np.isin(labels, tiny)
    stray = near & keep & dark & ~grow(bark, width + 2)
    keep &= ~(band & stray)
    # Holes left inside the trunk (grass that stood in front of it) are
    # filled; what opens on the background is background.
    holes = band & ~keep & ~reached(~keep, ~sil & ~band | ~grow(sil, 2))
    keep |= holes
    # The foot's outline rounded, then one piece.
    keep = np.where(band, smooth(keep, recipe.get("smooth", 4)), keep)
    labels, sizes = pieces(keep)
    main = labels == int(np.argmax(sizes[1:])) + 1
    keep &= main | ~band
    # Colours: the bark around fills what was not bark (the outlines are
    # left out of what the fill is made of).
    fill = keep & band & ~(sil & (bark | dark & ~stray))
    painted = inpaint(win, keep & ~fill & ~dark, fill)
    result = np.where(fill[..., None], painted, win.astype(np.float32))
    # The tree's outline along the foot's edge, wherever it lacks one.
    edge = keep & ~shrink(keep, width) & band
    need = edge & ~(dark & sil & ~fill)
    blend = np.asarray(Image.fromarray(need.astype(np.uint8) * 255).filter(ImageFilter.GaussianBlur(0.8))).astype(np.float32) / 255.0
    blend = np.maximum(blend, need.astype(np.float32)) * keep
    result = result * (1 - blend[..., None]) + colour[None, None, :] * blend[..., None]
    # What is no longer the tree, in the foot, becomes the background.
    result[band & ~keep] = white
    out[y0:y1, x0:x1] = np.clip(np.round(result), 0, 255).astype(np.int32)
    return out, {"outline": width, "colour": colour.round().astype(int).tolist(), "foot": (band_top, bottom)}


def main():
    sources = Path(sys.argv[1])
    only = sys.argv[2:]
    for name, recipe in RECIPES.items():
        if only and name not in only:
            continue
        path = sources / f"{name}.png"
        untouched = sources / f"{name}_before-feet.png"
        if not untouched.exists():
            Image.open(path).save(untouched)
        rgb = np.asarray(Image.open(untouched).convert("RGB")).astype(np.int32)
        cleaned, info = clean(rgb, recipe)
        Image.fromarray(cleaned.astype(np.uint8)).save(path)
        print(name, info)


if __name__ == "__main__":
    main()
