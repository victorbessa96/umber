# Windows test serialization and filtered diagnosis

- Lock: `razul-windows-test-serialization`.
- Brief: `25df19826fda` (`docs(ci): Windows test serialization brief`).
- Outcome: Windows tests are serial, with an optional dispatch-only filter for exact diagnosis.

## Trigger and change

CI `45949747730` failed twice at the same ~24-second stall with `0xc0000409 (STATUS_STACK_BUFFER_OVERRUN)` after `log_entry_format_and_jump_to_setting`, immediately before the mapped-ray WARP test.

The Windows step now uses `--test-threads=1`. A `workflow_dispatch` `test_filter` input can name one suspect; empty preserves the full suite. The four established WARP-slow skips and every other test leg are unchanged.

## Verification

- PyYAML and static assertions: filter wiring, serialization, and all four skips present; Linux command unchanged. The filter is passed as the quoted `TEST_FILTER` environment variable, closing the review's shell-hardening note; `cargo test -p umber-gui "" -- --list` listed the full suite, confirming empty-filter behavior.
- Filtered mapped-ray dispatch: pending push.
- Occluder dispatch (if needed): pending.
- Independent review `deleg_d8c14a1a`: **SHIP**. Empty filter preserves the full release gate; Linux retains parallel coverage. Informational shell-hardening and parallelism-masking notes are not gating defects.
- Commit/push/current-tip CI: not yet claimed.

## Non-goals

No suspect skipped, no release/tag/upstream PR, no centralhost work.
