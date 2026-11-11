#!/usr/bin/env python3
"""Pixel checks for the board canvas in scripts/ui-smoke.sh (Board slice 3).

Reads a screenshot (PNG at 1 px per point, from Rune --snap --scale 1 or from
ImageMagick `import` on Xvfb), the Rune --tree dump for the same window size
(for the board canvas rect), and the smoke board fixture. Needs Pillow.

  board_pixels.py fit    PNG TREE FIXTURE OUT.json
      Every colour element of the smoke board is where a fit puts it: the
      expected screen box is computed here from the file and the fit rule
      (board_camera.rs: FIT_MARGIN, FIT_MAX_ZOOM), and the measured box of
      that colour's pixels must match it. The text and the arrow have ink
      where they should, and the paper between shapes is the board's colour.
  board_pixels.py moved  PNG TREE PREV.json DX DY OUT.json
      Every colour box moved by DX, DY screen points from PREV.
  board_pixels.py zoomed PNG TREE PREV.json PX PY FACTOR OUT.json
      Every colour box scaled by FACTOR about window point PX, PY
      ("centre" for the canvas centre). FACTOR steps:BASE:LO:HI takes a whole
      number of BASE steps in LO..HI from the pixels (XTEST wheel notches).
  board_pixels.py same   PNG TREE PREV.json OUT.json
      Every colour box is where it was.

OUT.json holds the measured boxes for the next check. Exits non-zero with a
FAIL line on the first mismatch.
"""

import json
import sys

from PIL import Image

# board_camera.rs
FIT_MARGIN = 56.0
FIT_MAX_ZOOM = 2.0
MIN_ZOOM = 0.1
# The floating zoom controls and hint along the canvas's bottom edge.
BOTTOM_CHROME = 50
COLOURS = {
    "rect-red": "#e03131",
    "ellipse-blue": "#1971c2",
    "diamond-violet": "#9c36b5",
    "freedraw-orange": "#f08c00",
    "line-green": "#2f9e44",
}


def die(message):
    print(f"FAIL {message}", file=sys.stderr)
    sys.exit(1)


def rgb(hex_):
    return tuple(int(hex_[i : i + 2], 16) for i in (1, 3, 5))


def canvas_rect(tree_path):
    def walk(node):
        if isinstance(node, dict):
            yield node
            for value in node.values():
                yield from walk(value)
        elif isinstance(node, list):
            for value in node:
                yield from walk(value)

    for n in walk(json.load(open(tree_path)).get("elements", [])):
        if n.get("key") == "board-canvas" and isinstance(n.get("rect"), list):
            return n["rect"]
    die(f"{tree_path}: no board-canvas")


def area(rect):
    """The canvas minus its border and bottom chrome, as pixel bounds."""
    x, y, w, h = rect
    return (int(x) + 3, int(y) + 3, int(x + w) - 3, int(y + h) - BOTTOM_CHROME)


def boxes(png, rect, tolerance=48):
    img = Image.open(png).convert("RGB")
    x0, y0, x1, y1 = area(rect)
    x1, y1 = min(x1, img.width), min(y1, img.height)
    px = img.load()
    want = {key: rgb(c) for key, c in COLOURS.items()}
    found = {key: [10**9, 10**9, -1, -1, 0] for key in want}
    for y in range(y0, y1):
        for x in range(x0, x1):
            p = px[x, y]
            for key, c in want.items():
                if abs(p[0] - c[0]) + abs(p[1] - c[1]) + abs(p[2] - c[2]) <= tolerance:
                    b = found[key]
                    b[0], b[1], b[2], b[3] = min(b[0], x), min(b[1], y), max(b[2], x + 1), max(b[3], y + 1)
                    b[4] += 1
                    break
    out = {}
    for key, b in found.items():
        # A box near the inspected area's edge may be cut off (the canvas clips
        # to its rounded rect): not comparable.
        clipped = b[0] <= x0 + 6 or b[1] <= y0 + 6 or b[2] >= x1 - 6 or b[3] >= y1 - 6
        if b[4] >= 12 and not clipped:
            out[key] = b[:4]
    return out, img, (x0, y0, x1, y1)


