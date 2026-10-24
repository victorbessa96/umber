# Brief — W-C4 L2: per-channel dab density (both arms, one kill)

Written 2026-10-24 04:5x–05:2x by local-7de7d3ed (the restarted loop's
second-stretch derivation, host-live) so the kill executes from this
file. Source: the W-C4 plan's L2 section + the §5 item 1 risk. Anchors
re-derived at 04:5x from the live tree (all verified current).

## The risk (the §5 item 1, restated from the derivation)

`texels_per_uv_for(tile)` (paint_state.rs:1653) returns
`tile_size(tile).0 as f32` — the SET's density. It feeds BOTH the dab
spacing and the brush ring's probe twin, so today they agree — but
they agree on the WRONG number whenever a channel override diverges a
channel's size from its set's (a 256-channel under a 512 set): the
dabs space at 512-texel density while the channel's canvas is 256,
and the ring shows the same wrong span. One kill touches both arms so
they can never disagree — and both read the CHANNEL's true density.

## The seams (both verified at derivation time)

1. **The dab conversion** (paint_state.rs:4858):
   `let texel = local_texel([uv.x, uv.y], tile, self.texels_per_uv_for(tile));`
   — the UV→texel conversion feeding the conditioner (the spacing
   math operates entirely in texel space after this line).
2. **The ring probe** (viewport.rs:1424, `cursor_radius_px`):
   `let texels_per_uv = paint.texels_per_uv_for(tile_of_uv(uv));`
   → `brush_cursor::texels_spanned` → `screen_radius`.

## The kill's shape

1. **NEW** `texels_per_uv_for_channel(&self, tile: u16, channel: &str)
   -> f32` in paint_state.rs beside the old form:
   `self.size_of_channel(&self.set_of_tile(tile).to_string(), channel).0
   as f32` — the channel-aware density. The inherit rule
   (`size_of_channel`'s override-or-set) and the baseColor-always-set
   rule come along FREE through the existing resolution — do NOT
   re-derive them.
2. Seam 1 migrates: `self.texels_per_uv_for_channel(tile,
   &self.active_channel)` — the stroke paints the active channel (the
   state exists: `active_channel: String`, default baseColor,
   `active_channel()` pub at :1562).
3. Seam 2 migrates: `paint.texels_per_uv_for_channel(tile_of_uv(uv),
   paint.active_channel())` — the ring shows the brush on the channel
   being painted.
4. **The dead field dies** (found by derivation):
   `texels_per_uv: f32` (the field at :1182) is WRITE-ONLY — the only
   two references are its writes (:1552 at `set_active_set`, :4444 at
   `resize_set`); no reader exists in any crate. Delete the field, its
   two writes, and its doc comment (the :1180-1181 `/// UV→texel scale
   ... of the ACTIVE set` lines) — a write-only cache is a defect, not
   a cache. The module docs' mentions (:160, :191-193) get a one-line
   amendment pointing at the channel form.
5. **The old `texels_per_uv_for` STAYS** — the quick mask stays
   set-scale BY DESIGN (the plan's own note; paint_state.rs:160's doc
   says the mask reads it), and four same-class arms stay set-scale
   THIS kill (below). Its doc gains the boundary note: which callers
   belong on the channel form and why the four below do not YET.

## Same-class arms found by derivation — L2b's map (NOT this kill)

All four read the set-scale form today and should eventually read the
channel form; the plan scoped L2 to the two arms above, so these
land as the report's named follow-up:

- **:1753** the stroke symmetry snapshot (`start_tpv` — the texel
  transforms; a channel-divergent stroke's symmetry is off-scale).
- **:1812** the clone-stroke's source offset (`clone_offset_texels`).
- **:4199** the polygon fill's raster (it reads `tile_size(tile)` for
  `rasterize_polygon_mask` — a channel-divergent fill rasterizes at
  the SET's size; the deeper half of this arm is the raster's own
  dims, not just the tpv).
- **:4769** the seam mirror radius (`seam_radius_uv_for` — the brush
  footprint gate; the mirror should gate at the painted channel's
  density).

## The tests

- **Unit pins** (paint_state's test module, beside
  `resize_channel_touches_only_its_channel` at :10753): a set at 512
  with a channel override at 256 → `texels_per_uv_for_channel(tile,
  channel)` == 256.0; baseColor reads the set's 512.0; an unoverridden
  channel inherits the set's 512.0; a foreign set's tile reads its
  own set's resolution (the multi-set case).
- **The agreement pin** (the kill's point): under the override, the
  dab seam's conversion and the ring's probe read the SAME value —
  pinned by construction (both call the one new fn with the active
  channel); the unit pins above are its evidence. The GPU-golden
  route (a real divergent-densities stroke readback) stays with L2b's
  arms — the spacing's VISUAL pin needs the deeper arms consistent
  first, else the golden pins today's half-migrated state.

## Verification ladder

fmt (the formatter binary); the battery's umber-paint leg (the unit
pins compile + pass there) + umber-app's clippy (the viewport seam);
cross-review per the standing recipe; ONE push with the kill report
marking the plan's L2 row + this file's L2b map as the follow-up.

## Non-goals

- The four L2b arms stay set-scale (named, not touched).
- No GPU golden (L2b's, after the arms land).
- No quick-mask change (set-scale by design).
- No UI change of any kind.
