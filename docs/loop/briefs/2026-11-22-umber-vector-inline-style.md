# Design brief: umber-vector-inline-style (K8h, 2026-11-22)

Planned kill lock: `razul-vector-inline-style`.
Base: K8g implementation `1f49a533d0bde872591a7fd80f1bd8b3eeeb9e3e` and its
status/report tip `9dba956e8efcf6c11319666885776f926580463b`.

## What exists

`style` is a typed refusal on root, group, and graphics elements. Umber already
supports presentation attributes for fill/fill-opacity/fill-rule, stroke and
stroke paint/width/cap/join/miter/dash, display/visibility, and leaf
non-scaling-stroke. `inherited_vector_paint` composes inherited paint;
`effective_vector_paint` adds the current leaf's non-inherited vector-effect;
K8d computes display/visibility after attribute composition.

Normative SVG 1.1 Styling §6.4 and §6.12: inline `style` contains
semicolon-separated `name : value` declarations and has higher priority than
presentation attributes. SVG 1.1 §6.7 notes CSS property names are generally
case-insensitive. Multiple declarations of one property use CSS's last valid
declaration.

## Target

K8h adds a bounded inline-style declaration expander for properties Umber
already implements:

- Parse `style` on root, groups, and supported graphics elements before the
  existing effect/paint validation. Split semicolon-separated declarations,
  trim CSS whitespace, ASCII-lowercase property names, and reject declarations
  without a name, colon, or value as typed `SvgError::Unsupported`.
- Whitelist only `fill`, `fill-opacity`, `fill-rule`, `stroke`,
  `stroke-opacity`, `stroke-width`, `stroke-linecap`, `stroke-linejoin`,
  `stroke-miterlimit`, `stroke-dasharray`, `stroke-dashoffset`, `display`,
  `visibility`, and `vector-effect`. Every other property remains a typed
  refusal; no partial CSS engine is claimed.
- Replace same-named presentation attributes with inline declarations, so
  inline style wins. Later declarations replace earlier ones. Empty style and
  empty declarations are ignored. Accept a trailing case-insensitive
  `!important` on a declaration and remove it; it does not change ordering
  within the same inline style.
- Feed the expanded attributes through the existing root/group/shape paths.
  Root/group `vector-effect` therefore remains refused, hidden leaves retain
  validation, K8f/K8g non-scaling-stroke import/export is unchanged, and the
  raster stencil keeps its separate typed style refusal.
- No `<style>` element, selectors, classes, external CSS, variables,
  `currentColor`, CSS transforms, or new property semantics are added.
- Export continues to emit canonical presentation attributes, not the original
  `style` text; canonical geometry/paint/flag roundtrip remains the contract.

## Concurrency and safety

Locked files planned: `crates/umber-core/src/svg.rs`,
`crates/umber-core/src/vector_svg.rs`, this brief, the K8h kill report,
`docs/loop/queue.md`, `.dragon/COORDINATION-next-wave.md`, `STATE.md`, and
`docs/ROADMAP.md`. No model, serialization, export production, raster stencil,
app UI, dependency, release, public-PR, or out-of-lock work.

## Verification

- RED first: inline-style import tests fail on the current typed refusal, then
  pass afterward.
- Cover inline-over-presentation precedence for root/group/path, last
  declaration wins, trailing `!important`, CSS property-name case folding,
  empty style/declarations, and every whitelisted paint/display/visibility/
  vector-effect property.
- Pin root/group style inheritance, child overrides, hidden-leaf invalid-style
  validation, missing colon/name/value, unknown properties, unsupported
  opacity/transform/CSS variables, and root/group vector-effect refusal.
- Verify canonical export emits presentation attributes with the same parsed
  paint and non-scaling flag, not source style text.
- `rustfmt --edition 2021 --check` on touched Rust, `check-layers.py`,
  `scorecard.py --check`, strict core+app clippy, and full
  `cargo test -p umber-core -p umber-app` under `/tmp/umber-cargo.lock` with the
  app-only `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber` override.
- Bounded independent review; findings are reproduced RED and fixed before
  landing.

## Non-goals

No CSS stylesheets/selectors/classes, unsupported properties, CSS transforms,
currentColor, variables, source-style export preservation, raster style,
model/schema change, release tag, or public posting.
