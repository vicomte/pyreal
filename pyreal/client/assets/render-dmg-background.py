#!/usr/bin/env python3
"""Render dmg-background.svg into dmg-background.tiff, the disk image window's picture.

    python3 render-dmg-background.py                 the committed picture, beside this file
    python3 render-dmg-background.py --svg S --out T  another drawing, to try it

The TIFF holds the drawing at 1x (640 by 400, 72 dpi) and 2x (144 dpi), so the Finder picks the
sharp one on a Retina screen. `cargo xtask package pyreal` puts it on the image. Needs Google
Chrome (to draw the SVG) and ImageMagick.
"""

import argparse
import os
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
CHROME = os.environ.get("CHROME", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")
# The window's content size in points, as `finder.rs` sets it.
WIDTH, HEIGHT = 640, 400


def draw(svg, scale, out):
    """`svg` drawn at `scale` into the PNG `out`."""
    subprocess.run(
        [CHROME, "--headless", "--disable-gpu", "--hide-scrollbars",
         f"--window-size={WIDTH},{HEIGHT}", f"--force-device-scale-factor={scale}",
         f"--screenshot={out}", svg.as_uri()],
        check=True, capture_output=True,
    )


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--svg", type=Path, default=HERE / "dmg-background.svg")
    ap.add_argument("--out", type=Path, default=HERE / "dmg-background.tiff")
    args = ap.parse_args()
    with tempfile.TemporaryDirectory() as work:
        one, two = Path(work) / "1x.png", Path(work) / "2x.png"
        draw(args.svg.resolve(), 1, one)
        draw(args.svg.resolve(), 2, two)
        # Each page carries its own dpi, so both are the same size in points.
        subprocess.run(["magick", "-units", "PixelsPerInch",
                        "(", str(one), "-set", "density", "72", ")",
                        "(", str(two), "-set", "density", "144", ")",
                        "-compress", "zip", str(args.out)],
                       check=True, capture_output=True)
    print(args.out)


if __name__ == "__main__":
    main()
