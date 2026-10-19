# Kill report: stencil-hook-design (the stencil design's paint-hook + soft-edges amendments)

- Contract: the cross-review's explicit demand (5e730b4: "the design
  doc's wiring section should be amended when that kill lands") + the
  stencil-app report's Deferred "THE HOOK" spec + the design doc's
  own Open question 3.
- Kill-lock `stencil-hook-design`: `docs/specs/stencil-engine-design.md`
  + this report. Claimed in the coordination note BEFORE the build
  (16:2x), re-claimed at build start (16:3x) per the per-wakeup rule.
- HEAD at build: `ff9ac02`; the design doc was committed-clean there
  (f84d625's original), outside every lock until this kill registered.

## Built

**Amendment A — the paint hook, the wiring bullet corrected.** The
design's "The paint-path wiring" bullet claimed the shell rasterizes
and uploads through `write_mask_region`. That is impossible, and the
amendment records the verified mechanism instead: every link in the
upload chain is private to umber-paint; the composition lives INSIDE
`gate_plane` (the cavity-plane pattern); the hook is a pub
`set_stencil_gate(Option<(u32, u32, Vec<u8>)>)` following the
`set_cavity_mask` precedent (mid-stroke refusal, no-op re-set,
`mask_synced` clearing on adoption); the bytes are the stencil's RAW
R8 coverage at the active tile's quick-mask plane size, NOT
pre-composed; dimension match is the per-set scope; malformed bytes
are refused fail-closed at the setter; `stroke_mask_gate` arms with
`|| stencil_gates()`; the cost class is change-driven, never per
frame.

**Amendment B — soft edges, Open question 3 answered.** Soft edges
are a rasterize-only change (the bilinear coverage byte instead of
the threshold), and the two composition sites keep their own
semantics BY CONTRACT: the shell-side staged-label compose is min
(pinned by the app report's judgment call 6 + test 3's soft case),
`gate_plane`'s internal fold is product (the quick × cavity
precedent) — identical for binary v1 bytes, product in force at
paint time for soft values since the hook consumes raw bytes. Named
so the soft-edges kill never re-litigates it.

## Gates run

- **Anchor verification, the load-bearing gate**: every file:line
  citation re-verified against the LIVE tree by grep. The first
  draft carried six STALE anchors (pre-agent-brush-radius line
  numbers from an earlier wakeup's reads — the sibling's kill added
  13 lines above them); the gate caught all six and the doc was
  corrected before pinning. Verified anchors: `stroke_mask_gate`
  arming :1714, `gate_plane` :5171, the cavity fold :5183-5191,
  `gate_bytes` :5197, `set_cavity_mask` :5103,
  `rebuild_cavity_coverage`'s `mask_synced.clear()` :5165,
  `sync_mask_tile` :5218, `publish_mask` :5240,
  `publish_layer_mask` :2243, `mask_event` :5074.
- **The lesson, for the skill ledger**: a spec amendment citing
  file:line MUST re-anchor against the live tree at write time —
  sibling kills shift lines under parked docs, and a
  plausibly-correct stale anchor is the exact lie the anchor gate
  exists to catch.
- Scope: pure markdown; nothing host-gated, nothing to compile.

## Recovery

Both files ride the stencil wave commit's 5b extras (the script's
extras list extended + re-pinned this same wakeup); the
stencil-hook-design lock releases at that commit.
