# Brief — 2026-11-14 — wave-replay-exact decision prep (NOT a derivation)

Status: **prep only**. The wave plan gates this lane as a UX call: "default-off
until Bessa decides". This brief exists so the decision is one read, not a
research session. No code, no queue row dispatch.

## The question for Bessa

When a user records a path (stroke) and replays it, should the replay paint
pixel-exactly like the original stroke (deterministic re-execution), or is a
faithful-but-not-byte-identical replay acceptable as the default?

## The trade (measured state, 2026-11-14)

| Axis | replay-exact ON (default) | replay-exact OFF (default, status quo) |
| --- | --- | --- |
| What it costs | every paint primitive must be deterministic under replay: seeded RNG per stroke, fixed iteration order, no time/hashmap-order dependence, float-op-order stability. Audit surface: `umber-paint` stroke eval + `umber-core` journal replay. | nothing to build now. |
| What it buys | undo/redo and journal replay are byte-stable (crash recovery repaints identically); diff-based tests on paint output; user trust ("replay" means the same thing everywhere). | smaller audit; replays are allowed to drift slightly. |
| Risk if ON is default | any nondeterminism found is a correctness bug in disguise; the fuzz target + stroke-level golden tests become the enforcement. | drift accumulates silently; undo/redo can diverge from what the user saw. |
| Precedent in-tree | `journal::tests::set_geometry_mask_replays_after_crash` (geomask brief 2026-11-12) already asserts exact journal replay for one command class; the fuzz corpus (row 156-A, `fc8b9ce8`) covers importer determinism, not paint determinism. | — |

## What "ON" would derive (only if Bessa says yes)

1. A determinism audit of `umber-paint` stroke evaluation: RNG seeding,
   iteration order, float accumulation order. Kill surface:
   `crates/umber-paint/src/**` + golden stroke tests.
2. A `replay_exact` project-format flag (default per the decision), versioned
   so old projects keep their behavior.
3. Enforcement: stroke-level golden tests run in CI; the fuzz target gains a
   paint-replay target (same harness as `importers`).

## Recommendation to put in front of Bessa

Default OFF for v1 (the audit is real work and the win is invisible until it
bites), with the audit queued as its own wave so the decision can flip later
without a format break. But it is his call; the lane stays open until he
answers.
