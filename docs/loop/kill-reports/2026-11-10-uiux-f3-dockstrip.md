# Kill report 2026-11-10 razul-uiux-f3-dockstrip (dragon)

Authorship: dragon (Razul). Kill lock `razul-uiux-f3-dockstrip` over
`crates/umber-app/src/main.rs`, `crates/umber-app/src/workspaces.rs`,
`crates/umber-app/src/shell_menu.rs`. Implements finding F3 (P0) of
`docs/loop/audit-2026-11-10-uiux-code.md` and requirement R3-3.

## Built

- `DockArea` now builds with `show_close_buttons(false)`,
  `show_leaf_close_all_buttons(false)`, `show_leaf_collapse_buttons(false)`;
  the panel-group menu (add button + popup) stays. Verified against the
  vendored egui_dock 0.21.1 source (`widgets/dock_area/mod.rs:110,207,215`)
  before naming the hooks.
- `dock_tab_widths` call sites pass `close = 0.0` (no per-tab glyph);
  `dock_bar_width` subtracts one button, not three. Doc comments and the
  pinned width tests updated in the same kill.
- Presets rebalanced so no leaf holds more than 4 tabs: `side_layout`
  stacks `rest` in leaves of at most 4; the Graph preset's 8-tab leaf is
  now 4 + 4. Project column widened 0.19 -> 0.20 in `default_dock` and
  `side_layout` so the 4-tab project strip fits at 1400 pt (was 246 > 242).
- New test `r3_3_every_preset_leaf_fits_its_tab_strip_at_1400x900`: for all
  four workspace presets, every leaf's `dock_floor::leaf_sizes` width +
  real elided galley widths must fit `dock_bar_width` (close chrome gone).
  Fails before the fix, passes after.

## Judgment calls

- Tabs are still closable via the panel-group menu and re-openable via the
  rail; losing the per-tab X is the accepted trade (the audit's own fix 1).
- 0.20 was the smallest bump that clears the 4 pt overflow; the pinned
  `default_dock_order_and_widths` test does not pin exact fractions, only
  order/floors/viewport-dominance, and still passes.
- `LAYOUTS_VERSION` NOT bumped: the split fractions of the boot layout
  changed shape for Bake/Export/Graph presets and the Paint left column
  width. Saved user layouts are untouched by preset changes, but the
  boot default did change — bumped LAYOUTS_VERSION is the honest move for
  a default-layout change per the skill rule; done in this kill.

## Unverified

- Live-render pixel check of the strips at 1400x900 (the R3-3 test is the
  arithmetic the renderer follows; a control-channel screenshot pass is a
  follow-up smoke).

## Deferred

- F14 (protected tabs non-closable) is a separate finding; the group menu
  still lists Viewport/UV for closing — queued, not in this kill.

## Verification

Battery: rustfmt, check-layers, scorecard --check, clippy -p umber-app
-all-targets -D warnings, full umber-app suite (incl. r3_3 and the updated
tab-width pins) — see queue row for the green line.
