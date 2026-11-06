#!/usr/bin/env python3
"""safe-push: the only push path for the shared Umber tree.

Several sessions commit into one working tree, and a plain `git push`
carries every local commit, a sibling's half-done WIP included. This
guard pushes HEAD to origin/master only when all of these hold:

  1. ledger   every commit in origin/master..HEAD was committed through
              .dragon/dcommit.py by one of the caller's sessions (the
              ledger row's `session`). A commit made outside dcommit, or
              by another session, refuses the push.
  2. ff       HEAD descends from origin/master (no force, ever).
  3. fmt      `cargo fmt --all --check` is clean.
  4. score    `python3 scripts/scorecard.py --check` passes.
  5. layers   `python3 scripts/check-layers.py` reports GREEN.
  6. clippy   `cargo clippy -p <affected> --all-targets --no-deps -- -D warnings`.
  7. test     `cargo test -p <affected>`.

Gates 6-7 exist because CI no longer runs on every push (2026-11-06,
Bessa: CI runs at version bumps via scripts/release-ci.sh, weekly, and on
PRs). <affected> = the workspace crates the pushed range touches plus
every crate that depends on them; a root Cargo.toml / Cargo.lock /
rust-toolchain.toml / .cargo change means every crate, `assets/` means
umber-app, a docs-only or scripts/.github-only range means none (the
cargo gates are skipped and say so). An unknown path fails closed (every
crate). Cargo runs under the shared `flock /tmp/umber-cargo.lock nice -n
19` with the tree's own target dir (warm cache), CARGO_BUILD_JOBS=3,
debug info off.

The gates run on a throwaway worktree of the exact commit being pushed,
never on the shared working tree (a sibling's uncommitted WIP neither
fails nor passes them). HEAD is resolved to a SHA once, and that SHA is
what gets pushed, so a sibling commit landing mid-check never rides.

A commit rebased onto a newer origin/master gets a new hash; it still
counts as the caller's when its `git patch-id` equals a ledgered commit
of one of the caller's sessions (a conflict-free rebase keeps the patch
id). The guard is advisory against honest mistakes, not an access
control: the ledger is a local file and --session is self-asserted.

On refusal it prints which rule fired and what to do, exits 2, and
pushes nothing. `--dry-run` runs every check and stops before the push.

Usage:
  python3 scripts/safe-push.py [--session ID]... [--remote origin]
      [--branch master] [--dry-run] [--no-fetch]

Session identity: every --session given, else $DRAGON_SESSION, else
$HERMES_SESSION_ID (the same order as dcommit). A loop whose session id
rotates (compaction) passes each id it has used with --session.

Exit codes: 0 pushed (or dry-run clean), 2 refused, 3 git/push error.
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

LEDGER = Path(".dragon") / "commit-ledger.jsonl"


class Refused(Exception):
    def __init__(self, rule: str, msg: str, fix: str):
        super().__init__(msg)
        self.rule, self.msg, self.fix = rule, msg, fix


# cargo/rustfmt live in ~/.cargo/bin, which is not on every shell's PATH:
# appended (not prepended) so a cargo already on PATH wins.
ENV = dict(os.environ)
ENV["PATH"] = ENV.get("PATH", "") + os.pathsep + os.path.expanduser("~/.cargo/bin")


def sh(root, *cmd, check=False):
    r = subprocess.run(cmd, cwd=root, capture_output=True, text=True, env=ENV)
    if check and r.returncode != 0:
        print(f"safe-push: `{' '.join(cmd)}` failed:\n{r.stdout}{r.stderr}", file=sys.stderr)
        sys.exit(3)
    return r


def sessions_from(args):
    ids = list(args.session or [])
    if not ids:
        for var in ("DRAGON_SESSION", "HERMES_SESSION_ID"):
            if os.environ.get(var):
                ids = [os.environ[var]]
                break
    if not ids:
        raise Refused(
            "ledger",
            "no session identity",
            "pass --session <id> (the id your dcommit calls used) or set $DRAGON_SESSION",
        )
    return set(ids)


def ledger_owner(root):
    """hash -> session for every ledgered commit (amends keep the newest)."""
    owner = {}
    path = Path(root) / LEDGER
    if path.exists():
        for line in path.read_text().splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError:
                continue
            if row.get("hash"):
                owner[row["hash"]] = row.get("session")
    return owner


def patch_id(root, commit):
    """The stable patch id of `commit` (None for an empty or merge diff)."""
    diff = sh(root, "git", "show", "--no-color", "--pretty=format:", commit)
    if diff.returncode != 0 or not diff.stdout.strip():
        return None
    r = subprocess.run(
        ["git", "patch-id", "--stable"], cwd=root, input=diff.stdout, capture_output=True, text=True
    )
    return r.stdout.split()[0] if r.returncode == 0 and r.stdout.strip() else None


def rebased_owner(root, owner, sessions):
    """patch-id -> session for the caller's ledgered commits still in the repo."""
    out = {}
    for h, who in owner.items():
        if who in sessions and sh(root, "git", "cat-file", "-e", f"{h}^{{commit}}").returncode == 0:
            pid = patch_id(root, h)
            if pid:
                out[pid] = who
    return out


