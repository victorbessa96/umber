# Kill report — real mask pixels (wave-masks)

Date: 2026-10-12. Claw: opencode (died at the report step after ~28min;
the dragon verified the tree and wrote this report from the diff — the
claw's own final text was 'prune calls in save/open and the report-literal
fixes', and the prune calls ARE present at main.rs:1352-1596).

## What landed (7 files, +1467/-16)

1. **LayerMask grows pixels** (`crates/umber-core/src/layers.rs`):
   LayerMask gains the coverage model ([`mask_coverage`] per texel while
   `LayerMask::gates`); `attach_mask` creates metadata only — the planes
   live in the paint session (the same split as channels: the document is
   the model, the session is the pixels).
2. **The plane pool** (`crates/umber-app/src/paint_state.rs`, 54 symbols):
   per-layer mask planes in the session — white-by-default (ungated
   everywhere), lazily materialized, per-set sized, pruned with their
   layer; `set_layer_mask_paint` + the stroke routing (a stroke on a
   layer's mask-edit mode paints the MASK plane, not the color target);
   save/open stage and reload them through the same sidecar nest as
   channels (masks/<layer>/<tile>.png; the prune step drops detached
   layers' masks — main.rs:1352-1355).
3. **The compositor gate** (`crates/umber-gpu/src/compositor.rs`, +528):
   the per-layer alpha multiply by the mask plane per texel, the WGSL arm
   mirroring the CPU oracle, the gate OFF = byte-identical to today
   (pinned); black/white masks, partial coverage, the enabled flag.
4. **gate0_journey.rs** extended: the journey test now exercises a masked
   layer end to end.

## Tests written by the claw (run at wave end per the binding rule)

`layer_mask_planes_are_white_lazy_sized_and_pruned` + the panel/compositor
suites' additions (the compositor's mask goldens; the round-trip).

## Judgment calls (from the diff)

- White-by-default planes: an attached-but-unpainted mask gates NOTHING
  (matches Substance: attach ≠ enable-paint).
- Masks nest under `masks/<layer-id>/` — layer-id-keyed, not name-keyed
  (renames don't orphan planes).
- The gate multiplies ALPHA pre-blend (the same point QuickMask gates).

## Deferred

- Mask STACKS with effects (the requirements row's long tail) — this kill
  is one mask per layer.
- Bitmap masks (grayscale paint only).

## Doc deltas

<!-- requirements row: Masks: grayscale paint mask, bitmap mask, black/white -->
| Masks: grayscale paint mask, bitmap mask, black/white; mask stacks with effects | P0 | 2 | ✓ built (wave-masks, 2026-10-12): per-layer grayscale planes — white-by-default, lazily materialized, per-set sized, pruned with their layer; the compositor multiplies layer alpha by mask coverage (WGSL + CPU oracle, gate-off byte-identical pinned); mask-edit stroke routing; save/open round-trip. v1 boundary: ONE mask per layer (stacks with effects = follow-up); bitmap masks = follow-up. |
