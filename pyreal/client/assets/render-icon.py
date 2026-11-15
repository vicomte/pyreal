#!/usr/bin/env python3
"""Render pyreal.svg into Pyreal's icon files.

    python3 render-icon.py                          the committed icons, beside this file
    python3 render-icon.py --emblem E --out DIR     a coin with E's shape struck into it, in DIR

Writes Pyreal.icon (its layer is the coin alone), Pyreal.icns, pyreal.ico and pyreal-256.png (the
whole tile). An emblem (a PNG or an SVG) is struck by its shape: its opaque parts are raised, its
colours are not used (a PNG with no transparency is read as dark on light: its dark parts are
raised). It is trimmed to its shape and fills the coin's face, so a round emblem's own ring stands
against the coin's rim, in place of the face's beads. `cargo xtask bundle-macos` and `package pyreal` take the icon from
$PYREAL_ICON_DIR when it is set; build.py sets it from build.ini's `icon_emblem`. Needs Google
Chrome (to draw the SVG), ImageMagick and iconutil.
"""

import argparse
import base64
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
CHROME = os.environ.get("CHROME", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")
# The coin is drawn larger on macOS's own tile than on the flat one.
LAYER_SCALE = 1.2
# A supplied emblem fills the coin's face (radius 250), and the field's clip opens to the step.
EMBLEM_BOX = (262, 262, 500, 500)
EMBLEM_CLIP = 258
# How much finer than drawn a PNG emblem's shape is made, smoothed, before it is drawn.
EMBLEM_UPSCALE = 4
MIME = {".png": "image/png", ".svg": "image/svg+xml"}


def group(svg, name):
    """The span of `<g id="name">` up to its first `</g>`."""
    start = svg.index(f'<g id="{name}">')
    return start, svg.index("</g>", start) + len("</g>")


def emblem_mask(emblem, work):
    """A PNG emblem as a smooth shape: trimmed, made finer and its edges smoothed, opaque where it
    is raised. A PNG with transparency keeps its own; one without is read dark on light."""
    opaque = subprocess.run(
        ["magick", str(emblem), "-format", "%[opaque]", "info:"],
        check=True, capture_output=True, text=True,
    ).stdout.strip().lower() == "true"
    shape = ["-colorspace", "gray", "-negate"] if opaque else ["-alpha", "extract"]
    out = work / "emblem.png"
    magick(
        emblem, *shape, "-trim", "+repage",
        "-filter", "Lanczos", "-resize", f"{EMBLEM_UPSCALE * 100}%",
        "-blur", f"0x{EMBLEM_UPSCALE / 2}", "-level", "35%,65%",
        "-alpha", "copy", "-channel", "RGB", "-evaluate", "set", "0", "+channel", out,
    )
    return out


def with_emblem(svg, emblem, work):
    mime = MIME.get(emblem.suffix.lower())
    if mime is None:
        raise SystemExit(f"render-icon: {emblem} is not a .png or an .svg")
    if mime == "image/png":
        emblem = emblem_mask(emblem, work)
    data = base64.b64encode(emblem.read_bytes()).decode()
    x, y, w, h = EMBLEM_BOX
    image = (
        f'<g id="emblem-art"><image href="data:{mime};base64,{data}" x="{x}" y="{y}" '
        f'width="{w}" height="{h}" preserveAspectRatio="xMidYMid meet"/></g>'
    )
    a, b = group(svg, "emblem-art")
    svg = svg[:a] + image + svg[b:]
    svg = svg.replace('<circle id="beads" ', '<circle id="beads" display="none" ', 1)
    return svg.replace('id="field-edge" cx="512" cy="512" r="232"', f'id="field-edge" cx="512" cy="512" r="{EMBLEM_CLIP}"', 1)


def layer(svg):
    a, b = group(svg, "background")
    svg = svg[:a] + svg[b:]
    lift = f"translate(512 512) scale({LAYER_SCALE}) translate(-512 -512)"
    return svg.replace('<g id="coin">', f'<g id="coin" transform="{lift}">', 1)


def draw(svg, out, work):
    source = work / (out.stem + ".svg")
    source.write_text(svg)
    subprocess.run(
        [CHROME, "--headless", "--disable-gpu", "--hide-scrollbars", "--window-size=1024,1024",
         "--default-background-color=00000000", f"--screenshot={out}", source.as_uri()],
        check=True, capture_output=True,
    )


def magick(*args):
    subprocess.run(["magick", *map(str, args)], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--emblem", type=Path, help="a PNG or SVG whose shape is struck into the coin")
    parser.add_argument("--out", type=Path, default=HERE, help="where the icon files go")
    args = parser.parse_args()

    svg = (HERE / "pyreal.svg").read_text()
    out = args.out.resolve()
    (out / "Pyreal.icon/Assets").mkdir(parents=True, exist_ok=True)
    if out != HERE:
        shutil.copy(HERE / "Pyreal.icon/icon.json", out / "Pyreal.icon/icon.json")
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        if args.emblem:
            svg = with_emblem(svg, args.emblem.expanduser().resolve(), work)
        full = work / "full.png"
        draw(svg, full, work)
        draw(layer(svg), out / "Pyreal.icon/Assets/pyreal.png", work)
        iconset = work / "Pyreal.iconset"
        iconset.mkdir()
        for s in (16, 32, 128, 256, 512):
            magick(full, "-filter", "Lanczos", "-resize", f"{s}x{s}", iconset / f"icon_{s}x{s}.png")
            magick(full, "-filter", "Lanczos", "-resize", f"{2 * s}x{2 * s}", iconset / f"icon_{s}x{s}@2x.png")
        subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(out / "Pyreal.icns")], check=True)
        magick(full, "-filter", "Lanczos", "-resize", "256x256", out / "pyreal-256.png")
        magick(full, "-define", "icon:auto-resize=256,128,64,48,32,24,16", out / "pyreal.ico")
    print(f"render-icon: {out}")


if __name__ == "__main__":
    main()
