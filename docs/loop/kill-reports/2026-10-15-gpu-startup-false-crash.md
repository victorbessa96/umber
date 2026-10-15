# Kill report — gpu-startup false crash (regression fix)

Date: 2026-10-15. Author: dragon. Lock: `gpu-false-crash` over
`crates/umber-app/src/gpu_startup.rs`, `main.rs` (the `run_native` app
closure) and this report. Fixes a defect in `wave-gpu-safe-start`
(`docs/loop/kill-reports/2026-10-14-gpu-safe-start.md`).

## The incident (Bessa's report, centralhost, KDE Wayland, RTX 3070)

```
gpu startup: the previous launch died inside the GPU driver on vulkan
  (NVIDIA GeForce RTX 3070); falling back to gl
The only available wgpu adapter was not suitable: backend: Gl …
Exiting because of error: … gl not compatible with provided surface
```

The chain had two defects. The dragon's own smoke test triggered it.

1. **A false "driver death".** The release smoke ran `timeout 8 umber`
   over ssh. The start was confirmed on the 3rd frame, but that launch
   never got there: the window did not map in that session, so frames
   never came. `timeout` killed it with the marker still on disk. The
   next launch read that as a Vulkan driver crash. The marker could not
   tell the difference: it recorded the plan and the adapter, but not
   whether the device had already been created.
2. **The fallback was a dead end, and a sticky one.** GL on that NVIDIA
   stack cannot present to a Wayland surface. `run_native` returned
   `Err`, and `finish(false)` KEPT the marker (by design: an error was
   meant to mean "the driver failed this backend"). So every later
   launch retried GL and died the same way. A bricked desktop install.

Immediate remediation on centralhost (before the code fix): the stale
marker was backed up to `~/umber-gpu-state-backup/` and removed. No
`gpu-backend` persistence existed (the GL start never confirmed).

## Built

- **`Stage` in the marker** (`planned` → `adapter` → `device`).
  - `note_adapter` writes `adapter`. The new `note_device()`, called from
    the `run_native` app-creator closure (after eframe created the
    device, before `UmberApp::new`), writes `device`.
  - `Marker::tried` returns `None` past device creation. A start that
    died at `device` survived the driver, so it never steps the backend
    down.
  - Markers written before this field existed infer the stage: an
    adapter line means `adapter`, else `planned`.
  - The format stays `v1`. `stage=` is an added key, and unknown keys
    were already tolerated.
- **`exit_action(clean, stage, fallback)`**, a pure, table-tested exit
  rule:
  - A clean exit clears the marker.
  - An error at `device` is the app's own error: clear.
  - An error at `adapter` is a failed device request: keep the marker, so
    the next launch steps down.
  - An error at `planned` on the preferred backend: keep it. The default
    stack is broken, so try the safer rung.
  - An error at `planned` on a FALLBACK rung (stepped down, remembered or
    exhausted) means the fallback found no adapter that can present here.
    **Undo it:** clear the marker AND remove the persisted fallback, and
    log it, so the next launch is back on auto.
- Tests, 4 new; every existing table still passes:
  - `a_start_killed_after_device_creation_never_steps_down`, the
    regression itself;
  - `exit_rule_blames_the_backend_only_for_driver_failures`;
  - `a_dead_fallback_is_undone_so_the_next_launch_is_auto`, the exact
    remote failure end to end through a real marker dir;
  - `stage_round_trips_and_old_markers_infer_it`.

## Judgment calls

1. **The stage boundary is the app-creator closure, not the first frame.**
   Once eframe calls the closure, the instance, adapter, device and
   surface are configured; a driver that crashes on the first present
   would still be caught only by `adapter`-stage markers. The first
   present can also crash a driver, which a `device`-stage marker would
   now miss. That trade is deliberate: a killed or headless launch is
   far more common than a driver dying on present, and false positives
   brick installs. The narrow window is named here and not hidden.
2. **No auto-retry of the preferred backend within one launch.** egui's
   `run_native` builds the instance once. Retrying inside a launch needs
   a second `run_native`, which eframe does not support on all platforms.
   The undo makes the NEXT launch correct.

## Process note

- The smoke-test method itself is fixed in the umber-wave-ops skill: GUI
  smokes run with the real display env and wait for the confirm log line,
  never a bare `timeout` against a window that may not map.
