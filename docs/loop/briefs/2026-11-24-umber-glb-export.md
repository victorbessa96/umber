# Design brief: GLB mesh export round-trip (2026-11-24)

Planned kill lock: `razul-glb-export`.
Base: row-156 scoping and docs reconciliation status tip `9dcacafa080176b77a90c2fc223a6d2544cc106e`.

## What exists

OBJ geometry export landed at `8c9be579` with deterministic output, bounded names, typed failures, and real `load`/`load_obj` round-trips. `umber-mesh::load_gltf` accepts glTF 2.0/GLB and validates POSITION/NORMAL/TEXCOORD/indices layouts. The row-156 scoping record names GLB as the next code follow-up, not covered by OBJ.

The remaining slice is one measured GLB writer: deterministic binary geometry with a real export→import round-trip. It is not a material/texture promise and does not change the OBJ path.

## Target

- Add `glb.rs` with `to_glb_document(asset, mesh)` and `write_glb(out_dir, asset, mesh)`, using the existing `UsdMeshView` borrowed geometry shape.
- Emit a valid GLB 2.0 container: 12-byte header, JSON chunk padded to 4-byte alignment, BIN chunk with little-endian f32 positions/normals/UVs and u32 triangle indices.
- JSON accessors use `componentType` 5126 for POSITION/NORMAL/TEXCOORD and 5125 for indices; bufferViews cover each attribute with correct byte offsets and lengths. Include finite POSITION min/max.
- Require non-empty triangles, index count divisible by three, parallel attribute lengths, and in-range indices. Ragged meshes return `ExportError::Write`, never a partial file or silent zero-fill.
- Reuse the bounded ASCII name rule from OBJ (240 bytes) without allocating the name during the size preflight. Non-finite metadata sanitizes to `0.0`. Output is byte-identical across repeated calls.
- Preflight an upper-bound size estimate before allocating, then re-check the finished document. Refuse over-cap exports with `ExportError::TooLarge`.
- Export `pub mod glb` from `lib.rs`. Extend the hostile-output round-trip battery to enumerate `usda`, `obj`, and `glb`, re-importing GLB through both `load` and `load_gltf`.

## Failure handling

Empty/ragged meshes, out-of-range indices, oversized documents, and file-write failures use the existing typed `ExportError` variants. The writer validates and size-checks before creating the output directory; no partial document is written when validation or the size cap fails. Texture/material names are out of scope for this geometry-only slice.

## Tests

- Exact GLB header, chunk types, accessor component types, byte offsets, and total length.
- Real `write_glb` → `umber_mesh::load` and `load_gltf` round-trip with the same triangle count and finite positions/normals/UVs.
- Repeated document/file writes are byte-identical.
- Ragged meshes, empty triangles, out-of-range indices, short-normal/UV attributes, non-finite metadata, huge names, and over-cap exports are typed failures with no partial file.
- Extend `every_fixture_roundtrips_through_every_export_format_or_is_a_typed_err` to run all three formats; existing USDA/OBJ behavior must stay unchanged.
- RED first: prove `crate::glb` is absent, then implement and run strict `cargo clippy -p umber-export --all-targets -- -D warnings` and full `cargo test -p umber-export` under the repo flock with the slow-build environment.

## Concurrency and safety

Locked surface: `crates/umber-export/src/lib.rs`, `crates/umber-export/src/obj.rs` for shared name helpers, new `crates/umber-export/src/glb.rs`, `crates/umber-export/tests/export_roundtrip.rs`, this brief/report, `docs/ROADMAP.md`, `docs/loop/waves/2026-11-09-wave-plan.md`, and the current status/queue surfaces. No sibling lock or other source surface is touched. Any foreign dirty file, live sibling lock, origin movement, disk I/O error, or unexpected behavior delta stops the lane for re-census.

## Verification and acceptance

- RED first: run exact GLB acceptance tests and capture the missing-module failure.
- Run strict clippy and full `cargo test -p umber-export` separately with passed/failed counts; zero-test success is not a pass.
- Run rustfmt, `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check`, and `git diff --check`.
- Obtain independent review of the GLB container layout, accessor mapping, typed failures, size cap, deterministic output, and real re-import coverage.
- Record the kill and update the current remaining-work order, commit path-limited through dcommit, run `scripts/safe-push.py` bare, and independently verify `git ls-remote origin master`.

## Non-goals

No material/MTL or texture references, schema change, release action, or second-kill scope. EyeDepth, COW undo, generated parity table, theme matrix, and packaging deltas remain separately scoped candidates. OBJ behavior is preserved.
