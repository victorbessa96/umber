# Design brief: OBJ mesh export round-trip (2026-11-24)

Planned kill lock: `razul-obj-export`.
Base: control channel + bridge live smoke status tip `230b5e2cbf00bc6b8d5f0c5fec5d5e6a3a99970e`.

## What exists

`crates/umber-export/tests/export_roundtrip.rs` proves the public mesh-export surface is currently USDA-only. It explicitly names the glb/obj ledger row as unimplemented. `umber_mesh::load_obj` uses `tobj` with `single_index=true` and `triangulate=true`, returning parallel positions/normals/UVs and triangle indices. `UsdMeshView` already carries the borrowed geometry shape the export crate needs.

The remaining slice is one measured exporter: deterministic Wavefront OBJ geometry with a real export→import round-trip. It is not a GLB implementation and not a texture/material promise.

## Target

- Add `obj.rs` with `to_obj_document(asset, mesh)` and `write_obj(out_dir, asset, mesh)`, using the existing `UsdMeshView` borrowed geometry shape.
- Emit deterministic `v`, `vt`, `vn`, and `f v/vt/vn` records. Parallel attributes use the same one-based index at each corner, exactly what `tobj` reads with `single_index=true`.
- Require non-empty triangles, index count divisible by three, parallel attribute lengths, and in-range indices. Ragged meshes return `ExportError::Write`, never a partial file or silent zero-fill.
- Sanitize object/file names to a bounded ASCII identifier; non-finite floats sanitize to `0.0` so no NaN/Inf tokens enter the document. Output is LF-only and byte-identical across repeated calls.
- Preflight an upper-bound size estimate before allocating, then re-check the finished document. Refuse over-cap exports with `ExportError::TooLarge`.
- Export `pub mod obj` from `lib.rs`. Extend the existing hostile-output round-trip battery to enumerate `usda` and `obj`, re-importing OBJ through both `load` and `load_obj`.

## Failure handling

Empty/ragged meshes, out-of-range indices, oversized documents, and file-write failures use the existing typed `ExportError` variants. The writer creates the output directory only after the document validates; no partial document is written when validation or the size cap fails. Texture/material names are out of scope for this geometry-only slice.

## Tests

- Exact deterministic OBJ document shape and LF-only output.
- Real `write_obj` → `umber_mesh::load` and `load_obj` round-trip with the same triangle count and finite positions/normals/UVs.
- Repeated document/file writes are byte-identical.
- Ragged meshes, empty triangles, out-of-range indices, non-finite metadata, and over-cap exports are typed failures with no partial file.
- Extend `every_fixture_roundtrips_through_every_export_format_or_is_a_typed_err` to run both formats; existing USDA behavior must stay unchanged.
- RED first: prove `crate::obj` is absent, then implement and run strict `cargo clippy -p umber-export --all-targets -- -D warnings` and full `cargo test -p umber-export` under the repo flock with the slow-build environment.

## Concurrency and safety

Locked surface: `crates/umber-export/src/lib.rs`, new `crates/umber-export/src/obj.rs`, `crates/umber-export/tests/export_roundtrip.rs`, this brief/report, `docs/ROADMAP.md`, `docs/loop/waves/2026-11-09-wave-plan.md`, and the current status/queue surfaces. No sibling lock or other source surface is touched. Any foreign dirty file, live sibling lock, origin movement, disk I/O error, or unexpected behavior delta stops the lane for re-census.

## Verification and acceptance

- RED first: run exact OBJ acceptance tests and capture the missing-module failure.
- Run strict clippy and full `cargo test -p umber-export` separately with passed/failed counts; zero-test success is not a pass.
- Run rustfmt, `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check`, and `git diff --check`.
- Obtain independent review of the OBJ index mapping, typed failures, size cap, deterministic output, and real re-import coverage.
- Record the kill and update the current remaining-work order, commit path-limited through dcommit, run `scripts/safe-push.py` bare, and independently verify `git ls-remote origin master`.

## Non-goals

No GLB writer, material/MTL or texture references, schema change, release action, or second-kill scope. EyeDepth, COW undo, generated parity table, theme matrix, and packaging deltas remain separately scoped candidates. GLB export remains a named follow-up after OBJ lands.
