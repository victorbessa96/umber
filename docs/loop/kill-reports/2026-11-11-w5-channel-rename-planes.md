# Wave-5 channel rename carries the paint planes (2026-11-11)

- **Authorship:** a Razul-dispatched subagent (claw), resuming the design
  brief that the earlier dispatched subagent wrote and committed at
  `73bc5f51` (`docs/loop/briefs/2026-11-11-w5-channel-rename-planes-design.md`).
- **Kill-lock:** `razul-w5-channel-rename-planes`, holding
  `crates/umber-paint/src/paint_state.rs`,
  `crates/umber-paint/src/persistence.rs`, `crates/umber-core/src/lib.rs`,
  `crates/umber-core/src/project.rs`, `crates/umber-app/src/main.rs`,
  `assets/i18n/en-US.ftl`, `assets/i18n/pt-BR.ftl`, this report and
  `docs/loop/queue.md`. `crates/umber-cli/src/agent.rs` stayed out: no save
  signature changed.
- **Implements:** the brief's Target 1 (paint rename), Target 2
  (persistence prune), Target 3 (core doc sync), Target 4 (app wiring,
  localized refusals), Target 5 (project round-trip and the stranded-dir
  migration pin), and its Security model. Closes queue row 372's NEXT.

## Built

- `crates/umber-paint/src/paint_state.rs` (Target 1, Security model):
  - `PaintState::rename_channel(set, old, new) -> Result<(),
    ChannelRenameRefusal>`. On the set's tiles it re-keys the Base
    threads (`targets[tile].channels`), every `planes` entry (Base or
    layer owner), the composite `cache` thread (dirt re-marked `Full`
    under the new key), `coalesced_fulls`, `mask_synced`, the `(set, old)`
    `channel_sizes` override, retired tombstones and `retire_pending`;
    follows `active_channel` and `stroke_channel`. `plane_committed` and
    each moved `Plane::committed` are cleared, so the next save reads the
    planes back and stages them under `paint/<new>/`. The owner revisions
    bump (thumbnails rebuild). The retouch history clears and
    `stroke_epoch` bumps when anything moved (the `resize_channel`
    contract).
  - `ChannelRenameRefusal`: `StrokeInFlight`, `BaseColor` (both ways),
    `InvalidName` (re-checked through core's `is_valid_channel_name` plus
    trimmed and non-empty), `PlaneExists` (another channel on the set's
    tiles, among planes, caches, overrides or tombstones, folds to the new
    name case-insensitively), `CaseOnly` (see Judgment calls). Every
    refusal returns before any store is touched.
  - Tests: `rename_channel_carries_every_plane_and_the_override`,
    `rename_channel_refusals_change_nothing`.
- `crates/umber-paint/src/persistence.rs` (Target 2):
  `prune_empty_channel_dirs`, called from `apply_paint_commit` right
  after `prune_empty_layer_dirs`. It removes only EMPTY directories
  directly under `paint/` whose name passes the channel trust check,
  never `layers/` or `quick_mask/`. No new on-disk move: the existing
  one-manifest commit carries the rename. Tests:
  `a_renamed_channel_saves_under_its_new_dir_and_prunes_the_old` (save,
  rename, save: `paint/gloss/tile-1001.png` written,
  `paint/roughness/tile-1001.png` and the directory gone,
  `channel/TextureSet/gloss=32x32` in `target_sizes.txt`, reopen restores
  the bytes byte-identical under `gloss`),
  `prune_empty_channel_dirs_drops_only_empty_channel_dirs`,
  `a_stranded_old_channel_dir_still_loads`.
- `crates/umber-core/src/lib.rs` (Target 3): `TextureSet::rename_channel`
  keeps its checks; its doc comment now names the landed behavior (the
  app moves the planes between the clone check and the commit). The
  "Declaration only: planes ... stay under it" text is gone.
  `is_valid_channel_name` is now `pub` so the paint rename re-checks with
  the same rule. Test:
  `the_public_channel_name_rule_matches_the_rename_check`.
- `crates/umber-core/src/project.rs` (Target 5):
  `a_renamed_channel_round_trips_and_a_stranded_paint_dir_still_loads`
  (tiled set, rename, save, plant a stranded `paint/roughness/`, load: no
  error, declaration and tile lists say `gloss`).
- `crates/umber-app/src/main.rs` (Target 4): `rename_declared_channel`
  renames a CLONE of the set, then calls `PaintState::rename_channel`,
  then commits the clone; `channel_manage_ui` gains
  `Option<&mut PaintState>` and the Texture Sets panel passes the live
  session. `channel_rename_refusal_text` maps each refusal to a localized
  string. The panel and section doc comments name the landed behavior.
  Tests: `rename_field_redeclares_and_a_duplicate_is_refused`,
  `the_full_rename_moves_planes_and_a_paint_refusal_is_atomic`, and the
  existing localized-reason test now covers every refusal.
- `assets/i18n/en-US.ftl`, `assets/i18n/pt-BR.ftl`: three new keys in
  both catalogs, `texturesets.channel-error-busy`,
  `texturesets.channel-error-plane-exists`,
  `texturesets.channel-error-case-only`.

## Judgment calls

- **Case-only rename of a painted channel is refused** (`CaseOnly`). The
  brief allowed a case-only rename of the same channel (core does). On a
  case-insensitive filesystem the save would stage `paint/Roughness/...`
  into the existing `paint/roughness/` directory, then the prune would
  list the file as `roughness/tile-1001.png`, find it missing from the
  manifest, and delete the just-saved plane. Rejected alternative: a
  two-step rename through a temp name inside the save, which adds a
  second commit protocol the brief rules out. A case-only rename of a
  channel holding no planes still lands (nothing to strand).
- **The name re-check uses core's rule, not `is_safe_channel_component`.**
  The brief named the persistence trust check. That check admits `/`,
  device stems and trailing dots, which core refuses; a paint-side
  rename to `a/b` would nest a directory. Using
  `umber_core::is_valid_channel_name` (made `pub`) keeps one rule.
  Rejected alternative: widening `is_safe_channel_component`, which would
  change what the loader admits from existing manifests.
- **The collision census covers overrides and tombstones, not only live
  planes.** A stranded `channel_sizes` line or a tombstone under the
  folded name would otherwise be silently adopted by the moved planes.
- **A rename that moves nothing keeps the history.** The brief's history
  clear exists because undo snapshots name the old plane; with no plane
  under `old` there is no stale snapshot.
- **`stroke_channel` follows too.** The brief names it; it can only equal
  `old` between strokes (mid-stroke refuses), so this is defensive.

## Unverified

- The case-insensitive filesystem path is reasoned, not exercised: the
  tests run on ext4. No test drives the prune on a folding filesystem.
- The live GUI rename (typing in the panel field with a real session) was
  not smoke-tested against the running app; the app test drives
  `rename_declared_channel` directly, and the headless panel harness has
  no paint session.
- Layer-owned plane persistence after a rename (`paint/layers/<id>/<new>/`)
  is covered in memory (`read_plane` and `planes_present` under the new
  key) but not by a save/reopen test.
- Retired-tombstone and `retire_pending` re-keying have no dedicated
  test.

## Deferred

- Renaming a channel on a PARKED set (brief non-goal).
- An "adopt stranded plane" action for projects saved before this kill;
  such projects load the stranded plane as materialized-but-unlisted
  (pinned by test).
- Undo for channel edits, channel reorder, renaming baseColor.
- A case-only rename of a painted channel (refused here; a later kill can
  add a two-phase directory rename if it is wanted).
