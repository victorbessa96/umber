# Design brief: Assets browser mesh category (2026-11-24)

Planned kill lock: `razul-assets-mesh`.
Base: all-channel material picker status tip `2783a188754940338b0149851a0a4e16f8b27c05`.
Scout: `deleg_11326c15-2`.

## What exists

The Assets browser has Brushes, Smart materials, Smart masks, Environment maps, and Filters. `LibraryOutcome` carries brush/material/mask/env/filter but no mesh. Thumb kinds 0–4 exist. Environment and Filter categories establish the default-closed, epoch-cached row pattern. The existing `UmberApp::load_mesh_path` performs import, GPU upload, texture-set retargeting, geometry-source replacement, and `mesh_parts`/`mesh`/`mesh_path` commit only after success.

`umber_mesh::detect_format` accepts OBJ, GLTF/GLB, FBX, USD/USDA, USDC, and USDZ case-insensitively. Full import is capped at 1 GiB, so scanning must never parse candidate meshes eagerly.

## Target

- Add private `mesh_dirs`, `mesh_dirs_with`, and `list_meshes` beside the other library helpers. Search bundled `assets/meshes`, executable-side `meshes/`, then persisted extra folders with canonicalized first-wins directory dedup.
- Scan direct files only. Gate with `umber_mesh::detect_format`, dedup by complete filename, display the file stem, and sort by display name. Missing/unreadable directories and unsupported extensions are omitted. Recognized-but-corrupt files remain rows and fail honestly only on activation.
- Add `LibraryOutcome.mesh: Option<PathBuf>` and `THUMB_KIND_MESH: u8 = 5`.
- Add a deterministic `asset_thumbs::mesh_glyph(size)` generic wireframe swatch. No file parse, eager geometry render, SVG, atlas entry, or new icon dependency.
- Add a sixth `Meshes` `CollapsingHeader`, default closed, cached by `("assets-meshes", state.scan_epoch)`. Rows follow Env/Filter behavior: search filtering, full-path hover, AccessKit label, click sets `out.mesh`. No starter-content directory or missing-starter warning is added. Keep `assets.empty`/`assets.no-matches`.
- In the shell's Assets arm, apply `outcome.mesh` through the exact existing `load_mesh_path` transaction. No layer selection, undoable layer op, second importer, or new GPU path.
- Add only `assets.meshes` to both catalogs (`Meshes` / `Malhas`). Existing mesh-load/upload failure notices are reused.
- Extend the kittest outcome capture with `o.mesh.is_some()`.

## Failure handling

A deleted file after scanning, malformed input, unsupported content despite a recognized extension, or over-cap input reaches the existing mesh-load-failed notice. GPU upload failure reaches mesh-upload-failed. Prior session state survives import/upload failure because `load_mesh_path` commits only after success. No silent fallback mesh or partial document edit.

## Tests

- Lister unit test: all eight extensions with mixed case, reject `.txt`/extensionless paths, sort stems, first-directory-wins dedup, ignore missing directories, and explicitly prove malformed recognized files are listed without being parsed during scanning.
- Directory unit test: bundled/exe/extra ordering and canonicalized duplicate suppression.
- One kittest modeled on Filters: Meshes header exists and starts closed; opening reveals a unique executable-side `meshes/` fixture; search filters the row and produces no-match state; clicking returns the exact `outcome.mesh` path; a `THUMB_KIND_MESH` cache entry exists. Use a tall harness and clean up the fixture. A deliberately malformed OBJ proves no eager parse during the click/list path.
- Extend the bilingual key pin; the i18n suite independently enforces exact locale-key parity.
- Run focused Assets/i18n tests plus panel snapshots as regressions; no snapshot baseline regeneration.

## Concurrency and safety

Locked surface: `crates/umber-app/src/assets_panel.rs`, `asset_thumbs.rs`, `main.rs`, both i18n catalogs, this brief/report, `docs/ROADMAP.md`, `docs/loop/waves/2026-11-09-wave-plan.md`, and the current status/queue surfaces. No new starter content, license change, dependency, schema, or release action. Any foreign dirty file, live sibling lock, origin movement, disk I/O error, or unexpected behavior delta stops the lane for re-census.

## Verification and acceptance

- RED first: prove the mesh category and lister are absent before implementation, then run focused mesh/Assets/i18n tests with nonzero passed counts.
- Run strict `cargo clippy -p umber-app --all-targets -- -D warnings` and full `cargo test -p umber-app` separately under `/tmp/umber-cargo.lock` with the slow-build environment and `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`. Record every suite's passed/failed count; 0-test success is not a pass.
- Run rustfmt, `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check`, and `git diff --check`.
- Obtain independent review of lazy scanning, format coverage, dedup/order, thumbnail cache identity, default-closed UI behavior, failure atomicity, and the exact existing apply transaction.
- Record the kill and update the current remaining-work order, commit path-limited through dcommit, run `scripts/safe-push.py` bare, and independently verify `git ls-remote origin master`.

## Non-goals

No content-accurate mesh thumbnail renders, asynchronous import/render pipeline, starter meshes, drag/drop mesh outcome, alternate import path, or second-kill scope. OccluderScope and XI2 remain later independent kills.
