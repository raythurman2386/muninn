#!/usr/bin/env python3
"""Board slice 4 drawing checks for scripts/ui-smoke.sh.

The strokes (STROKES below) are planned in board-canvas points; the canvas
rect comes from a Rune --tree dump, so the same plan drives the headless
script and the XTEST input. Needs Pillow for `pixels` and `same`.

  board_draw.py script TREE OUT.json        headless script: open board-row-2,
                                            draw every tool, Cmd+S (marks before,
                                            drawn, saved, end)
  board_draw.py x11 WINDOW TREE             the same strokes with XTEST
  board_draw.py pixels BEFORE.png AFTER.png TREE
      each tool's ink is where its stroke went and nowhere it should not be
      (ellipse vs diamond vs rectangle, arrowhead vs none), and nothing else
      on the canvas changed
  board_draw.py same A.png B.png TREE        the canvas looks the same in both
  board_draw.py file SAVED ORIGINAL RESAVED TREE
      the saved board parses; holds the six new elements in Excalidraw's shape
      with the geometry the strokes give through the fit camera; keeps every
      old element, value for value against ORIGINAL and byte for byte against
      RESAVED (ORIGINAL through Muninn's own load/save, s1's contract)
"""

import json
import math
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import board_pixels  # noqa: E402
import harness  # noqa: E402

# (tool key, element type, canvas-point path). Box tools and lines use the
# first and last points; the freehand stroke keeps the whole path.
ZIGZAG = [[410, 300], [440, 400], [470, 300], [500, 400], [530, 300]]
STROKES = [
    ("r", "rectangle", [[110, 140], [140, 160], [230, 240]]),
    ("o", "ellipse", [[380, 240], [350, 220], [260, 140]]),
    ("d", "diamond", [[410, 140], [440, 160], [530, 240]]),
    ("l", "line", [[110, 300], [150, 330], [230, 400]]),
    ("a", "arrow", [[260, 350], [300, 352], [380, 350]]),
    ("p", "freedraw", ZIGZAG),
]
# Toolbar strip along the canvas top, excluded from the "nothing else
# changed" check (the picked tool's button lights up).
TOOLBAR = 56
ROW = "board-row-2"
DEFAULTS = {
    "angle", "strokeColor", "backgroundColor", "strokeWidth", "opacity", "isDeleted",
    "version", "versionNonce", "seed", "width", "height", "points", "text", "fontSize",
}


def die(message):
    print(f"FAIL {message}", file=sys.stderr)
    sys.exit(1)


def canvas(tree):
    return board_pixels.canvas_rect(tree)


def steps(path, n=8):
    out = [path[0]]
    for a, b in zip(path, path[1:]):
        for i in range(1, n + 1):
            out.append([a[0] + (b[0] - a[0]) * i / n, a[1] + (b[1] - a[1]) * i / n])
    return out


def cmd_script(tree, out):
    x0, y0, _, _ = canvas(tree)
    ev = [
        {"t": 0.3, "type": "key", "name": "4", "cmd": True},
        {"t": 0.8, "type": "tap", "key": ROW},
        {"t": 2.0, "type": "mark", "name": "before"},
    ]
    t = 2.4
    for key, _, path in STROKES:
        ev.append({"t": round(t, 3), "type": "type", "text": key})
        t += 0.15
        pts = steps(path)
        ev.append({"t": round(t, 3), "type": "down", "x": x0 + pts[0][0], "y": y0 + pts[0][1]})
        for p in pts[1:]:
            t += 1 / 60
            ev.append({"t": round(t, 3), "type": "move", "x": x0 + p[0], "y": y0 + p[1]})
        t += 0.05
        ev.append({"t": round(t, 3), "type": "up", "x": x0 + pts[-1][0], "y": y0 + pts[-1][1]})
        t += 0.3
    ev.append({"t": round(t + 0.3, 3), "type": "mark", "name": "drawn"})
    ev.append({"t": round(t + 0.8, 3), "type": "key", "name": "s", "cmd": True})
    ev.append({"t": round(t + 1.3, 3), "type": "mark", "name": "saved"})
    ev.append({"t": round(t + 1.8, 3), "type": "mark", "name": "end"})
    json.dump({"events": ev}, open(out, "w"), indent=1)
    print(f"ok wrote {out}: {len(ev)} events")


