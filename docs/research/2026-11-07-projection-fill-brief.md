# Research brief — projection fill (R-A2): screen-space material projection

- **Status:** research brief (the first artifact of the plan-kill sequence:
  brief → research → plan → implementation). No code in this wave.
- **The row:** `docs/ROADMAP.md` — "Projection fill (R-A2) | screen-space
  material projection | the decal workflow's other half; pairs with the
  landed stencil; rides the pwys design". The parity table still shows
  "Stencil/projection — PARITY on stencil (the lane closed); projection
  fill GAP (R-A2)".
- **What is already in the tree (counted from source, not memory):**
  - the STENCIL lane (landed): `umber_core::stencil::StencilImage` (an
    8-bit coverage plane), the app loader `load_image_rgba` (PNG and
    `.svg`, rows 266 + 269), and the paint seam
    `PaintState::set_stencil_gate(Option<(u32, u32, Vec<u8>)>)`
    (`crates/umber-paint/src/paint_state.rs:5761`) — a screen-space
    coverage mask that gates the paint.
  - the STROKE BUFFER (landed, row 272): the viewport freezes the camera
    at the stroke's first event (`stroke_camera: Option<OrbitCamera>` in
    `crates/umber-app/src/viewport.rs`) and resolves the stroke's
    raycasts against that snapshot.
  - the paint path: `PaintState::begin_screen_point_pressured` /
    `extend_screen_point_pressured` take an ALREADY-RESOLVED hit; the
    viewport resolves the pointer to a UV (`PaintProjection::Screen`) and
    the paint crate never sees the camera.
  - the projection enum: `PaintProjection::{Uv, Screen}`
    (`crates/umber-paint/src/paint_state.rs:456`) — the seam where a
    third projection mode would live.
- **The decision this brief takes (the dragon's call, recorded):** R-A2 is
  a **decal projector**: the user places a screen-space IMAGE (a logo, a
  decal, a reference picture) over the viewport; it is PROJECTED onto the
  mesh through the camera frozen at the placement moment, and the paint
  layers receive it as ordinary paint (so the history, the layers, the
  symmetry and the export need no new concepts). The stencil is the
  decal's MASK: the same screen-space coverage machinery that gates a
  stroke gates the projected image. The rejected alternative — a separate
  "decal channel" that the exporter composites later — is a bigger
  concept (new channel semantics, new export path, new undo shape) and
  buys nothing the paint layers do not already buy.

## The question the research must answer

What does a screen-space material projection look like in the tools
painters already use, and what is the smallest honest Umber version of it?

## The references to mine (public sources only)

- **Substance 3D's decal / projection workflow** — how a decal is
  placed, scaled, rotated, masked, and how its projection interacts with
  the camera and with the stencil.
- **Mari's projection model** — the screen-space projection semantics
  (what is frozen, what re-projects, what the artist controls).
- **Blender's "Project from View" / decal workflow** — the placement
  transform set and the freeze semantics.
- **The attribution ruling applies:** findings land in repo-facing docs as
  "exploration results assembled from online sources" — at most that
  phrase; never name the actual sources, never use "reverse-engineered",
  never name tools, binaries or methods involved.

## The inventory the research must return (one row per item)

1. **Placement transform set** — which controls the tools expose
   (position, scale, rotation, perspective/orthographic, opacity) and
   which of them Umber's v1 should carry.
2. **Freeze semantics** — when the projection is captured (at placement?
   at stroke start?), what happens if the camera moves afterwards, and
   how the tools communicate that to the artist.
3. **Masking** — how the decal's own alpha and the STENCIL interact
   (multiplied? the stencil clips the projection?), and whether the
   stencil is the decal's natural mask in the references too.
4. **Write semantics** — does the projection write into the paint layers
   (and therefore the history) or into a separate channel? Cite each
   reference's answer.
5. **The projection seam** — what `PaintProjection`'s third variant
   would carry, and which side (app or paint crate) resolves it.
6. **Effort + priority per row**, and the v1 cut line: what is in v1 and
   what is a named follow-up.

## Non-goals for the research

- No GPU reprojection (the pwys design's explicit non-goal).
- No live re-projection of an already-placed decal while the camera
  moves (that is the pwys option B follow-up, not this row).
- No new export format, no new channel type, no bake node.

## The output

`docs/research/2026-11-07-projection-fill-research.md` — the inventory
above with the sources cited, a summary table ranked by impact/effort, and
the v1 cut line. The PLAN (the next artifact, `docs/specs/`) turns the
cut line into wave rows with disjoint file surfaces.
