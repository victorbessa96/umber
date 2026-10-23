#!/usr/bin/env python3
"""vendor-checksums.py — make the staged vendor/ tree cargo-vendor-shaped.

Runs as the Flatpak build's FIRST build-command, inside the sandbox
(no network, no cargo needed). flatpak-builder's archive sources have
extracted every crate into `vendor/<name>-<version>/`; cargo's
source-replacement consumer requires each vendored crate to carry a
`.cargo-checksum.json` exactly like `cargo vendor` writes:

    { "files":  { "<path>": "<sha256>", ... },
      "package": "<Cargo.lock checksum>" }

The files map is computed over the extracted tree (every file except
the checksum file itself); the package checksum comes straight from
Cargo.lock — the registry `.crate` archive's own hash, which is what
cargo vendor records. Idempotent: re-running rewrites the same bytes.

Usage (from the build root, where Cargo.lock and vendor/ live):
    python3 flatpak/vendor-checksums.py
"""

import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VENDOR = ROOT / "vendor"
LOCK = ROOT / "Cargo.lock"


def lock_checksums() -> dict:
    """Cargo.lock -> {(name, version): checksum} for registry crates."""
    out = {}
    block = {}
    for line in LOCK.read_text().splitlines():
        if line.strip() == "[[package]]":
            if "name" in block and "checksum" in block:
                out[(block["name"], block["version"])] = block["checksum"]
            block = {}
            continue
        m = re.match(r'^name = "(.*)"$', line)
        if m:
            block["name"] = m.group(1)
            continue
        m = re.match(r'^version = "(.*)"$', line)
        if m:
            block["version"] = m.group(1)
            continue
        m = re.match(r'^checksum = "(.*)"$', line)
        if m:
            block["checksum"] = m.group(1)
    if "name" in block and "checksum" in block:
        out[(block["name"], block["version"])] = block["checksum"]
    return out


def split_dirname(dirname: str):
    """`<name>-<version>` -> (name, version). The version is a TRAILING
    semver — `\\d+.\\d+.\\d+` with optional prerelease/build segments
    that may contain '-': the seam is found by the semver SHAPE, not by
    a dash position (a crate NAME can contain dashes AND a prerelease
    version contains them too — proven live: cranelift-assembler-x64
    0.137.0-rc.1 died on the old last-dash rule, packaging run
    40017442313)."""
    m = re.search(r"-(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?)$", dirname)
    if not m:
        sys.exit(
            f"vendor-checksums: unparseable vendor dir {dirname!r} — every"
            " dir must be <crate>-<version>/ as cargo stages them. Flat"
            " crate CONTENTS (src/, benches/, .cargo/) means the"
            " flatpak-builder archive sources stripped their top dir:"
            " every .crate source needs \"strip-components\": 0"
            " (flatpak-builder's archive default is 1)."
        )
    return dirname[: m.start()], m.group(1)


def main() -> None:
    if not VENDOR.is_dir():
        sys.exit("vendor-checksums: no vendor/ dir — the archive sources did not stage")
    checksums = lock_checksums()
    written = 0
    for crate_dir in sorted(p for p in VENDOR.iterdir() if p.is_dir()):
        name, version = split_dirname(crate_dir.name)
        package = checksums.get((name, version))
        if package is None:
            sys.exit(
                f"vendor-checksums: vendor dir {crate_dir.name} has no "
                "Cargo.lock entry — vendor/ and Cargo.lock are out of sync"
            )
        files = {}
        for path in sorted(crate_dir.rglob("*")):
            if not path.is_file() or path.name == ".cargo-checksum.json":
                continue
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            files[str(path.relative_to(crate_dir))] = digest
        payload = {"files": files, "package": package}
        target = crate_dir / ".cargo-checksum.json"
        target.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
        written += 1
    if not written:
        sys.exit("vendor-checksums: vendor/ holds no crate directories")
    print(f"vendor-checksums: {written} crate checksum files written")


if __name__ == "__main__":
    main()
