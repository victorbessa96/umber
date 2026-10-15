# Substance 3D Painter GUI inventory: cited research (2026-10-15)

Input for the GUI plan in `gui-plan-prompt-2026-10-15.md`. Source: the pre-fetched
Adobe Experience League corpus (`~/Projects/.umber-gui-corpus-2026-10-15/`, 52 files, pages
dated "Last update: October 8, 2026"). I fetched nothing live.

Conventions:
- Each item is one line: what Painter does, then `[Sn]`, which points to a URL in the source key.
- `(inferred)` marks a claim that the cited page implies but does not state.
- `GAP` marks something the brief needs that the corpus does not cover.
- `Umber:` notes compare an item with "What Umber's GUI is today" from the brief. They come from
  the brief only; I did not read Umber's source tree.

## Source key

All URLs start with `https://experienceleague.adobe.com/en/docs/substance-3d-painter/using/`.

| id | path | id | path |
|---|---|---|---|
| S1 | interface/main-menu/file-menu | S2 | interface/main-menu/edit-menu |
| S3 | interface/main-menu/mode-menu | S4 | interface/main-menu/window-menu |
| S5 | interface/main-menu/help-menu | S6 | interface/properties |
| S7 | interface/layer-stack/creating-layers | S8 | interface/layer-stack/managing-layers |
| S9 | interface/layer-stack/masking-and-effects | S10 | effects/effects |
| S11 | interface/texture-set/texture-set-list | S12 | interface/texture-set/texture-set-settings |
| S13 | interface/texture-set/texture-set-reassignment | S14 | interface/assets/assets |
| S15 | interface/assets/navigation | S16 | interface/shader-settings/shader-settings |
| S17 | interface/display-settings/viewport-settings | S18 | interface/display-settings/camera-settings |
| S19 | interface/display-settings/environment-settings | S20 | interface/viewport/3d-view |
| S21 | interface/viewport/2d-view | S22 | interface/viewport/camera-management |
| S23 | export/export | S24 | export/export-window/export-window |
| S25 | baking/baking | S26 | baking/how-to-bake-mesh-maps |
| S27 | baking/baking-visualization-settings | S28 | interface/settings/settings |
| S29 | interface/settings/general-preferences | S30 | interface/settings/shortcuts |
| S31 | interface/project-configuration | S32 | interface/color-picker |
| S33 | interface/miscellaneous/sliders | S34 | painting/presets/presets |
| S35 | painting/paint-tools/paint-brush | S36 | painting/paint-tools/tool-list |
| S37 | painting/paint-tools/projection | S38 | painting/paint-tools/quick-mask |
| S39 | painting/path-tools/path | S40 | painting/straight-line |
| S41 | painting/lazy-mouse | S42 | painting/symmetry/symmetry |
| S43 | painting/fill-projections/fill-projections | S44 | painting/painting |
| S45 | painting/dynamic-strokes/dynamic-strokes | | |

These corpus files are "Page not found" stubs and have no content: `painting/symmetry`,
`painting/tool-list/tool-list`, `painting/brush-settings/{brush-settings,stencil}`,
`features/baking/baking`, `features/mesh-maps/mesh-maps`, `exporting/export-window`. Equivalent
content survives under S35, S36, S42, S24, S25 and S12.

---

## A. Shell and navigation

### A1. Main menu: File [S1]
- New: opens the new-project window [S1].
- Open, Recent files, Open sample (bundled sample projects) [S1].
- Close: closes the current project [S1].
- Save is incremental by default: only changed data is rewritten [S1].
- Save and compact: full rewrite that removes fragmentation and shrinks the file [S1].
- Save as: new name and location [S1].
- Save as copy: writes a copy and keeps the current project open [S1].
- Save as template: saves the project settings as a template for new projects [S1].
- Clean: removes unused resources; takes effect on the next Save [S1].
- Import resources: opens the import window [S1].
- Export mesh: options are without displacement (with optional triangulation) or with
  displacement/tessellation (with optional recompute of vertex normals) [S1][S23].
- Export textures: opens the Export window, Ctrl+Shift+E [S1][S24].
- Send to: list of hand-off actions to other applications [S1].
- Quit: prompts if there are unsaved changes [S1].

