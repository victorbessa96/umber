# Design brief: late material-picker acceptance coverage (2026-11-25)

Planned kill lock: `razul-material-picker-coverage`.
Base: final code-ready status `857886f57633895affb510b60022c36a0054cd9f`, clean and origin-equal.

## What exists

The all-channel picker and exclusive Ctrl+Alt handling are landed and independently reviewed. The late R1 review `deleg_e9845b8b` identified a real remaining acceptance gap: the five current tests do not pin active-mode non-default projection/mask preservation, non-fill/no-selection no-op behavior, or exclusion of a declared-but-unmaterialized channel.

## Target

Add three tests in the existing `material_pick_tests` module:

- Active mode preserves a selected fill's non-default projection, mask, and target channel while replacing only its sampled Solid.
- Active mode returns `Ignored` with no selection and with a non-fill selection, leaving layers/history unchanged.
- All-channel mode excludes a declared-but-unmaterialized channel. A `resize_channel` size override records the declaration without allocating a plane; the picker must not create it or a default fill for it.

## Failure handling

No production behavior changes. Do not weaken the existing five tests or the exclusive-gesture assertions. If a test fails, fix the fixture or production bug it proves rather than dropping the assertion.

## Verification and acceptance

- `cargo test -p umber-app material_pick` passes **8/8**.
- Strict `cargo clippy -p umber-app --all-targets -- -D warnings`, fmt, layers, scorecard, parity, and `git diff --check`.
- Independent read-only review, report, safe-push, and `git ls-remote origin master`.

## Non-goals

No picker feature changes, no new UI, and no new production surface beyond the acceptance tests. This closes the late R1 acceptance gap; final code-ready status follows the review.
