# GUI research — Blender interaction patterns worth adopting (2026-10-15)

Scope: Blender is the source for interaction patterns and design principles, not
feature parity. Umber's users come from Substance Painter. Each item has three parts:
what Blender does, the source, and why to adopt it for Umber. The context is the
brief's "What Umber's GUI is today" section: egui/egui_dock shell, an 11-panel dock,
a toolbar with 9 single-key shortcuts, a status bar, the `umber-gui` widgets
(`param_group`, `value_field`, `tool_row`, `swatch_grid`), and these gaps: 22 raw
sliders remaining, no brush cursor, no rich tooltips, a stock color picker, no
onboarding, and no stroke-method/cavity UI.

Citation key: `M/` = `https://docs.blender.org/manual/en/latest/`. All sources come
from the pre-fetched corpus at `~/Projects/.umber-gui-corpus-2026-10-15/` and were
read directly. `[inferred]` marks a claim that is not stated in the cited text,
including any Umber-side mapping that goes beyond a direct reading.

Blender HIG (one fetch): https://developer.blender.org/docs/features/interface/human_interface_guidelines/
The page fetched is only an index. It lists Design Paradigms, General Patterns,
Components, User Feedback, and Glossary, and says it is "still in development". Its
content pages were not fetched (budget), so the HIG supports no claims below.

---

## A. Window system

### A1. Areas, regions, docking

- A1.1 Areas are rectangles that each host one editor. Many shortcuts are scoped to
  the editor under the mouse, not to the focused editor. Example: Ctrl-J in the
  Outliner does nothing. — `M/interface/window_system/areas.html`
  Adopt because: hover-scoped keymaps let 2D UV and 3D share keys (F = frame, etc.)
  without collisions. [inferred: egui_dock gives hovered-tab rect; routing is ours]
- A1.2 Resizing an area border: Ctrl snaps to convenient sizes, and Shift moves nearby
  aligned borders together. — `M/interface/window_system/areas.html`
  Adopt because: these modifier semantics are cheap to add to dock splitters.
- A1.3 Dragging from an area corner splits or joins areas. Esc/RMB cancels the drag,
  and the areas about to be joined are drawn brighter. — `M/interface/window_system/areas.html`
  Adopt because: any drag-to-dock needs a preview and cancel (egui_dock has preview;
  verify Esc-cancel). [inferred]
- A1.4 Ctrl-Space maximizes an area and keeps the menu bar and status bar.
  Ctrl-Alt-Space is Focus Mode, which also hides the topbar, status bar, and
  toolbars/sidebars. A hover-only icon restores the layout. —
  `M/interface/window_system/areas.html`
  Adopt because: painters want a full-viewport mode. Make it a single toggle and
  keep a visible way out.
- A1.5 "Duplicate Area into New Window" exists for multi-monitor setups, and
  Shift-dragging a corner tears the area off. — `M/interface/window_system/areas.html`
  Adopt because: a 3D view and a UV view on separate monitors is a common Painter
  setup. [inferred: egui multi-viewport support needed]
- A1.6 Each editor is split into regions: Header, Main, Toolbar (T), Tool Settings
  strip, Sidebar (N), Adjust-Last-Operation HUD, and Footer. —
  `M/interface/window_system/regions.html`
  Adopt because: this vocabulary fits Umber's Viewport. The toolbar is a left strip,
  tool settings sit in a strip under the header, and an N-sidebar holds per-view
  options.
- A1.7 The header context menu can hide the header, flip it to the top or bottom,
  collapse its menus into an icon, and toggle the tool-settings strip. —
  `M/interface/window_system/regions.html`, `M/interface/controls/buttons/menus.html`
  Adopt because: collapsing menus saves space on narrow docks.
- A1.8 A region can be hidden by resizing it to zero. It then leaves a small arrow
  that brings it back. — `M/interface/window_system/regions.html`
  Adopt because: hidden UI must stay discoverable, so never hide a panel without
  leaving an affordance.
- A1.9 Ctrl-MMB drag or Numpad +/- scales a region's contents (for example the
  toolbar), and Home resets the scale. — `M/interface/window_system/regions.html`
  Adopt because: this gives per-panel zoom (layer stack thumbnails) without a global
  UI scale. [inferred]
- A1.10 The "Region Overlap" preference draws the toolbar and sidebar over the
  viewport instead of shrinking it. — `M/editors/preferences/interface.html`
  Adopt because: the 3D view keeps its full size while tool strips float.
- A1.11 The asset shelf is a region with Ctrl-F search, catalog tabs, a size slider,
  a names toggle, and a "By Active Tool" filter. — `M/interface/window_system/regions.html`
  Adopt because: a brush and smart-material shelf at the bottom of the viewport,
  filtered to the active tool, is Painter-like. It complements the Assets panel.

### A2. Workspaces

- A2.1 Workspaces are layouts of areas and editors stored as tabs in the topbar.
  Blender ships task presets (Texture Paint, UV Editing, Shading, …). A tab can be
  renamed by double-clicking, and has Duplicate/Delete/Reorder entries. Ctrl-PgUp/PgDn
  cycles workspaces. — `M/interface/window_system/workspaces.html`
  Adopt because: named dock presets ("Paint", "Bake", "Export", "Graph") would
  replace one overloaded default layout.
- A2.2 A workspace can pin a Mode that it switches to on activation. —
  `M/interface/window_system/workspaces.html`
  Adopt because: switching to "Bake" could also activate the bake panel and its tool
  set. [inferred mapping]
- A2.3 A custom set of workspaces can be saved as part of the Defaults (startup
  file). "Load UI" decides whether a file's layout overrides yours. —
  `M/interface/window_system/workspaces.html`, `M/interface/window_system/topbar.html`
  Adopt because: dock layout should be a user preference, not something stored in
  each project.
- A2.4 The Texture Paint workspace opens a 3D view in paint mode next to an Image
  Editor in paint mode. Strokes in either view show up immediately in the other. —
  `M/sculpt_paint/texture_paint/introduction.html`
  Adopt because: this confirms Umber's centre 3D + UV tabs. Offer a side-by-side
  split preset.

### A3. Topbar

- A3.1 The topbar holds the main menus (File/Edit/Render/Window/Help), the workspace
  tabs, and the Scene/View-Layer data-block selectors. — `M/interface/window_system/topbar.html`
  Adopt because: workspace tabs belong in the menu-bar row. This saves a row.
