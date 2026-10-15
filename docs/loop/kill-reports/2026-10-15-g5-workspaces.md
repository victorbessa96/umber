# Kill report: g5-workspaces (named dock layouts)

Date: 2026-10-15. Author: dragon. Lock: `g5-workspaces` over
`crates/umber-app/src/{workspaces,actions,toolbar,rich_tooltip,palette,main,shell_menu}.rs`,
both ftl catalogs and this report. Plan: `docs/specs/gui-plan-2026-10-15.md`
row `g5-workspaces` (B:A2.1-A2.3, P:A4).

## Built

- `workspaces.rs`: `Workspace { Paint, Bake, Export, Graph }`.
  - `preset(w)`: Paint is the boot layout. Bake and Export put their
    panel in a left column in front of the viewport. Graph puts the node
    graph in the centre. Every preset docks all 11 panels, so switching
    never loses one.
  - `Workspaces::switch` stores the outgoing workspace's live layout and
    brings back the incoming one's stored layout, or its preset the
    first time. Each workspace keeps the user's edits across switches.
  - `reset_active` gives the active workspace's preset.
  - `cycle` wraps both ways.
- `main.rs`:
  - The workspace tabs sit in the menu row after Help, as selectable
    labels.
  - Ctrl+PgDn / Ctrl+PgUp cycle through `NextWorkspace` /
    `PrevWorkspace`. Both are registry actions, so they also show in the
    palette and tooltips, and pass a text field.
  - A switch leaves any focus mode first, so the stored layout is the
    real one.
  - Window ▸ Reset Layout now resets the ACTIVE workspace to its own
    preset.
- Catalogs: a `workspace` block (four labels + next/prev), both locales.

## Tests (5, `workspaces::tests`)

Every preset docks every panel exactly once. Bake, Export and Graph each
show their own panel as an active tab. Cycling wraps both ways and
returns after four steps. Switching remembers each workspace's edits
(Paint's closed History and Bake's closed Graph both survive a round
trip). Same-workspace switch is a no-op. The labels exist in both
catalogs. Gate 1 (umber-app): fmt, clippy, 455 tests.

## Judgment calls

- **No F1-F4 viewport presets.** The plan row lists them, but F3 is
  already bound to the command palette (g3-command-palette, a sibling
  kill), and F1 is Help everywhere else. Remapping F3 is a keymap
  decision, so it waits for the keymap editor (g3-keymap-editor), where
  a preset can carry it. Named in Deferred.
- **No persistence across launches.** `DockState` serialization needs
  egui_dock's `serde` feature and a `Panel: Serialize` derive, which is
  a dependency-feature change in the workspace manifest, outside this
  surface. In-session memory is the honest v1, and the unit round-trip
  the plan asks for comes with the persistence kill.
- **Tabs, not a combo.** The plan says "tabs in the menu row", which is
  Blender's top bar.

## Deferred

- Persisting workspace layouts in the app config (egui_dock `serde`).
- The F1-F4 viewport presets, after the keymap editor decides F3.
- A kittest that switching workspaces swaps the drawn panels.
