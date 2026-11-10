#!/usr/bin/env python3
"""Gives a tree's new source the colours of its old version.

Development tool, run by hand once per redrawn tree; the game never runs
it. Needs Pillow and numpy. From the repository's root:

    python3 content/tools/match_tree_colours.py <source> <old art> <new art> <out>

- <source>: the new original on white (content/ai-reserve/sources/flora/);
- <old art>: the tree as the game shipped it before, whose colours are kept;
- <new art>: the new tree as flora_art.py ships it from <source>;
- <out>: where the corrected source is written (the same path as <source>
  to correct in place; keep the uncorrected one beside it first).

In CIE Lab, over the tree's own colours (opaque pixels; outlines and white
left aside): the lightness is mapped quantile by quantile onto the old
version's (so the share of dark shading comes back to what it was), the
hue is turned by the difference of the two mean hues, and the chroma is
scaled by the ratio of the two mean chromas (a colour never turns grey or
crosses to another hue on its own). The correction is per pixel: nothing is blurred. It fades out on
the darkest pixels (the outlines stay exactly as dark) and on the white
background (so the cut still finds it). Then flora_art.py --trees ships
the corrected sources.
"""

import sys

import numpy as np
from PIL import Image

# The correction fades in between these lightnesses (L, 0..100): the
# outlines, darker, keep their own colour.
OUTLINE_L = (10.0, 18.0)
QUANTILES = np.linspace(0.0, 1.0, 257)
# And fades out on what is near white (distance in Lab): the background.
BACKGROUND_DE = (6.0, 14.0)


def srgb_to_lab(rgb):
    c = rgb / 255.0
    c = np.where(c > 0.04045, ((c + 0.055) / 1.055) ** 2.4, c / 12.92)
    m = np.array([[0.4124564, 0.3575761, 0.1804375],
                  [0.2126729, 0.7151522, 0.0721750],
                  [0.0193339, 0.1191920, 0.9503041]])
    xyz = c @ m.T / np.array([0.95047, 1.0, 1.08883])
    f = np.where(xyz > 216 / 24389, np.cbrt(xyz), (24389 / 27 * xyz + 16) / 116)
    return np.stack([116 * f[..., 1] - 16, 500 * (f[..., 0] - f[..., 1]), 200 * (f[..., 1] - f[..., 2])], -1)


def lab_to_srgb(lab):
    fy = (lab[..., 0] + 16) / 116
    fx = fy + lab[..., 1] / 500
    fz = fy - lab[..., 2] / 200
    f = np.stack([fx, fy, fz], -1)
    xyz = np.where(f ** 3 > 216 / 24389, f ** 3, (116 * f - 16) / (24389 / 27))
    xyz = xyz * np.array([0.95047, 1.0, 1.08883])
    m = np.array([[3.2404542, -1.5371385, -0.4985314],
                  [-0.9692660, 1.8760108, 0.0415560],
                  [0.0556434, -0.2040259, 1.0572252]])
    c = xyz @ m.T
    c = np.where(c > 0.0031308, 1.055 * np.clip(c, 0, None) ** (1 / 2.4) - 0.055, 12.92 * c)
    return np.clip(c * 255.0, 0, 255)


def ramp(x, lo, hi):
    return np.clip((x - lo) / (hi - lo), 0.0, 1.0)


def colours(path):
    """A shipped tree's own colours in Lab: its opaque pixels lighter than
    the outlines."""
    a = np.asarray(Image.open(path).convert("RGBA")).astype(np.float64)
    lab = srgb_to_lab(a[..., :3])
    return lab[(a[..., 3] > 230) & (lab[..., 0] > OUTLINE_L[1])]


def mean_hue(lab):
    """The mean hue of colours in Lab, each weighted by its chroma."""
    return np.arctan2(lab[:, 2].sum(), lab[:, 1].sum())


def main():
    source, old, new, out = sys.argv[1:5]
    was, now = colours(old), colours(new)
    lab = srgb_to_lab(np.asarray(Image.open(source).convert("RGB")).astype(np.float64))
    moved = lab.copy()
    moved[..., 0] = np.interp(lab[..., 0], np.quantile(now[:, 0], QUANTILES), np.quantile(was[:, 0], QUANTILES))
    turn = mean_hue(was) - mean_hue(now)
    scale = np.hypot(was[:, 1], was[:, 2]).mean() / np.hypot(now[:, 1], now[:, 2]).mean()
    chroma = np.hypot(lab[..., 1], lab[..., 2]) * scale
    hue = np.arctan2(lab[..., 2], lab[..., 1]) + turn
    moved[..., 1], moved[..., 2] = chroma * np.cos(hue), chroma * np.sin(hue)
    white = np.sqrt((lab[..., 0] - 100) ** 2 + lab[..., 1] ** 2 + lab[..., 2] ** 2)
    weight = ramp(lab[..., 0], *OUTLINE_L) * ramp(white, *BACKGROUND_DE)
    result = lab + (moved - lab) * weight[..., None]
    Image.fromarray(lab_to_srgb(result).round().astype(np.uint8)).save(out)
    print(out.rsplit("/", 1)[-1], "median L", round(float(np.median(now[:, 0])), 1), "->", round(float(np.median(was[:, 0])), 1))


if __name__ == "__main__":
    main()