- A3.2 File menu entries: Open Recent shows a hover preview and file info, there is
  Revert and Recover (Last Session / Auto Save), and Save Incremental (Ctrl-Alt-S)
  never overwrites. — `M/interface/window_system/topbar.html`
  Adopt because: recent-file previews, crash recovery, and incremental save are
  cheap trust builders for long paint sessions.
- A3.3 Edit menu entries: Undo, Redo, Undo History, Adjust Last Operation, Repeat
  Last, Repeat History, Menu Search, Preferences (Ctrl-,). —
  `M/interface/window_system/topbar.html`
  Adopt because: this is a canonical Edit menu checklist.
- A3.4 The About box shows version, build date, git hash, branch, and windowing
  environment (Wayland/X11). — `M/interface/window_system/topbar.html`
  Adopt because: support reports become actionable. Umber runs on wgpu, so add the
  backend and adapter. [inferred]
- A3.5 Clean Up → "Manage Unused Data" opens a popup that lists orphaned data-blocks
  before they are purged. — `M/interface/window_system/topbar.html`
  Adopt because: unused texture sets and bakes should be reviewed before a purge, not
  dropped silently. [inferred mapping]

### A4. Status bar

- A4.1 The left side of the status bar shows mouse-button hints and the active
  tool's keymap. These are context-sensitive and change during modal operations. —
  `M/interface/window_system/status_bar.html`, `M/interface/tool_system.html`
  Adopt because: Umber's status bar is the natural home for "LMB paint · Shift line
  · Ctrl secondary color · F resize". This is the top-priority item for Painter
  migrants.
- A4.2 The middle of the status bar shows the running task's progress (bake). Hover
  shows a time estimate, there is a cancel button, and transient report messages
  fade and open the full log when clicked. — `M/interface/window_system/status_bar.html`
  Adopt because: one place for bake and export progress, with cancel. See E.
- A4.3 The right side is chosen by RMB on the bar: scene statistics, memory, version.
  — `M/interface/window_system/status_bar.html`, `M/editors/preferences/interface.html`
  Adopt because: a user-pickable right side (VRAM, texture-set resolution, triangle
  count) keeps the bar uncluttered.
- A4.4 Tapping Alt shows a tool-key prompt in the status bar. The next keypress picks
  a tool, and Alt again cancels. — `M/interface/tool_system.html`,
  `M/editors/preferences/keymap.html`
  Adopt because: it teaches the 9 tool keys without a cheat sheet. Make it optional.

### A5. Toolbar and tool system

- A5.1 Each workspace and mode has one active tool, which is remembered when you
  leave the mode and come back. — `M/interface/tool_system.html`
  Adopt because: returning from UV view to 3D should restore the last tool.
- A5.2 Tool groups show a corner triangle. Holding LMB (or dragging) opens the group.
  A short hover shows the name and a longer hover shows the full tooltip. —
  `M/interface/tool_system.html`
  Adopt because: grouping (Brush/Eraser/Smudge, or the selection tools) keeps
  `tool_row` short. Two-stage hover is a good tooltip delay model.
- A5.3 Widening the toolbar switches it to two icon columns, then to icon + label. —
  `M/interface/tool_system.html`
  Adopt because: responsive labels help newcomers at no cost to experts.
- A5.4 Shift-Space opens a pop-up toolbar at the cursor with each tool's key shown.
  The Spacebar can be configured as a modifier (Space+T …). —
  `M/interface/tool_system.html`, `M/editors/preferences/keymap.html`
  Adopt because: tool access at the cursor keeps the eyes on the canvas.
- A5.5 Binding a key to a tool group with "Cycle" makes repeated presses step through
  the group's tools. — `M/interface/tool_system.html`
  Adopt because: one key can cover a family (for example S cycles selection tools).
- A5.6 The Q "Quick Favorites" popup is user-curated: any tool or menu item can be
  added through its context menu. — `M/interface/tool_system.html`
  Adopt because: a personal menu cuts down on menu diving, and it is cheap.
- A5.7 Tool settings appear in three places that stay in sync: the header
  tool-settings strip, Sidebar → Tool, and the Properties → Active Tool tab. —
  `M/interface/tool_system.html`
  Adopt because: show size, strength, and stroke in a viewport strip, and the full
  brush in Properties. [inferred: one source of truth, many views]

### A6. Tabs and panels

- A6.1 Ctrl-Wheel anywhere switches vertical tabs. Ctrl-Tab and Shift-Ctrl-Tab
  cycle them, and dragging LMB across the tab icons scrubs through them. —
  `M/interface/window_system/tabs_panels.html`
  Adopt because: Umber's left-column tab stack (sets/bakes/export/graph/display)
  needs fast switching.
- A6.2 Panels collapse when the header is clicked, or with A over a panel.
  Ctrl-click a header to solo that panel (collapse all others), and drag across
  headers to toggle many at once. — `M/interface/window_system/tabs_panels.html`
  Adopt because: `param_group` should support solo and drag-collapse, which matters
  in the long Properties panel.
- A6.3 Panels can be reordered by dragging the "::::" grip on the header. —
  `M/interface/window_system/tabs_panels.html`
  Adopt because: users can reorder brush sections (stroke above texture). Persist
  the order.
- A6.4 Pinning a panel keeps it visible across tabs (Sidebar only). —
  `M/interface/window_system/tabs_panels.html`
  Adopt because: a pinned "Brush" group stays visible while browsing the layer tab.
  [inferred mapping]
- A6.5 Panel presets: a header menu to apply, add, or remove named value sets,
  stored as plain files. — `M/interface/window_system/tabs_panels.html`
  Adopt because: export and bake panels need named presets ("UE5 packed ORM").
  Plain files can be shared.

### A7. Pie menus

- A7.1 A pie menu spreads its items radially around the cursor. The fast path is
  press-hold, flick, release, which picks the closest item. A tap leaves the menu
  open for clicking. A centre disc shows the direction, items have underlined
  accelerators and number keys, and a "+" icon marks a sub-pie. —
  `M/interface/controls/buttons/menus.html`
  (The dedicated `…/pie_menus.html` page is 404 in the corpus.)
  Adopt because: Painter users lack these, but a pie for blend mode, channel, or
  view would be a differentiator. Consider it optional.
- A7.2 Pie tuning preferences: animation timeout, tap-key timeout, recenter timeout,
  radius, selection threshold, confirm threshold. — `M/editors/preferences/interface.html`
  Adopt because: these are the parameters to expose if pies ship.
