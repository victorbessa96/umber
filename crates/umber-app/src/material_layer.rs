//! Dynamic material layers: the headless eval driver (Kill A).
//!
//! (`docs/specs/dynamic-materials-design.md` §1–§3.) A material is one layer
//! that writes several channels at once: its graph DECLARES the channel set
//! (one `material_output` node per channel), and each declared channel
//! carries its own blend mode, opacity and enable in `MaterialParams`. This
//! module is deliberately headless (no egui): the bridge, discovery,
//! validation, resampler, per-channel composite and refresh driver are all
//! testable without a GPU session. Kill B owns the frame wiring (`main.rs`),
//! the Layers-panel rows and the badges.
//!
//! # Data flow (the fill precedent's destructive publish)
//!
//! There is no per-layer stack walk: the pool holds one flattened target per
//! tile and channel. A refresh, per dirty material layer, per present tile,
//! per declared + enabled channel: `read_channel_tile` (the CURRENT
//! flattened texels) → composite the resampled material output over it
//! (§1.3, one composite per channel) → `publish_fill_channel`.
//!
//! # v1 boundaries (each fixed only by per-layer pool storage)
//!
//! * Stack order is not preserved: a material composites over whatever the
//!   tile holds when it refreshes. Dirty layers refresh bottom-to-top.
//! * Re-eval compounds (Multiply darkens each time — pinned by test).
//! * Nothing un-publishes: hiding, disabling, deleting or failing stops
//!   FUTURE publishes; published pixels stay. Undo of `SetMaterial` restores
//!   PARAMS, not pixels.
//! * Anchors read the flattened channel (self-feedback on re-eval).
//! * The layer's own `blend_mode` is INERT on Material layers (carried,
//!   never read); layer `opacity` multiplies every channel's opacity and the
//!   layer mask gates every written channel.
//! * Eval runs ONCE per material per refresh at the SET size (never a
//!   literal) and resamples per channel — never once per channel size.
//! * A state created by the project-open path is seeded CLEAN (the saved
//!   sidecars already hold the published result; re-publishing on open
//!   would compound every non-Normal material). Runtime states start dirty.
//!   This deliberately departs from `FillLayerStates::ensure`, which dirties
//!   every fresh state.
//! * Memory: each cached Image node costs `w·h·4` bytes — nodes × 64 MiB at
//!   4096² per cache, times tiles when per-tile. `Layer::memory_bytes`
//!   counts metadata only; the cache is reported here, never there.
//!
//! # Judgment calls
//!
//! * The driver works against [`MaterialTileIo`], not `PaintState` directly:
//!   the trait is the headless seam (tests drive a scripted in-memory
//!   implementation; Kill B's frame wiring adapts the paint session).
//! * A per-tile eval cache is used when the graph has an `anchor` or
//!   `mesh_map_generator` NODE, or any `Anchor` PARAM value: both resolve
//!   through per-tile externals, so sharing one cache across tiles would
//!   serve tile A's anchor bytes to tile B. (The doc names the node case;
//!   the param case rides the same externals and is folded in here.)
//! * An invisible dirty layer KEEPS its dirty flag: visibility toggles do
//!   not flow through `ensure`, so clearing it would strand the pending
//!   refresh when the layer becomes visible again.
//! * A failed refresh clears the layer dirty flag but keeps the engine dirty
//!   set: the failure needs new inputs (a definition/params change
//!   re-dirties through `ensure`), while the engine set still names the
//!   failed node so the next attempt re-runs it plus downstream.
//! * Channel-only edits (blend/opacity/enable) re-run the COMPOSITE but not
//!   the graph: the engine dirty set stays empty and warm caches are reused.
//! * `material: None`, a missing definition id and an unparseable definition
//!   all publish NOTHING with a typed error — never a refused open, never
//!   silent wrong pixels. Every eval `Err` is whole-material (nothing
//!   publishes); a `Uniform` at a `material_output` or a forbidden blend on
//!   that channel skips ONLY that channel.
//! * A `publish_channel` rejection (`false`) is not modeled in the report:
//!   the publish was attempted destructively like every other path, and the
//!   pool logs the drop. Surfacing per-tile publish failures is a Kill B
//!   follow-up if the wiring needs it.

use std::collections::{BTreeSet, HashMap, HashSet};

use umber_core::layers::{BlendMode, Layer, LayerKind, MaterialParamValue};
use umber_core::MaterialParams;
use umber_gpu::{composite_pixel_masked, f32_to_u8};
use umber_graph::mtlx::from_mtlx;
use umber_graph::{
    eval_graph_cached, AnchorRef, EvalCache, EvalContext, EvalError, Graph, ImageBuffer,
    NodeOutput, NodeRegistry, ParamValue,
};

/// Everything that can go wrong refreshing a material layer. Whole-material
/// cases publish NOTHING — the last published result stays. No field is ever
/// named `source` (the thiserror trap: it would become the error source).
#[derive(Debug, thiserror::Error)]
pub enum MaterialError {
    /// The layer carries no material params (legacy/corrupt carry).
    #[error("material layer {layer} has no material params")]
    NoParams {
        /// The layer that cannot evaluate.
        layer: u64,
    },
    /// `material_id` names no embedded definition.
    #[error("material layer {layer} references missing definition {material_id:?}")]
    MissingDefinition {
        /// The layer that cannot evaluate.
        layer: u64,
        /// The id with no embedded definition.
        material_id: String,
    },
    /// The embedded definition does not parse as `.mtlx`.
    #[error("material layer {layer} has an unparseable definition: {message}")]
    DefinitionParse {
        /// The layer that cannot evaluate.
        layer: u64,
        /// The parse failure.
        message: String,
    },
    /// The graph failed to evaluate (unknown node, bad param, bad override
    /// target, unbound anchor, missing mesh map, …). Nothing publishes.
    #[error(transparent)]
    Eval(#[from] EvalError),
    /// A `material_output.channel` names a channel the set does not have.
    #[error("material layer {layer} declares unknown channel {channel:?}")]
    UnknownChannel {
        /// The layer that cannot evaluate.
        layer: u64,
        /// The channel the set does not name.
        channel: String,
        /// The set's channels (for the badge text).
        known: Vec<String>,
    },
    /// Two `material_output` nodes name the same channel (v1 is one output
    /// per channel).
    #[error("material layer {layer} declares channel {channel:?} twice")]
    DuplicateChannel {
        /// The layer that cannot evaluate.
        layer: u64,
        /// The twice-declared channel.
        channel: String,
    },
    /// A `material_output` node's `channel` param is missing or not an
    /// `Asset` string — never a silent default channel (that would paint the
    /// wrong channel).
    #[error("material layer {layer} node {node} has a bad channel param")]
    BadChannelParam {
        /// The layer that cannot evaluate.
        layer: u64,
        /// The offending node.
        node: u64,
    },
    /// An override names a missing node or a missing param on it.
    #[error("material layer {layer} override targets missing node {node} param {param:?}")]
    MissingOverrideTarget {
        /// The layer that cannot evaluate.
        layer: u64,
        /// The override's node id.
        node: u64,
        /// The override's param name.
        param: String,
    },
    /// Reached an allegedly unreachable state without panicking (a node the
    /// successful eval should have produced is absent). Loud, never wrong
    /// pixels.
    #[error("material layer {layer} internal error: {message}")]
    Internal {
        /// The layer that cannot evaluate.
        layer: u64,
        /// What went wrong.
        message: String,
    },
}

/// One channel's refresh outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelReport {
    /// The declared channel name.
    pub name: String,
    /// What happened to it.
    pub status: ChannelStatus,
}

/// Whether one declared channel published.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelStatus {
    /// Composited and published.
    Published,
    /// Disabled in `MaterialParams` (never read, never written).
    SkippedDisabled,
    /// A `Uniform` arrived at its `material_output` (siblings unaffected).
    SkippedUniform,
    /// A forbidden blend on this channel (`Passthrough` anywhere, or a
    /// normal-map mode off the `normal` channel). The material path never
    /// falls back to Normal.
    SkippedBlend,
}

/// One layer's refresh outcome (the badge + status-line source for Kill B).
#[derive(Debug)]
pub enum MaterialResult {
    /// The layer refreshed: per-channel reports (possibly all skipped).
    Refreshed {
        /// One report per declared channel, in declaration order.
        channels: Vec<ChannelReport>,
        /// Listed channels the definition no longer declares: ignored, and
        /// the status line names them once (Kill B).
        ignored_undeclared: Vec<String>,
    },
    /// The layer is invisible: tiles not read, not composited, not written.
    /// The dirty flag is KEPT (see the module docs).
    SkippedInvisible,
    /// Whole-material no-op: nothing published for ANY channel (the last
    /// published result stays).
    Failed(MaterialError),
}

/// One refreshed layer plus its outcome, in refresh order.
#[derive(Debug)]
pub struct MaterialRefreshReport {
    /// The refreshed layer's id.
    pub layer: u64,
    /// What happened.
    pub result: MaterialResult,
}

/// Converts the umber-core serde mirror to the graph value: the six numeric
/// variants map 1:1 (the bridge Kill A owns).
pub fn material_param_to_graph(value: &MaterialParamValue) -> ParamValue {
    match value {
        MaterialParamValue::Float(v) => ParamValue::Float(*v),
        MaterialParamValue::Vec2(v) => ParamValue::Vec2(*v),
        MaterialParamValue::Vec3(v) => ParamValue::Vec3(*v),
        MaterialParamValue::Color(v) => ParamValue::Color(*v),
        MaterialParamValue::Int(v) => ParamValue::Int(*v),
        MaterialParamValue::Bool(v) => ParamValue::Bool(*v),
    }
}

/// One declared channel: a `material_output` node's id plus its channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialOutput {
    /// The declaring node's id.
    pub node_id: u64,
    /// The channel it writes.
    pub channel: String,
}

/// One declared channel's composite-time plan.
#[derive(Debug, Clone)]
struct ChannelPlan {
    output: MaterialOutput,
    blend: BlendMode,
    opacity: f32,
    enabled: bool,
}

/// Discovers the declared channel set: every `material_output` node, sorted
/// by node id (deterministic — graph node order is not).
///
/// # Errors
///
/// [`MaterialError::BadChannelParam`] on a missing/non-`Asset` channel;
/// [`MaterialError::DuplicateChannel`] on two outputs for one channel.
pub fn discover_material_outputs(
    layer: u64,
    graph: &Graph,
) -> Result<Vec<MaterialOutput>, MaterialError> {
    let mut ids: Vec<u64> = graph
        .nodes
        .iter()
        .filter(|n| n.node_def == "material_output")
        .map(|n| n.id)
        .collect();
    ids.sort_unstable();
    let mut outputs = Vec::with_capacity(ids.len());
    let mut seen = HashSet::new();
    for id in ids {
        let node =
            graph
                .nodes
                .iter()
                .find(|n| n.id == id)
                .ok_or_else(|| MaterialError::Internal {
                    layer,
                    message: format!("declared node {id} vanished"),
                })?;
        let channel = match node.params.iter().find(|(n, _)| n == "channel") {
            Some((_, ParamValue::Asset(channel))) => channel.clone(),
            _ => {
                return Err(MaterialError::BadChannelParam { layer, node: id });
            }
        };
        if !seen.insert(channel.clone()) {
            return Err(MaterialError::DuplicateChannel { layer, channel });
        }
        outputs.push(MaterialOutput {
            node_id: id,
            channel,
        });
    }
    Ok(outputs)
}

/// Validates the declared set against the texture set's channels.
///
/// # Errors
///
/// [`MaterialError::UnknownChannel`] on the first channel the set lacks.
pub fn validate_material_channels(
    layer: u64,
    outputs: &[MaterialOutput],
    set_channels: &[String],
) -> Result<(), MaterialError> {
    for output in outputs {
        if !set_channels.iter().any(|c| c == &output.channel) {
            return Err(MaterialError::UnknownChannel {
                layer,
                channel: output.channel.clone(),
                known: set_channels.to_vec(),
            });
        }
    }
    Ok(())
}

/// Applies the layer's overrides onto its copy of the definition graph
/// (before eval). Any param kind may be overridden (including `NodeRef` and
/// `Anchor` bindings — the engine reports whatever breaks downstream).
///
/// # Errors
///
/// [`MaterialError::MissingOverrideTarget`] naming the missing node/param.
pub fn apply_material_overrides(
    layer: u64,
    graph: &mut Graph,
    params: &MaterialParams,
) -> Result<(), MaterialError> {
    for override_ in &params.params {
        let node_id = override_.node_id;
        let param_name = override_.param.clone();
        let Some(node) = graph.nodes.iter_mut().find(|n| n.id == node_id) else {
            return Err(MaterialError::MissingOverrideTarget {
                layer,
                node: node_id,
                param: param_name,
            });
        };
        let value = material_param_to_graph(&override_.value);
        match node.params.iter_mut().find(|(n, _)| n == &param_name) {
            Some((_, slot)) => *slot = value,
            None => {
                return Err(MaterialError::MissingOverrideTarget {
                    layer,
                    node: node_id,
                    param: param_name,
                });
            }
        }
    }
    Ok(())
}