def cmd_x11(wid, tree):
    from Xlib import X
    from Xlib.ext import xtest

    x0, y0, _, _ = canvas(tree)
    for key, kind, path in STROKES:
        harness.x11_chord(wid, key)
        d = harness.display()
        w = harness.focus(d, wid)
        origin = w.translate_coords(d.screen().root, 0, 0)
        root = lambda p: (-origin.x + round(x0 + p[0]), -origin.y + round(y0 + p[1]))
        pts = steps(path)
        rx, ry = root(pts[0])
        xtest.fake_input(d, X.MotionNotify, x=rx, y=ry)
        d.sync()
        time.sleep(0.15)
        xtest.fake_input(d, X.ButtonPress, 1)
        d.sync()
        for p in pts[1:]:
            time.sleep(0.02)
            rx, ry = root(p)
            xtest.fake_input(d, X.MotionNotify, x=rx, y=ry)
            d.sync()
        time.sleep(0.1)
        xtest.fake_input(d, X.ButtonRelease, 1)
        d.sync()
        time.sleep(0.3)
        print(f"ok drew {kind} with {key!r}")


def window_px(img, rect, p):
    return (rect[0] + p[0], rect[1] + p[1])


def ink(img, at, r=2):
    return board_pixels.dark(img, (at[0] - r, at[1] - r, at[0] + r + 1, at[1] + r + 1))


def checks():
    """(label, canvas point, want ink?) for every tool."""
    out = []
    # Rectangle 110,140 → 230,240, rounded corners.
    for p in [(170, 140), (230, 190), (170, 240), (110, 190)]:
        out.append(("rectangle edge", p, True))
    out += [("rectangle inside", (170, 190), False), ("rectangle rounded corner", (110, 140), False)]
    # Ellipse in 260,140 → 380,240: centre 320,190, radii 60, 50.
    cx, cy, a, b = 320, 190, 60, 50
    for p in [(cx, cy - b), (cx + a, cy), (cx, cy + b), (cx - a, cy)]:
        out.append(("ellipse extreme", p, True))
    d45 = math.sqrt(0.5)
    out += [
        ("ellipse 45°", (cx + a * d45, cy - b * d45), True),
        ("ellipse box corner", (260 + 3, 140 + 3), False),
        ("ellipse inside", (cx + a / 2, cy - b / 2), False),
    ]
    # Diamond in 410,140 → 530,240: centre 470,190.
    cx = 470
    for p in [(cx, cy - b), (cx + a, cy), (cx, cy + b), (cx - a, cy), (cx + a / 2, cy - b / 2)]:
        out.append(("diamond edge", p, True))
    out += [("diamond outside at ellipse 45°", (cx + a * d45, cy - b * d45), False), ("diamond inside", (cx, cy), False)]
    # Line 110,300 → 230,400.
    (lx0, ly0), (lx1, ly1) = (110, 300), (230, 400)
    n = math.hypot(lx1 - lx0, ly1 - ly0)
    nx, ny = -(ly1 - ly0) / n, (lx1 - lx0) / n
    for f in (0.25, 0.5, 0.75):
        out.append(("line", (lx0 + (lx1 - lx0) * f, ly0 + (ly1 - ly0) * f), True))
        out.append(("beside line", (lx0 + (lx1 - lx0) * f + 12 * nx, ly0 + (ly1 - ly0) * f + 12 * ny), False))
    # board_scene.rs arrowhead: barbs 20 pt long at ±0.45 rad; sample them halfway.
    def barbs(tip, back):
        return [(tip[0] + 10 * math.cos(back + s * 0.45), tip[1] + 10 * math.sin(back + s * 0.45)) for s in (1, -1)]
    for p in barbs((lx1, ly1), math.atan2(ly0 - ly1, lx0 - lx1)):
        out.append(("no arrowhead on the line", p, False))
    for x in (290, 320, 350):
        out.append(("arrow shaft", (x, 350), True))
    for p in barbs((380, 350), math.pi):
        out.append(("arrowhead barb", p, True))
    out.append(("above the arrow", (320, 336), False))
    # Freehand zigzag.
    for (x0, y0), (x1, y1) in zip(ZIGZAG, ZIGZAG[1:]):
        out.append(("freehand", ((x0 + x1) / 2, (y0 + y1) / 2), True))
    out += [("between freehand peaks", (440, 312), False), ("under freehand valley", (470, 390), False)]
    return out


def regions():
    """Canvas boxes the strokes may touch, padded."""
    out = []
    for _, kind, path in STROKES:
        xs, ys = [p[0] for p in path], [p[1] for p in path]
        pad = 26 if kind == "arrow" else 10
        out.append((min(xs) - pad, min(ys) - pad, max(xs) + pad, max(ys) + pad))
    return out


