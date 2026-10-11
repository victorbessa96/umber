//! Fill layers: live procedural layers over the mesh's projections
//! (requirements.md §2's P0 row — the last big P0 model gap for 0.1.0).
//!
//! **Live, not baked paint.** A fill layer holds [`FillParams`] (source +
//! projection) and re-evaluates from them on demand: [`FillLayerState`]
//! tracks a dirty flag, and ANY source/projection change (panel edit, undo,
//! redo, journal replay) marks the cached tiles stale so the next
//! [`refresh_fill_layer`] re-runs [`umber_mesh::evaluate_fill_tile`]. A
//! fill layer is never "baked paint"; there is no paint-stroke data to go
//! stale, only parameters to re-read.
//!
//! **Pipeline order per tile** (see [`refresh_fill_layer`]):
//!
//! 1. [`umber_mesh::evaluate_fill_tile`] — the RAW projection result
//!    (coverage + sampled RGBA, uncovered texels transparent), already
//!    gated by the layer's polygon-fill [`FillMask`](umber_mesh::FillMask).
//! 2. [`apply_symmetry`] — THE awaited caller of
//!    [`umber_brush::symmetry::symmetrize_coverage`]: the raw coverage goes
//!    through the union of its symmetric images, and each newly covered
//!    texel copies its first covered sibling's color (identity first, so
//!    originally covered texels keep their own sample). Symmetry off is
//!    the zero-cost path (the tile passes through untouched).
//! 3. [`apply_quick_mask`] — the quick-mask gate (THIS closes the
//!    `quick_mask` docs' open follow-up: "the fill-coverage hook is not
//!    gated yet"): the post-symmetry coverage is multiplied by the
//!    session mask at each texel, AFTER symmetry expansion like every
//!    retouch consumer, and texels gated to zero go transparent black so
//!    the `WriteRegion` upload below erases them on both paths.
//! 4. [`PaintState::publish_fill`](crate::paint_state::PaintState::publish_fill)
//!    — the tile/texture-set plumbing: the evaluated bytes ride the same
//!    `WriteRegion` queue as retouch strokes into the tile's GPU target.
//! 5. [`composite_fill_over`] — the stack composite step for readers that
//!    composite on the CPU: the fill blends over the accumulated base with
//!    the layer's own [`BlendMode`](umber_core::BlendMode) + opacity, per
//!    channel, through the SAME [`BlendMode::blend_channel`](umber_core::BlendMode::blend_channel)
//!    primitive every other layer uses. (`Passthrough` is structural for
//!    folders, not a pixel op — a fill layer carrying it composites as
//!    `Normal`, documented at the function.)
//!
//! The property rows ([`fill_section_ui`]) edit through [`Document::run`](crate::document::Document::run)
//! with [`LayerCommand::SetFill`](umber_core::LayerCommand::SetFill), so
//! every fill edit is undoable and journaled like any other layer op.

use std::collections::HashMap;

use umber_brush::symmetry::{symmetrize_coverage, texel_transforms};
use umber_brush::{QuickMask, SymTransform, SymmetrySettings};
use umber_core::{BlendMode, Layer, LayerCommand, LayerKind};
use umber_mesh::{
    evaluate_fill_tile, uv_triangle_at, FillMask, FillParams, FillProjection, FillSource, FillTile,
    MeshData, ProjAxis, LASSO_MAX_POINTS,
};

use crate::document::Document;
use crate::i18n::tr;
use crate::paint_state::PaintState;

/// One tile's evaluated fill bytes (post-symmetry, pre-quick-mask: the
/// session mask gates AFTER the cache, so ephemeral mask edits never
/// dirty the projection cache — see [`refresh_fill_layer`]).
pub struct CachedTile {
    /// The tile size (`size`x`size`) this was evaluated at.
    pub size: u32,
    /// RGBA8 bytes, row-major top-down (the paint-target layout).
    pub rgba: Vec<u8>,
    /// The 0..=1 coverage plane matching `rgba` (the composite and gate
    /// input; kept beside the bytes because the quick-mask gate runs
    /// after the cache lookup).
    pub coverage: Vec<f32>,
}

/// A fill layer's live evaluation state: its parameters, its dirty flag,
/// and its per-tile cache.
///
/// The parameters' source of truth is the [`Document`]'s layer
/// (`Layer::fill`); this state is the evaluation cache beside it.
/// [`Self::sync_from_layer`] adopts document-side changes (panel edits run
/// through [`Document::run`](crate::document::Document::run), undo/redo
/// replays them) and marks the cache dirty, so the next
/// [`refresh_fill_layer`] re-evaluates.
pub struct FillLayerState {
    layer_id: u64,
    params: FillParams,
    /// The symmetry settings the cache was evaluated with: a symmetry
    /// change must re-evaluate just like a source change.
    last_symmetry: SymmetrySettings,
    dirty: bool,
    cache: HashMap<u16, CachedTile>,
}

impl FillLayerState {
    /// Tracks `layer_id` with `params`; starts dirty (nothing evaluated
    /// yet — the first [`refresh_fill_layer`] fills the cache).
    pub fn new(layer_id: u64, params: FillParams) -> Self {
        Self {
            layer_id,
            params,
            last_symmetry: SymmetrySettings::default(),
            dirty: true,
            cache: HashMap::new(),
        }
    }

    /// The document layer this state evaluates.
    pub fn layer_id(&self) -> u64 {
        self.layer_id
    }

    /// The parameters the cache (will) reflect once clean.
    pub fn params(&self) -> &FillParams {
        &self.params
    }

    /// Whether the cache is stale: set by every mutator, cleared by a
    /// successful [`Self::evaluate`].
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Marks the cache stale (the source-change hook: a fill layer
    /// re-evaluates when its source changes — it is live, not baked).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.cache.clear();
    }

    /// Replaces the fill source (the panel's source editor v1:
    /// color/gradient/noise) and marks the layer dirty.
    pub fn set_source(&mut self, source: FillSource) {
        if self.params.source != source {
            self.params.source = source;
            self.mark_dirty();
        }
    }

    /// Replaces the projection (the panel's projection picker) and marks
    /// the layer dirty.
    pub fn set_projection(&mut self, projection: FillProjection) {
        if self.params.projection != projection {
            self.params.projection = projection;
            self.mark_dirty();
        }
    }

    /// Replaces the whole parameter set and marks the layer dirty.
    pub fn set_params(&mut self, params: FillParams) {
        if self.params != params {
            self.params = params;
            self.mark_dirty();
        }
    }

    /// Adopts the document layer's current params (panel edits, undo, redo,
    /// journal replay all land in `Layer::fill` first). A change — or a
    /// layer that lost its params — marks the cache dirty. Unknown layers
    /// (removed mid-session) are ignored, never fatal.
    pub fn sync_from_layer(&mut self, layer: &Layer) {
        if layer.id != self.layer_id {
            return;
        }
        match &layer.fill {
            Some(params) if *params == self.params => {}
            Some(params) => {
                self.params = params.clone();
                self.mark_dirty();
            }
            None => self.mark_dirty(),
        }
    }

