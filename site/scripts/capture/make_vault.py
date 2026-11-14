#!/usr/bin/env python3
"""Build the synthetic "Tidepool" demo vault used for Muninn's marketing captures.

    python3 make_vault.py OUT_DIR [--paper white|night] [--fills pastel|none]

The Launch Plan board's look:
  --paper white   Excalidraw's default #ffffff paper (default)
  --paper night   #121212 paper; the default #1e1e1e ink is then painted with
                  Muninn's legible substitute (#88)
  --fills pastel  Excalidraw's pastel card fills (default)
  --fills none    coloured strokes only. Muninn 0.8.1/0.8.2 lift every fill to
                  3:1 against the paper, which turns pastel fills muddy on
                  white paper and leaves the (lightened) ink grey on pastel
                  cards on night paper, so the 0.8.2 captures use none.

Fake company, fake people, no real data. `muninn --check OUT_DIR` must report
0 broken links, and the app footer must not show "1 broken"."""
import json, os, sys
out = sys.argv[1]
import argparse
ap = argparse.ArgumentParser()
ap.add_argument("out")
ap.add_argument("--paper", choices=("white", "night"), default="white")
ap.add_argument("--fills", choices=("pastel", "none"), default="pastel")
opt = ap.parse_args()
style = "outline" if opt.fills == "none" else "pastel"
os.makedirs(out + "/Meetings", exist_ok=True)
def w(p, s): open(os.path.join(out, p), "w").write(s.lstrip("\n"))
w("Home.md", """
# Tidepool HQ

Lanternfly Studio's working notes for **Tidepool**, our calm habit tracker.

- [[Product Roadmap]] - what ships and when
- [[Launch Checklist]] - everything between us and 1.0
- [[Beta Feedback]] - what testers are telling us
- [[Pricing]] - plans and the free tier
- [[Team]] - who owns what
- Specs: [[Widgets]], [[Sync Engine]], [[Onboarding v2]], [[Paywall]]

Latest meeting: [[2026-11-10 Weekly Sync]]
""")
w("Product Roadmap.md", """
# Product Roadmap

Tidepool 1.0 launches **December 2**. The plan lives on the Launch Plan board, right here with the notes.

## Now (November)
- Finish the private beta and triage [[Beta Feedback]]
- Lock [[Pricing]] for launch

## Next (December)
- App Store review, then launch day: see [[Launch Checklist]]
- Day-to-day tasks: [[Launch Week]]
""")
w("Launch Checklist.md", """
# Launch Checklist

Owner: Priya. Target: December 2. Roadmap: [[Product Roadmap]].

## Product
- [x] Onboarding flow v2
- [x] Crash-free sessions above 99.5%
- [ ] Final copy pass on paywall
- [ ] Offline sync edge cases from [[Beta Feedback]]

## Store
- [ ] Screenshots for 6.7" and 5.5"
- [ ] Privacy nutrition labels
- [ ] Submit for review by Nov 24

## Marketing
- [ ] Landing page with [[Pricing]]
- [ ] Press kit and launch post
- [ ] Email the 1,200-person waitlist
""")
w("Beta Feedback.md", """
# Beta Feedback

142 testers, week 3. Summary for [[2026-11-10 Weekly Sync]].

- Streak reminders feel *just right* - keep the gentle tone
- Widgets are the most-loved feature
- Offline edits sometimes duplicate a check-in (Marco is on it)
- Three people asked for shared habits - parked for Q1 on the [[Product Roadmap]]
""")
w("Pricing.md", """
# Pricing

- **Free**: 3 habits, widgets, reminders
- **Tidepool Plus**: $3.99/month or $29/year - unlimited habits, insights, themes
- Launch offer: 40% off the first year for the waitlist

Decided at [[2026-11-10 Weekly Sync]].
""")
w("Team.md", """
# Team

- **Priya** - product and launch ([[Launch Checklist]])
- **Marco** - iOS and sync
- **Jun** - design and the store screenshots
- **Ellie** - marketing and the waitlist
""")
w("Meetings/2026-11-10 Weekly Sync.md", """
# 2026-11-10 Weekly Sync

Attendees: Priya, Marco, Jun, Ellie

## Decisions
- Launch date stays **December 2** ([[Product Roadmap]])
- Annual plan at $29 ([[Pricing]])

## Action items
- [ ] Marco: fix duplicate offline check-ins ([[Beta Feedback]])
- [ ] Jun: store screenshots by Nov 20
- [ ] Ellie: draft the launch post
- [ ] Priya: walk the [[Launch Checklist]] on Friday
""")
w("Launch Week.md", """
---
kanban-plugin: board
---

## To do

- [ ] Store screenshots for 6.7" and 5.5" #store
- [ ] Email the 1,200-person waitlist #marketing
- [ ] Final copy pass on the paywall, see [[Pricing]]
- [ ] Privacy nutrition labels #store


## Doing (3)

- [ ] Write the launch post with Ellie #marketing
- [ ] Fix duplicate offline check-ins from [[Beta Feedback]]
  - [x] Reproduce on iOS 18
  - [ ] Ship the fix to beta


## Done

**Complete**
- [x] Onboarding flow v2
- [x] Crash-free sessions above 99.5%
""")
os.makedirs(out + "/Specs", exist_ok=True)
w("Specs/Widgets.md", """
# Widgets

The most-loved feature in [[Beta Feedback]]. Small, medium and lock-screen sizes.

- Streak ring uses the same colours as [[Streak Reminders]]
- Tap opens today's check-in; sync rules in [[Sync Engine]]
""")
w("Specs/Streak Reminders.md", """
# Streak Reminders

Gentle nudges, never guilt. Testers say the tone is *just right* ([[Beta Feedback]]).

- One reminder a day, at the time the habit is usually done
- Quiet on weekends unless the habit is daily
- Copy reviewed with the [[Paywall]] copy pass
""")
w("Specs/Sync Engine.md", """
# Sync Engine

Owner: Marco. Offline first; check-ins merge by habit and day.

- Known issue: duplicate check-ins after offline edits ([[Beta Feedback]])
- Must be fixed before [[App Store Review]]
- Widgets read the same store ([[Widgets]])
""")
w("Specs/Onboarding v2.md", """
# Onboarding v2

Shipped in beta week 2. Three screens, then the first habit.

- Offers the free tier first, then [[Pricing]]
- Ends on the [[Widgets]] tip
""")
w("Specs/Paywall.md", """
# Paywall

Final copy pass is on the [[Launch Checklist]]. Prices come from [[Pricing]].

- Show the launch offer to the waitlist only
- No dark patterns: the close button is always visible
""")
w("Specs/App Store Review.md", """
# App Store Review

Submit by **Nov 24** ([[Launch Checklist]]). Jun owns the [[Press Kit]] and screenshots.

- Privacy nutrition labels: no tracking, health data stays on device
- Review notes mention [[Sync Engine]] background refresh
""")
w("Specs/Press Kit.md", """
# Press Kit

Owner: Ellie. Logo, screenshots, a one-page fact sheet and the launch post.

- Launch date from the [[Product Roadmap]]
- Quotes from [[Beta Feedback]]
""")
# Board
els = []; idx = [0]
def nid():
    idx[0] += 1; return idx[0]
