#!/usr/bin/env python3
"""Contact sheet: python3 scripts/sheet.py OUT.png IMG... (2 columns, 700 px wide tiles)."""
import sys
from PIL import Image, ImageDraw
out, imgs = sys.argv[1], sys.argv[2:]
W = 700
tiles = []
for p in imgs:
    im = Image.open(p).convert("RGB")
    h = int(im.height * W / im.width)
    im = im.resize((W, h), Image.LANCZOS)
    ImageDraw.Draw(im).text((8, 8), p.split("/")[-1], fill=(255, 255, 0))
    tiles.append(im)
cols = 2
rows = (len(tiles) + cols - 1) // cols
th = max(t.height for t in tiles)
sheet = Image.new("RGB", (cols * W, rows * th), (0, 0, 0))
for k, t in enumerate(tiles):
    sheet.paste(t, ((k % cols) * W, (k // cols) * th))
sheet.save(out)
print(out, sheet.size)
