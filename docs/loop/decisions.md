# Umber Decisions Log — standing rulings and their reasoning

The loop's rulings, one entry each, newest first. A decision lands here when
it changes standing behavior (not per-kill choices, which live in kill
reports). Bessa's overrides are BINDING and marked.

## 2026-10-12 ~23:20 — The five-ruling alignment pass (ALL BINDING, Bessa)

1. **.spp import RATIFIED, pulled to wave 3 BEFORE PSD import** — the
   pull-users-from-Substance story matters most; scope unchanged
   (import-only forever, clean-room from published community HDF5-layout
   docs, per-project import report, geometry stays FBX/USD, writing .spp
   stays the hard wall).
2. **Claude Code RATIONED**: design reviews + the biggest architectural
   kills only; opencode for everything else. (Today burned ~$22 of the
   Pro sub and hit the weekly wall mid-kill; the wall resets 2am and the
   tier is a budget, not a default.)
3. **Waves run 24/7** — the flock + one-cargo rule already protects the
   machine; workdays do NOT pause the loop. **The work window
   extends to Friday 23:59** (the "until midnight" scope is now "until
   Friday 23:59").
4. **Perf baseline seeded LOCALLY** — this machine's lavapipe produces
   the 4K-path numbers now; baseline.json commits and CI compares
   thereafter. The starved nightly queue stops being the blocker.
5. **Wave-3 priorities**: .spp import (per ruling 1), PSD import, then
   dynamic material layering (design-doc first, likely 2 kills). The
   stylus-Win32 dual-path and 8K export drop below the cut.

## 2026-10-12 — Cargo ONLY at the end of the wave (BINDING, Bessa)

Per-kill cargo gating is abolished. The wave protocol: claws write in
isolation (worktrees) with ZERO cargo — no fmt, no clippy, no tests mid-wave,
not even per-kill at consumption; the dragon consumes each kill's artifacts
(read, brace-balance, verify-kill's zero-cargo legs) and stages the branches,
then runs ONE full gate battery at the END of the wave: fmt once, clippy once
per touched crate, the touched crates' test suites once, then lands in order.
CI remains the per-branch suite verdict afterward. Reasoning: per-kill cargo
was the last serial bottleneck (each gate = a cold-ish build + minutes on the
locked flock); with worktrees there is no correctness reason to gate
serially — branches can't contaminate each other, and the one battery
catches the same defect classes at a fraction of the wall-clock. The kill's
OWN tests still run in the end-of-wave battery (the briefs keep writing
them; they just don't run until the wave closes). The exception: a kill the
dragon lands SOLO (no wave in flight) may gate itself — the rule binds
WAVES, not single kills.

## 2026-10-12 — .spp IMPORT: in scope for wave 4, pending Bessa's ratification

Earlier framing ("never touch .spp") reversed for the IMPORT direction.
Reasoning: writing .spp stays the wall (undocumented HDF5 + Adobe's format),
but READING a user-provided file is interoperability — the same standing
ufbx has against FBX, and the pull-user-from-Substance story the march
wants. Scope: import-only (no writing ever), no Adobe code/spec-derived
tables (clean-room from published community reverse-engineering of the
HDF5 layout, best-effort with a per-project import report naming what
survived), geometry stays FBX/USD-side. RATIFIER: Bessa — his legal risk
call as the shipper. Slots wave 4, after PSD-import (shares the flatten
machinery with blends).

## 2026-10-12 — Big-wave dispatch: the standing cadence

Bessa's directive: feature-parity with Substance Painter, waves not serial
kills. Protocol: dispatch 4-5 claws in per-kill worktrees (staggered 60s+
launches — the opencode DB lock kills simultaneous starts), dragon gates
each branch under the flock, fast-forward land, CI per branch. The gates
stay (they caught 10+ real defects on core-loop day); the SERIALIZATION
died. ROADMAP.md = plan of record; requirements.md = contract of record.

## 2026-10-12 — Cargo discipline: unchanged and LOAD-BEARING in waves

One cargo at a time under the flock even with parallel worktrees — Bessa's
stacked-cargo crash rule. CI reads are the default verification; local
cargo only when a wave-branch's gate needs it. The wave does not loosen
this; kill-worktree.sh enforces it.

## 2026-10-12 — Doc-deltas replace claw-edited shared docs

Claws never edit requirements.md / CHANGELOG.md / .ftl / queue.md. Deltas
ride the kill report; land-kill.sh --doc-deltas applies them (locator must
hit exactly one line; ftl deltas are new-keys-only, leaf-key matching). The
claw-seat review's collision fix, live-verified both directions.

## 2026-10-12 — Kill-locks: advisory under worktrees, kept for merge planning

kill-worktree.sh made collisions impossible by construction; locks stay
because the QUEUE of planned merges still needs the surface registry (which
branches touch which files = the rebase order). verify-kill's lock-audit
leg enforces honesty at the gate.
