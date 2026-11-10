# Kill brief: 4.2a community templates — authoring lane

- Lock: `razul-templates-authoring` over:
  - `assets/templates/` (new directory, everything under it)
  - `crates/umber-core/src/project.rs` (TEST MODULE ONLY — add a test that loads every shipped template)
- Implements the authoring half of wave-plan row 4.2. The loader/menu wiring ("whatever loads them") is a SEPARATE kill (4.2b) held until the fable UI/UX audit lands; do not touch umber-app, shell_menu, or any UI.

## Measured state

- `.umber` is a DIRECTORY format (crates/umber-core/src/project.rs: `myproject.umber/project.json` + `layers/<Set>/<Channel>/<n>.json`, `load_from_dir` refuses foreign versions).
- `assets/` today has brushes, env, fonts, gen, i18n, icons, smart — ZERO templates. `assets/templates/` does not exist yet.

## Build

1. Author 10 curated template projects under `assets/templates/<slug>.umber/`, each a valid loadable project per `load_from_dir` (project.json with the supported `version`, at least one texture set with at least one layer; use the real layer JSON shape from project.rs tests / an existing example — check `examples/` if present). Themes should cover the app's lanes: e.g. PBR metal/wood/leather set, sci-fi panel, terrain height set, character skin set, stylized toon, UI icon set, grunge overlay, water normal set, emissive sign, weathering kit. Each template should be small (1-3 sets, a handful of layers) and genuinely useful, not filler. Include a one-line description per template.
2. `assets/templates/TEMPLATES.md`: index table (slug, name, description, sets/channels) + the authoring rules (what makes a template shippable: loads clean, version-pinned, no absolute paths, no secrets).
3. Validation test in the project.rs test module: resolve `assets/templates` via the bundled data path (see bundled.rs `data_path`), walk `*.umber/` dirs, `load_from_dir` each, assert Ok; assert count == 10. Skip gracefully if the dir is absent (installed-binary case) but FAIL in the repo dev loop where it exists.

## Rules

- Edit only the locked surface: everything under `assets/templates/` (new) and the test module of project.rs. No app/UI/menu/ftl changes. No commit/push.
- Every template must actually load — run `cargo test -p umber-core` and show the new test passing. Also `cargo clippy -p umber-core --all-targets -- -D warnings` and `rustfmt --check` on touched .rs.
- Layer JSON must be hand-valid against the real structs (BlendMode etc.); do not invent fields.

## Dragon verification

Full battery on umber-core + JSON-validity scan of every template's project.json and layer files + confirm exactly 10 templates load + commit/push via dragon.
