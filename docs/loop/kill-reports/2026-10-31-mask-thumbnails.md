# Kill report: 2026-10-31 mask thumbnails in the layer stack IMPLEMENTED

- **Kill**: the g2-layer-thumbnails open follow-up "mask thumbnails" —
  the layer row's mask slot now shows the real mask swatch.
- **Lock**: `wave-mask-thumbs` over the 5 touched files.

## What landed

- **`umber-paint/paint_state.rs`** — mask writes bump the layer's
  `owner_revision` (`layer_mask_event`, `publish_layer_mask`), so the
  mask thumbnail cache has a content key. Verified `owner_revision`'s
  only consumer is the thumbnail cache. 1 new test (publish bumps).
- **`umber-app/thumbnails.rs`** — `Thumbnails` gains `mask_cache` +
  `mask_get(ctx, paint, layer, budget)`: reads `layer_mask(layer,
  THUMB_TILE)`, expands 8-bit gray to RGBA, box-filters through the same
  `downsample` (same row flip as the layer thumb), updates the texture
  in place. Same rules as layer thumbs: rebuild budget, never
  mid-stroke. `prune` covers both caches.
- **`document.rs`** — `layers_ui` takes `&LayerThumbs { layer, mask }`
  (was a bare map); the mask slot renders `thumbs.mask.get(id)` through
  the existing `mask_thumb` widget (Alt solo / Shift toggle / click
  paint unchanged). `#[allow(too_many_arguments)]` with the reason
  named (the panel's pinned shape; bundling would churn every kittest
  call site).
- **`main.rs`** — the per-frame thumb pass builds both maps (mask
  swatches only for layers whose `mask` is `Some`).
- **`tests/panel_kittest.rs`** — call sites updated to `LayerThumbs`.

## Verification

- Gate-1 battery GREEN: layers + fmt + clippy -D warnings (app+paint)
  + gpu leg + umber-app 898 tests + umber-paint 184 tests, 0 failed.
  CI on the pushed tip = the wave-end gate (prior wave's CI
  41222532262 SUCCESS on 1d001b6).

## Not in scope

- Mask thumbnails for tiles other than tile 1001 (the layer thumb shows
  the same single tile — consistent).
- Per-entry History-window thumbnails (the other open g2 follow-up).
