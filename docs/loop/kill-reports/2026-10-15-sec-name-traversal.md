# Kill report (UNRUN): mesh-supplied names can no longer climb out of the output dir

Date: 2026-10-15. Author: dragon. Lock: `sec-name-traversal` over
`crates/umber-mesh/src/mesh_maps.rs`, `crates/umber-export/src/lib.rs`,
the C2 audit, SECURITY.md and this report. Found by the static security
sweep run while the build host was down. Uncommitted until the battery
runs.

## The bug

Texture-set names come from the MESH FILE: an FBX/glTF/OBJ material name
(`texture_set_name`, `BakesPanel::set_names_for`). Two writers put them
straight into a path:

- **Bakes**: `format_mesh_map(dir, set, …)` =
  `dir.join(format!("{set}_{suffix}.png"))`. A material named
  `../../x` wrote `dir/../../x_ambient_occlusion.png`; one named
  `/etc/x` replaced the whole path (`Path::join` with an absolute
  component). The GUI Bakes panel, `umber-cli bake-all` /
  `bake-transfer` and the batch recipes all go through it.
- **Export**: `expand_template` substituted `$textureSet` and
  `$layerName` (also file-derived) raw. The template's `/` is
  deliberately a subfolder (Painter's convention), but a VALUE's `/` was
  too.

`project.rs` was already safe: `validate_set_name` rejects traversal and
reserved names before any `join`, on save and on load. The other two
writers never had an equivalent.

**Impact:** opening an untrusted mesh and baking or exporting it could
create or overwrite a `.png` (or `.exr`/`.tif`) outside the chosen
folder, anywhere the user can write. The file name ends in a fixed
suffix, so it cannot hit an arbitrary target name, but it can drop files
into other folders.

## Fix

- `umber_mesh::file_safe(name)` in `format_mesh_map`: separators, the
  drive/stream colon, Windows-reserved characters and control characters
  become `_`; dot-only names become underscores. Ordinary names
  (`Body`, `Sword_Hilt`, `Metal 01`) are byte-identical, so existing
  bake names do not move.
- `umber_export::path_safe_value(value)`, the same rule, applied to every
  token VALUE in `expand_template`. Template-literal `/` still makes
  subfolders.
- The two functions are duplicates on purpose: umber-mesh is below
  umber-export in `scripts/check-layers.py`'s order, and neither should
  depend on the other for eight lines.

## Tests (unrun)

- `a_crafted_material_name_cannot_place_a_bake_outside_the_dir`: five
  hostile names each land directly in `dir` with no `ParentDir`
  component; an ordinary name is unchanged.
- `token_values_cannot_climb_out_of_the_output_dir`: nine value → result
  pairs, plus a template whose own `/` survives while the value's
  becomes `_`.
- Both rules were modelled in Python against every expected pair before
  writing (all match).

## Mutation checks owed when the host returns

Remove `file_safe` from `format_mesh_map` (the bake test must fail), and
remove `path_safe_value` from `expand_template` (the export test must
fail).

## Third site: the paint sidecar manifest's channel segment (lock `sec-sidecar-channel`, UNRUN)

`parse_paint_sidecar_name` is the manifest's trust boundary (the
manifest is file content). It rejected an empty channel, `..` and `\\`,
but NOT:

- `C:`. On Windows, `PathBuf::join("C:")` drops the base and yields a
  drive-relative path, so a crafted `paint/manifest.txt` line
  `C:/tile-1001.png …` aims the roll-forward's rename at the current
  directory of drive C.
- `.` and `...`: Windows strips trailing dots, so `...` names the
  sidecar dir itself.
- `a:b`: an NTFS alternate data stream.
- `* ? < > | "`: invalid on Windows, so the roll-forward's rename fails
  loudly instead of being rejected cleanly.

`is_safe_channel_component` now rejects all of them, and the parse
rule's doc points at it. Both existing reject lists (flat and
layer-nested) gain those cases. Real channel names (`baseColor`,
`height`, user channels) are untouched: they are identifiers.

## Fourth site: the import refuses an unsafe channel at the source (lock `sec-import-channel`, UNRUN)

The mesh-map import's user-channel route (`<set>_<suffix>.png` →
channel `suffix`) could carry a `:` or `*` from a Linux file name into
the document; the save then writes `paint/<suffix>/…`, which fails on a
Windows checkout, and the load now rejects it. `classify_file_name` now
refuses such a suffix with `UnmatchedReason::UnsafeChannel`, by the same
rule as the sidecar parser. The import report row says why; both
catalogs have the new key. Test: four hostile names classify as
`UnsafeChannel`; the reason-key list includes it.

## Found on the way: 21 pt-BR strings were missing

A static parse of both `.ftl` catalogs found pt-BR missing 21 of
en-US's 650 keys, all of `import.*` (the whole mesh-map import panel)
plus `texturesets.{inherit, per-channel, channel-resize-clears}`. A
pt-BR user saw that panel in English. The catalog's own header said so
("as demais caem no en-US"). All 21 are translated now; the parse says
651 = 651, with nothing missing and nothing extra.