- A7.3 Click vs. drag on the same key, for example Tab to toggle and Tab-drag to open
  a pie of options. — `M/advanced/keymap_editing.html`
  Adopt because: one key can do two things without modifier soup.

### A8. Operator / menu search (F3)

- A8.1 F3 Menu Search lets you type part of a name, navigate with arrows and Return,
  and each result shows the menu path where the command lives. —
  `M/interface/operators.html`
  Adopt because: a command palette over Umber's menus that shows the menu path
  teaches menu locations. (`…/templates/operator_search.html` is 404 in the corpus.)
- A8.2 A separate developer-only "Operator Search" covers unexposed operators. —
  `M/interface/operators.html`
  Adopt because: debug commands can live behind a dev flag instead of cluttering
  menus.
- A8.3 Search results can be sorted most-recent-first. — `M/editors/preferences/interface.html`
  Adopt because: recency ranking makes the palette fast for repeated actions.
- A8.4 Pressing Space inside any open popup menu starts type-to-search over that
  menu. — `M/interface/controls/buttons/menus.html`
  Adopt because: long menus (blend modes, export presets) become searchable.
- A8.5 The Spacebar action preference offers Play / Tools / Search. The manual
  recommends Search for newcomers. — `M/editors/preferences/keymap.html`
  Adopt because: an onboarding option could bind Space to the palette.

### A9. Splash and onboarding

- A9.1 The splash offers New (from template), Recent Files, Open, Recover Last
  Session, and What's New. Click outside or press Esc to dismiss it, and reopen it
  from the app menu. — `M/interface/window_system/splash.html`
  Adopt because: this maps onto Umber's existing empty-state hero. Add
  recent-projects and recover entries.
- A9.2 On first run (or a version upgrade) the splash becomes Quick Setup: language,
  light/dark theme, keymap preset (Blender / Industry Compatible), select-mouse
  button, Spacebar action, and "Import Preferences From Previous Version". —
  `M/getting_started/configuration/defaults.html`,
  `M/interface/window_system/splash.html`
  Adopt because: first-run picks for language (en/pt-BR), theme, keymap preset
  ("Painter-like"), and navigation style are the cheapest way to win migrants.
  [inferred: the snippet ended at the keymap list. Select-mouse and spacebar are
  shown in the Keymap prefs, and their presence in Quick Setup is from memory, not
  verified in corpus]
- A9.3 A "Splash Screen" preference toggles the splash at startup. —
  `M/editors/preferences/interface.html`
  Adopt because: experts can switch it off.

### A10. Undo, history, adjust-last

- A10.1 Undo History lists actions with the newest on top and a dot on the current
  state. Clicking any entry jumps to it, and the first new edit truncates the
  future. — `M/interface/undo_redo.html`
  Adopt because: this is the semantics Umber's History panel should show (current
  marker, greyed future, truncation on new edit).
- A10.2 Adjust Last Operation (F9 / bottom-left HUD) re-tweaks the last operator's
  parameters after it has run. — `M/interface/undo_redo.html`,
  `M/interface/operators.html`, `M/interface/window_system/regions.html`
  Adopt because: fill/project/bake-type operations ("Fill layer → tweak tiling")
  without a modal dialog first. [inferred mapping]
- A10.3 Repeat Last (Shift-R) and Repeat History. — `M/interface/undo_redo.html`
  Adopt because: low priority. Useful for repeated "add fill layer with preset".
- A10.4 Session undo is lost on quit even if the file is saved (stated as a caveat).
  — `M/interface/undo_redo.html`
  Adopt because: Umber should state its own persistence rule in the History panel.
  [inferred]
- A10.5 Modal operators: Esc/RMB cancels, Return/LMB confirms. Header slider
  operators snap 10% with Ctrl and go fine with Shift, and E toggles overshoot past
  0-100%. — `M/interface/operators.html`
  Adopt because: a uniform confirm/cancel contract for every interactive op (brush
  resize, gradient drag, stencil move).

---

## B. Properties editor, lists, data-blocks

- B1 The Properties editor is a vertical icon tab bar ordered by context (Active
  Tool → Scene → Object → Data → Material/Texture). Tabs only show when relevant.
  — `M/editors/properties_editor.html`
  Adopt because: Umber's Properties panel can switch by context (Brush / Layer / Fill
  / Mask / Texture set) on a vertical icon rail. Hide tabs that don't apply.
- B2 "Visible Tabs" lets the user hide tab categories per workspace. —
  `M/editors/properties_editor.html`
  Adopt because: a Bake workspace hides brush tabs.
- B3 Display Filter (Ctrl-F / Alt-F clears) searches properties by name, greys
  non-matches, jumps to the tab of the first match, and highlights it. —
  `M/editors/properties_editor.html`
  Adopt because: property search across the long postfx/fill panels (22 sliders) is
  high value and low cost.
- B4 A data-context breadcrumb ("Cube → Mesh → Material") is shown in the header. —
  `M/editors/properties_editor.html`
  Adopt because: "TextureSet ▸ Layer 3 ▸ Mask ▸ Generator" tells the user what
  they are editing.
- B5 A Pin ID toggle locks the editor to the current data-block even when the
  selection changes. — `M/editors/properties_editor.html`
  Adopt because: pin a fill layer's properties while selecting other layers.
- B6 List View: click to select, double-click or Ctrl-click to rename, a "::::"
  resize handle, and a hidden filter drawer with Ctrl-F search, invert, sort by
  name, and reverse. The right-hand column has + / − / specials ▾ / ↑ ↓. —
  `M/interface/controls/templates/list_view.html`
  Adopt because: a standard list widget for texture sets, bakes, export presets,
  palettes, and UV sets. Build it once in `umber-gui`.
- B7 Tree View adds disclosure triangles, F2 rename, drag reordering and reparenting,
  RMB per-item menus, the same filter and sort, and the same side buttons. —
  `M/interface/controls/templates/tree_view.html`
  Adopt because: this is the layer stack's interaction contract (groups, masks as
  children, drag-reparent).
- B8 Data-block menu: type icon (drag it to assign), a dropdown with previews,
  search, and state icons, an editable name, a user-count button (click to make a
  single-user copy), a fake-user shield, New/duplicate, Open, and Unlink
  (Shift-click deletes fully). — `M/interface/controls/templates/data_block.html`
  Adopt because: a "material/smart-material/texture slot" picker with previews and
  drag-to-assign. The user count makes instancing visible (shared fill sources).
  [inferred mapping]
