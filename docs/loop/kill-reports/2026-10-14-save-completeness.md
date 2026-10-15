# Kill report — save-completeness (exhaustive destructuring in the project save/load)

Date: 2026-10-14. Author: dragon. Lock: `save-completeness` over
`crates/umber-core/src/project.rs` and this report. Source: ROADMAP wave
8b row `wave-save-completeness`; absorbed from photocraft
`crates/format/src/convert.rs` (see
`docs/research/absorption-2026-10-14-photocraft-blender.md`).

## Built

- `save_to_dir` destructures `ProjectModel` with every field named and
  no `..`, then builds `ProjectFile` from those bindings. Adding a field
  to `ProjectModel` now fails to compile in `save_to_dir` until the save
  path handles it. Before, the function read `model.<field>` piecemeal,
  so a new model field the author forgot to persist compiled cleanly and
  was dropped on every save.
- `load_from_dir` destructures the parsed `ProjectFile` the same way, so
  a field added to the on-disk schema but never carried into the model
  is also a compile error.
- Behaviour is unchanged: the same values flow to the same places. The
  existing project round-trip tests are the regression oracle.

## Judgment calls

1. **Destructure, not a test.** A round-trip test only covers the fields
   it populates; a new field defaults to its zero value and still
   compares equal after a lossy save. The compile error is the only check
   that catches a field nobody remembered.
2. **Layer bodies are left alone.** `Layer` is serialized whole through
   serde (`to_pretty_json_bytes(layer)`), so a new `Layer` field is
   persisted automatically. Destructuring it would add noise without
   catching anything.
3. **The load side was already half-covered.** `ProjectModel { … }` there
   is a struct literal, which already fails on a missing field. The new
   `ProjectFile` destructure covers the other direction (a file field
   nobody consumes).

## Unverified

- Nothing new to test at runtime: this is a compile-time property. The
  proof is that the crate compiles with the destructures present. A
  "negative test" (adding a dummy field and watching the build fail)
  needs a compile-fail harness (trybuild), which the workspace does not
  carry; not added for one site.

## Deferred

- The same shape for the paint sidecar
  (`umber_paint::persistence`) and the export preset files, once each has
  a model-to-disk converter worth guarding.