def key(i):
    return "a" + format(i, "03d")
def base(t, x, y, wd, ht, **kw):
    i = nid()
    e = {"id": f"{t}-{i}", "type": t, "x": x, "y": y, "width": wd, "height": ht, "angle": 0,
         "strokeColor": "#1e1e1e", "backgroundColor": "transparent", "fillStyle": "solid",
         "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1, "opacity": 100, "groupIds": [],
         "frameId": None, "index": key(i), "roundness": None, "seed": 1000 + i, "version": 2,
         "versionNonce": 2000 + i, "isDeleted": False, "boundElements": None,
         "updated": 1762900000000, "link": None, "locked": False}
    e.update(kw); els.append(e); return e
def text(x, y, s, size=20, container=None, color="#1e1e1e", align="left"):
    lines = s.split("\n")
    wd = max(len(l) for l in lines) * size * 0.5; ht = len(lines) * size * 1.25
    if container:
        x = container["x"] + (container["width"] - wd) / 2
        y = container["y"] + (container["height"] - ht) / 2
    t = base("text", x, y, wd, ht, strokeColor=color, text=s, originalText=s, fontSize=size,
             fontFamily=5, textAlign="center" if container else align,
             verticalAlign="middle" if container else "top",
             containerId=container["id"] if container else None, autoResize=True, lineHeight=1.25)
    if container:
        container["boundElements"] = [{"type": "text", "id": t["id"]}]
    return t
def card(x, y, label, stroke, fill, wd=220, ht=100):
    r = base("rectangle", x, y, wd, ht, strokeColor=stroke,
             backgroundColor="transparent" if style == "outline" else fill, roundness={"type": 3})
    text(0, 0, label, 20, container=r)
    return r
def arrow(x1, y1, x2, y2, color="#1e1e1e"):
    base("arrow", x1, y1, x2 - x1, y2 - y1, strokeColor=color, points=[[0, 0], [x2 - x1, y2 - y1]],
         roundness={"type": 2}, startBinding=None, endBinding=None, startArrowhead=None,
         endArrowhead="arrow", elbowed=False)
text(0, -110, "Tidepool 1.0 - launch plan", 36)
text(0, -60, "Each card has a note in the vault with the details.", 20, color="#868e96")
c1 = card(0, 0, "Private beta\n(142 testers)", "#1971c2", "#d0ebff")
c2 = card(320, 0, "Beta Feedback\ntriage", "#1971c2", "#d0ebff")
c3 = card(640, 0, "Lock Pricing", "#f08c00", "#fff3bf")
c7 = card(0, 200, "Press kit\n+ launch post", "#9c36b5", "#eebefa")
c5 = card(320, 200, "Submit for review\nby Nov 24", "#e03131", "#ffc9c9")
c6 = base("ellipse", 640, 190, 240, 120, strokeColor="#2f9e44",
         backgroundColor="transparent" if style == "outline" else "#b2f2bb")
text(0, 0, "Launch day\nDec 2", 22, container=c6)
arrow(225, 50, 315, 50); arrow(545, 50, 635, 50)
arrow(225, 250, 315, 250); arrow(545, 250, 635, 250)
arrow(750, 105, 755, 185, "#2f9e44")
text(0, 380, "Parking lot", 20, color="#868e96")
c4 = card(0, 420, "Store screenshots", "#f08c00", "#fff3bf")
board = {"type": "excalidraw", "version": 2, "source": "https://excalidraw.com",
         "elements": els, "appState": {"gridSize": 20, "viewBackgroundColor": "#121212" if opt.paper == "night" else "#ffffff"}, "files": {}}
json.dump(board, open(os.path.join(out, "Launch Plan.excalidraw"), "w"))