def cmd_pixels(before, after, tree):
    from PIL import Image

    rect = canvas(tree)
    img = Image.open(after).convert("RGB")
    ref = Image.open(before).convert("RGB")
    for label, p, want in checks():
        at = window_px(img, rect, p)
        n = ink(img, at)
        was = ink(ref, at)
        if was:
            die(f"{label} at canvas {p}: ink there before drawing ({was})")
        if want and n < 2:
            die(f"{label} at canvas {p}: no ink ({n})")
        if not want and n:
            die(f"{label} at canvas {p}: ink where there should be none ({n})")
        print(f"ok {label} at ({p[0]:.0f}, {p[1]:.0f}): {'ink' if want else 'paper'} ({n})")
    # Nothing else changed: every pixel outside the strokes' boxes, the
    # toolbar strip and the bottom chrome is as it was.
    x0, y0, x1, y1 = board_pixels.area(rect)
    boxes = [(rect[0] + a, rect[1] + b, rect[0] + c, rect[1] + d) for a, b, c, d in regions()]
    pa, pb = img.load(), ref.load()
    changed = 0
    for y in range(max(y0, int(rect[1]) + TOOLBAR), y1):
        for x in range(x0, x1):
            if any(a <= x <= c and b <= y <= d for a, b, c, d in boxes):
                continue
            if max(abs(u - v) for u, v in zip(pa[x, y], pb[x, y])) > 24:
                changed += 1
    if changed:
        die(f"{changed} pixels changed outside the strokes")
    print("ok nothing outside the strokes changed (old elements included)")


def cmd_same(a, b, tree):
    from PIL import Image

    x0, y0, x1, y1 = board_pixels.area(canvas(tree))
    pa = Image.open(a).convert("RGB").load()
    pb = Image.open(b).convert("RGB").load()
    changed = sum(
        1
        for y in range(y0 + TOOLBAR, y1)
        for x in range(x0, x1)
        if max(abs(u - v) for u, v in zip(pa[x, y], pb[x, y])) > 24
    )
    if changed > 20:
        die(f"{changed} canvas pixels differ between {a} and {b}")
    print(f"ok the canvas matches ({changed} pixels differ)")


def spans(text):
    """Top-level key -> (start, end) of its value, plus the element spans."""
    dec = json.JSONDecoder()
    ws = " \t\r\n"
    out, items = {}, []
    i = text.index("{") + 1
    while True:
        while text[i] in ws + ",":
            i += 1
        if text[i] == "}":
            break
        key, i = dec.raw_decode(text, i)
        while text[i] in ws + ":":
            i += 1
        _, end = dec.raw_decode(text, i)
        out[key] = (i, end)
        if key == "elements":
            j = i + 1
            while True:
                while text[j] in ws + ",":
                    j += 1
                if text[j] == "]":
                    break
                _, e = dec.raw_decode(text, j)
                items.append((j, e))
                j = e
        i = end
    return out, items


