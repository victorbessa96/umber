# Umber UI strings — en-US, the source of truth (docs/specs/i18n-design.md).
#
# Key scheme: the code calls `tr("group.name")`; Fluent ids cannot contain
# dots, so `group.name` resolves to message `group`, attribute `.name`.
# Every key the code calls must exist here (the completeness test in
# crates/umber-app/src/i18n.rs fails otherwise). Other locales may omit
# keys: a missing key falls back to this file, then to the key itself.

## Top menu bar

menu =
    .file = File
    .edit = Edit
    .view = View
    .help = Help
    .open-mesh = Open Mesh…
    .open-project = Open Project…
    .recent = Open Recent
    .recent-empty = No recent projects yet.
    .recent-missing = This folder is not reachable right now (moved, deleted, or on a disconnected drive).
    .recent-clear = Clear Recent
    .revert = Revert to Saved
    .revert-unsaved = The project has never been saved, so there is nothing to revert to.
    .revert-clean = No unsaved edits to discard.
    .revert-confirm-title = Revert to saved?
    .revert-confirm-body = This discards every edit since the last save and reloads the project from disk.
    .window = Window
    .panels = Panels
    .reset-layout = Reset Layout
    .about-copy = Copy to Clipboard
    .about-close = Close
    .save-project = Save Project…
    .export-png = Export Painted Map (PNG)…
    .load-environment = Load Environment…
    .perf-hud = Show Perf HUD
    .perf-hud-unavailable = Perf HUD needs --features perf
    .wireframe = Show Wireframe (W)
    .grid = Show Grid (G)
    .gizmo = Show Axis Gizmo
    .frame-mesh = Frame mesh (F)
    .about = About Umber
    .shortcuts = Keyboard Shortcuts
    .close-project = Close Project
    .show-log = Show Log
    .show-log-failed = Could not open the log { $path }: { $error }

## Dock panel titles

panel =
    .viewport = Viewport
    .uv-view = 2D UV
    .layers = Layers
    .properties = Properties
    .assets = Assets
    .history = History
    .texture-sets = Texture Sets
    .bakes = Bakes
    .export = Export
    .graph = Graph
    .display = Display

## Properties context rail (g2-properties-context)

ctx-rail =
    .brush = Brush
    .layer = Layer
    .fill = Fill
    .mask = Mask
    .texture-set = Texture Set
    .search-hint = Search properties…

## Primary buttons

button =
    .bake = Bake
    .export = Export
    .evaluate = Evaluate
    .save = Save
    .save-as = Save As…
    .reset = Reset
    .choose = Choose…
    .add = Add

## Status phrases

status =
    .pen-windows-ink = Pen (Windows Ink)
    .pen-wintab = Pen (WinTab)
    .pen-wayland = Pen (Wayland tablet)
    .pen-x11 = Pen (X11)
    .pen-mouse = Mouse (fallback)
    .baking = { $count ->
        [one] Baking { $count } map…
       *[other] Baking { $count } maps…
    }
    .plugins = { $loaded } plugin(s) loaded, { $failed } failed
    .uv = UV
    .tile = Tile
    .pen = Pen
    .fps = fps

## Settings

settings =
    .language = Language

## Shared fragments (several panels)

common =
    .none = none
    .out-dir = Out: { $dir }

## App shell: placeholder panels, file-dialog filters

shell =
    .assets-placeholder = Assets / shelf (Wave 4+)
    .texture-sets-placeholder = Texture sets (Wave 2)
    .filter-environment = Environment
    .filter-meshes = Meshes
    .new = New
    .tip-new = Start a new empty project (unsaved; the current one stays on disk if saved)
    .new-confirm-title = Unsaved changes
    .new-confirm-body = The project has unsaved edits. Save them first, or discard and start fresh?
    .new-confirm-save = Save…
    .new-confirm-discard = Discard
    .new-confirm-cancel = Cancel
    .hide-ui = Hide UI
    .maximize = Maximize Area
    .show-ui = Show UI

    .open = Open…
    .tip-open-mesh = Load a mesh to paint (or drop one on the viewport)
    .undo = Undo
    .redo = Redo
    .wireframe = Wireframe
    .grid = Grid
    .view-lit = Lit
    .view-other = Unlit / preview
    .tip-view-cycle = Switch between lit and unlit shading (V)
    .mask = Mask
    .invert-mask = Invert
    .hero-title = Drop a mesh here to start painting
    .hero-release = Release to load the mesh
    .hero-hint = or use Open Mesh… in the toolbar or the File menu
    .hero-formats = Supported:

## First-run Quick Setup (GUI plan g5-first-run)

setup =
    .title = Quick Setup
    .intro = A few choices before the first brush stroke. All of them live in Preferences later.
    .language = Language
    .theme = Theme
    .theme-follow-os = Follow the system
    .theme-light = Light
    .theme-dark = Dark
    .theme-high-contrast = High contrast
    .navigation = Mouse navigation
    .keymap = Keymap
    .tablet = Tablet
    .tablet-live = Pen detected — pressure is live.
    .tablet-pressure = Pressure:
    .tablet-none = No pen detected. Mouse and touch still paint.
    .finish = Finish
    .skip = Skip

## Home card (GUI plan g5-home-screen)

home =
    .new-project = New project
    .open-project = Open…
    .open-tip = Open a saved .umber project (or double-click empty space)
    .open-mesh = Open Mesh…
    .open-mesh-tip = Load a mesh to paint (or drop one on the viewport)
    .recent = Recent projects
    .recent-remove = Remove from recents
    .recent-clear = Clear all recents
    .recover = Recover last session
    .recover-tip = Umber closed without saving last time. Open the autosaved copy of that work.
    .browse-assets = Browse starter materials
    .browse-assets-tip = Open the Assets panel with the bundled starter brushes, materials, masks and environment maps