    /// Evaluates `tile` at `size`x`size` under `symmetry`, returning the
    /// cached post-symmetry bytes. Re-runs the projection when dirty, when
    /// the tile was never evaluated, when `size` changed, or when the
    /// symmetry settings changed since the last evaluation; otherwise
    /// returns the cache untouched (the live-re-eval contract: identical
    /// parameters never recompute).
    pub fn evaluate(
        &mut self,
        mesh: &MeshData,
        tile: u16,
        size: u32,
        symmetry: &SymmetrySettings,
    ) -> &[u8] {
        let stale = self.dirty
            || *symmetry != self.last_symmetry
            || self.cache.get(&tile).is_none_or(|c| c.size != size);
        if stale {
            let raw = evaluate_fill_tile(mesh, &self.params, tile, size, size);
            let done = apply_symmetry(raw, symmetry);
            self.cache.insert(
                tile,
                CachedTile {
                    size,
                    rgba: done.rgba,
                    coverage: done.coverage,
                },
            );
            self.last_symmetry = *symmetry;
            self.dirty = false;
        }
        // Inserted above when stale; present with matching size otherwise.
        self.cache
            .get(&tile)
            .map(|c| c.rgba.as_slice())
            .unwrap_or(&[])
    }

    /// The cached coverage plane for `tile` at `size`x`size`, or `None`
    /// when the tile was never evaluated at that size (the refresh path
    /// falls back to ungated in that case — which only happens when
    /// [`Self::evaluate`] was never called, since it inserts on stale).
    pub fn coverage_of(&self, tile: u16, size: u32) -> Option<&[f32]> {
        self.cache
            .get(&tile)
            .filter(|c| c.size == size)
            .map(|c| c.coverage.as_slice())
    }
}

/// Applies the session quick mask to an evaluated tile: the post-symmetry
/// coverage is multiplied by the mask at each texel — AFTER symmetry
/// expansion, at the sibling's own texels, exactly like the retouch
/// consumers ([`RetouchStroke::apply_masked`](crate::retouch::RetouchStroke::apply_masked)).
///
/// Ungated (`!mask.gates()`) or size-mismatched tiles pass through
/// untouched: both planes track the paint target's size and resize resets
/// both, so a mismatch means a resize is in flight and the next refresh
/// re-evaluates — blocking the world on a torn gate would be worse.
/// Untouched mask planes read the mask's empty value (an active empty
/// mask blocks everything, an inverted one allows it), and texels gated
/// to exactly zero go transparent black so the full-tile `WriteRegion`
/// upload erases them on the GPU path too. Fractional coverage keeps its
/// sampled color for [`composite_fill_over`]'s Porter-Duff blend.
pub fn apply_quick_mask(tile: FillTile, mask: &QuickMask, tile_id: u16) -> FillTile {
    if !mask.gates() {
        return tile;
    }
    let (mw, mh) = mask.dimensions();
    if mw != tile.width || mh != tile.height {
        return tile;
    }
    let n = tile.coverage.len().min(tile.rgba.len() / 4);
    let mut out = tile;
    for i in 0..n {
        if !(out.coverage[i] > 0.0) {
            continue;
        }
        let m = mask
            .sample(tile_id, (i % mw as usize) as u32, (i / mw as usize) as u32)
            .clamp(0.0, 1.0);
        out.coverage[i] *= m;
        if !(out.coverage[i] > 0.0) {
            out.coverage[i] = 0.0;
            out.rgba[i * 4..i * 4 + 4].copy_from_slice(&[0, 0, 0, 0]);
        }
    }
    out
}

/// Applies the symmetry settings to an evaluated tile: the coverage goes
/// through [`symmetrize_coverage`] (THIS is the hook its docs await — "the
/// pure function the fill op calls on its projected coverage"), and each
/// texel the union newly covers copies its first covered sibling's color
/// (inverse transforms in order, identity first, nearest-texel — the same
/// sampling rule as `symmetrize_coverage`, so color and coverage agree on
/// every texel: coverage 1.0 always has a copied color behind it).
/// Symmetry off returns the tile untouched (zero-cost, byte-identical).
pub fn apply_symmetry(tile: FillTile, symmetry: &SymmetrySettings) -> FillTile {
    if !symmetry.is_active() {
        return tile;
    }
    let (width, height) = (tile.width, tile.height);
    let transforms = texel_transforms(symmetry, width as f32);
    let coverage = symmetrize_coverage(&tile.coverage, width, height, &transforms)
        .unwrap_or(tile.coverage.clone());
    let inverses: Vec<SymTransform> = transforms.iter().map(SymTransform::inverse).collect();
    let (w, h) = (width as usize, height as usize);
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let center = [x as f32 + 0.5, y as f32 + 0.5];
            for inv in &inverses {
                let [sx, sy] = inv.apply(center);
                let (fx, fy) = (sx.floor(), sy.floor());
                if fx < 0.0 || fy < 0.0 || fx as usize >= w || fy as usize >= h {
                    continue;
                }
                let si = fy as usize * w + fx as usize;
                if tile.coverage[si] > 0.0 {
                    rgba[x * 4 + y * w * 4..][..4].copy_from_slice(&tile.rgba[si * 4..si * 4 + 4]);
                    break;
                }
            }
        }
    }
    FillTile {
        width,
        height,
        rgba,
        coverage,
    }
}

/// Composites one evaluated fill tile over `base` in place — the fill
/// layer's step in the stack composite, honoring its blend mode + opacity
/// exactly like any other layer.
///
/// Per texel: `src_alpha = fill_alpha * coverage * opacity`, the RGB blend
/// runs through [`BlendMode::blend_channel`](umber_core::BlendMode::blend_channel)
/// (the shared per-channel primitive), and the result combines
/// Porter-Duff-over (`blend * src_alpha + dst * dst_alpha * (1 - src_alpha)`,
/// renormalized). Coverage `0.0` (or zero effective alpha) leaves the base
/// byte-identical. `Passthrough` — structural for folders, not a pixel op —
/// composites as `Normal` here (a fill layer carrying it behaves like a
/// regular layer, not a folder).
///
/// Length contract: `base` and the tile hold the same texel count and the
/// tile's coverage matches its RGBA; mismatches composite the shared prefix
/// and ignore the rest (never a panic — tile resizes mid-session reorder
/// evaluations, they must not crash the composite).
pub fn composite_fill_over(base: &mut [u8], fill: &FillTile, blend: BlendMode, opacity: f32) {
    let op = if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let n = (base.len() / 4)
        .min(fill.rgba.len() / 4)
        .min(fill.coverage.len());
    for i in 0..n {
        let cov = fill.coverage[i];
        if !(cov > 0.0) || op <= 0.0 {
            continue;
        }
        let f = &fill.rgba[i * 4..i * 4 + 4];
        let sa = f[3] as f32 / 255.0 * cov * op;
        if !(sa > 0.0) {
            continue;
        }
        let b = &base[i * 4..i * 4 + 4];
        let da = b[3] as f32 / 255.0;
        let oa = sa + da * (1.0 - sa);
        if !(oa > 0.0) {
            continue;
        }
        let mut out = [0u8; 4];
        for c in 0..3 {
            let (s, d) = (f[c] as f32 / 255.0, b[c] as f32 / 255.0);
            // `None` is Passthrough (see the fn docs): plain Normal.
            let blended = blend.blend_channel(s, d).unwrap_or(s);
            out[c] = ((blended * sa + d * da * (1.0 - sa)) / oa * 255.0 + 0.5)
                .floor()
                .clamp(0.0, 255.0) as u8;
        }
        out[3] = ((oa * 255.0 + 0.5).floor()).clamp(0.0, 255.0) as u8;
        base[i * 4..i * 4 + 4].copy_from_slice(&out);
    }
}

