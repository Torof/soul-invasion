#!/usr/bin/env python3
"""A sheet of twelve steps of a seed's history, from the page's layers.

    tools/planche.py DIR SEED FIRST OUT.png [--layers rifts,frontieres]
                     [--crop x0,y0,x1,y1] [--steps 40] [--step-my 5]

DIR holds `couches/` (the folder `tools/page.py --out DIR` fills, or the
PLANET_OUT of a run with PLANET_LAYERS). Each tile is the relief of a step
with the named layers over it (default: the rifts), labelled with the step
and its age; the steps FIRST to FIRST + 11, three a row. Needs Pillow.
"""
import argparse

from PIL import Image, ImageDraw, ImageFont


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("dir")
    parser.add_argument("seed")
    parser.add_argument("first", type=int)
    parser.add_argument("out")
    parser.add_argument("--layers", default="rifts")
    parser.add_argument("--crop")
    parser.add_argument("--steps", type=int, default=40)
    parser.add_argument("--step-my", type=int, default=5)
    args = parser.parse_args()
    try:
        font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 22)
    except OSError:
        font = ImageFont.load_default()
    crop = tuple(int(v) for v in args.crop.split(",")) if args.crop else None
    tiles = []
    for t in range(args.first, min(args.first + 12, args.steps + 1)):
        name = f"{args.dir}/couches/h{args.seed}-{t:02d}"
        tile = Image.open(f"{name}-relief.png").convert("RGBA")
        for layer in filter(None, args.layers.split(",")):
            tile = Image.alpha_composite(tile, Image.open(f"{name}-{layer}.png").convert("RGBA"))
        if crop:
            tile = tile.crop(crop)
        draw = ImageDraw.Draw(tile)
        draw.rectangle([0, 0, 250, 30], fill=(255, 255, 255, 220))
        draw.text((6, 3), f"étape {t} ({(args.steps - t) * args.step_my} Ma)", font=font, fill=(0, 0, 0, 255))
        tiles.append(tile.convert("RGB"))
    w, h = tiles[0].size
    sheet = Image.new("RGB", (w * 3 + 20, h * ((len(tiles) + 2) // 3) + 10 * ((len(tiles) - 1) // 3)), (250, 248, 243))
    for k, tile in enumerate(tiles):
        sheet.paste(tile, (k % 3 * (w + 10), k // 3 * (h + 10)))
    sheet.save(args.out)
    print(args.out, sheet.size)


if __name__ == "__main__":
    main()
