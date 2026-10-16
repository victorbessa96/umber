# Kill report: g3-keymap-editor (GUI plan G3)

Date: 2026-10-16. Lock: `crates/umber-app/src/actions.rs`,
`crates/umber-app/src/preferences.rs`, `crates/umber-app/src/main.rs`
(minimal wiring), `assets/i18n/{en-US,pt-BR}.ftl`, this report.
Spec: `docs/specs/gui-plan-2026-10-15.md` row g3-keymap-editor; brief
`docs/loop/briefs/2026-10-16-g3-keymap-editor.md`.

## Built

- **Override model (`actions.rs`)**: `Overrides` is the per-action delta
  (full binding list, ≤ `MAX_BINDINGS` = 2, empty = unbound, entries equal
  to the defaults are dropped, kept in `ALL_ACTIONS` order). `table_for`
  builds the effective table: the defaults minus overridden actions, plus the
  override rows, stable-sorted most-modifiers-first. A thread-local cache
  (`set_overrides`) feeds the one funnel `with_table`. `binding_of`,
  `resolve_binding` (the poll), `shortcut_label` (menus, tooltips, status
  hints, home card) and `shortcut_rows` (Help ▸ Keyboard) all read it, so
  no consumer got a separate override-aware copy. `BINDINGS` is still the
  default table, and the generated `docs/user/keyboard.md` is unchanged.
- **Persistence**: `keymap=Action:spec,spec;Action:spec` on one line in
  the preferences file (`Preferences::keymap_overrides`). The action name is
  the Debug name (`SelectTool(Erase)`), and a spec is `binding_spec`
  (`ctrl+shift+z`, `tab`, `openbracket`). Parsing is lenient: unknown
  actions are skipped, and an entry whose specs are all invalid is skipped,
  so a typo cannot unbind an action. An empty or absent line means no
  overrides. The serialize/parse round trip is exact.
- **Capture**: `capture_shape(key, mods)` returns Assign(Chord), Cancel
  (Esc), Clear (Backspace) or Ignore (Alt, or macOS Control without Cmd).
  `main.rs`'s `raw_input_hook` hands the raw events to
  `KeymapEditor::intercept` before egui and the poll see them. The captured
  press therefore never fires its current action, and Tab is not taken by
  focus traversal. `apply_capture` assigns, refuses on conflict, or clears.
  Clearing an overridden action restores its defaults. Clearing a default
  action unbinds that slot.
- **Conflicts**: `conflict_in(table, action, binding)` checks exact binding
  equality against defaults plus overrides. When it finds one, the row shows
  "Conflicts with <action>: not assigned." and nothing changes.
  `first_conflict` checks pasted lines.
- **Page (`preferences::keymap_section`)**: a preset combo, its own search
  box, a striped 3-column grid (action label, binding, alternate), "Copy
  changed bindings" (`ctx.copy_text(export_line)`, which is the `keymap=`
  delta), and a paste box with "Paste bindings" (`parse_pasted`). A paste
  that is invalid or contains a conflict is refused whole and the reason is
  shown. Search (`keymap_row_matches`) covers the localized label, the
  English reference label, or every key token: "ctrl d", "Ctrl+S", "cmd s",
  "[", "f3". The window-level search also reaches the section.
- **Presets**: Umber (default) clears all overrides. Blender-like sets
  three swaps: Shift+Z wireframe, Z lit/unlit shading, Ctrl+I invert mask.
  The combo shows Custom for anything else.
- **main.rs wiring** (about 50 lines):
  - a `keymap_editor` field;
  - `set_overrides` at load time and whenever the overrides change;
  - `prefs` is cloned now, because it is no longer `Copy`;
  - capture is cancelled when the window closes;
  - the Tab→Hide-UI raw-input steal now runs only while bare Tab still
    resolves to Hide UI;
  - `actions::poll_override_keys` runs after `poll_shortcuts`.
- **Catalogs**: 18 keys in both en-US and pt-BR under `prefs`:
  `section-keymap`, `keymap-preset`, `keymap-preset-umber`,
  `keymap-preset-blender`, `keymap-preset-custom`, `keymap-search`,
  `keymap-col-action`, `keymap-col-binding`, `keymap-col-alt`,
  `keymap-unbound`, `keymap-press-keys`, `keymap-button-tip`,
  `keymap-conflict` (`$action`), `keymap-copy`, `keymap-paste`,
  `keymap-paste-hint`, `keymap-paste-invalid`, `keymap-paste-conflict`
  (`$key $first $second`).
