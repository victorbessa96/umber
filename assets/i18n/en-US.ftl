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
    .view = View
    .help = Help
    .open-mesh = Open Mesh…
    .open-project = Open Project…
    .save-project = Save Project…
    .export-png = Export Painted Map (PNG)…
    .load-environment = Load Environment…
    .perf-hud = Show Perf HUD
    .perf-hud-unavailable = Perf HUD needs --features perf
    .wireframe = Show Wireframe (W)
    .grid = Show Grid (G)
    .about = About Umber
    .about-text = Umber v0.1.0 — Wave 2 in progress

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

    .open = Open…
    .tip-open-mesh = Load a mesh to paint (or drop one on the viewport)
    .tip-open-project = Open a .umber project (Ctrl+O)
    .tip-save-project = Save the project (Ctrl+S)
    .undo = Undo
    .redo = Redo
    .tip-undo = Undo the last layer edit (Ctrl+Z)
    .tip-redo = Redo the last undone layer edit (Ctrl+Shift+Z)
    .wireframe = Wireframe
    .tip-wireframe = Show the mesh wireframe (W)
    .grid = Grid
    .tip-grid = Show the ground grid (G)
    .view-lit = Lit
    .view-other = Unlit / preview
    .tip-view-cycle = Switch between lit and unlit shading (V)
    .mask = Mask
    .tip-mask = Enter or leave quick-mask mode (X)
    .invert-mask = Invert
    .tip-invert-mask = Invert the quick mask (I)
    .hero-title = Drop a mesh here to start painting
    .hero-release = Release to load the mesh
    .hero-hint = or use Open Mesh… in the toolbar or the File menu
    .hero-formats = Supported:

## Texture Sets panel

texturesets =
    .set = Texture set
    .size = Paint target size
    .resize-clears = Changing the selected set's size clears that set's canvas and the stroke undo history, and resets its quick mask. This is intentional.
    .inherit = Inherit document resolution
    .per-channel = Per-channel sizes
    .channel-resize-clears = Resizing a channel clears its tiles
    .no-paint = No paint session — the GPU target is unavailable.

## Bakes panel

bakes =
    .map-ao = Ambient occlusion
    .map-curvature = Curvature
    .map-thickness = Thickness
    .map-position = Position
    .no-mesh = No mesh loaded — open a mesh to enable baking.
    .no-gpu = No GPU device — baking needs the wgpu device.
    .resolution = Resolution
    .rays = Rays
    .dilation = Dilation
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
    .view-canvas = Canvas
    .view-list = List
    .canvas-help = Drag empty space to pan, wheel to zoom, drag an output port onto an input to connect, right-click to add, Delete removes the selected edge.
    .output = Output: { $node }
    .nodes = Nodes
    .no-nodes = No nodes yet — add one below.
    .add-node = Add Node
    .remove-selected = Remove selected
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
    .no-preset = No preset selected
    .preset = Preset
    .preset-none = None
    .broken = { $count } broken
    .overridden = { $count } overridden
    .name = Name
    .color = Color
    .alpha = Alpha
    .hardness = Hardness
    .pressure-gamma = Pressure gamma
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
    .erase-hint = Alpha-only by default (color data is kept, so undo restores it). Enabling R/G/B erases that color channel toward zero leaving alpha intact.
    .smudge-strength = Strength
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
    .line-constraint-hint = Anchored at the stroke start; angle snap locks the direction to multiples of the snap angle.
    .projection = Projection
    .projection-screen = 3D (paint what you see)
    .projection-uv = UV only
    .projection-hint = 3D paints through the mesh under the pointer (misses skip); UV-only orbits in the 3D viewport.
    .quick-mask = Quick mask
    .quick-mask-mode = Mask mode (X): strokes paint the mask
    .quick-mask-active = Gate painting with the mask
    .quick-mask-paint = Mask paint
    .quick-mask-allow = White (allow)
    .quick-mask-block = Black (block)
    .quick-mask-invert = Invert (I)
    .quick-mask-clear = Clear
    .quick-mask-inverted = inverted
    .quick-mask-hint = Red marks blocked texels. The mask lasts for this session only: it is not saved and not undoable.

## Layers panel

layers =
    .add-paint = ＋ Add paint layer
    .add-fill = ＋ Add fill layer
    .masks-section = Masks
    .mask-add = ＋ Mask
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
    .instance-this = ⧉ Instance
    .instance-of = instance of { $source }
    .instance-name = { $source } (instance)
    .instance-paint-hint = Paint the source, not the instance.
    .add-material = ＋ Add material layer
    .default-material-name = Material { $n }
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
    .undo = ↶ Undo
    .redo = ↷ Redo
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
    .op-brush-stroke = Brush stroke
    .op-bake = Bake
    .op-export = Export
    .strokes = Smudge / clone / eraser strokes
    .undo-stroke = ↶ Undo stroke
    .redo-stroke = ↷ Redo stroke
    .travel-note = Layer rows rewind layers, stroke rows rewind paint — each travels on its own.

## UDIM tile selector (Bakes + Export)

tiles =
    .label = Tiles:
    .tile = Tile { $tile }

## Size combo (Bakes + Export)

size =
    .vram-note = { $size } × { $size } (~{ $mib } MB/texel-buffer)

## Mesh-map import by naming convention (§7 P1 row)

import =
    .title = Mesh-map import
    .hint = Drop a folder of TextureSetName_map.png files (what the baker emits), or choose one below. Known maps route to their texture-set channels; unknown suffixes become user channels. Every import creates a new layer — nothing is ever overwritten.
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
    .size-mismatch = Size mismatch — layer left empty
    .budget-refused = Plane budget refused ({ $bytes } bytes) — layer left empty
    .decode-failed = File undecodable — layer left empty

## PSD flatten-import (§7 row 101 — the composite-first honest flatten)

psd-import =
    .title = PSD import
    .hint = Drop a Photoshop .psd file. The flattened composite imports as a texture set at its native resolution; when a composite is absent the honest rasterizer applies pixel layers, group opacity and 1:1 masks, and every deviation lands in the import report.
    .no-files = No PSD scanned yet.
    .scanned = { $matched } PSD file(s) routed, { $unmatched } skipped.
    .apply = Import PSD
    .report-title = Import report
    .row-matched = { $file } → set “{ $set }”
    .row-unmatched = { $file } — { $reason }
    .reason-not-psd = not a .psd file

## .spp import (ruling 1 — import-only forever; the report is the artifact)

spp-import =
    .title = Substance .spp import
    .hint = Drop an Adobe Substance 3D Painter .spp project. The reader imports texture-set channels it can honestly parse and reports every dataset — HBO graphs are listed by era, not imported; geometry stays FBX/USD.
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

## Paint path tools (§2 P1 row)

path =
    .tool = Paint path
    .armed = Edit path (captures clicks)
    .hint = While editing, clicks in the 2D UV view build the path instead of painting. Drag points to move them, click a segment to insert, right-click to delete.
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