def check_ledger(root, upstream, sessions, head):
    revs = sh(root, "git", "rev-list", "--reverse", f"{upstream}..{head}", check=True)
    commits = [c for c in revs.stdout.split() if c]
    if not commits:
        raise Refused("ledger", f"nothing to push: HEAD is at or behind {upstream}", "commit first")
    owner = ledger_owner(root)
    by_patch = None
    for c in commits:
        who = owner.get(c)
        subject = sh(root, "git", "log", "-1", "--format=%h %s", c).stdout.strip()
        if who is None:
            # A rebased own commit: same patch id as a ledgered one.
            if by_patch is None:
                by_patch = rebased_owner(root, owner, sessions)
            pid = patch_id(root, c)
            who = by_patch.get(pid) if pid else None
        if who is None:
            raise Refused(
                "ledger",
                f"{subject}: not in the dcommit ledger (committed outside dcommit)",
                "if it is yours, it skipped the wrapper: re-verify it and recommit via "
                "dcommit; if it is a sibling's, wait for that lane to push it",
            )
        if who not in sessions:
            raise Refused(
                "ledger",
                f"{subject}: committed by session {who}, not by {sorted(sessions)}",
                "a sibling's commit rides this push; wait for its lane to push, "
                "or pass that session with --session only if you own it",
            )
    return commits


def check_ff(root, upstream, head):
    r = sh(root, "git", "merge-base", "--is-ancestor", upstream, head)
    if r.returncode != 0:
        raise Refused(
            "ff",
            f"HEAD does not descend from {upstream}",
            f"`git rebase {upstream}` (own commits keep their ledger standing by "
            "patch id); never force-push the shared branch",
        )


# Paths that never feed a cargo build or test (beyond docs-only files).
NON_CARGO_PREFIXES = ("scripts/", ".github/", ".dragon/", "logs/", "appimage/", "flatpak/", "perf/")
# A change here rebuilds everything.
ALL_CRATES_FILES = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "clippy.toml", "rustfmt.toml"}


def is_docs_only(path):
    """scripts/ci-classify.py's docs allowlist (one rule for CI and push)."""
    import importlib.util

    here = os.path.dirname(os.path.abspath(__file__))
    spec = importlib.util.spec_from_file_location("ci_classify", os.path.join(here, "ci-classify.py"))
    if spec is None or spec.loader is None:
        return False  # classifier missing: treat as code (fail closed)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.is_docs_only(path)


def workspace_crates(tree):
    """{crate dir: (package name, {dependency names})} from crates/*/Cargo.toml."""
    import tomllib

    out = {}
    crates = Path(tree) / "crates"
    if not crates.is_dir():
        return out
    sections = ("dependencies", "dev-dependencies", "build-dependencies")
    for manifest in sorted(crates.glob("*/Cargo.toml")):
        data = tomllib.loads(manifest.read_text())
        name = data.get("package", {}).get("name")
        if not name:
            continue
        deps = set()
        for section in sections:
            deps.update(data.get(section, {}).keys())
        for target in data.get("target", {}).values():
            for section in sections:
                deps.update(target.get(section, {}).keys())
        out[manifest.parent.name] = (name, deps)
    return out


def affected_crates(tree, changed):
    """Package names to clippy + test for `changed` paths (sorted; [] = none)."""
    crates = workspace_crates(tree)
    names = {name for name, _ in crates.values()}
    by_dir = {d: name for d, (name, _) in crates.items()}
    touched = set()
    for path in changed:
        parts = path.split("/")
        if path in ALL_CRATES_FILES or path.startswith(".cargo/"):
            return sorted(names)
        if parts[0] == "crates" and len(parts) > 1:
            if parts[1] not in by_dir:
                return sorted(names)  # a crate dir we cannot place: everything
            touched.add(by_dir[parts[1]])
        elif path.startswith("assets/"):
            if "umber-app" in names:
                touched.add("umber-app")
        elif is_docs_only(path) or path.startswith(NON_CARGO_PREFIXES):
            continue
        else:
            return sorted(names)  # unknown non-docs path: fail closed
    # Close over reverse dependencies: whoever depends on a touched crate.
    grew = True
    while grew:
        grew = False
        for name, deps in crates.values():
            if name not in touched and deps & touched:
                touched.add(name)
                grew = True
    return sorted(touched)


