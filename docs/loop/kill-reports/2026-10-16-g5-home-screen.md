# Kill report: g5-home-screen (GUI plan G5)

- Authorship: claw (dispatched by the dragon, brief
  `docs/loop/briefs/2026-10-16-g5-home-screen.md`).
- Kill-lock `g5-home-screen`: `crates/umber-app/src/empty_state.rs`,
  `crates/umber-app/src/viewport.rs`, `crates/umber-app/src/recovery.rs`
  (+ the workspace-root `assets/i18n/en-US.ftl` / `pt-BR.ftl`, granted by
  the brief).
- Implements: `docs/specs/gui-plan-2026-10-15.md` row `g5-home-screen`.
- Status: UNCOMMITTED, NOT COMPILED. No cargo ran (brief rule); rustfmt
  `--check` is clean on all three files. The battery is the dragon's.

## Built

- `empty_state.rs` — the painter-only hero becomes `home_card(ui, rect,
  files_hovering, Option<HomeCardInput>) -> Option<HomeCardAction>`, a
  real egui child `Ui` (`new_child` + `UiBuilder::max_rect`, top-down
  centered, inside a vertical `ScrollArea`) over the viewport:
  - title / hint / formats line from the existing `shell.hero-*` keys;
    the accent wash + outline while files hover, as before;
  - New project / Open… buttons; shortcut text derived from
    `actions::shortcut_label` (Open shows Ctrl/Cmd+O; New is unbound in
    the key table, so it shows none);
  - up to `HOME_RECENT_MAX` = 6 recent rows (name + dimmed parent dir),
    hover × = remove, right-click menu Remove / Clear all, click =
    `OpenRecent(path)`; rows whose path is gone draw faint, ignore the
    click, and say why on hover (`menu.recent-missing`);
  - "Recover last session" row only when `HomeCardInput::recovery` is
    `Some` (the shell's `pending_recovery`), click = `RecoverSession`;
    tooltip names the source project when there is one;
  - Remove / Clear edit `&mut RecentProjects` and persist via
    `RecentProjects::save(recent_path())`, the File menu's own path.
  - `HomeCardAction { NewProject, OpenProject, OpenRecent(PathBuf),
    RecoverSession }`, `HomeCardInput { recent, recovery }`.
  - Pure seams: `home_rows`, `row_labels`, `row_colors`,
    `row_click_action`, `resolve_action`, `apply_edit`, `content_height`,
    `card_rect` (grows `zone_rect` to the content, capped at the inset).
- `viewport.rs` — `ui` keeps its signature and forwards to the new
  `ui_with_home(…, home: Option<HomeCardInput>)`. With no mesh it draws
  the card after the viewport's own response (so card widgets win the
  hit test), resolves the card click or a double-click on the viewport
  (= Open) via `resolve_action`, and stores it in `home_action`;
  `take_home_action()` drains it. The `Option<ViewportClick>` return is
  untouched.