/// Re-evaluates `state` for every tile in `tiles` at `size`x`size`, gates
/// each through the session quick mask ([`apply_quick_mask`]), and stages
/// each through the paint session's tile plumbing
/// ([`PaintState::publish_fill`](crate::paint_state::PaintState::publish_fill)).
///
/// Call after any fill-param edit (panel, undo, redo), after mesh load, and
/// after a target resize (a size change invalidates the cache inside
/// [`FillLayerState::evaluate`]). Returns the tiles actually staged —
/// tiles whose target publish failed (dropped, logged inside `publish_fill`)
/// are absent, so the caller can surface them honestly instead of assuming.
pub fn refresh_fill_layer(
    state: &mut FillLayerState,
    mesh: &MeshData,
    paint: &mut PaintState,
    tiles: &[u16],
    size: u32,
    symmetry: &SymmetrySettings,
) -> Vec<u16> {
    let mut staged = Vec::new();
    for &tile in tiles {
        let rgba = state.evaluate(mesh, tile, size, symmetry).to_vec();
        let coverage = state.coverage_of(tile, size).unwrap_or(&[]).to_vec();
        let gated = apply_quick_mask(
            FillTile {
                width: size,
                height: size,
                rgba,
                coverage,
            },
            paint.quick_mask(),
            tile,
        );
        if paint.publish_fill(tile, size, size, gated.rgba) {
            staged.push(tile);
        }
    }
    staged
}

/// The polygon-fill tool's pick mode (§2 line 24's P1 row): which of the
/// four pixel-mask fills the click-path builds. The lasso (`Polygon`)
/// captures a multi-click path in the UV view; the other three capture
/// ONE pick and resolve it against the mesh at apply time, so a mesh
/// reload between pick and apply still fills the right faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PolygonFillMode {
    /// The 2D UV lasso → even-odd tile mask.
    #[default]
    Polygon,
    /// The single picked face.
    Triangle,
    /// The picked face's position-welded island (mesh-ID based).
    Mesh,
    /// The picked face's UV island (the UV chunk).
    UvChunk,
}

impl PolygonFillMode {
    /// The panel combo's i18n key for this mode.
    pub fn label_key(self) -> &'static str {
        match self {
            PolygonFillMode::Polygon => "fill.poly-mode-polygon",
            PolygonFillMode::Triangle => "fill.poly-mode-triangle",
            PolygonFillMode::Mesh => "fill.poly-mode-mesh",
            PolygonFillMode::UvChunk => "fill.poly-mode-uv-chunk",
        }
    }
}

/// The polygon-fill lasso tool state: the armed click-path capture that
/// feeds [`FillMask`]s into fill layers.
///
/// While [`Self::is_armed`], primary clicks in the UV view feed this tool
/// (lasso points in `Polygon` mode, the single pick otherwise) instead of
/// painting — the same capture convention as the clone tool's Alt-click
/// source. [`Self::apply_to`] builds the mask and writes it through
/// [`Document::run`](crate::document::Document::run) as one undoable,
/// journaled [`LayerCommand::SetFill`](umber_core::LayerCommand::SetFill),
/// preserving the target's source + projection. Session state only: never
/// serialized, never journaled.
#[derive(Debug, Clone, Default)]
pub struct PolygonFillTool {
    mode: PolygonFillMode,
    /// The fill layer receiving the mask (`None` until the panel offers
    /// one — the rows auto-select the first fill layer).
    target: Option<u64>,
    /// The lasso path (`Polygon` mode), global UVs in click order.
    points: Vec<[f32; 2]>,
    /// The single pick (other modes), global UV.
    pick: Option<[f32; 2]>,
    /// Whether UV-view clicks feed this tool instead of painting.
    armed: bool,
}

impl PolygonFillTool {
    /// The pick mode under capture.
    pub fn mode(&self) -> PolygonFillMode {
        self.mode
    }

    /// Switches the pick mode, dropping any in-progress capture (a lasso
    /// path is meaningless to a single-pick mode and vice versa).
    pub fn set_mode(&mut self, mode: PolygonFillMode) {
        if self.mode != mode {
            self.mode = mode;
            self.points.clear();
            self.pick = None;
        }
    }

    /// The fill layer receiving the mask.
    pub fn target(&self) -> Option<u64> {
        self.target
    }

    /// Retargets the mask write.
    pub fn set_target(&mut self, target: Option<u64>) {
        self.target = target;
    }

    /// Whether UV-view clicks feed this tool instead of painting.
    pub fn is_armed(&self) -> bool {
        self.armed
    }

    /// Arms or disarms the click capture.
    pub fn set_armed(&mut self, armed: bool) {
        self.armed = armed;
    }

    /// The captured lasso path, in click order.
    pub fn points(&self) -> &[[f32; 2]] {
        &self.points
    }

    /// The captured single pick (non-`Polygon` modes), global UV.
    pub fn pick(&self) -> Option<[f32; 2]> {
        self.pick
    }

    /// How many lasso points are captured.
    pub fn point_count(&self) -> usize {
        self.points.len()
    }

    /// Appends a lasso point: only while armed in `Polygon` mode;
    /// non-finite clicks never record, and past
    /// [`LASSO_MAX_POINTS`](umber_mesh::LASSO_MAX_POINTS) further clicks
    /// are ignored, never an error. Returns whether the point was stored.
    pub fn push_point(&mut self, uv: [f32; 2]) -> bool {
        if !self.armed || self.mode != PolygonFillMode::Polygon {
            return false;
        }
        if !uv[0].is_finite() || !uv[1].is_finite() {
            return false;
        }
        if self.points.len() >= LASSO_MAX_POINTS {
            return false;
        }
        self.points.push(uv);
        true
    }

    /// Drops the last lasso point; `false` when there is none.
    pub fn undo_point(&mut self) -> bool {
        self.points.pop().is_some()
    }

    /// Drops the whole capture (lasso path + single pick).
    pub fn clear_capture(&mut self) {
        self.points.clear();
        self.pick = None;
    }

    /// Whether any capture is held (a path or a pick).
    pub fn has_capture(&self) -> bool {
        !self.points.is_empty() || self.pick.is_some()
    }

    /// Records the single pick: only while armed in a non-`Polygon`
    /// mode; non-finite clicks never record. Replaces the previous pick.
    /// Returns whether the pick was stored.
    pub fn set_pick(&mut self, uv: [f32; 2]) -> bool {
        if !self.armed || self.mode == PolygonFillMode::Polygon {
            return false;
        }
        if !uv[0].is_finite() || !uv[1].is_finite() {
            return false;
        }
        self.pick = Some(uv);
        true
    }

    /// The picked triangle under the single pick, or `None` with no pick
    /// / on a miss (the UV-view pick agrees with the fill rasterizer's
    /// overlap rule — last in index order wins).
    pub fn pick_triangle(&self, mesh: &MeshData) -> Option<usize> {
        self.pick.and_then(|uv| uv_triangle_at(mesh, uv))
    }