def cmd_file(saved_path, original_path, resaved_path, tree):
    saved_text = open(saved_path, encoding="utf-8").read()
    try:
        saved = json.loads(saved_text)
    except ValueError as e:
        die(f"{saved_path} does not parse: {e}")
    original = json.load(open(original_path, encoding="utf-8"))
    if saved.get("type") != "excalidraw":
        die("not an excalidraw scene")
    old, new = saved["elements"][: len(original["elements"])], saved["elements"][len(original["elements"]):]
    if len(new) != len(STROKES):
        die(f"{len(new)} new elements, want {len(STROKES)}")
    print(f"ok parses: {len(old)} old + {len(new)} new elements")

    # Old elements, value for value against the original file.
    for o, s in zip(original["elements"], old):
        for k, v in o.items():
            if s.get(k) != v:
                die(f"old element {o.get('id')}: {k} {s.get(k)!r} != {v!r}")
        added = set(s) - set(o)
        if added - DEFAULTS:
            die(f"old element {o.get('id')}: unexpected new keys {sorted(added - DEFAULTS)}")
    for k, v in original.items():
        if k != "elements" and saved.get(k) != v:
            die(f"top-level {k} changed")
    print("ok every old element and top-level key keeps its values")

    # ... and byte for byte against Muninn's own load/save of the original.
    resaved_text = open(resaved_path, encoding="utf-8").read()
    sa, sitems = spans(saved_text)
    ra, ritems = spans(resaved_text)
    for k, (a, b) in ra.items():
        if k == "elements":
            continue
        if sa.get(k) is None or saved_text[slice(*sa[k])] != resaved_text[a:b]:
            die(f"top-level {k} bytes differ from s1's save")
    for i, ((a, b), (c, d)) in enumerate(zip(ritems, sitems)):
        if resaved_text[a:b] != saved_text[c:d]:
            die(f"old element {i} bytes differ from s1's save")
    if len(ritems) != len(old):
        die("element count differs from s1's save")
    print(f"ok {len(ritems)} old elements and {len(ra) - 1} top-level values are byte-identical to s1's save")

    # New elements: Excalidraw's fields, unique ids, increasing indexes.
    ids = {e.get("id") for e in original["elements"]}
    last_index = [e["index"] for e in original["elements"] if "index" in e][-1]
    now = time.time() * 1000
    common = {
        "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent", "fillStyle": "solid",
        "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1, "opacity": 100, "groupIds": [],
        "frameId": None, "isDeleted": False, "boundElements": None, "link": None, "locked": False,
        "version": 1,
    }
    for (key, kind, path), e in zip(STROKES, new):
        if e.get("type") != kind:
            die(f"new element is {e.get('type')}, want {kind}")
        for k, v in common.items():
            if e.get(k) != v:
                die(f"{kind}: {k} {e.get(k)!r}, want {v!r}")
        if not (isinstance(e.get("id"), str) and len(e["id"]) == 21) or e["id"] in ids:
            die(f"{kind}: bad or duplicate id {e.get('id')!r}")
        ids.add(e["id"])
        for k in ("seed", "versionNonce"):
            if not (isinstance(e.get(k), int) and 0 <= e[k] < 2**31):
                die(f"{kind}: {k} {e.get(k)!r}")
        for k in ("created", "updated"):
            if not (isinstance(e.get(k), int) and abs(e[k] - now) < 86_400_000):
                die(f"{kind}: {k} {e.get(k)!r} is not a recent ms timestamp")
        idx = e.get("index")
        if not isinstance(idx, str) or not idx > last_index:
            die(f"{kind}: index {idx!r} not after {last_index!r}")
        last_index = idx
        want_round = {"rectangle": {"type": 3}, "freedraw": None}.get(kind, {"type": 2})
        if e.get("roundness") != want_round:
            die(f"{kind}: roundness {e.get('roundness')!r}")
        if kind in ("line", "arrow"):
            heads = (e.get("startArrowhead"), e.get("endArrowhead"))
            if heads != (None, "arrow" if kind == "arrow" else None) or e.get("startBinding", 0) is not None:
                die(f"{kind}: arrowheads/bindings {heads}")
        if kind == "freedraw" and (e.get("pressures") != [] or e.get("simulatePressure") is not True):
            die("freedraw: pressures/simulatePressure")
    print("ok the new elements carry Excalidraw's fields, unique ids and increasing indexes")

    # Geometry through the fit camera, computed from the original file.
    rect = canvas(tree)
    boxes = board_pixels.element_boxes(original_path)
    bounds = [
        min(b[0][0] for b in boxes.values()), min(b[0][1] for b in boxes.values()),
        max(b[0][2] for b in boxes.values()), max(b[0][3] for b in boxes.values()),
    ]
    eye, zoom = board_pixels.fitted(bounds, (rect[2], rect[3]))
    to_scene = lambda p: (eye[0] + p[0] / zoom, eye[1] + p[1] / zoom)
    tol = 1.0 / zoom + 0.5
    for (key, kind, path), e in zip(STROKES, new):
        a, b = to_scene(path[0]), to_scene(path[-1])
        if kind in ("rectangle", "ellipse", "diamond"):
            want = (min(a[0], b[0]), min(a[1], b[1]), abs(b[0] - a[0]), abs(b[1] - a[1]))
            got = (e["x"], e["y"], e["width"], e["height"])
        elif kind in ("line", "arrow"):
            want = (a[0], a[1], b[0] - a[0], b[1] - a[1])
            pts = e["points"]
            if len(pts) != 2 or pts[0] != [0, 0]:
                die(f"{kind}: points {pts}")
            got = (e["x"], e["y"], pts[1][0], pts[1][1])
        else:
            xs = [to_scene(p)[0] for p in path]
            ys = [to_scene(p)[1] for p in path]
            want = (a[0], a[1], max(xs) - min(xs), max(ys) - min(ys))
            got = (e["x"], e["y"], e["width"], e["height"])
            pts = e["points"]
            if pts[0] != [0, 0]:
                die(f"freedraw first point {pts[0]}")
            for p in path:
                s = to_scene(p)
                if not any(math.hypot(e["x"] + q[0] - s[0], e["y"] + q[1] - s[1]) <= tol + 1 for q in pts):
                    die(f"freedraw does not pass through {p}")
        off = max(abs(g - w) for g, w in zip(got, want))
        if off > tol:
            die(f"{kind}: geometry {got}, want {tuple(round(v, 2) for v in want)} (off {off:.2f})")
        print(f"ok {kind}: {tuple(round(v, 2) for v in got)} (off {off:.2f})")


def main(argv):
    commands = {"script": cmd_script, "x11": cmd_x11, "pixels": cmd_pixels, "same": cmd_same, "file": cmd_file}
    if len(argv) < 2 or argv[1] not in commands:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    commands[argv[1]](*argv[2:])


if __name__ == "__main__":
    main(sys.argv)
