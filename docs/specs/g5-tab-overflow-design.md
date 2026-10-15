# g5-tab-overflow: what egui_dock 0.21.1 gives, and what we add

Date: 2026-10-15. Author: dragon. Static research from the registry
source (`egui_dock-0.21.1/src/widgets/dock_area/show/leaf.rs`, egui
0.36.2 `widget_text.rs`). Code waits on `main.rs`, which is locked by
`g5-workspace-persist` until the build host returns.

## The plan row asks for three things

1. Long tab titles elide with "…".
2. Overflowing tabs go into a » menu.
3. The selected tab is always visible.

## What egui_dock already does

- **A scrollable tab bar.** When the tabs are wider than the bar, the
  bar scrolls (`LeafNode.scroll`, wheel or drag), with a scroll bar
  (`TabBarStyle::show_scroll_bar_on_overflow`, default true). Overflowing
  tabs are reachable today; they just aren't listed.
- **No elision.** `tab_title` lays the title out with
  `into_galley(ui, None, f32::INFINITY, Button)`: an infinite width, so
  the tab grows to fit its text. `TabStyle::minimum_width` is a floor,
  never a cap.
- **No scroll-to-active.** Nothing moves `leaf.scroll` when the active
  tab changes. Ctrl+PgDn or a palette "focus panel" can activate a tab
  that is scrolled out of view.

## The design

1. **Elide in `title()`, which we own.** `TabViewer::title` returns a
   `WidgetText`. Returning `WidgetText::LayoutJob` with `wrap.max_width =
   TAB_TITLE_MAX`, `max_rows = 1`, `break_anywhere = true` and the "…"
   overflow character elides it, BUT `into_galley_impl` overwrites
   `job.wrap` with the caller's (infinite) wrapping. So the elision must
   be pre-laid: return `WidgetText::Galley(Arc<Galley>)`, which is passed
   through untouched. `title()` gets no `Ui`, so the galley is built from
   a `Context` clone held by `PanelViewer`
   (`ctx.fonts_mut(|f| f.layout_job(job))`). The full title goes in the
   hover tooltip via `on_tab_button(tab, response)`, which hands us the
   tab's `Response`.
2. **`TAB_TITLE_MAX` from the real strings.** The longest titles are
   en-US "Texture Sets" (12 chars) and pt-BR "Conjuntos de texturas"
   (21). A cap of about 140 px at the Button size keeps every en-US
   title whole and elides only that one pt-BR title ("Conjuntos de
   tex…"), so the elision is the exception, not the norm.
3. **The » menu through the add-button hook, no forking.** egui_dock
   reserves space at the bar's end for an add button
   (`DockArea::show_add_buttons`, `TAB_ADD_BUTTON_SIZE` taken from the
   available width before the tabs lay out) and shows our popup under it
   (`show_add_popup` → `TabViewer::add_popup(ui, path)`, leaf.rs:660).
   The popup gets the leaf's `NodePath`, so it lists that leaf's tabs.
   Picking one is recorded as `(path, index)` and applied after
   `DockArea::show` returns: `dock_state[path]` → `set_active_tab`, plus
   the scroll from (4).
   - The button is a "+" glyph by default; `Style::add_tab_*` sets its
     colours only, so the glyph stays "+". Either live with "+"
     reading as "panels in this group" (it also lists closed panels,
     which IS adding), or paint a » over its rect via `on_add`'s
     neighbour hook. The first is honest and zero-risk: the popup lists
     this leaf's tabs (pick = activate) and, under a separator, the
     panels not open anywhere (pick = add here).
   - The button shows on every leaf, overflowing or not. That matches
     Painter, where every panel group has its menu.
   - Reopening a closed panel already works through Window ▸ Panels
     (`shell_menu.rs`); the popup's second list only adds a closer
     route, placing it in THIS group rather than the default dock spot.
4. **Selected tab visible.** It falls out of (3): picking from » also
   sets `leaf.scroll` so the active tab's x lands inside the bar. Same
   for the keyboard path (`Ctrl+PgDn`). Width per tab is recomputed with
   the same galley + 16 px spacing + close-button width as `tab_title`,
   so the sum matches egui_dock's own layout.

## Tests (kittest)

- A pt-BR viewer at a narrow width shows "Conjuntos de tex…" and the
  tooltip has the full title.
- An eleven-tab leaf at 300 px shows the » button; picking the last tab
  makes it active AND its rect lies inside the bar.
- At 1600 px no » button exists (no overflow, no clutter).

## Risk

(3) uses only public hooks (`show_add_buttons`, `show_add_popup`,
`add_popup`). (4) depends on the leaf scroll field (`pub scroll: f32`) and
on egui_dock's tab-width arithmetic staying the same. Pin the
arithmetic in one helper with a test, so an egui_dock bump that
changes it fails loudly.
