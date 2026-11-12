#!/usr/bin/env python3
"""Kanban slice 7 smoke checks: the kanban file follows the disk and nothing is lost.

  kanban_sync.py expect FIXTURE OUT mine|theirs|mine-theirs
      write the bytes a step expects: FIXTURE with "Fix the sync bug" checked
      (mine), with one card another app added at the end of Done (theirs), or
      both (mine-theirs: theirs reloaded, then mine on top)
  kanban_sync.py atomic SRC DEST
      another app's atomic save: SRC's bytes to a temp file beside DEST,
      fsynced, then renamed over DEST
  kanban_sync.py seed-recovery TEXT VAULT STATE_HOME REL
      write a SAF-05 recovery copy of TEXT's bytes for REL the way Muninn
      writes one (<vault-id>~<key>.txt and its sidecar)
  kanban_sync.py recovery STATE_HOME REL FILE|gone
      the recovery copy for REL holds FILE's bytes (waiting up to 6 s for the
      app's own frame), or there is none
  kanban_sync.py log STATE_HOME NEEDLE
      the session log has a line with NEEDLE (waiting up to 6 s)
"""

import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from board_safety import copy_paths, die, recovery_dir, sanitize, vault_id  # noqa: E402

MINE = ("- [ ] Fix the sync bug\n", "- [x] Fix the sync bug\n")
THEIRS = ("- [ ] Tag the release\n", "- [ ] Tag the release\n- [ ] Added by another app\n")


def slow():
    return max(float(os.environ.get("UI_SMOKE_SLOW", "1") or 1), 1.0)


def cmd_expect(fixture, out, which):
    text = open(fixture, encoding="utf-8").read()
    edits = {"mine": [MINE], "theirs": [THEIRS], "mine-theirs": [THEIRS, MINE]}[which]
    for old, new in edits:
        if text.count(old) != 1:
            die(f"{fixture}: {old!r} is not there once")
        text = text.replace(old, new)
    open(out, "w", encoding="utf-8", newline="").write(text)
    print(f"ok {os.path.basename(out)}: the {which} bytes")


def cmd_atomic(src, dest):
    data = open(src, "rb").read()
    temp = os.path.join(os.path.dirname(dest), "." + os.path.basename(dest) + ".other-app.tmp")
    with open(temp, "wb") as f:
        f.write(data)
        f.flush()
        os.fsync(f.fileno())
    os.replace(temp, dest)
    print(f"ok {os.path.basename(dest)} replaced by rename ({len(data)} bytes)")


def cmd_seed_recovery(text_path, vault, state_home, rel):
    text = open(text_path, encoding="utf-8").read()
    d = recovery_dir(state_home)
    os.makedirs(d, exist_ok=True)
    stem = f"{vault_id(vault)}~{sanitize(rel)}"
    open(os.path.join(d, stem + ".txt"), "w", encoding="utf-8", newline="").write(text)
    meta = {"rel": rel, "vault": vault_id(vault), "ts": int(time.time())}
    open(os.path.join(d, stem + ".json"), "w", encoding="utf-8").write(json.dumps(meta, separators=(",", ":")) + "\n")
    print(f"ok seeded a recovery copy for {rel} ({len(text.encode())} bytes)")


def cmd_recovery(state_home, rel, want):
    if want == "gone":
        if copy_paths(state_home, rel):
            die(f"the recovery copy for {rel} is still there: {copy_paths(state_home, rel)}")
        print(f"ok no recovery copy for {rel}")
        return
    expected = open(want, "rb").read()
    deadline = time.monotonic() + 6 * slow()
    got = None
    while time.monotonic() < deadline:
        found = [p for p, m in copy_paths(state_home, rel) if os.path.isfile(p) and os.path.isfile(m)]
        if len(found) == 1:
            got = open(found[0], "rb").read()
            if got == expected:
                print(f"ok the recovery copy for {rel} is {os.path.basename(want)} byte for byte")
                return
        time.sleep(0.2)
    die(f"the recovery copy for {rel} is {got!r}, not {os.path.basename(want)}")


def cmd_log(state_home, needle):
    path = os.path.join(state_home, "muninn", "session.log")
    deadline = time.monotonic() + 6 * slow()
    while time.monotonic() < deadline:
        if os.path.isfile(path):
            lines = [l for l in open(path, encoding="utf-8") if needle in l]
            if lines:
                print(f"ok session.log: {lines[-1].strip()}")
                return
        time.sleep(0.2)
    die(f"session.log has no line with {needle!r}")


def main(argv):
    commands = {"expect": cmd_expect, "atomic": cmd_atomic, "seed-recovery": cmd_seed_recovery, "recovery": cmd_recovery, "log": cmd_log}
    if len(argv) < 2 or argv[1] not in commands:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    commands[argv[1]](*argv[2:])


if __name__ == "__main__":
    main(sys.argv)
