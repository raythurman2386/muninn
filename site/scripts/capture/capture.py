#!/usr/bin/env python3
"""Record Muninn's marketing stills and loops from a release binary.

    python3 capture.py --bin PATH/muninn --out DIR [--loader LDSO --libdir DIR]
                       [--vk-icd lvp_icd.json] [--display :93]
                       [--run-dir DIR] [--themes dark,light] [--scenes stills,board,kanban,graph,hero]

Writes lossless masters to DIR/masters/<theme>/: stills as PNG (2x device
pixels), loops as lossless H.264 RGB .mkv with a .json sidecar holding the
trim window. encode.py turns them into the site files.

Each scene starts a fresh app on a fresh copy of the Tidepool vault, in a
throwaway HOME/XDG tree under DIR/run/, on a private Xvfb, with its own
D-Bus session and --new-instance. Nothing touches the host desktop.
"""
import argparse
import json
import math
import os
import random
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from xdrive import Session, log  # noqa: E402

LOGICAL = (1200, 750)   # stills and loops: 2400x1500 device, 1600x1000 out
HERO = (1200, 794)      # hero + its poster: 1600x1058 out, matches the site

# Logical points in the 1200x750 layout (Muninn 0.8.2).
BOARD_ROW = (75, 131)          # "Launch Plan" in the Board list
PALETTE_BTN = (601, 188)       # toolbar "Stroke and fill colours"
STROKE = {"red": (410, 228), "blue": (518, 228), "green": (491, 228), "violet": (545, 228)}
GRAPH_LOCAL = (351, 87)
GRAPH_GLOBAL = (301, 87)


def args():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--loader", help="newer ld-linux, when the host glibc is too old")
    ap.add_argument("--libdir", help="its --library-path")
    ap.add_argument("--vk-icd", help="Vulkan ICD json (Mesa lavapipe)")
    ap.add_argument("--run-dir", help="throwaway HOME/XDG trees and vaults (default OUT/run)")
    ap.add_argument("--display", default=os.environ.get("CAPTURE_DISPLAY", ":93"))
    ap.add_argument("--themes", default="dark,light")
    ap.add_argument("--scenes", default="stills,board,kanban,graph,hero")
    return ap.parse_args()


class Run:
    def __init__(self, a):
        self.a = a
        self.out = os.path.abspath(a.out)

    def session(self, name, theme, logical=LOGICAL):
        root = os.path.join(os.path.abspath(self.a.run_dir or os.path.join(self.out, "run")), f"{name}-{theme}")
        shutil.rmtree(root, ignore_errors=True)
        os.makedirs(os.path.join(root, "logs"))
        vault = os.path.join(root, "vault")
        paper = "night" if theme == "dark" else "white"
        subprocess.run([sys.executable, os.path.join(HERE, "make_vault.py"), vault,
                        "--paper", paper, "--fills", "none"], check=True)
        loader = [self.a.loader, "--library-path", self.a.libdir] if self.a.loader else []
        cfg = dict(display=self.a.display, root=root, bin=os.path.abspath(self.a.bin), loader=loader,
                   vk_icd=self.a.vk_icd)
        s = Session(cfg, theme, logical).start(vault)
        s.vault = vault
        return s

    def dir(self, theme):
        d = os.path.join(self.out, "masters", theme)
        os.makedirs(d, exist_ok=True)
        return d


def go(s, query, settle=0.6, cps=30):
    """Open a note through the sidebar search, then clear the search."""
    s.key("ctrl+k", 0.3)
    s.type(query, cps=cps)
    s.key("enter", settle)
    s.key("ctrl+k", 0.2); s.key("ctrl+a", 0.1); s.key("backspace", 0.1); s.key("esc", 0.4)


def still(s, d, name, park=True):
    if park:
        s.park()
    time.sleep(0.4)
    s.grab(os.path.join(d, f"{name}.png"))


def sidecar(s, d, name, start, end, desc):
    json.dump({"start": round(start, 2), "end": round(end, 2), "desc": desc},
              open(os.path.join(d, f"{name}.json"), "w"), indent=1)


# ---- scenes -----------------------------------------------------------------
def scene_stills(r, theme):
    d = r.dir(theme)
    s = r.session("stills", theme)
    try:
        go(s, "product roadmap"); s.key("ctrl+1", 1.2); still(s, d, "editor")
        go(s, "launch checklist"); s.key("ctrl+2", 1.2); still(s, d, "read")
        s.key("ctrl+k", 0.3); s.type("sync", cps=20); time.sleep(0.6); still(s, d, "search", park=False)
        s.key("ctrl+a", 0.1); s.key("backspace", 0.1); s.key("esc", 0.4)
        s.key("ctrl+shift+p", 0.8); s.type("graph", cps=20); time.sleep(0.5); still(s, d, "command-palette", park=False)
        s.key("esc", 0.5)
        go(s, "home"); s.key("ctrl+3", 3.0); still(s, d, "graph")
        s.click(*GRAPH_LOCAL); time.sleep(2.5); still(s, d, "graph-local")
        s.click(*GRAPH_GLOBAL); time.sleep(0.5)
        go(s, "launch week"); s.key("ctrl+5", 2.0); still(s, d, "kanban")
        s.key("ctrl+4", 1.5); s.click(*BOARD_ROW); time.sleep(2.5); still(s, d, "board")
        s.click(*PALETTE_BTN); time.sleep(0.8); s.move(1100, 700, 0); time.sleep(0.4)
        still(s, d, "board-palette", park=False)
        s.key("esc", 0.3)
    finally:
        s.stop()
    # Read mode at the hero's size: the hero poster (site preview-1600).
    s = r.session("poster", theme, HERO)
    try:
        go(s, "launch checklist"); s.key("ctrl+2", 1.5); still(s, d, "preview")
    finally:
        s.stop()


