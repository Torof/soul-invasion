#!/usr/bin/env python3
"""Makes the fruit a tree shows (D48): a few small sprites of its fruit,
and the points of each of its images where they hang.

Development tool, run by hand; the game never runs it. Needs Pillow and
numpy. From the repository's root:

    python3 content/tools/tree_fruit.py sprites <id> <reference> <cell>... [--px N]
    python3 content/tools/tree_fruit.py points <id>

`sprites`: each <cell> is `<image>:<row>:<column>`, one cell of a
Midjourney sheet of four (2 x 2) showing whole fruit on white, or
`<image>:<row>:<column>:180` for a fruit drawn stalk down. Each fruit
is cut from the white (the white joined to the cell's border), the cut
tip of its stalk trimmed, and sized `--px` high (SPRITE_PX unless given:
fruit differ in size). With a <reference> (the tree drawn with its fruit),
the little green leaves at its stalk are taken away and its colours
brought to those of the fruit there (quantile by quantile on the fruit's
skin, per channel): the pear's way, its sheet edited from a laden tree.
With `-`, the fruit keeps its own colours and its green (a husk, a burr):
the fruit drawn on their own on 18 November 2026. They go side
by side into one image, `art/trees/fruit/<id>.webp` (one texture for the
kind: a tree's fruit is drawn in one call), each centred on its stalk so
that it hangs from the middle of its top edge.

`points`: on each image of the kind (`art/trees/<id>_adult_<nn>_summer.webp`),
the places a fruit hangs from: the lower edge of a lit leaf, where dark
outlines meet, the fruit's whole footprint under it inside the crown and
clear of the branches and the trunk (brown), at least SPACING px apart,
at most COUNT; then, where those are too few, any place in the foliage
the fruit fits, lit leaves first. They are ordered so that the first third is spread over
the crown (farthest point first): a tree a third laden shows those.

Both write `art/trees/fruit/fruit.json`: per kind its sprites'
rectangles in its image, per tree image its points, in the image's texels
padded to the compression's blocks of 4 (as `outlines.json`).
"""

import json
import sys
from collections import deque
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

ART = Path(__file__).resolve().parents[1] / "art" / "trees"
FRUIT = ART / "fruit"
DATA = FRUIT / "fruit.json"
# A sprite's height, in the texels of the trees' images (307 px a shown
# metre, as they are made): the fruit's size on the tree.
SPRITE_PX = 130
# Room around each sprite in its image, so the smaller mipmap levels do
# not bleed one into the next.
PAD = 8
COUNT = 30
SPACING = 140
QUANTILES = np.linspace(0, 1, 101)


def hsv(rgb):
    h = np.asarray(Image.fromarray(rgb.astype(np.uint8)).convert("HSV")).astype(float)
    return h[..., 0] * 360 / 255, h[..., 1] / 255, h[..., 2] / 255


def skin(rgb):
    """The fruit's skin: warm, saturated, lit."""
    h, s, v = hsv(rgb)
    return (h > 15) & (h < 45) & (s > 0.35) & (v > 0.5)


