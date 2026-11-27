# Kill brief: multi-texture-set USDA routing

- Lock: `razul-multiset-usda`.
- Trigger: formats audit finding 3 at `ec979743` showed that app exports write per-group texture files but build one USDA from the original mesh-level `TokenSources`, leaving references that do not match the files.
- Scope: `export_dialog.rs` group loop, a real two-set export test, and the kill report.

## Required fix

Emit one USDA per texture-set group using that group's `TokenSources` and local tile naming. Keep single-set behavior byte-identical. Every document's texture references must resolve to files written by the same run.

## Test

Build a genuine two-tile/two-set mesh and `PaintState`, enable USDA, run the real app export, and assert:

- `Body.usda` and `Hilt.usda` exist and are reported.
- Every `asset inputs:file = @...@` reference resolves to a real file.
- Body references stay Body-scoped and Hilt references stay Hilt-scoped.

## Constraints and verification

No whole-run transaction redesign, release, upstream PR, or centralhost. Run the regression RED first, then full export-dialog/app tests, rustfmt, clippy with `-D warnings`, and independent review before commit.
