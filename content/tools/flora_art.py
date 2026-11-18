#!/usr/bin/env python3
"""Turns the flora's Midjourney images into the game's art (D39).

Development tool, run by hand; the game never runs it. Needs Pillow and
numpy. From the repository's root:

    python3 content/tools/flora_art.py <sources> content/art [--trees [--tree-zoom Z]]
    python3 content/tools/flora_art.py --ink content/art <tree image>...

<sources> holds one image per kind and variant, on a white background,
named `<id>_<nn>.png` (`bramble_01.png`), as `content/ai-reserve/sources/flora/`.

For each image:
- the background is cut away: everything joined to the image's border
  through pixels close to the border's own colour (cream or white), and
  what is joined to it through light, colourless pixels (a cast shadow);
- small clear holes inside the plant (white showing between leaves, which
  became pale specks on the trees) are cleared too, except on mushrooms,
  which are white themselves;
- the edge is softened by one pixel, then the plant is cropped to itself.

Sizes follow the sharpness rule (D39, 10 November 2026), in two sizes.
The originals (the sources, the archive, never shipped) are made at 384 px
a shown metre: sharp at the closest zoom, 2.5, on the project lead's 3072 x
1920 screen. The game ships them reduced to 307.2 px a shown metre: sharp
at zoom 2.5 on a 2560-px-wide screen and below, magnified 1.25 times at the
closest zoom only on wider ones. An image's height is its kind's shown
height (`shown`, the client's `Scale.shown`, D40) times `px_per_m()`.
Changing the shipped size is a parameter (SHARP_SCREEN_STRETCH) and a
rerun. A source smaller than its original target is reported: only those
need making again; nothing is ever enlarged. Trees are made only with
`--trees` (at `--tree-zoom`, ZOOM_MAX unless given); without it they are
left untouched.

Kinds of `content/species.ron` (trees) go to `art/trees/` as
`<id>_adult_<nn>_summer.webp`. The rest go into one atlas,
`art/flora/atlas.webp`, with `art/flora/atlas.json` giving each image's
rectangle: one texture for all the flora, so the client draws it in a few
draw calls (D39); the client sets each plant's size on screen. Last, the
decorations of `art/decor/` wider than their target (their width on screen
is `size_m` of the ground spec's "decor" entry) are reduced in place.

Everything shipped is written as lossless WebP (E3): the same pixels as a
PNG, about a third lighter in git (`client/tools/webp_same_pixels.py`
proves it).
"""

import json
import re
import sys
from collections import deque
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

# The sharpness rule: pixels per shown metre = PX_PER_M (Iso.PX_PER_M, at
# zoom 1) x ZOOM_MAX (the camera stops there) x a screen's stretch of the
# 1280 x 720 viewport.
# - Originals, the archive, never shipped: the project lead's 3072 x 1920
#   screen, of which Godot keeps 3072 x 1728: stretch 2.4, 384 px a metre.
# - Shipped: a 2560-px-wide screen (1440p): stretch 1.92, 307.2 px a metre.
PX_PER_M = 64
ZOOM_MAX = 2.5
ORIGINAL_SCREEN_STRETCH = 2.4
SHARP_SCREEN_STRETCH = 1.92
# The cut is worked at most this tall unless the target needs more.
WORK_MAX = 1400
ATLAS_WIDTH = 4096
# Godot's GPU compression works on blocks of 4 x 4 pixels: every image
# starts on a block and is PAD clear pixels from the next, so no block
# mixes two plants and the mipmaps of one do not reach the next.
BLOCK = 4
PAD = 8
# Colour distance (0..441) still counted as the background, and as a
# shadow: light (above SHADOW_LIGHT) and nearly colourless.
BG_TOLERANCE = 42
SHADOW_LIGHT = 150
SHADOW_CHROMA = 34
HOLE_TOLERANCE = 24
HOLE_MIN = 12
# On a tree, a patch of pale (above SLIVER_LIGHT), colourless pixels
# smaller than SLIVER_MAX (in the worked image) is background showing
# between two outlines.
SLIVER_LIGHT = 186
SLIVER_CHROMA = 16
SLIVER_MAX = 400
# ... when at least SLIVER_HEMMED of what surrounds it is outline (darker
# than SLIVER_DARK) or background.
SLIVER_DARK = 90
SLIVER_HEMMED = 0.75
TRUNK_SHARE = 1 / 3


