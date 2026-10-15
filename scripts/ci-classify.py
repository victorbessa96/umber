#!/usr/bin/env python3
"""CI change classifier: prints `code=true` or `code=false` for
$GITHUB_OUTPUT. `false` (skip the build matrix) ONLY when every changed
file is docs-shaped; everything else, and every doubt, is `true`.

Inputs (env): EVENT (github.event_name), BASE (the PR base sha), FULL_CI
("true" when the PR carries the `full-ci` label).

    python3 scripts/ci-classify.py                 # CI usage
    python3 scripts/ci-classify.py --check a.md x.rs  # classify a list

Absorbed 2026-10-14 from photocraft's ci.yml `platforms` job (reshaped:
a docs ALLOWLIST rather than a platform-path list — Umber's every crate
is platform-sensitive, so the only safe skip is "nothing compilable
changed").
"""

import os
import subprocess
import sys
from pathlib import PurePosixPath

# A path is docs-only when it matches one of these. Anything else —
# including a .md INSIDE crates/ (doc-tests and include_str! read those)
# — counts as code.
DOCS_DIRS = ("docs/",)
DOCS_ROOT_FILES = {
    "README.md",
    "CHANGELOG.md",
    "CONTRIBUTING.md",
    "SECURITY.md",
    "CODE_OF_CONDUCT.md",
    "STATE.md",
    "DECISIONS.md",
    "CLAUDE.md",
    "AGENTS.md",
    "LICENSE",
}
DOCS_PREFIXES = (".github/ISSUE_TEMPLATE/", ".github/PULL_REQUEST_TEMPLATE")


def is_docs_only(path: str) -> bool:
    """Whether a changed path can never affect a build or a test."""
    p = PurePosixPath(path)
    text = p.as_posix()
    if text in DOCS_ROOT_FILES:
        return True
    if text.startswith(DOCS_PREFIXES):
        return True
    # docs/ is docs — except anything a test reads from it: only prose and
    # images count. Tests DO read docs/ (umber-graph's mtlx_doc.rs loads
    # docs/claw-artifacts/openpbr/open_pbr_surface.mtlx), so a .mtlx /
    # .json / any other file there stays code.
    return text.startswith(DOCS_DIRS) and p.suffix in {".md", ".png", ".svg", ".jpg"}


def classify(paths) -> bool:
    """True = run the matrix. An empty diff runs it (doubt -> build)."""
    paths = [p for p in paths if p.strip()]
    return not paths or not all(is_docs_only(p) for p in paths)


def changed_files(base: str):
    """The PR's changed files vs its base, or None if git cannot say."""
    try:
        out = subprocess.run(
            ["git", "diff", "--name-only", f"{base}...HEAD"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    return out.splitlines()


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--check":
        print(f"code={'true' if classify(sys.argv[2:]) else 'false'}")
        return 0
    event = os.environ.get("EVENT", "")
    base = os.environ.get("BASE", "")
    if event != "pull_request" or os.environ.get("FULL_CI") == "true" or not base:
        print("code=true")
        return 0
    files = changed_files(base)
    if files is None:
        print("code=true")  # fail open: a classifier error must not skip CI
        return 0
    print(f"code={'true' if classify(files) else 'false'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
