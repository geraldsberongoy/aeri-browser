#!/usr/bin/env python3
"""Regenerates the Aeri app icon glyph.

Source: the owner's render "Aeri Breeze Ribbon Icons.png" (left = light
variant). Only the glyph's shape is used: it is thresholded, smoothed by
supersampling and written as a black-on-transparent 1024x1024 layer to
`../AppIcon.icon/Assets/glyph.png`, positioned within the squircle as in the
render. `../AppIcon.icon/icon.json` (hand-written) supplies the white/black
background and the black/white glyph for Light/Dark; `cargo xtask bundle`
compiles it with actool. Usage: make_icon.py [path-to-render.png]
"""
import sys, pathlib
from PIL import Image, ImageFilter

src = sys.argv[1] if len(sys.argv) > 1 else "/Users/kuyaegg/Downloads/Aeri Breeze Ribbon Icons.png"
out = pathlib.Path(__file__).resolve().parent.parent / "AppIcon.icon/Assets/glyph.png"
im = Image.open(src).convert("L")
# Squircle of the light variant (render px); glyph is searched inside it,
# inset so the soft edge/shadow of the tile is never picked up.
X0, Y0, X1, Y1 = 80, 370, 596, 896
tile = im.crop((X0, Y0, X1, Y1))
ins = 40
inner = tile.crop((ins, ins, tile.width - ins, tile.height - ins))
l, t, r, bt = inner.point(lambda v: 255 if v < 128 else 0).getbbox()
bx0, bx1, by0, by1 = l + ins, r + ins, t + ins, bt + ins
pad = 6
glyph = tile.crop((bx0 - pad, by0 - pad, bx1 + pad, by1 + pad))
S = 1024 / (X1 - X0)  # tile -> 1024 artboard
SS = 4
w, h = glyph.size
big = glyph.resize((round(w * S * SS), round(h * S * SS)), Image.LANCZOS)
big = big.filter(ImageFilter.GaussianBlur(2.5 * SS))
m = big.point(lambda v: 255 if v < 128 else 0)
m = m.resize((round(w * S), round(h * S)), Image.LANCZOS)
canvas = Image.new("L", (1024, 1024), 0)
canvas.paste(m, (round((bx0 - pad) * S), round((by0 - pad) * S)))
rgba = Image.new("RGBA", (1024, 1024), (0, 0, 0, 255))
rgba.putalpha(canvas)
out.parent.mkdir(parents=True, exist_ok=True)
rgba.save(out)
print(out, "glyph bbox", (bx0, by0, bx1, by1))
