# Kill report: g6-display-mode-keys (M / C / B)

Date: 2026-10-15. Author: dragon. Lock: `g6-display-mode-keys` over
`crates/umber-app/src/{display_keys,actions,toolbar,rich_tooltip,palette,main}.rs`,
both ftl catalogs and this report. Plan:
`docs/specs/gui-plan-2026-10-15.md` row `g6-display-mode-keys` (P:C4, P:B10).

## Built

- `display_keys.rs`:
  - `cycle_channel` and `cycle_mesh_map` step through
    `SoloChannel::ALL` and `ViewMode::ALL_PREVIEW_KINDS`, wrapping both
    ways. Entering from another mode starts at the first item going
    forward and the last going back.
  - `mode_name` builds the toast text.
- Five registry actions:
  - **M** `ViewMaterial` returns to Lit.
  - **C** / **Shift+C** step the soloed channel.
  - **B** / **Shift+B** step the previewed mesh map.

  They show up in the palette and the tooltips too, with keys in both
  locales. Every switch posts a toast naming the new mode ("Channel:
  Roughness", "Mesh map: curvature"), which is the plan's "soloed name
  shown".
- **`Chord::Shift`**, a new chord: Shift with no Ctrl/Cmd/Alt. Its rows
  sort before every bare row, because `Chord::Bare` ignores Shift by
  contract and a bare C row first would swallow Shift+C. The existing
  most-modifiers-first table test pins the order.

## A bug the kill nearly shipped

`Focus::allows` let every non-bare chord through a text field, on the
assumption that "non-bare" meant Ctrl/Cmd. With `Chord::Shift` added,
typing a capital C in a layer name would have stepped the soloed
channel. The pass-through is now Ctrl/Cmd chords only. Pinned by
`shift_rows_resolve_and_stay_out_of_text_fields`, along with the
fall-through cases: Shift+Alt+C goes to the bare row by the module's
contract, and Ctrl+Shift+C resolves to nothing.

## Tests

- 3 in `display_keys` (cycle orders, wrap, cross-cycle entry, names,
  catalog keys).
- 1 in `actions` (the Shift rows).
- The binding census is updated (13 bare keys + 9 chords).
- Gate 1 (umber-app): fmt, clippy, 462 tests. The first battery caught
  the Shift+Alt expectation, which was wrong in the TEST, not in the
  code.

## Judgment calls

- **A toast, not a viewport overlay.** `viewport.rs` is under the
  sibling's live g1-axis-gizmo lock. The toast shows the name the plan
  asks for, and an in-viewport label can replace it when the lock frees.
- **Mesh-map names are file suffixes** (`curvature`, `bent_normal`).
  They are what the View menu's map combo already shows, so the two
  never disagree. Localized map names are a follow-up for both places.

## Process note

The sibling's in-flight `crates/umber-gpu/src/camera.rs` edit showed up
in `git diff` mid-kill. The final battery copies an explicit file list
and asserts that camera.rs equals HEAD, so the verdict does not include
the sibling's work.
