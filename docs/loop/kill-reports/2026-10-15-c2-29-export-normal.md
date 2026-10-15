# Kill report: C2-29 follow-up, Export writes the baked normal

Date: 2026-10-15. Author: dragon. Lock: `c2-29-export-normal` over
`crates/umber-app/src/{bakes_panel,bake_sources,export_dialog,main}.rs`,
the user guide and this report.

## Built

- `BakesPanel::baked_normal()`: the newest tile-1001 tangent normal the
  panel wrote, from a low bake or a Bake from high.
- `bake_sources::apply_baked_normal(set, size, Some((w, h, rgba)))`
  replaces the flat placeholder only at the export size. A wrong size or
  a missing file keeps the placeholder, with a log line; the normal is
  never resampled.
- `ExportContext.baked_normal`: the dialog reads the PNG and applies it
  to tile 1001's map set before the painted channels. A painted normal
  still wins.
- The user guide says which normal Export writes.

## Tests

- `a_baked_normal_replaces_the_flat_one_only_at_the_export_size`: a
  wrong size is declined, None keeps the flat normal, a matching size
  replaces it, and paint still overrides.
- The Bake-from-high GPU test asserts `baked_normal()` is the transfer's
  `Quad_normal_base.png`.
- Gate 1: umber-app 527. fmt and clippy clean (2 `useless_vec` fixed).

## Unverified

The dialog's file read and apply runs in `run` with a live mesh and GPU,
and no test drives the whole export with a baked normal on disk. The
pure half and the panel half are pinned.
