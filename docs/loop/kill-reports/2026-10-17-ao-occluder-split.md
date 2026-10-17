# Kill report: ao-occluder-split (per-set masked bakes, §3 bakers half)

- Authorship: dragon (the 14:16 loop session), taking the named
  follow-up while the sibling holds rail-row-labels.
- Kill-lock `ao-occluder-split`: `crates/umber-bake/src/ao.rs`,
  `crates/umber-bake/src/thickness.rs` (lib re-export rides).
- Implements `docs/specs/per-set-masked-bakes.md` §3's baker split:
  the ray maps' occluder argument made explicit — `with_occluder`
  entry points where the position map (WHERE rays cast from) comes
  from the raster mesh and the raycast triangles (WHAT rays may hit)
  come from the occluder mesh.

## Built

- `bake_ao_mesh_with_occluder` + `bake_ao_and_bent_mesh_with_occluder`
  (`ao.rs`): the shared `bake_ao_inner_with_occluder` core; the
  single-mesh entries delegate with `occluder == raster` —
  byte-identical to today's output (the design's step-3
  no-regression contract).
- `bake_thickness_mesh_with_occluder` (`thickness.rs`): same split,
  same delegation shape.
- lib.rs re-exports all three.
- An empty occluder (no triangles) yields a fully-lit map — nothing
  occludes; `EmptyMesh` validation stays on the raster side only.

## Tests (GPU, lavapipe)

- `with_occluder_same_mesh_is_byte_identical`: the split entry passing
  one mesh twice == `bake_ao_mesh`'s bytes, end to end — the
  no-regression pin.
- `with_occluder_empty_occluder_is_fully_lit`: same raster ground
  quad, empty occluder — every covered texel reads 255, proving the
  occluder buffer genuinely narrows the raycast.

## Verification

- fmt + clippy `-D warnings` clean (local + battery).
- Host battery GREEN (4 crates, centralhost 14:4x): bake 92 + mesh
  129 + app 787 + paint 170 — the app count includes the sibling's
  concurrent rail-row-labels landing (787 vs the 778 of my last
  battery; its files are outside my lock).

## Deferred (named)

- The PANEL row wiring (`OccluderScope` in the per-set bake settings)
  is step 4 of the design — UI work on `bakes_panel.rs`, which the
  sibling may claim; the landed `occluding_triangles` + these
  `with_occluder` entries are its complete data lane.
- Transfer keeps whole-mesh rays by design (its rays come from the
  HIGH mesh — §3's own note).