### A2. Main menu: Edit [S2]
- Undo and Redo, which step through the History stack [S2].
- Project configuration: opens the project-settings window (B12) [S2][S31].
- Settings: opens application preferences (F) [S2][S28].
- Bake Mesh maps: opens the Baking window, Ctrl+Shift+B [S2][S30].

### A3. Main menu: Mode [S3]
- Three exclusive modes, each with its own interface: Bake mesh maps (F8), Painting (F9) and
  Rendering/Iray (F10) [S3][S30].
- A Baking-mode button (croissant icon) sits in the contextual toolbar at the top right of the
  viewport; layouts can hide it [S25][S26].
- Umber: there is no mode concept; Bakes is a dock tab (inferred gap).

### A4. Main menu: Window (views, toolbars, layout) [S4]
- Views: a checklist of every window, showing whether each is visible [S4].
- Toolbars: visibility checkboxes for the Docks, Plugins and Tools toolbars [S4].
- Hide UI (Tab): hides every dock and window and maximises the viewports [S4][S30].
- Reset UI: restores the default window layout [S4].
- Viewport layout presets: 3D+2D (F1), 3D only (F2), 2D only (F3), swap 2D/3D (F4) [S30].
- GAP: the corpus has no page on named or saved workspace layouts. Reset UI is the only layout
  action documented.

### A5. Main menu: Help [S5]
- Links: Tutorials, Release notes, Documentation, Shortcuts list, Scripting docs, Forums,
  Give feedback [S5].
- Report a bug: opens a bug-report window [S5].
- Show log: opens the session log in a text editor. Export log: saves it for support [S5].
- Account: Manage my account, Sign out [S5].
- Screens: Welcome screen, What's new, Home screen [S5].
- Check for updates, About (version and legal), Partners [S5].
- GAP: no View menu page exists in the corpus. A View menu is not documented.

### A6. Toolbars
- Tool toolbar on the left side; the Path tool is the third icon from the top [S39].
- Manual paint tools are reached from the toolbar or their own shortcut. Selecting a tool shows
  its parameters in Properties [S44].
- Tool list: Paint, Eraser, Path, Projection, Polygon Fill, Smudge, Clone, Material Picker and
  Quick Mask [S36].
- Material Picker is temporary: after a pick, the previous tool returns [S36].
- Tool hotkeys (all editable) [S30]:
  - 1 Paint, 2 Eraser, 3 Projection, 4 Polygon Fill, 5 Smudge, 6 Clone.
  - Ctrl+1/2/3 select the particle (physical) variants.
  - Ctrl+6 selects Clone with an absolute source.
  - P Material Picker.
- Physical variant: a toolbar toggle switches a tool to "Physical" and shows the Physics
  section [S35].
- Docks toolbar: a dock toolbar opens windows such as Properties [S6][S4].
- Umber: tool hotkeys are W/G/V/X/I/E/Z/S/O. Painter uses W/E/R for the manipulator (A8).

### A7. Contextual toolbar (top of viewport, changes with the active tool)
- Paint tool: Size, Flow, Stroke Opacity, Spacing, plus Lazy mouse and Symmetry [S35].
- Lazy mouse: an on/off button (D) and a radius field [S41][S30].
- Symmetry: an on/off button (L) and a Symmetry-settings button that holds the display options.
  The display options exist only here, not in Properties [S42][S30].
- Symmetry is greyed out when a fill layer uses a projection that does not support it [S42].
- Path tool controls [S39]:
  - Show/hide overlay.
  - Display settings: handle size, path width, path colour, unselected colour, and toggles for
    normals, tangents and direction arrow.
  - Reverse direction, toggle corner/smooth, custom tangents.
  - Open/close path, delete vertex, symmetry, hide excluded geometry.
  - Per-vertex values such as pressure.
- Baking-mode button (croissant) [S26].
- Stencil: holding S shows a Reset button at the top right of the viewport [S35].

### A8. Fill-layer manipulator (gizmo) [S30]
- Q shows/hides the gizmo. + and - change its size. T cycles object/world space [S30].
- W translate, E rotate, R scale, Shift+W snap to surface, Shift+V toggles warp
  vertex-edit [S30].
- Shift snaps translate/rotate in 3D and constrains ratio in 2D. Ctrl mirrors around the pivot
  in 2D [S30].

### A9. Status and overlays
- Keyboard helper: while a modifier is held, the bottom-left of the viewport lists the active
  shortcuts. It can be turned off in preferences [S29].
