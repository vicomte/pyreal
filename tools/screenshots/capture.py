#!/usr/bin/env python3
"""Draws the screenshot library in docs/screenshots from its manifest, shots.json.

    tools/screenshots/capture.py                 every shot, then the pictures' docs
    tools/screenshots/capture.py SLUG...         those shots, then the docs
    tools/screenshots/capture.py --docs          only the docs: the README's pictures and the table
    tools/screenshots/capture.py --draft DIR SLUG...
                                                 those shots into DIR at --size (1280x720 by
                                                 default), shots.json and the docs left alone
    tools/screenshots/capture.py measure [SLUG...]
                                                 prints where each shot's auto exposure settles,
                                                 a starting point for its fixed exposure
    tools/screenshots/capture.py add SLUG "<report line>"
                                                 prints a manifest entry for a report line copied
                                                 in the client, its clock moved to a clear day

Each shot is drawn twice from the same camera at the same time of day, at 3840 by 2160 with no
interface: <slug>-retail.jpg is engine-view on the data files alone with every Pyreal addition
off, and <slug>-pyreal.jpg is engine-view over the base pack as near the Ultra preset as it goes,
which loads the land out to Horizon before it draws. A shot whose pyreal.renderer is "client" uses
the headless client on Ultra instead (it cannot place creatures).
<slug>-compare.jpg puts both side by side at half size, retail on the left.

Needs the viewer built in the test-release profile (cargo build --profile test-release -p
pyreal-engine-view; -p pyreal-client too for a shot drawn by the client), the base pack (cargo
xtask base-pack build), ImageMagick, and the data folders: PYREAL_TEST_DAT_DIR (End of Retail), PYREAL_TEST_PRETOD_DAT_DIR
(February 2005) and PYREAL_TEST_LAUNCH_DAT_DIR (the 1999 launch) for shots in those worlds.
CARGO_TARGET_DIR and BASE_PACK are read when set.
"""

import argparse
import datetime
import json
import math
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / "docs" / "screenshots"
MANIFEST = DOCS / "shots.json"
README = ROOT / "README.md"
SHOTS_README = DOCS / "README.md"
SIZE = (3840, 2160)
# engine-view's frame step, seconds.
FRAME_STEP = 1.0 / 60.0

HOURS = ["Darktide", "Darktide-and-Half", "Foredawn", "Foredawn-and-Half", "Dawnsong",
         "Dawnsong-and-Half", "Morntide", "Morntide-and-Half", "Midsong", "Midsong-and-Half",
         "Warmtide", "Warmtide-and-Half", "Evensong", "Evensong-and-Half", "Gloaming",
         "Gloaming-and-Half"]

# The retail look: no pack, the client's own light, and every Pyreal addition off.
RETAIL = ["--retail", "--no-surfaces", "--no-shadows", "--no-ao", "--no-bounce", "--no-cover",
          "--no-lamp-rays", "--no-water", "--clouds", "0", "--no-volumes", "--no-webs",
          "--no-face-detail", "--no-weather", "--lod-bias", "0"]
# engine-view keeps every model at its finest level (--lod-bias 0): a still would catch the screen
# door two levels crossfade through half done.
# The Ultra preset: the client's settings, and engine-view as near them as it goes.
ULTRA = ["Render.LandscapeDrawDistance=127", "Fidelity.CloudDetail=2", "Fidelity.WaterDetail=2",
         "Fidelity.GroundCoverDrawDistance=500", "Fidelity.LightDrawDistance=500",
         "Fidelity.ShadowDistance=500", "Fidelity.ObjectDetail=300", "Fidelity.RenderScale=100"]
PYREAL_VIEW = ["--relief", "2", "--god-rays", "1", "--clouds", "3", "--water-detail", "2",
               "--smoothing", "2", "--cover-reach", "5", "--light-reach", "5",
               "--shadow-distance", "500", "--lod-bias", "0", "--far", "127"]