def scene_board(r, theme):
    d = r.dir(theme)
    s = r.session("board", theme)
    try:
        s.key("ctrl+4", 1.5); s.click(*BOARD_ROW); time.sleep(2.5)
        s.move(1060, 690, 0)
        s.record(os.path.join(d, "loop-board.mkv"))
        t0 = s.mark()
        s.click(*PALETTE_BTN, dur=0.45); time.sleep(0.35)
        s.click(*STROKE["blue"], dur=0.3); time.sleep(0.1)
        s.key("r", 0.1); s.drag(690, 580, 870, 655, dur=0.6, pre=0.1, post=0.1)
        s.key("t", 0.1); s.click(740, 610, dur=0.3); s.type("Retro", cps=12); s.key("esc", 0.2)
        s.click(*PALETTE_BTN, dur=0.45); time.sleep(0.3); s.click(*STROKE["red"], dur=0.3); time.sleep(0.1)
        s.key("p", 0.1)
        random.seed(4)
        cx, cy, rx, ry = 958, 475, 114, 66
        pts = [(cx + rx * math.cos(i / 48 * 2 * math.pi * 1.08 - 2.4) + random.uniform(-1.4, 1.4),
                cy + ry * math.sin(i / 48 * 2 * math.pi * 1.08 - 2.4) + random.uniform(-1.4, 1.4)) for i in range(49)]
        s.stroke(pts, dt=1 / 55)
        s.key("a", 0.1); s.drag(578, 630, 684, 620, dur=0.45, pre=0.05, post=0.05)
        s.move(1060, 690, 0.5); time.sleep(0.8)
        t1 = s.mark()
        s.stop_record()
        sidecar(s, d, "loop-board", t0 - 0.3, t1,
                "Board: pick a stroke colour from the palette, draw a card, label it, circle a node in ink, add an arrow")
        still(s, d, "board-drawn")
    finally:
        s.stop()


def scene_kanban(r, theme):
    d = r.dir(theme)
    s = r.session("kanban", theme)
    try:
        go(s, "launch week"); s.key("ctrl+5", 2.0)
        s.move(900, 600, 0)
        s.record(os.path.join(d, "loop-kanban.mkv"))
        t0 = s.mark()
        s.drag(640, 180, 1000, 320, dur=1.1, pre=0.3, post=0.3)
        time.sleep(0.5)
        s.drag(380, 400, 650, 300, dur=1.1, pre=0.3, post=0.3)
        time.sleep(0.5)
        s.click(316, 180, dur=0.6)          # tick "Store screenshots"
        time.sleep(0.4)
        s.drag(380, 180, 1000, 400, dur=1.1, pre=0.3, post=0.3)
        s.move(900, 640, 0.5); time.sleep(0.9)
        t1 = s.mark()
        s.stop_record()
        sidecar(s, d, "loop-kanban", t0 - 0.3, t1, "Kanban: drag cards between lanes; Done marks them complete")
    finally:
        s.stop()


def scene_graph(r, theme):
    d = r.dir(theme)
    s = r.session("graph", theme)
    try:
        go(s, "home"); s.key("ctrl+3", 3.0)
        s.move(1000, 640, 0)
        s.record(os.path.join(d, "loop-graph.mkv"))
        t0 = s.mark()
        s.drag(1000, 640, 900, 580, dur=1.4, pre=0.1, post=0.1)     # glide
        s.drag(900, 580, 1000, 640, dur=1.4, pre=0.1, post=0.1)
        s.click(784, 459, dur=0.6); time.sleep(1.0)                  # select Beta Feedback
        s.click(*GRAPH_LOCAL, dur=0.6); time.sleep(2.2)              # its local graph
        s.click(*GRAPH_GLOBAL, dur=0.5); time.sleep(1.4)
        t1 = s.mark()
        s.stop_record()
        sidecar(s, d, "loop-graph", t0 - 0.3, t1, "Graph: glide the global graph, select a note, open its local graph")
    finally:
        s.stop()


def scene_hero(r, theme):
    d = r.dir(theme)
    s = r.session("hero", theme, HERO)
    try:
        go(s, "launch checklist"); s.key("ctrl+2", 1.5)
        s.move(1000, 700, 0)
        s.record(os.path.join(d, "hero.mkv"))
        t0 = s.mark()
        time.sleep(0.8)
        s.click(110, 63, dur=0.5); s.type("sync engine", cps=12); time.sleep(0.3)
        s.key("enter", 0.9)
        s.key("ctrl+k", 0.1); s.key("ctrl+a", 0.05); s.key("backspace", 0.05); s.key("esc", 0.3)
        s.key("ctrl+3", 1.2)
        s.drag(1000, 650, 920, 600, dur=1.2, pre=0.1, post=0.1)
        s.key("ctrl+4", 0.3); s.click(*BOARD_ROW, dur=0.5); time.sleep(1.6)
        go(s, "launch week", settle=0.3, cps=40); s.key("ctrl+5", 0.8)
        s.drag(640, 180, 1000, 320, dur=1.0, pre=0.25, post=0.25)
        s.move(1000, 700, 0.5); time.sleep(0.8)
        t1 = s.mark()
        s.stop_record()
        sidecar(s, d, "hero", t0 - 0.2, t1, "Tour: Read, search, Graph, Board, Kanban")
    finally:
        s.stop()


SCENES = dict(stills=scene_stills, board=scene_board, kanban=scene_kanban, graph=scene_graph, hero=scene_hero)


def main():
    a = args()
    r = Run(a)
    for theme in a.themes.split(","):
        for name in a.scenes.split(","):
            log(f"scene {name} ({theme})")
            SCENES[name](r, theme)


if __name__ == "__main__":
    main()
