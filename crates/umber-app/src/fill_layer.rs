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
//! 4. [`PaintState::publish_plane`](crate::paint_state::PaintState::publish_plane)
//!    — the tile/texture-set plumbing: the premultiplied bytes ride a
//!    whole-plane `WriteRegion` into `(Layer(id), tile, "baseColor")`'s
//!    own plane. The layer's blend/opacity apply at COMPOSITE time (the
//!    flush folds the stack in order) — never baked into the plane, so
//!    refreshes replace instead of compounding and hides un-publish.
//!
//! The property rows ([`fill_section_ui`]) edit through [`Document::run`](crate::document::Document::run)
//! with [`LayerCommand::SetFill`](umber_core::LayerCommand::SetFill), so
//! every fill edit is undoable and journaled like any other layer op.

use std::collections::HashMap;

use umber_brush::symmetry::{
    symmetrize_coverage, texel_transforms_rect, SymTransform, SymmetrySettings,
};
use umber_brush::QuickMask;
use umber_core::{Layer, LayerCommand, LayerKind};
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
    /// The tile width this was evaluated at.
    pub width: u32,
    /// The tile height this was evaluated at.
    pub height: u32,
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
    /// The layer's blend + opacity, adopted per frame for the entry
    /// builder's future use (today the shell reads them off the document
    /// layer straight into the composite snapshot — blend/opacity apply
    /// at composite time, never baked into the plane); set by
    /// [`FillLayerStates::ensure`] from the live layer, updated on every
    /// ensure (the panel re-sees the layer each frame, so blend edits
    /// propagate without a second path).
    blend: umber_core::BlendMode,
    opacity: f32,
    dirty: bool,
    cache: HashMap<u16, CachedTile>,
    /// Successful raster evaluations since construction. Cache hits and
    /// budget-only republishes never increment it.
    evaluation_count: u64,
    /// The plane-budget refusal that blocked the last refresh, if any
    /// (§1.3): the total `plane_bytes()` the materialization would have
    /// reached. The layer stays dirty but leaves the queue — it retries
    /// only when `plane_bytes()` falls below the refusal watermark or its
    /// params change (see [`FillLayerStates::retry_budget_freed`]). The row
    /// badge shows it (the Layers panel reads this through the shell).
    budget_blocked: Option<u64>,
    /// `plane_bytes()` when the last budget refusal happened. Unlike
    /// `budget_blocked` (the prospective total), this is the watermark a
    /// later byte drop must cross before retrying.
    budget_retry_watermark: Option<u64>,
    /// The hard plane cap at refusal. A cap increase is a retry signal too.
    budget_retry_cap: Option<u64>,
}

impl FillLayerState {
    /// The tracked layer's blend + opacity (the composite-aware publish).
    pub fn layer_blend_opacity(&self) -> (umber_core::BlendMode, f32) {
        (self.blend, self.opacity)
    }