WEATHER = {"clear": [], "rain": ["--rain"], "storm": ["--storm"], "snow": ["--snow"],
           "blizzard": ["--blizzard"]}
# Each world's data folder, and the End of Retail files an older world opens beside its own.
WORLDS = {"eor": ("PYREAL_TEST_DAT_DIR", False), "2005": ("PYREAL_TEST_PRETOD_DAT_DIR", True),
          "launch": ("PYREAL_TEST_LAUNCH_DAT_DIR", True)}
POSE_KEYS = ("stance", "motion", "time", "lift", "translucency")


def fail(message):
    sys.exit(f"capture.py: {message}")


def load():
    return json.loads(MANIFEST.read_text())


def save(manifest):
    MANIFEST.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")


def tools():
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    viewer = target / "test-release" / "engine-view"
    client = target / "test-release" / "pyreal"
    pack = Path(os.environ.get("BASE_PACK", target / "base-pack" / "base-pack.dat"))
    # The client only for a shot that asks for it (`draw` checks).
    for f in (viewer, pack):
        if not f.exists():
            fail(f"{f} is missing (see the header)")
    if not shutil.which("magick"):
        fail("ImageMagick's magick is not on PATH")
    return viewer, client, pack


def dats(world):
    var, later = WORLDS[world]
    folder = os.environ.get(var)
    if not folder:
        fail(f"{var} must name the {world} data folder")
    flags = ["--dat-dir", folder]
    if later:
        eor = os.environ.get("PYREAL_TEST_DAT_DIR")
        if not eor:
            fail("PYREAL_TEST_DAT_DIR must name the End of Retail data folder")
        flags += ["--later-dat-dir", eor]
    return flags


def hexed(v):
    return int(v, 16) if isinstance(v, str) else int(v)


