#!/usr/bin/env python3
"""Proves that the shipped WebP images hold the very pixels of the PNGs
they replaced (E3: the art is kept in git as lossless WebP).

Development tool; the game never runs it. Needs Pillow. From the
repository's root:

    python3 client/tools/webp_same_pixels.py [git ref]

For every `.webp` under `content/art/`, the PNG of the same name is read
from the git ref (default `origin/main`; a PNG still beside the WebP in
the working tree is read first), both are decoded to RGBA and their bytes
compared, transparent pixels included. Prints one line per mismatch and
the count; exits 1 when there is any.
"""

import io
import subprocess
import sys
from pathlib import Path

from PIL import Image


def old_png(path, ref):
    """The PNG that `path` (a `.webp`) replaced, decoded, or None."""
    png = path.with_suffix(".png")
    if png.exists():
        return Image.open(png)
    shown = subprocess.run(["git", "show", f"{ref}:{png.as_posix()}"], capture_output=True, check=False)
    if shown.returncode != 0:
        return None
    return Image.open(io.BytesIO(shown.stdout))


def main():
    ref = sys.argv[1] if len(sys.argv) > 1 else "origin/main"
    files = sorted(Path("content/art").rglob("*.webp"))
    mismatches = 0
    for path in files:
        old = old_png(path, ref)
        new = Image.open(path)
        if old is None:
            print(f"{path}: no PNG of that name in {ref}")
            mismatches += 1
        elif old.size != new.size:
            print(f"{path}: size {new.size}, the PNG {old.size}")
            mismatches += 1
        elif old.convert("RGBA").tobytes() != new.convert("RGBA").tobytes():
            print(f"{path}: pixels differ")
            mismatches += 1
    print(f"{len(files)} WebP images compared with their PNG ({ref}): {mismatches} mismatches")
    sys.exit(1 if mismatches else 0)


if __name__ == "__main__":
    main()
