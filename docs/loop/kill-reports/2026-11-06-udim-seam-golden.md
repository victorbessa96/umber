# Kill report — UDIM seam-clip golden (row 222(c) tail, queue row 251)

- Date: 2026-11-06 ~22:0x -03. Session: local-1625-rz6f2a (Razul). Kill-lock:
  `razul-udim-seam-golden` (crates/umber-paint/src/paint_state.rs, STATE.md,
  this report).
- What was open: row 222(c) "UDIM tile addressing + seam-clip goldens" — the
  addressing half landed in `6fc6e9e` (udim_tiles.rs 9/9); the seam-clip
  goldens (paint-side, byte-exact pins that a dab crossing a UDIM seam clips
  at its own tile and mirrors into the neighbor tile) were still open.

## The change (tests only, no src changes — the behavior was already right)

- New fixture `udim_seam_quad()`: one 3D quad whose two triangles share the
  diagonal 3D edge with DIFFERENT UVs (tri 0 island in tile 1001, tri 1
  island at uv ~1.4..2.0 = tile 1002) — exactly one `build_seam_graph` seam.
  (A coincident-UV duplicated edge is "continuous", not a seam — learned
  while writing the fixture.)
- New pin `udim_seam_dab_clips_at_the_tile_boundary_and_mirrors_byte_exact`:
  a stroke near the seam diagonal on the 1001 side stages batches ONLY in
  1001+1002; every changed texel in one tile maps through the engine's own
  `mirror_uv` to a changed texel in the other with bytes equal to within 1;
  the mirror paints 1002 (clip + mirror, not bleed). Control: the same
  stroke on the seam-free `two_tile_strip` leaves 1002 completely untouched.
- Iterations while landing: fresh-state strokes paint `PlaneOwner::Base`
  (not Layer(0)); unwritten planes read as absent (before = zeros); the
  conditioner needs several events before dabs stage; and the seam case's
  primary footprint is NOT byte-identical to the seam-free control (mirrored
  events share the stroke conditioner), so that pin is order-of-magnitude,
  while the mirror symmetry itself is pinned byte-exact via `mirror_uv`.

## Verification

- Focused: `cargo test -p umber-paint --lib udim_seam` 1/1 green.
- Battery (flock, nice 19, -j3, /tmp/umber-udim-battery.log): clippy
  -p umber-paint --all-targets -D warnings; cargo test -p umber-paint
  --all-targets — all suites ok, 0 failed, pin included.

## Records note

Queue row 251 + COORDINATION line DEFERRED: docs/loop/queue.md and
.dragon/COORDINATION-next-wave.md remain held by sibling lock
`razul-bundled-assets`.

## Not done / next

- Row 242 History-panel split wiring and installers gap #2 remain
  sibling-locked (document.rs/main.rs/bundled.rs). v0.1.0 tag partner-gated.
- merge-below-top: OPEN, needs a redo-top accessor in umber_core::undo
  (retouch.rs:658's note).
