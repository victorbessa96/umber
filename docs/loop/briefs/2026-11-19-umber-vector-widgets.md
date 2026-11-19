# Design brief: umber-vector-widgets (K4 of the path-tool vector wave, 2026-11-19)

Kill lock: `razul-vector-widgets`. K1 model is `46d4bff5`, K2 raster is
`0b5cb72d`, and K3 canvas editing is `98fdbb80` with closeout `84dec4ce`.
K5 SVG conversion/export remains separate.

## What exists

- `VectorShape` already carries `closed`, sRGBA `fill`, optional `Stroke {
  width, color }`, and anchors. `LayerCommand::SetVector` and its journal,
  History, and i18n paths landed in K3.
- The UV editor creates a fresh layer's params on its first edit. Its adopted
  baseline distinguishes `vector = None` from an initialized shape.
- The Properties rail has Brush, Layer, Fill, Mask, and TextureSet contexts.
  There is no Vector context or vector property editor. Fresh paths currently
  adopt `VectorShape::empty()`, whose transparent fill makes the first visible
  stroke/fill depend on a later property edit.
- egui 0.36.2 provides `color_edit_button_srgba_unmultiplied(&mut [u8; 4])`,
  matching the vector model without a manual color-space conversion.

## Target and decisions

1. **Dedicated Vector Properties context.** Add `ContextKind::Vector` after
   Fill in rail order, with the existing `spline` atlas glyph and localized
   `ctx-rail.vector` label. Selecting a vector layer exposes Layer + Vector;
   other layer kinds do not gain it. Register the vector section's row labels
   in the same order as its body.
2. **Useful new-path defaults.** Implement `Default for VectorParams` as an
   empty open path with opaque white fill and no stroke. `VectorShape::empty()`
   retains its transparent geometry/raster semantics. K3's fresh-layer adoption
   and K4's uninitialized panel both use `VectorParams::default()`, so the first
   drawn path is visible while the document baseline remains `None` until an
   actual edit. Enabling a stroke defaults to width `0.01` UV and opaque black.
3. **One section per vector layer.** New `vector_panel.rs` draws a collapsible
   section per vector layer: Closed checkbox, Fill sRGBA editor, Stroke-enabled
   checkbox, and (when enabled) Stroke width and sRGBA editors. Width uses the
   house `ValueField` with finite range `0.0001..=1.0` UV. All edits clone the
   current params (or the default for `None`) and run one
   `LayerCommand::set_vector` per egui change, matching the Fill panel. No
   anchor serialization or schema change.
4. **Preserve K3 editing behavior.** Property edits publish through the same
   SetVector adoption/raster path as canvas edits. No second command variant,
   journal work, per-frame timer, or direct `Layer.vector` mutation in the
   panel. Undo of the first property or canvas edit still restores `None`.
5. **Bounded K4.** No SVG conversion/export, fill-rule/cap/join/dash controls,
   gradients, multi-shape layers, new dependencies, release/version work, or
   live pointer-harness expansion.

## Tests

- Core default: empty anchors, open path, opaque white fill, no stroke;
  `VectorShape::empty()` remains transparent.
- Editor: fresh-layer first anchor inherits the visible default and undo still
  restores `None`; all K3 lifecycle tests remain green.
- Properties context: Vector appears only for `LayerKind::Vector`, ordered
  after Fill; row labels match the vector panel's drawn keys and localize in
  both catalogs. Existing context-order/index tests are updated deliberately.
- Panel state helpers: enabling stroke installs width `0.01` and opaque black;
  disabling removes it without touching fill or anchors; width input is finite
  and range-bound by the widget declaration. A document-level default-property
  edit creates params once and undo restores `None`.

Battery: rustfmt on touched `.rs`; `check-layers.py`; `scorecard.py --check`;
clippy and tests for `umber-core`, `umber-gui`, `umber-brush`, `umber-paint`,
and `umber-app` under the shared flock and machine-fit env. Bounded Kimi K3.1
Pro read-only review before commit; kill report, queue, STATE, ROADMAP,
coordination, task-progress, and vault checkpoint at landing.

## Exact lock surface

- `crates/umber-gui/src/widgets/context_rail.rs`
- `crates/umber-core/src/vector.rs`
- `crates/umber-app/src/vector_panel.rs` (new)
- `crates/umber-app/src/vector_edit.rs`
- `crates/umber-app/src/main.rs`
- `assets/i18n/en-US.ftl`, `assets/i18n/pt-BR.ftl`
- `README.md` (scorecard only if its bucket changes)
- `docs/ROADMAP.md`, this brief, the K4 kill report, `docs/loop/queue.md`,
  `STATE.md`, `.dragon/COORDINATION-next-wave.md`

## Non-goals

No K5 SVG io, schema migration, direct pixel writes, alternate color picker,
new command type, stroke cap/join/fill-rule selectors, or interactive dogfood
claim. Real pointer and visual feel remain manual verification, not a headless
test claim.