def save_webp(image, path):
    """Writes a shipped image as lossless WebP, the colour under
    transparent pixels kept (`exact`), so its pixels are the PNG's."""
    image.save(path, "WEBP", lossless=True, quality=100, method=6, exact=True)


def shown(metres):
    """The height a thing is drawn at (D40, the client's Scale.shown): true
    up to 3 m, eased above."""
    return metres if metres <= 3.0 else 3.0 * (metres / 3.0) ** 0.5


def px_per_m(zoom=ZOOM_MAX, stretch=SHARP_SCREEN_STRETCH):
    """Pixels per shown metre at `zoom` on a screen of that `stretch`: the
    shipped size unless told otherwise."""
    return PX_PER_M * zoom * stretch


def target_px(metres, zoom=ZOOM_MAX, stretch=SHARP_SCREEN_STRETCH):
    """The sharpness rule: the pixels a thing `metres` tall takes at `zoom`
    on a screen of that `stretch`."""
    return round(shown(metres) * px_per_m(zoom, stretch))


def border_colour(rgb):
    edge = np.concatenate([rgb[0], rgb[-1], rgb[:, 0], rgb[:, -1]])
    return np.median(edge, axis=0)


def flood(mask_ok, seeds):
    """Pixels reachable from `seeds` through `mask_ok` (4-connected)."""
    h, w = mask_ok.shape
    seen = np.zeros_like(mask_ok)
    q = deque()
    for y, x in seeds:
        if mask_ok[y, x] and not seen[y, x]:
            seen[y, x] = True
            q.append((y, x))
    while q:
        y, x = q.popleft()
        for ny, nx in ((y - 1, x), (y + 1, x), (y, x - 1), (y, x + 1)):
            if 0 <= ny < h and 0 <= nx < w and mask_ok[ny, nx] and not seen[ny, nx]:
                seen[ny, nx] = True
                q.append((ny, nx))
    return seen


def components(mask):
    """Labels of the 4-connected regions of `mask`, and their sizes."""
    h, w = mask.shape
    labels = np.zeros((h, w), dtype=np.int32)
    sizes = [0]
    for y0 in range(h):
        for x0 in range(w):
            if mask[y0, x0] and labels[y0, x0] == 0:
                n = len(sizes)
                labels[y0, x0] = n
                q = deque([(y0, x0)])
                size = 0
                while q:
                    y, x = q.popleft()
                    size += 1
                    for ny, nx in ((y - 1, x), (y + 1, x), (y, x - 1), (y, x + 1)):
                        if 0 <= ny < h and 0 <= nx < w and mask[ny, nx] and labels[ny, nx] == 0:
                            labels[ny, nx] = n
                            q.append((ny, nx))
                sizes.append(size)
    return labels, sizes


def keep_main(alpha):
    """Clears the pieces apart from the plant (a watermark, a stray leaf):
    the 8-connected regions under 2 % of the largest one."""
    mask = alpha > 0
    h, w = mask.shape
    labels = np.zeros((h, w), dtype=np.int32)
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
    biggest = max(sizes)
    small = [i for i, s in enumerate(sizes) if i and s < biggest * 0.02]
    alpha[np.isin(labels, small)] = 0
    return alpha


