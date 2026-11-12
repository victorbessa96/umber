# R-D1 shell wiring: bake_sync rides the .umber save/load (2026-11-11)

- **Authorship:** dragon (Razul), the two-line follow-up the R-D1 claw
  kill (f6b8fbfa, lock razul-rd1-baker-sync) named as deferred because
  `main.rs` sat outside its lock.
- **Kill-lock:** `razul-rd1-shell-wiring` over `crates/umber-app/src/main.rs`,
  `docs/loop/queue.md`, this report.
- **Implements:** the deferred "App save/load wiring" from
  `docs/loop/kill-reports/2026-11-11-rd1-baker-sync.md`.

## Built

- `write_project_files` (main.rs ~4227): the `ProjectModel` builder chain
  gains `.with_bake_sync(self.state.bakes.sync_carry())` after
  `.with_journal_seq(...)`. Empty carry is not serialized
  (`skip_serializing_if`), so projects without sync state re-save
  byte-identical.
- `open_project_at` (main.rs ~4421): first statement of the `Ok(model)`
  arm calls `self.state.bakes.load_sync_carry(&model.bake_sync)` before
  anything else reads the model.

## Verification

- Battery on the edited tree (UMBER_ASSETS_DIR set, flock):
  `BATWIRE2 fmt=0 layers=0 score=0 clippy=0 tests=0`, 1156 passed /
  0 failed (`cargo test -p umber-app`), log
  `~/.hermes/cache/scratch/wiring2-battery.log`.
- The claw half's dragon battery: `BATRD1 fmt=0 layers=0 score=0
  clippy=0 tests=0`, 1392 passed / 0 failed, log
  `~/.hermes/cache/scratch/rd1-battery.log`.
- Round-trip is pinned by the claw's core tests
  (`bake_sync_groups_and_settings_round_trip_byte_identical`,
  `pre_bake_sync_project_loads_with_zero_groups_and_resaves_without_the_key`)
  plus the app `sync_carry_round_trips_through_the_project_file`.

## Deferred (unchanged from the claw report)

- Per-set stored blocks driving manual Bake / Bake all / auto-rebake
  (needs one BakeRequest per block).
- Per-UV-tile bake exclusion. Group rename UI.
