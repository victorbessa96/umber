# Kill report: G5 menu completion, Close Project and Help ▸ Keyboard shortcuts

Date: 2026-10-15. Author: dragon. Lock: `g5-menu-shortcuts` over
`crates/umber-app/src/{main,actions}.rs`, both catalogs, the GUI plan and
this report.

## Built

- **Help ▸ Keyboard Shortcuts**: a window listing every binding as key,
  action and where it fires, in this platform's spelling.
  - It reads `actions::shortcut_rows(is_mac)`, the same rows
    `docs/user/keyboard.md` is generated from.
  - `keyboard_reference()` and the window share `reference_text` and
    `reference_scope`; their `dead_code` allows are gone because both are
    now live.
- **File ▸ Close Project** runs `ShellAction::NewProject`, so the
  unsaved-work confirm protects it like New (never a silent discard).
- Catalog keys `menu.shortcuts` and `menu.close-project`, in both
  catalogs.

## Found

Save As was already in the File menu (`button.save-as`, Ctrl+Shift+S); the
plan row's "Save As open" was stale. **Show log** stays open: the app logs
through `env_logger` to stderr only, so there is no log file to open. It
needs a file sink first, which is a separate kill.

## Test

`the_shortcuts_window_rows_match_every_binding`: on both platforms, one
row per binding, none undocumented, and every row's key and text appear
verbatim in the generated doc. Gate 1: umber-app 534.