def grow(mask, r):
    return np.asarray(Image.fromarray((mask * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(2 * r + 1))) > 0


def boxed(mask, r):
    return np.asarray(Image.fromarray((mask * 255).astype(np.uint8)).filter(ImageFilter.BoxBlur(r))).astype(float) / 255


def largest(mask, step=4):
    """The largest piece of `mask` (joined on a grid of `step` px)."""
    small = mask[::step, ::step]
    label = np.zeros(small.shape, np.int32)
    sizes = {}
    for y, x in zip(*np.nonzero(small)):
        if label[y, x]:
            continue
        n = len(sizes) + 1
        label[y, x] = n
        queue, count = deque([(y, x)]), 0
        while queue:
            cy, cx = queue.popleft()
            count += 1
            for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                ny, nx = cy + dy, cx + dx
                if 0 <= ny < small.shape[0] and 0 <= nx < small.shape[1] and small[ny, nx] and not label[ny, nx]:
                    label[ny, nx] = n
                    queue.append((ny, nx))
        sizes[n] = count
    if not sizes:
        return mask
    keep = np.kron(label == max(sizes, key=sizes.get), np.ones((step, step), bool))
    return grow(keep[: mask.shape[0], : mask.shape[1]], step + 1)


# A patch of white enclosed by the fruit wider than this (in a cell's
# pixels) is background too (between a pair of cherries' stalks); a
# highlight is smaller.
ENCLOSED_WHITE = 1500


def background(rgb):
    """The white around the fruit: near-white pixels joined to the border,
    and the large patches of it the fruit encloses (a pale highlight inside
    the fruit stays)."""
    white = rgb.min(2) > 225
    pad = Image.fromarray(np.pad(white, 1, constant_values=True).astype(np.uint8) * 255).copy()
    ImageDraw.floodfill(pad, (0, 0), 128)
    a = np.asarray(pad).copy()
    for y, x in zip(*np.nonzero(a == 255)):
        if a[y, x] != 255:
            continue
        one = Image.fromarray(a).copy()
        ImageDraw.floodfill(one, (int(x), int(y)), 64)
        patch = np.asarray(one) == 64
        a[patch] = 128 if patch.sum() > ENCLOSED_WHITE else 0
    return grow(a[1:-1, 1:-1] == 128, 2)


def sprite(rgb, target, px):
    """One fruit from a cell on white, recoloured to `target` (per channel
    quantiles of the reference's skin) unless None, trimmed and centred on
    its stalk."""
    if target is not None:
        m = skin(rgb)
        out = rgb.copy()
        for c in range(3):
            out[..., c] = np.interp(rgb[..., c], np.quantile(rgb[m][:, c], QUANTILES), target[c])
        weight = np.asarray(Image.fromarray((m * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(2))) / 255.0
        rgb = rgb + (out - rgb) * weight[..., None]
    alpha = np.where(background(rgb), np.clip((245 - rgb.min(2)) / 20, 0, 1), 1.0)
    if target is not None:
        green = (rgb[..., 1] > rgb[..., 0] + 8) & (rgb[..., 1] > rgb[..., 2] + 8)
        alpha[green] = 0
    alpha *= largest(alpha > 0.5)
    im = Image.fromarray(np.dstack([rgb, alpha * 255]).clip(0, 255).astype(np.uint8), "RGBA")
    im = im.crop(im.getbbox())
    im = im.resize((round(im.width * px / im.height), px), Image.LANCZOS)
    # The stalk's cut tip goes: the top 45 % of the narrow part.
    solid = np.asarray(im)[..., 3] > 128
    widths = solid.sum(1)
    stalk = np.nonzero(widths < 0.25 * widths.max())[0]
    stalk = stalk[stalk < im.height // 2]
    if len(stalk):
        im = im.crop((0, int(stalk.max() * 0.45), im.width, im.height))
    # Centred on the stalk's top.
    top = np.nonzero(np.asarray(im)[2, :, 3] > 128)[0]
    x = top.mean() if len(top) else im.width / 2
    half = int(np.ceil(max(x, im.width - x)))
    framed = Image.new("RGBA", (2 * half, im.height))
    framed.alpha_composite(im, (round(half - x), 0))
    return framed


def cell(spec):
    turn = 0
    if spec.endswith(":180"):
        spec, turn = spec[:-4], 180
    path, row, column = spec.rsplit(":", 2)
    im = Image.open(path).convert("RGB")
    w, h = im.width // 2, im.height // 2
    im = im.crop((int(column) * w, int(row) * h, (int(column) + 1) * w, (int(row) + 1) * h)).rotate(turn)
    return np.asarray(im).astype(float)


def padded(n):
    return -(-n // 4) * 4


def load():
    return json.loads(DATA.read_text()) if DATA.exists() else {"kinds": {}, "images": {}}


def save(data):
    FRUIT.mkdir(parents=True, exist_ok=True)
    DATA.write_text(json.dumps(data, sort_keys=True, separators=(",", ":")) + "\n")


def make_sprites(kind, reference, cells, px=SPRITE_PX):
    target = None
    if reference != "-":
        ref = np.asarray(Image.open(reference).convert("RGB")).astype(float)
        target = [np.quantile(ref[skin(ref)][:, c], QUANTILES) for c in range(3)]
    sprites = [sprite(cell(c), target, px) for c in cells]
    width = sum(s.width + 2 * PAD for s in sprites)
    sheet = Image.new("RGBA", (width, px + 2 * PAD))
    rects, x = [], 0
    for s in sprites:
        sheet.alpha_composite(s, (x + PAD, PAD))
        rects.append([x + PAD, PAD, s.width, s.height])
        x += s.width + 2 * PAD
    FRUIT.mkdir(parents=True, exist_ok=True)
    sheet.save(FRUIT / f"{kind}.webp", "WEBP", lossless=True, quality=100, method=6, exact=True)
    data = load()
    data["kinds"][kind] = {"sprites": rects}
    save(data)
    print(f"{kind}: {len(rects)} sprites, {sheet.width} x {sheet.height}")


def find_points(path, foot):
    """The points of one tree image (see the module's docstring)."""
    t = Image.open(path).convert("RGBA")
    a = np.asarray(t).astype(float)
    h, s, v = hsv(a[..., :3])
    opaque = a[..., 3] > 250
    leaf = opaque & (h > 65) & (h < 175) & (v > 0.25)
    # The branches and the trunk: brown, and wider than a drawn line (the
    # fig's and the apple's leaves are outlined in brown).
    brown = opaque & (h > 5) & (h < 50) & (s > 0.2) & (v > 0.15)
    brown = grow(~grow(~brown, 3), 3)
    dark = opaque & (v < 0.22)
    # The lower edge of a lit leaf: dark outline, the leaf just above, the
    # shade below.
    edge = dark & np.roll(leaf, 5, axis=0) & (np.roll(v, 5, axis=0) > 0.38) & (np.roll(v, 5, axis=0) > np.roll(v, -5, axis=0) + 0.05)
    meet = boxed(dark, 14)
    fw, fh = foot
    ok = boxed(~grow(~opaque, 18) & ~grow(brown, 22), 3) > 0.99
    rows = np.nonzero(opaque.any(1))[0]
    top, bottom = rows[0], rows[-1]
    cand = []
    for y, x in zip(*np.nonzero(edge)):
        if not top + 0.05 * (bottom - top) <= y <= top + 0.70 * (bottom - top):
            continue
        feet = ((y + 10, x), (y + fh // 2, x - fw // 3), (y + fh // 2, x + fw // 3), (y + fh - 10, x), (y + fh // 2, x))
        if all(0 <= py < ok.shape[0] and 0 <= px < ok.shape[1] and ok[py, px] for py, px in feet):
            cand.append((meet[y, x], int(x), int(y)))
    # Then any place in the foliage the fruit fits, lit leaves first, after
    # the edges: trees drawn with thin brown lines (the fig, the apple)
    # have few dark edges.
    for y, x in zip(*np.nonzero(leaf[::8, ::8])):
        y, x = 8 * y, 8 * x
        if not top + 0.05 * (bottom - top) <= y <= top + 0.70 * (bottom - top):
            continue
        feet = ((y + 10, x), (y + fh // 2, x - fw // 3), (y + fh // 2, x + fw // 3), (y + fh - 10, x), (y + fh // 2, x))
        if all(0 <= py < ok.shape[0] and 0 <= px < ok.shape[1] and ok[py, px] for py, px in feet):
            cand.append((v[y, x] - 2.0, int(x), int(y)))
    cand.sort(reverse=True)
    chosen = []
    for _, x, y in cand:
        if all((x - cx) ** 2 + (y - cy) ** 2 > SPACING ** 2 for cx, cy in chosen):
            chosen.append((x, y))
        if len(chosen) == COUNT:
            break
    if not chosen:
        return t.size, []
    # Farthest point first, from the highest: the first third is spread.
    p = np.array(chosen, float)
    order = [int(np.argmin(p[:, 1]))]
    while len(order) < len(p):
        d = np.min(np.linalg.norm(p[:, None] - p[order][None], axis=2), axis=1)
        d[order] = -1
        order.append(int(np.argmax(d)))
    return t.size, [list(chosen[i]) for i in order]


def make_points(kind):
    data = load()
    rects = data["kinds"].get(kind, {}).get("sprites", [])
    if not rects:
        sys.exit(f"{kind}: no sprites yet (run `sprites` first)")
    foot = (max(r[2] for r in rects), max(r[3] for r in rects))
    for path in sorted(ART.glob(f"{kind}_adult_*_summer.webp")):
        (w, h), points = find_points(path, foot)
        data["images"][path.name] = {"image": [padded(w), padded(h)], "points": points}
        print(f"{path.name}: {len(points)} points")
    save(data)


def main():
    args = sys.argv[1:]
    px = SPRITE_PX
    if "--px" in args:
        i = args.index("--px")
        px = int(args[i + 1])
        del args[i:i + 2]
    if len(args) >= 4 and args[0] == "sprites":
        make_sprites(args[1], args[2], args[3:], px)
    elif len(args) == 2 and args[0] == "points":
        make_points(args[1])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
