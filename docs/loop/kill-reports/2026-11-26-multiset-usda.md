# Multi-texture-set USDA routing

- Lock: `razul-multiset-usda`.
- Brief: `dfa1a70ab524` (`docs(export): multi-set USDA routing brief`).
- Outcome: every app-written USDA now uses the exact texture-set and tile names of the files its own export produced.

## Root cause and fix

The app wrote textures per group but built one USDA from the original mesh-level `TokenSources`. Body/Hilt files therefore coexisted with a document whose references used the mesh-stem name, and tiled references could retain a literal `<UDIM>` placeholder.

The group loop now emits one concrete USDA per tile:

- Lone 1001 keeps `<set>.usda` and existing concrete names.
- Tiled runs use `<set>_<tile>.usda`.
- Texture references use `obj_file_for_output(..., Some(tile))`, the same concrete-tile expansion as the files, so no `<UDIM>` placeholder remains.

## RED/GREEN evidence

- Before the fix, `Body.usda` did not exist. After the first group-loop change, `Hilt_1002.usda` still referenced `Hilt_baseColor_<UDIM>.png`. The final concrete-tile helper closes both failures.
- A genuine two-tile/two-set mesh and `PaintState` drives the real app export and asserts `Body.usda` plus `Hilt_1002.usda`, no cross-set references, and every `asset inputs:file` target exists.
- Fixture diagnosis was explicit: tile 1001 needed an explicit Body assignment because changing the active set cannot rename an existing default assignment.

## Verification

- Focused `multi_set_usda_references_the_files_each_group_wrote`: RED before implementation, GREEN after.
- Full `export_dialog::tests`: **34 passed, 0 failed**.
- Strict `cargo clippy -p umber-app --all-targets -- -D warnings`: clean.
- Rustfmt and `git diff --check`: clean.
- Independent bounded review `deleg_1f59a627`: **SHIP**, zero functional findings, with grouping/naming and test-honesty analysis accepted. The reviewer noted only pre-existing pathological naming collisions and optional assertion hardening outside this kill's scope.
- Commit/push/current-tip CI: local commit next; origin and CI are not yet claimed.

## Non-goals

No whole-run transaction redesign, release, upstream PR, or centralhost work.