## Texture Sets panel

texturesets =
    .set = Texture set
    .toggle-visible = Toggle this set visibility
    .show-all = Show all
    .hide-all = Hide all
    .invert = Invert
    .resize-confirm = Resize { $set } from { $from } to { $to }? This clears the set's canvas and the stroke undo history.
    .resize-apply = Resize
    .resize-cancel = Cancel
    .size = Paint target size
    .resize-clears = Changing the selected set's size clears that set's canvas and the stroke undo history, and resets its quick mask. This is intentional.
    .inherit = Inherit document resolution
    .inherit-short = Inherit
    .per-channel = Per-channel sizes
    .channel-resize-clears = Resizing a channel clears its tiles
    .no-paint = No paint session — the GPU target is unavailable.

## Bakes panel

bakes =
    .high-group = High poly
    .high-none = No high-poly mesh
    .cage-front = Cage front
    .cage-back = Cage back
    .bake-from-high = Bake from high
    .transfer-normal = Transfer normal
    .transfer-height = Transfer height
    .map-ao = Ambient occlusion
    .map-curvature = Curvature
    .map-thickness = Thickness
    .map-position = Position
    .map-world-normal = World-space normal
    .map-id = ID
    .map-bent-normal = Bent normal
    .map-height-transfer = Height (high-poly transfer)
    .no-mesh = No mesh loaded — open a mesh to enable baking.
    .no-gpu = No GPU device — baking needs the wgpu device.
    .resolution = Resolution
    .rays = Rays
    .dilation = Dilation
    .margin = Margin type
    .margin-extend = Extend
    .margin-adjacent = Adjacent faces
    .margin-hint = How the dilation width is filled: Extend repeats each island's border; Adjacent faces first copies the texels across each UV seam from the neighbouring face, then extends the rest.
    .select-map = Select at least one map to bake.
    .select-tile = Select at least one tile to bake.
    .no-bake-yet = No bake yet.
    .done = { $count ->
        [one] Baked { $count } map in { $ms } ms: { $files }
       *[other] Baked { $count } maps in { $ms } ms: { $files }
    }
    .skipped-note = skipped (v1 bakes only AO per tile; other maps tile 1001 only): { $pairs }
    .failed = Bake failed: { $error }
    .worker-failed = Bake failed: could not start the bake worker: { $error }
    .map-skew = Skew (correction mask)
    .skew-threshold = Skew threshold
    .occluder = Occluder scope
    .occluder-whole = Whole mesh
    .occluder-own = Baked set only
    .auto-rebake = Auto-rebake on change
    .bake-all = Bake all
    .sets = Texture sets:
    .select-set = Select at least one texture set to bake.
    .show-cage = Show cage
    .log = Bake log
    .log-empty = No bakes logged yet.
    .log-entry = [{ $time }] { $set } { $map } (tile { $tile }): { $ms } ms{ $warnings }
    .log-warnings = { $count ->
        [one] 1 skew-clamped texel
       *[other] { $count } skew-clamped texels
    }
    .skipped-skew-note = skew bakes tile 1001 only, skipped tiles: { $tiles }
    .settings-group = Settings
    .maps-group = Maps

## Export panel

export =
    .no-mesh = No mesh loaded — open a mesh to enable export.
    .no-gpu = No GPU device — export needs the wgpu device.
    .preset = Preset
    .size = Size
    .materialx = MaterialX (.mtlx)
    .usda = USD (.usda)
    .select-tile = Select at least one tile to export.
    .tile-badge = { $count ->
        [one] { $count } tile
       *[other] { $count } tiles
    }
    .source-painted = Base Color source: painted{ $tiles } — what you painted is what exports.
    .source-graph = Base Color source: graph node { $node }{ $tiles } — the panel's evaluated output.
    .source-flat-size = Base Color source: flat placeholder — the paint target is { $width } × { $height }, not the { $size } × { $size } export size.
    .source-flat-none = Base Color source: flat placeholder — no paint session or graph output live.
    .base-painted = painted
    .base-graph = graph node { $node }
    .base-flat = flat placeholder
    .base-none = Base Color: none
    .base = Base Color: { $source }
    .base-tiles = Base Color: { $source } ({ $tiles })
    .no-export-yet = No export yet.
    .done = Exported { $count } outputs ({ $base }; bake { $bake-ms } ms, write { $write-ms } ms): { $files }
    .skipped-output = { $file } (missing { $maps })
    .skipped = skipped: { $outputs }
    .tiles-skipped = tiles skipped (no mesh geometry): { $tiles }
    .failed = Export failed: { $error }

## Graph panel (list view, editor, status line)

graph =
    .trust-load = Trust & load
    .view-canvas = Canvas
    .view-list = List
    .canvas-help = Drag empty space to pan, wheel to zoom, drag an output port onto an input to connect, right-click to add, Delete removes the selected edge.
    .output = Output: { $node }
    .nodes = Nodes
    .no-nodes = No nodes yet — add one below.
    .add-node = Add Node
    .remove-selected = Remove selected
    .undo = Undo
    .redo = Redo
    .select-node = Select a node to edit its params.
    .node-title = Node { $id }: { $def }
    .set-output = Set as output
    .edges = Edges
    .edge = { $input } ← node { $from }
    .edge-input = input
    .edge-from = from
    .add-edge = Add edge
    .anchor-layer = layer
    .anchor-channel = channel
    .anchor-none = no layers to bind
    .no-output = No output yet — press Evaluate.
    .uniform-output = Uniform output: { $value }
    .no-eval-yet = No evaluation yet.
    .loaded = Graph loaded — press Evaluate.
    .loaded-unknown = Graph loaded with { $count } unknown node type(s) — press Evaluate.
    .load-failed = Graph load failed: { $error }
    .empty = Graph is empty — add a node first.
    .no-output-node = No output node — select one first.
    .evaluated = Evaluated { $count } node(s) at { $width }x{ $height }.
    .eval-failed = Eval failed: { $error }
    .save-material = Save as material
    .saved-material = Saved as { $id } — later graph edits never propagate.
    .save-material-empty = Nothing to save — the graph is empty.

