# Audit lens A — the TOOLS census (2026-10-24, grep-verified live)

You are auditing Umber (~/Projects/umber), a Rust 3D texture-painting
app. SOLO READ-ONLY TASK: no cargo, no builds, no file edits outside
your report file. Every claim cites file:line from the LIVE tree.

## The lens: the complete paint-tool inventory

Census every paint TOOL surface with (a) its live implementation
site (file:line), (b) its UI reach (panel/strip/shortcut), (c) its
test coverage (the test fn names that pin it), (d) its Substance-
parity state (complete / partial-with-boundary / absent).

The surfaces (verify each — add any you find that this list
misses):

1. The strip tools (crates/umber-app/src/toolbar.rs:148, ToolKind):
   Paint, Smudge, Clone, Erase, Pick (eyedropper), IdMask.
2. The brush tools (crates/umber-brush/src/retouch.rs:113,
   BrushTool): Paint (GPU splat), Smudge/Clone/Erase (CPU retouch).
3. The stroke methods
   (crates/umber-brush/src/stroke_method.rs, StrokeMethod): Space,
   Airbrush, Dash, DragDot — plus any others in the enum.
4. The brush dynamics: stamps (bitmap textures), jitter
   (size/opacity/position), angle modes (fixed/follow/random),
   falloff curves (the presets + custom), front-face falloff,
   hardness, flow/opacity, size-by-pressure, the one-euro
   conditioner + lazy-mouse + line constraint (the stabilizers).
5. The stroke shapes: path tool (PaintPath/PathOp — replay_path /
   replay_filled_path in crates/umber-paint/src/paint_state.rs),
   polygon fill (rasterize_polygon_mask + fill_runs), drag-dot,
   straight-line (shift-lock?).
6. The masks: quick mask (X/I), per-layer masks, stencil
   (crates/umber-app/src/stencil.rs — the paint-through screen
   mask), the cavity mask (curvature gate), the ID-mask gate
   (region select via the baked ID map).
7. Symmetry (umber_brush::symmetry — the mirror axes).
8. The fill/material tools: fill layers, material layers (the
   dynmat), anchors + instances.
9. Wash mode (build-up vs the normalized wash).
10. The retouch parameters: smudge strength, clone source offset,
    the eraser's channel scoping.

## The output

Write your report to docs/loop/audit-2026-10-24-lens-tools.md:
- A headline table: tool | engine site | UI reach | tests | parity
  state.
- A "the named gaps" section: every partial/absent surface, each
  with its named boundary (what exists, what is missing, where the
  seam sits).
- A "the surprises" section: anything the list above missed or got
  wrong about the live tree.

Be adversarial with the docs: if a doc claims a tool exists, verify
the implementation site exists. Cite everything.
