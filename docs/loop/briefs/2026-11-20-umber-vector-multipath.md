# Design brief: umber-vector-multipath (K7a, 2026-11-20)

Kill lock: `razul-vector-multipath`. Base: K6d dash patterns `58f9e160`, pushed
truth `417b62bc`.

## What exists

The vector model stores one `VectorShape` per layer. SVG import accepts exactly
one top-level path and refuses a second path. The panel installs one imported
layer in one history group. K6d dash patterns and all cap/join options already
work per path.

## Target

K7a adds bounded multi-path SVG input without changing the one-shape raster
model:
- `import_svg_many` returns one `VectorParams` per top-level path in document
  order, preserving each path's fill, stroke, cap, join, and dash state.
- Existing `import_svg` remains the strict single-path compatibility API.
- At most 256 paths and 65,536 anchors across the whole import; excess returns
  typed `SvgError::TooComplex` before further growth.
- Groups, non-path shapes, path transforms/style, and unsupported effects stay
  refused. Multiple subpaths inside one `d` remain a separate future slice.
- The panel installs every returned shape as its own vector layer inside one
  undoable history group, selects the last imported layer, and shows a plural
  localized notice for multi-layer imports.
- Parse-before-mutation and failure atomicity remain unchanged.

## Non-goals

No group inheritance, multi-subpath splitting, compound multi-shape layer
model, project version bump, or release work.

## Verification

Focused vector_svg and vector_panel suites, full six-crate clippy/test battery,
bounded independent review, then report/queue/STATE/ROADMAP/coordination/vault
and safe-push.
