# Per-Channel Tile Pool — Design

Date: 2026-10-12
Status: DRAFT (pre-review; the implementation brief derives from this doc)
Blocks: heightnormal's app wiring (worklist #12's named follow-up — "NOT
REACHABLE from the app: PaintState has no per-channel paint target to bind
from"); unblocks Solo-channel painting, per-channel export feeds, and the
ID-mask's channel-scoped variants.

## The problem, precisely

`PaintState` owns ONE `PaintThread` per UDIM tile
(`crates/umber-app/src/paint_state.rs:408 targets: BTreeMap<u16,
PaintThread>`). Every stroke lands in that single target regardless of which
channel the artist conceptually paints. The channel MODEL exists upstream —
`umber_core::Channel { name, kind: Color|Data }`, `TextureSet { channels }`
(`crates/umber-core/src/lib.rs:42-70`) — but the paint session has no
channel dimension. Consequences already landed as named blockers:

- The height→normal kill (#12) built the full GPU side (binding 12,
  `set_height_texture`, the Mikkelsen perturbation, goldens) and stopped at
  the app wiring: no height-channel target exists to bind
  (`viewport.rs::bind_paint_target`'s doc, the requirements §5 row's
  NOT-REACHABLE mark).
- Solo-channel data views render mid-gray placeholders for the same reason.
- Export reads base color only; per-channel map export (roughness/metallic/
  height/normal as real pixel sources) has no source of truth.

## Non-goals (v1)

- Per-channel RESOLUTION (the audit already deferred it; one size per SET
  stays the contract — `resize_set` semantics unchanged).
- Per-channel mask planes (QuickMask stays per-SET, gating all channels of
  the set equally; channel-scoped masks are a later row).
- Blending across channels (the compositor's channel story is a separate
  wave; this design only owns storage + routing + binding).

## The storage shape

`targets` gains a channel dimension WITHOUT breaking the tile-major API
every caller already uses:

```rust
/// One tile's paint targets: baseColor always present (the v1-era
/// surface; every existing caller), plus one PaintThread per ADDITIONAL
/// channel that has ever been routed to on this tile (lazy — see
/// "Materialization" below).
struct TileTargets {
    base_color: PaintThread,
    channels: BTreeMap<String, PaintThread>,
}

targets: BTreeMap<u16, TileTargets>,
```

Why nested over a flat `(u16, String)` key: the tile-major iteration stays
one hop (`targets.get(&tile)` → save/load/undo/compositor walk tiles, not
tile×channel pairs), and `base_color` keeps its field-level identity —
every existing `targets.get_mut(&tile)` call site reads as
`.base_color` with zero semantic change.

## Channel identity

Channel names are `String`s from the document's `TextureSet.channels` (the
document is the source of truth; the session mirrors it). The reserved name
`"baseColor"` (the umber-core default set's first channel, `new_default`)
maps to the `base_color` field — routing to it never allocates a
`channels` entry. `ChannelKind` (Color/Data) is carried for the LUT and
display decisions at BIND time, not stored per-target (the thread is
format-agnostic RGBA8 linear; kind is a view concern).

## Materialization (the memory contract)

One PaintThread per (tile, channel) multiplies VRAM by channel count — a
5-channel set at 4K would quintuple a 16-tile session's memory. The lazy
rule extends one level: a channel's PaintThread appears on FIRST ROUTE
(the artist strokes the channel, a bake writes it, or the app explicitly
pre-warms it via `ensure_channel(tile, channel)`); baseColor materializes
at tile creation exactly as today. A channel with no route costs nothing.
`ensure_channel` creates at the SET's size (the same `tile_size` math).

## Routing

`route_tile(uv)` is unchanged. Above it, one new dimension:

- `active_channel: String` (default `"baseColor"`) — the session-level
  channel the Texture Sets panel's new channel combo edits
  (`set_active_channel`, mirroring `set_active_set`).
- Every stroke/event routes to `route_tile(uv)` then the active channel's
  target (base_color field or the channels map — `target_of(tile)` private
  helper resolves it).
- Channel-aware tools that are NOT the active channel (the height arm of
  the lit viewport) read via `read_channel_tile(tile, channel)` — the
  eyedropper/ID-mask pattern (explicit tile + explicit channel, no ambient
  active state).

## Save / load

The manifest grows a channel column; the directory shape nests:

```
paint/
  baseColor/          <- the v2 layout, byte-identical files
    tile-1001.png
  height/
    tile-1001.png
  manifest.txt        <- one row per (tile, channel): tile, channel, ref, size
```

Migration: a v2 manifest (no channel column) loads every row as baseColor —
the old reader accepts the new file (new columns are additive; the codec's
unknown-column policy already tolerates this — verify at implementation).
A channel directory with no manifest row is orphaned temp (the atomic-save
recovery sweep already handles unknown files in paint/).

## The binding seam (unblocking #12)

`viewport.rs::bind_paint_target` extends: when the mesh pass binds base
color it ALSO asks the session for the ACTIVE SET's height-channel target
(if materialized) and binds it via the existing
`MeshBuffers::set_height_texture` — the exact call #12 built and could not
reach. The lit viewport then perturbs live while the artist paints height.
Solo-data views bind their channel's target through the same
`read_channel_tile`/`ensure_channel` path.

## Undo / history

The stroke-undo journal snapshots (tile, rect) byte deltas. With channels,
the snapshot key extends to (tile, channel) — the journal's byte accounting
is unchanged (a merged rect region budget; the #5 merge logic operates per
target). The resize-set path clears channel targets alongside base color
(the same `Resize` command publishes per thread).

## Compositor

The per-layer compositor reads base color only in v1 of this change. The
follow-up row (channel-aware layer compositing) consumes this design's
storage without changing it — noted here so the storage shape is not
designed twice.

## Test gates

1. Two-channel fixture (baseColor + height): stroke to height lands in
   height's target ONLY (baseColor bytes unchanged); undo restores per
   channel.
2. Lazy materialization: a set with 5 channels but 1 routed channel holds
   exactly 2 targets (baseColor + 1).
3. Save/load round-trip: both channels persist byte-identical at their
   sizes; the v2 project (no channel rows) loads all-baseColor
   byte-identical (the migration).
4. The #12 unblock, end-to-end: bind a height-channel target through
   `set_height_texture` and the existing golden
   (`height_stroke_perturbs_the_lit_normal`) holds with the session-fed
   texture — the GPU test already proves the shader; this gate proves the
   WIRING.
5. ID-mask select on the height channel: the gate applies to the routed
   channel's target (channel-scoped select composes with the #11 kill).

## Rollout

One kill, three landings inside it: (a) the storage + routing + tests
(headless), (b) save/load + migration, (c) the binding seam + the #12
end-to-end. (a) and (b) are claw-shaped; (c) touches the lit path the
heightnormal kill just landed — dragon-authored or claw with the
heightnormal report as required reading.
