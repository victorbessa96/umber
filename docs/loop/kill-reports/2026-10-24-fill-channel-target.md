# Kill report — the fill's channel target (the material picker's prerequisite, R-A10's first half)

Executed 2026-10-24 20:1x–20:4x by local-7de7d3ed, opened by
Bessa's "continue" (the queue's first hunt: the material picker's
channel-map derivation, the open question named in the roadmap
row).

## The derivation's answer

The question the row named: FillParams carries ONE source — does
`evaluate_fill_tile` write the same RGBA into every channel? THE
LIVE READ: it writes into ONE channel only, `BASE_COLOR_CHANNEL`,
hardcoded at `refresh_fill_layer`'s three sites (tile_channel_size
+ publish_plane + the import). There is NO per-channel mapping —
the fill layer model is one-source-one-channel. The material
picker ("pick all channels at cursor → fill layer") therefore has
a PREREQUISITE: the fill's publish target must become a parameter
before any picker can stage a picked value into height or
roughness. THIS KILL is that prerequisite.

## What landed

### umber-mesh (fill.rs)

- **`FillParams.channel: String`** with `#[serde(default =
  "default_fill_channel")]` — the additive-carry rule: every
  existing project's fills load targeting `baseColor`, exactly
  their pre-kill behavior. The doc names the degradation rule: a
  channel the set lacks stages nothing (the zero-size guard), so
  a renamed-channel project degrades to a no-op fill, never a
  crash.
- **`default_fill_channel()`** — the default ("baseColor").

### umber-app (fill_layer.rs + main.rs)

- **`FillLayerState::channel()`** — the getter beside
  `set_params`, read up front so the staging loop borrows nothing
  while evaluating.
- **`refresh_fill_layer`** — the three hardcoded sites now read
  the params' channel: `tile_channel_size(tile, &channel)` +
  `publish_plane(..., &channel, ...)`. THE RASTER IS
  CHANNEL-BLIND: `evaluate_fill_tile` unchanged (the same RGBA
  whatever the target) — only the publish target moves.
- **The panel row** — the fill section's per-layer rows gain the
  channel combo (the session set's channel names from
  `mesh_import.session_set`), the edit running through the same
  `LayerCommand::set_fill` journal path as every other fill edit.

## Tests (both green)

- **`pre_channel_fill_json_loads_base_color`** (umber-mesh): the
  file is REAL — a serialized params with the channel key
  STRIPPED (never a hand-guessed JSON shape; the first cut's
  `"type":"solid"` guess died on the external tag, the rewrite
  derives the file from serde itself) — loads `baseColor`; the
  explicit "height" target round-trips; the plain params equal
  the targeted one modulo the channel.
- **`set_fill_round_trips_through_undo_and_json`** (umber-core,
  existing test extended by the field): the channel survives the
  journal's undo/redo/JSON.

## The derivation's own misses (named — the script lesson)

1. **The brace-counter script mangled four files**: nested braces
   (`FillSource::Solid { color }`) threw the depth tracking; the
   channel lines landed at wrong closes (a fn signature, two fn
   bodies, a struct-field decl) and after `..` spreads (illegal).
   Recovery: three files checked out to pristine and the good
   edits reapplied by hand; the fourth repaired in place.
2. **The second scripted pass overmatched**: `mask:` lines in
   OTHER structs (Layer literals, the Row literal) took the
   field; removed, but the removal pass also stripped the two
   TRUE FillParams literals (net-zero) — caught by the
   all-targets round (the plain check had passed because the
   test literals sit in cfg(test)).
3. **The mixed-crate grep's head cut the core's E0063** — the
   direct per-crate run surfaced it.
4. **serde_json was not a dev-dep of umber-mesh** — added.
5. **The E0382 partial move** in the test's last assert — the
   clone fix.

THE LESSON (for the skill): scripted multi-file field insertions
into struct literals with nested-brace enums are NOT a
regex/brace-counter job — the site count is small enough for
per-site patches, and the compile round per CRATE (never a
mixed-crate grep with a head limit) is the arbiter.

## The picker's second half (now unblocked)

The row's open question is answered by this kill: the v1 picker =
the cursor's composite read per channel + N fill layers (one per
picked channel, each with its channel + Solid) — or the honest
trim: the picker fills the ACTIVE channel (the one edit the user
is looking at), the all-channels mode as the follow-up. The row
updates when the picker kill runs.
