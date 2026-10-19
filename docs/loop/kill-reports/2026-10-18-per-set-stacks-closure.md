# Kill report: per-set stacks design closure (2026-10-18)

Dragon-authored host-free by the Umber /loop session "Work on Umber
project coordination #3" (local-d3deea1d3c7631cc80f8782e73c37fe384c4c50d1248d89def19e4801b4c9a45, under Bessa's live steer
"do all the work that you can locally, keep developing the project").
Kill-lock `per-set-stacks-closure`, holding exactly one file:
`docs/specs/per-set-stacks-design.md`.

Context: the third stale-status closure of the pass (after pcpool and
dynmat). The C2-9 design read "DRAFT for review" while all four of
its prescribed stages plus two follow-ups landed 2026-10-15 — seven
c2-9 kill reports in the tree.

## Built

- Status line: DRAFT → LANDED (all stages + follow-ups, seven
  reports) + CLOSED 2026-10-18 with the closure pointer.
- The tail closure section, every symbol grep-verified the day it was
  written: hazard 1's global-id resolution (`LayerStack::next_id` +
  `from_parts`, umber-core layers.rs:913/:937), hazard 2's per-set
  compositing (`CompositeSnapshot.set_layers` — the design's
  `layers_by_set` under the implementation's reconciling name,
  recorded in the stage-2 report; `rows_for`/`set_of_tile`/
  `all_rows()` including the retire-census union so set switches
  retire nothing), stage 3's per-set document
  (`Document::activate_set`/`active_set`, document.rs:321/:348), and
  stage 4's panel/docs landing (the layers_ui stage-4 comment).
- Both open questions recorded as RESOLVED BY THE LANDING: the stroke
  timeline stayed global in v1 (stage-3b's boundary, the doc's own v1
  statement); the Bakes/Export panels needed no stage-2 change — the
  design's prediction held exactly.

## Judgment calls

1. The reconciling rename (`layers_by_set` → `set_layers`) is cited
   to the stage-2 report, not re-litigated here — the landing reports
   remain the authoritative implementation record.
2. The two-hazard analysis is named as the doc's residual authority —
   it is still the reference for WHY ids are global and WHY the
   composite unions.

## Unverified

- Nothing compile-shaped: a doc-only kill; every cited symbol
  grep-verified live (see the focused verifier run in this session's
  evidence).

## Gates run (host-free, this session)

- `scripts/verify-kill.sh`: PASS (trap scan, doc-attachment, lock
  audit — the `per-set-stacks-closure` lock live).
- Focused verifier: status bump, closure tail, all live anchors
  (from_parts, next_id, set_layers, set_of_tile, activate_set,
  active_set), the seven landing reports present, the manifest pins.

State: VERIFIED (host-free by nature), IN FLIGHT — commits at the
recovery via its manifest; lock release at that commit.