def cargo_cmd(*args):
    """cargo under the shared one-at-a-time lock when flock exists."""
    from shutil import which

    lock = os.environ.get("UMBER_CARGO_LOCK", "/tmp/umber-cargo.lock")
    pre = ["flock", lock, "nice", "-n", "19"] if which("flock") and which("nice") else []
    return [*pre, "cargo", *args]


def check_gate(root, rule, cmd, ok, fix):
    r = sh(root, *cmd)
    out = r.stdout + r.stderr
    if r.returncode != 0 or not ok(out):
        tail = "\n".join(out.strip().splitlines()[-8:])
        raise Refused(rule, f"`{' '.join(cmd)}` failed (exit {r.returncode}):\n{tail}", fix)


def main() -> int:
    ap = argparse.ArgumentParser(description="Guarded push for the shared Umber tree.")
    ap.add_argument("--session", action="append")
    ap.add_argument("--remote", default="origin")
    ap.add_argument("--branch", default="master")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--no-fetch", action="store_true")
    args = ap.parse_args()

    root = sh(".", "git", "rev-parse", "--show-toplevel", check=True).stdout.strip()
    upstream = f"{args.remote}/{args.branch}"
    head = sh(root, "git", "rev-parse", "HEAD", check=True).stdout.strip()
    gate_tree = None
    try:
        sessions = sessions_from(args)
        if not args.no_fetch:
            sh(root, "git", "fetch", "-q", args.remote, args.branch, check=True)
        commits = check_ledger(root, upstream, sessions, head)
        check_ff(root, upstream, head)
        # The gates read the pushed commit's own tree, not the shared one.
        scratch = os.environ.get("TMPDIR") or None
        gate_tree = tempfile.mkdtemp(prefix="safe-push-", dir=scratch)
        os.rmdir(gate_tree)
        sh(root, "git", "worktree", "add", "-q", "--detach", gate_tree, head, check=True)
        check_gate(
            gate_tree,
            "fmt",
            ["cargo", "fmt", "--all", "--check"],
            lambda _: True,
            "run `cargo fmt --all`, then commit the diff through dcommit",
        )
        check_gate(
            gate_tree,
            "score",
            ["python3", "scripts/scorecard.py", "--check"],
            lambda _: True,
            "run `python3 scripts/scorecard.py` to regenerate README claims, then commit",
        )
        check_gate(
            gate_tree,
            "layers",
            ["python3", "scripts/check-layers.py"],
            lambda out: "GREEN" in out,
            "fix the crate-layer violation the script names (DECISIONS.md layering rule)",
        )
        changed = sh(root, "git", "diff", "--name-only", f"{upstream}...{head}", check=True)
        crates = affected_crates(gate_tree, [p for p in changed.stdout.splitlines() if p])
        if not crates:
            print("safe-push: no crate touched, clippy/test gates skipped")
        else:
            print(f"safe-push: clippy + test on {len(crates)} crate(s): {' '.join(crates)}")
            # Warm cache: the tree's own target dir; the shared conventions.
            ENV.update(
                CARGO_TARGET_DIR=os.environ.get("CARGO_TARGET_DIR") or os.path.join(root, "target"),
                CARGO_BUILD_JOBS=os.environ.get("CARGO_BUILD_JOBS", "3"),
                CARGO_PROFILE_TEST_DEBUG="0",
                CARGO_PROFILE_DEV_DEBUG="0",
            )
            pkgs = [a for c in crates for a in ("-p", c)]
            check_gate(
                gate_tree,
                "clippy",
                cargo_cmd("clippy", *pkgs, "--all-targets", "--no-deps", "--", "-D", "warnings"),
                lambda _: True,
                "fix the lints clippy names, then commit through dcommit",
            )
            check_gate(
                gate_tree,
                "test",
                cargo_cmd("test", *pkgs),
                lambda _: True,
                "fix the failing tests, then commit through dcommit",
            )
    except Refused as e:
        print(f"SAFE-PUSH REFUSED [{e.rule}]: {e.msg}", file=sys.stderr)
        print(f"  fix: {e.fix}", file=sys.stderr)
        return 2
    finally:
        if gate_tree:
            sh(root, "git", "worktree", "remove", "--force", gate_tree)

    print(f"safe-push: {len(commits)} commit(s) clear all gates: " + " ".join(c[:8] for c in commits))
    if args.dry_run:
        print("safe-push: dry run, not pushing")
        return 0
    # Push the SHA the checks saw, never the symbolic HEAD.
    r = sh(root, "git", "push", args.remote, f"{head}:refs/heads/{args.branch}")
    sys.stdout.write(r.stdout)
    sys.stderr.write(r.stderr)
    if r.returncode != 0:
        print("safe-push: git push failed", file=sys.stderr)
        return 3
    print(f"safe-push: pushed {commits[-1][:8]} to {upstream}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
