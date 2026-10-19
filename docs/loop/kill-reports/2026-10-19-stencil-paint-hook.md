# stencil-paint-hook — the gate_plane paint hook (Amendment A implemented)

2026-10-19, session local-3d2151f3. Lock: stencil-paint-hook
(crates/umber-paint/src/paint_state.rs). The stencil wave's named
next kill: the umber-paint half of the stencil engine — the design
doc's Amendment A contract, implemented against the set_cavity_mask
precedent.

## Built

- `PaintState::set_stencil_gate(Option<(u32, u32, Vec<u8>)>)` — the
  pub setter on the `set_cavity_mask` precedent: refused mid-stroke
  (one stroke never changes its gate partway), a same-value re-set
  is a no-op, malformed bytes refused fail-closed at the setter (a
  zero side, or `len != w * h`), adoption clears `mask_synced` (the
  `rebuild_cavity_coverage` precedent — the next gated dab re-uploads
  the composed plane).
- `stroke_mask_gate` arms with `|| stencil_gates()` — the stencil
  ALONE arms the gate now (no quick mask, no cavity needed).
- The fold inside `gate_plane`, after the cavity block: RAW stencil
  bytes as f32 (`byte / 255.0`), product with the quick × cavity
  planes; per-tile dimension match (only the tile whose quick-mask
  dims equal the gate's `(w, h)` composes — other-sized tiles pass
  through stencil-ungated until the shell re-stages, Amendment A's
  per-set scope).
- The staged bytes are the stencil's RAW R8 coverage, NOT
  pre-composed with the quick mask — exactly Amendment A's contract
  (the shell-side `compose_gate` min stays the staged label's
  contract; Amendment B's product-vs-min split holds at both sites
  unchanged).

## Tests (in-file, the cavity gate test's shape)

`stencil_gate_composes_into_the_gate_plane_and_arms_alone`:
off = all-pass; malformed refused fail-closed (zero side, short
buffer — never staged); same-value re-set no-op; composition (open
half passes, blocked half gates out); dimension-mismatch
pass-through (an 8x8 gate composes nothing on the 4x4 tile);
arming with NO quick mask and NO cavity (`begin_stroke` ->
`stroke_mask_gate` true); mid-stroke re-staging refused; clearing
lifts the gate.

## Gates run

- rustfmt `--check` CLEAN (standalone, pre-battery; two assert
  reflows applied from its own diff).
- Host battery (`wave-battery.sh umber-paint`, centralhost):
  BATTERY_EXIT=0 — layers 16 crates GREEN, fmt PASS (applied),
  clippy umber-paint PASS (`-D warnings`), test umber-paint 171
  passed / 0 failed (170 prior + this kill's test). A green battery
  deletes its own log dir (wave-battery.sh:28-31's trap) — the
  count is corroborated by the targeted rerun: the named test green
  live on the real GPU (1 passed, 0.23s), which also proves the
  `try_request_device` leg executed rather than skipped.

## What this unblocks

The app-side wiring (the seam's own named follow-up): umber-app's
`StencilState::stage_gate` parks the composed bytes in `staged` +
the panel carries the pending notice — the wiring kill calls
`set_stencil_gate` on gate changes and flips the notice; battery
umber-app. After that, the viewport chords (design step 3) and the
docs tick (step 4: gui-plan g6 row, ROADMAP, queue).