def outdoor_cell(block, x, y):
    return (hexed(block) << 16) | (int(x // 24) * 8 + int(y // 24) + 1)


def report_line(shot):
    """The shot as a client report line: the place, the camera, the time and the weather."""
    c = shot["camera"]
    p = shot.get("player")
    if p:
        q = p["rotation"]
        loc = (f"loc {p['cell']} [{p['x']:.6f} {p['y']:.6f} {p['z']:.6f}] "
               f"{q[0]:.6f} {q[1]:.6f} {q[2]:.6f} {q[3]:.6f}")
    else:
        cell = c["cell"] if hexed(c["cell"]) & 0xFFFF >= 0x100 else \
            f"0x{outdoor_cell(c['block'], c['x'], c['y']):08X}"
        loc = f"loc {cell} [{c['x']:.6f} {c['y']:.6f} {c['z'] - 1.8:.6f}] 1 0 0 0"
    t = shot["time"]
    return "; ".join([
        "dereth report 1", loc,
        f"camera {c['block']} [{c['x']} {c['y']} {c['z']}] yaw {c['yaw']} pitch {c['pitch']} "
        f"cell {c['cell']}",
        f"fov {c.get('fov', 90)} vertical 58.343605 aspect 1", "mode free",
        f"time {t:.3f} {HOURS[int(t * 16) % 16]}, set here", f"clock {shot['clock']:.6f}",
        f"weather {shot.get('weather', 'clear')}",
    ])


def heading_of(dx, dy):
    """A compass heading, degrees, facing along (dx, dy) east and north."""
    return math.degrees(math.atan2(dx, dy)) % 360.0


def place(cam, b):
    """Where a body stands, block-local, from its place ahead of and right of the camera."""
    if "at" in b:
        return float(b["at"][0]), float(b["at"][1])
    h = -float(cam["yaw"])
    a, r = float(b.get("ahead", 8)), float(b.get("right", 0))
    return (float(cam["x"]) + a * math.sin(h) + r * math.cos(h),
            float(cam["y"]) + a * math.cos(h) - r * math.sin(h))


def scene_flags(shot, frames):
    """engine-view's flags for the shot's bodies, scripts and anything else it stages."""
    scene = shot.get("scene", {})
    cam = shot["camera"]
    ends = {}
    for b in scene.get("bodies", []):
        ends[b.get("name", "")] = place(cam, b)
    flags = []
    clip = (frames - 1) * FRAME_STEP
    for b in scene.get("bodies", []):
        x, y = ends[b.get("name", "")]
        facing = b.get("facing", "camera")
        if facing == "camera":
            facing = (math.degrees(-float(cam["yaw"])) + 180.0) % 360.0
        elif isinstance(facing, dict):
            tx, ty = ends[facing["toward"]]
            facing = heading_of(tx - x, ty - y)
        facing = float(facing)
        stride = b.get("stride")
        if "run" in b:
            h = math.radians(facing)
            stride = [b["run"] * math.sin(h), b["run"] * math.cos(h)]
        if stride:
            # It ends where it is put, on the last frame.
            x, y = x - stride[0] * clip, y - stride[1] * clip
        at = f"@{x:.2f},{y:.2f}" + (f",{b['z']}" if "z" in b else "")
        tail = [f"facing={facing:.1f}"]
        tail += [f"{k}={b[k]}" for k in POSE_KEYS if k in b]
        if stride:
            tail.append(f"stride={stride[0]:.3f},{stride[1]:.3f}")
        tail += [f"dress={d}" for d in b.get("dress", [])]
        tail += [f"wear={d}" for d in b.get("wear", [])]
        tail += [f"script={s}" for s in b.get("scripts", [])]
        if "character" in b:
            what = ["--character", b["character"] + at + ";" + ";".join(tail)]
        else:
            what = b["setup"]
            if "motion_table" in b:
                what += "/" + b["motion_table"]
            if "scale" in b:
                what += f"*{b['scale']}"
            what = ["--object", what + at + ";" + ";".join(tail)]
        flags += what
    # Objects a server places (a vent, a portal), as engine-view stands them: setup, block, x, y.
    placed = [f"{o['setup']},{o['block']},{o['x']},{o['y']}" for o in scene.get("objects", [])]
    if placed:
        flags += ["--objects", ";".join(placed)]
    for s in scene.get("scripts", []):
        x, y = place(cam, s)
        flags += ["--script", f"{s['script']}@{x:.2f},{y:.2f}" + (f",{s['z']}" if "z" in s else "")]
    return flags + scene.get("flags", [])


def run(cmd, log):
    with open(log, "w") as out:
        if subprocess.run(cmd, stdout=out, stderr=subprocess.STDOUT).returncode != 0:
            tail = Path(log).read_text().splitlines()[-8:]
            fail("a capture failed:\n  " + " ".join(cmd) + "\n" + "\n".join(tail))


def exposure(shot):
    """The Pyreal side's fixed exposure: no eye adaptation, so every capture is drawn alike."""
    if "exposure" not in shot:
        fail(f"{shot['slug']}: give the shot an exposure (engine-view prints its settled target)")
    return shot["exposure"]


def draw(shot, work, size, side):
    """Draws one side of a shot to work/<side>.png."""
    viewer, client, pack = tools()
    report = report_line(shot)
    scene = shot.get("scene", {})
    frames = int(scene.get("frames", 30))
    staged = scene_flags(shot, frames)
    weather = WEATHER[shot.get("weather", "clear")]
    world = dats(shot.get("world", "eor"))
    common = world + ["--report", report, "--size", f"{size[0]}x{size[1]}", "--frames",
                      str(frames), "--time", str(shot["time"])]
    out = work / f"{side}.png"
    if side == "retail":
        # Outdoors no lamp light: the viewer has no copy of the original's lamp light on the land.
        outdoors = hexed(shot["camera"]["cell"]) & 0xFFFF < 0x100
        cmd = [str(viewer)] + common + RETAIL + (["--no-lamps"] if outdoors else []) + weather \
            + staged + shot.get("retail", {}).get("flags", []) + ["--out", str(out)]
    elif staged or shot.get("pyreal", {}).get("renderer", "engine-view") == "engine-view":
        cmd = [str(viewer)] + common + ["--base-pack", str(pack)] + PYREAL_VIEW + weather \
            + ["--exposure", str(exposure(shot))] + staged \
            + shot.get("pyreal", {}).get("flags", []) + ["--out", str(out)]
    else:
        if shot.get("world", "eor") != "eor":
            fail(f"{shot['slug']}: the client side draws only End of Retail shots")
        if not client.exists():
            fail(f"{client} is missing (see the header)")
        (work / "report.txt").write_text(report + "\n")
        env = dict(os.environ, PYREAL_BASE_PACK=str(pack), PYREAL_SETTINGS_DIR=str(work / "settings"),
                   PYREAL_EXPOSURE=str(exposure(shot)))
        cmd = [str(client), "--headless", "--no-ui", "--no-character", "--frames", "60",
               "--size", f"{size[0]}x{size[1]}", "--report", str(work / "report.txt"),
               "--dat-dir", world[1],
               "--capture", str(out)] + shot.get("pyreal", {}).get("flags", [])
        # Ultra in the settings file the client reads at start.
        sections = {}
        for setting in ULTRA:
            key, value = setting.split("=")
            section, name = key.split(".")
            sections.setdefault(section, []).append(f"{name}={value}")
        (work / "settings").mkdir(exist_ok=True)
        (work / "settings" / "UserPreferences.ini").write_text(
            "".join(f"[{k}]\r\n" + "".join(f"{v}\r\n" for v in vs) for k, vs in sections.items()))
        with open(work / f"{side}.log", "w") as log:
            if subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, env=env).returncode:
                fail("a capture failed:\n  " + " ".join(cmd) + "\n"
                     + "\n".join((work / f"{side}.log").read_text().splitlines()[-8:]))
        return out
    run(cmd, work / f"{side}.log")
    return out


def encode(work, folder, slug):
    """The two sides as JPEGs (quality 90, lower only to stay under 3 MB) and side by side."""
    for side in ("retail", "pyreal"):
        target = folder / f"{slug}-{side}.jpg"
        for q in (90, 87, 84, 80):
            subprocess.run(["magick", str(work / f"{side}.png"), "-strip", "-sampling-factor",
                            "4:2:0", "-quality", str(q), str(target)], check=True)
            if target.stat().st_size < 3_000_000:
                break
    font = "/System/Library/Fonts/Supplemental/Arial.ttf"
    halves = []
    for side, label in (("retail", "Retail"), ("pyreal", "Pyreal")):
        half = work / f"{side}-half.png"
        subprocess.run(["magick", str(work / f"{side}.png"), "-resize", "1920x1080", "-font", font,
                        "-pointsize", "40", "-fill", "white", "-stroke", "black", "-strokewidth",
                        "2", "-annotate", "+30+60", label, str(half)], check=True)
        halves.append(str(half))
    subprocess.run(["magick"] + halves + ["+append", "-strip", "-sampling-factor", "4:2:0",
                                           "-quality", "88", str(folder / f"{slug}-compare.jpg")],
                   check=True)


def git(*args):
    return subprocess.run(["git", "-C", str(ROOT)] + list(args), capture_output=True,
                          text=True).stdout.strip()


def pack_version():
    m = re.search(r'^version = "([^"]+)"', (ROOT / "base-pack.toml").read_text(), re.M)
    return m.group(1) if m else "unknown"


def capture(manifest, slugs, draft, size):
    shots = {s["slug"]: s for s in manifest["shots"]}
    for slug in slugs:
        if slug not in shots:
            fail(f"no shot named {slug} in {MANIFEST.relative_to(ROOT)}")
    folder = Path(draft) if draft else DOCS
    folder.mkdir(parents=True, exist_ok=True)
    for slug in slugs:
        shot = shots[slug]
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            for side in ("retail", "pyreal"):
                print(f"{slug}: drawing the {side} side", flush=True)
                draw(shot, work, size, side)
            encode(work, folder, slug)
            if draft:
                for side in ("retail", "pyreal"):
                    shutil.copy(work / f"{side}.log", folder / f"{slug}-{side}.log")
        if not draft:
            # The manifest as it is now, so edits made while this shot drew are kept.
            latest = load()
            captured = {
                "date": datetime.date.today().isoformat(),
                "commit": git("rev-parse", "--short=8", "HEAD"),
                "base_pack": pack_version(),
            }
            for s in latest["shots"]:
                if s["slug"] == slug:
                    s["captured"] = captured
            save(latest)
            manifest.clear()
            manifest.update(latest)
        print(f"{slug}: wrote {folder}/{slug}-{{retail,pyreal,compare}}.jpg", flush=True)


def ordered(manifest):
    return sorted(manifest["shots"], key=lambda s: s["order"])


def readme_picture(shot):
    side = shot["readme"]
    return f"![{shot['alt']}](docs/screenshots/{shot['slug']}-{side}.jpg)"


def write_docs(manifest):
    """The README's pictures and the table in docs/screenshots/README.md, from the manifest."""
    shots = {s["slug"]: s for s in manifest["shots"]}
    lines = README.read_text().split("\n")
    seen = set()
    for i, line in enumerate(lines):
        m = re.fullmatch(r"<!-- shot: ([a-z0-9-]+) -->", line.strip())
        if not m:
            continue
        slug = m.group(1)
        shot = shots.get(slug)
        if not shot or not shot.get("readme"):
            fail(f"README.md shows {slug}, which shots.json does not put in the README")
        if i + 1 >= len(lines) or not lines[i + 1].startswith("!["):
            fail(f"README.md: the picture line after {slug}'s marker is missing")
        lines[i + 1] = readme_picture(shot)
        seen.add(slug)
    missing = [s for s in shots.values() if s.get("readme") and s["slug"] not in seen]
    if missing:
        fail("README.md has no marker for " + ", ".join(s["slug"] for s in missing))
    README.write_text("\n".join(lines))

    rows = ["| Shot | Retail | Pyreal | Side by side |", "|---|---|---|---|"]
    for s in ordered(manifest):
        slug = s["slug"]
        rows.append(f"| {s['caption']} (`{slug}`) | [retail]({slug}-retail.jpg) | "
                    f"[Pyreal]({slug}-pyreal.jpg) | [both]({slug}-compare.jpg) |")
    text = SHOTS_README.read_text()
    begin, end = "<!-- shots: begin -->", "<!-- shots: end -->"
    if begin not in text or end not in text:
        fail(f"{SHOTS_README.relative_to(ROOT)} has no {begin} ... {end} block")
    head, rest = text.split(begin, 1)
    _, tail = rest.split(end, 1)
    SHOTS_README.write_text(head + begin + "\n" + "\n".join(rows) + "\n" + end + tail)
    print("wrote README.md's pictures and docs/screenshots/README.md's table")


def f32(x):
    return struct.unpack("f", struct.pack("f", x))[0]


def day_group(clock, zero=3600.0, zero_year=10, day=7620.0, days=360, groups=20):
    """End of Retail's calendar: the region's game time and calc_present_day_group."""
    y = clock + zero
    years = math.floor(y / (day * days))
    d = math.floor((y - years * day * days) / day)
    v = (((years + zero_year) * days + d) * 0x6A42FDB2 - 0x7541E9AE) & 0xFFFFFFFF
    i = math.floor(f32(f32(f32(float(v)) * f32(2.3283064e-10)) * f32(float(groups))))
    return i if 0 <= i < groups else 0


def clear_day(clock, want=10):
    """The clock moved by whole days, its time of day kept, to the nearest day of group `want`
    ("Clear", next to no cloud)."""
    for k in sorted(range(-400, 401), key=abs):
        if day_group(clock + k * 7620.0) == want:
            return clock + k * 7620.0
    return clock


def add(slug, line):
    """A manifest entry for a report line, to fill in and paste into shots.json."""
    fields = {}
    for p in (x.strip() for x in line.strip().split(";")):
        key = p.split(" ", 1)[0]
        fields[key] = p[len(key) + 1:] if " " in p else ""
    cam = re.match(r"(0x[0-9A-Fa-f]{4}) \[(\S+) (\S+) (\S+)\] yaw (\S+) pitch (\S+) cell (0x[0-9A-Fa-f]{8})",
                   fields.get("camera", ""))
    if not cam:
        fail("the report line names no camera")
    loc = re.match(r"(0x[0-9A-Fa-f]{8}) \[(\S+) (\S+) (\S+)\] (\S+) (\S+) (\S+) (\S+)", fields.get("loc", ""))
    weather = fields.get("weather", "clear").split(",")[0].split()[0]
    weather = weather if weather in WEATHER else "clear"
    clock = float(fields.get("clock", "0"))
    entry = {
        "slug": slug, "name": "", "caption": "", "alt": "", "order": 0, "site": False,
        "site_top": False, "readme": None, "world": "eor",
        "camera": {"block": cam.group(1), "cell": cam.group(7), "x": float(cam.group(2)),
                   "y": float(cam.group(3)), "z": float(cam.group(4)), "yaw": float(cam.group(5)),
                   "pitch": float(cam.group(6)), "fov": float(fields.get("fov", "90").split()[0])},
        "time": round(float(fields.get("time", "0.5").split()[0]), 3),
        "clock": round(clear_day(clock) if weather == "clear" else clock, 6),
        "weather": weather,
    }
    if loc:
        entry["player"] = {"cell": loc.group(1), "x": float(loc.group(2)), "y": float(loc.group(3)),
                           "z": float(loc.group(4)),
                           "rotation": [float(loc.group(k)) for k in range(5, 9)]}
    print(json.dumps(entry, indent=2))


def measure(slugs):
    """Each shot's settled auto exposure on the Pyreal side, as a starting point for its exposure."""
    viewer, _, pack = tools()
    shots = {s["slug"]: s for s in load()["shots"]}
    scratch = tempfile.mkdtemp()
    for slug in slugs or list(shots):
        shot = shots[slug]
        cmd = [str(viewer)] + dats(shot.get("world", "eor")) + [
            "--report", report_line(shot), "--size", "640x360", "--frames", "240",
            "--time", str(shot["time"]), "--base-pack", str(pack)] + PYREAL_VIEW \
            + WEATHER[shot.get("weather", "clear")] + scene_flags(shot, 240) + ["--out", os.path.join(scratch, "measure.png")]
        out = subprocess.run(cmd, capture_output=True, text=True).stdout
        m = re.search(r"exposure ([0-9.]+) \(target ([0-9.]+)\)", out)
        print(f"{slug}: settles at {m.group(2) if m else '?'} (now {shot.get('exposure', 'unset')})")


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "measure":
        measure(sys.argv[2:])
        return
    if len(sys.argv) > 1 and sys.argv[1] == "add":
        if len(sys.argv) != 4:
            fail('add takes SLUG "<report line>"')
        add(sys.argv[2], sys.argv[3])
        return
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("slugs", nargs="*", help="the shots to draw (every one when none given)")
    ap.add_argument("--docs", action="store_true", help="only rewrite the docs")
    ap.add_argument("--draft", metavar="DIR", help="draw into DIR, leaving the library alone")
    ap.add_argument("--size", default=None, help="WxH (a draft's default 1280x720)")
    a = ap.parse_args()
    manifest = load()
    if not a.docs:
        size = SIZE
        if a.size or a.draft:
            w, h = (a.size or "1280x720").split("x")
            size = (int(w), int(h))
            if not a.draft and size != SIZE:
                fail("the library is drawn at 3840x2160; use --draft for another size")
        slugs = a.slugs or [s["slug"] for s in ordered(manifest)]
        capture(manifest, slugs, a.draft, size)
    if not a.draft:
        write_docs(manifest)


if __name__ == "__main__":
    main()
