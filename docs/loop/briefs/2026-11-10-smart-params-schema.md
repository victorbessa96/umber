# Kill brief: 4.1a smart-material parameter schema

- Lock: `razul-smart-params-schema` over exactly:
  - `crates/umber-core/src/smart.rs`
  - `assets/smart/aged-leather.umbersmart`
  - `assets/smart/rusted-iron.umbersmart`
- Implements wave-plan row 4.1a (docs/loop/waves/2026-11-09-wave-plan.md). The UI half (4.1b) is a separate kill; do NOT touch the inspector/app crate.

## Measured state

- `SmartMaterial` (smart.rs) today: `{ name, version, layers: Vec<SmartLayerTemplate> }` — NO parameters block. Shipped `.umbersmart` files carry only name/version/layers.
- `SmartLayerTemplate` layers bind mesh-map generators via `generator: { node_def, ... }` anchors.

## Build

1. Add an optional parameter block to the schema, backward compatible: `#[serde(default)] pub parameters: Vec<SmartParam>` on `SmartMaterial`, version stays 1 (old files parse with an empty block; new files with params still parse on old readers only if they ignore unknown fields — check serde attrs: if `deny_unknown_fields` is set anywhere on SmartMaterial, REMOVE it for this field path or bump version handling; document the choice).
2. `SmartParam` shape (decide and justify in the kill report): at minimum `id: String`, `label: String`, `kind` — a closed enum covering at least `Float { min, max, step, default }` and `Choice { options, default }` (add Bool only if a shipped material needs it). Parameters must be referable from generator bindings: extend the generator binding (or the template layer) with an optional `param: String` reference so a node input can be driven by a param id. Resolution/validation: every `param` reference must resolve to a declared parameter id; unknown id = parse/apply error, not silent ignore. Unit-test both directions.
3. Author real parameters on the two locked materials: `aged-leather.umbersmart` (e.g. wear amount / roughness driven by a float param bound into its generator chain) and `rusted-iron.umbersmart` (e.g. rust coverage float + metal/oxide choice). The params must be genuinely wired into the generator bindings, not decorative. Keep each file's existing layer structure and voice; additive only.
4. Unit tests in smart.rs: round-trip parse of both shipped files, param-reference resolution ok/err, old-format file (no parameters key) parses with empty vec.

## Rules

- Edit ONLY the three locked files. No app/inspector/UI changes, no .ftl, no cargo build beyond `cargo test -p umber-core` and `cargo clippy -p umber-core --all-targets -- -D warnings`.
- No commit/push. Minimal diffs, existing style, doc comments on new public items.
- If the schema shape has a real fork (e.g. params on the material vs on each layer), pick one, justify in one paragraph in the kill-report comment at the top of your summary; do not build both.

## Dragon verification

`cargo test -p umber-core`, clippy -D warnings, rustfmt --check on smart.rs, JSON validity of both .umbersmart files, grep that no other file references the new fields yet (4.1b will), then commit+push via dragon.
