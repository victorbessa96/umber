# Kill report: 4.1a smart-material parameter schema

- Authorship: opencode claw (proc_a49290334127); lock `razul-smart-params-schema` over smart.rs + aged-leather.umbersmart + rusted-iron.umbersmart; dragon battery green first pass (fmt/layers/score/clippy/tests 0, both assets JSON-valid).
- Implements: docs/loop/briefs/2026-11-10-smart-params-schema.md

## Built

- `SmartMaterial` gains an optional `parameters: Vec<SmartParam>` (`#[serde(default, skip_serializing_if)]`) — version stays 1; old files parse with an empty block and re-serialize byte-identically, new files are ignored-field-tolerant by additive serde, so old readers still load them. Documented in the module judgment calls.
- `SmartParam { id, label, kind }` with closed `SmartParamKind`: `Float { min, max, step, default }`, `Choice { options, default }`, `Bool`. Validation at parse: empty/dup ids, default-out-of-range, choice default not among options all fail loudly via new `SmartMaterialError` variants (unknown `param` binding included).
- Generator bindings gain an optional `param: String` (`skip_serializing_if`) resolved against declared ids at parse AND apply; unknown id = error, never silent.
- Authored real wired params: aged-leather (`wear`, `roughness` floats bound into its generator chain), rusted-iron (rust coverage + metal/oxide choice).
- Unit tests: round-trip of both shipped files, param resolution ok/err, legacy no-parameters parse.

## Judgment calls

- Params live on the material, not per-layer: one exposed-parameter panel per preset matches the UI lane (4.1b); per-layer params would be a second surface with no consumer yet.
- Version stays 1 deliberately: the block is additive-optional, and a bump would strand every shipped preset on old readers.

## Unverified

- No UI consumes the block yet — 4.1b's job; nothing in app/ references `parameters`/`param` (grep-verified).

## Deferred

- 4.1b inspector panel + live-render check; other 8 shipped .umbersmart files get params only when 4.1b proves the UI.