/// Bilinear RGBA8 resample, texel-centre aligned
/// (`src = (dst + 0.5) · src_w / dst_w − 0.5`, edge-clamped), each lane
/// `round`-quantized through the compositor's quantizer. Identity sizes are
/// byte-exact (an explicit copy, not a float round-trip).
///
/// When `channel == "normal"` (the NAME, not a blend gate) the bilinear lerp
/// runs on byte values as f32 and renormalization runs on that f32 result
/// BEFORE quantizing: decode RGB `v/255·2 − 1`, normalize, length `< 1e-6`
/// (or non-finite) → `(0, 0, 1)`, re-encode `(n + 1)/2·255`, then `round`.
/// Alpha is untouched. The composite-time lerp that follows stays
/// UNNORMALIZED, exactly as `composite_pixel_masked` does today —
/// documented, not fixed here.
///
/// Malformed input (zero dims, wrong-length bytes) yields a transparent
/// image, never a panic.
pub fn resample_image_rgba8(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
    channel: &str,
) -> Vec<u8> {
    let expected = src_w as usize * src_h as usize * 4;
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 || src.len() != expected {
        return vec![0u8; dst_w as usize * dst_h as usize * 4];
    }
    if src_w == dst_w && src_h == dst_h {
        return src.to_vec();
    }
    let normal = channel == "normal";
    let mut out = Vec::with_capacity(dst_w as usize * dst_h as usize * 4);
    for dy in 0..dst_h {
        let sy = (dy as f32 + 0.5) * src_h as f32 / dst_h as f32 - 0.5;
        let sy_clamped = sy.clamp(0.0, (src_h - 1) as f32);
        let y0 = sy_clamped.floor() as u32;
        let y1 = (y0 + 1).min(src_h - 1);
        let fy = sy_clamped - y0 as f32;
        for dx in 0..dst_w {
            let sx = (dx as f32 + 0.5) * src_w as f32 / dst_w as f32 - 0.5;
            let sx_clamped = sx.clamp(0.0, (src_w - 1) as f32);
            let x0 = sx_clamped.floor() as u32;
            let x1 = (x0 + 1).min(src_w - 1);
            let fx = sx_clamped - x0 as f32;
            let texel = |x: u32, y: u32| {
                let i = (y as usize * src_w as usize + x as usize) * 4;
                [src[i], src[i + 1], src[i + 2], src[i + 3]]
            };
            let tl = texel(x0, y0);
            let tr = texel(x1, y0);
            let bl = texel(x0, y1);
            let br = texel(x1, y1);
            // The lerp in byte-domain f32 (quantized only at the end, so the
            // normal-renormalize guard below sees the true f32 value).
            let mut value = [0.0f32; 4];
            for lane in 0..4 {
                let single_x = x1 == x0;
                let single_y = y1 == y0;
                value[lane] = if single_x && single_y {
                    f32::from(tl[lane])
                } else if single_x {
                    f32::from(tl[lane]) * (1.0 - fy) + f32::from(bl[lane]) * fy
                } else if single_y {
                    f32::from(tl[lane]) * (1.0 - fx) + f32::from(tr[lane]) * fx
                } else {
                    let top = f32::from(tl[lane]) * (1.0 - fx) + f32::from(tr[lane]) * fx;
                    let bot = f32::from(bl[lane]) * (1.0 - fx) + f32::from(br[lane]) * fx;
                    top * (1.0 - fy) + bot * fy
                };
            }
            if normal {
                let decoded = [
                    value[0] / 255.0 * 2.0 - 1.0,
                    value[1] / 255.0 * 2.0 - 1.0,
                    value[2] / 255.0 * 2.0 - 1.0,
                ];
                let len_sq =
                    decoded[0] * decoded[0] + decoded[1] * decoded[1] + decoded[2] * decoded[2];
                let len = len_sq.sqrt();
                let (nx, ny, nz) = if !len.is_finite() || len < 1e-6 {
                    // Covers exact zero (bytes 127 and 128 lerped at weight
                    // 0.5 decode to exactly 0) and f32 dust.
                    (0.0, 0.0, 1.0)
                } else {
                    (decoded[0] / len, decoded[1] / len, decoded[2] / len)
                };
                value[0] = (nx + 1.0) / 2.0 * 255.0;
                value[1] = (ny + 1.0) / 2.0 * 255.0;
                value[2] = (nz + 1.0) / 2.0 * 255.0;
            }
            out.extend_from_slice(&[
                f32_to_u8(value[0] / 255.0),
                f32_to_u8(value[1] / 255.0),
                f32_to_u8(value[2] / 255.0),
                f32_to_u8(value[3] / 255.0),
            ]);
        }
    }
    out
}

/// Nearest-neighbour single-plane resample for the layer-mask plane (the
/// mask is SET-sized while channels may be overridden — without this the CPU
/// composite fails loudly and the GPU path zero-fills).
///
/// Malformed input yields a fully-open plane (an attached mask with no plane
/// reads as fully white — attaching must never black out the layer).
pub fn resample_mask_nearest(
    mask: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
) -> Vec<u8> {
    let expected = src_w as usize * src_h as usize;
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 || mask.len() != expected {
        return vec![255u8; dst_w as usize * dst_h as usize];
    }
    if src_w == dst_w && src_h == dst_h {
        return mask.to_vec();
    }
    let mut out = Vec::with_capacity(dst_w as usize * dst_h as usize);
    for dy in 0..dst_h {
        let sy = (dy as usize * src_h as usize / dst_h as usize).min(src_h as usize - 1);
        for dx in 0..dst_w {
            let sx = (dx as usize * src_w as usize / dst_w as usize).min(src_w as usize - 1);
            out.push(mask[sy * src_w as usize + sx]);
        }
    }
    out
}

/// Composites one material output over the channel's current texels in
/// place: per texel `composite_pixel_masked(src, dst, blend, opacity, mask)`
/// with u8 → f32 as `byte / 255` and f32 → u8 through the compositor's
/// quantizer. `opacity` is the already-formed `layer.opacity ×
/// channel.opacity` product (clamped once inside the primitive, so a
/// non-finite factor zeroes coverage). `mask` is the resampled mask plane
/// (`None` = fully open).
///
/// Length contract: composites the shared texel prefix, never a panic (tile
/// resizes mid-session reorder evaluations — they must not crash).
pub fn composite_material_channel(
    dst: &mut [u8],
    src: &[u8],
    width: u32,
    height: u32,
    blend: BlendMode,
    opacity: f32,
    mask: Option<&[u8]>,
) {
    let texels = width as usize * height as usize;
    let n = (dst.len() / 4)
        .min(src.len() / 4)
        .min(texels)
        .min(mask.map(|m| m.len()).unwrap_or(usize::MAX));
    for i in 0..n {
        let s = &src[i * 4..i * 4 + 4];
        let d = &dst[i * 4..i * 4 + 4];
        let src_px = [
            f32::from(s[0]) / 255.0,
            f32::from(s[1]) / 255.0,
            f32::from(s[2]) / 255.0,
            f32::from(s[3]) / 255.0,
        ];
        let dst_px = [
            f32::from(d[0]) / 255.0,
            f32::from(d[1]) / 255.0,
            f32::from(d[2]) / 255.0,
            f32::from(d[3]) / 255.0,
        ];
        let mask_px = mask.map(|m| f32::from(m[i]) / 255.0).unwrap_or(1.0);
        let Some(out) = composite_pixel_masked(src_px, dst_px, blend, opacity, mask_px) else {
            // Unreachable by construction (the driver gates `Passthrough`
            // before compositing, and every other mode is total) — keep the
            // destination byte rather than inventing pixels.
            continue;
        };
        let quantized = [
            f32_to_u8(out[0]),
            f32_to_u8(out[1]),
            f32_to_u8(out[2]),
            f32_to_u8(out[3]),
        ];
        dst[i * 4..i * 4 + 4].copy_from_slice(&quantized);
    }
}

/// One material layer's live eval state: its params, its embedded
/// definition, and its eval caches.
#[derive(Debug)]
pub struct MaterialLayerState {
    layer_id: u64,
    params: MaterialParams,
    definition: String,
    set_size: (u32, u32),
    per_tile: HashMap<u16, EvalCache>,
    shared: EvalCache,
    engine_dirty: HashSet<u64>,
    full_eval: bool,
    dirty: bool,
}

impl MaterialLayerState {
    /// Tracks `layer_id` with `params` + `definition` at `set_size`.
    /// `dirty`: true for runtime states (nothing evaluated yet), false for
    /// project-open seeding (the sidecars already hold the result).
    fn new(
        layer_id: u64,
        params: MaterialParams,
        definition: String,
        set_size: (u32, u32),
        dirty: bool,
    ) -> Self {
        Self {
            layer_id,
            params,
            definition,
            set_size,
            per_tile: HashMap::new(),
            shared: EvalCache::new(),
            engine_dirty: HashSet::new(),
            full_eval: true,
            dirty,
        }
    }

    /// The tracked layer's id.
    pub fn layer_id(&self) -> u64 {
        self.layer_id
    }

    /// Whether a refresh is pending.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The params the next refresh evaluates.
    pub fn params(&self) -> &MaterialParams {
        &self.params
    }

    /// Adopts new params/definition/size (panel edits, undo/redo replays of
    /// `SetMaterial`, definition re-saves, resizes all land here before the
    /// frame's refresh drain). Returns whether the layer is dirty now. Any
    /// params change dirties the LAYER (the composite re-runs); only
    /// override/material-id changes dirty the ENGINE (channel-only edits
    /// reuse warm caches); definition and size changes clear the caches.
    fn sync(&mut self, params: &MaterialParams, definition: &str, set_size: (u32, u32)) -> bool {
        if *params != self.params {
            if params.material_id != self.params.material_id {
                self.full_eval = true;
            } else {
                for node_id in self
                    .params
                    .params
                    .iter()
                    .chain(params.params.iter())
                    .map(|o| o.node_id)
                {
                    self.engine_dirty.insert(node_id);
                }
            }
            self.params = params.clone();
            self.dirty = true;
        }
        if definition != self.definition {
            self.definition = definition.to_string();
            self.full_eval = true;
            self.dirty = true;
        }
        if set_size != self.set_size {
            self.set_size = set_size;
            self.full_eval = true;
            self.dirty = true;
        }
        self.dirty
    }
}

/// The live eval states beside the document (the `FillLayerStates`
/// precedent): one state per live material layer plus the dirty queue the
/// frame's refresh drain consumes.
#[derive(Debug, Default)]
pub struct MaterialLayerStates {
    states: HashMap<u64, MaterialLayerState>,
    dirty: Vec<u64>,
}

impl MaterialLayerStates {
    /// Ensures a state exists for `layer_id` and adopts the current
    /// params/definition/size. A fresh state or any change marks the layer
    /// dirty and queues it. Runtime path (layer add, `SetMaterial`).
    pub fn ensure(
        &mut self,
        layer_id: u64,
        params: &MaterialParams,
        definition: &str,
        set_size: (u32, u32),
    ) {
        let state = self.states.entry(layer_id).or_insert_with(|| {
            MaterialLayerState::new(
                layer_id,
                params.clone(),
                definition.to_string(),
                set_size,
                true,
            )
        });
        state.sync(params, definition, set_size);
        if state.is_dirty() && !self.dirty.contains(&layer_id) {
            self.dirty.push(layer_id);
        }
    }

    /// The project-open path: a fresh state is seeded CLEAN (no publish on
    /// open — the sidecars already hold the result); only a CHANGED state
    /// dirties. Existing states behave exactly like [`Self::ensure`].
    pub fn ensure_open(
        &mut self,
        layer_id: u64,
        params: &MaterialParams,
        definition: &str,
        set_size: (u32, u32),
    ) {
        let state = match self.states.get_mut(&layer_id) {
            None => {
                self.states.insert(
                    layer_id,
                    MaterialLayerState::new(
                        layer_id,
                        params.clone(),
                        definition.to_string(),
                        set_size,
                        false,
                    ),
                );
                return;
            }
            Some(state) => {
                state.sync(params, definition, set_size);
                state.is_dirty()
            }
        };
        if state && !self.dirty.contains(&layer_id) {
            self.dirty.push(layer_id);
        }
    }

    /// Whether `layer_id` has a pending refresh.
    pub fn is_dirty(&self, layer_id: u64) -> bool {
        self.states.get(&layer_id).is_some_and(|s| s.is_dirty())
    }

    /// Queues a refresh for a tracked layer without changing its params
    /// (the explicit "refresh material" action: re-publish on demand and
    /// the re-eval-compounds probe). No-op for untracked layers.
    pub fn mark_dirty(&mut self, layer_id: u64) {
        if let Some(state) = self.states.get_mut(&layer_id) {
            state.dirty = true;
            if !self.dirty.contains(&layer_id) {
                self.dirty.push(layer_id);
            }
        }
    }