- B9 Names starting with "." are hidden from lists unless the search starts with ".".
  — `M/interface/controls/templates/data_block.html`, `M/editors/preferences/interface.html`
  Adopt because: a convention for internal/helper resources. Low priority.
- B10 Data ID field: a select-only reference field whose name box is a search with
  Tab-autocomplete, plus an optional eyedropper and a clear (×) button. —
  `M/interface/controls/templates/data_block.html`
  Adopt because: mask-source, anchor-point, and bake-target references get a compact
  field.
- B11 Data-block previews are shown as large images in the dropdown. —
  `M/interface/controls/templates/data_block.html`
  Adopt because: material and brush pickers must be visual (Painter shelf
  expectations).
- B12 Brush assets: Duplicate Asset (name/library/catalog), Save Changes to Asset,
  Revert to Asset, Edit Metadata, Edit Preview Image. Brush edits are temporary
  until saved, and bundled "Essentials" are read-only. —
  `M/sculpt_paint/brush/brush_management.html`, `M/sculpt_paint/brush/brush_settings.html`
  Adopt because: an explicit dirty/save/revert model for brush presets, so a stray
  tweak never corrupts the library. Show a dirty marker. [inferred]

---

## C. 3D view, navigation, paint-tool UI, image editor

### C1. Navigation and navigation preferences

- C1.1 Defaults: MMB orbit, Shift-MMB pan, Ctrl-MMB or Wheel zoom. RMB cancels any
  navigation drag. — `M/editors/3dview/navigate/navigation.html`
  Adopt because: an RMB-cancel during an orbit or pan restores the view, which makes
  navigation forgiving.
- C1.2 Alt-MMB click sets the orbit pivot at the clicked point. Alt-MMB drag snaps
  the view to an axis in ortho. Holding Alt mid-orbit snaps to axes and diagonals. —
  `M/editors/3dview/navigate/navigation.html`, `M/editors/3dview/navigate/viewpoint.html`
  Adopt because: click-to-set-pivot is essential on large meshes when painting
  details.
- C1.3 Frame All (Home), Frame Selected, and Frame Last Stroke (Numpad . in paint
  modes) re-centre the view. — `M/editors/3dview/navigate/navigation.html`
  Adopt because: "frame last stroke" is a paint-specific gem, and F for frame-mesh is
  standard.
- C1.4 Zoom Region (Shift-B box) and Dolly (Shift-Ctrl-MMB) move past the point of
  interest. — `M/editors/3dview/navigate/navigation.html`
  Adopt because: dolly fixes the "can't zoom closer" wall on small UV islands.
- C1.5 Navigation preferences: Orbit Method (Turntable/Trackball), Orbit
  Sensitivity, Orbit Around Selection (in paint modes this is the centre of the last
  stroke), Auto-Perspective, Auto-Depth, Smooth View (ms; 0 disables), and the
  numpad Rotation Angle. — `M/editors/preferences/navigation.html`
  Adopt because: Auto-Depth plus Zoom-to-Mouse plus orbit-around-last-stroke is the
  best texture-painting navigation set, and Smooth View respects Reduce Motion.
- C1.6 Zoom preferences: Zoom Method (Scale/Continue/Dolly), Zoom Axis, Zoom to Mouse
  Position, and separate invert toggles for mouse and wheel. —
  `M/editors/preferences/navigation.html`
  Adopt because: wheel direction and zoom-to-cursor are the first things migrants
  look for.
- C1.7 Industry Compatible keymap: Alt-LMB orbit, Alt-MMB pan, Alt-RMB zoom, F frame
  selected, A frame all, F1-F4 views. — `M/interface/keymap/industry_compatible.html`
  Adopt because: Painter uses the same Alt+mouse (Maya) scheme. This should be
  Umber's default, with a Blender-style preset as the alternative. [inferred:
  Painter's scheme from domain knowledge, not this corpus]
- C1.8 The "Middle Mouse Action" preference swaps orbit and pan on MMB, and
  "Emulate 3 Button Mouse" maps Alt-LMB to MMB. —
  `M/editors/preferences/keymap.html`, `M/editors/preferences/input.html`
  Adopt because: laptop and tablet users need a no-MMB path.

### C2. Navigation gizmo

- C2.1 The navigation gizmo (top right) has an orbit ball with axis labels. Click a
  label to align to that axis, click it again for the opposite side, and drag the
  ball to orbit. Four buttons below handle zoom, pan, camera, and ortho/persp. —
  `M/editors/3dview/navigate/introduction.html`
  Adopt because: it gives pen and touch users navigation without keys and teaches
  orientation.
- C2.2 Preferences control gizmo size, the axes style (Interactive / Simple / Off),
  and "Show Navigation Controls" (off for experts). —
  `M/editors/preferences/viewport.html`, `M/editors/preferences/interface.html`
  Adopt because: experts can hide the gizmo.
- C2.3 Transform gizmo modifiers: Ctrl toggles snap, Shift after press goes fine,
  Shift before press plane-locks. The activation event (Press vs Drag) is a
  preference. — `M/editors/3dview/display/gizmo.html`, `M/editors/preferences/keymap.html`
  Adopt because: the same modifier semantics apply to Umber's stencil, projection,
  and anchor gizmos. [inferred]

### C3. Overlays

- C3.1 The header has one Overlays toggle plus a popover of individual overlays.
  Mode-specific overlays get a second popover. — `M/editors/3dview/display/overlays.html`
  Adopt because: Umber's Display panel could become a viewport-header popover
  (wireframe, UV seams, texel density, …) with one master toggle.
- C3.2 Text Info overlay (view name, object/data info) and Statistics (counts). —
  `M/editors/3dview/display/overlays.html`, `M/editors/preferences/viewport.html`
  Adopt because: a corner readout shows the active texture set, channel, resolution,
  and view.
- C3.3 The wireframe overlay has a threshold slider that hides edges on flat areas,
  plus an opacity control. — `M/editors/3dview/display/overlays.html`
  Adopt because: a wireframe-over-paint view is usable on dense meshes.
- C3.4 Face Orientation draws backfaces in red. — `M/editors/3dview/display/overlays.html`
  Adopt because: it diagnoses flipped normals before baking.
- C3.5 Texture-paint overlay: Stencil Mask Opacity. —
  `M/editors/3dview/display/overlays.html`
  Adopt because: a stencil and mask overlay alpha control is needed when stencils
  land.
- C3.6 Reference Spheres (glossy and diffuse) in material preview. —
  `M/editors/3dview/display/overlays.html`
  Adopt because: a lookdev aid next to the paint target. Low priority.

