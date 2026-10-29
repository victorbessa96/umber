# Kill report: 2026-10-29 g5-viewport-presets — the F1/F2/F4/F5 viewport presets

**Status: code landed + rustfmt clean; Gate 1 battery running at write
time (proc_80975d173492 regen, then wave-battery umber-app). Report
completes at the commit step.**

- **Kill**: `g5-viewport-presets` — the open tail of `g5-workspaces`
  (the 2026-10-15 kill report left layout persistence landed but the
  preset KEYS unbound; the gui-plan row said F1-F4). This kill binds
  them as registry actions over the two views, `Panel::Viewport` (3D)
  and `Panel::UvView` (2D).
- **Wave**: local-4e9bde4c's 2026-10-29 wave (claim entry in
  `.dragon/COORDINATION-next-wave.md`), wave-lock `wave-10-29-code`.
- **Authorship**: this session authored the bytes directly (no claw
  dispatch; the change is small and the registry contract is pinned by
  tests).
- **Kill-lock**: `wave-10-29-code` (11 files, registered before the
  first edit).
- **Design sources**: `docs/specs/gui-plan-2026-10-15.md` g5 row;
  `docs/loop/kill-reports/2026-10-15-g5-workspaces.md`; egui_dock 0.21.1
  vendored API (`TabPath.node/node_path/tab`, `DockState::leaf_mut`,
  `Node::{remove_tab, insert_tab}`, `LeafNode.tabs/.active`,
  `TabIndex(pub usize)`).

## What landed

- `crates/umber-app/src/toolbar.rs`: four `ShellAction` variants —
  `ViewportPresetBoth`, `ViewportPreset3d`, `ViewportPreset2d`,
  `ViewportPresetSwap`.
- `crates/umber-app/src/actions.rs`: BINDINGS rows `F1/F2/F4/F5` (F3
  keeps the command palette — **deviation from the plan's literal
  "F1-F4", recorded**: F3 was taken by g3's palette; the keymap editor
  can rebind), ALL_ACTIONS +4 (the exhaustive-list pin 25→29+tools),
  four `reference_text` lines, header comment updated.
- `crates/umber-app/src/workspaces.rs`: `ViewportPreset` enum +
  `apply_viewport_preset(dock, preset)` — Both/View3d/View2d dock or
  undock the views and activate the kept one; Swap exchanges the two
  tabs' slots in one leaf (flipping `active`), exchanges them across
  leaves when they live in different nodes, and with only one docked
  switches to the other. Seven new tests pin each arm plus
  idempotence, no-panel-lost, and survival across a workspace switch.
- `crates/umber-app/src/main.rs`: the dispatch arm — leaves any focus
  mode first (the `switch_workspace` rule), applies the preset to the
  live dock, then `save_layouts()` (a preset is a layout edit, so it
  persists like any other).
- `crates/umber-app/src/palette.rs`: four CATALOG entries under
  `menu.view` (`viewkeys.preset-*`).
- `assets/i18n/en-US.ftl` + `pt-BR.ftl`: `viewkeys.preset-both/-3d/-2d/
  -swap` in both locales (the i18n completeness test pins the pair).
- `docs/user/keyboard.md`: regenerated via
  `UMBER_REGEN_KEYBOARD_DOC=1 cargo test -p umber-app --bin umber
  keyboard_reference` (the drift test forbids hand edits).

## Judgment calls

- Presets act on the ACTIVE workspace's live layout only (not all four
  workspaces at once) — matches how every other layout edit behaves;
  the workspaces machinery carries them through switches and saves.
- No View-menu items added; palette + shortcuts + keyboard reference
  cover discoverability. The View menu can take them in a later pass if
  Bessa wants them there.
- No toast on preset apply (the layout change is self-evident, unlike
  the display-mode keys which change a hidden state).

## Not done (recorded, not silently dropped)

- Desktop checks 1-3 NOT run this wave: Bessa is working at the
  desktop (out-of-band message 2026-10-29 ~15:14, "stop taking
  screenshots right now") — the GUI drive needs the screen. The
  2026-10-29 debug binary was launched once for a census screenshot and
  killed cleanly; no Result lines filled; queue rows stay open.
- g7-keyboard-only, quick-mask design question, follow-up queue
  (compound one-step flatten, browser drag-drop/thumbnails): next
  waves.
