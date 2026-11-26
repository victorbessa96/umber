# Kill brief: D3–D5 app-worker routing

- Lock: `razul-d3-d5-worker-routing`.
- Trigger: engine audit at `ec979743` found Opacity, IdMulti, and GroundPlane selectable through `BakeSelection::ALL` but unable to execute through the app worker. `run_bake` falls through to `bake_extra_map`, which bails for all three, and `BakeRequest` carries none of their settings.
- Scope: one app file plus its own report. The full audit remains read-only on the same file.

## Targets

1. Add an owned special-bake parameter bundle to `BakeRequest` carrying:
   - `GroundPlaneParams` from the panel's size, height offset, subdivisions, map bitmask, rays, max distance, and bias;
   - `OpacityParams` from the panel's source, strength, invert, and shared ray count;
   - `IdMultiParams` from the panel's sources and blend mode at the current resolution.
2. Execute Opacity through the worker. For `IdMap` source, bake the ID map on the fly and pass its bytes to `bake_opacity_mesh`; no file picker is required.
3. Execute IdMulti through the worker with its configured sources and blend mode.
4. Execute GroundPlane through the worker. One job emits each selected AO/Height/ID output with its established suffix. Regenerate the ground plane for seam margining and dilate each output. Records remain `GroundPlane`, avoiding a schema change.
5. Add worker-level acceptance tests that drive `run_bake` or its real worker/request path, not only direct bake functions. Pin all three maps, GroundPlane multi-output naming/content, and the previous bail behavior as RED before implementation where practical.

## Concurrency and compatibility

One code file and one kill report. No public schema, serialization format, device feature, dependency, UI redesign, or release action. Existing bakers and output behavior remain byte-identical outside the previously unreachable selections.

## Verification

- Focused worker tests for Opacity (including IdMap), IdMulti, and GroundPlane multi-output.
- Full `umber-app` suite and full `umber-bake --features gpu` suite single-threaded.
- Workspace clippy, rustfmt, static gates, and an independent bounded review.
- Commit/push via `safe-push.py` and dispatch current-tip CI.

## Non-goals

No changes to the direct bake algorithms repaired earlier, no new UI controls, no material-picker redesign, no public PR, no centralhost, and no packaging work.
