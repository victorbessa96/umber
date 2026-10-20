# Kill report — dcommit-pre-commit-hook (the last M3 tail)

Date: 2026-10-20. Author: dragon (local-10e6ed18). Lock:
`dcommit-pre-commit-hook` (.dragon/dcommit.py, scripts/hooks/pre-commit,
this report). Doc/tooling only, battery-exempt; no cargo.

## Built

1. `scripts/hooks/pre-commit` — the tracked enforcement sample: a
   bare `git commit` on a clone with the hook installed refuses with
   the dcommit usage + the interlock rationale, unless
   DCOMMIT_INVOCATION=1 is in the environment (the stamp dcommit
   sets on its own commit subprocess). The human override is
   deliberate and documented IN the refusal: `git commit
   --no-verify` — the partner can always commit by hand; the hook
   catches agent forgetfulness, never gates the partner.
2. `.dragon/dcommit.py` (untracked agent tooling): the one-line
   DCOMMIT_INVOCATION=1 env stamp on the commit subprocess — the
   deterministic signal the hook reads. No behavior change in any
   dcommit gate path (py_compile clean; every refusal leg
   re-proven).
3. Installed on the shared tree: `.git/hooks/pre-commit`
   (byte-identical to the sample, executable). .git is never rsynced
   or pushed — per-clone install, one cp, documented in the hook's
   header.

## Verification (scratch-repo battery, 7/7)

- b: a staged bare `git commit` REFUSED, HEAD intact, still 1
  commit.
- c: dcommit index-mode PASSES through the hook (the stamp works),
  ledger row mode=index.
- d: dcommit path-limited PASSES, exactly the lock paths committed,
  ledger row mode=path-limited.
- e: `git commit --no-verify` PASSES (the deliberate override).
- f1: the no-session refusal still fires (env stripped: REFUSED).
- f2: the staged-escapee refusal still fires (REFUSED).
- f-tail: both refusal legs left HEAD untouched (4 commits).
- `bash -n` + `py_compile` clean; the installed hook `cmp`-identical
  to the sample.

## Judgment calls

- The hook guards the LOCAL clone only (.git is per-clone) — the
  right scope: the collisions it encodes (the 19:3x index sweep, the
  14:5x amend fold) are local-agent actions. The host's clone needs
  its own install only if it ever becomes an agent seat.
- `--no-verify` stays as the override rather than an env escape: it
  is the git-native, human-shaped gesture, and it leaves a visible
  marker in the command history — an agent using it is seen, the
  partner using it is sovereign.
- dcommit.py takes the env stamp (not a parent-process walk): the
  stamp is deterministic, inherited by git and the hook, and one
  line; a PPID hop is fragile across shells and wrappers.

## Unverified

- Nothing runtime-hidden: every leg ran live on the scratch repo.
  The shared-tree install is a byte-identical cp of the
  battery-tested file.

## Deferred

- The host-side install (only if centralhost ever becomes an agent
  seat — its clone's .git is host-local).
- M1 pass 2; the umber-wave-ops skill split (105.9k over its 100k
  cap) — the other open lanes.

## Gates

- Doc/tooling only, battery-exempt, no cargo; rides the recovery
  push. The scratch battery IS the verification — and this lane's
  own commit runs the gate live: dcommit committing through the
  newly installed hook is the final dogfood leg.
