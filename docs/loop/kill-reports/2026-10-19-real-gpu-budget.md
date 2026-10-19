# Kill report: the real-GPU budget row — C2-16 closed (2026-10-19)

Dragon-authored by the Razul main session under Bessa's "continue
the long horizon, do not stop". Kill-lock `c2-16-real-gpu-budget`,
holding `docs/loop/queue.md` (the C2-13..19 row tick),
`docs/perf/protocol.md` (the hardware record), and this report.

## Context

The C2 audit's honest-claims row 16: the requirements §12 budgets
were enforced on CI's lavapipe (the regression instrument) but the
real-hardware record — "one named mid-range GPU class, current
driver" — was owed on Bessa's host. The queue row read "16 (real-GPU
budget row: Bessa's host) todo" since 2026-10-15; the host returned
2026-10-19 and the row's moment arrived.

## Method

Tip `dc4b355` exported CLEAN via `git archive` into an isolated
`~/Projects/umber-budget/` on centralhost — the committed tip, not
the working tree (a sibling session's in-flight bytes were never
measured). `WGPU_BACKEND=vulkan` (the verified config; the host's
3070 on its native backend), cold release build with `--features
perf`, the three §12 harnesses run sequentially exactly as the
nightly runs them. Hardware: NVIDIA GeForce RTX 3070 (8,192 MiB),
DiscreteGpu, Vulkan — at/above the §12 reference class.

## The numbers (RTX 3070, 2026-10-19)

- **Viewport** (ref_scene_bench: 1000 frames, 1,002,252 tris, the
  4K set at 268,435,456 bytes resident, 1080p target): mean
  0.1931ms / p50 0.1900 / **p95 0.2034** / p99 0.2081 — the bench's
  own verdict field: `budget_p95_ms: 16.7, within_budget: true`.
  Against the §12 target (60fps sustained, P95 < 16.7ms): **~82x
  inside the budget**.
- **Input-to-photon** (input_photon_session: 4,998 events): p50
  0.0877ms / **p95 10.0825ms** / p99 10.0894ms against
  `budget_ms: 20.0` — half the budget; the ~10ms tail is the 60Hz
  present cadence of the drain-events-then-acquire loop.
- **Stroke throughput** (stroke_soak: 200Hz logical on the
  4096x4096 target): `staged=2914 composited=2914 drops=0`
  (inter-dispatch mean 8.56ms), `test result: ok` — **zero dropped
  dabs**, exactly as §12 demands.

## Judgment calls

1. The 3070 exceeds the §12 reference class (RTX 3060 / RX 6600
   named as examples) — the numbers are an upper-bound proof: the
   budgets hold with 82x / 2x / zero-drop margins on hardware
   STRONGER than the floor the requirements name. The lavapipe CI
   gate remains the regression instrument; this record is the
   honest-claims data point, not a new gate.
2. The host's clock skew (~20 min behind, per the RTC/memtest
   evidence in the centralhost hunt) affects file mtimes only, not
   the measurements — the harness timing is monotonic-clock based.

## Gates run

Host battery-exempt (docs-only kill; no crate bytes touched). The
evidence is the harness outputs themselves: `/tmp/budget-ref.json`
(the bench's `within_budget: true` verdict field), the soak's
verbatim zero-drop line re-verified on the warm target, and the i2p
JSON. The queue row and the protocol note carry the numbers.

State: VERIFIED (real hardware, three budgets, verbatim evidence);
commits at this wave, rides the next push.
