# Design brief: umber-vector-model (K1 of the path-tool vector wave, 2026-11-18)

Kill lock: `razul-vector-model` (code phase; this brief commits docs-first).
ROADMAP row 245's open half: live, editable vector paths on the canvas.
This is K1 of the 5-kill decomposition from the 2026-11-18 residuals census:
the data model only. Raster (K2), canvas editing (K3), panel (K4), and
SVG io (K5) follow as separate kills.

## Decision: mirror the Fill-layer precedent exactly

`LayerKind::Fill` is a plain tag with `Layer.fill: Option<FillParams>`
(`layers.rs:20-28`, `1001-1022`, `#[serde(default)]` — additive, no format
bump). The vector layer copies that shape one-for-one:

- `LayerKind::Vector` — plain tag variant in `layers.rs`.
- `Layer.vector: Option<VectorParams>` with `#[serde(default)]` on `Layer`
  (`layers.rs:1022` neighbor) — `Some` exactly for vector layers; old
  projects load with `None`.
- `VectorParams` in NEW `crates/umber-core/src/vector.rs`:

```rust
pub struct VectorShape {
    pub closed: bool,
    pub fill: [u8; 4],          // sRGBA
    pub stroke: Option<Stroke>, // width in UV units + color
    pub anchors: Vec<Anchor>,   // bezier: pos + in/out handles
}
pub struct Anchor { pub pos: [f32; 2], pub in_handle: [f32; 2], pub out_handle: [f32; 2] }
pub struct Stroke { pub width: f32, pub color: [u8; 4] }
pub struct VectorParams { pub shape: VectorShape }
```

- Serialization in `project.rs` next to the fill arm; additive keys only
  (R-D1 pattern: `skip_serializing_if` when `None`).
- Journal: `LayerCommand` gains NO new variants for K1 — vector layers are
  added/removed/reordered like any layer (`Add` with `LayerKind::Vector`).
  Anchor-level edit ops (`VectorEdit { layer, before: VectorShape, after:
  VectorShape }`) land as a `JournalOp` context variant in K3 when the
  canvas editor exists; K1 only needs add/remove + the project round-trip.
- Composite/eval: K1 leaves eval untouched — a vector layer composites as
  empty (fully transparent) until K2's rasterizer lands. `LayerStack`
  match arms over `LayerKind` get a `Vector` arm returning empty coverage
  (same as an unfilled Fill behaves today). No panic paths: every
  `match kind` is exhaustive so the compiler names every site.

## Reuse (do not rebuild)

- `crates/umber-app/src/path_tool.rs` `PaintPathTool` (736 LOC) — the
  editable-polyline UX template for K3; K1 does not touch it.
- `crates/umber-core/src/svg.rs` — the parsed `Shape` model (`svg.rs:1267`)
  is private; K2 promotes a pub path-shape API. K1's `Anchor` uses plain
  `[f32; 2]` UV coords, independent of svg.rs.
- `projection.rs` frozen-rasterize → one undo step → apply — the v1 fill
  path K2/K3 will reuse for rasterize-at-apply.

## Scope / non-goals for K1

- Data model + LayerKind + Layer field + project serialization +
  round-trip tests + empty-composite eval arm + layers-panel display name
  handling (panel row UI is K4; K1 only ensures the panel doesn't break —
  it renders the name like any layer).
- NO rasterizer (K2), NO canvas editing (K3), NO panel widgets (K4),
  NO SVG import conversion or export serializer (K5 — nothing exists
  today, grep-verified).
- No format version bump; no `.spp`/`.sbsar` anything (permanently
  forbidden).

## Tests (all headless in umber-core)

- `vector_layer_round_trips_through_project`: add Vector layer with a
  2-anchor shape → save → load → byte-equal `VectorParams`.
- `vector_layer_field_absent_in_old_projects_loads_as_none`: a project
  JSON without the `vector` key loads with `vector == None`.
- `vector_layer_composites_empty_until_k2`: eval of a stack with one
  vector layer yields all-transparent coverage (pins the K1 behavior).
- `layer_kind_vector_matches_are_exhaustive`: compile-time; plus a
  stack test add/remove/reorder/visibility/opacity on a vector layer.
- serde unit: `VectorShape` anchor handles serialize deterministically.

## Battery

rustfmt, check-layers, scorecard, clippy `-p umber-core --all-targets
-D warnings` (+ umber-app leg for the eval/panel arms),
`cargo test -p umber-core -p umber-app` under flock, kill report + queue row
+ dcommit --path-limited + safe-push + ls-remote + lock release.
