# Design brief: late Assets mesh acceptance coverage (2026-11-25)

Planned kill lock: `razul-assets-mesh-coverage`.
Base: final code-ready status `bc3d7fc98ae68085699b351c45c9228482ffdd26`, clean and origin-equal.

## What exists

The Assets mesh category and its first two R1 findings are landed and reviewed: empty scan results stay cached until `scan_epoch` changes, and the lister covers missing directories. The late R1 review `deleg_915dd909` also identified two remaining test weaknesses:

- The mesh no-match assertion accepts any global no-match label, so deleting the Meshes branch could leave it green.
- The directory test permits wrong bundled/exe ordering and does not prove built-in alias suppression.

## Target

- Change the category search to a query that matches existing non-mesh assets but no mesh, then assert exactly one no-match label and no mesh row.
- Assert the exact bundled→exe→extra directory sequence, including built-in aliases in the extras input.

## Failure handling

No production behavior changes. Do not weaken the existing empty-cache regression, missing-directory lister coverage, or the exact mesh outcome path.

## Verification and acceptance

- Focused Assets mesh tests pass with nonzero counts.
- Strict `cargo clippy -p umber-app --all-targets -- -D warnings`, fmt, layers, scorecard, parity, and `git diff --check`.
- Independent read-only review, report, safe-push, and `git ls-remote origin master`.

## Non-goals

No Assets feature changes, no new category, and no production surface beyond these acceptance assertions. This closes the late R1 acceptance gaps; final code-ready status follows the review.
