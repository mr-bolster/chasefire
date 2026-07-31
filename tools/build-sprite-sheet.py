#!/usr/bin/env python3
"""Stitch the delivered animation strips into the one sheet the app carries.

The artist hands over one PNG per animation, which is the sensible way to draw
them. The app wants a single strip, which is the sensible way to load them. This
is the ten lines in between, kept in the repo so that redoing it after a new
delivery is one command rather than an afternoon of remembering.

    python3 tools/build-sprite-sheet.py <folder-of-strips> [output.png]

It checks as it goes: every strip has to be the same cell size, a whole number
of frames wide, and have the frame count the app expects. A silent mistake here
turns into Pablo playing the wrong animation on a stage, so it would rather
refuse than guess.
"""

import sys
from pathlib import Path

from PIL import Image

# Order matters: it is the order the frames end up in, and the Rust side has
# the matching ranges. Change one and you must change the other.
ANIMATIONS = [
    ("asleep", 8),
    ("pyjamas", 6),
    ("playing", 8),
    ("wobbling", 6),
    ("shivering", 6),
    ("flourish-midi", 4),
    ("flourish-osc", 4),
    ("flourish-network", 4),
]

ASSETS = Path(__file__).resolve().parent.parent / "crates" / "pablo" / "assets"


def main() -> int:
    if len(sys.argv) not in (2, 3):
        print(__doc__)
        return 2

    source = Path(sys.argv[1])
    output = ASSETS / (sys.argv[2] if len(sys.argv) == 3 else "pablo.png")
    strips = []
    cell = None

    for name, expected_frames in ANIMATIONS:
        path = source / f"{name}.png"
        if not path.exists():
            print(f"missing: {path}")
            return 1

        image = Image.open(path).convert("RGBA")
        width, height = image.size

        if cell is None:
            cell = height
        elif height != cell:
            print(f"{name}: cells are {height}px but {cell}px elsewhere")
            return 1

        if width % height:
            print(f"{name}: {width}x{height} is not a whole number of squares")
            return 1

        frames = width // height
        if frames != expected_frames:
            print(f"{name}: {frames} frames, expected {expected_frames}")
            return 1

        strips.append((name, image, frames))

    total = sum(frames for _, _, frames in strips)
    sheet = Image.new("RGBA", (cell * total, cell), (0, 0, 0, 0))

    offset = 0
    print(f"{'animation':20} {'frames':>6}  range")
    for name, image, frames in strips:
        sheet.paste(image, (offset * cell, 0))
        print(f"{name:20} {frames:>6}  {offset}..{offset + frames}")
        offset += frames

    output.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(output, optimize=True)
    print(f"\n{output}: {total} frames of {cell}x{cell}, {output.stat().st_size} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
