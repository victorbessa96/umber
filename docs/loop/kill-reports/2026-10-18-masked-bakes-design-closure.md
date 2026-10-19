# Kill report: masked-bakes-design-closure (the per-set masked bakes design's verified-truth tail)

- Contract: the design doc's status line ("design, 2026-10-15... no
  code") was stale — the implementation landed in stages while the
  doc sat; the channel's "NO code" pointer (the sibling's 19:2x
  NEXT note) would have sent a session to rebuild landed work. This
  closure names what IS in the tree, by the doc's own §-order, and
  what genuinely remains.
- Kill-lock `masked-bakes-design-closure`:
  `docs/specs/per-set-masked-bakes.md` + this report. Claimed after a
  same-minute claim correction in the coordination note (the first
  claim this wakeup targeted §3+§4 as UNBUILT — false; the
  retraction pattern applied, the void lock released, no bytes
  built under it).

## Built

The verified-truth tail on the design doc: §1 (the field +
`submesh_for_material`) and §2 (all three importers: glTF's
all-or-nothing attach, FBX's per-face slots, OBJ's tobj pass)
LANDED; §3's RASTER half LANDED (per-set submeshes drive UV
coverage for curvature/position/normal/ID) while the ray half
(AO/thickness/bent/transfer) deliberately keeps the whole mesh —
the tree's own "with_occluder split is the named host-gated
follow-up" (bakes_panel.rs:2061-2068); §4's per-set fan-out LANDED
(`bake_jobs` → `submesh_for_material` per enabled set); §5's
tooling live (`assign_tile_to_set`). The named remaining order:
(a) the `with_occluder` split in umber-bake, (b) the
`OccluderScope` enum + panel row (double-gated: host compiler rule
+ the stencil-locked `.ftl` catalogs until recovery), (c) §5's
auto-mapping.

## Gates run

- Every anchor grep-verified against the live tree BEFORE writing:
  lib.rs:66/:87/:96, gltf.rs:23-40, fbx.rs:56-143, lib.rs:240-260,
  bakes_panel.rs:638-640/:1781-1815/:1797-1800/:1974-1989/
  :2061-2068 (the last read in full — the raster/ray split comment
  is the closure's load-bearing quote).
- The claim-correction discipline held: the false premise was
  retracted in the same minute, in the same file, named plainly —
  the channel's own pattern, applied to myself.
- Scope: pure markdown; nothing host-gated, nothing to compile.

## Recovery

Both files ride the wave commit's 5b extras (script extended +
re-pinned this same wakeup); the masked-bakes-design-closure lock
releases at that commit.
