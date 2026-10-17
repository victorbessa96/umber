# Kill report: g2-context-rail (GUI plan G2, widget half)

- Authorship: claw (dispatched by the dragon, W-GUI-2).
- Kill-lock `wgui2-context-rail`:
  `crates/umber-gui/src/widgets/context_rail.rs` (new),
  `crates/umber-gui/src/widgets/mod.rs`, `crates/umber-gui/src/lib.rs`.
- Implements: `docs/specs/gui-plan-2026-10-15.md:100` row
  `g2-properties-context` — the `umber-gui` widget only. The Properties
  dispatch in `main.rs` (held by `wgui2-gesture-integration`) is NOT
  touched; wiring is the follow-up kill.
- State: UNCOMPILED. No cargo was run (brief rule; the dragon gates on
  the build host). rustfmt `--check --edition 2021` is clean on all
  three files.

## Built

- `widgets/context_rail.rs` (new):
  - `ContextKind { Brush, Layer, Fill, Mask, TextureSet }` with `ALL`,
    `COUNT`, `index()`, `icon()` (lucide id) and `in_atlas()`.
  - `ContextRail` state: `active`, the applicable set
    (`set_applicable` dedups + rail-orders and falls back to the first
    applicable kind when the active one disappears; hidden kinds are
    absent, never drawn disabled), `select` (radio, no toggle-off,
    hidden kinds refused), registered property labels (`set_labels`;
    an identical list keeps the hit, a different list restarts),
    the search (`set_query`, `clear_search`, `matches`, `current_hit`,
    `next_hit` / `prev_hit` wrapping, `row_dimmed`), a one-shot focus
    request (`request_search_focus` / `take_search_focus`) and a
    one-shot scroll target (`take_scroll_target`).
  - Pure helpers: `ordered_kinds`, `breadcrumb_text` (joins with
    `" ▸ "`), `query_active`, `label_matches` (trimmed, lowercase
    substring), `match_indices`.
  - `ContextRail::ui(ui, &ContextRailCtx, body) -> (ContextRailOutcome, R)`:
    Ctrl+F (`Modifiers::COMMAND`) consumed → search focused; left a
    vertical icon rail (Panel family, `icon_button`, AccessKit
    `SelectableLabel` with selected state), right the breadcrumb (greyed
    at `DIM_ALPHA` while searching), the search field, and a vertical
    `ScrollArea` running the caller's `body` with a `PropertyRows`
    handle. `PropertyRows::row(ui, index, add)` greys non-matches and
    `scroll_to_me(Some(Align::Center))`s the hit jumped to this frame.
    Enter cycles forward, Shift+Enter back (focus is re-requested
    because a single-line `TextEdit` surrenders it on Enter), Escape
    clears.
  - `ContextRailOutcome { switched, search_focus, query_changed,
    jumped_to, cleared }` — returned-action shape, the shell dispatches
    after its closure.
- `widgets/mod.rs`: `pub mod context_rail;` + re-exports of
  `ContextKind, ContextRail, ContextRailCtx, ContextRailOutcome,
  PropertyRows`.
- `lib.rs`: the same five re-exported at the crate root; module doc
  line updated.

Tests (13, in-file):
`kinds_index_in_rail_order_and_name_their_icons`,
`hidden_kinds_are_absent_from_the_rail`,
`losing_the_active_kind_falls_back_to_the_first_applicable`,
`select_switches_once_and_reselect_is_a_noop`,
`search_is_a_case_insensitive_substring`,
`non_matches_grey_out_only_while_searching`,
`the_first_hit_is_the_first_match_in_row_order_scrolled_once`,
`enter_cycles_matches_and_wraps_both_ways`,
`editing_or_relabelling_restarts_at_the_first_hit`,
`the_breadcrumb_joins_its_segments`,
`a_focus_request_is_consumed_exactly_once`,
`ctrl_f_focuses_the_search_once_per_press` (headless `Context::run_ui`),
`typing_enter_and_escape_drive_the_search` (headless).

## egui 0.36.2 APIs used (each resolved in the registry source)

