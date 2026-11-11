#!/usr/bin/env python3
"""Board slice 5 edit checks for scripts/ui-smoke.sh: select, move, delete, undo, redo.

The plan works in scene units on Tools.excalidraw (fixture draw.excalidraw).
The fit camera comes from the fixture and the board-canvas rect from a Rune
--tree dump, so the same plan drives the headless script and the XTEST
input, and predicts where every outline and every moved element lands.

  board_edit.py script TREE FIXTURE OUT.json  headless script; marks before,
                                              selected, moved, marquee, nudged,
                                              deleted, undone, redone, saved, dirty
  board_edit.py point TREE FIXTURE NAME       window point NAME ("x y")
  board_edit.py x11-shift-click WINDOW X Y    Shift+button-1 click with XTEST
  board_edit.py pixels MARK PNG BEFORE.png TREE FIXTURE
      the screenshot at MARK shows what the plan predicts: outlines in the
      selection colour around the selected elements only, the rectangle's
      fill where it moved to and not where it was, the text's ink shifted by
      the nudges or gone, and every other canvas pixel as in BEFORE
  board_edit.py file SAVED ORIGINAL RESAVED TREE [TEXT_DELETED]
      the saved board keeps all 5 elements in order; keep-rect moved by the
      drag and keep-text by the nudges, each with a bumped version, a new
      versionNonce and a fresh `updated`, and nothing else changed;
      keep-text is a tombstone (isDeleted: true) unless TEXT_DELETED is "no";
      the untouched elements and top-level values are byte-identical to
      RESAVED (ORIGINAL through s1's own load/save)
"""

import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import board_draw  # noqa: E402
import board_pixels  # noqa: E402

ROW = "board-row-2"
# board_edit.rs: SELECTION, OUTLINE_GAP; board_draw.py: the toolbar strip.
OUTLINE_GAP = 4
TOOLBAR = board_draw.TOOLBAR
RECT_FILL = "#ffc9c9"
TEXT_INK = "#2f9e44"
# The plan: drag keep-rect by DRAG window pixels; nudge keep-text with
# Shift+Right, Shift+Right, Shift+Down, Right (5, 5, 5, 1 scene units).
DRAG = (100, 80)
NUDGES = [("Right", True), ("Right", True), ("Down", True), ("Right", False)]
NUDGE = (11, 5)
RECT = (0.0, 0.0, 60.0, 40.0)
TEXT = (0.0, 450.1234567890123, 72.5, 25.0)
# Versions in the fixture; each edit bumps once.
RECT_V, TEXT_V = 12, 4
UNTOUCHED = ["keep-ellipse", "keep-frame", "gone"]


def die(message):
    print(f"FAIL {message}", file=sys.stderr)
    sys.exit(1)


class Plan:
    def __init__(self, tree, fixture):
        self.rect = board_pixels.canvas_rect(tree)
        boxes = board_pixels.element_boxes(fixture)
        bounds = [
            min(b[0][0] for b in boxes.values()), min(b[0][1] for b in boxes.values()),
            max(b[0][2] for b in boxes.values()), max(b[0][3] for b in boxes.values()),
        ]
        self.eye, self.zoom = board_pixels.fitted(bounds, (self.rect[2], self.rect[3]))

    def win(self, s):
        """Window point of scene point s."""
        return (
            self.rect[0] + (s[0] - self.eye[0]) * self.zoom,
            self.rect[1] + (s[1] - self.eye[1]) * self.zoom,
        )

    def box(self, b, shift=(0.0, 0.0), screen_shift=(0.0, 0.0)):
        """Window box of scene box b (x, y, w, h), moved by scene and window offsets."""
        a = self.win((b[0] + shift[0], b[1] + shift[1]))
        c = self.win((b[0] + b[2] + shift[0], b[1] + b[3] + shift[1]))
        return (a[0] + screen_shift[0], a[1] + screen_shift[1], c[0] + screen_shift[0], c[1] + screen_shift[1])

    def rect_at(self, moved):
        return self.box(RECT, screen_shift=DRAG if moved else (0, 0))

    def text_at(self, nudged):
        return self.box(TEXT, shift=NUDGE if nudged else (0, 0))

    def points(self):
        r = self.rect_at(False)
        rc = ((r[0] + r[2]) / 2, (r[1] + r[3]) / 2)
        t = self.text_at(False)
        tc = ((t[0] + t[2]) / 2, (t[1] + t[3]) / 2)
        # Marquees start at their bottom right, away from the toolbar.
        pad = 12
        return {
            "rect": rc,
            "rect-moved": (rc[0] + DRAG[0], rc[1] + DRAG[1]),
            "text": tc,
            "marquee-from": (t[2] + pad, t[3] + pad),
            "marquee-to": (t[0] - pad, t[1] - pad),
        }


