# Kill report — the swatches test-isolation race (+ the segv's true cause, reconciled)

Date: 2026-10-23. Author: the dragon (local-7de7d3ed). Reconciled with
the parallel wake's runner-image pin (`c697bab`, same date) after both
sessions hunted the same CI red from two ends.

## The CI red's true cause (the parallel wake's pin, adopted)

The ubuntu leg's SIGSEGV was ENVIRONMENT-caused: `ubuntu-latest`
rotated 24.04 → 26.04 (resolute, mesa 26.0.8 lavapipe) on 2026-10-21
and that stack crashes the umber bin test binary mid-suite — same
binary hash (`umber-962ed9b619a5dbb9`) on both 10-23 tips, crash site
wanders (brush_panel in run 39903281308, bakes_panel in 39908194347),
green through 10-20 on 24.04 + mesa 25.2.8 with zero code change, and
the full local lavapipe run passes 635/635 (the tower runs the same
25.2.8 family). No code-side crash exists to name. The fix: pin the
check matrix's linux leg to ubuntu-24.04 (`c697bab`) + the Flatpak
job's runner pin (`f62d112`) — both ride the W-C3b close's push.
Full evidence: `2026-10-23-ci-ubuntu-image-pin.md`.

My session's earlier single-threading edit to the CI workflow was a
different mitigation aimed at the GPU-concurrency class; with the
image rotation root-caused and pinned it is REDUNDANT for this red
and slows every linux leg — REVERTED, uncommitted. The
GPU-crates-single-threaded discipline stays where it is proven needed:
the LOCAL wave-battery (the 2026-10-15 NVIDIA deadlock class).

## The surviving kill: the swatches test-isolation race

`preferences::swatches_tests::unique_dir()` (landed with the A3 row,
`5b38c96`) keyed its per-test tempdir on `line!()` — evaluated inside
the helper's own body, a CONSTANT for every caller, so all four
swatches tests shared one directory and raced. Proven live: the local
PARALLEL lavapipe run failed `garbage_lines_are_skipped` (634 passed,
1 failed — the parse read [] instead of the three colors) while every
single-threaded gate passed it.

Fix: an atomic per-CALL counter (`process::id()` + fetch_add) instead
of the constant `line!()`, with the macro-placement lesson in the
comment (`crates/umber-app/src/preferences.rs`).

## Verification

- The definitive parallel repro (lvp-pinned, default threads, the bin
  suite): FAILED 634/1 before the fix → PASSED 635/635 after
  (7.60 s) — the race fix proven at full concurrency, twice the
  evidence any single-threaded gate can give.
- fmt clean on the touched file; clippy umber-app PASS; the surgical
  battery (test -p umber-app, single-threaded per the GPU list,
  debug-stripped profile named in the battery log) gates the commit.
- This fix rides the W-C3b close's push alongside the runner pins.

## Lessons

- `line!()` inside a helper body is the caller-invariant constant —
  per-CALL uniqueness needs the counter at the call site or a
  runtime atomic. The parallel-tests lesson's first draft had the
  same trap it was written to prevent.
- Two sessions can hunt one red from two ends and both be right
  about different defects: the red had TWO independent causes (the
  image rotation's segv + this race). The reconciled record carries
  both.
- Verify tool timeouts against the WORK the env change creates: the
  debug-stripped profile re-fingerprinted the entire dependency tree
  and the 25-minute leg budget died on a ~40-minute serial rebuild
  (the battery's fifth death this arc — a timeout kill, not oomd;
  the re-fire with the doubled budget resumes the warm cache).