    /// Tracks `layer_id` with `params`; starts dirty (nothing evaluated
    /// yet — the first [`refresh_fill_layer`] fills the cache).
    pub fn new(layer_id: u64, params: FillParams) -> Self {
        Self {
            layer_id,
            params,
            last_symmetry: SymmetrySettings::default(),
            blend: umber_core::BlendMode::default(),
            opacity: 1.0,
            dirty: true,
            cache: HashMap::new(),
            evaluation_count: 0,
            budget_blocked: None,
            budget_retry_watermark: None,
            budget_retry_cap: None,
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

    /// Successful raster evaluations since construction. Tests use this to
    /// distinguish cache hits and budget-only republishes from real work.
    pub fn evaluation_count(&self) -> u64 {
        self.evaluation_count
    }

    /// The plane-budget refusal blocking this layer, if any (see the
    /// `budget_blocked` field docs).
    pub fn budget_blocked(&self) -> Option<u64> {
        self.budget_blocked
    }

    /// Marks the cache stale (the source-change hook: a fill layer
    /// re-evaluates when its source changes — it is live, not baked).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.cache.clear();
        self.budget_blocked = None;
        self.budget_retry_watermark = None;
        self.budget_retry_cap = None;
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

    /// The fill's publish target (the channel-map kill): read up front so
    /// the staging loop borrows nothing while evaluating.
    pub fn channel(&self) -> &str {
        &self.params.channel
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

    /// Evaluates `tile` at `width`x`height` under `symmetry`, returning the
    /// cached post-symmetry bytes. Re-runs the projection when dirty, when
    /// the tile was never evaluated, when either dimension changed, or when
    /// symmetry changed since the last evaluation; otherwise returns the
    /// cache untouched (the live-re-eval contract: identical parameters
    /// never recompute).
    pub fn evaluate(
        &mut self,
        mesh: &MeshData,
        tile: u16,
        width: u32,
        height: u32,
        symmetry: &SymmetrySettings,
    ) -> &[u8] {
        let stale = self.dirty
            || *symmetry != self.last_symmetry
            || self
                .cache
                .get(&tile)
                .is_none_or(|c| c.width != width || c.height != height);
        if stale {
            let raw = evaluate_fill_tile(mesh, &self.params, tile, width, height);
            let done = apply_symmetry(raw, symmetry);
            self.cache.insert(
                tile,
                CachedTile {
                    width,
                    height,
                    rgba: done.rgba,
                    coverage: done.coverage,
                },
            );
            self.last_symmetry = *symmetry;
            self.dirty = false;
            self.evaluation_count += 1;
        }
        // Inserted above when stale; present with matching dimensions otherwise.
        self.cache
            .get(&tile)
            .map(|c| c.rgba.as_slice())
            .unwrap_or(&[])
    }

    /// The cached coverage plane for `tile` at `width`x`height`, or `None`
    /// when the tile was never evaluated at those dimensions (the refresh
    /// path falls back to ungated in that case — which only happens when
    /// [`Self::evaluate`] was never called, since it inserts on stale).
    pub fn coverage_of(&self, tile: u16, width: u32, height: u32) -> Option<&[f32]> {
        self.cache
            .get(&tile)
            .filter(|c| c.width == width && c.height == height)
            .map(|c| c.coverage.as_slice())
    }
}

/// Applies the session quick mask to an evaluated tile: the post-symmetry
/// coverage is multiplied by the mask at each texel — AFTER symmetry
/// expansion, at the sibling's own texels, exactly like the retouch
/// consumers (`umber_paint::retouch::RetouchStroke::apply_masked`).
///
/// Ungated (`!mask.gates()`) or size-mismatched tiles pass through
/// untouched: both planes track the paint target's size and resize resets
/// both, so a mismatch means a resize is in flight and the next refresh
/// re-evaluates — blocking the world on a torn gate would be worse.
/// Untouched mask planes read the mask's empty value (an active empty
/// mask blocks everything, an inverted one allows it), and texels gated
/// to exactly zero go transparent black so the full-tile `WriteRegion`
/// upload erases them on the GPU path too. Fractional coverage keeps its
/// sampled color for the premultiplied fold below.
#[allow(
    dead_code,
    reason = "reference mask path used by the single-pass byte-equivalence test"
)]
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
        if !out.coverage[i].is_finite() || out.coverage[i] <= 0.0 {
            continue;
        }
        let m = mask
            .sample(tile_id, (i % mw as usize) as u32, (i / mw as usize) as u32)
            .clamp(0.0, 1.0);
        out.coverage[i] *= m;
        if !out.coverage[i].is_finite() || out.coverage[i] <= 0.0 {
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
/// Empty or size-mismatched grids also return untouched: there is no complete
/// texel grid to mirror, and indexing one would panic the UI thread.
pub fn apply_symmetry(tile: FillTile, symmetry: &SymmetrySettings) -> FillTile {
    if !symmetry.is_active() {
        return tile;
    }
    let (width, height) = (tile.width, tile.height);
    let texels = (width as usize).saturating_mul(height as usize);
    if texels == 0 || tile.coverage.len() != texels || tile.rgba.len() != texels * 4 {
        return tile;
    }
    let transforms = texel_transforms_rect(symmetry, width as f32, height as f32);
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

/// Folds one evaluated fill tile into premultiplied plane bytes (§5.1):
/// texel = `(c · α, α)` with `α = sample_a · coverage`, quantized by
/// `f32_to_u8`. Coverage is FOLDED IN — this retires the fast path's
/// silent drop (which published the sampled bytes and discarded
/// fractional quick-mask coverage). The layer's blend/opacity are NOT
/// applied here (they ride the composite snapshot); a `Passthrough` fill
/// composites as `Normal` at read time.
///
/// Length contract: folds the shared texel prefix, never a panic (tile
/// resizes mid-session reorder evaluations — they must not crash the
/// refresh).
#[allow(
    dead_code,
    reason = "reference fold path used by the single-pass byte-equivalence test"
)]
pub fn fold_coverage_to_premultiplied(rgba: &[u8], coverage: &[f32]) -> Vec<u8> {
    use umber_gpu::f32_to_u8;
    let n = (rgba.len() / 4).min(coverage.len());
    let mut out = Vec::with_capacity(n * 4);
    for i in 0..n {
        let cov = if coverage[i].is_finite() {
            coverage[i].clamp(0.0, 1.0)
        } else {
            0.0
        };
        let alpha = f32::from(rgba[i * 4 + 3]) / 255.0 * cov;
        out.push(f32_to_u8(f32::from(rgba[i * 4]) / 255.0 * alpha));
        out.push(f32_to_u8(f32::from(rgba[i * 4 + 1]) / 255.0 * alpha));
        out.push(f32_to_u8(f32::from(rgba[i * 4 + 2]) / 255.0 * alpha));
        out.push(f32_to_u8(alpha));
    }
    out
}

/// Single-pass quick-mask + premultiply used by vector refresh. Byte-identical
/// to `apply_quick_mask` followed by `fold_coverage_to_premultiplied`, but
/// borrows the cached tile and allocates only the final plane.
fn gate_and_fold(
    rgba: &[u8],
    coverage: &[f32],
    width: u32,
    height: u32,
    mask: &QuickMask,
    tile_id: u16,
) -> Vec<u8> {
    use umber_gpu::f32_to_u8;
    let n = (rgba.len() / 4).min(coverage.len());
    let gated = mask.gates() && mask.dimensions() == (width, height);
    let mut out = Vec::with_capacity(n * 4);
    for i in 0..n {
        let mut cov = if coverage[i].is_finite() && coverage[i] > 0.0 {
            coverage[i]
        } else {
            0.0
        };
        if gated && cov > 0.0 {
            cov *= mask
                .sample(
                    tile_id,
                    (i % width as usize) as u32,
                    (i / width as usize) as u32,
                )
                .clamp(0.0, 1.0);
        }
        let cov = if cov.is_finite() {
            cov.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let alpha = f32::from(rgba[i * 4 + 3]) / 255.0 * cov;
        out.push(f32_to_u8(f32::from(rgba[i * 4]) / 255.0 * alpha));
        out.push(f32_to_u8(f32::from(rgba[i * 4 + 1]) / 255.0 * alpha));
        out.push(f32_to_u8(f32::from(rgba[i * 4 + 2]) / 255.0 * alpha));
        out.push(f32_to_u8(alpha));
    }
    out
}

/// Re-evaluates `state` for every tile in `tiles` — each at ITS
/// tile/channel size ([m8]: tiles belong to different sets, so the one
/// `size` parameter is retired) — gates each through the session quick
/// mask ([`apply_quick_mask`]), folds coverage into premultiplied plane
/// bytes ([`fold_coverage_to_premultiplied`]), and publishes each into
/// `(Layer(state), tile, "baseColor")`'s own plane (a refresh REPLACES
/// the plane whole: re-eval is idempotent, hides un-publish).
///
/// Call after any fill-param edit (panel, undo, redo), after mesh load,
/// and after a target resize (a size change invalidates the cache inside
/// [`FillLayerState::evaluate`]). Returns the tiles actually staged.
/// A plane-budget refusal records `budget_blocked` and the refusal cap on
/// the state and stages nothing further on that tile. The cached grid stays
/// valid: retrying after bytes are freed or the cap rises republishes it
/// without re-rasterizing.
/// torn uploads drop loudly with a log, like the fill precedent.
pub fn refresh_fill_layer(
    state: &mut FillLayerState,
    mesh: &MeshData,
    paint: &mut PaintState,
    tiles: &[u16],
    symmetry: &SymmetrySettings,
) -> Vec<u16> {
    use crate::paint_state::PlaneOwner;
    let layer_id = state.layer_id();
    let mut staged = Vec::new();
    // The fill's publish target (the channel-map kill): the raster is
    // channel-blind, only this target moves. A channel the set lacks
    // falls to the zero-size guard below and stages nothing.
    let channel = state.channel().to_owned();
    let mut needs_retry = false;
    for &tile in tiles {
        let (width, height) = paint.tile_channel_size(tile, &channel);
        if width == 0 || height == 0 {
            needs_retry = true;
            continue;
        }
        let rgba = state.evaluate(mesh, tile, width, height, symmetry).to_vec();
        let coverage = state
            .coverage_of(tile, width, height)
            .unwrap_or(&[])
            .to_vec();
        let mut plane = gate_and_fold(&rgba, &coverage, width, height, paint.quick_mask(), tile);
        let expected_len = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        if plane.len() != expected_len {
            if rgba.is_empty() && coverage.is_empty() {
                let owned = paint
                    .planes_present()
                    .iter()
                    .any(|(owner, t, present_channel)| {
                        *owner == PlaneOwner::Layer(layer_id)
                            && *t == tile
                            && present_channel == &channel
                    });
                if owned {
                    plane = vec![0; expected_len];
                } else {
                    // Nothing was published on this tile, so a prior budget
                    // refusal and its retry watermark stay intact.
                    continue;
                }
            } else {
                needs_retry = true;
                log::warn!("fill layer {layer_id}: incomplete raster grid, retrying");
                continue;
            }
        }
        match paint.publish_plane(
            PlaneOwner::Layer(layer_id),
            tile,
            &channel,
            width,
            height,
            plane,
        ) {
            Ok(()) => {
                state.budget_blocked = None;
                state.budget_retry_watermark = None;
                staged.push(tile);
            }
            Err(crate::paint_state::PaintRefusal::PlaneBudget { bytes }) => {
                state.budget_blocked = Some(bytes);
                state.budget_retry_watermark = Some(paint.plane_bytes());
                state.budget_retry_cap = Some(paint.plane_hard_cap_bytes());
                // A budget refusal needs REPUBLISH, not re-rasterization: the
                // cached grid stays valid and is reused when bytes are freed.
                log::warn!("fill layer {layer_id}: plane budget refused ({bytes} bytes)");
                break;
            }
            Err(refusal) => {
                needs_retry = true;
                log::warn!("fill layer {layer_id}: publish refused ({refusal:?})");
            }
        }
    }
    // An empty tile list must preserve dirty: nothing could be staged, so
    // the layer still owes its first valid refresh. Budget refusal does not
    // force a re-raster because the cached grid remains valid.
    state.dirty = needs_retry || (staged.is_empty() && tiles.is_empty());
    staged
}

/// CPU cache ceiling per vector layer. The just-used tile is retained even
/// when it alone exceeds the cap; every older tile is evicted first.
const MAX_VECTOR_CACHE_BYTES_PER_LAYER: usize = 512 * 1024 * 1024;
/// Document-wide CPU cache ceiling. The active layer is the final eviction
/// candidate, so one just-used tile may exceed the global cap.
const MAX_VECTOR_CACHE_BYTES_TOTAL: usize = 1024 * 1024 * 1024;

/// One vector tile's cache entry. Unlike Fill's square-only cache this
/// records both dimensions because project-loaded channels may be non-square.
struct VectorCachedTile {
    generation: u64,
    symmetry: SymmetrySettings,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    coverage: Vec<f32>,
}

/// A live vector layer's eval cache (K2): `Layer.vector` is authoritative;
/// this state mirrors it and recomputes only when shape, size, or symmetry
/// changes. Raster errors degrade to transparent with a loud log, never a
/// panic in the UI thread.
pub struct VectorLayerState {
    layer_id: u64,
    params: umber_core::vector::VectorParams,
    generation: u64,
    dirty: bool,
    cache: HashMap<u16, VectorCachedTile>,
    cache_cap_bytes: usize,
    last_used: u64,
    evaluation_count: u64,
    refusal: Option<umber_core::vector::VectorError>,
    budget_blocked: Option<u64>,
    budget_retry_watermark: Option<u64>,
}

impl VectorLayerState {
    pub fn new(layer_id: u64, params: umber_core::vector::VectorParams) -> Self {
        Self {
            layer_id,
            params,
            generation: 0,
            dirty: true,
            cache: HashMap::new(),
            cache_cap_bytes: MAX_VECTOR_CACHE_BYTES_PER_LAYER,
            last_used: 0,
            evaluation_count: 0,
            refusal: None,
            budget_blocked: None,
            budget_retry_watermark: None,
        }
    }

    pub fn layer_id(&self) -> u64 {
        self.layer_id
    }

    pub fn params(&self) -> &umber_core::vector::VectorParams {
        &self.params
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn evaluation_count(&self) -> u64 {
        self.evaluation_count
    }

    pub fn refusal(&self) -> Option<&umber_core::vector::VectorError> {
        self.refusal.as_ref()
    }

    /// Straight RGBA + f32 coverage bytes retained by this layer.
    pub fn cache_bytes(&self) -> usize {
        self.cache
            .values()
            .map(|tile| tile.rgba.len() + tile.coverage.len() * std::mem::size_of::<f32>())
            .sum()
    }

    fn enforce_cache_budget(&mut self, keep: u16) {
        let mut candidates: Vec<u16> = self
            .cache
            .keys()
            .copied()
            .filter(|tile| *tile != keep)
            .collect();
        candidates.sort_unstable();
        for tile in candidates {
            if self.cache_bytes() <= self.cache_cap_bytes || self.cache.len() <= 1 {
                break;
            }
            self.cache.remove(&tile);
        }
    }

    pub fn budget_blocked(&self) -> Option<u64> {
        self.budget_blocked
    }

    pub fn mark_dirty(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.dirty = true;
        self.refusal = None;
        self.cache.clear();
    }

    pub fn set_params(&mut self, params: umber_core::vector::VectorParams) {
        if !self.params.same_cache_key(&params) {
            self.params = params;
            self.mark_dirty();
        }
    }

    /// Evaluates one tile, reusing the cached post-symmetry bytes when the
    /// complete cache key is unchanged.
    pub fn evaluate(
        &mut self,
        tile: u16,
        width: u32,
        height: u32,
        symmetry: &SymmetrySettings,
    ) -> &[u8] {
        let stale = self.cache.get(&tile).is_none_or(|c| {
            c.generation != self.generation
                || !c.symmetry.same_cache_key(symmetry)
                || c.width != width
                || c.height != height
        });
        if stale {
            let (rgba, coverage) =
                match umber_core::vector::rasterize(&self.params.shape, width, height) {
                    Ok(raster) => {
                        self.refusal = None;
                        raster.into_rgba_coverage()
                    }
                    Err(err) => {
                        let layer = self.layer_id;
                        log::warn!("vector layer {layer}: raster refused: {err}");
                        self.refusal = Some(err);
                        (Vec::new(), Vec::new())
                    }
                };
            let done = apply_symmetry(
                FillTile {
                    width,
                    height,
                    rgba,
                    coverage,
                },
                symmetry,
            );
            self.cache.insert(
                tile,
                VectorCachedTile {
                    generation: self.generation,
                    symmetry: *symmetry,
                    width,
                    height,
                    rgba: done.rgba,
                    coverage: done.coverage,
                },
            );
            self.enforce_cache_budget(tile);
            self.evaluation_count += 1;
        }
        self.dirty = false;
        self.cache
            .get(&tile)
            .map(|c| c.rgba.as_slice())
            .unwrap_or(&[])
    }

    pub fn coverage_of(&self, tile: u16, width: u32, height: u32) -> Option<&[f32]> {
        self.cache
            .get(&tile)
            .filter(|c| c.width == width && c.height == height)
            .map(|c| c.coverage.as_slice())
    }
}

/// Evaluates and publishes every requested tile of a vector layer. No mesh
/// is read: the shape is already in tile-local UV. The publish path is the
/// Fill pipeline verbatim, including quick-mask and premultiplication. Empty
/// and refused rasters erase already-owned planes without allocating new ones.
pub fn refresh_vector_layer(
    state: &mut VectorLayerState,
    paint: &mut PaintState,
    tiles: &[u16],
    symmetry: &SymmetrySettings,
) -> Vec<u16> {
    let layer_id = state.layer_id();
    let mut staged = Vec::new();
    let mut needs_retry = false;
    let mut refresh_refusal = None;
    for &tile in tiles {
        let (width, height) = paint.tile_channel_size(tile, crate::paint_state::BASE_COLOR_CHANNEL);
        if width == 0 || height == 0 {
            needs_retry = true;
            continue;
        }
        state.evaluate(tile, width, height, symmetry);
        if let Some(error) = state.refusal.clone() {
            refresh_refusal = Some(error);
        }
        // A refused raster has an empty cached grid. Keep flowing through the
        // common empty-grid path so a valid-to-invalid edit erases any plane
        // this layer already owns instead of leaving stale art on screen.
        let Some(cached) = state.cache.get(&tile) else {
            needs_retry = true;
            continue;
        };
        let (rgba, coverage) = (cached.rgba.as_slice(), cached.coverage.as_slice());
        let expected_len = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        let mut plane = gate_and_fold(rgba, coverage, width, height, paint.quick_mask(), tile);
        if plane.len() != expected_len {
            if rgba.is_empty() && coverage.is_empty() {
                let owned = paint.planes_present().iter().any(|(owner, t, channel)| {
                    *owner == crate::paint_state::PlaneOwner::Layer(layer_id)
                        && *t == tile
                        && channel == crate::paint_state::BASE_COLOR_CHANNEL
                });
                if owned {
                    plane = vec![0; expected_len];
                } else {
                    state.budget_blocked = None;
                    state.budget_retry_watermark = None;
                    continue;
                }
            } else {
                needs_retry = true;
                log::warn!("vector layer {layer_id}: incomplete raster grid, retrying");
                continue;
            }
        }
        match paint.publish_plane(
            crate::paint_state::PlaneOwner::Layer(layer_id),
            tile,
            crate::paint_state::BASE_COLOR_CHANNEL,
            width,
            height,
            plane,
        ) {
            Ok(()) => {
                state.budget_blocked = None;
                state.budget_retry_watermark = None;
                staged.push(tile);
            }
            Err(crate::paint_state::PaintRefusal::PlaneBudget { bytes }) => {
                state.budget_blocked = Some(bytes);
                state.budget_retry_watermark = Some(paint.plane_bytes());
                needs_retry = true;
                log::warn!("vector layer {layer_id}: plane budget refused ({bytes} bytes)");
                break;
            }
            Err(refusal) => {
                needs_retry = true;
                log::warn!("vector layer {layer_id}: publish refused ({refusal:?})");
            }
        }
    }
    // A later valid tile must not hide an earlier tile's refusal. The next
    // complete pass starts a fresh aggregate, so a genuine recovery clears it.
    state.refusal = refresh_refusal;
    state.dirty = needs_retry;
    staged
}

/// Whether every active tile on `channel` has this Fill layer's restored
/// plane. A partial restore is planeless for refresh purposes: seeding the
/// whole layer clean would suppress every missing tile until an unrelated edit.
pub fn fill_planes_complete(
    present: &[(crate::paint_state::PlaneOwner, u16, String)],
    layer: u64,
    channel: &str,
    tiles: &[u16],
) -> bool {
    !tiles.is_empty()
        && tiles.iter().all(|tile| {
            present
                .iter()
                .any(|(owner, present_tile, present_channel)| {
                    *owner == crate::paint_state::PlaneOwner::Layer(layer)
                        && *present_tile == *tile
                        && present_channel == channel
                })
        })
}

/// Whether every active BaseColor tile has this layer's restored plane.
/// A partial restore is planeless for refresh purposes: seeding the whole
/// layer clean would suppress every missing tile until an unrelated edit.
pub fn vector_planes_complete(
    present: &[(crate::paint_state::PlaneOwner, u16, String)],
    layer: u64,
    tiles: &[u16],
) -> bool {
    tiles.iter().all(|tile| {
        present.iter().any(|(owner, present_tile, channel)| {
            *owner == crate::paint_state::PlaneOwner::Layer(layer)
                && *present_tile == *tile
                && channel == crate::paint_state::BASE_COLOR_CHANNEL
        })
    })
}

/// The session's vector-layer caches and dirty queue (K2), kept beside
/// Fill's because both share the same document and publish lifecycle.
#[derive(Default)]
pub struct VectorLayerStates {
    pub map: HashMap<u64, VectorLayerState>,
    pub dirty: Vec<u64>,
    clock: u64,
    last_symmetry: Option<SymmetrySettings>,
    last_size: Option<(u32, u32)>,
}

impl VectorLayerStates {
    /// Adopts the document's current vector parameters. First sight or a
    /// shape change queues exactly one refresh; an unchanged ensure is inert.
    pub fn ensure(&mut self, layer: u64, params: &umber_core::vector::VectorParams) {
        let fresh = !self.map.contains_key(&layer);
        let state = self
            .map
            .entry(layer)
            .or_insert_with(|| VectorLayerState::new(layer, params.clone()));
        if fresh || !state.params().same_cache_key(params) {
            state.set_params(params.clone());
            state.budget_blocked = None;
            state.budget_retry_watermark = None;
            if !self.dirty.contains(&layer) {
                self.dirty.push(layer);
            }
        }
    }

    /// First sight uses a restored plane as authoritative; later sightings use
    /// normal document adoption so an edit is never suppressed by that plane.
    pub fn ensure_seeded(
        &mut self,
        layer: u64,
        params: &umber_core::vector::VectorParams,
        planed: bool,
    ) {
        if self.map.contains_key(&layer) {
            self.ensure(layer, params);
            return;
        }
        let mut state = VectorLayerState::new(layer, params.clone());
        if planed {
            state.dirty = false;
            state.budget_blocked = None;
            state.budget_retry_watermark = None;
        } else if !self.dirty.contains(&layer) {
            self.dirty.push(layer);
        }
        self.map.insert(layer, state);
    }

    /// Seeds a reopened saved plane CLEAN. The sidecar is authoritative;
    /// re-evaluating it would spend work and could alter the reopened image.
    pub fn ensure_open(&mut self, layer: u64, params: &umber_core::vector::VectorParams) {
        let state = self
            .map
            .entry(layer)
            .or_insert_with(|| VectorLayerState::new(layer, params.clone()));
        if !state.params().same_cache_key(params) {
            state.params = params.clone();
            state.mark_dirty();
        }
        state.refusal = None;
        state.budget_blocked = None;
        state.budget_retry_watermark = None;
        state.dirty = false;
    }

    /// Marks a planeless or explicitly refreshed vector dirty and queues it.
    pub fn mark_dirty(&mut self, layer: u64) {
        if let Some(state) = self.map.get_mut(&layer) {
            state.mark_dirty();
            state.budget_blocked = None;
            state.budget_retry_watermark = None;
            if !self.dirty.contains(&layer) {
                self.dirty.push(layer);
            }
        }
    }

    /// Drops deleted layers and their queued work. Returns sorted removed ids
    /// so the caller can clear their HUD badges deterministically.
    pub fn retain_live(&mut self, live: &std::collections::HashSet<u64>) -> Vec<u64> {
        let mut removed: Vec<u64> = self
            .map
            .keys()
            .copied()
            .filter(|id| !live.contains(id))
            .collect();
        removed.sort_unstable();
        for id in &removed {
            self.map.remove(id);
        }
        self.dirty.retain(|id| live.contains(id));
        removed
    }

    /// Total retained vector cache bytes in the document.
    pub fn cache_bytes(&self) -> usize {
        self.map.values().map(VectorLayerState::cache_bytes).sum()
    }

    /// Marks a layer most-recently-used for global cache eviction.
    pub fn touch(&mut self, layer: u64) {
        self.clock = self.clock.wrapping_add(1);
        if let Some(state) = self.map.get_mut(&layer) {
            state.last_used = self.clock;
        }
    }

    /// Applies the production document-wide cache ceiling.
    pub fn enforce_global_budget_default(&mut self, keep_layer: Option<u64>) {
        self.enforce_global_budget(MAX_VECTOR_CACHE_BYTES_TOTAL, keep_layer);
    }

    /// Evicts whole least-recently-used layer caches until the document cap is
    /// met. `keep_layer` is the active refresh and is never selected.
    pub fn enforce_global_budget(&mut self, cap: usize, keep_layer: Option<u64>) {
        let mut candidates: Vec<(u64, u64)> = self
            .map
            .iter()
            .filter(|(id, state)| {
                Some(**id) != keep_layer && !state.cache.is_empty() && state.last_used > 0
            })
            .map(|(id, state)| (state.last_used, *id))
            .collect();
        candidates.sort_unstable();
        for (_, id) in candidates {
            if self.cache_bytes() <= cap {
                break;
            }
            if let Some(state) = self.map.get_mut(&id) {
                state.cache.clear();
            }
        }
    }

    /// Observes frame-wide raster inputs. A symmetry or tile-size transition
    /// queues every live vector; cache entries remain valid by their own keys.
    pub fn observe_frame(&mut self, symmetry: &SymmetrySettings, size: Option<(u32, u32)>) {
        let changed = self
            .last_symmetry
            .is_some_and(|previous| !previous.same_cache_key(symmetry))
            || self
                .last_size
                .is_some_and(|previous| Some(previous) != size);
        self.last_symmetry = Some(*symmetry);
        self.last_size = size;
        if !changed {
            return;
        }
        for (id, state) in &mut self.map {
            state.dirty = true;
            if !self.dirty.contains(id) {
                self.dirty.push(*id);
            }
        }
    }

    /// Retries budget-blocked vectors only after plane bytes fall below the
    /// watermark captured at refusal. Cached tiles are reused for republish.
    pub fn retry_budget_freed(&mut self, plane_bytes: u64) {
        let retry: Vec<u64> = self
            .map
            .iter()
            .filter(|(_, state)| {
                state
                    .budget_retry_watermark
                    .is_some_and(|watermark| plane_bytes < watermark)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in retry {
            if let Some(state) = self.map.get_mut(&id) {
                state.budget_blocked = None;
                state.budget_retry_watermark = None;
                state.dirty = true;
            }
            if !self.dirty.contains(&id) {
                self.dirty.push(id);
            }
        }
    }
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
        // A fill layer without params is the default fill everywhere the
        // app reads one (the refresh path's `unwrap_or_default`
        // convention): apply adopts it rather than hard-rejecting, so a
        // mask can be lassoed onto a freshly added (never edited) fill
        // layer. The Fill-kind guard stays: adoption is for fill layers
        // only — a paint target still rejects (no stray set_fill write).
        let mut params = match doc
            .layers()
            .iter()
            .find(|l| l.id == target && l.kind == LayerKind::Fill)
            .map(|l| l.fill.clone().unwrap_or_default())
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
    crate::i18n::hint_line(ui, tr("fill.poly-hint"), None);
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
/// The session's per-fill-layer evaluation states (the cache host). Held by
/// the app, driven by [`refresh_fill_layer`] after any fill-param edit —
/// the journal replay path included (undo/redo re-fires SetFill, the panel
/// re-marks, the refresh re-evaluates: the same live-not-baked contract).
/// Per-channel size overrides used for cache invalidation.
type ChannelSizeSnapshot = Vec<(String, String, (u32, u32))>;

/// The session's per-fill-layer evaluation states (the cache host). Held by
/// the app, driven by [`refresh_fill_layer`] after any fill-param edit —
/// the journal replay path included (undo/redo re-fires SetFill, the panel
/// re-marks, the refresh re-evaluates: the same live-not-baked contract).
#[derive(Default)]
pub struct FillLayerStates {
    /// One state per live fill layer, keyed by layer id.
    pub map: std::collections::HashMap<u64, FillLayerState>,
    /// Layers whose params changed this frame (drained by the app's
    /// per-frame adoption pass; doc-side edits, undo/redo replays and
    /// first-sight layers all queue here).
    pub dirty: Vec<u64>,
    last_symmetry: Option<SymmetrySettings>,
    last_size: Option<(u32, u32)>,
    last_channel_sizes: Option<ChannelSizeSnapshot>,
    last_tiles: Option<Vec<u16>>,
}

impl FillLayerStates {
    /// Ensures a state exists for `layer` (constructing on first sight),
    /// then adopts the document's current `params`, `blend` and `opacity`.
    /// A params change — or a first sight — marks the cache dirty and
    /// queues the layer for the frame's refresh: panel edits, undo/redo
    /// and journal replays all land in `Layer::fill` before the frame's
    /// adoption pass runs, so every doc-side change re-evaluates through
    /// the one live path (the live-not-baked contract).
    pub fn ensure(
        &mut self,
        layer: u64,
        params: &FillParams,
        blend: umber_core::BlendMode,
        opacity: f32,
    ) {
        let fresh = !self.map.contains_key(&layer);
        let state = self
            .map
            .entry(layer)
            .or_insert_with(|| FillLayerState::new(layer, params.clone()));
        if fresh || state.params() != params {
            state.set_params(params.clone());
            // A param edit clears a budget block: new inputs may fit
            // (or the retry below re-blocks loudly — never silent).
            state.budget_blocked = None;
            state.budget_retry_watermark = None;
            if !self.dirty.contains(&layer) {
                self.dirty.push(layer);
            }
        }
        state.blend = blend;
        state.opacity = opacity;
    }

    /// First sight uses a restored plane set as authoritative; later
    /// sightings use normal document adoption so an edit is never
    /// suppressed by that plane.
    pub fn ensure_seeded(
        &mut self,
        layer: u64,
        params: &FillParams,
        blend: umber_core::BlendMode,
        opacity: f32,
        planed: bool,
    ) {
        if self.map.contains_key(&layer) {
            self.ensure(layer, params, blend, opacity);
            return;
        }
        let mut state = FillLayerState::new(layer, params.clone());
        state.blend = blend;
        state.opacity = opacity;
        if planed {
            state.dirty = false;
            state.budget_blocked = None;
            state.budget_retry_watermark = None;
        }
        // A planeless state starts dirty internally but does NOT queue yet:
        // the dormancy decision owns that. Dormant layers must never refresh
        // over legacy Base; the Dirty arm calls `mark_dirty` explicitly.
        self.map.insert(layer, state);
    }

    /// Queues a refresh for a tracked layer without changing its params
    /// (the shell's "v2 planeless ⇒ dirty" seeding and the explicit
    /// refresh path). No-op for untracked layers.
    pub fn mark_dirty(&mut self, layer: u64) {
        if let Some(state) = self.map.get_mut(&layer) {
            state.mark_dirty();
            state.budget_blocked = None;
            state.budget_retry_watermark = None;
            if !self.dirty.contains(&layer) {
                self.dirty.push(layer);
            }
        }
    }

    /// Drops deleted layers and their queued work. Returns sorted removed
    /// ids so the caller can clear badges and dormancy deterministically.
    pub fn retain_live(&mut self, live: &std::collections::HashSet<u64>) -> Vec<u64> {
        let mut removed: Vec<u64> = self
            .map
            .keys()
            .copied()
            .filter(|id| !live.contains(id))
            .collect();
        removed.sort_unstable();
        for id in &removed {
            self.map.remove(id);
        }
        self.dirty.retain(|id| live.contains(id));
        removed
    }

    /// Observes frame-wide raster inputs. A symmetry or tile-size transition
    /// queues every live Fill once; cache entries remain valid by their own
    /// keys, so unchanged frames do not queue or recompute.
    /// Observes frame-wide raster inputs. A symmetry, set-size, or channel-size
    /// transition invalidates every cached tile and queues every live Fill
    /// once. Repeated identical frames never queue or recompute.
    /// Observes frame-wide raster inputs. A symmetry, set-size, channel-size,
    /// or tile-membership transition invalidates every cached tile and queues
    /// every non-dormant live Fill once. Dormant layers wake only on an
    /// explicit edit, never on an ambient frame transition.
    pub fn observe_frame(
        &mut self,
        symmetry: &SymmetrySettings,
        size: Option<(u32, u32)>,
        channel_sizes: &std::collections::BTreeMap<(String, String), (u32, u32)>,
        tiles: &[u16],
        dormant: &std::collections::HashSet<u64>,
    ) {
        let current_channels: ChannelSizeSnapshot = channel_sizes
            .iter()
            .map(|((set, channel), dimensions)| (set.clone(), channel.clone(), *dimensions))
            .collect();
        let mut current_tiles = tiles.to_vec();
        current_tiles.sort_unstable();
        let changed = self
            .last_symmetry
            .is_some_and(|previous| previous != *symmetry)
            || self
                .last_size
                .is_some_and(|previous| Some(previous) != size)
            || self
                .last_channel_sizes
                .as_ref()
                .is_some_and(|previous| *previous != current_channels)
            || self
                .last_tiles
                .as_ref()
                .is_some_and(|previous| *previous != current_tiles);
        self.last_symmetry = Some(*symmetry);
        self.last_size = size;
        self.last_channel_sizes = Some(current_channels);
        self.last_tiles = Some(current_tiles);
        if !changed {
            return;
        }
        for (id, state) in &mut self.map {
            if dormant.contains(id) {
                continue;
            }
            state.mark_dirty();
            if !self.dirty.contains(id) {
                self.dirty.push(*id);
            }
        }
    }

    /// Requeues budget-blocked layers whose block may have cleared: a
    /// state blocked at `blocked_bytes` retries when the session now
    /// holds fewer plane bytes (a plane freed). Retrying every frame
    /// would spam; the shell calls this when bytes fall (or params
    /// change, which requeues through [`Self::ensure`]). This queues a
    /// republish, not a re-rasterization: the cached grid stays valid.
    pub fn retry_budget_freed(&mut self, plane_bytes: u64, hard_cap: u64) {
        let retry: Vec<u64> = self
            .map
            .iter()
            .filter(|(_, state)| {
                state
                    .budget_retry_watermark
                    .is_some_and(|watermark| plane_bytes < watermark)
                    || state
                        .budget_retry_cap
                        .is_some_and(|refusal_cap| hard_cap > refusal_cap)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in retry {
            if let Some(state) = self.map.get_mut(&id) {
                state.budget_blocked = None;
                state.budget_retry_watermark = None;
                state.budget_retry_cap = None;
            }
            if !self.dirty.contains(&id) {
                self.dirty.push(id);
            }
        }
    }
}

/// The catalog keys of [`fill_section_ui`]'s labelled rows, in draw
/// order: the section title, one fill layer's projection and source
/// parameter rows, then [`polygon_fill_rows`] (which the section draws
/// last). The Properties rail's search vocabulary for the Fill context,
/// ahead of `path_tool::ROW_LABEL_KEYS` (`rail-row-labels`; the inclusion
/// rule is stated on `brush_panel::ROW_LABEL_KEYS`).
///
/// - The projection and source combos have no leading label, so they are
///   absent; so are the empty-state line, the graph note and the
///   polygon tool's hint, status readouts and buttons.
/// - `fill.scale` is drawn twice (triplanar scale, noise scale): listed
///   once, at its first row.
/// - Static, one layer's worth: the section repeats these rows per fill
///   layer under the layer's own name, and the projection- and
///   source-gated rows stay listed while hidden (the Brush list's v1
///   boundary).
const ROW_LABEL_KEYS: &[&str] = &[
    "fill.section",
    "fill.sharpness",
    "fill.scale",
    "fill.axis",
    "fill.alpha",
    "fill.color-a",
    "fill.color-b",
    "fill.angle",
    "fill.seed",
    "fill.octaves",
    "fill.poly-tool",
    "fill.poly-armed",
];

/// The fill section's row labels, localized. Pure: no `Ui`, no document.
pub fn row_labels() -> Vec<String> {
    ROW_LABEL_KEYS.iter().copied().map(tr).collect()
}

pub fn fill_section_ui(
    ui: &mut egui::Ui,
    doc: &mut Document,
    mesh: Option<&MeshData>,
    tool: &mut PolygonFillTool,
    channels: &[String],
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
                // The fill's publish target (the channel-map kill): the
                // combo over the session set's channels; a target the set
                // lacks (a renamed project) keeps the row but stages
                // nothing — the refresh's zero-size guard.
                let mut picked = edited.channel.clone();
                egui::ComboBox::from_id_salt(format!("fill-channel-{id}"))
                    .selected_text(picked.clone())
                    .show_ui(ui, |ui| {
                        for name in channels {
                            ui.selectable_value(&mut picked, name.clone(), name);
                        }
                    });
                if picked != edited.channel {
                    edited.channel = picked;
                    changed = true;
                }
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
            if field_row(
                ui,
                &tr("fill.sharpness"),
                umber_gui::ValueField::new(sharpness)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(1.0..=8.0),
            ) {
                *changed = true;
            }
            if field_row(
                ui,
                &tr("fill.scale"),
                umber_gui::ValueField::new(scale)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.1..=8.0),
            ) {
                *changed = true;
            }
        }
        FillProjection::Planar { axis } | FillProjection::Cylindrical { axis } => {
            axis_rows(ui, axis, changed);
        }
        FillProjection::Uv | FillProjection::Spherical => {}
    }
}

/// One labelled numeric row in the house grammar (muted label + a
/// [`umber_gui::ValueField`]); true when the value changed. Same shape as
/// the postfx panel's `field_row`.
fn field_row<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    field: umber_gui::ValueField<'_, N>,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(umber_gui::muted()));
        ui.add(field).changed()
    })
    .inner
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
            if crate::preferences::picker_button(ui, &mut rgb).changed() {
                color[0] = rgb[0];
                color[1] = rgb[1];
                color[2] = rgb[2];
                *changed = true;
            }
            if field_row(
                ui,
                &tr("fill.alpha"),
                umber_gui::ValueField::new(&mut color[3])
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.0..=1.0),
            ) {
                *changed = true;
            }
        }
        FillSource::Gradient { a, b, angle } => {
            for (label, c) in [(tr("fill.color-a"), a), (tr("fill.color-b"), b)] {
                let mut rgb = [c[0], c[1], c[2]];
                ui.horizontal(|ui| {
                    ui.label(label);
                    if crate::preferences::picker_button(ui, &mut rgb).changed() {
                        c[0] = rgb[0];
                        c[1] = rgb[1];
                        c[2] = rgb[2];
                        *changed = true;
                    }
                });
            }
            if field_row(
                ui,
                &tr("fill.angle"),
                umber_gui::ValueField::new(angle)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.0..=std::f32::consts::TAU),
            ) {
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
            if field_row(
                ui,
                &tr("fill.scale"),
                umber_gui::ValueField::new(scale)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.1..=16.0),
            ) {
                *changed = true;
            }
            if field_row(
                ui,
                &tr("fill.octaves"),
                umber_gui::ValueField::new(octaves)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(1..=8),
            ) {
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
mod row_label_tests {
    use super::*;
    use crate::document::{assert_row_label_keys, drawn_source};

    #[test]
    fn the_fill_row_labels_are_the_rows_the_section_draws() {
        let src = include_str!("fill_layer.rs");
        let drawn = drawn_source(src, "ROW_LABEL_KEYS").expect("list");
        let labels = row_labels();
        assert_row_label_keys(ROW_LABEL_KEYS, &labels, &drawn);
        // 12 labelled rows: the section title first, the polygon tool's
        // armed toggle (the section's last labelled row) last.
        assert_eq!(labels.len(), 12);
        assert_eq!(labels.first().map(String::as_str), Some("Fill layers"));
        assert_eq!(
            labels.last().map(String::as_str),
            Some("Capture clicks (lasso)")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_brush::{MirrorAxis, MirrorSettings};
    use umber_core::BlendMode;
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
            triangle_materials: Vec::new(),
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
            channel: "baseColor".into(),
        }
    }

    #[test]
    fn gate_and_fold_matches_mask_then_fold_exactly() {
        let tile = FillTile {
            width: 16,
            height: 16,
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 255, 255, 255, 0,
            ]
            .repeat(64),
            coverage: [1.0, 0.5, 0.0, f32::NAN].repeat(64),
        };
        let mask = left_half_mask();
        let gated = apply_quick_mask(
            FillTile {
                width: 16,
                height: 16,
                rgba: tile.rgba.clone(),
                coverage: tile.coverage.clone(),
            },
            &mask,
            1001,
        );
        let expected = fold_coverage_to_premultiplied(&gated.rgba, &gated.coverage);
        let actual = gate_and_fold(&tile.rgba, &tile.coverage, 16, 16, &mask, 1001);
        assert_eq!(actual, expected);
    }

    #[test]
    fn empty_tile_symmetry_is_an_untouched_passthrough() {
        let tile = FillTile {
            width: 16,
            height: 16,
            rgba: Vec::new(),
            coverage: Vec::new(),
        };
        let done = apply_symmetry(tile, &mirror_x_half());
        assert_eq!((done.width, done.height), (16, 16));
        assert!(done.rgba.is_empty());
        assert!(done.coverage.is_empty());
    }

    #[test]
    fn non_square_mirror_y_maps_rows_exactly() {
        let (width, height) = (32u32, 16u32);
        let mut coverage = vec![0.0f32; (width * height) as usize];
        coverage[..(width * 4) as usize].fill(1.0);
        let tile = FillTile {
            width,
            height,
            rgba: [255, 0, 0, 255].repeat((width * height) as usize),
            coverage,
        };
        let settings = SymmetrySettings {
            mirror: MirrorSettings {
                enabled: true,
                axis: MirrorAxis::Y,
                offset: 0.5,
            },
            ..SymmetrySettings::default()
        };
        let done = apply_symmetry(tile, &settings);
        for y in 0..height as usize {
            let want = if !(4..12).contains(&y) { 1.0 } else { 0.0 };
            for x in 0..width as usize {
                assert_eq!(
                    done.coverage[y * width as usize + x],
                    want,
                    "texel ({x},{y})"
                );
            }
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

        let first = state.evaluate(&mesh, 1001, 16, 16, &off).to_vec();
        assert!(!state.is_dirty(), "evaluate clears the dirty flag");
        assert!(
            left_is(&first, [255, 0, 0, 255]),
            "left half red, right empty"
        );

        // No change: the cache serves byte-identical bytes, still clean.
        let cached = state.evaluate(&mesh, 1001, 16, 16, &off).to_vec();
        assert_eq!(cached, first);
        assert!(!state.is_dirty());

        // The source change: dirty immediately, tile updates on evaluate.
        state.set_source(FillSource::Solid {
            color: [0.0, 0.0, 1.0, 1.0],
        });
        assert!(state.is_dirty(), "a source change must dirty the layer");
        let second = state.evaluate(&mesh, 1001, 16, 16, &off).to_vec();
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
        tracked.evaluate(&mesh, 1001, 16, 16, &off);
        assert!(!tracked.is_dirty());
        stack.set_fill(
            id,
            FillParams {
                source: FillSource::Solid {
                    color: [0.0, 1.0, 0.0, 1.0],
                },
                projection: FillProjection::Uv,
                mask: None,
                channel: "baseColor".into(),
            },
        );
        tracked.sync_from_layer(stack.layer(id).unwrap());
        assert!(
            tracked.is_dirty(),
            "document-side edit must dirty the cache"
        );
        let third = tracked.evaluate(&mesh, 1001, 16, 16, &off).to_vec();
        assert!(left_is(&third, [0, 255, 0, 255]), "left half green now");
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
            triangle_materials: Vec::new(),
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
            channel: "baseColor".into(),
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
        let plane = fold_coverage_to_premultiplied(&gated.rgba, &gated.coverage);
        for y in 0..16 {
            for x in 0..16 {
                let px = &plane[(y * 16 + x) * 4..][..4];
                if x < 8 && y < 8 {
                    assert_eq!(px, &[255, 0, 0, 255], "fold ({x},{y})");
                } else {
                    assert_eq!(px, &[0, 0, 0, 0], "fold ({x},{y})");
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

    /// Test (h): the adoption contract — `FillLayerStates::ensure` adopts
    /// the document's CURRENT params on every call (not just first sight),
    /// marks the cache dirty, and queues the layer for the frame's refresh;
    /// a no-op ensure (params unchanged) neither dirties nor queues. This
    /// is the composed live-edit link the 06:11Z integration left open:
    /// panel edits, undo/redo and journal replays all land in `Layer::fill`
    /// before the frame's adoption pass reads it, so every doc-side change
    /// re-evaluates through this one path.
    #[test]
    fn fill_states_adopt_doc_changes_and_queue_refresh() {
        let mut stack = umber_core::LayerStack::new();
        let id = stack.add_layer("Tint", LayerKind::Fill);
        stack.set_fill(
            id,
            FillParams {
                source: FillSource::Solid {
                    color: [1.0, 0.0, 0.0, 1.0],
                },
                projection: FillProjection::Uv,
                mask: None,
                channel: "baseColor".into(),
            },
        );
        let params_of =
            |stack: &umber_core::LayerStack| stack.layer(id).unwrap().fill.clone().unwrap();

        // First sight: constructed, marked dirty, queued.
        let mut states = FillLayerStates::default();
        let blend = stack.layer(id).unwrap().blend_mode;
        let opacity = stack.layer(id).unwrap().opacity;
        states.ensure(id, &params_of(&stack), blend, opacity);
        assert!(states.dirty.contains(&id), "first sight must queue");
        assert_eq!(states.dirty.len(), 1);
        states.dirty.clear();

        // No-op ensure: doc params unchanged — no dirty, no queue. The
        // cache's own dirty flag first settles (evaluate once, the
        // sibling tests' pattern) so the assertion observes the no-op,
        // not the never-evaluated initial state.
        let mesh = left_half_quad();
        let off = SymmetrySettings::default();
        states
            .map
            .get_mut(&id)
            .unwrap()
            .evaluate(&mesh, 1001, 16, 16, &off);
        states.ensure(id, &params_of(&stack), blend, opacity);
        assert!(states.dirty.is_empty(), "no-op must not queue");
        let state = states.map.get(&id).unwrap();
        assert!(!state.is_dirty(), "no-op must not dirty the cache");

        // Doc-side edit (the panel/undo/journal path): ensure adopts the
        // fresh params, marks the cache dirty, and queues the layer.
        stack.set_fill(
            id,
            FillParams {
                source: FillSource::Solid {
                    color: [0.0, 1.0, 0.0, 1.0],
                },
                projection: FillProjection::Uv,
                mask: None,
                channel: "baseColor".into(),
            },
        );
        states.ensure(id, &params_of(&stack), blend, opacity);
        assert!(states.dirty.contains(&id), "doc edit must queue");
        let state = states.map.get(&id).unwrap();
        assert!(state.is_dirty(), "doc edit must dirty the cache");
        assert_eq!(
            state.params().source,
            FillSource::Solid {
                color: [0.0, 1.0, 0.0, 1.0],
            }
        );

        // Blend/opacity adoption rides every ensure without dirtying on
        // its own (the composite publish reads them at refresh time).
        stack.set_blend_mode(id, BlendMode::Multiply);
        states.ensure(id, &params_of(&stack), BlendMode::Multiply, 0.5);
        let (blend_now, opacity_now) = states.map.get(&id).unwrap().layer_blend_opacity();
        assert_eq!(blend_now, BlendMode::Multiply);
        assert_eq!(opacity_now, 0.5);
    }

    /// §8 T12: sample `(255,0,0,255)` at quick-mask coverage 0.25 folds
    /// to the plane texel `(64,0,0,64)` (`255·0.25 = 63.75 → 64`).
    /// Sentinel: the v1 fast path published `(255,0,0,255)`, dropping
    /// coverage.
    #[test]
    fn fold_premultiplies_with_coverage_folded_in() {
        let plane = fold_coverage_to_premultiplied(&[255, 0, 0, 255], &[0.25]);
        assert_eq!(plane, vec![64, 0, 0, 64]);
        assert_ne!(plane, vec![255, 0, 0, 255], "coverage must not drop");
        // Full coverage is the identity on opaque texels; zero coverage
        // (and non-finite coverage) goes transparent black; every output
        // satisfies `p ≤ a`.
        assert_eq!(
            fold_coverage_to_premultiplied(&[10, 20, 30, 255], &[1.0]),
            vec![10, 20, 30, 255]
        );
        assert_eq!(
            fold_coverage_to_premultiplied(&[10, 20, 30, 255], &[0.0]),
            vec![0, 0, 0, 0]
        );
        assert_eq!(
            fold_coverage_to_premultiplied(&[10, 20, 30, 255], &[f32::NAN]),
            vec![0, 0, 0, 0]
        );
        // Straight-alpha in, premultiplied out: half-alpha red at full
        // coverage folds to `(128,0,0,128)` (`α = 128/255`, `c·α =
        // 128/255·255 = 128.0` exactly).
        let half = fold_coverage_to_premultiplied(&[255, 0, 0, 128], &[1.0]);
        assert_eq!(half, vec![128, 0, 0, 128]);
        for px in half.chunks_exact(4) {
            assert!(px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3]);
        }
    }

    /// [B2]'s open-seeding: `ensure_open` inserts CLEAN (never queued),
    /// while the per-frame `ensure` keeps queuing fresh layers. A v1
    /// fill opened over legacy Base output therefore never re-evaluates
    /// on open (no double-count); runtime adds still evaluate.
    #[test]
    fn ensure_open_seeds_clean_while_ensure_queues_fresh() {
        let params = solid_fill([1.0, 0.0, 0.0, 1.0]);
        let mut states = FillLayerStates::default();
        states.ensure_seeded(7, &params, BlendMode::Normal, 1.0, true);
        let state = states.map.get(&7).expect("open seeds the state");
        assert!(!state.is_dirty(), "open-seeded fills start clean");
        assert!(states.dirty.is_empty(), "open never queues");
        // A second open over the same id stays clean too.
        states.ensure_seeded(7, &params, BlendMode::Normal, 1.0, true);
        assert!(states.dirty.is_empty());
        // The runtime path still queues first sight (panel adds, undo of
        // an add, journal replay).
        states.ensure(9, &params, BlendMode::Normal, 1.0);
        assert!(states.dirty.contains(&9));
        assert!(states.map.get(&9).unwrap().is_dirty());
        // ... and an explicit/bitten refresh dirties on demand.
        states.mark_dirty(7);
        assert!(states.dirty.contains(&7));
        assert!(states.map.get(&7).unwrap().is_dirty());
    }

    #[test]
    fn fill_state_cache_key_covers_both_dimensions_and_symmetry() {
        let mesh = left_half_quad();
        let off = SymmetrySettings::default();
        let mut state = FillLayerState::new(7, solid_fill([1.0, 0.0, 0.0, 1.0]));
        let first = state.evaluate(&mesh, 1001, 32, 16, &off).to_vec();
        assert_eq!(first.len(), 32 * 16 * 4);
        assert_eq!(state.coverage_of(1001, 32, 16).unwrap().len(), 512);
        assert_eq!(state.evaluation_count(), 1);

        let cached = state.evaluate(&mesh, 1001, 32, 16, &off);
        assert_eq!(cached, first);
        assert_eq!(state.evaluation_count(), 1, "identical key must hit");

        let taller = state.evaluate(&mesh, 1001, 32, 32, &off).to_vec();
        assert_eq!(taller.len(), 32 * 32 * 4);
        assert_eq!(state.evaluation_count(), 2, "height is cache identity");
        let narrower = state.evaluate(&mesh, 1001, 16, 16, &off).to_vec();
        assert_eq!(narrower.len(), 16 * 16 * 4);
        assert_eq!(state.evaluation_count(), 3, "width is cache identity");

        let mirrored = state
            .evaluate(&mesh, 1001, 32, 16, &mirror_x_half())
            .to_vec();
        assert_eq!(mirrored.len(), 32 * 16 * 4);
        assert_eq!(state.evaluation_count(), 4, "symmetry is cache identity");
        state.set_source(FillSource::Solid {
            color: [0.0, 0.0, 1.0, 1.0],
        });
        assert!(state.is_dirty());
        let blue = state.evaluate(&mesh, 1001, 32, 16, &off).to_vec();
        assert_eq!(&blue[(7 * 32 + 7) * 4..][..4], &[0, 0, 255, 255]);
        assert_eq!(state.evaluation_count(), 5);
    }

    #[test]
    fn fill_planes_complete_drives_open_seeding() {
        let params = solid_fill([1.0, 0.0, 0.0, 1.0]);
        let channel = params.channel.clone();
        let mut present = vec![(
            crate::paint_state::PlaneOwner::Layer(7),
            1001,
            channel.clone(),
        )];
        assert!(!fill_planes_complete(&present, 7, &channel, &[1001, 1002]));
        assert!(!fill_planes_complete(&[], 7, &channel, &[]));
        let foreign_channel = vec![(
            crate::paint_state::PlaneOwner::Layer(7),
            1001,
            "roughness".to_owned(),
        )];
        assert!(
            !fill_planes_complete(&foreign_channel, 7, &channel, &[1001]),
            "a plane on another channel never satisfies completeness"
        );
        present.push((
            crate::paint_state::PlaneOwner::Layer(7),
            1002,
            channel.clone(),
        ));
        assert!(fill_planes_complete(&present, 7, &channel, &[1001, 1002]));

        let mut partial = FillLayerStates::default();
        partial.ensure_seeded(7, &params, BlendMode::Normal, 1.0, false);
        assert!(partial.map[&7].is_dirty());
        assert!(
            partial.dirty.is_empty(),
            "seeding must not preempt dormancy"
        );
        partial.mark_dirty(7);
        assert_eq!(partial.dirty, vec![7]);

        let mut complete = FillLayerStates::default();
        complete.ensure_seeded(7, &params, BlendMode::Normal, 1.0, true);
        assert!(!complete.map[&7].is_dirty());
        assert!(complete.dirty.is_empty());
        let changed = solid_fill([0.0, 1.0, 0.0, 1.0]);
        complete.ensure_seeded(7, &changed, BlendMode::Normal, 1.0, true);
        assert!(complete.map[&7].is_dirty());
        assert_eq!(
            complete.dirty,
            vec![7],
            "restored planes never suppress edits"
        );
    }

    #[test]
    fn fill_states_retain_and_observe_like_vector() {
        let off = SymmetrySettings::default();
        let params = solid_fill([0.0, 1.0, 0.0, 1.0]);
        let mut states = FillLayerStates::default();
        states.ensure(3, &params, BlendMode::Normal, 1.0);
        states.dirty.clear();

        states.observe_frame(
            &off,
            Some((16, 16)),
            &std::collections::BTreeMap::new(),
            &[],
            &std::collections::HashSet::new(),
        );
        states.observe_frame(
            &off,
            Some((16, 16)),
            &std::collections::BTreeMap::new(),
            &[],
            &std::collections::HashSet::new(),
        );
        assert!(states.dirty.is_empty());
        states.observe_frame(
            &mirror_x_half(),
            Some((16, 16)),
            &std::collections::BTreeMap::new(),
            &[],
            &std::collections::HashSet::new(),
        );
        assert_eq!(states.dirty, vec![3]);
        states.dirty.clear();
        states.observe_frame(
            &mirror_x_half(),
            Some((32, 32)),
            &std::collections::BTreeMap::new(),
            &[],
            &std::collections::HashSet::new(),
        );
        assert_eq!(states.dirty, vec![3]);

        let mut channels = std::collections::BTreeMap::new();
        states.dirty.clear();
        let dormant = std::collections::HashSet::from([3]);
        states.observe_frame(
            &mirror_x_half(),
            Some((32, 32)),
            &channels,
            &[1001],
            &dormant,
        );
        assert!(
            states.dirty.is_empty(),
            "ambient transitions never wake dormant fills"
        );
        states.observe_frame(
            &mirror_x_half(),
            Some((32, 32)),
            &channels,
            &[1001, 1002],
            &std::collections::HashSet::new(),
        );
        assert_eq!(states.dirty, vec![3], "tile membership invalidates fills");
        states.dirty.clear();
        channels.insert(("TextureSet".to_owned(), "roughness".to_owned()), (32, 16));
        states.observe_frame(
            &mirror_x_half(),
            Some((32, 32)),
            &channels,
            &[1001, 1002],
            &std::collections::HashSet::new(),
        );
        assert_eq!(states.dirty, vec![3], "a channel resize invalidates fills");

        states.map.insert(9, FillLayerState::new(9, params.clone()));
        states.dirty = vec![3, 9];
        let live = std::collections::HashSet::from([3]);
        assert_eq!(states.retain_live(&live), vec![9]);
        assert!(!states.map.contains_key(&9));
        assert_eq!(states.dirty, vec![3]);
    }

    #[test]
    fn fill_budget_retry_waits_for_watermark_and_reuses_cache() {
        let mesh = left_half_quad();
        let off = SymmetrySettings::default();
        let params = solid_fill([0.0, 1.0, 0.0, 1.0]);
        let mut states = FillLayerStates::default();
        states.ensure(3, &params, BlendMode::Normal, 1.0);
        states.dirty.clear();
        let state = states.map.get_mut(&3).expect("fill state exists");
        state.evaluate(&mesh, 1001, 16, 16, &off);
        state.budget_retry_watermark = Some(100);

        states.retry_budget_freed(100, 100);
        assert!(states.dirty.is_empty(), "no memory was freed, so no retry");
        states.retry_budget_freed(101, 100);
        assert!(states.dirty.is_empty(), "growth is not a retry signal");

        let state = states.map.get_mut(&3).expect("fill state exists");
        state.budget_retry_cap = Some(50);
        states.retry_budget_freed(100, 100);
        assert_eq!(states.dirty, vec![3], "a cap increase retries too");
        states.dirty.clear();
        states.map.get_mut(&3).unwrap().budget_retry_watermark = Some(100);
        states.retry_budget_freed(99, 100);
        assert_eq!(states.dirty, vec![3]);
        let state = states.map.get_mut(&3).expect("fill state exists");
        assert_eq!(
            state.cache.len(),
            1,
            "the cached tile survives for republish"
        );
        assert_eq!(
            state.evaluation_count(),
            1,
            "budget retry does not rasterize again"
        );
        assert!(state.budget_retry_watermark.is_none());
    }

    #[test]
    fn budget_refusal_retry_republishes_cached_grid() {
        let Some((device, queue)) = try_vector_device() else {
            assert!(
                std::env::var_os("UMBER_REQUIRE_GPU").is_none(),
                "UMBER_REQUIRE_GPU=1 but no suitable wgpu adapter is available"
            );
            eprintln!("SKIP budget_refusal_retry: no suitable wgpu adapter");
            return;
        };
        let mut paint =
            PaintState::with_target_size(device, queue, 16).expect("paint state builds");
        paint
            .publish_plane(
                crate::paint_state::PlaneOwner::Layer(99),
                1001,
                "baseColor",
                16,
                16,
                vec![0; 16 * 16 * 4],
            )
            .expect("an unrelated plane creates a watermark");
        let watermark = paint.plane_bytes();
        paint.set_plane_hard_cap_bytes(watermark);

        let mesh = left_half_quad();
        let off = SymmetrySettings::default();
        let mut state = FillLayerState::new(7, solid_fill([1.0, 0.0, 0.0, 1.0]));
        let refused = refresh_fill_layer(&mut state, &mesh, &mut paint, &[1001], &off);
        assert!(refused.is_empty(), "the new plane is budget-refused");
        assert!(state.budget_blocked().is_some());
        assert_eq!(state.evaluation_count(), 1);
        assert!(
            !state.is_dirty(),
            "budget refusal does not invalidate the grid"
        );

        paint.clear_layer_planes();
        let mut states = FillLayerStates::default();
        states.map.insert(7, state);
        states.retry_budget_freed(0, watermark);
        assert_eq!(states.dirty, vec![7]);
        assert!(!states.map[&7].is_dirty(), "retry only republishes");

        paint.set_plane_hard_cap_bytes(u64::MAX);
        let staged = refresh_fill_layer(
            states.map.get_mut(&7).expect("state exists"),
            &mesh,
            &mut paint,
            &[1001],
            &off,
        );
        assert_eq!(staged, vec![1001]);
        assert_eq!(states.map[&7].evaluation_count(), 1);
        assert!(!states.map[&7].is_dirty());
    }

    #[test]
    fn fill_refresh_non_square_reuses_cache_and_missing_channel_allocates_nothing() {
        let Some((device, queue)) = try_vector_device() else {
            assert!(
                std::env::var_os("UMBER_REQUIRE_GPU").is_none(),
                "UMBER_REQUIRE_GPU=1 but no suitable wgpu adapter is available"
            );
            eprintln!("SKIP fill_refresh_non_square: no suitable wgpu adapter");
            return;
        };
        let mut paint =
            PaintState::with_target_size(device, queue, 16).expect("paint state builds");
        paint.resize_channel("TextureSet", "roughness", 32, 16);
        let mut params = solid_fill([1.0, 0.0, 0.0, 1.0]);
        params.channel = "roughness".into();
        let mut state = FillLayerState::new(7, params);
        let mesh = left_half_quad();
        let off = SymmetrySettings::default();
        let staged = refresh_fill_layer(&mut state, &mesh, &mut paint, &[1001], &off);
        assert_eq!(staged, vec![1001]);
        let plane = paint
            .read_plane(crate::paint_state::PlaneOwner::Layer(7), 1001, "roughness")
            .expect("the non-square fill plane materialized");
        assert_eq!((plane.width, plane.height), (32, 16));
        assert_eq!(state.evaluation_count(), 1);

        let staged = refresh_fill_layer(&mut state, &mesh, &mut paint, &[1001], &off);
        assert_eq!(staged, vec![1001]);
        assert_eq!(
            state.evaluation_count(),
            1,
            "republish reuses the cached grid"
        );
        assert!(!state.is_dirty());

        paint.resize_channel("TextureSet", "does-not-exist", 0, 0);
        let mut missing = FillLayerState::new(99, solid_fill([1.0, 0.0, 0.0, 1.0]));
        missing.params.channel = "does-not-exist".into();
        let staged = refresh_fill_layer(&mut missing, &mesh, &mut paint, &[1001], &off);
        assert!(staged.is_empty(), "a zero-size channel stages nothing");
        assert!(
            missing.is_dirty(),
            "a zero-size channel leaves the cache stale"
        );
        assert!(
            !paint.planes_present().iter().any(|(owner, tile, channel)| {
                *owner == crate::paint_state::PlaneOwner::Layer(99)
                    && *tile == 1001
                    && channel == "does-not-exist"
            })
        );
    }

    fn test_vector_params(color: [u8; 4]) -> umber_core::vector::VectorParams {
        use umber_core::vector::{Anchor, VectorShape};
        umber_core::vector::VectorParams {
            shape: VectorShape {
                closed: true,
                fill_rule: umber_core::svg::FillRule::NonZero,
                fill: color,
                stroke: None,
                anchors: vec![
                    Anchor::corner([0.25, 0.25]),
                    Anchor::corner([0.75, 0.25]),
                    Anchor::corner([0.75, 0.75]),
                    Anchor::corner([0.25, 0.75]),
                ],
            },
        }
    }

    #[test]
    fn vector_state_evaluates_caches_and_invalidates() {
        let off = SymmetrySettings::default();
        let mut state = VectorLayerState::new(7, test_vector_params([255, 0, 0, 255]));
        assert!(state.is_dirty());
        let first = state.evaluate(1001, 16, 16, &off).to_vec();
        assert_eq!(state.evaluation_count(), 1);
        assert!(!state.is_dirty());
        fn at(buf: &[u8], x: usize, y: usize) -> &[u8] {
            &buf[(y * 16 + x) * 4..][..4]
        }
        assert_eq!(at(&first, 7, 7), &[255, 0, 0, 255]);
        assert_eq!(at(&first, 0, 0), &[0, 0, 0, 0]);
        let second = state.evaluate(1001, 16, 16, &off);
        assert_eq!(second, first);
        assert_eq!(
            state.evaluation_count(),
            1,
            "unchanged input reuses the cache"
        );

        state.set_params(test_vector_params([0, 255, 0, 255]));
        assert!(state.is_dirty());
        let green = state.evaluate(1001, 16, 16, &off).to_vec();
        assert_eq!(at(&green, 7, 7), &[0, 255, 0, 255]);
        assert_eq!(state.evaluation_count(), 2);

        let mirrored = state.evaluate(1001, 16, 16, &mirror_x_half()).to_vec();
        assert_eq!(state.evaluation_count(), 3, "symmetry is cache identity");
        assert_eq!(mirrored.len(), first.len());
        let resized = state.evaluate(1001, 32, 32, &off);
        assert_eq!(resized.len(), 32 * 32 * 4);
        assert_eq!(state.evaluation_count(), 4, "tile size is cache identity");
    }

    #[test]
    fn vector_state_refreshes_every_tile_and_symmetry() {
        let off = SymmetrySettings::default();
        let mut state = VectorLayerState::new(7, test_vector_params([255, 0, 0, 255]));
        state.evaluate(1001, 16, 16, &off);
        state.evaluate(1002, 16, 16, &off);
        state.set_params(test_vector_params([0, 255, 0, 255]));

        let first = state.evaluate(1001, 16, 16, &off).to_vec();
        let second = state.evaluate(1002, 16, 16, &off).to_vec();
        assert_eq!(first, second);
        assert_eq!(&first[(7 * 16 + 7) * 4..][..4], &[0, 255, 0, 255]);
        assert_eq!(
            state.evaluation_count(),
            4,
            "both tiles recompute after the edit"
        );

        let mirrored_first = state.evaluate(1001, 16, 16, &mirror_x_half()).to_vec();
        let mirrored_second = state.evaluate(1002, 16, 16, &mirror_x_half()).to_vec();
        assert_eq!(mirrored_first, mirrored_second);
        assert_eq!(
            state.evaluation_count(),
            6,
            "both tiles adopt the new symmetry"
        );
    }

    #[test]
    fn budget_retry_waits_for_bytes_to_fall_and_reuses_cache() {
        let off = SymmetrySettings::default();
        let mut states = VectorLayerStates::default();
        states.ensure(3, &test_vector_params([0, 255, 0, 255]));
        states.dirty.clear();
        let state = states.map.get_mut(&3).expect("vector state exists");
        state.evaluate(1001, 16, 16, &off);
        state.budget_retry_watermark = Some(100);

        states.retry_budget_freed(100);
        assert!(states.dirty.is_empty(), "no memory was freed, so no retry");
        states.retry_budget_freed(101);
        assert!(states.dirty.is_empty(), "growth is not a retry signal");

        states.retry_budget_freed(99);
        assert_eq!(states.dirty, vec![3]);
        let state = states.map.get_mut(&3).expect("vector state exists");
        assert_eq!(
            state.cache.len(),
            1,
            "the cached tile survives for republish"
        );
        state.evaluate(1001, 16, 16, &off);
        assert_eq!(
            state.evaluation_count(),
            1,
            "budget retry does not rasterize again"
        );
    }

    #[test]
    fn frame_observation_requeues_on_symmetry_and_size_changes() {
        let off = SymmetrySettings::default();
        let mut states = VectorLayerStates::default();
        states.ensure(3, &test_vector_params([0, 255, 0, 255]));
        states.dirty.clear();

        states.observe_frame(&off, Some((16, 16)));
        states.observe_frame(&off, Some((16, 16)));
        assert!(states.dirty.is_empty());

        states.observe_frame(&mirror_x_half(), Some((16, 16)));
        assert_eq!(states.dirty, vec![3]);
        states.dirty.clear();
        states.observe_frame(&mirror_x_half(), Some((32, 32)));
        assert_eq!(states.dirty, vec![3]);
    }

    #[test]
    fn vector_cache_is_bounded_and_pruned_with_live_layers() {
        let off = SymmetrySettings::default();
        let mut state = VectorLayerState::new(3, test_vector_params([0, 255, 0, 255]));
        state.cache_cap_bytes = 1024;
        state.evaluate(1001, 16, 16, &off);
        state.evaluate(1002, 16, 16, &off);
        assert_eq!(state.cache.len(), 1, "the over-cap older tile is evicted");
        assert!(
            state.cache.contains_key(&1002),
            "the just-used tile survives"
        );
        assert_eq!(
            state.cache_bytes(),
            state.cache[&1002].rgba.len() + state.cache[&1002].coverage.len() * 4
        );

        let mut states = VectorLayerStates::default();
        states.map.insert(3, state);
        states.map.insert(
            9,
            VectorLayerState::new(9, test_vector_params([255, 0, 0, 255])),
        );
        states.touch(3);
        states.touch(9);
        states.map.get_mut(&9).unwrap().evaluate(1001, 16, 16, &off);
        states.enforce_global_budget(1024, Some(9));
        assert!(
            states.map[&3].cache.is_empty(),
            "the older layer yields first"
        );
        assert!(
            !states.map[&9].cache.is_empty(),
            "the active layer keeps its tile"
        );

        states.dirty = vec![3, 9];
        let live = std::collections::HashSet::from([3]);
        assert_eq!(states.retain_live(&live), vec![9]);
        assert!(!states.map.contains_key(&9));
        assert_eq!(states.dirty, vec![3]);
    }

    #[test]
    fn vector_refusal_is_recorded_without_fallback_allocation_and_cleared() {
        let off = SymmetrySettings::default();
        let mut state = VectorLayerState::new(3, test_vector_params([0, 255, 0, 255]));
        state.evaluate(1001, 8193, 8193, &off);
        assert_eq!(
            state.refusal,
            Some(umber_core::vector::VectorError::TooLarge {
                width: 8193,
                height: 8193,
                max_pixels: umber_core::vector::MAX_VECTOR_RASTER_PIXELS,
            })
        );
        assert!(state.cache[&1001].rgba.is_empty());
        assert!(state.cache[&1001].coverage.is_empty());

        state.evaluate(1001, 16, 16, &off);
        assert_eq!(state.refusal, None);
        assert_eq!(state.cache[&1001].rgba.len(), 16 * 16 * 4);
    }

    #[test]
    fn identical_nan_geometry_and_symmetry_do_not_invalidate() {
        let mut params = test_vector_params([0, 255, 0, 255]);
        params.shape.anchors[0].pos[0] = f32::NAN;
        let mut symmetry = mirror_x_half();
        symmetry.mirror.offset = f32::NAN;
        let mut states = VectorLayerStates::default();
        states.ensure(3, &params);
        states.dirty.clear();
        states
            .map
            .get_mut(&3)
            .unwrap()
            .evaluate(1001, 16, 16, &symmetry);
        assert_eq!(states.map[&3].evaluation_count(), 1);

        states.ensure(3, &params);
        assert!(
            states.dirty.is_empty(),
            "identical NaN geometry must not look edited"
        );
        states
            .map
            .get_mut(&3)
            .unwrap()
            .evaluate(1001, 16, 16, &symmetry);
        assert_eq!(
            states.map[&3].evaluation_count(),
            1,
            "identical NaN symmetry must reuse the cache"
        );

        states.observe_frame(&symmetry, Some((16, 16)));
        states.observe_frame(&symmetry, Some((16, 16)));
        assert!(!states.map[&3].is_dirty());
        assert!(states.dirty.is_empty());
    }

    #[test]
    fn open_seed_recomputes_every_planeless_tile() {
        let off = SymmetrySettings::default();
        let mut states = VectorLayerStates::default();
        let mut previous = VectorLayerState::new(3, test_vector_params([255, 0, 0, 255]));
        previous.evaluate(1001, 16, 16, &off);
        previous.evaluate(1002, 16, 16, &off);
        states.map.insert(3, previous);
        let next = test_vector_params([0, 255, 0, 255]);
        states.ensure_open(3, &next);
        states.mark_dirty(3);

        let state = states.map.get_mut(&3).expect("open state exists");
        let first = state.evaluate(1001, 16, 16, &off).to_vec();
        let second = state.evaluate(1002, 16, 16, &off).to_vec();
        assert_eq!(first, second);
        assert_eq!(&first[(7 * 16 + 7) * 4..][..4], &[0, 255, 0, 255]);
        assert_eq!(
            state.evaluation_count(),
            4,
            "open-seed recomputes every tile"
        );
    }

    #[test]
    fn partial_restore_is_not_clean() {
        let params = test_vector_params([255, 0, 0, 255]);
        let mut present = vec![(
            crate::paint_state::PlaneOwner::Layer(7),
            1001,
            crate::paint_state::BASE_COLOR_CHANNEL.to_string(),
        )];
        assert!(!vector_planes_complete(&present, 7, &[1001, 1002]));
        let mut states = VectorLayerStates::default();
        states.ensure_seeded(
            7,
            &params,
            vector_planes_complete(&present, 7, &[1001, 1002]),
        );
        assert!(states.map[&7].is_dirty());
        assert_eq!(states.dirty, vec![7]);

        present.push((
            crate::paint_state::PlaneOwner::Layer(7),
            1002,
            crate::paint_state::BASE_COLOR_CHANNEL.to_string(),
        ));
        assert!(vector_planes_complete(&present, 7, &[1001, 1002]));
        let mut complete = VectorLayerStates::default();
        complete.ensure_seeded(
            7,
            &params,
            vector_planes_complete(&present, 7, &[1001, 1002]),
        );
        assert!(!complete.map[&7].is_dirty());
        assert!(complete.dirty.is_empty());
    }

    #[test]
    fn ensure_seeded_honors_saved_planes_only_on_first_sight() {
        let params = test_vector_params([255, 0, 0, 255]);
        let mut states = VectorLayerStates::default();
        states.ensure_seeded(7, &params, true);
        assert!(!states.map[&7].is_dirty());
        assert!(states.dirty.is_empty());

        let changed = test_vector_params([0, 255, 0, 255]);
        states.ensure_seeded(7, &changed, true);
        assert!(states.map[&7].is_dirty());
        assert_eq!(
            states.dirty,
            vec![7],
            "an existing state's edit is never suppressed"
        );

        states.ensure_seeded(9, &params, false);
        assert!(states.map[&9].is_dirty());
        assert!(states.dirty.contains(&9));
    }

    #[test]
    fn vector_states_open_clean_or_planeless_dirty() {
        let params = test_vector_params([255, 0, 0, 255]);
        let mut opened = VectorLayerStates::default();
        opened.ensure_open(7, &params);
        assert!(!opened.map.get(&7).unwrap().is_dirty());
        assert!(opened.dirty.is_empty());
        opened.ensure_open(7, &params);
        assert!(
            opened.dirty.is_empty(),
            "re-seeding a saved plane never queues"
        );
        opened.mark_dirty(7);
        assert_eq!(opened.dirty, vec![7]);

        let mut runtime = VectorLayerStates::default();
        runtime.ensure(9, &params);
        assert!(runtime.dirty.contains(&9));
        assert!(runtime.map.get(&9).unwrap().is_dirty());
        runtime.ensure(9, &params);
        assert_eq!(
            runtime.dirty,
            vec![9],
            "an unchanged ensure does not double-queue"
        );
    }

    fn try_vector_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            return None;
        };
        if !adapter
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        {
            return None;
        }
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
            ..Default::default()
        }))
        .ok()
    }

