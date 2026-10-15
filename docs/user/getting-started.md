# Getting started with Umber

This guide walks the core loop once: open a mesh, bake its maps, paint,
save, and export textures an engine can use. Every step names the real
menu or panel. Shortcuts are listed in [keyboard.md](keyboard.md).

Umber is pre-release, so expect gaps. What is not there yet is tracked in
`docs/ROADMAP.md`, and this guide says so where a gap affects a step.

## 0. Run it

```bash
cargo run --release -p umber-app
```

Umber needs a Vulkan (Linux) or DX12 (Windows) GPU. If the driver
crashes at startup, the next launch automatically steps down to a safer
backend; `--safe-gpu` forces that. Without a usable GPU the app still
opens, but baking, painting and export say they need the device.

## 1. Open a mesh

**File ▸ Open Mesh…**, or drop a file on the viewport. Umber reads FBX,
glTF/GLB, OBJ, USDA, USDC and USDZ.

The mesh needs a **UV unwrap**: paint and bakes live in UV space, so a
mesh without UVs (every UV at one point) paints nothing useful. Meshes
with UDIM tiles (1001, 1002, …) work, and each tile is its own texture.

Orbit, pan and zoom the viewport with the mouse. The **2D UV** panel
shows the same paint flat, in UV space.

## 2. Bake the mesh maps

Open the **Bakes** panel. Pick the maps (ambient occlusion, curvature,
thickness, position and the tangent normal are on by default; world-space
normal, ID and bent normal are opt-in), a **Resolution**, the **Rays** per
texel (more rays mean smoother AO and a slower bake) and **Dilation**,
the number of texels each UV island bleeds past its edge so mip-mapping
never samples the background.

**Margin type** decides what fills that dilation:

- **Extend** repeats each island's border.
- **Adjacent faces** first copies the texels from the neighbouring face
  across each UV seam, so a seam stays invisible when the texture is
  filtered.

Then press **Bake**.

To bake detail from a sculpt, use the **High poly** group: choose the
high-poly mesh, set **Cage front** and **Cage back** (how far outside and
inside the low surface a ray looks for the high one, in world units), tick
**Transfer normal** and/or **Transfer height** and press **Bake from high**. The maps land
beside the others as `<set>_normal_base.png` and `<set>_height.png`. The newest
baked normal (from either button) is what **Export** writes in the normal
slot, when its size matches the export; a painted normal still wins. The
baked maps feed the viewport's mesh-map view
modes (press `B`), the brush's cavity mask, and the export.

## 3. Paint

The **Layers** panel holds the stack. Add a paint layer, then paint in
the viewport (in 3D, on the mesh) or in the 2D UV view.

- **The Brush panel** sets size (`[` and `]`), alpha, spacing, hardness,
  the falloff curve, pressure response and the stabilizer. Pen pressure
  works through the tablet's standard input (Windows Ink is a
  `stylus --features winink` build option, not in the default build).