def count(img, region, colour, tolerance=90):
    c = rgb(colour)
    x0, y0, x1, y1 = (int(v) for v in region)
    px = img.load()
    n = 0
    for y in range(max(y0, 0), min(y1, img.height)):
        for x in range(max(x0, 0), min(x1, img.width)):
            p = px[x, y]
            if abs(p[0] - c[0]) + abs(p[1] - c[1]) + abs(p[2] - c[2]) <= tolerance:
                n += 1
    return n


def dark(img, region, limit=150):
    """Pixels darker than `limit` on every channel: antialiased ink on white."""
    x0, y0, x1, y1 = (int(v) for v in region)
    px = img.load()
    n = 0
    for y in range(max(y0, 0), min(y1, img.height)):
        for x in range(max(x0, 0), min(x1, img.width)):
            if max(px[x, y]) < limit:
                n += 1
    return n


def element_boxes(fixture):
    """Scene boxes (x0, y0, x1, y1) and stroke widths of the smoke elements."""
    out = {}
    for e in json.load(open(fixture))["elements"]:
        if e.get("isDeleted"):
            continue
        if "points" in e:
            xs = [p[0] for p in e["points"]]
            ys = [p[1] for p in e["points"]]
            box = (e["x"] + min(xs), e["y"] + min(ys), e["x"] + max(xs), e["y"] + max(ys))
        else:
            box = (e["x"], e["y"], e["x"] + e["width"], e["y"] + e["height"])
        width = e.get("strokeWidth", 1) * (1.6 if e["type"] == "freedraw" else 1.0)
        out[e["id"]] = (box, width)
    return out


def fitted(bounds, view):
    span = (max(bounds[2] - bounds[0], 1.0), max(bounds[3] - bounds[1], 1.0))
    room = (max(view[0] - 2 * FIT_MARGIN, 1.0), max(view[1] - 2 * FIT_MARGIN, 1.0))
    zoom = min(max(min(room[0] / span[0], room[1] / span[1]), MIN_ZOOM), FIT_MAX_ZOOM)
    centre = ((bounds[0] + bounds[2]) / 2, (bounds[1] + bounds[3]) / 2)
    eye = (centre[0] - view[0] / 2 / zoom, centre[1] - view[1] / 2 / zoom)
    return eye, zoom


def save(out, measured):
    json.dump(measured, open(out, "w"), indent=1)


def inside(expected, rect, margin=8):
    """The expected boxes that land wholly inside the inspected area."""
    x0, y0, x1, y1 = area(rect)
    return {
        k: b
        for k, b in expected.items()
        if b[0] > x0 + margin and b[1] > y0 + margin and b[2] < x1 - margin and b[3] < y1 - margin
    }


def compare(measured, expected, tol, what):
    if len(expected) < 3:
        die(f"{what}: only {len(expected)} comparable shapes")
    for key, want in expected.items():
        got = measured.get(key)
        if got is None:
            die(f"{what}: {key} not found on screen (want {[round(v) for v in want]})")
        off = max(abs(g - w) for g, w in zip(got, want))
        if off > tol:
            die(f"{what}: {key} at {got}, want {[round(v, 1) for v in want]} (off {off:.1f} > {tol})")
        print(f"ok {what}: {key} {got} (off {off:.1f})")


