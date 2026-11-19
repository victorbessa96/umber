# Brief: snapshot-breadth-2 (2026-11-18, kill `snapshot-breadth-2`)

Closes the headless snapshot harness's constructible surface. The W-C3a
census (scout-umber-residuals.md §3) found exactly one lib-pub renderable
surface still unpinned: the command-palette overlay
(`palette::Palette::show`, `palette.rs:357`) — constructible headless with
`open = true`, static `CATALOG`, and pass-through label/english/shortcut
closures. Everything else worth snapshotting is bin-only (a deliberate
bin→lib promotion decision, not a default) or GPU-shaped.

## Scope

Two new baselines from the one surface:

- `palette_overlay.png` — open, empty query: full recents/command list,
  first row highlighted.
- `palette_query_filtered.png` — open, `set_query("save")`: filtered hits
  (save-project, save-as).

Files: `crates/umber-app/tests/panel_snapshot.rs` (2 tests) + the 2 PNGs.
Baselines host-generated with `UPDATE_SNAPSHOTS=1` before the battery, then
byte-compared — the A2 precedent. Vision-checked: both PNGs render the
real overlay (dark palette, search field, command rows; filtered view shows
only the save hits).

## Non-goals

No bin→lib promotions, no dock_rail PNG (`side_column`/`collapse` are pure
DockState functions, not renderable UI), no viewport/bakes (GPU-shaped).

## Battery

rustfmt, check-layers, scorecard, clippy `-p umber-app --all-targets
-D warnings`, `cargo test -p umber-app --test panel_snapshot` (11/0 with the
two new tests comparing against the fresh baselines). Then kill report +
queue row 544 + dcommit --path-limited + safe-push + ls-remote + lock
release.
