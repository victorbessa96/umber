# Per-channel resolution — wave-perchanres

Task: requirements §2's sizing row, the deferred half ("per-CHANNEL sizes
stay the boundary, paint_state module docs"). The per-channel tile pool
landed in `dc5c2c9` (`TileTargets` nests per-channel `PaintThreads`
lazily, every channel target built at its tile's SET size); this kill
extends it so each channel of a set can carry its own resolution.

Lock: `crates/umber-app/src/paint_state.rs` + the `texture_sets_panel_ui`
section of `crates/umber-app/src/main.rs` + this report. No other files
touched (requirements/ftl deltas below are for the dragon to apply).

## Built

**`paint_state.rs` — the sizing model.** New `channel_sizes:
BTreeMap<(String, String), (u32, u32)>` field on `PaintState`, keyed
`(set, channel)`; absent = inherit the set's size. New API, all `pub`:

- `size_of_channel(set, channel)` — the override when recorded, else
  `size_of_set`; `BASE_COLOR_CHANNEL` short-circuits to the set size.
- `channel_override(set, channel)` — the recorded override or `None`
  (always `None` for baseColor); the panel's inherit-default read.
- `channel_sizes()` — the whole override map, sorted; the save census.
- `tile_channel_size(tile, channel)` — set resolution first, then the
  fallback; the load path's per-artifact size check.
- `resize_channel(set, channel, w, h)` — the channel's `resize_set`
  (records the override, resizes ONLY that channel's materialized
  targets on that set's tiles, same session-wide history clear +
  epoch bump; baseColor delegates to `resize_set`).
- `clear_channel_size(set, channel)` — drops the override and rebuilds
  at the set's size; no-op (history kept) when nothing was recorded.

Changed behavior: `ensure_channel` builds at the CHANNEL's size
(`tile_channel_size`, was `tile_size`); `resize_set` preserves overrides
(overridden channels keep their size while baseColor + inheriting
channels rebuild — the override map is snapshotted before the
per-target loop, E0502). Module docs: the "per-CHANNEL sizes stay the
named follow-up" v1-boundary bullet is replaced by the landed contract
plus the judgment-call block (baseColor-is-canvas, set-density strokes,
set-sized masks, 8K/per-tile deferrals).

**`main.rs` — codec, save/load, panel.**

- `PaintSetSizes` gains `channels: BTreeMap<(String, String),
  (u32, u32)>`. Format emits `channel/<set>/<channel>=<WxH>` lines
  sorted between sizes and assignments (exact form below); parse adds
  the `channel/` arm. `PaintTileSkip` gains `channel: String` (one tile
  can skip channels independently; `Copy` dropped, `Clone` kept).
- Save (`commit_paint_tiles`) writes overrides for exactly the persisted
  sets (the set-size rule applied to channels). Load
  (`load_paint_tiles_reported`) applies overrides AFTER the set resizes
  and BEFORE the PNG uploads in both mapping branches (plus the
  channel-only-sets join the resize-name collection), and size-checks
  each artifact against `tile_channel_size` instead of `tile_size`.
- Panel: `texture_sets_panel_ui` takes `channels: &[String]` (the
  caller passes the import session set's channel names) and draws one
  row per document channel under the set's combo — `SIZE_PRESETS` plus
  the `inherit` default — via the new `ChannelSizeChoice`
  (`Inherit`/`Preset`/`Custom(display-only)`) enum. Rows list doc
  channels ∪ materialized-but-unlisted, skip baseColor (it IS the set
  row). Picking a size calls `resize_channel`, picking inherit calls
  `clear_channel_size`; the channel intentional-clear hint is a new
  `tr` string.

Tests: 4 new, 2 existing updated (field additions only, premises
unchanged — `set_sizes_codec…` gains the `channels` field + an
all-inherit assert; the two-set mismatch skip gains
`channel: baseColor`).

- `overridden_channel_materializes_at_its_size_while_base_color_keeps_the_set`
  (paint_state, GPU): 4096 default set, height overridden to 512 —
  census via `channels_present` + `read_channel_tile` dims pin
  (512, 512) vs (4096, 4096); inherit reads the set size; unrouted
  reads stay `None`.
- `resize_channel_touches_only_its_channel` (paint_state, GPU): fill
  patterns on base + height, a height stroke (history non-vacuous),
  resize to 256 — history cleared, set size unchanged, height dims
  (256, 256), baseColor dims AND bytes (`[5, 76, 147, 218]`-seeded
  pattern) untouched; then a set resize to 1024 preserves the 256
  override; then clear rebuilds at the set size while clearing a
  never-overridden channel keeps history.
- `channel_sizes_codec_round_trips_and_old_files_load_all_inherit`
  (main.rs, CPU): exact pinned text (header, sort order), channel-only
  file parses, 5 malformed shapes skip (incl. the double-`=` sketch),
  pre-channel file parses with empty channels.
- `channel_size_override_round_trips_and_mismatches_skip_per_channel`
  (main.rs, GPU): save pins the exact mapping text with the override
  line; fresh 4K session restores height byte-identical at 256 and base
  at 512 with inherit for unlisted channels; a clobbered height PNG
  refuses with `channel: "height"` on the skip record.

## The size-resolution rule (channel → set fallback)

Every read funnels through `size_of_channel`:

1. `channel == BASE_COLOR_CHANNEL` → `size_of_set` (it IS the canvas).
2. `channel_sizes[(set, channel)]` hit → the override.
3. Miss → `size_of_set` (unknown sets → `DEFAULT_TARGET_SIZE`²).

Who reads what: `ensure_channel`/`publish_fill_channel` materialize at
(2)/(3) via `tile_channel_size`; the save census walks the raw map;
the load check reads `tile_channel_size`; the panel reads
`channel_override` (inherit = `None`). There is exactly one fallback
implementation — no call site re-derives it.

## The codec's exact additive form (verified against the parser source)

Canonical line: `channel/<set>/<channel>=<WxH>`, e.g.
`channel/Sword/height=512x512`, sorted by (set, channel) between the
size lines and the `tile-<N>` lines. Verified against
`parse_paint_set_sizes`'s shape: it does `split_once('=')`, trims, then
tries arms in order — `tile-` prefix (numeric) → `channel/` prefix
(`split_once('/')`, both sides non-empty, right side through the
existing `parse_paint_target_size` `<WxH>` grammar) → size line. An old
file (no channel lines) parses to an empty channel map: every channel
inherits — the migration is the ABSENCE of lines, no version branch
(the parser skips `#` headers either way).

Deviations from the brief's sketch, deliberate: the brief suggested
`channel=<set>/<name> <WxH>` "or the parser's natural additive form".
The single-`=` form won because (a) the parser splits on the FIRST
`=`, so a second `=` lands in the size field and rejects (`channel=
Body/ao=64x64` is pinned-skipped in the codec test); (b) the space form
needs a new value grammar while `channel/<set>/<channel>=<WxH>` reuses
BOTH existing grammars (left identity, right `<WxH>`); (c) it mirrors
the sidecar's existing `<channel>/tile-<N>.png` nesting. New-file-on-old-
reader degrades gracefully: the line parses as a size line for a bogus
set named `channel/<set>/<channel>` (no tiles ever join it — the same
reserved-prefix class as `tile-`, documented on the parser). A set or
channel name holding `/` cannot round-trip (first `/` splits set from
channel); sidecar nesting already assumes `/`-free channel names.

The `# Umber paint-set sizes v1` header is intentionally NOT bumped: a
file without overrides saves byte-identical to before, and the parser
never branched on it.

## Resize semantics

- `resize_channel`: records the override first (later tiles build at
  it), clears the shared stroke history + pending commits + bumps the
  stroke epoch (snapshots at the old size no longer fit — the same
  session-wide contract as `resize_set`, whose module-docs rationale
  for the shared clear is unchanged), then publishes ONE `Resize` per
  MATERIALIZED target of that channel on that set's tiles. Nothing
  else moves: no other channel, no other set, no mask, no layer plane.
- `resize_set`: publishes per-thread sizes — baseColor at the set's,
  each materialized channel at its override or the set's. Overrides
  survive; the history/mask contracts are unchanged.
- `clear_channel_size`: removes the override; no-op (history kept) if
  none existed; else the full resize-channel contract at the set's
  size.
- Load ordering: assignments → set resizes → channel overrides →
  `process_pending` → per-artifact uploads size-checked against the
  channel's size. The override is therefore in place before its pixels
  land (upload materialization builds at the override).

## Judgment calls (all also in the module docs)

- baseColor never carries an override (lead-vs-follow); `resize_channel`
  on it IS `resize_set`, `clear` on it is a no-op.
- Strokes still condition at the SET's texel density on every channel
  (per-channel dab density deferred — painting a smaller channel lands
  set-scale dabs on fewer texels). The undo capture already handles
  mixed sizes (it uses the snapshot's own dims).
- Quick mask stays per-set (gates baseColor only); layer-mask planes
  stay set-sized; neither follows a channel resize.
- New user strings go through `tr()` (3 keys), but the `.ftl` files are
  outside this kill's lock — deltas below for the dragon. Consequence:
  `every_called_key_exists_in_en` fails until they land (flagged, not
  hidden).
- Panel rows = doc channels ∪ materialized-but-unlisted (an override
  outliving its import stays editable instead of stranded); empty row
  list renders nothing (no header, no hint).
- `Custom(w, h)` (stored non-square) is display-only in the combo; the
  codec preserves it exactly.

## Deferred (recorded here + paint_state module docs)

- Per-channel dab density (stroke conditioning at the channel's texel
  density); fill-path raster dims ride the same follow-up.
- Per-TILE channel sizes (key would grow a tile id; no caller needs it
  while sets stay square).
- 8K-per-channel VRAM guidance (the set combo's note covers 8K once).
- Export-dialog per-channel size awareness: nothing to fix today —
  verified by reading that no export path assumes channel dims (save
  writes readback dims; per-tile export reads baseColor).
- Layer-mask planes under a channel resize stay set-sized; how the
  compositor gates a smaller channel with a set-sized plane is the
  compositor's (future) problem, named in the module docs.

## Unverified (gaps, not papered over)

- NOTHING was executed: no `cargo build/test/clippy/fmt`, no
  `verify-kill.sh` (binding wave-end rule: no cargo of any kind, no
  bash scripts, no commits). CI (ubuntu + windows, lavapipe) is the
  gate — in particular the 4 new tests' GPU assertions (Resize-publish
  ordering, readback dims, byte patterns) have never run.
- `rustfmt --edition 2021 --check` (the bare binary, read-only — not
  cargo, not a script) on the two touched files: CLEAN after 4
  self-fixes. Caveat: local rustfmt reports `1.9.0-stable`, which may
  not match the pinned toolchain's; mitigating evidence is that every
  remaining shape mirrors CI-green in-file precedent.
- Clippy `-D warnings` by hand only: no >7-arg fns, no derivable
  Default added, no negated float comparisons, no `map_or(true, …)`,
  no field-shorthand or lifetime issues introduced, no `#[must_use]`
  results ignored (`let _ =` on publishes, matching existing code),
  no new `new()` without `Default`, no format-string field access, no
  `matches!` additions, no `struct` in `impl`, no E0502 (override map
  snapshotted before the mutable target loop — stated in the code).
- Panel behavior needs a runtime run: no egui UI tests exist in the
  repo, so the rows/combos/inherit switching are compile-checked only.
- `every_called_key_exists_in_en` WILL fail until the dragon applies
  the ftl deltas below (3 `tr` keys, `include_str!` at compile time).
- Egui `ComboBox::from_id_salt` + blanket `AsIdSalt` verified against
  vendored `egui-0.36.2` source (`id_salt.rs:7-9`); `selectable_value`
  + `preset_label` reuse the existing `size_combo` shapes.

## Doc deltas (for the dragon — do NOT edit these files in this kill)

Requirements row (`docs/specs/requirements.md:43`, §2 sizing row):
replace the tail clause `per-CHANNEL sizes stay the boundary
(paint_state module docs)` with `per-CHANNEL sizes ✓ built (PENDING) —
each channel of a set carries its own resolution in the paint session
(absent = inherit the set's size) + paint/target_sizes.txt
channel/<set>/<channel> lines (pre-channel files load all-inherit); v1
boundary: strokes still condition at the set's texel density, masks stay
per-set (paint_state module docs)`.

`assets/i18n/en-US.ftl`, after the `texturesets` `.no-paint` line:

```ftl
    .per-channel = Per-channel sizes
    .inherit = Inherit (uses the set's size)
    .channel-resize-clears = Changing a channel's size clears that channel's canvas and the stroke undo history. This is intentional.
```

`assets/i18n/pt-BR.ftl`, after the `texturesets` `.no-paint` line:

```ftl
    .per-channel = Tamanhos por canal
    .inherit = Herdar (usa o tamanho do conjunto)
    .channel-resize-clears = Mudar o tamanho de um canal limpa a tela dele e o histórico de traços. Isso é intencional.
```

## Gates run

- `cargo fmt --all` / `cargo fmt --check`: NOT RUN (wave-end rule).
  Substitute: read-only `rustfmt --edition 2021 --check` on
  `paint_state.rs` + `main.rs` — 4 diffs found in own hunks, all 4
  fixed to rustfmt's exact suggestion, re-check CLEAN (the only
  remaining diffs in the crate are `postfx_panel.rs`'s, another live
  kill's surface, untouched).
- `cargo clippy -p umber-app --all-targets -- -D warnings`: NOT RUN
  (wave-end rule). Trap-list hand-check above.
- `bash scripts/verify-kill.sh`: NOT RUN (no bash scripts rule).
- `cargo build` / `cargo test`: NOT RUN (standing contract + wave-end
  rule). CI is the runtime gate.

## Files touched

- `crates/umber-app/src/paint_state.rs` (channel sizing model + 2 GPU
  tests + module-docs contract)
- `crates/umber-app/src/main.rs` (`target_sizes.txt` channel lines,
  save/load, `PaintTileSkip.channel`, panel rows + `ChannelSizeChoice`,
  2 tests, 2 existing tests' constructions)
- `docs/loop/kill-reports/2026-10-12-wave-perchanres.md` (this file)

Tests added: 4 (`overridden_channel_materializes_at_its_size_while_
base_color_keeps_the_set`, `resize_channel_touches_only_its_channel`,
`channel_sizes_codec_round_trips_and_old_files_load_all_inherit`,
`channel_size_override_round_trips_and_mismatches_skip_per_channel`).
Existing tests updated: 2 (`set_sizes_codec_round_trips_exactly_and_
rejects_garbage`, the two-set mismatch skip) — constructions only.
