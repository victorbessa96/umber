# Design brief: OccluderScope combo → request → resolution kittest (2026-11-24)

Planned kill lock: `razul-occluder-kittest`.
Base: Assets mesh category status tip `927c0470e0eede538e7bcb5e02c098d62f3bf67a`.

## What exists

`bakes_panel.rs` already carries the `OccluderScope` row with `bakes.occluder-whole` and `bakes.occluder-own`. `bake_now` captures `occluder_own_set` into `BakeRequest` at `crates/umber-app/src/bakes_panel.rs:3554`. `run_bake` resolves it inline at AO WholeMesh and AO Tile, thickness, and bent-normal call sites. `mapped_ray_maps_raster_the_submesh_but_occlude_cross_material` proves the underlying AO behavior: the whole-mesh occluder darkens under the floating slab, while a ground-only occluder stays fully lit. The inline `BakesPanel::show` harness drives map checkboxes through `egui_kittest`.

The remaining work is the missing panel-thread proof: the combo's state must reach the actual `bake_current` request and produce the corresponding resolution/output, not merely exist in the UI.

## Target

- Add one full `egui_kittest` using the real `BakesPanel::show` with a `GpuContext` and the two-material ground-plus-floating-slab mesh.
- Drive the `OccluderScope` combo from Whole mesh to Baked set only, then call the existing `bake_current` with the same device/queue/mesh. Assert the request snapshot is OwnSet (`true`, plus request size/rays) and that the Ground AO result is fully lit because the slab is excluded.
- Change the combo back to Whole mesh, run a second real bake, assert the request snapshot is WholeMesh (`false`) and that the Ground AO result darkens under the slab.
- Assert both jobs complete through the real worker/poll path with `completed_jobs == 2` and no failure, and read the actual Ground AO PNGs for the lit/dark assertions.
- Keep the Slab set disabled so only Ground's result matters. The request snapshot is test-only state (`#[cfg(test)]`) in `BakesPanel`, captured immediately before `start_job`; it is not a new production setting or alternate bake path.
- Do not change `BakeRequest`, `run_bake`, the four call sites, i18n, settings carry, or any bake semantics. This is a test/kittest kill with a small test-only observability hook.

## Failure handling

A worker failure, missing GPU adapter, PNG decode failure, missing Ground record, or unexpected AO result is a hard test failure, not a graceful skip. No vacuous green on a skipped adapter. Test fixtures live under the scratch temp directory and are removed after assertions.

## Tests

- `occluder_scope_combo_drives_request_and_resolution`: default Whole mesh, click the combo and choose Baked set only, assert request OwnSet and lit Ground AO; choose Whole mesh, assert request WholeMesh and darkened Ground AO. Real worker/poll and real PNGs.
- The existing `mapped_ray_maps_raster_the_submesh_but_occlude_cross_material` remains the direct AO control for the underlying occluder behavior.
- Strict clippy on `umber-app` and full `cargo test -p umber-app` with the repo asset-root override. No new dependency, icon, schema, or release action.

## Concurrency and safety

Locked surface: `crates/umber-app/src/bakes_panel.rs`, this brief/report, `docs/ROADMAP.md`, `docs/loop/waves/2026-11-09-wave-plan.md`, and the current status/queue surfaces. No sibling lock or other source surface is touched. Any foreign dirty file, live sibling lock, origin movement, disk I/O error, or unexpected behavior delta stops the lane for re-census.

## Verification and acceptance

- RED first: prove the full combo → request → resolution test is absent, then implement the test-only request snapshot and real GPU kittest.
- Run the exact kittest with a nonzero passed count, then strict `cargo clippy -p umber-app --all-targets -- -D warnings` and full `cargo test -p umber-app` separately under `/tmp/umber-cargo.lock` with the slow-build environment and `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`. Record every suite's passed/failed count.
- Run rustfmt, `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check`, and `git diff --check`.
- Obtain independent review of the real UI path, request observability, worker completion, and lit/dark output assertions.
- Record the kill and update the current remaining-work order, commit path-limited through dcommit, run `scripts/safe-push.py` bare, and independently verify `git ls-remote origin master`.

## Non-goals

No new occluder features, set-list picker, alternate worker path, broad bake refactor, or second-kill scope. XI2 hotplug re-query remains the next independent kill.