## Graph canvas (noodle editor)

canvas =
    .empty-hint = Right-click to add a node
    .add-node = Add node

## Example-based texture synthesis node (§4 P1 row)

synth =
    .help = Multiresolution stochastic synthesis from an example image. map_mode: 0 = Random Walk (fills the whole canvas), 1 = Inpaint (needs a wired `mask` input; unmasked pixels pass through byte-identical). tiling_mode: 0 = clamped sampling, 1 = Tiled (wrap-sampled, seamless at the edges). randomness widens the candidate pool away from the single best match; seed makes the result reproducible.

## Display panel

display =
    .transform = Transform
    .view = View
    .exposure = Exposure (EV)
    .gamma = Gamma
    .preview = Preview (linear ramp, 1 EV per swatch):
    .chain-identity = identity (Raw, 0 EV, gamma 1)
    .chain = exposure { $exposure } EV → { $view } → gamma { $gamma }
    .live = Live in the 3D viewport (mesh pass) and the UV view (paint display) via the GPU display LUT.
    .rebuilding = Rebuilding the GPU display LUT for the 3D viewport and the UV view.
    .active-chain = Active chain: { $chain }. { $status }

## OCIO config adoption (Display panel section, §9 "in panel")
display-ocio =
    .title = OCIO / ACES
    .config = Config
    .space = Display space
    .space-pick = Pick a display space
    .none = Built-in curves only
    .unavailable = OCIO unavailable in this build — the vendored OCIO library is not compiled in, so the built-in ACES 2.0 configs cannot load. The fixed view transforms remain active.

## Post effects (Display panel section, §7 P1 row)

postfx =
    .title = Post effects
    .empty = No effects — the scene goes straight to the display transform.
    .add = Add effect
    .dof = Depth of field
    .bloom = Bloom
    .vignette = Vignette
    .sharpen = Sharpen
    .tonemap = Tone map
    .focus = Focus distance
    .aperture = Aperture
    .threshold = Threshold
    .intensity = Intensity
    .radius = Radius (px)
    .strength = Strength
    .softness = Softness
    .amount = Amount
    .mode = Curve
    .exposure = Exposure (EV)
    .mode-aces = ACES (fitted)
    .mode-filmic = Filmic
    .mode-reinhard = Reinhard
    .drag-hint = Drag to reorder
    .move-up = Move up
    .move-down = Move down
    .remove = Remove effect
    .summary = { $active } of { $total } effects enabled, applied top to bottom.
    .status-idle = Nothing enabled: the 3D viewport draws directly, with no offscreen pass.
    .status-live = Live in the 3D viewport: the scene renders scene-linear offscreen, runs through this chain, then the display transform.
    .status-bypassed = Bypassed in the 3D viewport: the current view mode shows raw data, which skips post effects and the display transform.
    .session-only = Session only (not saved with the project).

## Viewport view modes (§5 P0 row)

viewmode =
    .label = View mode
    .lit = Lit (V)
    .unlit = Unlit (V)
    .solo = Solo channel…
    .mesh-preview = Mesh-map preview…
    .channel = Channel
    .map = Map
    .basecolor = Base color
    .emissive = Emissive
    .roughness = Roughness
    .metallic = Metallic
    .ao = Ambient occlusion
    .height = Height
    .normal = Normal

## Brush properties panel