def background(rgb, keep_white, tree=False):
    """What of `rgb` (an image on a plain background, int32) is the
    background: True there."""
    h, w, _ = rgb.shape
    bg = border_colour(rgb)
    distance = np.sqrt(((rgb - bg) ** 2).sum(axis=2))
    light = rgb.mean(axis=2)
    chroma = rgb.max(axis=2) - rgb.min(axis=2)
    like_bg = distance < BG_TOLERANCE
    shadow = (light > SHADOW_LIGHT) & (chroma < SHADOW_CHROMA)
    border = [(y, x) for y in range(h) for x in (0, w - 1)] + [(y, x) for x in range(w) for y in (0, h - 1)]
    outside = flood(like_bg, border)
    # A shadow touching the background goes with it.
    grow = outside | shadow
    outside = flood(grow, border)
    if not keep_white:
        labels, sizes = components((distance < HOLE_TOLERANCE) & ~outside)
        holes = np.isin(labels, [i for i, s in enumerate(sizes) if i and s >= HOLE_MIN])
        outside |= holes
    if tree:
        # Slivers of the background left between two outlines: small
        # patches of pale, colourless pixels. On the trunk (the lowest
        # TRUNK_SHARE of the tree), only those hemmed in by outlines or by
        # the background: a pale patch of bark (a highlight on the birch's
        # or the ash's trunk) has bark around it, and stays, or it would be
        # a hole in the trunk.
        pale = (light > SLIVER_LIGHT) & (chroma < SLIVER_CHROMA) & ~outside
        labels, sizes = components(pale)
        small = [i for i, s in enumerate(sizes) if i and s < SLIVER_MAX]
        hemmed = hemmed_in(labels, small, outside | (light < SLIVER_DARK))
        rows = np.nonzero((~outside).any(axis=1))[0]
        trunk_top = rows[-1] - (rows[-1] - rows[0]) * TRUNK_SHARE
        lowest = np.zeros(len(sizes))
        ys = np.nonzero(labels)[0]
        np.maximum.at(lowest, labels[labels > 0], ys)
        outside |= np.isin(labels, [i for i in small if hemmed[i] >= SLIVER_HEMMED or lowest[i] < trunk_top])
    return outside


def hemmed_in(labels, which, wall):
    """For each region of `labels` listed in `which`: the share of the
    pixels around it (up to 2 px away) that are `wall`."""
    sliver = np.isin(labels, which)
    lab = np.where(sliver, labels, 0)
    ring = np.zeros_like(lab)
    for dy in range(-2, 3):
        for dx in range(-2, 3):
            moved = np.roll(lab, (dy, dx), axis=(0, 1))
            ring = np.where((ring == 0) & (moved > 0) & ~sliver, moved, ring)
    n = int(labels.max()) + 1
    total = np.bincount(ring.ravel(), minlength=n)
    walls = np.bincount(ring.ravel(), weights=wall.ravel(), minlength=n)
    return walls / np.maximum(total, 1)


def cut(path, keep_white, tree=False, work=WORK_MAX):
    """The plant of `path` on a clear background, cropped to itself."""
    im = Image.open(path).convert("RGB")
    # Work at most `work` px tall: enough for the target, and faster.
    if im.height > work:
        im = im.resize((round(im.width * work / im.height), work), Image.LANCZOS)
    rgb = np.asarray(im).astype(np.int32)
    outside = background(rgb, keep_white, tree)
    alpha = np.where(outside, 0, 255).astype(np.uint8)
    alpha = keep_main(alpha)
    # One pixel in from the edge before softening it: the outermost ring
    # is the white background blended into the outline (pale specks).
    a = Image.fromarray(alpha).filter(ImageFilter.MinFilter(3)).filter(ImageFilter.GaussianBlur(0.7))
    out = im.convert("RGBA")
    out.putalpha(a)
    box = Image.fromarray(alpha).getbbox()
    return out.crop(box)


def trim_to_foot(image):
    """Crops the bottom rows the softened edge left faint, so the foot is
    the image's last row (the client stands the tree on it). The last row
    kept is nearly opaque, so it stays so once the GPU's compression (BC7)
    has rounded its alpha."""
    alpha = np.asarray(image)[..., 3]
    rows = np.nonzero(alpha.max(axis=1) > 240)[0]
    return image.crop((0, 0, image.width, int(rows[-1]) + 1))


def bleed(image, passes=8):
    """Gives the clear pixels next to a plant the plant's colour (still
    clear): read smaller far away (mipmaps), a plant's edge then blends
    with its own colour, not with black."""
    a = np.asarray(image).astype(np.float32)
    rgb, alpha = a[..., :3].copy(), a[..., 3]
    filled = alpha > 0
    for _ in range(passes):
        acc = np.zeros_like(rgb)
        cnt = np.zeros(alpha.shape, np.float32)
        for dy, dx in ((-1, 0), (1, 0), (0, -1), (0, 1)):
            sf = np.roll(filled, (dy, dx), axis=(0, 1))
            acc += np.roll(rgb, (dy, dx), axis=(0, 1)) * sf[..., None]
            cnt += sf
        grow = ~filled & (cnt > 0)
        rgb[grow] = acc[grow] / cnt[grow][:, None]
        filled |= grow
    out = np.dstack([rgb, alpha]).clip(0, 255).astype(np.uint8)
    return Image.fromarray(out, "RGBA")


