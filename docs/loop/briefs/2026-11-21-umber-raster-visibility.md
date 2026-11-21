# Design brief: umber-raster-visibility (K8e, 2026-11-21)

Planned kill lock: `razul-raster-visibility`.
Base: K8d implementation `4f1858aa763c9b83a8d2e0bd994607050fbb5748` and its
status/report commit `93483b84993fbcfe35de732dbed2285aea77c163`.

## What exists

The raster stencil path `parse_document` in `crates/umber-core/src/svg.rs`
uses `Style { painted, rule, ctm }` and `apply_presentation` for root/group/
shape fill, fill-rule, and transforms. Its module docs currently list
`display`/`visibility` as unparsed. The walker skips unknown/path subtrees with
`None`, builds every painted shape, and charges the raster `MAX_SHAPES` budget.
K8d implemented display/visibility only for editable vector import; it left the
raster path deliberately unchanged.

Normative source: W3C SVG 1.1 (Second Edition), Painting §11.5,
https://www.w3.org/TR/SVG11/painting.html#VisibilityControl . `display` is not
inherited; `none` removes direct rendering for the element and its descendants,
while every other value renders. `visibility` is inherited, starts `visible`,
and `hidden`/`collapse` suppress paint unless a descendant selects `visible`.

## Target

K8e implements display/visibility in the **raster SVG stencil** path:

- Extend raster `Style` with effective `display` and `visible` booleans. Root
  display starts rendered and root visibility starts `visible`; the existing
  `vector_display_renders` and `vector_visibility_renders` helpers supply local
  values and typed invalid-visibility errors.
- `apply_presentation` continues parsing fill/fill-rule/transform for every
  root/group/shape, including hidden elements, then applies the local display
  and visibility values. A hidden invalid transform remains a parse error.
- A group frame keeps `display = parent.display && display`; descendants cannot
  re-enable a pruned ancestor. Visibility remains child-overridable. Unknown and
  path subtrees retain the independent `None` skip behavior.
- A shape builds raster geometry only when effective display, effective
  visibility, and `style.painted` are all true. Hidden shapes do not reach the
  edge builder or consume `MAX_SHAPES`; visible shapes keep exact budget and
  transform behavior.
- All-hidden documents produce the normal blank coverage plane at the parsed
  dimensions, not the editable importer's typed no-geometry refusal.
- Update module docs: raster now interprets display/visibility under SVG 1.1
  §11.5. Editable import behavior from K8d is unchanged.

## Concurrency and safety

Locked files planned: `crates/umber-core/src/svg.rs`, this brief, the K8e kill
report, `docs/loop/queue.md`, `.dragon/COORDINATION-next-wave.md`, `STATE.md`,
and `docs/ROADMAP.md`. No editable-import, app, model, serialization, export,
dependency, release, public-PR, or out-of-lock work.

## Verification

- RED first: raster display/visibility tests fail against the current parser
  before implementation, then pass afterward.
- Cover root/group/shape display-none, non-none display values, inherited
  visible/hidden/collapse/inherit, child-visible override, no display override,
  sibling/order behavior, invalid visibility, hidden invalid transforms, blank
  all-hidden coverage, and hidden shapes excluded from the exact shape budget.
- Verify hidden content does not affect coverage bytes and visible content still
  matches the existing rasterizer's exact pixels.
- `rustfmt --edition 2021 --check crates/umber-core/src/svg.rs`,
  `check-layers.py`, `scorecard.py --check`, strict core+app clippy, and full
  `cargo test -p umber-core -p umber-app` under `/tmp/umber-cargo.lock` with the
  app-only `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber` override.
- Bounded independent review of the final diff; findings are reproduced RED and
  fixed before landing.

## Non-goals

No editable-vector changes, opacity, CSS style, clip/mask/filter/markers,
`vector-effect`, `paint-order`, events, references, text layout, hidden-layer
persistence, schema/project version, release tag, or public posting.