def path(a, b, n=10):
    return [(a[0] + (b[0] - a[0]) * i / n, a[1] + (b[1] - a[1]) * i / n) for i in range(n + 1)]


def cmd_script(tree, fixture, out):
    plan = Plan(tree, fixture)
    p = plan.points()
    ev = [
        {"t": 0.3, "type": "key", "name": "4", "cmd": True},
        {"t": 0.8, "type": "tap", "key": ROW},
        {"t": 2.0, "type": "mark", "name": "before"},
    ]
    t = [2.4]

    def at(e):
        e["t"] = round(t[0], 3)
        ev.append(e)

    def gap(dt):
        t[0] += dt

    def stroke(pts):
        at({"type": "down", "x": pts[0][0], "y": pts[0][1]})
        for q in pts[1:]:
            gap(1 / 60)
            at({"type": "move", "x": q[0], "y": q[1]})
        gap(0.05)
        at({"type": "up", "x": pts[-1][0], "y": pts[-1][1]})
        gap(0.4)

    def mark(name):
        at({"type": "mark", "name": name})
        gap(0.4)

    def key(name, **mods):
        at({"type": "key", "name": name, **mods})
        gap(0.15)

    stroke([p["rect"], p["rect"]])
    mark("selected")
    stroke(path(p["rect"], p["rect-moved"]))
    mark("moved")
    key("Escape")
    stroke(path(p["marquee-from"], p["marquee-to"]))
    mark("marquee")
    for name, shift in NUDGES:
        key(name, shift=shift)
    mark("nudged")
    key("Delete")
    mark("deleted")
    key("z", cmd=True)
    mark("undone")
    key("z", cmd=True, shift=True)
    mark("redone")
    key("s", cmd=True)
    gap(0.3)
    mark("saved")
    key("z", cmd=True)
    mark("dirty")
    mark("end")
    json.dump({"events": ev}, open(out, "w"), indent=1)
    print(f"ok wrote {out}: {len(ev)} events, zoom {plan.zoom:.4f}")


def cmd_point(tree, fixture, name):
    x, y = Plan(tree, fixture).points()[name]
    print(f"{round(x)} {round(y)}")


def cmd_x11_shift_click(wid, x, y):
    from Xlib import X, XK
    from Xlib.ext import xtest

    import harness

    d = harness.display()
    w = harness.focus(d, wid)
    rx, ry = harness.to_root(d, w, x, y)
    shift = d.keysym_to_keycode(XK.string_to_keysym("Shift_L"))
    xtest.fake_input(d, X.MotionNotify, x=rx, y=ry)
    d.sync()
    time.sleep(0.15)
    xtest.fake_input(d, X.KeyPress, shift)
    d.sync()
    time.sleep(0.1)
    xtest.fake_input(d, X.ButtonPress, 1)
    d.sync()
    time.sleep(0.08)
    xtest.fake_input(d, X.ButtonRelease, 1)
    d.sync()
    time.sleep(0.1)
    xtest.fake_input(d, X.KeyRelease, shift)
    d.sync()
    print(f"ok shift-clicked {x},{y} in {wid}")