- **Tests**:
  - actions.rs: `every_action_is_listed`,
    `binding_specs_round_trip_for_every_default_row`,
    `overrides_round_trip_through_the_line`,
    `an_override_changes_every_consumer`,
    `conflicts_name_the_owning_action`,
    `capture_maps_the_pressed_shape_to_a_chord`,
    `presets_are_conflict_free_and_recognised`.
  - preferences.rs: `keymap_line_round_trips_and_is_lenient`,
    `capture_assigns_refuses_conflicts_and_clears`,
    `intercept_takes_keys_only_while_capturing`,
    `keymap_search_matches_labels_and_keys`,
    `export_is_the_delta_and_paste_reverses_it`,
    `every_keymap_key_is_in_both_catalogs`.
  - The existing `round_trip_and_defaults` now also carries the Blender
    overrides.

## Judgment calls

- **Thread-local effective table** rather than threading `&Overrides`
  through every caller. The signatures of `binding_of`, `shortcut_label` and
  `resolve_binding` stay the same, so empty_state.rs, rich_tooltip.rs,
  palette.rs and toolbar.rs needed no edits. This is the same pattern as
  i18n's current catalog. Tests run one per thread, and the tests that
  install overrides reset them at the end.
- **The `toolbar::SHORTCUT_KEYS` pre-filter** only covers the default
  keys, and toolbar.rs is outside my lock. Instead of editing it,
  `actions::poll_override_keys` polls any key that only an override binds,
  using the same `resolve_binding` and `Focus::allows`, and consumes it.
  The `shortcut_keys_are_exactly_the_bound_keys` guard still holds against
  the defaults.
- **Persisted format**: the brief's `action:spec;...` is extended with
  `,` for the alternate binding (`CommandPalette:ctrl+shift+p,f3`), and
  `Action:` with nothing after it means unbound. Without these the
  3-column grid and Backspace-to-clear could not be persisted.
- **Ctrl+C / X / V cannot be captured**: egui-winit turns them into
  Copy/Cut/Paste events and never sends the key, so a binding on them
  could never fire. While capturing they are swallowed and nothing is
  assigned.
- **Clear on a default slot** writes an override that unbinds it (for
  example, it drops F3 while keeping Ctrl+Shift+P). Clear on an overridden
  action restores its defaults, as the brief specifies.
- **Row labels** use the palette's i18n label (localized). Actions not in
  the palette fall back to the English `reference_text`, exposed through a
  new pub `reference_label`.
- **Blender-like swaps that were skipped because no action exists for
  them**: G/R/S grab/rotate/scale, Ctrl+Space already matches, H hide,
  Numpad views (those belong to `NavPreset`), and A select-all. Paint
  letters other than E have no `SelectTool` binding in the default table,
  and the brief says not to invent actions.

## Unverified

- **Nothing compiled or ran here.** The brief forbids cargo. Only
  `rustfmt --check` passed, on actions.rs and preferences.rs. The dragon's
  battery has to confirm the build, clippy `-D warnings` and the tests.
- **The capture flow is untested in a running app**: click → press →
  assign/conflict, Tab capture, macOS ⌘ spellings. There is no kittest.
  The plan row's "kittest capture flow" is deferred.
- **pt-BR wording is mine.** It should go through `wave-i18n-audit`.
- **Tests that install overrides would leak them on panic** under
  `--test-threads=1`, because every test then shares the main thread.
  Each test resets at its end.

## Deferred

- The kittest capture flow.
- Right-click → "Assign Shortcut" from `prop_menu.rs`, which the brief
  places outside this kill.
- Showing the effective (overridden) key in `docs/user/keyboard.md` is
  intentionally not done: the doc documents the defaults.

## Process note

Other sessions edited both catalogs and main.rs at the same time
(g5-home-screen, g6-uv-view-parity). All my catalog and main.rs edits are
anchored inserts or replacements, and no file was rewritten whole.
