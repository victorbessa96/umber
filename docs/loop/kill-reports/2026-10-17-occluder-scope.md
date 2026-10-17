# Kill report: occluder-scope (per-set masked bakes, §3 data half)

- Authorship: dragon (solo, host-down pass).
- Kill-lock `occluder-scope`: `crates/umber-bake/src/lib.rs`.
- Implements `docs/specs/per-set-masked-bakes.md` §3's occluder
  scope — the ENUM and its triangle resolution, pure and tested. The
  baker signature change (`with_occluder(mesh)` beside the raster
  mesh) and the panel row remain host-gated (the design doc's step 3,
  which lands WholeMesh byte-for-byte first).

## Built

- `OccluderScope { WholeMesh, OwnSet, Sets(BTreeSet<u16>) }` with
  `Default = WholeMesh`:
  - **WholeMesh**: today's behavior, every triangle occludes — the
    reference tools' agreed default (AO/thickness rays cross
    materials).
  - **OwnSet**: Marmoset's Ignore Groups — only the baked set's own
    triangles.
  - **Sets**: the named sets plus the baked set itself — the
    glass-panel case Substance Painter cannot express per-set
    ("everything but Glass darkens the gauges"). An empty list
    degenerates to OwnSet.
- `occluding_triangles(triangle_materials, baked_set) -> Vec<u32>`:
  resolves the scope against the mesh's triangle→material map (the
  field step 1 lands host-side). An EMPTY map — every existing mesh
  today — resolves to no named triangles, which the caller treats as
  the whole-mesh fallback: the v1 no-regression contract holds
  without the field.
- Tests (2 in the BakeMap tests mod): whole-mesh is every triangle;
  OwnSet/Sets resolution including the always-include-self rule, the
  empty-Sets degeneration, Default, and the empty-map fallback.

## Judgment calls

- **The RAYS narrow, not the raster**: the doc's ruling encoded —
  `occluding_triangles` answers only the raycast mesh; the raster
  submesh is the caller's `filter_mesh_for_triangles` call (the
  subset-mesh kill's primitive).
- **Sets implies the baked set itself**: a set that excluded its own
  triangles from its own bake's rays would produce a DIFFERENT map
  for the same inputs depending on scope choice — the
  always-include-self rule keeps the baked set's AO self-occlusion
  stable across scopes. Pinned by test.
- **Empty map = fallback, not panic**: step 1 has not landed, so
  every mesh today passes `[]`; the resolution names nothing and the
  caller (once wired) falls back to the whole mesh — matching
  today's byte-identical output until the importers fill the field.

## Unverified

- Battery (host down since 18:25). Static: rustfmt PASS; derives
  verified (PartialEq for the test's assert_eq); no exhaustive-match
  sites exist yet (the enum is new, no consumers until step 3).

## Deferred

- Step 3's baker plumbing: `bake_ao_mesh`'s occluder argument (the
  GPU signature change, host-verified), then thickness + bent normals.
- Step 4's panel row + step 1's `triangle_materials` field (the
  148-site compiler-guided pass).
