# Projection fill (R-A2) — research: screen-space material projection

- **Artifact:** the research of `docs/research/2026-11-07-projection-fill-brief.md`
  (the brief's question, the tree's current state and the inventory it asks
  for). The next artifact is the PLAN (`docs/specs/`), which turns the v1 cut
  line below into wave rows with disjoint file surfaces.
- **Sources:** public product documentation, cited inline. The attribution
  ruling applies to anything derived from shipped artifacts; nothing here is.

## 1. The question

What does a screen-space material projection look like in the tools painters
already use, and what is the smallest honest Umber version of it?

## 2. What the tree already has (counted from source)

| Seam | Where | What it gives the projection |
| --- | --- | --- |
| the stencil plane | `umber_core::stencil::StencilImage` (8-bit coverage) | a screen-space mask |
| the stencil gate | `PaintState::set_stencil_gate` (`crates/umber-paint/src/paint_state.rs:5761`) | the paint-side seam a screen-space mask rides |
| the loader | `load_image_rgba` (rows 266 + 269) | PNG and `.svg` sources, already rasterized |
| the stroke buffer | `stroke_camera: Option<OrbitCamera>` (`crates/umber-app/src/viewport.rs`, row 272) | a camera FROZEN at a chosen moment |
| the paint entry | `begin_screen_point_pressured` / `extend_screen_point_pressured` | takes an already-resolved hit; the paint crate never sees the camera |
| the projection enum | `PaintProjection::{Uv, Screen}` (`paint_state.rs:456`) | the seam a third variant would occupy |

## 3. The inventory (what the references do)

### 3.1 Placement transform set

| Reference | Controls | Notes |
| --- | --- | --- |
| Painter's **Projection** tool (Adobe, "Projection") | translate, scale (zoom), rotate — the SAME chords as the stencil (S + middle / right / left drag, Shift snapping the rotation) | "a tool that allows to paint a material by projecting it in screen/viewport space… It shares similar controls to the stencil"; the projection can REPEAT a pattern over the viewport |
| Painter's **planar projection** (a fill-layer projection, not the screen-space tool) | UV transform (tiling, rotation, offset, physical size) + 3D transform (offset, rotation, scale) + depth culling, backface culling, shape crop, UV wrap, filtering | the full 3D-projector family; out of Umber's v1 |
| Mari's **paint buffer** | Translation, Scale, Rotation, Painting Opacity, Buffer Size, Reset-on-bake | a screen-space canvas that "floats above the model until you bake it on" |
| Blender's **UV Project modifier** | projector object (a camera or an empty), aspect X/Y, scale X/Y (camera projectors only), up to ten projectors | "acts like a slide projector… each face will choose the closest and aligned projector with its surface normal" |

**Reading:** every reference carries exactly THREE placement transforms —
translate, scale, rotate — plus opacity. Tiling/repeat exists only where the
projection is a FILL (3D) rather than a screen-space tool.

### 3.2 Freeze semantics (the sharpest difference between the references)

| Reference | What happens when the camera/model moves | Where the paint lands |
| --- | --- | --- |
| Mari's paint buffer | the buffer is tied to the view: with `AutoBakeAndClear` it bakes automatically when you move the model; with `Manual`/`ClearOnly` you can move the model UNDER the buffer and bake by hand | on BAKE, in SURFACE space, through the chosen blend mode and opacity |
| Painter's Projection / stencil | the projection and stencil are attached to the VIEWPORT; the artist edits their transform with the stencil chords; the paint is applied through them as ordinary strokes | immediately, in the mesh's material, as ordinary paint |
| Blender's "Project from View" | a STATIC snapshot of the view (one unwrap operation) | into the UV map, permanently |
| Blender's UV Project modifier (camera projector) | LIVE: the texture keeps following the camera as it moves | into the UV map each evaluation |

**Reading:** the references split cleanly into "bake into the surface at a
chosen moment" (Mari, Blender's Project-from-View) and "keep following the
view" (Blender's camera projector, and Painter's live stencil while the view
is still). Umber's row-272 stroke buffer gives the FIRST flavor for free: the
camera can be frozen at a chosen moment. The v1 cut is therefore the static
one — a placement captured at the placement moment, never re-projected.

### 3.3 Masking

- Painter: the stencil is "a grayscale mask for the brush stroke… attached to
  the viewport"; brush strokes through it "will only be applied where the
  stencil isn't black" — the stencil MASKS THE STROKE, and the projection tool
  shares the stencil's chords, so the same screen-space object gates the paint.
