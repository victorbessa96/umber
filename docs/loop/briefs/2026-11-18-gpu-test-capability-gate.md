# Design brief: GPU test capability gate (2026-11-18)

Kill: `razul-gpu-capability-gate`. Symptom: `scripts/safe-push.py` exits 2 because its
full-crate test matrix unifies `umber-bake/gpu` on (umber-cli `default = ["gpu"]` pulls
`umber-bake/gpu`), so the `#[cfg(feature = "gpu")]` tests compile and run on this host's
only adapter (Intel SKL via ANV, or wgpu's GL fallback when Vulkan finds nothing). 15
tests then fail: 9 `id` (shader uses `enable wgpu_binding_array` -> `Features::TEXTURE_BINDING_ARRAY`,
unsupported on SKL -> shader-validation panic), 4 `ground_plane` and 2 `opacity`
(texel mismatches on the same adapter). Bisect proof: identical 15 failures at
`072442bb` with `WGPU_BACKEND=vulkan cargo test -p umber-bake --lib --features gpu`
(`pre-gpu-vulkan.log`), so the failures pre-exist and are environmental, not from the
D4 diff. CI stays green because its runner uses lavapipe, which supports the feature.

## Decision

Capability gate in the test helpers (next-steps B4), two layers, both skip with a
named reason:

1. `UMBER_RUN_GPU_TESTS` env opt-in: GPU parity tests only run when the caller sets
   `UMBER_RUN_GPU_TESTS=1`. Default (local runs, safe-push's feature-unified matrix)
   skips, because umber-cli's `default = ["gpu"]` unifies `umber-bake/gpu` into every
   multi-crate `cargo test`, and this host's only adapters (Intel SKL via ANV, wgpu GL
   fallback) cannot run the shaders correctly: the `id_multi` shader needs
   `enable wgpu_binding_array` (fails validation), and `ground_plane`/`opacity`
   produce texel mismatches. Proven pre-existing: identical 15 failures at base
   `072442bb` (`pre-gpu-vulkan.log`). CI sets the env on its two dedicated lavapipe
   GPU steps (ci.yml), so coverage there is unchanged.
2. Adapter feature check: even with the env set, skip unless the adapter reports
   `wgpu::Features::TEXTURE_BINDING_ARRAY`, and request the device with that feature
   so shader validation matches the adapter.

```rust
if std::env::var_os("UMBER_RUN_GPU_TESTS").is_none() {
    eprintln!("skipping: UMBER_RUN_GPU_TESTS not set (2026-11-18 capability gate)");
    return None;
}
if !adapter.features().contains(wgpu::Features::TEXTURE_BINDING_ARRAY) {
    eprintln!("skipping: adapter lacks TEXTURE_BINDING_ARRAY (2026-11-18 capability gate)");
    return None;
}
// device requested with required_features: TEXTURE_BINDING_ARRAY
```

## Scope

- The `try_request_device`-style helpers in the 11 `umber-bake` GPU test modules
  (ao, curvature, dilation, gbuffer x2, ground_plane, id, normal_map, opacity,
  position x2, thickness, transfer): 13 helper sites get the two-layer gate; the
  device request gains `required_features: TEXTURE_BINDING_ARRAY`. No test body
  changes.
- `.github/workflows/ci.yml`: the two Linux lavapipe GPU test steps set
  `UMBER_RUN_GPU_TESTS=1` so CI keeps running the GPU family; the workspace
  `cargo test --workspace` step skips them (they were only running there by
  feature-unification accident).
- `umber-paint` / `umber-gpu` / `umber-cli` helpers already carry their own skip
  checks and produced zero failures in the failing matrix run; left untouched to keep
  the kill surface minimal (recorded here so the reviewer sees it was considered).
- No production code changes. No shader changes.

## Acceptance

- `cargo test -p umber-bake --lib` and the safe-push 8-crate matrix on this host:
  green (GPU tests skip with named reasons).
- `cargo clippy -p umber-bake --features gpu --all-targets -- -D warnings`: green
  (feature-on test code still compiles and lints).
- `cargo test -p umber-bake --lib --features gpu` with `UMBER_RUN_GPU_TESTS=1` on a
  capable adapter (CI lavapipe step) still runs them.
- rustfmt clean on touched files; `check-layers.py` + `scorecard.py --check` clean.
- `python3 scripts/safe-push.py` exits 0 and pushes all three local commits
  (`76f1c549`, `90aff880`, this kill); `git ls-remote origin master` shows the new
  tip; kill locks released.
