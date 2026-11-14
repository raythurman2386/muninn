#!/usr/bin/env python3
"""Turn capture.py masters into site assets, GIFs, a manifest and a contact sheet.

    python3 encode.py --masters OUT/masters --out ASSETS [--version 0.8.2]

Needs ffmpeg (libx264, libvpx-vp9) and Pillow with WebP and AVIF.
Budgets: each loop <= 1.5 MB at 1600 px and <= 600 KB at 960 px, hero <= 3 MB,
GIF <= 3 MB. The CRF steps up until a file fits; a miss is reported, not hidden.
"""
import argparse
import glob
import json
import os
import subprocess
import sys

from PIL import Image

KB = 1024
LOOP_BUDGET = {1600: 1536 * KB, 960: 600 * KB}
HERO_BUDGET = 3 * 1024 * KB
GIF_BUDGET = 3 * 1024 * KB
TARGET_LOOP_S = 9.5      # loops are sped up (never slowed) to fit 6-10 s
TARGET_HERO_S = 13.0
FPS = 30
STILL_WIDTHS = (1600, 960)

# Drop-in replacements for the files the current site uses (site/public/assets).
DROPIN_IMG = {"preview": "preview-1600", "editor": "notes-wide-1600", "graph": "graph-wide-1600",
              "graph-local": "local-wide-1600"}


def run(cmd):
    subprocess.run(cmd, check=True)


def probe(path):
    out = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
                          "stream=width,height,codec_name:format=duration", "-of", "json", path],
                         check=True, capture_output=True, text=True).stdout
    j = json.loads(out)
    st = j["streams"][0]
    return st["width"], st["height"], float(j["format"]["duration"]), st["codec_name"]


def save_still(src, dst_base, width):
    im = Image.open(src).convert("RGB")
    h = round(im.height * width / im.width)
    im = im.resize((width, h), Image.LANCZOS)
    im.save(dst_base + ".png", optimize=True)
    im.save(dst_base + ".webp", quality=86, method=6)
    im.save(dst_base + ".avif", quality=70, speed=4)
    return [dst_base + ext for ext in (".png", ".webp", ".avif")]


def vf(width, speed, start, end):
    return (f"trim=start={start}:end={end},setpts=(PTS-STARTPTS)/{speed:.4f},fps={FPS},"
            f"scale={width}:-2:flags=lanczos:out_color_matrix=bt709,format=yuv420p")


COLOR = ["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv"]


def encode_video(src, dst, codec, width, speed, start, end, budget, crf0):
    crf = crf0
    while True:
        if codec == "h264":
            enc = ["-c:v", "libx264", "-preset", "slow", "-crf", str(crf), "-profile:v", "high",
                   "-movflags", "+faststart"]
        else:
            enc = ["-c:v", "libvpx-vp9", "-crf", str(crf), "-b:v", "0", "-row-mt", "1", "-deadline", "good",
                   "-cpu-used", "2", "-tile-columns", "2"]
        run(["ffmpeg", "-v", "error", "-y", "-i", src, "-vf", vf(width, speed, start, end), "-an",
             *enc, *COLOR, dst])
        size = os.path.getsize(dst)
        if size <= budget or crf >= crf0 + 14:
            return size <= budget, crf
        crf += 2


