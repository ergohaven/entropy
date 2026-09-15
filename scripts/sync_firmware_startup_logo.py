#!/usr/bin/env python3
"""Extract the exact LVGL startup logo from the M4CR0Pad firmware."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

from PIL import Image


WIDTH = 240
HEIGHT = 72
PALETTE_BYTES = 16 * 4


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()

    source = args.source.read_text(encoding="utf-8")
    match = re.search(
        r"eh_logo_map\[\]\s*=\s*\{(?P<data>.*?)\};",
        source,
        flags=re.DOTALL,
    )
    if match is None:
        raise SystemExit("eh_logo_map was not found")
    values = bytes(int(value, 16) for value in re.findall(r"0x([0-9a-fA-F]{2})", match.group("data")))
    expected = PALETTE_BYTES + WIDTH * HEIGHT // 2
    if len(values) != expected:
        raise SystemExit(f"expected {expected} bytes, got {len(values)}")

    palette = []
    for offset in range(0, PALETTE_BYTES, 4):
        blue, green, red, alpha = values[offset : offset + 4]
        palette.append((red, green, blue, alpha))

    pixels = []
    for packed in values[PALETTE_BYTES:]:
        pixels.append(palette[packed >> 4])
        pixels.append(palette[packed & 0x0F])
    image = Image.new("RGBA", (WIDTH, HEIGHT))
    image.putdata(pixels)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    image.save(args.output, optimize=True)


if __name__ == "__main__":
    main()