def violet(p):
    """The selection colour #6965db, alone or blended with paper."""
    r, g, b = p[:3]
    return b - g > 20 and -15 <= r - g <= 25 and b > 150


def violets(img, region):
    x0, y0, x1, y1 = (int(round(v)) for v in region)
    px = img.load()
    return sum(
        1
        for y in range(max(y0, 0), min(y1, img.height))
        for x in range(max(x0, 0), min(x1, img.width))
        if violet(px[x, y])
    )


def outlined(img, box):
    """Each side of the outline OUTLINE_GAP outside box has selection ink."""
    x0, y0, x1, y1 = box[0] - OUTLINE_GAP, box[1] - OUTLINE_GAP, box[2] + OUTLINE_GAP, box[3] + OUTLINE_GAP
    sides = []
    for f in (0.25, 0.75):
        x, y = x0 + (x1 - x0) * f, y0 + (y1 - y0) * f
        sides += [(x, y0), (x, y1), (x0, y), (x1, y)]
    return [violets(img, (sx - 2, sy - 2, sx + 3, sy + 3)) for sx, sy in sides]


def ink_box(img, region, colour, tolerance=90):
    x0, y0, x1, y1 = (int(round(v)) for v in region)
    c = board_pixels.rgb(colour)
    px = img.load()
    xs, ys = [], []
    for y in range(max(y0, 0), min(y1, img.height)):
        for x in range(max(x0, 0), min(x1, img.width)):
            p = px[x, y]
            if abs(p[0] - c[0]) + abs(p[1] - c[1]) + abs(p[2] - c[2]) <= tolerance:
                xs.append(x)
                ys.append(y)
    if len(xs) < 20:
        return None, len(xs)
    return (min(xs), min(ys), max(xs), max(ys)), len(xs)


def grow(b, n):
    return (b[0] - n, b[1] - n, b[2] + n, b[3] + n)


def shrink(b, n):
    return grow(b, -n)


# What each mark shows: (rect moved?, selected ids, text state).
STATES = {
    "before": (False, [], "there"),
    "selected": (False, ["rect"], "there"),
    "moved": (True, ["rect"], "there"),
    "both": (True, ["rect", "text"], "there"),
    "marquee": (True, ["text"], "there"),
    "nudged": (True, ["text"], "nudged"),
    "deleted": (True, [], "gone"),
    "undone": (True, ["text"], "nudged"),
    "redone": (True, [], "gone"),
    "saved": (True, [], "gone"),
    "dirty": (True, ["text"], "nudged"),
}