- Profile label at the top-left of the 3D view shows the loaded colour profile [S20].
- Axis gizmo: XYZ at the bottom-right of the 3D view, UV in the 2D view [S20][S21].
- The current grid unit is shown at the bottom-left of the viewport when the grid is on [S17].
- When a single colour channel (R/G/B/A) is soloed, its name is shown in the viewport [S17].
- GAP: no page describes a status bar. Painter puts status in viewport overlays instead
  (inferred).

---

## B. Panels

### B1. Layer stack [S7][S8][S9][S10]
- Header buttons: new paint layer, new fill layer, new folder, add mask, add effect,
  delete [S7][S9][S10].
- Duplicate: right-click > Duplicate, Ctrl+D, or Ctrl-drag [S7].
- Drag from Assets into the stack:
  - A material makes a fill layer; a smart material makes a folder; an effect is added to the
    stack [S7].
  - Ctrl while dropping a fill also creates a mask [S9].
- Drag to reorder. A drop bar shows above, between or below; dropping on a folder nests the
  layer [S8].
- Multi-select with Ctrl-click or Shift range-click. Ctrl+G or right-click > Group
  Layers [S8][S30].
- Clipboard (pointer must be over the stack) [S30]:
  - Ctrl+C / Ctrl+X / Ctrl+V / Delete act on layers.
  - Ctrl+Shift+C / Ctrl+Shift+V copy and paste a layer's content plus its effects.
- Masks are always grayscale; colour paint is converted to gray [S9].
- Mask thumbnail gestures [S9][S30]:
  - Alt+click shows the mask alone in the viewport.
  - Shift+click toggles the mask on or off.
- Mask right-click menu: Copy mask content / Paste into mask, Invert mask background, Toggle
  mask, Create smart mask [S9].
- Re-adding or removing a mask destroys it together with its effects [S9].
- Effect indicator: a line under each thumbnail is grey with no effects and red with at least
  one. Content and mask each have their own effect stack [S9].
- Effect types: Generator, Paint, Fill, Levels, Compare Mask, Filter, Anchor Point [S10].
- Most effects have a blend mode and opacity. Reorder by right-click or Alt+Up/Down [S10].
- Dropping a smart mask creates a black mask if none exists, otherwise merges with the existing
  effects. Ctrl+drop replaces them [S9].
- Thumbnails: real thumbnails by default. The "Use simplified thumbnails" preference switches to
  icons for speed; UDIM projects always use icons [S29].
- GAP: the corpus has no page on layer blend modes, the layer opacity widget, the visibility eye
  or per-channel blending UI. These are standard Painter features but uncited here.
- Umber: there are no layer thumbnails (brief, Phase 2 gap).

### B2. Properties [S6][S35][S43][S39]
- One window for tool, brush and layer parameters. Opened from the dock toolbar or by
  right-clicking in the viewport [S6][S30].
- Previews at the top [S35]:
  - The brush preview can be drawn in (no pen pressure).
  - The material preview can be dragged to rotate its lighting.
- Brush section [S35]:
  - Size, Flow, Stroke Opacity, Spacing, Angle, Follow Path.
  - Jitter on Size, Flow, Angle and Position (0 to 1).
  - Alignment: Camera, Tangent|Wrap (default), Tangent|Planar, UV.
  - Backface Culling angle.
  - Size Space: Object (default), Viewport, Texture.
- A pen-pressure toggle button sits next to the pressure-capable parameters (Size, Flow) [S35].
- Alpha slot takes a Substance file or bitmap. An exposed "hardness" parameter becomes
  shortcut-driven [S35].
- Physics section appears only for physical tools or particle presets [S35].
- Stencil slot: tiling None (default), Horizontal, Vertical, or H and V [S35].
- Material section [S35]:
  - A Material-mode button loads a Substance file or preset into all channels.
  - Per-channel toggle buttons; a deselected channel is neither editable nor painted.
- Fill properties: Projection dropdown with Fill (per UV tile), UV, Tri-planar, Planar,
  Spherical, Cylindrical, Warp [S43].
- Fill layers have a Symmetry section with an "Enable symmetry" checkbox (projection-dependent)
  [S42].
- Path tool adds a Presets section at the top with Favorites [S39]:
  - "Show compatible presets in assets".
  - Brush gains Projection depth and Up axis.
