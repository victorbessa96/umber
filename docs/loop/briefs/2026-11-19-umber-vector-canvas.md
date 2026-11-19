# Design brief: umber-vector-canvas (K3 of the path-tool vector wave, 2026-11-19)

Kill lock: `razul-vector-canvas` (brief phase; code phase takes the same lock
after the docs-first commit). K1 model is `46d4bff5`; K2 raster is `0b5cb72d`.
ROADMAP row 245's next half is interactive editing. K4 property widgets and K5
SVG conversion/export remain separate kills.

## What exists

- K1 stores one `VectorShape` (cubic anchors + in/out handles, closed flag,
  fill, optional stroke) per vector layer, serde-additive with no format bump.
- K2 evaluates that shape into cached eval-owned baseColor planes. Its frame
  adoption notices every `Layer.vector` change and refreshes without a mesh.
- `path_tool.rs` already owns UV append/insert/delete/drag gestures for the
  session-only paint-replay path. `uv_view.rs` routes those handles before
  ordinary paint and draws the overlay. These are editor mechanics, not a
  document-backed vector layer editor.
- `LayerCommand::SetFill` and `LayerStack::set_fill` provide the exact reversible
  command pattern. `HistoryKind::from_command` is total over `LayerCommand`.

## Target and decisions

1. **One reversible vector edit command.** Add `LayerCommand::SetVector { id,
   new: VectorParams, old: Option<Option<VectorParams>> }` plus
   `LayerCommand::set_vector` and `LayerStack::set_vector`, mirroring SetFill's
   apply/revert and wrong-kind-inert conventions. No schema change. Add
   `HistoryKind::SetVector` and en/pt labels.
2. **Active vector layer enters edit mode.** When the active target is a vector
   layer, the shell adopts its id and params into a session `VectorEditTool`.
   Selecting another kind disarms; new/open/reset clears the tool. No K4 panel
   widgets are needed to enter or leave the mode.
3. **Working copy during a gesture, one command on completion.** Anchor and
   handle drags mutate only the tool's working shape. The UV overlay previews
   that shape; the document receives exactly one `SetVector` on release. If the
   document changed externally during the drag, the gesture is abandoned and
   the document wins instead of being clobbered. Undo/redo adopts the current
   document shape when no drag is active.
4. **Complete anchor mechanics.** Append corner anchors, insert near the nearest
   anchor segment (closing segment included), delete nearest, and toggle closed.
   Each is exactly one journal op. `MAX_VECTOR_ANCHORS` and non-finite UVs refuse
   before mutation. Dragging an anchor moves its in/out handles by the same
   delta; dragging either handle edits only that handle.
5. **Correct cubic preview.** Expose K2's centerline as a read-only
   `pub fn centerline(...) -> Result<Vec<[f64;2]>, VectorError>`. The UV overlay
   draws that exact centerline plus anchor/handle guides. This is a visibility
   change only, not a second flattener or raster path.
6. **Bounded K3, no K4/K5.** Fill/stroke editors, stroke cap/join/fill-rule
   selectors, gradients, multi-shape layers, SVG conversion/export, and any
   format migration stay out.

## Tests

- Core: SetVector apply/undo/redo restores the exact previous `VectorParams`;
  None-before restores None; unknown/wrong-kind behavior follows SetFill;
  target/history mapping is total.
- Editor unit tests: append/insert/delete/close each produce one undo step;
  anchor drag preserves handle deltas and produces one undo step; in/out handle
  drags stay independent and produce one undo step; non-finite input and anchor
  cap refuse; undo/redo adopts the document; external mid-gesture edits are not
  clobbered; release with no net change writes no history row.
- UV mapping: guide centerline uses the same points as rasterization; existing
  UV paint-path tests stay green. No new egui harness is built just for K3.

Battery: rustfmt on touched `.rs`; `check-layers.py`; `scorecard.py --check`;
clippy and tests for `umber-core`, `umber-brush`, `umber-paint`, and `umber-app`
under the shared flock and machine-fit env. Independent read-only review before
commit; kill report with built / judgment calls / unverified / deferred / process.

## Exact lock surface

- `crates/umber-core/src/vector.rs` (read-only centerline visibility)
- `crates/umber-core/src/layers.rs` (SetVector + stack method + tests)
- `crates/umber-app/src/vector_edit.rs` (new editor + tests)
- `crates/umber-app/src/uv_view.rs`
- `crates/umber-app/src/main.rs` (module, app state, PanelViewer, reset/adoption)
- `crates/umber-app/src/history_panel.rs`
- `assets/i18n/en-US.ftl`
- `assets/i18n/pt-BR.ftl`
- `README.md` (scorecard only if its bucket changes)
- `docs/ROADMAP.md`
- `docs/loop/briefs/2026-11-19-umber-vector-canvas.md`
- `docs/loop/kill-reports/2026-11-19-umber-vector-canvas.md`
- `docs/loop/queue.md`
- `STATE.md`
- `.dragon/COORDINATION-next-wave.md`

## Non-goals

No K4 property widgets, K5 SVG conversion/export, per-frame history coalescing
infrastructure, schema changes, multi-shape layers, new dependencies, or live
raster republishing during a drag. K2 publishes on command completion; the
working centerline is the drag preview.
