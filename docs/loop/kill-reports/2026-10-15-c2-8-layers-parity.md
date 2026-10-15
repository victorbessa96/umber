# Kill report: C2-8 Layers panel parity (reorder, rename, opacity, blend, duplicate, folders)

Date: 2026-10-15. Author: dragon. Lock: `c2-8-layers-parity` over
`crates/umber-app/src/document.rs` and this report. Source:
`docs/loop/audit-2026-10-15-c2.md` C2-8. The core had `LayerCommand::
{Reorder, Rename, SetOpacity, SetBlendMode}` all along; app code only
called them from tests.

## Built (`document.rs`, `layers_ui`)

- **＋ Add folder layer** beside the paint, fill and material adds
  (`LayerKind::Folder { passthrough: false }`, selected on add, like the
  others).
- **▲ / ▼ move** per row through `LayerCommand::reorder`. `move_target`
  is the pure one-step rule, with no move at either end. The panel lists
  top-first, so ▲ raises the stack index.
- **⧉ duplicate** per row. `duplicate_layer` adds a same-kind copy named
  "<name> copy", moves it directly above the source, and replays opacity,
  visibility and blend. Each step is an ordinary undoable command.
- **The selected layer's properties row:**
  - opacity as a `ValueField` (0..1, default 1, the property menu);
  - blend mode as a combo, `layer_blend_options`: the material panel's
    list plus Passthrough for folders only. Material layers hide it,
    since their own blend is inert;
  - rename as a text field committed on focus loss through
    `rename_commit` (trimmed; empty or unchanged commits nothing).

  Every edit is one command, issued only when the value changes.

## Tests (4)

`move_target` (both ends, unknown, single layer); `rename_commit` +
`duplicate_name`; blend options (Passthrough for folders only, exactly
one extra); and **duplicate end to end on a real `Document`**: the copy
lands directly above the source with its opacity and blend, as four
history entries, and four undos restore the original stack exactly.
Gate 1 (umber-app): fmt, clippy, 472 tests.

## Judgment calls

- **No new strings.** Both catalogs are under the sibling's live lock,
  so labels reuse the History panel's operation keys ("Add folder
  layer", "Set opacity", "Set blend mode", "Rename layer"). The move
  buttons are glyphs with the "Reorder layer" tooltip. The duplicate
  button has NO tooltip rather than a wrong one, the " copy" suffix is
  the one untranslated string, and blend modes show their identifiers
  (the material panel's existing rule).
- **Duplicate copies settings, not content.** Paint pixels live in the
  paint session, not the document, and fill / material / mask
  parameters ride their own commands. v1 copies kind, opacity,
  visibility and blend. Content copy is the follow-up.
- **An opacity drag is many commands.** Each one captures its exact
  previous value, so every step undoes. Coalescing a drag into one entry
  belongs to the one-timeline work (C2-7).
- **Buttons, not drag-to-reorder.** egui has no built-in list
  drag-and-drop, and a hand-rolled one needs a kittest the panel does not
  have yet. ▲ / ▼ covers the need. Drag reorder stays the GUI plan's
  g2-layer-gestures row.

## Deferred

- Duplicate with content (planes, fill / material / mask params).
- Drag reorder and multi-select (g2-layer-gestures).
- Proper "Duplicate" / "Move up/down" / "Opacity" strings (after the
  catalog lock).
