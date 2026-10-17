# Umber full-project audit (2026-10-17): what works, what's missing, what to improve

Date: 2026-10-17 (00:15-00:4x). Author: dragon, solo, every number
re-derived live from the tree at `3442fac` + the uncommitted W-GUI-2
working set. Host (centralhost) down since 18:25 — all cargo-verifiable
claims below cite their last verified state; nothing here pretends the
battery ran tonight.

## The headline

The project is a **feature-broad, well-tested engine whose remaining
risk is concentrated in three places**: verification debt from a 5-hour
build-host outage (13 landed-but-uncompiled kills in W-GUI-2), perf
baseline drift on the nightly (a known stale-baseline red, not a code
regression), and a release lane whose gates are all partner decisions
waiting on him.

## What works well (measured, not asserted)

1. **Test discipline is real and even.** 166,540 LOC across 16 crates
   carries 1,931 unit + 130 integration tests (12.4/kLOC). No crate is
   below 7/kLOC; the GUI/brush/bake/export crates sit at 15-17/kLOC.
   Only five >80-line files in the workspace have no tests, and three
   are legitimately hard targets (WGSL shader sources, a snapshot
   facade, a builder) — recorded below as the honest remainder, not a
   hidden gap.
2. **The loop protocol holds under stress.** The host-outage arc
   landed 13 kills through claws + dragon, every one statically
   verified (APIs resolved against pinned registry sources, Python
   ground-truth ports, blast-radius sweeps), and the cross-kill
   adversarial review caught 11 real defects — including one BLOCKER
   (Quick Setup would have re-opened every launch) and two dead
   wirings — BEFORE any battery ran. The two-gate model, kill locks,
   and the manifest are all functioning as designed.
3. **The security posture is honest.** SECURITY.md carries 10
   threat-surface rows, all with HEAD anchors, zero stale PENDING
   claims. Spot-verified tonight: the WASM consent gate (blake3 hash
   before load, plugins.rs:22), the spp never-crash lint gate
   (deny(unwrap/expect/panic), the one `panic!` in the crate sits
   inside `#[cfg(test)]`), the importer's memory caps and
   decompression-bomb footprint checks.
4. **CI is green where it matters and red only with reasons.**
   W-GUI-1 (5e0d89e) green both platforms; the .spp chunk-key fix
   (ee1a078) green; the only reds in the last 20 runs are the known
   perf-baseline story (below) and two packaging/bakes blips that were
   fixed forward same-day (2d3837e landed 14:52's fix at 14:57's
   push).
5. **The core loop is Gate-0-proven end to end.** FBX → 4K pressured
   paint → compositor → undo/redo byte-exact → sidecar save/reopen
   byte-identical → ORM export decode-verified, on BOTH platforms
   (lavapipe + WARP). The .spp corpus gate opens all three real
   Painter files with content pins (nonzero floors, material names,
   chunk-key fix verified on real bytes).
6. **Docs are unusually alive.** 271 markdown files, and the tracking
   docs get re-derived at wave boundaries — this week's audits found
   and closed stale rows (R6's m3 verification was discovered FULLY
   LANDED AND WIRED while re-planning; the row is now ticked with
   file:line anchors instead of re-hunted).

## What's missing (the real gaps)

1. **Verification debt: 13 uncompiled kills.** The W-GUI-2 working set
   (38 dirty/untracked files, hash-pinned manifest) has never been
   compiled together. ~50 new tests exist only as source. This is the
   single highest-priority item in the repo: the moment the build host
   answers, the runbook in `docs/loop/queue.md` fires (clean verify
   trees → rsync → wave battery over umber-app + umber-gui + spp +
   umber-core → commit-by-path → ONE push).