def cmd_pixels(mark, png, before, tree, fixture):
    from PIL import Image

    if mark not in STATES:
        die(f"unknown mark {mark}")
    moved, selected, text = STATES[mark]
    plan = Plan(tree, fixture)
    img = Image.open(png).convert("RGB")
    ref = Image.open(before).convert("RGB")
    canvas = board_pixels.area(plan.rect)
    canvas = (canvas[0], max(canvas[1], int(plan.rect[1]) + TOOLBAR), canvas[2], canvas[3])
    rect_now, rect_was = plan.rect_at(moved), plan.rect_at(False)
    text_box = plan.text_at(text == "nudged")
    boxes = {"rect": rect_now, "text": text_box}

    # The rectangle's fill is where the plan put it, and not where it was.
    inner = shrink(rect_now, 6)
    area = (inner[2] - inner[0]) * (inner[3] - inner[1])
    fill = board_pixels.count(img, inner, RECT_FILL, 40)
    if fill < 0.8 * area:
        die(f"{mark}: rectangle fill at {tuple(round(v) for v in inner)} is {fill}/{area:.0f} px")
    print(f"ok {mark}: rectangle fill where planned ({fill}/{area:.0f} px)")
    if moved:
        left = board_pixels.count(img, shrink(rect_was, 6), RECT_FILL, 40)
        if left > 10:
            die(f"{mark}: {left} px of rectangle fill left where it was")
        print(f"ok {mark}: no fill where the rectangle was ({left} px)")

    # The text's ink: in place, shifted exactly by the nudges, or gone.
    search = grow(plan.text_at(False), 30)
    search = (search[0], search[1], search[2] + NUDGE[0] * plan.zoom, search[3] + NUDGE[1] * plan.zoom)
    # Inside the inspected canvas only: the key hint below it has grey-green text.
    search = (max(search[0], canvas[0]), max(search[1], canvas[1]), min(search[2], canvas[2]), min(search[3], canvas[3]))
    was, _ = ink_box(ref, search, TEXT_INK)
    if was is None:
        die("no text ink in the before shot")
    now, n = ink_box(img, search, TEXT_INK)
    if text == "gone":
        if now is not None:
            die(f"{mark}: text ink is still drawn ({n} px at {now})")
        print(f"ok {mark}: the text is gone ({n} px)")
    else:
        if now is None:
            die(f"{mark}: no text ink ({n} px)")
        want = (0.0, 0.0) if text == "there" else (NUDGE[0] * plan.zoom, NUDGE[1] * plan.zoom)
        off = (now[0] - was[0], now[1] - was[1])
        # The far edges get 3 px: the text's ink runs a little past its box,
        # under the 1 px outline that is drawn OUTLINE_GAP outside it.
        err = max(abs(off[0] - want[0]), abs(off[1] - want[1]))
        far = max(abs((now[2] - was[2]) - want[0]), abs((now[3] - was[3]) - want[1]))
        if err > 1.5 or far > 3:
            die(f"{mark}: text ink moved by {off}, want {tuple(round(v, 2) for v in want)}")
        print(f"ok {mark}: text ink moved by {off} (want {want[0]:.2f}, {want[1]:.2f})")

    # Outlines around exactly the selected elements.
    for name, b in boxes.items():
        sides = outlined(img, b)
        if name in selected:
            if min(sides) < 1:
                die(f"{mark}: the {name} outline is missing on a side {sides}")
            print(f"ok {mark}: {name} outlined {sides}")
        elif max(sides) > 0:
            die(f"{mark}: the {name} has selection ink {sides} but is not selected")
    if not selected:
        stray = violets(img, canvas)
        if stray > 4:
            die(f"{mark}: {stray} px of selection colour with nothing selected")
        print(f"ok {mark}: no selection drawn ({stray} px)")

    # Every other canvas pixel is as before (the untouched elements included).
    keep_out = [grow(b, 16) for b in (rect_was, plan.rect_at(True), plan.text_at(False), plan.text_at(True))]
    if len(selected) > 1:
        union = (min(rect_now[0], text_box[0]), min(rect_now[1], text_box[1]), max(rect_now[2], text_box[2]), max(rect_now[3], text_box[3]))
        keep_out.append(("union", grow(union, 16)))
    pa, pb = img.load(), ref.load()
    changed = 0
    for y in range(canvas[1], canvas[3]):
        for x in range(canvas[0], canvas[2]):
            if any(_inside(x, y, b) for b in keep_out):
                continue
            if max(abs(u - v) for u, v in zip(pa[x, y], pb[x, y])) > 24:
                changed += 1
    if changed:
        die(f"{mark}: {changed} canvas pixels changed outside the edited elements")
    print(f"ok {mark}: nothing else on the canvas changed")


def _inside(x, y, b):
    if isinstance(b[0], str):
        u = b[1]
        # The union box: only its outline ring counts as touched.
        return (u[0] <= x <= u[2] and u[1] <= y <= u[3]) and not (u[0] + 24 <= x <= u[2] - 24 and u[1] + 24 <= y <= u[3] - 24)
    return b[0] <= x <= b[2] and b[1] <= y <= b[3]


