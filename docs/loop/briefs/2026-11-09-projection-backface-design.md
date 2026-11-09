# Design brief — R-A2 v2c: the decal projector culls back-facing surfaces

Date: 2026-11-09 (row 297). Parent: `docs/specs/projection-fill-plan.md`
(v1, rows 277-281) + v2a (row 287, tiling) + v2b (row 289, follow camera).
Row 281's kill report cut line, verbatim:

> decal channel · NO seam/symmetry expansion of the projection · `.svg`
> sources

and the v1 plan §3 named the artifacts:

> Known v1 artifacts, all on the cut line: no depth test (a texel behind
> another surface still writes), no backface culling (both faces write),
> no tiling.

Tiling (v2a) and the live projector (v2b) are landed. **Backface culling
is the next cut.** This brief fixes its semantics; the kill brief carries
the implementation contract.

## 1. The problem, from the source

`rasterize_with` (`crates/umber-app/src/projection.rs`) is target-texel
driven:

```
texel → texel_center_uv → world_at_uv(mesh, uv) → view_proj * world
      → NDC → viewport-local px → inverse_map(transform) → bilinear sample
```

Every texel whose UV lands on the mesh is written, regardless of which way
that patch of surface faces. Consequences, both visible to an artist:

1. **Back faces take the decal.** A character's back UV island projects
   into the logo's screen region and gets painted with it. Orbit to the
   back and the logo is there.
2. **A sheet paints on both sides.** A plane's back face is not visible
   from the camera, yet it receives the decal.

