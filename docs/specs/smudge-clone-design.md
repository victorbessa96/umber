# Smudge + Clone Brushes — Design (§2's P1 row)

Written 2026-10-10 16:35 against `860b2e4`. The brush engine's
two named-missing tools: smudge (sample+blur blend) and clone
(source offset + follow-stroke).

## What exists

- The brush engine (`umber-brush`): the dab shape/spacing/
  conditioning pipeline, the stamped/tiling stamp modes (the
  seam-aware stamping + high-to-low transfer arcs), the flow/
  opacity accumulation.
- The paint pipeline (`umber-app/paint_state.rs`): the stroke
  begin/extend/end path, the dab staging, the composite step,
  the tile routing.
- The blend vocabulary: the layer blend modes (normal, multiply,
  overlay...) — the same enum a smudge op extends.

## Smudge — the design

The classic Mari/Substance shape: each dab SAMPLES the current
target at its footprint, BLURS/lerps that sample toward the
stroke direction, and WRITES it back with the brush's opacity
flow.

- `SmudgeParams { strength: f32, smear: f32 }` — strength = the
  sample/write mix (0 = pure original, 1 = fully replaced by the
  smeared sample); smear = the blur kernel radius factor.
- The op: at each dab, read the target's texels under the
  footprint (the readback path the high-to-low transfer already
  built), box-blur them by smear*radius, write back with
  strength as the alpha — all on the CPU composite step (the
  same place the existing stamp's blend happens; NO new GPU
  pass — the readback + write is the existing round trip).
- The follow: the stroke's motion vector tilts the sample window
  (the smear shifts toward the incoming direction — the
  "dragging" feel).

## Clone — the design

- `CloneParams { source_offset: (f32, f32), follow: bool }` —
  the offset in UV space; follow = the offset drifts with the
  stroke (the source point tracks the cursor minus the offset —
  the classic follow-stroke clone).
- The op: each dab reads the source point (cursor + offset in
  follow mode, or the fixed source anchor), samples the SOURCE
  LAYER/texture-set, and writes with the brush's normal flow.
- The source set by alt-click (the UI affordance the panel
  gains); v1 clones within the same texture set + layer (the
  cross-set clone named as the follow-up).

## Tests

1. Smudge: a hard edge stroked across with strength=1 blurs the
   boundary texels (the before/after delta asserted at the
   edge's columns); strength=0 is a no-op (the identity).
2. Smudge smear: a wider smear radius spreads further (the
   affected-texel count monotonic in the radius).
3. Clone fixed: two identical dabs appear at cursor and
   cursor+offset; moving the cursor with follow=false keeps
   sampling the SAME source.
4. Clone follow: the source tracks (the sampled origin moves
   with the stroke — assert the source coordinates' trajectory).
5. Both: the undo stack records them (one undo per stroke, the
   existing contract).

## Build

One claw slice, code-only: the two ops in the brush/composite
path (the readback plumbing exists), the params + the panel
rows, the five tests. No new deps.

## As built (2026-10-10) — where the code departs from the above

- **No CPU composite step existed** — the stamp blend is the GPU
  compute pass (`PaintCompositor::splat_dabs`). The ops are new CPU
  code (`umber-brush/src/retouch.rs`), mirroring the shader's
  footprint + falloff texel for texel.
- **The readback** is `PaintTarget::read_back_rgba8`
  (`umber-gpu/src/paint.rs`; the transfer bake reads its own target
  the same way). It is full-target + blocking, so it runs ONCE per
  tile per stroke (the stroke's first touch) into a CPU mirror.
- **No write-back existed**: the target gained `COPY_DST` and
  `PaintTarget::write_region` / `PaintThreadCommand::WriteRegion`
  (encoder-recorded, so it orders with the splat dispatches).
  Per-event uploads are dirty-rect sized.
- **The hook**: `PaintState::push_event_inner`, right after
  `DabAdapter::stamps_to_dabs` — retouch tools branch to the mirror
  there; the paint path is untouched.
- **No paint undo existed** (the document journal holds layer
  commands only). Retouch strokes get their own per-stroke history
  in `PaintState` (`umber-app/src/retouch.rs`'s
  `PaintStrokeCommand`, before/after dirty-rect snapshots), with
  undo/redo-stroke buttons in the History panel. Snapshot restores
  would erase newer paint, so a paint-tool stroke (or a target
  resize) clears that history: retouch undo is valid only until the
  next paint stroke. Unifying it with the layer journal (and giving
  paint-tool strokes undo) is the follow-up.
- **Readback citation**: `PaintTarget::read_back_rgba8`
  (`umber-gpu/src/paint.rs`) is the fn the transfer bake reaches
  through `transfer.rs` → the AO target's `read_back_rgba8`
  (`umber-bake/src/ao.rs`) wrapper.
- **Clone source** is set by Alt + press in the 2D UV view (clone
  tool only; the 3D viewport's Alt is not wired). It resolves to
  `source_offset = source - stroke start`: on every stroke in
  fixed mode (each stroke samples the alt-clicked point), on the
  first stroke after the click in follow mode (that offset is then
  kept). Clone stays in the stroke's start tile; an off-tile
  source texel is skipped. Retouch strokes don't seam-mirror in
  v1.
- Tool + params live on the brush panel, not the preset (the
  `.umberbrush` format is unchanged).
