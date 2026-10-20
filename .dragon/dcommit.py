#!/usr/bin/env python3
"""dcommit — fail-closed commit gate for the shared umber tree (.dragon/ tooling).

Encodes the 2026-10-19 collisions as interlocks instead of lesson-memory:

  (1) The index sweep (19:3x): `git commit` takes the WHOLE index, so a
      sibling session's staged hunks ride your commit. dcommit refuses a
      commit whose staged set escapes the caller's declared --lock paths.

  (2) The amend fold (14:5x): `git commit --amend` rewrote a sibling's
      commit that had landed seconds earlier. dcommit refuses --amend
      unless the commit ledger proves HEAD was committed by THIS session.
      Unknown HEAD = refuse (all sessions share one git identity, so git
      authorship cannot prove ownership; the ledger is the oracle).

Fail-closed throughout: no session identity, unknown/foreign HEAD,
empty staged set, staged files outside the locks, lock paths outside
the repo, a bare `--` pathspec in the commit args, or an interactive-
editor commit -> refuse, exit 2, tree untouched. Every wrapper-performed
commit appends one row to .dragon/commit-ledger.jsonl (append-only).

Usage:
  .dragon/dcommit.py [--session ID] [--lock PATH]... [--amend] [--path-limited] -- <git commit args>

  --session       caller identity; default $DRAGON_SESSION, then
                  $HERMES_SESSION_ID; absent -> refuse.
  --lock PATH     repeatable; the claimed lock paths for THIS commit.
                  Index mode: every staged file must be inside a lock
                  path (escapees are named in the refusal).
  --amend         gate and pass --amend; ownership-checked against the
                  ledger (also detected when given inside the tail).
  --path-limited  run the path-limited form `git commit <args> -- <locks>`
                  (commits only the lock paths even with a dirty index).

Exit codes: 0 = committed + ledgered; 2 = refused (tree untouched);
other = git's own exit code.

This file is agent tooling: python3 stdlib + git only, no cargo, no
battery. Written 2026-10-20 by session local-f15cdefa (M3 lane,
steer 23:52); report: docs/loop/kill-reports/2026-10-20-dragon-commit-wrapper.md
"""

import argparse
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import NoReturn

LEDGER_NAME = os.path.join(".dragon", "commit-ledger.jsonl")


def refuse(msg) -> NoReturn:
    """Fail-closed: print the refusal, leave the tree untouched."""
    print(f"dcommit: REFUSED: {msg}", file=sys.stderr)
    sys.exit(2)


def git(root, *args):
    return subprocess.run(
        ["git", "-C", root, *args], capture_output=True, text=True
    )


def git_or_refuse(root, *args, what="git command"):
    p = git(root, *args)
    if p.returncode != 0:
        refuse(f"{what} failed: git {' '.join(args)}: {p.stderr.strip()}")
    return p.stdout


def resolve_session(args):
    session = args.session or os.environ.get("DRAGON_SESSION") or os.environ.get(
        "HERMES_SESSION_ID"
    )
    if not session:
        refuse(
            "no session identity (--session, $DRAGON_SESSION, $HERMES_SESSION_ID "
            "all unset); a shared-tree commit cannot be attributed, so it cannot "
            "be gated"
        )
    return session


def resolve_locks(raw_locks, root, caller_cwd):
    """Normalize lock paths to repo-root-relative form."""
    if not raw_locks:
        refuse("no --lock paths given; declare the lock scope of this commit")
    locks = []
    for lock in raw_locks:
        abspath = os.path.abspath(os.path.join(caller_cwd, lock))
        try:
            rel = os.path.relpath(abspath, root)
        except ValueError:
            refuse(f"lock path {lock!r} cannot be related to the repo root")
        if rel == ".." or rel.startswith(".." + os.sep):
            refuse(f"lock path {lock!r} is outside the repository")
        locks.append(rel)
    return locks


def covered(path, locks):
    """True if path equals a lock or lives under a directory lock."""
    for lock in locks:
        if path == lock or path.startswith(lock + "/"):
            return True
    return False


def load_ledger(root):
    ledger = Path(root) / LEDGER_NAME
    rows = []
    if ledger.exists():
        for line in ledger.read_text().splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError:
                # A corrupt line never grants ownership; skip it.
                continue
    return rows


def append_ledger(root, row):
    ledger_dir = Path(root) / ".dragon"
    ledger_dir.mkdir(parents=True, exist_ok=True)
    with open(ledger_dir / "commit-ledger.jsonl", "a") as fh:
        fh.write(json.dumps(row) + "\n")


