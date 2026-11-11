#!/usr/bin/env python3
"""Board slice 6 text checks for scripts/ui-smoke.sh: place, type, edit, undo, redo.

The plan works in scene units on Tools.excalidraw (fixture draw.excalidraw),
with the fit camera from the fixture and the board-canvas rect from a Rune
--tree dump (board_edit.Plan), so one plan drives the headless script and
the XTEST input and predicts where the ink lands.

  board_text.py script TREE FIXTURE OUT.json   headless script; marks before,
                                               typing, committed, editing,
                                               edited, undone, redone, saved
  board_text.py point TREE FIXTURE NAME        window point NAME ("x y")
  board_text.py x11-type WINDOW TEXT           type TEXT with XTEST, mapping a
                                               spare keycode for non-ASCII
  board_text.py x11-double-click WINDOW X Y    a double click with XTEST
  board_text.py pixels MARK PNG BEFORE.png TREE FIXTURE
      MARK's screenshot: the new text's dark ink where it was placed (from
      "typing" on), none before; the drag after the commit drew nothing (the
      keys typed in the editor picked no tool); keep-text's green ink as in
      BEFORE, longer once edited, back after undo, longer again after redo;
      every other canvas pixel as in BEFORE
  board_text.py file SAVED ORIGINAL RESAVED TREE
      the saved board keeps the 5 elements in order and appends one text
      element with TYPED as text and originalText, Excalidraw's text fields
      and plausible measured geometry at the click; keep-text reads EDITED
      with its version bumped by the edit, the undo and the redo; the other
      elements and top-level values are byte-identical to RESAVED
"""

import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import board_draw  # noqa: E402
import board_edit  # noqa: E402
import board_pixels  # noqa: E402

ROW = "board-row-2"
TOOLBAR = board_draw.TOOLBAR
TEXT_INK = board_edit.TEXT_INK
# The plan: T, click at NEW, type TYPED (tool letters and a digit in it),
# Esc; a drag over DRAG_FROM..DRAG_TO (a rectangle if "r" had picked the
# tool); double-click keep-text, type "!", Esc; Ctrl+Z, Ctrl+Shift+Z; save.
NEW = (200.0, 180.0)
TYPED = "r1 dü\nok"
DRAG_FROM, DRAG_TO = (300.0, 280.0), (380.0, 360.0)
EDITED = "keep me!"
TEXT = board_edit.TEXT
TEXT_V = board_edit.TEXT_V
UNTOUCHED = ["keep-rect", "keep-ellipse", "keep-frame", "gone"]
die = board_edit.die


def points(plan):
    t = plan.text_at(False)
    return {
        "new": plan.win(NEW),
        "drag-from": plan.win(DRAG_FROM),
        "drag-to": plan.win(DRAG_TO),
        "text": ((t[0] + t[2]) / 2, (t[1] + t[3]) / 2),
    }


def cmd_script(tree, fixture, out):
    plan = board_edit.Plan(tree, fixture)
    p = points(plan)
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

    def click(q, then=0.4):
        at({"type": "down", "x": q[0], "y": q[1]})
        gap(0.05)
        at({"type": "up", "x": q[0], "y": q[1]})
        gap(then)

    def mark(name):
        at({"type": "mark", "name": name})
        gap(0.4)

    def key(name, **mods):
        at({"type": "key", "name": name, **mods})
        gap(0.15)

    def typed(s):
        for line_no, line in enumerate(s.split("\n")):
            if line_no:
                key("Enter")
            at({"type": "type", "text": line, "every": 0.05})
            gap(0.05 * len(line) + 0.2)

    key("t")
    click(p["new"])
    typed(TYPED)
    mark("typing")
    key("Escape")
    pts = board_edit.path(p["drag-from"], p["drag-to"])
    at({"type": "down", "x": pts[0][0], "y": pts[0][1]})
    for q in pts[1:]:
        gap(1 / 60)
        at({"type": "move", "x": q[0], "y": q[1]})
    gap(0.05)
    at({"type": "up", "x": pts[-1][0], "y": pts[-1][1]})
    gap(0.6)
    mark("committed")
    click(p["text"], then=0.08)
    click(p["text"])
    mark("editing")
    typed("!")
    key("Escape")
    mark("edited")
    key("z", cmd=True)
    mark("undone")
    key("z", cmd=True, shift=True)
    mark("redone")
    key("s", cmd=True)
    gap(0.3)
    mark("saved")
    mark("end")
    json.dump({"events": ev}, open(out, "w"), indent=1)
    print(f"ok wrote {out}: {len(ev)} events, zoom {plan.zoom:.4f}")