- **Channel** (top of the Brush panel's paint section) picks what you
  paint into: base color, roughness, metallic, height, normal. Painting
  roughness changes how shiny the surface is, not its colour. Press `C`
  to *view* channels one at a time in the viewport.
- **Tools**: paint, smudge, clone, erase (`E`), pick, ID mask.
- **Masks**: `X` toggles mask painting on the selected layer; `I`
  inverts the mask.
- **Cavity mask** (Brush panel) restricts a stroke to the baked curvature
  map's cavities or ridges: paint dirt into creases, or wear onto edges.

Layers can be reordered (▲ ▼), duplicated (⧉), renamed, faded (opacity)
and blended (the blend-mode list). Fill layers cover a whole area with a
colour, gradient or noise. Material layers are live node-graph materials,
edited in the **Graph** panel. The collapsed **Material ▸ .umbersmart**
section adds a whole smart-material stack in one click.

**Smart materials** live in the Layers panel's `.umbersmart` shelf. Applying
one stacks its layers; each fill driven by a mesh map (AO, curvature,
position, ...) takes the newest bake of that map from the Bakes panel, so
bake first. A map that was never baked leaves its fill plain (the log names
it); bake it and apply again.

**Smart masks** (`.umbermask`) sit in the same shelf. Select a layer, then
press ＋ on a mask: it attaches a mask to that layer and fills it from the
preset, e.g. **Edge wear** opens the mask where the baked curvature is
sharp. It is ordinary mask paint afterwards, so brush over it to touch it
up. A mask stage can also read another layer's channel by that layer's
name (e.g. a "Dirt" layer's height). If no layer has that name, the stage
leaves the mask open. Smart materials whose fills use **noise** need no
bake: the fill gets its own noise pattern.

**Several materials on one mesh** become texture sets, each with its own
layer stack. The **Texture Sets** panel picks the active one, and the
Layers panel (headed with the set's name), undo and History follow it.
Saving writes every set.

**Undo** is `Ctrl+Z` and **redo** `Ctrl+Shift+Z`. The **History** panel
lists every layer step, and clicking an entry travels back to it.
Strokes have their own undo in the same panel.

Strokes and layer edits share one timeline: `Ctrl+Z` steps back through
both, newest first, and clicking a History row moves both to that point.

## 4. Save

**File ▸ Save Project…** writes a `.umber` project directory: a
diffable `project.json` plus a `paint/` folder with every layer's pixels
as PNGs. The save is crash-safe, so an interrupted save leaves the
previous version intact. **File ▸ Open Recent** lists your last ten
projects, and **File ▸ Revert to Saved** discards unsaved edits.

Layer edits are journaled as you work: after a crash, reopening the
project replays them.

`Ctrl+S` asks for a folder the first time and then re-saves there
silently; `Ctrl+Shift+S` (Save As) always asks.

Unsaved work is snapshotted every two minutes; after a crash, the next
launch asks *Recover unsaved work?*. Closing
the window with unsaved edits asks Save / Discard / Cancel first.

## 5. Export

Open the **Export** panel. Pick a **Preset** for your engine (Unreal,
Unity, glTF, Blender) and a **Size**.

The preset decides how the maps pack into files. Unreal's ORM puts
occlusion, roughness and metallic in one texture, and the normal map is
written in that engine's convention (DirectX or OpenGL Y). Every channel
you painted exports. A channel you never touched exports a neutral value
(mid roughness, non-metal, flat normal), or the output that needs it is
skipped and named in the status line. The status line also says which
maps came from paint.

The dialog writes PNG (8/16-bit), EXR (32-bit float), TIFF or JPEG files, and can add a MaterialX
(`.mtlx`) or USD (`.usda`) material that references them.

## 6. The menus

- **File ▸ Save** asks for a folder the first time; **Save As** always
  asks. **Close Project** clears the session, after the same unsaved-work
  confirm as New.
- **Help ▸ Keyboard Shortcuts** lists every binding, and
  **Help ▸ Show Log** opens this session's log (kept from the previous
  session as `umber.prev.log` — the file to attach to a bug report).
- The menu row's workspace tabs (**Paint / Bake / Export / Graph**) swap
  the whole dock layout; Ctrl+PgDn cycles them. Your edits to each layout
  are remembered between launches.

## 7. Without the window

The same engine runs headless:

```bash
# bake every map from a mesh
cargo run -p umber-cli -- bake-all model.fbx out/ --size 2048 --rays 16 --dilate 16
# pack through an engine preset
cargo run -p umber-cli -- export model.fbx out/ --preset unreal
# drive the paint engine over JSONL, one step per line
printf '%s\n' '"version"' '"quit"' | cargo run -p umber-cli -- agent
```

## Plugins

Graph nodes can come from sandboxed WebAssembly plugins. A plugin in
your plugins folder does not run until you trust it in the Graph panel.
See [the plugin authoring guide](../plugins/authoring-guide.md).