def check_amend_ownership(root, session):
    head = git_or_refuse(root, "rev-parse", "HEAD", what="HEAD lookup").strip()
    rows = [r for r in load_ledger(root) if r.get("hash") == head]
    if not rows:
        refuse(
            f"--amend on HEAD {head[:12]} which is not in the commit ledger: "
            "not provably this session's commit. Banked rule (14:5x collision): "
            "never --amend on the shared tree without proving HEAD is your own."
        )
    owner = rows[-1].get("session")
    if owner != session:
        refuse(
            f"--amend on HEAD {head[:12]} owned by session {owner!r}, "
            f"not {session!r}"
        )
    return head


def tail_has_message(tail):
    msg_flags = {"-m", "-F", "-C", "-c"}
    for tok in tail:
        if tok in msg_flags:
            return True
        if tok in ("--file", "--message") or tok.startswith(("--file=", "--message=")):
            return True
    return False


def main():
    caller_cwd = os.getcwd()
    ap = argparse.ArgumentParser(
        prog="dcommit",
        description="fail-closed commit gate for the shared tree",
    )
    ap.add_argument("--session", help="calling session id (default: env)")
    ap.add_argument("--lock", action="append", default=[], help="lock path (repeatable)")
    ap.add_argument(
        "--amend",
        action="store_true",
        help="gate and pass --amend (ownership-checked against the ledger)",
    )
    ap.add_argument(
        "--path-limited",
        action="store_true",
        help="commit only the lock paths (git commit <args> -- <locks>)",
    )
    ap.add_argument(
        "tail", nargs=argparse.REMAINDER, help="git commit args after a bare --"
    )
    argv = sys.argv[1:]
    if "--" not in argv:
        refuse("no argument separator '--' found; usage: dcommit [opts] -- <git commit args>")
    args = ap.parse_args(argv)

    tail = list(args.tail)
    if tail and tail[0] == "--":
        tail = tail[1:]
    if not tail:
        refuse("no git commit arguments after '--'")

    session = resolve_session(args)

    is_amend = args.amend or "--amend" in tail
    if not tail_has_message(tail) and not is_amend:
        refuse(
            "no commit message flag (-m/-F/-C/-c) in the commit args; interactive "
            "editor commits are not allowed in agent context"
        )

    root_p = git(caller_cwd, "rev-parse", "--show-toplevel")
    if root_p.returncode != 0:
        refuse(f"not a git work tree: {caller_cwd}")
    root = root_p.stdout.strip()

    locks = resolve_locks(args.lock, root, caller_cwd)

    if "--" in tail:
        refuse(
            "bare '--' pathspec inside the commit args; the --lock paths are the "
            "pathspec (use --path-limited for the path-limited form)"
        )

    amended_from = None
    if is_amend:
        amended_from = check_amend_ownership(root, session)

    amend_args = ["--amend"] if args.amend and "--amend" not in tail else []

    if args.path_limited:
        # The path-limited form: git itself commits only the lock paths,
        # so a dirty index elsewhere cannot ride. Typo guard first.
        for rel in locks:
            listed = git(root, "ls-files", "--", rel)
            in_index = listed.returncode == 0 and bool(listed.stdout.strip())
            if not Path(root, rel).exists() and not in_index:
                refuse(
                    f"lock path {rel!r} matches neither the work tree nor the "
                    "index (typo?)"
                )
        commit_cmd = ["commit"] + amend_args + tail + ["--"] + locks
        mode = "path-limited"
        committed_paths = locks
    else:
        staged = [
            s for s in git_or_refuse(
                root, "diff", "--cached", "--name-only", what="staged-set read"
            ).splitlines() if s.strip()
        ]
        if not staged:
            refuse("nothing staged; refusing (an empty commit attempt or a forgotten git add)")
        escapees = [s for s in staged if not covered(s, locks)]
        if escapees:
            refuse(
                "staged set escapes the declared lock paths — unstage them or "
                "declare locks for them: " + ", ".join(escapees)
            )
        for lock in locks:
            if not any(covered(s, [lock]) for s in staged):
                print(f"dcommit: note: lock path {lock!r} matched nothing staged", file=sys.stderr)
        commit_cmd = ["commit"] + amend_args + tail
        mode = "index"
        committed_paths = staged

    p = subprocess.run(
        ["git", "-C", root, *commit_cmd],
        env={**os.environ, "DCOMMIT_INVOCATION": "1"},
    )
    if p.returncode != 0:
        print(f"dcommit: git commit exited {p.returncode}; nothing ledgered", file=sys.stderr)
        sys.exit(p.returncode)

    new_hash = git_or_refuse(root, "rev-parse", "HEAD", what="post-commit HEAD read").strip()
    row = {
        "ts": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "session": session,
        "mode": mode,
        "hash": new_hash,
        "amended_from": amended_from,
        "locks": locks,
        "paths": committed_paths,
    }
    append_ledger(root, row)
    print(
        f"dcommit: OK {new_hash[:12]} session={session} mode={mode} "
        f"paths={len(committed_paths)} ledger_row=appended"
    )
    sys.exit(0)


if __name__ == "__main__":
    main()
