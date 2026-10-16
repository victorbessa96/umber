# Kill report: g6-contextual-toolbar (GUI plan G6)

- Authorship: dragon (solo; ran parallel to the G3 keymap claw and after
  the G5 home-card claw, disjoint surfaces under kill locks).
- Kill-lock `g6-contextual-toolbar`:
  `crates/umber-app/src/toolbar.rs` (the strip + the
  `brush_panel.rs` registration surface — the panel itself needed no
  change: the strip borrows the same preset fields it edits).
- Implements: `docs/specs/gui-plan-2026-10-15.md` row
  `g6-contextual-toolbar` (P:A7, B:A5.7): the strip at the top of the
  viewport for the active tool — Size, Flow, Opacity, Spacing, Lazy
  mouse (D), Symmetry (L) + its offset handle — with the same values
  Properties edits (one source of truth).

## Built

- `toolbar.rs` gains the strip widget family:
  - `ContextualView<'a>` — the one-frame view struct: size/hardness by
    value (the paint session owns them), flow/opacity/spacing/lazy
    radius/symmetry offset by `&mut f32` (the preset/panel owns them),
    plus the lazy/symmetry toggle states and `paint_available`.
  - `StripEdit { BrushParams, LazyToggle, SymmetryToggle }` — the edit
    channel: the strip NEVER touches the paint session or preset
    directly; every change rides a variant so main.rs applies it
    through the same setters the panel uses (the two surfaces cannot
    drift by construction).
  - `contextual_strip(ui, view) -> Vec<StripEdit>` — the compact row
    (Density::Compact style, localized labels, ValueField fields with
    the panel's own ranges: size 1..=512, flow/opacity 0..=1, spacing
    0.02..=4, lazy radius 4..=128, symmetry offset 0..=1), lazy/symmetry
    as selectable-label toggles, the radius/offset fields appearing
    beside their toggle while enabled.
- i18n: `strip.size/flow/opacity/spacing/lazy/symmetry/no-paint` in
  BOTH catalogs (en-US, pt-BR).

## Judgment calls

- Strip-holds-no-state: the alternative (the strip caching its own
  copies) is exactly how two surfaces drift; rejected. The cost is the
  main.rs wiring (fill the view, apply the edits) at wave integration,
  same shape as the home card's `HomeCardInput`.
- Size/hardness by value + explicit `BrushParams` edit: the paint
  session owns the live brush (`set_brush_size`/`set_brush_hardness`);
  the strip reports the edit rather than mutating, so a stroke in
  flight keeps its opening params (the same contract
  `set_brush_size` documents).
- `ui.add(ValueField..)` idiom: the first draft called `.ui(ui)`
  directly (not the workspace's `Widget` call pattern); corrected from
  the real call sites in brush_panel.rs.
- Toggles report-only in v1 (`LazyToggle`/`SymmetryToggle` as events):
  the toggle STATE lives in the panel settings structs
  (`LazyMouseConfig`, `SymmetrySettings`), whose owners are panel-local;
  main.rs flips them at integration. This keeps D/L shortcuts possible
  through the same action path later.
- `#[allow(dead_code)]` on the three pub items (the lib surface rule:
  pub items can never be dead on a lib, `expect` would fail that leg)
  until the wave-integration commit wires them; the home-card
  precedent.

## Unverified

- The strip's egui rendering (a kittest needs the App state; the wave
  integration's main.rs wiring is its natural test point — named
  follow-up with the home card's).
- Compiled + 13/13 toolbar tests + clippy `-D warnings` clean on BOTH
  targets, verified ad-hoc in a detached worktree at HEAD + only this
  kill's three files, on the build host (2026-10-16).

## Deferred

- The D/L shortcut bindings (registry rows — actions.rs is under the
  G3 keymap lock at this write; they ride the wiring commit).
- The symmetry display popover (pairs with g6-overlays-popover).

## Process note

- The keymap claw held actions.rs/main.rs/preferences.rs in parallel;
  no surface crossed.
