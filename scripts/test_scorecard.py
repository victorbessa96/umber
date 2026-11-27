#!/usr/bin/env python3
"""Unit tests for scorecard marker completeness and drift (stdlib only)."""

import importlib.util
from pathlib import Path

MODULE = Path(__file__).resolve().parent / "scorecard.py"
spec = importlib.util.spec_from_file_location("scorecard", MODULE)
assert spec is not None and spec.loader is not None
scorecard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scorecard)
VALS = {"crates": "16", "crates-layout": "16", "tests": "3,300+", "tests-exact": "3,345"}


def marker(name, value):
    return f"<!-- scorecard:{name} -->{value}<!-- /scorecard -->"


def document(vals=VALS):
    return "\n".join(marker(name, value) for name, value in vals.items())


def expect_failure(text, needle):
    try:
        scorecard.validate_markers(text, VALS)
    except SystemExit as error:
        assert needle in str(error), (needle, str(error))
        return
    raise AssertionError(f"expected marker failure containing {needle!r}")


def main():
    scorecard.validate_markers(document(), VALS)
    for name in VALS:
        expect_failure(document({k: v for k, v in VALS.items() if k != name}), "missing")
        expect_failure(document() + "\n" + marker(name, VALS[name]), "duplicate")
    expect_failure(document() + "\n" + marker("typo", "x"), "unknown")
    for name in ("crates2", "Crates", "crates_extra"):
        expect_failure(document() + "\n" + marker(name, "x"), "unknown")
    expect_failure(document() + "\n<!-- scorecard:crates -->", "malformed")
    expect_failure(
        document() + "\n<!--scorecard:crates-->99<!--/scorecard-->",
        "duplicate",
    )
    rewritten, changed = scorecard.rewrite(
        document({**VALS, "tests": "3,200+"}), VALS
    )
    assert changed == ["tests: '3,200+' -> '3,300+'"], changed
    scorecard.validate_markers(rewritten, VALS)
    print("scorecard marker tests: 15/15 passed")


if __name__ == "__main__":
    main()