- Right-click in Properties > Create tool preset [S39].
- Umber: 22 raw `egui::Slider`s remain (brief). Painter's Properties is one long stack of
  sections (inferred from S35).

### B3. Texture Set list [S11]
- Lists every material ID on the mesh. Selecting one switches the Layer stack and Shader
  settings. Only one set can be edited at a time [S11].
- States: Selected, Visible/Hidden, Disabled (no matching mesh material) [S11].
- Visibility controls [S11][S30]:
  - Per-set eye button.
  - Menu: Show all / Hide all / Invert.
  - Focus Mode isolates the active set (also Alt+Q).
- Right-click menu: Show/Hide, Edit name (also by double-click), Reset name to the original,
  Edit description. The name is used at export [S11].
- Shader button per row: "New shader instance" gives a set its own shader [S11].
- Settings menu [S11]:
  - Hide empty / Hide all / Show all descriptions.
  - Import Shader Parameters (json).
  - Reassign Texture Sets.
- Baking mode adds a checkbox per set and per UV tile to choose bake targets [S26].
- Material-layering workflow shows sub-stacks under each set name [S11].
- Viewport shortcut: Ctrl+Alt+right-click selects a texture set under the cursor [S30].

### B4. Texture Set settings [S12]
- Name. Description, which also appears in the list and the Baking window [S12].
- Size: two resolution dropdowns with a lock between them; unlocking allows non-square sizes [S12].
  - Resolution is dynamic: maximum 4096 in the app and 8192 at export.
  - The UI warns that a change can trigger long recomputation.
- Shader instance selector [S12].
- Channel list [S12]:
  - "+" opens a menu grouped as Supported / Unsupported / User channels.
  - "−" removes a channel; its data is kept so the channel can be restored.
  - Double-click renames a user channel.
- Channel settings menu [S12]:
  - Storage format: sRGB8, L8, RGB8, L16, RGB16, L16F, RGB16F, L32F, RGB32F.
  - A Color-channel toggle (user channels only) and a colour-managed badge.
- Mixing [S12]:
  - Normal mixing: Replace or Combine.
  - Height-to-normal: Sharp or Smooth (Sobel).
  - AO mixing: Replace or Multiply.
  - UV padding: 3D-space neighbour or 2D-space neighbour.
- Mesh maps: a slot per baked map for the set [S12].

### B5. Texture Set reassignment window (modal) [S13]
- Three lists: Disabled sets, Project sets, Mesh materials. Reassign by drag-and-drop or by
  swapping entries [S13].
- The window has its own Undo/Redo, plus Apply and Cancel [S13].
- An × marks a disabled set for deletion; deletion cannot be undone once Apply is pressed [S13].

### B6. Shelf / Assets [S14][S15]
- Three regions: filter area, asset list, bottom bar [S14].
- Library sources: "Starter assets" (install folder) and "Your assets" (Documents). Extra
  library locations can be added [S14].
- Breadcrumbs: arrows reveal the on-disk tree; empty folders are greyed but still
  navigable [S15].
- Search matches title, path and tags, and supports advanced query syntax [S15].
- Asset-type icon filters, multi-select with Ctrl [S15]:
  - Materials (the default), Smart materials, Smart masks, Filters.
  - Brushes (brushes, particles, tools), Alphas (and brush makers).
  - Textures (grunges, procedurals, baked maps, hard-surface normals, LUTs), Environment maps.
- Filters, breadcrumbs and search combine. A mismatch gives an empty list, and the docs flag
  this as a pitfall [S15].
- Environment maps can be dropped straight onto the viewport [S15].
- Mini-shelf: resource buttons (environment map, shader file) open a small inline picker instead
  of the full Assets window [S19][S16].
- Right-click on a preset > Add to / Remove from favorites [S39].
- GAP: the corpus does not describe the asset-list layout (grid vs list, thumbnail size) or the
  bottom bar's contents.

### B7. History
- Undo (Ctrl+Z) and Redo (Ctrl+Y) step through "the History stack" [S2][S30].
- The shader has its own undo/redo stack inside Shader settings so it does not conflict with
  painting history [S16].
- Modal windows (Reassignment) keep their own local undo [S13].
- GAP: no History-panel page exists in the corpus; its contents are not documented here.

### B8. Display settings: camera [S18][S22]
- Camera section [S18][S22]:
  - FOV (degrees) and Focal length (mm).
  - Focus Distance, which Ctrl+middle-click auto-sets.
  - Aperture.
  - DOF needs post effects enabled.
