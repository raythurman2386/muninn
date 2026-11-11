"""x11_chrome: the canvas chrome stays put when the selection changes.

Raymond's report: selecting something and clicking off made the key hint
bar (and the zoom controls) slide back across the bottom of the board.

    board_chrome.py x11-burst WID X Y DIR NAME N   click at X,Y; grab the window
                                                   before, then N frames as fast as
                                                   possible, then once settled
    board_chrome.py still TREE BASE FRAME...       every FRAME shows the hint bar and
                                                   the zoom controls exactly as BASE
    board_chrome.py shifted BASE OUT DX            BASE with the chrome moved DX px
                                                   (the negative control)
"""
import json
import os
import sys
import time

from PIL import Image, ImageChops

import harness

SLOW = max(float(os.environ.get("UI_SMOKE_SLOW") or 1), 1)
HINT = "Click/drag select"


def die(message):
    harness.die(message)


def regions(tree_path):
    """The hint bar and the zoom controls, from the headless tree, each grown
    by 24 px sideways so a slide into or out of place shows."""
    tree = json.load(open(tree_path))
    hint = zoom = None
    for n in harness.walk(tree.get("elements", [])):
        rect = n.get("rect")
        if not isinstance(rect, list):
            continue
        if hint is None and isinstance(n.get("text"), str) and n["text"].startswith(HINT):
            hint = rect
        if zoom is None and n.get("key") == "board-zoom":
            zoom = rect
    if hint is None or zoom is None:
        die(f"{tree_path}: no hint bar ({hint}) or zoom label ({zoom})")
    out = []
    for name, (x, y, w, h) in (("hint bar", hint), ("zoom controls", zoom)):
        pad = 14 if name == "hint bar" else 60
        out.append((name, (round(x - 24), round(y - 8), round(x + w + 24 + pad), round(y + h + 8))))
    return out


def grab(d, w):
    g = w.get_geometry()
    raw = w.get_image(0, 0, g.width, g.height, 2, 0xFFFFFFFF)  # ZPixmap
    return Image.frombytes("RGB", (g.width, g.height), raw.data, "raw", "BGRX")


def x11_burst(wid, x, y, out, name, n):
    from Xlib import X
    from Xlib.ext import xtest

    d = harness.display()
    w = harness.focus(d, wid)
    origin = w.translate_coords(d.screen().root, 0, 0)
    rx, ry = -origin.x + int(x), -origin.y + int(y)
    xtest.fake_input(d, X.MotionNotify, x=rx, y=ry)
    d.sync()
    time.sleep(0.4 * SLOW)
    grab(d, w).save(f"{out}/{name}-base.png")
    xtest.fake_input(d, X.ButtonPress, 1)
    d.sync()
    time.sleep(0.03)
    xtest.fake_input(d, X.ButtonRelease, 1)
    d.sync()
    t0 = time.monotonic()
    times = []
    for i in range(int(n)):
        grab(d, w).save(f"{out}/{name}-{i:02d}.png")
        times.append(time.monotonic() - t0)
        time.sleep(0.02)
    time.sleep(1.0 * SLOW)
    grab(d, w).save(f"{out}/{name}-settled.png")
    span = ", ".join(f"{t:.2f}" for t in times)
    print(f"ok clicked {x},{y}; {n} frames at {span} s after the release, then settled")


def still(tree, base, *frames):
    b = Image.open(base).convert("RGB")
    worst = 0
    for f in frames:
        im = Image.open(f).convert("RGB")
        for name, box in regions(tree):
            diff = ImageChops.difference(b.crop(box), im.crop(box)).convert("L")
            moved = diff.point(lambda v: 255 if v > 40 else 0).histogram()[255]
            worst = max(worst, moved)
            if moved > 6:
                die(f"{os.path.basename(f)}: the {name} moved or changed ({moved} px differ from {os.path.basename(base)})")
    print(f"ok {len(frames)} frames: hint bar and zoom controls where they were (at most {worst} px differ)")


def shifted(base, out, dx):
    """BASE with its bottom 70 px (where the chrome sits) moved DX px right."""
    b = Image.open(base).convert("RGB")
    w, h = b.size
    strip = b.crop((0, h - 70, w, h))
    m = b.copy()
    m.paste(strip, (int(dx), h - 70))
    m.save(out)
    print(f"ok wrote {out}")


def main(argv):
    commands = {"x11-burst": x11_burst, "still": still, "shifted": shifted}
    if len(argv) < 2 or argv[1] not in commands:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    commands[argv[1]](*argv[2:])


if __name__ == "__main__":
    main(sys.argv)
