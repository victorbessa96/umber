# Kill report: dpi-runtime-reraster (2026-11-16)

## Built

- `preferences::hidpi_class(pixels_per_point)` (`preferences.rs`): the
  >= 1.75 hidpi threshold as a named fn; the startup atlas pick in
  `main.rs` now calls it too, so startup and runtime agree by
  construction.
- Per-frame watcher at the top of `UmberApp::fn ui` (`main.rs`): reads
  `ctx.input(|i| i.pixels_per_point)`, and when the hidpi class flips,
  reloads the merged icon atlas via
  `umber_gui::icons::load_baked_atlas_merged(ctx, hidpi)` and calls
  `ctx.request_repaint()` so the viewport re-rasterizes at the new
  scale. On reload failure the previous atlas is kept (never a frame
  error). New `UmberApp.last_hidpi` field tracks the class.
- Under `ui_scale_auto` (default) egui's zoom already follows
  `pixels_per_point`; under a manual `ui_scale` the zoom stays pinned by
  the existing `style_delta` path — unchanged.
- `icons.rs` module docs updated: the "Deferred (v1 boundary)" note is
  replaced by the LANDED runtime re-raster description (doc-sync rule).
- Test: `preferences::hidpi_tests::hidpi_class_boundary_is_1_75`.

## Judgment calls

- The watcher keys on the hidpi CLASS (1x vs 2x atlas), not on every
  fractional ppp change: continuous ppp values within a class only
  rescale egui's zoom, which needs no atlas work. The atlas is the only
  density-baked asset in the shell.
- Paint channel rasters are sized by `channel_sizes` (project prefs), not
  by DPI, so no paint-side invalidation is owed; the viewport re-renders
  from the wgpu pipeline each frame, and the forced repaint covers the
  one frame the input event lands on.

## Unverified

- Real multi-monitor drag not exercised headless; the class-flip path is
  covered by the unit test and the watcher is a pure per-frame compare.

## Deferred

- winink live-HWND (L4) stays gated (Windows-only).
- Release packet lanes stay partner-gated.

## Gates

- rustfmt --check clean on the three touched .rs files; check-layers
  GREEN; scorecard current.
- `cargo clippy -p umber-app -p umber-gui --all-targets -- -D warnings`:
  exit 0 (first run caught a doc-comment-on-expression-field warning in
  the constructor init; fixed to a plain comment, rerun clean).
- `cargo test -p umber-app -p umber-gui` under
  `UMBER_ASSETS_DIR=<repo root>` + flock: **1335 passed, 0 failed**,
  including `hidpi_class_boundary_is_1_75`.
- Lock `dpi-runtime-reraster` held for the whole edit, removed at commit.
