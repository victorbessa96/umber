# Kill brief: scheduled-CI GPU parity repair

- Status: implementation brief, 2026-11-25.
- Lock: `razul-ci-gpu-parity`.
- Trigger: scheduled CI run `45221968683`, job `137858417245`, failed at `7f0c976bdedf923cb671f9372624c25034063bf3` with 13 `umber-bake --features gpu` failures. The previous code-ready documents did not include this run in their closure evidence.

## Reproduced current failures

At `d6b8110909c4a09afbaf72df08ad3a002e6c1595`, on the local Intel Mesa 26.3.1 adapter with `UMBER_RUN_GPU_TESTS=1`:

- `ground_plane::gpu_tests::ground_plane_gpu_ao_matches_cpu_texel_for_texel` fails at `(0,0)`: GPU 255 vs CPU 80. This matches CI, so the failure is not lavapipe-specific.
- `id::tests::gpu::id_multi_cpu_gpu_parity_texel_for_texel` rejects `umber_bake_id_multi_shader`: `textureLoad(mask_tex[i], coord, 0)` requires `TEXTURE_AND_SAMPLER_BINDING_ARRAY_NON_UNIFORM_INDEXING`, which the test does not request and the implementation does not gate on.

CI additionally reports the other three ground-plane parity tests and the remaining nine IdMulti GPU tests failing for the same two root mechanisms. No bake source changed between the failed CI tip and the current tip, so these are live defects, not stale-run artifacts.

## What exists

### Ground plane

`generate_ground_plane` writes vertex normals as `+Y`, but emits each quad as `(i0,i1,i2)` and `(i1,i3,i2)`. With X advancing first and Z second, the face cross product is `-Y`. `bake_position_and_normal` recomputes the face normal from that winding, so the ground-plane shader raycasts downward away from the scene. The CPU reference assumes `+Y`. Height and ID also use `-Y` in both WGSL versions even though their scene is above the generated plane.

The CPU test helpers additionally infer texture resolution from plane subdivisions instead of the bake target dimensions and do not mirror the UV G-buffer's Y flip. That oracle defect must be corrected without weakening the expected production semantics.

### IdMulti

The shader indexes a binding array with loop variable `i`, which is non-uniform. Naga requires the non-uniform indexing capability. The implementation requests only `TEXTURE_BINDING_ARRAY`, so the current tests fail before dispatch on both CI lavapipe and local Intel. The source-count loop is bounded by `min(params.sources.len(), 8)` and the layout always binds eight views, so a fully unrolled eight-iteration shader removes the non-uniform capability requirement without changing source semantics.

## Target

1. Make generated ground-plane face normals agree with the existing `+Y` vertex-normal contract, with a regression test that fails on the current winding.
2. Cast ground-plane Height and ID rays toward the scene (`+Y`) in both the active 2D fallback and the retained 3D shader.
3. Make the CPU parity oracle use actual bake-target dimensions and the same Y-oriented UV mapping as the UV G-buffer. Keep byte-exact assertions; do not relax them.
4. Replace IdMulti's non-uniform binding-array load with an eight-step unrolled, compile-time-indexed implementation. Preserve source count, zero-weight, mask, Replace-first-covered, and uncovered behavior. Request no new device feature.
5. Run every previously failing test plus the full `umber-bake --features gpu` suite single-threaded, standard and GPU clippy, static gates, and an independent review.

## Concurrency model

One kill lock covers the mesh generator, both WGSL variants, the ground-plane Rust oracle/tests, IdMulti shader/driver as needed, and close-out docs. No sibling edits or cargo overlap. GPU tests run one test thread under `/tmp/umber-cargo.lock` with `CARGO_BUILD_JOBS=3`, stripped dev/test debug info, and `nice 19`.

## Security and compatibility model

No new dependency, file format, project schema, network behavior, or device-feature requirement. Geometry behavior changes only where the old path provably cast away from the scene. IdMulti's public API and serialized configuration remain unchanged. No tests are skipped or converted into adapter-dependent passes.

## Verification

- RED evidence already captured for one test in each defect class at the current tip.
- Add a CPU face-winding regression and prove it fails before the generator fix.
- After the fix, run all 13 CI-failing tests to green, then `cargo test -p umber-bake --features gpu -- --test-threads=1` with a nonzero expected count and zero failures.
- Run standard and slash-form GPU clippy, rustfmt, `check-layers.py`, `scorecard.py --check`, and `parity.py --check` if requirement-facing text changes.
- Review the final diff independently, then record a kill report and truth the current work map. Push only the completed wave through `scripts/safe-push.py` and verify `origin/master` by read-back.

## Non-goals

No ground-plane feature redesign, product-default decision, GPU capability expansion, release tag, centralhost contact, public PR, or broad refactor. No hiding the failures behind adapter skips or loosening parity tolerances.

## Implementation addendum

- Target item 4 is superseded by the stronger default-device design: eight ordinary texture bindings (5..=12) replace the binding array entirely. This avoids both the non-uniform-indexing capability and the default `max_binding_array_elements_per_shader_stage = 0` limit discovered after the first literal-load draft. The test uses a default device to prove it.
- The same pipeline exposed an independent ABI defect: WGSL records require a 32-byte `IdSourceUniform` stride and 272-byte `IdMultiUniform`, not Rust's old 28/240-byte layout. Explicit padding, alignment, and compile-time size assertions now pin the contract.
- A new black-first Replace regression found and closed an existing semantic bug: first-covered wins even when the first source's RGB is zero. An additional acceptance loop covers every mask binding 0..7.
- The original D5 brief's downward Height/ID direction was wrong for a plane below the scene and is corrected to +Y in the same kill. The CPU AO oracle also had to mirror the shader's Duff basis, whose second axis is -Z for +Y.
