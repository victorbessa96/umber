# Design brief: umber-picker-delegate (L7, 2026-11-23)

Planned kill lock: `razul-picker-delegate`.
Base: K8i status tip `7f0c976bdedf923cb671f9372624c25034063bf3`.

## What exists

`crates/umber-gui/src/widgets/color_picker.rs:664-726` carries a complete stock `color_button` popup implementation. `color_button_with` at `:745-835` duplicates that implementation and adds caller-supplied `PickerLabels` plus `RecentColors`. G1 (`21ada34`) and A3 (`5b38c96`) landed long ago; the three app sites use `preferences::picker_button` and `color_button_with`, while the stock button remains consumed by its own kittests. The L7 contract in `docs/loop/next-wave-plan-draft.md:116-120` and the readiness audit in `docs/loop/reviews/2026-11-17-deferred-readiness.md:59-71` therefore remain open: make the stock button a thin delegate instead of maintaining two popup implementations.

## Target

Replace only the body of `color_button` with a call to `color_button_with` using `PickerLabels::default()` and a fresh, per-call empty `RecentColors`. The recents list is deliberately throwaway: it is empty while the popup is open, receives at most the close-time entry, and is dropped when the call returns, so no recents grid or persistence becomes visible to stock callers.

Preserve the public signature and the existing stock behavior: a 40x18 swatch, linear-RGB writeback, English glyph labels, popup state in egui memory, the shared slider picker, outside-click/Escape close behavior, AccessKit button role, and focus ring. Remove the duplicated popup body rather than adding a second compatibility path. Update the module and function docs to describe the delegation and per-call recents lifetime accurately.

This is a behavior-preserving refactor, not a feature addition. No new test is pinned merely to prove source shape. The existing stock-button kittests are the acceptance surface and run before and after the edit; the complete `umber-gui` suite is the regression gate.

## Concurrency and safety

Locked surface: `crates/umber-gui/src/widgets/color_picker.rs`, this brief, `docs/loop/kill-reports/2026-11-23-umber-picker-delegate.md`, `docs/loop/next-steps-2026-11-23.md`, `docs/loop/queue.md`, `.dragon/COORDINATION-next-wave.md`, `STATE.md`, `docs/ROADMAP.md`, and `docs/loop/waves/2026-11-09-wave-plan.md`.

No app, schema, dependency, i18n, persistence, release, or public-PR change is included. Any foreign dirty file, live sibling lock, origin movement, disk I/O error, or unexpected behavior delta stops the lane for re-census.

## Verification

- Run the stock-button acceptance tests at the clean base, then rerun them after the delegation: `cargo test -p umber-gui color_button -- --nocapture`. Require real nonzero passed counts and zero failures.
- Run `~/.cargo/bin/rustfmt --edition 2021 --check crates/umber-gui/src/widgets/color_picker.rs`, `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check`, and `git diff --check`.
- Run strict `cargo clippy -p umber-gui --all-targets -- -D warnings` and full `cargo test -p umber-gui` separately under `/tmp/umber-cargo.lock` with `CARGO_BUILD_JOBS=3`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_PROFILE_DEV_DEBUG=0`, and `nice -n 19`.
- Obtain an independent read-only review of the source diff. Check specifically that recents cannot leak across stock calls, close-time state handling matches the old body, and no app-visible persistence or label change was introduced.
- Record the kill and refreshed remaining-work census, commit path-limited through dcommit, run `scripts/safe-push.py` bare, and independently verify `git ls-remote origin master`. Per-push CI is disabled by design; safe-push supplies the affected-crate gate, and no CI run will be fabricated.

## Non-goals

No changes to `color_button_with`, app call sites, recents persistence, translations, widget layout, color math, GUI themes, dependencies, release artifacts, version numbers, or unrelated vector/SVG work. No second kill rides this wave.
