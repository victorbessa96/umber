# Brief — W-C4 L2: per-channel dab density (ONE kill, both seams)

Written 2026-10-22 12:3x by local-7de7d3ed (charter cycle, host-free).
Source: the W-C4 plan's L2 lane + the module doc's own named follow-up
(paint_state.rs:157-163) + §5 item 1. Anchors from the WORKTREE
2026-10-22 (carries the parked b10 bytes in viewport.rs + P1's in
paint_state.rs — the close commits exactly these bytes; re-derive at
dispatch).

## Timing

POST-CLOSE, after L1 (the plan's dispatch order: tangent wiring → L1 →
THIS KILL → L3 → L5 → L6). Both seams sit in files the parked fleet
owns until its rows commit: viewport.rs (b10) + paint_state.rs (P1).

## The current model (the documented deferral this kill closes)

- The module doc states it verbatim (paint_state.rs:157-163): "Strokes
  still condition at the SET's texel density ([texels_per_uv_for])
  whatever channel they land in — per-channel dab density is the named
  follow-up, so painting a smaller channel today lands set-scale dabs
  on fewer texels." The quick mask stays per-SET (gates baseColor's
  thread only); layer-mask planes stay set-sized.
- `texels_per_uv_for(tile)` (paint_state.rs:1653) = `tile_size(tile).0`
  = the SET's width — the per-tile form of the ACTIVE-set field
  `texels_per_uv` (struct field at :1180-1183; set at construction
  :1464 and in `set_active_set` :1552, BETWEEN strokes only — a
  mid-stroke switch would move the density under the live conditioner;
  the channel form must inherit the same discipline).
- The channel override machinery already exists: `channel_sizes`
  (BTreeMap keyed `(set, channel)`, :1149) + `tile_channel_size(tile,
  channel)` (:1632); `BASE_COLOR_CHANNEL` short-circuits to the set
  size (:157-159 — baseColor IS the set's canvas).

## The two arms (the kill touches both, never one)

1. **The dab routing** — the per-tile sites the doc names: "routing,
   seam radius, clone offset, fill replay". Today every site resolves
   through `texels_per_uv_for` → the SET density, whatever channel the
   dabs land in.
2. **The ring's probe twin** — viewport.rs:1423-1427:
   `paint.texels_per_uv_for(tile_of_uv(uv))` →
   `brush_cursor::texels_spanned(uv, uv_b, texels_per_uv)` →
   `combine_spans` → `screen_radius(paint.brush_radius_texels(), t)`.
   The §5 item 1 risk: the ring advertises set-scale spacing while the
   dabs land (after this kill) at channel scale — the disagreement the
   one-kill rule exists to make impossible.

## The change

Resolve the density from the TARGET CHANNEL's actual size —
`tile_channel_size(tile, channel).0 as f32` — at every site whose
output lands on a channel plane; the ring uses the SAME channel's
density. Adjudicate each `texels_per_uv_for` call site at dispatch
against this table (re-grep the call sites first — the list below is
the doc's, verified only at the named lines):

| site | reads | ruling (expected) |
|---|---|---|
| stroke routing (the dab plan) | the stroke's target channel | CHANNEL density (the point of the kill) |
| the ring probe (viewport.rs:1423) | the brush's target channel (the one the dabs will land in) | CHANNEL density — the twin |
| seam mirror radius (paint_state.rs:581) | the seam mirror operates on the set-surface planes | SET density (stays) |
| clone offset, fill replay | re-derive at dispatch | channel-targeted → CHANNEL; set-surface → SET |

The field `texels_per_uv` itself is NOT redefined (it stays the
active-set density — the between-strokes contract and its conditioner
note stay true); the channel resolution happens at the routing sites,
computed read-only per stroke/dab from `tile_channel_size`. No mid-
stroke density mutation of any persistent field.

## The test plan

1. A 4K set, one channel overridden to 2K: stroke the channel → the
   dab conditioning uses 2048 (assert the routing's computed density
   or its observable — dabs-per-UV versus the set-density baseline;
   re-derive the cleanest assert from the routing seam at dispatch).
2. The ring on the same fixture: its density input is the channel's
   2048 (unit-level on the probe path or its extracted seam) — the
   never-disagree pin.
3. baseColor on the same set: unchanged, 4096 (the short-circuit).
4. A channel WITHOUT an override: density == set size — byte-identical
   behavior to today (the no-regression pin).
5. The seam radius: unchanged (set-scale).

## Non-goals

- NO quick-mask change (per-SET by design, :163-164).
- NO layer-mask plane resizing (set-sized by design).
- NO 8K-per-channel VRAM guidance (the doc's separate deferral).
- NO mid-stroke channel-density mutation of persistent fields.
- NO change to the `texels_per_uv` field's meaning.

## Verification ladder

rustfmt --check per touched file; the umber-paint + umber-app battery
legs when sanctioned; cross-review by the other claw; the module doc's
named-follow-up paragraph (:157-163) closes in the same commit; the
requirements row + CHANGELOG in the same commit; no CI claims until
the wave push (M2).
