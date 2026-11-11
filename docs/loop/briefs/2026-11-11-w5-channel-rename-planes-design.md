# Design brief: w5-channel-rename-planes (rename carries the paint planes)

- Status: DESIGN ONLY, implementation BLOCKED on the lock surface (see
  "Blocker" below). Written 2026-11-11 by a Razul-dispatched subagent under
  kill-lock `razul-w5-channel-rename-planes`.
- Implements the rule `docs/loop/briefs/2026-11-10-channel-manage.md` set
  and row 370 deferred: "Rename = re-declare under the new name; planes
  keyed by channel name must follow". Queue row 372's NEXT.

## What exists (measured on 5af473f8)

- `TextureSet::rename_channel` (`crates/umber-core/src/lib.rs:164-187`)
  renames the declaration and every tile's channel list. Its doc comment:
  "Declaration only: planes keyed by the old name stay under it".
  Name rules via `check_new_name` + `is_valid_channel_name`: trimmed,
  non-empty, case-insensitive dup refused (the renamed channel exempt),
  no separators / Windows-reserved characters / control characters, not
  dot-only, no trailing dot or space, no Windows device stem (CON, PRN,
  AUX, NUL, COM1-9, LPT1-9 by stem). baseColor is protected both ways.
- The panel (`crates/umber-app/src/main.rs:3703-3864`, `channel_manage_ui`)
  takes only `&mut umber_core::TextureSet`; it has no paint session.
  `ChannelEdit::Rename` calls `session.rename_channel` and nothing else.
- Paint planes live in the live `PaintState`
  (`crates/umber-paint/src/paint_state.rs`), keyed by channel NAME in
  ELEVEN private stores: `targets[tile].channels` (Base planes, line 647),
  `planes` (1172), `plane_committed` (1179), `cache` (1185),
  `cache_dirty` (1190), `coalesced_fulls` (1194), `active_channel` (1216),
  `stroke_channel` (1220), `channel_sizes` (1233, keyed `(set, channel)`),
  `mask_synced` (1342), `retired` (1414, `RetiredPlane.channel`) plus
  `retire_pending`, and the retouch history snapshots its plane's channel
  (`retouch.rs:171`).
- No public `PaintState` method removes or renames a Base channel thread:
  grep for `targets.(retain|remove|clear)` and `channels.(retain|remove)`
  over `crates/umber-paint/src/*.rs` returns nothing. The public surface
  only ADDS (`ensure_channel`, `publish_plane`, `materialize_plane`) or
  resizes (`resize_channel`, `clear_channel_size`).
- On disk (`crates/umber-paint/src/persistence.rs`): a non-baseColor Base
  plane is `paint/<channel>/tile-<N>.png` (`paint_channel_tile_path`, 46);
  a layer plane is `paint/layers/<id>/<channel>/tile-<N>.png` (158); the
  per-channel size override is a `channel/<set>/<channel>=WxH` line in
  `paint/target_sizes.txt` (291). The save (`commit_paint_tiles`, 967)
  stages one file per `PaintState::planes_present()` entry, commits one
  manifest, and the apply step PRUNES every listed sidecar file the
  manifest does not name (816-821).

## Why a disk-only move is wrong

The session is the save's source of truth. A plane-directory move done
on disk while the session still holds the planes under the OLD name is
undone by the next save: `planes_present()` lists `(Base, tile,
"roughness")`, the save re-writes `paint/roughness/`, and the apply step
prunes the moved `paint/gloss/` files because the manifest no longer names
them. The declaration says "gloss", the data is back under "roughness":
exactly today's strand, reached through an extra disk churn. A save-time
name translation (write session "roughness" planes as "gloss") is also
rejected: in-session the panel still lists "roughness" as
materialized-but-unlisted and "gloss" reads empty until a reopen, which is
the half-wired shape the channel-manage brief forbids.

Republishing through the public API (read_plane old, publish_plane new)
leaves the old thread materialized (nothing can drop it), so the next
save writes BOTH directories. Rejected.

The rename therefore has to move the planes INSIDE `PaintState`, and the
save then writes the new layout through the existing crash-safe commit.

## Target

