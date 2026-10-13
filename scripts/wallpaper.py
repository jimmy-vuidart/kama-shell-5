#!/usr/bin/env python3
"""Draws the "glass" wallpaper: frosted panes rising over a pastel sky.

    scripts/wallpaper.py                    # the sizes of the three screens
    scripts/wallpaper.py 2560x1440 -o DIR   # another size, somewhere else

Everything is computed per pixel from the distance to each pane, in floating
point, and dithered at the end: no banding, and any size is as sharp as any
other. The scene is laid out in units of the image height around its centre,
so a 16:9 image shows the middle of what a 21:9 one shows.

Needs numpy and Pillow.
"""

import argparse
import math
from pathlib import Path

import numpy as np
from PIL import Image

DEFAULT_SIZES = ["3440x1440", "1920x1080"]


def rgb(hex_color):
    h = hex_color.lstrip("#")
    return np.array([int(h[i : i + 2], 16) / 255 for i in (0, 2, 4)], dtype=np.float32)


# The sky, left to right (position in the width, colour).
SKY = [
    (0.00, "#aebcf0"),
    (0.30, "#c3c6f3"),
    (0.55, "#dccff2"),
    (0.80, "#f3d2de"),
    (1.00, "#f8ddd0"),
]

# The panes, back to front. Each is a large rounded square seen by one corner,
# the rest of it running out of the image:
#   x, y    where that corner points (x from the centre, both in image heights)
#   round   radius of the corner, in image heights
#   turn    degrees; 45 stands the corner upright, less leans it to the left
#   tint    colour of the glass at its tip, then lower down
PANES = [
    dict(x=-1.40, y=0.34, round=0.22, turn=36, tint=("#bcc6f2", "#cdcbf3")),
    dict(x=1.36, y=0.17, round=0.25, turn=35, tint=("#f6d6d6", "#f3d8e2")),
    dict(x=-0.09, y=0.08, round=0.27, turn=34, tint=("#a3bcee", "#bcc3f0")),
    dict(x=-0.42, y=0.20, round=0.23, turn=37, tint=("#c6c8f3", "#c8c4f0")),
    dict(x=0.33, y=0.27, round=0.23, turn=34, tint=("#efd0e2", "#e6cdea")),
    dict(x=-1.02, y=0.50, round=0.20, turn=39, tint=("#d9d0f3", "#dccff1")),
    dict(x=-0.60, y=0.41, round=0.21, turn=40, tint=("#f7e4d4", "#efd8dd")),
    dict(x=0.66, y=0.39, round=0.21, turn=38, tint=("#cdc4f0", "#dccff2")),
    dict(x=1.06, y=0.52, round=0.19, turn=37, tint=("#f4dccf", "#f1d6de")),
]

SIDE = 2.6  # side of a pane, in image heights: its other corners are never seen
SHADOW = rgb("#5a62b4")
WHITE = np.ones(3, dtype=np.float32)


