# Dynamic Material Layering — Design

Date: 2026-10-13 (wave-3 kill 3, doc-only kill per BINDING ruling 5 #3:
"dynamic material layering (design-doc first, likely 2 kills)").
Status: DRAFT (pre-review; the two implementation kills derive from this doc).
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
baseColor AND roughness AND normal, each with its own internal blend
logic" — yet that is exactly Painter's dynamic-material contract:

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

So the substance of requirements.md:46 against what umber already has:
a MATERIAL must own per-channel sub-stacks that the SHADER declares —
a weathering material writes baseColor+roughness+normal with internal
per-channel logic the user never re-wires per channel — while the outer
stack keeps treating the material as ONE layer with its own
blend/opacity/visibility/mask.

### What umber already has (verified anchors, read live 2026-10-13)

- **Layer model** (`umber-core/src/layers.rs`): `LayerKind`
  (Paint/Fill/Folder/Instance), `Layer { id, name, kind, opacity,
  visible, blend_mode, mask, fill }`, `BlendMode` (full 32: separable
  `blend_channel`, triplet `blend_rgb`, `is_normal_map` routing gate),
  `LayerStack` (ordered, undoable via `LayerCommand`), `LayerMask`
  (metadata here, planes in the app pool).
- **Channels** (`umber-core/src/lib.rs:40-104`): `Channel { name, kind:
  Color|Data }`, `TextureSet::new_default` (baseColor/roughness/
  metallic/normal/height). Channel names are `String`s validated at
  resolve time, never an enum (the `smart.rs` judgment call).
- **Per-channel storage + sizing** (`umber-app/src/paint_state.rs`):
  tile-major `TileTargets { base_color, channels }` with lazy
  materialization, `active_channel` routing, `channel_sizes`
  overrides, read-only `channel_target` binding seam for the viewport.
  A material's per-channel outputs land in exactly this pool — no new
  storage system.
- **Graph engine** (`umber-graph`, `docs/specs/node-graph-design.md`):
  DAG + Kahn order, typed `NodeOutput` (Uniform/Image, `value.rs`),
  `eval_graph_cached` + `EvalCache` + dirty propagation (`eval.rs`),
  `.mtlx` round-trip with DECLARED custom nodedefs and no false
  interchange claim, app bridge (`umber-app/src/graph_panel.rs`:
  registry, dirty set, cached eval, canvas-pos carry). A material
  sub-stack IS a per-channel graph evaluation on this engine —
  not a new parallel system.
- **Fill-layer live-eval precedent** (`umber-app/src/fill_layer.rs`):
  `FillLayerState` (dirty flag + per-tile cache + `sync_from_layer`
  adoption of document-side edits incl. undo/redo), `refresh_fill_layer`
  pipeline (evaluate → symmetry → quick-mask → publish →
  `composite_fill_over`). Material eval follows this shape.
- **Smart materials** (`umber-core/src/smart.rs`): `.umbersmart`
  folder presets (template instantiation, closed `SmartLayerKind`
  enum — unknown kinds fail LOUDLY), anchors (`AnchorRef`:
  typed `(layer, channel)` with resolve-time validation), smart masks
  (ordered effect stacks). Smart materials are INSTANTIATION
  templates; dynamic materials are LIVE shader-declared layers.
  Related, deliberately distinct (see §6).
- **Doc form precedent**: `per-channel-tile-pool-design.md`
  (problem → non-goals → storage → routing → save/load → binding seam
  → undo → compositor → test gates → rollout) and
  `pwys-stroke-buffer-design.md` (options A/B with a chosen v1 +
  named follow-ups). This doc follows that structure.

## Non-goals (v1) — the explicit boundaries

1. **No new pixel store.** Material outputs publish into the existing
   per-channel tile pool (`TileTargets`), at each channel's resolved
   size. Storage, lazy materialization, save/load of painted bytes,
   and the viewport binding seam are all reused untouched.