    #[test]
    fn mixed_size_refusal_is_aggregated_in_both_orders() {
        let Some((device, queue)) = try_vector_device() else {
            assert!(
                std::env::var_os("UMBER_REQUIRE_GPU").is_none(),
                "UMBER_REQUIRE_GPU=1 but no suitable wgpu adapter is available"
            );
            eprintln!(
                "SKIP mixed_size_refusal_is_aggregated_in_both_orders: no suitable wgpu adapter"
            );
            return;
        };
        let mut paint =
            PaintState::with_target_size(device, queue, 16).expect("paint state builds");
        paint.resize_set("huge", 8193, 8193);
        paint.assign_tile_to_set(1002, "huge");

        for tiles in [[1002, 1001], [1001, 1002]] {
            let mut state = VectorLayerState::new(7, test_vector_params([255, 0, 0, 255]));
            let staged =
                refresh_vector_layer(&mut state, &mut paint, &tiles, &SymmetrySettings::default());
            assert_eq!(staged, vec![1001], "only the in-cap tile publishes");
            assert!(matches!(
                state.refusal(),
                Some(umber_core::vector::VectorError::TooLarge {
                    width: 8193,
                    height: 8193,
                    ..
                })
            ));
            assert!(!state.is_dirty(), "a completed refresh does not churn");
        }
    }

