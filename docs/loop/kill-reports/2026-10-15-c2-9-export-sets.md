# Kill report: C2-9, Export names each texture set's files by its set

Date: 2026-10-15. Author: dragon. Lock: `c2-9-export-sets` over
`crates/umber-app/src/export_dialog.rs` and this report. Found while
hunting the per-set design's remaining `tiles_present()` users.

## Bug

Export ran ONE tiled preset over every present tile with ONE
`$textureSet`, the mesh-derived name. With two sets (Body on 1001, Hilt
on 1002), Hilt's maps were written as `<mesh>_BaseColor_1002.png` beside
Body's. The set names that make a multi-material export importable
(`Body_BaseColor.png`, `Hilt_BaseColor.png`) never appeared.

## Fix

- `tile_set_groups(tiles, set_of)` groups the exported tiles by the paint
  session's `set_of_tile`.
- With fewer than two distinct sets it returns ONE unnamed group, so a
  single-set export keeps the mesh-derived name: byte-identical output.
- With several, export runs one tiled preset per set with that set's
  `$textureSet`.

## Test

`tiles_group_by_texture_set_only_when_there_are_several`: one set and no
session each give one unnamed group; two sets give `(Body, [1001, 1003])`
and `(Hilt, [1002])` in first-seen order. Gate 1: umber-app 533.

## Not changed

- The `.usda` still names the whole mesh by the mesh-derived set name.
  One material per set in the USD is a separate change.
- A set whose only tile is 1002 writes tile-suffixed names
  (`Hilt_BaseColor_1002.png`), the driver's rule for any non-1001 tile.
