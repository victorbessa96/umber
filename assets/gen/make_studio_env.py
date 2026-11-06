#!/usr/bin/env python3
"""Studio equirects for umber's IBL defaults (wave-4 item 6; K8 adds the
two variants).

Generates assets/env/*.png: 1024x512 LDR equirects with a soft overcast
gradient (bright zenith, mid horizon, dark ground) plus a key light and a
cool/warm fill, in three deterministic variants:

  studio-neutral.png   — balanced neutral key + cool fill (the original)
  studio-warm-key.png  — dominant warm key light, weak cool fill
  studio-cool-fill.png — dominant cool fill, weak warm key

No licensed content, fully procedural: every pixel is pure math.

PyPNG-style dependency-free: the PNG encoder below is ~25 lines of stdlib
(struct + zlib). Fully deterministic (pure math, no RNG): re-running
produces byte-identical output for every scene. Pin that contract with

  python3 assets/gen/make_studio_env.py --check

which re-renders every scene in memory and compares against the committed
PNG byte-for-byte WITHOUT writing; exit 0 when all match, 1 otherwise.

Layout convention (see umber-gpu/src/ibl.rs): the PNG is stored
viewer-conventional — displayed TOP row = +Y zenith. umber's PNG loader
flips rows into GPU order on read, so the top row here lands on the last
data row (v=1 = +Y). Do NOT "fix" the orientation: it is load-bearing.
"""

import math
import os
import struct
import sys
import zlib

WIDTH = 1024
HEIGHT = 512

# Base palette + light rig: the neutral scene. Kept as the module-level
# defaults so studio-neutral.png stays byte-identical to the original
# single-scene generator.
BASE = {
    # Gradient stops in sin(elevation): (sin_e, (r, g, b)) linear-light 0..1.
    "ZENITH": (0.82, 0.85, 0.92),  # soft top light, neutral-cool
    "HORIZON": (0.55, 0.55, 0.56),  # neutral mid
    "NADIR": (0.16, 0.14, 0.13),  # dark warm-gray ground
    "KEY_AZIMUTH": 0.30,  # fraction of full turn
    "KEY_ELEVATION": math.radians(35.0),
    "KEY_COLOR": (0.55, 0.38, 0.22),
    "KEY_STRENGTH": 0.55,
    "KEY_WIDTH": 0.45,  # radians (gaussian sigma)
    "FILL_AZIMUTH": 0.80,
    "FILL_ELEVATION": math.radians(10.0),
    "FILL_COLOR": (0.30, 0.38, 0.52),
    "FILL_STRENGTH": 0.30,
    "FILL_WIDTH": 0.70,
}

# Per-scene overrides on top of BASE. Warm key: hot low warm light, faint
# cool counter-fill, warm-tinted gradient. Cool fill: blue fill dominates,
# key reduced to a whisper, cool-tinted gradient.
SCENES = {
    "studio-neutral": {},
    "studio-warm-key": {
        "ZENITH": (0.78, 0.74, 0.66),
        "HORIZON": (0.55, 0.50, 0.44),
        "NADIR": (0.18, 0.15, 0.12),
        "KEY_AZIMUTH": 0.25,
        "KEY_ELEVATION": math.radians(40.0),
        "KEY_COLOR": (0.66, 0.42, 0.18),
        "KEY_STRENGTH": 0.80,
        "KEY_WIDTH": 0.38,
        "FILL_COLOR": (0.18, 0.26, 0.42),
        "FILL_STRENGTH": 0.15,
    },
    "studio-cool-fill": {
        "ZENITH": (0.80, 0.86, 0.95),
        "HORIZON": (0.52, 0.56, 0.62),
        "NADIR": (0.14, 0.15, 0.18),
        "KEY_COLOR": (0.30, 0.24, 0.18),
        "KEY_STRENGTH": 0.25,
        "FILL_AZIMUTH": 0.75,
        "FILL_ELEVATION": math.radians(15.0),
        "FILL_COLOR": (0.24, 0.38, 0.60),
        "FILL_STRENGTH": 0.65,
        "FILL_WIDTH": 0.60,
    },
}


