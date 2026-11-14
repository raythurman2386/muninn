"""Drive a Muninn release binary on a private Xvfb for marketing captures.

Everything runs in a throwaway environment: own Xvfb display, own HOME and
XDG_* dirs, own D-Bus session (dbus-run-session), always --new-instance.
Only processes started here are ever signalled. Needs python-xlib.

Coordinates passed to the helpers are LOGICAL app pixels (the app runs at
SCALE x, so a 1600x1000 logical window is 3200x2000 device pixels).
"""
import math
import os
import shutil
import signal
import subprocess
import sys
import time

from Xlib import X, XK, display as xdisplay
from Xlib.ext import xtest


def log(*a):
    print("[capture]", *a, file=sys.stderr, flush=True)


class Session:
    def __init__(self, cfg, theme, logical=(1600, 1000), scale=2):
        self.cfg, self.theme, self.scale = cfg, theme, scale
        self.lw, self.lh = logical
        self.procs = []
        self.rec = None

    # ---- lifecycle -------------------------------------------------------
    def start(self, vault):
        cfg = self.cfg
        disp = cfg["display"]
        n = disp.lstrip(":")
        if os.path.exists(f"/tmp/.X11-unix/X{n}") or os.path.exists(f"/tmp/.X{n}-lock"):
            raise SystemExit(f"display {disp} is in use; pick another CAPTURE_DISPLAY")
        sw, sh = self.lw * self.scale, self.lh * self.scale
        xvfb = subprocess.Popen(["Xvfb", disp, "-screen", "0", f"{sw}x{sh}x24", "-nolisten", "tcp", "-noreset"],
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        self.procs.append(xvfb)
        for _ in range(50):
            if os.path.exists(f"/tmp/.X11-unix/X{n}"):
                break
            time.sleep(0.1)
        env = self.env()
        assert env["DISPLAY"] == disp and "WAYLAND_DISPLAY" not in env
        cmd = list(cfg.get("loader", [])) + [cfg["bin"], "--new-instance", "--vault", vault, "--theme", self.theme]
        logf = open(os.path.join(cfg["root"], "logs", f"app-{self.theme}.log"), "a")
        app = subprocess.Popen(["dbus-run-session", "--"] + cmd, env=env, stdout=logf, stderr=logf,
                               start_new_session=True)
        self.procs.append(app)
        self.d = xdisplay.Display(disp)
        self.win = self.wait_window("Muninn", 30)
        self.resize(self.lw, self.lh)
        time.sleep(2.5)
        return self

    def env(self):
        cfg, r = self.cfg, os.path.join(self.cfg["root"], "run")
        env = {k: v for k, v in os.environ.items()
               if k not in ("WAYLAND_DISPLAY", "MUNINN_VAULT", "MUNINN_GOOGLE_CLIENT_SECRET",
                            "DBUS_SESSION_BUS_ADDRESS", "XDG_SESSION_TYPE", "SESSION_MANAGER")}
        dirs = dict(HOME="home", XDG_CONFIG_HOME="config", XDG_DATA_HOME="data", XDG_STATE_HOME="state",
                    XDG_CACHE_HOME="cache", XDG_RUNTIME_DIR="xdg-runtime", TMPDIR="tmp")
        for k, sub in dirs.items():
            p = os.path.join(r, sub)
            os.makedirs(p, exist_ok=True)
            env[k] = p
        os.chmod(env["XDG_RUNTIME_DIR"], 0o700)
        env.update(DISPLAY=cfg["display"], MUNINN_OMARCHY_COLORS="", WINIT_X11_SCALE_FACTOR=str(self.scale),
                   XCURSOR_THEME="Adwaita", XCURSOR_SIZE=str(24 * self.scale))
        if cfg.get("vk_icd"):
            env["VK_DRIVER_FILES"] = env["VK_ICD_FILENAMES"] = cfg["vk_icd"]
        return env

    def stop(self):
        if self.rec:
            self.stop_record()
        for p in reversed(self.procs):
            if p.poll() is None:
                try:
                    os.killpg(p.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    p.wait(5)
                except subprocess.TimeoutExpired:
                    os.killpg(p.pid, signal.SIGKILL)
                    p.wait()
        self.procs = []

    # ---- X11 -------------------------------------------------------------
    def walk(self, w):
        yield w
        try:
            for c in w.query_tree().children:
                yield from self.walk(c)
        except Exception:
            return

    def wait_window(self, title, timeout):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            for w in self.walk(self.d.screen().root):
                try:
                    if w.get_wm_name() == title and w.get_attributes().map_state == X.IsViewable:
                        return w
                except Exception:
                    pass
            time.sleep(0.25)
        raise SystemExit(f"no window {title!r}")

    def resize(self, lw, lh):
        self.lw, self.lh = lw, lh
        self.win.configure(x=0, y=0, width=lw * self.scale, height=lh * self.scale)
        self.win.set_input_focus(X.RevertToParent, X.CurrentTime)
        self.d.sync()

    def _dev(self, x, y):
        return int(round(x * self.scale)), int(round(y * self.scale))

    pos = (0, 0)

    def move(self, x, y, dur=0.0, fps=60):
        """Ease the pointer to logical x,y over dur seconds."""
        x0, y0 = self.pos
        steps = max(1, int(dur * fps))
        for i in range(1, steps + 1):
            t = i / steps
            e = 0.5 - 0.5 * math.cos(math.pi * t)
            px, py = self._dev(x0 + (x - x0) * e, y0 + (y - y0) * e)
            xtest.fake_input(self.d, X.MotionNotify, x=px, y=py)
            self.d.sync()
            if steps > 1:
                time.sleep(dur / steps)
        self.pos = (x, y)

    def click(self, x, y, dur=0.35, button=1, count=1, hold=0.08):
        self.win.set_input_focus(X.RevertToParent, X.CurrentTime)
        self.move(x, y, dur)
        time.sleep(0.08)
        for i in range(count):
            xtest.fake_input(self.d, X.ButtonPress, button); self.d.sync(); time.sleep(hold if count == 1 else 0.04)
            xtest.fake_input(self.d, X.ButtonRelease, button); self.d.sync()
            if i + 1 < count:
                time.sleep(0.06)
        time.sleep(0.25)

    def drag(self, x, y, x2, y2, dur=0.8, path=None, pre=0.25, post=0.25):
        """Button-1 drag from x,y to x2,y2 (or along path, a list of points)."""
        self.move(x, y, 0.35)
        time.sleep(0.1)
        xtest.fake_input(self.d, X.ButtonPress, 1); self.d.sync()
        time.sleep(pre)
        pts = path or [(x2, y2)]
        seg = dur / len(pts)
        for px, py in pts:
            self.move(px, py, seg)
        time.sleep(post)
        xtest.fake_input(self.d, X.ButtonRelease, 1); self.d.sync()
        time.sleep(0.25)

    def stroke(self, pts, dt=1 / 60):
        """Freehand: press at pts[0], move through each point, release."""
        self.move(*pts[0], 0.3)
        xtest.fake_input(self.d, X.ButtonPress, 1); self.d.sync()
        for p in pts[1:]:
            px, py = self._dev(*p)
            xtest.fake_input(self.d, X.MotionNotify, x=px, y=py); self.d.sync()
            time.sleep(dt)
        self.pos = pts[-1]
        xtest.fake_input(self.d, X.ButtonRelease, 1); self.d.sync()
        time.sleep(0.2)

    def wheel(self, x, y, button, n, gap=0.08):
        self.move(x, y, 0.2)
        for _ in range(n):
            xtest.fake_input(self.d, X.ButtonPress, button); xtest.fake_input(self.d, X.ButtonRelease, button)
            self.d.sync(); time.sleep(gap)

    def key(self, chord, after=0.3):
        names = {"ctrl": "Control_L", "shift": "Shift_L", "alt": "Alt_L", "esc": "Escape", "enter": "Return",
                 "tab": "Tab", "space": "space", "down": "Down", "up": "Up", "left": "Left", "right": "Right",
                 "backspace": "BackSpace", "\\": "backslash", "delete": "Delete"}
        codes = [self.d.keysym_to_keycode(XK.string_to_keysym(names.get(k, k))) for k in chord.split("+")]
        if 0 in codes:
            raise SystemExit(f"unknown key in {chord}")
        for c in codes:
            xtest.fake_input(self.d, X.KeyPress, c); self.d.sync(); time.sleep(0.03)
        for c in reversed(codes):
            xtest.fake_input(self.d, X.KeyRelease, c); self.d.sync(); time.sleep(0.03)
        time.sleep(after)

    def type(self, text, cps=14):
        shift = self.d.keysym_to_keycode(XK.string_to_keysym("Shift_L"))
        special = {" ": "space", "-": "minus", ".": "period", ",": "comma", "'": "apostrophe", "[": "bracketleft",
                   "]": "bracketright", "#": "numbersign", "!": "exclam", "?": "question", ":": "colon",
                   "/": "slash", "(": "parenleft", ")": "parenright", "\n": "Return", "+": "plus", "&": "ampersand"}
        for ch in text:
            sym = XK.string_to_keysym(special.get(ch, ch))
            code = self.d.keysym_to_keycode(sym)
            if not code:
                raise SystemExit(f"cannot type {ch!r}")
            need_shift = self.d.keycode_to_keysym(code, 0) != sym
            if need_shift:
                xtest.fake_input(self.d, X.KeyPress, shift)
            xtest.fake_input(self.d, X.KeyPress, code); xtest.fake_input(self.d, X.KeyRelease, code)
            if need_shift:
                xtest.fake_input(self.d, X.KeyRelease, shift)
            self.d.sync(); time.sleep(1 / cps)
        time.sleep(0.2)

    def park(self):
        """Move the pointer off the window content (bottom-right corner)."""
        self.move(self.lw - 2, self.lh - 2, 0)

    # ---- capture ---------------------------------------------------------
    def grab(self, path):
        w, h = self.lw * self.scale, self.lh * self.scale
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "x11grab", "-draw_mouse", "0", "-video_size", f"{w}x{h}",
                        "-i", self.cfg["display"], "-frames:v", "1", path], check=True)
        log("still", path)

    def record(self, path, fps=30, mouse=True):
        w, h = self.lw * self.scale, self.lh * self.scale
        self.rec = subprocess.Popen(
            ["ffmpeg", "-v", "error", "-y", "-f", "x11grab", "-draw_mouse", "1" if mouse else "0",
             "-framerate", str(fps), "-video_size", f"{w}x{h}", "-i", self.cfg["display"],
             "-c:v", "libx264rgb", "-crf", "0", "-preset", "ultrafast", path],
            stdin=subprocess.PIPE, start_new_session=True)
        self.rec_path, self.rec_t0 = path, time.monotonic()
        time.sleep(0.6)

    def mark(self):
        return time.monotonic() - self.rec_t0

    def stop_record(self):
        self.rec.stdin.write(b"q"); self.rec.stdin.flush()
        self.rec.wait(30)
        log("recorded", self.rec_path)
        self.rec = None
