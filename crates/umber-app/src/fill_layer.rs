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
//!    (coverage + sampled RGBA, uncovered texels transparent).
//! 2. [`apply_symmetry`] — THE awaited caller of
//!    [`umber_brush::symmetry::symmetrize_coverage`]: the raw coverage goes
//!    through the union of its symmetric images, and each newly covered
//!    texel copies its first covered sibling's color (identity first, so
//!    originally covered texels keep their own sample). Symmetry off is
//!    the zero-cost path (the tile passes through untouched).
//! 3. [`PaintState::publish_fill`](crate::paint_state::PaintState::publish_fill)
//!    — the tile/texture-set plumbing: the evaluated bytes ride the same
//!    `WriteRegion` queue as retouch strokes into the tile's GPU target.
//! 4. [`composite_fill_over`] — the stack composite step for readers that
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
use umber_brush::{SymTransform, SymmetrySettings};
use umber_core::{BlendMode, Layer, LayerCommand, LayerKind};
use umber_mesh::{
    evaluate_fill_tile, FillParams, FillProjection, FillSource, FillTile, MeshData, ProjAxis,
};

use crate::document::Document;
use crate::i18n::tr;
use crate::paint_state::PaintState;

/// One tile's evaluated fill bytes (post-symmetry, pre-composite).
pub struct CachedTile {
    /// The tile size (`size`x`size`) this was evaluated at.
    pub size: u32,
    /// RGBA8 bytes, row-major top-down (the paint-target layout).
    pub rgba: Vec<u8>,
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

/// Re-evaluates `state` for every tile in `tiles` at `size`x`size` and
/// stages each through the paint session's tile plumbing
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
        if paint.publish_fill(tile, size, size, rgba) {
            staged.push(tile);
        }
    }
    staged
}

/// The fill section of the Properties panel: one collapsible row per
/// [`LayerKind::Fill`](umber_core::LayerKind::Fill) layer in the stack
/// (bottom-to-top), each with the projection picker, the projection's
/// axis/sharpness rows, and the v1 source editor (color / gradient / noise;
/// a graph snapshot shows its dims — wiring a fresh graph output into the
/// snapshot is the named follow-up).
///
/// Edits dispatch [`LayerCommand::SetFill`](umber_core::LayerCommand::SetFill)
/// through [`Document::run`](crate::document::Document::run) ONLY when a
/// widget changed (each edit is one undoable, journaled op — scrubbing a
/// slider still emits per-frame ops, matching the brush panel's live-edit
/// convention). All strings go through [`tr`](crate::i18n::tr).
pub fn fill_section_ui(ui: &mut egui::Ui, doc: &mut Document) {
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
}
