# Scheduled-CI GPU parity repair

- Lock: `razul-ci-gpu-parity`.
- Brief: `444904f870f21b85e93590b8c2a50f57fcefe220` plus its implementation addendum.
- Claim: `b1248d6098829ae701e854137d376ce0942a62b4`.
- Implementation: `543e038ea9706ee7edd67b27408e7bc51d2b9954`.
- Trigger: scheduled CI run `45221968683`, Ubuntu job `137858417245`, failed at `7f0c976bdedf923cb671f9372624c25034063bf3` with 13 `umber-bake --features gpu` failures.
- Prior pushed truth: `d6b8110909c4a09afbaf72df08ad3a002e6c1595` incorrectly declared code-ready completion without consuming this run.

## Reproduction

At the prior tip, on the local Intel Mesa 26.3.1 adapter with `UMBER_RUN_GPU_TESTS=1`:

- `ground_plane_gpu_ao_matches_cpu_texel_for_texel` failed at `(0,0)`: GPU 255 vs CPU 80, matching CI lavapipe.
- `id_multi_cpu_gpu_parity_texel_for_texel` failed shader validation at `textureLoad(mask_tex[i], coord, 0)` because non-uniform indexing capability was not requested.

These were live defects, not stale CI artifacts and not lavapipe-only failures. No bake source changed between the failed run's tip and the reproduction.

## Root causes and fixes

### Ground plane

1. `generate_ground_plane` wrote `+Y` vertex normals but emitted triangles whose recomputed face normals were `-Y`. The position pass therefore sent scene rays away from the scene. The winding is now `+Y`, pinned by `ground_plane_face_winding_points_up` (RED before the fix, GREEN after) and full position/normal parity through the real GPU pass.
2. Height and ID shaders cast `-Y` despite the scene being above the plane. Both the retained 3D shader and active 2D fallback now cast `+Y`.
3. The CPU oracle inferred resolution from plane subdivisions, ignored the raster Y flip, and used a different tangent frame from the shader. It now takes actual target dimensions, mirrors the UV G-buffer Y mapping, and mirrors the Duff basis exactly. The direction probe proved the old CPU Z component was inverted while the GPU basis was correct.
4. `ground_plane_position_map_matches_cpu_authority` was added to pin every position and normal independently of the AO/height/ID raycasts. No tolerance was widened.

### IdMulti

1. The binding-array mask load required non-uniform indexing capability. Eight ordinary texture bindings (5..=12) now replace the array, with literal branches in `mask_covers`. This also avoids the default binding-array element limit of zero discovered after the first draft. The tests request a default device.
2. The old Rust uniform layout was 28 bytes per source and 240 bytes total; WGSL requires 32-byte records and 272 bytes total. Explicit padding and alignment plus compile-time size assertions now pin the ABI.
3. The mask-view list is padded to eight entries, preserving every source's mask association and dummy-view behavior.
4. Replace previously used accumulated RGB zero to decide whether a source had landed, so a covered black first source could be overwritten. `replace_first_set` now mirrors the CPU reference. `id_multi_replace_black_first_still_wins` was RED (`[255,0,0,255]` instead of `[0,0,0,255]`) before the fix and GREEN after.
5. `id_multi_each_mask_binding_selects_its_source` exercises every ordinary binding 0..7 with earlier slots empty.

## Verification

- Ground-plane module: **15 passed, 0 failed** with the GPU feature enabled.
- IdMulti GPU module, including the two new regressions: **12 passed, 0 failed** after the final additions.
- Full `cargo test -p umber-bake --features gpu -- --test-threads=1`: **206 passed, 0 failed**, zero ignored.
- `cargo test -p umber-mesh -p umber-gpu`: **156 umber-gpu / 167 mesh-lib**, zero failures; mesh integration targets also green (15+1 ignored, 2, 32+1 ignored, 9).
- Standard strict clippy on all three touched crates: clean.
- Slash-form GPU clippy on `umber-gpu` and `umber-bake`: clean.
- CI-equivalent `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `rustfmt`, `check-layers.py`, `scorecard.py --check`, `parity.py --check`, and `git diff --check`: clean.
- Independent final review: `deleg_cf9f2d78` returned **SHIP, zero findings** after independently checking the ABI, basis, winding, UV orientation, mask bindings, and Replace semantics.
- Committed-HEAD rerun at `543e038ea9706ee7edd67b27408e7bc51d2b9954`: **206 passed, 0 failed** with the GPU feature enabled.
- Current-tip CI: pending; this section must name the actual run before the wave closes.

## Judgment calls

- Ordinary bindings were chosen over requesting binding-array features/limits because they are the smallest default-device-compatible implementation and remove two independent device requirements at once.
- The original D5 brief's downward direction was wrong for its geometry and is corrected to +Y. The repair brief's initial literal-index design is superseded by the ordinary-binding implementation addendum.
- Debug hit-mask/direction instrumentation was removed before the final diff; only production behavior and permanent regressions remain.

## Unverified and gates

- Current-tip Windows and lavapipe CI are not claimed green until a run tied to the final implementation SHA completes successfully. Local Intel evidence is not represented as cross-platform evidence.
- Human dogfood, real-pen, native-speaker, signing, and release decisions remain unchanged and outside this kill.
