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
3. **The » menu without forking.** egui_dock exposes no overflow hook.
   It is drawn by us: a `TabViewer::add_popup` is the only built-in
   popup in the bar, and it is for adding tabs. The honest path is our
   own button at the bar's right edge, enabled only when the leaf
   overflows (sum of tab widths > `leaf.rect.width()`). It lists the
   leaf's tabs and sets `leaf.active` on pick. It is drawn after
   `DockArea::show` into the leaf's tab-bar rect, which is known from
   `LeafNode.rect` and `TabBarStyle::height`.
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

(3) and (4) depend on the leaf scroll field (`pub scroll: f32`) and
on egui_dock's tab-width arithmetic staying the same. Pin the
arithmetic in one helper with a test, so an egui_dock bump that
changes it fails loudly.