def cmd_fit(png, tree, fixture, out):
    rect = canvas_rect(tree)
    elements = element_boxes(fixture)
    bounds = [
        min(b[0][0] for b in elements.values()),
        min(b[0][1] for b in elements.values()),
        max(b[0][2] for b in elements.values()),
        max(b[0][3] for b in elements.values()),
    ]
    eye, zoom = fitted(bounds, (rect[2], rect[3]))
    to_screen = lambda sx, sy: (rect[0] + (sx - eye[0]) * zoom, rect[1] + (sy - eye[1]) * zoom)
    print(f"canvas {rect}, fit zoom {zoom:.4f}, eye ({eye[0]:.2f}, {eye[1]:.2f})")
    measured, img, _ = boxes(png, rect)
    expected = {}
    worst = 0.0
    for key in COLOURS:
        (x0, y0, x1, y1), width = elements[key]
        half = width * zoom / 2
        a, b = to_screen(x0, y0), to_screen(x1, y1)
        expected[key] = [a[0] - half, a[1] - half, b[0] + half, b[1] + half]
        worst = max(worst, half)
    compare(measured, expected, 3.0 + worst, "fit")
    # The text and the arrow are ink on paper.
    (tx0, ty0, tx1, ty1), _ = elements["text-ink"]
    a, b = to_screen(tx0, ty0), to_screen(tx1, ty1)
    text_ink = dark(img, (a[0] - 4, a[1] - 4, b[0] + (b[0] - a[0]) * 0.5, b[1] + 6))
    if text_ink < 40:
        die(f"fit: text box has {text_ink} ink pixels")
    print(f"ok fit: text-ink box has {text_ink} ink pixels")
    (ax0, ay0, ax1, ay1), _ = elements["arrow-ink"]
    a, b = to_screen(ax0, ay0 - 12), to_screen(ax1, ay1 + 12)
    arrow_ink = dark(img, (a[0], a[1], b[0] + 2, b[1]))
    # A 2-point line about 100 px long, plus the two barbs.
    if arrow_ink < 80:
        die(f"fit: arrow has {arrow_ink} ink pixels")
    print(f"ok fit: arrow-ink has {arrow_ink} ink pixels")
    # Paper between the rectangle and the ellipse is the board's white.
    p = to_screen(200, 50)
    paper = count(img, (p[0] - 3, p[1] - 3, p[0] + 3, p[1] + 3), "#ffffff", 12)
    if paper < 36:
        die(f"fit: paper at {p} is not white ({paper}/36)")
    print(f"ok fit: paper at ({p[0]:.0f}, {p[1]:.0f}) is white")
    save(out, measured)


def cmd_moved(png, tree, prev, dx, dy, out):
    dx, dy = float(dx), float(dy)
    rect = canvas_rect(tree)
    measured, _, _ = boxes(png, rect)
    before = json.load(open(prev))
    expected = {k: [b[0] + dx, b[1] + dy, b[2] + dx, b[3] + dy] for k, b in before.items()}
    expected = inside(expected, rect)
    compare(measured, expected, 2.0, f"moved {dx:+.0f},{dy:+.0f}")
    save(out, measured)


def cmd_zoomed(png, tree, prev, px, py, factor, out):
    rect = canvas_rect(tree)
    if px == "centre":
        px, py = rect[0] + rect[2] / 2, rect[1] + rect[3] / 2
    px, py = float(px), float(py)
    measured, _, _ = boxes(png, rect)
    before = json.load(open(prev))
    if factor.startswith("steps:"):
        # steps:BASE:LO:HI, a whole number of wheel steps in LO..HI. Xvfb's XTEST
        # wheel button also drives the pointer's scroll valuator, so winit may see
        # one or two wheel events per notch; take the count from the pixels.
        _, base, lo, hi = factor.split(":")
        base, lo, hi = float(base), int(lo), int(hi)
        common = [k for k in before if k in measured]
        if not common:
            die("zoomed: no shape on both screens")
        ratios = [(measured[k][2] - measured[k][0]) / (before[k][2] - before[k][0]) for k in common]
        ratio = sorted(ratios)[len(ratios) // 2]
        import math

        steps = round(math.log(ratio) / math.log(base))
        if not lo <= steps <= hi:
            die(f"zoomed: width ratio {ratio:.3f} is {steps} steps of {base}, want {lo}..{hi}")
        f = base**steps
        print(f"width ratio {ratio:.3f}: {steps} wheel steps of {base}")
    else:
        f = float(factor)
    expected = {
        k: [px + (b[0] - px) * f, py + (b[1] - py) * f, px + (b[2] - px) * f, py + (b[3] - py) * f]
        for k, b in before.items()
    }
    expected = inside(expected, rect)
    compare(measured, expected, 3.0, f"zoomed x{f:.4f} at ({px:.0f}, {py:.0f})")
    save(out, measured)


def cmd_same(png, tree, prev, out):
    measured, _, _ = boxes(png, canvas_rect(tree))
    before = json.load(open(prev))
    compare(measured, before, 1.5, "same")
    save(out, measured)


def main(argv):
    commands = {"fit": cmd_fit, "moved": cmd_moved, "zoomed": cmd_zoomed, "same": cmd_same}
    if len(argv) < 2 or argv[1] not in commands:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    commands[argv[1]](*argv[2:])


if __name__ == "__main__":
    main(sys.argv)
