#!/usr/bin/env python3
"""Unit tests for source/lockfile ownership (stdlib only)."""

import importlib.util
import json
from pathlib import Path

MODULE = Path(__file__).resolve().parent / "gen-cargo-sources.py"
spec = importlib.util.spec_from_file_location("gen_cargo_sources", MODULE)
assert spec is not None and spec.loader is not None
gen = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gen)
OLD = "0" * 40
NEW = "1" * 40
LOCK = """[[package]]
name = \"demo\"
version = \"1.0.0\"
source = \"registry+https://github.com/rust-lang/crates.io-index\"
checksum = \"abc123\"
"""
TEMPLATE = json.dumps(
    {
        "modules": [
            {
                "sources": [
                    {"type": "git", "tag": "old-tag", "commit": OLD},
                    "@@CARGO_SOURCES@@",
                ]
            }
        ]
    }
)


def main():
    seen = []

    def fake_git_show(commit):
        seen.append(commit)
        return LOCK.replace("abc123", commit)

    template, lock_text, commit = gen.source_context(TEMPLATE, {}, fake_git_show)
    assert template == TEMPLATE
    assert commit == OLD and seen == [OLD]
    assert gen.parse_lock(lock_text) == [("demo", "1.0.0", OLD)]
    _, _, commit = gen.source_context(
        TEMPLATE,
        {"GITHUB_REF": "refs/heads/master", "GITHUB_SHA": NEW},
        fake_git_show,
    )
    assert commit == OLD and seen[-1] == OLD

    env = {"GITHUB_REF": "refs/tags/v2.0.0", "GITHUB_REF_NAME": "v2.0.0", "GITHUB_SHA": NEW}
    template, lock_text, commit = gen.source_context(TEMPLATE, env, fake_git_show)
    assert commit == NEW and seen[-1] == NEW
    source = json.loads(template)["modules"][0]["sources"][0]
    assert source["tag"] == "v2.0.0" and source["commit"] == NEW
    assert gen.parse_lock(lock_text)[0][2] == NEW

    quoted = gen.source_context(
        TEMPLATE,
        {**env, "GITHUB_REF_NAME": 'v"quoted'},
        fake_git_show,
    )[0]
    assert json.loads(quoted)["modules"][0]["sources"][0]["tag"] == 'v"quoted'

    try:
        gen.source_context(TEMPLATE, {**env, "GITHUB_SHA": "bad"}, fake_git_show)
    except SystemExit as error:
        assert "invalid source commit" in str(error)
    else:
        raise AssertionError("invalid release SHA accepted")
    print("flatpak source/lock ownership: 5/5 passed")


if __name__ == "__main__":
    main()