    #[test]
    fn vector_refresh_publishes_plane_without_mesh() {
        let Some((device, queue)) = try_vector_device() else {
            assert!(
                std::env::var_os("UMBER_REQUIRE_GPU").is_none(),
                "UMBER_REQUIRE_GPU=1 but no suitable wgpu adapter is available"
            );
            eprintln!("SKIP vector_refresh_publishes_plane_without_mesh: no suitable wgpu adapter");
            return;
        };
        let mut paint =
            PaintState::with_target_size(device, queue, 16).expect("paint state builds");
        paint.restore_quick_mask(crate::paint_state::DEFAULT_TEXTURE_SET, left_half_mask());
        let mut state = VectorLayerState::new(7, test_vector_params([255, 0, 0, 255]));
        let staged = refresh_vector_layer(
            &mut state,
            &mut paint,
            &[1001],
            &SymmetrySettings::default(),
        );
        assert_eq!(staged, vec![1001]);
        let plane = paint
            .read_plane(
                crate::paint_state::PlaneOwner::Layer(7),
                1001,
                crate::paint_state::BASE_COLOR_CHANNEL,
            )
            .expect("the vector plane materialized");
        assert_eq!((plane.width, plane.height), (16, 16));
        let at = |x: u32, y: u32| {
            let i = ((y * 16 + x) * 4) as usize;
            &plane.data[i..i + 4]
        };
        assert_eq!(at(7, 7), &[255, 0, 0, 255], "inside shape and mask");
        assert_eq!(at(11, 7), &[0, 0, 0, 0], "inside shape but quick-masked");
        assert_eq!(at(0, 0), &[0, 0, 0, 0], "outside the shape");

        state.dirty = true; // budget-recovery republish must reuse the cache.
        let republished = refresh_vector_layer(
            &mut state,
            &mut paint,
            &[1001],
            &SymmetrySettings::default(),
        );
        assert_eq!(republished, vec![1001]);
        assert_eq!(state.evaluation_count(), 1, "the cached tile is reused");
        assert!(
            !state.is_dirty(),
            "a successful cache-hit refresh drains dirty"
        );

        state.set_params(umber_core::vector::VectorParams {
            shape: umber_core::vector::VectorShape {
                anchors: vec![
                    umber_core::vector::Anchor::corner([0.25, 0.5]);
                    umber_core::vector::MAX_VECTOR_ANCHORS + 1
                ],
                ..umber_core::vector::VectorShape::empty()
            },
        });
        let refused = refresh_vector_layer(
            &mut state,
            &mut paint,
            &[1001],
            &SymmetrySettings::default(),
        );
        assert_eq!(
            refused,
            vec![1001],
            "a valid-to-invalid edit erases the stale owned plane"
        );
        assert!(matches!(
            state.refusal(),
            Some(umber_core::vector::VectorError::TooComplex {
                what: "anchors",
                ..
            })
        ));
        assert!(!state.is_dirty(), "a refused raster does not churn");
        let plane = paint
            .read_plane(
                crate::paint_state::PlaneOwner::Layer(7),
                1001,
                crate::paint_state::BASE_COLOR_CHANNEL,
            )
            .expect("the refused plane was explicitly erased");
        assert!(plane.data.iter().all(|byte| *byte == 0));

        state.set_params(test_vector_params([255, 0, 0, 255]));
        assert_eq!(
            refresh_vector_layer(
                &mut state,
                &mut paint,
                &[1001],
                &SymmetrySettings::default(),
            ),
            vec![1001],
            "a valid edit recovers after refusal"
        );
        assert!(state.refusal().is_none());

        state.set_params(umber_core::vector::VectorParams {
            shape: umber_core::vector::VectorShape::empty(),
        });
        let erased = refresh_vector_layer(&mut state, &mut paint, &[1001], &mirror_x_half());
        assert_eq!(erased, vec![1001], "an existing plane is explicitly erased");
        assert!(!state.is_dirty());
        let plane = paint
            .read_plane(
                crate::paint_state::PlaneOwner::Layer(7),
                1001,
                crate::paint_state::BASE_COLOR_CHANNEL,
            )
            .expect("the erased plane remains present");
        assert!(plane.data.iter().all(|byte| *byte == 0));

        let mut empty = VectorLayerState::new(
            99,
            umber_core::vector::VectorParams {
                shape: umber_core::vector::VectorShape::empty(),
            },
        );
        let staged = refresh_vector_layer(&mut empty, &mut paint, &[1001], &mirror_x_half());
        assert!(
            staged.is_empty(),
            "a planeless empty layer allocates nothing"
        );
        assert!(!empty.is_dirty());
        assert!(
            !paint.planes_present().iter().any(|(owner, tile, channel)| {
                *owner == crate::paint_state::PlaneOwner::Layer(99)
                    && *tile == 1001
                    && channel == crate::paint_state::BASE_COLOR_CHANNEL
            })
        );
    }
}