1. `PaintState::rename_channel(set: &str, old: &str, new: &str) ->
   Result<(), ChannelRenameRefusal>` in `paint_state.rs`: for every tile of
   `set` (`tiles_of_set`), re-key the Base thread, every layer-owned plane,
   its `plane_committed` record (cleared, not moved: the bytes now belong
   at a new path, so the clean-skip must not fire), the cache thread and
   its dirt (marked `Full`), `coalesced_fulls`, `mask_synced`, the
   `(set, old)` size override, retired tombstones and `retire_pending`
   entries; follow `active_channel` when it names `old`. Refuse while
   stroking (the stroke snapshot names `old`). Clear the retouch history
   and bump `stroke_epoch` (the `resize_channel` contract), because undo
   snapshots name the old plane.
   Refuse when the session already holds any plane whose channel equals
   `new` case-insensitively on those tiles (a stranded undeclared plane of
   that name exists): the move would merge two planes. baseColor refused
   both ways.
2. Persistence (`persistence.rs`): no new on-disk move. The next save
   stages the planes under `paint/<new>/` and the apply step prunes
   `paint/<old>/*.png`; add `prune_empty_channel_dirs` beside
   `prune_empty_layer_dirs` so the emptied `paint/<old>/` directory goes
   too. Crash model: the save is one manifest rename, so a crash leaves the
   old layout (before the commit point) or rolls forward to the new one
   (after it), never a mixture. This is the existing two-phase commit;
   the rename adds no second commit protocol.
3. Core (`lib.rs`): keep `TextureSet::rename_channel` as the declaration
   check + edit; update its doc comment to name the landed behavior (the
   app calls the paint rename first, then the declaration).
4. App (`main.rs`): `channel_manage_ui` gains `Option<&mut PaintState>`.
   Order: validate the declaration rename on a CLONE of the set
   (`rename_channel` on the clone), then `PaintState::rename_channel`,
   then commit the clone into the set. A paint refusal leaves both
   untouched (atomic in memory). Mark the document dirty; the planes reach
   disk on the next Save through the commit above. A refused rename shows a
   localized reason (new keys `texturesets.channel-error-busy` and
   `texturesets.channel-error-plane-exists`, both catalogs).
5. Project (`project.rs`): round-trip test (rename, save, load: the
   declaration says the new name). Migration: a project already saved with
   a stranded `paint/<old>/` and a declaration naming `<new>` still loads
   (the loader loads every safe channel directory into Base whatever the
   declaration says; pin it with a test). The stranded plane surfaces as
   materialized-but-unlisted, so the user can re-adopt it by renaming
   `<new>` back or by a later "adopt stranded plane" action (deferred).

## Security model

A channel name is a path segment under `paint/`. The declaration side
already refuses separators, `..`/dot-only, `:`, the other Windows-reserved
characters, control characters, trailing dot/space and the device stems.
The paint rename re-checks the target with the persistence trust check
(`is_safe_channel_component`), so `PaintState` cannot be driven into an
escaping path by a caller that skipped core. Collision: refused when any
existing plane of the set's tiles carries the target name
case-insensitively (two directories that one case-insensitive filesystem
would fold into one). The prune only removes files the trust boundary
admits (`list_paint_tile_names`), so foreign files in `paint/<old>/` keep
the directory alive and are never deleted.

## Verification plan

- umber-paint GPU-free tests are not possible for `PaintState` (it needs
  a wgpu device); follow the crate's existing headless-device test
  pattern: rename moves a painted Base plane and a layer plane, the old
  name reads `None`, the new name reads the bytes, the override follows,
  refusal while stroking, refusal on a case-folded existing plane.
- persistence: save after rename writes `paint/<new>/tile-1001.png`,
  removes `paint/<old>/tile-1001.png` and the empty dir; reopen restores
  the bytes under the new name.
- core + app tests as listed in Target; both `.ftl` catalogs complete.
- Battery: rustfmt --check, check-layers, scorecard --check, clippy and
  tests for umber-core, umber-paint, umber-app, umber-gui (umber-cli too:
  it consumes the persistence API).

## Blocker (why this brief lands without code)

The required `PaintState::rename_channel` lives in
`crates/umber-paint/src/paint_state.rs`, which this kill's lock does not
hold (the dispatch forbade it explicitly). Every plane store is a private
field and no public method can drop or re-key a Base channel thread, so no
change confined to the locked files can make the planes follow the rename
without the save undoing it (see "Why a disk-only move is wrong"). The
next dispatch needs the lock widened to add `paint_state.rs` (and
`crates/umber-cli/src/agent.rs` only if a save signature changes, which
the target above avoids).

## Non-goals

- Channel reorder, undo for channel edits, clearing planes on remove.
- Renaming a channel on a PARKED set (its planes are not in the session's
  active tiles until switched to; the panel already edits only the active
  set).
- An "adopt stranded plane" action for projects saved before this kill.
- Renaming baseColor (its plane is the flat legacy sidecar).
