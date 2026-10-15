# GUI research: photocraft (egui 0.36 / eframe wgpu) → Umber

Source: https://github.com/storytold/photocraft @ main, crate `crates/ui-egui` (~340 source files).
Stack match: `egui = "0.36"`, `eframe 0.36` features `["wgpu", "accesskit"]`, `egui_kittest 0.36`
(`wgpu`, `snapshot`, `eframe`), `egui_extras 0.36` (svg). Same egui minor as Umber, so the APIs below
port as-is. photocraft does NOT use egui_dock: it uses its own dock (`dock.rs`) plus `craft_ui::docking`
from `storytold/craft-libs` (not read).

Effort: S = under 1 day, M = 1–3 days, L = a week or more.

## Patterns Umber lacks or does worse

### 1. Brush/tool cursor as an OS cursor bitmap (no frame lag)
- Files: `src/tool_cursor.rs`, `src/brush_cursor.rs`.
- What: the brush outline is rasterized into an `egui::CustomCursorImage` and handed to the OS
  through `ctx.set_cursor_image(Some(image))` with `CursorIcon::None`. The OS draws the outline, so it
  follows the hardware pointer instead of trailing it by render + present latency (their #1343).
  The painted fallback (`paint(painter, at, &shape)`) is used only where bitmaps fail: web, non-root
  viewports, Wayland at scale ≠ 1 (`scaled_wayland()`), and tips larger than `MAX_SIDE`
  (2048 on Win/mac, 255 on Linux because of X/HW cursor plane limits). Those fallbacks paint the
  outline and add a small OS crosshair at the hotspot.
- Lifecycle via an egui plugin:
  ```rust
  impl egui::plugin::Plugin for CursorLifecycle {
      fn on_begin_pass(&mut self, ui: &mut egui::Ui) { ui.ctx().set_cursor_image(None); }
      fn on_end_pass(&mut self, ui: &mut egui::Ui) { /* if a later widget set a cursor icon, drop the image;
          if None, fall back to Crosshair so a backend without bitmap upload still shows a pointer */ }
  }
  ```
- Caching: `Shape` enum (`Circle{radius,centre}`, `Crosshair`, `Hand`, `Zoom`, `Outline{rings}`) +
  `native_scale` key, cached in `ctx.data` temp under `Id::new("photocraft-tool-cursor")`.
  `brush_cursor::rings()` caches the traced tip outline keyed on tip/angle/roundness/hardness/flip.
  `tip_axes(xf, size)` maps the radius through the view transform (zoom, rotation, flip, pixel aspect).
  Below 3 px on screen it falls back to a crosshair (`MIN_RADIUS`). "Normal brush tip" uses the 50 %
  coverage radius (`half_coverage_radius(radius, hardness)`).
- `native_scale`: `zoom_factor()` on macOS (NSCursor uses points), `pixels_per_point()` elsewhere.
- Umber surface: the viewport/canvas widget (the missing in-viewport brush cursor). Start with the
  painted circle via `ui.painter()` + `CursorIcon::None`, then add the bitmap path behind the plugin.
- Effort: M (painted circle S; bitmap path, platform fallbacks and tests M).

### 2. Command palette (fuzzy search over menus + tools, with recents)
- File: `src/palette.rs` (`fuzzy_score`, `exact_bonus`, `note_recent`, `recent_hits`, `menu_hits`, `show`).
- What: a ⌘F overlay built from two `egui::Area`s: a scrim at `Order::Foreground` (click closes) and
  the card at `Order::Tooltip`, pivot `CENTER_TOP`, 110 pt from the top. The query, selection index
  and recent list live in `ctx.data` temp (`"palette-query"`, `"palette-sel"`, `"palette-recent"`).
  Rows are `allocate_exact_size` + painter text (label left, menu path + pretty shortcut right,
  disabled rows in `text_faint`). ↑/↓/Enter/Esc are read with `key_pressed`. Hits are capped at 12.
- Scoring: a subsequence match with +8 for contiguous chars, +5 for word starts, a length penalty,
  and +1000 for an exact name. It matches both the translated and the English label/path.
- Details worth copying:
  - `if !resp.has_focus() { resp.request_focus(); }`: only request focus when missing, because
    requesting it every frame interrupts the IME (a test checks `platform_output.ime`).
  - On Enter, `ctx.input_mut(|i| i.consume_key(NONE, Enter))` so a dialog opened in the same frame
    doesn't read the Enter as OK.
  - An empty query shows the recently run commands (cap 8, dedup, newest first).
- Umber surface: new overlay in umber-gui fed by Umber's menu/action list. Umber's Fluent ids give
  translated + source text to match on.
- Effort: S–M (S if Umber already has a flat action list with ids/labels/shortcuts).

### 3. Single binding table + focus-aware shortcut dispatch
- Files: `src/shortcut_dispatch.rs` (`bindings`, `Focus`, `Focus::allows`, `dispatch_pressed`,
  `dispatch`, `disabled_reason`, `claim_delete_keys`, `set_dry_run`, `take_log`), `src/shortcuts.rs`
  (`parse`, `pretty`, `shortcut_label`, `command_label`, `tip_label`, `key_matches`, `consume`).
- What:
  - One table merges command shortcuts, menu catalogue defaults, user overrides and alternate keys,
    sorted most modifiers first so ⇧⌘Z beats ⌘Z. The menus print from the same table, so any shortcut
    shown next to a menu item actually fires.
  - `Focus::of(ctx)` gives `None | Widget | DeletingWidget | Text`, from `ctx.text_edit_focused()`,
    `ctx.egui_wants_keyboard_input()` and a claimed delete owner. `allows(sc)` lets a focused widget
    keep its navigation keys, and a text field keep everything except ⌘ shortcuts (minus its own
    edit keys) and the F-keys.
  - Events are dispatched in arrival order and removed from `i.events`. The loop stops when a
    command hands the keyboard to something else, detected by comparing a `key_owner()` tuple
    (menu open, dialog count, prompt, …) before and after.
  - `POSITIONAL` keys (Period, Comma) match on `physical_key`, so non-US layouts still work.
  - A shortcut whose command is disabled writes "{label} is not available: {reason}" to the status
    bar instead of doing nothing silently.
  - When a shortcut consumes Tab it cancels egui's focus move: `m.move_focus(FocusDirection::None)`.
  - Test hooks: a dry-run flag and a 64-entry outcome log in `ctx.data`.
- Umber surface: Umber's keymap layer; this is the dispatch core the keymap editor (#4) needs.
- Effort: M.

### 4. Keymap editor with capture and conflict reporting
- Files: `src/prefs_ui.rs` (`shortcuts_body` ~L1661, capture block ~L1728, `shortcut_text`,
  `is_modifier_key`, `shortcut_items`), test `src/shortcut_capture_tests.rs`.
- What: a searchable 3-column `egui::Grid` (striped) inside a `ScrollArea` with
  `id_salt("shortcuts-scroll")`. Click a shortcut cell to capture. While capturing it scans
  `i.events` for the first `Event::Key{pressed:true}` that is not a bare modifier (egui reports
  `ControlLeft` etc. as keys, their #292), then `consume_key`s it.
  Esc cancels; ⌫/Delete with no modifiers removes the binding. A clash prints "X is already in use by
  Menu › Item and will be removed from it when you click OK." Alternate keys come from an "Add
  Shortcut" button. All editor state is in the dialog's `serde_json::Map` fields, so automation can
  drive it.
- Umber surface: new Preferences › Keymap page in umber-gui, on top of #3.
- Effort: M.

### 5. Toast/notice stack with hover-pause auto-hide
- File: `src/notices.rs` (`Notice`, `post`, `show`, `expire`, `MAX_NOTICES = 3`, `MAX_LINES = 8`).
- What: `egui::Area::new("photocraft-notices").order(Foreground).anchor(RIGHT_BOTTOM, vec2(-16,-36))`
  (clears the status bar). Cards are `Frame::NONE.fill(card).stroke(warning|card_border).shadow(..)`
  with a "×" button. Auto-hide starts from the first frame the notice is drawn (`shown_at`), and the
  timer resets while the pointer is over the stack and again on the frame the pointer leaves.
  Repaint strategy: `request_repaint_after(remaining)`, no continuous repaint. `request_repaint()`
  runs once when a card expires. Text is stored as English templates + `args` and translated
  at draw time, so it follows a language change. A notice can carry `dismiss_pref` to remember
  "don't show again". The cap drops the oldest *temporary* notice first.
- Umber surface: umber-gui widget + an app-level `notices: Vec<Notice>`. It replaces
  status-bar-only errors for import/export warnings.
- Effort: S.

### 6. Content-keyed layer / mask / history thumbnails
- Files: `src/lib.rs` (`layer_thumb`, `mask_thumb`, `snapshot_thumb`, `store_thumb`, `prune_thumbs`,
  `surface_fingerprint`, `thumb_image`, `layer_thumb_frame`), `src/panels.rs` (`draw_layer_thumb`,
  `layer_thumb_fit`), tests `src/layer_thumb_bounds_tests.rs`.
- What: a 64 px `ColorImage` per (layer id, kind), stored as `TextureHandle` in
  `HashMap<(LayerId,u8),(u64 rev, TextureHandle)>`. The rev is a fingerprint of the COW tile `Arc`
  pointers + default pixel + doc width + clip flag, so painting another layer doesn't rebuild this
  one. An existing texture is updated in place with `tex.set(img, LINEAR)`, not re-allocated.
  `prune_thumbs` walks the layer tree only when a document's `Weak` snapshot changes. History
  thumbnails use `compose::thumbnail(&snap.doc, 56)`, centred in a square, and are dropped when the
  snapshot leaves `live`. An option clips thumbnails to layer bounds.
- Umber surface: Layers panel rows (missing thumbnails) and the History panel. Key on Umber's tile
  revision or Arc pointers rather than the doc revision.
- Effort: M.

### 7. Custom color picker (2D field + component strip via `Mesh`)
- File: `src/color_picker_ui.rs` (`grid_mesh`, `flat_grid_mesh`, `color_field`, `field_color`,
  `slider_color`, `locate`, `out_of_gamut`, `not_web_safe`, `swatches`, `body`), tests in
  `src/color_tab_theme_tests.rs`, `layer_style/color_picker.rs`.
- What: the field is an `egui::Mesh` grid of 33×33 vertex colours (`m.colored_vertex` +
  `add_triangle`). The GPU interpolates between vertices, so there are no texture uploads per drag.
  Field and strip are `allocate_exact_size(.., Sense::click_and_drag())`; the value comes from
  `interact_pointer_pos()` clamped to 0..1. Modes: H/S/B/R/G/B + Lab L/a/b. HSB floats are kept
  separately (`Keep::Hsv`) so hue survives greys. The marker colour flips black/white on light
  colours. It also has new/current swatches with click-to-restore, an out-of-gamut / web-safe
  warning triangle, and hex + HSB/RGB/Lab/CMYK fields. While the picker is the top dialog the canvas
  acts as its eyedropper (`sample_at`). Every custom hit area gets `resp.widget_info(||
  WidgetInfo::labeled/selected(..))` for AccessKit.
- Umber surface: replace egui's stock picker in the Color panel / foreground-background chips.
- Effort: M (field + strip + hex S; Lab/CMYK + gamut warnings M).

### 8. Scrubby labels, wheel notches, Photoshop double-click, expression fields
- Files: `src/widgets.rs` (`scrub_label`, `scrub_option_label`, `label_scrub_response`,
  `wheel_notches_of`, `RowGestures`, `second_click`, `slider_row*`, `value_field_units`,
  `popup_value_field`), `src/numeric_expression.rs` (`parse_with_units`, `Units`),
  `src/scrub_wrap.rs`.
- What:
  - Dragging a label scrubs the value field right after it (1 step/pt, Shift ×10, Alt ×0.1). A
    fractional accumulator is stored in `ctx.data` under `resp.id.with("pc-scrub-acc")`, and the
    first drag frame counts from `press_origin()`. The label is only scrubby if a field linked to it
    last frame (`"pc-scrub-linked"`), and it sets `CursorIcon::ResizeHorizontal`.
  - Wheel over sliders accumulates fractional `MouseWheel` deltas (Point units ÷
    `line_scroll_speed`) into whole notches. `RowGestures { reset: Some(v) }` means a double-click
    resets the value.
  - Numeric fields take bounded arithmetic with units (`1in` in a px field converts at doc DPI).
  - `scrub_wrap.rs`: fullscreen scrubbing removes the cursor-warp jump from the value.
- Umber surface: `ValueField` / `ParamGroup` in umber-gui. Scrubbing and wheel notches are the
  pieces most clearly missing.
- Effort: S (scrub + wheel), M (expression parser with units).

### 9. Theme tokens published via `ctx.data` + multi-theme + live token reload
- File: `src/theme.rs` (`ThemeKind` with 14 themes, `Tokens`, `Tokens::get`, `apply`, `TOOLTIP_DELAY`,
  `ROW_GAP`, `mod live::LiveTokens`, `apply_overrides`).
- What: `Tokens` is a `Copy` struct of ~40 semantic colours + radii + layout flags (`bevel`, `pro`,
  `round_chips`). `apply()` inserts it under `Id::new("photocraft-theme")` and maps it onto
  `Visuals` (widgets inactive/hovered/active/open, selection, shadows, `ScrollStyle::thin()` with
  `interact_handle_opacity = 1.0` so the thin thumb doesn't vanish, `tooltip_width = 280`,
  `tooltip_delay = 0.35`). Then `set_style_of(Theme::Light, ..)` and `set_style_of(Theme::Dark, ..)`
  both get the same style, so an OS appearance flip can't swap in egui's default palette.
  `ctx.set_theme(ThemePreference::System)` is kept so OS appearance events still arrive.
- Dev loop: in debug builds `PHOTOCRAFT_THEME_FILE=tokens.json` is polled every 400 ms
  (`request_repaint_after`), overrides are applied by field name (macro), and unknown keys are
  logged.
- Umber surface: umber-gui already has theme tokens. Take the "both style branches" fix, the
  live-reload JSON overrides, and `cursor_outline()` (fixed black/white, independent of theme).
- Effort: S.

### 10. Preferences that sync egui style every frame (tooltips, animations, UI scale)
- File: `src/prefs_ui.rs` (`sync_tooltips`, `sync_animations`, `sync_display_scale`, `display_scale`,
  `preference_sections` search ~L1143).
- What: "Show tooltips" off sets `tooltip_delay = f32::INFINITY`. "Animations" off sets
  `animation_time = 0` + `ScrollAnimation::none()` via `all_styles_mut`. UI scale (Auto = native ppp,
  fractional values included) uses `ctx.set_zoom_factor(scale / native)`. Each is checked per frame
  and written only when it differs, because a theme change rebuilds the style. Settings have a
  search box ("Search settings…") that filters sections and rows.
- Also from `lib.rs::setup_context`: `o.zoom_with_keyboard = false` (⌘± zooms the canvas, not the UI).
- Umber surface: Preferences window; accessibility (reduced motion, scale).
- Effort: S.

### 11. Empty state / Home screen
- File: `src/canvas.rs` (`start_screen`, `open_on_double_click`, `home_recent`,
  `start_screen_drop_hint`), `src/chrome_ui.rs` (`ChromeState::home`), tests
  `src/empty_workspace_tests.rs`.
- What: with no document, a centred card (`scope_builder(UiBuilder::new().max_rect(card))`) shows
  the title, a subtitle, primary "New document…" and secondary "Open…" buttons whose labels include
  the live shortcut (`command_label`), a drop hint (Wayland gets a paste hint instead, since winit
  0.30 has no Wayland DnD), and up to 6 recent files with a hover × plus a context menu (Remove /
  Clear). Double-clicking empty workspace opens File › Open. The `ui.interact(area, ..)` call is
  registered *before* the card so the card's widgets keep their own clicks.
- Umber surface: viewport when no document is open (covers the onboarding gap).
- Effort: S.

### 12. Disabled-reason feedback + Photoshop enablement rules
- Files: `src/enable_rules.rs` (`disabled_for`, `background_blocks`, `kind_blocks`),
  `src/shortcut_dispatch.rs::disabled_reason`, `src/menus.rs::is_enabled`.
- What: one `is_enabled(app, id)` is used by menus, shortcuts and context menus, with layer-kind
  rules on top, so all three grey the same items. A shortcut on a disabled command explains why on
  the status bar ("no document open", engine reason, …).
- Umber surface: menu bar + shortcut layer. Feed the reason into a tooltip
  (`on_disabled_hover_text`) as well.
- Effort: S.

### 13. Dropdown type-ahead
- File: `src/widgets/type_ahead.rs` (`keys`, `list_open`, `PAUSE = 1.0`).
- What: in an open dropdown, typing a letter jumps to the next matching option, the same letter
  again cycles through matches, and quick typing builds a prefix. `list_open(ctx)` (pass-number
  stamp in `ctx.data`, valid for 2 passes) lets the shortcut layer give the keyboard to the list so
  letters don't switch tools.
- Umber surface: blend-mode and other dropdowns in umber-gui.
- Effort: S.

### 14. Responsive tab strips / dock groups
- Files: `src/tab_strip.rs`, `src/dock.rs` (`Group`, `DockLayout`, `show`, `resize`, `drop_before`,
  `persist`/`restore`/`apply`), `src/panel_docking.rs`.
- What: tabs shrink and elide with "…" and overflow into a » chevron menu; the selected tab always
  stays visible and no tab runs under the panel-menu button. Groups have fixed heights and the last
  expanded one fills the rest. Double-clicking a tab collapses its group, drag reorders tabs and
  groups, and Lock Workspace freezes the layout. The layout is serde and persisted in prefs.
- Umber: egui_dock already covers docking. The tab-overflow chevron and the collapse-to-strip
  double-click are the parts worth copying, via egui_dock's `TabViewer` / style hooks or a fork.
- Effort: M.

### 15. Text-field plugins (single-line drag, right-click menu)
- Files: `src/single_line_drag.rs` (`SingleLineDrag` plugin), `src/text_field_menu.rs` (plugin;
  only referenced, not read).
- What: before egui sees the input, the plugin rewrites the pointer's y onto the focused
  single-line `TextEdit` row, so drag-selecting doesn't jump to the start/end when the pointer drifts
  vertically. A field counts as single-line if it is no taller than two `interact_size` heights.
  The plugin is registered with `ctx.add_plugin(..)`.
- Umber surface: every ValueField / text input. A cheap polish win.
- Effort: S.

### 16. Platform-ordered dialog buttons
- File: `src/widgets.rs` (`DialogButton`, `ButtonRole`, `dialog_buttons`, `primary_button`,
  `secondary_button`).
- What: each modal declares its buttons by role, and `dialog_buttons` orders them per OS
  (`ctx.os() == Mac`), lays them out left to right so Tab follows reading order, and renders the
  `Default` role as the primary button.
- Umber surface: all umber-gui dialogs.
- Effort: S.

### 17. Held-key temporary tools
- File: `src/hold_keys.rs` (+ `hold_keys/tests.rs`, 36 KB).
- What: Space = Hand, ⌘Space / ⌥Space = zoom, ⌘ = Move, `~` = erase while held. The current tool is
  never changed, so releasing the key restores the previous one, and a drag started under a held
  tool lasts until button-up. Space during a marquee drag repositions it.
- Umber surface: viewport input layer.
- Effort: M.

### 18. AccessKit labelling of painter-drawn widgets
- Evidence: `resp.widget_info(|| egui::WidgetInfo::labeled(WidgetType::Button, enabled, label))` /
  `WidgetInfo::selected(WidgetType::RadioButton|Checkbox|SelectableLabel, ..)` in `actions.rs`,
  `color_picker_ui.rs`, `layer_row_ui.rs`, `panels.rs` (toolbar, D/X colour buttons, footer
  buttons).
- Why: every `allocate_exact_size` + painter widget becomes queryable, both by screen readers and
  by kittest (`get_by_role_and_label(Role::Button, "B")`, `query_all_by_role(Role::CheckBox)`).
- Umber surface: SwatchGrid, ToolRow, the layer rows and the new picker. It is also what makes
  the tests in #19 possible.
- Effort: S per widget.

### 19. Test harness patterns (kittest at scale)
- Files: `src/shortcut_capture_tests.rs`, `src/empty_workspace_tests.rs`,
  `src/color_tab_theme_tests.rs`, `tests/checker_canvas.rs`, plus about 100 `*_tests.rs` modules.
- Patterns:
  - `Harness::builder().with_size(..).with_pixels_per_point(ppp).with_max_steps(64).build_eframe(|cc| {
    App::setup_context(&cc.egui_ctx, ..); App::new(..) })` runs the full app.
    `build_ui_state(|ui, app| ..)` renders a single panel (e.g. `panels::dock_body(app, ui, Group::Color, tab)`).
  - Fonts gate: return early until `ctx.fonts(|f| f.families().contains(&FontFamily::Name("medium")))`,
    then `run_steps(3..4)`. Named families panic on the first frame otherwise.
  - Theme sweeps: `for kind in ThemeKind::ALL { .. }` with real drag/drop on a computed point.
  - The widget publishes its origin rect in `ctx.data` (`"color-tab-test-origin"`) so tests can
    compute hit points without hard-coding layout.
  - Input: `h.event(Event::Key{..})`, `key_press`, `hover_at`/`drag_at`/`drop_at`, and a fixed
    step dt (`with_step_dt(1/60)`) for double-click timing.
  - GPU pixel tests: `.wgpu().build_eframe(..)` with `cc.wgpu_render_state`, reading pixels back
    offscreen, and skipped when no adapter is present.
  - Restart persistence: serialize prefs, construct a new app, feed raw `Event::Key` into a bare
    `Context::begin_pass/end_pass`, then assert.
  - Assertions go mostly through app state and AccessKit roles rather than image snapshots, which
    keeps the tests stable across fonts and themes.
- Umber surface: grow from 2 snapshot tests to behaviour tests per panel. Port the fonts gate and
  the theme sweep first.
- Effort: S to start, ongoing.

### 20. Alt-drag on-canvas brush resize with HUD
- File: `src/brush_resize.rs`.
- What: Alt+right-drag (Win/Linux) or ⌃⌥-drag (mac): horizontal sets diameter, vertical sets
  hardness. The circle stays at the drag origin, tinted by hardness, with a Diameter/Hardness
  readout. All `tools.setBrush` calls share one `coalesce` key, so the drag is one journal entry and
  no history step.
- Umber surface: viewport; pairs with #1.
- Effort: S–M.

## egui techniques (cross-cutting)

- Repaint strategy: reactive by default. Timers use `request_repaint_after(remaining)` (notices,
  live tokens at 400 ms, screenshots at 16 ms only while pending). `request_repaint()` is called only
  for state that egui can't see: queued synthetic input, files hovered over the window (no pointer
  events arrive then), an expired notice, and the font warm-up frame. A comment in `lib.rs::logic`
  records that polling every 50 ms had made the idle app render at 20 fps. Long work goes to worker
  threads (`jobs_ui.rs`, `background_redraw.rs`, `object_finder.rs` with a 0.12 s hover debounce),
  and frames keep coming only while a job runs (`jobs_ui::keep_polling`).
- App structure: eframe 0.36's split `fn logic(&mut self, ctx, frame)` (input, shortcuts, jobs,
  prefs tick) plus `fn ui(&mut self, ui, frame)` (drawing), plus `raw_input_hook` to rewrite input
  before egui sees it (pinch folding, clipboard keys, native menu keys, synthetic events). Panels are
  drawn in a fixed order and overlays (palette, dialogs, notices) after the central panel.