# Trees pale by nature (the birch's and the ash's bark, the linden's
# flowers), left as they are by `clear_pale`.
PALE_TREES = ("birch", "ash", "linden")
# Trees of many stems: the gaps between their trunks keep their own drawn
# line, the thick ink ringed them as black slits.
INK_KEEP_HOLES = ("wild_fig",)


def clear_pale(image):
    """Clears what is left of the white background inside a tree: pale,
    colourless pixels (a sliver along a hole's rim, a dot between leaves),
    the client's test `has no pale specks` reading them so. The light the
    trees are drawn with is pale too, but yellow, and stays."""
    a = np.asarray(image).copy()
    rgb = a[..., :3].astype(np.int16)
    lo, hi = rgb.min(axis=2), rgb.max(axis=2)
    pale = (a[..., 3] > 102) & (lo > 186) & (hi - lo <= 20)
    a[..., 3][pale] = 0
    return Image.fromarray(a)


# The ink line (18 November 2026): every tree gets a dark outline at least
# INK_PX wide at the shipped size (307.2 px a shown metre; the walnut's own
# line is about 7), half outside its silhouette and half inside, so the
# leaves of the edge stay seen. The silhouette is first closed over small
# notches and holes (INK_CLOSE px) and rounded (INK_ROUND, a blur), so the
# line runs on like a drawn one instead of ringing every gap; the bottom of
# a notch it closes takes the colours of the leaves around it. The canvas
# keeps its size: the line is cut at the image's edges, so the foot, the
# outline of `outlines.json` and the fruit points stay where they were. A
# line already darker than the ink is left as it is.
INK = np.array([12, 14, 10], np.float32)
INK_PX = 9
INK_CLOSE = 5
INK_ROUND = 2.0


def _step(mask, grow_it, diagonal):
    p = np.pad(mask, 1, constant_values=not grow_it)
    n = (p[:-2, 1:-1], p[2:, 1:-1], p[1:-1, :-2], p[1:-1, 2:])
    if diagonal:
        n += (p[:-2, :-2], p[:-2, 2:], p[2:, :-2], p[2:, 2:])
    out = mask.copy()
    for m in n:
        out = out | m if grow_it else out & m
    return out


def ink_grow(mask, r):
    for i in range(r):
        mask = _step(mask, True, i % 2)
    return mask


def ink_shrink(mask, r):
    for i in range(r):
        mask = _step(mask, False, i % 2)
    return mask


def fill_from_around(rgb, known, todo, passes=60):
    """The colours of the `todo` pixels taken from their known neighbours,
    ring by ring."""
    rgb, known = rgb.copy(), known.copy()
    h, w = known.shape
    for _ in range(passes):
        if not (todo & ~known).any():
            break
        p = np.pad(rgb, ((1, 1), (1, 1), (0, 0)))
        k = np.pad(known, 1).astype(np.float32)
        total = np.zeros_like(rgb)
        count = np.zeros(known.shape, np.float32)
        for dy in range(3):
            for dx in range(3):
                if dy == dx == 1:
                    continue
                kk = k[dy:dy + h, dx:dx + w]
                total += p[dy:dy + h, dx:dx + w] * kk[..., None]
                count += kk
        new = todo & ~known & (count > 0)
        rgb[new] = total[new] / count[new][:, None]
        known |= new
    return rgb


def enclosed(clear):
    """The parts of `clear` (a mask) not joined to the image's border."""
    # A copy: an image made from an array shares its memory, read only,
    # and the fill would leave it unchanged.
    pad = Image.fromarray(np.pad(clear, 1, constant_values=True).astype(np.uint8) * 255).copy()
    ImageDraw.floodfill(pad, (0, 0), 128)
    return np.asarray(pad)[1:-1, 1:-1] == 255