- Preset dropdown chooses which imported camera to edit [S22].
  - Restore reverts its attributes.
  - A modified camera's name turns italic with a `*`.
  - A lock icon freezes its parameters.
  - Display ratio is shown, or "Unspecified".
- TAA: Accumulations (16 recommended, 64 for high-contrast alpha dither). It only accumulates
  while the camera is still [S18].
- Sections for Post effects, SSS and Color profile link to pages outside the corpus [S18].
- Tone mapping: Linear (default, clamps) or ACES [S18].

### B9. Display settings: environment [S19]
- Environment Map slot (mini-shelf) [S19].
- Override environment colour space (colour-managed projects only) [S19].
- Opacity and Blur affect the background only, not the lighting [S19].
- Exposure (EV). Rotation, also Shift+right-drag [S19][S30].
- Alignment: World or Local [S19].
- Shadows: on/off. Computation mode: Intensive, Average or Lightweight (default). Shadow
  opacity [S19].

### B10. Display settings: viewport [S17]
- Texture filtering [S17]:
  - Anisotropic: Disabled, 2, 4 (default), 8 or 16 spp.
  - MipMap bias from 0 (Soft, default) to −3 (Very Sharp).
- Camera frame toggle and Gate mask opacity [S17][S22].
- Tool display: Hide stencil when painting, Stencil display opacity, Projection preview
  channel [S17].
- Mesh wireframe: show toggle, colour, opacity [S17].
- Channel display (solo mode only) [S17]:
  - Unlit solo.
  - Scale HDR values.
  - ± colours for HDR values.
  - Colour channel: RGBA, Gray+A, R, G, B or A.
- Grid: show toggle, axis (default Y), colour, opacity. Divisions adapt to zoom [S17].

### B11. Shader settings [S16]
- Header: local Undo/Redo, Shader-file button (mini-shelf, with an "Outdated" flag), Instance
  name, Restore defaults [S16].
- Parameters are generated from the loaded shader file [S16].
- Displacement [S16]:
  - Source channel: Height or Displacement.
  - Scale unit: Normalized, Scene or Physical cm.
  - Scale amount.
- Tessellation [S16]:
  - Mode: Uniform (count 1 to 32) or Edge Length (max length 1/value).

### B12. Project configuration (modal) [S31]
- 3D mesh: file path, Select button, "Reimport mesh" checkbox (auto-checked when the file
  changes) [S31].
- Normal map format: OpenGL or DirectX, with a per-engine recommendation table [S31].
- Compute tangent space per fragment [S31].
- Changing either normal setting means mesh maps must be re-baked [S31].
- USD-only fields: Scope/variants picker, Subdivision level, Frame [S31].
- UV Tiles settings are read-only after project creation [S31].
- Import [S31]:
  - Import cameras.
  - Preserve stroke positions on the mesh.
  - Auto-unwrap, with an Options button.
- Physical size [S31]:
  - Mesh internal unit, or a custom unit scale.
  - Option to switch fill scaling to physical size automatically.
- Color management section links outside the corpus [S31].

### B13. Export window (modal, Ctrl+Shift+E) [S24][S23]
- Three tabs: Export settings, Output templates, List of exports [S24].
- Footer buttons: Reset all (Settings tab only), Cancel, Save settings (stored in the project),
  Export [S24].
- Export switches to the List-of-exports tab to show progress [S24].
- Output templates control file naming, channel packing, format and bit depth. Dozens of presets
  for target engines ship with Painter [S23].
- GAP: the subpages for the Export-settings and Output-template tabs are not in the corpus. Their
  per-field contents are undocumented here.

### B14. Bakes (Baking mode) [S25][S26][S27]
- Entry: croissant button, Mode menu, or F8 [S26].
- Targets: per-texture-set and per-UV-tile checkboxes in the Texture Set list [S26].
- Mesh Map Bakers window: a checkbox per map type [S26].
- Common settings: resolution, dilation width, high-poly meshes [S26].
- Cage options [S26]:
  - Distance-based (frontal and rear distances), Automatic (experimental), or a custom file
    (vertex count must match).
  - "Use low poly mesh as high poly" checkbox.
- The cage shows red spots where it intersects the mesh [S26].
- The Bake button is at the bottom of the viewport [S26].
- A Baking Log window lists errors. An arrow next to each error jumps to the baker setting that
  caused it [S26].