def encode_gif(src, dst, speed, start, end, width=800, fps=12):
    pal = dst + ".palette.png"
    base = f"trim=start={start}:end={end},setpts=(PTS-STARTPTS)/{speed:.4f},fps={fps},scale={width}:-2:flags=lanczos"
    run(["ffmpeg", "-v", "error", "-y", "-i", src, "-vf", base + ",palettegen=max_colors=128:stats_mode=diff", pal])
    run(["ffmpeg", "-v", "error", "-y", "-i", src, "-i", pal, "-lavfi",
         base + "[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle", "-loop", "0", dst])
    os.remove(pal)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--masters", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--version", default="0.8.2")
    ap.add_argument("--gif-theme", default="dark")
    a = ap.parse_args()
    out = os.path.abspath(a.out)
    misses, files = [], []

    def add(path, **meta):
        files.append((path, meta))

    for tdir in sorted(glob.glob(os.path.join(a.masters, "*"))):
        theme = os.path.basename(tdir)
        sd = os.path.join(out, "stills", theme)
        ld = os.path.join(out, "loops", theme)
        os.makedirs(sd, exist_ok=True); os.makedirs(ld, exist_ok=True)
        for png in sorted(glob.glob(os.path.join(tdir, "*.png"))):
            name = os.path.splitext(os.path.basename(png))[0]
            for w in STILL_WIDTHS:
                for f in save_still(png, os.path.join(sd, f"{name}-{w}"), w):
                    add(f, kind="still", theme=theme, scene=name)
            if name in DROPIN_IMG:
                dd = os.path.join(out, "dropin", "site", "public", "assets", "img", theme)
                os.makedirs(dd, exist_ok=True)
                im = Image.open(png).convert("RGB")
                im = im.resize((1600, round(im.height * 1600 / im.width)), Image.LANCZOS)
                f = os.path.join(dd, DROPIN_IMG[name] + ".webp")
                im.save(f, quality=86, method=6)
                add(f, kind="dropin-still", theme=theme, scene=name)
        for mkv in sorted(glob.glob(os.path.join(tdir, "*.mkv"))):
            name = os.path.splitext(os.path.basename(mkv))[0]
            side = json.load(open(mkv[:-4] + ".json"))
            start, end = side["start"], side["end"]
            raw = end - start
            hero = name == "hero"
            speed = max(1.0, raw / (TARGET_HERO_S if hero else TARGET_LOOP_S))
            meta = dict(theme=theme, scene=name, desc=side["desc"], speed=round(speed, 3))
            if hero:
                vd = os.path.join(out, "dropin", "site", "public", "assets", "video")
                os.makedirs(vd, exist_ok=True)
                for codec, ext, crf in (("h264", "mp4", 20), ("vp9", "webm", 31)):
                    f = os.path.join(vd, f"{theme}-hero-tour.{ext}")
                    ok, used = encode_video(mkv, f, codec, 1600, speed, start, end, HERO_BUDGET, crf)
                    add(f, kind="hero", crf=used, budget=HERO_BUDGET, **meta)
                    if not ok:
                        misses.append(f)
                continue
            for w in (1600, 960):
                for codec, ext, crf in (("h264", "mp4", 20), ("vp9", "webm", 31)):
                    f = os.path.join(ld, f"{name}-{w}.{ext}")
                    ok, used = encode_video(mkv, f, codec, w, speed, start, end, LOOP_BUDGET[w], crf)
                    add(f, kind="loop", crf=used, budget=LOOP_BUDGET[w], **meta)
                    if not ok:
                        misses.append(f)
                # Poster: the loop's first frame, so it matches the video before it plays.
                p = os.path.join(ld, f"{name}-{w}-poster")
                run(["ffmpeg", "-v", "error", "-y", "-ss", str(start), "-i", mkv, "-frames:v", "1", p + "-src.png"])
                for f in save_still(p + "-src.png", p, w):
                    add(f, kind="poster", theme=theme, scene=name)
                os.remove(p + "-src.png")
            if theme == a.gif_theme:
                gd = os.path.join(out, "gif")
                os.makedirs(gd, exist_ok=True)
                f = os.path.join(gd, f"{name}-{theme}-800.gif")
                encode_gif(mkv, f, speed, start, end)
                add(f, kind="gif", budget=GIF_BUDGET, **meta)
                if os.path.getsize(f) > GIF_BUDGET:
                    misses.append(f)

    manifest = {"version": a.version, "generated_by": "site/scripts/capture/encode.py", "files": []}
    for path, meta in files:
        entry = {"path": os.path.relpath(path, out), "bytes": os.path.getsize(path)}
        if path.endswith((".mp4", ".webm", ".gif")):
            w, h, dur, codec = probe(path)
            entry.update(width=w, height=h, duration_s=round(dur, 2), codec=codec)
        else:
            with Image.open(path) as im:
                entry.update(width=im.width, height=im.height)
        entry.update({k: v for k, v in meta.items() if k != "budget"})
        if "budget" in meta:
            entry["budget_bytes"] = meta["budget"]
            entry["within_budget"] = entry["bytes"] <= meta["budget"]
        entry["version"] = a.version
        manifest["files"].append(entry)
    manifest["budget_misses"] = [os.path.relpath(m, out) for m in misses]
    json.dump(manifest, open(os.path.join(out, "manifest.json"), "w"), indent=1)
    print(json.dumps({"files": len(files), "misses": manifest["budget_misses"]}))


if __name__ == "__main__":
    main()
