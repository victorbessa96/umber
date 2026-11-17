# Kill report: D5 ground plane bake — 2026-11-16

Kill: `razul-d5-ground-plane` (next-steps item 7). Brief:
`docs/loop/briefs/2026-11-16-d5-ground-plane.md` (aee6e2de).

## What landed

- `crates/umber-mesh/src/bake_support.rs`: `GroundPlaneDesc` +
  `generate_ground_plane` CPU mesh generator (size / height_offset /
  subdivisions).
- `crates/umber-bake/src/ground_plane.rs` (new): `GroundPlaneParams`,
  `GroundPlaneMap` (AO/Height/Id), CPU reference fns, GPU dispatch with
  `GROUND_PLANE_BAKE_SHADER` (3-layer output texture).
- `crates/umber-gpu/src/bake_shaders.rs`: `GROUND_PLANE_BAKE_SHADER`.
- `crates/umber-app/src/bakes_panel.rs`: `BakeSelection::GroundPlane`,
  settings group (size/height/subdivs/maps), `settings_snapshot` /
  `apply_settings` / `plan_bake` wiring, additive BakeSync persistence
  (no version bump), i18n keys in `en-US.ftl` + `pt-BR.ftl`.
- `crates/umber-app/src/viewport.rs`: `show_ground_plane` /
  `set_ground_plane_wire` viewport preview mirroring the D2 cage overlay.
- `crates/umber-gpu/src/view_mode.rs`: `MeshMapKind::GroundPlane` preview
  kind (preview id 10).

## Kittest-first DoD (9/9 pass)

Plane mesh gen (vertex/triangle counts, bounds), AO/height/ID CPU
reference parity, byte-identical-off, GPU-CPU bit-match, double-bake
compositing, weight/mask gating, preview routing.

## Recovery notes (HDD incident session)

The dispatched claw (deleg_e1f6c345) left the work UNCOMMITTED and the
kill-lock held; the dragon took the tree over, fixed the lint/test debt
it left (unused imports/fields, `///`-in-fn doc lints in umber-mesh,
preview-id test for the new kind, dead texture block in ground_plane.rs),
and landed the commit under the same lock. Battery at landing: clippy
`-D warnings` clean (4 crates, --all-targets), rustfmt clean,
check-layers GREEN (16 crates), scorecard current, tests 319+121+156+161
lib + 821 bin, 0 failed.

## Push

safe-push deferred to the project-watch lane (sibling ledger commit
28f6c467 rides the stack). Local tip recorded in queue row.