2. **The perf nightly is red on a stale baseline, and it will stay
   red until W-GUI-2 pushes.** Run 38957356307 (19:46, on 5e0d89e)
   measured viewport_mean 45.54ms / p95 57.24ms against origin's
   baseline 5.49/6.24ms — the exact hardware-class mismatch the reseed
   commit (9acdc1c) was written to close. **9acdc1c is LOCAL,
   unpushed** (it rides W-GUI-2's push per the one-push rule). Every
   nightly until that push reads red. Not a code regression; a
   sequencing consequence of the outage. Recorded here so nobody
   re-triages it blind.
3. **Five files with no tests** (the honest remainder):
   `umber-gpu/src/bake_shaders.rs` + `shaders.rs` (WGSL sources — the
   shader pipeline's coverage is behavioral, through the render tests,
   not per-file), `umber-paint/src/snapshot.rs` (exercised indirectly
   through paint_state), `spp/src/builder.rs` (the synthetic writer —
   tested through every round-trip in lib.rs, none of its own),
   `umber-wasm/src/runtime.rs` (26 integration tests cover the surface).
   Named, not hidden.
4. **The GUI plan's shell-halves queue.** Five landings wait on the
   main.rs lock release (ContextRail dispatch, the a11y app half, the
   density Preferences row, the held-key poll, TypeAhead/dialog
   consumers) plus the g4 curve-widget's consumers were never built
   (falloff Custom + Levels never adopted CurveEdit — census finding).
   W-GUI-3's shape is already determined by what's queued.
5. **The .spp v2 lane is fully corpus-gated.** R1's native-precision
   payload, R2 assbin mesh, R3 resource images, R4 HBO layer-stack
   schema (the flagship), R5 baking configs — all need the corpus on
   the host. R6 was found already done; R7's phase 1 (dense-group
   detect+report) too. Nothing else host-free remains there.
6. **Release lane is 100% partner-gated.** Windows signing (SignPath
   metadata wiring decision), SPEC v1.1 ratification, the dogfood
   asset, the alpha tag, C2-16's real-GPU budget row — all Bessa's
   calls, all documented with the evidence packets he needs. The 0.1.0
   checklist itself is written and waiting.

## What to improve (process, ranked by leverage)

1. **Push the perf-baseline reseed OUTSIDE the wave gate.** The one
   one-push rule exists to stop per-kill pushes; a 2-line baseline file
   with a red nightly screaming behind it is the exception case. Either
   Bessa pushes 9acdc1c himself or the next session folds it into the
   FIRST push of any wave, not the last. (Named as a rule refinement:
   "docs+config-only commits that close a live CI red ride the next
   push of ANY wave, and a red nightly justifies breaking the wave
   gate for that commit alone.")
2. **The two-gate model needs its outage runbook automated.** Tonight
   proved the manual runbook works (queue.md's dated table + manifest);
   the next step is a `scripts/recovery-close.sh` that reads the
   manifest and runs the sequence, so the host's return costs one
   command instead of a careful session.
3. **Register new widget files in the same kill that creates them.**
   type_ahead.rs + dialog_buttons.rs sat unregistered for hours because
   mod.rs was lock-held — the battery would have silently passed over
   dead files. Rule: a new module's registration lines go IN the brief
   of the kill that writes it, lock extension negotiated then, not at
   wave close.
4. **The claw briefs should pin the "verify before claiming" ladder
   explicitly.** Tonight's claws did this well (registry-resolved
   APIs, python ports), but it was brief-by-brief discretion. One
   paragraph in the batch brief template makes it mandatory: rustfmt +
   API resolution + python port for pure logic + the blast-radius
   sweep for enum/struct changes.
5. **Perf: close the "authentic CI numbers" loop.** The reseeded
   baseline comes from a single run's numbers. After the next TWO
   green nightlies, reseed once more from their mean so one machine's
   variance doesn't become the floor (the protocol.md triage row for
   this already exists — scheduled, not forgotten).

## Amendment (2026-10-17 03:30, after the /loop iterations)

The loop's five host-free iterations (01:07-01:31) landed five more
kills after this audit's snapshot: g1-nav-zoom-frame (camera half),
subset-mesh, occluder-scope, psd-publish, and the recovery-close
script. The uncompiled count is **18 kills / 60 manifest rows**, not
13 — the wave table in `docs/loop/queue.md` carries the current
census. The audit's analysis stands; only its snapshot count moved.

## The numbers (audit snapshot, 2026-10-17 00:30)

| measure | value |
|---|---|
| Crates | 16 (+ spp + stylus) |
| LOC (src) | 166,540 |
| Unit tests | 1,931 |
| Integration tests | 130 |
| Test density | 12.4/kLOC (min 7.1, max 30.7) |
| Requirements rows | 100: 90 ✓, 2 ◐, 8 open (3 are perf-budget headers) |
| True open features | dynamic material layering, tessellation preview, user WGSL shaders, stylus dual-path |
| CI (last 20 runs) | 14 green, 6 red (all dispositioned) |
| Uncompiled W-GUI-2 kills | 13 (manifest-pinned, runbook in queue.md) |
| Docs files | 271 |
| SECURITY.md rows | 10, 0 stale |

## Verdict

The engine works, the loop works, the security story works. The
project's constraint is not quality — it is the outage's verification
debt and a release lane whose every remaining gate is a partner
decision. The next session's job is mechanical: fire the runbook,
consume the battery, push once, and hand Bessa the release packet.
