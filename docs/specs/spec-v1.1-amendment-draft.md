# SPEC v1.1 — proposed amendment (DRAFT, for ratification at the 0.1.0 RC)

Not in force. SPEC.md changes only by Bessa's explicit decision (its own
"Sources of truth" rule 1). This file is the diff to ratify alongside v1.0,
so the contract describes the project that exists. Each item cites the
ruling or artifact it reconciles.

## A1. Banner: replace "no implementation work beyond Wave-0"

The v1.0 banner gates all implementation on ratification. Every wave since
ran under Bessa's standing directive (2026-10-12, "do NOT wait on me…
work autonomously", docs/loop/loop-prompt.md §0). Proposed banner:

> Ratified scope contract. Execution order lives in docs/ROADMAP.md (the
> plan of record); the Waves table below is the scope mapping, not the
> schedule.

## A2. Non-goals: the .spp line

v1.0: ".spp is undocumented HDF5" (no exception). Ruling 1 of 2026-10-12
(docs/loop/decisions.md, BINDING) ratified IMPORT; it landed in wave 3
(`30b966e` + `0a5c20d`). Proposed:

> **No Adobe format execution or writing, ever.** .sbsar never runs.
> .spp is never written; user-provided .spp files may be IMPORTED
> (clean-room, from published community HDF5-layout docs; per-project
> import report; geometry stays FBX/USD).

Constraints line "No Adobe SDKs, no .sbsar/.spp execution ever" stays as
written ("execution" already excludes reading).

## A3. Waves table: mark it the scope mapping

Add under the table: "Status per wave lives in docs/ROADMAP.md." Two rows
moved since v1.0, named so ratification sees them:

- W1 stylus: v1.0 names the raw Windows Ink path as the first backend.
  Today the shipped, verified path is Linux XI2 + raw pressure (ROADMAP
  scoreboard); a Windows Ink module exists with its live-HWND
  integration still an open item, and the Wintab half of the dual path
  is open (ROADMAP wave 5, `wave-stylus-win32`). To confirm at the RC:
  which Windows path is actually live.
- W4 per-layer pixels: delivered by the per-layer pool arc (stages 1a–5,
  2026-10-14), not the v1 destructive-publish path.

## A4. Acceptance criteria: walk them at the RC, don't pre-tick

All 15 boxes are unchecked in v1.0. The RC walkthrough
(docs/specs/release-checklist-0.1.0.md) ticks each with its evidence
(a CI run id, a golden, a protocol log). Evidence already known:

- Workspace build + tests + clippy, both OSes: CI run 38628118144
  (2026-10-14, green).
- Stylus input-to-photon: stage-3 baseline p95 10.19 ms (budget 20 ms;
  local lavapipe — the [QA] reference-rig run is still owed).

Everything else is verified at the walkthrough, not asserted here.

## A5. Unchanged (confirm only)

One-sentence goal, the rest of the non-goals, deliverables 1–7,
constraints, risks, sustainability. The sustainability section's "human
bus factor of one" is still true.
