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