- Visualization panel inside the viewport [S27]:
  - Hide baking meshes; show only the selected texture set.
  - HP mesh, with colour.
  - Matching-error highlight, with colour.
  - Cage surface and opacity; cage wireframe and opacity.
  - Missing seams on hard edges, with colour.
  - Project mesh, with colour and a neutral-material group: quality, roughness, metallic, AO
    intensity, bent normal and its diffuse/specular amounts.
- Preferences [S29]:
  - Save preprocessed scene files.
  - Live preview of the bake on the mesh.
  - GPU raytracing.
- GAP: no per-baker parameter pages are in the corpus (`features/mesh-maps` is a 404 stub).
- Umber: there is no bake-panel control for Adjacent-Faces margin (brief). Painter's "dilation
  width" in common settings is the closest equivalent.

### B15. Settings window [S28]
- Three sections: General preferences, Shortcuts, Libraries configuration. Opened from
  Edit > Settings [S28].
- Contents are listed under F.
- GAP: no Libraries-configuration page exists in the corpus.

---

## C. Viewport interaction

### C1. Brush cursor and painting feedback
- Lazy mouse draws a grey circle around the brush cursor at the lazy radius [S41].
- Straight line: holding Shift draws a dotted preview line, and a click paints it. Ctrl+Shift
  snaps the angle to 5° steps [S40][S30].
  - Caveat: the line is computed in screen space, so moving the camera between strokes misplaces
    it [S40].
- Resume previous stroke (A) continues the last stroke without a seam [S35][S30].
- In-viewport drag to adjust the brush [S30]:
  - Ctrl+right-drag: horizontal changes Size, vertical changes Hardness.
  - Ctrl+left-drag: horizontal changes Flow, vertical changes Rotation.
- [ and ] change size. X inverts grayscale when painting a mask [S30].
- Follow Path needs at least two stamps, so a single click paints nothing [S35].
- GAP: the corpus does not describe the brush cursor's shape (outline, size ring, normal-aligned
  disc). Painter shows one, but it is uncited here.
- Umber: there is no in-viewport brush cursor (brief).

### C2. Stencil and projection
- The stencil is a global grayscale mask in screen space; the alpha is per stamp [S35].
- Hold S [S35][S37][S30]:
  - S+left-drag rotates, with Shift snapping to 90°.
  - S+right-drag scales.
  - S+middle-drag moves.
  - A Reset button appears at the top right.
- N held: temporarily ignores the stencil [S35][S30].
- Viewport settings control stencil opacity and hiding while painting [S17].
- The Projection tool uses the same S-controls. "Physical projection" adds particles [S37].
- Projection and stencil in the 2D view do not support symmetry [S42].

### C3. Quick mask, geometry mask, clone
- Quick mask: U enters/exits edit mode, Y clears, I inverts. The S38 page text writes the
  enter/exit key as both Y and U; the shortcut table gives U toggle and Y clear [S38][S30].
- Alt+H hides or ignores geometry excluded by the Geometry Mask [S30].
- V+left-click sets the Clone source [S30].
- Ctrl+drag of a material onto the viewport applies it to one part of the model only [S30].

### C4. 3D view [S20]
- Top-left: colour-profile label [S20].
- Camera dropdown, shown only when imported cameras exist [S20].
- Display-mode dropdown: Material (lit) or Solo (a single channel or mesh map) [S20].
- Bottom-right: axis gizmo [S20].
- Display-mode keys [S30]:
  - M: material.
  - C / Shift+C: next/previous channel.
  - B / Shift+B: next/previous mesh map.

### C5. 2D (UV) view [S21]
- Shows the UV islands of the selected texture set and can be painted on [S21].
- Top-right display-mode dropdown: channel, mesh map, or lit material [S21].
- UV-tile info button, only in UDIM projects [S21].
- Bottom-right: UV axis indicator [S21].
- Regular projects: only the [0,1] range can be painted. Outside it is shown but inactive, and
  the paintable area has a light-grey background [S21].
- UDIM projects: a tile grid with UDIM numbers; every tile can be painted [S21].

### C6. Camera management [S22]
- Imports cameras from FBX/ABC: name, transform, FOV and aspect. Imported cameras are listed with
  "Default camera" in the dropdown [S22].