- Plugins: `ctx.add_plugin(..)` with `on_begin_pass`/`on_end_pass` for cross-cutting concerns
  (cursor image lifecycle, single-line drag, text-field context menu). This is a clean place for
  behaviour that would otherwise have to be threaded through every widget.
- Id handling: stable string ids for singletons (`Id::new("palette-query")`, `"photocraft-notices"`),
  `resp.id.with("pc-scrub-acc")` / `id.with("wheel-acc")` for per-widget side state, and `id_salt`
  on scroll areas and grids. Cross-widget coordination uses ids stored in data
  (`claim_delete_keys` stores the owner `Id` and compares it with `memory.focused()`). Pass-number
  stamps (`cumulative_pass_nr`) mark "was drawn recently" (dropdown open, scrub link).
- State storage: three tiers. (1) Durable, serde UI state in `UiState` (notices, dock layout,
  dialogs as `serde_json::Map` fields), which the control channel can inspect and set. (2) Ephemeral
  per-widget state in `ctx.data` temp (`insert_temp/get_temp`, `get_temp_mut_or_default`).
  (3) GPU resources as `TextureHandle` maps on the app, keyed by content fingerprint and updated with
  `tex.set`.
- Theming approach: one `Tokens` struct, published to `ctx.data` and read by every widget with
  `Tokens::get(ctx)`, never hard-coded. `apply()` maps tokens onto `Visuals` + `Spacing`, then
  writes the same style to both the light and dark branches. Layout flags inside tokens (`pro`,
  `bevel`) switch widget grammar (pill tabs vs tab strips, bevels). Fixed contrast colours are kept
  apart for canvas overlays (`cursor_outline`). Fonts ship deflated and are installed once in
  `setup_context`, with a one-frame skip until named families exist.
