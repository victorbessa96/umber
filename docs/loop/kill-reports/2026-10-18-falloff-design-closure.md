# Kill report: falloff-custom design closure (2026-10-18)

Dragon-authored host-free by the Umber /loop session "Work on Umber
project coordination #3" (local-d3deea1d3c7631cc80f8782e73c37fe384c4c50d1248d89def19e4801b4c9a45, under Bessa's live steer
"do all the work that you can locally … and do not stop"). Kill-lock
`falloff-design-closure`, holding exactly one file:
`docs/specs/falloff-custom-design.md`.

Context: the fourth stale-design closure of the day (after pcpool,
dynmat, per-set-stacks). The falloff-custom design read "DRAFT for
the panel round" — the scout found the ENTIRE design in the tree:
the GPU half (the RwLock LUT + binding 5 + id 10 + set_falloff_lut +
the lazy white fallback), the brush half (the Custom variant + ALL +
eval's LUT arm + BrushParams.falloff_custom), AND the panel round
itself (the combo row's Custom branch + falloff_custom_editor
in-place, the combo key in both catalogs), plus the apply-path
placement and every test the doc planned.

## Built

- Status line: DRAFT → LANDED (engine + panel round) + CLOSED
  2026-10-18 with the closure pointer.
- The tail closure, every symbol grep-verified the day it was
  written: the engine half (FALLOFF_CUSTOM/FALLOFF_LUT_LEN, the
  stamp-mirrored RwLock + AtomicBool fields, set_falloff_lut at
  paint.rs:849, the WGSL binding-5 + case-10 tap), the brush half
  (the unit variant with samples in BrushParams per the Copy
  constraint, custom_tap/custom_index, the serde resolution — the
  doc's "VERIFY at implementation" answered in the enum's own docs:
  old binaries reject the unknown variant), the panel round (the
  in-place editor the doc's open question 3 recommended, the
  brush.falloff-custom key), and the apply path (paint_state's
  set_falloff_lut call at the set_stamp placement).
- All three open questions recorded as RESOLVED (R32Float, 256 taps,
  in-place row); the one named sequel (the Level editor's CurveEdit
  consumer, graph_eval.rs) recorded as the live follow-up, not
  staleness.

## Judgment calls

1. The four killed alternatives are named as the doc's residual
   authority — the reference for WHY the curve rides a LUT binding.
2. No edits to any landed code: the closure is doc-only; every claim
   cites the live tree.

## Unverified

- Nothing compile-shaped: doc-only; every cited symbol grep-verified
  live this session (see the focused verifier).

## Gates run (host-free, this session)

- `scripts/verify-kill.sh`: PASS (trap scan, doc-attachment, lock
  audit — the falloff-design-closure lock live).
- Focused verifier: status bump, closure tail, every live anchor
  (set_falloff_lut, the WGSL binding + case, the variant + eval arm,
  the panel row's Custom branch, the catalog key), the manifest pins.

State: VERIFIED (host-free by nature), IN FLIGHT — commits at the
recovery via its manifest; lock release at that commit.
