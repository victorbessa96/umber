# R-D1 baker settings sync + linked bake groups (2026-11-11)

**Kill:** `razul-rd1-baker-sync` (census D1, wave-5 power-user tail, P1).
Brief: `docs/loop/briefs/2026-11-11-rd1-baker-sync.md`. Dispatched
subagent (claw).

## Landed

| file | change |
|---|---|
| `umber-core/src/project.rs` | `BakeParamCarry` (Float/Int/Bool), `BakeSettingsBlockCarry` (baker key -> param name -> value + `out_dir`), `BakeGroupCarry`, `BakeSyncCarry`; `ProjectModel::bake_sync` + `with_bake_sync`; `ProjectFile.bake_sync` as `serde(default, skip_serializing_if = "BakeSyncCarry::is_empty")`, threaded through both exhaustive destructures |
| `umber-app/src/bakes_panel.rs` | `BakeSettingsBlock` (`BakeSelection` -> `umber_graph::ParamValue` map + out dir), `BakeGroup`, `BakeSync` (per-set blocks, groups, session clipboard), `BakeSyncError`; `BakeSelection::kind_key`/`from_kind_key`; panel methods `settings_snapshot`, `apply_settings`, `set_settings_focus`, `commit_focus_settings`, `copy_settings`, `paste_settings`, `link_sets`, `unlink_set`, `group_plan`, `sync_carry`, `load_sync_carry`; new "Settings sync" ParamGroup (Settings-for combo, Copy / Paste to checked sets / Link checked sets / Unlink / Bake group) |
| `assets/i18n/{en-US,pt-BR}.ftl` | 22 new `bakes.*` keys each, all read by `bakes_panel.rs` |

## Design

- **One edit surface.** The panel sliders stay the only editor. The sync
  group's "Settings for" combo picks a set; its block (the group's when
  linked) loads into the sliders, and each frame the sliders write back
  to it. Editing any member writes the group block. With the combo on
  "This panel" (the default) nothing is read or written, so the
  pre-R-D1 panel, manual Bake, Bake all and the auto-rebake fire gate
  run exactly as before. No baker code changed.
- **Block shape.** One entry per checked map, carrying the params that
  baker reads: resolution, dilation, margin for all; rays + occluder
  scope for AO/thickness/bent normal; skew threshold for AO. The key set
  is the set's baker set.
- **Paste is all or nothing.** Every target is staged first; a target
  without a carried baker refuses the whole paste with
  `MissingBaker { set, kind }` (message names both). A carried baker's
  params and the out dir overwrite; target bakers the clipboard lacks
  stay as they were.
- **Groups.** Link needs two or more distinct checked sets, none already
  linked (one group per set). The shared block comes from the focused
  set when it is a member, else the first member's block, else the
  sliders. Bake group dispatches the members (filtered to sets on the
  mesh) through the existing multi-set `bake_jobs` fan-out: one
  `BakeJob` per member per planned map. `group_plan` is the pure form
  the tests pin. Unlink copies the group block into the member; a group
  left with one member dissolves the same way.
- **Persistence.** `bake_sync` is an optional key on `project.json`.
  Absent loads as zero groups; empty is not written, so old files
  re-save byte-identically. No version bump. The clipboard is session
  only. Loading is lenient: unknown baker keys drop, a set claimed by
  two groups stays in the first, sub-two-member groups dissolve, and a
  missing out dir falls back to the panel's.

## Tests (new)

- core: `bake_sync_groups_and_settings_round_trip_byte_identical`,
  `pre_bake_sync_project_loads_with_zero_groups_and_resaves_without_the_key`.
- app: `plain_panel_without_a_focus_reads_and_writes_no_block`,
  `snapshot_round_trips_through_apply`,
  `linked_group_of_three_fans_out_three_jobs_with_equal_settings`,
  `editing_through_member_a_is_visible_reading_member_b`,
  `unlink_materializes_the_group_block_and_later_edits_diverge`,
  `link_refusals_change_nothing`,
  `paste_onto_a_set_with_the_same_bakers_applies_every_carried_value`,
  `paste_onto_a_set_missing_a_baker_refuses_naming_the_kind`,
  `sync_carry_round_trips_through_the_project_file`,
  `hand_edited_sync_carry_loads_with_mercy`,
  `kind_keys_are_stable_and_invert`.
- No existing test changed.

## Lock deviations

None. Every edit is inside the lock list.

## Deferred

- **App save/load wiring.** `main.rs` builds `ProjectModel` and restores
  settings; it is outside this lock, so the `.umber` round trip is
  available (`with_bake_sync(bakes.sync_carry())` on save,
  `bakes.load_sync_carry(&model.bake_sync)` on load) but not called by
  the shell yet. Two lines in `main.rs` (~4213 save, the load-restore
  site) are the follow-up.
- **Per-set settings for manual Bake.** Manual Bake / Bake all / auto
  rebake still use the sliders for every enabled set (the unchanged
  fire gate). Only Bake group uses a stored block. Per-set blocks
  driving a mixed multi-set dispatch needs one `BakeRequest` per block.
- Per-UV-tile bake exclusion (census D1's other row) is untouched.
- Group rename UI (groups get localized default names "Group N").
