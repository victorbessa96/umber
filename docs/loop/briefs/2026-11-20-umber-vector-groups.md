# Design brief: umber-vector-groups (K7c, 2026-11-20)

Kill lock: `razul-vector-groups`. Base: K7b stroke-only subpath import
`3d087302`, pushed truth `e0d8aca9`.

## What exists

`import_svg_many` handles ordered top-level paths and stroke-only subpaths.
Every `<g>` is refused, and root paint other than `fill-rule` is refused even
though SVG fill, stroke, dash, cap, join, width, opacity, and fill-rule are
inheritable presentation properties. `parse_vector_paint` already implements
the leaf paint semantics; the tokenizer and raster document walker already
provide strict nesting and scoped frames.

## Target

K7c adds bounded root and nested-group presentation inheritance without
silently accepting effects:
- Root and `<g>` frames carry `fill`, `fill-opacity`, `fill-rule`, `stroke`,
  `stroke-opacity`, `stroke-width`, `stroke-linecap`, `stroke-linejoin`,
  `stroke-dasharray`, and `stroke-dashoffset`.
- A child attribute overrides the same inherited property. Empty or `inherit`
  values preserve the parent property. Nested groups compose by scoped frame;
  sibling groups cannot leak attributes.
- Paths consume one effective inherited attribute frame through the existing
  `parse_vector_paint`, preserving geometry, paint, dash, order, budgets, and
  stroke-only subpath behavior.
- Root and group `transform`, `style`, `opacity`, `display`, `visibility`,
  clip/mask/filter/markers/vector-effect/paint-order remain typed refusals.
  Unknown elements retain the established skipped-subtree policy.
- The 256-shape and 65,536-total-anchor budgets still cover every imported
  path/subpath. Parsing remains failure-atomic before panel mutation.
- Existing `import_svg` remains the strict single-shape API. Grouped imports
  use K7a's one-history-group multi-layer installation and localized notice.

## Concurrency and safety

All changes stay inside the registered lock. No changes to raster SVG import,
vector schema, rasterization, serialization, or project version. Unsupported
semantics fail rather than being dropped. Parser errors occur before any
`Document` mutation.

## Verification

Focused tests cover root inheritance, nested override and sibling isolation,
all stroke/dash properties, empty/`inherit` preservation, group refusal
surfaces, unknown subtree skipping, budgets, and grouped panel undo/redo.
Then full six-crate clippy/test battery, bounded independent review, kill
report, queue/STATE/ROADMAP/coordination/vault bookkeeping, safe-push, and
origin verification.

## Non-goals

No group transforms, CSS `<style>` resolution, group opacity/compositing,
non-path shapes, filled compound subpaths, release work, or public PR.