    /// The queued dirty layer ids, in queue order.
    pub fn dirty_ids(&self) -> &[u64] {
        &self.dirty
    }

    /// The state for `layer_id`, if tracked.
    pub fn state(&self, layer_id: u64) -> Option<&MaterialLayerState> {
        self.states.get(&layer_id)
    }
}

/// The tile-store seam the refresh driver runs against (the headless
/// boundary: tests drive an in-memory implementation, Kill B adapts the
/// paint session — whose `read_channel_tile` never side-effect-creates and
/// whose `publish_fill_channel` materializes — at the call site).
pub trait MaterialTileIo {
    /// The set's paint-target size (the eval resolution — never a literal).
    fn set_size(&self) -> (u32, u32);
    /// `channel`'s paint-target size (its override, else the set's size).
    fn channel_size(&self, channel: &str) -> (u32, u32);
    /// The tiles to refresh, ascending.
    fn tiles(&self) -> Vec<u16>;
    /// `tile`'s `channel` texels, or `None` when never routed (composites
    /// over transparent; never side-effect-creates).
    fn read_channel(&mut self, tile: u16, channel: &str) -> Option<(u32, u32, Vec<u8>)>;
    /// Uploads composited texels into `tile`'s `channel`, materializing it.
    fn publish_channel(
        &mut self,
        tile: u16,
        channel: &str,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> bool;
    /// The layer's SET-sized mask plane bytes, or `None` when absent (reads
    /// as fully white — attaching never blacks out).
    fn mask_plane(&mut self, layer_id: u64, tile: u16) -> Option<(u32, u32, Vec<u8>)>;
    /// One anchor's composited channel image for `tile`, or `None` when the
    /// source layer was deleted or never composited (an eval error, never a
    /// silent default).
    fn anchor_image(&mut self, tile: u16, layer: u64, channel: &str)
        -> Option<(u32, u32, Vec<u8>)>;
    /// One baked-map image by name, or `None` when unprovided (Kill B
    /// supplies the Graph panel's flat placeholders; tests pass explicit
    /// images).
    fn mesh_map_image(&mut self, name: &str) -> Option<(u32, u32, Vec<u8>)>;
}

/// Refreshes every dirty material layer in `layers` order (bottom-to-top —
/// the caller passes stack order; the driver honors call order), draining
/// [`MaterialLayerStates`]' queue. Layers deleted since queueing are skipped
/// (the fill ghost-publish guard); non-material layers never have states.
///
/// Reports come back in refresh order. Invisible layers report
/// [`MaterialResult::SkippedInvisible`] and KEEP their dirty flag; every
/// other attempt (published or failed) clears it.
pub fn refresh_dirty_materials(
    states: &mut MaterialLayerStates,
    layers: &[Layer],
    definitions: &HashMap<String, String>,
    set_channels: &[String],
    registry: &NodeRegistry,
    io: &mut impl MaterialTileIo,
) -> Vec<MaterialRefreshReport> {
    let mut queue = std::mem::take(&mut states.dirty);
    let mut reports = Vec::new();
    for layer in layers {
        let Some(pos) = queue.iter().position(|id| *id == layer.id) else {
            continue;
        };
        queue.remove(pos);
        if layer.kind != LayerKind::Material {
            // A queued id for a non-material layer is stale (kinds never
            // mutate, so no state should exist): dropped, never refreshed.
            continue;
        }
        let Some(state) = states.states.get_mut(&layer.id) else {
            continue;
        };
        if !layer.visible {
            states.dirty.push(layer.id);
            reports.push(MaterialRefreshReport {
                layer: layer.id,
                result: MaterialResult::SkippedInvisible,
            });
            continue;
        }
        let params = match layer.material.clone() {
            Some(params) => params,
            None => {
                state.dirty = false;
                reports.push(MaterialRefreshReport {
                    layer: layer.id,
                    result: MaterialResult::Failed(MaterialError::NoParams { layer: layer.id }),
                });
                continue;
            }
        };
        // Adopt document-side edits made since `ensure` (belt and braces
        // beside the frame's adoption pass — after `ensure` this is a no-op;
        // direct driver callers skip `ensure`, so the refresh still sees
        // current params).
        let definition_hint = state.definition.clone();
        let size_hint = state.set_size;
        state.sync(&params, &definition_hint, size_hint);
        let result = refresh_one(
            state,
            layer,
            &params,
            definitions,
            set_channels,
            registry,
            io,
        );
        state.dirty = false;
        reports.push(MaterialRefreshReport {
            layer: layer.id,
            result,
        });
    }
    // Leftover queue entries name deleted layers: dropped. (`states.dirty`
    // already holds the invisible layers re-queued above; `queue` itself is
    // discarded.)
    drop(queue);
    reports
}

/// The per-layer refresh: definition lookup → parse → overrides →
/// discovery/validation → eval (whole-material on `Err`) → per-channel
/// composite + publish. Reads `state`'s caches and engine dirty set; updates
/// them on success.
fn refresh_one(
    state: &mut MaterialLayerState,
    layer: &Layer,
    params: &MaterialParams,
    definitions: &HashMap<String, String>,
    set_channels: &[String],
    registry: &NodeRegistry,
    io: &mut impl MaterialTileIo,
) -> MaterialResult {
    let layer_id = layer.id;
    let material_id = params.material_id.clone();
    let Some(definition) = definitions.get(&material_id) else {
        return MaterialResult::Failed(MaterialError::MissingDefinition {
            layer: layer_id,
            material_id,
        });
    };
    if definition != &state.definition {
        state.definition = definition.clone();
        state.full_eval = true;
    }
    let (graph, _, _) = match from_mtlx(definition) {
        Ok(parsed) => parsed,
        Err(err) => {
            return MaterialResult::Failed(MaterialError::DefinitionParse {
                layer: layer_id,
                message: err.to_string(),
            });
        }
    };
    let mut graph = graph;
    if let Err(err) = apply_material_overrides(layer_id, &mut graph, params) {
        return MaterialResult::Failed(err);
    }
    let outputs = match discover_material_outputs(layer_id, &graph) {
        Ok(outputs) => outputs,
        Err(err) => return MaterialResult::Failed(err),
    };
    if let Err(err) = validate_material_channels(layer_id, &outputs, set_channels) {
        return MaterialResult::Failed(err);
    }

    // Per-channel controls: the definition wins — declared channels missing
    // from the list use the defaults (Normal/1.0/enabled); listed channels
    // the definition no longer declares are ignored.
    let mut ignored_undeclared = Vec::new();
    for channel in &params.channels {
        if !outputs.iter().any(|o| o.channel == channel.name) {
            ignored_undeclared.push(channel.name.clone());
        }
    }
    let mut plans = Vec::with_capacity(outputs.len());
    for output in &outputs {
        let control = params.channels.iter().find(|c| c.name == output.channel);
        let (blend, opacity, enabled) = match control {
            Some(c) => (c.blend, c.opacity, c.enabled),
            None => (BlendMode::Normal, 1.0, true),
        };
        plans.push(ChannelPlan {
            output: output.clone(),
            blend,
            opacity,
            enabled,
        });
    }
    let mut channels: Vec<ChannelReport> = Vec::with_capacity(plans.len());
    for plan in plans.iter().filter(|p| !p.enabled) {
        channels.push(ChannelReport {
            name: plan.output.channel.clone(),
            status: ChannelStatus::SkippedDisabled,
        });
    }
    // Forbidden blends skip the channel (never a Normal fallback).
    let mut active: Vec<&ChannelPlan> = Vec::new();
    for plan in plans.iter().filter(|p| p.enabled) {
        let channel_name = plan.output.channel.clone();
        if matches!(plan.blend, BlendMode::Passthrough)
            || (plan.blend.is_normal_map() && channel_name != "normal")
        {
            channels.push(ChannelReport {
                name: channel_name,
                status: ChannelStatus::SkippedBlend,
            });
        } else {
            active.push(plan);
        }
    }
    if active.is_empty() {
        state.engine_dirty.clear();
        state.full_eval = false;
        return MaterialResult::Refreshed {
            channels,
            ignored_undeclared,
        };
    }

    // Eval externals + cache shape.
    let anchors = collect_anchor_refs(&graph);
    let mesh_names = collect_mesh_map_names(&graph);
    let per_tile = needs_per_tile_eval(&graph);
    let (set_w, set_h) = io.set_size();
    state.set_size = (set_w, set_h);
    let engine_dirty = if state.full_eval {
        state.per_tile.clear();
        state.shared = EvalCache::new();
        graph.nodes.iter().map(|n| n.id).collect()
    } else {
        state.engine_dirty.clone()
    };
    // Per-channel outcomes, keyed by channel (tile-independent skips land
    // once; publishes accumulate across tiles).
    let mut statuses: HashMap<String, ChannelStatus> = HashMap::new();

    if per_tile {
        for tile in io.tiles() {
            let external = build_externals(io, tile, &anchors, &mesh_names);
            let ctx = EvalContext {
                resolution: (set_w, set_h),
                external,
            };
            let cache = state.per_tile.entry(tile).or_default();
            let outputs_map = match eval_graph_cached(
                &graph,
                registry,
                HashMap::new(),
                &ctx,
                cache,
                &engine_dirty,
            ) {
                Ok(map) => map,
                Err(err) => {
                    return MaterialResult::Failed(MaterialError::Eval(err));
                }
            };
            if let Err(err) = publish_tile(layer, &active, tile, &outputs_map, io, &mut statuses) {
                return MaterialResult::Failed(err);
            }
        }
    } else {
        let external = build_externals(io, 0, &[], &mesh_names);
        let ctx = EvalContext {
            resolution: (set_w, set_h),
            external,
        };
        let outputs_map = match eval_graph_cached(
            &graph,
            registry,
            HashMap::new(),
            &ctx,
            &mut state.shared,
            &engine_dirty,
        ) {
            Ok(map) => map,
            Err(err) => {
                return MaterialResult::Failed(MaterialError::Eval(err));
            }
        };
        for tile in io.tiles() {
            if let Err(err) = publish_tile(layer, &active, tile, &outputs_map, io, &mut statuses) {
                return MaterialResult::Failed(err);
            }
        }
    }
    // Whole-material failures return before any publish; reaching here means
    // every active channel resolved on every tile.
    for plan in &active {
        let status = statuses
            .get(&plan.output.channel)
            .copied()
            .unwrap_or(ChannelStatus::Published);
        channels.push(ChannelReport {
            name: plan.output.channel.clone(),
            status,
        });
    }
    state.engine_dirty.clear();
    state.full_eval = false;
    MaterialResult::Refreshed {
        channels,
        ignored_undeclared,
    }
}

/// Collects every anchor the graph resolves: `anchor` nodes plus any `Anchor`
/// param value, deduped and sorted (deterministic externals).
fn collect_anchor_refs(graph: &Graph) -> Vec<AnchorRef> {
    let mut refs = BTreeSet::new();
    for node in &graph.nodes {
        if node.node_def == "anchor" {
            let layer_id = match node.params.iter().find(|(n, _)| n == "layer_id") {
                Some((_, ParamValue::Int(id))) if *id >= 0 => *id as u64,
                _ => continue,
            };
            let channel = match node.params.iter().find(|(n, _)| n == "channel") {
                Some((_, ParamValue::Asset(channel))) => channel.clone(),
                _ => continue,
            };
            refs.insert((layer_id, channel));
        }
        for (_, value) in &node.params {
            if let ParamValue::Anchor(anchor) = value {
                refs.insert((anchor.layer_id, anchor.channel.clone()));
            }
        }
    }
    refs.into_iter()
        .map(|(layer_id, channel)| AnchorRef::new(layer_id, channel))
        .collect()
}

/// Collects every baked-map name the graph reads, sorted.
fn collect_mesh_map_names(graph: &Graph) -> Vec<String> {
    let mut names = BTreeSet::new();
    for node in &graph.nodes {
        if node.node_def == "mesh_map_generator" {
            if let Some((_, ParamValue::Asset(name))) =
                node.params.iter().find(|(n, _)| n == "map_name")
            {
                names.insert(name.clone());
            }
        }
    }
    names.into_iter().collect()
}

/// Whether the graph needs a per-tile eval: an `anchor` or
/// `mesh_map_generator` node, or any `Anchor` param value (all resolve
/// through per-tile externals).
fn needs_per_tile_eval(graph: &Graph) -> bool {
    graph.nodes.iter().any(|n| {
        n.node_def == "anchor"
            || n.node_def == "mesh_map_generator"
            || n.params
                .iter()
                .any(|(_, v)| matches!(v, ParamValue::Anchor(_)))
    })
}

/// Builds one eval's externals: anchors for `tile` plus the baked maps.
/// Absent images are SKIPPED (the eval then fails loud with the typed
/// `Anchor`/`MissingInput` error — never a silent default). Wrong-size byte
/// blobs are skipped the same way (an anchor that decodes to no image is a
/// bridge bug, and `MissingInput` names the absent map).
fn build_externals(
    io: &mut impl MaterialTileIo,
    tile: u16,
    anchors: &[AnchorRef],
    mesh_names: &[String],
) -> HashMap<String, NodeOutput> {
    let mut external = HashMap::new();
    for anchor in anchors {
        let image = io.anchor_image(tile, anchor.layer_id, &anchor.channel);
        if let Some((w, h, bytes)) = image {
            if let Ok(buf) = ImageBuffer::new(w, h, bytes) {
                external.insert(anchor.key(), NodeOutput::Image(buf));
            }
        }
    }
    for name in mesh_names {
        let key = format!("mesh_map:{name}");
        if let Some((w, h, bytes)) = io.mesh_map_image(name) {
            if let Ok(buf) = ImageBuffer::new(w, h, bytes) {
                external.insert(key, NodeOutput::Image(buf));
            }
        }
    }
    external
}

/// The per-tile publish: resample each active output to its channel size,
/// composite over the current texels (transparent when never routed) with
/// the resampled mask, publish. Records each channel's outcome in
/// `statuses` (tile-independent skips land once; publishes accumulate).
///
/// # Errors
///
/// [`MaterialError::Internal`] when a successfully-evaluated node has no
/// output (allegedly unreachable — loud, never wrong pixels).
fn publish_tile(
    layer: &Layer,
    active: &[&ChannelPlan],
    tile: u16,
    outputs_map: &HashMap<u64, NodeOutput>,
    io: &mut impl MaterialTileIo,
    statuses: &mut HashMap<String, ChannelStatus>,
) -> Result<(), MaterialError> {
    for plan in active {
        let channel = plan.output.channel.clone();
        let output =
            outputs_map
                .get(&plan.output.node_id)
                .ok_or_else(|| MaterialError::Internal {
                    layer: layer.id,
                    message: format!("evaluated node {} has no output", plan.output.node_id),
                })?;
        let image = match output {
            NodeOutput::Uniform(_) => {
                statuses
                    .entry(channel)
                    .or_insert(ChannelStatus::SkippedUniform);
                continue;
            }
            NodeOutput::Image(buf) => buf,
        };
        let (cw, ch) = io.channel_size(&channel);
        let src = resample_image_rgba8(&image.data, image.width, image.height, cw, ch, &channel);
        let mut dst = match io.read_channel(tile, &channel) {
            Some((w, h, bytes))
                if w == cw && h == ch && bytes.len() == cw as usize * ch as usize * 4 =>
            {
                bytes
            }
            Some(_) => {
                // A wrong-size readback is a bridge bug: composite over
                // transparent rather than misaligning texels.
                vec![0u8; cw as usize * ch as usize * 4]
            }
            None => vec![0u8; cw as usize * ch as usize * 4],
        };
        let mask = layer
            .mask
            .as_ref()
            .filter(|mask_meta| mask_meta.gates())
            .and_then(|_| io.mask_plane(layer.id, tile))
            .map(|(mw, mh, bytes)| resample_mask_nearest(&bytes, mw, mh, cw, ch));
        // The opacity product is formed first and clamped once inside the
        // primitive, so a non-finite factor zeroes coverage.
        let opacity = layer.opacity * plan.opacity;
        composite_material_channel(&mut dst, &src, cw, ch, plan.blend, opacity, mask.as_deref());
        let _ = io.publish_channel(tile, &channel, cw, ch, dst);
        statuses.insert(channel, ChannelStatus::Published);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Evaluated anchor planes keyed by (tile, layer, channel) — the
    /// FakeTileIo's anchor payload shape.
    type AnchorPlanes = HashMap<(u16, u64, String), (u32, u32, Vec<u8>)>;

    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use umber_core::layers::{
        LayerStack, MaterialChannel, MaterialParamOverride, MaterialParamValue,
    };
    use umber_graph::mtlx::{painter_nodedefs, to_mtlx};
    use umber_graph::{Edge, Node};

    const SET_CHANNELS: &[&str] = &["baseColor", "roughness", "metallic", "normal", "height"];

    fn set_channels() -> Vec<String> {
        SET_CHANNELS.iter().map(|s| s.to_string()).collect()
    }

    /// Scripted in-memory tile store: the headless stand-in for the paint
    /// session (sizes, tiles, masks, anchors, mesh maps — all explicit).
    #[derive(Debug, Default)]
    struct FakeTileIo {
        set_size: (u32, u32),
        channel_sizes: HashMap<String, (u32, u32)>,
        tile_list: Vec<u16>,
        tiles: HashMap<(u16, String), (u32, u32, Vec<u8>)>,
        masks: HashMap<(u64, u16), (u32, u32, Vec<u8>)>,
        anchors: AnchorPlanes,
        meshes: HashMap<String, (u32, u32, Vec<u8>)>,
        read_log: Vec<(u16, String)>,
        publish_log: Vec<(u16, String)>,
        anchor_log: Vec<(u16, u64, String)>,
        mesh_log: Vec<String>,
    }

    impl FakeTileIo {
        fn flat(set_size: (u32, u32), tile: u16, channel: &str, pixel: [u8; 4]) -> Self {
            let (w, h) = set_size;
            let mut tiles = HashMap::new();
            tiles.insert(
                (tile, channel.to_string()),
                (w, h, vec![pixel; w as usize * h as usize].concat()),
            );
            Self {
                set_size,
                tile_list: vec![tile],
                tiles,
                ..Default::default()
            }
        }

        fn tile_bytes(&self, tile: u16, channel: &str) -> Vec<u8> {
            self.tiles[&(tile, channel.to_string())].2.clone()
        }
    }

    impl MaterialTileIo for FakeTileIo {
        fn set_size(&self) -> (u32, u32) {
            self.set_size
        }

        fn channel_size(&self, channel: &str) -> (u32, u32) {
            self.channel_sizes
                .get(channel)
                .copied()
                .unwrap_or(self.set_size)
        }

        fn tiles(&self) -> Vec<u16> {
            self.tile_list.clone()
        }

        fn read_channel(&mut self, tile: u16, channel: &str) -> Option<(u32, u32, Vec<u8>)> {
            self.read_log.push((tile, channel.to_string()));
            self.tiles.get(&(tile, channel.to_string())).cloned()
        }

        fn publish_channel(
            &mut self,
            tile: u16,
            channel: &str,
            width: u32,
            height: u32,
            rgba: Vec<u8>,
        ) -> bool {
            self.publish_log.push((tile, channel.to_string()));
            self.tiles
                .insert((tile, channel.to_string()), (width, height, rgba));
            true
        }

        fn mask_plane(&mut self, layer_id: u64, tile: u16) -> Option<(u32, u32, Vec<u8>)> {
            self.masks.get(&(layer_id, tile)).cloned()
        }

        fn anchor_image(
            &mut self,
            tile: u16,
            layer: u64,
            channel: &str,
        ) -> Option<(u32, u32, Vec<u8>)> {
            self.anchor_log.push((tile, layer, channel.to_string()));
            self.anchors
                .get(&(tile, layer, channel.to_string()))
                .cloned()
        }

        fn mesh_map_image(&mut self, name: &str) -> Option<(u32, u32, Vec<u8>)> {
            self.mesh_log.push(name.to_string());
            self.meshes.get(name).cloned()
        }
    }

    fn uniform_node(id: u64, color: [f32; 3]) -> Node {
        Node {
            id,
            node_def: "uniform".into(),
            params: vec![("color".into(), ParamValue::Color(color))],
            canvas: None,
        }
    }

    fn output_node(id: u64, channel: &str) -> Node {
        Node {
            id,
            node_def: "material_output".into(),
            params: vec![("channel".into(), ParamValue::Asset(channel.into()))],
            canvas: None,
        }
    }

    fn edge(from: u64, to: u64) -> Edge {
        Edge {
            from,
            to,
            input: "input".into(),
        }
    }

    /// A one-output definition: solid `color` into `channel`.
    fn solid_def(color: [f32; 3], channel: &str) -> String {
        let mut graph = Graph::new();
        graph.add_node(uniform_node(1, color));
        graph.add_node(output_node(2, channel));
        graph.add_edge(edge(1, 2));
        to_mtlx(&graph, &painter_nodedefs())
    }

    fn channel_params(material_id: &str, channels: Vec<MaterialChannel>) -> MaterialParams {
        MaterialParams {
            material_id: material_id.into(),
            channels,
            params: vec![],
        }
    }

    fn material_layer(
        stack: &mut LayerStack,
        name: &str,
        params: MaterialParams,
        opacity: f32,
    ) -> Layer {
        let id = stack.add_layer(name, LayerKind::Material);
        stack.set_material(id, params);
        stack.set_opacity(id, opacity);
        stack.layer(id).unwrap().clone()
    }

    /// Drives one full refresh pass: ensure every material layer, then drain
    /// the dirty queue bottom-to-top through the shared registry.
    fn drive(
        layers: &[Layer],
        definitions: &HashMap<String, String>,
        io: &mut FakeTileIo,
    ) -> (MaterialLayerStates, Vec<MaterialRefreshReport>) {
        let mut states = MaterialLayerStates::default();
        let size = io.set_size();
        for layer in layers {
            if layer.kind == LayerKind::Material {
                if let Some(params) = layer.material.clone() {
                    let definition = definitions
                        .get(&params.material_id)
                        .cloned()
                        .unwrap_or_default();
                    states.ensure(layer.id, &params, &definition, size);
                }
            }
        }
        let reports = refresh_dirty_materials(
            &mut states,
            layers,
            definitions,
            &set_channels(),
            &umber_graph::full_registry(),
            io,
        );
        (states, reports)
    }

    fn status_of(reports: &[MaterialRefreshReport], layer: u64, channel: &str) -> ChannelStatus {
        for report in reports {
            if report.layer != layer {
                continue;
            }
            match &report.result {
                MaterialResult::Refreshed { channels, .. } => {
                    for entry in channels {
                        if entry.name == channel {
                            return entry.status;
                        }
                    }
                    panic!("no report for channel {channel}");
                }
                other => panic!("expected Refreshed, got {other:?}"),
            }
        }
        panic!("no report for layer {layer}");
    }

    fn failed_of(reports: &[MaterialRefreshReport], layer: u64) -> &MaterialError {
        for report in reports {
            if report.layer != layer {
                continue;
            }
            match &report.result {
                MaterialResult::Failed(err) => return err,
                other => panic!("expected Failed, got {other:?}"),
            }
        }
        panic!("no report for layer {layer}");
    }

    #[test]
    fn bridge_converts_all_six_mirror_variants() {
        // The umber-core mirror maps 1:1 onto the graph values — every
        // variant round-trips exactly (a mistyped bridge would BadParam the
        // whole material at eval time).
        let cases = [
            (MaterialParamValue::Float(0.5), ParamValue::Float(0.5)),
            (
                MaterialParamValue::Vec2([1.0, 2.0]),
                ParamValue::Vec2([1.0, 2.0]),
            ),
            (
                MaterialParamValue::Vec3([1.0, 2.0, 3.0]),
                ParamValue::Vec3([1.0, 2.0, 3.0]),
            ),
            (
                MaterialParamValue::Color([1.0, 0.0, 0.25]),
                ParamValue::Color([1.0, 0.0, 0.25]),
            ),
            (MaterialParamValue::Int(-7), ParamValue::Int(-7)),
            (MaterialParamValue::Bool(true), ParamValue::Bool(true)),
        ];
        for (mirror, graph) in cases {
            assert_eq!(material_param_to_graph(&mirror), graph);
        }
    }

    #[test]
    fn channel_declaration_golden_two_outputs_metallic_untouched() {
        // §5.1: two material_outputs (baseColor solid red, roughness solid
        // gray 128) over a known base, Normal at 1.0 × 1.0: both publish
        // byte-exact src values; metallic (undeclared) is byte-identical
        // before/after and never read (a never-routed metallic stays
        // unmaterialized — no read, no publish).
        let mut graph = Graph::new();
        graph.add_node(uniform_node(1, [1.0, 0.0, 0.0]));
        graph.add_node(uniform_node(2, [0.5, 0.5, 0.5]));
        graph.add_node(output_node(3, "baseColor"));
        graph.add_node(output_node(4, "roughness"));
        graph.add_edge(edge(1, 3));
        graph.add_edge(edge(2, 4));
        let definition = to_mtlx(&graph, &painter_nodedefs());

        let mut stack = LayerStack::new();
        let params = channel_params(
            "umber.two-channel",
            vec![
                MaterialChannel::named("baseColor"),
                MaterialChannel::named("roughness"),
            ],
        );
        let layer = material_layer(&mut stack, "Two", params, 1.0);
        let mut definitions = HashMap::new();
        definitions.insert("umber.two-channel".into(), definition);

        let mut io = FakeTileIo::flat((2, 2), 1001, "baseColor", [10, 20, 30, 255]);
        io.tiles.insert(
            (1001, "roughness".into()),
            (2, 2, [200, 200, 200, 255].repeat(4).to_vec()),
        );
        io.tiles.insert(
            (1001, "metallic".into()),
            (2, 2, [5, 6, 7, 255].repeat(4).to_vec()),
        );
        let before_metallic = io.tile_bytes(1001, "metallic");

        let (_, reports) = drive(std::slice::from_ref(&layer), &definitions, &mut io);
        assert_eq!(
            status_of(&reports, layer.id, "baseColor"),
            ChannelStatus::Published
        );
        assert_eq!(
            status_of(&reports, layer.id, "roughness"),
            ChannelStatus::Published
        );
        // Byte-exact src values (Normal at full coverage is idempotent).
        assert!(io
            .tile_bytes(1001, "baseColor")
            .chunks_exact(4)
            .all(|px| px == [255, 0, 0, 255]));
        assert!(io
            .tile_bytes(1001, "roughness")
            .chunks_exact(4)
            .all(|px| px == [128, 128, 128, 255]));
        // Metallic: byte-identical, never read, never published.
        assert_eq!(io.tile_bytes(1001, "metallic"), before_metallic);
        assert!(
            io.read_log.iter().all(|(_, c)| c != "metallic"),
            "undeclared channels are never read: {:?}",
            io.read_log
        );
        assert!(
            io.publish_log.iter().all(|(_, c)| c != "metallic"),
            "undeclared channels are never published: {:?}",
            io.publish_log
        );
    }

    #[test]
    fn opacity_product_golden_170_with_wrong_order_sentinels() {
        // §5.2a: dst 200, src 100 (s = 0.392157), Multiply at layer 0.5 ×
        // channel 0.5 → cov 0.25 → 200·(1 − 0.25·0.607843) = 169.608 → 170.
        // The r1 two-level order would give 97, a single 0.5 would give 139:
        // the test asserts both sentinels to pin the single-composite model.
        let gray100 = 100.0f32 / 255.0;
        let mut stack = LayerStack::new();
        let params = channel_params(
            "umber.multiply",
            vec![MaterialChannel {
                name: "baseColor".into(),
                blend: BlendMode::Multiply,
                opacity: 0.5,
                enabled: true,
            }],
        );
        let layer = material_layer(&mut stack, "M", params, 0.5);
        let mut definitions = HashMap::new();
        definitions.insert(
            "umber.multiply".into(),
            solid_def([gray100, gray100, gray100], "baseColor"),
        );

        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [200, 200, 200, 255]);
        let (_, reports) = drive(std::slice::from_ref(&layer), &definitions, &mut io);
        assert_eq!(
            status_of(&reports, layer.id, "baseColor"),
            ChannelStatus::Published
        );
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![170, 170, 170, 255]);
        // The wrong-order sentinels (commented as such in the design): the
        // r1 inner-then-outer order gives 97, a lone 0.5 gives 139.
        let r = io.tile_bytes(1001, "baseColor")[0];
        assert_ne!(r, 97, "two-level Multiply order must not appear");
        assert_ne!(r, 139, "single-0.5 coverage must not appear");
    }