- `recovery.rs` — `RecoveryMeta::source_name()` (the source project's
  leaf, for the Recover row's tooltip) + its test; module doc names the
  home card as a `pending` reader.
- i18n (both catalogs, new `home` message): `home.new-project`,
  `home.open-project`, `home.open-tip`, `home.recent`,
  `home.recent-remove`, `home.recent-clear`, `home.recover`,
  `home.recover-tip`. Reused: `shell.hero-title/-release/-hint/-formats`,
  `shell.tip-new`, `menu.recent-missing`.

## Tests (pure, `empty_state::tests` unless noted)

Kept: `every_listed_extension_is_one_the_loader_reads`,
`mesh_routing_is_case_insensitive_and_rejects_maps_and_folders`,
`formats_line_lists_every_extension_in_order`, `zone_insets_then_caps`.
New: `the_card_grows_with_its_content_but_never_past_the_inset`,
`content_height_counts_rows_and_the_recover_row`,
`the_card_lists_at_most_six_most_recent_first`,
`only_the_listed_rows_are_checked_and_missing_ones_flag`,
`missing_rows_gray_out_and_ignore_clicks`,
`row_labels_split_name_and_parent`,
`a_card_click_wins_over_double_click_open`,
`remove_and_clear_edit_the_recent_list`,
`the_buttons_show_their_bound_shortcuts_only`;
`recovery::tests::source_name_is_the_project_leaf_or_none`.

## Judgment calls

- 6 of 10: `RecentProjects` caps at `MAX_RECENT` = 10 (the File menu's
  list). The card shows the first 6 instead of lowering the shared cap,
  and stats only those 6 paths per frame (pinned).
- Return channel: a stored `home_action` + `take_home_action()` rather
  than a new `ViewportClick` variant or a tuple return — main.rs matches
  `ViewportClick` exhaustively (outside the lock), and a new variant
  would break the build until the dragon wires it. Rejected alternative
  is cleaner long-term; the dragon can fold it in at integration.
- `ui` stays as-is (forwards with `home = None`), so main.rs compiles
  unchanged; the card then shows New / Open only until main.rs calls
  `ui_with_home` with `HomeCardInput { recent: &mut self.recent,
  recovery: self.pending_recovery.as_ref() }` (PanelViewer would need
  those two borrows added — main.rs, outside the lock).
- `#[allow(dead_code)]` on `HomeCardAction`, `HomeCardInput`,
  `home_action` and `take_home_action`: the bin target cannot read them
  until main.rs wires them. Remove the allows at integration.
- `allocate_ui_at_rect` (the brief's name) is gone in egui 0.36; used
  `Ui::new_child(UiBuilder::new().max_rect(..))`, no cursor advance (the
  card overlays the viewport rect the viewport already allocated).
- Missing-path rows stay removable (× and context menu) but do not open.
- Double-click = Open fires anywhere the viewport response receives it
  (empty space inside or outside the card), not on the card's widgets.
- Card height is an estimate (`content_height`) from fixed line heights,
  not a measured sizing pass; a ScrollArea covers any shortfall.
- The ×/remove persist happens inside the card (via `recent_path()`),
  not through the shell's `save_recent` (private to main.rs); same file,
  same format.
- Recover row tooltip is plain concatenation (`tip\nname`), not a
  Fluent placeable — keeps the key argument-free in both catalogs.

## Unverified

- Everything compiles only by construction: every egui call was resolved
  against the pinned egui-0.36.2 registry source (`new_child`,
  `UiBuilder::{id_salt,max_rect,layout}`, `ScrollArea::{id_salt,
  auto_shrink,max_height}`, `Button::{shortcut_text,frame,min_size}`,
  `Response::{context_menu,widget_info,double_clicked}`, `Ui::{place,
  close,rect_contains_pointer,allocate_exact_size}`,
  `WidgetInfo::labeled`, `RichText::{size,font,color}`), but no cargo
  build, clippy or test run happened.
- Hit-test ordering (card widgets over the viewport's `click_and_drag`
  response) and the ScrollArea's drag sense not eating the empty-space
  double-click: reasoned from egui's topmost-widget rule, not exercised.
- No kittest / snapshot (brief: not required this kill; the plan row
  names "kittest: recent row opens; snapshot" for integration).

## Deferred

- main.rs wiring (dragon's): pass `HomeCardInput` via PanelViewer, read
  `viewport.take_home_action()` after `ui_with_home`, dispatch
  NewProject → `new_project()`, OpenProject → `open_project()`,
  OpenRecent → `open_project_at(dir)` (its missing-dir removal already
  exists), RecoverSession → the `recovery_prompt` Recover arm
  (`pending_recovery = None; recover(&meta)`). Then drop the
  `dead_code` allows.
- The plan row's "paste hint on Wayland" — not in the brief; untouched.
- Live-shortcut for New: none until the key table binds Ctrl+N.

## Process note

- Shared tree had sibling dirt outside this lock (`uv_view.rs`,
  `umber-gpu/src/texture_display.rs`, g3/g6 locks live); none touched.
  The ftl catalogs got one new `home` block each, inserted before the
  Texture Sets section — a sibling editing the catalogs merges by hunk.
