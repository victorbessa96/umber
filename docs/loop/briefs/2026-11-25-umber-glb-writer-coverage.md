# Design brief: late GLB writer failure coverage (2026-11-25)

Planned kill lock: `razul-glb-writer-coverage`.
Base: final code-ready status `2c3cb1afa91e7003d21c49dc9c6885637f171376`, clean and origin-equal.

## What exists

The GLB exact-layout assertions and writer failure atomicity for invalid geometry with pre-existing directories are landed and reviewed. The detailed late P2 finding text exposes two remaining coverage gaps:

- The non-multiple case `[0, 1]` exits through empty-triangle handling, so the divisibility branch is not exercised. A document-level `[0, 1, 2, 0]` case is required.
- Writer-level invalid geometry cases use pre-existing directories. The detailed finding requires fresh nonexistent directories and `!dir.exists()` assertions for every invalid geometry shape.

## Target

- Add a document-level non-multiple `[0, 1, 2, 0]` case, which reaches divisibility validation.
- For empty indices, non-multiple `[0, 1, 2, 0]`, out-of-range indices, short normals, and short UVs, call `write_glb` with a fresh nonexistent output directory, assert `ExportError::Write`, `!dir.exists()`, and no `.glb` file.
- Keep the existing pre-existing-directory failure atomicity, over-cap, huge-name, and actual write-failure coverage unchanged.

## Failure handling

No production behavior changes. Do not weaken the existing exact-layout, preflight, or writer failure assertions.

## Verification and acceptance

- `cargo test -p umber-export glb::tests` passes with nonzero counts.
- Strict `cargo clippy -p umber-export --all-targets -- -D warnings`, fmt, layers, scorecard, parity, and `git diff --check`.
- Independent read-only review, report, safe-push, and `git ls-remote origin master`.

## Non-goals

No GLB feature changes, no new output behavior, and no production surface beyond these acceptance assertions. This closes the detailed late P2 coverage gap; final code-ready status follows the review.
