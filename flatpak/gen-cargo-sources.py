#!/usr/bin/env python3
"""gen-cargo-sources.py — render the Flatpak manifest hermetic.

Reads Cargo.lock (v3/v4) and the hand-maintained template
`org.umber.Umber.json.in`, splices one flatpak-builder source entry
per registry crate into the `@@CARGO_SOURCES@@` slot, and writes
`org.umber.Umber.json` (the committed, CI-validated artifact).

Each registry package becomes:

    { "type": "archive",
      "archive-type": "tar-gzip",
      "url": "https://static.crates.io/crates/<name>/<name>-<version>.crate",
      "sha256": "<Cargo.lock checksum>",
      "dest": "vendor" }

`archive-type` is declared explicitly: flatpak-builder infers the
archive format from the URL suffix and `.crate` is not in its table —
the packaging dispatch 39263847856 died with `Unknown archive format`
after the sha256 had already verified (the downloads were the real
crate bytes, not a CDN transient). The official command reference
lists `tar-gzip` among the accepted values for unguessable suffixes.

flatpak-builder downloads the archives in its SOURCE phase (network
allowed there), and the `.crate` gzipped tar's single root directory
`<name>-<version>/` extracts into `vendor/` — cargo-vendor's layout,
consumed offline through the source-replacement config the template
stages beside it. The sha256 is Cargo.lock's own checksum (the
registry `.crate` archive hash — no download needed to generate).

Workspace members (path dependencies, no checksum, no `source`) are
skipped: the git source IS the workspace. Any package with a
`source` that is not the crates.io registry is an error — it would
need its own manifest source shape, not this generator's.

The lint job (packaging.yml) re-runs this and diffs, so a Cargo.lock
change without a regenerated manifest fails CI (the drift gate).

Usage: python3 flatpak/gen-cargo-sources.py [--check]
  --check: exit 1 if the rendered manifest differs from the committed
  one (CI's drift gate; prints nothing on a clean tree).
"""

import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
LOCK = HERE.parent / "Cargo.lock"
TEMPLATE = HERE / "org.umber.Umber.json.in"
OUT = HERE / "org.umber.Umber.json"
MARKER = '"@@CARGO_SOURCES@@"'
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"


def parse_lock(text: str):
    """Cargo.lock v3/v4 [[package]] blocks -> [(name, version, checksum)].

    Workspace members (no checksum, no source) are skipped. A package
    with a source that is not the crates.io registry is an error.
    """
    packages = []
    block = {}
    for line in text.splitlines():
        if line.strip() == "[[package]]":
            if block:
                packages.append(block)
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
        m = re.match(r"^source = (.*)$", line)
        if m:
            block["source"] = m.group(1).strip('"')
            continue
        m = re.match(r'^checksum = "(.*)"$', line)
        if m:
            block["checksum"] = m.group(1)
            continue
    if block:
        packages.append(block)

    crates = []
    seen = set()
    for pkg in packages:
        name, version = pkg.get("name"), pkg.get("version")
        if not name or not version:
            sys.exit(f"gen-cargo-sources: Cargo.lock package missing name/version: {pkg}")
        source = pkg.get("source")
        if source is None:
            continue  # workspace member: the git source is the workspace
        if source != REGISTRY:
            sys.exit(
                f"gen-cargo-sources: {name} {version} has non-registry source "
                f"{source!r} — it needs its own manifest source entry; refusing "
                "to silently drop it"
            )
        checksum = pkg.get("checksum")
        if not checksum:
            sys.exit(
                f"gen-cargo-sources: registry package {name} {version} has no "
                "checksum in Cargo.lock (corrupt lock?) — refusing"
            )
        key = (name, version)
        if key in seen:
            continue
        seen.add(key)
        crates.append((name, version, checksum))
    if not crates:
        sys.exit("gen-cargo-sources: no registry crates found in Cargo.lock")
    return crates


def render() -> str:
    crates = parse_lock(LOCK.read_text())
    template = TEMPLATE.read_text()
    if MARKER not in template:
        sys.exit(f"gen-cargo-sources: {TEMPLATE.name} lost its {MARKER} slot")
    entries = ",\n".join(
        json.dumps(
            {
                "type": "archive",
                "archive-type": "tar-gzip",
                "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
                "sha256": checksum,
                "dest": "vendor",
            },
            indent=8,
        )
        .replace("\n", "\n" + " " * 8)
        .lstrip()
        for name, version, checksum in crates
    )
    rendered = template.replace(MARKER, entries)
    # The splice must produce valid JSON — fail here, never in CI's lint.
    json.loads(rendered)
    return rendered


def main() -> None:
    check = "--check" in sys.argv[1:]
    rendered = render()
    if check:
        committed = OUT.read_text()
        if committed != rendered:
            sys.exit(
                "gen-cargo-sources: flatpak/org.umber.Umber.json is stale — "
                "run `python3 flatpak/gen-cargo-sources.py` and commit the "
                "result (Cargo.lock changed without regenerating the manifest)"
            )
        return
    OUT.write_text(rendered)
    crates = parse_lock(LOCK.read_text())
    print(f"gen-cargo-sources: wrote {OUT.name} — {len(crates)} registry crates")


if __name__ == "__main__":
    main()
