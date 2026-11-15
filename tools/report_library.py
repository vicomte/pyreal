#!/usr/bin/env python3
"""Draw the report library's places with the client, on the engine, and write a page to look at them.

    python3 tools/report_library.py                 every place
    python3 tools/report_library.py door clip       the places whose names hold these words

The places are in pyreal/client/tests/gpu/report-library.txt. Each is captured by the client itself
(`pyreal --headless --no-ui --report <line> --capture`), with no window, at SIZE (1280x720 by
default). The page is written to target/report-library/index.html.

PYREAL_TEST_DAT_DIR names the dats (data/ by default), PYREAL_TEST_PRETOD_DAT_DIR the February 2005
pair a `pre-tod` place draws from, PACK the art pack, BEFORE a folder of an earlier run, drawn
beside the current one, and VIEWER=1 also draws each place with the engine's viewer
(`engine-view`), which places the library's objects (doors, portals) the client, offline, has no
server to make. Each place draws with the options its report line names.
"""

import html
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIBRARY = ROOT / "pyreal/client/tests/gpu/report-library.txt"
OUT = ROOT / "target/report-library"
SIZE = os.environ.get("SIZE", "1280x720")


def env_var(name, default):
    """PYREAL_<name>, else its old DERETH_<name>, else the default."""
    return os.environ.get(f"PYREAL_{name}", os.environ.get(f"DERETH_{name}", default))


def places():
    """Each place's name, note, objects, report line and whether it is drawn from the pre-ToD dats."""
    doors, found, heading = "", [], None
    for line in LIBRARY.read_text().splitlines():
        if line.startswith("@doors "):
            doors = line[len("@doors "):]
        elif line.startswith("dereth report") and heading:
            parts = [part.strip() for part in heading.split("|")]
            name, note, objects = parts[:3]
            pre_tod = len(parts) > 3 and parts[3] == "pre-tod"
            found.append((name, note, doors if objects == "doors" else objects, line, pre_tod))
            heading = None
        elif line and not line.startswith("#"):
            heading = line
    return found


def binaries():
    """The client and the viewer, built in release and copied so a later build cannot change them."""
    packages = ["-p", "pyreal-client"]
    if os.environ.get("VIEWER"):
        packages += ["-p", "pyreal-engine-view"]
    subprocess.run(["cargo", "build", "--release", *packages], cwd=ROOT, check=True)
    OUT.mkdir(parents=True, exist_ok=True)
    client = shutil.copy2(ROOT / "target/release/pyreal", OUT / "pyreal")
    viewer = None
    if os.environ.get("VIEWER"):
        viewer = shutil.copy2(ROOT / "target/release/engine-view", OUT / "engine-view")
    return client, viewer


def dats():
    return env_var("TEST_DAT_DIR", str(ROOT / "data"))


def older(pre_tod):
    """The client's arguments for a place drawn from the February 2005 dats."""
    if not pre_tod:
        return []
    return ["--classic-dat-dir", env_var("TEST_PRETOD_DAT_DIR", str(ROOT / "data/classic")),
            "--era", "infiltration"]


def pack():
    return os.environ.get("PACK", str(ROOT / "data/art-packs/enhanced-environment.dat"))


def draw(client, viewer, name, objects, report, pre_tod):
    """Capture one place; True when the client drew it."""
    folder = OUT / name
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "report.txt").write_text(report + "\n")
    with tempfile.TemporaryDirectory() as settings:
        env = dict(os.environ, PYREAL_ART_PACK=pack(), PYREAL_SETTINGS_DIR=settings)
        result = subprocess.run(
            [client, "--headless", "--no-ui", "--frames", "24", "--size", SIZE,
             "--report", str(folder / "report.txt"), "--capture", str(folder / "client.png"),
             "--dat-dir", dats(), *older(pre_tod)],
            cwd=ROOT, env=env, capture_output=True, text=True)
    (folder / "client.log").write_text(result.stdout + result.stderr)
    if result.returncode != 0:
        print(f"{name}: the client did not draw\n{result.stderr[-600:]}")
        return False
    if viewer and not pre_tod:
        args = [viewer, "--dat-dir", dats(), "--art-pack", pack(), "--report", report,
                "--size", SIZE, "--frames", "24", "--out", str(folder / "viewer.png")]
        if objects:
            args += ["--objects", objects]
        subprocess.run(args, cwd=ROOT, capture_output=True, text=True, check=False)
    for picture in ("client", "viewer"):
        if (folder / f"{picture}.png").exists():
            subprocess.run(["sips", "-Z", "1100", "-s", "format", "jpeg", f"{picture}.png",
                            "--out", f"{picture}.jpg"], cwd=folder, capture_output=True, check=False)
    return True


def page(drawn):
    """The page comparing what was drawn."""
    before = os.environ.get("BEFORE")
    rows = []
    for name, note, *_ in drawn:
        cells = []
        if before:
            cells.append(f'<figure><img loading="lazy" src="{before}/{name}/client.jpg"><figcaption>Before</figcaption></figure>')
        cells.append(f'<figure><img loading="lazy" src="{name}/client.jpg"><figcaption>The client</figcaption></figure>')
        if (OUT / name / "viewer.jpg").exists():
            cells.append(f'<figure><img loading="lazy" src="{name}/viewer.jpg"><figcaption>The viewer, with the objects</figcaption></figure>')
        rows.append(f"<section><h2>{html.escape(name)}</h2><p>{html.escape(note)}</p>"
                    f"<div>{''.join(cells)}</div></section>")
    (OUT / "index.html").write_text(
        "<!doctype html><meta charset=utf-8><title>Report library</title><style>"
        "body{font:15px system-ui;margin:24px;background:#16171a;color:#e8e8e8}"
        "section div{display:flex;gap:8px;flex-wrap:wrap}figure{margin:0;flex:1;min-width:300px}"
        "img{width:100%}figcaption{opacity:.7;padding:4px 0}</style>"
        "<h1>Report library</h1>" + "".join(rows))


def main():
    words = sys.argv[1:]
    chosen = [p for p in places() if not words or any(w in p[0] for w in words)]
    client, viewer = binaries()
    drawn = []
    for place in chosen:
        name, _, objects, report, pre_tod = place
        print(f"drawing {name}", flush=True)
        if draw(client, viewer, name, objects, report, pre_tod):
            drawn.append(place)
    page(drawn)
    print(f"{len(drawn)} of {len(chosen)} drawn: {OUT / 'index.html'}")


if __name__ == "__main__":
    main()