def cmd_point(tree, fixture, name):
    x, y = points(board_edit.Plan(tree, fixture))[name]
    print(f"{round(x)} {round(y)}")


def cmd_x11_type(wid, text):
    """Type TEXT; a character without a keycode is mapped onto a spare one."""
    from Xlib import X, XK
    from Xlib.ext import xtest

    import harness

    d = harness.display()
    harness.focus(d, wid)
    shift = d.keysym_to_keycode(XK.string_to_keysym("Shift_L"))
    lo, hi = d.display.info.min_keycode, d.display.info.max_keycode
    mapping = d.get_keyboard_mapping(lo, hi - lo + 1)
    spare = next((lo + i for i, syms in enumerate(mapping) if not any(syms)), None)

    def press(code, upper=False):
        if upper:
            xtest.fake_input(d, X.KeyPress, shift)
        xtest.fake_input(d, X.KeyPress, code)
        xtest.fake_input(d, X.KeyRelease, code)
        if upper:
            xtest.fake_input(d, X.KeyRelease, shift)
        d.sync()
        time.sleep(0.06)

    for ch in text:
        name = {" ": "space", "\n": "Return", "!": "exclam"}.get(ch, ch)
        sym = XK.string_to_keysym(name)
        code = d.keysym_to_keycode(sym) if sym else 0
        if code:
            press(code, ch.isupper() or ch == "!")
            continue
        if spare is None:
            die(f"cannot type {ch!r}: no spare keycode")
        u = ord(ch)
        sym = u if u < 0x100 else 0x01000000 | u
        d.change_keyboard_mapping(spare, [(sym, sym)])
        d.sync()
        time.sleep(0.3)
        press(spare)
        time.sleep(0.2)
        d.change_keyboard_mapping(spare, [(0, 0)])
        d.sync()
    harness.settle()
    print(f"ok typed {text!r} on {wid}")


def cmd_x11_double_click(wid, x, y):
    """Two button-1 clicks well inside a double-click interval."""
    from Xlib import X
    from Xlib.ext import xtest

    import harness

    d = harness.display()
    w = harness.focus(d, wid)
    rx, ry = harness.to_root(d, w, int(x), int(y))
    xtest.fake_input(d, X.MotionNotify, x=rx, y=ry)
    d.sync()
    time.sleep(0.2)
    for _ in range(2):
        xtest.fake_input(d, X.ButtonPress, 1)
        d.sync()
        time.sleep(0.04)
        xtest.fake_input(d, X.ButtonRelease, 1)
        d.sync()
        time.sleep(0.08)
    harness.settle()
    print(f"ok double-clicked {x},{y} in {wid}")


# What each mark shows: (new text drawn?, keep-text state).
STATES = {
    "before": (False, "there"),
    "typing": (True, "there"),
    "committed": (True, "there"),
    "editing": (True, "caret"),
    "edited": (True, "longer"),
    "undone": (True, "there"),
    "redone": (True, "longer"),
    "saved": (True, "longer"),
}