- Input discipline: shortcuts consume their events (`i.events.remove`, `consume_key`) so later
  widgets don't double-handle them. Modals that take all input use `egui::Modal`. Overlays use
  explicit `Order` (Foreground scrim, Tooltip card).
- i18n: English source strings are kept in state and translated at draw time (`tl!`, `i18n::fmt`
  with `{name}` args), so a language change updates existing notices and labels. 22 TSV catalogs are
  deflated by `build.rs`. (Umber uses Fluent; the store-template-translate-at-draw rule carries over.)
- Testing approach: see #19. kittest + AccessKit queries + app-state assertions, real GPU pixel
  tests in `tests/` that skip without an adapter, dry-run dispatch logs in `ctx.data`, and per-feature
  `*_tests.rs` sibling modules (`#[path = "..._tests.rs"] mod ..`).

## Priority for Umber (by gap × effort)

1. #1 brush cursor (M) and #20 resize HUD: closes the top missing feature.
2. #2 palette (S–M) → #3 dispatch (M) → #4 keymap editor (M): palette first; editor depends on the
   unified table.
3. #6 layer thumbnails (M), #5 notices (S), #11 home screen (S).
4. #7 color picker (M).
5. Polish batch (all S): #8 scrub/wheel, #9 both-branch style + live tokens, #10 prefs sync,
   #12 disabled reasons, #13 type-ahead, #15 single-line drag, #16 dialog buttons, #18 AccessKit info.
