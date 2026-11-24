# XI2 hierarchy hotplug re-query

- Date: 2026-11-24.
- Author: Razul (dragon), session `local-f150e2eb08b49cfc70e9c18de051af45f6e45accdfc10fe757ff67a5476d841d`.
- Brief: `docs/loop/briefs/2026-11-24-umber-xi2-hotplug.md` (`2c1b9ca7aaa8`).
- Claim: `1bd4c1efae12`, queue row 592.
- Implementation: `5d874bfbcce3a00878408e0113c427ee574bb1a7`.
- Final pre-commit source-diff SHA-256: `ca274b14242d731d3d7e0d207173e9e1a38a7aa5488a8c2e64ffc454e89d06d3`.
- Kill lock: `razul-xi2-hotplug`.

## Built

- `select_root` now selects `XI_HierarchyChanged` alongside Motion, ButtonPress/Release, Enter, and Leave.
- A hierarchy cookie calls `refresh_devices`, which runs one fresh `XIQueryDevice` pass and emits no `SourcedEvent` for the notification itself.
- `scan_devices_into` builds a fresh result; `apply_refresh` swaps mappings and eraser set only on success. A later `QueryFailed` logs a warning and preserves the previous known-good state. A subsequent successful refresh replaces it again.
- The initial `open` query still returns `Xi2Error::QueryFailed` on failure. No second connection, worker, or real-tablet claim was added.
- The module docs now describe live hierarchy refresh and the nonfatal-failure rule.

## Review and dispositions

- Independent review `deleg_b52f29bb`: **SHIP**, zero findings. It independently confirmed refresh builds fresh mappings/erasers before mutation, success replaces both, failure preserves known-good state, `XI_HierarchyChanged` is selected and handled before other payload mapping, and no `SourcedEvent` is emitted. Initial open errors, the 256-event drain bound, and the XFreeEventData path remain intact.

## Verification

- RED: `hierarchy_refresh_swaps_preserves_and_swaps_again` failed against the `apply_refresh` stub. The default feature-off test target ran zero tests and was discarded as vacuous; `cargo test -p stylus --features xi2` reproduced the real RED.
- GREEN: full `stylus --features xi2` suite **23/23**, including successful swap, failure preservation, and a second successful replacement.
- Strict `cargo clippy -p stylus --all-targets --features xi2 -- -D warnings` → exit 0.
- Default `cargo test -p stylus` → **7/7**, confirming the feature-off suite is unaffected.
- Full `cargo test -p umber-app` with `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`: **1,277 passed, 0 failed across 11 suite rows**: 323 lib + 907 bin + 2 filter-apply + 1 Gate0 + 3 keyboard-menu + 17 panel-kittest + 11 snapshots + 10 panic-hunt + 2 pool-perf + 1 set-switch + 0 doc-tests.
- `~/.cargo/bin/rustfmt --edition 2021 --check crates/stylus/src/xi2.rs` → exit 0.
- `python3 scripts/check-layers.py` → `=== layers: GREEN (16 crates) ===`, exit 0.
- `python3 scripts/scorecard.py --check` → `scorecard: README.md current`, exit 0.
- `git diff --check` → clean.
- A fresh OS-safe `hermes-verify-xi2-hotplug-*.sh` probe reran the full xi2 suite: **23/23**, `ADHOC-VERIFY: PASS`, script deleted and verified absent. This is focused ad-hoc evidence in addition to the app suite, not a workspace-green claim.

## Judgment calls

- Refresh failure preserves the previous maps rather than erasing them: an old working tablet keeps its axes while a transient X server query failure is logged.
- The hierarchy notification itself is not a tablet event, so no `SourcedEvent` is emitted. The 256-event drain bound and the existing XFreeEventData path are unchanged.
- No real-tablet or X server integration claim is made; the deterministic semantics are tested without an X server, while the production path uses the existing display and `XIQueryDevice` contract.

## Unverified

- No physical hotplug was performed. The real X server cookie, axis-label scan, and hotplug timing remain unverified on hardware, as the module docs state. No Windows run, full workspace test, or fresh CI dispatch was performed. Per-push CI is disabled by design; safe-push supplies the affected-crate gate.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- Next independent kill: the bounded vector/SVG slice and control-channel smoke tail.
- Real-tablet verification and winit display sharing remain separate follow-ups.
