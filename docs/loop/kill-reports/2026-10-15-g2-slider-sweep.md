# Kill report — g2-slider-sweep (GUI plan G2)

Date: 2026-10-15. Author: claw (claude-opus-5.5 subagent, interrupted by a
~11-min provider hang after the edits and before its battery read). The
dragon reviewed it and finished it. Lock: `g2-slider-sweep` over
`crates/umber-app/src/{fill_layer.rs, path_tool.rs, display_panel.rs,
layers_panel.rs}`, `tests/panel_snapshot.rs`, `tests/snapshots/`, this
report. Plan: `docs/specs/gui-plan-2026-10-15.md` row
`g2-raw-slider-sweep`.

## Built

11 raw `egui::Slider`s migrated to `umber_gui::ValueField`. No raw
slider is left in these three files.

| File | Sliders | Rows |
|---|---|---|
| `fill_layer.rs` | 6 | sharpness 1..8, scale 0.1..8, alpha 0..1, angle 0..τ, noise scale 0.1..16, octaves 1..8 |
| `path_tool.rs` | 4 | smudge strength 0..1, smear 0..1, path size `PATH_MIN_SIZE..=PATH_MAX_SIZE`, opacity 0..1 |
| `layers_panel.rs` | 1 | material-channel opacity 0..1 (inline: it sits in the channel row's existing `horizontal` beside the enable checkbox and the blend combo) |

- Every range is identical to its slider's (dragon-reviewed line by line).
- `fill_layer.rs` and `path_tool.rs` each gain the postfx panel's
  `field_row` helper (muted label + field).
- `display_panel.rs` had 0 sliders and is untouched.

## Judgment calls

1. **No snapshots.** `fill_layer`, `path_tool` and `layers_panel` are
   bin-only modules (declared in `main.rs`, not `lib.rs`), so
   `tests/panel_snapshot.rs` cannot reach them, and `lib.rs` was outside
   this kill's lock. Re-queued under the G0 kittest-harness kill, which
   can expose the panels through `lib.rs` deliberately.
2. **The fill angle stays radians, 0..τ.** A degrees display is a
   `ValueField` unit/scale feature, not a range change. Re-queued under
   g4-valuefield-grammar.

## Unverified

- Visual parity, with no snapshot (judgment call 1).
