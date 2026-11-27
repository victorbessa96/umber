# D3-D5 WARP worker crash isolation

- Lock: `razul-d3d5-warp-crash`.
- Brief: `4bb6adc65763` (`docs(bake): D3-D5 WARP crash isolation brief`).
- Outcome: worker composition is isolated into three minimal real-output regressions.

## Evidence so far

- `45999725905`: exact D3-D5 worker test fails on Windows with `0xc0000409`.
- `45995180806`: mapped-ray test passes on Windows and Ubuntu.
- `46006540672`: direct multi-map GroundPlane passes on Windows.
- `46010462534`: direct cavity opacity passes on Windows.
- The full serial run `45990982180` also reaches `d3_d5_maps_execute_through_app_worker` and crashes there, so this is not the mapped-ray test.

Three new tests run GroundPlane only, Manual Opacity only, and IdMulti with an IdMap source through the real `run_bake` worker and decode each output. All three pass locally and strict app clippy is clean. Their independent Windows results are pending; do not claim a production root cause or fix yet.

## Verification

- Focused `cargo test -p umber-app worker_ -- --test-threads=1`: **7 passed, 0 failed** (three new plus four existing worker tests).
- `cargo clippy -p umber-app --all-targets -- -D warnings`: clean.
- Rustfmt clean.
- Independent review: pending.
- Commit/push/current-tip CI: not yet claimed.

## Non-goals

No suspect skipped, no weakened D3-D5 oracle, no release/tag/upstream PR, no centralhost work.