def lerp(a, b, t):
    return tuple(x + (y - x) * t for x, y in zip(a, b))


def angular_distance(az1, el1, az2, el2):
    # Great-circle distance via the spherical law of cosines, clamped
    # against rounding at coincident points.
    c = (
        math.sin(el1) * math.sin(el2)
        + math.cos(el1) * math.cos(el2) * math.cos(az1 - az2)
    )
    return math.acos(max(-1.0, min(1.0, c)))


def pixel(u, v, s):
    """u, v in [0, 1): u = longitude fraction, v = 0 at displayed top.
    s = the scene's merged constant dict."""
    az = u * 2.0 * math.pi
    el = (0.5 - v) * math.pi  # displayed top = +pi/2 zenith
    se = math.sin(el)
    if se >= 0.0:
        base = lerp(s["HORIZON"], s["ZENITH"], se)
    else:
        base = lerp(s["HORIZON"], s["NADIR"], -se)
    key = s["KEY_STRENGTH"] * math.exp(
        -((angular_distance(az, el, s["KEY_AZIMUTH"] * 2 * math.pi, s["KEY_ELEVATION"])) / s["KEY_WIDTH"]) ** 2
    )
    fill = s["FILL_STRENGTH"] * math.exp(
        -((angular_distance(az, el, s["FILL_AZIMUTH"] * 2 * math.pi, s["FILL_ELEVATION"])) / s["FILL_WIDTH"]) ** 2
    )
    rgb = tuple(
        min(1.0, max(0.0, b + s["KEY_COLOR"][i] * key + s["FILL_COLOR"][i] * fill))
        for i, b in enumerate(base)
    )
    return rgb


def png_bytes(width, height, rows):
    def chunk(ctype, data):
        out = struct.pack(">I", len(data)) + ctype + data
        out += struct.pack(">I", zlib.crc32(ctype + data) & 0xFFFFFFFF)
        return out

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    raw = b"".join(b"\x00" + bytes(row) for row in rows)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def write_png(path, width, height, rows):
    with open(path, "wb") as f:
        f.write(png_bytes(width, height, rows))


def render_rows(scene):
    rows = []
    for y in range(HEIGHT):
        v = (y + 0.5) / HEIGHT
        row = []
        for x in range(WIDTH):
            u = (x + 0.5) / WIDTH
            r, g, b = pixel(u, v, scene)
            row += [round(r * 255), round(g * 255), round(b * 255), 255]
        rows.append(row)
    return rows


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    check = "--check" in argv
    here = os.path.dirname(os.path.abspath(__file__))
    out_dir = os.path.normpath(os.path.join(here, "..", "env"))
    status = 0
    for name, overrides in SCENES.items():
        scene = {**BASE, **overrides}
        data = png_bytes(WIDTH, HEIGHT, render_rows(scene))
        out = os.path.join(out_dir, name + ".png")
        if check:
            # Determinism pin: re-render in memory and compare against the
            # committed file WITHOUT writing. Byte-identical or it fails.
            try:
                with open(out, "rb") as f:
                    existing = f.read()
            except FileNotFoundError:
                print(f"MISSING {out}")
                status = 1
                continue
            if existing == data:
                print(f"ok      {out} ({len(data)} bytes, byte-identical regen)")
            else:
                print(f"STALE   {out} (on disk {len(existing)} B, regen {len(data)} B)")
                status = 1
        else:
            os.makedirs(out_dir, exist_ok=True)
            write_png(out, WIDTH, HEIGHT, render_rows(scene))
            print(f"wrote {out} ({WIDTH}x{HEIGHT})")
    return status


if __name__ == "__main__":
    sys.exit(main())
