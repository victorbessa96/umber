# Design brief: umber-vector-non-scaling-stroke (K8f, 2026-11-21)

Planned kill lock: `razul-vector-non-scaling-stroke`.
Base: K8e implementation `c2831099ec106e2e68745d6f5989507d64d689d0` and its
status/report tip `119c00a82e9e16668dde910c1dbb15cf2c53ed9f`.

## What exists

`parse_vector_paint` in `crates/umber-core/src/svg.rs` scales `stroke-width`,
`stroke-dasharray`, and `stroke-dashoffset` by
`effective_ctm.max_scale() / min(raster_width, raster_height)`. This is correct
for ordinary SVG strokes. Root/group/path `vector-effect` is currently a typed
`SvgError::Unsupported` refusal. K7d's transforms already give each shape an
effective CTM, and K8d's hidden-leaf path still calls `parse_vector_paint` for
validation without building geometry.

Normative sources:

- SVG 2 Coordinate Systems §8.13, https://svgwg.org/svg2-draft/coords.html#VectorEffectProperty :
  `vector-effect` is not inherited and applies to graphics elements, not groups.
- SVG 2 Painting §13.6, https://svgwg.org/svg2-draft/painting.html#non-scaling-stroke :
  the stroke outline is computed in the furthest ancestral viewport coordinate
  system, making width independent of element transforms, non-uniform scale,
  shear, and zoom.

## Target

K8f implements leaf `vector-effect="non-scaling-stroke"` for editable vector
import:

- Remove `vector-effect` only from `refuse_shape_import_effects`. Root and group
  `vector-effect` remain typed refusals; no inheritance or group/root form is
  added.
- Parse the local value in `parse_vector_paint`: `none`, empty, and `inherit`
  select ordinary behavior; `non-scaling-stroke` selects host-viewport units;
  every other value is a typed refusal. Hidden leaves retain this validation.
- When selected, use `1 / min(raster_width, raster_height)` for stroke width,
  dash lengths, and dash offset instead of `effective_ctm.max_scale() /
  min(...)`. Geometry still uses the full effective CTM. The resulting constant
  UV width/dashes represent host-viewport units around already transformed
  geometry, matching SVG 2's host-space stroke-outline algorithm.
- Ordinary paths keep the existing effective-CTM scale. The `vector-effect`
  attribute is not added to inherited paint attributes, so it remains local to
  the graphics element.
- Named limitation: the imported `Stroke` stores the resulting UV width/dashes,
  not the source `vector-effect` flag. `export_svg` does not emit or preserve
  `vector-effect`; no model/schema field or export roundtrip is claimed. This
  matches the brief's one-way import scope and keeps raster/model/serialization
  untouched.

## Concurrency and safety

Locked files planned: `crates/umber-core/src/svg.rs`,
`crates/umber-core/src/vector_svg.rs`, this brief, the K8f kill report,
`docs/loop/queue.md`, `.dragon/COORDINATION-next-wave.md`, `STATE.md`, and
`docs/ROADMAP.md`. No app, raster stencil, model, serialization, export,
dependency, release, public-PR, or out-of-lock work.

## Verification

- RED first: non-scaling width/dash tests fail against the current typed
  refusal, then pass afterward.
- Use a 200x100 viewport with viewBox `0 0 10 10`, group `scale(4 1)`, and path
  `scale(2)`. Ordinary scale produces `effective_max_scale = 160`; the
  non-scaling path uses raw viewport units. Assert width `2 / 100`, dash lengths
  `1 / 100` and `2 / 100`, and offset `0.5 / 100`, with the ordinary sibling
  showing the old effective-CTM scale.
- Add non-uniform/shear coverage, root/group scope refusals, invalid
  `non-scaling-size`, ordinary `none`/empty/inherit behavior, and hidden-leaf
  invalid-value validation.
- Verify no `vector-effect` string appears in inherited paint attributes or
  emitted SVG; no roundtrip preservation is claimed.
- `rustfmt --edition 2021 --check` on touched Rust, `check-layers.py`,
  `scorecard.py --check`, strict core+app clippy, and full
  `cargo test -p umber-core -p umber-app` under `/tmp/umber-cargo.lock` with the
  app-only `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber` override.
- Bounded independent review; findings are reproduced RED and fixed before
  landing.

## Non-goals

No root/group vector-effect, `non-scaling-size`, `non-rotation`,
`fixed-position`, marker behavior, CSS style, opacity, clip/mask/filter,
`paint-order`, hidden-layer state, model field, serialization/export
preservation, schema/project version, release tag, or public posting.