### C4. Brush cursor and in-viewport adjustment

- C4.1 The brush cursor is a circle whose radius matches the brush. It has
  configurable add and inverse (subtract) colors, an opacity-textured overlay that
  shows falloff and brush texture, an alpha slider, and "Override Overlay" to hide
  it during strokes. — `M/sculpt_paint/brush/cursor.html`
  Adopt because: Umber has no in-viewport cursor today. A ring plus a falloff/texture
  preview inside it is the minimum viable cursor.
- C4.2 Pressing F drags the size interactively with the brush texture visible inside
  the ring, LMB accepts, and number keys type a value. Shift-F does the same for
  strength. [ and ] step the size. — `M/sculpt_paint/brush/brush_settings.html`
  Adopt because: Painter users expect drag-to-resize (Ctrl+RMB style in Painter).
  Offer F as well, with a numeric readout. [inferred: Painter binding not from
  corpus]
- C4.3 Ctrl-F rotates the texture angle interactively and also accepts typed values.
  — `M/sculpt_paint/brush/texture.html`
  Adopt because: one interaction contract for all in-viewport brush scalars.

### C5. Texture-paint tool settings and slots

- C5.1 The Texture Slots panel has two modes. Material mode auto-lists the image
  nodes of the materials as a list view. Single Image mode takes an image + UV map.
  Selecting a slot also shows it in the Image Editor. —
  `M/sculpt_paint/texture_paint/tool_settings/texture_slots.html`
  Adopt because: confirms one active paint target per channel, and selecting it
  syncs the UV view. Maps onto TextureSets plus the active channel.
- C5.2 "Save All Images", plus a header asterisk when an image is unsaved. —
  `M/sculpt_paint/texture_paint/tool_settings/texture_slots.html`,
  `M/sculpt_paint/texture_paint/introduction.html`
  Adopt because: dirty indicators on texture sets and the project title.
- C5.3 A missing UV map triggers a warning plus an "Add Simple UVs" quick fix. —
  `M/sculpt_paint/texture_paint/introduction.html`,
  `M/sculpt_paint/texture_paint/tool_settings/texture_slots.html`
  Adopt because: an empty or invalid state should offer the fix button, not only the
  error.
- C5.4 The tool-settings section index (Brushes, Mask, Stencil Mask, Cavity Mask,
  Symmetry, Options, External, Tiling) is grouped as collapsible panels. —
  `M/sculpt_paint/texture_paint/tool_settings/index.html`
  Adopt because: a ready-made panel taxonomy for Umber's brush Properties. The Cavity
  Mask panel is where the landed cavity module gets its UI.
- C5.5 Brush types: Draw, Soften (blur/sharpen with kernel radius), Smear, Clone
  (Ctrl-LMB sets the source), Fill (threshold, gradient linear/radial by dragging),
  Mask. — `M/sculpt_paint/texture_paint/brushes.html`
  Adopt because: a cross-check list for Umber's tool row. Gradient fill by drag is a
  good direct-manipulation pattern.
- C5.6 Sample Color: Shift-X samples the surface color and Shift-Ctrl-X samples the
  merged/lit viewport color. — `M/sculpt_paint/texture_paint/editing.html`
  Adopt because: the distinction between the layer value and the final composite
  needs two picks. Map these to Umber's I tool plus a modifier.
- C5.7 Brushes have primary and secondary colors. Ctrl while painting uses the
  secondary color, and X swaps them. "Unified" toggles share size, strength, and
  color across brushes. — `M/sculpt_paint/brush/brush_settings.html`
  Adopt because: a fg/bg color pair plus unified size and strength (on by default)
  is what Painter users expect. [inferred: Painter parity from domain knowledge]
- C5.8 Randomize color by H/S/V, each with a per-stroke or per-dab toggle and a
  pressure toggle. Gradient color source modes: Pressure / Clamp / Repeat. —
  `M/sculpt_paint/brush/brush_settings.html`
  Adopt because: a compact per-channel jitter row layout (value + two icon toggles).
- C5.9 Each pressure-capable param has an inline pressure icon toggle plus an
  expand arrow that reveals that param's own pressure curve. —
  `M/sculpt_paint/brush/brush_settings.html`, `M/sculpt_paint/brush/stroke.html`
  Adopt because: progressive disclosure of pressure curves is a pattern
  `value_field` should support (icon toggle + optional curve drawer).

### C6. Falloff, stroke, texture

- C6.1 The falloff panel has a curve preset dropdown (Smooth, Smoother, Sphere, Root,
  Sharp, Linear, Sharper, Inverse Square, Constant) and Custom, which reveals the
  curve widget. Front-face falloff by normal angle is included. —
  `M/sculpt_paint/brush/falloff.html`
  Adopt because: the preset thumbnails plus custom curve pattern. Normal-angle
  falloff is a must for projection painting.
- C6.2 Stroke Method (Alt-E menu): Dots / Drag Dot / Space (+Spacing) / Airbrush
  (+Rate) / Anchored / Line (Alt for 45°) / Curve. Each method reveals only its own
  params. — `M/sculpt_paint/brush/stroke.html`
  Adopt because: a direct template for the stroke-method UI (airbrush, dash, and
  drag-dot are landing with no UI). Use a dropdown with conditional params.
- C6.3 Dash Ratio and Dash Length apply to Space, Line, and Curve. —
  `M/sculpt_paint/brush/stroke.html`
  Adopt because: the dash UI parameters come with names and units.
- C6.4 Curve stroke: Ctrl-RMB adds control points, handles are dragged, G/R/S
  transform, X deletes, Return draws. The curve is saved as a reusable "Paint Curve"
  data-block. — `M/sculpt_paint/brush/stroke.html`
  Adopt because: reusable path strokes. Relates to Umber's path_tool. [inferred]
- C6.5 Jitter (with pressure and curve), input-sample smoothing, and Stabilize Stroke
  (Shift-S, radius + factor, "lazy mouse"). Adjust Strength for Spacing. —
  `M/sculpt_paint/brush/stroke.html`
  Adopt because: a lazy-mouse stabilizer is expected by painters, with a one-key
  toggle.
- C6.6 Brush texture mapping: View Plane / Area Plane / Tiled / 3D / Random /
  Stencil. Stencil is transformed in the viewport with RMB move, Shift-RMB scale,
  Ctrl-RMB rotate (Alt for the mask stencil), X/Y constrain scale. There are Image
  Aspect and Reset Transform actions, plus Rake and Random angle. —
  `M/sculpt_paint/brush/texture.html`
  Adopt because: a complete direct-manipulation spec for a stencil overlay (Painter
  has an equivalent stencil). Copy the reset/aspect affordances.