- Mari: masking is a separate family (edge mask, channel mask, AO mask, depth
  mask, fractal-noise mask) with amount/contrast/curve/invert per mask, all
  applied to the buffer at bake time.
- Blender: the shape comes from the UV layout, not from a mask; backfacing
  geometry is a known artifact case ("vertices behind the camera are not
  properly mapped").

**Reading:** the references agree that the screen-space object and its mask are
the same object. Umber already has the mask half (the stencil gate) — the
projection's alpha should ride the SAME gate, not invent a second mask system.

### 3.4 Write semantics

- Painter: the projection paints INTO the material — a fill layer plus a mask
  plus a fill effect, i.e. ordinary material editing; the history shows the
  layer ops.
- Mari: the buffer bakes DOWN onto the layer surface with a blend mode and a
  painting opacity, and the buffer can persist across bakes (`Manual`) or
  clear (`ClearOnly`, `AutoBakeAndClear`).
- Blender: the projection writes UVs (a different medium entirely).

**Reading:** Painter and Mari both write into the existing material/layer
system (no new medium); Blender writes a different medium (UVs) and is not the
model for a painting DCC.

### 3.5 The projection seam in Umber

The paint crate never sees the camera: the viewport resolves a pointer to a UV
and hands the hit over (`begin_screen_point_pressured`). The stencil's seam is
the same shape — a screen-space COVERAGE MASK handed to the paint crate
(`set_stencil_gate`). A projection therefore needs no new paint-side concept:

- the APP owns the source image, the placement transform, the viewport rect
  and the frozen camera;
- the paint side receives the SAME kind of screen-space artifact the stencil
  already receives — a coverage plane — plus the color to apply;
- `PaintProjection` gains a third variant only if the paint crate needs to
  know WHICH mode produced the hit (it does not for v1: the hit is still a UV).

### 3.6 Effort + priority, and the v1 cut line

| # | Item | Effort | Priority | v1? |
| --- | --- | --- | --- | --- |
| 1 | A projection SOURCE: an image (PNG or the rasterized `.svg`) placed over the viewport, with translate/scale/rotate + opacity | M | P0 | **yes** |
| 2 | The write: the projected image is applied to the mesh through the EXISTING paint path (as ordinary paint, so history/layers/symmetry/export are unchanged), gated by the stencil | M | P0 | **yes** |
| 3 | The freeze: the placement captures the camera at the placement moment (row 272's snapshot) — the projected pixels do NOT follow the camera afterwards | S | P0 | **yes** |
| 4 | The chords: the stencil's existing transform chords (S + drag) drive the projection's transform | S | P1 | **yes** |
| 5 | The stencil as the projection's mask (one screen-space gate, two consumers) | S | P1 | **yes** |
| 6 | Tiling / repeating the projected pattern over the viewport (Painter's projection repeats) | M | P2 | no — v2 |
| 7 | A live projector that follows the camera (Blender's camera projector) | L | P2 | no — the pwys option-B follow-up, and it contradicts row 272's frozen rule |
| 8 | Backface culling, depth culling, shape crop, filtering (Painter's planar projection family) | L | P2 | no — those are 3D-projector concepts |
| 9 | Mari's bake-behavior family (auto-bake on view change, buffer persistence, blend modes at bake time) | L | P3 | no — Umber writes into the paint layers immediately; there is no buffer to bake |
| 10 | A separate decal CHANNEL | L | P3 | no — a new medium the paint layers do not need |

## 4. The v1 cut line (one paragraph)

Umber v1 gets a **decal projector**: the artist picks an image (PNG or `.svg`),
places it over the viewport with translate/scale/rotate + opacity, and the
image is projected onto the mesh through the camera FROZEN at the placement
moment; the paint lands in the ordinary paint layers (so the history, the
layers, the symmetry and the export need no new concepts), and the existing
stencil gates it (one screen-space gate, two consumers). Nothing re-projects
afterwards, nothing tiles, nothing bakes later, and there is no decal channel.
The named follow-ups are the tiling/repeat, the live camera-following projector
(the pwys option B), and Painter's 3D-projector family.

## 5. The plan's first rows (for the next artifact, not a commitment)

1. The projection state + its transform (app-side, new module): the source
   image, the placement transform, the frozen camera, the computed coverage.
2. The write path: the projected color composited through the existing paint
   entry (the same seam the stencil uses), with the stencil gate applied.
3. The chords + the panel: the stencil's S-chords drive the projection, and a
   small panel row shows the transform (the stencil panel already exists).

Each row names a disjoint file surface; the plan states which rows supersede
nothing (this row is open in the ROADMAP) and which rows are v2.
