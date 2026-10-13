# Dynamic Material Layering — Design

> **Revision history.**
> - r1 (2026-10-13): original draft. Adversarial review verdict: NOT
>   READY (4 BLOCKERs, 8 MAJORs, 5 MINORs, 2 NITs). Full findings with
>   file:line evidence and fixes:
>   `docs/loop/artifacts/2026-10-13-dynmat-review.json` (review findings
>   #1–#19, cited below as "review #N").
> - r2 (2026-10-13): every finding is folded into the normative sections
>   below (applied, not appendixed). The review chose option (b) for
>   BLOCKER #1: v1 uses the fill precedent's destructive publish, not a
>   per-layer stack walk. Cut items are listed in §6 "Cut from v1".
>   Nothing below is contradicted by an addendum; this revision is the
>   whole contract.

Date: 2026-10-13 (wave-3 kill 3, doc-only kill per BINDING ruling 5 #3:
"dynamic material layering (design-doc first, likely 2 kills)").
Status: READY TO IMPLEMENT (r2; the two implementation kills derive from
this doc).
Requirement: `docs/specs/requirements.md:46` —
"Dynamic material layering (shader-declared sub-stacks) | P2 | 6+".

## The problem, precisely

The layer stack today is flat per channel: `LayerStack` is an ordered
`Vec<Layer>` (`crates/umber-core/src/layers.rs`), each layer carrying one
`blend_mode` + one `opacity` applied uniformly wherever the layer writes.
A fill layer (`LayerKind::Fill` + `Layer::fill: Option<FillParams>`,
`crates/umber-app/src/fill_layer.rs`) evaluates ONE RGBA result and
composites it with the layer's single blend/opacity pair
(`composite_fill_over`). There is no way to say "this layer writes
baseColor AND roughness AND normal, each with its own blend" — yet
that is exactly Painter's dynamic-material contract:

- Painter's layers are **multi-channel**; a fill layer driven by a
  material writes several channels at once with per-channel enable
  toggles (`docs/research/01-painter-feature-inventory.md:33,53`).
- **Dynamic Material Layering** proper: shader-declared sub-stacks
  (e.g. `pbr-material-layering`: 4 materials, 3 masks); the CHANNELS
  ARE DEFINED IN THE SHADER via `//: materials` metadata and
  `param auto Material.channel` bindings; export carries
  "Export shaders parameters" JSON describing stacks/materials/params
  (`01-painter-feature-inventory.md:81`).
- The Texture Set List shows **sub-stacks** for the material-layering
  workflow (`01-painter-feature-inventory.md:79`).

So the substance of requirements.md:46 that v1 delivers: a MATERIAL is
one layer that writes several channels at once, the channel set is
DECLARED by the material's graph, and each declared channel carries
its own blend mode, opacity and enable. The user never re-wires a
weathering material per channel. This is **per-channel blending**: one
graph output per channel, each composited once. Painter's multi-entry
sub-stacks (several materials stacked inside one channel) are the named
follow-up (§6), not v1. The doc says "per-channel blending" for what
v1 builds and keeps "sub-stack" only when quoting Painter.

### What umber already has (verified anchors, read live 2026-10-13)

- **Layer model** (`umber-core/src/layers.rs`): `LayerKind`
  (Paint/Fill/Folder/Instance), `Layer { id, name, kind, opacity,
  visible, blend_mode, mask, fill }`, `BlendMode` (full 32: separable
  `blend_channel`, triplet `blend_rgb`, `is_normal_map` routing gate),
  `LayerStack` (ordered, undoable via `LayerCommand`), `LayerMask`
  (metadata here, planes in the app pool). `LayerKind::Fill` is a
  PLAIN TAG with its payload on `Layer::fill` (`layers.rs:22-26`: "so
  the journal/project matches over `LayerKind` never churn").
- **Channels** (`umber-core/src/lib.rs:40-104`): `Channel { name, kind:
  Color|Data }`, `TextureSet::new_default` (baseColor/roughness/
  metallic/normal/height). Channel names are `String`s validated at
  resolve time, never an enum (the `smart.rs` judgment call).
- **Per-channel storage + sizing** (`umber-app/src/paint_state.rs`):
  tile-major `targets: BTreeMap<u16, TileTargets>` (`:531`), where
  `TileTargets { base_color, channels }` (`:465`) holds ONE FLATTENED
  target per tile and channel, never per layer. Lazy materialization,
  `active_channel` routing, `channel_sizes` overrides
  (`size_of_channel`, `:865`), `size_of_set` (`:853`), read-only
  `channel_target` binding seam for the viewport. The read/write
  seams a material uses are `read_channel_tile(tile, channel) ->
  Option<RgbaBuffer>` (`:2625`, `None` on a never-routed channel, no
  side-effect creation) and `publish_fill_channel(tile, channel, w,
  h, rgba) -> bool` (`:1625`, materializes the channel, drops a
  wrong-length upload loudly). Layer-mask planes stay SET-sized:
  "neither follows a channel resize" (`:161-164`).
- **No per-layer stack walk in the app.** Nothing in umber-app calls
  umber-gpu's `StackCompositor`/`composite_stack_cpu`; the compositor
  is a test oracle and GPU path that the app's pool does not feed
  (review #1). Every "layer composite" the app performs today is the
  fill precedent's read-modify-publish on the flattened tile.
- **The per-pixel composite primitive**
  (`umber-gpu/src/compositor.rs:693`): `composite_pixel_masked(src,
  dst, mode, opacity, mask) -> Option<[f32; 4]>`: blend via
  `blend_rgb` for triplet modes else `blend_channel` per RGB lane,
  coverage `cov = src_a · clamped_opacity(opacity) · clamp01(mask)`,
  `out_rgb = base + (blend − base)·cov`, straight-alpha Porter-Duff
  `out_a = cov + base_a·(1 − cov)`; `None` for `Passthrough` (and when
  the blend itself returns `None`). Quantization convention:
  `(v * 255.0).round()` (`compositor.rs:645`).
- **Graph engine** (`umber-graph`, `docs/specs/node-graph-design.md`):
  DAG + Kahn order, typed `NodeOutput` (Uniform/Image; `ImageBuffer`
  is CPU RGBA8, `value.rs:9`), `eval_graph_cached` + `EvalCache` +
  dirty propagation (`eval.rs:547-612`; returns `Err` on the FIRST
  node failure and discards outputs — whole-graph failure),
  `.mtlx` round-trip with DECLARED custom nodedefs (`anchor`,
  `mesh_map_generator`, …, `mtlx.rs:~100-145`) and no false
  interchange claim. `Node { id: u64, node_def, params, canvas }`
  has NO name field; mtlx writes node names as `N{id}`
  (`mtlx.rs:18,318`). mtlx `string` inputs parse to
  `ParamValue::Asset` (`mtlx.rs:744`). The crate has no serde.
  App bridge: `umber-app/src/graph_panel.rs` (registry, dirty set,
  cached eval, canvas-pos carry, and a GPU tile evaluator at
  `:556,618`).
- **Fill-layer live-eval precedent** (`umber-app/src/fill_layer.rs`):
  `FillLayerState` (dirty flag + per-tile cache + `sync_from_layer`
  adoption of document-side edits incl. undo/redo),
  `FillLayerStates::ensure` (a fresh state or changed params pushes
  the layer onto `dirty`, `:843-863`), `refresh_fill_layer`
  (evaluate → symmetry → quick-mask → `read_tile` →
  `composite_fill_over` → `publish_fill`, `:383-430`), driven from
  `main.rs:~2660-2700` with a literal `1024` size. Material eval
  follows this DATA FLOW but NOT its blend call:
  `composite_fill_over` does `blend_channel(s, d).unwrap_or(s)`
  (`:363`), which silently degrades triplet and normal-map modes to
  Normal (review #17). Materials call `composite_pixel_masked`.
- **Smart materials** (`umber-core/src/smart.rs`): `.umbersmart`
  folder presets (template instantiation, closed `SmartLayerKind`
  enum — unknown kinds fail LOUDLY), anchors (`AnchorRef`:
  typed `(layer, channel)` with resolve-time validation, mirrored in
  umber-core so core never depends on umber-graph), smart masks
  (ordered effect stacks). Smart materials are INSTANTIATION
  templates; dynamic materials are LIVE graph-declared layers.
  Related, deliberately distinct (see §6).
- **Project carry** (`umber-core/src/project.rs`): `ProjectModel` /
  on-disk `ProjectFile` already embed node graphs as opaque mtlx
  strings (`graphs_mtlx`, `:208` / `:260-263`, `serde(default)`);
  per-layer JSON lives in `layers/<set>/<id>.json` via `Layer`'s
  serde (`:504-505`); `LayerSetOrder` (`:271-279`) carries only ids.
  The version probe REJECTS any version `!= CURRENT_PROJECT_VERSION`
  (`:540`).
- **Doc form precedent**: `per-channel-tile-pool-design.md`
  (problem → non-goals → storage → routing → save/load → binding seam
  → undo → compositor → test gates → rollout) and
  `pwys-stroke-buffer-design.md` (options A/B with a chosen v1 +
  named follow-ups). This doc follows that structure.

## Non-goals (v1) — the explicit boundaries

1. **No new pixel store, and no per-layer storage.** Material outputs
   publish destructively into the existing flattened per-channel tile
   pool (`TileTargets`), at each channel's resolved size — exactly
   like fills (§2). Per-layer pool storage (which would make a real
   bottom-to-top walk, order preservation and non-destructive
   re-eval possible) is the named follow-up that gates the v2 of this
   row; it is NOT smuggled in here.
2. **No custom-WGSL material authorship in v1.** Painter authors
   dynamic materials as GLSL shaders with `//: materials` metadata.
   Umber's v1 material authoring surface is the NODE GRAPH only
   (custom WGSL user shaders are their own P2 row in requirements
   §5 — a separate kill).
3. **No material instancing/inheritance.** Each material layer holds
   its own `MaterialParams`. Definitions are embedded COPIES (§3):
   editing the source graph in the Graph panel after "save as
   material" does not propagate. "Edit the preset everywhere" is a
   named follow-up. `LayerKind::Instance` of a material layer renders
   NOTHING in v1: instances pass `None` pixels to the compositor
   (`compositor.rs:526-527`) and there is no per-layer result to
   reference (review #1).
4. **No cross-set materials.** A material layer belongs to ONE
   texture set and may only write channels that set names.
5. **No nested materials, no multi-entry sub-stacks.** A material
   declares at most one `material_output` per channel (§1.2); a
   material never contains another material.
6. **No per-channel masks inside the material in v1.** The material
   layer carries the standard single `LayerMask` (resampled per
   written channel, §2). Per-channel enable toggles ARE v1 — an enable
   is not a mask — but per-channel mask planes are a follow-up.
7. **No `.sbsar`/Adobe execution.** The hard wall stands: materials
   are umber graphs, never Substance archives.

## 1. The material model

### 1.1 A new layer kind — a PLAIN TAG

```rust
pub enum LayerKind {
    Paint,
    Fill,
    Folder { passthrough: bool },
    Instance { source_id: u64 },
    /// A live graph-declared material: writes one or more channels
    /// from `Layer::material` (Some exactly for Material layers — the
    /// same `#[serde(default)]` + inert-on-wrong-kind convention as
    /// `Layer::fill`). A plain tag: the payload never rides the kind.
    Material,
}

// umber-core (serde), on `Layer`:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub material: Option<MaterialParams>,

pub struct MaterialParams {
    /// Key into the project's embedded material definitions (§3),
    /// e.g. "umber.weathered-steel".
    pub material_id: String,
    /// Per-channel controls, one per channel the definition declares.
    pub channels: Vec<MaterialChannel>,
    /// Graph-param overrides on the embedded definition.
    pub params: Vec<MaterialParamOverride>,
}

pub struct MaterialChannel {
    pub name: String,        // a channel name of the layer's set
    pub blend: BlendMode,    // default Normal
    pub opacity: f32,        // default 1.0
    pub enabled: bool,       // default true
}

pub struct MaterialParamOverride {
    pub node_id: u64,        // umber_graph::Node::id
    pub param: String,       // the node param name
    pub value: MaterialParamValue,
}

/// umber-core serde MIRROR of the overridable subset of
/// umber_graph::ParamValue (umber-graph has no serde, and umber-core
/// does not depend on umber-graph — the AnchorRef mirror precedent).
pub enum MaterialParamValue {
    Float(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Color([f32; 3]),
    Int(i32),
    Bool(bool),
}
```

JUDGMENT CALL — a new `LayerKind::Material` variant rather than
growing `FillParams`: a fill layer evaluates one RGBA result for one
channel path; a material evaluates N per-channel results, each with its
own blend. Bolting N-channel semantics onto the fill pipeline would
fork every branch of `refresh_fill_layer`. The variant keeps the fill
path untouched and gives the app one clean dispatch arm.

JUDGMENT CALL — the variant is a PLAIN TAG (review #9). The
`layers.rs:22-26` rule ("Fill stays a plain tag … so the
journal/project matches over `LayerKind` never churn") applies, no
command mutates `kind` (so a `material_id` on the variant could never
be re-pointed), and binding a `String` field in the by-value `match
layer.kind` at `document.rs:284` is the E0507 trap. `material_id`
lives in `MaterialParams`; re-pointing a layer at another definition
is an ordinary `LayerCommand::SetMaterial`.

JUDGMENT CALL — the per-channel controls live ONLY in
`MaterialParams` (review #16): not in the graph, not as mtlx nodedef
inputs, not in the definition. One source of truth. When a layer's
`channels` list disagrees with the definition's declared set (a
definition re-saved with a channel added/removed), the definition
wins: declared channels missing from the list use the defaults
(Normal/1.0/enabled); listed channels the definition no longer
declares are ignored and the status line names them once.

JUDGMENT CALL — `MaterialParamValue` mirrors only the six numeric
variants (review #10). `NodeRef`, `Asset` and `Anchor` are graph
STRUCTURE, edited in the definition, not per-layer overrides. The
app bridge (Kill A's `material_layer.rs`, §6) converts the mirror to
`ParamValue` and applies each override onto its copy of the
definition graph before eval; an override naming a missing node or
param is an eval error (whole-material no-op, §2).

`SetMaterial` is one new `LayerCommand` variant (old/new
`Option<MaterialParams>`), undoable and journaled through the
existing machinery (`JournalOp::SetMaterial` in `umber-core`
`journal.rs`).

`LayerStack::check_paintable` (signature unchanged:
`Result<(), InstanceError>`) gains a `LayerKind::Material` arm
returning a new `InstanceError::PaintOnMaterial { layer: u64 }`
(same shape as `PaintOnInstance`; no field named `source`, the
thiserror trap): materials carry no paint pixels, only published
outputs. NOTE (review #11): `check_paintable` is called today only for
a label at `document.rs:289` and in tests; no stroke path calls it,
and its `_ => Ok(())` arm (`layers.rs:1182`) means the compiler won't
flag the new variant. Kill A adds the arm + a unit test; Kill B wires
`check_paintable` into stroke begin (`main.rs`) and owns the
stroke-level rejection test (§5.8).

A material layer with `material: None` (legacy/corrupt carry), or
whose `material_id` names no embedded definition, PUBLISHES NOTHING:
the refresh skips it and the canvas keeps whatever it already holds.
Never a crash, never a refused open (§5, never-crash doctrine).

### 1.2 The graph declares its channel set — the `material_output` nodedef

Painter's declaration mechanism is shader metadata
(`//: materials`, `param auto Material.channel`). Umber's v1 surface
for materials is the node graph, so the declaration is ONE custom
nodedef (review #5):

```
material_output { input: color3, channel: string }   // output: color3
```

- Added to umber-graph's declared custom nodedef list
  (`mtlx.rs:~100-145`, beside `anchor`, which already carries a
  `channel` string; the port is named `input`, the existing
  convention — e.g. `direction_warp`'s `input`), with a passthrough
  `NodeImpl` registered in `NodeRegistry`: its output is its `input`
  value UNCHANGED, `Image` or `Uniform` alike — the impl never errs on
  the variant, so a `Uniform` cannot fail the whole graph.
- Registration: Kill A adds `register_material_nodes` to umber-graph
  and a public `umber_graph::full_registry()` holding the seed + every
  `register_*` family `graph_panel.rs`'s private `full_registry`
  (`:103-112`) calls today + the material nodes. `graph_panel.rs`'s
  fn becomes a call to it, so the Graph panel and the material driver
  share one registry (no drift) and the panel can open a definition
  containing `material_output`.
- The `channel` param is read as `ParamValue::Asset` (mtlx `string`
  parses to `Asset`, `mtlx.rs:744`). It names a channel of the layer's
  texture set (`baseColor`, `roughness`, `normal`, …).
- The material's DECLARED channel set is exactly the set of
  `material_output` nodes present. No separate manifest: the graph
  IS the declaration, so definition and declaration can never drift.
- Build-time validation (typed errors, the `AnchorError::UnknownChannel`
  shape listing the set's channels): a `channel` the set does not
  name; two `material_output` nodes naming the same channel (v1 is one
  output per channel, non-goal 5); a non-`Asset` `channel` param. A
  definition failing any of these is a whole-material no-op with the
  badge.
- A `Uniform` arriving at a `material_output` (instead of an `Image`)
  is the ONE per-channel eval check: after an `Ok` eval,
  `eval_graph_cached` returns the FULL per-node output map
  (`Result<HashMap<u64, NodeOutput>, _>`, `eval.rs:547-555`), and the
  driver reads each `material_output` node's entry by id. A `Uniform`
  there makes that channel publish nothing; the other declared
  channels still publish; the error lands on the badge + status line.
  Every eval `Err` is whole-material (§2).

JUDGMENT CALL — a custom nodedef over the r1 "node named
`out:<channel>`" convention: `Node` has no name field, mtlx writes
names as `N{id}` so a name cannot survive round-trip, and an
`out:baseColor` `node_def` would be `UnknownNode` in the registry
(review #5). The nodedef is additive (no engine fork), serializes
through the existing `to_mtlx`/`from_mtlx` like `anchor`, and is
visible and editable in the Graph panel with zero new UI. Renaming a
set channel orphans the output; the build validation above names it
loudly, it never paints the wrong channel.

Channel-kind routing reuses the existing gates: `ChannelKind`
(Color/Data) decides LUT/display treatment at bind time (the
per-channel-pool rule); `BlendMode::is_normal_map` restricts the
normal-map modes to the `normal` channel. The Layers-panel blend combo
(Kill B) offers them only on `normal`; a deserialized normal-map mode
on another channel, or `Passthrough` on any channel, makes THAT
channel publish nothing with the badge (a per-channel config check,
like the `Uniform` case — `composite_pixel_masked` returns `None` for
`Passthrough`, and the material path never falls back to Normal).

### 1.3 Per-channel blend composition

ONE composite per declared, enabled channel, no inner/outer levels
(review #3). For each texel of each written channel:

```
out = composite_pixel_masked(
    src  = material_output image texel (resampled, §2),
    dst  = the channel's current flattened tile texel,
    mode = channel.blend,
    opacity = layer.opacity × channel.opacity,
    mask = layer-mask texel (resampled nearest-neighbour, §2; 1.0 if none),
)
```

with u8 → f32 as `byte / 255` and f32 → u8 as `(v * 255.0).round()`
(the compositor's quantizer, `compositor.rs:645` — NOT the fill path's
`+0.5 floor`; identical for in-range values, named so the goldens and
the code agree). The opacity product is formed first and
`composite_pixel_masked` clamps it once (`clamped_opacity`: non-finite
→ 0.0, then clamp01), so a non-finite factor zeroes coverage, matching
the compositor exactly.

- Disabled channels, channels the definition does not declare, and
  invisible layers (`layer.visible == false`) publish NOTHING: their
  tiles are not read, not composited, not written.
- A channel never routed on a tile (`read_channel_tile` → `None`)
  composites over transparent `[0, 0, 0, 0]`; `publish_fill_channel`
  materializes it.
- **The layer's own `blend_mode` is INERT on Material layers in v1.**
  It is carried (serde, undo) but never read by the material path; the
  Layers panel does not show the blend combo on a material row
  (Kill B). Layer `opacity` IS live (it multiplies every channel's
  opacity) and the layer mask IS live (gates every written channel).

JUDGMENT CALL — single composite over the r1 inner-then-outer order:
inner-then-outer feeds `composite_pixel_masked` a `src` that already
contains `dst`, so "below" is applied twice (review #3's worked
example: below 0.5, material 0.8, inner Normal @ 0.5, outer Multiply
@ 1.0 gives 0.325 where an isolated group gives 0.45; inner+outer
Multiply is src·below²). One composite per channel is also Painter's
per-channel blend model. The outer blend mode is cut (§6).

## 2. Eval order, dirt, and cache

**v1's truth: the FILL PRECEDENT's destructive publish** (review #1,
option b). There is no running composite of "the layers below" in the
app; the pool holds one flattened target per tile and channel. A
material refresh, per dirty material layer, per present tile
(`paint.tiles_present()`), per declared+enabled channel:

1. `read_channel_tile(tile, channel)` — the CURRENT flattened texels
   (every layer, above and below, plus this material's own previous
   output).
2. Composite the material's resampled output over it (§1.3).
3. `publish_fill_channel(tile, channel, w, h, rgba)`.

The consequences, stated as v1 boundaries (each one is fixed only by
per-layer pool storage, the named follow-up):

- **Stack order is not preserved.** A material composites over
  whatever the tile holds when it refreshes, including paint and
  layers ABOVE it. Reordering a material layer changes nothing until
  it re-publishes. When several material layers are dirty in one
  frame, they refresh in stack order bottom-to-top (pinned, §5.2).
- **Re-eval compounds.** Every re-publish composites again over the
  previous result. Normal at full coverage is idempotent; Normal at
  partial coverage converges toward the material; every other mode
  compounds (Multiply darkens each time — pinned, §5.2).
- **Nothing un-publishes.** Hiding the layer, disabling a channel,
  deleting the layer, or a failed re-eval stops FUTURE publishes; the
  pixels already published stay. Undo of `SetMaterial` restores
  PARAMS, not pixels: the reverted params mark the layer dirty and it
  re-publishes over the current canvas. Pixel-exact undo of material
  edits is a follow-up.
- **Anchors read the flattened channel.** An `anchor` node inside a
  material resolves to the flattened tile channel — which includes
  layers above and the material's own last output (self-feedback on
  every re-eval). The r1 "anchors resolve downward only, no extra
  pass" claim is withdrawn; v1 documents the feedback and does not
  forbid it.

JUDGMENT CALL — option (b) over gating the row on per-layer storage
(review #1's option a): per-layer storage is a pool redesign touching
every paint, fill, save and binding path; (b) ships the user-visible
row on the seams fills already use, and every wart above is the SAME
wart fills have today, so v1 adds no new class of bug. The boundaries
are written into `material_layer.rs`'s module docs.

**Externals.** The driver takes the eval externals from its caller:
anchor externals (`anchor:{id}:{channel}`, `AnchorRef::key`) are built
by `material_layer.rs` from `read_channel_tile` for the tile being
refreshed; mesh-map externals (`mesh_map:<name>`) are passed in by the
caller (Kill B supplies the Graph panel's flat placeholders; Kill A's
tests pass explicit images).

**Where in the frame.** Beside the fill refresh in `main.rs`
(`~2660-2700`): collect `LayerKind::Material` layers, `ensure` a
`MaterialLayerState` per layer, drain the dirty list bottom-to-top,
skip layers deleted since queueing (the fill ghost-publish guard).
Kill B owns this wiring.

**What dirties.** `MaterialLayerStates::ensure(layer, &params)` marks a
layer dirty when its `MaterialParams` changed (panel edit, undo/redo
replay of `SetMaterial`, journal replay — everything flows through
`Document::run`), when its embedded definition changed, when an
auto-rebake bumps a mesh-map revision the graph reads, or when the
set/channel size changed (`resize_set`, channel-size override). Edits
to OTHER layers never dirty a material (anchors included — v1 does
not track anchor sources; an explicit "refresh material" action is
Kill B's).

JUDGMENT CALL — a state created by the PROJECT-OPEN path is seeded
CLEAN; only states created at runtime (layer add, `SetMaterial`) start
dirty. The saved sidecar PNGs already contain the published result, so
re-publishing on open would compound every non-Normal material on
every open. This deliberately departs from `FillLayerStates::ensure`,
which dirties every fresh state — the fill path compounds on open
today; that is a fill bug noted here, not fixed by this row.

**Eval resolution.** Evaluate at `PaintState::size_of_set(set)`, never
a literal (review #15 — the fill driver's `1024` literal is NOT the
precedent here). `TextureSet.resolution` (the bake resolution) is a
different quantity and is not used.

**Cache shape.** Per material layer: one `EvalCache` keyed per
`(material layer, tile)` (review #14). Cross-tile reuse (one cache
shared by every tile) is allowed ONLY when the definition graph has no
`anchor` and no `mesh_map_generator` nodes — those externals are per
tile (mesh maps are flat placeholders today, `graph_panel.rs:14-17`,
which hides the difference; v1 does not rely on that). Memory cost, the
documented v1 boundary: `ImageBuffer` is RGBA8, so each cached Image
node costs `w·h·4` bytes — **nodes × 64 MiB at 4096²** per cache, times
tiles when per-tile. `Layer::memory_bytes` counts metadata only (the
honesty contract); the cache is reported by the app state, the pool
owns the published tile bytes (no double count).

**The eval error is a WHOLE-material no-op** (review #6).
`eval_graph_cached` returns `Err` on the first failing node and
discards every output, so an `UnknownNode`, a bad param, a bad
override, or an anchor whose source layer was deleted fails EVERY
channel. On `Err`: nothing publishes (the last published result
stays — destructive publish), the layer's badge carries the error, the
status line names it once. There is no per-channel isolation except
the two per-channel CONFIG checks of §1.2 (a `Uniform` at a
`material_output`; a forbidden blend on that channel).

**Resampling (BUILT in Kill A — review #8, #12).** No resampler exists
in the workspace today (the only resampling-shaped fn is
`downsample_hole`, `synth.rs:322`; nothing in export resamples). Kill
A adds one deterministic module in `material_layer.rs`:

- **Image:** bilinear RGBA8, texel-centre aligned
  (`src = (dst + 0.5) · src_w / dst_w − 0.5`, edge-clamped), each lane
  `round`-quantized. Identity when sizes match (byte-exact, pinned).
  Applied when a channel's `size_of_channel(set, channel)` differs
  from `size_of_set(set)`.
- **Normal renormalize iff `channel.name == "normal"`** (the name,
  not a blend gate — a gate belongs to a blend mode, not a channel):
  the bilinear lerp runs on byte values as f32 and renormalization
  runs on that f32 result BEFORE quantizing (on quantized bytes no
  triple decodes to zero length, so the guard would be unreachable):
  decode RGB `v/255·2 − 1`, normalize, `length < 1e-6` → `(0, 0, 1)`
  (covers exact zero and f32 dust), re-encode `(n + 1)/2·255`, then
  `round`. Alpha untouched. Reachable guard case: bytes 127 and 128
  lerped at weight 0.5 give exactly 127.5 → decodes to exactly 0.
  The composite-time lerp that follows stays UNNORMALIZED, exactly as
  `composite_pixel_masked` does today (`compositor.rs:728-732`) —
  documented, not "fixed" here.
- **Masks:** the layer mask plane is SET-sized (`paint_state.rs:161-164`),
  so for every written channel whose size differs, the mask is
  resampled NEAREST-NEIGHBOUR to that channel's `size_of_channel`
  before compositing. Without this the CPU compositor fails loudly
  (`InvalidMaskData`, `compositor.rs:~752`) and the GPU path zero-fills
  (`compositor.rs:73-76`) — the channel-override collision (review #12).

JUDGMENT CALL — graph evaluates ONCE per material per refresh at set
size and is resampled per channel, rather than once per channel size:
eval dominates cost, and outputs are resolution-agnostic patterns far
more often than texel-exact maps. Texel-exact per-size eval is the
named follow-up if goldens show drift on data channels.

## 3. Persistence

**`.umber` carry (additive rule, NO version bump — review #7).**
`Layer` gains `#[serde(default, skip_serializing_if =
"Option::is_none")] pub material: Option<MaterialParams>`, and
`LayerKind` gains the plain `Material` variant. Both ride `Layer`'s
serde in `layers/<set>/<id>.json` (`project.rs:504-505`), like `fill`.
`LayerSetOrder` carries only ids and is unchanged.

- The project version STAYS 1. No bump: `project.rs:540` rejects any
  `version != CURRENT_PROJECT_VERSION`, so bumping to 2 would make the
  new build refuse every existing project; serde does not need it (no
  `deny_unknown_fields`; the change is a defaulted field + a new
  variant).
- Pre-material files (no `material` key, no such variant) are
  LOAD-IDENTICAL: same layers, kinds, ids, `material: None`
  (`from_parts` monotonicity untouched). `skip_serializing_if` keeps
  re-saved non-material layer JSON free of a `"material": null` key
  (review #19 — the `fill` precedent, `layers.rs:717`, has no
  `skip_serializing_if` and does write `null`; materials do not
  repeat it).
- **Definitions EMBED in the project** (review #13): a new
  `material_defs_mtlx: BTreeMap<String, String>` (`material_id` → the
  definition graph's mtlx string, opaque to umber-core) on
  `ProjectModel` and the on-disk `ProjectFile`, beside `graphs_mtlx`,
  `#[serde(default)]` — the `graphs_mtlx` precedent. Saving writes
  every definition a layer references; a project opened on another
  machine evaluates identically.
- A layer whose `material_id` is absent from `material_defs_mtlx`, or
  whose definition fails `from_mtlx`, still LOADS: the project opens,
  the layer survives, it publishes nothing, the badge names the
  error. Never a refused open, never silent wrong pixels.
- Backward compat (new file on an old build): fails loudly at parse
  (unknown variant) — the documented v1 boundary, same as every prior
  `LayerKind` addition.

JUDGMENT CALL — a keyed sibling map over appending to `graphs_mtlx`:
`graphs_mtlx` is the Graph panel's list (unkeyed, panel-owned);
mixing material definitions into it would make the panel load them as
editable graphs and lose the `material_id` key. A separate map keeps
the panel's carry untouched.

**`.mtlx` angle (custom nodedefs, no interchange overclaim).** A
definition serializes through the existing `to_mtlx`/`from_mtlx`;
`material_output` is a declared custom nodedef (§1.2), legible to
consuming DCCs, evaluable only where implemented. The per-channel
blend/opacity/enable controls are NOT in the mtlx (they live only in
`MaterialParams`, §1.1). Standard nodes interoperate for real; no
"free interchange" claim anywhere in the UI or docs.

## 4. UI surface

**V1: expandable rows in the Layers panel** (not a separate panel).
Studied presentation (`01-painter-feature-inventory.md`):

- Painter shows sub-stacks in the Texture Set List (:79) while the
  layer itself stays ONE row with per-channel blending via the channel
  dropdown (:53). Umber follows: the material layer is one row (name,
  visibility, layer opacity, mask affordance — existing widgets; NO
  layer blend combo, §1.3), with a disclosure triangle expanding to
  one sub-row per DECLARED channel: channel name + enable checkbox +
  blend combo (normal-map modes only on `normal`, never
  `Passthrough`) + opacity slider.
- Sub-row edits go through `Document::run`
  (`LayerCommand::SetMaterial`) — undoable, journaled, one op per
  widget change (the fill-section live-edit convention).
- Undeclared channels are NOT shown; disabled channels render dimmed.
- Material PARAMETERS are edited in the Graph panel on the definition
  (v1 has no per-layer param editor; overrides in
  `MaterialParams.params` are written by Kill A's tests and carried,
  and a per-layer editor is cut, §6).
- The Graph panel gains "Save as material" (Kill B): COPIES the open
  graph into `material_defs_mtlx` under a new `material_id`. Later
  Graph-panel edits to the source graph do not propagate (non-goal 3).
- Malformed material (missing definition, eval error, bad
  `material_output`): the row renders an honest error badge and keeps
  every control operable (re-point `material_id` via `SetMaterial`,
  reset channels) — the never-crash doctrine at the UI layer.

JUDGMENT CALL — expandable rows over a separate Materials panel over
Painter's channel dropdown: a separate panel loses stack context
(reorder/drag targets live in Layers); the dropdown hides multi-channel
state behind one-at-a-time views, wrong for a material whose point is
seeing all its channels at once. No new shortcuts in v1; GUI phase 0b
is unaffected.

All user-facing strings go through `tr()` with `en-US.ftl` +
`pt-BR.ftl` entries. Kill A adds the two keys its exhaustive-match
arms need (`layers.kind-material`, `history.op-set-material`); Kill B
adds the panel strings.

## 5. Test plan

Inline `#[cfg(test)]` beside the code (house norm). Every test pins
EXACT values from the arithmetic; range checks only for
implementation-defined behavior with a reason comment (house rule).
All goldens below: opaque src and dst (alpha 255 → 255), mask 1.0,
quantizer `(v * 255.0).round()`; values were hand-derived and each
lands ≥ 0.1 from a rounding boundary (f32 error is ~1e-5, so the
pinned bytes are not implementation-defined).

1. **Channel-declaration golden:** a definition with two
   `material_output` nodes (`baseColor` solid red, `roughness` solid
   gray 128) over a known base, Normal, layer 1.0 × channel 1.0:
   baseColor and roughness publish byte-exact src values; `metallic`
   (undeclared) is byte-identical before/after and never read
   (`read_channel_tile` not called — a never-routed metallic stays
   unmaterialized).
2. **Per-channel composition goldens** (one channel, dst byte 200,
   src byte 100, i.e. s = 0.392157, 1 − s = 0.607843):
   - **2a — the opacity product.** Multiply, layer opacity 0.5 ×
     channel opacity 0.5 → cov 0.25:
     `out = 200·(1 − 0.25·0.607843) = 169.608` → **170**. Discriminates:
     the r1 two-level order (Multiply at 0.5 then again at 0.5) gives
     `200·0.696078 = 139.216 → 139`, then `139·0.696078 = 96.755` →
     **97**; using only one 0.5 gives **139**. The test asserts 170 and
     asserts ≠ 97 and ≠ 139 (commented as the wrong-order sentinels).
   - **2b — refresh order (ordering-sensitive).** Two material layers
     on the same channel over dst 200: M1 below (Normal, src 204, layer
     1.0 × channel 0.5), M2 above (Multiply, src 100, 0.5 × 0.5). Stack
     order M1 → M2: `200 + (204 − 200)·0.5 = 202.0` → 202, then
     `202·0.848039 = 171.304` → **171**. Reversed M2 → M1: 170 (2a),
     then `170 + (204 − 170)·0.5 = 187.0` → **187**. The driver test
     asserts the bottom-to-top refresh yields 171 (and the reversed
     call order 187, proving order matters under destructive publish).
   - **2c — re-eval compounds (the documented boundary).** Refreshing
     2a's layer a second time over the published 170:
     `170·0.848039 = 144.167` → **144**. Pins the v1 boundary so a
     future non-destructive fix flips this test on purpose.
3. **Triplet routing:** a material with `material_output` on `normal`
   and a normal-map blend composites via `blend_rgb` (pinned bytes
   hand-derived from `blend_rgb`'s formula — NOT assumed identity:
   byte 128 decodes to +0.0039, not exactly flat);
   a normal-map mode on `roughness` and `Passthrough` on any channel
   each make that channel publish nothing while a sibling channel
   still publishes.
4. **Dirty/cache:** an override edit re-runs only dirty nodes +
   downstream (the engine's run-counter pattern); an edit to another
   layer does not dirty the material; a project-open-seeded state is
   clean (no publish on open); a runtime-added state is dirty; a size
   change invalidates the caches; a graph with an `anchor` node gets a
   per-tile cache, one without anchors/mesh-map nodes shares one.
5. **Migration:** a pre-material `.umber` loads LOAD-IDENTICAL (same
   layers/kinds/ids, `material: None`), re-saves without a
   `"material"` key on any layer, and the version stays 1; a project
   with a material layer + embedded definition round-trips
   (save → load → equal `MaterialParams` and definition string).
6. **Never-crash doctrine — whole-material no-op.** Each case: the
   project loads, the layer survives, the refresh publishes NOTHING
   for ANY channel (every channel's tile bytes are byte-identical to
   before the failed refresh — including a previously published good
   result, which stays), the typed error is reported, nothing panics.
   Cases (one test each): truncated/unparseable definition mtlx;
   `material_id` absent from `material_defs_mtlx`; unknown
   `node_def` in the definition; `material_output.channel` naming a
   channel the set lacks; two `material_output`s on one channel; an
   override naming a missing node; `material: None` on a Material
   layer. Per-channel exception (one test): a `Uniform` wired to one
   `material_output` → that channel unchanged, the sibling channel
   publishes its golden bytes.
7. **Resampler goldens** (replaces r1's export-JSON round-trip, cut):
   identity size is byte-exact; a 2×2 → 4×4 bilinear upsample pins
   every output byte by hand; a `normal` channel's resampled texels
   renormalize (a pinned texel), and the reachable guard case — a 2×1
   `normal` row of (127,127,127) and (128,128,128) upsampled to 3×1,
   whose middle texel sits at source x = 0.5 under the texel-centre
   formula and decodes to exactly (0,0,0) — yields (128,128,255) for
   that texel (`(0,0,1)` re-encoded under `round`: 127.5 → 128); the
   same input on a non-`normal` channel does NOT renormalize (middle
   texel 127.5 → 128 per lane, blue 128); a set-sized mask
   nearest-neighbour-resampled to an overridden roughness channel
   composites without `InvalidMaskData` and with pinned bytes.
8. **Paint rejection:** Kill A — `check_paintable` on a material layer
   returns the typed error (unit). Kill B — a stroke begun on a
   material layer is rejected at stroke begin (no pixels change).

## 6. Effort + kill split (the "likely 2 kills" cut line)

Total ≈ the 6+ estimate in requirements.md:46 (P2). Kill A is NOT
headless (review #2): the new `LayerKind` and `LayerCommand` variants
break exhaustive matches in umber-app, and CI builds the whole
workspace.

**Kill A — model + eval + persistence (no frame wiring, no panels).**
File surface (register all with `kill-locks.sh`):

- `umber-core`: `layers.rs` (`LayerKind::Material`, `MaterialParams`
  / `MaterialChannel` / `MaterialParamOverride` / `MaterialParamValue`,
  `Layer::material`, `LayerCommand::SetMaterial`, `check_paintable`
  arm + test, `memory_bytes` honesty), `journal.rs`
  (`JournalOp::SetMaterial` + its exhaustive matches, `:48`,
  `:146-213`, `:231-279`), `project.rs` (`material_defs_mtlx` carry,
  version unchanged, migration test). (`undo.rs` holds no
  `LayerCommand`/`LayerKind` match — not in the surface.)
- `umber-graph`: `mtlx.rs` (`material_output` declared nodedef), the
  passthrough `NodeImpl` + `register_material_nodes`, and the public
  `full_registry()` (§1.2) — in `nodes.rs`/`lib.rs`, whichever the
  kill finds holds the `register_*` families.
- `umber-app`: `document.rs` (`match layer.kind` arm,
  `tr("layers.kind-material")`, `:284-287`), `history_panel.rs`
  (`Add { kind }` arm, `HistoryKind::SetMaterial` + `label()` arm with
  `tr("history.op-set-material")`, `:154-194`), `graph_panel.rs`
  (`full_registry` delegates to `umber_graph::full_registry()`), NEW
  `material_layer.rs` (`MaterialParamValue → ParamValue` bridge,
  `material_output` discovery/validation, `MaterialLayerState(s)`,
  per-channel composite on `composite_pixel_masked`, the resampler),
  declared as `pub mod material_layer;` in `lib.rs` (the
  `paint_state`/`retouch` library-surface precedent: reachable and
  testable without the shell, no dead code before Kill B adds the
  `main.rs` `mod` line and wiring).
- `assets/i18n/en-US.ftl` + `pt-BR.ftl`: `layers.kind-material`,
  `history.op-set-material`.

Exhaustive-match census (r2, grep of `LayerKind::`/`LayerCommand::`/
`JournalOp::` over `crates/`): the exhaustive matches are in
`layers.rs`, `journal.rs`, `document.rs` and `history_panel.rs` only;
the other hits (`fill_layer.rs`, `main.rs`, `export_dialog.rs`,
`mesh_map_import.rs`, `perf_soak.rs`, `smart.rs`'s
`SmartLayerKind → LayerKind` map, `project.rs`/`compositor.rs`
tests, `umber-brush`/`umber-mesh`) are constructors or `==`
comparisons that a new variant does not break. The kill re-runs the
grep before locking (the tree moves).

Tests: §5.1–5.7 and §5.8's unit half. Crate homes: the composite and
resampler live in umber-app because `composite_pixel_masked` is in
umber-gpu and the bridge needs umber-graph, and umber-core depends on
neither (only umber-app depends on both). No new crate.

**Kill B — app wiring + UI.** `umber-app`: `main.rs` (`mod
material_layer;`, the refresh beside the fill refresh at `size_of_set`,
project-open seeding as clean, `check_paintable` at stroke begin),
Layers-panel expandable sub-rows, Graph-panel "Save as material",
error badges, ftl keys (en-US + pt-BR). Tests: §5.8's stroke half +
UI-logic tests (headless where possible; egui drawing untested by
construction per `graph_panel.rs`). Depends on A; touches the live
publish path — dragon-authored or claw-with-the-fill-report-as-required-
reading (the per-channel-pool rollout precedent).

### Cut from v1

- **The outer (layer) blend mode** — single composite per channel is
  the correct model (review #3); the field stays carried but inert.
- **Export-shaders-parameters JSON + re-import** — no consumer exists
  in umber's export targets; Painter parity on this alone isn't worth
  a format.
- **The Properties panel "exposed params" section** — `Node` has no
  expose flag; it needs new graph-model work. Params are edited in the
  Graph panel on the definition.
- **The asset store as the definitions' home** — breaks project
  portability (review #13); definitions embed, and the asset store is
  at most a library to copy from (follow-up).
- **The mtlx per-channel blend inputs** — a second copy of
  `MaterialParams`' controls (review #16).
- **The word "sub-stack" for v1** — one `material_output` per channel
  is per-channel blending.

**Explicitly NOT in either kill (recorded follow-ups):** per-layer pool
storage (unlocks order preservation, non-destructive re-eval,
un-publish, Instance-of-material, downward-only anchors); real
multi-entry sub-stacks (several materials per channel); pixel-exact
undo of material edits; live re-parameterization DURING a stroke
(params apply between strokes); material instancing/inheritance +
"save layer as smart material" (smart presets stay instantiation
templates; teaching `SmartLayerKind` a `material` kind is the follow-
up — the closed enum stays closed); linked (non-copy) definitions;
an asset-store material library; cross-set and nested materials;
per-channel mask planes; custom-WGSL material authorship (own P2 row);
texel-exact per-size graph eval; the fill path's compound-on-open bug
(§2, a fill fix, not this row).

## Deviation log (doc kill vs brief)

- Brief listed "read `docs/claw-artifacts/<crate>/`" and "`umber-gpu`
  graph_eval WGSL arm" as anchors. Corrected in r2 (review #18): the
  graph GPU arm EXISTS — `graph_panel.rs:556,618` runs a GPU tile
  evaluator. Materials still evaluate through CPU `eval_graph_cached`
  in v1 (the dirty/cache API and the whole-graph error semantics are
  specified against it); routing material eval through the GPU tile
  evaluator is a follow-up, not a v1 contract.
- Brief's anchor "`document.rs` + `undo.rs`" for the layer-stack
  model: the model lives in `umber-core/src/layers.rs` (+ `project.rs`
  carry, `smart.rs` presets); `umber-app/src/document.rs` is the
  `Document::run` dispatch shell. Corrected in r2 (review #18):
  `LayerSetOrder` carries only ids; `fill` (and `material`) ride
  `Layer`'s serde in `layers/<set>/<id>.json`.
- Brief asked "how per-channel blend modes inside the material compose
  with the outer stack's blend": r1 decided inner-then-outer; r2
  replaces it with one composite per channel and an inert layer blend
  (§1.3, review #3).
- Brief's "shader DECLARES its channel set (an attribute on the
  graph?)": no attribute; the `material_output` nodes ARE the
  declaration (§1.2). r1's `out:<channel>` node-name convention is
  withdrawn (review #5).