    /// Whether the capture is complete enough to build a mask WITHOUT
    /// walking the mesh (the panel's per-frame Apply enablement — the
    /// floods run once inside [`Self::apply_to`], not every frame).
    pub fn can_apply(&self) -> bool {
        if self.target.is_none() {
            return false;
        }
        match self.mode {
            PolygonFillMode::Polygon => self.points.len() >= 3,
            PolygonFillMode::Triangle | PolygonFillMode::Mesh | PolygonFillMode::UvChunk => {
                self.pick.is_some()
            }
        }
    }

    /// Builds the mask for the current capture: the lasso loop in
    /// `Polygon` mode, or the picked triangle resolved through
    /// [`uv_triangle_at`] + the island floods otherwise. `None` when the
    /// capture is incomplete or the pick misses.
    pub fn build_mask(&self, mesh: &MeshData) -> Option<FillMask> {
        match self.mode {
            PolygonFillMode::Polygon => {
                (self.points.len() >= 3).then(|| FillMask::polygon(self.points.clone()))
            }
            PolygonFillMode::Triangle => self.pick_triangle(mesh).map(FillMask::triangle),
            PolygonFillMode::Mesh => self
                .pick_triangle(mesh)
                .map(|t| FillMask::mesh_island(mesh, t)),
            PolygonFillMode::UvChunk => self
                .pick_triangle(mesh)
                .map(|t| FillMask::uv_chunk(mesh, t)),
        }
    }

    /// Writes the built mask into the target fill layer through
    /// [`Document::run`](crate::document::Document::run) — one undoable,
    /// journaled op preserving the layer's source + projection — and
    /// clears the capture (Painter closes the lasso on apply). Returns
    /// whether an op was dispatched: `false` with no target, an
    /// incomplete capture, a missed pick, or a target that is not a fill
    /// layer (a stray write stays inert data — rejected here, never
    /// written).
    pub fn apply_to(&mut self, doc: &mut Document, mesh: &MeshData) -> bool {
        let target = match self.target {
            Some(id) => id,
            None => return false,
        };
        let mask = match self.build_mask(mesh) {
            Some(mask) => mask,
            None => return false,
        };
        let mut params = match doc
            .layers()
            .iter()
            .find(|l| l.id == target)
            .and_then(|l| l.fill.clone())
        {
            Some(params) => params,
            None => return false,
        };
        params.mask = Some(mask);
        doc.run(LayerCommand::set_fill(target, params));
        self.clear_capture();
        true
    }
}

/// The polygon-fill tool rows of the Properties panel: the armed toggle,
/// the four-mode picker, the target-layer picker, the capture status, and
/// the Undo-point / Clear / Apply / Clear-mask buttons.
///
/// Apply dispatches through [`PolygonFillTool::apply_to`] (one undoable,
/// journaled op); every other widget edits the session tool only. All
/// strings go through [`tr`](crate::i18n::tr).
pub fn polygon_fill_rows(
    ui: &mut egui::Ui,
    doc: &mut Document,
    mesh: Option<&MeshData>,
    tool: &mut PolygonFillTool,
) {
    ui.add_space(4.0);
    ui.strong(tr("fill.poly-tool"));
    let fills: Vec<(u64, String)> = doc
        .layers()
        .iter()
        .filter(|l| l.kind == LayerKind::Fill)
        .map(|l| (l.id, l.name.clone()))
        .collect();
    if fills.is_empty() {
        ui.label(tr("fill.none"));
        return;
    }
    if tool
        .target
        .is_none_or(|id| fills.iter().all(|(f, _)| *f != id))
    {
        tool.set_target(fills.first().map(|(id, _)| *id));
    }
    ui.horizontal(|ui| {
        let mut armed = tool.is_armed();
        ui.checkbox(&mut armed, tr("fill.poly-armed"));
        tool.set_armed(armed);
    });
    ui.label(tr("fill.poly-hint"));
    // The target layer: the mask write preserves its source+projection.
    let mut target = tool.target();
    egui::ComboBox::from_id_salt("fill-poly-target")
        .selected_text(
            fills
                .iter()
                .find(|(id, _)| Some(*id) == target)
                .map(|(_, name)| name.clone())
                .unwrap_or_default(),
        )
        .show_ui(ui, |ui| {
            for (id, name) in &fills {
                ui.selectable_value(&mut target, Some(*id), name.clone());
            }
        });
    tool.set_target(target);
    // The four-mode picker (the P1 row's four pixel-mask fills; keys as
    // literals so the i18n completeness scan sees every call site).
    let mut mode = tool.mode();
    let mode_label = match mode {
        PolygonFillMode::Polygon => tr("fill.poly-mode-polygon"),
        PolygonFillMode::Triangle => tr("fill.poly-mode-triangle"),
        PolygonFillMode::Mesh => tr("fill.poly-mode-mesh"),
        PolygonFillMode::UvChunk => tr("fill.poly-mode-uv-chunk"),
    };
    egui::ComboBox::from_id_salt("fill-poly-mode")
        .selected_text(mode_label)
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut mode,
                PolygonFillMode::Polygon,
                tr("fill.poly-mode-polygon"),
            );
            ui.selectable_value(
                &mut mode,
                PolygonFillMode::Triangle,
                tr("fill.poly-mode-triangle"),
            );
            ui.selectable_value(&mut mode, PolygonFillMode::Mesh, tr("fill.poly-mode-mesh"));
            ui.selectable_value(
                &mut mode,
                PolygonFillMode::UvChunk,
                tr("fill.poly-mode-uv-chunk"),
            );
        });
    tool.set_mode(mode);
    // The capture status: lasso length, or the picked triangle.
    match tool.mode() {
        PolygonFillMode::Polygon => {
            ui.label(format!(
                "{}: {}",
                tr("fill.poly-points"),
                tool.point_count()
            ));
        }
        _ => {
            let pick = mesh
                .and_then(|m| tool.pick_triangle(m))
                .map(|t| t.to_string())
                .unwrap_or_else(|| tr("fill.poly-no-pick"));
            ui.label(format!("{}: {pick}", tr("fill.poly-pick")));
        }
    }
    ui.horizontal(|ui| {
        let undo = egui::Button::new(tr("fill.poly-undo-point"));
        if ui.add_enabled(!tool.points().is_empty(), undo).clicked() {
            tool.undo_point();
        }
        if ui
            .add_enabled(tool.has_capture(), egui::Button::new(tr("fill.poly-clear")))
            .clicked()
        {
            tool.clear_capture();
        }
        if let Some(m) = mesh {
            if ui
                .add_enabled(tool.can_apply(), egui::Button::new(tr("fill.poly-apply")))
                .clicked()
            {
                tool.apply_to(doc, m);
            }
        }
    });
    // The mask round trip: a target carrying a mask offers to drop it.
    let has_mask = tool
        .target()
        .and_then(|id| doc.layers().iter().find(|l| l.id == id))
        .and_then(|l| l.fill.clone())
        .is_some_and(|p| p.mask.is_some());
    if has_mask && ui.button(tr("fill.poly-clear-mask")).clicked() {
        if let Some(id) = tool.target() {
            if let Some(mut params) = doc
                .layers()
                .iter()
                .find(|l| l.id == id)
                .and_then(|l| l.fill.clone())
            {
                params.mask = None;
                doc.run(LayerCommand::set_fill(id, params));
            }
        }
    }
}