brush =
    .color = Color
    .save-as-label = Save as
    .new-preset-name = New preset name
    .import-abr = Import .abr…
    .abr-filter = Photoshop brushes
    .no-preset = No preset selected
    .cavity = Cavity mask
    .cavity-enable = Paint by curvature
    .cavity-cavities = Cavities
    .cavity-ridges = Ridges
    .cavity-threshold = Threshold
    .cavity-width = Softness
    .cavity-invert = Invert
    .cavity-no-map = Bake the Curvature map to use this (Bakes panel).
    .stamped = Bitmap stamp
    .stamp-angle = Stamp angle
    .angle-none = Unrotated
    .angle-fixed = Fixed
    .angle-follow = Follow stroke
    .angle-random = Random
    .angle-degrees = Angle
    .angle-offset = Offset
    .jitter-size = Size jitter
    .jitter-angle = Angle jitter
    .jitter-position = Scatter
    .jitter-flow = Flow jitter
    .stamp-image = Stamp image
    .stamp-none = None (a full square)
    .size = Size
    .method = Stroke method
    .method-space = Space
    .method-airbrush = Airbrush
    .method-dash = Dash
    .method-drag-dot = Drag dot
    .method-rate = Rate
    .method-dash-ratio = Dash ratio
    .method-dash-length = Dash length
    .preset = Preset
    .preset-none = None
    .broken = { $count } broken
    .overridden = { $count } overridden
    .name = Name
    .color = Color
    .alpha = Alpha
    .hardness = Hardness
    .pressure-gamma = Pressure gamma
    .pressure-curve = Pressure curve
    .pressure-flow = Pressure → flow
    .pressure-size = Pressure → size
    .stabilizer = Stabilizer (one-euro)
    .min-cutoff = Min cutoff
    .beta = Beta
    .d-cutoff = d cutoff
    .lazy-mouse = Lazy mouse
    .radius-px = Radius px
    .strength = Strength
    .spacing = Spacing
    .dabs-per-radius = Dabs per radius
    .pressure-alpha = Pressure → alpha
    .pressure-radius = Pressure → radius
    .curve-presets = Presets
    .curve-preset-linear = Linear
    .curve-preset-smooth = Smooth
    .curve-preset-ease = Ease
    .wash-mode = Mode
    .wash = Wash
    .build-up = Build-up
    .falloff = Falloff
    .falloff-smooth = Smooth
    .falloff-smoother = Smoother
    .falloff-sphere = Sphere
    .falloff-root = Root
    .falloff-sharp = Sharp
    .falloff-sharper = Sharper
    .falloff-linear = Linear
    .falloff-inverse-square = Inverse square
    .falloff-constant = Constant
    .falloff-custom = Custom
    .front-face-falloff = Front-face falloff
    .save-as-label = Save as
    .save-as-hint = New preset name
    .save-failed = Save failed: { $error }
    .save-as-needs-name = Save As needs a name
    .save-as-needs-preset = Save As needs an active preset
    .save-as-no-dir = Save As failed: no user preset dir
    .save-as-failed = Save As failed: { $error }
    .reset-failed = Reset failed: { $error }
    .tool = Tool
    .tool-paint = Paint
    .tool-smudge = Smudge
    .tool-clone = Clone
    .tool-eraser = Eraser
    .tool-pick = Eyedropper
    .tool-id-mask = ID mask
    .erase-channels = Erase channels
    .erase-hint = Erases alpha only by default; R/G/B erase that color channel.
    .erase-hint-more = Alpha-only by default (color data is kept, so undo restores it). Enabling R/G/B erases that color channel toward zero leaving alpha intact.
    .smudge-strength = Strength (smudge)
    .smudge-smear = Smear
    .clone-offset = Source offset (UV)
    .clone-follow = Follow stroke
    .clone-hint = Alt-click in the 2D UV view to set the source.
    .symmetry = Symmetry
    .symmetry-mirror = Mirror
    .symmetry-axis = Axis
    .symmetry-axis-x = U (X)
    .symmetry-axis-y = V (Y)
    .symmetry-offset = Offset (UV)
    .symmetry-hint = Drag the mirror line in the 2D UV view to move it.
    .symmetry-radial = Radial
    .symmetry-count = Copies
    .symmetry-flip-u = Flip U
    .symmetry-flip-v = Flip V
    .line-constraint = Straight line
    .line-constraint-off = Off
    .line-constraint-line = Line
    .line-constraint-snap = Angle snap
    .line-constraint-angle = Snap angle
    .line-constraint-hint = Anchored at the stroke start; angle snap locks the direction.
    .line-constraint-hint-more = Anchored at the stroke start; angle snap locks the direction to multiples of the snap angle.
    .projection = Projection
    .projection-screen = 3D (paint what you see)
    .projection-uv = UV only
    .projection-hint = 3D paints the mesh under the pointer; UV only lets the 3D view orbit.
    .projection-hint-more = 3D paints through the mesh under the pointer (misses skip); UV-only orbits in the 3D viewport.
    .quick-mask = Quick mask
    .quick-mask-mode = Mask mode (X): strokes paint the mask
    .quick-mask-active = Gate painting with the mask
    .quick-mask-paint = Mask paint
    .quick-mask-allow = White (allow)
    .quick-mask-block = Black (block)
    .quick-mask-invert = Invert (I)
    .quick-mask-clear = Clear
    .quick-mask-inverted = inverted
    .quick-mask-hint = Red marks blocked texels. Saved with the project, not undoable.
    .quick-mask-hint-more = Red marks blocked texels. The mask is saved with the project (8-bit planes in the paint sidecar) but is not undoable.

## Layers panel

layers =
    .duplicate = Duplicate layer
    .set-header = Texture set: { $set }
    .add-paint = + Paint layer
    .add-menu = + ▼
    .add-menu-hint = Add a fill, folder or material layer
    .masks-section = Masks
    .masks-empty = Add a layer, then give it a mask here.
    .mask-add = + Mask
    .mask-default-name = Mask for { $layer }
    .mask-remove = Remove mask
    .mask-done = Done
    .mask-edit = Paint mask
    .default-paint-name = Paint { $n }
    .default-fill-name = Fill { $n }
    .kind-paint = paint
    .kind-fill = fill
    .kind-folder = folder
    .kind-material = material
    .kind-instance = instance
    .instance-this = ⊗ Instance
    .instance-of = instance of { $source }
    .instance-name = { $source } (instance)
    .instance-paint-hint = Paint the source, not the instance.
    .attach-smart-mask-tip = Attach this smart mask
    .delete-layer-tip = Delete layer (asks for confirmation)
    .add-smart-material-tip = Add this smart material
    .add-material = + Add material layer
    .default-material-name = Material { $n }
    .flatten = Flatten selection
    .flatten-needs-two = Select two or more layers to flatten.
    .material-channels = Material channels
    .material-id = Material
    .material-id-none = (no material)
    .material-no-defs = No materials yet — save a graph as a material first.
    .material-reset-channels = Reset channels
    .material-refresh = Refresh material
    .material-channel-opacity = Opacity
    .material-badge-no-params = No material — pick one below.
    .material-badge-missing-def = Missing definition { $id } — re-point or re-save it.
    .material-ignored = Ignores undeclared channels: { $channels }
    .material-unpaintable = Publishes through refresh — strokes cannot paint it.
    .fill-unpaintable = Publishes procedurally — strokes cannot paint it. Paint the mask instead.
    .folder-unpaintable = Holds no pixels — select a paint layer.
    .base-paint = Base paint
    .base-paint-hint = Paint that belongs to no layer: v1 projects, and strokes made with no layer selected.
    .budget-blocked = Plane budget blocked ({ $bytes } bytes)
    .budget-line = Plane memory { $used } / { $cap } bytes
    .stroke-refused = Stroke refused
    .tombstone-lost = Layer { $layer } revives empty on undo (its tombstone was freed)
    .dormant-badge = Pre-v2 result lives in the Base paint

