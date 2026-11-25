# Design brief: EyeDepth visibility contract (2026-11-25)

Planned kill lock: `razul-eye-depth`.
Base: OBJ material/MTL status tip `b44ca50473ccd679ed35e72b6e6ee36bc3d3a947`, clean and origin-equal.

## What exists

The 3D stroke already raycasts every pointer against the mesh. `ray_intersect` returns the nearest positive hit; `pick_with` maps that hit to UV and `stroke_event` feeds it to the paint session. Hidden/excluded triangles are filtered through `visible_mesh`, and the start camera is frozen for the stroke. The existing raycast also has a positive-distance epsilon (`t <= 1e-5`) and a nearest-occluder regression. A second screen-space depth buffer would duplicate this visibility decision rather than add a new capability.

## Target

- Name the existing positive hit epsilon `EYE_DEPTH_HIT_EPS = 1e-5` in `umber-mesh::raycast` and use it at the existing rejection site. Document the EyeDepth contract: the nearest positive ray hit is the camera-visibility mask; no hit, including behind-eye/near-plane, produces no dab.
- Add an epsilon regression pinning below/at/above the threshold and nearest-occluder selection.
- Add a viewport GPU-readback regression with two overlapping quads at different depths and distinct UVs. The nearer quad must drive `stroke_event` and the tile readback, proving CPU hit selection and GPU paint agree.
- In the same test, a pointer ray that misses must return `None` and leave the tile bytes unchanged. No panic and no phantom dab.

## Failure handling

No new depth buffer, UI toggle, renderer change, or shader duplicate. Do not change the nearest-hit semantics or the epsilon value. A missing adapter skips the GPU test through the existing `try_request_device` pattern; it is never reported as a pass on unavailable hardware.

## Verification and acceptance

- `cargo test -p umber-mesh raycast` proves the epsilon and nearest surface.
- `cargo test -p umber-app eye_depth_nearest_visible_surface_drives_stroke_and_miss_writes_nothing` proves actual paint readback and no-hit byte identity when an adapter exists.
- Strict clippy on `umber-mesh` and `umber-app`, fmt, layers, scorecard, parity, and `git diff --check`.
- Independent read-only review, report, safe-push, and `git ls-remote origin master`.

## Non-goals

No new projection occlusion work, no GPU depth-buffer implementation, no new camera controls, and no change to projection rasterization. The theme matrix and COW A/B measurement remain queued after this contract closes.