- Orbiting, panning or zooming while an imported camera is active switches to Default camera, so
  imported cameras never move [S22].
- F5 perspective, F6 orthographic [S30].
- F frames the whole mesh [S30].

### C7. Navigation keys [S30]
- Alt+left-drag rotates the camera. Alt+Shift+left-drag rotates with 90° snaps [S30].
- Panning: Alt+middle-drag, or Ctrl+Alt+left-drag as an alternative [S30].
- Alt+right-drag zooms [S30].
- Shift+right-drag rotates the environment [S30].
- Ctrl+middle-click auto-focuses depth of field [S30][S18].
- Preferences scale rotation, zoom and wheel speed, and can invert zoom and wheel [S29].
- Right-click opens a viewport quick menu that gives access to Properties [S30][S6].
- Space pauses/resumes particle animation. Shift+Esc pauses engine computation [S30].

### C8. Path tool interaction [S39]
- Click on the surface to add points (at least two). Click on the line to insert a point [S39].
- Esc or Enter ends editing. Click empty surface to start a new path; click a path to select
  it [S39].
- Double-click a vertex to switch corner/smooth. Alt-drag breaks the tangents; Ctrl-drag scales
  both [S39].
- Rectangular selection picks multiple vertices [S39].
- Paths panel inside the viewport (shown only for the Path tool on a paint layer) [S39]:
  - Double-click renames; Delete removes.
  - Copy, paste and duplicate.
  - Eye icon (applied or not).
  - Right-click: copy properties or position to another path.

---

## D. Flagship widgets

### D1. Colour picker [S32]
- Opens from any colour field. Floating and semi-persistent: it stays open until the context
  changes, can go on any screen, and cannot be docked [S32].
- Vertical layout: spectrum, then sliders, then swatches [S32].
- Spectrum: a vertical hue strip and a 2D field with a crosshair [S32].
- Current/previous swatch pair: click "previous" to revert [S32].
- Hex field is always sRGB, for copy/paste between apps [S32].
- Display selector (colour-managed projects only) [S32].
- Eyedropper picks anywhere on screen [S32].
  - In the viewport, Shift picks the raw channel value, which avoids a lossy display round-trip.
  - Every colour field also has its own inline eyedropper.
  - The eyedropper colour-space setting defaults to auto sRGB.
- Sliders: HSV or RGB dropdown [S32].
- Settings [S32]:
  - "Dynamic sliders": slider tracks are tinted by the current colour.
  - "Floating point" off: hue in degrees, S/V in %, RGB as 0-255.
- Working-colour-space readout; hover shows the space's name (colour management only) [S32].
- Swatches persist across projects and sessions, stored as sRGB in a user config file [S32].
  - "+" adds the current colour, only if it differs from the last swatch.
  - Hover shows hex; click loads.
  - Right-click deletes one; the settings menu deletes all.
- Umber: uses egui's stock picker (brief).

### D2. Sliders [S33]
- Drag on the slider to change the value [S33].
- Shift-drag: precision drag over a longer distance [S33].
- Click anywhere on the track to jump to that value [S33].
- After focusing, Up and Down arrows step the value [S33].
- Click the numeric text field above the slider to type a value [S33].
- (inferred) This gives a label/value field over a track, which matches Umber's
  `value_field` + slider pairing.

### D3. Presets [S34][S39]
- Three kinds [S34]:
  - Brush preset: size, flow, jitter, stencil, alpha.
  - Material preset: channels and Substance files.
  - Tool preset: brush plus material plus tool type.
- Saved as self-contained files in the Assets folder, so they copy between machines [S34].
- Also: particle presets and Photoshop ABR import [S34].
- Created from right-click in Properties > Create tool preset. Choosing one in Assets switches
  tool automatically [S39].
- Favorites list in the Properties Presets section [S39].

### D4. Resource slot / mini-shelf
- A button shows the current resource; clicking it opens a filtered mini-shelf picker. Used for
  the environment map and the shader file [S19][S16].
- (inferred) Alpha and stencil slots use the same pattern [S35].

---

## E. Feedback and state

- Imported-camera edits show as italic name plus `*`, with Restore to revert [S22].
- Effect presence: grey or red bar under each layer thumbnail [S9].
- Disabled texture sets are flagged in the list and fixed through the Reassignment window
  [S11][S13].
