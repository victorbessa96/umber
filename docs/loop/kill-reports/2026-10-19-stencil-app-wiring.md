# stencil-app-wiring — the paint hook wired (the seam's TODO closed)

2026-10-19, session local-3d2151f3. Lock: stencil-app-wiring
(stencil.rs, display_panel.rs, main.rs, both catalogs, this report).
The stencil-paint-hook kill's (3a8b4cb) named follow-up: the
app-side hand-off that closes the seam's TODO(stencil-paint-hook).

## Built

- `StencilState::hook_gate(target)` — the hook's bytes (Amendment A):
  the RAW rasterized coverage at the target size, `None` while the
  stencil contributes no gate (which CLEARS paint's staged gate, so
  paint passes ungated). NEVER the composed `staged` bytes — the
  two contracts are now pinned apart by test: with a quick gate
  present, the LABEL composes min ([255,0,0,0, 255,0,0,0, 0,0,255,0,
  0,0,0,0] for the pattern x quick4 case) while the HOOK stays RAW
  (the pattern's own bytes) — gate_plane folds the quick mask in
  itself (product), Amendment B's split intact at both sites.
- `panel_section` now takes `Option<&mut PaintState>` (the viewer's
  field was already `Option<&'a mut PaintState>` — main.rs's call
  moves `as_deref` -> `as_deref_mut`) and hands
  `state.hook_gate(target)` to `set_stencil_gate` EVERY panel frame:
  the setter's same-value no-op carries the design's change-driven
  cost contract, and a stencil staged before a document opened
  still hands off on the next popover frame. The label's re-stage
  path (stage_gate, min-composed) is unchanged.
- The pending row flips live: `overlays.stencil-gate-pending` ->
  `overlays.stencil-gate-live` in BOTH catalogs (EN: "Gate live
  ({width}x{height}, {open}% open) — paint passes only where it
  opens."; pt-BR: "Máscara ativa (...) — a tinta passa apenas onde
  ela abre."), the display_panel reference + the completeness
  test's key list with it. The seam's module-doc v1-boundary bullet
  and stage_gate's doc comment now record the landed truth.

## Tests

- New: `hook_gate_hands_off_raw_coverage_and_clears_when_gone`
  (stencil.rs, bin target) — raw at identity, None on no-target
  (clearing), None inactive (clearing), label-min vs hook-raw with a
  quick gate present (the Amendment A/B split).
- Updated: the i18n completeness key list (gate-live replaces
  gate-pending; every_shipped_locale green in the lib target — the
  key ships in both locales).

## Gates run

- rustfmt CLEAN (standalone, pre-battery, all three .rs files).
- Host battery (`wave-battery.sh umber-app`, centralhost):
  BATTERY_EXIT=0 — layers 16 GREEN, fmt PASS (applied), clippy
  umber-app PASS (-D warnings), test umber-app 821 passed / 0
  failed (820 prior + this kill's test). Targeted confirmation:
  the named test green live in the bin target (1 passed, 609
  filtered); `every_shipped_locale` green in the lib target.

## Named follow-ups (not this kill)

- The active-tile-change re-stage (per-set scope): a stencil staged
  on one tile passes stencil-ungated on other-sized tiles until the
  shell re-stages at the activate_set sites — Amendment A's
  documented safe degradation; wire it with (or after) the viewport
  chords.
- The viewport chords + the overlay draw's per-frame transform
  (design step 3), then the docs tick (step 4: gui-plan g6 row,
  ROADMAP, queue).
