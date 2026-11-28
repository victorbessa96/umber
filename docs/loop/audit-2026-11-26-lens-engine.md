# Engine lens: late-review reconciliation (2026-11-28)

Review source: `deleg_320688ac-1`, targeting `ec979743`. Reconciled against `e2939773`. **No actionable finding remains at current HEAD.** All six reported items are repaired in code or explicitly narrowed in the current requirements; no new code is needed.

## Finding dispositions

| # | Disposition | Current evidence |
|---|---|---|
| 1 — Opacity, IdMulti, and GroundPlane cannot execute through the app worker | **Stale: already fixed** | `SpecialBakeParams` carries all three parameter blocks. Dedicated worker arms execute `bake_opacity_mesh_with_occluder`, `bake_id_multi_mesh`, and `bake_ground_plane_mesh` before the generic fallthrough. `worker_opacity_only_smoke`, `worker_id_multi_only_smoke`, `worker_ground_plane_only_smoke`, and `d3_d5_maps_execute_through_app_worker` cover real output decoding. |
| 2 — Auto-cage triangle indices use a 12-byte record against a 16-byte WGSL stride | **Stale: already fixed** | `transfer.rs` pads every low-triangle index to `[u32; 4]` before upload. The two-triangle CPU/GPU oracle was repaired and green; the full nine-crate gate at `4e3df3ec` includes the bake/GPU suites. |
| 3 — Material ID produces no contribution | **Stale: already fail-closed** | `IdBakeError::MaterialAssignmentUnsupported` is explicit, `validate_multi` rejects Material unconditionally, and the IdMulti editor exposes only Triangle, IdMap, and Manual. Crafted Material input gets a typed refusal rather than a silent empty bake. |
| 4 — “1M-tri + 4K measured” claim covers an unsampled set | **Already truth-tagged partial** | `requirements.md` and `release-checklist-0.1.0.md` now say the RTX 3070 record is 1,002,252 triangles plus a **resident-but-unsampled** 4K set. Full sampled-4K measurement remains an explicit open HW gate, not a measured claim. |
| 5 — UDIM cross-tile continuity marked built | **Already truth-tagged partial** | Requirements now use ◐ for both rows: per-event cross-tile painting is built, while tile-transition conditioner/spacing continuity remains a follow-up. `udim-design.md` retains the same honest v1 boundary. |
| 6 — Capability and module docs deny landed work | **Already fixed** | Dynamic material layering is ◐ v1 with a named sub-stack boundary; AssetStore GC is documented through `asset_gc::collect` and `umber gc`; merge-below-top and split-redo are documented with implementation references. The ID cache comment names the three-entry recency cache, the L2b comment names the per-channel density arms, and the transfer module documents the cage buffer. |

## Baseline rows retained from the review

Split redo and merge-below-top, L2b per-channel density arms, L3b multi-entry ID cache, unified timeline, angle/stamp/jitter, OccluderScope, dynamic material layering v1, displacement preview, and AssetStore GC remain closed.

## Verification evidence

- Full nine-crate gate at `4e3df3ec`: app, bake, brush, CLI, core, export, GPU, GUI, and paint suites all green.
- App: **931/931**; CLI: **123/123** plus every integration target; export: **122/122** plus **7/7** integration tests.
- GroundPlane Windows readback and control-timeout recovery are closed with targeted CI receipts; IdMulti source-bounds review returned **SHIP — zero findings**.
- No new verification run is needed for this docs-only reconciliation; the current tree and report claims were read directly after those green runs.

## Genuine remaining gates, not new code-ready defects

- Bind and measure the 4K texture set on real hardware to close the full sampled-4K performance requirement.
- Implement tile-offset conditioning if cross-tile spacing/smoothing continuity is promoted beyond its documented follow-up.
- WinInk live WndProc hand-off and Wintab32 polling remain partial. Passive decoding is not claimed as a live backend.
