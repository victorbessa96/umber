# Kill report — cavity wiring (the cavity mask paints)

Date: 2026-10-15. Author: dragon. Lock: `cavity-wiring` over
`crates/umber-paint/src/paint_state.rs`, `crates/umber-app/src/{main,
brush_panel}.rs`, `crates/umber-brush/src/{preset, preset_library,
cavity}.rs`, `assets/i18n/*.ftl` and this report. `bakes_panel.rs` was
NOT taken: another session holds it under `g2-bakes-margin-ui` and was
writing it during this kill. The feed below reads only its public
`last_records()`. Plan: `docs/specs/gui-plan-2026-10-15.md` row
`g2-brush-method-cavity-ui`, cavity half (ROADMAP 6a
`wave-cavity-wiring`). Engine: `umber_brush::cavity` (405cad6).

## Built

- **One composed gate.** `PaintState::gate_plane(tile)` is the quick
  mask's coverage where the quick mask gates, else all-pass `1.0`, TIMES
  the cavity coverage where the cavity mask gates that tile.
  Multiplication, so both masks apply. Both gate consumers read it:
  - the GPU path: `publish_mask` uploads `gate_bytes(tile, rect)`, the
    8-bit form;
  - the retouch path (smudge / clone / eraser on the CPU mirror) builds a
    `MaskView` over the same composed plane.
  `stroke_mask_gate` is now `quick_mask.gates() || cavity_gates()`.
- **Cavity state:** `set_cavity_mask(Option<CavityMask>)` refuses
  mid-stroke and refuses invalid params.
  - `set_curvature_map(tile, w, h, rgba)` refuses a size that does not
    match the tile's paint size: the coverage must line up texel for
    texel.
  - `has_curvature_map(tile)`.
  - The coverage is recomputed when the params or the map change, and
    that clears `mask_synced` so the next gated dab re-uploads.
  - With no curvature map for a tile, the mask is INERT there. It does
    not block all paint; the panel says why.
- **sRGB decode.** The Bakes panel writes curvature PNGs through
  `Transfer::Srgb`, so flat curvature (128) lands on disk as 188 and
  would read as concave everywhere. `curvature_png_to_linear` inverts the
  writer's exact curve. It is checked in Python first, then pinned by a
  test over all 256 bytes: max error 1, and exact at the encoding's
  anchors 0, 128 and 255.
- **The feed (main.rs):** `CurvatureFeed` turns the Bakes panel's
  `last_records()` into "newly landed Curvature maps", once per
  `(path, mtime)`, so a re-bake to the same path loads again. Each map is
  read, decoded and handed to the session for its tile; a failure logs
  and leaves the mask inert.
- **Panel:** a Cavity Mask section (a ParamGroup after the quick mask):
  - an enable checkbox, Cavities / Ridges, Threshold, Softness and
    Invert;
  - a muted "Bake the Curvature map to use this" hint when the active
    tile has no map.
  It edits the session directly, like the quick mask: the mask depends
  on the mesh's bake, not on the brush preset. Eight i18n keys, en-US and
  pt-BR.
- **A latent bug fixed on the way:** `set_mask_mode` /
  `toggle_mask_mode` / `set_mask_active` now clear `mask_synced`. The
  uploaded gate bytes depend on the mode, so a mode change must force a
  re-upload. Before this kill the GPU always received the quick mask's
  raw coverage whatever the mode, so the bug could not show; the composed
  gate makes the bytes mode-dependent, and this keeps them correct.
- **Tests:**
  - `cavity_gate_composes_with_the_quick_mask_and_is_inert_without_a_map`:
    off is all-pass; no map is inert; a wrong size is refused; a cavity
    texel passes and a flat one blocks; an armed empty quick mask blocks
    both (the multiplication); clearing lifts the gate.
  - `mask_mode_changes_invalidate_the_synced_gpu_gate`.
  - `curvature_png_decode_inverts_the_bake_writers_srgb_transfer`.

## Judgment calls

1. **Session state, not preset state.** Blender keeps the cavity mask in
   the texture-paint tool settings, not on the brush. It is a property of
   this mesh's curvature, and a preset carried to another mesh would
   gate against a map that is not there.
2. **The feed reads the panel's public record list** instead of adding a
   hook inside `bakes_panel.rs`, because that file is another session's
   live lock. Polling a short `Vec` once per frame costs nothing. The
   `(path, mtime)` key makes it exactly-once without the panel's
   cooperation.
3. **Curvature from a bake only.** A curvature map imported through the
   mesh-map import path is not fed yet. That path's records live in
   `mesh_map_import.rs`; same pattern, follow-up row.

## Unverified

- The live paint result on a real mesh with a real curvature bake
  (manual on hardware).
- An imported (not baked) curvature map: not fed (judgment call 3).