def ink(image, px, keep_holes=False):
    """`image` (a tree, RGBA) with its ink line, `px` wide. With
    `keep_holes`, the holes inside the tree (the gaps between the trunks
    of a tree of many stems) keep their own drawn line: only the outer
    edge is inked."""
    a = np.asarray(image.convert("RGBA")).astype(np.float32)
    mask = a[..., 3] >= 128
    closed = ink_shrink(ink_grow(mask, INK_CLOSE), INK_CLOSE)
    blurred = Image.fromarray(closed.astype(np.uint8) * 255).filter(ImageFilter.GaussianBlur(INK_ROUND))
    drawn = np.asarray(blurred) >= 128
    holes = enclosed(~drawn) if keep_holes else np.zeros_like(drawn)
    # The line follows `shape`: the tree with its holes filled when they
    # keep their own line.
    shape = drawn | holes
    half = px / 2
    n = int(np.ceil(half)) + 1
    # Depth inside the shape (1 on its edge) and distance outside it.
    depth = np.zeros(shape.shape, np.float32)
    m = shape.copy()
    for _ in range(n + 1):
        depth += m
        m = ink_shrink(m, 1)
    away = np.full(shape.shape, n + 2, np.float32)
    m = shape.copy()
    for i in range(1, n + 2):
        g = ink_grow(m, 1)
        away[g & ~m] = i
        m = g
    w_in = np.clip(half + 1 - depth, 0, 1) * shape
    w_out = np.clip(half + 1 - away, 0, 1) * ~shape
    # Colours taken 2 px inside the drawing, never from its cut edge, which
    # holds some of the cream background.
    # A hole takes the colours around it too, though clear: the texture's
    # filtering mixes them into its rim.
    rgb = fill_from_around(a[..., :3], ink_shrink(mask, 2) & drawn, shape & ~ink_shrink(mask, 2))
    lighter = rgb.mean(axis=2) > INK.mean()
    k = (w_in * lighter)[..., None]
    rgb = rgb * (1 - k) + INK * k
    # Outside, the ink's colour everywhere, even where clear: a compressed
    # texture mixes the colours of a block, and the white left there showed
    # as pale specks along the line.
    rgb[~shape] = INK
    a[..., :3] = rgb
    a[..., 3] = np.where(holes, 0.0, np.where(shape, 255.0, w_out * 255))
    return Image.fromarray(np.clip(np.round(a), 0, 255).astype(np.uint8))


def species_heights(content):
    text = (content / "species.ron").read_text()
    return {m[0]: float(m[1]) for m in re.findall(r'id: "([a-z_]+)".*?height_m: ([0-9.]+)', text, re.S)}


def flora_kinds(content):
    text = (content / "flora.ron").read_text()
    out = {}
    for section in ("trees", "shrubs", "plants", "mushrooms"):
        start = text.index(f"{section}:")
        rest = text[start:]
        end = rest.index("\n    ],")
        for m in re.finditer(r'id: "([a-z_]+)".*?height_m: ([0-9.]+)', rest[:end], re.S):
            out[m.group(1)] = (section, float(m.group(2)))
    return out


def fit(image, tall, original, name, short):
    """`image` brought to `tall` px high (the shipped size), never enlarged.
    A source under `original` (the size its original should have) is noted
    in `short`."""
    if image.height < original:
        short.append(f"{name}: {image.height} px, original target {original}, shipped {tall}")
    if image.height <= tall:
        return image
    return image.resize((round(image.width * tall / image.height), tall), Image.LANCZOS)


def work_height(tall):
    """How tall to work the cut so the plant itself still reaches `tall`
    (a plant fills about two thirds of its source)."""
    return max(WORK_MAX, round(tall * 1.6))


