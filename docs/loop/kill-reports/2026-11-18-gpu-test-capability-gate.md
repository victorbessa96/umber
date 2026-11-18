# Kill report: GPU test capability gate (2026-11-18)

Kill lock: `razul-gpu-capability-gate`. Brief: `docs/loop/briefs/2026-11-18-gpu-test-capability-gate.md` (commit `70c52779`).

## Symptom and root cause

`python3 scripts/safe-push.py` exited 2 on every attempt while pushing the D4 kill
(`90aff880`). Its full-crate matrix (`cargo test -p umber-app -p umber-bake -p umber-brush
-p umber-cli -p umber-core -p umber-gpu -p umber-gui -p umber-paint`) unifies
`umber-bake/gpu` ON because `crates/umber-cli/Cargo.toml` has `default = ["gpu"]` with
`gpu = ["umber-bake/gpu"]`. The `#[cfg(feature = "gpu")]` tests then compile and run on
this host's only adapters (Intel SKL via ANV; wgpu GL fallback when Vulkan finds none):

- 9 `id::tests::gpu::id_multi_*` fail at shader validation: `enable wgpu_binding_array`
  unsupported / array-index expression invalid on SKL.
- 4 `ground_plane::gpu_tests::*` and 2 `opacity::tests::gpu::*` fail with texel
  mismatches (e.g. AO 255 vs CPU 80) on the same adapter.

Bisect proof the failures pre-exist and are environmental: the identical 15 tests fail
at base commit `072442bb` with `WGPU_BACKEND=vulkan cargo test -p umber-bake --lib
--features gpu` (log `~/.hermes/cache/scratch/d4-battery/pre-gpu-vulkan.log`), in a
worktree with the D4 diff absent. The D4 diff is clean.

## Fix

Two-layer capability gate inserted into all 13 GPU-test helper sites across the 11
`umber-bake` modules (ao, curvature, dilation, gbuffer x2, ground_plane, id,
normal_map, opacity, position x2, thickness, transfer); no test bodies changed:

1. `UMBER_RUN_GPU_TESTS` env opt-in (skip with named reason when unset).
2. Adapter must report `wgpu::Features::TEXTURE_BINDING_ARRAY`; the device is now
   requested with that `required_features` so shader validation matches the adapter.

`.github/workflows/ci.yml`: the two dedicated Linux lavapipe GPU steps
("GPU feature tests", "umber-bake GPU feature tests") set `UMBER_RUN_GPU_TESTS=1`, so
CI keeps full GPU coverage; the workspace `cargo test --workspace` step now skips the
GPU family (it only ran them via feature unification).

umber-paint / umber-gpu / umber-cli helpers were left untouched: they carry their own
skip checks and produced zero failures in the failing matrix run.

## Battery (this host, flock /tmp/umber-cargo.lock, CARGO_BUILD_JOBS=3, nice 19)

- rustfmt --check on all touched .rs: clean. check-layers GREEN (16 crates). scorecard current.
- `cargo clippy -p umber-bake --all-targets -- -D warnings`: 0 errors.
- `cargo clippy -p umber-bake --features gpu --all-targets -- -D warnings`: 0 errors
  (feature-on test code still lints).
- `cargo test -p umber-bake --lib`: 121 passed / 0 failed.
- `cargo test -p umber-bake --lib --features gpu`: 201 passed / 0 failed (GPU tests
  skip with the named env-gate reason).
- safe-push full 8-crate matrix: green (see queue row 537); push + ls-remote verified.

## Notes for the reviewer

- Local GPU verification of the binding-array id_multi shader still has no capable
  adapter on minihost (SKL/GL only); CI lavapipe is the executor, as before.
- centralhost (RTX 3070) is powered off; when it returns, `UMBER_RUN_GPU_TESTS=1`
  runs the full GPU family on real hardware.
