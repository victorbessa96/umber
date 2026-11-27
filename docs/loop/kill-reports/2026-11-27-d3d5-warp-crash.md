# D3-D5 WARP GroundPlane readback

- Lock: `razul-d3d5-warp-crash`.
- Brief: `4bb6adc65763` (`docs(bake): D3-D5 WARP crash isolation brief`).
- Outcome: the Windows abort is root-caused and the readback is repaired without skipping or weakening the oracle.

## RED evidence and root cause

- `45999725905`: the original D3-D5 worker oracle fails with `0xc0000409`.
- `46015829454`: the minimal `worker_ground_plane_only_smoke` reproduces the same abort.
- `46020819953`: optional no-capture output proves the first panic is `poll timeout: Timeout` at `ground_plane.rs:787`, followed during unwinding by `sender.send(r).unwrap()` at line 781. The double panic aborts the process.
- Correction: `46006540672` was **not** direct GroundPlane evidence. Its Windows feature-gated family ran **0 tests**. The no-capture worker run is the authoritative RED.
- `46010462534`: direct cavity opacity passes, keeping the hunt on GroundPlane rather than blaming every special map.

GroundPlane used a hard ten-second `Device::poll` timeout and panic-based map handling. WARP exceeded that timeout on a small real bake. The callback then panicked while the first panic was already unwinding.

## Fix

`read_back_rgba8` now follows the position baker's established contract:

- no artificial poll timeout;
- typed `GroundPlaneBakeError::Readback` errors for poll, callback, map, and mapped-range failures;
- a non-panicking map callback;
- 256-byte GPU copy-row alignment with tightly packed CPU output;
- readback bands bounded by `device.limits().max_buffer_size`;
- explicit buffer unmapping between bands.

A pure regression covers row alignment and band sizing. Three real worker regressions cover GroundPlane, Manual Opacity, and IdMulti independently; the original D3-D5 worker oracle remains intact. Temporary stage instrumentation was removed. `c9adafab` retains the optional Windows no-capture CI diagnostic.

## Local verification

- `cargo test -p umber-bake --features gpu -- --test-threads=1`: **215 passed, 0 failed**.
- GroundPlane-only subset: **16 passed, 0 failed**.
- `worker_ground_plane_only_smoke`: pass.
- Original `d3_d5_maps_execute_through_app_worker`: pass.
- Clean-rebuilt `cargo test -p umber-app -- --test-threads=1`: **930 bin tests plus all integration targets passed, 0 failed**.
- Standard and `gpu`-feature clippy for `umber-bake`/`umber-app`: clean.
- Rustfmt, scorecard (**3,334**), layers, and parity: clean.
- `safe-push` cache contamination found during verification was fixed separately at `59849861`; its regression suite is **18/18**.
- Independent review was not obtained for this final readback patch. Do not relabel the older unrelated GPU-parity SHIP as review of this fix.
- Windows RED-to-GREEN confirmation and current-tip CI: pending.

## Non-goals

No suspect skipped, no weakened D3-D5 oracle, no release/tag/upstream PR, no centralhost work.
