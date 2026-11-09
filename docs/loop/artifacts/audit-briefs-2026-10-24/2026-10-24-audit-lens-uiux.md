# Audit lens B — the UI/UX census (2026-10-24, grep-verified live)

You are auditing Umber (~/Projects/umber), a Rust 3D texture-painting
app (egui/eframe). SOLO READ-ONLY TASK: no cargo, no builds, no file
edits outside your report file. Every claim cites file:line from the
LIVE tree.

## The lens: the complete UI/UX surface census

### 1. The GUI plan's waves G0-G7

docs/specs/gui-plan-2026-10-15.md defines eight waves (~45 kills).
For EACH named kill in the plan: verify against the live tree —
landed (site + test), partial (what's the boundary), or absent.
The waves' themes: G0 foundations (action registry, focus dispatch,
kittest harness, toasts), G1 the first five minutes (brush cursor,
Ctrl+RMB drag, status-bar hints, straight-line preview, nav
presets, axis gizmo), G2 panels (layer thumbnails + gestures,
history panel, texture-set list, Properties + search, raw sliders,
bakes margin UI, stroke-method/cavity UI), G3 find anything
(command palette, preferences, keymap editor, rich tooltips), G4
flagship widgets (colour picker, curve editor, gradient ramp, field
grammar, property context menu), G5 shell (menu completion,
workspaces, Hide UI, home screen, first-run, tab overflow), G6
viewport depth (overlays popover, contextual toolbar, display-mode
keys, UV view, stencil overlay), G7 a11y + polish (AccessKit,
keyboard-only, UI scale, polish).

### 2. The panels

List every panel the shell renders (crates/umber-app/src/main.rs's
panel fns + the panel modules: layers_panel, history_panel,
display_panel, bakes_panel, brush_panel, export_dialog,
empty_state...): its live site, what it edits, its kittest
coverage.

### 3. The i18n state

The .ftl catalogs (find them): key counts per locale, the
completeness test's pin, ANY lock/staleness state (grep the
docs/loop for ftl lock notes — the freeze-3 recovery was the last
named lock; is it lifted?). Name the exact lock state — a pending
kill (the OccluderScope panel row) is double-gated on this answer.

### 4. The a11y + keyboard state

The keyboard coverage: every shortcut/accelerator grep
(ui.input/key bindings in the shell) — list them (binding →
action). The focus/tab state, any AccessKit usage (grep
accesskit), the UI scale/zoom handling.

### 5. The desktop UX gaps

The desktop-checks.md smokes (docs/loop/desktop-checks.md): which
Result lines are filled, which are empty (the unclaimed smokes).

## The output

Write your report to docs/loop/audit-2026-10-24-lens-uiux.md:
- The G0-G7 scorecard table: wave | kills landed | partial |
  absent (with sites).
- The panels table.
- The i18n verdict (including the .ftl lock state — this answer
  gates a named kill).
- The shortcuts inventory.
- "The named gaps" — every absent/partial surface with its seam.
- "The surprises" — anything the docs get wrong about the live UI.

Cite everything file:line.
