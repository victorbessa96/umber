# Kill report: umber-vector-model (K1 of the path-tool vector wave, 2026-11-18, lock `razul-vector-model`)

Brief: `docs/loop/briefs/2026-11-18-umber-vector-model.md` (`a5c3ebce`, docs-first).
ROADMAP row 245 open half, K1 (data model only; raster/canvas/panel/io are K2-K5).

## Changes

- NEW `crates/umber-core/src/vector.rs`: `Anchor` (pos + in/out handles, UV),
  `Stroke`, `VectorShape` (closed, fill sRGBA, optional stroke, anchors),
  `VectorParams`; serde round-trip deterministic; 3 unit tests.
- `layers.rs`: `LayerKind::Vector` plain tag (Fill precedent) + `Layer.vector:
  Option<VectorParams>` with `#[serde(default, skip_serializing_if)]` (R-D1
  additive, no format bump); `InstanceError::PaintOnVector` + check_paintable arm.
- `paint_state.rs`: `SnapshotLayerKind::Vector`; `snapshot.rs`: Vector arms in
  both LayerKind matches (composites empty until K2).
- `document.rs`: layers-panel kind label `layers.kind-vector`; `history_panel.rs`:
  `HistoryKind::AddVector` + labels; i18n keys in both catalogs (en/pt).
- `project.rs` tests: vector shape round-trips save→load→save byte-identical;
  pre-vector project files load with `vector == None` and re-save without the key.

## Battery

rustfmt clean; check-layers GREEN; scorecard current; clippy `--workspace
--all-targets -D warnings` 0 errors; cargo test 27 suites ok / 0 failed
(5 new vector tests green). Full `--workspace` run's only failure is
`umber-cli` `library_list_step_replies_rows_over_the_real_dirs`, proven
pre-existing at clean HEAD (fails with the diff stashed) — environmental,
not this kill. safe-push green; ls-remote verified; lock released; queue row 545.
