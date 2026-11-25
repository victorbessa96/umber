# Design brief: late OBJ writer failure coverage (2026-11-25)

Planned kill lock: `razul-obj-writer-coverage`.
Base: final code-ready status `17e85c1db3f20c9d6732406a8de7c211ee377c56`, clean and origin-equal.

## What exists

The OBJ name bound, allocation-free preflight, writer over-cap/huge-name no-directory coverage, and actual `std::fs::write` failure test are landed and reviewed. The detailed late P2 finding text exposes two remaining coverage gaps:

- Ragged cases are asserted only through `to_obj_document`, not through `write_obj` with a nonexistent output directory.
- The `create_dir_all` failure branch is not exercised.

## Target

- For empty indices, non-multiple indices, out-of-range indices, short normals, and short UVs, call `write_obj` with a previously nonexistent output directory, assert `ExportError::Write`, and assert the directory is not created.
- Make the output directory path itself a regular file so `create_dir_all` fails; assert `ExportError::Write`.
- Keep the existing over-cap writer and actual `std::fs::write` failure coverage unchanged.

## Failure handling

No production behavior changes. Do not weaken the existing name-bound, over-cap, huge-name, or write-failure assertions.

## Verification and acceptance

- `cargo test -p umber-export obj::tests` passes with nonzero counts.
- Strict `cargo clippy -p umber-export --all-targets -- -D warnings`, fmt, layers, scorecard, parity, and `git diff --check`.
- Independent read-only review, report, safe-push, and `git ls-remote origin master`.

## Non-goals

No OBJ feature changes, no new output behavior, and no production surface beyond these acceptance assertions. This closes the detailed late P2 coverage gap; final code-ready status follows the review.