2. **No custom-WGSL material authorship in v1.** Painter authors
   dynamic materials as GLSL shaders with `//: materials` metadata.
   Umber's v1 material authoring surface is the NODE GRAPH only
   (custom WGSL user shaders are their own P2 row in requirements
   §5 — a separate kill, not smuggled in here).
3. **No material instancing/inheritance.** Each material layer is an
   independent copy of its parameters. Editing one never propagates
   to another; "edit the preset everywhere" is a named follow-up.
   (Layer instancing, `LayerKind::Instance`, already exists for
   whole layers and is unaffected.)
4. **No cross-set materials.** A material layer belongs to ONE
   texture set and may only write channels that set names. Same-set
   anchors (`AnchorRef`) may feed it; cross-set references are
   rejected at resolve time like any unknown channel.
5. **No nested materials.** A material's sub-stack contains plain
   per-channel graph outputs, never another material layer. One
   level only; a graph output wired from a material layer's channel
   via anchor reads its COMPOSITED result (see §3), it does not
   recurse into the material.
6. **No per-channel masks inside the material in v1.** The material
   layer carries the standard single `LayerMask` (outer gate, reused
   as-is). Per-channel enable toggles (Painter's Material-section
   switches) ARE v1 — an enable is not a mask — but per-channel mask
   planes are a follow-up.
7. **No `.sbsar`/Adobe execution.** The hard wall stands: materials
   are umber graphs, never Substance archives.

## 1. The material model

### 1.1 A new layer kind, not a fill-layer extension

```rust
pub enum LayerKind {
    Paint,
    Fill,
    Folder { passthrough: bool },
    Instance { source_id: u64 },
    /// A live shader-declared material: owns per-channel sub-stacks
    /// evaluated from `Layer::material` (Some exactly for Material
    /// layers — the same `#[serde(default)]` + inert-on-wrong-kind
    /// convention as `Layer::fill`).
    Material { material_id: String },
}
```

JUDGMENT CALL — a new `LayerKind::Material` variant rather than
growing `FillParams`: a fill layer evaluates one RGBA result with one
blend path (`composite_fill_over`); a material evaluates N
per-channel results with N inner blends plus an outer composite.
Bolting N-channel semantics onto the fill pipeline would fork every
branch of `refresh_fill_layer`. The variant keeps the fill path
byte-identical and gives the compositor one clean dispatch arm.
`material_id` (a stable string key, e.g. `"umber.weathered-steel"`)
names the material DEFINITION (the graph + channel contract, stored
in the asset store / preset dir); the layer's live PARAMETERS ride
`Layer::material: Option<MaterialParams>` beside it, so undo,
journal, and save/load flow through the existing `LayerCommand`
machinery with one new command variant (`SetMaterial`).

`LayerStack::check_paintable` rejects direct painting on material
layers with a typed error naming the layer (same shape as
`PaintOnInstance`): materials carry no paint pixels, only evaluated
outputs. A material layer with `material: None` (legacy/corrupt
carry) evaluates to nothing and composites as a no-op — never a
crash, never a hole (see §5, never-crash doctrine).

### 1.2 The shader declares its channel set — as a graph output contract

Painter's declaration mechanism is shader metadata
(`//: materials`, `param auto Material.channel`). Umber's shader
surface for materials in v1 is the node graph, so the declaration
is a GRAPH OUTPUT CONTRACT:

- A material definition designates, per channel it writes, one
  **channel-output node**: the graph node whose evaluated `Image`
  output feeds that stack channel. Convention: output nodes named
  `out:<channel>` (e.g. `out:baseColor`, `out:roughness`,
  `out:normal`) — a naming rule enforced at material-build time
  (unknown/unmatched names are a typed build error listing the
  set's channels, mirroring `AnchorError::UnknownChannel`).
- The material's DECLARED channel set is exactly the set of
  `out:<channel>` nodes present. No separate manifest: the graph IS
  the declaration, so definition and declaration can never drift.
- Each declared channel carries, in the material definition
  (NOT in the graph): inner `blend_mode` + inner `opacity` +
  `enabled` flag — the per-channel sub-stack controls. Defaults:
  `Normal` / `1.0` / enabled.
- Non-`Image` outputs on an `out:<channel>` node (a `Uniform`
  landing there) are a typed eval error for that channel only:
  that channel evaluates to transparent/no-op, the others still
  composite, and the error lands on the status line (the graph
  panel's honest-failure convention).

JUDGMENT CALL — output-nodes-named-`out:<channel>` over a separate
channel-manifest field: a manifest duplicates the graph and rots
(the smart-preset lesson: keep one source of truth); the graph
panel already lists nodes, so `out:*` nodes are visible, editable,
and `.mtlx`-serializable with zero new UI. The cost — renaming a
channel orphans the output node — is handled by the same
resolve-time validation anchors already use: unmatched `out:`
nodes warn loudly, they never silently paint the wrong channel.

Channel-kind routing reuses the existing gates: `ChannelKind`
(Color/Data) decides LUT/display treatment at bind time (the
per-channel-pool rule); `BlendMode::is_normal_map` restricts the
normal-map modes to the normal channel at the material editor
(the routing-time gate documented on `NormalDetail`), exactly as
the stack compositor will.

### 1.3 Inner vs outer blend composition

Two levels, both through the SAME primitives
(`blend_channel`/`blend_rgb`/`composite_channel`):

1. **Inner (per-channel, inside the material):** each declared
   channel's evaluated image blends over the channel's current
   stack content (layers BELOW the material layer) with the
   channel's own inner blend + inner opacity. Disabled channels
   are skipped (contribute nothing). This is the sub-stack:
   per-channel logic the user never re-wires per channel.
2. **Outer (the material as one stack layer):** the layer's own
   `blend_mode` + `opacity` + `visible` + `mask` then apply to the
   INNER result per written channel — i.e. the inner composite is
   treated as the layer's "source pixels" and composited with the
   standard layer step. Channels the material does NOT declare
   pass through byte-identical (the outer step skips them: a
   material contributes nothing where it declares nothing).

`Passthrough` on a material layer's outer blend degrades to
`Normal` (the `composite_fill_over` precedent — structural for
folders, never a hole). Opacity semantics (non-finite → 0.0,
clamp01) match `composite_channel` exactly so inner and outer
agree on degenerate input.

JUDGMENT CALL — inner-first, then outer (rather than folding the
outer into each inner op): keeps the material a well-behaved
single layer for every existing consumer (undo previews, flatten,
export, instances — an `Instance` of a material layer renders its
composited channels with zero new code), and matches Painter's
presentation where the material collapses to one row with one
opacity while expanding to per-channel control.

## 2. Eval order, dirt, and cache

Follows the fill-layer live-eval shape (`FillLayerState` +
`refresh_fill_layer`), extended to N channels:

**Where in the frame.** Bottom-to-top stack walk, per tile. Plain
layers composite as today. At a Material layer:

1. Ensure the material's graph cache is clean for this tile:
   re-run ONLY dirty nodes + downstream via `eval_graph_cached`
   (the engine's slice-4 API), at the tile's set resolution
   (the graph v1 single-resolution rule — see judgment below).
2. For each declared+enabled channel: take the `out:<channel>`
   image, resample to the channel's resolved size
   (`channel_sizes` override or set size), blend inner over the
   channel's running composite (inner blend+opacity).
3. Outer step (§1.3) per written channel; unwritten channels
   untouched.

**What dirties.** The material state (`MaterialLayerState`,
sibling to `FillLayerState`) tracks: its `MaterialParams`
revision, the graph dirty set (param edits, structural edits via
the graph panel's existing dirty marking), upstream anchor inputs
(anchors resolve to composited channels BELOW the layer —
evaluated before the material in the same walk, so no extra
pass), baked mesh-map inputs (auto-rebake bumps the revision),
and tile size (override edits / `resize_set` invalidate the
cache like `FillLayerState::evaluate`'s size check). ANY
document-side change (panel edit, undo/redo replay of
`SetMaterial`, journal replay) flows through `Document::run`
and marks dirty via `sync_from_layer`-style adoption — the
live-not-baked contract, same as fills.

**Cache shape.** Per material layer:
`HashMap<(tile, channel), CachedTile>` (RGBA bytes + size, the
`CachedTile` shape) PLUS the shared `EvalCache` for the graph
itself (node outputs reused across channels AND across tiles —
the graph is UV-space, tile-independent; per-tile cost is only
the resample + composite). Memory accounting extends
`Layer::memory_bytes`' honesty contract: metadata here, tile
bytes in the pool (the pool already owns them — no double
count).

JUDGMENT CALL — graph evaluates ONCE per material per revision
(at set resolution), resampled per channel, rather than once per
(channel size): graph eval dominates cost and outputs are
resolution-agnostic patterns far more often than texel-exact
maps; the resampler is the same one the export path uses (the
implementation kill names it — it must be deterministic and
pinned by a golden). Texel-exact-per-size eval is the named
follow-up if goldens show resample drift on data channels
(normal/height are the risk: resampling a normal map without
renormalization visibly breaks lighting — v1 renormalizes
decoded directions after resample on channels whose blend gate
is normal-map OR whose name is `normal`, documented in the
module docs).

JUDGMENT CALL — anchors inside materials resolve DOWNWARD only
(to composited channels of layers below the material in the same
set, same walk): upward/self references are resolve-time errors
(`AnchorError`-shaped), never an eval cycle. The instance-eval
DAG (`instance_eval_order`) is unaffected — instances see only
the material's composited result.

## 3. Persistence

**`.umber` carry (additive rule).** `Layer` gains
`#[serde(default)] pub material: Option<MaterialParams>` and the
`LayerKind::Material` variant; `LayerSetOrder` carries them like
`fill` today (`project.rs`). The rule, matching the fill-row and
tile-map precedents:

- Pre-material files (no `material` key, no such variant) load
  UNCHANGED — `None` fields, existing kinds, same ids
  (`from_parts` monotonicity untouched). A dedicated migration
  test pins byte-identical load (see §5).
- Material definitions live OUTSIDE the project (asset store /
  preset dir, content-addressed like `umber-core::assets`):
  the project stores `material_id` + `MaterialParams` (param
  overrides) + per-channel inner blend/opacity/enables. A project
  referencing a missing definition loads LOUDLY (typed error,
  layer evaluates to no-op) — never silent wrong pixels, never a
  refused open (the smart-preset closed-enum precedent: loud,
  not lossy — except here the layer SURVIVES as a no-op rather
  than failing the parse, because a project must always open).
- Backward compat (new file on old build): out of scope, fails
  loudly at parse (unknown variant) — the documented v1
  boundary, same as every prior `LayerKind` addition. The
  project version stays 1 in v1 (additive-only change); if the
  implementation kill finds serde needs it, bumping to 2 with
  the existing `version != CURRENT` probe is pre-approved here.

**`.mtlx` angle (§4's rule: custom nodedefs, no interchange
overclaim).** The material's graph serializes through the
existing `to_mtlx`/`from_mtlx` (custom nodes declared before
use; `out:<channel>` nodes as a declared `umber:` custom
nodedef family carrying the channel name as a string input —
legible to consuming DCCs, evaluable only where implemented).
The per-channel inner blend/opacity/enables ride as extra
inputs on those nodedefs. Standard nodes interoperate for real;
painter-specific nodes stay honestly custom. No "free
interchange" claim anywhere in the UI or docs.

**Export JSON (Painter parity).** Painter's "Export shaders
parameters" emits JSON describing stacks/materials/params
(`01-painter-feature-inventory.md:81`). V1's export path gains
the same: per material layer, `{ material_id, channels:
[{ name, blend, opacity, enabled }], params }` beside the
existing per-map export. Round-trips back through the material
editor (importable back — Painter parity is the acceptance
criterion, pinned by a JSON round-trip test).

## 4. UI surface

**V1: expandable rows in the Layers panel** (not a separate
panel). Studied presentation (`01-painter-feature-inventory.md`):

- Painter shows sub-stacks in the Texture Set List for the
  material-layering workflow (:79) while the layer itself stays
  ONE row with per-channel blending via the channel dropdown
  (:53). Umber follows: the material layer is one row (name,
  visibility, outer opacity/blend, mask affordance — all
  existing widgets), with a disclosure triangle expanding to
  one sub-row per DECLARED channel: channel name + enable
  checkbox + inner blend combo + inner opacity slider.
- The expanded sub-rows edit through `Document::run`
  (`LayerCommand::SetMaterial`) — undoable, journaled, one op
  per widget change (the fill-section live-edit convention).
- Undeclared channels are NOT shown (nothing to control);
  disabled channels render dimmed with their cached preview
  grayed (the solo-view placeholder convention).
- Material PARAMETERS (graph params: e.g. weathering amount,
  edge wear width) surface in the Properties panel's fill-style
  section: one collapsible per material layer reusing the graph
  panel's param-editor widgets (the `graph_panel.rs` "node list +
  param editors" precedent — no noodle canvas inside the layer
  row). Full graph editing stays in the Graph panel; the
  Properties section edits only exposed params.
- Malformed material (missing definition, bad JSON, orphaned
  `out:` nodes): the row renders an honest error badge + keeps
  every control operable (re-point `material_id`, reset params)
  — the never-crash doctrine at the UI layer.

JUDGMENT CALL — expandable rows over a separate Materials
panel over Painter's channel-dropdown: a separate panel loses
stack context (reorder/drag targets live in Layers); the
channel-dropdown hides multi-channel state behind one-at-a-time
views (fine for blending modes, wrong for a 4-channel material
whose whole point is seeing all sub-stacks at once). The
expandable row shows the full declared set in place. The 9
shortcuts / shell-chrome work (GUI phase 0b) is unaffected —
no new shortcuts in v1.

All user-facing strings go through `tr()` with `en-US.ftl` +
`pt-BR.ftl` entries (house rule — the implementation kills own
this, listed here so it isn't forgotten).

## 5. Test plan

Unit/integration tests live beside the code (inline
`#[cfg(test)]`, house norm). Every test pins EXACT values from
the arithmetic; range checks only for implementation-defined
behavior with a reason comment (house rule).

1. **Channel-declaration golden:** a 2-output test graph
   (`out:baseColor` solid red, `out:roughness` solid 0.5 gray)
   over a known base: baseColor and roughness composite to
   byte-exact values through Normal/1.0; an undeclared channel
   (metallic) is byte-identical before/after.
2. **Inner/outer composition golden:** inner Multiply @ 0.5 on
   one channel + outer Normal @ 1.0, hand-computed through
   `blend_channel` + Porter-Duff-over — asserts the two-level
   order (swap the levels and the bytes differ; the test would
   catch an implementation that folds outer into inner).
3. **Triplet routing:** a material declaring `out:normal` with
   an inner normal-map blend composites via `blend_rgb`
   (Whiteout/RNM identities: flat detail = base unchanged);
   selecting a normal-map mode for a non-normal channel is
   rejected by the editor-side gate.
4. **Dirty/cache:** param edit re-runs only dirty nodes +
   downstream (the engine's run-counter pattern); unrelated
   layer edits above/below do NOT re-evaluate the graph
   (cache-hit assertion); tile resize invalidates per-tile
   caches.
5. **Migration:** a pre-material `.umber` (no `material` key)
   loads with all layers paint/fill-identical and every
   material-less layer evaluating exactly as before
   (byte-identical composite golden).
6. **Never-crash doctrine on malformed material JSON:**
   truncated JSON, unknown `node_def`, `out:` node naming a
   channel the set doesn't have, `Uniform` wired to
   `out:<channel>`, missing definition file — EACH yields a
   typed error + no-op channel(s) + loadable project + usable
   UI row. One test per case (six tests); none may panic,
   none may poison other channels.
7. **Export JSON round-trip:** write → read → write
   byte-identical; re-import restores inner blends/params.
8. **Paint-rejection:** stroking a material layer returns the
   typed error (not a panic, not silent pixels).

## 6. Effort + kill split (the "likely 2 kills" cut line)

Total ≈ the 6+ estimate in requirements.md:46 (P2, second-highest
bracket). Cut along the headless/app seam — the repo's standard
seam (per-channel pool, fill layers, and every graph slice all
cut here):

**Kill A — model + CPU eval + persistence (headless, no UI).**
`umber-core`: `LayerKind::Material`, `MaterialParams` +
`MaterialDef` types, `LayerCommand::SetMaterial` (+ undo/journal
wiring), `check_paintable` arm, project carry + migration,
`memory_bytes` honesty. `umber-graph` (or a small new
`umber-material` module IF the kill finds the contract wants a
home — default is NO new crate: the output-contract is ~100
lines on the existing engine): `out:<channel>` discovery +
validation, per-channel `eval_graph_cached` driver,
`MaterialLayerState` (dirty/cache/resample-with-renormalize),
inner/outer composite fns on the shared blend primitives.
Tests §5.1–6,8 (goldens, doctrine, rejection). No egui, no GPU,
no ftl. Lands with the doc's test gates green in CI.

**Kill B — app wiring + UI + export.** `umber-app`:
`refresh_material_layer` pipeline in the frame walk (beside
`refresh_fill_layer`), Layers-panel expandable sub-rows +
Properties material-param section (with ftl keys en+pt-BR),
Graph-panel "expose as material" affordance (saving a graph as
a material definition into the asset store), export-dialog JSON
("Export shaders parameters" parity) + re-import, error badges.
Tests §5.7 + UI-logic tests (headless where possible; egui
drawing untested by construction per `graph_panel.rs`).
Depends on A; touches the live compositor path — dragon-authored
or claw-with-the-fill-report-as-required-reading (the
per-channel-pool rollout precedent).

**Explicitly NOT in either kill (recorded follow-ups):**
live re-parameterization DURING a stroke (params apply between
strokes; mid-stroke edits queue like the retouch drain);
material instancing/inheritance + "save layer as smart material"
(the smart↔dynamic bridge: smart presets stay instantiation
templates; teaching `SmartLayerKind` a `material` kind is the
named follow-up, NOT v1 — the closed enum stays closed);
cross-set materials; nested materials; per-channel mask planes;
custom-WGSL material authorship (own P2 row); texel-exact
per-size graph eval (see §2 resample judgment); 32-texture
channel-limit work (Painter's Windows ceiling — umber's pool is
lazy per routed channel, so no equivalent ceiling is expected;
revisit with perf numbers if one appears).

## Deviation log (doc kill vs brief)

- Brief listed "read `docs/claw-artifacts/<crate>/`" and
  "`umber-gpu` graph_eval WGSL arm" as anchors; neither exists
  on this tree (no `docs/claw-artifacts/` dir; `umber-gpu` holds
  renderer/paint passes/bakers/postfx per AGENTS.md, graph eval
  is CPU in `umber-graph` with a GPU arm still wave-6 per the
  node-graph design). Designed against the live tree instead:
  CPU `eval_graph_cached` as the eval arm, GPU material passes
  as a Kill-B/follow-up concern. No WGSL-side contract is
  specified here beyond "compositor calls the same CPU
  composite in v1".
- Brief's anchor "`document.rs` + `undo.rs`" for the layer-stack
  model: the model lives in `umber-core/src/layers.rs` (+
  `project.rs` carry, `smart.rs` presets); `umber-app/src/
  document.rs` is the `Document::run` dispatch shell. Read all
  four; the doc cites the real homes.
- Brief asked "how per-channel blend modes inside the material
  compose with the outer stack's blend" — decided as
  inner-first-then-outer (§1.3) rather than leaving options
  open, because the Instance/folder/flatten consumers need one
  order to code against.
- Brief's "shader DECLARES its channel set (an attribute on the
  graph?)" — decided: no attribute; the `out:<channel>` output
  nodes ARE the declaration (§1.2). Rationale recorded there.
