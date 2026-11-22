# Design brief: umber-vector-non-scaling-stroke-preservation (K8g, 2026-11-21)

Planned kill lock: `razul-vector-non-scaling-stroke-preservation`.
Base: K8f implementation `9071c3df89fdd320c94b15365546322cb22323bc` and its
status/report tip `4f616112f125b35f9deb8db9bd61d48c2edfb353`.

## What exists

K8f imports leaf `vector-effect="non-scaling-stroke"` into host-viewport UV
width, dash lengths, and offset. It deliberately stores only those computed
values; the source flag is lost, `export_svg` never emits `vector-effect`, and
K8f's report names that one-way limitation.

`Stroke` in `crates/umber-core/src/vector.rs` already has additive false/default
precedents for cap, join, dash, and miter limit. `stroke_cache_eq` compares each
field. Native JSON uses Serde defaults for backward compatibility. The core
`Stroke` literal inventory is `vector.rs`, `vector_svg.rs`, `svg.rs`, one
`layers.rs` test, and one `project.rs` test. Three `vector_panel.rs`
production/test constructors also require mechanical `false` defaults but no app
behavior change.

## Target

K8g preserves non-scaling-stroke in the editable model and SVG export:

- Add `Stroke.non_scaling_stroke: bool` with `#[serde(default,
  skip_serializing_if = "is_false")]`. Old native JSON without the field loads
  as `false`; ordinary strokes omit the field, preserving current JSON bytes.
- Include the flag in `stroke_cache_eq`, so changing it invalidates the vector
  cache exactly like cap/join/dash/miter changes.
- K8f's import sets the flag from the local leaf value. Ordinary
  `none`/empty/inherit stores `false`; `non-scaling-stroke` stores `true`.
  Hidden/no-stroke validation and root/group refusals are unchanged.
- `export_svg` emits ` vector-effect="non-scaling-stroke"` only when the flag is
  true. Importing that output recovers the same flag and host-unit width/dashes,
  making canonical export→import→export byte-stable for the supported subset.
- Existing `Stroke` literals gain an explicit `non_scaling_stroke: false`; only
  K8f import can set true. Native rendering keeps the existing UV geometry;
  there is no independent editor transform property to reinterpret.
- No project-version bump: the field is additive, false-default, and omitted for
  every legacy/ordinary stroke.

## Concurrency and safety

Locked files planned: `crates/umber-core/src/vector.rs`,
`crates/umber-core/src/svg.rs`, `crates/umber-core/src/vector_svg.rs`,
`crates/umber-core/src/layers.rs`, `crates/umber-core/src/project.rs`,
`crates/umber-app/src/vector_panel.rs`, this brief, the K8g kill report,
`docs/loop/queue.md`, `.dragon/COORDINATION-next-wave.md`, `STATE.md`, and
`docs/ROADMAP.md`. No app UI/behavior, raster stencil, dependency, release,
public-PR, or out-of-lock work.

## Verification

- RED first: flag/roundtrip tests fail because the field does not exist, then
  pass afterward.
- Extend K8f's host-scale fixture: fixed stroke has `non_scaling_stroke == true`,
  ordinary stroke false; fixed export contains the attribute, ordinary export
  does not. Canonical export→import→export is byte-identical and preserves the
  host-unit width, dash lengths, and offset.
- Cache identity differs when only the flag differs. Legacy JSON without the
  field loads false; ordinary serialization omits it; true serializes it.
- All existing `Stroke` literals compile with explicit false defaults. Run full
  core+app tests, not only the focused import suite.
- `rustfmt --edition 2021 --check` on touched Rust, `check-layers.py`,
  `scorecard.py --check`, strict core+app clippy, and full
  `cargo test -p umber-core -p umber-app` under `/tmp/umber-cargo.lock` with the
  app-only `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber` override.
- Bounded independent review; findings are reproduced RED and fixed before
  landing.

## Non-goals

No root/group vector-effect, other vector-effect values, independent editor
transform state, project-version bump, raster stencil change, app UI, opacity,
CSS style, markers, release tag, or public posting.
