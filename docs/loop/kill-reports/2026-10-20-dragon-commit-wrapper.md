# Kill report — dragon-commit-wrapper (M3): dcommit

- Date: 2026-10-20 (02:5x local), session local-f15cdefa (wake-13383)
- Lane: M3 of the PARTNER STEER 23:52 meta-hunt set (Bessa: "continue the
  autonomous work locally, keep improving both the project and the meta hunt
  there, and don't stop")
- Envelope held: host-free, no cargo, no battery, no push; doc/tooling only,
  battery-exempt. Claimed in .dragon/COORDINATION-next-wave.md before build.

## The incidents it encodes (why this is machinery, not a convention note)

1. **The index sweep (2026-10-19 19:3x).** Two sibling-staged cleanups (the
   LANDING_NOTES duplicate + the agent-contract draft deletion) rode the
   flatpak-strip commit 4d69808 because `git commit` takes the whole index
   and nobody checked the index before committing. Lesson-memory ("check the
   index next time") decays; an interlock does not.
2. **The amend fold (2026-10-19 14:5x).** A `git commit --amend` for a
   baseline.json precision fixup fired seconds after the sibling's
   tile-restage commit landed and rewrote dcc5e89 into a phantom 58d2553.
   Restored by soft-reset, but the class (amend without proving HEAD is
   your own) was only closed by a promise, not by a gate.

Both incidents share a root: all sessions share one git identity, so git
authorship proves nothing about WHICH session owns a commit, and the
default `git commit` semantics are optimized for a single actor.

## The gate

`.dragon/dcommit.py` (python3 stdlib + git only). Usage:
`.dragon/dcommit.py [--session ID] [--lock PATH]... [--amend] [--path-limited] -- <git commit args>`

Interlocks, all fail-closed (exit 2, tree untouched):

- **Session identity**: `--session` > `$DRAGON_SESSION` > `$HERMES_SESSION_ID`;
  none resolvable -> refuse. Unattributable commits cannot be gated.
- **Staged-set lock check** (index mode): every staged file must sit inside a
  declared `--lock` path (file or directory prefix); escapees are named in the
  refusal. Nothing staged -> refuse. This is the 19:3x interlock.
- **Amend ownership**: `--amend` (flag or in-tail) requires the commit ledger
  to prove HEAD's hash was committed by THIS session. Unknown HEAD -> refuse
  with the banked rule quoted; foreign session -> refuse naming the owner.
  This is the 14:5x interlock. The ledger (`.dragon/commit-ledger.jsonl`,
  gitignored, born at first use) is the ownership oracle — git identity
  cannot serve.
- **--path-limited mode**: runs `git commit <args> -- <locks>`, the 19:3x
  lesson's own recommended form; commits only the lock paths even with a
  dirty mixed index.
- **Editor guard**: no -m/-F/-C/-c -> refuse (no interactive editor in agent
  context).
- **Pathspec guard**: a bare `--` in the commit args -> refuse (locks are the
  pathspec).
- **Repo guard**: lock paths outside the repo -> refuse; cwd not a git tree ->
  refuse.

Every gated commit appends one JSON row (ts, session, mode, hash,
amended_from, locks, paths) to the ledger; ownership lookups read it back.
Corrupt ledger lines never grant ownership.

## Verification — 36-leg scratch battery, ALL GREEN

Scratch repos only; the live tree was never a test subject. Legs:

- T1 index commit inside lock -> OK, ledger row 1.
- T2/T3 staged escape (sibling file / nonexistent lock) -> refused, escapee
  named, HEAD byte-identical before/after.
- T4 nothing staged -> refused.
- T5 amend OWN ledger-proven commit -> allowed; new hash; row 2 with
  amended_from == row 1 hash.
- T6 amend as a different session -> refused, owner named, HEAD untouched.
- T7 amend on a plain-git HEAD (not in ledger) -> refused with the banked
  rule, HEAD untouched.
- T8 no session identity (env scrubbed) -> refused.
- T9 no message flag -> refused (editor guard).
- T10 path-limited with a dirty mixed index -> committed exactly the locked
  path; sibling files stayed staged.
- T11 lock outside the repo -> refused.
- T13 bare `--` pathspec -> refused.
- T12 non-repo cwd -> refused (GIT_CEILING_DIRECTORIES used to make the leg
  honest: the scratch dir sits inside the ~/.hermes repo).
- Ledger: 3 rows, valid JSONL, all fields present, amend chain correct.

Two real defects were caught and fixed by the battery itself: `--amend`
before the `--` separator was eaten by argparse (now a first-class
wrapper flag, still detected in-tail), and an unreachable "disjoint
refusal" branch (removed — if no staged file escapes, all are covered).

## Dogfood

This commit is the wrapper's first real use: report + wrapper + .gitignore
committed through dcommit with three declared locks. The ledger's first row
is this commit.

## Non-coverage (named, not hand-waved)

- The gate is file-granularity: it cannot detect mixed-authorship hunks
  INSIDE one file. That class (e.g. the kill-ledger currently carrying the
  sibling's uncommitted rows) stays with the coordination-file protocol —
  which is why this kill's own ledger row was appended worktree-side instead
  of committed here.
- The ledger proves ownership only for commits made THROUGH the wrapper;
  all pre-existing commits are "unknown HEAD" -> amend refused (correct
  fail-closed posture, but it means the first session to amend an old
  commit must do it in plain git deliberately, against the banked rule).
- No Windows/`git.exe` path-separator handling (agent tooling; Linux-only
  sessions).
- Not wired into any hook: adoption is convention + the coordination notes;
  a future lane could add a pre-commit hook sample.
