#!/usr/bin/env python3
"""Scorecard: the README's headline numbers, generated from the tree.

Handwritten counts drift (ROADMAP `wave-scorecard`: one pass found four
drifted docs). This script counts from the source, rewrites the marked
spans in README.md, and in `--check` mode fails when they disagree, so CI
catches drift instead of a reader.

    python3 scripts/scorecard.py          # rewrite README.md's marked spans
    python3 scripts/scorecard.py --check  # exit 1 if any span is stale
    python3 scripts/scorecard.py --print  # the numbers, nothing written

What is counted (pure Python, stdlib only, no cargo, seconds):

- crates: the root `[workspace] members`.
- tests: `#[test]` attributes under `crates/`, the number the battery runs
  BEFORE gating (GPU tests skip without an adapter, but they are tests
  in the tree, so they count). A static count, deliberately: a run count
  depends on the host, a feature set and an adapter.

A marked span is `<!-- scorecard:NAME -->value<!-- /scorecard -->`. The
value is rewritten, the markers stay, and text outside them is never
touched.
"""

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TEST_ATTR = re.compile(r"^\s*#\[test\]", re.MULTILINE)
SPAN = re.compile(r"(<!-- scorecard:([a-z-]+) -->)(.*?)(<!-- /scorecard -->)")


def crate_count() -> int:
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    return len(manifest["workspace"]["members"])


def test_count() -> int:
    return sum(
        len(TEST_ATTR.findall(path.read_text(encoding="utf-8")))
        for path in (ROOT / "crates").rglob("*.rs")
    )


def rounded_down(n: int, step: int = 100) -> str:
    """`1,912` → `1,900+`: a floor that stays true as tests are added."""
    return f"{n // step * step:,}+"


def values() -> dict[str, str]:
    tests = test_count()
    return {
        "crates": str(crate_count()),
        "tests": rounded_down(tests),
        "tests-exact": f"{tests:,}",
    }


def rewrite(text: str, vals: dict[str, str]) -> tuple[str, list[str]]:
    """Returns the rewritten text and the names whose value changed.
    Unknown names are an error: a typo'd marker must not pass silently."""
    changed: list[str] = []

    def sub(m: re.Match) -> str:
        name = m.group(2)
        if name not in vals:
            raise SystemExit(f"scorecard: unknown marker {name!r} (known: {sorted(vals)})")
        if m.group(3) != vals[name]:
            changed.append(f"{name}: {m.group(3)!r} -> {vals[name]!r}")
        return f"{m.group(1)}{vals[name]}{m.group(4)}"

    return SPAN.sub(sub, text), changed


def main(argv: list[str]) -> int:
    vals = values()
    if "--print" in argv:
        for k, v in vals.items():
            print(f"{k}: {v}")
        return 0
    readme = ROOT / "README.md"
    text = readme.read_text(encoding="utf-8")
    if not SPAN.search(text):
        print("scorecard: README.md has no scorecard markers", file=sys.stderr)
        return 1
    new, changed = rewrite(text, vals)
    if "--check" in argv:
        if changed:
            print("scorecard: README.md is stale (run scripts/scorecard.py):", file=sys.stderr)
            for c in changed:
                print(f"  {c}", file=sys.stderr)
            return 1
        print("scorecard: README.md current")
        return 0
    if changed:
        readme.write_text(new, encoding="utf-8")
        for c in changed:
            print(f"scorecard: {c}")
    else:
        print("scorecard: README.md already current")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
