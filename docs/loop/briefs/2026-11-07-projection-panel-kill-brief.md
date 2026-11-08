# Kill brief — row 281: the projection panel + shell wiring (closes R-A2 v1)

You are a claw in the Umber workspace (`/home/bessa/Projects/umber`, branch
master). You know nothing except this brief. READ first:
`docs/specs/projection-fill-plan.md` (YOUR row is 281, the last v1 row),
`crates/umber-app/src/projection.rs` (row 278's module — its API:
`ProjectionState { image, transform, opacity, armed, frozen, path, load_error }`,
`adopt(path, Result<RgbaImage, ProjectionLoadError>, view: [f32; 2])`,
`arm(camera: OrbitCamera, rect: egui::Rect)`, `disarm()`,
`load_image_rgba8(path, max_pixels)`, `rasterize_projection`, `overlay_quad`),
and `crates/umber-app/src/viewport.rs` row 280 (`ViewportExtras.projection`,
`Viewport::apply_projection(rect, mesh, paint, projection) -> bool`).

## Your file surface (touch NOTHING else)

- `crates/umber-app/src/main.rs`
- `crates/umber-app/src/display_panel.rs`
- `assets/i18n/en-US.ftl`
- `assets/i18n/pt-BR.ftl`

Do NOT run cargo/clippy/tests. Do NOT commit or touch git. No new deps.

## What to build

1. **main.rs**
   - next to `pub use umber_app::stencil;` (~line 187) add
     `pub use umber_app::projection;` (the bin's re-export — row 280's docs
     note it is missing);
   - in the session-state struct next to `pub stencil: stencil::StencilState`
     (~line 1685) add `pub projection: projection::ProjectionState,`
     (never serialized, like the stencil);
   - at the viewport UI call (~line 5257, `stencil: &mut self.state.stencil`)
     add `projection: &mut self.state.projection,` in the same `ViewportExtras`;
   - next to the `stencil::panel_section(...)` call (~line 1554) add the
     projection section: call `display_panel::projection_rows(ui, ProjectionRow{...})`
     borrowing the fields (see 2) and handle its response:
     - `picked: Some(path)` → `projection::load_image_rgba8(&path, DEFAULT cap —
       use `umber_core::svg::DEFAULT_SVG_RASTER_PIXELS` as max_pixels)` →
       `self.state.projection.adopt(path, result, view)` where `view` is the
       active tile dims from paint (`paint.quick_mask_for_tile(paint.active_tile()).dimensions()`
       as `[f32;2]`, mirroring `stencil::panel_section`'s view logic in
       `crates/umber-app/src/stencil.rs:456`), then ARM:
       `self.state.projection.arm(self.viewport.camera(), viewport_rect)` —
       the viewport's live camera and its current rect (find how the viewport
       exposes its camera/rect; if the rect is not exposed, store the last
       rect the viewport drew at — add a tiny accessor on Viewport if needed,
       but keep viewport.rs UNTOUCHED: prefer reading what exists, e.g.
       `Viewport::camera()` if present, else capture the rect at the ui call
       site in main.rs);
     - `apply: true` → find the mesh handle used where the viewport UI is
       invoked (search the `ui_with_stencil` call in main.rs for the mesh
       argument) and call
       `self.viewport.apply_projection(rect, mesh, paint, &self.state.projection)`;
     - `armed toggle` → `projection.armed = !projection.armed`
       (arming also re-freezes: call `arm(camera, rect)` again);
     - `opacity` → `projection.opacity = v`.
   - the load-failure path maps `ProjectionLoadError` to the
     `overlays.projection-load-failed` key with a `reason` arg (mirror the
     stencil's `overlays.stencil-load-failed` handling at display_panel.rs:342).

2. **display_panel.rs** — mirror `stencil_rows` (`:323`) and `StencilRow`/`StencilRowResponse`:
   - `pub struct ProjectionRow<'a> { pub path: Option<&'a Path>, pub load_error: Option<&'a str>, pub has_image: bool, pub armed: bool, pub opacity: &'a mut f32 }`
     (borrowed fields only — the panel must NOT name `crate::projection`, same
     rule as the stencil row, module doc `:94-101`);
   - `pub struct ProjectionRowResponse { pub picked: Option<PathBuf>, pub toggled_arm: bool, pub apply: bool, pub opacity: Option<f32> }` + Default;
   - `pub fn projection_rows(ui: &mut Ui, row: ProjectionRow<'_>) -> ProjectionRowResponse`
     with rows: path label + Load button (rfd filter "PNG / SVG", png+svg),
     error label, Arm checkbox, Opacity ValueField (percent), Apply button
     (enabled when has_image && armed);
   - extend the i18n key-parity test at `:1034` with the new keys.

3. **i18n** — add to BOTH `assets/i18n/en-US.ftl` and `assets/i18n/pt-BR.ftl`
   (pt-BR in Brazilian Portuguese, matching the file's style):
   - `overlays.projection` = "Projection" / "Projeção"
   - `overlays.projection-no-image` = "no image" / "sem imagem"
   - `overlays.projection-load` = "Load…" / "Carregar…"
   - `overlays.projection-load-failed` = "projection load failed: {$reason}" / "falha ao carregar a projeção: {$reason}"
   - `overlays.projection-arm` = "Arm (freeze view)" / "Armar (congelar visão)"
   - `overlays.projection-apply` = "Apply to paint" / "Aplicar na pintura"
   - `overlays.projection-opacity` = "Opacity" / "Opacidade"

## Pins

- display_panel's i18n parity test passes with the new keys (extend it);
- `projection_rows` compiles against the borrowed-field adapter (the panel
  names no `crate::projection` type — a compile-time property, plus keep the
  existing pattern);
- if you add a small pure helper in main.rs (e.g. mapping a load error to the
  reason string), pin it.

## Doc comments

Name row 281 of the plan and close the v1 loop: with this row the decal
projector is usable end-to-end (load → arm/freeze → S-chords place → Apply
writes through the frozen camera into the ordinary layers, stencil-gated).

Write the code, keep it compiling by careful reading (you cannot run cargo),
and stop. Report what you changed and where.