def cmd_file(saved_path, original_path, resaved_path, tree, text_deleted="yes"):
    saved_text = open(saved_path, encoding="utf-8").read()
    try:
        saved = json.loads(saved_text)
    except ValueError as e:
        die(f"{saved_path} does not parse: {e}")
    original = json.load(open(original_path, encoding="utf-8"))
    ids = [e.get("id") for e in saved["elements"]]
    want_ids = [e.get("id") for e in original["elements"]]
    if ids != want_ids:
        die(f"elements {ids}, want {want_ids} in the same order")
    print(f"ok parses: the same {len(ids)} elements in order")
    for k, v in original.items():
        if k != "elements" and saved.get(k) != v:
            die(f"top-level {k} changed")
    plan = Plan(tree, original_path)
    tol = 1.0 / plan.zoom + 0.5
    by = {e["id"]: e for e in saved["elements"]}
    orig = {e["id"]: e for e in original["elements"]}
    now = time.time() * 1000
    deleted = text_deleted != "no"
    edits = {
        # id: (expected x, y, tolerance, versions, isDeleted)
        "keep-rect": (DRAG[0] / plan.zoom, DRAG[1] / plan.zoom, tol, RECT_V + 1, False),
        # Four nudges, the delete, its undo and its redo; or one more undo.
        "keep-text": (TEXT[0] + NUDGE[0], TEXT[1] + NUDGE[1], 1e-9, TEXT_V + 4 + 3 + (0 if deleted else 1), deleted),
    }
    for id_, (x, y, t, version, gone) in edits.items():
        e, o = by[id_], orig[id_]
        if abs(e["x"] - x) > t or abs(e["y"] - y) > t:
            die(f"{id_}: at {e['x']}, {e['y']}, want {x:.3f}, {y:.3f} (±{t:.3g})")
        if e["version"] != version:
            die(f"{id_}: version {e['version']}, want {version}")
        if e["versionNonce"] == o["versionNonce"] or not (isinstance(e["versionNonce"], int) and 0 <= e["versionNonce"] < 2**31):
            die(f"{id_}: versionNonce {e['versionNonce']} was not renewed")
        if not (isinstance(e.get("updated"), int) and abs(e["updated"] - now) < 86_400_000 and e["updated"] > o["updated"]):
            die(f"{id_}: updated {e.get('updated')} is not a fresh ms timestamp")
        if e["isDeleted"] is not gone:
            die(f"{id_}: isDeleted {e['isDeleted']}, want {gone}")
        for k, v in o.items():
            if k in ("x", "y", "version", "versionNonce", "updated", "isDeleted"):
                continue
            if e.get(k) != v:
                die(f"{id_}: {k} {e.get(k)!r} != {v!r}")
        if set(e) - set(o):
            die(f"{id_}: new keys {sorted(set(e) - set(o))}")
        print(f"ok {id_}: at ({e['x']:.3f}, {e['y']:.3f}), version {o['version']}→{e['version']}, new nonce, updated {e['updated']}, isDeleted {e['isDeleted']}; every other field kept")

    resaved_text = open(resaved_path, encoding="utf-8").read()
    sa, sitems = board_draw.spans(saved_text)
    ra, ritems = board_draw.spans(resaved_text)
    for k, (a, b) in ra.items():
        if k == "elements":
            continue
        if sa.get(k) is None or saved_text[slice(*sa[k])] != resaved_text[a:b]:
            die(f"top-level {k} bytes differ from s1's save")
    resaved = json.loads(resaved_text)
    for i, e in enumerate(resaved["elements"]):
        if e["id"] in UNTOUCHED:
            a, b = ritems[i]
            c, d = sitems[i]
            if resaved_text[a:b] != saved_text[c:d]:
                die(f"untouched element {e['id']} bytes differ from s1's save")
    print(f"ok {', '.join(UNTOUCHED)} and {len(ra) - 1} top-level values are byte-identical to s1's save")


def main(argv):
    commands = {
        "script": cmd_script, "point": cmd_point, "x11-shift-click": cmd_x11_shift_click,
        "pixels": cmd_pixels, "file": cmd_file,
    }
    if len(argv) < 2 or argv[1] not in commands:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    commands[argv[1]](*argv[2:])


if __name__ == "__main__":
    main(sys.argv)
