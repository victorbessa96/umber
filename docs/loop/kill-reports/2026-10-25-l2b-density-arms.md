# Kill report — L2b: the four same-class density arms migrated

Executed 2026-10-25 01:5x by local-7de7d3ed, on the reopened
"continue" stretch (the third kill of the second wind).

## What landed

The L2 kill (306a502) introduced
`texels_per_uv_for_channel(tile, channel)` — the per-CHANNEL tile
density (channels of one set can differ in size) — and migrated
the dab conversion's own arm. The L2b row carried the four
remaining same-class arms, each still on the set-level scalar
`texels_per_uv_for`:

1. **The symmetry snapshot** (paint_state.rs, the
   stroke-open site): the stroke-start tile's density for the
   symmetry transforms. The stroke paints the ACTIVE channel →
   `texels_per_uv_for_channel(route_tile, &active_channel)`.
2. **The clone offset** (the Clone arm of the same stroke-open
   fn): the source-offset's texel conversion. Same stroke, same
   active channel.
3. **The fill raster** (the polygon fill's rasterize site): the
   mask runs' density. The fill strokes the active channel.
4. **The seam mirror radius** (`seam_radius_uv_for`): the brush
   footprint's mirror gate. The brush strokes the active channel.

All four serve the active channel's stroke/fill — each migrated
on the L2 kill's own proven pattern
(`texels_per_uv_for_channel(tile, &self.active_channel)`).

## What stays

- `texels_per_uv_for` itself (the set-level scalar) remains: its
  definition + the two test assertions that pin the SET-level
  truth (1001 → 512, 1002 → 256). It documents the class's other
  half and the tests keep it honest.
- The behavior differs from the scalar only when a channel's
  tile size ≠ its set's density — the exotic config; the named
  follow-up pin (a differing-channel fill case) rides the L2
  report's own class.

## Verification

`cargo check -p umber-paint` zero errors; the battery
(umber-paint + umber-app) gates the commit.
