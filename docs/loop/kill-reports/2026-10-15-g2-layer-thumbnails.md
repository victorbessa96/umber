# Kill report — g2-layer-thumbnails (GUI plan G2)

Date: 2026-10-15. Author: dragon. Lock: `g2-layer-thumbnails` over
`crates/umber-paint/src/paint_state.rs`, `crates/umber-app/src/document.rs`,
`crates/umber-app/src/thumbnails.rs` (new), `main.rs` and this report.
Plan: `docs/specs/gui-plan-2026-10-15.md` row `g2-layer-thumbnails`.
Sources: Painter B1 ("Thumbnails: real thumbnails by default"),
photocraft #6 (content-keyed thumbnails, `tex.set` in place, pruned on
delete).

## Built

- **`PaintState::owner_revision(owner)`**: a per-owner content counter.
  - The bump sits at `clear_committed`, the one call every plane pixel
    write already makes (stroke stage, retouch region, undo/redo, fill
    publish, resize). `publish_plane` inlines `clear_committed`'s work, so
    it bumps explicitly. Reads never bump.
  - Test `owner_revision_moves_only_with_that_owners_pixels`: a layer
    publish moves only that layer, a Base fill moves only Base, and reads
    move nothing.
- **`thumbnails.rs` (new):**
  - `downsample`, a box filter to 32 px over a 4 px two-tone checker
    (alpha reads as transparency), rows flipped (the plane's row 0 is
    V = 0, the image bottom).
  - `Thumbnails`: one `TextureHandle` per layer, keyed by
    `(layer id, revision)`.
    - A rebuild reads the layer's tile-1001 baseColor plane and updates
      the existing texture IN PLACE with `tex.set`, so there is no
      re-allocation per stroke.
    - Rebuilds are capped at 2 per frame (each is a plane readback), with
      a repaint requested when the budget ran out, so a project open with
      many layers fills in over a few frames instead of stalling one.
    - `prune` drops deleted layers.
  - 4 tests: block average and row flip, the transparent checker,
    degenerate input never panics, prune.
- **Layer rows** (`document::layers_ui`): a 28 px thumbnail slot after the
  visibility eye. An empty outlined slot for a layer with no pixels yet,
  so the rows align.

## Judgment calls

1. **The baseColor of tile 1001 only.** Painter's stack thumbnail shows
   the color channel; multi-tile and per-channel thumbnails are a
   follow-up (a tile picker for the stack is G2 texture-set work).
2. **Readback, not a GPU blit.** A plane readback per CHANGED layer,
   rate-limited, is simple and correct today. A GPU downsample pass into
   the thumbnail texture would avoid the CPU round trip, but it is a
   umber-gpu change; worth it only if the readback shows in frame times
   while painting. During one stroke on one layer the rebuild runs at
   most once per frame for that layer.
3. **No Base-row thumbnail.** Base is the session's own paint target and
   is visible in the viewport. Adding it is a one-line follow-up.

4. **No rebuild mid-stroke.** A stroke writes its layer every frame. The
   first draft would have rebuilt that layer's thumbnail every frame (a
   4K-plane readback in the paint loop's hot path). Rebuilds now wait for
   `!is_stroking()`: the thumbnail updates on the frame the stroke ends,
   and the revision key guarantees it is not missed.

## Unverified

- The live look on hardware: manual.
