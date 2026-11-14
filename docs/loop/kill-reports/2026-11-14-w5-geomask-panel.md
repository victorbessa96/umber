# Kill report — 2026-11-14 w5 geomask-panel (R-B2 K4, wave-5 final UI kill)

**Kill:** K4 `geomask-panel` of the geometry-masks wave (R-B2), per
`docs/loop/briefs/2026-11-12-geomask-design.md`. K1 `5ff1510c`, K2 `0e4e5035`,
K3 `c98a7ea6`, K5 `744e114f` were already on master; K4 was the last open kill.
**Landed:** `4170fac5` on branch `hermes-subagent/subagent-sa-0-a9b06e64`
(parent `8f7ac53f` = master tip at dispatch; ff-mergeable). Queue row **384**
marked LANDED with the hash.

## Lock hygiene

Dead-holder adoption done first: `/tmp/umber-kill-locks/razul-geomask-panel.lock`
was stale (holder `local-34964c4b8f`, last state.db message 2026-11-14 02:46:27 UTC,
no cargo/rustc processes, worktree `.worktrees/geomask-k4-1794576177` clean).
Re-verified the holder was dead (process census + read-only sqlite census of
`~/.hermes/state.db?mode=ro`), then `scripts/kill-locks.sh remove` +
`add razul-geomask-panel <13 lock paths>`, verified with `list`.

## What landed (4170fac5, 10 files, +656/-14)

- `crates/umber-app/src/layers_panel.rs` — geometry-mask section: include-
  checkboxes over the visible mesh parts and present UV tiles; unchecking writes
  one undoable `LayerCommand::SetGeometryMask`; include-all / exclude-all /
  invert = one command on the active layer over the VISIBLE checklist only
  (non-visible exclusions survive); copy/paste over a session clipboard
  (never persisted), paste = one command per selected layer, empty clipboard =
  no-op + notice. Pure helpers (`escape_part_name` with control/bidi escaping,
  `toggle`/`invert`/`set_visible`) + 4 unit tests.
- `crates/umber-app/src/main.rs` — `AppState` gains `mesh_parts` +
  `geometry_clipboard`; `load_mesh_path` uses `load_with_parts` and hands the
  paint session `set_geometry_source` (K3 seam); View menu gains the
  hide-excluded checkbox driving `viewport.set_hide_excluded`; the Viewport
  tab feeds `set_triangle_visibility(triangle_visibility(mask, parts))` every
  frame; the Layers panel draws the section under `layer_mask_ui`.
- `crates/umber-app/src/viewport.rs` — the K5 `dead_code` allowances lifted;
  doc comments synced to the now-landed wiring (no behavior change).
- `assets/i18n/en-US.ftl` + `pt-BR.ftl` — 13 new keys in BOTH catalogs
  (`menu.hide-excluded`, `layers.geometry-*`).
- `crates/umber-app/tests/panel_kittest.rs` — 4 behavior kittests:
  `geometry_uncheck_writes_one_command`, `paste_geometry_mask_one_command_per_selected`,
  `paste_empty_clipboard_is_noop`, `invert_flips_visible_checklist_only`.
- `crates/umber-app/tests/panel_snapshot.rs` +
  `tests/snapshots/layers_panel_geometry_section.png` — the
  `layers_panel_geometry_section_snapshot` baseline named in the brief
  (generated with `UPDATE_SNAPSHOTS=1`, visually verified: section header,
  body ☑ / gun ☐, Tile 1001 ☑ / Tile 1002 ☐, all five buttons).
- `README.md` — scorecard regenerated (`tests: '2,900+' -> '3,000+'`), the only
  label change.
- `docs/loop/queue.md` — row 384.

Serde carry is unchanged from K1 and additive only: `default` +
`skip_serializing_if` when empty, **no project version bump** (Bessa's R-B2
ruling). Doc comments synced to landed behavior in the same commit.

## Battery (all real runs, worktree `.worktrees/subagent-sa-0-a9b06e64`,
shared `CARGO_TARGET_DIR`, one cargo command at a time under
`/tmp/umber-cargo.lock`, `PATH` incl. `~/.cargo/bin`, no `--features gpu`,
no `--ignored`)

| gate | result |
|---|---|
| `cargo fmt --all --check` | 0 (clean) |
| `python3 scripts/check-layers.py` | GREEN (16 crates) |
| `python3 scripts/scorecard.py --check` | current (regenerated once: 2,900+ → 3,000+) |
| `cargo clippy -p umber-app -p umber-core --all-targets -- -D warnings` | 0 warnings |
| `cargo test -p umber-app -p umber-core` | 0 failed (all binaries ok; 1,427 passed total, incl. the 5 new K4 tests) |

Disk pre-checks passed (`df -h /`, `df -h /tmp` above the 22G floor).

## Deviations

- Worked in the dispatched subagent worktree
  `.worktrees/subagent-sa-0-a9b06e64` on branch
  `hermes-subagent/subagent-sa-0-a9b06e64` per the worktree-isolation
  instruction; the parent agent merges the branch (ff-only) to master. No
  direct master push was made from this session.
- Queue row timestamp written as `09:2x` (dispatch clock); battery ran
  09:21–09:4x local.
- No UNRUN pins: every battery gate ran for real.
