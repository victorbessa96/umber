# Kill report — a2-snapshot-breadth (the W-C3a plan's lane A2)

Date: 2026-10-20. Author: dragon (local-10e6ed18). Lock:
`a2-snapshot-breadth` (crates/umber-app/tests/panel_snapshot.rs, this
report). Coded host-free under the freeze-4 envelope + the 2026-10-20
code steer; tests UNRUN until the recovery battery.

## Built

Four kittests on the existing harness pattern (ensure_i18n +
Harness::builder().with_size().build_ui + run_ok + snapshot):

1. `history_panel_empty_snapshot` — `history_ui(ui, &mut
   Document::default(), None)`: the panel the module doc has claimed
   since v1 finally exists as a test (the plan's named doc defect
   repairs itself by landing, no doc edit needed).
2. `history_panel_ops_snapshot` — the same panel after two journalled
   layer ops (`LayerCommand::Add { name, kind, id: None }` twice: a
   Paint then a Fill), exercising the journal rows the panel renders.
3. `toolbar_snapshot` — `toolbar::show` over a fully-specified
   `ToolbarView` (Paint tool, can_undo, grid on, defaults elsewhere)
   with `IconAtlas::empty()`.
4. `rich_tooltip_snapshot` — `rich_tooltip::draw` with a full
   `TooltipParts` fixture (title + description + shortcut,
   unavailable: None).

The four PNG baselines (`history_panel_empty`, `history_panel_ops`,
`toolbar`, `rich_tooltip`) cannot be produced host-free: they are
generated on the build host with `UPDATE_SNAPSHOTS=1 cargo test -p
umber-app --test panel_snapshot` in the close script's pre-battery
step (under the cargo flock, one command), pulled back, then pinned
by the CI comparison from then on.

## Judgment calls

- `IconAtlas::empty()` (not `load_embedded_atlas`): the headless
  constructor the codebase itself uses in test contexts
  (main.rs:4945's precedent). The baseline honestly pins the
  toolbar with placeholder glyphs; a raster-atlas baseline is a
  follow-up if the harness ever wants it.
- The ops test uses `LayerCommand::Add` with `id: None` — the
  journal-integrated path (`apply` allocates), the same command shape
  the layers panel's add button runs.
- The existing three baselines must stay byte-unchanged when the new
  ones generate (the close script cmp-asserts this; a drift there
  means the stencil/eyedropper lanes moved a panel and the drift is
  theirs to reconcile).

## Unverified

- The tests are UNRUN (no compile host-free). The API shapes were
  registry-resolved: history_ui/toolbar::show/rich_tooltip::draw
  signatures, ToolbarView's six fields, ViewToggles, ToolKind::Paint,
  `umber_gpu::ViewMode::default()` (#[default] Lit), LayerCommand::Add
  + LayerKind::{Paint,Fill}, IconAtlas::empty — every one read from
  the live tree before writing.
- The baselines do not exist yet; the tests fail by construction
  until the host generates them (the close script's step 4a, before
  the battery).

## Deferred

- The GPU-hungry panels (Viewport, Bakes) stay out until their state
  is headless-constructible (the v1 module-doc boundary).

## Gates

- Battery: umber-app (`--test panel_snapshot` inside the full
  battery). The close script's a2 commit row carries the manifest
  paths + the four generated PNGs; the row's lock releases at its
  commit.
