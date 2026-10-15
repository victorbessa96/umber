# Kill report: C2-4 Ctrl+S re-saves silently; Save As is Ctrl+Shift+S

Date: 2026-10-15. Author: dragon. Lock: `c2-4-silent-save` over
`crates/umber-app/src/{main,toolbar,actions,rich_tooltip,palette}.rs`,
`docs/user/{keyboard,getting-started}.md` and this report. Source: audit
C2-4 (`main.rs:1863` always opened the dialog).

## Built

- `ShellAction::SaveProjectAs` with its own binding (`Ctrl+Shift+S`), a
  palette entry, a tooltip and a File-menu item. Labels reuse the
  existing `button.save-as` key, so no catalog change was needed.
- `save_project` (Ctrl+S) decides through the pure `save_target`: the
  open project's directory means **save in place, silently**; a
  never-saved document means Save As. The in-place overwrite already
  goes through the crash-safe atomic manifest commit, so overwriting
  needs no new safety.
- `save_project_as` always asks. Both feed the unchanged save body
  (`save_project_to`).
- The keyboard reference regenerated itself: Ctrl+S now reads "Save
  project (asks for a folder the first time)", and the Ctrl+Shift+S row
  was added. The drift test would have failed without the regen.

## Tests

- `save_is_silent_once_the_project_has_a_directory` (the pure target).
- Three key-table pins re-pinned **deliberately** to the new contract,
  none deleted:
  - `text_focus_keeps_letters…`: Save As passes a text field like Save;
  - `shift_rows_resolve…`: Ctrl+Shift+S is now Save As, no longer
    unbound;
  - `every_polled_key_is_bound`: 10 chords, not 9.
- Gate 1: umber-app 511. fmt and clippy clean.

## Judgment call

**Save in place is silent: no toast.** Painter and Blender say nothing
on a successful Ctrl+S, and the title bar's dirty mark is the
feedback. A failed save still posts its sticky error (`notice.save-failed`).
