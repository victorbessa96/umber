# 2026-11-08 — perf-nightly red 42468823675 verdict: FLEET, not code

- **Author:** dragon (research/verdict kill, docs-only).
- **Question:** scheduled perf-nightly run 42468823675 (tip `3d638e31`,
  2026-11-08 09:49Z) failed the A/B ratio gate vs the seed `0955ac8`:
  viewport_mean x1.100, viewport_p95 x1.371 (band 1.10). Green the day
  before: 42360362538 on `ac27a34c`.

## Leg 1 — the bench-visible code delta is nil on the runtime path

`git diff ac27a34c..3d638e31` over the ref-scene bench's dependency graph
(umber-gpu + its deps umber-core/umber-mesh/umber-graph + profiling):

- `crates/umber-mesh/src/uv_gbuffer.rs`: PURELY ADDITIVE (zero removed
  lines — `world_at_uv` + re-export only). The bench's mesh pass never
  calls it.
- `crates/umber-core/src/{svg.rs, bundled.rs}`: new modules; `ref_scene.rs`
  ("production viewport frame minus egui") references neither. umber-gpu
  gained an `umber-core` edge in-window (Cargo.lock +1) but calls nothing
  from the new modules.
- umber-gpu itself: untouched in the window.

## Leg 2 — manual canary runs were infra-red, not gate-red

Both workflow_dispatch canaries (`ref=3d638e31` run 42502607457,
`ref=ac27a34c` run 42513782719) FAILED AT CHECKOUT: actions/checkout fetches
`refs/heads/<sha>` and a bare short SHA is not fetchable. No bench ran.
(The workflow comment's `ref=<sha>` canary path needs a FULL sha AND a
reachable-sha1-enabled fetch; recorded here so the next session does not
re-read it as a gate verdict.)

## Leg 3 — local same-machine A/B (this host, Intel HD 530 via hasvk)

`scripts/perf_gate.sh` could not run locally: /tmp tmpfs was 100% full
(7.8G held by deleted files of root-owned processes — baloo_file at 2.9G
RSS is the prime suspect — not reclaimable agent-side), and its hardcoded
/tmp paths (AB_DIR/AB_TARGET/JSONs) hit ENOSPC. A manual A/B with all
artifacts under `~/.hermes/cache/scratch` (local-ab-manual.sh) built both
trees and completed pass pair 1 before the harness reaped the background
job; the pair is complete for the viewport bench:

| pass | tip | mean_ms | p95_ms |
| --- | --- | --- | --- |
| cur-r1 | current (ea664fe4 lineage) | 4.3185 | 6.1002 |
| abref-r1 | `ac27a34c` (last green nightly tip) | 4.2767 | 6.2753 |
| ratio cur/ref | | **x1.010** | **x0.972** |

Same machine, same adapter, back-to-back: the current tip is WITHIN band
(mean x1.010 < 1.10; p95 actually faster x0.972) against the last-green
tip. One pair only (the job died mid-run), but it is the only same-machine
reading available and it shows no code regression.

## Verdict

FLEET NOISE, same class as the 2026-10-30 verdict (hosted lavapipe
throughput moves x1.16..x1.51 day to day on identical code; the absolute
band vs the 2026-10-19 seed cannot resolve that). The code delta between
the last green and first red nightly tip has zero runtime effect on the
bench path (Leg 1) and measures within band on a fixed machine (Leg 3).
No code change lands with this hunt. Confirmation reading: the
2026-11-09 scheduled nightly on the current tip; if it reds again on the
SAME fleet day as a green same-tip reading, re-open with a full local
3-pair A/B (scratch paths, not /tmp).

**Correction (2026-11-09):** this report first named the confirmation
reading as "the 2026-11-09 **03:00Z** scheduled nightly". The workflow's
cron IS `0 3 * * *`, but GitHub dispatches it 6-7 hours late on this
repo — nine consecutive schedule runs fired between 09:15Z and 10:29Z
(10-31 through 11-08). The 11-09 nightly therefore appears between
roughly 09:00Z and 10:30Z, not at 03:00Z, and it measures the tip as of
its FIRE time, not as of 03:00Z. Do not read the absence of a 03:00Z run
as a skipped nightly. Evidence: vault
`projects/umber/2026-11-09-perf-nightly-fire-time.md`.

## Process notes

- /tmp tmpfs fills from sibling litter + deleted root-process files; check
  `df -h /tmp` before any /tmp-writing gate script (perf_gate.sh included).
- perf_gate.sh's PERF_AB_REF fetch needs the FULL 40-char sha, not short.