`InputState::consume_key(Modifiers, Key)` (+ `Modifiers::COMMAND`,
`Modifiers::CTRL`, `BitOr`), `InputState::key_pressed`,
`InputState::modifiers`, `Context::input` / `input_mut` / `memory`
(through `Ui: Deref<Target = Context>`), `Memory::has_focus`,
`TextEdit::singleline(..).id(Id).hint_text(String).desired_width(f32).show(ui)`
→ `TextEditOutput.response.response`, `Response::{changed, lost_focus,
request_focus, clicked, on_hover_text, widget_info, scroll_to_me}`,
`WidgetInfo::selected(WidgetType::SelectableLabel, ..)`,
`Ui::{horizontal_top, vertical, scope, separator, multiply_opacity, add}`,
`Label::new(..).truncate()`, `RichText::color`,
`ScrollArea::vertical().id_salt(..).auto_shrink([bool; 2]).show(..)`,
`Event::{Key { key, physical_key, pressed, repeat, modifiers },
ModifiersChanged, Text}`, `RawInput { events, .. }`,
`FullOutput.textures_delta.clear()`.

Scroll: egui 0.36.2 has NO `scroll_to_id`; the jump is
`Response::scroll_to_me` (response.rs:842) on the row's `scope` response.

## Judgment calls

- Icons: `Brush → paintbrush`, `Fill → paint-bucket`, `Mask →
  square-dashed` reuse declared atlas ids. `Layer → layers` and
  `TextureSet → layout-grid` are lucide ids NOT in the atlas
  (`TOOL_ICONS` / `PANEL_ICONS`, `assets/icons/lucide/`); no atlas entry
  was invented. They draw the atlas's placeholder box until a raster
  pass adds them; `in_atlas()` + a test pin flip on purpose then.
  Missing ids to add: `layers`, `layout-grid`.
- Rows are registered by index (`set_labels` in row order, then
  `rows.row(ui, i, ..)`), not by label string: duplicate labels across
  sections stay distinct and the hit order is the row order.
- Rows are greyed with `Ui::multiply_opacity(DIM_ALPHA = 0.35)`, not
  disabled: a non-matching control stays editable (search is a finder,
  not a filter). Alternative (`add_enabled_ui(false)`) rejected because
  it blocks edits mid-search.
- Shift+Enter cycles backwards — beyond the brief, one branch, cheap.
- Ctrl+F is consumed inside `ContextRail::ui`, so the shell must call
  `ui` only while Properties is visible, and Ctrl+F must not also be a
  `ShellAction` binding (it is not today: `actions.rs` binds no
  `Key::F`).
- The `body` closure receives `&PropertyRows` (a shared borrow of the
  rail) instead of the shell calling a second method after `ui`: the
  grey-out and the scroll must happen while the rows are being laid
  out, inside the scroll area.

## Ad-hoc verification (static, NOT suite-verified)

- rustfmt `--check --edition 2021`: clean on all three files.
- Every egui call resolved verbatim in `egui-0.36.2/src` (list above).
- Pure logic ported line-for-line to Python (scratch `hermes-verify-*`
  script, deleted after the run). The 10 pure Rust tests' pinned cases
  all pass against the port. 11 mutants, each killed by at least one
  case: case-sensitive match (substr/grey/first/cycle/restart),
  dropped inactive-query guard (substr), input-order rail (hidden,
  fallback), wrong separator (crumb), no fallback (fallback), hidden
  kind selectable (hidden), relabel-always restart (restart), no wrap
  (cycle), inverted grey-out (grey), sticky focus request (focus),
  sticky scroll target (first). The two headless frame tests are not
  covered by the port.

## Unverified

- Everything compiles only by construction (APIs resolved verbatim in
  the pinned source; not built). Two headless tests encode focus
  timing read from `memory/mod.rs` (`begin_pass` Escape handling,
  `end_pass` recently-gained-focus rule, `TextEdit` Enter →
  `surrender_focus`); if the battery finds a pass-timing mismatch, the
  pure tests still pin the semantics.
- `scroll_to_me` actually moving the scroll area has no assertion (a
  scroll offset read needs a laid-out tall body); `jumped_to` pins only
  WHICH row is asked to scroll.
- The breadcrumb's `▸` (U+25B8) glyph coverage in the Inter + stock
  fallback fonts is not checked.

## Deferred

- The Properties dispatch in `umber-app/src/main.rs` (per-target
  applicable kinds, labels per context, the kittest "selecting a fill
  layer shows the fill tab") — the follow-up wiring kill, after
  `wgui2-gesture-integration` releases main.rs.
- Atlas ids `layers` and `layout-grid` (raster pass + manifest census
  pins in `icons.rs`).
- i18n: the widget holds no strings; tooltips, breadcrumb segments and
  the search placeholder are the wiring kill's catalog keys.

## Process note

- Lock surface respected: only the three locked files changed. No
  cargo, no commits. The shared tree carries a sibling's dirty
  `umber-app` files and both `.ftl` catalogs; none were touched.
