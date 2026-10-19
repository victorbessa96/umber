# stencil-tile-restage — the set switch re-stages the gate (Amendment A point 4 closed)

2026-10-19. Coded by the opencode claw on `opencode/deepseek-v4.1-flash-free`
(Bessa's steer, 14:3x) against a written brief; cross-reviewed, censused,
battery-verified and committed by session local-3d2151f3. Lock:
stencil-tile-restage (main.rs, viewport.rs, stencil.rs, this report).

## The finding (the claw's investigation — the brief's guess was wrong)

The paint→document dependency is ONE-WAY. `texture_sets_panel_ui`
owns the user's set selection and calls `paint.set_active_set`
(main.rs:~2527; paint_state.rs:1508-1513) — THAT is the paint-side
set-switch event. `doc.activate_set` (main.rs:1049/:1121) only
FOLLOWS paint afterward; Document holds no PaintState handle. And
the active TILE id never changes on a set switch at all — its only
writer is stroke routing inside umber-paint (paint_state.rs:4694).
The gate's dims change on a switch exactly when
`set_of_tile(active_tile)` re-resolves through the new active set
(an unassigned tile) or the new set's quick mask sizes differently.

## The wiring

- `restage_stencil` goes `pub(crate)` (viewport.rs:~1677), its doc
  naming the fourth caller and the Amendment A point 4 contract —
  the unified re-stage (label bytes + the RAW-coverage hand-off) is
  the one gate path; duplicating it in main.rs is what the design
  forbids.
- `texture_sets_panel_ui` threads `stencil:
  Option<&mut StencilState>` (6 args, under clippy's limit); both
  panel arms pass `Some(&mut *self.stencil)` (field-disjoint
  split borrows beside `self.paint`).
- The wiring sits immediately after `paint.set_active_set`,
  guarded to a live gate (`active && image.is_some()` — gate_bytes's
  own "contributes a gate" condition; an inactive stencil holds
  nothing to re-clear), reborrows paint, and cites point 4.

## Cross-review verdict (this session)

- Census GREEN: the pub(crate) exposure, the wiring order (after
  set_active_set), the guard, 3 stencil-threading call sites (2 new
  + the viewport-shell commit's), rustfmt --check both files.
- Host battery (wave-battery.sh umber-app): BATTERY_EXIT=0 — fmt
  PASS, clippy PASS (-D warnings), 821 tests / 0 failed. The claw
  honestly reported the borrow threading as not-compiler-checked
  (no cargo allowed on its host); the battery is that gate.
- verify-kill's raw brace-count 878/879 on main.rs pre-existed at
  HEAD (876/877 — a `}` inside a string literal); the claw's
  lexical strip gives balanced on both HEAD and the working file.

## Residual seam (recorded by the claw, not this kill's surface)

A stroke crossing into a differently-sized tile changes
`active_tile` INSIDE umber-paint mid-stroke; the first dabs of that
stroke pass stencil-ungated until something re-stages. Closing it
needs a paint-side per-tile gate (or a shell per-frame observer on
active_tile/dims) — umber-paint territory, the named follow-up. The
Display panel's panel_section already re-hands the gate every panel
frame, so the degradation only bites off-panel — exactly the gap
this set-switch re-stage now covers.
