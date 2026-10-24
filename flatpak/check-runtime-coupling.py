#!/usr/bin/env python3
"""check-runtime-coupling.py — assert the flatpak manifest's runtime
version and the packaging workflow's install refs move TOGETHER.

Provenance: 2026-10-23's wall-3 (packaging run 40025359694) — the
manifest's runtime-version bumped to 26.08 (e3ae21d) while the
workflow's install step still installed the 24.08 platform/SDK trio,
and the job died 'org.freedesktop.Sdk/x86_64/26.08 not installed'
(fixed by e76be0c). The two surfaces are version-coupled by design;
this check turns silent drift into a loud lint failure before any
dispatch burns a 30-minute runner on it.

Usage: python3 flatpak/check-runtime-coupling.py   (exit 0 = coupled)
"""
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
MANIFEST = REPO / "flatpak" / "org.umber.Umber.json"
WORKFLOW = REPO / ".github" / "workflows" / "packaging.yml"

TRIO = (
    "org.freedesktop.Platform//",
    "org.freedesktop.Sdk//",
    "org.freedesktop.Sdk.Extension.rust-stable//",
)


def main() -> int:
    version = json.loads(MANIFEST.read_text())["runtime-version"]
    wf = WORKFLOW.read_text()
    missing = [ref + version for ref in TRIO if ref + version not in wf]
    if missing:
        print(
            "FAIL: the workflow's flatpak install refs lag the manifest's "
            f"runtime-version {version}: {missing} — bump the trio in the "
            "install step together with the manifest (the e76be0c lesson)"
        )
        return 1
    print(f"coupling OK: all three {version} refs present in the workflow")
    return 0


if __name__ == "__main__":
    sys.exit(main())
