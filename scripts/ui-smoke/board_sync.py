#!/usr/bin/env python3
"""Board slice 7 checks for scripts/ui-smoke.sh: autosave and external changes.

The plan reuses board_edit.Plan on Tools.excalidraw (fixture draw.excalidraw).

  board_sync.py script TREE FIXTURE OUT.json   headless script: T, a click,
                                               "auto", Esc; marks typed (just
                                               after Esc) and autosaved (3 s
                                               later); no Cmd+S anywhere
  board_sync.py point TREE FIXTURE NAME        window point NAME ("x y")
  board_sync.py external SRC DST               DST = SRC with keep-text's text
                                               changed to THEIRS, written in
                                               place as another app might
  board_sync.py file SAVED BASE                SAVED keeps BASE's elements and
                                               top-level values exactly, and
                                               appends one text element "auto"
  board_sync.py theirs PNG BEFORE.png TREE FIXTURE
      keep-text's green ink in PNG reaches at least 40 px further right than
      in BEFORE (THEIRS is much longer than "keep me")
  board_sync.py same FILE BASE                 FILE is byte-identical to BASE
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import board_edit  # noqa: E402
import board_text  # noqa: E402

ROW = board_text.ROW
NEW = board_text.NEW
TYPED = "auto"
THEIRS = "keep me, changed by another app"
TEXT_INK = board_edit.TEXT_INK
die = board_edit.die


def points(plan):
    p = board_text.points(plan)
    p.update(plan.points())
    return p


def cmd_script(tree, fixture, out):
    plan = board_edit.Plan(tree, fixture)
    p = points(plan)
    ev = [
        {"t": 0.3, "type": "key", "name": "4", "cmd": True},
        {"t": 0.8, "type": "tap", "key": ROW},
        {"t": 2.0, "type": "mark", "name": "before"},
        {"t": 2.4, "type": "key", "name": "t"},
        {"t": 2.6, "type": "down", "x": p["new"][0], "y": p["new"][1]},
        {"t": 2.65, "type": "up", "x": p["new"][0], "y": p["new"][1]},
        {"t": 3.0, "type": "type", "text": TYPED, "every": 0.05},
        {"t": 3.5, "type": "key", "name": "Escape"},
        # The autosave waits at least 1 s of rest: just after Esc it is
        # still unsaved; 3 s later it is saved, with no Cmd+S.
        {"t": 3.7, "type": "mark", "name": "typed"},
        {"t": 6.5, "type": "mark", "name": "autosaved"},
        {"t": 7.0, "type": "mark", "name": "end"},
    ]
    json.dump({"events": ev}, open(out, "w"), indent=1)
    print(f"ok wrote {out}: {len(ev)} events, zoom {plan.zoom:.4f}")


def cmd_point(tree, fixture, name):
    x, y = points(board_edit.Plan(tree, fixture))[name]
    print(f"{round(x)} {round(y)}")


def cmd_external(src, dst):
    v = json.load(open(src))
    for e in v["elements"]:
        if e.get("id") == "keep-text":
            e["text"] = e["originalText"] = THEIRS
            e["width"] = round(e["width"] * len(THEIRS) / len("keep me"), 2)
            e["version"] += 1
            e["versionNonce"] += 1
            break
    else:
        die("no keep-text in " + src)
    # In place, as many editors write: the folder's time does not change, so
    # only the board watcher (not a vault rescan) can notice it.
    with open(dst, "r+") as f:
        f.truncate(0)
        json.dump(v, f, indent=2)
    print(f"ok another app wrote {dst} in place: keep-text reads {THEIRS!r}")


def cmd_file(saved_path, base_path):
    saved, base = json.load(open(saved_path)), json.load(open(base_path))
    se, be = saved.get("elements", []), base.get("elements", [])
    if len(se) != len(be) + 1:
        die(f"{len(se)} elements, expected {len(be)} + 1")
    for a, b in zip(se, be):
        if a != b:
            die(f"element {b.get('id')} changed")
    new = se[-1]
    if new.get("type") != "text" or new.get("text") != TYPED or new.get("originalText") != TYPED:
        die(f"the last element is not the typed text: {new.get('type')} {new.get('text')!r}")
    if new.get("isDeleted"):
        die("the typed text is deleted")
    for k in base:
        if k != "elements" and saved.get(k) != base[k]:
            die(f"top-level {k} changed")
    print(f"ok {saved_path}: {len(be)} elements kept exactly, {TYPED!r} appended")


def cmd_theirs(png, before, tree, fixture):
    from PIL import Image

    plan = board_edit.Plan(tree, fixture)
    t = plan.text_at(False)
    rect = plan.rect
    region = (int(t[0]) - 4, int(t[1]) - 4, int(rect[0] + rect[2]) - 2, int(t[3]) + 4)
    widths = []
    for path in (before, png):
        b, _ = board_edit.ink_box(Image.open(path).convert("RGB"), region, TEXT_INK)
        if not b:
            die(f"no keep-text ink in {path}")
        widths.append(b[2] - b[0])
    if widths[1] < widths[0] + 40:
        die(f"keep-text ink {widths[1]} px wide, before {widths[0]} px: not their longer text")
    print(f"ok keep-text ink {widths[0]} -> {widths[1]} px: their text is shown")


def cmd_same(path, base):
    if open(path, "rb").read() != open(base, "rb").read():
        die(f"{path} differs from {base}")
    print(f"ok {path} is byte-identical to {base}")


def main(argv):
    cmds = {
        "script": (cmd_script, 3),
        "point": (cmd_point, 3),
        "external": (cmd_external, 2),
        "file": (cmd_file, 2),
        "theirs": (cmd_theirs, 4),
        "same": (cmd_same, 2),
    }
    if len(argv) < 2 or argv[1] not in cmds or len(argv) - 2 != cmds[argv[1]][1]:
        die(__doc__)
    cmds[argv[1]][0](*argv[2:])


if __name__ == "__main__":
    main(sys.argv)