def pack(plants):
    """Skyline packing, tallest first: each image goes where it ends
    highest up, on a block and PAD from its neighbours. {name: [x, y, w,
    h]} and the atlas height (a whole number of blocks)."""
    def up(v):
        return -(-v // BLOCK) * BLOCK

    # The height already taken in each column of blocks.
    columns = [0] * (ATLAS_WIDTH // BLOCK)
    rects = {}
    for name, image in sorted(plants, key=lambda p: (-p[1].height, -p[1].width)):
        span = up(image.width + PAD) // BLOCK
        need = up(image.width) // BLOCK
        best = None
        for c in range(len(columns) - need + 1):
            top = max(columns[c:c + span])
            if best is None or top < best[0]:
                best = (top, c)
        top, c = best
        rects[name] = [c * BLOCK, top, image.width, image.height]
        for i in range(c, min(c + span, len(columns))):
            columns[i] = up(top + image.height + PAD)
    return rects, up(max(y + h for _, y, _, h in rects.values()))


def size_decor(art, short):
    """Reduces in place the decorations wider than their shipped width
    (the client draws them `size_m` wide, ChunkView._draw_decor). They have
    no originals apart: what is reduced here is the only copy."""
    spec = json.loads((art.parent.parent / "client" / "world" / "ground.json").read_text())
    for kind, rule in spec.get("decor", {}).items():
        if not isinstance(rule, dict):
            continue
        wide = round(float(rule.get("size_m", 0.5)) * px_per_m())
        for path in sorted((art / "decor").glob(f"{kind}_[0-9][0-9].webp")):
            image = Image.open(path)
            if image.width > wide:
                image = image.resize((wide, round(image.height * wide / image.width)), Image.LANCZOS)
                save_webp(image, path)
                print("decor", path.name, image.size)
            elif image.width < wide:
                short.append(f"{path.name}: {image.width} px wide, shipped target {wide}")


def ink_shipped(art, names, tree_zoom):
    """Gives the ink line to tree images already shipped (`art/trees/`),
    once: run again, it would draw a second line outside the first."""
    art = Path(art)
    trees = species_heights(art.parent)
    for name in names:
        path = art / "trees" / name
        kind = re.match(r"(.+)_adult_\d\d_summer\.webp$", name).group(1)
        tall = target_px(trees[kind], tree_zoom)
        image = Image.open(path)
        save_webp(ink(image, INK_PX * image.height / tall), path)
        print("inked", name, image.size)


def main():
    args = sys.argv[1:]
    make_trees = "--trees" in args
    if make_trees:
        args.remove("--trees")
    tree_zoom = ZOOM_MAX
    if "--tree-zoom" in args:
        i = args.index("--tree-zoom")
        tree_zoom = float(args[i + 1])
        del args[i:i + 2]
    if args and args[0] == "--ink":
        ink_shipped(args[1], args[2:], tree_zoom)
        return
    sources, art = Path(args[0]), Path(args[1])
    content = art.parent
    trees = species_heights(content)
    kinds = flora_kinds(content)
    plants = []
    short = []
    for path in sorted(sources.glob("*_[0-9][0-9].png")):
        m = re.match(r"(.+)_(\d\d)\.png$", path.name)
        kind, n = m.group(1), m.group(2)
        section, height = kinds.get(kind, ("trees", trees.get(kind, 0)))
        if kind in trees or section == "trees":
            if not make_trees:
                continue
            metres = trees.get(kind) or height
            tall = target_px(metres, tree_zoom)
            original = target_px(metres, tree_zoom, ORIGINAL_SCREEN_STRETCH)
            image = trim_to_foot(cut(path, keep_white=False, tree=True, work=work_height(tall)))
            image = fit(image, tall, original, path.name, short)
            image = image.filter(ImageFilter.UnsharpMask(radius=1.2, percent=60, threshold=2))
            if not kind.startswith(PALE_TREES):
                image = clear_pale(image)
            # The line is INK_PX at the shipped size: thinner in the pixels
            # of an image short of it, which the client magnifies.
            image = ink(image, INK_PX * image.height / tall, kind.startswith(INK_KEEP_HOLES))
            target = art / "trees" / f"{kind}_adult_{n}_summer.webp"
            save_webp(image, target)
            print("tree", target.name, image.size)
        else:
            tall = target_px(height)
            original = target_px(height, stretch=ORIGINAL_SCREEN_STRETCH)
            image = cut(path, keep_white=(section == "mushrooms"), work=work_height(tall))
            image = fit(image, tall, original, path.name, short)
            image = image.filter(ImageFilter.UnsharpMask(radius=1.0, percent=50, threshold=2))
            plants.append((f"{kind}_{n}", image))
    if plants:
        rects, height = pack(plants)
        atlas = Image.new("RGBA", (ATLAS_WIDTH, height), (0, 0, 0, 0))
        for name, image in plants:
            r = rects[name]
            atlas.alpha_composite(image, (r[0], r[1]))
        atlas = bleed(atlas)
        (art / "flora").mkdir(exist_ok=True)
        save_webp(atlas, art / "flora" / "atlas.webp")
        (art / "flora" / "atlas.json").write_text(json.dumps(dict(sorted(rects.items())), indent=1) + "\n")
        print(len(plants), "plants in an atlas of", atlas.size)
    size_decor(art, short)
    for line in short:
        print("smaller than its target (to make again to be as sharp):", line)


if __name__ == "__main__":
    main()
