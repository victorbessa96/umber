# Kill report 2026-11-11 razul-channel-manage (claw, wave-5 channel manage)

Authorship: claw (Razul dispatched subagent, isolated worktree
`hermes-subagent/subagent-sa-0-56d8d24b`). Kill lock `razul-channel-manage`
held over `crates/umber-core/src/lib.rs`, `crates/umber-core/src/project.rs`,
`crates/umber-app/src/main.rs`, `crates/umber-app/src/document.rs` (not
edited in the end), `assets/i18n/en-US.ftl`, `assets/i18n/pt-BR.ftl`.
Implements brief `docs/loop/briefs/2026-11-10-channel-manage.md` (Build 1-4),
ROADMAP:240 / census R-B4 tail "channel add/remove/rename open"; queue row 354.

## Built

- **Core (`umber-core/src/lib.rs`)** (Build 1): `TextureSet::add_channel(name,
  kind)`, `remove_channel(name) -> Channel`, `rename_channel(old, new)`, all
  `Result<_, ChannelError>`. Names are trimmed. Refused: empty, duplicate
  (case-insensitive, because per-channel planes save as one directory each and
  Windows folds case), unsafe as one sidecar directory name (`/ \ : < > " | ? *`,
  control characters, dot-only), unknown channel, and any remove/rename of
  `baseColor` or rename onto it. A case-only rename of the same channel is
  allowed. Tiled sets (`tiles` non-empty) get the same edit on every tile's
  channel list, so the per-tile lists stay in step. New public
  `umber_core::BASE_COLOR_CHANNEL` and `umber_core::ChannelError`
  (thiserror). Five unit tests: add (trim, dup, empty, unsafe names, refused
  add is a no-op), remove (baseColor protected, unknown), rename (order and
  kind kept, case-only rename, dup, baseColor both directions, refused rename
  is a no-op), tiled-set lockstep, the constant names the default first channel.
- **Persistence (`umber-core/src/project.rs`)** (Build 1): test
  `managed_channels_round_trip_through_project_json`: add + rename + remove,
  save, load equals the model, re-save byte-identical. No production change:
  `TextureSet.channels` already serializes, and the app's save path already
  writes `mesh_import.session_set` (active) and `parked_texture_sets`.
- **App (`umber-app/src/main.rs`)** (Build 2): `texture_sets_panel_ui` now
  takes `&mut umber_core::TextureSet` (the session set) instead of
  `(session_set: &str, channels: &[String])` and returns whether the
  declaration changed. Both call sites (Properties rail TextureSet context and
  the Texture Sets dock panel) set `doc.dirty = true` on a change, so the
  quit/New/Open confirm and the autosave pick it up and Save writes it to
  project.json. New `channel_manage_ui`: one row per declared channel with a
  rename `TextEdit` committed on focus loss through `document::rename_commit`
  (the layer rename convention), its kind label, and a Remove button that
  ARMS a confirm group (Remove channel / Keep channel, GUI plan rule 8);
  baseColor is listed as a plain label with a hover reason and no controls;
  an add row (name field, Data/Color combo, Add channel). Refusals show the
  localized reason in the warn colour. The section draws only when the panel's
  active set equals the session set, so a set-switch frame cannot edit the
  outgoing set. The per-channel size rows now read the session set's channels
  directly (same contents as before). `TEXTURE_SET_LABEL_KEYS` gained
  `texturesets.channels` (the rail search vocabulary; the row-label test
  pin updated to the four labels).
- **Catalogs** (Build 3): 18 new `texturesets.*` attributes in BOTH
  `en-US.ftl` and `pt-BR.ftl` (`channels`, `channels-hint`,
  `channel-base-color`, `channel-rename`, `channel-remove`,
  `channel-remove-hint`, `channel-remove-confirm`, `channel-remove-apply`,
  `channel-remove-cancel`, `channel-add-name`, `channel-add`, `kind-color`,
  `kind-data`, five `channel-error-*`). Same key set in both, checked by
  script and by the existing `every_shipped_locale_covers_every_en_us_key`.
- **App tests** (Build 4): `channel_manage_tests` kittest module (GPU-free:
  `channel_manage_ui` takes no paint session): remove arms then confirm drops
  the declaration and baseColor has no Remove (4 buttons for 5 channels);
  cancel disarms; add declares with the default Data kind, a case-folded
  duplicate is refused and its reason is drawn; every `ChannelError` has a
  localized reason.

## Judgment calls

- **Removal is declaration-only** (the brief's DEFAULT). Painted planes stay
  and still edit through the per-channel rows (the existing
  materialized-but-unlisted rule). Plane clearing is not offered; it would be
  a separate destructive confirm (see Deferred). The confirm is kept anyway,
  because removing a declaration changes what export and the panel list.
- **Rename does NOT follow paint planes: the strand rule.** Paint planes are
  keyed by channel name inside `umber-paint`'s `PaintState` (targets,
  per-channel overrides, `channels_present`, the `paint/<channel>/` sidecar
  directories and manifest names). `umber-paint` has no rename API, and
  `umber-paint` is outside this kill's lock. A rename therefore re-declares
  only: pixels painted under the old name stay under the old name, and the
  panel's per-channel rows keep listing the old name as a
  materialized-but-unlisted channel, so nothing is lost or hidden. The new
  name starts with no planes. Wiring half of it here (for example renaming
  only the override map) would be worse than either, per the brief's "do not
  half-wire it" rule. Named follow-up below.
- **Case-insensitive duplicate check**, stricter than the brief's "dup
  refused": `roughness` vs `Roughness` would collide as sidecar directories
  on Windows/macOS. Same reasoning as `validate_project_structure`'s set-name
  rule.
- **Name safety check in core**, beyond the brief: a declared channel the
  paint persistence would later refuse as a sidecar component (`a/b`, `C:`)
  is refused at declaration time instead.
- **No undo entry.** Channel edits are not `LayerCommand`s; the layer undo
  journal (`Document::run`) cannot carry them without a new command family in
  umber-core's journal. The edit marks the document dirty instead. The
  remove confirm covers the destructive case.
- **`document.rs` not edited.** No Document-level wrapper was needed: the
  session set lives on `MeshMapImportState`, and `doc.dirty` is a pub field.

## Unverified

- The live release app was not launched; the Channels section is verified by
  the kittest module on `channel_manage_ui` alone, not inside the full panel
  with a GPU paint session.
- The rename text-edit commit path (focus loss) has no dedicated kittest; it
  reuses `document::rename_commit`, which has its own unit test, and the core
  rename is tested.
- The `doc.dirty = true` wiring at the two call sites holds by construction
  (no headless test drives `PanelViewer`).
- Parked sets (not active) are edited only after switching to them; the
  parked copy's save path is unchanged and holds by construction.

## Deferred

- **Paint-side rename follow (named follow-up `w5-channel-rename-planes`)**:
  add `PaintState::rename_channel(set, old, new)` in `umber-paint` moving
  targets, overrides and the sidecar directory, then call it from the panel
  after a successful core rename. Until then the strand rule above holds.
- **Clear planes on remove** (a separate destructive confirm), only if users
  ask; declaration-only is the default.
- **Undo for channel edits** (a channel command family in the journal).
- **Channel reorder** (not in the brief).

## Process note

Worked in an isolated subagent worktree on master `2ad43e94`. Pre-existing
stale lock `razul-templates-land.lock` (assets/templates only) left in place.
No sibling held locks on these files at dispatch.
