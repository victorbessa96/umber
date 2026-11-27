# Kill brief: Material ID honest refusal

- Lock: `razul-material-id-refusal`.
- Trigger: engine audit finding 3 at `ec979743` showed that `IdSource::Material` is accepted when per-triangle assignments exist but neither CPU nor GPU can produce a material contribution.
- Scope: `id.rs` validation, the app picker, direct regressions, and the kill report.

## Required behavior

- `validate_multi` returns `MaterialAssignmentUnsupported` for `IdSource::Material` regardless of whether assignments exist, matching ordinary `IdFlavor::Material` refusal.
- Remove Material from the selectable ID-source options. Existing project files carrying it fail with the same typed error instead of silent zero coverage.
- Add a regression with nonempty `triangle_materials` proving refusal; keep the existing missing-assignment refusal and all other sources intact.

## Constraints and verification

No fake material hash, release, upstream PR, or centralhost. Run focused tests, full GPU-enabled bake and relevant app tests, rustfmt, clippy with `-D warnings`, and independent review before commit.
