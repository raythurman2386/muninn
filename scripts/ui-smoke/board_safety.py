#!/usr/bin/env python3
"""#38 review smoke checks: unsaved board drawing is never dropped or lost.

  board_safety.py seed-recovery FIXTURE VAULT STATE_HOME REL OUT.json
      write a SAF-05 recovery copy for board REL (FIXTURE plus one green
      rectangle, id "recovered-rect") the way Muninn writes one: the board
      JSON in <state>/muninn/recovery/<rel>.txt and the sidecar beside it
      (exact rel, FNV-1a of the canonical vault root, Unix time); OUT.json
      gets the same board for later checks
  board_safety.py rect PNG TREE BOARD.json [absent]
      the green rectangle of BOARD.json is drawn where the fit camera puts
      it (or, with "absent", is not drawn)
  board_safety.py text TREE KEY NEEDLE    some element KEY in the tree (its
                                            accessibility node included) shows NEEDLE
  board_safety.py file PATH COUNT [ID...]   the board at PATH parses and
                                            holds COUNT elements, these ids among them
  board_safety.py recovery STATE_HOME REL COUNT|gone
      the recovery copy for REL holds a board of COUNT elements, or is gone
"""

import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import board_pixels  # noqa: E402

GREEN = "#b2f2bb"


def die(message):
    print(f"FAIL {message}", file=sys.stderr)
    sys.exit(1)


def vault_id(root):
    h = 0xCBF29CE484222325
    for b in os.path.realpath(root).encode():
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def sanitize(rel):
    out = []
    for c in rel:
        if c in "/\\":
            out.append("__")
        elif c.isascii() and (c.isalnum() or c in "-_."):
            out.append(c)
        else:
            out.append("_")
    return "".join(out)


def recovery_dir(state_home):
    return os.path.join(state_home, "muninn", "recovery")


def cmd_seed_recovery(fixture, vault, state_home, rel, out):
    board = json.load(open(fixture, encoding="utf-8"))
    rect = dict(next(e for e in board["elements"] if e["type"] == "rectangle"))
    rect.update({
        "id": "recovered-rect", "x": 140, "y": 180, "width": 120, "height": 80,
        "strokeColor": "#2f9e44", "backgroundColor": GREEN, "groupIds": [],
        "index": "b0", "version": 1, "versionNonce": 7, "seed": 7, "customData": None,
    })
    rect.pop("customData")
    board["elements"].append(rect)
    text = json.dumps(board, indent=2)
    d = recovery_dir(state_home)
    os.makedirs(d, exist_ok=True)
    key = sanitize(rel)
    open(os.path.join(d, key + ".txt"), "w", encoding="utf-8").write(text)
    meta = {"rel": rel, "vault": vault_id(vault), "ts": int(time.time())}
    open(os.path.join(d, key + ".json"), "w", encoding="utf-8").write(json.dumps(meta, separators=(",", ":")) + "\n")
    open(out, "w", encoding="utf-8").write(text)
    print(f"ok seeded a recovery copy for {rel}: {len(board['elements'])} elements")


def cmd_rect(png, tree, board_path, absent=None):
    from PIL import Image

    rect = board_pixels.canvas_rect(tree)
    boxes = board_pixels.element_boxes(board_path)
    bounds = [
        min(b[0][0] for b in boxes.values()), min(b[0][1] for b in boxes.values()),
        max(b[0][2] for b in boxes.values()), max(b[0][3] for b in boxes.values()),
    ]
    eye, zoom = board_pixels.fitted(bounds, (rect[2], rect[3]))
    (x0, y0, x1, y1), _ = boxes["recovered-rect"]
    sx0, sy0 = rect[0] + (x0 - eye[0]) * zoom + 6, rect[1] + (y0 - eye[1]) * zoom + 6
    sx1, sy1 = rect[0] + (x1 - eye[0]) * zoom - 6, rect[1] + (y1 - eye[1]) * zoom - 6
    img = Image.open(png).convert("RGB")
    area = (sx1 - sx0) * (sy1 - sy0)
    n = board_pixels.count(img, (sx0, sy0, sx1, sy1), GREEN, 40)
    if absent:
        if n > 0.05 * area:
            die(f"the recovered rectangle is drawn ({n}/{area:.0f} px) but should not be")
        print(f"ok the recovered rectangle is not drawn ({n} px)")
        return
    if n < 0.8 * area:
        die(f"the recovered rectangle is not drawn where planned ({n}/{area:.0f} px)")
    print(f"ok the recovered rectangle is drawn at the fit ({n}/{area:.0f} px, zoom {zoom:.3f})")


