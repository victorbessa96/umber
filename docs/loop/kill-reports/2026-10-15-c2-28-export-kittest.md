# Kill report (IN FLIGHT, unrun): C2-28, an Export-panel kittest

Date: 2026-10-15. Author: dragon. Lock: `c2-28-export-kittest` over
`crates/umber-app/src/export_dialog.rs` and this report. Written while the
build host is down; uncommitted and unrun.

## Built (unverified)

`the_export_toggles_flip_and_export_is_gated_without_a_mesh`, the
bakes-panel kittest's shape (`Harness::new_ui_state`, builtin catalogs):

- With no mesh and no GPU, the reason line (`export.no-mesh`) is on screen.
- The MaterialX and USDA checkboxes each flip their state.
- The Export button reports disabled through AccessKit, and a click on it
  runs nothing: `last_written` stays empty and the status keeps
  `export.no-export-yet`.

## Static checks done instead of a run

- Every label the test queries exists in `en-US.ftl` (`export.no-mesh`,
  `export.materialx`, `export.usda`, `button.export`,
  `export.no-export-yet`).
- Inside this harness, "Export" is rendered only by the button: the
  panel's group titles are Preset and the tiles label, so `get_by_label`
  is unambiguous.

## When the host returns

Run it. Mutation-check the gate (enable the button unconditionally): the
click must then reach `export_now` and change the status, failing the
last assert.