- C6.7 Texture and Texture Mask are separate panels with the same settings (color
  source vs. strength source). — `M/sculpt_paint/brush/texture.html`
  Adopt because: reuse one panel component for both.

### C7. Image editor (2D view)

- C7.1 The Sample tool: hold LMB and the footer shows XY, RGBA, color-managed RGB,
  HSV, and luminance, with a Sample Size averaging square. —
  `M/editors/image/introduction.html`
  Adopt because: a pixel inspector in the UV view footer helps debug bakes and
  channels.
- C7.2 Display Channels: Color & Alpha (checker), Color, Alpha (grey), Z, R/G/B
  single. — `M/editors/image/introduction.html`
  Adopt because: a per-channel solo in the UV view is essential for packed-ORM
  checks.
- C7.3 The Image Pin stops auto-switching to the selected object's texture (which
  only happens in paint mode). — `M/editors/image/introduction.html`
  Adopt because: pin the UV view to one channel or texture set while painting
  another.
- C7.4 Render slots (1/2/3, J/Alt-J cycle) keep multiple results for A/B
  comparison. — `M/editors/image/introduction.html`
  Adopt because: bake A/B comparison slots in the Bakes panel. [inferred mapping]
- C7.5 2D navigation reuses the 3D conventions (pan and zoom), and there are
  dedicated 2D navigate gizmos. — `M/editors/3dview/navigate/introduction.html`,
  `M/editors/image/introduction.html`
  Adopt because: UV view and 3D view must share one navigation keymap.

---

## D. Widgets

- D1 Number fields come in two kinds. Stepper fields show hover arrows (< >).
  Slider fields have a fill bar for bounded ranges. —
  `M/interface/controls/buttons/fields.html`
  Adopt because: replace the 22 raw `egui::Slider`s with a `value_field` that has a
  slider variant (fill bar) and a stepper variant.
- D2 Drag-scrub: LMB-drag horizontally changes the value, with Ctrl to snap steps
  and Shift for precision. Shift-Ctrl gives fine snapping. Ctrl/Shift are pressed
  after the drag starts. — `M/interface/controls/buttons/fields.html`,
  `M/interface/keymap/blender_default.html`
  Adopt because: this is the core `value_field` contract, and it must be identical
  everywhere.
- D3 Click (no drag) or Return enters text editing. Return or click-outside applies,
  Esc/RMB cancels, Tab/Shift-Tab moves to the next/previous field, and Minus over a
  field negates it. — `M/interface/controls/buttons/fields.html`
  Adopt because: Tab-chaining through fields is a big keyboard-efficiency win
  (tiling X/Y, offset).
- D4 Ctrl-Wheel over a field steps its value. Over an enum or dropdown it cycles the
  option, and over a toggle it flips it. —
  `M/interface/controls/buttons/fields.html`, `M/interface/controls/buttons/menus.html`,
  `M/interface/controls/buttons/buttons.html`
  Adopt because: Ctrl-Wheel avoids conflict with plain-wheel panel scrolling.
  Note: plain wheel must scroll the panel, not change values. [inferred rule]
- D5 Multi-value editing: press on the first field, drag vertically over others,
  then drag horizontally or type to set them all at once. —
  `M/interface/controls/buttons/fields.html`
  Adopt because: set tiling U/V or RGB together.
- D6 Drag-toggle: press on a checkbox and drag across others to set them all. —
  `M/interface/controls/buttons/buttons.html`
  Adopt because: channel enable toggles in Export and Bake grids.
- D7 Soft vs. hard limits: dragging is clamped to the soft range, while typing can go
  out to the hard range. — `M/interface/controls/buttons/fields.html`
  Adopt because: keeps sliders useful (0-100 brush size) while still allowing 4000
  via typing.
- D8 Expressions in fields (`3*2`, `sqrt(2)`, `pi`) and units (`1m 3mm`). —
  `M/interface/controls/buttons/fields.html`
  Adopt because: a tiny safe math evaluator. Pixel and percent units suit texel
  density and resolution. [inferred: Blender uses Python, Umber needs a Rust
  evaluator]
- D9 Hover shortcuts on any field: Ctrl-C/V copies one value, Ctrl-Alt-C/V copies a
  whole vector or color, Backspace resets to default, Alt applies to all selected. —
  `M/interface/keymap/blender_default.html`
  Adopt because: copy and reset on hover is a top power-user feature, and Alt
  multi-edit applies across selected layers.
- D10 The RMB context menu on any property has Reset to Default, Copy Data Path,
  Copy To Selected, Assign/Change/Remove Shortcut, Online Manual (F1), and Edit
  Source (dev). — `M/interface/controls/buttons/menus.html`
  Adopt because: one generic property context menu in `umber-gui`. "Assign Shortcut"
  from the menu makes keymap customization discoverable.
- D11 Decorators: small state icons right of a property (keyframe dot/rhombus,
  driver). — `M/interface/controls/buttons/decorators.html`
  Adopt because: no animation in Umber. Reuse the decorator column for "overridden
  vs. inherited from preset", "driven by graph node", or "differs from default".
  [inferred mapping]
- D12 Eyedropper (E over a button) samples a color anywhere in the window. Drag
  averages the colors passed over, and Space restarts the average. It can also pick
  objects or data and sample a line into a color ramp. —
  `M/interface/controls/buttons/eyedropper.html`
  Adopt because: drag-to-average sampling of noisy photo refs, plus a "pick object
  or texture-set from viewport" eyedropper on reference fields.
- D13 Eyedropper caveat: the view transform affects the sampled color, and sampling
  overlays is inaccurate. — `M/interface/controls/buttons/eyedropper.html`,
  `M/interface/controls/templates/color_picker.html`
  Adopt because: Umber must decide whether to sample the display-referred or the
  working value and say which in the UI.
- D14 The color picker is a popup with a 2D field + third-component slider (wheel
  adjusts V/L), a Linear/Perceptual readout toggle, RGB/HSV/HSL models, hex input
  with shorthand (FC0), an eyedropper, and alpha. Ctrl-drag snaps hue to 30°,
  Shift-drag goes fine, Backspace resets. —
  `M/interface/controls/templates/color_picker.html`
  Adopt because: this is the spec to replace egui's stock picker. The
  Linear/Perceptual toggle matters for PBR albedo values.
