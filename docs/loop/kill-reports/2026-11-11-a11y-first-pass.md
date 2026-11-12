# Kill report: 2026-11-11 wave-a11y-first-pass (G7 keyboard-only journeys)

- **Authorship**: claw (dispatched hermes subagent) for the dragon (Razul).
- **Kill-lock**: `razul-a11y-first-pass` (held by the dragon): `crates/umber-gui/src/`,
  `crates/umber-app/src/`, both `.ftl` catalogs (untouched: no new keys),
  `docs/loop/queue.md` (row 378), this report.
- **Brief**: docs/loop/briefs/2026-11-11-a11y-first-pass.md (Build 1-3).
  Closes the G7 `g7-keyboard-only` row the 2026-10-24 lens audit
  (docs/loop/audit-2026-10-24-lens-uiux.md:32) recorded OPEN.

The e008dcc kill landed the focus-ring TOKEN, the Esc predicate and the
menu digit pick, but drove none of them as a keyboard journey. Writing
the journeys first exposed five real defects; each fix below is the
minimal edit that turns a journey green.

## Built

Kittests (they define done), `crates/umber-app/src/keyboard_journeys.rs`
(new, bin target: `cargo test -p umber-app --bin umber keyboard_journeys`).
Every journey uses key presses only, never the pointer:

- **Import**: `import_journey_file_menu_number_key_fires_open_mesh` (Tab to
  File, Enter opens it, `2` fires Open Mesh and closes the menu);
  `import_journey_dock_tabs_are_named_tab_stops_and_enter_selects` (every
  dock tab is a `Role::Tab` named by its panel; Tab to Export, Enter
  selects it).
- **Paint**: `paint_journey_tab_reaches_each_tool_by_its_own_name_and_enter_picks_it`
  (toolbar Tab order starts at New; each tool slot is a toggle named as
  itself; Enter picks Smudge; Shift+Tab walks back);
  `paint_journey_the_focused_icon_slot_paints_the_theme_focus_ring` (no
  ring unfocused, exactly one theme-token ring on the focused slot, read
  from the frame's shapes).
- **Export**: `export_journey_tab_order_is_header_combos_toggles_and_every_stop_is_named`
  (pins the full reading order: group header, Preset combo, Size combo,
  MaterialX, USDA, Choose, tiles header; no unnamed stop; Shift+Tab
  reverses); `export_journey_space_toggles_and_esc_closes_a_combo_without_a_pick`;
  `export_journey_the_group_header_toggles_from_the_keyboard`.
- **Esc**: `esc_journey_an_open_popup_owns_esc_and_the_window_behind_it_stays`
  (combo in a window) and `esc_journey_the_colour_picker_owns_esc_inside_a_window`.

Fixes (brief Build 2/3):

1. **Painter-drawn widgets showed no focus ring** (Build 1 "visible focus
   ring"). egui paints the `active` visuals only for its own widgets.
   `theme.rs`: `focus_ring_stroke()` (FOCUS_RING_WIDTH in the theme accent)
   + `paint_focus_ring(ui, &response)` (focus-only; drawn Inside so a
   full-panel canvas never clips it). Called by `icons::icon_button_enabled`
   (every toolbar/rail icon), `ParamGroup` header, `ValueField`, `Curve`,
   `Ramp`, `SwatchGrid`, and the colour picker's channel rows, SV square,
   hue strip, previous swatch and both colour buttons.
2. **ToolRow mislabelled its first slot** (AccessKit gap). The union
   response's `widget_info` wrote the ACTIVE tool's name onto slot 1's
   node, so with Erase active two slots read "Eraser". `tool_row.rs`: each
   slot reports its own name as a toggle (`WidgetInfo::selected`).
3. **Disabled icon buttons were Tab stops.** `icon_button_enabled(enabled
   = false)` now senses `Sense::CLICK` without FOCUSABLE (egui's own rule
   for disabled widgets). Pointer behaviour is unchanged: the response
   still never reports a click the caller acts on.
4. **Unnamed Tab stops** (AccessKit gap): `ParamGroup` headers (now
   `CollapsingHeader` → Button role, title, open state); egui_dock tab
   titles (bare focusable nodes; `main.rs::dock_tab_a11y` names them
   `Role::Tab` from `on_tab_button`); row-labelled combos (the Export
   preset combo and `size_presets::size_combo`, which gained a `name`
   argument used at all four call sites). New primitive
   `umber_gui::widgets::a11y::name`.
5. **One Esc closed two layers.** With a combo or the colour picker open
   inside Preferences, Esc closed the popup AND the window.
   `actions::esc_closes_overlays` now defers while `egui::Popup::is_any_open`
   or an Umber popup holds Esc; `widgets::a11y::{claim_escape,
   escape_claimed}` is the pass-stamped claim the colour picker makes while
   open (no release call to forget).

## Judgment calls

- **Journeys on the bin target, not `tests/`.** The lock excludes
  `crates/umber-app/tests/`, and the Export dialog plus the dock viewer are
  bin-private. Alternative rejected: widening `lib.rs` (it would drag the
  export_dialog closure: bakes_panel, graph_panel, bake_sources).
- **The full shell is not driven.** `UmberApp::new` needs an eframe wgpu
  render state; the journeys drive the real components (`toolbar::show`,
  `ExportDialog::show`, `DockArea` with the shell's tab naming,
  `menu_digit_pick`, `esc_closes_overlays`) in the order a user meets them.
- **Focus ring drawn Inside.** Outside would be clipped on the 3D/UV
  canvases (allocated full-panel) if those ever become Tab stops; Inside
  is uniform and theme-token driven.
- **No new i18n keys.** Every accessible name reuses an existing visible
  string, so both catalogs are untouched and stay complete.

## Unverified

- Real screen-reader output (Orca/NVDA): verified through the AccessKit
  tree kittest reads, not a live reader.
- Live-app keyboard pass over `--control`: not run in this kill.
- Light/High Contrast ring contrast: the ring uses `theme::accent()`, the
  same token the existing `active` ring uses; no new pixel sampling.

## Deferred

- **Panel focus traversal ACROSS the dock** (F6-style "next panel"): Tab
  already walks all widgets across leaves in draw order; a dedicated
  panel-cycling key needs a binding choice (F6 is free) and a hint entry.
  Named follow-up.
- **The 3D/UV viewports are not Tab stops** (`Sense::click_and_drag` on a
  rect is focusable, but painting by keyboard has no gesture yet).
  Keyboard painting is out of scope for G7.
- **SwatchGrid per-cell focus**: the grid body is one stop (the d9fa931
  note); per-cell arrow-key picking is a widget migration.
- **The Layers/Properties panels' own Tab order** is not pinned by a
  journey here (they had role/label kittests from d9fa931).

## Battery

`UMBER_ASSETS_DIR=<worktree>`, under `flock /tmp/umber-cargo.lock`: rustfmt
--check on every touched .rs clean; check-layers GREEN (16 crates);
scorecard current; `cargo clippy -p umber-gui -p umber-app --all-targets --
-D warnings` exit 0; `cargo test -p umber-gui -p umber-app` 1315 passed,
0 failed (umber-gui lib 150, umber-app bin 811 incl. the 9 journeys).
No panel snapshot changed (none regenerated).

## Process note

- Battery under `flock /tmp/umber-cargo.lock` with
  `UMBER_ASSETS_DIR=<worktree>`; load from sibling projects stretched the
  first test build to ~30 min.