/// The fill section of the Properties panel: one collapsible row per
/// [`LayerKind::Fill`](umber_core::LayerKind::Fill) layer in the stack
/// (bottom-to-top), each with the projection picker, the projection's
/// axis/sharpness rows, and the v1 source editor (color / gradient / noise;
/// a graph snapshot shows its dims — wiring a fresh graph output into the
/// snapshot is the named follow-up) — plus the polygon-fill tool rows
/// ([`polygon_fill_rows`]).
///
/// Edits dispatch [`LayerCommand::SetFill`](umber_core::LayerCommand::SetFill)
/// through [`Document::run`](crate::document::Document::run) ONLY when a
/// widget changed (each edit is one undoable, journaled op — scrubbing a
/// slider still emits per-frame ops, matching the brush panel's live-edit
/// convention). All strings go through [`tr`](crate::i18n::tr).
pub fn fill_section_ui(
    ui: &mut egui::Ui,
    doc: &mut Document,
    mesh: Option<&MeshData>,
    tool: &mut PolygonFillTool,
) {
    ui.add_space(8.0);
    ui.separator();
    ui.strong(tr("fill.section"));
    let fills: Vec<(u64, String, FillParams)> = doc
        .layers()
        .iter()
        .filter(|l| l.kind == LayerKind::Fill)
        .map(|l| (l.id, l.name.clone(), l.fill.clone().unwrap_or_default()))
        .collect();
    if fills.is_empty() {
        ui.label(tr("fill.none"));
        return;
    }
    for (id, name, params) in fills {
        egui::CollapsingHeader::new(name.clone())
            .default_open(true)
            .show(ui, |ui| {
                let mut edited = params;
                let mut changed = false;
                projection_rows(ui, id, &mut edited.projection, &mut changed);
                ui.separator();
                source_rows(ui, id, &mut edited.source, &mut changed);
                if changed {
                    doc.run(LayerCommand::set_fill(id, edited));
                }
            });
    }
    ui.separator();
    polygon_fill_rows(ui, doc, mesh, tool);
}

/// The projection picker plus the active projection's parameter rows.
fn projection_rows(
    ui: &mut egui::Ui,
    layer_id: u64,
    proj: &mut FillProjection,
    changed: &mut bool,
) {
    let mut kind = match proj {
        FillProjection::Uv => 0,
        FillProjection::Triplanar { .. } => 1,
        FillProjection::Planar { .. } => 2,
        FillProjection::Spherical => 3,
        FillProjection::Cylindrical { .. } => 4,
    };
    let old_kind = kind;
    egui::ComboBox::from_id_salt(format!("fill-projection-{layer_id}"))
        .selected_text(match kind {
            0 => tr("fill.proj-uv"),
            1 => tr("fill.proj-triplanar"),
            2 => tr("fill.proj-planar"),
            3 => tr("fill.proj-spherical"),
            _ => tr("fill.proj-cylindrical"),
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut kind, 0, tr("fill.proj-uv"));
            ui.selectable_value(&mut kind, 1, tr("fill.proj-triplanar"));
            ui.selectable_value(&mut kind, 2, tr("fill.proj-planar"));
            ui.selectable_value(&mut kind, 3, tr("fill.proj-spherical"));
            ui.selectable_value(&mut kind, 4, tr("fill.proj-cylindrical"));
        });
    if kind != old_kind {
        // v1: switching projection resets its parameters to defaults (the
        // source is untouched) — per-kind state across switches is the
        // named follow-up.
        *proj = match kind {
            1 => FillProjection::Triplanar {
                sharpness: 2.0,
                scale: 1.0,
            },
            2 => FillProjection::Planar { axis: ProjAxis::Y },
            3 => FillProjection::Spherical,
            4 => FillProjection::Cylindrical { axis: ProjAxis::Y },
            _ => FillProjection::Uv,
        };
        *changed = true;
    }
    match proj {
        FillProjection::Triplanar { sharpness, scale } => {
            if ui
                .add(egui::Slider::new(sharpness, 1.0..=8.0).text(tr("fill.sharpness")))
                .changed()
            {
                *changed = true;
            }
            if ui
                .add(egui::Slider::new(scale, 0.1..=8.0).text(tr("fill.scale")))
                .changed()
            {
                *changed = true;
            }
        }
        FillProjection::Planar { axis } | FillProjection::Cylindrical { axis } => {
            axis_rows(ui, axis, changed);
        }
        FillProjection::Uv | FillProjection::Spherical => {}
    }
}

/// The axis picker shared by planar + cylindrical projections.
fn axis_rows(ui: &mut egui::Ui, axis: &mut ProjAxis, changed: &mut bool) {
    let mut picked = *axis;
    ui.horizontal_wrapped(|ui| {
        ui.strong(tr("fill.axis"));
        ui.selectable_value(&mut picked, ProjAxis::X, tr("fill.axis-x"));
        ui.selectable_value(&mut picked, ProjAxis::Y, tr("fill.axis-y"));
        ui.selectable_value(&mut picked, ProjAxis::Z, tr("fill.axis-z"));
    });
    if picked != *axis {
        *axis = picked;
        *changed = true;
    }
}

