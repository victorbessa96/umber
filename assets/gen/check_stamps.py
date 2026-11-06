#!/usr/bin/env python3
"""Build-independent validator for the brush stamp sidecars (assets content
lane, 2026-11-06 brush-starter slice).

For every `*.umberbrush` in assets/brushes (default: the sibling `brushes/`
dir of this script; override with a path argument):

  - a preset with `"stamped": true` under `params` MUST have a
    `<stem>.stamp.png` sidecar, and that PNG must parse fully: magic,
    per-chunk CRC32, IHDR = 8-bit RGBA (color type 6), non-interlaced,
    IDAT inflates to exactly h*(1 + w*4) bytes, IEND last, no trailing
    bytes — the shape `umber_export::png::read_png_rgba8` accepts and
    `write_stamp_sidecar` writes;
  - a preset without `stamped` MUST NOT carry a sidecar (the app keys the
    stamp load on the flag; an orphan sidecar would swatch but never
    stamp);
  - every `*.stamp.png` MUST have a matching `.umberbrush`.

Stdlib only (json + struct + zlib), no build required. Exit 0 when the
whole shelf is consistent, 1 otherwise.

  python3 assets/gen/check_stamps.py [brushes_dir]
"""

import json
import os
import struct
import sys
import zlib

PNG_MAGIC = b"\x89PNG\r\n\x1a\n"


def parse_png_rgba8(data):
    """Full structural parse of an 8-bit RGBA non-interlaced PNG.
    Returns (width, height); raises ValueError on any defect."""
    if len(data) < 8 or data[:8] != PNG_MAGIC:
        raise ValueError("bad PNG magic")
    pos = 8
    ihdr = None
    idat = bytearray()
    seen_iend = False
    while pos < len(data):
        if pos + 8 > len(data):
            raise ValueError("truncated chunk header")
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        ctype = data[pos + 4 : pos + 8]
        if pos + 12 + length > len(data):
            raise ValueError(f"truncated {ctype!r} chunk")
        body = data[pos + 4 : pos + 8 + length]
        (crc,) = struct.unpack(">I", data[pos + 8 + length : pos + 12 + length])
        if zlib.crc32(body) & 0xFFFFFFFF != crc:
            raise ValueError(f"bad CRC in {ctype!r} chunk")
        if ctype == b"IHDR":
            if length != 13:
                raise ValueError("IHDR length must be 13")
            w, h, bd, ct, comp, filt, inter = struct.unpack(">IIBBBBB", body[4:])
            if not (1 <= w <= 4096 and 1 <= h <= 4096):
                raise ValueError(f"dims out of range: {w}x{h}")
            if (bd, ct, comp, filt, inter) != (8, 6, 0, 0, 0):
                raise ValueError(
                    f"not 8-bit RGBA non-interlaced: bit_depth={bd} color_type={ct} "
                    f"compression={comp} filter={filt} interlace={inter}"
                )
            ihdr = (w, h)
        elif ctype == b"IDAT":
            idat += body[4:]
        elif ctype == b"IEND":
            seen_iend = True
            pos += 12 + length
            break
        pos += 12 + length
    if ihdr is None:
        raise ValueError("no IHDR chunk")
    if not seen_iend:
        raise ValueError("no IEND chunk")
    if pos != len(data):
        raise ValueError("trailing bytes after IEND")
    w, h = ihdr
    raw = zlib.decompress(bytes(idat))
    if len(raw) != h * (1 + w * 4):
        raise ValueError(f"IDAT inflates to {len(raw)} B, expected {h * (1 + w * 4)}")
    return ihdr


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv:
        brushes_dir = os.path.abspath(argv[0])
    else:
        here = os.path.dirname(os.path.abspath(__file__))
        brushes_dir = os.path.normpath(os.path.join(here, "..", "brushes"))
    if not os.path.isdir(brushes_dir):
        print(f"MISSING dir {brushes_dir}")
        return 1
    errors = 0
    presets = sorted(
        f for f in os.listdir(brushes_dir) if f.endswith(".umberbrush")
    )
    sidecars = sorted(f for f in os.listdir(brushes_dir) if f.endswith(".stamp.png"))
    for name in presets:
        path = os.path.join(brushes_dir, name)
        stem = name[: -len(".umberbrush")]
        sidecar = stem + ".stamp.png"
        sidecar_path = os.path.join(brushes_dir, sidecar)
        try:
            with open(path, "r", encoding="utf-8") as f:
                preset = json.load(f)
        except (OSError, ValueError) as e:
            print(f"ERROR   {name}: unreadable/invalid JSON: {e}")
            errors += 1
            continue
        stamped = bool(preset.get("params", {}).get("stamped", False))
        if stamped:
            if not os.path.isfile(sidecar_path):
                print(f"ERROR   {name}: stamped:true but no {sidecar}")
                errors += 1
                continue
            with open(sidecar_path, "rb") as f:
                data = f.read()
            try:
                w, h = parse_png_rgba8(data)
            except ValueError as e:
                print(f"ERROR   {sidecar}: {e}")
                errors += 1
                continue
            print(f"ok      {name} -> {sidecar} ({w}x{h} RGBA8, {len(data)} bytes, valid)")
        else:
            if os.path.isfile(sidecar_path):
                print(f"ERROR   {name}: not stamped but {sidecar} exists")
                errors += 1
            else:
                print(f"ok      {name} (radial, no sidecar)")
    for name in sidecars:
        if name[: -len(".stamp.png")] + ".umberbrush" not in presets:
            print(f"ERROR   {name}: sidecar without a matching .umberbrush")
            errors += 1
    if errors:
        print(f"{errors} problem(s)")
        return 1
    print(f"all consistent: {len(presets)} presets, {len(sidecars)} sidecars")
    return 0


if __name__ == "__main__":
    sys.exit(main())
