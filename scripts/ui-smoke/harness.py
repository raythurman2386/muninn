#!/usr/bin/env python3
"""Helpers for scripts/ui-smoke.sh. Not shipped; standard library plus python-xlib.

  harness.py tree-mode TREE.json EXPECTED      assert the active mode button in a Rune --tree dump
  harness.py settings-mode SETTINGS.json EXPECTED   assert settings.json "mode"
  harness.py x11-wait TITLE TIMEOUT            print the id of the mapped window titled TITLE
  harness.py x11-chord WINDOW CHORD            focus WINDOW, press CHORD (e.g. ctrl+2) via XTEST
  harness.py x11-close WINDOW                  send WM_DELETE_WINDOW (the normal close path)

Mode keys are Muninn's settings keys: editor (Write), preview (Read), graph (Graph).
Later slices add board steps here (pointer drags for shapes, select, move, text).
"""

import json
import sys
import time

MODE_BUTTONS = {"editor": "mode-editor", "preview": "mode-preview", "graph": "mode-graph"}


def die(message):
    print(f"FAIL {message}", file=sys.stderr)
    sys.exit(1)


def walk(node):
    if isinstance(node, dict):
        yield node
        for value in node.values():
            yield from walk(value)
    elif isinstance(node, list):
        for value in node:
            yield from walk(value)


def tree_mode(path, expected):
    tree = json.load(open(path))
    # The selected segment of the Write/Read/Graph switch is the only one with
    # an opaque fill; the others are transparent (#rrggbb00).
    active = sorted(
        {
            n["key"]
            for n in walk(tree.get("elements", []))
            if n.get("key") in MODE_BUTTONS.values()
            and isinstance(n.get("fill"), str)
            and not n["fill"].endswith("00")
        }
    )
    want = MODE_BUTTONS[expected]
    if active != [want]:
        die(f"{path}: active mode buttons {active}, expected [{want}]")
    print(f"ok {path}: active mode {want}")


def settings_mode(path, expected):
    mode = json.load(open(path)).get("mode")
    if mode != expected:
        die(f"{path}: mode {mode!r}, expected {expected!r}")
    print(f"ok {path}: mode {mode}")


def display():
    from Xlib import display as xdisplay

    return xdisplay.Display()


def x11_wait(title, timeout):
    from Xlib import X

    d = display()
    deadline = time.monotonic() + float(timeout)
    while time.monotonic() < deadline:
        for w in walk_windows(d.screen().root):
            try:
                name = w.get_wm_name()
                attrs = w.get_attributes()
            except Exception:
                continue
            if name == title and attrs.map_state == X.IsViewable:
                g = w.get_geometry()
                print(f"{w.id:#x}")
                print(f"window {w.id:#x} '{name}' {g.width}x{g.height}", file=sys.stderr)
                return
        time.sleep(0.25)
    die(f"no mapped window titled {title!r} within {timeout}s")


def walk_windows(window):
    yield window
    try:
        children = window.query_tree().children
    except Exception:
        return
    for child in children:
        yield from walk_windows(child)


def window(d, wid):
    return d.create_resource_object("window", int(wid, 16))


def x11_chord(wid, chord):
    from Xlib import X, XK
    from Xlib.ext import xtest

    d = display()
    w = window(d, wid)
    # No window manager runs on the virtual display, so give the window focus
    # and put the pointer over it before typing.
    w.set_input_focus(X.RevertToParent, X.CurrentTime)
    g = w.get_geometry()
    xtest.fake_input(d, X.MotionNotify, x=g.x + g.width // 2, y=g.y + g.height // 2)
    d.sync()
    time.sleep(0.2)
    names = {"ctrl": "Control_L", "shift": "Shift_L", "alt": "Alt_L"}
    keys = [names.get(k, k) for k in chord.split("+")]
    codes = [d.keysym_to_keycode(XK.string_to_keysym(k)) for k in keys]
    if 0 in codes:
        die(f"unknown key in {chord}")
    for code in codes:
        xtest.fake_input(d, X.KeyPress, code)
        d.sync()
        time.sleep(0.03)
    for code in reversed(codes):
        xtest.fake_input(d, X.KeyRelease, code)
        d.sync()
        time.sleep(0.03)
    print(f"ok pressed {chord} on {wid}")


def x11_close(wid):
    from Xlib import X
    from Xlib.protocol import event

    d = display()
    w = window(d, wid)
    protocols = d.intern_atom("WM_PROTOCOLS")
    delete = d.intern_atom("WM_DELETE_WINDOW")
    ev = event.ClientMessage(window=w, client_type=protocols, data=(32, [delete, X.CurrentTime, 0, 0, 0]))
    w.send_event(ev, event_mask=X.NoEventMask)
    d.sync()
    print(f"ok sent WM_DELETE_WINDOW to {wid}")


def main(argv):
    commands = {
        "tree-mode": tree_mode,
        "settings-mode": settings_mode,
        "x11-wait": x11_wait,
        "x11-chord": x11_chord,
        "x11-close": x11_close,
    }
    if len(argv) < 2 or argv[1] not in commands:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    commands[argv[1]](*argv[2:])


if __name__ == "__main__":
    main(sys.argv)
