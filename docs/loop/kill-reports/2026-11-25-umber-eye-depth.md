# EyeDepth visibility contract

- Date: 2026-11-25.
- Author: Razul (dragon), session `20261124_193811_e97c1e`.
- Brief: `docs/loop/briefs/2026-11-25-umber-eye-depth.md` (`078ef410d0ed`).
- Claim: `28504429fbff`, queue row 610.
- Implementation: `b62397c2fbe6ad893788f3f5398a07e80f0b1b10`.
- Reviewed implementation-delta SHA-256: `994466e720f7bf05b0a55a39bf70505cb76adf949a0c19731b0e7d998aa5ed43`.
- Kill lock: `razul-eye-depth`.

## Built

- Named the existing nearest-hit epsilon `EYE_DEPTH_HIT_EPS = 1e-5` in `umber-mesh::raycast` and used it at the existing rejection site. The doc now states the contract: the nearest positive ray hit is the 3D stroke's camera-visibility mask, and no hit means no dab.
- Added `eye_depth_epsilon_rejects_at_and_below_threshold` to pin below/at/above behavior and the ordinary positive hit.
- Added `viewport::tests::eye_depth_nearest_visible_surface_drives_stroke_and_miss_writes_nothing`: a z=2 quad wins over a z=0 quad, its UV gradient drives `stroke_event`, the actual tile readback contains paint, and an off-mesh event returns `None` with byte-identical tile data. GPU row order uses `(1 - v)` for the lookup.
- Deliberately did not add a second depth buffer, shader, renderer path, or UI toggle: the existing nearest-hit pick already performs camera-visibility selection.

## Review and dispositions

- Independent review `deleg_c43df54b`: **SHIP, zero findings**.

## Verification

- `cargo test -p umber-mesh raycast` → **8 passed / 0 failed**, including the epsilon boundary and nearest occluder.
- `cargo test -p umber-app eye_depth_nearest_visible_surface_drives_stroke_and_miss_writes_nothing` → **1 passed / 0 failed** with actual paint at the nearer surface and unchanged bytes on a miss.
- `cargo clippy -p umber-mesh -p umber-app --all-targets -- -D warnings` → clean.
- `cargo fmt --all -- --check`, `check-layers.py`, `scorecard.py --check`, `parity.py --check`, and `git diff --check` all pass.
- Fresh self-deleting `hermes-verify-obj-eye-*.sh` probe: **11 targeted tests passed / 0 failed** (9 OBJ core + EyeDepth + keyboard journey), strict export/mesh/app clippy clean, and static parity/layers/scorecard/diff gates green. The script was verified deleted. This is focused ad-hoc evidence, not a workspace-green claim.

## Judgment calls

- The initial probe used a constant UV and produced no dab. The final test uses a real UV gradient and sixteen events, which is the actual stroke-spacing path rather than a synthetic shortcut. The center UV is asserted once; subsequent hits must remain on the nearer quad.
- No adapter means the existing `try_request_device` skip path, not a false pass.

## Unverified

- Full workspace tests are not rerun for this two-file contract; only the focused mesh/app tests, clippy, and static gates ran.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- The theme matrix and COW A/B measurement remain queued after this kill.