def cmd_pixels(mark, png, before, tree, fixture):
    from PIL import Image

    if mark not in STATES:
        die(f"unknown mark {mark}")
    new, text = STATES[mark]
    plan = board_edit.Plan(tree, fixture)
    img = Image.open(png).convert("RGB")
    ref = Image.open(before).convert("RGB")
    canvas = board_pixels.area(plan.rect)
    canvas = (canvas[0], max(canvas[1], int(plan.rect[1]) + TOOLBAR), canvas[2], canvas[3])
    z = plan.zoom
    # Two lines of 20 × 1.25 from NEW, up to ~8 em wide.
    a = plan.win(NEW)
    new_box = (a[0] - 4, a[1] - 4, a[0] + 170 * z, a[1] + 54 * z)
    ink = board_pixels.dark(img, new_box)
    if new:
        if ink < 30:
            die(f"{mark}: no new text ink at {tuple(round(v) for v in new_box)} ({ink} px)")
        # Both lines: ink in the upper and the lower half.
        mid = (new_box[1] + new_box[3]) / 2
        top = board_pixels.dark(img, (new_box[0], new_box[1], new_box[2], mid))
        low = board_pixels.dark(img, (new_box[0], mid, new_box[2], new_box[3]))
        if top < 10 or low < 10:
            die(f"{mark}: the new text is not on two lines (top {top} px, bottom {low} px)")
        print(f"ok {mark}: new text ink on two lines at the click ({top} + {low} px)")
    elif ink > 4:
        die(f"{mark}: {ink} px of dark ink where no text is yet")
    else:
        print(f"ok {mark}: nothing at the click yet ({ink} px)")

    search = board_edit.grow(plan.text_at(False), 30)
    search = (search[0], search[1], search[2] + 80 * z, search[3])
    search = (max(search[0], canvas[0]), max(search[1], canvas[1]), min(search[2], canvas[2]), min(search[3], canvas[3]))
    was, _ = board_edit.ink_box(ref, search, TEXT_INK)
    now, n = board_edit.ink_box(img, search, TEXT_INK)
    if was is None or now is None:
        die(f"{mark}: no keep-text ink ({n} px)")
    grew = now[2] - was[2]
    # In its editor the text sits where it was drawn, plus the caret after
    # it (a 1 px bar a little taller than the glyphs).
    caret = 3 if text == "caret" else 1.5
    if abs(now[0] - was[0]) > 1.5 or abs(now[1] - was[1]) > caret:
        die(f"{mark}: keep-text moved: {was} -> {now}")
    if text == "caret" and not 0 <= grew <= 6:
        die(f"{mark}: the editor's text does not end where keep-text did (+{grew} px)")
    if text == "there" and abs(grew) > 2:
        die(f"{mark}: keep-text ink width changed by {grew} px")
    if text == "longer" and grew < 3:
        die(f"{mark}: keep-text ink did not grow (by {grew} px)")
    print(f"ok {mark}: keep-text ink in place, right edge {grew:+d} px ({text})")

    # Every other canvas pixel is as before: the drag drew no rectangle.
    keep_out = [board_edit.grow(new_box, 8), board_edit.grow(search, 4)]
    pa, pb = img.load(), ref.load()
    changed = 0
    for y in range(canvas[1], canvas[3]):
        for x in range(canvas[0], canvas[2]):
            if any(board_edit._inside(x, y, b) for b in keep_out):
                continue
            if max(abs(u - v) for u, v in zip(pa[x, y], pb[x, y])) > 24:
                changed += 1
    if changed:
        die(f"{mark}: {changed} canvas pixels changed outside the text")
    print(f"ok {mark}: nothing else on the canvas changed (no shape from the drag)")


