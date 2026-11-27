# D3–D5 app-worker routing hardening

- Lock: `razul-d3-d5-worker-routing`.
- Brief: `59e83d8bb4589f0df78e21271afe7f0fb85f43dc`.
- Scope: Opacity, IdMulti, and GroundPlane execute through the real app worker with their configured parameters, independent occlusion, correct data encoding, and collision-free output names.

## Initial failure and first implementation

The audit found all three maps selectable but unable to execute through `run_bake`. The first implementation routed them through owned `SpecialBakeParams`. Its initial RED (`Opacity is baked by its own arm`) became green, but independent review `deleg_e4b79b70` returned REQUEST_CHANGES with seven findings. Those findings drove the second implementation, not a commit.

## Review findings and dispositions

1. **GroundPlane AO/ID collided with ordinary mesh outputs.** GroundPlane now writes `ground_plane_ambient_occlusion`, `ground_plane_height`, and `ground_plane_id`, tiled or untiled.
2. **IdMulti collided with ordinary ID.** The panel now uses the established `id_multi` suffix while retaining ID's `MeshMapKind` for import purposes.
3. **New data maps were written through the sRGB curve.** Opacity, IdMulti, and all GroundPlane outputs now write `Transfer::Linear`.
4. **Material-split jobs lost WholeMesh occlusion.** GroundPlane selects the whole source mesh unless OwnSet is active. New `bake_opacity_mesh_with_occluder` separates raster and occluder meshes; the existing same-mesh entry delegates to it unchanged.
5. **IdMulti's IdMap source was always unbound.** The worker now bakes and uploads the actual ID texture whenever a configured source requires it, with the texture owner and view alive through the call.
6. **The picker advertised formats the worker could not decode.** It now advertises only lossless PNG, the format the worker actually reads, using the existing i18n key.
7. **The acceptance test checked only non-empty files.** It now asserts unique record paths and decoded pixel bytes against direct bake oracles, including 25% opacity, real IdMap content, and GroundPlane AO/Height/ID. The oracle accounts for `read_png_rgba8`'s documented row flip.

## RED/GREEN evidence

- Collision regression before naming fix: **7 records, only 4 unique paths**.
- After naming fix: **7 records, 7 unique paths**, with GroundPlane and IdMulti files present.
- Byte-exact worker test covers ordinary AO/ID in the same run as IdMulti and GroundPlane, actual IdMap sourcing, and non-0/255 data values.
- A separate two-material regression proves WholeMesh and OwnSet differ, then matches the worker to direct oracles in both scopes for Cavity and GroundPlane.
- Full app run caught and fixed one i18n regression from the initial PNG filter literal; the existing translation key is now used.

## Verification

- Focused `d3_d5_maps_execute_through_app_worker`: **1/0**.
- Focused `special_output_names_never_collide_with_mesh_maps`: **1/0**.
- Focused `whole_mesh_occlusion_reaches_cavity_and_ground_plane`: **1/0**.
- Full app: **1,289 passed, 0 failed** (323 lib + 919 binary + 47 integration).
- Full GPU-enabled bake after the opacity API change: **210 passed, 0 failed**.
- Strict clippy across all touched crates and targets: clean. Layers, scorecard, parity, rustfmt, and `git diff --check`: clean.
- Desktop-requested ad-hoc verifier: **8/8 focused regressions passed**, including both D3–D5 worker regressions; temporary script deleted. This is separate from suite evidence.
- Final independent review `deleg_240b61e8-1`: **SHIP**, all seven findings fixed, no new blocking defects. The reviewer noted only a non-blocking future multi-tile raw-pointer-key fragility in GroundPlane margin caching; current single-tile/Extend behavior is unaffected.
- Commit/push/current-tip CI: local commit next; origin and CI are not yet claimed.

## Non-goals

No new mask decoder dependency, no protocol/schema change, no release packaging, no upstream PR, no centralhost work.
