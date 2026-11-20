# Design brief: umber-vector-transforms (K7d, 2026-11-20)

Kill lock: `razul-vector-transforms`. Base: K7c root/group inheritance
`f04ed236`, pushed truth `97f33a9e`.

## What exists

The shared SVG `Affine` parser supports matrix, translate, scale, rotate, and
skew composition. Vector import applies the root transform once, while K7c
adds scoped inherited paint frames. Group and path `transform` attributes are
still refused even though the geometry builder and stroke/dash scaler already
accept an arbitrary CTM.

## Target

K7d adds bounded nested group and leaf-path affine transforms:
- Each scoped import frame carries both inherited paint and the current CTM.
- A group transform composes `parent_ctm.compose(local)` in document order;
  a path transform composes once more for that leaf only.
- Root viewBox/root transform, nested groups, and path transforms are applied
  exactly once. Sibling transforms cannot leak.
- Geometry uses the effective CTM for exact anchor/handle UV mapping.
  Stroke width, dash lengths, and dash offset use the same effective CTM's
  established maximum-scale conversion.
- Existing paint inheritance, document order, subpath behavior, 256-shape and
  65,536-total-anchor budgets, strict single-shape API, and grouped panel
  undo/redo remain unchanged.
- CSS `style`, group opacity, visibility, effects, filled compound subpaths,
  and non-default miter limits retain their typed refusal behavior.

## Concurrency and safety

All changes stay inside the registered lock. No changes to the raster SVG
walker, vector schema, serialization, or project version. Transform parse and
non-finite geometry failures occur before panel document mutation.

## Verification

Focused tests cover nested group + path composition, root-once behavior,
sibling isolation, stroke/dash scaling, transform parse failures, and grouped
panel undo/redo. Then full six-crate clippy/test battery, bounded independent
review, kill report, queue/STATE/ROADMAP/coordination/vault bookkeeping,
safe-push, and origin verification.

## Non-goals

No CSS transforms/style, nested `<svg>` viewports, group opacity/compositing,
non-path shapes, filled compound subpaths, release work, or public PR.