    #[test]
    fn refresh_order_is_bottom_to_top_171_and_reversed_187() {
        // §5.2b (ordering-sensitive): M1 below (Normal, src 204, 1.0 × 0.5),
        // M2 above (Multiply, src 100, 0.5 × 0.5) over dst 200. Stack order
        // M1 → M2: 202 then 171. Reversed call order: 170 then 187 — proving
        // order matters under destructive publish.
        let gray100 = 100.0f32 / 255.0;
        let gray204 = 204.0f32 / 255.0;
        let mut stack = LayerStack::new();
        let m1 = material_layer(
            &mut stack,
            "M1",
            channel_params(
                "umber.m1",
                vec![MaterialChannel {
                    name: "baseColor".into(),
                    blend: BlendMode::Normal,
                    opacity: 0.5,
                    enabled: true,
                }],
            ),
            1.0,
        );
        let m2 = material_layer(
            &mut stack,
            "M2",
            channel_params(
                "umber.m2",
                vec![MaterialChannel {
                    name: "baseColor".into(),
                    blend: BlendMode::Multiply,
                    opacity: 0.5,
                    enabled: true,
                }],
            ),
            0.5,
        );
        let mut definitions = HashMap::new();
        definitions.insert(
            "umber.m1".into(),
            solid_def([gray204, gray204, gray204], "baseColor"),
        );
        definitions.insert(
            "umber.m2".into(),
            solid_def([gray100, gray100, gray100], "baseColor"),
        );

        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [200, 200, 200, 255]);
        let (_, reports) = drive(&[m1.clone(), m2.clone()], &definitions, &mut io);
        assert_eq!(reports.len(), 2);
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![171, 171, 171, 255]);

        // Reversed call order over a fresh store: 170 (M2 first) then 187.
        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [200, 200, 200, 255]);
        let (_, reports) = drive(&[m2, m1], &definitions, &mut io);
        assert_eq!(reports.len(), 2);
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![187, 187, 187, 255]);
    }

    #[test]
    fn reeval_compounds_the_documented_boundary() {
        // §5.2c: refreshing 2a's layer a second time over the published 170
        // gives 170·0.848039 = 144.167 → 144. Pins the v1 destructive-publish
        // boundary so a future non-destructive fix flips this test on
        // purpose.
        let gray100 = 100.0f32 / 255.0;
        let mut stack = LayerStack::new();
        let params = channel_params(
            "umber.multiply",
            vec![MaterialChannel {
                name: "baseColor".into(),
                blend: BlendMode::Multiply,
                opacity: 0.5,
                enabled: true,
            }],
        );
        let layer = material_layer(&mut stack, "M", params, 0.5);
        let mut definitions = HashMap::new();
        definitions.insert(
            "umber.multiply".into(),
            solid_def([gray100, gray100, gray100], "baseColor"),
        );

        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [200, 200, 200, 255]);
        let layers = std::slice::from_ref(&layer);
        let (mut states, _) = drive(layers, &definitions, &mut io);
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![170, 170, 170, 255]);
        // A second explicit refresh (the re-publish): compounds to 144.
        states.mark_dirty(layer.id);
        let reports = refresh_dirty_materials(
            &mut states,
            layers,
            &definitions,
            &set_channels(),
            &umber_graph::full_registry(),
            &mut io,
        );
        assert_eq!(reports.len(), 1);
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![144, 144, 144, 255]);
    }

    #[test]
    fn triplet_modes_route_through_blend_rgb_with_pinned_bytes() {
        // §5.3: Saturation (red over gray 128) is EXACT through the HSV
        // path: hue+sat from red at gray's value → [128, 0, 0, 255]. A
        // scalar-primitive routing would return None (channel unpublished);
        // asserting the bytes proves blend_rgb ran.
        let mut graph = Graph::new();
        graph.add_node(uniform_node(1, [1.0, 0.0, 0.0]));
        graph.add_node(output_node(2, "baseColor"));
        graph.add_edge(edge(1, 2));
        let definition = to_mtlx(&graph, &painter_nodedefs());

        let mut stack = LayerStack::new();
        let params = channel_params(
            "umber.saturation",
            vec![MaterialChannel {
                name: "baseColor".into(),
                blend: BlendMode::Saturation,
                opacity: 1.0,
                enabled: true,
            }],
        );
        let layer = material_layer(&mut stack, "S", params, 1.0);
        let mut definitions = HashMap::new();
        definitions.insert("umber.saturation".into(), definition);

        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [128, 128, 128, 255]);
        let (_, reports) = drive(std::slice::from_ref(&layer), &definitions, &mut io);
        assert_eq!(
            status_of(&reports, layer.id, "baseColor"),
            ChannelStatus::Published
        );
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![128, 0, 0, 255]);
    }

    #[test]
    fn normal_combine_pins_whiteout_bytes_not_identity() {
        // §5.3: tilt-over-tilt through NormalCombine gives the Whiteout
        // result [248, 128, 170] — NOT the src bytes back (a blend_rgb
        // bypass/identity shortcut would return [218, 128, 218], and the
        // scalar path would publish nothing). Byte 128 decodes to +0.0039,
        // not exactly flat, so this also pins the non-flat math.
        let tilt = 218.0f32 / 255.0;
        let flat = 128.0f32 / 255.0;
        let mut graph = Graph::new();
        graph.add_node(uniform_node(1, [tilt, flat, tilt]));
        graph.add_node(output_node(2, "normal"));
        graph.add_edge(edge(1, 2));
        let definition = to_mtlx(&graph, &painter_nodedefs());

        let mut stack = LayerStack::new();
        let params = channel_params(
            "umber.combine",
            vec![MaterialChannel {
                name: "normal".into(),
                blend: BlendMode::NormalCombine,
                opacity: 1.0,
                enabled: true,
            }],
        );
        let layer = material_layer(&mut stack, "N", params, 1.0);
        let mut definitions = HashMap::new();
        definitions.insert("umber.combine".into(), definition);

        let mut io = FakeTileIo::flat((1, 1), 1001, "normal", [218, 128, 218, 255]);
        let (_, reports) = drive(std::slice::from_ref(&layer), &definitions, &mut io);
        assert_eq!(
            status_of(&reports, layer.id, "normal"),
            ChannelStatus::Published
        );
        assert_eq!(io.tile_bytes(1001, "normal"), vec![248, 128, 170, 255]);
    }

    #[test]
    fn forbidden_blends_skip_only_their_channel() {
        // §5.3: a normal-map mode on `roughness` and `Passthrough` on any
        // channel each make THAT channel publish nothing, while a sibling
        // channel still publishes its golden bytes.
        let mut graph = Graph::new();
        graph.add_node(uniform_node(1, [1.0, 0.0, 0.0]));
        graph.add_node(uniform_node(2, [0.5, 0.5, 0.5]));
        graph.add_node(output_node(3, "baseColor"));
        graph.add_node(output_node(4, "roughness"));
        graph.add_edge(edge(1, 3));
        graph.add_edge(edge(2, 4));
        let definition = to_mtlx(&graph, &painter_nodedefs());

        let mut stack = LayerStack::new();
        let params = channel_params(
            "umber.forbidden",
            vec![
                MaterialChannel {
                    name: "baseColor".into(),
                    blend: BlendMode::Passthrough,
                    opacity: 1.0,
                    enabled: true,
                },
                MaterialChannel {
                    name: "roughness".into(),
                    blend: BlendMode::NormalDetail,
                    opacity: 1.0,
                    enabled: true,
                },
            ],
        );
        let layer = material_layer(&mut stack, "F", params, 1.0);
        let mut definitions = HashMap::new();
        definitions.insert("umber.forbidden".into(), definition);

        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [10, 20, 30, 255]);
        io.tiles
            .insert((1001, "roughness".into()), (1, 1, vec![200, 200, 200, 255]));
        let (_, reports) = drive(std::slice::from_ref(&layer), &definitions, &mut io);
        assert_eq!(
            status_of(&reports, layer.id, "baseColor"),
            ChannelStatus::SkippedBlend
        );
        assert_eq!(
            status_of(&reports, layer.id, "roughness"),
            ChannelStatus::SkippedBlend
        );
        // Neither tile was composited (Passthrough never falls back to
        // Normal — a fallback would have published red here).
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![10, 20, 30, 255]);
        assert_eq!(io.tile_bytes(1001, "roughness"), vec![200, 200, 200, 255]);

        // Sibling control: the same definition with Normal on baseColor
        // publishes red while roughness still skips.
        let params = channel_params(
            "umber.forbidden",
            vec![
                MaterialChannel::named("baseColor"),
                MaterialChannel {
                    name: "roughness".into(),
                    blend: BlendMode::NormalDetail,
                    opacity: 1.0,
                    enabled: true,
                },
            ],
        );
        let layer = material_layer(&mut stack, "F2", params, 1.0);
        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [10, 20, 30, 255]);
        io.tiles
            .insert((1001, "roughness".into()), (1, 1, vec![200, 200, 200, 255]));
        let (_, reports) = drive(std::slice::from_ref(&layer), &definitions, &mut io);
        assert_eq!(
            status_of(&reports, layer.id, "baseColor"),
            ChannelStatus::Published
        );
        assert_eq!(
            status_of(&reports, layer.id, "roughness"),
            ChannelStatus::SkippedBlend
        );
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![255, 0, 0, 255]);
        assert_eq!(io.tile_bytes(1001, "roughness"), vec![200, 200, 200, 255]);
    }

    #[test]
    fn resampler_identity_is_byte_exact() {
        // Identity sizes copy bytes exactly (not a float round-trip).
        let src: Vec<u8> = (0..48).collect();
        for channel in ["baseColor", "normal", "roughness"] {
            assert_eq!(resample_image_rgba8(&src, 4, 3, 4, 3, channel), src);
        }
        let mask: Vec<u8> = (0..12).collect();
        assert_eq!(resample_mask_nearest(&mask, 4, 3, 4, 3), mask);
    }

    #[test]
    fn resampler_bilinear_upsample_pins_every_byte() {
        // §5.7: 2×2 → 4×4, R lane {0, 64, 128, 255}, G = 200, B = 50,
        // A = 255 constant. Hand-derived under the texel-centre formula
        // (src = (dst + 0.5) · 2/4 − 0.5, edge-clamped); every value sits
        // ≥ 0.25 from a rounding boundary.
        let src: Vec<u8> = vec![
            0, 200, 50, 255, 64, 200, 50, 255, 128, 200, 50, 255, 255, 200, 50, 255,
        ];
        let out = resample_image_rgba8(&src, 2, 2, 4, 4, "baseColor");
        let rows: Vec<Vec<u8>> = out
            .chunks_exact(16)
            .map(|row| row.chunks_exact(4).map(|px| px[0]).collect())
            .collect();
        assert_eq!(
            rows,
            vec![
                vec![0, 16, 48, 64],
                vec![32, 52, 92, 112],
                vec![96, 124, 179, 207],
                vec![128, 160, 223, 255],
            ]
        );
        // The constant lanes survive untouched on every texel.
        for px in out.chunks_exact(4) {
            assert_eq!(&px[1..4], &[200, 50, 255]);
        }
    }

    #[test]
    fn resampler_normal_guard_pins_the_reachable_zero_case() {
        // §5.7: a 2×1 `normal` row of (127,127,127) and (128,128,128)
        // upsampled to 3×1. The middle texel sits at source x = 0.5 and
        // lerps to exactly 127.5, which decodes to exactly (0,0,0) — the
        // guard yields (0,0,1), re-encoded under `round` (127.5 → 128) as
        // (128,128,255). The same input on a non-`normal` channel does NOT
        // renormalize (middle 127.5 → 128 per lane, blue 128).
        let src: Vec<u8> = vec![127, 127, 127, 255, 128, 128, 128, 255];
        assert_eq!(
            resample_image_rgba8(&src, 2, 1, 3, 1, "normal"),
            vec![54, 54, 54, 255, 128, 128, 255, 255, 201, 201, 201, 255,]
        );
        assert_eq!(
            resample_image_rgba8(&src, 2, 1, 3, 1, "baseColor"),
            vec![127, 127, 127, 255, 128, 128, 128, 255, 128, 128, 128, 255,]
        );
    }

    #[test]
    fn mask_nearest_resample_pins_blocks_and_drives_composite() {
        // §5.7: a 2×2 mask nearest-resampled to 4×4 replicates 2×2 blocks,
        // and a set-sized mask resampled to an overridden channel
        // composites with pinned bytes (no InvalidMaskData class of
        // failure — the headless analogue is exact output bytes).
        assert_eq!(
            resample_mask_nearest(&[0, 255, 255, 0], 2, 2, 4, 4),
            vec![0, 0, 255, 255, 0, 0, 255, 255, 255, 255, 0, 0, 255, 255, 0, 0,]
        );

        // End to end: set 1×1, roughness overridden to 2×2. The graph
        // evaluates once at the set size (1×1 src 100), the output AND the
        // 1×1 mask resample to 2×2, and every texel composites the §5.2a
        // golden (Multiply 0.5 × 0.5 over 200 → 170).
        let gray100 = 100.0f32 / 255.0;
        let mut stack = LayerStack::new();
        let params = channel_params(
            "umber.override",
            vec![MaterialChannel {
                name: "roughness".into(),
                blend: BlendMode::Multiply,
                opacity: 0.5,
                enabled: true,
            }],
        );
        let layer = material_layer(&mut stack, "O", params, 0.5);
        let layer_id = layer.id;
        let mut definitions = HashMap::new();
        definitions.insert(
            "umber.override".into(),
            solid_def([gray100, gray100, gray100], "roughness"),
        );

        let mut io = FakeTileIo {
            set_size: (1, 1),
            channel_sizes: HashMap::from([("roughness".to_string(), (2, 2))]),
            tile_list: vec![1001],
            tiles: HashMap::from([((1001, "roughness".to_string()), (2, 2, vec![200u8; 16]))]),
            masks: HashMap::from([((layer_id, 1001), (1, 1, vec![255u8]))]),
            ..Default::default()
        };
        let (_, reports) = drive(std::slice::from_ref(&layer), &definitions, &mut io);
        assert_eq!(
            status_of(&reports, layer.id, "roughness"),
            ChannelStatus::Published
        );
        assert_eq!(io.tile_bytes(1001, "roughness"), vec![170u8; 16]);

        // A fully-gated mask (0) publishes nothing: bytes identical.
        let mut io = FakeTileIo {
            set_size: (1, 1),
            channel_sizes: HashMap::from([("roughness".to_string(), (2, 2))]),
            tile_list: vec![1001],
            tiles: HashMap::from([((1001, "roughness".to_string()), (2, 2, vec![200u8; 16]))]),
            masks: HashMap::from([((layer_id, 1001), (1, 1, vec![0u8]))]),
            ..Default::default()
        };
        let (_, reports) = drive(std::slice::from_ref(&layer), &definitions, &mut io);
        assert_eq!(
            status_of(&reports, layer.id, "roughness"),
            ChannelStatus::Published
        );
        assert_eq!(io.tile_bytes(1001, "roughness"), vec![200u8; 16]);
    }

    #[test]
    fn discovery_rejects_duplicates_bad_params_and_unknown_channels() {
        // Build-time validation, headless: duplicate outputs, a non-Asset
        // channel and a set-unknown channel are typed errors (whole-material
        // no-ops at refresh time).
        let mut graph = Graph::new();
        graph.add_node(uniform_node(1, [1.0, 0.0, 0.0]));
        graph.add_node(output_node(2, "baseColor"));
        graph.add_node(output_node(3, "baseColor"));
        graph.add_edge(edge(1, 2));
        graph.add_edge(edge(1, 3));
        assert!(matches!(
            discover_material_outputs(7, &graph),
            Err(MaterialError::DuplicateChannel { layer: 7, .. })
        ));

        let mut graph = Graph::new();
        graph.add_node(uniform_node(1, [1.0, 0.0, 0.0]));
        graph.add_node(Node {
            id: 2,
            node_def: "material_output".into(),
            params: vec![("channel".into(), ParamValue::Float(1.0))],
            canvas: None,
        });
        graph.add_edge(edge(1, 2));
        assert!(matches!(
            discover_material_outputs(7, &graph),
            Err(MaterialError::BadChannelParam { layer: 7, node: 2 })
        ));

        let mut graph = Graph::new();
        graph.add_node(uniform_node(1, [1.0, 0.0, 0.0]));
        graph.add_node(output_node(2, "clown"));
        graph.add_edge(edge(1, 2));
        let outputs = discover_material_outputs(7, &graph).expect("discovers");
        assert_eq!(
            outputs,
            vec![MaterialOutput {
                node_id: 2,
                channel: "clown".into(),
            }]
        );
        assert!(matches!(
            validate_material_channels(7, &outputs, &set_channels()),
            Err(MaterialError::UnknownChannel { layer: 7, .. })
        ));
        // And the declaration order is by node id, not insertion order.
        let mut graph = Graph::new();
        graph.add_node(output_node(9, "roughness"));
        graph.add_node(output_node(4, "baseColor"));
        let outputs = discover_material_outputs(7, &graph).expect("discovers");
        assert_eq!(
            outputs.iter().map(|o| o.node_id).collect::<Vec<_>>(),
            vec![4, 9]
        );
    }

    #[test]
    fn overrides_apply_and_missing_targets_fail_loud() {
        // An override rewrites its param on the definition copy (whole-
        // material on the NEXT refresh); a missing node or param is a typed
        // error, never a silent skip.
        let mut graph = Graph::new();
        graph.add_node(Node {
            id: 1,
            node_def: "uniform".into(),
            params: vec![("color".into(), ParamValue::Color([1.0, 0.0, 0.0]))],
            canvas: None,
        });
        let params = MaterialParams {
            material_id: "umber.x".into(),
            channels: vec![],
            params: vec![MaterialParamOverride {
                node_id: 1,
                param: "color".into(),
                value: MaterialParamValue::Color([0.0, 1.0, 0.0]),
            }],
        };
        apply_material_overrides(7, &mut graph, &params).expect("applies");
        assert_eq!(
            graph.nodes[0].params,
            vec![("color".to_string(), ParamValue::Color([0.0, 1.0, 0.0]))]
        );

        let missing_node = MaterialParams {
            material_id: "umber.x".into(),
            channels: vec![],
            params: vec![MaterialParamOverride {
                node_id: 99,
                param: "color".into(),
                value: MaterialParamValue::Float(1.0),
            }],
        };
        assert!(matches!(
            apply_material_overrides(7, &mut graph, &missing_node),
            Err(MaterialError::MissingOverrideTarget {
                layer: 7,
                node: 99,
                ..
            })
        ));
        let missing_param = MaterialParams {
            material_id: "umber.x".into(),
            channels: vec![],
            params: vec![MaterialParamOverride {
                node_id: 1,
                param: "nope".into(),
                value: MaterialParamValue::Float(1.0),
            }],
        };
        assert!(matches!(
            apply_material_overrides(7, &mut graph, &missing_param),
            Err(MaterialError::MissingOverrideTarget {
                layer: 7,
                node: 1,
                ..
            })
        ));
    }

    /// Ensures one layer's states entry (the `drive` queue half without the
    /// drain, so never-crash tests control each refresh explicitly).
    fn drive_states(
        layers: &[Layer],
        definitions: &HashMap<String, String>,
        size: (u32, u32),
    ) -> MaterialLayerStates {
        let mut states = MaterialLayerStates::default();
        for layer in layers {
            if layer.kind == LayerKind::Material {
                if let Some(params) = layer.material.clone() {
                    let definition = definitions
                        .get(&params.material_id)
                        .cloned()
                        .unwrap_or_default();
                    states.ensure(layer.id, &params, &definition, size);
                }
            }
        }
        states
    }

    #[test]
    fn never_crash_truncated_definition_publishes_nothing() {
        let mut stack = LayerStack::new();
        let layer = material_layer(
            &mut stack,
            "Broken",
            channel_params("umber.truncated", vec![MaterialChannel::named("baseColor")]),
            1.0,
        );
        let mut definitions = HashMap::new();
        definitions.insert(
            "umber.truncated".into(),
            "<materialx version=\"1.38\">\n<no".into(),
        );
        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [10, 20, 30, 255]);
        let before: HashMap<(u16, String), Vec<u8>> =
            HashMap::from([((1001, "baseColor".into()), io.tile_bytes(1001, "baseColor"))]);
        let mut states = drive_states(std::slice::from_ref(&layer), &definitions, (1, 1));
        let reports = refresh_dirty_materials(
            &mut states,
            std::slice::from_ref(&layer),
            &definitions,
            &set_channels(),
            &umber_graph::full_registry(),
            &mut io,
        );
        assert!(matches!(
            failed_of(&reports, layer.id),
            MaterialError::DefinitionParse { .. }
        ));
        assert_eq!(
            io.tile_bytes(1001, "baseColor"),
            before[&(1001, "baseColor".into())]
        );
    }

    #[test]
    fn never_crash_missing_definition_id_publishes_nothing() {
        let mut stack = LayerStack::new();
        let layer = material_layer(
            &mut stack,
            "Orphan",
            channel_params("umber.gone", vec![MaterialChannel::named("baseColor")]),
            1.0,
        );
        let definitions = HashMap::new();
        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [10, 20, 30, 255]);
        let before = io.tile_bytes(1001, "baseColor");
        let mut states = drive_states(std::slice::from_ref(&layer), &definitions, (1, 1));
        let reports = refresh_dirty_materials(
            &mut states,
            std::slice::from_ref(&layer),
            &definitions,
            &set_channels(),
            &umber_graph::full_registry(),
            &mut io,
        );
        assert!(matches!(
            failed_of(&reports, layer.id),
            MaterialError::MissingDefinition { .. }
        ));
        assert_eq!(io.tile_bytes(1001, "baseColor"), before);
    }

    #[test]
    fn never_crash_unknown_node_def_fails_every_channel() {
        let mut graph = Graph::new();
        graph.add_node(Node {
            id: 1,
            node_def: "quantum_flux_capacitor".into(),
            params: vec![],
            canvas: None,
        });
        graph.add_node(output_node(2, "baseColor"));
        graph.add_edge(edge(1, 2));
        let mut stack = LayerStack::new();
        let layer = material_layer(
            &mut stack,
            "Future",
            channel_params("umber.future", vec![MaterialChannel::named("baseColor")]),
            1.0,
        );
        let mut definitions = HashMap::new();
        definitions.insert("umber.future".into(), to_mtlx(&graph, &painter_nodedefs()));
        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [10, 20, 30, 255]);
        let before = io.tile_bytes(1001, "baseColor");
        let mut states = drive_states(std::slice::from_ref(&layer), &definitions, (1, 1));
        let reports = refresh_dirty_materials(
            &mut states,
            std::slice::from_ref(&layer),
            &definitions,
            &set_channels(),
            &umber_graph::full_registry(),
            &mut io,
        );
        assert!(matches!(
            failed_of(&reports, layer.id),
            MaterialError::Eval(_)
        ));
        assert_eq!(io.tile_bytes(1001, "baseColor"), before);
    }

    #[test]
    fn never_crash_unknown_channel_duplicate_and_bad_override_keep_good_pixels() {
        // A previously published good result STAYS through a later failure:
        // publish red, then break the definition three ways in turn.
        let mut stack = LayerStack::new();
        let params = channel_params("umber.good", vec![MaterialChannel::named("baseColor")]);
        let layer = material_layer(&mut stack, "G", params, 1.0);
        let mut definitions = HashMap::new();
        definitions.insert("umber.good".into(), solid_def([1.0, 0.0, 0.0], "baseColor"));
        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [10, 20, 30, 255]);
        let layers = std::slice::from_ref(&layer);
        let (mut states, _) = drive(layers, &definitions, &mut io);
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![255, 0, 0, 255]);

        // 1. Unknown channel.
        let mut graph = Graph::new();
        graph.add_node(uniform_node(1, [0.0, 1.0, 0.0]));
        graph.add_node(output_node(2, "clown"));
        graph.add_edge(edge(1, 2));
        definitions.insert("umber.good".into(), to_mtlx(&graph, &painter_nodedefs()));
        states.ensure(
            layer.id,
            layer.material.as_ref().unwrap(),
            &definitions["umber.good"],
            (1, 1),
        );
        let reports = refresh_dirty_materials(
            &mut states,
            layers,
            &definitions,
            &set_channels(),
            &umber_graph::full_registry(),
            &mut io,
        );
        assert!(matches!(
            failed_of(&reports, layer.id),
            MaterialError::UnknownChannel { .. }
        ));
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![255, 0, 0, 255]);

        // 2. Duplicate channel.
        let mut graph = Graph::new();
        graph.add_node(uniform_node(1, [0.0, 1.0, 0.0]));
        graph.add_node(output_node(2, "baseColor"));
        graph.add_node(output_node(3, "baseColor"));
        graph.add_edge(edge(1, 2));
        graph.add_edge(edge(1, 3));
        definitions.insert("umber.good".into(), to_mtlx(&graph, &painter_nodedefs()));
        states.ensure(
            layer.id,
            layer.material.as_ref().unwrap(),
            &definitions["umber.good"],
            (1, 1),
        );
        let reports = refresh_dirty_materials(
            &mut states,
            layers,
            &definitions,
            &set_channels(),
            &umber_graph::full_registry(),
            &mut io,
        );
        assert!(matches!(
            failed_of(&reports, layer.id),
            MaterialError::DuplicateChannel { .. }
        ));
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![255, 0, 0, 255]);

        // 3. Override naming a missing node (params change re-dirties).
        let mut changed = layer.material.clone().unwrap();
        changed.params = vec![MaterialParamOverride {
            node_id: 99,
            param: "color".into(),
            value: MaterialParamValue::Float(1.0),
        }];
        let mut stack2 = LayerStack::new();
        let layer2 = material_layer(&mut stack2, "G", changed, 1.0);
        let layers2 = std::slice::from_ref(&layer2);
        definitions.insert("umber.good".into(), solid_def([0.0, 1.0, 0.0], "baseColor"));
        let mut states2 = drive_states(layers2, &definitions, (1, 1));
        let reports = refresh_dirty_materials(
            &mut states2,
            layers2,
            &definitions,
            &set_channels(),
            &umber_graph::full_registry(),
            &mut io,
        );
        assert!(matches!(
            failed_of(&reports, layer2.id),
            MaterialError::MissingOverrideTarget { .. }
        ));
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![255, 0, 0, 255]);
    }

    #[test]
    fn never_crash_material_none_publishes_nothing() {
        let mut stack = LayerStack::new();
        let id = stack.add_layer("Empty", LayerKind::Material);
        let layer = stack.layer(id).unwrap().clone();
        assert_eq!(layer.material, None);
        let definitions = HashMap::new();
        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [10, 20, 30, 255]);
        let before = io.tile_bytes(1001, "baseColor");
        // A params-less material layer still queues (layer add is runtime).
        let mut states = MaterialLayerStates::default();
        states.ensure(
            id,
            &MaterialParams {
                material_id: String::new(),
                channels: vec![],
                params: vec![],
            },
            "",
            (1, 1),
        );
        let reports = refresh_dirty_materials(
            &mut states,
            std::slice::from_ref(&layer),
            &definitions,
            &set_channels(),
            &umber_graph::full_registry(),
            &mut io,
        );
        assert!(matches!(
            failed_of(&reports, id),
            MaterialError::NoParams { .. }
        ));
        assert_eq!(io.tile_bytes(1001, "baseColor"), before);
    }

    #[test]
    fn uniform_at_one_output_skips_only_that_channel() {
        // The per-channel exception: a Uniform wired to the roughness output
        // leaves roughness unchanged while baseColor publishes its golden.
        let mut graph = Graph::new();
        graph.add_node(uniform_node(1, [1.0, 0.0, 0.0]));
        graph.add_node(Node {
            id: 2,
            node_def: "uniform".into(),
            params: vec![("value".into(), ParamValue::Float(0.5))],
            canvas: None,
        });
        graph.add_node(output_node(3, "baseColor"));
        graph.add_node(output_node(4, "roughness"));
        graph.add_edge(edge(1, 3));
        graph.add_edge(edge(2, 4));
        let mut stack = LayerStack::new();
        let params = channel_params(
            "umber.half-uniform",
            vec![
                MaterialChannel::named("baseColor"),
                MaterialChannel::named("roughness"),
            ],
        );
        let layer = material_layer(&mut stack, "U", params, 1.0);
        let mut definitions = HashMap::new();
        definitions.insert(
            "umber.half-uniform".into(),
            to_mtlx(&graph, &painter_nodedefs()),
        );
        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [10, 20, 30, 255]);
        io.tiles
            .insert((1001, "roughness".into()), (1, 1, vec![200, 200, 200, 255]));
        let (_, reports) = drive(std::slice::from_ref(&layer), &definitions, &mut io);
        assert_eq!(
            status_of(&reports, layer.id, "baseColor"),
            ChannelStatus::Published
        );
        assert_eq!(
            status_of(&reports, layer.id, "roughness"),
            ChannelStatus::SkippedUniform
        );
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![255, 0, 0, 255]);
        assert_eq!(io.tile_bytes(1001, "roughness"), vec![200, 200, 200, 255]);
    }

    /// Run-counting image node: the engine's dirty-propagation probe. Sources
    /// (no inputs) emit a `bias`-tagged solid at the context resolution;
    /// consumers echo their input. Every run increments its counter.
    struct CountImage {
        count: Arc<AtomicUsize>,
    }

    impl umber_graph::NodeImpl for CountImage {
        fn eval(
            &self,
            inputs: Vec<(String, NodeOutput)>,
            params: &[(String, ParamValue)],
            ctx: &EvalContext,
        ) -> Result<NodeOutput, EvalError> {
            self.count.fetch_add(1, Ordering::SeqCst);
            if inputs.is_empty() {
                let bias = match params.iter().find(|(n, _)| n == "bias") {
                    Some((_, ParamValue::Int(b))) => *b,
                    _ => {
                        return Err(EvalError::BadParam {
                            node: 0,
                            param: "bias".into(),
                        });
                    }
                };
                let (w, h) = ctx.resolution;
                let pixel = [bias as u8, 0, 0, 255];
                Ok(NodeOutput::Image(
                    ImageBuffer::filled(w, h, pixel).map_err(EvalError::Image)?,
                ))
            } else {
                let (_, value) = inputs
                    .into_iter()
                    .next()
                    .unwrap_or((String::new(), NodeOutput::Uniform(ParamValue::Float(0.0))));
                Ok(value)
            }
        }
    }

    fn count_node(id: u64, def: &str, bias: i32) -> Node {
        Node {
            id,
            node_def: def.into(),
            params: vec![("bias".into(), ParamValue::Int(bias))],
            canvas: None,
        }
    }

    /// A definition with two countable stages feeding one output, plus a
    /// registry that counts each stage's runs (the material nodes ride the
    /// real impl — only the stages count).
    fn counting_setup() -> (String, NodeRegistry, [Arc<AtomicUsize>; 2]) {
        let counts = [Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))];
        let mut registry = NodeRegistry::new();
        registry.register(
            "count_a",
            Arc::new(CountImage {
                count: counts[0].clone(),
            }),
        );
        registry.register(
            "count_b",
            Arc::new(CountImage {
                count: counts[1].clone(),
            }),
        );
        umber_graph::material::register_material_nodes(&mut registry);
        let mut graph = Graph::new();
        graph.add_node(count_node(1, "count_a", 10));
        graph.add_node(count_node(2, "count_b", 0));
        graph.add_node(output_node(3, "baseColor"));
        graph.add_edge(edge(1, 2));
        graph.add_edge(edge(2, 3));
        // The engine needs the seed's uniform? No — this graph is closed.
        (to_mtlx(&graph, &painter_nodedefs()), registry, counts)
    }

    fn run_counts(counts: &[Arc<AtomicUsize>; 2]) -> [usize; 2] {
        [
            counts[0].load(Ordering::SeqCst),
            counts[1].load(Ordering::SeqCst),
        ]
    }

    #[test]
    fn override_edit_reruns_only_dirty_nodes_plus_downstream() {
        // §5.4: the engine's run-counter pattern through the driver. An
        // override on node 2 re-runs 2 (+ downstream output) but NOT node 1;
        // a channel-only edit re-runs NOTHING (warm caches, composite only).
        let (definition, registry, counts) = counting_setup();
        let mut stack = LayerStack::new();
        let params = channel_params("umber.counted", vec![MaterialChannel::named("baseColor")]);
        let layer = material_layer(&mut stack, "C", params, 1.0);
        let layers = std::slice::from_ref(&layer);
        let mut definitions = HashMap::new();
        definitions.insert("umber.counted".into(), definition);
        let mut io = FakeTileIo::flat((2, 2), 1001, "baseColor", [0, 0, 0, 255]);

        let mut states = drive_states(layers, &definitions, (2, 2));
        let drain = |states: &mut MaterialLayerStates, io: &mut FakeTileIo| {
            refresh_dirty_materials(states, layers, &definitions, &set_channels(), &registry, io)
        };
        drain(&mut states, &mut io);
        assert_eq!(run_counts(&counts), [1, 1]);
        assert!(io
            .tile_bytes(1001, "baseColor")
            .chunks_exact(4)
            .all(|px| px == [10, 0, 0, 255]));

        // Channel-only edit (blend flip): the composite re-runs, the graph
        // does not — warm caches serve byte-identical outputs.
        let mut changed = layer.material.clone().unwrap();
        changed.channels[0].blend = BlendMode::Multiply;
        states.ensure(layer.id, &changed, &definitions["umber.counted"], (2, 2));
        drain(&mut states, &mut io);
        assert_eq!(
            run_counts(&counts),
            [1, 1],
            "channel-only edits reuse the caches"
        );
        // Multiply by [10,0,0] over black stays black (sanity: the composite
        // really re-ran with the new blend, not just skipped).
        assert!(io
            .tile_bytes(1001, "baseColor")
            .chunks_exact(4)
            .all(|px| px == [0, 0, 0, 255]));

        // Override edit on node 2: node 1 stays at 1 run, node 2 re-runs.
        // (Node 2 echoes its input, so the bytes are unchanged — the pin is
        // the run counts, not the pixels.)
        changed.params = vec![MaterialParamOverride {
            node_id: 2,
            param: "bias".into(),
            value: MaterialParamValue::Int(5),
        }];
        changed.channels[0].blend = BlendMode::Normal;
        states.ensure(layer.id, &changed, &definitions["umber.counted"], (2, 2));
        drain(&mut states, &mut io);
        assert_eq!(
            run_counts(&counts),
            [1, 2],
            "only the dirty node + downstream re-run"
        );
        assert!(io
            .tile_bytes(1001, "baseColor")
            .chunks_exact(4)
            .all(|px| px == [10, 0, 0, 255]));

        // Override edit on node 1 (bias 10 → 20): everything re-runs and the
        // new bias tag publishes.
        changed.params = vec![MaterialParamOverride {
            node_id: 1,
            param: "bias".into(),
            value: MaterialParamValue::Int(20),
        }];
        states.ensure(layer.id, &changed, &definitions["umber.counted"], (2, 2));
        drain(&mut states, &mut io);
        assert_eq!(run_counts(&counts), [2, 3]);
        assert!(io
            .tile_bytes(1001, "baseColor")
            .chunks_exact(4)
            .all(|px| px == [20, 0, 0, 255]));
    }

    #[test]
    fn other_layer_edits_size_changes_and_open_seeding() {
        // §5.4: an edit to another layer never dirties a material; a
        // size change invalidates the caches (full re-run); a
        // project-open-seeded state is clean while a runtime one is dirty.
        let mut stack = LayerStack::new();
        let params_a = channel_params("umber.a", vec![MaterialChannel::named("baseColor")]);
        let params_b = channel_params("umber.b", vec![MaterialChannel::named("baseColor")]);
        let layer_a = material_layer(&mut stack, "A", params_a, 1.0);
        let layer_b = material_layer(&mut stack, "B", params_b, 1.0);
        let mut definitions = HashMap::new();
        definitions.insert("umber.a".into(), solid_def([1.0, 0.0, 0.0], "baseColor"));
        definitions.insert("umber.b".into(), solid_def([0.0, 1.0, 0.0], "baseColor"));
        let layers = [layer_a.clone(), layer_b.clone()];

        let mut states = MaterialLayerStates::default();
        states.ensure(
            layer_a.id,
            layer_a.material.as_ref().unwrap(),
            &definitions["umber.a"],
            (2, 2),
        );
        states.ensure(
            layer_b.id,
            layer_b.material.as_ref().unwrap(),
            &definitions["umber.b"],
            (2, 2),
        );
        assert!(states.is_dirty(layer_a.id));
        assert!(states.is_dirty(layer_b.id));
        let mut io = FakeTileIo::flat((2, 2), 1001, "baseColor", [0, 0, 0, 255]);
        let drain = |states: &mut MaterialLayerStates, io: &mut FakeTileIo| {
            refresh_dirty_materials(
                &mut *states,
                &layers,
                &definitions,
                &set_channels(),
                &umber_graph::full_registry(),
                io,
            )
        };
        drain(&mut states, &mut io);
        assert!(!states.is_dirty(layer_a.id));
        assert!(!states.is_dirty(layer_b.id));

        // Another layer's edit: only that layer queues.
        let mut changed_b = layer_b.material.clone().unwrap();
        changed_b.channels[0].opacity = 0.5;
        states.ensure(layer_b.id, &changed_b, &definitions["umber.b"], (2, 2));
        assert!(
            !states.is_dirty(layer_a.id),
            "other-layer edits never dirty a material"
        );
        assert!(states.is_dirty(layer_b.id));
        assert_eq!(states.dirty_ids(), &[layer_b.id]);

        // A size change dirties AND re-runs everything next refresh (the
        // caches hold wrong-size images).
        drain(&mut states, &mut io);
        states.ensure(
            layer_a.id,
            layer_a.material.as_ref().unwrap(),
            &definitions["umber.a"],
            (4, 4),
        );
        assert!(states.is_dirty(layer_a.id));

        // Project-open seeding: fresh states start clean (no publish on
        // open); the runtime path starts dirty. Same inputs, both arms.
        let mut opened = MaterialLayerStates::default();
        opened.ensure_open(
            layer_a.id,
            layer_a.material.as_ref().unwrap(),
            &definitions["umber.a"],
            (2, 2),
        );
        assert!(
            !opened.is_dirty(layer_a.id),
            "open-seeded states publish nothing"
        );
        assert!(opened.dirty_ids().is_empty());
        let mut runtime = MaterialLayerStates::default();
        runtime.ensure(
            layer_a.id,
            layer_a.material.as_ref().unwrap(),
            &definitions["umber.a"],
            (2, 2),
        );
        assert!(runtime.is_dirty(layer_a.id), "runtime states refresh once");
        // …and an open-seeded state with CHANGED params still dirties.
        opened.ensure_open(layer_a.id, &changed_b, &definitions["umber.a"], (2, 2));
        assert!(opened.is_dirty(layer_a.id));
    }

    #[test]
    fn anchor_graphs_eval_per_tile_others_share() {
        // §5.4: a graph with an `anchor` node gets a per-tile cache (each
        // tile's anchor bytes evaluate separately); one without
        // anchors/mesh-map nodes shares one eval (no anchor reads at all).
        let mut graph = Graph::new();
        graph.add_node(Node {
            id: 1,
            node_def: "anchor".into(),
            params: vec![
                ("layer_id".into(), ParamValue::Int(7)),
                ("channel".into(), ParamValue::Asset("baseColor".into())),
            ],
            canvas: None,
        });
        graph.add_node(output_node(2, "baseColor"));
        graph.add_edge(edge(1, 2));
        let mut stack = LayerStack::new();
        let layer = material_layer(
            &mut stack,
            "Anchored",
            channel_params("umber.anchored", vec![MaterialChannel::named("baseColor")]),
            1.0,
        );
        let mut definitions = HashMap::new();
        definitions.insert(
            "umber.anchored".into(),
            to_mtlx(&graph, &painter_nodedefs()),
        );

        let mut io = FakeTileIo {
            set_size: (1, 1),
            tile_list: vec![1001, 1002],
            tiles: HashMap::from([
                ((1001, "baseColor".into()), (1, 1, vec![0, 0, 0, 255])),
                ((1002, "baseColor".into()), (1, 1, vec![0, 0, 0, 255])),
            ]),
            anchors: HashMap::from([
                ((1001, 7, "baseColor".into()), (1, 1, vec![10, 0, 0, 255])),
                ((1002, 7, "baseColor".into()), (1, 1, vec![0, 200, 0, 255])),
            ]),
            ..Default::default()
        };
        let layers = std::slice::from_ref(&layer);
        let (states, reports) = drive(layers, &definitions, &mut io);
        assert_eq!(
            status_of(&reports, layer.id, "baseColor"),
            ChannelStatus::Published
        );
        // Each tile composited ITS anchor's bytes (per-tile externals, not a
        // shared tile-1001 result).
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![10, 0, 0, 255]);
        assert_eq!(io.tile_bytes(1002, "baseColor"), vec![0, 200, 0, 255]);
        assert_eq!(states.state(layer.id).unwrap().per_tile.len(), 2);
        assert_eq!(io.anchor_log.len(), 2);

        // Shared path: no anchor reads, one eval for all tiles.
        let mut definitions = HashMap::new();
        definitions.insert(
            "umber.plain".into(),
            solid_def([1.0, 0.0, 0.0], "baseColor"),
        );
        let params = channel_params("umber.plain", vec![MaterialChannel::named("baseColor")]);
        let layer = material_layer(&mut stack, "Plain", params, 1.0);
        let layers = std::slice::from_ref(&layer);
        let mut io = FakeTileIo {
            set_size: (1, 1),
            tile_list: vec![1001, 1002],
            tiles: HashMap::from([
                ((1001, "baseColor".into()), (1, 1, vec![0, 0, 0, 255])),
                ((1002, "baseColor".into()), (1, 1, vec![0, 0, 0, 255])),
            ]),
            ..Default::default()
        };
        let (states, _) = drive(layers, &definitions, &mut io);
        assert!(io.anchor_log.is_empty(), "shared evals read no anchors");
        assert!(states.state(layer.id).unwrap().per_tile.is_empty());
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![255, 0, 0, 255]);
        assert_eq!(io.tile_bytes(1002, "baseColor"), vec![255, 0, 0, 255]);
    }

    #[test]
    fn mesh_map_graphs_eval_per_tile_and_missing_maps_fail_whole() {
        // `mesh_map_generator` forces the per-tile cache (the doc's
        // conservative rule — today's placeholders are flat, tomorrow's
        // bakes are per tile). An unprovided map is a whole-material no-op.
        let mut graph = Graph::new();
        graph.add_node(Node {
            id: 1,
            node_def: "mesh_map_generator".into(),
            params: vec![("map_name".into(), ParamValue::Asset("AO".into()))],
            canvas: None,
        });
        graph.add_node(output_node(2, "baseColor"));
        graph.add_edge(edge(1, 2));
        let mut stack = LayerStack::new();
        let layer = material_layer(
            &mut stack,
            "Meshed",
            channel_params("umber.meshed", vec![MaterialChannel::named("baseColor")]),
            1.0,
        );
        let mut definitions = HashMap::new();
        definitions.insert("umber.meshed".into(), to_mtlx(&graph, &painter_nodedefs()));
        let layers = std::slice::from_ref(&layer);

        let mut io = FakeTileIo {
            set_size: (1, 1),
            tile_list: vec![1001, 1002],
            tiles: HashMap::from([
                ((1001, "baseColor".into()), (1, 1, vec![0, 0, 0, 255])),
                ((1002, "baseColor".into()), (1, 1, vec![0, 0, 0, 255])),
            ]),
            meshes: HashMap::from([("AO".into(), (1, 1, vec![77, 77, 77, 255]))]),
            ..Default::default()
        };
        let (states, reports) = drive(layers, &definitions, &mut io);
        assert_eq!(
            status_of(&reports, layer.id, "baseColor"),
            ChannelStatus::Published
        );
        assert_eq!(io.mesh_log, vec!["AO".to_string(), "AO".to_string()]);
        assert_eq!(states.state(layer.id).unwrap().per_tile.len(), 2);
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![77, 77, 77, 255]);

        // Unprovided map: MissingInput at eval → whole-material no-op, tiles
        // byte-identical.
        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [9, 9, 9, 255]);
        let (_, reports) = drive(layers, &definitions, &mut io);
        assert!(matches!(
            failed_of(&reports, layer.id),
            MaterialError::Eval(_)
        ));
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![9, 9, 9, 255]);
    }

    #[test]
    fn invisible_layers_keep_dirty_and_deleted_layers_drop() {
        // Invisible: SkippedInvisible, tiles untouched, dirty KEPT (no
        // ensure on visibility toggles — the pending refresh fires when the
        // layer becomes visible). Deleted: dropped from the queue silently.
        let mut stack = LayerStack::new();
        let params = channel_params("umber.hider", vec![MaterialChannel::named("baseColor")]);
        let layer = material_layer(&mut stack, "H", params, 1.0);
        let mut definitions = HashMap::new();
        definitions.insert(
            "umber.hider".into(),
            solid_def([1.0, 0.0, 0.0], "baseColor"),
        );
        let mut hidden = layer.clone();
        hidden.visible = false;

        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [10, 20, 30, 255]);
        let mut states = drive_states(std::slice::from_ref(&hidden), &definitions, (1, 1));
        let reports = refresh_dirty_materials(
            &mut states,
            std::slice::from_ref(&hidden),
            &definitions,
            &set_channels(),
            &umber_graph::full_registry(),
            &mut io,
        );
        assert!(matches!(
            reports[0].result,
            MaterialResult::SkippedInvisible
        ));
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![10, 20, 30, 255]);
        assert!(
            states.is_dirty(hidden.id),
            "invisible layers keep their dirty flag"
        );
        assert!(io.read_log.is_empty(), "invisible layers read nothing");

        // Visible again with no other change: the pending refresh publishes.
        let reports = refresh_dirty_materials(
            &mut states,
            std::slice::from_ref(&layer),
            &definitions,
            &set_channels(),
            &umber_graph::full_registry(),
            &mut io,
        );
        assert_eq!(
            status_of(&reports, layer.id, "baseColor"),
            ChannelStatus::Published
        );
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![255, 0, 0, 255]);
        assert!(!states.is_dirty(layer.id));

        // Deleted since queueing: no report, queue drained (the fill
        // ghost-publish guard).
        let mut states = drive_states(std::slice::from_ref(&layer), &definitions, (1, 1));
        let reports = refresh_dirty_materials(
            &mut states,
            &[],
            &definitions,
            &set_channels(),
            &umber_graph::full_registry(),
            &mut io,
        );
        assert!(reports.is_empty());
        assert!(states.dirty_ids().is_empty());
    }

    #[test]
    fn ignored_undeclared_channels_are_reported_not_painted() {
        // Listed-but-undeclared channels are ignored (the definition wins)
        // and reported for the status line — while the declared channel
        // publishes.
        let mut stack = LayerStack::new();
        let params = channel_params(
            "umber.drifted",
            vec![
                MaterialChannel::named("baseColor"),
                MaterialChannel::named("roughness"),
            ],
        );
        let layer = material_layer(&mut stack, "D", params, 1.0);
        let mut definitions = HashMap::new();
        definitions.insert(
            "umber.drifted".into(),
            solid_def([1.0, 0.0, 0.0], "baseColor"),
        );
        let mut io = FakeTileIo::flat((1, 1), 1001, "baseColor", [10, 20, 30, 255]);
        io.tiles
            .insert((1001, "roughness".into()), (1, 1, vec![200, 200, 200, 255]));
        let (_, reports) = drive(std::slice::from_ref(&layer), &definitions, &mut io);
        match &reports[0].result {
            MaterialResult::Refreshed {
                channels,
                ignored_undeclared,
            } => {
                assert_eq!(ignored_undeclared, &vec!["roughness".to_string()]);
                assert_eq!(channels.len(), 1);
                assert_eq!(channels[0].name, "baseColor");
            }
            other => panic!("expected Refreshed, got {other:?}"),
        }
        assert_eq!(io.tile_bytes(1001, "baseColor"), vec![255, 0, 0, 255]);
        assert_eq!(io.tile_bytes(1001, "roughness"), vec![200, 200, 200, 255]);
        assert!(
            io.read_log.iter().all(|(_, c)| c != "roughness"),
            "undeclared channels are never read even when listed"
        );
    }
}
