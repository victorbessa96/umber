# Kill report: C2-28 kittest breadth, round 1 (Bakes checklist, Brush channel combo)

Date: 2026-10-15. Author: dragon. Lock: `c2-28-kittest-breadth` over
`crates/umber-app/src/{bakes_panel,brush_panel}.rs` and this report.
The kittest harness had one behavior test (Display Reset). This round adds
two, on today's new surfaces.

## Tests

- `bakes_panel::clicking_a_map_checkbox_changes_what_bakes`: the real
  `BakesPanel::show` with no mesh or GPU. Clicking "ID" (default off)
  adds it to `selected_maps()`, and clicking "Normal" (default on)
  removes it. This pins C2-29's checklist wiring: the loop over `ALL`
  must write back through `set_map_enabled`.
- `brush_panel::the_channel_combo_retargets_the_paint_session` (GPU,
  ran): a real `PaintState` on a paint-capable device. It opens the
  Channel combo, picks Roughness, and the session's `active_channel()`
  is "roughness". **Mutation check:** dropping the
  `set_active_channel` call fails it (`"baseColor" != "roughness"`).

Both live in-crate (`src/*` test modules): neither panel is on the
library surface the `tests/` harness links.

Gate 1: umber-app 490. fmt and clippy clean.

## Lessons (into the `umber-wave-ops` skill)

- egui 0.36 exposes a `ComboBox`'s selected text as its accessibility
  VALUE, not its label. `get_by_label("Base color")` finds nothing; use
  `get_by_role(Role::ComboBox)`, then pick the popup item by label.
- A `PaintState` test device needs
  `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`. A default device fails with
  `MissingDeviceFeature`.

## Round 2: the Layers panel (lock `c2-28-layers-kittest`, `document.rs`)

`document::layers_panel_buttons_add_and_reorder_through_history`: the
real `layers_ui` over a 2-layer `Document`, with every context struct
Default-built. Clicking "＋ Add folder layer" adds a folder, which is one
history entry. Clicking the BOTTOM row's ▲ (the last of three) raises
"Bottom" one place, a second history entry.

**Mutation check:** making ▲ move down fails it at "Bottom rose one
place". The test pins the direction, which a top-first panel listing
makes easy to get backwards. Gate 1: umber-app 491.

## Open (C2-28's remainder)

The Export dialog (it needs a GPU + mesh fixture) and the Graph panel's
plugin Trust & load row.
