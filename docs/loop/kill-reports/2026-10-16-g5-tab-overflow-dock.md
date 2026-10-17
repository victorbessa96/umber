# Kill report (UNRUN, host-gated): g5-tab-overflow dock wiring

Date: 2026-10-16. Author: claw (wgui2 batch, KILL A). Lock:
`wgui2-tab-overflow` over `crates/umber-app/src/main.rs` (only file
touched). Implements GUI plan row g5-tab-overflow
(`docs/specs/gui-plan-2026-10-15.md`), design
`docs/specs/g5-tab-overflow-design.md` §1 (elision in `title()`) and §4
(selected tab visible), consuming the landed
`umber_gui::widgets::tab_overflow` (2026-10-15 core kill). No cargo run;
rustfmt `--check` clean on main.rs.

## Built

All in `crates/umber-app/src/main.rs`:

- `use umber_gui::widgets::tab_overflow;` and `NodePath` added to the
  egui_dock import.
- `tab_title_text(ctx, title, font) -> WidgetText::Galley(..)`: the
  elided galley at `TAB_TITLE_MAX` (§1). egui_dock 0.21.1 passes a
  `Galley` through `into_galley` untouched (egui 0.36.2
  `widget_text.rs:805`), so the cut survives its infinite-width layout.
- `PanelViewer` gains `ctx: egui::Context` + `tab_font: FontId` (the
  Button style, egui_dock's own choice for a string title); `title()`
  returns `tab_title_text`. `on_tab_button` puts the full title in the
  hover text when the galley `elided` (§1's tooltip).
- `dock_tab_widths(text, spacing, minimum, close)` and
  `dock_bar_width(leaf_width, margin_x)`: egui_dock 0.21.1's layout
  arithmetic restated (`leaf.rs` `tab_title`: `max(minimum, text + 2·8 +
  close)`; `tab_bar`: width − inner margin − close-all − collapse, the
  two end buttons the shell's DockArea shows by default; no add button).
  Spacing is charged at the END of every tab but the last, so
  `sum(widths[..k])` is tab k's left edge — `scroll_into_view`'s contract.
- `scroll_active_tabs_into_view(dock, seen, ctx, font, style) -> bool`:
  run right after `DockArea::show_inside`. For each leaf whose ACTIVE
  panel differs from the last pass (`UmberApp::tab_active_seen:
  HashMap<NodePath, Panel>`), sets `LeafNode::scroll =
  tab_overflow::scroll_into_view(..)`; prunes dead leaves; returns
  whether a strip moved (the caller requests a repaint, since the dock
  already drew this frame with the old scroll).
- `UmberApp::tab_active_seen` field + its `HashMap::new()` in `new`.
- `mod tab_overflow_dock_tests` (4 tests, headless, fonts only):
  - `the_tab_title_is_a_pre_laid_elided_galley` — the brief's fallback
    unit pin: title → `WidgetText::Galley`, drawn row ends with "…",
    `text()` keeps the full title, width ≤ cap; a short title is whole.
  - `tab_widths_follow_egui_dock_layout` — pinned widths + bar width.
  - `a_newly_active_tab_is_scrolled_into_the_strip_once` — an
    eleven-tab leaf at 300 pt: activating the last tab scrolls to
    `bar − total` (egui_dock's clamp end); a hand scroll back to 0 with
    the same active tab is NOT undone.
  - `a_leaf_not_laid_out_yet_is_retried_later` — `Rect::NOTHING` leaf is
    skipped and not recorded; once it has a width the pass scrolls it.

## Judgment calls

- **Change-triggered scroll, not every frame.** Re-applying
  `scroll_into_view` each frame would pin the strip to the active tab and
  fight the user's wheel/drag scroll. Tracking the last active panel per
  `NodePath` scrolls exactly once per activation (click, Window ▸ Panels
  toggle pushing into the focused leaf, layout restore). Alternative
  rejected: a `TabViewer` hook — egui_dock has no on-activate callback,
  and `leaf.active` is written inside the dock (`leaf.rs:583`).
- **Post-show pass + one repaint** rather than pre-show: the leaf rect
  (hence the bar width) is only known after the dock lays out; the pass
  costs one extra frame on an activation.
- **Arithmetic restated, not read back.** egui_dock's button sizes are
  `pub(crate)`; `DOCK_BUTTON_SIZE = 24.0` and `DOCK_TAB_X_SPACING = 8.0`
  are copied from 0.21.1 with the pin test, per the design's Risk note.
- **Fallback test instead of a snapshot.** The plan's snapshot at a
  narrow width needs a rendered `DockArea` inside kittest; `PanelViewer`
  borrows the whole app state (GPU context, paint session) and lives in
  the binary, so it is not headless-constructible. The brief's fallback
  (title → galley → ends with "…") is pinned instead, plus the scroll
  pass on a real `DockState`.
- No new user-facing strings: titles still come from `panel_title`
  (`tr`), so no ftl keys.

## Unverified

- Nothing compiled or run (no cargo by brief). API names resolved against
  the registry sources: egui_dock 0.21.1 `iter_leaves_mut`, `LeafNode`
  `pub rect/tabs/active/scroll`, `set_active_tab`, `NodePath` (Hash/Eq),
  `TabBarStyle::{height, inner_margin}`, `TabStyle::{spacing,
  minimum_width}`, `TabViewer::on_tab_button`; egui 0.36.2
  `WidgetText::Galley`, `Response::on_hover_text`, `Margin::sum`,
  `TextStyle::resolve`, `tex_manager().write().take_delta()`.
- The hover tooltip is attached through a CLONED `Response`
  (`on_tab_button` gets `&Response`); `on_hover_ui` keys on the
  response's id/rect so this should show, but no test covers it.
- The width restatement matches egui_dock only for the default
  `fill_tab_bar = false` (the shell's `Style::from_egui`) and with the
  close button shown on every tab (all panels closeable today).
- The "eleven tabs overflow 300 pt" fixture asserts its own overflow, so
  a font change that made it fit fails loudly rather than vacuously.

## Deferred

- The » overflow menu (design §3: `show_add_buttons` + `add_popup`
  listing the leaf's tabs). Not in this kill's brief; when it lands,
  `dock_bar_width` must also subtract the add button (24 pt), and the
  » pick should clear `tab_active_seen` for that leaf or rely on the
  change detection (it will, since `active` changes).
- The kittest snapshot at a narrow width (plan row's test): needs a
  headless dock harness with a stub `TabViewer` (titles only) in
  `umber-app`'s lib surface — follow-up kill.
- Window surfaces: `window_surface.rs` also calls `title()` for the
  window caption and applies `.color(..)`, which is a no-op on a
  `Galley` (`map_rich_text` passes it through), so a floating window's
  caption keeps the placeholder colour, i.e. the default text colour.
  Acceptable; noted for the snapshot follow-up.

## Process note

- Only main.rs edited, inside the `wgui2-tab-overflow` lock. KILLs B
  (layers_panel.rs) and C (viewport.rs, display_panel.rs) are disjoint.
- Gate for the dragon: `bash scripts/wave-battery.sh umber-app` on the
  host, or targeted `cargo test -p umber-app --bin umber -- tab_overflow_dock_tests`
  + clippy. Discriminance check: make `scroll_active_tabs_into_view`
  return early → `a_newly_active_tab_is_scrolled_into_the_strip_once`
  must go red.


## Ad-hoc verification addendum (2026-10-16, host-down static pass)

The 4 headless tests could not run — the build host went unreachable
mid-verification (ssh refused, then ping dead; the outage is logged in
docs/loop/queue.md). Static verification per the cargo-free ladder:

1. rustfmt `--check --edition 2021` on main.rs: PASS (run before the
   outage; bytes unchanged since).
2. Every egui_dock 0.21.1 / egui 0.36.2 API the kill calls resolved
   against the pinned registry sources (the claw did this; the dragon
   re-read the four helpers after landing).
3. Python port of `dock_tab_widths` + `dock_bar_width`: the width
   formula's 4 pinned cases pass; mutants killed: double-close
   (case 3) and no-buttons-subtracted (case 4). The
   leading-spacing mutant (spacing counted at each tab's start
   instead of its end) survives a TOTAL-sum comparison but is killed
   by the edge contract (sum(widths[..k]) == tab k's left edge), which
   is what `tab_widths_follow_egui_dock_layout` pins.

Verdict class: static/ad-hoc-verified, NOT suite-verified. The 4
headless tests remain OWED on host recovery, alongside the merged
wave gate. The elision galley test
(`the_tab_title_is_a_pre_laid_elided_galley`) needs a real egui
context and cannot be ported — it runs first on recovery.