def cmd_file(saved_path, original_path, resaved_path, tree):
    saved_text = open(saved_path, encoding="utf-8").read()
    try:
        saved = json.loads(saved_text)
    except ValueError as e:
        die(f"{saved_path} does not parse: {e}")
    original = json.load(open(original_path, encoding="utf-8"))
    ids = [e.get("id") for e in saved["elements"]]
    want = [e.get("id") for e in original["elements"]]
    if ids[: len(want)] != want or len(ids) != len(want) + 1:
        die(f"elements {ids}, want {want} and one new element")
    print(f"ok parses: the {len(want)} elements in order and one new")
    for k, v in original.items():
        if k != "elements" and saved.get(k) != v:
            die(f"top-level {k} changed")
    plan = board_edit.Plan(tree, original_path)
    tol = 2.0 / plan.zoom + 0.5
    e = saved["elements"][-1]
    fields = {
        "type": "text", "text": TYPED, "originalText": TYPED, "fontSize": 20,
        "fontFamily": 5, "textAlign": "left", "verticalAlign": "top",
        "lineHeight": 1.25, "containerId": None, "autoResize": True,
        "isDeleted": False, "angle": 0, "locked": False,
    }
    for k, v in fields.items():
        if e.get(k) != v:
            die(f"new text: {k} {e.get(k)!r}, want {v!r}")
    if abs(e["x"] - NEW[0]) > tol or abs(e["y"] - NEW[1]) > tol:
        die(f"new text at {e['x']}, {e['y']}, want {NEW} (±{tol:.2f})")
    if not (40 <= e["width"] <= 160) or abs(e["height"] - 50) > 0.01:
        die(f"new text size {e['width']} × {e['height']}: want ~5 glyphs wide, 2 × 25 high")
    for k in ("id", "seed", "versionNonce", "index", "updated"):
        if e.get(k) in (None, ""):
            die(f"new text: no {k}")
    print(f"ok new text: {TYPED!r} as text and originalText, at ({e['x']}, {e['y']}), {e['width']} × {e['height']}, Excalidraw's text fields")

    by = {x["id"]: x for x in saved["elements"]}
    orig = {x["id"]: x for x in original["elements"]}
    k, o = by["keep-text"], orig["keep-text"]
    if k["text"] != EDITED or k["originalText"] != EDITED:
        die(f"keep-text reads {k['text']!r} / {k['originalText']!r}, want {EDITED!r}")
    if k["version"] != TEXT_V + 3:
        die(f"keep-text version {k['version']}, want {TEXT_V + 3} (edit, undo, redo)")
    if k["versionNonce"] == o["versionNonce"] or not k["updated"] > o["updated"]:
        die("keep-text versionNonce / updated not renewed")
    if not k["width"] > o["width"] or k["height"] != o["height"]:
        die(f"keep-text size {k['width']} × {k['height']} (was {o['width']} × {o['height']})")
    for f, v in o.items():
        if f in ("text", "originalText", "width", "version", "versionNonce", "updated"):
            continue
        if k.get(f) != v:
            die(f"keep-text: {f} {k.get(f)!r} != {v!r}")
    print(f"ok keep-text: {EDITED!r}, version {o['version']}→{k['version']}, wider ({o['width']}→{k['width']}); every other field kept")

    resaved_text = open(resaved_path, encoding="utf-8").read()
    sa, sitems = board_draw.spans(saved_text)
    ra, ritems = board_draw.spans(resaved_text)
    for key_, (a, b) in ra.items():
        if key_ != "elements" and (sa.get(key_) is None or saved_text[slice(*sa[key_])] != resaved_text[a:b]):
            die(f"top-level {key_} bytes differ from s1's save")
    resaved = json.loads(resaved_text)
    for i, x in enumerate(resaved["elements"]):
        if x["id"] in UNTOUCHED:
            a, b = ritems[i]
            c, d = sitems[i]
            if resaved_text[a:b] != saved_text[c:d]:
                die(f"untouched element {x['id']} bytes differ from s1's save")
    print(f"ok {', '.join(UNTOUCHED)} and {len(ra) - 1} top-level values are byte-identical to s1's save")


def main(argv):
    commands = {
        "script": cmd_script, "point": cmd_point, "x11-type": cmd_x11_type,
        "x11-double-click": cmd_x11_double_click,
        "pixels": cmd_pixels, "file": cmd_file,
    }
    if len(argv) < 2 or argv[1] not in commands:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    commands[argv[1]](*argv[2:])


if __name__ == "__main__":
    main(sys.argv)
