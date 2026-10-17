# Coordination note — the host's GPU tests: WEDGED on default backend, GREEN single-threaded on Vulkan (RESOLVED)

Written 2026-10-17 ~16:30, resolved ~16:50, by the loop session.

## What happened

Your `--features gpu` verification of with_occluder wedged, and the
cause is now pinned:

1. At ~15:00 THREE concurrent copies of `cargo test -p umber-bake
   --features gpu --lib` sat futex-deadlocked (75+ min, ~40s CPU
   each, zero progress). Killed.
2. ONE clean copy at 16:20 also wedged: 16 GPU tests stuck past 60s
   each on the free RTX 3070 — parallel test threads racing wgpu
   instance/adapter creation on the default backend.
3. THE FIX: `WGPU_BACKEND=vulkan` + `--test-threads=1`. Verified
   twice: the single failing-named test passes in 0.26s, then the
   FULL GPU suite: **150/150 passed, 0 failed, 9.73s**
   (/tmp/ao-gpu-vulkan.log, EXIT=0). The host's real-GPU row is
   GREEN — with_occluder is verified on real hardware, not just
   lavapipe/WARP.

## The command (for any future host GPU run)

    cd ~/Projects/umber && WGPU_BACKEND=vulkan cargo test \
      -p umber-bake --features gpu --lib -- --test-threads=1

Parallel threads + default backend = the wedge. Single-threaded
Vulkan = green. (CI's lavapipe/WARP runs never hit this because CI
sets its own env.)

## Standing suggestion

The wave-battery's GPU legs on this host should carry
`WGPU_BACKEND=vulkan --test-threads=1` for the `--features gpu`
suites — the plain `cargo test` fan-out is the trap. Worth a
scripts/wave-battery.sh tweak in the next docs wave.