- D15 Picker types are a user preference: Circle HSV, Circle HSL, Square SV+H, HS+V,
  HV+S. — `M/interface/controls/templates/color_picker.html`,
  `M/editors/preferences/interface.html`
  Adopt because: painters have strong habits (square vs. wheel), so ship at least
  two.
- D16 A color field with alpha is split, with alpha shown over a checker. Fields can
  be dragged and dropped onto each other, and hover shows a large swatch with hex,
  RGBA, and HSVA. — `M/interface/controls/buttons/fields.html`
  Adopt because: the swatch tooltip and drag-drop between swatches fit `swatch_grid`.
- D17 The palette is a data-block selector with + / − / ↑ ↓ / sort by H/S/V/L.
  Clicking sets the primary color, Ctrl-click edits, Backspace resets. —
  `M/interface/controls/templates/color_palette.html`
  Adopt because: `swatch_grid` gains persistence as a named palette asset, with sort.
- D18 Curve widget: click to add a point, drag to move, X deletes. Zoom in/out with
  pan, clipping min/max, extend horizontal/extrapolated, handle types (auto, vector,
  auto-clamped …). Ctrl-C/V copies the whole curve between widgets, and brush curves
  get presets. — `M/interface/controls/templates/curve.html`
  Adopt because: one curve widget for falloff, pressure, and levels in postfx. Curve
  copy/paste and auto-clamped (no overshoot) are key.
- D19 Color ramp: stops with drag; Ctrl-click adds, and + / − buttons add and
  remove. An active-stop index field selects overlapping stops, with Position and
  Color fields for the active stop. Flip and distribute evenly / from left, an
  eyedropper samples a gradient, RGB/HSV/HSL interpolation is Linear / Ease /
  B-Spline / Cardinal / Constant, and hue direction is CW / CCW / Near / Far. —
  `M/interface/controls/templates/color_ramp.html`
  Adopt because: the gradient-map filter and generator UI. The active-stop index is
  an accessibility fallback for tiny targets.
- D20 Menus: number keys pick items (Alt-number for 11-20), underlined accelerators,
  arrows plus Return, close on mouse-leave, and every item shows its shortcut. —
  `M/interface/controls/buttons/menus.html`
  Adopt because: show the shortcut beside every menu entry (teaches keys) and keep
  menus keyboard-drivable.
- D21 Select menu vs. expanded enum (segmented row, active item highlighted,
  Shift-click for multi). Popovers are richer dropdowns that hold sliders. —
  `M/interface/controls/buttons/menus.html`
  Adopt because: rules for when to use a dropdown, a segmented row, or a popover in
  `umber-gui`. [inferred rule]
- D22 The direction button is a sphere you drag to set a direction, with Ctrl to snap.
  — `M/interface/controls/buttons/buttons.html`
  Adopt because: light direction for bake preview and the triplanar axis.
- D23 Text fields support standard editing keys (word-jump, word-delete, select-all,
  clipboard). Multi-line boxes have a resize grip and Shift-Enter for a newline. —
  `M/interface/controls/buttons/fields.html`
  Adopt because: a parity checklist for egui TextEdit in layer names and notes.

---

## E. Feedback

- E1 Reports appear as transient status-bar messages that fade. Clicking one opens
  the full log in the Info editor. — `M/interface/window_system/status_bar.html`
  Adopt because: transient toasts plus a persistent, clickable log panel. Errors must
  not vanish without trace.
- E2 "Report Missing Files" warns in the header, and no warning means none are
  missing. — `M/interface/window_system/topbar.html`
  Adopt because: a missing-texture/asset check on project open.
- E3 Progress shows in the status bar with a bar, a hover time estimate, and a cancel
  button. — `M/interface/window_system/status_bar.html`
  Adopt because: bake and export jobs need ETA and cancel in one consistent place.
- E4 Tooltips explain the control and show its hotkey. A preference turns them off,
  and holding Alt while hovering forces one. — `M/editors/preferences/interface.html`
  Adopt because: rich tooltips (Phase 2 gap) need description + shortcut + an off
  switch.
- E5 Python Tooltips (preference) add the API path below the tooltip. Developer
  Extras add icon names and "Copy Python Command". —
  `M/editors/preferences/interface.html`, `M/advanced/keymap_editing.html`
  Adopt because: an optional "dev tooltip" line with the command ID or settings key
  (for keymap editing, CLI parity, and bug reports). Umber's analogue is the
  operator/command id. [inferred]
- E6 Copy the command of a hovered operator button (Ctrl-C) or a field's data path
  (Shift-Ctrl-C). — `M/interface/keymap/blender_default.html`
  Adopt because: a cheap bridge to Umber's CLI and scripting, and useful in bug
  reports.
- E7 F1 over a control opens the manual page for it. —
  `M/interface/controls/buttons/menus.html`, `M/interface/keymap/blender_default.html`
  Adopt because: context help once Umber has docs.
- E8 Viewport text info turns yellow on a keyframe and FPS turns red when below
  target. — `M/editors/3dview/display/overlays.html`
  Adopt because: color-coded status in the corner readout (red when GPU frame time
  exceeds budget). [inferred mapping]
- E9 Panel/region transitions honor Smooth View and the Reduce Motion accessibility
  preference. — `M/editors/preferences/navigation.html`,
  `M/editors/preferences/interface.html`
  Adopt because: Umber's motion uses (currently 3) should check a reduce-motion
  setting.
- E10 Destructive asset ops (Delete Asset) have a confirm popup. X deletes with
  confirmation and Delete deletes without it. —
  `M/sculpt_paint/brush/brush_management.html`, `M/interface/keymap/blender_default.html`
  Adopt because: a two-key convention (confirm vs. immediate) for layer and asset
  deletion.

---

## F. Customisation

- F1 The keymap editor offers presets (Blender, 2.7x, Industry Compatible) plus add,
  remove, import, and export custom configs. Export can be "modified only" (a keymap
  delta) or all. — `M/editors/preferences/keymap.html`
  Adopt because: ship "Umber default (Painter-like)" and "Blender-like" presets.
  Export deltas keep user keymaps small and robust to updates.
- F2 Keymap filtering by Name or by Key Binding (type "ctrl n" to see what is
  bound). — `M/editors/preferences/keymap.html`
  Adopt because: reverse lookup ("what does Ctrl-D do?") is the most-used keymap
  feature. [inferred]
