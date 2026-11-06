#!/usr/bin/env python3
"""Brush dab stamps for the stamped starter presets (assets content lane,
2026-11-06 brush-starter slice of docs/loop/briefs/2026-11-05-assets-panel-starter-content.md).

Generates assets/brushes/<stem>.stamp.png — the `<stem>.stamp.png` sidecar
a stamped `.umberbrush` preset carries (crates/umber-app/src/brush_panel.rs
`load_stamp_for_active` / `write_stamp_sidecar`): an 8-bit RGBA PNG, linear
transfer, white shape on transparency (stamp coverage = Rec. 709 luma x
alpha, so white-on-transparent and white-on-black stamp identically).

Four original swatches, one per new starter preset:

  flat-liner.stamp.png   hard-edged bar tilted -30 deg (a flat liner's edge)
  wide-marker.stamp.png  wide rounded-rect marker, soft ~1.5 px edge
  soft-pencil.stamp.png  gaussian soft disc
  speckle.stamp.png      deterministic speckle dots (splitmix64 integer hash)

Starter wave 2 adds two more:

  chalk.stamp.png        grainy disc: soft disc x per-pixel hashed grain
  crosshatch.stamp.png   two crossed hatch-line sets (+45 / -45 deg) in a disc

No licensed content, fully procedural: every pixel is pure math.
Dependency-free like make_studio_env.py — the PNG encoder is ~15 lines of
stdlib (struct + zlib). Fully deterministic (pure math + integer hashing;
no RNG, no clock, no platform entropy): re-running produces byte-identical
output for every stamp. Pin that contract with

  python3 assets/gen/make_brush_stamps.py --check

which re-renders every stamp in memory and compares against the committed
PNG byte-for-byte WITHOUT writing; exit 0 when all match, 1 otherwise.

Cross-check the preset<->sidecar pairing and full PNG structure (magic,
chunk CRCs, IHDR fields, IDAT inflate) build-independently with

  python3 assets/gen/check_stamps.py
"""

import math
import os
import struct
import sys
import zlib

SIZE = 128  # square stamp, 8-bit RGBA

MASK64 = 0xFFFFFFFFFFFFFFFF


def splitmix64(x):
    """Pure-integer 64-bit hash: stable across Python versions/platforms."""
    z = (x + 0x9E3779B97F4A7C15) & MASK64
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK64
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK64
    return z ^ (z >> 31)


def smoothstep(t):
    t = min(1.0, max(0.0, t))
    return t * t * (3.0 - 2.0 * t)


def edge_alpha(d, soft):
    """Coverage for signed distance `d` from a shape edge (positive =
    outside), smoothed over `soft` px."""
    return 255.0 * smoothstep(0.5 - d / soft)


def coverage_flat_liner():
    """Hard-edged bar (the flat liner's chisel edge), tilted -30 deg."""
    angle = math.radians(-30.0)
    ca, sa = math.cos(angle), math.sin(angle)
    half_len, half_wid, soft = 52.0, 7.0, 1.2
    cov = [[0.0] * SIZE for _ in range(SIZE)]
    for y in range(SIZE):
        for x in range(SIZE):
            dx = x + 0.5 - SIZE / 2.0
            dy = y + 0.5 - SIZE / 2.0
            u = dx * ca + dy * sa
            v = -dx * sa + dy * ca
            d = max(abs(u) - half_len, abs(v) - half_wid)
            cov[y][x] = edge_alpha(d, soft)
    return cov


def coverage_wide_marker():
    """Wide rounded-rect marker, slight -8 deg tilt, soft edge."""
    angle = math.radians(-8.0)
    ca, sa = math.cos(angle), math.sin(angle)
    hw, hh, r, soft = 50.0, 15.0, 12.0, 1.5
    cov = [[0.0] * SIZE for _ in range(SIZE)]
    for y in range(SIZE):
        for x in range(SIZE):
            dx = x + 0.5 - SIZE / 2.0
            dy = y + 0.5 - SIZE / 2.0
            u = dx * ca + dy * sa
            v = -dx * sa + dy * ca
            qx = abs(u) - (hw - r)
            qy = abs(v) - (hh - r)
            d = math.hypot(max(qx, 0.0), max(qy, 0.0)) + min(max(qx, qy), 0.0) - r
            cov[y][x] = edge_alpha(d, soft)
    return cov


