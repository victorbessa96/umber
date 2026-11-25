# Design brief: OBJ material/MTL export (2026-11-24)

Planned kill lock: `razul-obj-mtl`.
Base: landed-state documentation tip `be9e5328a3f60f99a6a24b7127cf6553199789d3`, clean and origin-equal.

## What exists

`crates/umber-export/src/obj.rs` writes deterministic geometry only. `usd_textures` already walks the real filtered preset and maps each written output to OpenPBR inputs with passthrough-vs-packed channel analysis. `usda_file_for_output` mirrors the driver's names. The CLI and app call that walk after `run_preset_tiled_dilated`, so the same actual-file mapping can drive OBJ/MTL.

`tobj::load_obj` resolves `mtllib` relative to the OBJ and parses `newmtl`, `map_Kd`, `map_Bump`, `map_d`, `map_Ka`, and `map_Ns`; unknown PBR keys remain available in `unknown_param`. `umber_mesh::load_obj` maps each model's `material_id` into `material_names`, so a valid `mtllib` + `usemtl` pair is observable after re-import.

## Target

- Add additive `ExportPreset::obj_mtl: bool` with `#[serde(default)]`, off in every built-in preset and every older JSON recipe. Preserve it through filtering and all app/CLI construction paths.
- Add `to_mtl_document`, `to_obj_document_with_material`, and `write_obj_with_mtl` to `obj.rs`, accepting the existing `UsdTexture` walk rather than duplicating preset logic. Return the OBJ and MTL paths as a pair. Keep the geometry-only API byte-compatible.
- MTL mapping: `base_color` → `map_Kd`, `geometry_normal` → `map_Bump`, `specular_roughness` → `map_Pr`, `base_metalness` → `map_Pm`, `emission_color` → `map_Ke`, `geometry_opacity` → `map_d`. Emit one bounded `newmtl` with a default `Kd 1 1 1` and `illum 2`.
- Accept only passthrough references (`rgb` for color/vector, `r` for scalar). Any packed-channel output (`g`/`b`/`a`) is a typed refusal naming the need for separate maps. Never pretend an ORM/metallicRoughness channel is a standalone MTL texture.
- Reject absolute paths, URI schemes, parent traversal, backslashes, newlines/NUL, and `<UDIM>` placeholders. The writer verifies every referenced texture exists under the output directory before writing either document.
- For tiled runs, write one whole-mesh OBJ/MTL pair per tile with concrete `_<tile>` names and concrete per-tile texture references. No `<UDIM>` token is emitted. Single-tile 1001 keeps `<asset>.obj`/`.mtl`.
- MTL is written first with previous bytes restored on error; an OBJ write failure restores the previous MTL state. Existing directories at either target are rejected before any document write. No new partial OBJ/MTL pair is left after a typed failure.
- Add `ExportDocFlags { usda, obj_mtl }` so `run_export` stays within the argument cap. CLI `--obj`, batch/agent `obj_mtl` fields (default off), and the app's Export checkbox all reach this path. Add `export.obj` to both i18n catalogs.

## Failure handling

Missing or unsafe texture references, packed channels, invalid meshes, oversized documents, existing directory targets, and I/O failures are typed `ExportError`s. No silent fallback to geometry-only output when `obj_mtl` was requested. No automatic roughness/shininess conversion and no invented per-channel MTL syntax.

## Verification and acceptance

- Real driver test: Blender separate-map preset through `run_preset`, then `usd_textures` + `write_obj_with_mtl`; every reference names an actual file. Re-import through `umber_mesh::load` and `load_obj`, asserting triangle count, finite attributes, and the material name from the MTL.
- Determinism: two complete OBJ/MTL writes are byte-identical.
- Packed-channel refusal leaves no OBJ/MTL pair. Unsafe and missing references fail before output. Existing directory targets leave no new pair.
- Tiled case writes concrete `_1002` OBJ/MTL and texture names, with re-import proving the tiled material name.
- Serde compatibility: older preset JSON and batch recipes keep `obj_mtl=false`; app and CLI toggles reach the filtered preset.
- App 8x8 real-GPU export writes and re-imports OBJ/MTL. Existing export/history behavior remains unchanged when the toggle is off.
- `cargo fmt --check`, `check-layers.py`, `scorecard.py --check`, strict clippy for every touched crate, and counted tests for `umber-export`, `umber-cli`, and `umber-app` run under the repo flock. `scripts/parity.py` regenerates `docs/parity.md` after the requirements row changes.
- Independent read-only review, kill report, safe-push, and `git ls-remote origin master` complete the landing.

## Non-goals

No packed-channel syntax invention, no per-triangle material splitting, no GLB material work, no change to the geometry-only OBJ/GLB APIs, no app history semantic change, and no release or packaging action. EyeDepth, the theme matrix, and the COW A/B measurement remain separate queued kills.