The viewport's own mesh pass already draws the correct thing: the mesh
pipeline is `cull_mode: Back` + `FrontFace::default()` (CCW front faces)
(`crates/umber-gpu/src/renderer.rs:846`), and the CPU tangent-frame
builder reads the same convention ("Assumes CCW front faces, matching the
pipeline's `cull_mode::Back`", `renderer.rs:2106`). **The projector should
cull exactly what the viewport culls** — otherwise the artist sees a
surface the viewport says is not there.

## 2. The rule (decided)

For each texel, take the covering triangle's **geometric** normal
(`(p1 - p0) x (p2 - p0)` over the triangle's positions IN INDEX ORDER —
the same expression and the same index order the renderer uses) and the
texel's world point `p`. With the camera eye `e`:

```
front-facing  ⟺  dot(n, e - p) >= 0
```

- `dot < 0` → **cull** (skip the texel; it writes nothing).
- `dot >= 0` → keep (the `>=` boundary keeps grazing texels, so the
  silhouette has no one-texel flicker).
- A degenerate normal (zero-length cross product, non-finite) → **keep**.
  The rule never silently kills paint on malformed geometry; the same
  posture `front_facing_weight` takes for a non-finite `facing`
  (`crates/umber-mesh/src/raycast.rs:116`).

The eye comes from the SAME camera the raster projects through
(`OrbitCamera::eye()`, `crates/umber-gpu/src/camera.rs:120`): the frozen
placement's camera in the frozen path, the live camera in the follow path.

### Why the geometric normal, not `MeshData::normals`

`MeshData::normals` is per-VERTEX and optional (an importer may leave it
empty or short); the geometric normal always exists when positions and
indices do, and it is per-TRIANGLE — the granularity the GPU culls at.
Smooth-shaded vertex normals would make the cull disagree with the
viewport's silhouette.

### Why "the same as the viewport" and not "physically correct"

`dot(n, e - p) > 0` for a perspective camera is exactly the screen-space
winding test the rasterizer performs. Using the same index order and the
same CCW-front convention makes the CPU decision and the GPU decision the
same decision. A mesh with inverted winding is inside-out in the viewport
too — the projector agrees with what the artist sees instead of silently
disagreeing with it.

## 3. Where the code lives

One new seam in `umber-mesh`, one use in `umber-app`:

- **`crates/umber-mesh/src/uv_gbuffer.rs`** — one new public function that
  returns the world point AND the covering triangle's geometric normal
  from ONE barycentric scan:
  `world_and_face_normal_at_uv(mesh, uv) -> Option<([f32; 3], [f32; 3])>`.
  `world_at_uv` is re-implemented as a thin wrapper over it (same
  interpolation order → byte-identical results for its existing callers).
  Rationale: the raster already pays an O(triangles) scan per texel
  (`uv_to_barycentric`); a second scan for the normal would double it.
- **`crates/umber-app/src/projection.rs`** — the raster culls; the state
  gains the flag.

No new crate dependency. `umber-app` already depends on `umber-mesh`.

## 4. The flag and the default

`ProjectionState.cull_backfaces: bool`, **default `true`**.

- Default `true` is the artist-facing fix: the decal lands only on the
  surfaces facing the camera. This matches Substance Painter's projector
  (a screen-space operation that only affects the visible surface).
- The **escape hatch** is the checkbox turned OFF, which restores v1's
  "both faces write" behavior exactly.
- It is a placement mode, so `ProjectionState::adopt` KEEPS it (like
  `tiling` and `follow_camera`), and `arm`/`disarm` never touch it.
- It composes orthogonally with v2a (tiling wraps the image point) and
  v2b (follow picks the camera): the cull is a per-texel normal test, it
  reads neither.

**The default flip is user-visible and is echoed in the wave summary:**
before v2c, a decal reached back faces; after v2c it does not, unless the
artist unchecks "Cull back faces".

## 5. Depth culling is NOT in this kill

The other half of the plan's phrase — "a texel behind another surface
still writes" — needs the VISIBLE surface's depth at that screen pixel.
The CPU raster has no depth buffer, and the mesh crate's `UvGBuffer`
carries UV hits, not depth. Two routes exist, both out of scope here:

1. Read the GPU depth of the rendered frame (a new cross-crate seam into
   `umber-gpu`'s frame resources), or
2. Rasterize the mesh's depth from the camera on the CPU (a second
   rasterizer to maintain and to keep in sync).

Named as the deferred follow-up, with the route choice open.

## 6. Seam / symmetry expansion of the projection stays OUT

The v1 cut line also named "NO seam/symmetry expansion of the projection".
That is NOT this kill and NOT the next one, on purpose: a decal is a
screen-space placement, not a stroke — mirroring it across the UV axis
would duplicate a logo across the mesh in a way no reference tool does
(Painter's symmetry applies to strokes; its projector does not mirror).
The seam/symmetry question is therefore a PRODUCT question, not an
engineering one: it needs a decision on what a mirrored decal even means.
It is named here as deferred-with-a-question, not silently dropped.

## 7. Verification (what the kill must pin)

Mesh crate:
- `world_and_face_normal_at_uv` returns the same world point
  `world_at_uv` returns (byte-identical) and the unit geometric normal.
- A reversed-winding triangle yields the opposite normal (the cull's
  direction is the index order's, not the geometry's).
- A degenerate (collinear) triangle → `None` from the normal, and the
  raster KEEPS the texel.
- A UV miss → `None` (no texel written), unchanged from v1.

App crate:
- A front-facing quad takes the decal (the v1 pin, byte-identical with
  culling ON).
- The SAME quad with reversed winding takes NOTHING with culling ON.
- The same reversed quad takes the decal again with culling OFF (the
  escape hatch restores v1 exactly).
- Tiling ON + culling ON compose (the tiled decal still skips back
  faces).
- Follow-camera ON + culling ON compose (the live camera's eye is used).

## 8. Non-goals

- No depth / occlusion culling (§5).
- No seam/symmetry expansion (§6).
- No decal channel, no bake family, no 3D-projector family (plan §7).
- No persistence of the flag (same as v2a/v2b).
- No change to the overlay: the overlay still previews one quad.
- No change to the raster's per-texel budget: one barycentric scan per
  texel, as before.
