# GUI plan — the research prompt (2026-10-15)

This is the brief the research subagent ran from. It is kept beside the plan
so the plan's sources and scope can be audited.

## The ask (Bessa, 2026-10-15)

"Make a complete multi-wave plan for the UI/GUI, use as reference Substance
Painter, the Blender project and Photocraft. Make sure it's complete, start
with the prompt then do the research based on the prompt in a subagent,
write down the complete plan and implement it all."

## What Umber's GUI is today (verified in tree, 2026-10-15)

- Stack: egui/eframe 0.36, egui_dock 0.21, wgpu 30. Immediate mode.
- Shell: `MenuBar` (File/Edit/View …), top toolbar (`toolbar.rs`: tool row
  with lucide icons, 9 shortcuts W/G/V/X/I/E/Z/S/O), bottom status bar
  (`status_bar.rs`), a dock of 11 panels (`Panel` enum in `main.rs`):
  Viewport, UvView, LayerStack, Properties, Assets, History, TextureSets,
  Bakes, Export, Graph, Display. Default layout: left column (layers +
  sets/bakes/export/graph/display tabs) over Properties/Assets; centre
  3D + UV tabs; History right.
- `crates/umber-gui` (the design-language crate, Phase 0 of
  `docs/specs/gui-audit-and-roadmap.md`): `theme.rs` (palette tokens →
  `egui::Style`, density), `fonts.rs` (Inter + JetBrains Mono), `icons.rs`
  (lucide atlas), widgets `param_group`, `value_field`, `tool_row`,
  `swatch_grid`. Empty-state hero exists (`empty_state.rs`).
- Phase 1 (panel migration) is PARTIAL: 22 raw `egui::Slider`s remain in
  postfx_panel (10), fill_layer (6), path_tool (4), layers_panel (1), …
- Phase 2/3 are almost untouched: no in-viewport brush cursor, no rich
  tooltips, no layer thumbnails, no onboarding, the color picker is
  egui's stock one, motion is used in 3 places.
- Snapshot tests exist for 2 panels only (Display, PSD import section),
  via egui_kittest (`crates/umber-app/tests/panel_snapshot.rs`).
- i18n: Fluent en-US + pt-BR, a key-parity test; every user string goes
  through `tr()` (CLAUDE.md rule 7).
- Brush engine features with no UI yet: stroke methods (airbrush / dash /
  drag-dot, landing now), the cavity mask (pure module landed), the
  Adjacent Faces bake margin (CLI-wired now, no bake-panel control).

## The research question

What does a COMPLETE, professional GUI for a Substance-Painter-class texture
painter consist of — every surface, panel, interaction and affordance — and
what is the best phased path from Umber's current state to it?

## Sources (read directly; cite every claim)

1. **Adobe Substance 3D Painter** — the user guide
   (helpx.adobe.com/substance-3d-painter/interface.html and its children:
   the shelf, layer stack, properties, texture set list/settings, display
   settings, history, viewport toolbar, contextual toolbar, the
   brush/stencil/particles, the export window, preferences, shortcuts).
   This is the product Umber's users come from: their muscle memory.
2. **Blender** — the manual's user-interface chapters
   (docs.blender.org/manual/en/latest/interface/: window system, areas,
   editors, tool system, toolbar + tool settings, the properties editor,
   operator search / F3, the pie menus, the asset browser, keymap
   customisation, themes, the status bar) and the texture-paint mode UI.
   And Blender's UI design guidelines
   (developer.blender.org/docs/features/interface/human_interface_guidelines/).
3. **photocraft** (github.com/storytold/photocraft) — an egui/eframe/wgpu
   Rust app, i.e. the SAME stack as Umber: how it structures its shell,
   panels, command palette, theming, tool options, layer panel, and which
   egui patterns it uses (read its `apps/`, `crates/ui-egui*` or equivalent
   via the GitHub API tree + raw files). The point is egui-native patterns
   Umber can copy directly.

## The deliverable

A ranked, complete inventory of GUI surfaces, organized as:

- **A. Shell & navigation**: menus, toolbar, contextual toolbar, status bar,
  command palette / operator search, workspaces/layout presets, docking,
  multi-window, fullscreen/focus modes.
- **B. Panels** (one entry per panel, with its must-have contents from the
  reference apps): layer stack, properties (per tool / per layer), texture
  set list + settings, shelf / asset browser, history, bakes, export,
  display/viewport settings, node graph, preferences, shortcuts.
- **C. Viewport interaction**: brush cursor, stencil, gizmos, camera
  navigation conventions (Painter's vs Blender's), view cube / axis widget,
  overlays, the 2D/3D split, picking.
- **D. Flagship widgets**: color picker, curve editor, gradient editor,
  value fields, thumbnails, swatches, preset browser.
- **E. Feedback & state**: tooltips, progress, notifications/toasts, error
  surfaces, empty states, onboarding, undo visualization.
- **F. Customisation & accessibility**: themes, density/scale, keymap
  editing, language, high-contrast, keyboard-only use, screen readers
  (AccessKit in egui).
- **G. Engine features that need UI** (Umber-specific): the list above.

For each item: what the references do (cite the page), what Umber has
today (if known), effort (S/M/L), and priority (P0 = a Painter user is
blocked or lost without it; P1 = expected; P2 = delight). Then a proposed
**wave plan**: waves of 3–6 kill-sized items each, every kill naming its
file surface so waves can run in parallel without collisions, and a
verification approach per kill (snapshot test, unit test, or manual).

Rules: cite sources; mark anything not read directly as inferred; do not
invent features of the reference apps; prefer concrete, scoped items over
"improve the UX".
