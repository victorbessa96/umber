# Tools lens: late-review reconciliation (2026-11-28)

Review source: `deleg_320688ac-3`, targeting `ec979743`. Reconciled against `416da680`. **No actionable finding remains at current HEAD.** All nine items are repaired with named regression coverage; no new code is needed.

## Finding dispositions

| # | Disposition | Current evidence |
|---|---|---|
| 1 — Transfer/default-library writes bypass confinement | **Stale: already fixed** | `Workspace::confine_step` is variant-aware through `step_path_fields`, including transfer `low/high` reads. `confine_import_library` authorizes the implicit per-user target inside the write root. `confine_step_covers_every_path_bearing_variant` and `import_asset_without_library_requires_write_authority` pin the negative cases. |
| 2 — Direct MCP tools dispatch the wrong shape | **Stale: already fixed** | The six passthrough tools construct the full `{name: body}` envelope and call `dispatch` with it. `direct_step_tools_dispatch_the_full_envelope` covers the previous `set_active`/`set` failure. |
| 3 — Agent errors return outer `ok:true` | **Stale: already fixed** | `step_reply` propagates the inner failure; batch `steps` also returns top-level failure with completed results. `an_agent_level_error_is_a_top_level_failure` pins the previous nested-success defect. |
| 4 — Control client still times out long jobs at 30 seconds | **Stale: already fixed** | `control_timeout` gives `command.run` 30 minutes plus 30 seconds, while other methods retain 30 seconds. `command_run_uses_the_long_job_reply_window` covers the delayed mock server. |
| 5 — Token files permit symlink clobbering and retain permissive modes | **Stale: already fixed** | `write_token_file` rejects symlinks/non-regular destinations, writes a private create-new sibling, syncs and renames it, and enforces mode 0600 after replacement. `token_write_refuses_symlink_and_repairs_existing_mode` pins both failure classes. |
| 6 — Dead MCP child leaves bridge busy and client hanging | **Stale: already fixed** | Both pump exits call `bridge_cleanup`; generation guards prevent stale cleanup from closing a newer client. `bridge_cleanup_reaps_child_and_releases_client` and `stale_bridge_cleanup_cannot_close_a_newer_client` cover cleanup and the successor-client race. |
| 7 — Safe-push omits parity freshness | **Stale: already fixed** | `safe-push.py` runs `parity.py --check` after scorecard and layers, before affected-crate clippy/tests. The Python gate harness includes parity as a required gate. |
| 8 — Asset changes gate only the app | **Stale: already fixed** | Asset paths touch core, CLI, GUI, and app before reverse-dependency closure. `an_asset_change_gates_every_asset_consumer` passes in the current 18/18 safe-push battery. |
| 9 — Scorecard accepts a missing required marker | **Stale: already fixed** | `validate_markers` requires exactly one of every expected marker, rejects duplicates and unknown names, and detects malformed/unclosed markers. `test_scorecard.py` proves each missing marker individually: 15/15. |

## Baseline rows retained from the review

Quick-mask persistence/undo, symmetry persistence, split-redo and merge-below-top, cached ID-mask/density arms, Assets/flatten/material-picker/OccluderScope/timeline, AssetStore GC, DPI runtime re-raster, and local-socket transport remain closed. Materials and instances carrying no stroke paint remain an intentional boundary. Client-library/`--script`/`umber-cli new` remain open roadmap items, not regressions.

## Verification evidence

- Full nine-crate gate at `4e3df3ec`: app, bake, brush, CLI, core, export, GPU, GUI, and paint suites all green.
- App: **931/931**; CLI: **123/123** plus every integration target; export: **122/122** plus **7/7** integration tests.
- `scripts/test_safe_push.py`: **18/18 passed**.
- `scripts/test_scorecard.py`: **15/15 passed**.
- No new production edit is needed for this docs-only reconciliation; the current code and tests were read directly after those green runs.
