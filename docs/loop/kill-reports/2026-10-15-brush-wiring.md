# Kill report — brush-wiring (the preset reaches the brush)

Date: 2026-10-15. Author: dragon. Lock: `brush-wiring` over
`crates/umber-paint/src/paint_state.rs`, `crates/umber-app/src/{main,
brush_panel}.rs`, `crates/umber-brush/src/{preset, preset_library,
stroke_method, wiring}.rs`, `assets/i18n/*.ftl` and this report. Plan:
`docs/specs/gui-plan-2026-10-15.md` row `g2-brush-method-cavity-ui`
(stroke-method half) and ROADMAP 6a `wave-stroke-methods-wiring`. It also
takes the brush size and hardness that g1-brush-resize kept as session
values. The first claw attempt died on a provider hang and left nothing.

## Found first: the session never used the preset's brush

`PaintState.brush` was set to `BrushParams::default()` at construction
and never written again; only the eyedropper's `set_brush_color` touched
it. The brush panel's live editor (alpha, hardness, pressure gamma,
falloff curve, color) changed only the preset, saved to `.umberbrush`
files that painted exactly like the default. The falloff-ui kill's
report claimed the curve was "wired"; it was wired to the file, not the
stroke. Only the wash mode had a sync (`sync_wash_mode`), and that is why
it worked.

## Built

- **`PaintState::sync_brush_params(&params, preserve_color)`**: the
  preset's dab parameters, between strokes only. The wash mode keeps its
  own sync. `preserve_color` keeps an eyedropper pick until the preset
  itself changes. Also new: `brush_params()` and
  `sync_stroke_method(method) -> bool`, which refuses mid-stroke and
  refuses invalid methods.
- **Drag Dot works.** `end_stroke` now flushes the conditioner's
  `finish()` tail (Drag Dot's one release dab) BEFORE the stroke records,
  so the dab joins the same undo step.
  - To share the exact live-dab path, `push_event_styled`'s tail (dab
    conversion, then layer-mask / quick-mask / retouch hooks, symmetry,
    pool staging) was extracted verbatim into `stage_stamps(tile,
    stamps, radius, expand_seams)`. The live path calls it with the same
    arguments, so live behaviour is unchanged by construction; the only
    edit is `&stamps` becoming `stamps`.
- **Preset schema** (`preset.rs`): `stroke_method: StrokeMethod`
  (serde default Space) and `size: f32` (serde default 1.0). Old files
  load unchanged (test strips both keys). `validate` covers size
  (0.05..=8) and the method's own domain. The non-default fixture
  carries Dash + size 1.5, so the byte-identical round trip covers both.
- **Brush panel:** a Size row and a Stroke Method combo
  (Space / Airbrush / Dash / Drag Dot) with ONLY the chosen method's
  params (Blender's conditional rows): Airbrush rate 1–60 Hz, Dash ratio
  0.05–1 and length 1–512 px, all inside the methods' valid domains.
  - `preset_changed` (set on pick / edit, taken by the sync) decides
    when the session adopts the preset's color and size.
  - `adopt_live_size` writes `[` / `]` / Ctrl+RMB size and hardness BACK
    into the active preset (marked dirty), so the per-frame sync never
    snaps a live resize back and a Save keeps it.
- **main.rs:** the Properties sync now adopts the live size into the
  preset, then the preset's params, method and (when changed) size into
  the session.
- i18n: 9 keys (size, method, the four method names, rate, dash ratio,
  dash length), en-US and pt-BR.
- Tests:
  - `preset_params_and_method_reach_the_session_between_strokes_only`:
    params land; the color override survives; a mid-stroke sync is
    ignored; an invalid method is refused.
  - `drag_dot_stamps_once_at_release_in_the_same_undo_step`: nothing is
    staged while dragging, a dab lands at release, and it is undoable.
  - `a_preset_without_method_or_size_loads_as_space_at_unit_size`.
  - Validate extensions for size and method.

## Judgment calls

1. **The color is preserved across syncs until the preset changes.** The
   alternative, re-applying the preset's color every frame, would wipe
   an eyedropper pick the next frame. Painter's picker writes the
   preset; that is the G4 color-picker kill's change.
2. **Airbrush dwell needs events.** The app feeds the conditioner on
   pointer events. A pen held perfectly still sends none on some
   backends, so Airbrush accumulates only while events arrive. A frame
   tick for strokes in flight is the follow-up row; it is a timing
   change in the stroke path and deserves its own kill.
3. **The cavity mask is NOT in this kill.** Its wiring needs the
   curvature bake routed into the paint session's gate, which touches
   the mesh-map import path. The plan row stays open for the cavity half.

## Unverified

- Visual check that each preset field changes the stroke: manual on
  hardware. The sync is tested at the params level, not by pixels.