/// The v1 source editor: solid color, two-stop gradient, value noise. A
/// graph snapshot shows its dims (refreshing it from a fresh graph
/// evaluation is the named follow-up — the panel never silently re-runs
/// the graph).
fn source_rows(ui: &mut egui::Ui, layer_id: u64, source: &mut FillSource, changed: &mut bool) {
    let mut kind = match source {
        FillSource::Solid { .. } => 0,
        FillSource::Gradient { .. } => 1,
        FillSource::Noise { .. } => 2,
        FillSource::GraphImage { .. } => 3,
    };
    let old_kind = kind;
    egui::ComboBox::from_id_salt(format!("fill-source-{layer_id}"))
        .selected_text(match kind {
            0 => tr("fill.src-solid"),
            1 => tr("fill.src-gradient"),
            2 => tr("fill.src-noise"),
            _ => tr("fill.src-graph"),
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut kind, 0, tr("fill.src-solid"));
            ui.selectable_value(&mut kind, 1, tr("fill.src-gradient"));
            ui.selectable_value(&mut kind, 2, tr("fill.src-noise"));
            ui.selectable_value(&mut kind, 3, tr("fill.src-graph"));
        });
    if kind != old_kind {
        // v1: switching source resets to that kind's default (the same
        // convention as the projection picker above).
        *source = match kind {
            1 => FillSource::Gradient {
                a: [0.0, 0.0, 0.0, 1.0],
                b: [1.0, 1.0, 1.0, 1.0],
                angle: 0.0,
            },
            2 => FillSource::Noise {
                seed: 1,
                scale: 3.0,
                octaves: 4,
            },
            3 => FillSource::GraphImage {
                width: 0,
                height: 0,
                rgba: Vec::new(),
            },
            _ => FillSource::Solid {
                color: [1.0, 1.0, 1.0, 1.0],
            },
        };
        *changed = true;
    }
    match source {
        FillSource::Solid { color } => {
            let mut rgb = [color[0], color[1], color[2]];
            if ui.color_edit_button_rgb(&mut rgb).changed() {
                color[0] = rgb[0];
                color[1] = rgb[1];
                color[2] = rgb[2];
                *changed = true;
            }
            if ui
                .add(egui::Slider::new(&mut color[3], 0.0..=1.0).text(tr("fill.alpha")))
                .changed()
            {
                *changed = true;
            }
        }
        FillSource::Gradient { a, b, angle } => {
            for (label, c) in [(tr("fill.color-a"), a), (tr("fill.color-b"), b)] {
                let mut rgb = [c[0], c[1], c[2]];
                ui.horizontal(|ui| {
                    ui.label(label);
                    if ui.color_edit_button_rgb(&mut rgb).changed() {
                        c[0] = rgb[0];
                        c[1] = rgb[1];
                        c[2] = rgb[2];
                        *changed = true;
                    }
                });
            }
            if ui
                .add(egui::Slider::new(angle, 0.0..=std::f32::consts::TAU).text(tr("fill.angle")))
                .changed()
            {
                *changed = true;
            }
        }
        FillSource::Noise {
            seed,
            scale,
            octaves,
        } => {
            ui.horizontal(|ui| {
                ui.label(tr("fill.seed"));
                if ui.add(egui::DragValue::new(seed).speed(1)).changed() {
                    *changed = true;
                }
            });
            if ui
                .add(egui::Slider::new(scale, 0.1..=16.0).text(tr("fill.scale")))
                .changed()
            {
                *changed = true;
            }
            if ui
                .add(egui::Slider::new(octaves, 1..=8).text(tr("fill.octaves")))
                .changed()
            {
                *changed = true;
            }
        }
        FillSource::GraphImage {
            width,
            height,
            rgba,
        } => {
            ui.label(format!("{width} × {height} ({} bytes)", rgba.len()));
            ui.label(tr("fill.graph-note"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_brush::{MirrorAxis, MirrorSettings};
    use umber_mesh::MeshData;

    /// Quad with UVs squashed into the LEFT half ([0, 0.5] x [0, 1]):
    /// only the left half of the tile is occupied.
    fn left_half_quad() -> MeshData {
        MeshData {
            positions: vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [0.5, 0.0], [0.5, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            material_names: vec![],
        }
    }

    fn mirror_x_half() -> SymmetrySettings {
        SymmetrySettings {
            mirror: MirrorSettings {
                enabled: true,
                axis: MirrorAxis::X,
                offset: 0.5,
            },
            ..SymmetrySettings::default()
        }
    }

    fn solid_fill(color: [f32; 4]) -> FillParams {
        FillParams {
            source: FillSource::Solid { color },
            projection: FillProjection::Uv,
            mask: None,
        }
    }

    /// Test (f): the coverage symmetry — a left-half fill mirrored at
    /// U = 0.5 covers the right half too, and the mirror halves match
    /// texel-for-texel.
    #[test]
    fn fill_coverage_symmetry_mirrors_halves() {
        let raw = evaluate_fill_tile(
            &left_half_quad(),
            &solid_fill([1.0, 0.0, 0.0, 1.0]),
            1001,
            16,
            16,
        );
        // Sanity: the raw fill really is left-only.
        for y in 0..16 {
            for x in 0..16 {
                let want = if x < 8 { 1.0 } else { 0.0 };
                assert_eq!(raw.coverage[y * 16 + x], want, "raw texel ({x},{y})");
            }
        }
        let done = apply_symmetry(raw, &mirror_x_half());
        assert_eq!(done.coverage.len(), 256);
        for y in 0..16 {
            for x in 0..16usize {
                assert_eq!(
                    done.coverage[y * 16 + x],
                    1.0,
                    "symmetrized texel ({x},{y}) must be covered"
                );
                let (l, r) = (y * 16 + x, y * 16 + (15 - x));
                assert_eq!(
                    &done.rgba[l * 4..l * 4 + 4],
                    &done.rgba[r * 4..r * 4 + 4],
                    "mirror halves match at row {y}, cols {x}/{}",
                    15 - x
                );
            }
        }
        // The mirrored color is the sibling's red, not background.
        assert!(done.rgba.chunks_exact(4).all(|px| px == [255, 0, 0, 255]));
        // Symmetry off is the zero-cost path: bytes untouched.
        let raw = evaluate_fill_tile(
            &left_half_quad(),
            &solid_fill([1.0, 0.0, 0.0, 1.0]),
            1001,
            16,
            16,
        );
        let (rgba, cov) = (raw.rgba.clone(), raw.coverage.clone());
        let done = apply_symmetry(raw, &SymmetrySettings::default());
        assert_eq!(done.rgba, rgba);
        assert_eq!(done.coverage, cov);
    }

    /// Test (g): the live re-eval — changing the source color dirties the
    /// layer and the tile updates on the next evaluation (and only then).
    #[test]
    fn fill_reevaluates_live_on_source_change() {
        let mesh = left_half_quad();
        let off = SymmetrySettings::default();
        // The left-half mesh occupies columns 0..8 (any row).
        let left_is = |buf: &[u8], px: [u8; 4]| {
            buf.chunks_exact(4).enumerate().all(|(i, q)| {
                if i % 16 < 8 {
                    q == px
                } else {
                    q == [0, 0, 0, 0]
                }
            })
        };
        let mut state = FillLayerState::new(7, solid_fill([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(state.layer_id(), 7);
        assert_eq!(state.params(), &solid_fill([1.0, 0.0, 0.0, 1.0]));
        assert!(state.is_dirty(), "fresh state must evaluate once");

        let first = state.evaluate(&mesh, 1001, 16, &off).to_vec();
        assert!(!state.is_dirty(), "evaluate clears the dirty flag");
        assert!(
            left_is(&first, [255, 0, 0, 255]),
            "left half red, right empty"
        );

        // No change: the cache serves byte-identical bytes, still clean.
        let cached = state.evaluate(&mesh, 1001, 16, &off).to_vec();
        assert_eq!(cached, first);
        assert!(!state.is_dirty());

        // The source change: dirty immediately, tile updates on evaluate.
        state.set_source(FillSource::Solid {
            color: [0.0, 0.0, 1.0, 1.0],
        });
        assert!(state.is_dirty(), "a source change must dirty the layer");
        let second = state.evaluate(&mesh, 1001, 16, &off).to_vec();
        assert!(!state.is_dirty());
        assert_ne!(second, first);
        assert!(left_is(&second, [0, 0, 255, 255]), "left half blue now");

        // A no-op set (same source) does NOT dirty: no wasted re-eval.
        state.set_source(FillSource::Solid {
            color: [0.0, 0.0, 1.0, 1.0],
        });
        assert!(!state.is_dirty());

        // The document path: panel/undo edits land in Layer::fill first,
        // sync_from_layer adopts them and dirties.
        let mut stack = umber_core::LayerStack::new();
        let id = stack.add_layer("Tint", LayerKind::Fill);
        let mut tracked = FillLayerState::new(id, solid_fill([0.0, 0.0, 1.0, 1.0]));
        tracked.evaluate(&mesh, 1001, 16, &off);
        assert!(!tracked.is_dirty());
        stack.set_fill(
            id,
            FillParams {
                source: FillSource::Solid {
                    color: [0.0, 1.0, 0.0, 1.0],
                },
                projection: FillProjection::Uv,
                mask: None,
            },
        );
        tracked.sync_from_layer(stack.layer(id).unwrap());
        assert!(
            tracked.is_dirty(),
            "document-side edit must dirty the cache"
        );
        let third = tracked.evaluate(&mesh, 1001, 16, &off).to_vec();
        assert!(left_is(&third, [0, 255, 0, 255]), "left half green now");
    }

    /// Compositing: the fill honors blend mode + opacity like any layer —
    /// Normal at full opacity replaces covered texels, opacity 0 and
    /// coverage 0 keep the base byte-identical, Multiply darkens by the
    /// book, and Passthrough degrades to Normal (never a hole).
    #[test]
    fn fill_composites_like_any_layer() {
        // 2x1: texel 0 covered red, texel 1 uncovered.
        let fill = FillTile {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 0, 0],
            coverage: vec![1.0, 0.0],
        };
        let base_of = |px: [u8; 4]| vec![px[0], px[1], px[2], px[3], px[0], px[1], px[2], px[3]];

        // Normal, opacity 1: covered texel becomes the fill exactly.
        let mut base = base_of([10, 128, 200, 255]);
        composite_fill_over(&mut base, &fill, BlendMode::Normal, 1.0);
        assert_eq!(&base[0..4], &[255, 0, 0, 255]);
        assert_eq!(&base[4..8], &[10, 128, 200, 255], "uncovered untouched");

        // Opacity 0: the base is byte-identical everywhere.
        let mut base = base_of([10, 128, 200, 255]);
        composite_fill_over(&mut base, &fill, BlendMode::Normal, 0.0);
        assert_eq!(base, base_of([10, 128, 200, 255]));

        // Multiply by the book: 1.0 * dst = dst for red-on-gray's R,
        // 0.0 * dst = 0 for G/B.
        let mut base = base_of([100, 150, 200, 255]);
        composite_fill_over(&mut base, &fill, BlendMode::Multiply, 1.0);
        assert_eq!(&base[0..4], &[100, 0, 0, 255]);
        assert_eq!(&base[4..8], &[100, 150, 200, 255]);

        // Half opacity mixes halfway (10 -> (255+10)/2 = 132.5 -> 132/133
        // by rounding; assert the band, not the byte — encoders differ).
        let mut base = base_of([10, 128, 200, 255]);
        composite_fill_over(&mut base, &fill, BlendMode::Normal, 0.5);
        assert!(
            (120..=140).contains(&base[0]),
            "R mixes halfway: {}",
            base[0]
        );
        assert_eq!(&base[4..8], &[10, 128, 200, 255]);

        // Passthrough is Normal for a fill's own step, never a hole.
        let mut base = base_of([10, 128, 200, 255]);
        composite_fill_over(&mut base, &fill, BlendMode::Passthrough, 1.0);
        assert_eq!(&base[0..4], &[255, 0, 0, 255]);

        // Partial coverage scales the alpha: coverage 0.5 at opacity 1
        // over transparent black yields half-alpha fill color.
        let half = FillTile {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
            coverage: vec![0.5],
        };
        let mut base = vec![0, 0, 0, 0];
        composite_fill_over(&mut base, &half, BlendMode::Normal, 1.0);
        assert_eq!(&base[0..3], &[255, 0, 0]);
        assert!((126..=130).contains(&base[3]), "alpha ~= 128: {}", base[3]);
    }

    /// Helper: a session mask allowing exactly the left half (one hard
    /// dab per texel — texel centers stamp to exactly 1.0, the right
    /// half stays at the empty 0.0).
    fn left_half_mask() -> QuickMask {
        use umber_brush::retouch::RetouchDab;
        let mut mask = QuickMask::new(16, 16);
        mask.set_active(true);
        for y in 0..16u32 {
            for x in 0..8u32 {
                mask.stamp(
                    1001,
                    &RetouchDab {
                        pos: [x as f32 + 0.5, y as f32 + 0.5],
                        radius: 0.6,
                        alpha: 1.0,
                        hardness: 1.0,
                    },
                );
            }
        }
        mask
    }

    /// The quick-mask gate on fills: an inactive mask passes the tile
    /// through byte-identical; an active mask multiplies the post-symmetry
    /// coverage per texel and zeroes texels gated to zero.
    #[test]
    fn quick_mask_gates_fill_coverage_per_texel() {
        let mesh = left_half_quad();
        let raw = evaluate_fill_tile(&mesh, &solid_fill([1.0, 0.0, 0.0, 1.0]), 1001, 16, 16);
        // Inactive: untouched (the zero-cost path, like symmetry off).
        let idle = QuickMask::new(16, 16);
        let done = apply_quick_mask(
            FillTile {
                width: 16,
                height: 16,
                rgba: raw.rgba.clone(),
                coverage: raw.coverage.clone(),
            },
            &idle,
            1001,
        );
        assert_eq!(done.rgba, raw.rgba);
        assert_eq!(done.coverage, raw.coverage);

        // Active left-half mask over the left-half fill: everything stays.
        let gated = apply_quick_mask(
            FillTile {
                width: 16,
                height: 16,
                rgba: raw.rgba.clone(),
                coverage: raw.coverage.clone(),
            },
            &left_half_mask(),
            1001,
        );
        assert_eq!(gated.rgba, raw.rgba);
        assert_eq!(gated.coverage, raw.coverage);

        // Same mask over a FULL-quad fill: the right half goes transparent.
        let full = umber_mesh::MeshData {
            positions: vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            material_names: vec![],
        };
        let raw = evaluate_fill_tile(&full, &solid_fill([1.0, 0.0, 0.0, 1.0]), 1001, 16, 16);
        let gated = apply_quick_mask(
            FillTile {
                width: 16,
                height: 16,
                rgba: raw.rgba,
                coverage: raw.coverage,
            },
            &left_half_mask(),
            1001,
        );
        for y in 0..16 {
            for x in 0..16 {
                let i = y * 16 + x;
                if x < 8 {
                    assert_eq!(gated.coverage[i], 1.0);
                    assert_eq!(&gated.rgba[i * 4..i * 4 + 4], &[255, 0, 0, 255]);
                } else {
                    assert_eq!(gated.coverage[i], 0.0);
                    assert_eq!(&gated.rgba[i * 4..i * 4 + 4], &[0, 0, 0, 0]);
                }
            }
        }

        // Size mismatch passes through (a resize is in flight; the next
        // refresh re-evaluates) — never blocks the world on a torn gate.
        let wide = QuickMask::new(32, 16);
        let raw = evaluate_fill_tile(&full, &solid_fill([1.0, 0.0, 0.0, 1.0]), 1001, 16, 16);
        let (rgba, cov) = (raw.rgba.clone(), raw.coverage.clone());
        let done = apply_quick_mask(
            FillTile {
                width: 16,
                height: 16,
                rgba,
                coverage: cov.clone(),
            },
            &wide,
            1001,
        );
        assert_eq!(done.coverage, cov);
    }

    /// The full stack: polygon mask → symmetry → quick mask → composite.
    /// A top-half lasso over the left-half mesh, mirrored at U = 0.5,
    /// gated by a left-half quick mask, composited Normal over gray:
    /// only the top-left quadrant turns red.
    #[test]
    fn masked_fill_composes_with_symmetry_and_quick_mask() {
        let mesh = left_half_quad();
        let lasso = FillMask::polygon(vec![[0.0, 0.5], [1.0, 0.5], [1.0, 1.0], [0.0, 1.0]]);
        let params = FillParams {
            source: FillSource::Solid {
                color: [1.0, 0.0, 0.0, 1.0],
            },
            projection: FillProjection::Uv,
            mask: Some(lasso),
        };
        let raw = evaluate_fill_tile(&mesh, &params, 1001, 16, 16);
        // Sanity: the masked raw fill is the top-left quadrant only
        // (left-half mesh ∩ top-half lasso).
        for y in 0..16 {
            for x in 0..16 {
                let want = if x < 8 && y < 8 { 1.0 } else { 0.0 };
                assert_eq!(raw.coverage[y * 16 + x], want, "raw ({x},{y})");
            }
        }
        let sym = apply_symmetry(raw, &mirror_x_half());
        // Mirrored: the whole top half (both left and right).
        for y in 0..16 {
            for x in 0..16 {
                let want = if y < 8 { 1.0 } else { 0.0 };
                assert_eq!(sym.coverage[y * 16 + x], want, "sym ({x},{y})");
            }
        }
        let gated = apply_quick_mask(sym, &left_half_mask(), 1001);
        // Gated: back to the top-left quadrant.
        for y in 0..16 {
            for x in 0..16 {
                let want = if x < 8 && y < 8 { 1.0 } else { 0.0 };
                assert_eq!(gated.coverage[y * 16 + x], want, "gated ({x},{y})");
            }
        }
        let mut base = vec![128u8; 16 * 16 * 4];
        composite_fill_over(&mut base, &gated, BlendMode::Normal, 1.0);
        for y in 0..16 {
            for x in 0..16 {
                let px = &base[(y * 16 + x) * 4..][..4];
                if x < 8 && y < 8 {
                    assert_eq!(px, &[255, 0, 0, 255], "composite ({x},{y})");
                } else {
                    assert_eq!(px, &[128, 128, 128, 128], "composite ({x},{y})");
                }
            }
        }
    }

    /// The lasso tool: disarmed capture refuses points, the armed path
    /// records/clears/undoes, Apply writes one journaled op preserving
    /// source+projection, and misses/non-fill targets are no-ops.
    #[test]
    fn polygon_tool_capture_build_and_apply() {
        let mesh = left_half_quad();
        let mut doc = Document::default();
        if doc.layers().iter().all(|l| l.kind != LayerKind::Fill) {
            doc.run(LayerCommand::add("Fill 1", LayerKind::Fill));
        }
        let fill_id = doc
            .layers()
            .iter()
            .find(|l| l.kind == LayerKind::Fill)
            .map(|l| l.id)
            .expect("a fill layer exists");

        let mut tool = PolygonFillTool::default();
        // The mode labels match the panel's literal call sites (the i18n
        // completeness scan only sees literals).
        assert_eq!(
            [
                PolygonFillMode::Polygon.label_key(),
                PolygonFillMode::Triangle.label_key(),
                PolygonFillMode::Mesh.label_key(),
                PolygonFillMode::UvChunk.label_key(),
            ],
            [
                "fill.poly-mode-polygon",
                "fill.poly-mode-triangle",
                "fill.poly-mode-mesh",
                "fill.poly-mode-uv-chunk",
            ]
        );
        assert!(!tool.push_point([0.25, 0.25]), "disarmed: no capture");
        assert!(!tool.set_pick([0.25, 0.5]), "disarmed: no pick");
        assert!(!tool.can_apply());
        assert!(tool.build_mask(&mesh).is_none());

        tool.set_armed(true);
        assert!(
            !tool.set_pick([0.25, 0.5]),
            "polygon mode takes paths, not picks"
        );
        assert!(tool.push_point([0.0, 0.0]));
        assert!(tool.push_point([0.5, 0.0]));
        assert!(
            !tool.push_point([f32::NAN, 0.0]),
            "non-finite never records"
        );
        assert_eq!(tool.point_count(), 2);
        assert!(!tool.can_apply(), "two points are no polygon");
        assert!(tool.push_point([0.5, 1.0]));
        assert!(tool.push_point([0.0, 1.0]));
        assert!(tool.undo_point());
        assert_eq!(tool.point_count(), 3);
        assert!(tool.push_point([0.0, 1.0]));
        // The target auto-select is the panel's job; the tool needs one.
        tool.set_target(Some(fill_id));
        assert!(tool.can_apply());
        let mask = tool.build_mask(&mesh).expect("three points build");
        assert!(
            matches!(mask, FillMask::Polygon { .. }),
            "lasso builds the polygon mask: {mask:?}"
        );
        assert!(tool.apply_to(&mut doc, &mesh), "apply dispatches");
        assert!(!tool.has_capture(), "apply closes the lasso");
        let params = doc
            .layers()
            .iter()
            .find(|l| l.id == fill_id)
            .and_then(|l| l.fill.clone())
            .expect("target keeps its fill params");
        assert_eq!(params.mask, Some(mask));
        assert!(
            matches!(params.source, FillSource::Solid { .. }),
            "source preserved across the mask write"
        );

        // Single-pick modes resolve through the UV pick + floods.
        tool.set_mode(PolygonFillMode::Triangle);
        assert!(tool.set_pick([0.25, 0.5]));
        let pick = tool.pick_triangle(&mesh).expect("left-half pick hits");
        assert_eq!(
            tool.build_mask(&mesh),
            Some(FillMask::triangle(pick)),
            "triangle builds the picked face"
        );
        tool.set_mode(PolygonFillMode::Mesh);
        assert!(tool.set_pick([0.25, 0.5]));
        assert_eq!(
            tool.build_mask(&mesh),
            Some(FillMask::mesh_island(&mesh, pick))
        );
        tool.set_mode(PolygonFillMode::UvChunk);
        assert!(tool.set_pick([0.25, 0.5]));
        assert_eq!(
            tool.build_mask(&mesh),
            Some(FillMask::uv_chunk(&mesh, pick))
        );
        // Mode switches drop the in-progress capture.
        tool.set_mode(PolygonFillMode::Polygon);
        assert!(!tool.has_capture());

        // Misses and non-fill targets are no-ops, never stray writes.
        tool.set_mode(PolygonFillMode::Triangle);
        assert!(
            tool.set_pick([0.9, 0.9]),
            "recorded (right half is empty mesh)"
        );
        assert_eq!(tool.pick_triangle(&mesh), None, "but it misses");
        assert!(tool.build_mask(&mesh).is_none());
        assert!(!tool.apply_to(&mut doc, &mesh));
        let paint_id = {
            doc.run(LayerCommand::add("Paint 1", LayerKind::Paint));
            doc.layers().last().map(|l| l.id).unwrap()
        };
        tool.set_target(Some(paint_id));
        assert!(tool.set_pick([0.25, 0.5]));
        assert!(!tool.apply_to(&mut doc, &mesh), "paint layers take no mask");
        assert!(doc
            .layers()
            .iter()
            .find(|l| l.id == paint_id)
            .and_then(|l| l.fill.clone())
            .is_none());
    }
}
