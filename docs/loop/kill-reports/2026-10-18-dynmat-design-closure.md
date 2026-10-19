# Kill report: dynamic materials design closure (2026-10-18)

Dragon-authored host-free by the Umber /loop session "Work on Umber
project coordination #3" (local-d3deea1d3c7631cc80f8782e73c37fe384c4c50d1248d89def19e4801b4c9a45, under Bessa's live steer
"do all the work that you can locally, keep developing the project").
Kill-lock `dynmat-design-closure`, holding exactly one file:
`docs/specs/dynamic-materials-design.md`.

Context: the second stale-status closure of the pass (the pattern
proved on the per-channel tile pool minutes earlier). The doc read
"READY TO IMPLEMENT (r2; the two implementation kills derive from
this doc)" — while BOTH derivation kills shipped 2026-10-13/14 and
their symbols live throughout the tree. A design doc claiming work
is pending when it is finished is the same class of lie as a stale
comment: the next session that trusts it re-derives landed work.

## Built

- Status line: READY TO IMPLEMENT → LANDED (Kill A 10-13, Kill B
  10-14) + CLOSED 2026-10-18 with the closure pointer.
- The tail closure section, every symbol grep-verified the day it was
  written: Kill A's surface (LayerKind::Material, the MaterialParams
  family, SetMaterial command + journaling, material_defs_mtlx carry,
  umber-graph's material_output nodedef + register_material_nodes +
  full_registry) and Kill B's (the document row arm + both catalogs'
  keys — en-US.ftl:539/:634 —, HistoryKind::SetMaterial,
  graph_panel's delegation, material_layer.rs, the panel's
  material_section).
- The STILL-OPEN record kept honest: the per-layer param editor
  remains CUT BY DESIGN (blocked on new i18n keys under the stencil
  lock — a live constraint, not staleness); the per-channel config
  checks and never-crash badge paths hold as specified.
- Anchor drift named as positional only (pre-extraction paths);
  no section falsified; the two kill reports stay the authoritative
  implementation record.

## Judgment calls

1. Closure cites the landing reports rather than re-deriving their
   diffs — the reports exist and are pinned by their own kills.
2. The per-layer param editor is recorded as design-open, NOT
   doc-stale — the closure must not read as closing the cut-list.

## Unverified

- Nothing compile-shaped: a doc-only kill; every cited symbol was
  grep-verified live (see the standing verifier).

## Gates run (host-free, this session)

- `scripts/verify-kill.sh`: PASS (trap scan, doc-attachment, lock
  audit — the `dynmat-design-closure` lock live).
- Standing verifier `hermes-verify-dynmat-closure.py`: every closure
  claim re-checked against the live tree + the manifest pins.

State: VERIFIED (host-free by nature), IN FLIGHT — commits at the
recovery via its manifest; lock release at that commit.