def cmd_text(tree, key, needle):
    def nodes(n):
        if isinstance(n, dict):
            yield n
            for v in n.values():
                yield from nodes(v)
        elif isinstance(n, list):
            for v in n:
                yield from nodes(v)

    def strings(n):
        for m in nodes(n):
            for v in m.values():
                if isinstance(v, str):
                    yield v

    keyed = [n for n in nodes(json.load(open(tree, encoding="utf-8"))) if n.get("key") == key]
    if not keyed:
        die(f"{tree}: no element {key}")
    for n in keyed:
        found = [s for s in strings(n) if needle in s]
        if found:
            print(f"ok {os.path.basename(tree)}: {key} says {found[0]!r}")
            return
    die(f"{tree}: {key} does not say {needle!r}")


def cmd_file(path, count, *ids):
    try:
        board = json.load(open(path, encoding="utf-8"))
    except (OSError, ValueError) as e:
        die(f"{path}: {e}")
    have = [e.get("id") for e in board.get("elements", [])]
    if len(have) != int(count):
        die(f"{path}: {len(have)} elements, want {count}")
    for i in ids:
        if i not in have:
            die(f"{path}: no element {i}")
    print(f"ok {os.path.basename(path)}: {len(have)} elements{', with ' + ', '.join(ids) if ids else ''}")


def copy_paths(state_home, rel):
    """The recovery copy's (text, sidecar) for rel: `<vault-id>~<key>` since
    the #38 review (M1), `<key>` in the older shared layout."""
    d = recovery_dir(state_home)
    key = sanitize(rel)
    names = os.listdir(d) if os.path.isdir(d) else []
    stems = sorted({n[: -len(".txt")] for n in names if n.endswith(".txt")} | {n[: -len(".json")] for n in names if n.endswith(".json")})
    stems = [st for st in stems if st == key or (len(st) == 17 + len(key) and st[16] == "~" and st.endswith(key))]
    return [(os.path.join(d, st + ".txt"), os.path.join(d, st + ".json")) for st in stems]


def cmd_recovery(state_home, rel, want):
    if want == "gone":
        if copy_paths(state_home, rel):
            die(f"the recovery copy for {rel} is still there: {copy_paths(state_home, rel)}")
        print(f"ok no recovery copy for {rel}")
        return
    # The app copies on its own frame: give a loaded machine a moment.
    deadline = time.monotonic() + 6 * max(float(os.environ.get("UI_SMOKE_SLOW", "1") or 1), 1.0)
    found = []
    while time.monotonic() < deadline:
        found = [(p, m) for p, m in copy_paths(state_home, rel) if os.path.isfile(p) and os.path.isfile(m)]
        if found:
            break
        time.sleep(0.2)
    if len(found) != 1:
        die(f"want one recovery copy (text and sidecar) for {rel}, found {found}")
    path, meta = found[0]
    if "~" not in os.path.basename(path):
        die(f"the recovery copy for {rel} is not keyed by its vault: {path}")
    cmd_file(path, want)
    print(f"ok the vault-keyed recovery copy for {rel} holds the drawing")


def main(argv):
    commands = {"seed-recovery": cmd_seed_recovery, "text": cmd_text, "rect": cmd_rect, "file": cmd_file, "recovery": cmd_recovery}
    if len(argv) < 2 or argv[1] not in commands:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    commands[argv[1]](*argv[2:])


if __name__ == "__main__":
    main(sys.argv)
