# Kill report — stroke-methods (2026-10-15)

- Author: subagent (claw), dispatched by the dragon.
- Lock: `wave-stroke-methods` — `crates/umber-brush/src/stroke_method.rs`,
  `crates/umber-brush/src/lib.rs`, `crates/umber-brush/src/spacing.rs`,
  `crates/umber-brush/src/wiring.rs`, this report.
- Source: ROADMAP wave 8b row `wave-stroke-methods`; Blender manual, Stroke
  panel (https://docs.blender.org/manual/en/latest/sculpt_paint/brush/stroke.html):
  Space, Airbrush (Rate), Drag Dot, Dash Ratio / Dash Length.

## Built

- `stroke_method.rs` (new): `StrokeMethod { Space (#[default]), Airbrush { rate_hz },
  Dash { ratio, length_px }, DragDot }`, `StrokeMethodError`, `validate()`
  (rate finite > 0; ratio finite in (0,1]; length finite > 0), serde, and
  `MethodState` (per-stroke runtime: `seed` / `apply` / `finish` / `reset`),
  `AIRBRUSH_MAX_DABS_PER_PUSH = 64`. 7 tests (default, validation, serde
  wire shape + round trip, deserialize validates, Space pass-through touches
  no state, airbrush cap + backwards time, dash ratio 1 keeps all).
- `wiring.rs`: `StrokeConditioner` gains `method` + `method_state`,
  `set_method(StrokeMethod) -> Result<(), ConditionerError>`, `method()`,
  `finish() -> Vec<DabPlan>`; `ConditionerError::Method` variant; `advance`
  takes the event time and runs the method after spacing; `reset` drops
  method state. 6 tests: Space bit-identical to a hand-composed copy of the
  pre-method pipeline (raw f32 bits, default and explicitly-set Space, lazy
  leash + dwell + varying pressure, >10 dabs compared); airbrush 10 Hz held
  still 0.35 s = per-event `[0,1,0,1,0,1,0]`, 3 dabs at the dwell point;
  airbrush moving == Space; dash 0.5/20 over 100 px at 5 px step keeps
  x = 5, 20, 25, 40, 45, 60, 65, 80, 85, 100, identical as 1 segment or 10;
  drag dot (nothing during, one dab at last position on `finish`, once;
  click-only dot; reset drops the pending dot); `set_method` rejects invalid
  and keeps the current method.
- `lib.rs`: `pub mod stroke_method;` + re-exports. Also, per the
  orchestrator's mid-flight instruction, `pub mod cavity;` + `pub use
  cavity::{CavityMask, CavityMaskError, CavityTarget};` (cavity.rs itself is
  the orchestrator's, untouched).
- `spacing.rs`: unchanged.

## Judgment calls

- Integration: a field on `StrokeConditioner`, not a wrapper. Airbrush needs
  the conditioned cursor/pressure and event clock, Drag Dot the last
  conditioned cursor, Dash the spaced dabs — all internal to `advance`. A
  wrapper would have to re-expose them. Spacing and the anchor update run
  unconditionally, so methods only post-filter; Space returns the spacing
  Vec untouched and mutates no state.
- Serde: the public enum serializes through a private `RawStrokeMethod`
  (`#[serde(rename_all = "snake_case", tag = "kind")]`) with
  `try_from`/`into`, so deserialization validates. Wire: `{"kind":"space"}`,
  `{"kind":"airbrush","rate_hz":10.0}`, `{"kind":"dash","ratio":0.5,"length_px":20.0}`,
  `{"kind":"drag_dot"}`.
- Airbrush semantics: the bank accumulates `elapsed_ns * rate_hz / 1e9` (f64)
  per event and emits one dab per whole unit at the current cursor; any
  spacing dab in the push drains the bank. So moving strokes are spaced
  exactly like Space and spray only fills dwells. Rejected: Blender-style
  "spray on top of motion", which doubles density on moving strokes. Backlog
  beyond the 64-per-push cap is dropped, not replayed; backwards time =
  zero elapsed.
- Dash length is in pixels of path (the brief), not Blender's "stroke
  samples". Phase = arc length along the emitted dab chain from the seed
  point (carried across pushes), kept iff `phase mod length < ratio*length`;
  phase quantized to 1/256 px so on-boundary dabs don't flicker on float
  noise.
- Drag Dot: the seed sets the pending dot, so a click with no motion still
  leaves a dot. `finish` does not reset; callers reset as at any stroke end.
- `set_method` resets only the method state, not the filters/spacing.
- Dash test compares positions within 1e-4: spacing itself places the 12th
  step of a 100 px single segment at 60.000004 (`from + segment * t`); the
  count pins the selection exactly.
- Added `ConditionerError::Method` (enum not `non_exhaustive`); grep found no
  exhaustive matches on `ConditionerError` outside umber-brush.

## Unverified

- No paint-state or GPU run: the methods are not wired to any consumer yet,
  so no stroke in the app uses them.
- Airbrush behavior under the smoothed (non-exact) path while dwelling: the
  one-euro cursor keeps creeping toward a still pointer, which can emit
  spacing dabs that drain the spray bank. Tested only under exact mode;
  smoothed dwell density is by-construction, not pinned.

## Deferred

- `paint_state` wiring (umber-paint: call `set_method` from the brush,
  call `finish()` before `reset()` at stroke end).
- Brush panel UI (method picker, rate / dash ratio / dash length rows, i18n keys).
- Persistence of the method in `.umberbrush` presets (`preset.rs`).
- Blender's Dots, Anchored, Line and Curve methods, and jitter.

## Process note

- lib.rs now also registers the orchestrator's `cavity` module (instructed
  mid-flight); its tests compile and pass in the same `cargo test -p umber-brush` run.
