# Kill report: masked-bakes ray-half implementation brief (2026-10-18)

Dragon-authored host-free by the Umber /loop session "Work on Umber
project coordination #3" (local-d3deea1d3c7631cc80f8782e73c37fe384c4c50d1248d89def19e4801b4c9a45, under Bessa's live steer
"do all the work that you can locally … and do not stop"). Kill-lock
`masked-bakes-ray-half`, holding
`crates/umber-app/src/bakes_panel.rs`, `crates/umber-app/src/bake_sources.rs`,
and the brief
`docs/loop/briefs/2026-10-18-masked-bakes-ray-half.md`.

Context: the masked-bakes design's closure tail (the sibling's
`masked-bakes-design-closure` kill) names the ray half as the next
code work: AO/thickness/bent keep the whole-mesh bake today while
the raster maps already bake per-set. The sibling's note left the
code lane open. Claimed and scouted to completion this pass.

## What the scout found (the load-bearing facts)

- The baker-level split ALREADY EXISTS: `bake_ao_mesh_with_occluder`
  (ao.rs:581), `bake_ao_and_bent_mesh_with_occluder` (ao.rs:604),
  `bake_thickness_mesh_with_occluder` (thickness.rs:231) — the plain
  fns delegate with `(mesh, mesh)`.
- The panel's job loop already carries the per-job submesh
  (`raster_mesh`/`raster_source`/`raster_is_submesh`,
  bakes_panel.rs:2061-2072) and the raster maps already route
  through the reusable `raster_margin` seam machinery
  (:1974-1989, per-submesh seam graphs cached).
- `bake_extra_map` (:2276) routes BentNormal to
  `bake_ao_and_bent_mesh` directly — the one open design question of
  the brief settled by scout: an `occluder: Option<&MeshData>`
  parameter, `None` = today's bytes exactly.

## Built (the brief as the park — a plan kill)

`docs/loop/briefs/2026-10-18-masked-bakes-ray-half.md`: the
contract (mapped sets raster their submesh against the whole-mesh
occluder; unmapped meshes keep byte-for-byte output; no UI, no i18n),
the six changes with file:line anchors (the two new bake_sources
helpers; the Ao arm's mapped branches; the Thickness arm; the
BentNormal arm's occluder parameter; the stale comment's update),
the four test gates (per-set divergence on the two-material fixture;
the cross-material occlusion pin — the property a naive submesh bake
would LOSE; the unmapped no-regression gate; the per-set margin),
and the rollout (one kill, battery umber-app + umber-bake at the
recovery; the OccluderScope slice stays i18n-double-gated).

## Judgment calls

1. Brief over code at this park: the session is deep in compression
   territory; a six-call-site surgical kill started now risks a
   half-built park. The brief carries every decision — the next pass
   (either dragon; the lock decides) builds mechanically in one
   focused sitting.
2. The kill-lock holds the two code files NOW so no parallel build
   collides with the brief's exact anchors; the code bytes on disk
   are UNTOUCHED by this kill (the lock is the claim, the manifest
   pins the brief + this report only — the code files' pins join
   when the build lands).

## Unverified

- By design: the code kill's gates (fmt/battery/GPU tests) belong to
  the build pass; this park's verification is the brief's
  factual census — every anchor grep-verified live this session.

## Gates run (host-free, this session)

- `scripts/verify-kill.sh` on the brief: PASS (doc-attachment, trap
  scan, lock audit).
- Focused verifier: the brief's every cited symbol and line anchor
  re-checked against the live tree + the manifest pins.

State: the BRIEF is VERIFIED and parked; the CODE build is the named
next pass. The lock releases at the code kill's commit, not before.
