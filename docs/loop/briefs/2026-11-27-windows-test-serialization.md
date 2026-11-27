# Kill brief: Windows test serialization and filtered diagnosis

- Lock: `razul-windows-test-serialization`.
- Trigger: CI `45949747730` failed twice at the same point with `0xc0000409 (STATUS_STACK_BUFFER_OVERRUN)` after `log_entry_format_and_jump_to_setting`, immediately before the mapped-ray WARP test. The stall repeats at ~24 seconds.

## Required changes

- Run Windows workspace tests with `--test-threads=1`. WARP/native GPU paths must not share the process across parallel test threads; this also makes the next test in the log deterministic.
- Add an optional `workflow_dispatch` `test_filter` string used only by the Windows test step. Empty keeps the normal full suite; a filter allows the exact suspect to run without another full-suite wait.
- Preserve the four established WARP-slow skips and every other platform/test leg unchanged.

## Verification and gates

Parse the workflow and assert the filter wiring, serialization, and unchanged skips. Dispatch first `mapped_ray_maps_raster_the_submesh_but_occlude_cross_material`, then the occluder kittest if needed. A filtered crash identifies a real code defect; filtered passes followed by a full serial pass identify the parallel-WARP failure mode. No skip of the suspect, no release/tag/upstream PR, no centralhost.
