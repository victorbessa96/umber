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
        [one] — 1 skew-clamped texel
       *[other] — { $count } skew-clamped texels
    }
    .skipped-skew-note = skew bakes tile 1001 only, skipped tiles: { $tiles }

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

## Graph canvas (noodle editor)

canvas =
    .empty-hint = Right-click to add a node
    .add-node = Add node

## Display panel

display =
    .view = View
    .exposure = Exposure (EV)
    .gamma = Gamma
    .preview = Preview (linear ramp, 1 EV per swatch):
    .chain-identity = identity (Raw, 0 EV, gamma 1)
    .chain = exposure { $exposure } EV → { $view } → gamma { $gamma }
    .live = Live in the 3D viewport (mesh pass) and the UV view (paint display) via the GPU display LUT.
    .rebuilding = Rebuilding the GPU display LUT for the 3D viewport and the UV view.
    .active-chain = Active chain: { $chain }. { $status }

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
    .default-paint-name = Paint { $n }
    .default-fill-name = Fill { $n }
    .kind-paint = paint
    .kind-fill = fill
    .kind-folder = folder
    .kind-instance = instance
    .instance-this = ⧉ Instance
    .instance-of = instance of { $source }
    .instance-name = { $source } (instance)
    .instance-paint-hint = Paint the source, not the instance.

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
    .op-add-instance = Add instance layer
    .op-remove = Remove layer
    .op-reorder = Reorder layer
    .op-set-opacity = Set opacity
    .op-set-visible = Set visibility
    .op-set-blend = Set blend mode
    .op-set-fill = Fill layer
    .op-set-instance-source = Set instance source
    .op-brush-stroke = Brush stroke
    .op-bake = Bake
    .op-export = Export
    .strokes = Smudge / clone / eraser strokes
    .undo-stroke = ↶ Undo stroke
    .redo-stroke = ↷ Redo stroke

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