- F3 Keymap behavior preferences: select mouse button, Spacebar action, gizmo
  press/drag, Tool Keys (immediate vs. activate tool), Alt tool prompt, Alt tool
  access, Select All toggles, N region pie, Tab for pie, Pie on drag, MMB
  orbit/pan, Alt-MMB relative/absolute, tilde action. —
  `M/editors/preferences/keymap.html`
  Adopt because: expose a few high-leverage switches (MMB action, Space action, tool
  keys) instead of making users rebind.
- F4 F5-F8 (and F10) are reserved for users, and the OSKey and Hyper modifiers are
  never used by defaults. — `M/advanced/keymap_editing.html`,
  `M/interface/keymap/blender_default.html`
  Adopt because: reserve a documented free key range in Umber's default keymap.
- F5 Generic bindable operations: call any menu, pie, or panel (`wm.call_menu`,
  `wm.call_menu_pie`, `wm.call_panel`), and `wm.context_toggle` / cycle for any
  property without writing a dedicated operator. — `M/advanced/keymap_editing.html`
  Adopt because: a "toggle setting X" / "open panel Y" command family makes every
  setting bindable with no new code per setting.
- F6 Assign a shortcut from a button's RMB menu. — `M/interface/controls/buttons/menus.html`
  Adopt because: binding without opening the keymap editor.
- F7 Industry Compatible key set: 1-3 component modes, Tab = menu search, Shift-Tab =
  favorites, Return = rename, Ctrl-[ / Ctrl-] = toolbar and sidebar, Backspace deletes
  with confirm, Delete deletes without, W/E/R = move/rotate/scale, Q = select tools.
  — `M/interface/keymap/industry_compatible.html`
  Adopt because: Industry Compatible is a good baseline for Umber's preset. Check it
  against Umber's existing W/G/V/X/I/E/Z/S/O tool keys for conflicts. [inferred]
- F8 Themes: per-editor color sets edited live, presets saved as XML (add, remove,
  install, reset), and bundled Light/Dark. — `M/editors/preferences/themes.html`
  Adopt because: Umber's palette tokens in `theme.rs` should serialize to a
  shareable file (TOML/RON), with live-edit and reset. [inferred format]
- F9 Resolution Scale (relative to detected DPI), Line Width (thin/default/thick),
  interface and mono font replacement, and text AA and hinting. —
  `M/editors/preferences/interface.html`
  Adopt because: a UI scale separate from OS DPI (egui `pixels_per_point`
  multiplier), plus a line-width token in `theme.rs`. Font override helps
  accessibility and CJK.
- F10 Border Width (larger hit zones for area edges) and Corner Handles for touch and
  pen. — `M/editors/preferences/interface.html`, `M/interface/window_system/areas.html`
  Adopt because: a density token for splitter hit zones, aimed at tablet painters.
- F11 Numeric Input Arrows (always vs. on hover) and Factor Display (0-1 vs. %). —
  `M/editors/preferences/interface.html`
  Adopt because: the factor-as-percent display matches Painter's 0-100 opacity
  habits.
- F12 Menus: Open on Mouse Over with top/sub delays, and Close Menu on Leave. —
  `M/editors/preferences/interface.html`
  Adopt because: small, predictable menu-behavior prefs. Low priority.
- F13 Temporary editors: render, file browser, and preferences can open as a new
  window, a maximized area, or in place. — `M/editors/preferences/interface.html`
  Adopt because: prefs and export dialogs as separate windows vs. in-dock is a user
  choice. [inferred]
- F14 Language preference with separate translate toggles for Tooltips / Interface /
  New Data names, plus date and time formats. — `M/editors/preferences/interface.html`
  Adopt because: Umber has en-US + pt-BR Fluent. Letting users keep English tooltips
  or data names while translating the UI is a cheap, appreciated split.
- F15 Reduce Motion accessibility toggle. — `M/editors/preferences/interface.html`
  Adopt because: gate all `umber-gui` motion on it.
- F16 Input: Emulate Numpad, Emulate 3-Button Mouse (Alt or OSKey), Continuous Grab
  (cursor warp, auto-off for tablets), Release Confirms, and thresholds for double
  click, mouse/tablet drag, and motion. — `M/editors/preferences/input.html`
  Adopt because: a separate tablet drag threshold and motion threshold remove pen
  jitter on clicks. Continuous grab should be disabled for absolute devices.
- F17 Tablet: API choice (Windows Ink/Wintab), Max Threshold (pressure for full
  intensity), and Softness (gamma of low-pressure onset). —
  `M/editors/preferences/input.html`
  Adopt because: a global pressure response curve is essential for a painting app,
  and Umber's per-brush curves sit on top of it.
- F18 Touchpad multi-touch gestures and natural vs. traditional scroll; NDOF (3D
  mouse) modes. — `M/editors/preferences/input.html`
  Adopt because: touchpad pinch-zoom and two-finger orbit for laptop users. NDOF is
  low priority.
- F19 Defaults: "Save Startup File" and "Load Factory Settings" are separate from the
  preferences, and "Import Preferences From Previous Version" runs on upgrade. —
  `M/interface/window_system/topbar.html`, `M/getting_started/configuration/defaults.html`
  Adopt because: a factory-reset escape hatch and preference migration across Umber
  versions.

---

## Gaps and caveats

- These corpus pages are 404 stubs: `interface/window_system/pie_menus.html`,
  `…/screens.html`, `…/tool_settings.html`,
  `interface/controls/templates/operator_search.html`,
  `interface/keymap/introduction.html` and `common_shortcuts.html`. Pie menus and
  operator search were covered from `controls/buttons/menus.html` and
  `interface/operators.html`.
- The corpus has no dedicated pages for the Info editor (report log), tooltips, or
  Image Editor navigation. E1, E4, and C7.5 are cited from the status bar,
  preferences, and 3D navigation pages.
- The HIG fetch returned only an index. Its paradigm and component pages
  (`…/paradigms/`, `…/components/`, `…/user_feedback/`) are the next fetch if
  design-principle citations are needed.
- The keymap-prefs text was read up to "Pie Menu on Drag". Later options on that page
  were not read.
- Not read in depth (budget): `editors/asset_browser.html`, `interface/selecting.html`,
  `interface/controls/nodes/*`. These are relevant to the Assets and Graph panels and
  are worth a follow-up pass.
- Claims about Painter's own bindings or behavior (C1.7, C4.2, C5.7) come from domain
  knowledge, not this corpus. Verify them against the Painter research doc.
