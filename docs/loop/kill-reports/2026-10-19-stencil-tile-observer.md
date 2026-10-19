# stencil-tile-observer — the cross-tile-stroke residual seam closed

2026-10-19. Coded by the opencode claw on
`opencode/deepseek-v4.1-flash-free` against a written brief (design
pinned by this session); cross-reviewed, censused, battery-verified
and committed by session local-3d2151f3. Lock:
stencil-tile-observer (stencil.rs, viewport.rs, this report).

## The seam (the claw's verification, all anchors live)

umber-paint's `push_event_styled` moves `active_tile` mid-stroke
(paint_state.rs:4694-4698) when a stroke crosses into another tile;
the staged gate keeps the OLD tile's dims; `gate_plane` skips
composing on a dims mismatch (:5235) — so the new tile's dabs paint
stencil-ungated indefinitely, until a panel interaction re-staged.

## Built

- `StencilState::staged_for: Option<(u16, u32, u32)>` — the
  observer's bookkeeping: tile id + gate dims the HOOK's bytes were
  handed off for. Set ONLY on an ADOPTED hand-off
  (`set_stencil_gate` returns bool; it refuses mid-stroke and
  no-ops on same value). Recording on anything but `true` would let
  the label bytes (which always stage) convince the observer the
  hook matches — and the stale gate would live forever. The new
  test `label_staging_never_records_an_adopted_hand_off` pins the
  separation at the state level.
- `restage_stencil` records on adoption only; an adopted CLEAR
  (deactivation) records `None`, so a mid-stroke reactivation
  refusal is retried once the stroke ends.
- The observer at the top of `ui_with_stencil` (before the
  zero-size early return): live-gate guard + `!is_stroking()`,
  dims-only compare (the hook bytes are a pure function of the
  dims; comparing the tile id would trip the same-value no-op and
  re-rasterize every frame), re-stage on mismatch. Frame-cheap.
- `panel_section` records its adoption too (the second adoption
  site) — without it, a panel adoption over a stale viewport record
  would loop the observer on a no-op that can never refresh it.
- The mid-stroke window stays BY DESIGN (the snapshot contract:
  one stroke never changes its gate partway); the first
  non-stroking frame after the stroke closes it. Named in the code
  comments and the module docs.

## Cross-review + gates

- Census GREEN (12/12): the field + adoption-only doc, Default,
  the test, the panel + restage recording sites, the observer
  guard + dims-only compare, the BY-DESIGN note, rustfmt both files.
- Host battery (wave-battery.sh umber-app): BATTERY_EXIT=0 — fmt
  PASS, clippy PASS (-D warnings), test umber-app 822 / 0 failed
  (821 prior + the claw's state-level test).
- verify-kill's one WARN (a `matches!` arity grep) is the
  documented false positive (the trailing string belongs to an
  `assert!(matches!(...), "{err:?}")` — pre-existing test code).

## The stencil lane's ledger

Engine (wave) -> paint hook (3a8b4cb) -> panel wiring (6c95e26) ->
viewport shell + N (2bb8c60) -> set-switch re-stage (dcc5e89) ->
this observer. The only remaining named follow-up inside the
stencil feature is design step 3's viewport chords... which landed
with the wave and were REACHABLE from 2bb8c60 — the lane is
CLOSED. (Soft edges remain the design's own named Amendment B
follow-up, unchanged.)
