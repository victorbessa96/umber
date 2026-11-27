# Material ID honest refusal

- Lock: `razul-material-id-refusal`.
- Brief: `4677f727e460` (`docs(bake): Material ID refusal brief`).
- Outcome: `IdSource::Material` now fails consistently instead of producing silent zero coverage.

## Root cause and fix

With nonempty `triangle_materials`, validation accepted Material even though neither CPU nor GPU had material-index hashing. The GPU path returned an all-zero image. The picker offered this nonfunctional source.

`validate_multi` now returns `MaterialAssignmentUnsupported` for every Material source before other validation, matching ordinary `IdFlavor::Material` refusal. The Material option is removed from the picker; both project decoders preserve it with an explicit warning, and validation refuses it after load. Source/weight failures now have precise variants (`NoSources`, `InvalidWeight`, `NoContributingWeight`) and reject non-finite weights. CPU/shader comments no longer claim Material is a valid zero fallback.

## RED/GREEN evidence

- Before the fix, CPU validation returned `Ok(())` with nonempty assignments and the GPU returned an all-zero buffer.
- After the fix, both assigned-material tests return `MaterialAssignmentUnsupported` before dispatch.
- The existing missing-assignment refusal and all other source types remain intact.

## Verification

- Focused assigned-material CPU and GPU tests: RED before, GREEN after.
- Full `cargo test -p umber-bake --features gpu -- --test-threads=1`: **214 passed, 0 failed**.
- Full `cargo test -p umber-app -- --test-threads=1`: **923 unit tests plus every integration target passed, 0 failed**.
- `cargo clippy -p umber-gpu -p umber-bake -p umber-app --all-targets -- -D warnings`: clean.
- Rustfmt and `git diff --check`: clean.
- Fresh final review `deleg_94cffb1e`: **SHIP**, zero findings.
- Commit/push/current-tip CI: not yet claimed.

## Non-goals

No fake material hash, release, upstream PR, or centralhost work.