def smoothstep(lo, hi, v):
    t = np.clip((v - lo) / (hi - lo), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def mix(a, b, t):
    return a + (b - a) * t


def blur(image, sigma):
    """Gaussian blur of an (h, w, 3) image, through its Fourier transform."""
    pad = int(3 * sigma) + 1
    padded = np.pad(image, ((pad, pad), (pad, pad), (0, 0)), mode="reflect")
    h, w = padded.shape[:2]
    fy = np.fft.fftfreq(h).astype(np.float32)[:, None]
    fx = np.fft.rfftfreq(w).astype(np.float32)[None, :]
    kernel = np.exp(-2.0 * (math.pi * sigma) ** 2 * (fx * fx + fy * fy))
    spectrum = np.fft.rfft2(padded, axes=(0, 1)) * kernel[:, :, None]
    out = np.fft.irfft2(spectrum, s=(h, w), axes=(0, 1))
    return out[pad:-pad, pad:-pad].astype(np.float32)


def sky(width, height, px, py):
    stops = np.array([s for s, _ in SKY], dtype=np.float32)
    colors = np.stack([rgb(c) for _, c in SKY])
    u = px / width
    v = py / height
    # A slight slant, so the colours do not change along straight columns.
    t = np.clip(u + 0.10 * (v - 0.5), 0.0, 1.0)
    image = np.stack([np.interp(t, stops, colors[:, c]) for c in range(3)], axis=-1)
    image = image.astype(np.float32)

    # Lighter towards the ground, with a glow low in the middle.
    image = mix(image, WHITE, (0.30 * smoothstep(0.25, 1.0, v))[..., None])
    glow = np.exp(-(((u - 0.52) / 0.38) ** 2 + ((v - 1.02) / 0.42) ** 2))
    image = mix(image, WHITE, (0.35 * glow)[..., None])
    # A deeper blue in the top left corner.
    deep = np.exp(-(((u + 0.02) / 0.35) ** 2 + ((v + 0.05) / 0.60) ** 2))
    return mix(image, rgb("#a5b6ee"), (0.45 * deep)[..., None])


def draw_pane(image, pane, width, height, px, py):
    side = SIDE * height
    radius = pane["round"] * height
    turn = math.radians(pane["turn"])
    cos, sin = math.cos(turn), math.sin(turn)

    # The corner that points up is the top left one of the square, turned.
    tip_angle = math.radians(225) + turn
    tip = np.array([math.cos(tip_angle), math.sin(tip_angle)])
    reach = side / math.sqrt(2) - radius * (math.sqrt(2) - 1)
    cx = width / 2 + pane["x"] * height - tip[0] * reach
    cy = pane["y"] * height - tip[1] * reach

    # Pixels in the frame of the square.
    dx, dy = px - cx, py - cy
    lx = dx * cos + dy * sin
    ly = -dx * sin + dy * cos

    # Signed distance to the rounded square: negative inside.
    qx = np.abs(lx) - (side / 2 - radius)
    qy = np.abs(ly) - (side / 2 - radius)
    outside = np.hypot(np.maximum(qx, 0.0), np.maximum(qy, 0.0))
    dist = outside + np.minimum(np.maximum(qx, qy), 0.0) - radius

    # How far below the tip, in image heights.
    drop = (reach - (dx * tip[0] + dy * tip[1])) / height
    unit = height / 1440  # one pixel of the reference size

    # What the glass lets through: what is behind it, out of focus.
    behind = blur(image, 16 * unit)

    # The pane darkens a little what it stands in front of.
    shade = 0.13 * np.exp(-((np.maximum(dist, 0.0) / (52 * unit)) ** 2))
    shade *= 1.0 - smoothstep(0.5, 1.3, drop)
    image = mix(image, SHADOW, shade[..., None])

    tip_color, foot_color = (rgb(c) for c in pane["tint"])
    depth = 1.0 - smoothstep(0.0, 0.75, drop)
    tint = mix(foot_color, tip_color, depth[..., None])
    glass = mix(behind, tint, mix(0.34, 0.80, depth)[..., None])

    # Light caught by the glass: a wide wash from its steep side, a thinner
    # one under its long slope, and a bright rim right at the edge.
    from_left = lx + side / 2
    from_top = ly + side / 2
    lit = 1.0 - smoothstep(0.05, 1.0, drop)
    wash = 0.30 * np.exp(-from_left / (150 * unit))
    wash += 0.14 * np.exp(-from_top / (70 * unit))
    rim = 0.38 * np.exp(dist / (9 * unit))
    glass = mix(glass, WHITE, np.clip((wash + rim) * lit, 0.0, 1.0)[..., None])

    # The edge itself: a hairline, brightest at the tip.
    line_width = 1.6 * unit
    line = np.clip(1.0 - np.abs(dist + line_width / 2) / (line_width / 2 + 0.5), 0.0, 1.0)
    line *= mix(0.25, 0.95, 1.0 - smoothstep(0.0, 0.9, drop))
    glass = mix(glass, WHITE, line[..., None])

    cover = np.clip(0.5 - dist, 0.0, 1.0)
    return mix(image, glass, cover[..., None])


def render(width, height, seed=7):
    py, px = np.mgrid[0:height, 0:width].astype(np.float32)
    px += 0.5
    py += 0.5

    image = sky(width, height, px, py)
    for pane in PANES:
        image = draw_pane(image, pane, width, height, px, py)

    # Triangular dither: the gradients are too gentle for eight bits.
    rng = np.random.default_rng(seed)
    noise = rng.random(image.shape, dtype=np.float32) - rng.random(image.shape, dtype=np.float32)
    pixels = np.clip(image * 255.0 + noise, 0.0, 255.0).round().astype(np.uint8)
    return Image.fromarray(pixels, "RGB")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("sizes", nargs="*", default=DEFAULT_SIZES, metavar="WIDTHxHEIGHT")
    parser.add_argument(
        "-o",
        "--out",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "assets" / "wallpapers",
        help="directory to write into (default: assets/wallpapers)",
    )
    args = parser.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    for size in args.sizes:
        width, height = (int(n) for n in size.lower().split("x"))
        path = args.out / f"glass-{width}x{height}.png"
        render(width, height).save(path, optimize=True)
        print(path)


if __name__ == "__main__":
    main()
