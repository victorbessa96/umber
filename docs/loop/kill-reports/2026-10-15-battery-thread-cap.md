# Kill report — battery test-thread cap (build-host infrastructure)

Date: 2026-10-15. Author: dragon. Lock: rides `brush-wiring`
(`scripts/wave-battery.sh` added to that lock's file list), this report.
Found during the brush-wiring battery on the build host.

## The finding

The first brush-wiring battery on centralhost (Ryzen 7 5800X, 16
threads, RTX 3070, NVIDIA 615.71) hung: 16 umber-paint tests "running
for over 60 seconds", the suite never returning. Bisecting at the parent
commit (379ddb9, before any brush-wiring change), on a clean clone:

| test threads | umber-paint lib suite |
|---|---|
| 1 | 154 passed in ~18 s, every run |
| 4 | passed once (12.94 s), then hung 3/3 at the parent commit 379ddb9 |
| 8 | hung, killed at 120 s |
| 12 | hung, killed at 120 s |
| default (16, one per CPU) | hung |

The umber-app snapshot suite showed it too: two kittest tests hung at
"running for over 60 seconds" inside a battery, then passed in 0.37 s
alone. Thread states were `futex_do_wait` / `rt_mutex_schedule` with the
NVIDIA Vulkan worker threads (`[vkcf]`, `[vkrt]`, `[vkps]`) parked. ptrace
is restricted on the host (`yama/ptrace_scope=1`), so no backtrace.

So it is not a code regression. Each GPU test builds its own wgpu device,
and past a handful at once the NVIDIA driver deadlocks. libtest's default
of one thread per CPU means the battery's verdict silently depended on
the host's core count. The laptop's 8 threads and CI's software adapters
(lavapipe / WARP) never crossed the line.

## Built

`scripts/wave-battery.sh`:
- runs the GPU crates (`umber-gpu umber-paint umber-app umber-bake
  umber-cli umber-gui`, every crate that requests a wgpu device in its
  tests) single-threaded: `-- --test-threads=$UMBER_GPU_TEST_THREADS`,
  default 1. Other crates keep libtest's default, or
  `UMBER_TEST_THREADS` if set;
- bounds each suite with `timeout $UMBER_SUITE_TIMEOUT` (default 1200 s);
- reports a timeout as `FAIL test <crate> (TIMEOUT after Ns at T threads
  …)`, so a hang is a loud, named failure instead of a battery that never
  returns.

## Judgment calls

1. **One thread, not "a few".** 4 passed once, then hung on repeat: a
   probabilistic deadlock, so any cap above 1 only lowers the odds. 1 is
   deterministic, and costs ~5 s on the umber-paint suite.
2. **The cap is in the battery, not in each GPU test.** A per-test mutex
   around device creation would also work, but it changes every GPU
   test's code. The cap is one line, reversible, overridable, and matches
   how the suite is meant to run on a real GPU.
3. **CI unchanged.** CI's adapters are software and do not deadlock; its
   timings are tuned. If a future GPU runner joins CI, it needs the same
   cap.

## Unverified

- Whether the deadlock is the driver's or wgpu's: not chased (out of
  scope). The cap is the working mitigation; a minimal reproducer for an
  upstream report is a follow-up if it recurs at low thread counts.
