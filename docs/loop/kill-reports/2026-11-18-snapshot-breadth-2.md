# Kill report: snapshot-breadth-2 (2026-11-18, lock `snapshot-breadth-2`)

Brief: `docs/loop/briefs/2026-11-18-snapshot-breadth-2.md` (`aa79d4e3`,
docs-first). Adds the last headless-constructible lib-pub surface to the
panel-snapshot harness: the command-palette overlay.

## Changes

- `crates/umber-app/tests/panel_snapshot.rs`: `palette_overlay_snapshot`
  (open, empty query) + `palette_query_filtered_snapshot` (`set_query("save")`).
- 2 new baselines in `crates/umber-app/tests/snapshots/` (`palette_overlay.png`,
  `palette_query_filtered.png`), host-generated with UPDATE_SNAPSHOTS=1,
  vision-checked (real overlay rendering; filtered view shows only save hits).

## Battery

rustfmt clean; check-layers GREEN; scorecard current;
clippy `-p umber-app --all-targets -D warnings` 0 errors;
`cargo test -p umber-app --test panel_snapshot` 11/0 ok in compare mode.
safe-push green; ls-remote verified; lock released; queue row 544.