6. #19 test patterns throughout; #14 / #17 as time allows.

## Files read

Fetched to `~/.hermes/cache/scratch/pc/` and read (whole or the relevant ranges):
- `README.md`, `ROADMAP.md` (the UI rows only; ROADMAP gives UI/UX fidelity as 40 %), root `Cargo.toml`, `crates/ui-egui/Cargo.toml`
- `src/theme.rs`, `src/tool_cursor.rs`, `src/brush_cursor.rs`, `src/brush_resize.rs` (header)
- `src/notices.rs`, `src/palette.rs`
- `src/shortcuts.rs`, `src/shortcut_dispatch.rs`, `src/menu_catalog.rs` (header), `src/enable_rules.rs`,
  `src/menus.rs` (symbols)
- `src/prefs_ui.rs` (sync_*, shortcuts editor, capture, settings search)
- `src/color_picker_ui.rs` (mesh field/strip, symbols)
- `src/widgets.rs` (symbols; scrub, wheel, dialog buttons), `src/widgets/type_ahead.rs`,
  `src/numeric_expression.rs` (header), `src/scrub_wrap.rs` (header)
- `src/lib.rs` (`logic`, `raw_input_hook`, `ui`, `setup_context`, thumbnail cache)
- `src/canvas.rs` (start screen / empty workspace region only), `src/chrome_ui.rs` (header)
- `src/dock.rs` (header + symbols), `src/panel_docking.rs` (header), `src/tab_strip.rs` (header)
- `src/layer_row_ui.rs` (header), `src/quick_pick.rs`, `src/object_finder.rs` (header),
  `src/hold_keys.rs` (header), `src/tool_feedback.rs` (header), `src/single_line_drag.rs` (header),
  `src/background_redraw.rs` (header), `src/jobs_ui.rs` (header), `src/icons.rs` (header + symbols),
  `src/swatches_ui.rs` (header)
- Tests: `src/empty_workspace_tests.rs`, `src/shortcut_capture_tests.rs`, `src/color_tab_theme_tests.rs`,
  `tests/checker_canvas.rs` (harness part)
- Grepped across all fetched files: `widget_info`/AccessKit usage, `on_hover_ui`/tooltip usage
  (none beyond `on_hover_text`; there is no rich-tooltip pattern to port).

Not read: `craft-ui` (external crate behind docking/tabs/icon buttons), `panels.rs` beyond grep hits,
`text_field_menu.rs`, `i18n/mod.rs`, benches/examples.