def coverage_soft_pencil():
    """Gaussian soft disc (a pencil's soft core)."""
    r0 = 46.0
    cov = [[0.0] * SIZE for _ in range(SIZE)]
    for y in range(SIZE):
        for x in range(SIZE):
            dx = x + 0.5 - SIZE / 2.0
            dy = y + 0.5 - SIZE / 2.0
            d2 = (dx * dx + dy * dy) / (r0 * r0)
            cov[y][x] = min(255.0, 255.0 * math.exp(-3.2 * d2))
    return cov


def coverage_speckle():
    """Deterministic speckle: 170 gaussian dots at splitmix64-hashed
    positions/radii, additively composited."""
    cov = [[0.0] * SIZE for _ in range(SIZE)]
    for i in range(170):
        h1 = splitmix64(0x51EC ^ (i * 2 + 1))
        h2 = splitmix64(0x5EED ^ (i * 2 + 2))
        cx = ((h1 >> 11) & ((1 << 53) - 1)) / float(1 << 53) * SIZE
        cy = ((h2 >> 11) & ((1 << 53) - 1)) / float(1 << 53) * SIZE
        rad = 1.0 + ((h1 >> 40) % 1000) / 1000.0 * 2.6
        amp = 235.0
        x0 = max(0, int(cx - rad * 3.0))
        x1 = min(SIZE, int(cx + rad * 3.0) + 1)
        y0 = max(0, int(cy - rad * 3.0))
        y1 = min(SIZE, int(cy + rad * 3.0) + 1)
        for y in range(y0, y1):
            for x in range(x0, x1):
                d2 = (x + 0.5 - cx) ** 2 + (y + 0.5 - cy) ** 2
                cov[y][x] = min(255.0, cov[y][x] + amp * math.exp(-2.4 * d2 / (rad * rad)))
    return cov


def coverage_chalk():
    """Grainy disc (chalk on a rough surface): a soft-edged disc whose
    coverage is thinned by a per-pixel splitmix64 grain threshold."""
    r0, soft = 50.0, 6.0
    cov = [[0.0] * SIZE for _ in range(SIZE)]
    for y in range(SIZE):
        for x in range(SIZE):
            dx = x + 0.5 - SIZE / 2.0
            dy = y + 0.5 - SIZE / 2.0
            disc = smoothstep((r0 - math.hypot(dx, dy)) / soft)
            h = splitmix64(0xC4A1 ^ (y * SIZE + x))
            grain = ((h >> 11) & 0xFFFF) / 65535.0
            cov[y][x] = 255.0 * disc * smoothstep((grain - 0.25) / 0.5)
    return cov


def coverage_crosshatch():
    """Two crossed sets of parallel hatch lines (+45 / -45 deg), clipped
    to a soft disc."""
    period, half_wid, soft, r0 = 14.0, 1.6, 1.0, 54.0
    cov = [[0.0] * SIZE for _ in range(SIZE)]
    s = math.sqrt(0.5)
    for y in range(SIZE):
        for x in range(SIZE):
            dx = x + 0.5 - SIZE / 2.0
            dy = y + 0.5 - SIZE / 2.0
            best = 0.0
            for u in ((dx + dy) * s, (dx - dy) * s):
                m = u - period * math.floor(u / period + 0.5)
                best = max(best, smoothstep(0.5 - (abs(m) - half_wid) / soft))
            disc = smoothstep((r0 - math.hypot(dx, dy)) / 4.0)
            cov[y][x] = 255.0 * best * disc
    return cov


STAMPS = {
    "flat-liner": coverage_flat_liner,
    "wide-marker": coverage_wide_marker,
    "soft-pencil": coverage_soft_pencil,
    "speckle": coverage_speckle,
    "chalk": coverage_chalk,
    "crosshatch": coverage_crosshatch,
}


def render_rows(render):
    """Coverage grid -> PNG rows: white RGB, alpha = coverage (the sidecar
    shape write_stamp_sidecar writes; coverage is re-derived on load as
    luma x alpha)."""
    cov = render()
    rows = []
    for y in range(SIZE):
        row = []
        for x in range(SIZE):
            row += [255, 255, 255, int(round(cov[y][x]))]
        rows.append(row)
    return rows


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


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    check = "--check" in argv
    here = os.path.dirname(os.path.abspath(__file__))
    out_dir = os.path.normpath(os.path.join(here, "..", "brushes"))
    status = 0
    for name, render in STAMPS.items():
        data = png_bytes(SIZE, SIZE, render_rows(render))
        out = os.path.join(out_dir, name + ".stamp.png")
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
            with open(out, "wb") as f:
                f.write(data)
            print(f"wrote {out} ({SIZE}x{SIZE} RGBA8, {len(data)} bytes)")
    return status


if __name__ == "__main__":
    sys.exit(main())
