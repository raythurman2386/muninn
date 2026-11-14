#!/usr/bin/env python3
"""Contact sheet of the encoded assets, for review: python3 contact_sheet.py ASSETS OUT.png"""
import json
import os
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw, ImageFont

root, out = sys.argv[1], sys.argv[2]
m = json.load(open(os.path.join(root, "manifest.json")))
TW, PAD, LABEL = 480, 16, 40
try:
    font = ImageFont.truetype("DejaVuSans.ttf", 15)
except OSError:
    font = ImageFont.load_default()

tiles = []
tmp = tempfile.mkdtemp()
for f in m["files"]:
    p = os.path.join(root, f["path"])
    if f["kind"] in ("still", "dropin-still") and p.endswith("-1600.png"):
        tiles.append((p, f"{f['theme']} · {f['scene']} · still {f['width']}x{f['height']}"))
    elif f["kind"] in ("loop", "hero") and (p.endswith("-1600.mp4") or (f["kind"] == "hero" and p.endswith(".mp4"))):
        # Frames at 25 %, 60 % and 95 % of the clip.
        for frac in (0.25, 0.6, 0.95):
            g = os.path.join(tmp, f"{len(tiles)}.png")
            subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", str(f["duration_s"] * frac), "-i", p,
                            "-frames:v", "1", g], check=True)
            kb = f["bytes"] // 1024
            tiles.append((g, f"{f['theme']} · {f['scene']} @{f['duration_s'] * frac:.1f}s · {kb} KB"))

cols = 4
rows = (len(tiles) + cols - 1) // cols
th = round(TW * 1058 / 1600)
sheet = Image.new("RGB", (cols * (TW + PAD) + PAD, rows * (th + LABEL + PAD) + PAD + 40), "#2b2b2b")
d = ImageDraw.Draw(sheet)
d.text((PAD, 12), f"Muninn {m['version']} marketing captures · {len(m['files'])} files · budget misses: "
       f"{len(m['budget_misses'])}", fill="white", font=font)
for i, (p, label) in enumerate(tiles):
    im = Image.open(p).convert("RGB")
    im.thumbnail((TW, th), Image.LANCZOS)
    x = PAD + (i % cols) * (TW + PAD)
    y = 40 + PAD + (i // cols) * (th + LABEL + PAD)
    sheet.paste(im, (x, y))
    d.text((x, y + im.height + 6), label, fill="#dddddd", font=font)
sheet.save(out, optimize=True)
print(out, sheet.size)
