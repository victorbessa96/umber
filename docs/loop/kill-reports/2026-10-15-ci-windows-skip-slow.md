# Kill report: Windows CI skips the three WARP-slow GPU tests

Date: 2026-10-15. Author: dragon. Lock: `ci-windows-skip-slow` over
`.github/workflows/ci.yml` and this report.

## Measured (run 38797679021, Windows job, 52 min total)

| step | time |
|---|---|
| rust-cache restore | 1 m 10 s |
| clippy | 1 m 04 s |
| **tests** | **41 m 11 s** |
| release smoke | 7 m 42 s |
| rust-cache save (post) | 1 s |

This corrects the earlier claim that about 10 minutes went to saving the
cache: on this run the save takes one second. The time goes to GPU tests
on WARP (software D3D12). Three tests account for about 24 of the 41
minutes:

- `pool_drag_session` (`umber-app/tests/pool_perf.rs`): 13.8 min, against
  29 s on Linux.
- `adjacent_margin_bakes_every_map_and_differs_from_extend` and
  `agent_bake_margin_adjacent_differs_and_absent_is_extend`
  (`umber-cli/tests/margin_cli.rs`): 10.3 min together.

## Change

- Windows runs `cargo test --workspace -- --skip <those three>`; Linux
  runs the whole suite unchanged.
- Every other GPU test, including the remaining `umber-bake` GPU module
  (~6.6 min on WARP), still exercises the D3D12 path on Windows.
- Checked on the build host: the skip flags filter exactly these tests
  (margin_cli 2 filtered, pool_perf 1 filtered).

Expected Windows job: about 28 min instead of 52. The next CI run gives
the measured number.

## Not changed

The release smoke (7.7 min) stays. It is the only per-push check that
the release profile builds on Windows.
