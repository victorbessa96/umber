# Kill report — per-channel dab density, both arms (W-C4 L2)

Executed 2026-10-24 05:1x–05:4x by local-7de7d3ed (the restarted
loop's second stretch), from
`docs/loop/briefs/2026-10-24-l2-channel-density.md` — the brief was
written from the derivation FIRST (the §5 item 1 risk restated with
both seams located live), then the kill executed from it.

## What landed

- **`texels_per_uv_for_channel(tile, channel)`** (paint_state.rs,
  beside the old form): the channel's own density through
  `size_of_channel` — the inherit rule and the baseColor-always-set
  rule arrive free through the existing resolution, never re-derived.
- **Both arms migrated in one kill** (the §5 item 1 risk dies only if
  they move together):
  - the dab conversion (`push_event_styled`'s `local_texel` — the
    conditioner operates in texel space after this line, so this IS
    the dab spacing's seam) reads the ACTIVE channel;
  - the ring's probe twin (`cursor_radius_px`) reads the same fn with
    the same active channel — one fn, both arms, no disagreement
    possible under a channel override.
- **The dead field died** (found by derivation): `texels_per_uv: f32`
  was WRITE-ONLY — its declaration, initializer, and both writes
  (at `set_active_set` and `resize_set`) existed with zero readers in
  any crate. A write-only cache is a defect, not a cache. The module
  docs' two mentions amended (the "strokes still condition at the
  SET's texel density — per-channel dab density is the named
  follow-up" sentence is now the kill's own closure note).
- The old `texels_per_uv_for` STAYS set-scale with the boundary note
  naming its callers.

## L2b — the same-class arms found by derivation (NOT this kill)

Four more arms read the set-scale form and should eventually read the
channel form; the plan scoped L2 to the dab/ring pair, so these are
the named follow-up (the map is the derivation's):

- `:1753`-class — the stroke symmetry snapshot (`start_tpv`; a
  channel-divergent stroke's symmetry transforms are off-scale).
- `:1812`-class — the clone-stroke's source offset
  (`clone_offset_texels`).
- `:4199`-class — the polygon fill's raster (reads `tile_size` for
  `rasterize_polygon_mask` — the deeper half is the raster's own dims,
  not just the tpv).
- `:4769`-class — the seam mirror radius (`seam_radius_uv_for`).

The GPU-golden route (a real divergent-densities stroke readback)
belongs to L2b too: the spacing's VISUAL pin needs the deeper arms
consistent first, else the golden pins today's half-migrated state.

## Evidence ladder

- `cargo check -p umber-paint -p umber-app` clean (7.26s — the
  migration + the field's death resolving at once).
- The unit pin green on first strike through the real adapter
  (0.85s): `texels_per_uv_for_channel_tracks_the_override` — inherit
  reads 512, baseColor reads 512, the override diverges roughness to
  256, baseColor stays 512 beside it, the old form stays 512.
- The battery's umber-paint + umber-app legs (the app's clippy covers
  the viewport seam).

## Non-goals held

- The four L2b arms untouched (named, mapped, deferred).
- No GPU golden (L2b's).
- The quick mask set-scale by design (untouched).
- No UI change of any kind.
