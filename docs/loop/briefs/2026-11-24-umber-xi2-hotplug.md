# Design brief: XI2 hierarchy hotplug re-query (2026-11-24)

Planned kill lock: `razul-xi2-hotplug`.
Base: OccluderScope kittest status tip `5817b6cd8747bf052d7eba4a1b63e5cbd57dbd56`.

## What exists

`Xi2Reader::open` snapshots `mappings` and `erasers` once through `XIQueryDevice(XIAllDevices)`, then `select_root` selects Motion, ButtonPress/Release, Enter, and Leave. `drain` ignores every other cookie type. The module docs explicitly record the hotplug gap: a tablet plugged in later strokes at pressure 1.0 with no tilt until restart.

`x11-dl-2.21.0/src/xinput2.rs` provides `XI_HierarchyChanged` and `XI_HierarchyChangedMask`. The same display and `XIQueryDevice` contract already used at open can be reused for refresh, without a second connection or a new worker thread.

## Target

- Select `XI_HierarchyChanged` alongside the existing root-window events.
- On a hierarchy cookie, run one fresh `XIQueryDevice` pass and rebuild the pressure/tilt mappings and eraser set. Return no `SourcedEvent` for the hierarchy notification itself.
- Refresh atomically: build the next maps/sets first, then swap them only on success. A failed re-query logs a warning and preserves the previous known-good state instead of erasing it. The next successful refresh replaces it again.
- Keep the existing open-time behavior and failure contracts unchanged. `open` still fails through `Xi2Error` on its initial query; later refresh failures are nonfatal and preserve state.
- Keep the 256-event drain bound and the existing XFreeEventData path.
- Update the module docs to describe live hierarchy refresh and the nonfatal-failure rule.

## Failure handling

An initial `XIQueryDevice` failure remains `Xi2Error::QueryFailed` during `open`. A later failure is logged as a hierarchy-refresh warning and leaves the previous maps/erasers intact. A newly connected tablet still uses the existing full-pressure fallback until a successful query resolves its axes. No event is emitted for the refresh itself.

## Tests

- Pure deterministic `apply_refresh` tests: successful refresh swaps both maps and eraser set; a later `QueryFailed` preserves the previous maps and eraser set; a subsequent successful refresh swaps again. No X server is required for this semantic coverage.
- The production path imports and selects `XI_HierarchyChanged`, handles the hierarchy cookie before other mapping, and performs a real `XIQueryDevice` refresh through the same display used at open.
- RED first: prove the refresh semantics are absent, then implement the atomic swap and hierarchy wiring.
- Strict clippy on `stylus` and full `cargo test -p stylus` with the slow-build environment. `xi2` is optional on Linux; the existing unit-test target runs the mapping layer. No new dependency, icon, schema, or release action.

## Concurrency and safety

Locked surface: `crates/stylus/src/xi2.rs`, this brief/report, `docs/ROADMAP.md`, `docs/loop/waves/2026-11-09-wave-plan.md`, and the current status/queue surfaces. No sibling lock or other source surface is touched. Any foreign dirty file, live sibling lock, origin movement, disk I/O error, or unexpected behavior delta stops the lane for re-census.

## Verification and acceptance

- RED first: run the refresh tests against an `apply_refresh` stub and capture the missing behavior.
- Run exact refresh tests with nonzero passed counts, then strict `cargo clippy -p stylus --all-targets -- -D warnings` and full `cargo test -p stylus` separately under `/tmp/umber-cargo.lock` with the slow-build environment.
- Run rustfmt, `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check`, and `git diff --check`.
- Obtain independent review of hierarchy selection, event handling, atomic replacement, and failure preservation.
- Record the kill and update the current remaining-work order, commit path-limited through dcommit, run `scripts/safe-push.py` bare, and independently verify `git ls-remote origin master`.

## Non-goals

No real-tablet claim, X server fixture, winit display sharing, new event type, or second-kill scope. The bounded vector/SVG slice and control-channel smoke tail remain later independent work.
