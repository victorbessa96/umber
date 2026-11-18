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

Capability detection in the test helpers (next-steps B4): after the adapter is
acquired, skip with a named reason unless the adapter reports
`wgpu::Features::TEXTURE_BINDING_ARRAY`:

```rust
if !adapter.features().contains(wgpu::Features::TEXTURE_BINDING_ARRAY) {
    eprintln!("skipping: adapter lacks TEXTURE_BINDING_ARRAY (2026-11-18 capability gate)");
    return None;
}
```

Rationale: every GPU-parity shader in `umber-bake` is validated against lavapipe in CI,
which supports the feature; an adapter without it (SKL, GL fallback) cannot run the
binding-array shader and has shown texel divergence in the storage-3D/AO paths, so
running there produces noise, not signal. Skip = pass, with the reason printed, exactly
like the existing "no wgpu adapter available" skip.

## Scope

- The `try_request_device`-style helpers in the 11 `umber-bake` GPU test modules
  (ao, curvature, dilation, gbuffer x2, ground_plane, id, normal_map, opacity,
  position x2, thickness, transfer). Insert the check after the adapter binding in
  each existing shape; no helper is rewritten, no test body changes.
- `umber-paint` / `umber-gpu` / `umber-cli` helpers already carry their own skip
  checks and produced zero failures in the failing matrix run; left untouched to keep
  the kill surface minimal (recorded here so the reviewer sees it was considered).
- No production code changes. No feature-flag changes. No shader changes.

## Acceptance

- `cargo test -p umber-bake --lib --features gpu` on this host: green via named skips
  (SKL lacks TEXTURE_BINDING_ARRAY).
- `cargo test -p umber-bake --lib` (no feature): still 121+ passed / 0 failed.
- clippy `-p umber-bake --all-targets -- -D warnings` green; rustfmt clean on touched
  files; `check-layers.py` + `scorecard.py --check` clean.
- `python3 scripts/safe-push.py` exits 0 and pushes; `git ls-remote origin master`
  shows the new tip; then the D4 commits (`76f1c549` + code commit) ride along and
  both kill locks are released.
