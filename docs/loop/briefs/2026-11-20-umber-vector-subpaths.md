# Design brief: umber-vector-subpaths (K7b, 2026-11-20)

Kill lock: `razul-vector-subpaths`. Base: K7a multi-path SVG import `29a02f9a`,
pushed truth `07010149`.

## What exists

`import_svg_many` splits top-level path elements into separate vector layers.
`VectorImportBuilder` still rejects a second moveto inside one path's `d`.
One-shape-per-layer rasterization cannot represent compound fill-rule holes.

## Target

K7b adds safe multi-subpath input for stroke-only paths:
- The import builder collects each moveto-delimited subpath, including a new
  segment after closepath, in document order.
- Stroke-only compound paths become one vector shape per subpath, with the
  same stroke/cap/join/dash state and transparent fill.
- Filled compound subpaths remain a typed refusal because splitting them
  would change nonzero/evenodd hole semantics.
- The 256-shape and 65,536-total-anchor budgets include subpaths, not only
  top-level path elements.
- Existing `import_svg` remains the strict single-shape API. Panel import
  creates all split layers in one undo group through the K7a path.

## Non-goals

No compound filled-path model, group inheritance, or release work.

## Verification

Focused vector_svg and vector_panel suites, full six-crate clippy/test battery,
bounded independent review, then report/queue/STATE/ROADMAP/coordination/vault
and safe-push.