## Fill layer properties (projection picker + v1 source editor)

fill =
    .section = Fill layers
    .none = No fill layers — add one from the Layers panel.
    .proj-uv = UV
    .proj-triplanar = Tri-planar
    .proj-planar = Planar
    .proj-spherical = Spherical
    .proj-cylindrical = Cylindrical
    .axis = Axis
    .axis-x = X
    .axis-y = Y
    .axis-z = Z
    .sharpness = Sharpness
    .scale = Scale
    .src-solid = Solid color
    .src-gradient = Gradient
    .src-noise = Noise
    .src-graph = Graph snapshot
    .alpha = Alpha
    .color-a = Color A
    .color-b = Color B
    .angle = Angle
    .seed = Seed
    .octaves = Octaves
    .graph-note = Snapshot only: re-evaluate the graph to refresh it (live graph wiring is a follow-up).
    .poly-tool = Polygon fill
    .poly-armed = Capture clicks (lasso)
    .poly-hint = While capturing, clicks in the 2D UV view build the mask instead of painting.
    .poly-mode-polygon = Polygon (lasso)
    .poly-mode-triangle = Triangle
    .poly-mode-mesh = Mesh island
    .poly-mode-uv-chunk = UV chunk
    .poly-points = Lasso points
    .poly-pick = Picked triangle
    .poly-no-pick = none
    .poly-undo-point = Undo point
    .poly-clear = Clear
    .poly-apply = Apply mask
    .poly-clear-mask = Clear mask

## History panel

history =
    .section-controls = Undo / redo
    .section-log = Session log
    .undo = ↺ Undo
    .redo = ↻ Redo
    .split-redo = ⇥ Split redo
    .split-done = { $count ->
        [one] Split: redid 1 stroke of the merged entry
       *[other] Split: redid the first of { $count } merged strokes
    }
    .split-refused = Nothing to split (the next redo is a single stroke, or the envelope is full).
    .journal = { $count ->
        [one] { $count } entry in journal
       *[other] { $count } entries in journal
    }
    .empty = No history yet.
    .op-add-paint = Add paint layer
    .op-add-fill = Add fill layer
    .op-add-folder = Add folder layer
    .op-add-material = Add material layer
    .op-add-instance = Add instance layer
    .op-remove = Remove layer
    .op-reorder = Reorder layer
    .op-set-opacity = Set opacity
    .op-set-visible = Set visibility
    .op-set-blend = Set blend mode
    .op-rename = Rename layer
    .op-set-fill = Fill layer
    .op-set-material = Set material
    .op-set-instance-source = Set instance source
    .op-attach-mask = Attach mask
    .op-detach-mask = Detach mask
    .op-set-mask-enabled = Toggle mask
    .op-flatten = Flatten layers
    .op-brush-stroke = Brush stroke
    .op-graph-add-node = Add graph node
    .op-graph-remove-node = Remove graph node
    .op-graph-connect = Connect graph nodes
    .op-graph-disconnect = Disconnect graph nodes
    .op-graph-set-param = Set graph param
    .op-graph-set-output = Set graph output
    .op-graph-load = Load graph
    .op-bake = Bake
    .op-export = Export
    .op-filter-apply = Apply filter
    .strokes = Smudge / clone / eraser strokes
    .undo-stroke = ↺ Undo stroke
    .redo-stroke = ↻ Redo stroke
    .travel-note = Layer rows rewind layers, stroke rows rewind paint — each travels on its own.

## UDIM tile selector (Bakes + Export)

tiles =
    .none = none
    .label = Tiles:
    .tile = Tile { $tile }

## Size combo (Bakes + Export)

size =
    .vram-note = { $size } × { $size } (~{ $mib } MB/texel-buffer)

## Mesh-map import by naming convention (§7 P1 row)

