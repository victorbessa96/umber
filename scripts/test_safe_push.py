#!/usr/bin/env python3
"""Scratch-repo tests for scripts/safe-push.py (stdlib only).

Builds a bare "origin" plus a clone with stub gate scripts (scorecard,
check-layers) and a stub `cargo` on PATH, then exercises each rule:

  * clean push of a ledgered own commit is allowed and lands on origin;
  * a sibling session's ledgered commit in the range is refused [ledger];
  * a commit made outside dcommit (no ledger row) is refused [ledger];
  * a non-fast-forward HEAD is refused [ff];
  * a failing fmt / scorecard / layers gate is refused [fmt|score|layers];
  * no session identity is refused [ledger];
  * --dry-run with everything clean pushes nothing.

Run: python3 scripts/test_safe_push.py   (exit 0 = all pass)
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile

GUARD = os.path.join(os.path.dirname(os.path.abspath(__file__)), "safe-push.py")
ME, SIB = "session-me", "session-sibling"


def git(cwd, *args):
    r = subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"git {args}: {r.stderr}")
    return r.stdout.strip()


class Repo:
    def __init__(self):
        self.base = tempfile.mkdtemp(prefix="safe-push-test-")
        self.origin = os.path.join(self.base, "origin.git")
        self.tree = os.path.join(self.base, "tree")
        self.bin = os.path.join(self.base, "bin")
        os.makedirs(self.bin)
        self.cargo_codes = {}
        git(self.base, "init", "-q", "--bare", "-b", "master", self.origin)
        git(self.base, "clone", "-q", self.origin, self.tree)
        git(self.tree, "config", "user.email", "t@t")
        git(self.tree, "config", "user.name", "t")
        os.makedirs(os.path.join(self.tree, "scripts"))
        os.makedirs(os.path.join(self.tree, ".dragon"))
        self.set_gate("fmt", 0)
        self.set_gate("score", 0)
        self.set_gate("layers", 0)
        self.set_gate("clippy", 0)
        self.set_gate("test", 0)
        # Three crates: core <- paint <- app (app also on core), plus gui
        # standalone, so the reverse-dependency closure is observable.
        for d, deps in (
            ("umber-core", []),
            ("umber-paint", ["umber-core"]),
            ("umber-app", ["umber-paint", "umber-core"]),
            ("umber-gui", []),
        ):
            os.makedirs(os.path.join(self.tree, "crates", d, "src"))
            lines = [f'[package]\nname = "{d}"\n\n[dependencies]']
            lines += [f"{x} = {{ workspace = true }}" for x in deps]
            self.write(f"crates/{d}/Cargo.toml", "\n".join(lines) + "\n")
            self.write(f"crates/{d}/src/lib.rs", "\n")
        shutil.copy(
            os.path.join(os.path.dirname(GUARD), "ci-classify.py"),
            os.path.join(self.tree, "scripts", "ci-classify.py"),
        )
        self.write("README", "base\n")
        git(self.tree, "add", "-A")
        git(self.tree, "commit", "-q", "-m", "base")
        git(self.tree, "push", "-q", "origin", "HEAD:master")

    def set_gate(self, gate, code):
        """Stub one gate: exit `code` (layers prints GREEN only on 0).

        fmt / clippy / test share the stub `cargo` on PATH: each
        subcommand reads its own exit code, and every call is logged to
        bin/cargo.log so a case can see which crates were gated."""
        if gate in ("fmt", "clippy", "test"):
            self.cargo_codes[gate] = code
            path = os.path.join(self.bin, "cargo")
            log = os.path.join(self.bin, "cargo.log")
            c = self.cargo_codes
            body = (
                "#!/bin/sh\n"
                f'echo "$@" >> {log}\n'
                'case "$1" in\n'
                f"  fmt) exit {c.get('fmt', 0)} ;;\n"
                f"  clippy) exit {c.get('clippy', 0)} ;;\n"
                f"  test) exit {c.get('test', 0)} ;;\n"
                "esac\nexit 0\n"
            )
        elif gate == "score":
            path = os.path.join(self.tree, "scripts", "scorecard.py")
            body = f"import sys\nsys.exit({code})\n"
        else:
            path = os.path.join(self.tree, "scripts", "check-layers.py")
            word = "GREEN" if code == 0 else "RED"
            body = f"print('=== layers: {word} ===')\nimport sys\nsys.exit({code})\n"
        with open(path, "w") as fh:
            fh.write(body)
        os.chmod(path, 0o755)

    def write(self, name, text):
        with open(os.path.join(self.tree, name), "w") as fh:
            fh.write(text)

    def cargo_calls(self):
        path = os.path.join(self.bin, "cargo.log")
        if not os.path.exists(path):
            return []
        with open(path) as fh:
            return [ln.split() for ln in fh.read().splitlines()]

    def gated_crates(self, sub):
        """The -p crates of the last `cargo <sub>` call ([] = never ran)."""
        calls = [c for c in self.cargo_calls() if c and c[0] == sub]
        if not calls:
            return []
        args = calls[-1]
        return sorted(args[i + 1] for i, a in enumerate(args) if a == "-p")

    def commit(self, name, session=None):
        """Commit one file; ledger it under `session` (None = outside dcommit)."""
        os.makedirs(os.path.dirname(os.path.join(self.tree, name)) or self.tree, exist_ok=True)
        self.write(name, name + "\n")
        git(self.tree, "add", name)
        git(self.tree, "commit", "-q", "-m", f"add {name}")
        h = git(self.tree, "rev-parse", "HEAD")
        if session:
            with open(os.path.join(self.tree, ".dragon", "commit-ledger.jsonl"), "a") as fh:
                fh.write(json.dumps({"session": session, "hash": h}) + "\n")
        return h

    def guard(self, *args, session: "str | None" = ME):
        env = dict(os.environ)
        env["PATH"] = self.bin + os.pathsep + env["PATH"]
        env.pop("DRAGON_SESSION", None)
        # A private lock: a sibling's real cargo must not stall the stubs.
        env["UMBER_CARGO_LOCK"] = os.path.join(self.base, "cargo.lock")
        env.pop("HERMES_SESSION_ID", None)
        cmd = [sys.executable, GUARD, *args]
        if session:
            cmd += ["--session", session]
        r = subprocess.run(cmd, cwd=self.tree, capture_output=True, text=True, env=env)
        return r.returncode, r.stdout + r.stderr

    def origin_head(self):
        return git(self.origin, "rev-parse", "master")

    def close(self):
        shutil.rmtree(self.base, ignore_errors=True)


CASES = []


def case(fn):
    CASES.append(fn)
    return fn


def expect(cond, msg):
    if not cond:
        raise AssertionError(msg)


@case
def clean_own_commit_pushes():
    r = Repo()
    try:
        h = r.commit("a", ME)
        code, out = r.guard()
        expect(code == 0, f"exit {code}: {out}")
        expect(r.origin_head() == h, "origin did not move to HEAD")
    finally:
        r.close()


@case
def sibling_commit_refused():
    r = Repo()
    try:
        before = r.origin_head()
        r.commit("sib", SIB)
        r.commit("mine", ME)
        code, out = r.guard()
        expect(code == 2 and "[ledger]" in out and SIB in out, f"exit {code}: {out}")
        expect(r.origin_head() == before, "refused push still moved origin")
    finally:
        r.close()


@case
def unledgered_commit_refused():
    r = Repo()
    try:
        r.commit("raw", None)
        code, out = r.guard()
        expect(code == 2 and "not in the dcommit ledger" in out, f"exit {code}: {out}")
    finally:
        r.close()


@case
def two_own_sessions_allowed():
    r = Repo()
    try:
        r.commit("a", ME)
        h = r.commit("b", "session-me-after-compaction")
        code, out = r.guard("--session", "session-me-after-compaction")
        expect(code == 0, f"exit {code}: {out}")
        expect(r.origin_head() == h, "origin did not move")
    finally:
        r.close()


@case
def non_fast_forward_refused():
    r = Repo()
    try:
        # Someone else advances origin; our HEAD diverges from it.
        other = os.path.join(r.base, "other")
        git(r.base, "clone", "-q", r.origin, other)
        git(other, "config", "user.email", "o@o")
        git(other, "config", "user.name", "o")
        with open(os.path.join(other, "x"), "w") as fh:
            fh.write("x\n")
        git(other, "add", "x")
        git(other, "commit", "-q", "-m", "other")
        git(other, "push", "-q", "origin", "HEAD:master")
        r.commit("mine", ME)
        code, out = r.guard()
        expect(code == 2 and "[ff]" in out, f"exit {code}: {out}")
    finally:
        r.close()


@case
def failing_gates_refused():
    for gate in ("fmt", "score", "layers", "clippy", "test"):
        r = Repo()
        try:
            # The gates read the pushed commit's tree, so a failing gate
            # script must be IN the commit (cargo's stub is on PATH). A
            # crate file rides so clippy/test have something to gate.
            r.set_gate(gate, 1)
            r.write("crates/umber-gui/src/lib.rs", f"// {gate}\n")
            git(r.tree, "add", "-A")
            git(r.tree, "commit", "-q", "--allow-empty", "-m", f"break {gate}")
            h = git(r.tree, "rev-parse", "HEAD")
            with open(os.path.join(r.tree, ".dragon", "commit-ledger.jsonl"), "a") as fh:
                fh.write(json.dumps({"session": ME, "hash": h}) + "\n")
            before = r.origin_head()
            code, out = r.guard()
            expect(code == 2 and f"[{gate}]" in out, f"{gate}: exit {code}: {out}")
            expect(r.origin_head() == before, f"{gate}: refused push moved origin")
        finally:
            r.close()


@case
def rebased_own_commit_keeps_ledger_standing():
    r = Repo()
    try:
        r.commit("mine", ME)
        other = os.path.join(r.base, "other")
        git(r.base, "clone", "-q", r.origin, other)
        git(other, "config", "user.email", "o@o")
        git(other, "config", "user.name", "o")
        with open(os.path.join(other, "x"), "w") as fh:
            fh.write("x\n")
        git(other, "add", "x")
        git(other, "commit", "-q", "-m", "other")
        git(other, "push", "-q", "origin", "HEAD:master")
        git(r.tree, "fetch", "-q", "origin")
        git(r.tree, "rebase", "-q", "origin/master")
        code, out = r.guard()
        expect(code == 0, f"rebased own commit refused: exit {code}: {out}")
    finally:
        r.close()


@case
def rebased_sibling_commit_still_refused():
    r = Repo()
    try:
        r.commit("sib", SIB)
        git(r.tree, "commit", "-q", "--amend", "-m", "sib reworded")
        code, out = r.guard()
        expect(code == 2 and "[ledger]" in out, f"exit {code}: {out}")
    finally:
        r.close()


@case
def gates_read_the_pushed_commit_not_the_dirty_tree():
    r = Repo()
    try:
        h = r.commit("a", ME)
        # Uncommitted WIP that breaks the layers gate in the working tree
        # must not refuse a push whose COMMIT is clean.
        with open(os.path.join(r.tree, "scripts", "check-layers.py"), "w") as fh:
            fh.write("print('RED')\nimport sys\nsys.exit(1)\n")
        code, out = r.guard()
        expect(code == 0, f"dirty tree refused a clean commit: exit {code}: {out}")
        expect(r.origin_head() == h, "origin did not move")
    finally:
        r.close()


@case
def a_commit_landing_mid_check_never_rides():
    r = Repo()
    try:
        h = r.commit("a", ME)
        # The stub layers gate commits a sibling's work while the guard
        # runs; the guard must push the SHA it checked, not the new HEAD.
        hook = (
            "import subprocess, sys\n"
            f"t = {r.tree!r}\n"
            "open(t + '/late', 'w').write('late')\n"
            "subprocess.run(['git', '-C', t, 'add', 'late'])\n"
            "subprocess.run(['git', '-C', t, 'commit', '-q', '-m', 'late sibling'])\n"
            "print('=== layers: GREEN ===')\n"
        )
        path = os.path.join(r.tree, "scripts", "check-layers.py")
        with open(path, "w") as fh:
            fh.write(hook)
        git(r.tree, "add", path)
        git(r.tree, "commit", "-q", "--amend", "--no-edit")
        h = git(r.tree, "rev-parse", "HEAD")
        with open(os.path.join(r.tree, ".dragon", "commit-ledger.jsonl"), "a") as fh:
            fh.write(json.dumps({"session": ME, "hash": h}) + "\n")
        code, out = r.guard()
        expect(code == 0, f"exit {code}: {out}")
        expect(r.origin_head() == h, "pushed something other than the checked SHA")
    finally:
        r.close()


@case
def a_core_change_gates_core_and_every_dependent():
    r = Repo()
    try:
        r.commit("crates/umber-core/src/x.rs", ME)
        code, out = r.guard()
        expect(code == 0, f"exit {code}: {out}")
        want = ["umber-app", "umber-core", "umber-paint"]
        expect(r.gated_crates("clippy") == want, f"clippy on {r.gated_crates('clippy')}")
        expect(r.gated_crates("test") == want, f"test on {r.gated_crates('test')}")
    finally:
        r.close()


@case
def a_leaf_change_gates_only_the_leaf():
    r = Repo()
    try:
        r.commit("crates/umber-gui/src/x.rs", ME)
        code, out = r.guard()
        expect(code == 0, f"exit {code}: {out}")
        expect(r.gated_crates("test") == ["umber-gui"], f"test on {r.gated_crates('test')}")
    finally:
        r.close()


@case
def a_docs_or_scripts_change_skips_the_cargo_gates():
    r = Repo()
    try:
        r.commit("docs/note.md", ME)
        r.commit("scripts/tool.py", ME)
        code, out = r.guard()
        expect(code == 0 and "gates skipped" in out, f"exit {code}: {out}")
        expect(not r.gated_crates("clippy") and not r.gated_crates("test"), str(r.cargo_calls()))
    finally:
        r.close()


@case
def a_lockfile_or_unknown_path_gates_every_crate():
    for path in ("Cargo.lock", "build-helpers/gen.txt"):
        r = Repo()
        try:
            r.commit(path, ME)
            code, out = r.guard()
            expect(code == 0, f"{path}: exit {code}: {out}")
            want = ["umber-app", "umber-core", "umber-gui", "umber-paint"]
            expect(r.gated_crates("test") == want, f"{path}: test on {r.gated_crates('test')}")
        finally:
            r.close()


@case
def no_session_refused():
    r = Repo()
    try:
        r.commit("a", ME)
        code, out = r.guard(session=None)
        expect(code == 2 and "no session identity" in out, f"exit {code}: {out}")
    finally:
        r.close()


@case
def dry_run_pushes_nothing():
    r = Repo()
    try:
        before = r.origin_head()
        r.commit("a", ME)
        code, out = r.guard("--dry-run")
        expect(code == 0 and "dry run" in out, f"exit {code}: {out}")
        expect(r.origin_head() == before, "dry run moved origin")
    finally:
        r.close()


def main() -> int:
    failed = 0
    for fn in CASES:
        try:
            fn()
            print(f"ok   {fn.__name__}")
        except AssertionError as e:
            failed += 1
            print(f"FAIL {fn.__name__}: {e}")
    print(f"{len(CASES) - failed}/{len(CASES)} passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