- Shader "Outdated" flag on the shader-file button [S16].
- Greyed-out controls when unavailable [S42][S15]:
  - Symmetry button with an unsupported projection.
  - Empty breadcrumb folders.
- Cage intersections show red in the viewport. Matching-error colour shows HP areas outside the
  cage [S26][S27].
- Hard edges that are not seams are highlighted in both 2D and 3D [S27].
- The Baking Log has error-to-setting navigation [S26].
- Export progress appears in the List-of-exports tab [S24].
- TAA converges only while the camera is still, which reads as a progressive "settling"
  image [S18].
- Shadows: Lightweight mode converges over a few seconds without stalling the view [S19].
- Live bake preview on the mesh (preference) [S29].
- Destructive or slow actions are labelled in the UI [S12][S9][S13]:
  - Resolution change warns of recomputation.
  - Mask re-add destroys its effects.
  - Reassignment deletion cannot be undone.
- Keyboard helper overlay while modifiers are held [S29].
- Pause engine computation (Shift+Esc) is a user-visible engine state [S30].
- Save: incremental vs compact; Clean takes effect on the next save; unsaved changes prompt on
  quit [S1].
- GAP: the corpus has nothing on progress bars, toasts or notifications, the busy/compute
  indicator, or crash recovery.

---

## F. Customisation

### F1. Shortcuts [S30]
- Settings > Shortcuts. Click the pen icon and press the new combination; the last key commits.
  Right-click resets to the default [S30].
- Separate Windows and macOS columns [S30].
- Some shortcuts are fixed (not editable) and depend on the pointer position, for example
  layer copy/paste only works over the stack [S30].
- Some actions ship with no binding, for example "Select paint along path tool" [S39].
- Editable defaults:
  - Tab Hide UI. Ctrl+Z / Ctrl+Y undo and redo. F8/F9/F10 modes.
  - Ctrl+N/O/S new, open, save. Ctrl+F4 close.
  - Q/+/-/T/W/E/R/Shift+W/Shift+V manipulator.
  - L symmetry. D lazy mouse. 1-6 tools. P picker.
  - [ ] size. X invert.
  - Alt+H geometry mask. F5/F6 projection. C/B/M display. F frame.
  - U/Y/I quick mask. F1-F4 layouts. Alt+Q isolate.
  - Mouse chords (C7). N stencil off. A resume stroke. V clone source.
  - Ctrl+Shift+B bake. Ctrl+Shift+E export. Space animation. Shift+Esc pause.
- Help > Shortcuts list links to the docs [S5].

### F2. General preferences [S29]
- Interface [S29]:
  - Language (needs a restart): system default, EN, DE, FR, JA, ZH-Hans.
  - Show keyboard helper. Show world axes.
  - Viewport background as a two-colour gradient.
  - "Only display selected material when painting".
  - Viewport scaling: None, or Auto (half resolution on HiDPI).
- Layer stack: default UV scale for materials; simplified thumbnails [S29].
- Camera: rotation speed, zoom speed (with invert), wheel speed (with invert) [S29].
- Baking: save preprocessed scenes, live preview, GPU raytracing [S29].
- Preview [S29]:
  - Local thumbnail cache directory and budget (MB).
  - Material preview shader (needs a restart).
- Temp-files cache directory. Sparse-virtual-texture hardware acceleration (both can be
  overridden by environment variables) [S29].
- Iray hardware list (CPU and CUDA GPUs) [S29].
- Privacy: usage statistics toggle [S29].
- Umber: i18n with en-US and pt-BR already exceeds the brief's "every string via tr()" baseline.
  Painter's language switch needs a restart [S29].

### F3. Layout customisation
- Show/hide each window and toolbar; Hide UI; Reset UI; F1-F4 viewport layouts [S4][S30].
- The colour picker is explicitly not dockable, which implies the other windows are dockable
  (inferred) [S32].
- Texture-set description visibility mode (hide empty / hide all / show all) [S11].

---

## Corpus gaps

These are needed by the brief but missing from the corpus:
- History panel.
- View menu.
- Named or saved workspaces.
- Status bar.
- Layer blend modes, opacity and the visibility eye.
- Export-settings and Output-template tab fields.
- Per-baker parameters.
- Libraries configuration.
- Post effects, colour profile and colour management pages.
- Brush-cursor visuals.
- Notifications and progress UI.
- Asset-list layout.
- Seven corpus URLs are 404 stubs (listed under the source key).