import =
    .title = Mesh-map import
    .hint = Drop or choose a folder of TextureSetName_map.png files.
    .hint-more = Drop a folder of TextureSetName_map.png files (what the baker emits), or choose one below. Known maps route to their texture-set channels; unknown suffixes become user channels. Every import creates a new layer — nothing is ever overwritten.
    .choose = Choose folder…
    .apply = Import as layers
    .need-project = Save or open a project first — imports are stored content-hashed in the project before they become layers.
    .no-files = No files scanned yet.
    .scan-failed = Folder scan failed: { $error }
    .scanned = { $matched } file(s) routed, { $unmatched } skipped.
    .applied = { $count ->
        [one] Imported { $count } file as a new layer.
       *[other] Imported { $count } files as new layers.
    }
    .matched-title = { $count ->
        [one] { $count } routed file
       *[other] { $count } routed files
    }
    .unmatched-title = { $count ->
        [one] { $count } skipped file
       *[other] { $count } skipped files
    }
    .row-matched = { $file } → { $set } / { $channel } as “{ $layer }”
    .row-unmatched = { $file } — { $reason }
    .reason-not-png = not a PNG file (the baker emits PNG)
    .reason-no-separator = no “_” separator — not a TextureSetName_map name
    .reason-empty-set = nothing before the “_” — no texture-set name
    .reason-empty-suffix = nothing after the “_” — no map name
    .reason-tile-suffixed = tile-suffixed UDIM output (Set_map_tile) — import the per-tile files individually
    .reason-unsafe-channel = the map name holds a character a channel folder cannot use on every OS (: \ / * ? < > | " or a control character)
    .size-mismatch = Size mismatch — layer left empty
    .budget-refused = Plane budget refused ({ $bytes } bytes) — layer left empty
    .decode-failed = File undecodable — layer left empty

## PSD flatten-import (§7 row 101 — the composite-first honest flatten)

psd-import =
    .title = PSD import
    .hint = Drop a .psd file to import its flattened image as a texture set.
    .hint-more = Drop a Photoshop .psd file. The flattened composite imports as a texture set at its native resolution; when a composite is absent the honest rasterizer applies pixel layers, group opacity and 1:1 masks, and every deviation lands in the import report.
    .no-files = No PSD scanned yet.
    .scanned = { $matched } PSD file(s) routed, { $unmatched } skipped.
    .apply = Import PSD
    .report-title = Import report
    .row-matched = { $file } → set “{ $set }”
    .row-unmatched = { $file } — { $reason }
    .reason-not-psd = not a .psd file

## .spp import (ruling 1 — import-only forever; the report is the artifact)

spp-import =
    .extract-channels = Extract { $count } channel(s) as layers
    .title = Substance .spp import
    .hint = Drop a Substance 3D Painter .spp project to import its channels.
    .hint-more = Drop an Adobe Substance 3D Painter .spp project. The reader imports texture-set channels it can honestly parse and reports every dataset — HBO graphs are listed by era, not imported; geometry stays FBX/USD.
    .no-files = No .spp file scanned yet.
    .apply = Import .spp
    .row-pending = { $file } waiting
    .report-title = { $file } — Substance { $version }
    .set-row = Set “{ $set }” — { $count } channel(s)
    .channel-row = { $channel } ({ $w}×{ $h}, { $c} component(s))
    .note-row = { $note }

## 2D UV view

uv =
    .no-mesh = no mesh loaded
    .mask-mode = MASK MODE (X to exit)
    .isolate = Channel
    .pick-applied = Picked { $color } into the fill layer

## Paint path tools (§2 P1 row)

path =
    .tool = Paint path
    .armed = Edit path (captures clicks)
    .hint = While editing, clicks in the 2D UV view build the path.
    .hint-more = While editing, clicks in the 2D UV view build the path instead of painting. Drag points to move them, click a segment to insert, right-click to delete.
    .op-paint = Paint
    .op-erase = Erase
    .op-smudge = Smudge
    .closed = Closed loop
    .points = Points
    .selected = Selected
    .size = Size
    .opacity = Opacity
    .replay = Replay path
    .fill = Fill closed path
    .clear = Clear path
    .no-paint = No paint session — replay needs the GPU target.
    .need-points = Add at least two points to replay.

## HDR surface output (§5 P1 row 85)

display-hdr =
    .title = HDR
    .toggle = HDR output
    .reason-not-exposed = Not available yet: this build's windowing layer (eframe) doesn't expose the surface's color space, so umber can't ask the display whether it supports HDR.
    .reason-sdr-only = This adapter or surface only offered the SDR color space.

## Why an action is unavailable (the registry's enabled-check, GUI plan G0)

action =
    .unavailable-nothing-to-undo = Nothing to undo
    .unavailable-nothing-to-redo = Nothing to redo
    .unavailable-no-mesh = Load a mesh first
    .unavailable-not-editing-mask = Only while editing the quick mask (X)
    .unavailable-bake-running = A bake is already running

## Status-bar mouse/key hints (GUI plan G1 g1-status-hints)

hint =
    .drop = Drop
    .straight-line = straight line from the last stroke
    .ctrl-shift = +Ctrl
    .snap-angle = snap to 5°
    .rmb-drag = RMB drag
    .resize = size / hardness
    .brackets = [ ]
    .size = brush size
    .hud-size = Size
    .hud-hardness = Hardness
    .hud-flow = Flow
    .hud-angle = Angle
    .alt-lmb = Alt+LMB
    .alt-mmb = Alt+MMB
    .alt-rmb = Alt+RMB
    .mmb = MMB
    .shift-mmb = Shift+MMB
    .load-mesh = a mesh to start
    .ctrl-lmb = Ctrl+LMB
    .lmb-drag = LMB drag
    .lmb = LMB
    .shift-lmb = Shift+LMB
    .wheel = Wheel
    .ctrl-z = Ctrl+Z
    .paint = paint
    .paint-stroke = paint the stroke
    .orbit = orbit
    .pan = pan
    .zoom = zoom
    .undo = undo
    .sample-color = sample the color
    .select-region = select the region
    .ctrl-alt-lmb = Ctrl+Alt+LMB
    .material-pick = pick material into fill layer

## Toast notices (GUI plan G0 integration)

notice =
    .mesh-load-failed = Mesh load failed
    .mesh-upload-failed = Mesh upload failed
    .environment-failed = Environment load failed
    .filter-failed = Filter apply failed
    .export-failed = Export failed
    .save-failed = Project save failed
    .open-failed = Project open failed

## Pixel autosave recovery (C2-3)

recovery =
    .title = Recover unsaved work?
    .body = Umber closed without saving last time. An autosaved copy of that work (layers and paint) is available.
    .recover = Recover

## Command palette (GUI plan G3)

palette =
    .placeholder = Type a command…
    .path-mask = Quick mask
    .path-tools = Tools
    .path-brush = Brush
    .brush-smaller = Smaller brush
    .brush-larger = Larger brush

## Preferences (GUI plan G3)

prefs =
    .title = Preferences
    .search = Search settings…
    .section-interface = Interface
    .ui-scale = UI scale
    .ui-scale-auto = UI scale: auto (follow the system)
    .splitter-hit-zone = Larger splitter grab zone (tablet)
    .tooltips = Show tooltips
    .reduce-motion = Reduce motion
    .key-hints = Status-bar key hints
    .show-fps = Frame-rate readout in the status bar
    .theme = Theme
    .brush-shelf = Brush shelf under the 3D view
    .section-navigation = Navigation
    .nav-preset = Mouse navigation
    .nav-painter = Painter (Alt + mouse)
    .nav-umber = Umber classic
    .nav-blender = Blender (middle mouse)
    .nav-wheel-invert = Invert wheel zoom (up = zoom out)
    .nav-env-override = UMBER_NAV is set: it overrides this choice for this launch.
    .section-keymap = Keymap
    .keymap-preset = Preset
    .keymap-preset-umber = Umber (default)
    .keymap-preset-blender = Blender-like
    .keymap-preset-custom = Custom
    .keymap-search = Search actions or keys (e.g. ctrl s)…
    .keymap-col-action = Action
    .keymap-col-binding = Shortcut
    .keymap-col-alt = Alternate
    .keymap-unbound = —
    .keymap-press-keys = Press keys…
    .keymap-button-tip = Click, then press the new shortcut. Esc cancels; Backspace clears (or restores the default).
    .keymap-conflict = Conflicts with { $action }: not assigned.
    .keymap-copy = Copy changed bindings
    .keymap-paste = Paste bindings
    .keymap-paste-hint = keymap=…
    .keymap-paste-invalid = Not a keymap line: nothing changed.
    .keymap-paste-conflict = { $key } would be both { $first } and { $second }: nothing changed.

## Rich tooltips (GUI plan G3): descriptions only, the registry prints the key

tip =
    .hide-ui = Hide the panels, toolbar and status bar: the viewport fills the window. Tab again (or the Show UI button) brings them back.
    .maximize = Fill the window with the focused area, keeping the menu, toolbar and status bar.
    .open-project = Open a saved .umber project.
    .save-project = Save the project.
    .undo = Undo the last layer edit.
    .redo = Redo the last undone layer edit.
    .wireframe = Show the mesh wireframe over the paint.
    .grid = Show the ground grid.
    .view-cycle-title = Shading mode
    .view-cycle = Switch between lit and unlit shading.
    .mask = Enter or leave quick-mask mode: strokes paint the mask instead of the layer.
    .invert-mask = Invert the quick mask.
    .palette-title = Command palette
    .palette = Find and run any command by name.
    .brush-popup = Pick a brush from its swatch, at the cursor.

## Workspaces (GUI plan g5-workspaces)

workspace =
    .paint = Paint
    .bake = Bake
    .export = Export
    .graph = Graph
    .next = Next Workspace
    .prev = Previous Workspace

## Display-mode keys (GUI plan g6-display-mode-keys)

viewkeys =
    .material = View Material
    .next-channel = Next Channel
    .prev-channel = Previous Channel
    .next-map = Next Mesh Map
    .prev-map = Previous Mesh Map
    .preset-both = Viewport preset: 3D + 2D
    .preset-3d = Viewport preset: 3D only
    .preset-2d = Viewport preset: 2D (UV) only
    .preset-swap = Swap the 3D and 2D views
    .toast-material = Viewing the material
    .toast-unlit = Viewing unlit
    .toast-channel = Channel:
    .toast-map = Mesh map:

## Property menu (GUI plan g4-property-context-menu)

propmenu =
    .reset = Reset to Default
    .copy = Copy Value
    .paste = Paste Value
    .set-default = Set as Default
    .assign-shortcut = Assign Shortcut
    .capture-hint = Press a key for { $label } — Esc cancels, Backspace clears
    .bound = { $label } bound to { $key }
    .unbound = { $label } unbound
    .capture-cancelled = Shortcut capture cancelled

## Contextual toolbar strip (GUI plan g6-contextual-toolbar)

strip =
    .size = Size
    .flow = Flow
    .spacing = Spacing
    .lazy = Lazy mouse (D)
    .symmetry = Symmetry (L)
    .no-paint = no paint session
    .no-preset = no preset selected

# The viewport's Overlays button + popover (GUI plan g6-overlays-popover).
overlays =
    .filter-raster = PNG / SVG
    .button = Overlays
    .tooltip = Viewport overlays: wireframe, grid, UV seams, face orientation, channel name.
    .wireframe = Wireframe
    .wireframe-opacity = Opacity
    .grid = Ground grid
    .uv-seams = UV seams
    .face-orientation = Face orientation
    .face-orientation-pending = Back-face tint needs the renderer update (not drawn yet).
    .channel-readout = Channel name readout
    .stencil = Stencil
    .stencil-enable = Paint through stencil
    .stencil-opacity = Stencil opacity
    .stencil-reset = Reset stencil
    .stencil-load = Load…
    .stencil-no-image = No stencil image
    .stencil-load-failed = Stencil not loaded: { $reason }
    .stencil-unreadable = the file could not be read.
    .stencil-too-large = { $width }×{ $height } is over the { $max }-pixel limit.
    .stencil-unsupported = not a supported PNG (8-bit RGB or RGBA) or SVG.
    .stencil-unsupported-svg = the SVG could not be used: { $detail }
    .stencil-n-hint = N toggles the stencil — paint passes through while it is ignored.
    .stencil-gate-live = Gate live ({ $width }×{ $height }, { $open }% open) — paint passes only where it opens.
    .projection = Projection
    .projection-no-image = no image
    .projection-load = Load…
    .projection-load-failed = projection load failed: { $reason }
    .projection-arm = Arm (freeze view)
    .projection-apply = Apply to paint
    .projection-opacity = Opacity
    .projection-tile = Tile
    .projection-tile-hint = Repeat the image across the frozen view instead of leaving its outside clear. The overlay previews one tile.
    .projection-follow = Follow camera
    .projection-follow-hint = Apply projects through the current camera and view instead of the view frozen at Arm.
    .projection-cull-backfaces = Cull back faces
    .projection-cull-backfaces-hint = Surfaces facing away from the camera do not take the decal. Uncheck to paint both sides.
    .projection-occlusion = Occlusion
    .projection-occlusion-hint = Texels behind a nearer surface do not take the decal. Uncheck to let every covered texel write.

## Shared colour-picker row labels (picker-app-sites; channel letters are
## the international convention — a locale may localize them)
picker =
    .r = R
    .g = G
    .b = B
    .h = H
    .s = S
    .v = V

## The Assets panel's library browser (R-C1)

assets =
    .catalog-all = All
    .catalog-basic = Basic
    .catalog-detail = Detail
    .catalog-texture = Texture
    .catalog-erase = Erase
    .catalog-utility = Utility
    .catalog-other = Other
    .refresh = Refresh the list
    .search = Search
    .brushes = Brushes
    .materials = Smart materials
    .env-maps = Environment maps
    .filters = Filters
    .filter-apply-tip = Apply this filter to the active layer (one undo step)
    .empty = Nothing here yet.
    .no-matches = No matches for this search.
    .saved-searches = Saved searches
    .save-current = Save current
    .save-current-tip = Save the current search under the entered name (the query itself when no name is entered)
    .saved-name-hint = Name for the saved search
    .saved-apply-tip = Apply this saved search
    .saved-delete-tip = Delete this saved search
    .import = Import…
    .import-title = Import
    .extra-dirs = Extra library folders
    .add-dir-tip = Add this folder to the library
    .remove-dir-tip = Remove this folder from the library
    .use-brush-tip = Use this brush
    .add-material-tip = Add this material to the layer
    .attach-mask-tip = Attach this mask to the layer
    .drag-to-layers = Drag onto the Layers panel: materials become a new layer, masks bind to the layer under the drop. Release outside any panel to drag a copy out to disk.
    .drag-out-saved = Drag-out saved a copy to { $path }.
    .drag-out-failed = Drag-out failed

# Command labels (queue row 154, wave 2b-i) — cmd.<dotted id with dashes>
cmd-file-new-project = New Project
cmd-file-open-mesh = Open Mesh…
cmd-file-open-project = Open Project…
cmd-file-save-project = Save
cmd-file-save-project-as = Save As…
cmd-edit-undo = Undo
cmd-edit-redo = Redo
cmd-edit-split-redo = Split redo
cmd-view-toggle-wireframe = Toggle Wireframe
cmd-view-toggle-grid = Toggle Grid
cmd-view-cycle-view-mode = Cycle View Mode
cmd-view-view-material = View Material
cmd-view-next-channel = Next Channel
cmd-view-prev-channel = Previous Channel
cmd-view-next-mesh-map = Next Mesh Map
cmd-view-prev-mesh-map = Previous Mesh Map
cmd-view-viewport-preset-both = Viewport Preset: Both
cmd-view-viewport-preset-3d = Viewport Preset: 3D
cmd-view-viewport-preset-2d = Viewport Preset: 2D
cmd-view-viewport-preset-swap = Swap Viewport Presets
cmd-paint-toggle-mask-mode = Toggle Quick Mask
cmd-paint-invert-mask = Invert Mask
cmd-tool-brush-smaller = Smaller Brush
cmd-tool-brush-larger = Larger Brush
cmd-tool-brush-popup = Brush Popup
cmd-tool-select = Select Tool
cmd-window-toggle-hide-ui = Toggle Hide UI
cmd-window-toggle-maximize = Toggle Maximize
cmd-window-command-palette = Command Palette
cmd-window-preferences = Preferences
cmd-window-next-workspace = Next Workspace
cmd-window-prev-workspace = Previous Workspace
cmd-layer-add-paint = Add paint layer
cmd-layer-add-fill = Add fill layer
cmd-layer-add-folder = Add folder layer
cmd-window-show-layers = Show Layers panel
cmd-window-show-texture-sets = Show Texture Sets panel
cmd-window-show-assets = Show Assets panel
cmd-window-show-bakes = Show Bakes panel
cmd-window-show-export = Show Export panel
cmd-window-show-history = Show History panel
cmd-bake-run = Bake (current settings)
cmd-export-run = Export (current settings)
cmd-window-fold-left = Fold left column to a rail
cmd-window-fold-right = Fold right column to a rail
rail-expand = Unfold the column

## The panel-group menu under each tab bar's end button (UI/UX U2-3).
dock =
    .group-closed = Closed panels

## Blend modes (audit F10 — never the Debug name)

blend =
    .normal = Normal
    .passthrough = Passthrough (folder)
    .multiply = Multiply
    .screen = Screen
    .overlay = Overlay
    .darken = Darken
    .lighten = Lighten
    .add = Add (Linear Dodge)
    .subtract = Subtract
    .difference = Difference
    .soft-light = Soft Light
    .hard-light = Hard Light
    .color-dodge = Color Dodge
    .color-burn = Color Burn
    .linear-burn = Linear Burn
    .divide = Divide
    .inverse-divide = Inverse Divide
    .inverse-subtract = Inverse Subtract
    .exclusion = Exclusion
    .add-sub = Add/Subtract
    .vivid-light = Vivid Light
    .linear-light = Linear Light
    .pin-light = Pin Light
    .disable = Disable
    .replace = Replace
    .tint = Tint
    .saturation = Saturation
    .color = Color
    .value = Value
    .normal-combine = Normal (Combine)
    .normal-detail = Normal (Detail)
    .normal-inverse-detail = Normal (Inverse Detail)
